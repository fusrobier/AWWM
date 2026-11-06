//! Pixel orientation, the visible pixel grid and the graceful exit request, driven with
//! `Engine::inject_gesture` against real windows of `awwm-testapp`.
//!
//! Overlay and message-window lookups are restricted to this test process, so an AWWM
//! instance running on the same desktop never influences (or receives) anything.
#![cfg(windows)]

mod common;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GW_HWNDNEXT, GetWindow, GetWindowThreadProcessId, HWND_MESSAGE, IsWindowVisible,
    PostMessageW, RegisterWindowMessageW,
};
use windows::core::{PCWSTR, w};

use awwm_core::gesture::GestureKind;
use awwm_core::pixel_grid;
use awwm_core::settings::Settings;
use awwm_core::{Point, Rect};
use awwm_engine::{ENGINE_WINDOW_CLASS, EXIT_MESSAGE_NAME, EngineEvent, GestureMsg};
use common::{
    TIMEOUT, TestApp, TestEngine, frame_rect, monitors, poll, settings, setup, spot, wait_frame,
    wait_until, window_rect,
};

fn own_pid_of(h: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: read-only query with a valid out pointer.
    unsafe { GetWindowThreadProcessId(h, Some(&raw mut pid)) };
    pid
}

/// Handles of this process's visible overlay windows.
fn own_overlay_handles() -> Vec<HWND> {
    let me = std::process::id();
    let mut found = Vec::new();
    let mut after = None;
    // SAFETY: enumerates top-level windows by class; `after` is a previous result.
    while let Ok(h) = unsafe { FindWindowExW(None, after, w!("AWWM.Overlay"), PCWSTR::null()) } {
        if h.is_invalid() {
            break;
        }
        // SAFETY: read-only query.
        if own_pid_of(h) == me && unsafe { IsWindowVisible(h) }.as_bool() {
            found.push(h);
        }
        after = Some(h);
    }
    found
}

fn own_overlays() -> Vec<Rect> {
    own_overlay_handles()
        .into_iter()
        .map(|h| window_rect(h.0 as usize as u64))
        .collect()
}

/// Visible overlays of this process that cover a whole monitor (the pixel grid).
fn grid_windows() -> Vec<Rect> {
    let bounds: Vec<Rect> = monitors().iter().map(|m| m.bounds).collect();
    own_overlays()
        .into_iter()
        .filter(|r| bounds.contains(r))
        .collect()
}

/// A small overlay of this process containing `p` (the size/position label).
fn label_at(p: Point) -> Option<Rect> {
    own_overlays()
        .into_iter()
        .find(|r| r.contains(p) && r.width() < 800 && r.height() < 120)
}

fn grid_settings(size: u32) -> Settings {
    let mut s = settings();
    s.alignment.show_grid = true;
    s.alignment.grid_size = size;
    s
}

fn offset(p: Point, dx: i32, dy: i32) -> Point {
    Point::new(p.x + dx, p.y + dy)
}

#[test]
fn the_pixel_grid_covers_every_monitor_while_moving_and_is_removed_on_release() {
    let _guard = setup();
    let engine = TestEngine::start(grid_settings(20));
    let app = TestApp::spawn_at("AWWM.Test.GridMove", spot(0), &["--size", "420,300"]);
    let h = app.hwnd(0);
    let f0 = frame_rect(h);
    assert!(grid_windows().is_empty(), "no grid before the gesture");

    engine.start_gesture(1, h, GestureKind::Move, f0.center());
    engine.gesture(GestureMsg::Move {
        cursor: offset(f0.center(), 40, 30),
    });
    let expected = monitors().len();
    assert!(
        wait_until(TIMEOUT, || grid_windows().len() == expected),
        "one grid window per monitor ({expected}): {:?}",
        own_overlays()
    );

    // Directly below the moved window in Z order, so the grid never covers it.
    // SAFETY: read-only Z-order query.
    let below = unsafe { GetWindow(HWND(h as usize as *mut _), GW_HWNDNEXT) }.expect("below");
    assert!(
        own_overlay_handles().contains(&below)
            && grid_windows().contains(&window_rect(below.0 as usize as u64)),
        "the window right below the moved window is a grid window"
    );

    engine.gesture(GestureMsg::End);
    assert!(
        wait_until(TIMEOUT, || grid_windows().is_empty()),
        "grid removed on release: {:?}",
        grid_windows()
    );
    let f1 = f0.translate(40, 30);
    assert_eq!(wait_frame(h, f1), f1, "the grid does not change movement");
}

#[test]
fn the_pixel_grid_is_removed_when_the_dragged_window_closes() {
    let _guard = setup();
    let engine = TestEngine::start(grid_settings(10));
    let mut app = TestApp::spawn_at("AWWM.Test.GridAbort", spot(0), &["--size", "420,300"]);
    let h = app.hwnd(0);
    let f0 = frame_rect(h);
    engine.start_gesture(1, h, GestureKind::Move, f0.center());
    engine.gesture(GestureMsg::Move {
        cursor: offset(f0.center(), 20, 10),
    });
    assert!(
        wait_until(TIMEOUT, || grid_windows().len() == monitors().len()),
        "grid shown: {:?}",
        own_overlays()
    );
    app.close(0);
    // The next mouse move finds the window gone and aborts the gesture.
    engine.gesture(GestureMsg::Move {
        cursor: offset(f0.center(), 40, 20),
    });
    assert!(
        wait_until(TIMEOUT, || grid_windows().is_empty()),
        "grid removed after the abort: {:?}",
        grid_windows()
    );
    engine.gesture(GestureMsg::End);
}

#[test]
fn a_very_short_drag_leaves_no_grid_behind() {
    let _guard = setup();
    let engine = TestEngine::start(grid_settings(4));
    let app = TestApp::spawn_at("AWWM.Test.GridShort", spot(0), &["--size", "420,300"]);
    let h = app.hwnd(0);
    let f0 = frame_rect(h);
    for seq in 1..=5 {
        // Released immediately: the grid may still be under construction when it is
        // cancelled.
        engine.start_gesture(seq, h, GestureKind::Move, f0.center());
        engine.gesture(GestureMsg::End);
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        wait_until(TIMEOUT, || grid_windows().is_empty()),
        "no grid after the drags: {:?}",
        grid_windows()
    );
}

#[test]
fn the_pixel_grid_is_shown_while_resizing_and_removed_on_cancel() {
    let _guard = setup();
    let engine = TestEngine::start(grid_settings(8));
    let app = TestApp::spawn_at("AWWM.Test.GridResize", spot(0), &["--size", "480,360"]);
    let h = app.hwnd(0);
    let f0 = frame_rect(h);
    let start = Point::new(f0.left + f0.width() * 5 / 6, f0.top + f0.height() * 5 / 6);

    engine.start_gesture(1, h, GestureKind::Resize, start);
    engine.gesture(GestureMsg::Move {
        cursor: offset(start, 30, 20),
    });
    assert!(
        wait_until(TIMEOUT, || grid_windows().len() == monitors().len()),
        "grid during resize: {:?}",
        own_overlays()
    );
    engine.gesture(GestureMsg::Cancel);
    assert!(
        wait_until(TIMEOUT, || grid_windows().is_empty()),
        "grid removed on cancel"
    );
    assert_eq!(wait_frame(h, f0), f0, "cancel restored the size");
}

#[test]
fn no_grid_when_it_is_turned_off() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at("AWWM.Test.NoGrid", spot(0), &["--size", "420,300"]);
    let h = app.hwnd(0);
    let f0 = frame_rect(h);
    engine.start_gesture(1, h, GestureKind::Move, f0.center());
    engine.gesture(GestureMsg::Move {
        cursor: offset(f0.center(), 25, 15),
    });
    let f1 = f0.translate(25, 15);
    assert_eq!(wait_frame(h, f1), f1);
    // Give the gesture thread time to (wrongly) show something.
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(grid_windows().is_empty(), "{:?}", own_overlays());
    engine.gesture(GestureMsg::End);
}

#[test]
fn pixel_orientation_shows_a_position_label_while_moving() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at("AWWM.Test.Orientation", spot(0), &["--size", "520,360"]);
    let h = app.hwnd(0);
    let f0 = frame_rect(h);

    engine.start_gesture(1, h, GestureKind::Move, f0.center());
    engine.gesture(GestureMsg::Move {
        cursor: offset(f0.center(), 50, 20),
    });
    let f1 = f0.translate(50, 20);
    assert_eq!(wait_frame(h, f1), f1);
    assert!(
        poll(TIMEOUT, || label_at(f1.center())).is_some(),
        "position label at the window centre: {:?}",
        own_overlays()
    );
    engine.gesture(GestureMsg::End);
    assert!(
        wait_until(TIMEOUT, || label_at(f1.center()).is_none()),
        "label removed on release"
    );

    // Turned off: moving shows no label.
    let mut off = settings();
    off.alignment.pixel_orientation = false;
    engine.apply_settings(off);
    engine.start_gesture(2, h, GestureKind::Move, f1.center());
    engine.gesture(GestureMsg::Move {
        cursor: offset(f1.center(), -30, 10),
    });
    let f2 = f1.translate(-30, 10);
    assert_eq!(wait_frame(h, f2), f2);
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(label_at(f2.center()).is_none(), "{:?}", own_overlays());
    engine.gesture(GestureMsg::End);
}

#[test]
fn snapping_aligns_edges_to_the_visible_pixel_grid() {
    let _guard = setup();
    let mut s = grid_settings(20);
    // Only the pixel grid: no other candidate may compete.
    s.snapping.monitor_edges = false;
    s.snapping.work_area_edges = false;
    s.snapping.window_edges = false;
    s.snapping.center_lines = false;
    s.snapping.grid_enabled = false;
    s.snapping.distance = 16;
    let engine = TestEngine::start(s.clone());
    let app = TestApp::spawn_at("AWWM.Test.GridSnap", spot(0), &["--size", "500,350"]);
    let h = app.hwnd(0);
    let f0 = frame_rect(h);
    let m = awwm_engine::win::monitor::from_rect(f0).expect("monitor");
    let on_x = |x: i32| {
        pixel_grid::bracketing_lines(m.bounds.left, m.bounds.width(), x, 20, m.dpi).contains(&x)
    };
    let on_y = |y: i32| {
        pixel_grid::bracketing_lines(m.bounds.top, m.bounds.height(), y, 20, m.dpi).contains(&y)
    };
    // 3 physical px right of and 2 below grid lines, well inside the snap distance.
    let near = f0.with_origin(Point::new(
        pixel_grid::line_position(m.bounds.left, 12, 20, m.dpi) + 3,
        pixel_grid::line_position(m.bounds.top, 9, 20, m.dpi) + 2,
    ));
    let start = f0.center();
    let cursor = offset(start, near.left - f0.left, near.top - f0.top);

    engine.start_gesture(1, h, GestureKind::Move, start);
    engine.gesture(GestureMsg::Move { cursor });
    assert_eq!(
        wait_frame(h, near),
        near,
        "no snapping without the snap key"
    );
    engine.gesture(GestureMsg::Snap { active: true });
    let snapped = poll(TIMEOUT, || {
        let f = frame_rect(h);
        ((on_x(f.left) || on_x(f.right)) && (on_y(f.top) || on_y(f.bottom))).then_some(f)
    })
    .unwrap_or_else(|| panic!("frame on grid lines: {:?}", frame_rect(h)));
    assert_eq!(snapped.size(), near.size());
    assert!((snapped.left - near.left).abs() <= 16 && (snapped.top - near.top).abs() <= 16);
    engine.gesture(GestureMsg::End);

    // Snap to grid off: holding the snap key leaves the frame where the cursor put it.
    s.alignment.snap_to_grid = false;
    engine.apply_settings(s);
    let f1 = frame_rect(h);
    let start = f1.center();
    engine.start_gesture(2, h, GestureKind::Move, start);
    engine.gesture(GestureMsg::Snap { active: true });
    engine.gesture(GestureMsg::Move {
        cursor: offset(start, near.left - f1.left, near.top - f1.top),
    });
    assert_eq!(wait_frame(h, near), near, "grid is not a snap target");
    engine.gesture(GestureMsg::End);
}

/// The engine message window of this process (never another AWWM's).
fn own_engine_window() -> Option<HWND> {
    let me = std::process::id();
    let mut after = None;
    loop {
        // SAFETY: enumerates message-only windows by class; `after` is a previous result.
        let h = unsafe {
            FindWindowExW(
                Some(HWND_MESSAGE),
                after,
                ENGINE_WINDOW_CLASS,
                PCWSTR::null(),
            )
        }
        .ok()?;
        if h.is_invalid() {
            return None;
        }
        if own_pid_of(h) == me {
            return Some(h);
        }
        after = Some(h);
    }
}

#[test]
fn the_exit_request_message_is_reported_as_an_event() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let window = poll(TIMEOUT, own_engine_window).expect("engine message window");
    let mark = engine.mark();
    // SAFETY: registers a message name and posts a pointer-free message to our own window.
    unsafe {
        let msg = RegisterWindowMessageW(EXIT_MESSAGE_NAME);
        assert_ne!(msg, 0);
        PostMessageW(Some(window), msg, WPARAM(0), LPARAM(0)).expect("post");
    }
    assert!(
        engine
            .wait_event_since(mark, TIMEOUT, |e| *e == EngineEvent::ExitRequested)
            .is_some(),
        "ExitRequested event"
    );
}
