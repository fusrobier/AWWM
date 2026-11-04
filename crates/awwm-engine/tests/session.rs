//! Session persistence / crash recovery and engine start-stop.
#![cfg(windows)]

mod common;

use std::path::Path;

use awwm_core::settings::{Settings, opacity_to_alpha};
use awwm_engine::ManagedSource;
use awwm_engine::manager::ManagedEntry;
use awwm_engine::transparency::OriginalLayering;
use common::{TempDir, TestApp, TestEngine, alpha, is_layered, is_topmost, settings, setup, spot};

fn recovery(recover: bool) -> Settings {
    let mut s = settings();
    s.general.recover_session = recover;
    s
}

/// Pins `h` at 60 % with an engine that then stops *without* restoring it (like a crash
/// or a forced kill would).
fn leave_managed(session: &Path, h: u64) {
    let engine = TestEngine::start_with_session(recovery(true), Some(session.to_path_buf()));
    engine
        .handle()
        .pin(h)
        .unwrap_or_else(|e| panic!("pin: {e}"));
    engine
        .handle()
        .set_opacity(h, 60)
        .unwrap_or_else(|e| panic!("opacity: {e}"));
    engine.shutdown(false);
}

#[test]
fn shutdown_without_restore_keeps_the_window_state_and_the_session_file() {
    let _guard = setup();
    let dir = TempDir::new("session-keep");
    let app = TestApp::spawn_pinnable("AWWM.Test.SessionKeep", spot(0), &[]);
    let h = app.hwnd(0);

    leave_managed(&dir.session_file(), h);

    assert!(is_topmost(h), "still topmost");
    assert_eq!(alpha(h), Some(opacity_to_alpha(60)), "still transparent");
    let text = std::fs::read_to_string(dir.session_file()).expect("session file exists");
    assert!(
        text.contains(&format!("\"hwnd\": {h}")),
        "the window is recorded: {text}"
    );
}

#[test]
fn next_start_with_recovery_adopts_the_window_as_recovered() {
    let _guard = setup();
    let dir = TempDir::new("session-adopt");
    let app = TestApp::spawn_pinnable("AWWM.Test.SessionAdopt", spot(0), &[]);
    let h = app.hwnd(0);
    leave_managed(&dir.session_file(), h);

    let engine = TestEngine::start_with_session(recovery(true), Some(dir.session_file()));

    let info = engine.managed(h).expect("re-adopted at start");
    assert_eq!(info.source, ManagedSource::Recovered);
    assert!(info.pinned);
    assert_eq!(info.opacity, 60);
    assert!(is_topmost(h));
    assert_eq!(alpha(h), Some(opacity_to_alpha(60)));
}

#[test]
fn shutdown_with_restore_after_recovery_restores_the_window_and_removes_the_session() {
    let _guard = setup();
    let dir = TempDir::new("session-restore");
    let app = TestApp::spawn_pinnable("AWWM.Test.SessionRestore", spot(0), &[]);
    let h = app.hwnd(0);
    leave_managed(&dir.session_file(), h);
    let engine = TestEngine::start_with_session(recovery(true), Some(dir.session_file()));
    assert!(engine.managed(h).is_some());

    engine.shutdown(true);

    assert!(!is_topmost(h), "topmost removed");
    assert!(!is_layered(h), "WS_EX_LAYERED added by AWWM removed");
    assert!(!dir.session_file().exists(), "nothing left to recover");
}

#[test]
fn next_start_without_recovery_restores_the_window() {
    let _guard = setup();
    let dir = TempDir::new("session-norecover");
    let app = TestApp::spawn_pinnable("AWWM.Test.SessionNoRecover", spot(0), &[]);
    let h = app.hwnd(0);
    leave_managed(&dir.session_file(), h);

    let engine = TestEngine::start_with_session(recovery(false), Some(dir.session_file()));

    assert!(engine.managed(h).is_none(), "not adopted");
    assert!(!is_topmost(h), "original z-order restored");
    assert!(!is_layered(h), "original layering restored");
    assert!(!dir.session_file().exists(), "the session is consumed");
}

#[test]
fn recovery_skips_an_entry_whose_window_belongs_to_another_process() {
    let _guard = setup();
    let dir = TempDir::new("session-pid");
    let app = TestApp::spawn_at("AWWM.Test.SessionPid", spot(0), &[]);
    let h = app.hwnd(0);
    // A stale entry: same handle value, but recorded for a different process.
    let entry = ManagedEntry {
        hwnd: h,
        pid: app.pid().wrapping_add(4),
        exe_path: env!("CARGO_BIN_EXE_awwm-testapp").to_owned(),
        class_name: "AWWM.Test.SessionPid".to_owned(),
        title: "AWWM Test Window".to_owned(),
        pinned: true,
        opacity: 50,
        original_topmost: false,
        original_layering: Some(OriginalLayering::NotLayered),
        source: ManagedSource::Manual,
        rule_id: None,
        pin_pending: false,
        release_pending: false,
    };
    let json = serde_json::json!({ "version": 1, "entries": [entry] });
    std::fs::write(dir.session_file(), serde_json::to_vec(&json).expect("json"))
        .expect("write session");

    let engine = TestEngine::start_with_session(recovery(true), Some(dir.session_file()));

    assert!(
        engine.managed(h).is_none(),
        "a recycled handle must not be adopted"
    );
    assert!(!is_topmost(h));
    assert!(!is_layered(h));
}

#[test]
fn corrupt_session_file_is_ignored() {
    let _guard = setup();
    let dir = TempDir::new("session-corrupt");
    std::fs::write(dir.session_file(), b"{ not json").expect("write");

    let engine = TestEngine::start_with_session(recovery(true), Some(dir.session_file()));

    assert!(engine.handle().managed().expect("list").is_empty());
}

#[test]
fn engine_can_be_started_and_stopped_repeatedly() {
    let _guard = setup();
    let app = TestApp::spawn_pinnable("AWWM.Test.Restart", spot(0), &[]);
    let h = app.hwnd(0);
    for round in 0..5 {
        let engine = TestEngine::start(settings());
        engine
            .handle()
            .pin(h)
            .unwrap_or_else(|e| panic!("round {round}: pin failed: {e}"));
        assert!(is_topmost(h), "round {round}");
        engine.shutdown(true);
        assert!(!is_topmost(h), "round {round}: restored on shutdown");
    }
}

#[test]
fn dropping_the_engine_restores_managed_windows() {
    let _guard = setup();
    let app = TestApp::spawn_pinnable("AWWM.Test.Drop", spot(0), &["--count", "2"]);
    let (a, b) = (app.hwnd(0), app.hwnd(1));
    {
        let engine = TestEngine::start(settings());
        engine.handle().pin(a).expect("pin a");
        engine.handle().set_opacity(b, 40).expect("opacity b");
        assert!(is_topmost(a) && is_layered(b));
    }

    assert!(!is_topmost(a));
    assert!(!is_layered(b));
}
