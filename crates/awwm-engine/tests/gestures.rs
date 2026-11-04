//! Move / resize / snap gestures driven with `Engine::inject_gesture` (no real input; the
//! global mouse hook stays off).
#![cfg(windows)]

mod common;

use awwm_core::gesture::GestureKind;
use awwm_core::settings::Settings;
use awwm_core::{Point, Rect};
use awwm_engine::GestureMsg;
use common::{
    TIMEOUT, TestApp, TestEngine, frame_rect, monitors, poll, primary, settings, setup, spot,
    visible_overlays, wait_frame, wait_until, window_rect,
};

fn offset(p: Point, dx: i32, dy: i32) -> Point {
    Point::new(p.x + dx, p.y + dy)
}

/// Horizontal and vertical size of the invisible borders (window rect − visible frame).
fn margins(h: u64) -> (i32, i32) {
    let (w, f) = (window_rect(h), frame_rect(h));
    (w.width() - f.width(), w.height() - f.height())
}

fn move_cursor(engine: &TestEngine, cursor: Point) {
    engine.gesture(GestureMsg::Move { cursor });
}

#[test]
fn move_keeps_the_cursor_offset_so_the_frame_follows_the_cursor_exactly() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at("AWWM.Test.Move", spot(0), &["--size", "500,350"]);
    let h = app.hwnd(0);
    let (f0, w0) = (frame_rect(h), window_rect(h));
    let start = Point::new(f0.left + 37, f0.top + 23);

    engine.start_gesture(1, h, GestureKind::Move, start);
    move_cursor(&engine, offset(start, 120, 80));
    let expected = f0.translate(120, 80);
    assert_eq!(wait_frame(h, expected), expected, "first step");

    move_cursor(&engine, offset(start, -45, 61));
    let expected = f0.translate(-45, 61);
    assert_eq!(wait_frame(h, expected), expected, "second step");
    engine.gesture(GestureMsg::End);

    assert_eq!(
        window_rect(h),
        w0.translate(-45, 61),
        "the window rect moved by the same delta"
    );
}

#[test]
fn resize_from_the_bottom_right_zone_grows_with_the_top_left_corner_fixed() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at("AWWM.Test.ResizeBR", spot(0), &["--size", "480,360"]);
    let h = app.hwnd(0);
    let f0 = frame_rect(h);
    let start = Point::new(f0.left + f0.width() * 5 / 6, f0.top + f0.height() * 5 / 6);

    engine.start_gesture(1, h, GestureKind::Resize, start);
    move_cursor(&engine, offset(start, 60, 40));
    engine.gesture(GestureMsg::End);

    let expected = Rect::new(f0.left, f0.top, f0.right + 60, f0.bottom + 40);
    assert_eq!(wait_frame(h, expected), expected);
}

#[test]
fn resize_from_the_left_middle_zone_moves_only_the_left_edge() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at("AWWM.Test.ResizeL", spot(0), &["--size", "480,360"]);
    let h = app.hwnd(0);
    let f0 = frame_rect(h);
    let start = Point::new(f0.left + f0.width() / 6, f0.top + f0.height() / 2);

    engine.start_gesture(1, h, GestureKind::Resize, start);
    move_cursor(&engine, offset(start, -70, 25));
    engine.gesture(GestureMsg::End);

    let expected = Rect::new(f0.left - 70, f0.top, f0.right, f0.bottom);
    assert_eq!(
        wait_frame(h, expected),
        expected,
        "right edge fixed, vertical movement ignored"
    );
}

#[test]
fn resize_below_the_minimum_size_keeps_the_right_edge_from_drifting() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at(
        "AWWM.Test.ResizeMin",
        spot(0),
        &["--size", "600,450", "--min-size", "500,400"],
    );
    let h = app.hwnd(0);
    let f0 = frame_rect(h);
    let (mx, _) = margins(h);
    let start = Point::new(f0.left + f0.width() / 6, f0.top + f0.height() / 2);
    // The app accepts a 500 px wide window rect at the least.
    let clamped = Rect::new(f0.right - (500 - mx), f0.top, f0.right, f0.bottom);

    engine.start_gesture(1, h, GestureKind::Resize, start);
    move_cursor(&engine, offset(start, 250, 0));
    assert_eq!(
        wait_frame(h, clamped),
        clamped,
        "clamped to the minimum, right edge fixed"
    );
    assert_eq!(window_rect(h).width(), 500);

    move_cursor(&engine, offset(start, 400, 0));
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(
        frame_rect(h),
        clamped,
        "dragging further does not move the anchored edge"
    );

    move_cursor(&engine, offset(start, -50, 0));
    let grown = Rect::new(f0.left - 50, f0.top, f0.right, f0.bottom);
    assert_eq!(
        wait_frame(h, grown),
        grown,
        "growing again follows the cursor"
    );
    engine.gesture(GestureMsg::End);
}

#[test]
fn resize_from_the_top_left_zone_below_the_minimum_keeps_the_bottom_right_corner() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at(
        "AWWM.Test.ResizeMinTL",
        spot(0),
        &["--size", "600,450", "--min-size", "500,400"],
    );
    let h = app.hwnd(0);
    let f0 = frame_rect(h);
    let (mx, my) = margins(h);
    let start = Point::new(f0.left + f0.width() / 6, f0.top + f0.height() / 6);

    engine.start_gesture(1, h, GestureKind::Resize, start);
    move_cursor(&engine, offset(start, 300, 300));
    engine.gesture(GestureMsg::End);

    let expected = Rect::new(
        f0.right - (500 - mx),
        f0.bottom - (400 - my),
        f0.right,
        f0.bottom,
    );
    assert_eq!(wait_frame(h, expected), expected);
    let w = window_rect(h);
    assert_eq!((w.width(), w.height()), (500, 400));
}

#[test]
fn resize_above_the_maximum_size_keeps_the_top_left_corner() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at(
        "AWWM.Test.ResizeMax",
        spot(0),
        &["--size", "500,400", "--max-size", "650,520"],
    );
    let h = app.hwnd(0);
    let f0 = frame_rect(h);
    let (mx, my) = margins(h);
    let start = Point::new(f0.left + f0.width() * 5 / 6, f0.top + f0.height() * 5 / 6);

    engine.start_gesture(1, h, GestureKind::Resize, start);
    move_cursor(&engine, offset(start, 300, 300));
    engine.gesture(GestureMsg::End);

    let expected = Rect::new(f0.left, f0.top, f0.left + 650 - mx, f0.top + 520 - my);
    assert_eq!(wait_frame(h, expected), expected);
}

fn snap_settings() -> Settings {
    let mut s = settings();
    // Only monitor/work-area edges: the user's own windows must not influence the result.
    s.snapping.window_edges = false;
    s.snapping.center_lines = false;
    s.snapping.grid_enabled = false;
    s
}

#[test]
fn snap_aligns_the_frame_flush_with_the_work_area_corner() {
    let _guard = setup();
    let engine = TestEngine::start(snap_settings());
    let app = TestApp::spawn_at("AWWM.Test.Snap", spot(0), &["--size", "500,350"]);
    let h = app.hwnd(0);
    let f0 = frame_rect(h);
    let work = awwm_engine::win::monitor::from_rect(f0)
        .expect("monitor")
        .work_area;
    // Unsnapped target: 6 px right of the left edge, 7 px above the bottom edge (within
    // the 16 px snap distance at 100 %).
    let near = f0.with_origin(Point::new(work.left + 6, work.bottom - 7 - f0.height()));
    let start = f0.center();
    let cursor = offset(start, near.left - f0.left, near.top - f0.top);

    engine.start_gesture(1, h, GestureKind::Move, start);
    move_cursor(&engine, cursor);
    assert_eq!(
        wait_frame(h, near),
        near,
        "without the snap modifier the frame follows the cursor"
    );

    engine.gesture(GestureMsg::Snap { active: true });
    let flush = f0.with_origin(Point::new(work.left, work.bottom - f0.height()));
    assert_eq!(
        wait_frame(h, flush),
        flush,
        "flush with the work area's left and bottom edges"
    );

    engine.gesture(GestureMsg::Snap { active: false });
    assert_eq!(
        wait_frame(h, near),
        near,
        "releasing the snap modifier re-evaluates"
    );
    engine.gesture(GestureMsg::End);
}

#[test]
fn snap_while_resizing_moves_only_the_dragged_edge_onto_the_work_area_edge() {
    let _guard = setup();
    let engine = TestEngine::start(snap_settings());
    let app = TestApp::spawn_at("AWWM.Test.SnapResize", spot(0), &["--size", "500,350"]);
    let h = app.hwnd(0);
    let f0 = frame_rect(h);
    let work = awwm_engine::win::monitor::from_rect(f0)
        .expect("monitor")
        .work_area;
    // Right-middle zone: only the right edge follows the cursor.
    let start = Point::new(f0.left + f0.width() * 5 / 6, f0.top + f0.height() / 2);
    let near = Rect::new(f0.left, f0.top, work.right - 9, f0.bottom);

    engine.start_gesture(1, h, GestureKind::Resize, start);
    move_cursor(&engine, offset(start, near.right - f0.right, 0));
    assert_eq!(wait_frame(h, near), near, "unsnapped");

    engine.gesture(GestureMsg::Snap { active: true });
    let flush = Rect::new(f0.left, f0.top, work.right, f0.bottom);
    assert_eq!(
        wait_frame(h, flush),
        flush,
        "right edge flush with the work area, other edges fixed"
    );
    engine.gesture(GestureMsg::End);
}

#[test]
fn cancel_restores_the_original_window_rect() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at("AWWM.Test.Cancel", spot(0), &["--size", "480,360"]);
    let h = app.hwnd(0);
    let (f0, w0) = (frame_rect(h), window_rect(h));
    let start = Point::new(f0.left + f0.width() * 5 / 6, f0.top + f0.height() * 5 / 6);
    engine.start_gesture(1, h, GestureKind::Resize, start);
    move_cursor(&engine, offset(start, 80, 60));
    let resized = Rect::new(f0.left, f0.top, f0.right + 80, f0.bottom + 60);
    assert_eq!(wait_frame(h, resized), resized);

    engine.gesture(GestureMsg::Cancel);

    assert!(
        wait_until(TIMEOUT, || window_rect(h) == w0),
        "window rect restored: {:?} != {w0:?}",
        window_rect(h)
    );
}

#[test]
fn cancel_after_a_move_restores_the_original_position() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at("AWWM.Test.CancelMove", spot(0), &["--size", "480,360"]);
    let h = app.hwnd(0);
    let (f0, w0) = (frame_rect(h), window_rect(h));
    engine.start_gesture(1, h, GestureKind::Move, f0.center());
    move_cursor(&engine, offset(f0.center(), 90, 70));
    assert_eq!(wait_frame(h, f0.translate(90, 70)), f0.translate(90, 70));

    engine.gesture(GestureMsg::Cancel);

    assert!(
        wait_until(TIMEOUT, || window_rect(h) == w0),
        "window rect restored: {:?} != {w0:?}",
        window_rect(h)
    );
}

#[test]
fn gesture_on_a_window_closed_mid_gesture_ends_cleanly() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let mut app = TestApp::spawn_at(
        "AWWM.Test.GestureClose",
        spot(0),
        &["--count", "2", "--size", "400,300"],
    );
    let (gone, other) = (app.hwnd(0), app.hwnd(1));
    let f = frame_rect(gone);
    engine.start_gesture(1, gone, GestureKind::Move, f.center());
    move_cursor(&engine, offset(f.center(), 30, 30));
    assert_eq!(wait_frame(gone, f.translate(30, 30)), f.translate(30, 30));

    app.close(0);
    move_cursor(&engine, offset(f.center(), 60, 60));
    engine.gesture(GestureMsg::End);

    assert!(
        engine.handle().managed().is_ok(),
        "the engine still responds"
    );
    // The next gesture works normally.
    let g = frame_rect(other);
    engine.start_gesture(2, other, GestureKind::Move, g.center());
    move_cursor(&engine, offset(g.center(), -40, 35));
    engine.gesture(GestureMsg::End);
    assert_eq!(
        wait_frame(other, g.translate(-40, 35)),
        g.translate(-40, 35)
    );
}

#[test]
fn gesture_on_an_already_closed_window_is_ignored() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let mut app = TestApp::spawn_at("AWWM.Test.GestureGone", spot(0), &["--count", "2"]);
    let (gone, other) = (app.hwnd(0), app.hwnd(1));
    app.close(0);

    engine.start_gesture(1, gone, GestureKind::Resize, Point::new(10, 10));
    move_cursor(&engine, Point::new(50, 50));
    engine.gesture(GestureMsg::End);

    let g = frame_rect(other);
    engine.start_gesture(2, other, GestureKind::Move, g.center());
    move_cursor(&engine, offset(g.center(), 25, 15));
    engine.gesture(GestureMsg::End);
    assert_eq!(wait_frame(other, g.translate(25, 15)), g.translate(25, 15));
}

#[test]
fn pin_frame_follows_a_pinned_window_that_is_moved() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_pinnable("AWWM.Test.PinFrame", spot(0), &["--size", "420,320"]);
    let h = app.hwnd(0);
    engine.handle().pin(h).expect("pin");
    let f0 = frame_rect(h);
    // 2 logical px border outside the frame (the test monitors run at 100 %).
    let dpi = awwm_engine::win::monitor::from_rect(f0)
        .expect("monitor")
        .dpi;
    let t = awwm_core::geometry::logical_to_physical(2, dpi);
    assert!(
        poll(TIMEOUT, || visible_overlays()
            .contains(&f0.inflate(t, t, t, t))
            .then_some(()))
        .is_some(),
        "pin frame around the window: {:?}",
        visible_overlays()
    );

    engine.start_gesture(1, h, GestureKind::Move, f0.center());
    move_cursor(&engine, offset(f0.center(), 70, 50));
    engine.gesture(GestureMsg::End);
    let f1 = f0.translate(70, 50);
    assert_eq!(wait_frame(h, f1), f1);

    assert!(
        poll(TIMEOUT, || visible_overlays()
            .contains(&f1.inflate(t, t, t, t))
            .then_some(()))
        .is_some(),
        "pin frame follows the window: {:?}",
        visible_overlays()
    );
}

#[test]
fn move_gesture_places_the_window_on_every_monitor() {
    let _guard = setup();
    let all = monitors();
    if all.len() < 2 {
        eprintln!(
            "note: only {} monitor(s); the multi-monitor test needs at least 2 - skipped",
            all.len()
        );
        return;
    }
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at(
        "AWWM.Test.Monitors",
        primary_spot_for(&all),
        &["--size", "420,300"],
    );
    let h = app.hwnd(0);
    for (i, m) in all.iter().enumerate() {
        let f = frame_rect(h);
        let from_dpi = awwm_engine::win::monitor::from_rect(f).map_or(96, |x| x.dpi);
        let target = Point::new(m.work_area.left + 80, m.work_area.top + 60);
        let start = f.center();
        let cursor = offset(start, target.x - f.left, target.y - f.top);
        let seq = u64::try_from(i).expect("index") + 1;

        engine.start_gesture(seq, h, GestureKind::Move, start);
        move_cursor(&engine, cursor);
        engine.gesture(GestureMsg::End);

        let expected = f.with_origin(target);
        let got = wait_frame(h, expected);
        let on = awwm_engine::win::monitor::from_rect(got).expect("monitor");
        assert_eq!(
            on.bounds, m.bounds,
            "monitor {i}: the frame {got:?} must be on {:?}",
            m.bounds
        );
        if m.dpi == from_dpi {
            assert_eq!(got, expected, "monitor {i} (same DPI): exact placement");
        } else {
            // A per-monitor-aware window keeps its window rect but its invisible borders
            // are rescaled, so the visible frame may shift by a few pixels.
            let off = (got.left - expected.left)
                .abs()
                .max((got.top - expected.top).abs());
            assert!(
                off <= 16,
                "monitor {i} (DPI {from_dpi} -> {}): {got:?} vs {expected:?}",
                m.dpi
            );
        }
        eprintln!("monitor {i} {:?} dpi {}: frame {got:?}", m.bounds, m.dpi);
    }
}

fn primary_spot_for(all: &[awwm_core::MonitorInfo]) -> Point {
    let w = all
        .iter()
        .find(|m| m.primary)
        .map_or(primary().work_area, |m| m.work_area);
    Point::new(w.left + 100, w.top + 100)
}
