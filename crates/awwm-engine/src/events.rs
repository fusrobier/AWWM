//! Out-of-context WinEvent hooks (`SetWinEventHook`) feeding the engine thread.
//!
//! The callback runs on the engine thread whenever it pumps messages — including while it
//! waits inside a cross-thread Win32 call — so it must never touch engine state directly.
//! It only appends a small record to a thread-local queue and posts one wake-up message;
//! the engine's main loop drains the queue between messages.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, EVENT_OBJECT_LOCATIONCHANGE, EVENT_SYSTEM_DESKTOPSWITCH, OBJID_WINDOW,
    PostThreadMessageW, WINEVENT_OUTOFCONTEXT,
};

use crate::shared::WM_APP_EVENTS;
use crate::win::WindowHandle;

/// A queued WinEvent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WinEventRecord {
    pub event: u32,
    pub hwnd: WindowHandle,
}

/// Upper bound of queued events; protects memory if the engine thread is blocked for a
/// long time while the system produces events.
const MAX_QUEUED: usize = 16_384;

thread_local! {
    static QUEUE: RefCell<VecDeque<WinEventRecord>> = const { RefCell::new(VecDeque::new()) };
    static WAKE_PENDING: Cell<bool> = const { Cell::new(false) };
    static THREAD_ID: Cell<u32> = const { Cell::new(0) };
}

/// Registers the calling thread as the consumer of WinEvents (call once on the engine
/// thread before installing hooks).
pub fn init_consumer(thread_id: u32) {
    THREAD_ID.with(|t| t.set(thread_id));
}

/// Takes all queued records.
pub fn drain() -> Vec<WinEventRecord> {
    WAKE_PENDING.with(|w| w.set(false));
    QUEUE.with(|q| {
        q.try_borrow_mut()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default()
    })
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _event_thread: u32,
    _time: u32,
) {
    let window_event =
        id_object == OBJID_WINDOW.0 && id_child == CHILDID_SELF as i32 && !hwnd.is_invalid();
    if !window_event && event != EVENT_SYSTEM_DESKTOPSWITCH {
        return;
    }
    let record = WinEventRecord {
        event,
        hwnd: WindowHandle::from_hwnd(hwnd),
    };
    QUEUE.with(|q| {
        if let Ok(mut q) = q.try_borrow_mut() {
            // Location changes arrive in bursts while a window moves: keep only the latest.
            if event == EVENT_OBJECT_LOCATIONCHANGE && q.back() == Some(&record) {
                return;
            }
            if q.len() < MAX_QUEUED {
                q.push_back(record);
            }
        }
    });
    if !WAKE_PENDING.with(|w| w.replace(true)) {
        let tid = THREAD_ID.with(Cell::get);
        if tid != 0 {
            // SAFETY: posts a pointer-free message to our own thread.
            unsafe {
                let _ = PostThreadMessageW(tid, WM_APP_EVENTS, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// An installed WinEvent hook, removed on drop. Must be dropped on the installing thread.
#[derive(Debug)]
pub struct WinEventHook {
    handle: HWINEVENTHOOK,
}

impl WinEventHook {
    /// Hooks events `min..=max` for all processes (`pid == 0`) or one process.
    #[must_use]
    pub fn install(min: u32, max: u32, pid: u32) -> Option<Self> {
        // SAFETY: the callback is a 'static function; out-of-context hooks need no module.
        let handle = unsafe {
            SetWinEventHook(
                min,
                max,
                None,
                Some(win_event_proc),
                pid,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        if handle.is_invalid() {
            tracing::warn!(min, max, pid, "SetWinEventHook failed");
            None
        } else {
            Some(Self { handle })
        }
    }
}

impl Drop for WinEventHook {
    fn drop(&mut self) {
        // SAFETY: the hook was installed by this thread and is removed exactly once.
        unsafe {
            let _ = UnhookWinEvent(self.handle);
        }
    }
}
