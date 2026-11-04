//! Error handling of engine commands.
#![cfg(windows)]

mod common;

use awwm_engine::{Engine, EngineError, EngineOptions};
use common::{TestApp, TestEngine, is_topmost, settings, setup, spot};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::w;

#[test]
fn pin_of_a_null_handle_is_window_not_found() {
    let _guard = setup();
    let engine = TestEngine::start(settings());

    assert_eq!(engine.handle().pin(0), Err(EngineError::WindowNotFound));
}

#[test]
fn pin_of_a_closed_window_is_window_not_found() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let mut app = TestApp::spawn_at("AWWM.Test.Closed", spot(0), &["--count", "2"]);
    let h = app.hwnd(0);
    app.close(0);

    assert_eq!(engine.handle().pin(h), Err(EngineError::WindowNotFound));
    assert_eq!(
        engine.handle().set_opacity(h, 50),
        Err(EngineError::WindowNotFound)
    );
    assert!(engine.handle().managed().expect("list").is_empty());
}

#[test]
fn unpin_of_an_unmanaged_window_is_window_not_found() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at("AWWM.Test.UnpinUnmanaged", spot(0), &[]);

    assert_eq!(
        engine.handle().unpin(app.hwnd(0)),
        Err(EngineError::WindowNotFound)
    );
}

#[test]
fn pin_of_a_window_of_the_engine_process_is_refused() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    // SAFETY: creates (and below destroys) a hidden popup of a system class on this thread.
    let own = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("own"),
            WS_POPUP,
            0,
            0,
            50,
            50,
            None,
            None,
            None,
            None,
        )
    }
    .expect("own window");

    let result = engine.handle().pin(own.0 as usize as u64);

    // SAFETY: destroys the window created above on the same thread.
    unsafe {
        let _ = DestroyWindow(own);
    }
    assert!(
        matches!(result, Err(EngineError::InvalidArgument(_))),
        "{result:?}"
    );
}

#[test]
fn opacity_out_of_range_is_clamped_to_opaque() {
    let _guard = setup();
    let engine = TestEngine::start(settings());
    let app = TestApp::spawn_at("AWWM.Test.Clamp", spot(0), &[]);
    let h = app.hwnd(0);

    let info = engine.handle().set_opacity(h, 250).expect("clamped");

    assert_eq!(info.opacity, 100);
    assert!(engine.managed(h).is_none());
    assert!(!common::is_layered(h));
}

#[test]
fn a_second_engine_cannot_start_while_one_is_running() {
    let _guard = setup();
    let _engine = TestEngine::start(settings());

    let second = Engine::start(
        EngineOptions {
            settings: settings(),
            session_path: None,
        },
        std::sync::Arc::new(|_| {}),
    );

    assert!(
        matches!(second, Err(EngineError::Internal(_))),
        "{second:?}"
    );
}

#[test]
fn commands_after_shutdown_fail_without_touching_windows() {
    let _guard = setup();
    let app = TestApp::spawn_at("AWWM.Test.AfterShutdown", spot(0), &[]);
    let engine = TestEngine::start(settings());
    let handle = engine.handle().clone();
    engine.shutdown(true);

    let result = handle.pin(app.hwnd(0));

    assert!(
        matches!(result, Err(EngineError::Internal(_))),
        "{result:?}"
    );
    assert!(!is_topmost(app.hwnd(0)));
}
