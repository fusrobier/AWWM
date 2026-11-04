//! Behaviour driven by WinEvents on managed windows: state re-assertion and the pin frame
//! following, hiding and showing with its window. Uses only non-activating commands.
#![cfg(windows)]

mod common;

use awwm_core::settings::opacity_to_alpha;
use common::{
    TIMEOUT, TestApp, TestEngine, TopmostRefused, alpha, frame_rect, is_topmost, os_allows_topmost,
    pin_frame_around, poll, retry_if_topmost_refused, settings, setup, spot, visible_overlays,
    wait_until,
};

fn frame_visible_around(h: u64) -> bool {
    visible_overlays().contains(&pin_frame_around(frame_rect(h)))
}

#[test]
fn topmost_reset_by_the_app_is_reasserted_when_the_window_is_restored() {
    let _guard = setup();
    retry_if_topmost_refused(|| {
        let engine = TestEngine::start(settings());
        let mut app = TestApp::spawn_pinnable("AWWM.Test.ReassertTop", spot(0), &[]);
        let h = app.hwnd(0);
        engine.handle().pin(h).expect("pin");
        app.set_topmost(0, false);
        assert!(!is_topmost(h), "the app reset its own z-order");

        app.ok("minimize 0");
        app.ok("restore 0");

        if !wait_until(TIMEOUT, || is_topmost(h)) {
            // Distinguish an engine failure from Windows refusing topmost while a
            // full-screen application has the foreground.
            assert!(
                !os_allows_topmost(h),
                "topmost re-applied on EVENT_SYSTEM_MINIMIZEEND"
            );
            return Err(TopmostRefused(h));
        }
        assert!(engine.managed(h).is_some_and(|m| m.pinned));
        Ok(())
    });
}

#[test]
fn alpha_reset_by_the_app_is_reasserted_when_the_window_is_restored() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let mut app = TestApp::spawn_at("AWWM.Test.ReassertAlpha", spot(0), &[]);
    let h = app.hwnd(0);
    engine.handle().set_opacity(h, 55).expect("opacity");
    app.ok("alpha 0 255");
    assert_eq!(alpha(h), Some(255), "the app reset its own alpha");

    app.ok("minimize 0");
    app.ok("restore 0");

    assert!(
        wait_until(TIMEOUT, || alpha(h) == Some(opacity_to_alpha(55))),
        "alpha re-applied: {:?}",
        alpha(h)
    );
}

#[test]
fn reassertion_can_be_turned_off() {
    let _guard = setup();
    let mut s = settings();
    s.pinning.reassert_state = false;
    let engine = TestEngine::start(s);
    let mut app = TestApp::spawn_pinnable("AWWM.Test.NoReassert", spot(0), &[]);
    let h = app.hwnd(0);
    engine.handle().pin(h).expect("pin");
    app.set_topmost(0, false);

    app.ok("minimize 0");
    app.ok("restore 0");

    std::thread::sleep(std::time::Duration::from_millis(800));
    assert!(
        !is_topmost(h),
        "the app's own z-order wins when re-assertion is off"
    );
}

#[test]
fn pin_frame_surrounds_a_pinned_window() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_pinnable("AWWM.Test.Frame", spot(0), &[]);
    let h = app.hwnd(0);

    engine.handle().pin(h).expect("pin");

    assert!(
        wait_until(TIMEOUT, || frame_visible_around(h)),
        "frame around the window: {:?}",
        visible_overlays()
    );
}

#[test]
fn pin_frame_is_removed_on_unpin() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_pinnable("AWWM.Test.FrameUnpin", spot(0), &[]);
    let h = app.hwnd(0);
    engine.handle().pin(h).expect("pin");
    assert!(wait_until(TIMEOUT, || frame_visible_around(h)));
    let around = pin_frame_around(frame_rect(h));

    engine.handle().unpin(h).expect("unpin");

    assert!(
        wait_until(TIMEOUT, || !visible_overlays().contains(&around)),
        "frame removed"
    );
}

#[test]
fn pin_frame_is_not_shown_when_borders_are_disabled() {
    let _guard = setup();
    let mut s = settings();
    s.pinning.show_border = false;
    let engine = TestEngine::start(s);
    let app = TestApp::spawn_pinnable("AWWM.Test.NoFrame", spot(0), &[]);
    let h = app.hwnd(0);

    engine.handle().pin(h).expect("pin");

    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(
        !frame_visible_around(h),
        "no frame: {:?}",
        visible_overlays()
    );
}

#[test]
fn pin_frame_follows_when_the_app_moves_its_window() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let mut app = TestApp::spawn_pinnable("AWWM.Test.FrameFollow", spot(0), &["--size", "400,300"]);
    let h = app.hwnd(0);
    engine.handle().pin(h).expect("pin");
    assert!(wait_until(TIMEOUT, || frame_visible_around(h)));
    let w = common::window_rect(h);
    let before = frame_rect(h);

    app.ok(&format!("pos 0 {} {} 460 340", w.left + 55, w.top + 35));

    let moved = frame_rect(h);
    assert_eq!(
        (moved.left, moved.top),
        (before.left + 55, before.top + 35),
        "sanity: the app moved its window"
    );
    assert!(
        poll(TIMEOUT, || frame_visible_around(h).then_some(())).is_some(),
        "the frame follows the window (EVENT_OBJECT_LOCATIONCHANGE): {:?} vs {:?}",
        visible_overlays(),
        pin_frame_around(frame_rect(h))
    );
}

#[test]
fn pin_frame_hides_while_the_window_is_minimized_and_returns_on_restore() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let mut app = TestApp::spawn_pinnable("AWWM.Test.FrameMin", spot(0), &[]);
    let h = app.hwnd(0);
    engine.handle().pin(h).expect("pin");
    assert!(wait_until(TIMEOUT, || frame_visible_around(h)));
    let around = pin_frame_around(frame_rect(h));

    app.ok("minimize 0");
    assert!(
        wait_until(TIMEOUT, || !visible_overlays().contains(&around)),
        "hidden while minimized"
    );

    app.ok("restore 0");
    assert!(
        wait_until(TIMEOUT, || frame_visible_around(h)),
        "shown again: {:?}",
        visible_overlays()
    );
}

#[test]
fn pin_frame_hides_while_the_window_is_hidden_and_returns_when_shown() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let mut app = TestApp::spawn_pinnable("AWWM.Test.FrameHide", spot(0), &[]);
    let h = app.hwnd(0);
    engine.handle().pin(h).expect("pin");
    assert!(wait_until(TIMEOUT, || frame_visible_around(h)));
    let around = pin_frame_around(frame_rect(h));

    app.ok("hide 0");
    assert!(
        wait_until(TIMEOUT, || !visible_overlays().contains(&around)),
        "hidden with the window"
    );

    app.ok("show 0");
    assert!(
        wait_until(TIMEOUT, || frame_visible_around(h)),
        "shown again: {:?}",
        visible_overlays()
    );
    assert!(is_topmost(h), "still pinned");
}

#[test]
fn window_list_reports_test_windows_and_their_managed_state() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_pinnable("AWWM.Test.List", spot(0), &["--title", "Listed window"]);
    let h = app.hwnd(0);
    let find = || {
        engine
            .handle()
            .list_windows()
            .expect("list")
            .into_iter()
            .find(|w| w.hwnd == h)
    };

    let before = find().expect("listed");
    assert_eq!(before.title, "Listed window");
    assert_eq!(before.class_name, "AWWM.Test.List");
    assert!(
        before.exe_name.eq_ignore_ascii_case("awwm-testapp.exe"),
        "{}",
        before.exe_name
    );
    assert_eq!(before.pid, app.pid());
    assert!(!before.managed && !before.topmost && before.accessible);

    engine.handle().pin(h).expect("pin");

    let after = find().expect("listed");
    assert!(after.managed && after.topmost);
}

#[test]
fn managed_list_reports_the_current_title() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let mut app = TestApp::spawn_pinnable("AWWM.Test.LiveTitle", spot(0), &["--title", "Before"]);
    let h = app.hwnd(0);
    engine.handle().pin(h).expect("pin");

    app.set_title(0, "After");

    assert_eq!(engine.managed(h).map(|m| m.title), Some("After".to_owned()));
}
