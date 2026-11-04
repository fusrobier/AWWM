//! Pin / unpin (always on top) against real windows.
#![cfg(windows)]

mod common;

use awwm_engine::{EngineError, ManagedSource};
use common::{TestApp, TestEngine, is_layered, is_topmost, settings, setup, spot};

#[test]
fn pin_makes_window_topmost_without_activating_it() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let mut app = TestApp::spawn_pinnable("AWWM.Test.PinActivate", spot(0), &[]);
    let h = app.hwnd(0);
    assert!(!is_topmost(h));
    let stats_before = app.stats(0);
    assert_ne!(
        common::foreground(),
        h,
        "the test window must not start in the foreground"
    );

    let info = engine.handle().pin(h).expect("pin");

    assert!(info.pinned);
    assert_eq!(info.opacity, 100);
    assert_eq!(info.source, ManagedSource::Manual);
    assert!(is_topmost(h), "WS_EX_TOPMOST must be set");
    assert_eq!(
        app.stats(0),
        stats_before,
        "pinning must not activate or focus the window"
    );
    assert_ne!(
        common::foreground(),
        h,
        "pinning must not bring the window to the foreground"
    );
}

#[test]
fn unpin_removes_topmost_and_forgets_the_window() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_pinnable("AWWM.Test.Unpin", spot(0), &[]);
    let h = app.hwnd(0);
    engine.handle().pin(h).expect("pin");

    engine.handle().unpin(h).expect("unpin");

    assert!(!is_topmost(h), "WS_EX_TOPMOST must be removed");
    assert!(engine.managed(h).is_none());
}

#[test]
fn window_that_was_topmost_before_pin_stays_topmost_after_unpin() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_pinnable("AWWM.Test.AlreadyTopmost", spot(0), &["--topmost"]);
    let h = app.hwnd(0);
    assert!(is_topmost(h));

    engine.handle().pin(h).expect("pin");
    engine.handle().unpin(h).expect("unpin");

    assert!(
        is_topmost(h),
        "a window that was topmost on its own must stay topmost"
    );
}

#[test]
fn window_made_topmost_by_the_app_stays_topmost_after_unpin() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let mut app = TestApp::spawn_pinnable("AWWM.Test.AppTopmost", spot(0), &[]);
    let h = app.hwnd(0);
    app.set_topmost(0, true);
    assert!(is_topmost(h));

    engine.handle().pin(h).expect("pin");
    engine.handle().unpin(h).expect("unpin");

    assert!(is_topmost(h));
}

#[test]
fn windows_are_pinned_and_unpinned_independently() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_pinnable("AWWM.Test.PinMany", spot(0), &["--count", "2"]);
    let (a, b) = (app.hwnd(0), app.hwnd(1));
    engine.handle().pin(a).expect("pin a");
    engine.handle().pin(b).expect("pin b");
    assert!(is_topmost(a) && is_topmost(b));

    engine.handle().unpin(a).expect("unpin a");

    assert!(!is_topmost(a), "a was unpinned");
    assert!(is_topmost(b), "b is still pinned");
    assert!(engine.managed(a).is_none());
    assert!(engine.managed(b).is_some_and(|m| m.pinned));
}

#[test]
fn pinning_twice_is_idempotent() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_pinnable("AWWM.Test.PinTwice", spot(0), &[]);
    let h = app.hwnd(0);

    engine.handle().pin(h).expect("first pin");
    engine.handle().pin(h).expect("second pin");

    let managed = engine.handle().managed().expect("list");
    assert_eq!(managed.iter().filter(|m| m.hwnd == h).count(), 1);
    engine.handle().unpin(h).expect("unpin");
    assert!(!is_topmost(h));
}

#[test]
fn unpin_all_releases_every_window() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_pinnable("AWWM.Test.UnpinAll", spot(0), &["--count", "3"]);
    for i in 0..3 {
        engine.handle().pin(app.hwnd(i)).expect("pin");
    }
    engine
        .handle()
        .set_opacity(app.hwnd(1), 60)
        .expect("opacity");

    engine.handle().unpin_all().expect("unpin all");

    assert!(engine.handle().managed().expect("list").is_empty());
    for i in 0..3 {
        assert!(!is_topmost(app.hwnd(i)), "window {i} still topmost");
        assert!(!is_layered(app.hwnd(i)), "window {i} still layered");
    }
}

#[test]
fn pin_of_a_window_that_vetoes_topmost_fails_without_leaving_a_trace() {
    let _guard = setup();
    let mut s = settings();
    s.pinning.default_opacity = 70;
    let engine = TestEngine::start(s);
    // The app vetoes every move into the topmost band (WM_WINDOWPOSCHANGING).
    let app = TestApp::spawn_at(
        "AWWM.Test.Veto",
        spot(0),
        &["--refuse-topmost-ms", "600000"],
    );
    let h = app.hwnd(0);

    let err = engine
        .handle()
        .pin(h)
        .expect_err("the window refuses topmost");

    // A manual pin reports the refusal right away (it is never deferred) …
    assert!(matches!(err, EngineError::TopmostRefused(_)), "{err:?}");
    // … with the stable "unsupported" code the UI understands.
    assert_eq!(err.code(), "unsupported");
    assert!(engine.managed(h).is_none());
    assert!(!is_topmost(h));
    assert!(
        !is_layered(h),
        "the default opacity must not be applied either"
    );
}
