//! window_movement + window_resizing: the gesture thread.
//!
//! Receives coalesced input from the hook, computes the new geometry with the pure
//! functions of `awwm_core` (move/resize/snap) and applies it with synchronous
//! `SetWindowPos`. Synchronous calls rate-limit naturally to what the target application
//! can process, and let us read back the size it actually accepted (min/max track size)
//! so the anchored edge never drifts. This thread also owns the resize outline, the size
//! label and the snap guides.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::HiDpi::GetSystemMetricsForDpi;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, GetWindowPlacement, MSG, PostThreadMessageW, SM_CXMINTRACK,
    SM_CYMINTRACK, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOSIZE,
    SetWindowPlacement, ShowWindow, TranslateMessage, WINDOWPLACEMENT,
};

use awwm_core::geometry::{logical_to_physical, physical_to_logical, round_to_i32};
use awwm_core::gesture::GestureKind;
use awwm_core::resize::{ResizeEdges, SizeConstraints, anchor_size, resize_frame};
use awwm_core::snap::{GridSpec, SnapConfig, SnapEnvironment, SnapResult, snap_move, snap_resize};
use awwm_core::{Point, Rect, Size};

use crate::enumerator;
use crate::overlay::{FrameOverlay, GuideOverlays, LabelOverlay, size_label};
use crate::shared::{
    GestureMsg, GestureRuntime, INJECTED_MARKER, SharedState, WM_APP_FRAME_SYNC, WM_APP_GESTURE,
    unpoison,
};
use crate::types::{EngineEvent, Notice, NoticeLevel};
use crate::win::window::{self, FrameMargins, ZOrder};
use crate::win::{WindowHandle, monitor, process};

/// Unassigned virtual key injected to stop a lone Alt/Win release from activating the
/// menu bar or Start menu.
const NEUTRAL_KEY: u16 = 0xE8;
/// Minimum interval between "window is elevated" notices for the same window.
const BLOCKED_NOTICE_INTERVAL: Duration = Duration::from_secs(30);

struct Active {
    seq: u64,
    target: WindowHandle,
    pid: u32,
    kind: GestureKind,
    cfg: GestureRuntime,
    start_cursor: Point,
    start_frame: Rect,
    /// Window rect at gesture start (used to cancel).
    original_window: Rect,
    /// Placement before a maximized window was restored for moving (used to cancel).
    original_placement: Option<WINDOWPLACEMENT>,
    margins: FrameMargins,
    /// Cursor offset from the frame's top-left corner (move).
    offset: Point,
    /// Current frame size (move; changes when crossing monitors with different DPI).
    size: Size,
    edges: ResizeEdges,
    constraints: SizeConstraints,
    env: Option<SnapEnvironment>,
    snap: bool,
    last_cursor: Point,
    last_frame: Rect,
}

struct GestureThread {
    ctx: Arc<SharedState>,
    active: Option<Active>,
    outline: Option<FrameOverlay>,
    label: Option<LabelOverlay>,
    guides: GuideOverlays,
    blocked_notices: HashMap<WindowHandle, Instant>,
}

fn inject_neutral_key() {
    let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(NEUTRAL_KEY),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: INJECTED_MARKER,
            },
        },
    };
    let inputs = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
    // SAFETY: `inputs` is a valid slice of initialised INPUT structures.
    unsafe {
        SendInput(
            &inputs,
            i32::try_from(std::mem::size_of::<INPUT>()).unwrap_or(40),
        );
    }
}

fn min_track_size(dpi: u32, margins: FrameMargins) -> Size {
    // SAFETY: plain metric queries.
    let (w, h) = unsafe {
        (
            GetSystemMetricsForDpi(SM_CXMINTRACK, dpi),
            GetSystemMetricsForDpi(SM_CYMINTRACK, dpi),
        )
    };
    let total = margins.total();
    Size::new((w - total.width).max(1), (h - total.height).max(1))
}

fn snap_config(cfg: &GestureRuntime, dpi: u32) -> SnapConfig {
    let s = &cfg.snap;
    SnapConfig {
        threshold: logical_to_physical(i32::try_from(s.distance).unwrap_or(16), dpi),
        monitor_edges: s.monitor_edges,
        work_area_edges: s.work_area_edges,
        window_edges: s.window_edges,
        center_lines: s.center_lines,
        grid: s.grid_enabled.then_some(GridSpec {
            columns: s.grid_columns,
            rows: s.grid_rows,
        }),
    }
}

fn scale(value: i32, from: i32, to: i32) -> i32 {
    if from <= 0 {
        value
    } else {
        round_to_i32(f64::from(value) * f64::from(to) / f64::from(from))
    }
}

impl GestureThread {
    fn handle(&mut self, msg: GestureMsg) {
        match msg {
            GestureMsg::Start {
                seq,
                target,
                kind,
                cursor,
                snap,
                suppress_menu,
            } => {
                if self.active.is_some() {
                    self.finish();
                }
                if suppress_menu {
                    inject_neutral_key();
                }
                match self.begin(seq, target, kind, cursor, snap) {
                    Some(active) => {
                        self.active = Some(active);
                        self.update(cursor, true);
                    }
                    None => self.ctx.gesture_aborted.store(seq, Ordering::Release),
                }
            }
            GestureMsg::Move { cursor } => self.update(cursor, false),
            GestureMsg::Snap { active } => {
                if let Some(a) = &mut self.active
                    && a.snap != active
                {
                    a.snap = active;
                    let cursor = a.last_cursor;
                    self.update(cursor, true);
                }
            }
            GestureMsg::End => self.finish(),
            GestureMsg::Cancel => self.cancel(),
            GestureMsg::Blocked { target } => self.notify_blocked(target),
        }
    }

    fn notify_blocked(&mut self, target: WindowHandle) {
        let now = Instant::now();
        self.blocked_notices
            .retain(|_, t| now.duration_since(*t) < BLOCKED_NOTICE_INTERVAL);
        if self.blocked_notices.contains_key(&target) {
            return;
        }
        self.blocked_notices.insert(target, now);
        let exe = process::exe_path(window::process_id(target))
            .map(|p| process::exe_name(&p))
            .unwrap_or_default();
        let app = if exe.is_empty() {
            window::title(target)
        } else {
            exe
        };
        tracing::info!(hwnd = %target, app = %app, "gesture refused: elevated window");
        self.ctx.emit(EngineEvent::Notice(Notice::new(
            NoticeLevel::Warning,
            crate::error::EngineError::elevated(&app).to_string(),
            Some(target.as_u64()),
        )));
    }

    fn begin(
        &mut self,
        seq: u64,
        target: WindowHandle,
        kind: GestureKind,
        cursor: Point,
        snap: bool,
    ) -> Option<Active> {
        if !window::exists(target) {
            return None;
        }
        let pid = window::process_id(target);
        let cfg = unpoison(self.ctx.gesture.read()).clone();
        let original_window = window::window_rect(target)?;
        let mut original_placement = None;
        if kind == GestureKind::Move && window::is_maximized(target) {
            if !cfg.restore_maximized_on_move {
                return None;
            }
            original_placement = Some(restore_for_move(target, cursor)?);
        }
        let (_, frame, margins) = window::geometry(target)?;
        let dpi = monitor::dpi_for_rect(frame);
        let edges = ResizeEdges::from_cursor(frame, cursor);
        tracing::debug!(hwnd = %target, ?kind, ?frame, ?edges, "gesture started");
        Some(Active {
            seq,
            target,
            pid,
            kind,
            constraints: SizeConstraints {
                min: min_track_size(dpi, margins),
                max: None,
            },
            cfg,
            start_cursor: cursor,
            start_frame: frame,
            original_window,
            original_placement,
            margins,
            offset: cursor.delta_from(frame.origin()),
            size: frame.size(),
            edges,
            env: None,
            snap,
            last_cursor: cursor,
            last_frame: frame,
        })
    }

    fn update(&mut self, cursor: Point, force: bool) {
        let Some(mut a) = self.active.take() else {
            return;
        };
        if !force && cursor == a.last_cursor {
            self.active = Some(a);
            return;
        }
        if !window::is_same_window(a.target, a.pid) {
            self.abort(&a);
            return;
        }
        a.last_cursor = cursor;
        let dpi = monitor::from_point(cursor).map_or(96, |m| m.dpi);
        let snapping = a.snap && a.cfg.snap.enabled;
        if snapping && a.env.is_none() {
            a.env = Some(SnapEnvironment {
                monitors: monitor::all(),
                windows: enumerator::snap_targets(a.target, self.ctx.own_pid),
            });
        }
        let result = match a.kind {
            GestureKind::Move => self.apply_move(&mut a, cursor, snapping, dpi),
            GestureKind::Resize => self.apply_resize(&mut a, cursor, snapping, dpi),
        };
        if let Some(result) = result {
            self.show_feedback(&a, &result, snapping);
            // Coalesced: at most one frame-sync message is queued at a time.
            if unpoison(self.ctx.pinned.read()).contains(&a.target)
                && !self.ctx.frame_sync_pending.swap(true, Ordering::AcqRel)
            {
                let engine = self.ctx.engine_thread.load(Ordering::Acquire);
                // SAFETY: pointer-free message carrying the HWND value as an integer.
                let posted = unsafe {
                    PostThreadMessageW(engine, WM_APP_FRAME_SYNC, WPARAM(0), LPARAM(a.target.raw()))
                };
                if posted.is_err() {
                    self.ctx.frame_sync_pending.store(false, Ordering::Release);
                }
            }
        }
        self.active = Some(a);
    }

    fn apply_move(
        &self,
        a: &mut Active,
        cursor: Point,
        snapping: bool,
        dpi: u32,
    ) -> Option<SnapResult> {
        let desired = Rect::from_xywh(
            cursor.x - a.offset.x,
            cursor.y - a.offset.y,
            a.size.width,
            a.size.height,
        );
        let result = match (&a.env, snapping) {
            (Some(env), true) => snap_move(desired, env, &snap_config(&a.cfg, dpi)),
            _ => SnapResult {
                rect: desired,
                guides: Vec::new(),
            },
        };
        let target_rect = a.margins.frame_to_window(result.rect);
        window::set_window_pos(
            a.target,
            ZOrder::Keep,
            target_rect,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        )
        .ok()?;
        let (_, actual, margins) = window::geometry(a.target)?;
        if actual.size() != a.size {
            // A per-monitor-DPI-aware app resized itself after crossing to a monitor with a
            // different scale: keep the pointer at the same relative spot of the window.
            a.offset = Point::new(
                scale(a.offset.x, a.size.width, actual.width()),
                scale(a.offset.y, a.size.height, actual.height()),
            );
            a.size = actual.size();
        }
        a.margins = margins;
        a.last_frame = actual;
        Some(SnapResult {
            rect: actual,
            guides: result.guides,
        })
    }

    fn apply_resize(
        &self,
        a: &mut Active,
        cursor: Point,
        snapping: bool,
        dpi: u32,
    ) -> Option<SnapResult> {
        let delta = cursor.delta_from(a.start_cursor);
        let desired = resize_frame(a.start_frame, a.edges, delta, &a.constraints);
        let result = match (&a.env, snapping) {
            (Some(env), true) => snap_resize(
                desired,
                a.edges,
                env,
                &snap_config(&a.cfg, dpi),
                &a.constraints,
            ),
            _ => SnapResult {
                rect: desired,
                guides: Vec::new(),
            },
        };
        let requested = result.rect;
        if requested == a.last_frame {
            return Some(SnapResult {
                rect: requested,
                guides: result.guides,
            });
        }
        window::set_window_pos(
            a.target,
            ZOrder::Keep,
            a.margins.frame_to_window(requested),
            SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        )
        .ok()?;
        let (_, mut actual, margins) = window::geometry(a.target)?;
        a.margins = margins;
        if actual.size() != requested.size() {
            // The application enforced its own size (minimum/maximum track size, character
            // grid, aspect ratio). Keep the fixed edge where it was. The accepted size is
            // deliberately not remembered as a hard limit: apps that round their size
            // would otherwise lock it for the rest of the gesture.
            let anchored = anchor_size(requested, a.edges, actual.size());
            if anchored.origin() != actual.origin() {
                let _ = window::set_window_pos(
                    a.target,
                    ZOrder::Keep,
                    a.margins.frame_to_window(anchored),
                    SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                );
                actual = window::frame_rect(a.target).unwrap_or(anchored);
            }
        }
        a.last_frame = actual;
        Some(SnapResult {
            rect: actual,
            guides: result.guides,
        })
    }

    fn show_feedback(&mut self, a: &Active, result: &SnapResult, snapping: bool) {
        let frame = result.rect;
        let mon = monitor::from_rect(frame);
        let dpi = mon.map_or(96, |m| m.dpi);
        let color = a.cfg.overlay_color;
        let show_outline = match a.kind {
            GestureKind::Resize => a.cfg.show_resize_overlay,
            GestureKind::Move => snapping && a.cfg.snap.show_guides,
        };
        if show_outline {
            if self.outline.is_none() {
                self.outline = FrameOverlay::create(color);
            }
            if let Some(outline) = &mut self.outline {
                outline.set_color(color);
                outline.show(frame, logical_to_physical(2, dpi), ZOrder::Topmost);
            }
        } else if let Some(outline) = &mut self.outline {
            outline.hide();
        }
        if a.kind == GestureKind::Resize && a.cfg.show_resize_overlay {
            if self.label.is_none() {
                self.label = LabelOverlay::create();
            }
            if let Some(label) = &mut self.label {
                let text = size_label(
                    physical_to_logical(frame.width(), dpi),
                    physical_to_logical(frame.height(), dpi),
                );
                let bounds = mon.map_or(Rect::default(), |m| m.work_area);
                label.show(&text, frame.center(), bounds, dpi);
            }
        }
        if snapping && a.cfg.snap.show_guides && !result.guides.is_empty() {
            self.guides
                .show(&result.guides, logical_to_physical(2, dpi), color);
        } else {
            self.guides.hide();
        }
    }

    fn hide_feedback(&mut self) {
        if let Some(o) = &mut self.outline {
            o.hide();
        }
        if let Some(l) = &mut self.label {
            l.hide();
        }
        self.guides.hide();
    }

    fn finish(&mut self) {
        if let Some(a) = self.active.take() {
            tracing::debug!(hwnd = %a.target, frame = ?a.last_frame, "gesture finished");
        }
        self.hide_feedback();
    }

    fn abort(&mut self, a: &Active) {
        tracing::debug!(hwnd = %a.target, "gesture aborted: window gone");
        self.ctx.gesture_aborted.store(a.seq, Ordering::Release);
        self.hide_feedback();
    }

    fn cancel(&mut self) {
        let Some(a) = self.active.take() else { return };
        // The hook keeps swallowing the button-up of this gesture but must stop treating
        // it as live (no more snap-key swallowing or move forwarding).
        self.ctx.gesture_aborted.store(a.seq, Ordering::Release);
        self.hide_feedback();
        if !window::is_same_window(a.target, a.pid) {
            return;
        }
        match &a.original_placement {
            Some(placement) => {
                // SAFETY: `placement` is a fully initialised WINDOWPLACEMENT read earlier.
                unsafe {
                    let _ = SetWindowPlacement(a.target.hwnd(), placement);
                }
            }
            None => {
                let _ = window::set_window_pos(
                    a.target,
                    ZOrder::Keep,
                    a.original_window,
                    SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                );
            }
        }
        tracing::debug!(hwnd = %a.target, "gesture cancelled");
    }

    /// Aborts an active gesture from outside (desktop switch, shutdown).
    fn abort_active(&mut self) {
        if let Some(a) = self.active.take() {
            self.ctx.gesture_aborted.store(a.seq, Ordering::Release);
        }
        self.hide_feedback();
    }
}

/// Restores a maximized window so it can be dragged, placing it under the cursor at the
/// same relative horizontal position. Returns the previous placement.
fn restore_for_move(target: WindowHandle, cursor: Point) -> Option<WINDOWPLACEMENT> {
    let mut placement = WINDOWPLACEMENT {
        length: u32::try_from(std::mem::size_of::<WINDOWPLACEMENT>()).unwrap_or(44),
        ..WINDOWPLACEMENT::default()
    };
    // SAFETY: `placement` is a correctly sized out struct.
    unsafe { GetWindowPlacement(target.hwnd(), &raw mut placement) }.ok()?;
    let before = window::frame_rect(target)?;
    // SAFETY: restores without activating; the call is synchronous with the target thread,
    // which the hook verified is responsive.
    unsafe {
        let _ = ShowWindow(target.hwnd(), SW_SHOWNOACTIVATE);
    }
    let (_, frame, margins) = window::geometry(target)?;
    let fraction = if before.width() > 0 {
        f64::from(cursor.x - before.left) / f64::from(before.width())
    } else {
        0.5
    };
    let left = cursor.x - round_to_i32(fraction * f64::from(frame.width()));
    let top = cursor.y - (cursor.y - before.top).clamp(0, frame.height() / 2);
    let placed = frame.with_origin(Point::new(left, top));
    let _ = window::set_window_pos(
        target,
        ZOrder::Keep,
        margins.frame_to_window(placed),
        SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
    );
    Some(placement)
}

/// Message telling the gesture thread to abort the active gesture (desktop switch).
pub const WM_APP_GESTURE_ABORT: u32 = WM_APP_GESTURE + 0x10;

/// Thread entry point.
pub fn run(ctx: Arc<SharedState>, ready: std::sync::mpsc::Sender<Result<u32, String>>) {
    crate::win::make_thread_dpi_aware();
    let tid = crate::win::init_message_queue();
    ctx.gestures.set_thread(tid);
    let mut thread = GestureThread {
        ctx,
        active: None,
        outline: None,
        label: None,
        guides: GuideOverlays::default(),
        blocked_notices: HashMap::new(),
    };
    let _ = ready.send(Ok(tid));
    tracing::info!("gesture thread running");
    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid out buffer; the loop ends on WM_QUIT (0) or error (-1).
        let r = unsafe { GetMessageW(&raw mut msg, None, 0, 0) };
        if r.0 <= 0 {
            break;
        }
        if msg.hwnd.is_invalid() && msg.message == WM_APP_GESTURE {
            for m in thread.ctx.gestures.drain() {
                thread.handle(m);
            }
            continue;
        }
        if msg.hwnd.is_invalid() && msg.message == WM_APP_GESTURE_ABORT {
            thread.abort_active();
            continue;
        }
        // SAFETY: standard dispatch (overlay window painting).
        unsafe {
            let _ = TranslateMessage(&raw const msg);
            DispatchMessageW(&raw const msg);
        }
    }
    thread.abort_active();
    tracing::info!("gesture thread stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_offsets_proportionally() {
        assert_eq!(scale(100, 1000, 1500), 150);
        assert_eq!(scale(100, 0, 1500), 100);
        assert_eq!(scale(-40, 800, 400), -20);
    }
}
