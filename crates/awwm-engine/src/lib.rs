//! AWWM window-management engine (Windows only).
//!
//! [`Engine::start`] spawns three threads — the engine thread (managed windows, rules,
//! WinEvents, hotkeys, pin frames), the gesture thread (move/resize/snap and their
//! overlays) and the low-level input-hook thread — and returns an [`EngineHandle`] for
//! commands. The engine knows nothing about Tauri; it is driven directly by the
//! integration tests in `tests/` against windows created by the `awwm-testapp` binary.
//!
//! See `docs/ARCHITECTURE.md` for the design.
#![cfg(windows)]

pub mod enumerator;
pub mod error;
pub mod events;
pub mod gesture;
pub mod hotkeys;
pub mod input_hook;
pub mod manager;
pub mod overlay;
pub mod pinning;
pub mod session;
pub mod shared;
pub mod transparency;
pub mod types;
pub mod win;

mod engine_thread;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, HWND_MESSAGE, PostMessageW, PostThreadMessageW, RegisterWindowMessageW, WM_QUIT,
};
use windows::core::PCWSTR;

use awwm_core::settings::Settings;

use engine_thread::{Command, Peers, Reply};
pub use engine_thread::{ENGINE_WINDOW_CLASS, SHOW_MESSAGE_NAME};
pub use error::{EngineError, EngineResult};
pub use shared::{EventSink, GestureMsg};
pub use types::{
    EngineEvent, HotkeyAction, HotkeyStatus, ManagedSource, ManagedWindowInfo, Notice, NoticeLevel,
    WindowInfo,
};

/// Timeout for a command round-trip to the engine thread.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
/// Total time `Engine::stop` waits for its threads before detaching them.
const JOIN_TIMEOUT: Duration = Duration::from_secs(3);
/// Timeout for each thread to report readiness.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Options for [`Engine::start`].
#[derive(Debug, Clone)]
pub struct EngineOptions {
    /// Initial (sanitized) settings.
    pub settings: Settings,
    /// Where the managed-window session is persisted; `None` disables persistence.
    pub session_path: Option<PathBuf>,
}

/// Cloneable, thread-safe handle for sending commands to the engine thread.
#[derive(Clone, Debug)]
pub struct EngineHandle {
    tx: Sender<Command>,
    thread_id: u32,
}

impl EngineHandle {
    fn request<T>(&self, make: impl FnOnce(Reply<T>) -> Command) -> EngineResult<T> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.tx
            .send(make(reply_tx))
            .map_err(|_| EngineError::Internal("the engine has stopped".into()))?;
        // SAFETY: pointer-free wake-up message to the engine thread.
        unsafe { PostThreadMessageW(self.thread_id, shared::WM_APP_COMMAND, WPARAM(0), LPARAM(0)) }
            .map_err(|e| {
                EngineError::Internal(format!(
                    "could not reach the engine thread: {}",
                    e.message()
                ))
            })?;
        reply_rx
            .recv_timeout(COMMAND_TIMEOUT)
            .map_err(|_| EngineError::Internal("the engine did not respond in time".into()))?
    }

    /// Pins a window (always on top) with the default opacity, or pins an already managed
    /// transparent window.
    ///
    /// # Errors
    /// Window gone, elevated, hung, or AWWM's own window.
    pub fn pin(&self, hwnd: u64) -> EngineResult<ManagedWindowInfo> {
        self.request(|reply| Command::Pin { hwnd, reply })
    }

    /// Unpins a window and restores its original state (topmost and opacity).
    ///
    /// # Errors
    /// The window was not managed, or Windows rejected the restoration.
    pub fn unpin(&self, hwnd: u64) -> EngineResult<()> {
        self.request(|reply| Command::Unpin { hwnd, reply })
    }

    /// Sets a window's opacity (0–100 %). Unmanaged windows become managed (transparent
    /// only); a non-pinned window set back to 100 % is released.
    ///
    /// # Errors
    /// Window gone, elevated, or its rendering does not support uniform transparency.
    pub fn set_opacity(&self, hwnd: u64, opacity: u8) -> EngineResult<ManagedWindowInfo> {
        self.request(|reply| Command::SetOpacity {
            hwnd,
            opacity,
            reply,
        })
    }

    /// Releases every managed window.
    ///
    /// # Errors
    /// Some windows could not be fully restored.
    pub fn unpin_all(&self) -> EngineResult<()> {
        self.request(|reply| Command::UnpinAll { reply })
    }

    /// The managed windows.
    ///
    /// # Errors
    /// Engine not responding.
    pub fn managed(&self) -> EngineResult<Vec<ManagedWindowInfo>> {
        self.request(|reply| Command::ListManaged { reply })
    }

    /// Application windows that can be managed.
    ///
    /// # Errors
    /// Engine not responding.
    pub fn list_windows(&self) -> EngineResult<Vec<WindowInfo>> {
        self.request(|reply| Command::ListWindows { reply })
    }

    /// Applies new settings (they must already be sanitized). Returns hotkey statuses.
    ///
    /// # Errors
    /// Engine not responding.
    pub fn apply_settings(&self, settings: Settings) -> EngineResult<Vec<HotkeyStatus>> {
        self.request(|reply| Command::ApplySettings {
            settings: Box::new(settings),
            reply,
        })
    }

    /// Applies enabled rules to all current windows. Returns the number of windows changed.
    ///
    /// # Errors
    /// Management disabled or engine not responding.
    pub fn apply_rules_now(&self) -> EngineResult<u32> {
        self.request(|reply| Command::ApplyRulesNow { reply })
    }

    /// Brings a window to the foreground (restoring it if minimized).
    ///
    /// # Errors
    /// Window gone or Windows refused the activation.
    pub fn activate(&self, hwnd: u64) -> EngineResult<()> {
        self.request(|reply| Command::Activate { hwnd, reply })
    }

    /// Registration status of the global hotkeys.
    ///
    /// # Errors
    /// Engine not responding.
    pub fn hotkey_status(&self) -> EngineResult<Vec<HotkeyStatus>> {
        self.request(|reply| Command::HotkeyStatus { reply })
    }
}

/// The running engine. Dropping it shuts it down and restores managed windows.
pub struct Engine {
    handle: EngineHandle,
    ctx: Arc<shared::SharedState>,
    peers: Peers,
    threads: Vec<JoinHandle<()>>,
    stopped: bool,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("handle", &self.handle)
            .field("peers", &self.peers)
            .finish_non_exhaustive()
    }
}

fn spawn<F>(name: &str, f: F) -> EngineResult<JoinHandle<()>>
where
    F: FnOnce() + Send + 'static,
{
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(f)
        .map_err(|e| EngineError::Internal(format!("could not start the {name} thread: {e}")))
}

fn wait_ready(rx: &mpsc::Receiver<Result<u32, String>>, name: &str) -> EngineResult<u32> {
    match rx.recv_timeout(STARTUP_TIMEOUT) {
        Ok(Ok(tid)) => Ok(tid),
        Ok(Err(e)) => Err(EngineError::Internal(e)),
        Err(_) => Err(EngineError::Internal(format!(
            "the {name} thread did not start"
        ))),
    }
}

fn post_quit(tid: u32) {
    if tid != 0 {
        // SAFETY: pointer-free message to a thread we own.
        unsafe {
            let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

impl Engine {
    /// Starts the engine. Only one engine can run per process (the low-level hooks are
    /// process-global).
    ///
    /// # Errors
    /// Another engine is running, or a thread or hook could not be started.
    pub fn start(options: EngineOptions, sink: EventSink) -> EngineResult<Self> {
        if RUNNING.swap(true, Ordering::AcqRel) {
            return Err(EngineError::Internal(
                "the engine is already running".into(),
            ));
        }
        Self::start_inner(options, sink).inspect_err(|_| RUNNING.store(false, Ordering::Release))
    }

    fn start_inner(options: EngineOptions, sink: EventSink) -> EngineResult<Self> {
        let settings = options.settings.sanitized();
        let ctx = Arc::new(shared::SharedState::new(
            win::process::current_pid(),
            &settings,
            sink,
        ));
        let mut threads = Vec::with_capacity(3);

        let (tx, rx) = mpsc::channel();
        let c = Arc::clone(&ctx);
        threads.push(spawn("awwm-gesture", move || gesture::run(c, tx))?);
        let gesture_thread = wait_ready(&rx, "gesture")?;

        let (tx, rx) = mpsc::channel();
        let c = Arc::clone(&ctx);
        let hooks_enabled = settings.general.management_enabled && settings.gestures.enabled;
        match spawn("awwm-input", move || input_hook::run(c, hooks_enabled, tx)) {
            Ok(t) => threads.push(t),
            Err(e) => {
                post_quit(gesture_thread);
                return Err(e);
            }
        }
        let hook_thread = match wait_ready(&rx, "input hook") {
            Ok(t) => t,
            Err(e) => {
                post_quit(gesture_thread);
                return Err(e);
            }
        };

        let peers = Peers {
            gesture_thread,
            hook_thread,
        };
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (tx, rx) = mpsc::channel();
        let c = Arc::clone(&ctx);
        let session = session::SessionStore::new(options.session_path);
        match spawn("awwm-engine", move || {
            engine_thread::run(c, settings, session, peers, cmd_rx, tx)
        }) {
            Ok(t) => threads.push(t),
            Err(e) => {
                post_quit(gesture_thread);
                post_quit(hook_thread);
                return Err(e);
            }
        }
        let engine_thread = match wait_ready(&rx, "engine") {
            Ok(t) => t,
            Err(e) => {
                post_quit(gesture_thread);
                post_quit(hook_thread);
                return Err(e);
            }
        };
        ctx.engine_thread.store(engine_thread, Ordering::Release);
        tracing::info!(engine_thread, gesture_thread, hook_thread, "engine started");
        Ok(Self {
            handle: EngineHandle {
                tx: cmd_tx,
                thread_id: engine_thread,
            },
            ctx,
            peers,
            threads,
            stopped: false,
        })
    }

    /// A command handle (cheap to clone, usable from any thread).
    #[must_use]
    pub fn handle(&self) -> EngineHandle {
        self.handle.clone()
    }

    /// Feeds a gesture message directly to the gesture thread, bypassing the input hook.
    /// Used by integration tests and automation to drive moves/resizes deterministically.
    pub fn inject_gesture(&self, msg: GestureMsg) {
        self.ctx.gestures.push(msg);
    }

    /// Stops all threads. With `restore`, managed windows are returned to their original
    /// state; otherwise they are left as they are and recorded for the next start.
    pub fn shutdown(mut self, restore: bool) {
        self.stop(restore);
    }

    fn stop(&mut self, restore: bool) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        if let Err(e) = self
            .handle
            .request(|reply| Command::Shutdown { restore, reply })
        {
            tracing::error!(error = %e, "engine thread did not shut down cleanly");
        }
        post_quit(self.peers.gesture_thread);
        post_quit(self.peers.hook_thread);
        // Bounded join: a thread stuck in a synchronous call to a hung application must not
        // make AWWM's exit hang. Any window it could not restore is still in session.json.
        let deadline = std::time::Instant::now() + JOIN_TIMEOUT;
        let mut detached = 0;
        for t in self.threads.drain(..) {
            while !t.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            if t.is_finished() {
                let _ = t.join();
            } else {
                detached += 1;
            }
        }
        if detached == 0 {
            RUNNING.store(false, Ordering::Release);
            tracing::info!("engine stopped");
        } else {
            // Leave RUNNING set: a detached hook thread still owns the global hook context.
            tracing::error!(
                detached,
                "engine threads did not stop in time and were detached"
            );
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop(true);
    }
}

/// Asks an already running AWWM instance (in this session) to show its settings window.
/// Returns `true` when an instance was found and notified.
#[must_use]
pub fn signal_running_instance() -> bool {
    // SAFETY: looks up a message-only window by class and posts a pointer-free registered
    // message to it.
    unsafe {
        let Ok(hwnd) = FindWindowExW(
            Some(HWND_MESSAGE),
            None,
            ENGINE_WINDOW_CLASS,
            PCWSTR::null(),
        ) else {
            return false;
        };
        let msg = RegisterWindowMessageW(SHOW_MESSAGE_NAME);
        msg != 0 && PostMessageW(Some(hwnd), msg, WPARAM(0), LPARAM(0)).is_ok()
    }
}
