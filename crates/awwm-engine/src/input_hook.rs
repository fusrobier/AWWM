//! global_mouse_hook + keyboard_state: the low-level input hook thread.
//!
//! `WH_MOUSE_LL` / `WH_KEYBOARD_LL` callbacks run on this thread (through its message
//! loop) for every input event in the session, so they do the bare minimum: track
//! modifier state, decide synchronously whether a button event starts/ends a gesture
//! (and must be swallowed), and forward everything else to the gesture thread through a
//! coalescing queue. They never move windows, enumerate windows or perform I/O.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU,
    VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, LLKHF_LOWER_IL_INJECTED,
    LLMHF_LOWER_IL_INJECTED, MSG, MSLLHOOKSTRUCT, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use awwm_core::Point;
use awwm_core::gesture::{GestureKind, Modifier, ModifierState, MouseButton};
use awwm_core::rules::exe_matches;
use awwm_core::settings::GestureScope;

use crate::shared::{GestureMsg, INJECTED_MARKER, SharedState, WM_APP_HOOKS, unpoison};
use crate::win::{WindowHandle, monitor, point_to_win, process, window};

/// Context for the hook callbacks (they receive no user pointer).
static HOOK_CONTEXT: RwLock<Option<Arc<SharedState>>> = RwLock::new(None);

fn context() -> Option<Arc<SharedState>> {
    unpoison(HOOK_CONTEXT.read()).clone()
}

#[derive(Clone, Copy, Debug)]
struct ActiveGesture {
    seq: u64,
    button: MouseButton,
}

/// Hook-thread-only state.
#[derive(Default)]
struct HookState {
    mods: ModifierState,
    active: Option<ActiveGesture>,
    /// Buttons whose down event we swallowed and whose up event must be swallowed too.
    swallowed_buttons: HashSet<u8>,
    /// Keys whose down event we swallowed during a gesture.
    swallowed_keys: HashSet<u32>,
    /// Snap state last sent to the gesture thread for the active gesture.
    last_snap: Option<bool>,
    /// Virtual keys currently held (seen down, not yet up) with the time of their latest
    /// key-down. Distinguishes a fresh press from keyboard auto-repeat, which arrives as
    /// further key-down events.
    down_keys: HashMap<u32, u32>,
}

thread_local! {
    static STATE: RefCell<HookState> = RefCell::new(HookState::default());
}

const fn button_id(b: MouseButton) -> u8 {
    match b {
        MouseButton::Left => 0,
        MouseButton::Right => 1,
        MouseButton::Middle => 2,
    }
}

fn modifier_for_vk(vk: u32) -> Option<Modifier> {
    let vk = u16::try_from(vk).ok()?;
    match vk {
        v if v == VK_MENU.0 || v == VK_LMENU.0 || v == VK_RMENU.0 => Some(Modifier::Alt),
        v if v == VK_CONTROL.0 || v == VK_LCONTROL.0 || v == VK_RCONTROL.0 => Some(Modifier::Ctrl),
        v if v == VK_SHIFT.0 || v == VK_LSHIFT.0 || v == VK_RSHIFT.0 => Some(Modifier::Shift),
        v if v == VK_LWIN.0 || v == VK_RWIN.0 => Some(Modifier::Win),
        _ => None,
    }
}

fn async_down(vk: u16) -> bool {
    // SAFETY: plain query of the asynchronous key state.
    (unsafe { GetAsyncKeyState(i32::from(vk)) } as u16) & 0x8000 != 0
}

/// Physical modifier state as reported by the system (used to resynchronise our tracking
/// in case a key-up was delivered to another desktop, e.g. after Win+L).
fn async_modifiers() -> ModifierState {
    ModifierState {
        alt: async_down(VK_MENU.0),
        ctrl: async_down(VK_CONTROL.0),
        shift: async_down(VK_SHIFT.0),
        win: async_down(VK_LWIN.0) || async_down(VK_RWIN.0),
    }
}

/// Modifier state used to classify a button press.
///
/// The system key state (`GetAsyncKeyState`) is the source of truth: AWWM never swallows
/// the activation modifier, and the system state is correct even when this hook missed a
/// key event (another low-level hook earlier in the chain, e.g. a WebView's keyboard
/// capture, can keep events from reaching it; a key-up can go to another desktop). The only
/// keys the system does not see are snap-modifier presses AWWM swallowed during an earlier
/// gesture that are still held; they are added from `swallowed_keys`.
fn effective_modifiers(synced: ModifierState, swallowed_keys: &HashSet<u32>) -> ModifierState {
    swallowed_keys
        .iter()
        .filter_map(|&vk| modifier_for_vk(vk))
        .fold(synced, |m, held| m.with(held, true))
}

/// Reason a button press did not start a gesture.
enum Refusal {
    /// Pass the event through silently (the reason is logged at debug level).
    Ignore(&'static str),
    /// Pass it through and tell the user the window is elevated.
    Elevated(WindowHandle),
}

/// Decides whether the window under `pt` may be moved/resized. Only cheap, non-blocking
/// queries are used (no messages are sent to the target).
fn eligible_target(
    ctx: &SharedState,
    pt: Point,
    kind: GestureKind,
) -> Result<WindowHandle, Refusal> {
    // SAFETY: WindowFromPoint only reads window-manager state.
    let hit = WindowHandle::from_hwnd(unsafe {
        windows::Win32::UI::WindowsAndMessaging::WindowFromPoint(point_to_win(pt))
    });
    if hit.is_null() {
        return Err(Refusal::Ignore("no window under the cursor"));
    }
    let target = window::root(hit);
    if target.is_null() || !window::is_visible(target) || window::is_shell_window(target) {
        return Err(Refusal::Ignore("desktop, shell or invisible window"));
    }
    let pid = window::process_id(target);
    if pid == 0 || pid == ctx.own_pid {
        return Err(Refusal::Ignore("AWWM's own window"));
    }
    // Copy the few values needed so no lock is held across the Win32 calls below.
    let (scope, restore_maximized, ignore_fullscreen, has_exclusions) = {
        let cfg = unpoison(ctx.gesture.read());
        (
            cfg.scope,
            cfg.restore_maximized_on_move,
            cfg.ignore_fullscreen,
            !cfg.excluded_apps.is_empty(),
        )
    };
    if scope == GestureScope::PinnedOnly && !unpoison(ctx.pinned.read()).contains(&target) {
        return Err(Refusal::Ignore("not pinned (pinned-only scope)"));
    }
    if window::is_maximized(target) && (kind == GestureKind::Resize || !restore_maximized) {
        return Err(Refusal::Ignore("maximized"));
    }
    if window::is_minimized(target) {
        return Err(Refusal::Ignore("minimized"));
    }
    if ignore_fullscreen
        && monitor::from_point(pt).is_some_and(|m| window::is_fullscreen(target, m.bounds))
    {
        return Err(Refusal::Ignore("full-screen window"));
    }
    if has_exclusions {
        let exe = process::exe_path(process::effective_pid(target)).unwrap_or_default();
        let excluded = unpoison(ctx.gesture.read())
            .excluded_apps
            .iter()
            .any(|pattern| exe_matches(pattern, &exe));
        if excluded {
            return Err(Refusal::Ignore("excluded application"));
        }
    }
    if process::access(pid) == process::Access::HigherIntegrity {
        return Err(Refusal::Elevated(target));
    }
    if window::is_hung(target) {
        return Err(Refusal::Ignore("not responding"));
    }
    Ok(target)
}

/// Outcome of the button bookkeeping that precedes gesture classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ButtonStep {
    Swallow,
    Pass,
    /// A button-down that may start a gesture.
    Classify,
}

/// Swallow bookkeeping for a button event. Invariant: a button-up is swallowed only when
/// the button-down of the same press was swallowed, so Windows and applications never see
/// a mouse button stuck down.
fn button_step(
    st: &mut HookState,
    ctx: &SharedState,
    button: MouseButton,
    down: bool,
) -> ButtonStep {
    let id = button_id(button);
    // A gesture the gesture thread aborted by itself is no longer live: stop intercepting
    // other buttons (its own button-up is still swallowed below).
    if let Some(a) = st.active
        && ctx.gesture_aborted.load(Ordering::Acquire) == a.seq
        && a.button != button
    {
        st.active = None;
    }
    if !down {
        if st.active.is_some_and(|a| a.button == button) {
            st.active = None;
            st.swallowed_buttons.remove(&id);
            ctx.gestures.push(GestureMsg::End);
            return ButtonStep::Swallow;
        }
        return if st.swallowed_buttons.remove(&id) {
            ButtonStep::Swallow
        } else {
            ButtonStep::Pass
        };
    }
    // A new press proves any up event we were waiting to swallow for this button was lost
    // (desktop switch, skipped hook call). Forget it so this press's own up event is never
    // swallowed unless we swallow this press too.
    st.swallowed_buttons.remove(&id);
    if let Some(a) = st.active {
        if a.button == button {
            // The up event of the previous gesture was lost: finish it and evaluate this
            // press normally.
            st.active = None;
            ctx.gestures.push(GestureMsg::End);
        } else {
            // Other buttons are ignored (as complete press/release pairs) during a gesture.
            st.swallowed_buttons.insert(id);
            return ButtonStep::Swallow;
        }
    }
    ButtonStep::Classify
}

fn handle_mouse(msg: u32, info: &MSLLHOOKSTRUCT) -> bool {
    if info.dwExtraInfo == INJECTED_MARKER {
        return false;
    }
    let Some(ctx) = context() else { return false };
    let pt = Point::new(info.pt.x, info.pt.y);
    let (button, down) = match msg {
        WM_MOUSEMOVE => {
            STATE.with(|cell| {
                let Ok(mut st) = cell.try_borrow_mut() else {
                    return;
                };
                let Some(a) = st.active else { return };
                if ctx.gesture_aborted.load(Ordering::Acquire) == a.seq {
                    return;
                }
                ctx.gestures.push(GestureMsg::Move { cursor: pt });
                // Another low-level keyboard hook earlier in the chain (a focused WebView,
                // including AWWM's own settings window) can keep key-down events from this
                // hook. Check the system key state while the pointer moves so Esc and the
                // snap modifier still work.
                if async_down(VK_ESCAPE.0) {
                    ctx.gestures.push(GestureMsg::Cancel);
                }
                let bindings = *unpoison(ctx.bindings.read());
                let snap = bindings
                    .snap_active(effective_modifiers(async_modifiers(), &st.swallowed_keys))
                    || bindings.snap_active(st.mods);
                if st.last_snap != Some(snap) {
                    st.last_snap = Some(snap);
                    ctx.gestures.push(GestureMsg::Snap { active: snap });
                }
            });
            return false;
        }
        WM_LBUTTONDOWN => (MouseButton::Left, true),
        WM_LBUTTONUP => (MouseButton::Left, false),
        WM_RBUTTONDOWN => (MouseButton::Right, true),
        WM_RBUTTONUP => (MouseButton::Right, false),
        WM_MBUTTONDOWN => (MouseButton::Middle, true),
        WM_MBUTTONUP => (MouseButton::Middle, false),
        _ => return false,
    };
    STATE.with(|cell| {
        let Ok(mut st) = cell.try_borrow_mut() else {
            return false;
        };
        let id = button_id(button);
        match button_step(&mut st, &ctx, button, down) {
            ButtonStep::Swallow => return true,
            ButtonStep::Pass => return false,
            ButtonStep::Classify => {}
        }

        // Input synthesized by a process at a lower integrity level must never start a
        // gesture: otherwise such a process could use AWWM to move windows it may not
        // touch itself (UIPI confused deputy).
        if info.flags & LLMHF_LOWER_IL_INJECTED != 0 {
            return false;
        }
        let synced = async_modifiers();
        let tracked = st.mods;
        let mods = effective_modifiers(synced, &st.swallowed_keys);
        // Re-synchronise our tracking with the truth at every press.
        st.mods = mods;
        let bindings = *unpoison(ctx.bindings.read());
        let involved = tracked.is_down(bindings.modifier) || synced.is_down(bindings.modifier);
        let Some(kind) = bindings.classify(button, mods) else {
            if involved {
                tracing::debug!(
                    ?button,
                    ?tracked,
                    ?synced,
                    "modifier+click did not match a gesture"
                );
            }
            return false;
        };
        match eligible_target(&ctx, pt, kind) {
            Ok(target) => {
                let seq = ctx.gesture_started.fetch_add(1, Ordering::AcqRel) + 1;
                st.active = Some(ActiveGesture { seq, button });
                st.swallowed_buttons.insert(id);
                st.last_snap = Some(bindings.snap_active(mods));
                ctx.gestures.push(GestureMsg::Start {
                    seq,
                    target,
                    kind,
                    cursor: pt,
                    snap: bindings.snap_active(mods),
                    suppress_menu: matches!(bindings.modifier, Modifier::Alt | Modifier::Win),
                });
                true
            }
            Err(Refusal::Elevated(target)) => {
                ctx.gestures.push(GestureMsg::Blocked { target });
                false
            }
            Err(Refusal::Ignore(reason)) => {
                tracing::debug!(?kind, reason, "gesture not started");
                false
            }
        }
    })
}

fn handle_key(msg: u32, info: &KBDLLHOOKSTRUCT) -> bool {
    // Our own neutral key, and keys synthesized by lower-integrity processes (which must
    // not be able to fake the activation modifier), are ignored entirely.
    if info.dwExtraInfo == INJECTED_MARKER {
        return false;
    }
    if info.flags.contains(LLKHF_LOWER_IL_INJECTED) {
        tracing::debug!(
            vk = info.vkCode,
            flags = info.flags.0,
            "ignored key injected at a lower integrity level"
        );
        return false;
    }
    if info.vkCode == u32::from(VK_ESCAPE.0) || modifier_for_vk(info.vkCode).is_some() {
        tracing::trace!(vk = info.vkCode, msg, flags = info.flags.0, "key event");
    }
    let Some(ctx) = context() else { return false };
    let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
    let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
    if !down && !up {
        return false;
    }
    let vk = info.vkCode;
    STATE.with(|cell| {
        let Ok(mut st) = cell.try_borrow_mut() else {
            return false;
        };
        let live = st
            .active
            .is_some_and(|a| ctx.gesture_aborted.load(Ordering::Acquire) != a.seq);
        process_key(&mut st, &ctx, vk, down, info.time, live)
    })
}

/// Longest gap between two auto-repeat key-down events of a held key with default keyboard
/// settings (repeat delay ≤ 1 s). Only used to recognise a stale *swallowed* press.
const MAX_REPEAT_GAP_MS: u32 = 1_200;

/// Key bookkeeping + swallow decision for one key event (`down == false` means key-up).
///
/// A key-down for a key we already saw go down (and not up) is an auto-repeat:
/// * if that press was passed to the system, the repeat is never swallowed — whatever the
///   repeat timing (accessibility settings can space repeats seconds apart) — so a key the
///   system saw go down always gets its key-up;
/// * if that press was swallowed, the repeat is swallowed too, unless it arrives so late
///   that the press must have ended unseen (key-up delivered to another desktop); then it
///   counts as a new press. Either way the system sees complete down/up pairs.
fn process_key(
    st: &mut HookState,
    ctx: &SharedState,
    vk: u32,
    down: bool,
    time: u32,
    live: bool,
) -> bool {
    let up = !down;
    let fresh = if down {
        match st.down_keys.insert(vk, time) {
            None => true,
            Some(previous) => {
                st.swallowed_keys.contains(&vk) && time.wrapping_sub(previous) > MAX_REPEAT_GAP_MS
            }
        }
    } else {
        let seen_down = st.down_keys.remove(&vk).is_some();
        // Another low-level hook earlier in the chain (a focused WebView, including AWWM's
        // own settings window) can hide key-downs from this hook while key-ups still arrive.
        // An Esc released during a gesture whose press we never saw still cancels it.
        if live && !seen_down && vk == u32::from(VK_ESCAPE.0) {
            ctx.gestures.push(GestureMsg::Cancel);
        }
        false
    };
    let modifier = modifier_for_vk(vk);
    if let Some(m) = modifier {
        st.mods = st.mods.with(m, down);
    }
    decide_key(st, ctx, vk, modifier, down, up, fresh, live)
}

/// Decides whether a key event is swallowed. Invariant: a key-up is swallowed only when
/// the fresh key-down of the same press was swallowed, so Windows and applications never
/// see a key stuck down.
#[allow(clippy::too_many_arguments)]
fn decide_key(
    st: &mut HookState,
    ctx: &SharedState,
    vk: u32,
    modifier: Option<Modifier>,
    down: bool,
    up: bool,
    fresh: bool,
    live: bool,
) -> bool {
    let bindings = *unpoison(ctx.bindings.read());
    // Pressing or releasing a modifier during a gesture re-evaluates snapping at once.
    if live && modifier.is_some() && (up || fresh) {
        let active = bindings.snap_active(st.mods);
        st.last_snap = Some(active);
        ctx.gestures.push(GestureMsg::Snap { active });
    }
    if up {
        return st.swallowed_keys.remove(&vk);
    }
    if !down {
        return false;
    }
    if !fresh {
        // Auto-repeat: swallow it exactly when the original press was swallowed.
        return st.swallowed_keys.contains(&vk);
    }
    // A fresh press: any swallow pending for this key belonged to a press whose key-up
    // we never saw (it went to another desktop); it must not affect this press.
    st.swallowed_keys.remove(&vk);
    if !live {
        return false;
    }
    let swallow = vk == u32::from(VK_ESCAPE.0)
        || (modifier.is_some()
            && modifier == bindings.snap_modifier
            && modifier != Some(bindings.modifier));
    if vk == u32::from(VK_ESCAPE.0) {
        ctx.gestures.push(GestureMsg::Cancel);
    }
    if swallow {
        // Esc cancels the gesture; a snap-modifier press must not reach the target as a
        // shortcut. Their key-ups are swallowed too.
        st.swallowed_keys.insert(vk);
    }
    swallow
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: for HC_ACTION, lParam points to a MSLLHOOKSTRUCT valid for this call.
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let msg = u32::try_from(wparam.0).unwrap_or(0);
        // A panic must never unwind into the system hook chain.
        if catch_unwind(AssertUnwindSafe(|| handle_mouse(msg, info))).unwrap_or(false) {
            return LRESULT(1);
        }
    }
    // SAFETY: forwarding the original arguments to the next hook.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: for HC_ACTION, lParam points to a KBDLLHOOKSTRUCT valid for this call.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let msg = u32::try_from(wparam.0).unwrap_or(0);
        if catch_unwind(AssertUnwindSafe(|| handle_key(msg, info))).unwrap_or(false) {
            return LRESULT(1);
        }
    }
    // SAFETY: forwarding the original arguments to the next hook.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Installed hooks, removed on drop.
struct Hooks {
    mouse: HHOOK,
    keyboard: HHOOK,
}

impl Hooks {
    fn install() -> windows::core::Result<Self> {
        // SAFETY: the procs are 'static functions in this module; hmod is this module.
        unsafe {
            let module = GetModuleHandleW(None)?;
            let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(module.into()), 0)?;
            match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(module.into()), 0) {
                Ok(keyboard) => Ok(Self { mouse, keyboard }),
                Err(e) => {
                    let _ = UnhookWindowsHookEx(mouse);
                    Err(e)
                }
            }
        }
    }
}

impl Drop for Hooks {
    fn drop(&mut self) {
        // SAFETY: both hooks were installed by this thread and are removed exactly once.
        unsafe {
            let _ = UnhookWindowsHookEx(self.mouse);
            let _ = UnhookWindowsHookEx(self.keyboard);
        }
    }
}

/// Thread entry point. Sends `Ok(thread id)` once the message queue exists, then runs
/// until `WM_QUIT`. Hooks are installed when `enabled` and toggled with `WM_APP_HOOKS`.
pub fn run(
    ctx: Arc<SharedState>,
    enabled: bool,
    ready: std::sync::mpsc::Sender<Result<u32, String>>,
) {
    crate::win::make_thread_dpi_aware();
    // SAFETY: adjusts the priority of the current thread only.
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
    }
    *unpoison(HOOK_CONTEXT.write()) = Some(ctx);
    let tid = crate::win::init_message_queue();
    let mut hooks = if enabled {
        match Hooks::install() {
            Ok(h) => Some(h),
            Err(e) => {
                // Pinning, transparency and rules still work without the hook; only the
                // mouse gestures are unavailable. Tell the user instead of refusing to run.
                tracing::error!(error = %e.message(), "could not install the global mouse hook");
                if let Some(ctx) = context() {
                    ctx.emit(crate::types::EngineEvent::Notice(crate::types::Notice::new(
                        crate::types::NoticeLevel::Error,
                        format!(
                            "Mouse gestures are unavailable: Windows refused the global mouse hook ({}).",
                            e.message()
                        ),
                        None,
                    )));
                }
                None
            }
        }
    } else {
        None
    };
    let _ = ready.send(Ok(tid));
    tracing::info!(hooks = hooks.is_some(), "input hook thread running");

    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid out buffer; the loop ends on WM_QUIT (0) or error (-1).
        let r = unsafe { GetMessageW(&raw mut msg, None, 0, 0) };
        if r.0 <= 0 {
            break;
        }
        if msg.hwnd.is_invalid() && msg.message == WM_APP_HOOKS {
            let want = msg.wParam.0 != 0;
            if want && hooks.is_none() {
                match Hooks::install() {
                    Ok(h) => {
                        hooks = Some(h);
                        tracing::info!("mouse/keyboard hooks installed");
                    }
                    Err(e) => {
                        tracing::error!(error = %e.message(), "failed to install input hooks")
                    }
                }
            } else if !want && hooks.is_some() {
                hooks = None;
                STATE.with(|s| {
                    if let Ok(mut s) = s.try_borrow_mut() {
                        // Without hooks the button-up will never be seen: end the gesture
                        // now so its overlays do not stay on screen.
                        if s.active.is_some()
                            && let Some(ctx) = context()
                        {
                            ctx.gestures.push(GestureMsg::End);
                        }
                        *s = HookState::default();
                    }
                });
                tracing::info!("mouse/keyboard hooks removed");
            }
            continue;
        }
        // SAFETY: standard message dispatch on this thread.
        unsafe {
            let _ = TranslateMessage(&raw const msg);
            DispatchMessageW(&raw const msg);
        }
    }
    drop(hooks);
    *unpoison(HOOK_CONTEXT.write()) = None;
    tracing::info!("input hook thread stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_vk_mapping() {
        assert_eq!(modifier_for_vk(u32::from(VK_LMENU.0)), Some(Modifier::Alt));
        assert_eq!(
            modifier_for_vk(u32::from(VK_RCONTROL.0)),
            Some(Modifier::Ctrl)
        );
        assert_eq!(
            modifier_for_vk(u32::from(VK_LSHIFT.0)),
            Some(Modifier::Shift)
        );
        assert_eq!(modifier_for_vk(u32::from(VK_RWIN.0)), Some(Modifier::Win));
        assert_eq!(modifier_for_vk(0x41), None);
    }

    use awwm_core::settings::Settings;

    const CTRL: u32 = VK_LCONTROL.0 as u32;
    const ESC: u32 = VK_ESCAPE.0 as u32;
    const KEY_A: u32 = 0x41;

    fn ctx() -> SharedState {
        // Defaults: Alt activates gestures, Ctrl snaps.
        SharedState::new(1, &Settings::default(), Arc::new(|_| {}))
    }

    fn live_gesture(st: &mut HookState, seq: u64) {
        st.active = Some(ActiveGesture {
            seq,
            button: MouseButton::Left,
        });
        st.swallowed_buttons.insert(button_id(MouseButton::Left));
    }

    // Arguments of `key`: vk, down, time (ms), gesture live.
    fn key(st: &mut HookState, c: &SharedState, vk: u32, down: bool, t: u32, live: bool) -> bool {
        process_key(st, c, vk, down, t, live)
    }

    #[test]
    fn snap_key_held_before_the_gesture_is_never_swallowed_even_when_it_repeats() {
        let (c, mut st) = (ctx(), HookState::default());
        assert!(
            !key(&mut st, &c, CTRL, true, 0, false),
            "press before gesture passes"
        );
        live_gesture(&mut st, 1);
        for t in [500, 530, 560] {
            assert!(!key(&mut st, &c, CTRL, true, t, true), "auto-repeat passes");
        }
        assert!(
            !key(&mut st, &c, CTRL, false, 600, true),
            "its key-up passes"
        );
        assert!(st.swallowed_keys.is_empty());
    }

    #[test]
    fn a_delivered_press_is_never_swallowed_even_with_very_slow_auto_repeat() {
        // Filter Keys can space repeats ~2 s apart; the system key state still shows the
        // press, so timing never matters for a delivered press.
        let (c, mut st) = (ctx(), HookState::default());
        assert!(!key(&mut st, &c, CTRL, true, 0, false));
        live_gesture(&mut st, 1);
        assert!(!key(&mut st, &c, CTRL, true, 2_000, true));
        assert!(!key(&mut st, &c, CTRL, true, 4_000, true));
        assert!(!key(&mut st, &c, CTRL, false, 4_100, true));
    }

    #[test]
    fn snap_key_pressed_during_the_gesture_is_swallowed_with_repeats_and_key_up() {
        let (c, mut st) = (ctx(), HookState::default());
        live_gesture(&mut st, 1);
        assert!(key(&mut st, &c, CTRL, true, 0, true));
        // A swallowed press never reaches the system key state.
        assert!(key(&mut st, &c, CTRL, true, 500, true), "repeat swallowed");
        // The gesture ended meanwhile: the key-up of the swallowed press is still eaten.
        st.active = None;
        assert!(key(&mut st, &c, CTRL, false, 900, false));
        assert!(st.swallowed_keys.is_empty());
        // Only the press during the live gesture toggled snapping.
        assert_eq!(c.gestures.drain(), vec![GestureMsg::Snap { active: true }]);
    }

    #[test]
    fn releasing_the_snap_key_mid_gesture_turns_snapping_off() {
        let (c, mut st) = (ctx(), HookState::default());
        let _ = key(&mut st, &c, CTRL, true, 0, false);
        live_gesture(&mut st, 1);
        assert!(!key(&mut st, &c, CTRL, false, 100, true));
        assert_eq!(c.gestures.drain(), vec![GestureMsg::Snap { active: false }]);
    }

    #[test]
    fn a_lost_key_up_never_eats_a_later_press() {
        let (c, mut st) = (ctx(), HookState::default());
        live_gesture(&mut st, 1);
        assert!(key(&mut st, &c, CTRL, true, 0, true));
        // The key-up went to the secure desktop; much later, Ctrl is pressed normally.
        st.active = None;
        assert!(
            !key(&mut st, &c, CTRL, true, 60_000, false),
            "fresh press passes"
        );
        assert!(
            !key(&mut st, &c, CTRL, false, 60_100, false),
            "and its key-up too"
        );
    }

    #[test]
    fn escape_cancels_and_is_swallowed_as_a_pair_but_a_held_escape_is_not() {
        let (c, mut st) = (ctx(), HookState::default());
        live_gesture(&mut st, 1);
        assert!(key(&mut st, &c, ESC, true, 0, true));
        assert!(key(&mut st, &c, ESC, false, 50, true));
        assert_eq!(c.gestures.drain(), vec![GestureMsg::Cancel]);

        let mut st = HookState::default();
        assert!(!key(&mut st, &c, ESC, true, 0, false));
        live_gesture(&mut st, 2);
        assert!(
            !key(&mut st, &c, ESC, true, 500, true),
            "repeat of a held Esc passes"
        );
        assert!(!key(&mut st, &c, ESC, false, 600, true));
        assert!(c.gestures.drain().is_empty(), "a held Esc does not cancel");
    }

    #[test]
    fn an_esc_release_whose_press_was_hidden_from_the_hook_still_cancels() {
        let (c, mut st) = (ctx(), HookState::default());
        live_gesture(&mut st, 1);
        // The key-down never reached this hook (another hook ate it); only the key-up came.
        assert!(
            !key(&mut st, &c, ESC, false, 100, true),
            "the key-up passes through"
        );
        assert_eq!(c.gestures.drain(), vec![GestureMsg::Cancel]);
        // Outside a gesture, or for a press we saw, nothing extra happens.
        let mut st = HookState::default();
        assert!(!key(&mut st, &c, ESC, false, 100, false));
        assert!(c.gestures.drain().is_empty());
    }

    #[test]
    fn ordinary_keys_are_never_swallowed() {
        let (c, mut st) = (ctx(), HookState::default());
        live_gesture(&mut st, 1);
        assert!(!key(&mut st, &c, KEY_A, true, 0, true));
        assert!(!key(&mut st, &c, KEY_A, false, 10, true));
    }
    #[test]
    fn button_presses_use_the_system_key_state_plus_swallowed_snap_keys() {
        let alt_only = ModifierState {
            alt: true,
            ..ModifierState::default()
        };
        // The hook's own tracking is irrelevant: the system says Alt is down.
        assert_eq!(effective_modifiers(alt_only, &HashSet::new()), alt_only);
        // Nothing held according to the system means no modifiers.
        assert_eq!(
            effective_modifiers(ModifierState::default(), &HashSet::new()),
            ModifierState::default()
        );
        // A Ctrl press AWWM swallowed (invisible to the system) still counts as held.
        let swallowed: HashSet<u32> = [CTRL, KEY_A].into_iter().collect();
        let m = effective_modifiers(alt_only, &swallowed);
        assert!(m.alt && m.ctrl && !m.shift && !m.win);
    }

    #[test]
    fn gesture_button_is_swallowed_as_a_pair_and_ends_the_gesture() {
        let (c, mut st) = (ctx(), HookState::default());
        live_gesture(&mut st, 1);
        assert_eq!(
            button_step(&mut st, &c, MouseButton::Left, false),
            ButtonStep::Swallow
        );
        assert!(st.active.is_none());
        assert_eq!(c.gestures.drain(), vec![GestureMsg::End]);
        assert_eq!(
            button_step(&mut st, &c, MouseButton::Left, false),
            ButtonStep::Pass
        );
    }

    #[test]
    fn other_buttons_during_a_gesture_are_swallowed_as_pairs() {
        let (c, mut st) = (ctx(), HookState::default());
        live_gesture(&mut st, 1);
        assert_eq!(
            button_step(&mut st, &c, MouseButton::Right, true),
            ButtonStep::Swallow
        );
        assert_eq!(
            button_step(&mut st, &c, MouseButton::Right, false),
            ButtonStep::Swallow
        );
        assert_eq!(
            button_step(&mut st, &c, MouseButton::Right, false),
            ButtonStep::Pass
        );
    }

    #[test]
    fn a_lost_button_up_never_makes_the_next_click_stick() {
        let (c, mut st) = (ctx(), HookState::default());
        live_gesture(&mut st, 1);
        // The gesture's button-up was lost. The next press of that button ends the stale
        // gesture and is classified normally; its own button-up must pass through.
        assert_eq!(
            button_step(&mut st, &c, MouseButton::Left, true),
            ButtonStep::Classify
        );
        assert_eq!(c.gestures.drain(), vec![GestureMsg::End]);
        assert_eq!(
            button_step(&mut st, &c, MouseButton::Left, false),
            ButtonStep::Pass
        );
    }

    #[test]
    fn an_aborted_gesture_releases_other_buttons_but_still_eats_its_own_up() {
        let (c, mut st) = (ctx(), HookState::default());
        live_gesture(&mut st, 7);
        c.gesture_aborted.store(7, Ordering::Release);
        assert_eq!(
            button_step(&mut st, &c, MouseButton::Right, true),
            ButtonStep::Classify
        );
        assert_eq!(
            button_step(&mut st, &c, MouseButton::Right, false),
            ButtonStep::Pass
        );
        // The aborted gesture's own button-down was swallowed, so its up is swallowed too.
        assert_eq!(
            button_step(&mut st, &c, MouseButton::Left, false),
            ButtonStep::Swallow
        );
        assert_eq!(
            button_step(&mut st, &c, MouseButton::Left, false),
            ButtonStep::Pass
        );
    }
}
