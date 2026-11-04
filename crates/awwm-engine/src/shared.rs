//! State shared between the engine, gesture and input-hook threads.
//!
//! Everything here is either immutable, atomic, or behind a lock that is only held for a
//! few instructions (never across a Win32 call that could block), so the low-level hook
//! callbacks never wait noticeably.

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP};

use awwm_core::Point;
use awwm_core::gesture::{GestureBindings, GestureKind};
use awwm_core::settings::{GestureScope, Settings, SnapSettings, parse_hex_color};

use crate::overlay::Rgb;
use crate::types::EngineEvent;
use crate::win::WindowHandle;

/// Wakes the gesture thread: the gesture queue has messages.
pub const WM_APP_GESTURE: u32 = WM_APP + 1;
/// Wakes the engine thread: the command channel has messages.
pub const WM_APP_COMMAND: u32 = WM_APP + 2;
/// Wakes the engine thread: queued WinEvents need processing.
pub const WM_APP_EVENTS: u32 = WM_APP + 3;
/// Tells the input thread to install (`wparam = 1`) or remove (`0`) its hooks.
pub const WM_APP_HOOKS: u32 = WM_APP + 4;
/// Posted by the gesture thread after it moved a pinned window (`lparam` = HWND) so the
/// engine repositions its frame even if the location event is attributed to our process.
pub const WM_APP_FRAME_SYNC: u32 = WM_APP + 5;

/// `dwExtraInfo` marker on input injected by AWWM so the hooks can ignore it.
pub const INJECTED_MARKER: usize = 0x4157_574D; // "AWWM"

/// Callback receiving engine events.
pub type EventSink = Arc<dyn Fn(EngineEvent) + Send + Sync>;

/// Recovers the data of a poisoned lock: every value guarded here stays consistent even if
/// a panic interrupted a writer (they are replaced wholesale), so continuing is safe and
/// keeps the hooks alive.
pub fn unpoison<T>(r: Result<T, PoisonError<T>>) -> T {
    r.unwrap_or_else(PoisonError::into_inner)
}

/// Gesture-related configuration read by the hook and gesture threads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GestureRuntime {
    pub scope: GestureScope,
    pub excluded_apps: Vec<String>,
    pub ignore_fullscreen: bool,
    pub restore_maximized_on_move: bool,
    pub show_resize_overlay: bool,
    pub overlay_color: Rgb,
    pub snap: SnapSettings,
}

impl GestureRuntime {
    /// Derives the runtime configuration from settings.
    #[must_use]
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            scope: s.gestures.scope,
            excluded_apps: s.gestures.excluded_apps.clone(),
            ignore_fullscreen: s.gestures.ignore_fullscreen,
            restore_maximized_on_move: s.gestures.restore_maximized_on_move,
            show_resize_overlay: s.gestures.show_resize_overlay,
            overlay_color: parse_hex_color(&s.gestures.overlay_color).unwrap_or((59, 130, 246)),
            snap: s.snapping.clone(),
        }
    }
}

/// A message from the input hook to the gesture thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GestureMsg {
    Start {
        seq: u64,
        target: WindowHandle,
        kind: GestureKind,
        cursor: Point,
        snap: bool,
        /// Inject a neutral key so releasing Alt/Win does not open a menu / Start.
        suppress_menu: bool,
    },
    Move {
        cursor: Point,
    },
    Snap {
        active: bool,
    },
    End,
    Cancel,
    /// A gesture was refused because the target runs at a higher integrity level.
    Blocked {
        target: WindowHandle,
    },
}

/// Coalescing queue from the hook thread to the gesture thread.
#[derive(Debug, Default)]
pub struct GestureQueue {
    queue: Mutex<VecDeque<GestureMsg>>,
    wake_pending: AtomicBool,
    thread_id: AtomicU32,
}

impl GestureQueue {
    /// Registers the thread that consumes the queue.
    pub fn set_thread(&self, tid: u32) {
        self.thread_id.store(tid, Ordering::Release);
    }

    /// Queues a message; consecutive `Move`s are merged so a slow target never builds a
    /// backlog. Wakes the gesture thread if it is not already scheduled to run.
    pub fn push(&self, msg: GestureMsg) {
        {
            let mut q = unpoison(self.queue.lock());
            if let (GestureMsg::Move { .. }, Some(GestureMsg::Move { .. })) = (&msg, q.back()) {
                q.pop_back();
            }
            q.push_back(msg);
        }
        if !self.wake_pending.swap(true, Ordering::AcqRel) {
            let tid = self.thread_id.load(Ordering::Acquire);
            if tid != 0 {
                // SAFETY: posting a message without pointers to a thread id; failure only
                // means the thread already exited.
                unsafe {
                    let _ = PostThreadMessageW(tid, WM_APP_GESTURE, WPARAM(0), LPARAM(0));
                }
            }
        }
    }

    /// Takes all queued messages (called by the gesture thread).
    pub fn drain(&self) -> Vec<GestureMsg> {
        // Clear the flag first so a push racing with this drain schedules another wake.
        self.wake_pending.store(false, Ordering::Release);
        unpoison(self.queue.lock()).drain(..).collect()
    }
}

/// State shared by all engine threads.
pub struct SharedState {
    /// Our own process id (our windows are never managed).
    pub own_pid: u32,
    /// Input classification used by the hook.
    pub bindings: RwLock<GestureBindings>,
    /// Gesture behaviour.
    pub gesture: RwLock<GestureRuntime>,
    /// Handles of pinned windows (for the "pinned windows only" scope).
    pub pinned: RwLock<HashSet<WindowHandle>>,
    /// Hook → gesture thread messages.
    pub gestures: GestureQueue,
    /// Thread id of the engine thread (0 until it runs).
    pub engine_thread: AtomicU32,
    /// A `WM_APP_FRAME_SYNC` message is queued for the engine thread.
    pub frame_sync_pending: AtomicBool,
    /// Sequence number of the last gesture started by the hook.
    pub gesture_started: AtomicU64,
    /// Sequence number of the last gesture the gesture thread aborted on its own
    /// (target gone, desktop switch). Lets the hook stop treating it as active.
    pub gesture_aborted: AtomicU64,
    /// Event callback to the application layer.
    pub sink: EventSink,
}

impl std::fmt::Debug for SharedState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedState")
            .field("own_pid", &self.own_pid)
            .finish_non_exhaustive()
    }
}

impl SharedState {
    /// Creates the shared state from settings.
    #[must_use]
    pub fn new(own_pid: u32, settings: &Settings, sink: EventSink) -> Self {
        Self {
            own_pid,
            bindings: RwLock::new(settings.gesture_bindings()),
            gesture: RwLock::new(GestureRuntime::from_settings(settings)),
            pinned: RwLock::new(HashSet::new()),
            gestures: GestureQueue::default(),
            engine_thread: AtomicU32::new(0),
            frame_sync_pending: AtomicBool::new(false),
            gesture_started: AtomicU64::new(0),
            gesture_aborted: AtomicU64::new(0),
            sink,
        }
    }

    /// Replaces the gesture configuration.
    pub fn update_settings(&self, settings: &Settings) {
        *unpoison(self.bindings.write()) = settings.gesture_bindings();
        *unpoison(self.gesture.write()) = GestureRuntime::from_settings(settings);
    }

    /// Emits an event to the application layer.
    pub fn emit(&self, event: EngineEvent) {
        (self.sink)(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_are_coalesced_but_other_messages_kept_in_order() {
        let q = GestureQueue::default();
        q.push(GestureMsg::Move {
            cursor: Point::new(1, 1),
        });
        q.push(GestureMsg::Move {
            cursor: Point::new(2, 2),
        });
        q.push(GestureMsg::Snap { active: true });
        q.push(GestureMsg::Move {
            cursor: Point::new(3, 3),
        });
        q.push(GestureMsg::Move {
            cursor: Point::new(4, 4),
        });
        q.push(GestureMsg::End);
        assert_eq!(
            q.drain(),
            vec![
                GestureMsg::Move {
                    cursor: Point::new(2, 2)
                },
                GestureMsg::Snap { active: true },
                GestureMsg::Move {
                    cursor: Point::new(4, 4)
                },
                GestureMsg::End,
            ]
        );
        assert!(q.drain().is_empty());
    }

    #[test]
    fn runtime_from_settings_parses_colour() {
        let mut s = Settings::default();
        s.gestures.overlay_color = "#102030".into();
        assert_eq!(
            GestureRuntime::from_settings(&s).overlay_color,
            (0x10, 0x20, 0x30)
        );
    }
}
