//! Shared helpers for the engine integration tests.
//!
//! * [`setup`] serialises the tests of a binary (the engine is process-global: one
//!   `Engine` at a time, global hooks) and makes the test thread per-monitor-DPI aware so
//!   Win32 geometry queries return physical pixels, like the engine's.
//! * [`TestApp`] runs `awwm-testapp` with piped stdin/stdout and kills it on drop.
//! * [`TestEngine`] starts the engine with an event sink that records every
//!   [`EngineEvent`], a private session file and test-friendly settings ([`settings`]):
//!   gestures (global mouse hook), hotkeys and rules off.
//! * Win32 queries ([`ex_style`], [`layered_attributes`], [`frame_rect`], …) and polling
//!   helpers ([`wait_until`], [`poll`]) to assert asynchronous effects.
//!
//! # Windows refusing topmost for fresh windows
//!
//! While a full-screen application in the foreground receives user input (e.g. a game),
//! Windows refuses to move *some freshly created windows* into the topmost band:
//! `SetWindowPos(HWND_TOPMOST)` succeeds, the window receives `WM_WINDOWPOSCHANGING` with
//! the full-screen window as insert-after, and nothing changes (no
//! `WM_WINDOWPOSCHANGED`). Such a window stays refused for many seconds, whoever asks.
//! The engine correctly reports this as an error, but it is not what a test is about. Tests
//! therefore separate the two cases with [`os_allows_topmost`] — a raw `SetWindowPos` from
//! the test thread that bypasses the engine — and repeat the scenario with a fresh window
//! ([`retry_if_topmost_refused`], [`TestApp::spawn_pinnable`]) only when Windows itself
//! refuses. Every engine assertion stays strict.
#![allow(
    dead_code,
    reason = "each test binary uses a different subset of the helpers"
)]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test helpers: a panic is the desired failure mode (clippy.toml only covers #[test] items)"
)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{COLORREF, HWND, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GWL_EXSTYLE, GWL_STYLE, GetForegroundWindow, GetLayeredWindowAttributes,
    GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, HWND_NOTOPMOST, HWND_TOPMOST,
    IsWindow, IsWindowVisible, LAYERED_WINDOW_ATTRIBUTES_FLAGS, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SetWindowPos, WS_CAPTION, WS_EX_LAYERED, WS_EX_TOPMOST,
};
use windows::core::{PCWSTR, w};

use awwm_core::gesture::GestureKind;
use awwm_core::rules::Rule;
use awwm_core::settings::Settings;
use awwm_core::{MonitorInfo, Point, Rect};
use awwm_engine::win::WindowHandle;
use awwm_engine::{
    Engine, EngineEvent, EngineHandle, EngineOptions, EventSink, GestureMsg, ManagedWindowInfo,
};

/// Default timeout for asynchronous effects (WinEvents, gestures, child output).
pub const TIMEOUT: Duration = Duration::from_secs(5);
/// How long to wait before asserting that something did *not* happen. Longer than the
/// engine's 2 s rule re-evaluation timer.
pub const QUIET_PERIOD: Duration = Duration::from_millis(2600);
/// Attempts of a scenario whose fresh window Windows refused to make topmost.
const TOPMOST_ATTEMPTS: usize = 8;

static SERIAL: Mutex<()> = Mutex::new(());

/// Serialises the tests of this binary and prepares the test thread. Keep the guard alive
/// for the whole test.
pub fn setup() -> MutexGuard<'static, ()> {
    let guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    awwm_engine::win::make_thread_dpi_aware();
    guard
}

// ------------------------------------------------------------------------- topmost refusal

/// A fresh test window that Windows itself (not the engine) refused to make topmost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopmostRefused(pub u64);

/// Whether Windows currently lets `h` enter the topmost band. Probes with a raw
/// `SetWindowPos` from the test thread (bypassing the engine) and restores the window.
pub fn os_allows_topmost(h: u64) -> bool {
    if is_topmost(h) {
        return true;
    }
    let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
    // SAFETY: plain value arguments; Win32 validates the handle.
    let _ = unsafe { SetWindowPos(hwnd(h), Some(HWND_TOPMOST), 0, 0, 0, 0, flags) };
    let allowed = is_topmost(h);
    if allowed {
        // SAFETY: as above.
        let _ = unsafe { SetWindowPos(hwnd(h), Some(HWND_NOTOPMOST), 0, 0, 0, 0, flags) };
    }
    allowed
}

/// Runs `scenario` until it completes. When it reports that Windows refused topmost for a
/// fresh window, the scenario is repeated from scratch (new engine, new windows).
pub fn retry_if_topmost_refused(mut scenario: impl FnMut() -> Result<(), TopmostRefused>) {
    for attempt in 1..=TOPMOST_ATTEMPTS {
        match scenario() {
            Ok(()) => return,
            Err(TopmostRefused(h)) => {
                eprintln!(
                    "note: Windows refused to make fresh test window {h} topmost (attempt {attempt}; usually a \
                     full-screen app in the foreground receiving input) - repeating with a new window"
                );
                // Refusals come in bursts: back off (250 ms, 500 ms, ... ≈ 7 s in total).
                std::thread::sleep(
                    Duration::from_millis(250) * u32::try_from(attempt).unwrap_or(1),
                );
            }
        }
    }
    panic!(
        "Windows refused topmost for every fresh test window; close full-screen applications and rerun"
    );
}

// ------------------------------------------------------------------------- test app

/// A running `awwm-testapp` process. Killed (with all its windows) on drop.
#[derive(Debug)]
pub struct TestApp {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    hwnds: Vec<u64>,
}

fn parse_hwnd(line: &str) -> u64 {
    line.strip_prefix("HWND ")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or_else(|| panic!("expected `HWND <n>` from awwm-testapp, got `{line}`"))
}

impl TestApp {
    /// Starts the test app with the given options and waits for its window handles.
    pub fn spawn(args: &[&str]) -> Self {
        let count = args
            .windows(2)
            .find(|w| w[0] == "--count")
            .map_or(1, |w| w[1].parse::<usize>().expect("numeric --count"));
        let mut child = Command::new(env!("CARGO_BIN_EXE_awwm-testapp"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn awwm-testapp");
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stdin = child.stdin.take();
        let mut app = Self {
            child,
            stdin,
            lines: rx,
            hwnds: Vec::new(),
        };
        for _ in 0..count {
            let line = app.read_line();
            app.hwnds.push(parse_hwnd(&line));
        }
        app
    }

    /// Like [`TestApp::spawn`] with an explicit window class and position (physical px).
    pub fn spawn_at(class: &str, pos: Point, extra: &[&str]) -> Self {
        let pos = format!("{},{}", pos.x, pos.y);
        let mut args = vec!["--class", class, "--pos", pos.as_str()];
        args.extend_from_slice(extra);
        Self::spawn(&args)
    }

    /// Like [`TestApp::spawn_at`], but only returns an app whose windows Windows lets enter
    /// the topmost band (see the module documentation).
    pub fn spawn_pinnable(class: &str, pos: Point, extra: &[&str]) -> Self {
        let mut app = None;
        retry_if_topmost_refused(|| {
            let candidate = Self::spawn_at(class, pos, extra);
            if let Some(&h) = candidate.hwnds.iter().find(|&&h| !os_allows_topmost(h)) {
                return Err(TopmostRefused(h));
            }
            app = Some(candidate);
            Ok(())
        });
        app.expect("pinnable app")
    }

    fn read_line(&self) -> String {
        self.lines
            .recv_timeout(TIMEOUT)
            .expect("awwm-testapp did not answer in time")
    }

    /// Window handle `i` (as printed by the app; updated by [`TestApp::recreate`]).
    pub fn hwnd(&self, i: usize) -> u64 {
        self.hwnds[i]
    }

    /// Process id of the app.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Sends a command and returns its answer line.
    pub fn command(&mut self, cmd: &str) -> String {
        let stdin = self.stdin.as_mut().expect("stdin open");
        writeln!(stdin, "{cmd}").expect("write command");
        stdin.flush().expect("flush command");
        self.read_line()
    }

    /// Sends a command that must answer `OK`.
    pub fn ok(&mut self, cmd: &str) {
        let answer = self.command(cmd);
        assert_eq!(answer, "OK", "awwm-testapp command `{cmd}` failed");
    }

    /// `SetWindowTextW` on window `i`.
    pub fn set_title(&mut self, i: usize, title: &str) {
        self.ok(&format!("title {i} {title}"));
    }

    /// Destroys window `i`.
    pub fn close(&mut self, i: usize) {
        self.ok(&format!("close {i}"));
    }

    /// Destroys window `i` and creates an identical replacement; returns its handle.
    pub fn recreate(&mut self, i: usize) -> u64 {
        let answer = self.command(&format!("recreate {i}"));
        let h = parse_hwnd(&answer);
        self.hwnds[i] = h;
        h
    }

    /// Makes window `i` topmost (or not) by itself.
    pub fn set_topmost(&mut self, i: usize, on: bool) {
        self.ok(&format!("topmost {i} {}", if on { "on" } else { "off" }));
    }

    /// `(activations, focus)` counters of window `i`.
    pub fn stats(&mut self, i: usize) -> (u32, u32) {
        let answer = self.command(&format!("stats {i}"));
        let mut parts = answer
            .strip_prefix("STATS ")
            .expect("STATS answer")
            .split_whitespace();
        let mut next = || {
            parts
                .next()
                .and_then(|v| v.parse().ok())
                .expect("numeric stats")
        };
        (next(), next())
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ------------------------------------------------------------------------- engine

/// A unique temporary directory, removed on drop.
#[derive(Debug)]
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("awwm-it-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// `session.json` inside the directory.
    pub fn session_file(&self) -> PathBuf {
        self.0.join("session.json")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Test settings: management on, gestures (global mouse hook) off, no hotkeys, no rules.
pub fn settings() -> Settings {
    let mut s = Settings::default();
    s.gestures.enabled = false;
    s.rules = Vec::new();
    s.hotkeys.toggle_pin = None;
    s.hotkeys.opacity_up = None;
    s.hotkeys.opacity_down = None;
    s
}

/// An enabled rule for `awwm-testapp.exe`.
pub fn testapp_rule(id: &str, pin: bool, opacity: u8) -> Rule {
    Rule {
        id: id.to_owned(),
        name: id.to_owned(),
        enabled: true,
        exe: "awwm-testapp.exe".to_owned(),
        title: None,
        pin,
        opacity,
    }
}

/// The engine plus a log of everything it emitted.
pub struct TestEngine {
    engine: Option<Engine>,
    handle: EngineHandle,
    events: Arc<Mutex<Vec<EngineEvent>>>,
    _dir: Option<TempDir>,
}

impl std::fmt::Debug for TestEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestEngine")
            .field("handle", &self.handle)
            .finish_non_exhaustive()
    }
}

impl TestEngine {
    /// Starts the engine with a private, temporary session file.
    pub fn start(settings: Settings) -> Self {
        let dir = TempDir::new("engine");
        let mut engine = Self::start_with_session(settings, Some(dir.session_file()));
        engine._dir = Some(dir);
        engine
    }

    /// Starts the engine with the given session file (`None` disables persistence).
    pub fn start_with_session(settings: Settings, session_path: Option<PathBuf>) -> Self {
        let events = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&events);
        let sink: EventSink = Arc::new(move |event| {
            log.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(event);
        });
        let engine = Engine::start(
            EngineOptions {
                settings: settings.sanitized(),
                session_path,
            },
            sink,
        )
        .expect("engine starts");
        let handle = engine.handle();
        Self {
            engine: Some(engine),
            handle,
            events,
            _dir: None,
        }
    }

    pub fn handle(&self) -> &EngineHandle {
        &self.handle
    }

    pub fn engine(&self) -> &Engine {
        self.engine.as_ref().expect("engine running")
    }

    /// Stops the engine (see [`Engine::shutdown`]).
    pub fn shutdown(mut self, restore: bool) {
        if let Some(engine) = self.engine.take() {
            engine.shutdown(restore);
        }
    }

    /// Applies (sanitized) settings.
    pub fn apply_settings(&self, settings: Settings) {
        self.handle
            .apply_settings(settings.sanitized())
            .expect("apply settings");
    }

    /// Position in the event log; pass it to [`TestEngine::wait_event_since`].
    pub fn mark(&self) -> usize {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Events emitted since `mark`.
    pub fn events_since(&self, mark: usize) -> Vec<EngineEvent> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(mark..)
            .map(<[_]>::to_vec)
            .unwrap_or_default()
    }

    /// Waits for the first event after `mark` matching `pred`.
    pub fn wait_event_since(
        &self,
        mark: usize,
        timeout: Duration,
        mut pred: impl FnMut(&EngineEvent) -> bool,
    ) -> Option<EngineEvent> {
        poll(timeout, || {
            self.events_since(mark).into_iter().find(|e| pred(e))
        })
    }

    /// Notices (user-facing messages) about `hwnd` emitted so far.
    pub fn notices_for(&self, hwnd: u64) -> Vec<String> {
        self.events_since(0)
            .into_iter()
            .filter_map(|e| match e {
                EngineEvent::Notice(n) if n.hwnd == Some(hwnd) => Some(n.message),
                _ => None,
            })
            .collect()
    }

    /// The managed-window entry of `hwnd`, if any.
    pub fn managed(&self, hwnd: u64) -> Option<ManagedWindowInfo> {
        self.handle
            .managed()
            .expect("managed list")
            .into_iter()
            .find(|m| m.hwnd == hwnd)
    }

    /// Waits until `hwnd` is managed and returns its entry.
    pub fn wait_managed(&self, hwnd: u64) -> ManagedWindowInfo {
        poll(TIMEOUT, || self.managed(hwnd)).unwrap_or_else(|| {
            panic!(
                "window {hwnd} did not become managed; engine notices: {:?}",
                self.notices_for(hwnd)
            )
        })
    }

    /// Like [`TestEngine::wait_managed`] for a fresh window the engine pins by itself (rule
    /// or reattachment): returns [`TopmostRefused`] when the engine reported a failure for
    /// the window *and* Windows refuses a raw topmost change for it too.
    pub fn wait_pinned_by_engine(&self, hwnd: u64) -> Result<ManagedWindowInfo, TopmostRefused> {
        let found = poll(TIMEOUT, || {
            if let Some(info) = self.managed(hwnd) {
                return Some(Some(info));
            }
            (!self.notices_for(hwnd).is_empty()).then_some(None)
        });
        if let Some(Some(info)) = found {
            // The engine keeps a refused automatic pin as "pending" (retried on the next
            // foreground change); for these scenarios that is the same Windows refusal.
            if info.pin_pending && !os_allows_topmost(hwnd) {
                return Err(TopmostRefused(hwnd));
            }
            return Ok(info);
        }
        if !os_allows_topmost(hwnd) {
            return Err(TopmostRefused(hwnd));
        }
        panic!(
            "window {hwnd} did not become managed; engine notices: {:?}",
            self.notices_for(hwnd)
        )
    }

    /// Waits until `hwnd` is no longer managed.
    pub fn wait_unmanaged(&self, hwnd: u64) {
        assert!(
            wait_until(TIMEOUT, || self.managed(hwnd).is_none()),
            "window {hwnd} is still managed"
        );
    }

    /// Injects a gesture message (bypassing the input hook).
    pub fn gesture(&self, msg: GestureMsg) {
        self.engine().inject_gesture(msg);
    }

    /// Starts a gesture on `hwnd` at `cursor` without snapping.
    pub fn start_gesture(&self, seq: u64, hwnd: u64, kind: GestureKind, cursor: Point) {
        let target = WindowHandle::from_u64(hwnd).expect("non-null handle");
        self.gesture(GestureMsg::Start {
            seq,
            target,
            kind,
            cursor,
            snap: false,
            suppress_menu: false,
        });
    }
}

impl Drop for TestEngine {
    fn drop(&mut self) {
        if let Some(engine) = self.engine.take() {
            engine.shutdown(true);
        }
    }
}

// ------------------------------------------------------------------------- polling

/// Polls `cond` every 20 ms until it holds or `timeout` elapses.
pub fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    poll(timeout, || cond().then_some(())).is_some()
}

/// Polls `f` every 20 ms until it returns `Some` or `timeout` elapses.
pub fn poll<T>(timeout: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Waits until the visible frame of `hwnd` equals `expected`; returns the last frame seen.
pub fn wait_frame(hwnd: u64, expected: Rect) -> Rect {
    let mut last = frame_rect(hwnd);
    wait_until(TIMEOUT, || {
        last = frame_rect(hwnd);
        last == expected
    });
    last
}

// ------------------------------------------------------------------------- Win32 queries

pub fn hwnd(h: u64) -> HWND {
    HWND(h as usize as *mut core::ffi::c_void)
}

/// `IsWindow`.
pub fn exists(h: u64) -> bool {
    // SAFETY: IsWindow accepts any handle value.
    unsafe { IsWindow(Some(hwnd(h))).as_bool() }
}

/// `GetWindowLongPtrW(GWL_EXSTYLE)`.
pub fn ex_style(h: u64) -> u32 {
    // SAFETY: read-only query on an arbitrary handle.
    unsafe { GetWindowLongPtrW(hwnd(h), GWL_EXSTYLE) as u32 }
}

pub fn is_topmost(h: u64) -> bool {
    ex_style(h) & WS_EX_TOPMOST.0 != 0
}

pub fn is_layered(h: u64) -> bool {
    ex_style(h) & WS_EX_LAYERED.0 != 0
}

/// `GetLayeredWindowAttributes` as `(key, alpha, flags)`; `None` when it fails.
pub fn layered_attributes(h: u64) -> Option<(u32, u8, u32)> {
    let mut key = COLORREF(0);
    let mut alpha = 0u8;
    let mut flags = LAYERED_WINDOW_ATTRIBUTES_FLAGS(0);
    // SAFETY: the out pointers reference valid locals for the duration of the call.
    unsafe {
        GetLayeredWindowAttributes(
            hwnd(h),
            Some(&raw mut key),
            Some(&raw mut alpha),
            Some(&raw mut flags),
        )
    }
    .ok()?;
    Some((key.0, alpha, flags.0))
}

/// Alpha byte of a layered window (`None` when not readable).
pub fn alpha(h: u64) -> Option<u8> {
    layered_attributes(h).map(|(_, a, _)| a)
}

/// `GetWindowRect` (physical pixels: the test thread is PMv2 aware).
pub fn window_rect(h: u64) -> Rect {
    let mut r = RECT::default();
    // SAFETY: `r` is a valid out pointer.
    unsafe { GetWindowRect(hwnd(h), &raw mut r) }.expect("GetWindowRect");
    Rect::new(r.left, r.top, r.right, r.bottom)
}

/// The visible frame (`DWMWA_EXTENDED_FRAME_BOUNDS`).
pub fn frame_rect(h: u64) -> Rect {
    let mut r = RECT::default();
    // SAFETY: `r` is a valid, correctly sized out buffer for this attribute.
    unsafe {
        DwmGetWindowAttribute(
            hwnd(h),
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut r).cast(),
            u32::try_from(std::mem::size_of::<RECT>()).expect("RECT size"),
        )
    }
    .expect("DwmGetWindowAttribute");
    Rect::new(r.left, r.top, r.right, r.bottom)
}

/// Window rects of the visible pin-frame / outline overlays (`AWWM.Overlay`) of this — the
/// engine's — process.
pub fn visible_overlays() -> Vec<Rect> {
    let mut found = Vec::new();
    let mut after = None;
    loop {
        // SAFETY: enumerates top-level windows by class name; `after` is a previous result.
        let Ok(h) = (unsafe { FindWindowExW(None, after, w!("AWWM.Overlay"), PCWSTR::null()) })
        else {
            break;
        };
        if h.is_invalid() {
            break;
        }
        // SAFETY: read-only query.
        if unsafe { IsWindowVisible(h) }.as_bool() {
            found.push(window_rect(h.0 as usize as u64));
        }
        after = Some(h);
    }
    found
}

/// The pin frame expected around a window's visible frame (2 logical px outside it).
pub fn pin_frame_around(frame: Rect) -> Rect {
    let dpi = awwm_engine::win::monitor::from_rect(frame).map_or(96, |m| m.dpi);
    let t = awwm_core::geometry::logical_to_physical(2, dpi);
    frame.inflate(t, t, t, t)
}

/// The current foreground window.
pub fn foreground() -> u64 {
    // SAFETY: no arguments.
    unsafe { GetForegroundWindow() }.0 as usize as u64
}

/// All monitors, primary first.
pub fn monitors() -> Vec<MonitorInfo> {
    awwm_engine::win::monitor::all()
}

/// The primary monitor.
pub fn primary() -> MonitorInfo {
    monitors()
        .into_iter()
        .find(|m| m.primary)
        .expect("primary monitor")
}

/// Bounds of the monitor covered by a full-screen foreground window of another process
/// (a game or video), if any.
fn fullscreen_foreground_monitor() -> Option<Rect> {
    let fg = foreground();
    if fg == 0 {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: `pid` is a valid out pointer.
    unsafe { GetWindowThreadProcessId(hwnd(fg), Some(&raw mut pid)) };
    if pid == std::process::id() {
        return None;
    }
    // SAFETY: read-only query.
    let style = unsafe { GetWindowLongPtrW(hwnd(fg), GWL_STYLE) } as u32;
    let r = window_rect(fg);
    monitors().into_iter().map(|m| m.bounds).find(|b| {
        style & WS_CAPTION.0 != WS_CAPTION.0
            && r.left <= b.left
            && r.top <= b.top
            && r.right >= b.right
            && r.bottom >= b.bottom
    })
}

/// A spot for a test window (physical px): inside the work area of the primary monitor, or
/// of another monitor when a full-screen application of the user covers the primary one
/// (so pinned test windows do not flash over it). `slot` offsets the spot so several apps
/// do not overlap exactly.
pub fn spot(slot: i32) -> Point {
    let covered = fullscreen_foreground_monitor();
    let all = monitors();
    let m = all
        .iter()
        .find(|m| Some(m.bounds) != covered)
        .unwrap_or(&all[0]);
    let w = m.work_area;
    Point::new(
        w.left + w.width() / 6 + 30 * slot,
        w.top + w.height() / 6 + 30 * slot,
    )
}

/// A spot inside the primary monitor's work area (for geometry tests that need it).
pub fn primary_spot(slot: i32) -> Point {
    let w = primary().work_area;
    Point::new(
        w.left + w.width() / 6 + 30 * slot,
        w.top + w.height() / 6 + 30 * slot,
    )
}
