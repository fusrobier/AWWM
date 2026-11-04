//! Lifetime of managed windows: destruction and handle changes (reattachment).
#![cfg(windows)]

mod common;

use awwm_core::settings::opacity_to_alpha;
use awwm_engine::EngineEvent;
use common::{
    QUIET_PERIOD, TIMEOUT, TestApp, TestEngine, alpha, is_topmost, retry_if_topmost_refused,
    settings, setup, spot,
};

#[test]
fn closing_a_managed_window_emits_managed_changed_without_it() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let mut app = TestApp::spawn_pinnable("AWWM.Test.Close", spot(0), &["--count", "2"]);
    let (closed, kept) = (app.hwnd(0), app.hwnd(1));
    engine.handle().pin(closed).expect("pin closed");
    engine.handle().pin(kept).expect("pin kept");
    let mark = engine.mark();

    app.close(0);

    let event = engine.wait_event_since(mark, TIMEOUT, |e| {
        matches!(e, EngineEvent::ManagedChanged(list) if !list.iter().any(|m| m.hwnd == closed))
    });
    let Some(EngineEvent::ManagedChanged(list)) = event else {
        panic!("no ManagedChanged after closing the window")
    };
    assert!(
        list.iter().any(|m| m.hwnd == kept),
        "the other window is still managed: {list:?}"
    );
    assert!(engine.managed(closed).is_none());
}

#[test]
fn recreated_window_within_grace_period_inherits_pin_and_opacity() {
    let _guard = setup();
    retry_if_topmost_refused(|| {
        let engine = TestEngine::start(settings());
        let mut app =
            TestApp::spawn_pinnable("AWWM.Test.Reattach", spot(0), &["--title", "Reattach me"]);
        let old = app.hwnd(0);
        engine.handle().pin(old).expect("pin");
        engine.handle().set_opacity(old, 60).expect("opacity");

        let new = app.recreate(0);
        assert_ne!(new, old);

        let info = engine.wait_pinned_by_engine(new)?;
        assert!(info.pinned);
        assert_eq!(info.opacity, 60);
        assert!(is_topmost(new), "the replacement is topmost");
        assert_eq!(
            alpha(new),
            Some(opacity_to_alpha(60)),
            "the replacement is transparent"
        );
        assert!(engine.managed(old).is_none(), "the old handle is forgotten");
        Ok(())
    });
}

#[test]
fn recreated_window_is_not_reattached_when_grace_period_is_zero() {
    let _guard = setup();
    let mut s = settings();
    s.general.reattach_grace_secs = 0;
    let engine = TestEngine::start(s);
    let mut app = TestApp::spawn_pinnable("AWWM.Test.NoReattach", spot(0), &[]);
    let old = app.hwnd(0);
    engine.handle().pin(old).expect("pin");

    let new = app.recreate(0);

    engine.wait_unmanaged(old);
    std::thread::sleep(QUIET_PERIOD);
    assert!(
        engine.managed(new).is_none(),
        "no reattachment without a grace period"
    );
    assert!(!is_topmost(new));
}

#[test]
fn window_of_another_class_does_not_inherit_a_departed_window() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let mut first = TestApp::spawn_pinnable("AWWM.Test.DepartedA", spot(0), &[]);
    let old = first.hwnd(0);
    engine.handle().pin(old).expect("pin");
    first.close(0);
    engine.wait_unmanaged(old);

    let other = TestApp::spawn_at("AWWM.Test.DepartedB", spot(1), &[]);

    std::thread::sleep(QUIET_PERIOD);
    assert!(engine.managed(other.hwnd(0)).is_none());
    assert!(!is_topmost(other.hwnd(0)));
}

#[test]
fn restarted_app_within_grace_period_inherits_the_state() {
    let _guard = setup();
    retry_if_topmost_refused(|| {
        let engine = TestEngine::start(settings());
        let first = TestApp::spawn_pinnable("AWWM.Test.Restart", spot(0), &[]);
        let old = first.hwnd(0);
        engine.handle().pin(old).expect("pin");
        engine.handle().set_opacity(old, 75).expect("opacity");
        drop(first);
        engine.wait_unmanaged(old);

        let second = TestApp::spawn_at("AWWM.Test.Restart", spot(0), &[]);

        let info = engine.wait_pinned_by_engine(second.hwnd(0))?;
        assert!(info.pinned);
        assert_eq!(info.opacity, 75);
        assert!(is_topmost(second.hwnd(0)));
        Ok(())
    });
}
