//! The engine thread: single owner of all managed-window state.
//!
//! It processes commands from the application, WinEvents (window shown/destroyed/moved,
//! foreground changes, virtual-desktop cloaking, desktop switches), rule re-evaluation
//! timers and global hotkeys, and it owns the pin-frame overlays.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::Duration;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EVENT_OBJECT_CLOAKED,
    EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_NAMECHANGE,
    EVENT_OBJECT_SHOW, EVENT_OBJECT_UNCLOAKED, EVENT_SYSTEM_DESKTOPSWITCH, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MOVESIZEEND,
    GetForegroundWindow, GetMessageW, HWND_MESSAGE, KillTimer, MSG, PostThreadMessageW,
    RegisterClassExW, RegisterWindowMessageW, SW_RESTORE, SetForegroundWindow, SetTimer,
    ShowWindowAsync, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_HOTKEY, WM_TIMER,
    WNDCLASSEXW,
};
use windows::core::{PCWSTR, w};

use awwm_core::geometry::logical_to_physical;
use awwm_core::rules::{WindowFacts, any_title_rules, find_matching_rule};
use awwm_core::settings::{Settings, parse_hex_color};

use crate::enumerator;
use crate::error::{EngineError, EngineResult};
use crate::events::{self, WinEventHook, WinEventRecord};
use crate::gesture::WM_APP_GESTURE_ABORT;
use crate::hotkeys::{Hotkeys, action_for_id};
use crate::manager::{Desired, ManagedEntry, Manager, restore_entry};
use crate::overlay::FrameOverlay;
use crate::session::SessionStore;
use crate::shared::{
    SharedState, WM_APP_COMMAND, WM_APP_EVENTS, WM_APP_FRAME_SYNC, WM_APP_HOOKS, unpoison,
};
use crate::types::{
    EngineEvent, HotkeyAction, HotkeyStatus, ManagedSource, ManagedWindowInfo, Notice, NoticeLevel,
    WindowInfo,
};
use crate::win::window::{self, ZOrder};
use crate::win::{WindowHandle, monitor, process};

/// Class of the engine's message-only window (found by a second instance).
pub const ENGINE_WINDOW_CLASS: PCWSTR = w!("AWWM.Engine");
/// Registered message a second instance posts to ask for the settings window.
pub const SHOW_MESSAGE_NAME: PCWSTR = w!("AWWM.ShowSettings");

/// Rule re-evaluation delays after a window is shown (title or UWP host not ready yet).
const REEVALUATE_AFTER_MS: [u32; 2] = [500, 2000];

/// Reply channel of a command.
pub type Reply<T> = SyncSender<EngineResult<T>>;

/// Commands from the application layer.
#[derive(Debug)]
pub enum Command {
    Pin {
        hwnd: u64,
        reply: Reply<ManagedWindowInfo>,
    },
    Unpin {
        hwnd: u64,
        reply: Reply<()>,
    },
    SetOpacity {
        hwnd: u64,
        opacity: u8,
        reply: Reply<ManagedWindowInfo>,
    },
    UnpinAll {
        reply: Reply<()>,
    },
    ListManaged {
        reply: Reply<Vec<ManagedWindowInfo>>,
    },
    ListWindows {
        reply: Reply<Vec<WindowInfo>>,
    },
    ApplySettings {
        settings: Box<Settings>,
        reply: Reply<Vec<HotkeyStatus>>,
    },
    ApplyRulesNow {
        reply: Reply<u32>,
    },
    Activate {
        hwnd: u64,
        reply: Reply<()>,
    },
    HotkeyStatus {
        reply: Reply<Vec<HotkeyStatus>>,
    },
    Shutdown {
        restore: bool,
        reply: Reply<()>,
    },
}

/// Thread ids the engine thread talks to.
#[derive(Clone, Copy, Debug)]
pub struct Peers {
    pub gesture_thread: u32,
    pub hook_thread: u32,
}

struct EngineThread {
    ctx: Arc<SharedState>,
    settings: Settings,
    manager: Manager,
    session: SessionStore,
    msg_window: WindowHandle,
    show_message: u32,
    peers: Peers,
    hotkeys: Hotkeys,
    frames: HashMap<WindowHandle, FrameOverlay>,
    location_hooks: HashMap<u32, WinEventHook>,
    base_hooks: Vec<WinEventHook>,
    name_hook: Option<WinEventHook>,
    /// Windows a rule has already been applied to (never auto-applied twice).
    rule_applied: HashMap<WindowHandle, String>,
    /// Resolved executable path per window.
    exe_cache: HashMap<WindowHandle, String>,
    timers: HashMap<usize, TimerKind>,
    next_timer: usize,
    /// Windows that currently have a pin frame (effectively pinned, frames enabled).
    framed: HashSet<WindowHandle>,
    /// A debounced session write is scheduled.
    session_save_pending: bool,
    /// Which windows were managed (and how) at the last immediate session write.
    saved_shape: Vec<(u64, bool, bool, bool)>,
}

/// What a one-shot timer of the engine window is for.
#[derive(Clone, Copy, Debug)]
enum TimerKind {
    /// Re-run reattachment and rules for a window shown moments ago.
    Reevaluate(WindowHandle),
    /// Re-check a destroy notification that arrived while the window still existed.
    VerifyDestroyed(WindowHandle),
}

/// Delay before re-checking a destroy notification for a still-existing window.
const DESTROY_RECHECK_MS: u32 = 250;

/// Timer id of the debounced session write (other timers count up from 1).
const SESSION_TIMER: usize = usize::MAX;
/// Delay of the debounced session write. Slider drags change opacity ~25×/s; the session
/// file only needs the settled state (it exists for crash recovery).
const SESSION_SAVE_DELAY_MS: u32 = 500;

fn enabled(settings: &Settings) -> bool {
    settings.general.management_enabled
}

impl EngineThread {
    fn emit(&self, event: EngineEvent) {
        self.ctx.emit(event);
    }

    fn notice(&self, level: NoticeLevel, message: impl Into<String>, hwnd: Option<WindowHandle>) {
        let message = message.into();
        match level {
            NoticeLevel::Error => tracing::error!(%message, "notice"),
            NoticeLevel::Warning => tracing::warn!(%message, "notice"),
            NoticeLevel::Info => tracing::info!(%message, "notice"),
        }
        self.emit(EngineEvent::Notice(Notice::new(
            level,
            message,
            hwnd.map(WindowHandle::as_u64),
        )));
    }

    /// Propagates a change of the managed set: shared pinned set, session file, frames,
    /// location hooks and the UI.
    fn publish(&mut self) {
        let pinned: HashSet<WindowHandle> = self.manager.pinned_handles().collect();
        *unpoison(self.ctx.pinned.write()) = pinned;
        // Windows added/removed/pinned/unpinned are recorded at once (crash safety);
        // pure opacity changes (slider drags) are debounced.
        let shape: Vec<(u64, bool, bool, bool)> = self
            .manager
            .entries()
            .iter()
            .map(|e| (e.hwnd, e.pinned, e.pin_pending, e.release_pending))
            .collect();
        if shape == self.saved_shape {
            self.schedule_session_save();
        } else {
            self.saved_shape = shape;
            self.session.save(self.manager.entries());
        }
        if self.wanted_frames() != self.framed {
            self.sync_frames();
        }
        self.emit(EngineEvent::ManagedChanged(self.manager.infos()));
    }

    fn schedule_session_save(&mut self) {
        if self.session_save_pending {
            return;
        }
        // SAFETY: one-shot timer on our own message window, killed when it fires.
        let ok = unsafe {
            SetTimer(
                Some(self.msg_window.hwnd()),
                SESSION_TIMER,
                SESSION_SAVE_DELAY_MS,
                None,
            )
        } != 0;
        if ok {
            self.session_save_pending = true;
        } else {
            self.session.save(self.manager.entries());
        }
    }

    fn resolve(&self, hwnd: u64) -> EngineResult<WindowHandle> {
        let h = WindowHandle::from_u64(hwnd).ok_or(EngineError::WindowNotFound)?;
        if !window::exists(h) {
            return Err(EngineError::WindowNotFound);
        }
        let h = window::root(h);
        if window::process_id(h) == self.ctx.own_pid {
            return Err(EngineError::InvalidArgument(
                "AWWM cannot manage its own windows.".into(),
            ));
        }
        if window::is_shell_window(h) {
            return Err(EngineError::InvalidArgument(
                "The desktop and taskbar cannot be managed.".into(),
            ));
        }
        Ok(h)
    }

    // ---------------------------------------------------------------- commands

    fn pin(&mut self, h: WindowHandle) -> EngineResult<ManagedWindowInfo> {
        let desired = match self.manager.get(h) {
            Some(e) => Desired {
                pinned: true,
                opacity: e.opacity,
                source: e.source,
                rule_id: e.rule_id.clone(),
                defer_refused_pin: false,
            },
            None => Desired {
                pinned: true,
                opacity: self.settings.pinning.default_opacity,
                source: ManagedSource::Manual,
                rule_id: None,
                defer_refused_pin: false,
            },
        };
        let result = self.manager.manage(h, desired).map(ManagedEntry::info);
        self.publish();
        result
    }

    fn unpin(&mut self, h: WindowHandle) -> EngineResult<()> {
        let result = self.manager.release(h).map(|_| ());
        self.publish();
        result
    }

    fn set_opacity(&mut self, h: WindowHandle, opacity: u8) -> EngineResult<ManagedWindowInfo> {
        let opacity = opacity.min(100);
        let result = if self.manager.contains(h) {
            match self.manager.set_opacity(h, opacity) {
                Ok(e) if !e.pinned && e.opacity >= 100 => {
                    // Nothing left to manage: release completely.
                    self.manager.release(h).map(|e| ManagedWindowInfo {
                        opacity: 100,
                        ..e.info()
                    })
                }
                Ok(e) => Ok(e.info()),
                Err(e) => Err(e),
            }
        } else if opacity >= 100 {
            Ok(enumerated_info(h))
        } else {
            self.manager
                .manage(
                    h,
                    Desired {
                        pinned: false,
                        opacity,
                        source: ManagedSource::Manual,
                        rule_id: None,
                        defer_refused_pin: false,
                    },
                )
                .map(ManagedEntry::info)
        };
        self.publish();
        result
    }

    fn list_windows(&self) -> Vec<WindowInfo> {
        enumerator::app_windows(self.ctx.own_pid)
            .into_iter()
            .map(|h| enumerator::window_info(h, self.manager.contains(h)))
            .collect()
    }

    fn activate(&self, h: WindowHandle) -> EngineResult<()> {
        // SAFETY: plain calls; AWWM's settings window is in the foreground when the user
        // clicks "Bring to front", which allows SetForegroundWindow.
        unsafe {
            if window::is_minimized(h) {
                // Asynchronous: never block the engine thread on the target application.
                let _ = ShowWindowAsync(h.hwnd(), SW_RESTORE);
            }
            if !SetForegroundWindow(h.hwnd()).as_bool() {
                return Err(EngineError::Unsupported(
                    "Windows did not allow bringing the window to the front.".into(),
                ));
            }
        }
        Ok(())
    }

    fn apply_settings(&mut self, new: Settings) -> Vec<HotkeyStatus> {
        let old = std::mem::replace(&mut self.settings, new);
        self.ctx.update_settings(&self.settings);
        let now_enabled = enabled(&self.settings);
        let hooks_wanted = now_enabled && self.settings.gestures.enabled;
        // SAFETY: pointer-free messages to our own threads.
        unsafe {
            let _ = PostThreadMessageW(
                self.peers.hook_thread,
                WM_APP_HOOKS,
                WPARAM(usize::from(hooks_wanted)),
                LPARAM(0),
            );
            if !hooks_wanted {
                // No button-up will be seen any more: end a gesture in progress now.
                let _ = PostThreadMessageW(
                    self.peers.gesture_thread,
                    WM_APP_GESTURE_ABORT,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
        let statuses = if old.hotkeys != self.settings.hotkeys || enabled(&old) != now_enabled {
            let statuses = self.hotkeys.apply(&self.settings.hotkeys, now_enabled);
            self.emit(EngineEvent::HotkeysChanged(statuses.clone()));
            statuses
        } else {
            self.hotkeys.status()
        };
        self.sync_name_hook();

        // Windows managed by a rule follow edits of *that* rule's effect. Edits of other
        // rules (or reordering) never revert manual changes made to such windows.
        if old.rules != self.settings.rules {
            let updates: Vec<(WindowHandle, Desired)> = self
                .manager
                .entries()
                .iter()
                .filter(|e| e.source == ManagedSource::Rule)
                .filter_map(|e| {
                    let rule = self
                        .settings
                        .rules
                        .iter()
                        .find(|r| Some(&r.id) == e.rule_id.as_ref())?;
                    let before = old.rules.iter().find(|r| r.id == rule.id)?;
                    let effect_changed = before.pin != rule.pin || before.opacity != rule.opacity;
                    (effect_changed
                        && rule.enabled
                        && rule.has_effect()
                        && (rule.pin != e.pinned || rule.opacity != e.opacity))
                        .then(|| {
                            (
                                e.handle(),
                                Desired {
                                    pinned: rule.pin,
                                    opacity: rule.opacity,
                                    source: ManagedSource::Rule,
                                    rule_id: Some(rule.id.clone()),
                                    defer_refused_pin: true,
                                },
                            )
                        })
                })
                .collect();
            for (h, desired) in updates {
                if let Err(e) = self.manager.manage(h, desired) {
                    tracing::warn!(hwnd = %h, error = %e, "could not update window after rule change");
                }
            }
        }
        if now_enabled && (!enabled(&old) || old.rules != self.settings.rules) {
            self.apply_rules_to_existing(false);
        }
        // Border colour/thickness or the master switch may have changed.
        self.sync_frames();
        self.publish();
        statuses
    }

    // ---------------------------------------------------------------- rules

    fn has_enabled_rules(&self) -> bool {
        self.settings
            .rules
            .iter()
            .any(|r| r.enabled && r.has_effect())
    }

    fn exe_for(&mut self, h: WindowHandle) -> String {
        if let Some(exe) = self.exe_cache.get(&h) {
            return exe.clone();
        }
        let pid = window::process_id(h);
        let effective = process::effective_pid(h);
        let exe = process::exe_path(effective).unwrap_or_default();
        // A UWP frame whose app has not attached yet still resolves to the host process;
        // do not cache that so a later evaluation sees the real app.
        let unresolved_host = effective == pid && window::class_name(h) == "ApplicationFrameWindow";
        if !exe.is_empty() && !unresolved_host {
            self.exe_cache.insert(h, exe.clone());
        }
        exe
    }

    /// Applies the first matching rule to `h`. Returns `true` when a rule was applied.
    /// With `force`, windows already processed by a rule are re-evaluated (explicit
    /// "Apply rules now").
    fn evaluate_rules(&mut self, h: WindowHandle, force: bool) -> bool {
        if !enabled(&self.settings) || !self.has_enabled_rules() || self.manager.contains(h) {
            return false;
        }
        if !force && self.rule_applied.contains_key(&h) {
            return false;
        }
        if !window::is_app_window(h) || window::process_id(h) == self.ctx.own_pid {
            return false;
        }
        let exe = self.exe_for(h);
        let title = window::title(h);
        let Some(rule) = find_matching_rule(
            &self.settings.rules,
            &WindowFacts {
                exe_path: &exe,
                title: &title,
            },
        )
        .cloned() else {
            return false;
        };
        self.rule_applied.insert(h, rule.id.clone());
        let desired = Desired {
            pinned: rule.pin,
            opacity: rule.opacity,
            source: ManagedSource::Rule,
            rule_id: Some(rule.id.clone()),
            defer_refused_pin: true,
        };
        match self.manager.manage(h, desired) {
            Ok(entry) => {
                let pending = entry.pin_pending;
                tracing::info!(hwnd = %h, rule = %rule.name, exe = %exe, pending, "rule applied");
                if pending {
                    self.notice(
                        NoticeLevel::Info,
                        format!(
                            "Rule \u{201c}{}\u{201d} matched \u{201c}{}\u{201d}. Windows does not allow it \
                             to stay on top right now (a full-screen application has priority); AWWM \
                             will pin it as soon as Windows allows it.",
                            rule.name,
                            process::exe_name(&exe)
                        ),
                        Some(h),
                    );
                }
                true
            }
            Err(EngineError::NotResponding) => {
                // Transient: forget the attempt so a later event (title change, timer,
                // "Apply rules now") can apply the rule once the application responds.
                self.rule_applied.remove(&h);
                tracing::info!(hwnd = %h, rule = %rule.name, "rule postponed: application not responding");
                false
            }
            Err(e) => {
                self.notice(
                    NoticeLevel::Warning,
                    format!(
                        "Rule \u{201c}{}\u{201d} could not be applied to \u{201c}{}\u{201d}: {e}",
                        rule.name,
                        process::exe_name(&exe)
                    ),
                    Some(h),
                );
                false
            }
        }
    }

    fn apply_rules_to_existing(&mut self, force: bool) -> u32 {
        let mut count = 0;
        for h in enumerator::app_windows(self.ctx.own_pid) {
            if self.evaluate_rules(h, force) {
                count += 1;
            }
        }
        count
    }

    fn schedule_reevaluation(&mut self, h: WindowHandle) {
        for delay in REEVALUATE_AFTER_MS {
            self.schedule_timer(delay, TimerKind::Reevaluate(h));
        }
    }

    fn schedule_timer(&mut self, delay_ms: u32, kind: TimerKind) {
        self.next_timer = self.next_timer.wrapping_add(1).max(1);
        let id = self.next_timer;
        // SAFETY: one-shot timer on our own message window, killed when it fires.
        let ok = unsafe { SetTimer(Some(self.msg_window.hwnd()), id, delay_ms, None) } != 0;
        if ok {
            self.timers.insert(id, kind);
        }
    }

    /// A managed window's destroy notification is waiting to be confirmed; a window shown
    /// meanwhile may be its replacement and must be re-checked once it is confirmed.
    fn destroy_check_pending(&self) -> bool {
        self.timers
            .values()
            .any(|t| matches!(t, TimerKind::VerifyDestroyed(_)))
    }

    /// Forgets a destroyed window (keeping its identity for reattachment if managed).
    fn forget_destroyed(&mut self, h: WindowHandle) {
        self.rule_applied.remove(&h);
        self.exe_cache.remove(&h);
        if self.manager.contains(h) {
            let grace = Duration::from_secs(u64::from(self.settings.general.reattach_grace_secs));
            self.manager.on_destroyed(h, grace);
            self.frames.remove(&h);
            self.publish();
        }
    }

    fn on_timer(&mut self, id: usize) {
        // SAFETY: kills a timer we created on our own window.
        unsafe {
            let _ = KillTimer(Some(self.msg_window.hwnd()), id);
        }
        if id == SESSION_TIMER {
            self.session_save_pending = false;
            self.session.save(self.manager.entries());
            return;
        }
        let mut changed = false;
        match self.timers.remove(&id) {
            Some(TimerKind::VerifyDestroyed(h)) => {
                let alive = self
                    .manager
                    .get(h)
                    .is_some_and(|e| window::is_same_window(h, e.pid));
                if alive {
                    tracing::warn!(hwnd = %h, "ignored a destroy notification for a window that still exists");
                } else {
                    self.forget_destroyed(h);
                }
            }
            Some(TimerKind::Reevaluate(h)) if window::exists(h) && !self.manager.contains(h) => {
                // UWP frames resolve to their real app only after a moment: retry the
                // handle-change reattachment and the rules now.
                changed |= self.try_reattach(h) || self.evaluate_rules(h, false);
            }
            _ => {}
        }
        if enabled(&self.settings) && self.manager.has_pending_pins() {
            changed |= self.manager.retry_pending_pins();
        }
        if changed {
            self.publish();
        }
    }

    fn sync_name_hook(&mut self) {
        let wanted = enabled(&self.settings) && any_title_rules(&self.settings.rules);
        if wanted && self.name_hook.is_none() {
            self.name_hook =
                WinEventHook::install(EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE, 0);
        } else if !wanted {
            self.name_hook = None;
        }
    }

    // ---------------------------------------------------------------- frames

    fn frames_wanted(&self) -> bool {
        enabled(&self.settings) && self.settings.pinning.show_border
    }

    fn wanted_frames(&self) -> HashSet<WindowHandle> {
        if self.frames_wanted() {
            self.manager.effectively_pinned_handles().collect()
        } else {
            HashSet::new()
        }
    }

    fn sync_frames(&mut self) {
        let wanted = self.wanted_frames();
        self.framed.clone_from(&wanted);
        self.frames.retain(|h, _| wanted.contains(h));
        for h in &wanted {
            self.update_frame(*h);
        }
        // One location hook per process that owns a pinned window.
        let pids: HashSet<u32> = self
            .manager
            .entries()
            .iter()
            .filter(|e| e.pinned && wanted.contains(&e.handle()))
            .map(|e| e.pid)
            .collect();
        self.location_hooks.retain(|pid, _| pids.contains(pid));
        for pid in pids {
            if !self.location_hooks.contains_key(&pid)
                && let Some(hook) = WinEventHook::install(
                    EVENT_OBJECT_LOCATIONCHANGE,
                    EVENT_OBJECT_LOCATIONCHANGE,
                    pid,
                )
            {
                self.location_hooks.insert(pid, hook);
            }
        }
    }

    fn update_frame(&mut self, h: WindowHandle) {
        if !self.frames_wanted()
            || !self
                .manager
                .get(h)
                .is_some_and(|e| e.pinned && !e.pin_pending)
        {
            self.frames.remove(&h);
            return;
        }
        let color = parse_hex_color(&self.settings.pinning.border_color).unwrap_or((245, 158, 11));
        if let std::collections::hash_map::Entry::Vacant(e) = self.frames.entry(h) {
            match FrameOverlay::create(color) {
                Some(f) => {
                    e.insert(f);
                }
                None => return,
            }
        }
        let Some(frame) = self.frames.get_mut(&h) else {
            return;
        };
        frame.set_color(color);
        let hidden = !window::exists(h)
            || !window::is_visible(h)
            || window::is_minimized(h)
            || window::is_cloaked(h);
        let Some(rect) = (!hidden).then(|| window::frame_rect(h)).flatten() else {
            frame.hide();
            return;
        };
        let dpi = monitor::dpi_for_rect(rect);
        let thickness = logical_to_physical(
            i32::try_from(self.settings.pinning.border_thickness).unwrap_or(2),
            dpi,
        );
        let z = match window::window_above(h) {
            Some(above) if above == frame.handle() => ZOrder::Keep,
            Some(above) => ZOrder::After(above),
            None => ZOrder::Topmost,
        };
        frame.show(rect, thickness, z);
    }

    fn hide_frame(&mut self, h: WindowHandle) {
        if let Some(f) = self.frames.get_mut(&h) {
            f.hide();
        }
    }

    // ---------------------------------------------------------------- events

    fn on_win_event(&mut self, ev: WinEventRecord) {
        let h = ev.hwnd;
        match ev.event {
            EVENT_OBJECT_DESTROY => {
                // Any process can raise WinEvents, so a managed window is only forgotten once
                // it is really gone. The notification may arrive while DestroyWindow is
                // still running, so a window that still exists is checked again shortly.
                if self
                    .manager
                    .get(h)
                    .is_some_and(|e| window::is_same_window(h, e.pid))
                {
                    self.schedule_timer(DESTROY_RECHECK_MS, TimerKind::VerifyDestroyed(h));
                    return;
                }
                self.forget_destroyed(h);
            }
            EVENT_OBJECT_SHOW => self.on_shown(h),
            EVENT_OBJECT_HIDE | EVENT_SYSTEM_MINIMIZESTART | EVENT_OBJECT_CLOAKED => {
                self.hide_frame(h)
            }
            EVENT_OBJECT_UNCLOAKED
            | EVENT_SYSTEM_MINIMIZEEND
            | EVENT_SYSTEM_MOVESIZEEND
            | EVENT_SYSTEM_FOREGROUND => {
                // Pins Windows refused while a full-screen app had the foreground usually
                // become possible once the foreground changes.
                if ev.event == EVENT_SYSTEM_FOREGROUND
                    && enabled(&self.settings)
                    && self.manager.has_pending_pins()
                    && self.manager.retry_pending_pins()
                {
                    self.publish();
                }
                if self.manager.contains(h) {
                    if self.settings.pinning.reassert_state
                        && enabled(&self.settings)
                        && self.manager.reassert(h)
                    {
                        // A pending pin may have been applied: frames, session and UI.
                        self.publish();
                    }
                    self.update_frame(h);
                }
            }
            EVENT_OBJECT_LOCATIONCHANGE => {
                if self.frames.contains_key(&h) {
                    self.update_frame(h);
                }
            }
            EVENT_OBJECT_NAMECHANGE => {
                if !self.manager.contains(h)
                    && !self.rule_applied.contains_key(&h)
                    && window::root(h) == h
                    && self.evaluate_rules(h, false)
                {
                    self.publish();
                }
            }
            EVENT_SYSTEM_DESKTOPSWITCH => {
                // UAC prompt or lock screen: abort any gesture whose button-up we will not see.
                // SAFETY: pointer-free message to the gesture thread.
                unsafe {
                    let _ = PostThreadMessageW(
                        self.peers.gesture_thread,
                        WM_APP_GESTURE_ABORT,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            }
            _ => {}
        }
    }

    fn on_shown(&mut self, h: WindowHandle) {
        if self.manager.contains(h) {
            self.update_frame(h);
            return;
        }
        if !enabled(&self.settings)
            || !window::is_app_window(h)
            || window::process_id(h) == self.ctx.own_pid
        {
            return;
        }
        if self.try_reattach(h) || self.evaluate_rules(h, false) {
            self.publish();
        } else if (self.has_enabled_rules() && !self.rule_applied.contains_key(&h))
            || self.manager.has_departed()
            || self.destroy_check_pending()
        {
            self.schedule_reevaluation(h);
        }
    }

    /// A managed window of the same application was destroyed moments ago: `h` is most
    /// likely its replacement (handle change or quick restart). Returns `true` when the
    /// previous state was re-applied.
    fn try_reattach(&mut self, h: WindowHandle) -> bool {
        if !self.manager.has_departed() {
            return false;
        }
        let grace = Duration::from_secs(u64::from(self.settings.general.reattach_grace_secs));
        let exe = self.exe_for(h);
        let Some(prev) =
            self.manager
                .take_departed(&exe, &window::class_name(h), &window::title(h), grace)
        else {
            return false;
        };
        let desired = Desired {
            pinned: prev.pinned,
            opacity: prev.opacity,
            source: prev.source,
            rule_id: prev.rule_id.clone(),
            defer_refused_pin: true,
        };
        match self.manager.manage(h, desired) {
            Ok(_) => {
                if let Some(id) = prev.rule_id {
                    self.rule_applied.insert(h, id);
                }
                tracing::info!(hwnd = %h, previous = prev.hwnd, "reattached state to replacement window");
                true
            }
            Err(e) => {
                tracing::warn!(hwnd = %h, error = %e, "could not reattach state");
                false
            }
        }
    }

    // ---------------------------------------------------------------- hotkeys

    fn on_hotkey(&mut self, action: HotkeyAction) {
        if !enabled(&self.settings) {
            return;
        }
        // SAFETY: no arguments; returns the foreground window or null.
        let fg = WindowHandle::from_hwnd(unsafe { GetForegroundWindow() });
        let h = window::root(fg);
        if h.is_null() || window::process_id(h) == self.ctx.own_pid || window::is_shell_window(h) {
            return;
        }
        let step = self.settings.hotkeys.opacity_step;
        let result = match action {
            HotkeyAction::TogglePin => match self.manager.get(h).map(|e| e.pinned) {
                Some(true) => self.unpin(h).map(|()| "unpinned"),
                _ => self.pin(h).map(|_| "pinned"),
            },
            HotkeyAction::OpacityUp | HotkeyAction::OpacityDown => {
                let current = self.manager.get(h).map_or(100, |e| e.opacity);
                let next = if action == HotkeyAction::OpacityUp {
                    current.saturating_add(step).min(100)
                } else {
                    current.saturating_sub(step)
                };
                self.set_opacity(h, next).map(|_| "opacity changed")
            }
        };
        match result {
            Ok(what) => tracing::info!(hwnd = %h, ?action, what, "hotkey handled"),
            Err(e) => self.notice(NoticeLevel::Warning, e.to_string(), Some(h)),
        }
    }

    // ---------------------------------------------------------------- lifecycle

    fn recover_session(&mut self) {
        let entries = self.session.load();
        if entries.is_empty() {
            return;
        }
        let recover = self.settings.general.recover_session;
        let mut adopted = 0;
        for mut entry in entries {
            let h = entry.handle();
            if !window::is_same_window(h, entry.pid) {
                continue;
            }
            let same_exe = process::exe_path(process::effective_pid(h))
                .is_some_and(|p| p.eq_ignore_ascii_case(&entry.exe_path));
            if !same_exe {
                continue;
            }
            // A window whose release had failed is restored, never re-adopted.
            if recover && !entry.release_pending {
                if entry.source == ManagedSource::Manual {
                    entry.source = ManagedSource::Recovered;
                }
                if let Some(id) = &entry.rule_id {
                    self.rule_applied.insert(h, id.clone());
                }
                match self.manager.adopt(entry) {
                    Ok(()) => adopted += 1,
                    Err(e) => tracing::warn!(hwnd = %h, error = %e, "could not re-adopt window"),
                }
            } else if let Err(e) = restore_entry(&entry) {
                tracing::warn!(hwnd = %h, error = %e, "could not restore window from previous session");
            }
        }
        tracing::info!(adopted, recover, "previous session processed");
        // Persist the outcome right away (not debounced): the previous file must not
        // survive into a second crash with stale entries.
        self.session.save(self.manager.entries());
        self.publish();
    }

    fn handle_command(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::Pin { hwnd, reply } => {
                let r = self.resolve(hwnd).and_then(|h| self.pin(h));
                let _ = reply.send(r);
            }
            Command::Unpin { hwnd, reply } => {
                let r = WindowHandle::from_u64(hwnd)
                    .ok_or(EngineError::WindowNotFound)
                    .and_then(|h| self.unpin(h));
                let _ = reply.send(r);
            }
            Command::SetOpacity {
                hwnd,
                opacity,
                reply,
            } => {
                let r = if self
                    .manager
                    .contains(WindowHandle::from_u64(hwnd).unwrap_or(WindowHandle::from_raw(0)))
                {
                    WindowHandle::from_u64(hwnd)
                        .ok_or(EngineError::WindowNotFound)
                        .and_then(|h| self.set_opacity(h, opacity))
                } else {
                    self.resolve(hwnd)
                        .and_then(|h| self.set_opacity(h, opacity))
                };
                let _ = reply.send(r);
            }
            Command::UnpinAll { reply } => {
                let failures = self.manager.release_all();
                self.publish();
                let r = if failures == 0 {
                    Ok(())
                } else {
                    Err(EngineError::Unsupported(format!(
                        "{failures} window(s) could not be restored yet (usually because the \
                         application is not responding). They stay listed; try again later."
                    )))
                };
                let _ = reply.send(r);
            }
            Command::ListManaged { reply } => {
                let _ = reply.send(Ok(self.manager.infos()));
            }
            Command::ListWindows { reply } => {
                let _ = reply.send(Ok(self.list_windows()));
            }
            Command::ApplySettings { settings, reply } => {
                let statuses = self.apply_settings(*settings);
                let _ = reply.send(Ok(statuses));
            }
            Command::ApplyRulesNow { reply } => {
                let r = if enabled(&self.settings) {
                    let n = self.apply_rules_to_existing(true);
                    self.publish();
                    Ok(n)
                } else {
                    Err(EngineError::InvalidArgument(
                        "Window management is turned off.".into(),
                    ))
                };
                let _ = reply.send(r);
            }
            Command::Activate { hwnd, reply } => {
                let r = self.resolve(hwnd).and_then(|h| self.activate(h));
                let _ = reply.send(r);
            }
            Command::HotkeyStatus { reply } => {
                let _ = reply.send(Ok(self.hotkeys.status()));
            }
            Command::Shutdown { restore, reply } => {
                self.shutdown(restore);
                let _ = reply.send(Ok(()));
                return true;
            }
        }
        false
    }

    fn shutdown(&mut self, restore: bool) {
        if self.session_save_pending {
            // SAFETY: kills our own pending timer; the session is written below.
            unsafe {
                let _ = KillTimer(Some(self.msg_window.hwnd()), SESSION_TIMER);
            }
            self.session_save_pending = false;
        }
        // Record the current state first: if a restore below blocks on a hung application
        // and this thread is abandoned, the session file is still up to date.
        self.session.save(self.manager.entries());
        if restore {
            let failures = self.manager.release_all();
            if failures > 0 {
                tracing::warn!(failures, "some windows could not be restored on exit");
            }
        }
        // With `restore`, this keeps only windows that could not be restored, so the next
        // start can still restore them; otherwise it keeps everything for re-adoption.
        self.session.save(self.manager.entries());
        self.frames.clear();
        self.location_hooks.clear();
        self.name_hook = None;
        self.base_hooks.clear();
        self.hotkeys.apply(&self.settings.hotkeys, false);
        tracing::info!(restore, "engine thread shut down");
    }
}

/// Info for a window that is not managed (used when opacity 100 is requested for it).
fn enumerated_info(h: WindowHandle) -> ManagedWindowInfo {
    let info = enumerator::window_info(h, false);
    ManagedWindowInfo {
        hwnd: info.hwnd,
        title: info.title,
        exe_name: info.exe_name,
        exe_path: info.exe_path,
        pid: info.pid,
        pinned: false,
        pin_pending: false,
        opacity: 100,
        source: ManagedSource::Manual,
        rule_id: None,
    }
}

unsafe extern "system" fn engine_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    // SAFETY: forwarding to the default procedure; everything else is handled in the loop.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn create_message_window() -> Option<WindowHandle> {
    // SAFETY: registers a class with a static name and creates a message-only window on
    // this thread; failures are reported as None.
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class = WNDCLASSEXW {
            cbSize: u32::try_from(std::mem::size_of::<WNDCLASSEXW>()).unwrap_or(80),
            lpfnWndProc: Some(engine_window_proc),
            hInstance: instance.into(),
            lpszClassName: ENGINE_WINDOW_CLASS,
            ..WNDCLASSEXW::default()
        };
        // Registration fails harmlessly with "class already exists" when an engine is
        // restarted in the same process.
        RegisterClassExW(&raw const class);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            ENGINE_WINDOW_CLASS,
            w!("AWWM"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        )
        .ok()?;
        Some(WindowHandle::from_hwnd(hwnd))
    }
}

/// Engine thread entry point.
pub fn run(
    ctx: Arc<SharedState>,
    settings: Settings,
    session: SessionStore,
    peers: Peers,
    commands: Receiver<Command>,
    ready: std::sync::mpsc::Sender<Result<u32, String>>,
) {
    crate::win::make_thread_dpi_aware();
    let tid = crate::win::init_message_queue();
    events::init_consumer(tid);
    let Some(msg_window) = create_message_window() else {
        let _ = ready.send(Err("Could not create the engine message window.".into()));
        return;
    };
    // SAFETY: registers (or looks up) a system-wide message id for a static string.
    let show_message = unsafe { RegisterWindowMessageW(SHOW_MESSAGE_NAME) };
    let mut engine = EngineThread {
        ctx,
        settings,
        manager: Manager::default(),
        session,
        msg_window,
        show_message,
        peers,
        hotkeys: Hotkeys::new(msg_window),
        frames: HashMap::new(),
        location_hooks: HashMap::new(),
        base_hooks: Vec::new(),
        name_hook: None,
        rule_applied: HashMap::new(),
        exe_cache: HashMap::new(),
        timers: HashMap::new(),
        next_timer: 0,
        framed: HashSet::new(),
        session_save_pending: false,
        saved_shape: Vec::new(),
    };
    for (min, max) in [
        (EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE),
        (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
        (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
        (EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZEEND),
        (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
        (EVENT_SYSTEM_DESKTOPSWITCH, EVENT_SYSTEM_DESKTOPSWITCH),
    ] {
        if let Some(hook) = WinEventHook::install(min, max, 0) {
            engine.base_hooks.push(hook);
        }
    }
    engine.sync_name_hook();
    engine.recover_session();
    if enabled(&engine.settings) && engine.settings.general.apply_rules_on_startup {
        let n = engine.apply_rules_to_existing(false);
        if n > 0 {
            engine.publish();
        }
    }
    let statuses = engine
        .hotkeys
        .apply(&engine.settings.hotkeys, enabled(&engine.settings));
    engine.emit(EngineEvent::HotkeysChanged(statuses));
    let _ = ready.send(Ok(tid));
    tracing::info!("engine thread running");

    let mut msg = MSG::default();
    'outer: loop {
        // SAFETY: `msg` is a valid out buffer; the loop ends on WM_QUIT (0) or error (-1).
        let r = unsafe { GetMessageW(&raw mut msg, None, 0, 0) };
        if r.0 <= 0 {
            break;
        }
        let thread_msg = msg.hwnd.is_invalid();
        if thread_msg && msg.message == WM_APP_COMMAND {
            while let Ok(cmd) = commands.try_recv() {
                if engine.handle_command(cmd) {
                    break 'outer;
                }
            }
        } else if thread_msg && msg.message == WM_APP_EVENTS {
            // Process in batches; events produced while handling are picked up next round.
            for ev in events::drain() {
                engine.on_win_event(ev);
            }
        } else if thread_msg && msg.message == WM_APP_FRAME_SYNC {
            engine
                .ctx
                .frame_sync_pending
                .store(false, std::sync::atomic::Ordering::Release);
            engine.update_frame(WindowHandle::from_raw(msg.lParam.0));
        } else if msg.hwnd == msg_window.hwnd() && msg.message == WM_TIMER {
            engine.on_timer(msg.wParam.0);
        } else if msg.hwnd == msg_window.hwnd() && msg.message == WM_HOTKEY {
            if let Some(action) = i32::try_from(msg.wParam.0).ok().and_then(action_for_id) {
                engine.on_hotkey(action);
            }
        } else if msg.hwnd == msg_window.hwnd()
            && msg.message == engine.show_message
            && engine.show_message != 0
        {
            engine.emit(EngineEvent::ShowRequested);
        } else {
            // SAFETY: standard dispatch (pin-frame overlay painting).
            unsafe {
                let _ = TranslateMessage(&raw const msg);
                DispatchMessageW(&raw const msg);
            }
        }
    }
    drop(engine.hotkeys);
    // SAFETY: destroys the message window created on this thread.
    unsafe {
        let _ = DestroyWindow(msg_window.hwnd());
    }
    tracing::info!("engine thread stopped");
}
