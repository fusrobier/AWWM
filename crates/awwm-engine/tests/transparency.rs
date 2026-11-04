//! Window opacity (layered-window alpha) against real windows.
#![cfg(windows)]

mod common;

use awwm_core::settings::opacity_to_alpha;
use awwm_engine::EngineError;
use common::{
    TestApp, TestEngine, alpha, is_layered, is_topmost, layered_attributes, settings, setup, spot,
};

const LWA_ALPHA: u32 = 0x2;

#[test]
fn opacity_sets_the_matching_layered_alpha() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at("AWWM.Test.Opacity", spot(0), &[]);
    let h = app.hwnd(0);
    assert!(!is_layered(h));

    for opacity in [85u8, 50, 0, 37] {
        let info = engine
            .handle()
            .set_opacity(h, opacity)
            .expect("set opacity");
        assert_eq!(info.opacity, opacity);
        assert!(is_layered(h), "WS_EX_LAYERED at {opacity} %");
        let (_, a, flags) = layered_attributes(h).expect("readable attributes");
        assert_eq!(a, opacity_to_alpha(opacity), "alpha at {opacity} %");
        assert_ne!(flags & LWA_ALPHA, 0, "LWA_ALPHA at {opacity} %");
    }
    assert!(!is_topmost(h), "opacity alone must not pin");
}

#[test]
fn opacity_100_releases_an_unpinned_window_and_removes_the_layered_style() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at("AWWM.Test.Opacity100", spot(0), &[]);
    let h = app.hwnd(0);
    engine.handle().set_opacity(h, 50).expect("50 %");

    let info = engine.handle().set_opacity(h, 100).expect("100 %");

    assert_eq!(info.opacity, 100);
    assert!(
        !is_layered(h),
        "WS_EX_LAYERED added by AWWM must be removed"
    );
    assert!(engine.managed(h).is_none(), "nothing left to manage");
}

#[test]
fn opacity_100_on_a_pinned_window_removes_the_layered_style_but_keeps_the_pin() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_pinnable("AWWM.Test.Opacity100Pinned", spot(0), &[]);
    let h = app.hwnd(0);
    engine.handle().pin(h).expect("pin");
    engine.handle().set_opacity(h, 40).expect("40 %");
    assert!(is_layered(h));

    engine.handle().set_opacity(h, 100).expect("100 %");

    assert!(!is_layered(h));
    assert!(is_topmost(h));
    assert!(
        engine
            .managed(h)
            .is_some_and(|m| m.pinned && m.opacity == 100)
    );
}

#[test]
fn unpin_removes_the_layered_style_added_by_the_engine() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_pinnable("AWWM.Test.UnpinLayered", spot(0), &[]);
    let h = app.hwnd(0);
    engine.handle().pin(h).expect("pin");
    engine.handle().set_opacity(h, 50).expect("50 %");

    engine.handle().unpin(h).expect("unpin");

    assert!(!is_layered(h));
    assert!(!is_topmost(h));
}

#[test]
fn pin_applies_the_default_opacity() {
    let _guard = setup();
    let mut s = settings();
    s.pinning.default_opacity = 80;
    let engine = TestEngine::start(s);
    let app = TestApp::spawn_pinnable("AWWM.Test.DefaultOpacity", spot(0), &[]);
    let h = app.hwnd(0);

    let info = engine.handle().pin(h).expect("pin");

    assert_eq!(info.opacity, 80);
    assert_eq!(alpha(h), Some(opacity_to_alpha(80)));
}

#[test]
fn opacity_is_independent_per_window() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at("AWWM.Test.OpacityMany", spot(0), &["--count", "2"]);
    let (a, b) = (app.hwnd(0), app.hwnd(1));

    engine.handle().set_opacity(a, 70).expect("a");
    engine.handle().set_opacity(b, 40).expect("b");
    assert_eq!(alpha(a), Some(opacity_to_alpha(70)));
    assert_eq!(alpha(b), Some(opacity_to_alpha(40)));

    engine
        .handle()
        .set_opacity(a, 100)
        .expect("a back to opaque");
    assert!(!is_layered(a));
    assert_eq!(alpha(b), Some(opacity_to_alpha(40)), "b is unaffected");
}

#[test]
fn window_layered_by_the_app_gets_relative_alpha_and_its_own_alpha_back() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at("AWWM.Test.AppLayered", spot(0), &["--layered-alpha", "200"]);
    let h = app.hwnd(0);
    assert_eq!(layered_attributes(h), Some((0, 200, LWA_ALPHA)));

    engine.handle().set_opacity(h, 50).expect("50 %");
    // 50 % of the app's own 200/255.
    let expected = u8::try_from((u32::from(opacity_to_alpha(50)) * 200 + 127) / 255).expect("byte");
    assert_eq!(alpha(h), Some(expected));

    engine.handle().unpin(h).expect("release");
    assert!(is_layered(h), "the app's own WS_EX_LAYERED must stay");
    assert_eq!(
        layered_attributes(h),
        Some((0, 200, LWA_ALPHA)),
        "the app's own alpha is restored"
    );
}

#[test]
fn window_layered_by_the_app_keeps_its_alpha_after_opacity_100() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_pinnable(
        "AWWM.Test.AppLayered100",
        spot(0),
        &["--layered-alpha", "200"],
    );
    let h = app.hwnd(0);
    engine.handle().pin(h).expect("pin");
    engine.handle().set_opacity(h, 30).expect("30 %");

    engine.handle().set_opacity(h, 100).expect("100 %");

    assert!(is_layered(h));
    assert_eq!(layered_attributes(h), Some((0, 200, LWA_ALPHA)));
}

#[test]
fn per_pixel_layered_window_rejects_opacity() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at("AWWM.Test.PerPixel", spot(0), &["--per-pixel"]);
    let h = app.hwnd(0);
    assert!(is_layered(h));
    assert_eq!(
        layered_attributes(h),
        None,
        "UpdateLayeredWindow windows have no readable attributes"
    );

    let err = engine
        .handle()
        .set_opacity(h, 50)
        .expect_err("per-pixel windows cannot get uniform alpha");

    assert!(matches!(err, EngineError::Unsupported(_)), "{err:?}");
    assert!(
        engine.managed(h).is_none(),
        "a failed request leaves no trace"
    );
    assert!(!is_topmost(h));
}

#[test]
fn per_pixel_layered_window_can_be_pinned_at_full_opacity() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_pinnable("AWWM.Test.PerPixelPin", spot(0), &["--per-pixel"]);
    let h = app.hwnd(0);

    let info = engine.handle().pin(h).expect("pin at 100 %");
    assert!(info.pinned);
    assert!(is_topmost(h));

    let err = engine
        .handle()
        .set_opacity(h, 50)
        .expect_err("still no uniform alpha");
    assert!(matches!(err, EngineError::Unsupported(_)), "{err:?}");
    assert!(is_topmost(h), "the failed opacity change keeps the pin");

    engine.handle().unpin(h).expect("unpin");
    assert!(!is_topmost(h));
    assert!(is_layered(h), "the app's per-pixel layering is untouched");
}
