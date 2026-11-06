//! The pixel grid's own thread.
//!
//! Creating and painting one monitor-sized overlay window per monitor takes tens of
//! milliseconds (measured: 60–160 ms for three monitors, depending on the grid size). Done on
//! the gesture thread, that would freeze the dragged window at the start of every gesture.
//! The gesture thread therefore only records the grid it wants (latest request wins) and
//! wakes this thread, which owns the [`GridOverlay`] windows (they are thread-affine) and
//! runs their message loop. A newer request — including "hide" when the drag ends — aborts a
//! grid that is still being built between two monitors.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PostThreadMessageW, TranslateMessage, WM_QUIT,
};

use awwm_core::MonitorInfo;

use crate::overlay::{GridOverlay, Rgb};
use crate::shared::{WM_APP_GRID, unpoison};
use crate::win::WindowHandle;

/// The grid to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridRequest {
    pub monitors: Vec<MonitorInfo>,
    /// Spacing in logical pixels.
    pub size: u32,
    pub color: Rgb,
    /// The window being dragged; the grid is placed directly below it.
    pub below: WindowHandle,
}

#[derive(Debug, Default)]
struct Shared {
    /// The wanted grid (`None` = no grid).
    wanted: Mutex<Option<GridRequest>>,
    /// Incremented on every change of `wanted`.
    generation: AtomicU64,
    thread_id: AtomicU32,
}

/// Handle to the grid thread; dropping it stops the thread (and removes any grid).
#[derive(Debug)]
pub struct GridThread {
    shared: Arc<Shared>,
    join: Option<JoinHandle<()>>,
}

impl GridThread {
    /// Starts the thread. `None` when it could not be started.
    #[must_use]
    pub fn spawn() -> Option<Self> {
        let shared = Arc::new(Shared::default());
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread_shared = Arc::clone(&shared);
        let join = std::thread::Builder::new()
            .name("awwm-grid".into())
            .spawn(move || run(&thread_shared, &ready_tx))
            .ok()?;
        let tid = ready_rx.recv().ok()?;
        shared.thread_id.store(tid, Ordering::Release);
        Some(Self {
            shared,
            join: Some(join),
        })
    }

    /// Shows `request` (replacing any grid that is shown or being built).
    pub fn show(&self, request: GridRequest) {
        self.set(Some(request));
    }

    /// Removes the grid as soon as possible.
    pub fn hide(&self) {
        self.set(None);
    }

    fn set(&self, wanted: Option<GridRequest>) {
        *unpoison(self.shared.wanted.lock()) = wanted;
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        let tid = self.shared.thread_id.load(Ordering::Acquire);
        // SAFETY: pointer-free wake-up message; failure only means the thread has exited.
        unsafe {
            let _ = PostThreadMessageW(tid, WM_APP_GRID, WPARAM(0), LPARAM(0));
        }
    }
}

impl Drop for GridThread {
    fn drop(&mut self) {
        // Cancels a grid that is still being built, so joining never waits for it.
        self.set(None);
        let tid = self.shared.thread_id.load(Ordering::Acquire);
        // SAFETY: pointer-free message to our own thread.
        unsafe {
            let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Brings the shown grid in line with the latest request. Loops until no newer request
/// arrived while a grid was being built.
fn sync(shared: &Shared, grid: &mut GridOverlay, shown: &mut Option<GridRequest>) {
    loop {
        let generation = shared.generation.load(Ordering::Acquire);
        let wanted = unpoison(shared.wanted.lock()).clone();
        if wanted == *shown {
            return;
        }
        grid.hide();
        *shown = None;
        if let Some(request) = &wanted {
            let complete = grid.show(
                &request.monitors,
                request.size,
                request.color,
                request.below,
                || shared.generation.load(Ordering::Acquire) != generation,
            );
            if !complete {
                // Superseded mid-way: start over with the newer request.
                grid.hide();
                continue;
            }
        }
        *shown = wanted;
        if shared.generation.load(Ordering::Acquire) == generation {
            return;
        }
    }
}

fn run(shared: &Shared, ready: &mpsc::Sender<u32>) {
    crate::win::make_thread_dpi_aware();
    let tid = crate::win::init_message_queue();
    let _ = ready.send(tid);
    let mut grid = GridOverlay::default();
    let mut shown: Option<GridRequest> = None;
    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid out buffer; the loop ends on WM_QUIT (0) or error (-1).
        let r = unsafe { GetMessageW(&raw mut msg, None, 0, 0) };
        if r.0 <= 0 {
            break;
        }
        if msg.hwnd.is_invalid() && msg.message == WM_APP_GRID {
            sync(shared, &mut grid, &mut shown);
            continue;
        }
        // SAFETY: standard dispatch (grid window painting).
        unsafe {
            let _ = TranslateMessage(&raw const msg);
            DispatchMessageW(&raw const msg);
        }
    }
    grid.hide();
}
