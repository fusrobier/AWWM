//! `awwm-testapp`: controllable native Win32 windows for the engine's integration tests.
//!
//! The process is per-monitor-DPI-aware (v2), so every coordinate below is in physical
//! pixels. It creates the requested top-level windows, prints one line per window to
//! stdout (`HWND <decimal>`), then runs a message loop and executes line commands read
//! from stdin. Every command answers with exactly one line (`OK`, `HWND <decimal>`,
//! `STATS <activations> <focus>` or `ERR <message>`), so a test can wait for the answer
//! to know the command has completed. The process exits on `quit` and when stdin is
//! closed, so a killed test run never leaves windows behind.
//!
//! Options:
//!
//! | Option              | Meaning                                                          |
//! |---------------------|------------------------------------------------------------------|
//! | `--count N`         | Number of windows (default 1).                                   |
//! | `--title T`         | Window title (default `AWWM Test Window`).                       |
//! | `--class C`         | Window class name (default `AWWM.TestApp`).                      |
//! | `--pos X,Y`         | Window rect origin of window 0; window `i` is offset by `40*i`.  |
//! | `--size W,H`        | Window rect size (default 400,300).                              |
//! | `--min-size W,H`    | Minimum track size (`WM_GETMINMAXINFO`, window rect).            |
//! | `--max-size W,H`    | Maximum track size (`WM_GETMINMAXINFO`, window rect).            |
//! | `--layered-alpha A` | Start with `WS_EX_LAYERED` + `SetLayeredWindowAttributes(A)`.    |
//! | `--per-pixel`       | Per-pixel layered window (`UpdateLayeredWindow`, 32-bpp DIB).    |
//! | `--tool`            | `WS_EX_TOOLWINDOW`.                                              |
//! | `--topmost`         | Start with `WS_EX_TOPMOST`.                                      |
//! | `--dpi-resize`      | Apply the suggested rect of `WM_DPICHANGED` (like real apps).    |
//! | `--refuse-topmost-ms N` | For `N` ms after creation, veto every Z-order change         |
//! |                     | (`WM_WINDOWPOSCHANGING` adds `SWP_NOZORDER`): `SetWindowPos(HWND_TOPMOST)` |
//! |                     | then succeeds without effect, like an app that manages its own   |
//! |                     | Z-order (or Windows refusing a fresh window over a full-screen app). |
//! | `--trace`           | Log `WM_WINDOWPOSCHANGING/CHANGED` (insert-after, flags) to stderr. |
//!
//! Commands (`i` is the 0-based window index; `recreate` keeps the index):
//!
//! | Command                  | Effect                                                     |
//! |--------------------------|------------------------------------------------------------|
//! | `ping`                   | Answers `OK`.                                              |
//! | `title <i> <text>`       | `SetWindowTextW`.                                          |
//! | `recreate <i>`           | Destroys window `i`, creates an identical replacement at   |
//! |                          | the same place (same class/title), answers `HWND <n>`.     |
//! | `close <i>`              | `DestroyWindow`.                                           |
//! | `topmost <i> on\|off`     | `SetWindowPos(HWND_TOPMOST / HWND_NOTOPMOST)`.            |
//! | `pos <i> X Y W H`        | `SetWindowPos` (no activation, no Z-order change).         |
//! | `stats <i>`              | `STATS <activations> <focus>`: how often the window was    |
//! |                          | activated / received the keyboard focus.                   |
//! | `minimize <i>`           | `ShowWindow(SW_SHOWMINNOACTIVE)`.                          |
//! | `restore <i>`            | `ShowWindow(SW_SHOWNOACTIVATE)` (restores a minimized one).|
//! | `hide <i>` / `show <i>`  | `ShowWindow(SW_HIDE)` / `ShowWindow(SW_SHOWNOACTIVATE)`.    |
//! | `alpha <i> <a>`          | `SetLayeredWindowAttributes(LWA_ALPHA, a)` (adds           |
//! |                          | `WS_EX_LAYERED` if needed): the app resets its own alpha.  |
//! | `quit`                   | Destroys all windows and exits.                            |

#[cfg(windows)]
mod app {
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};
    use std::io::{BufRead as _, Write as _};
    use std::sync::atomic::{AtomicIsize, AtomicU16, Ordering};
    use std::sync::{Mutex, OnceLock, PoisonError};
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
        CreateCompatibleDC, CreateDIBSection, CreateSolidBrush, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDC, HGDIOBJ, ReleaseDC, SelectObject,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GWL_EXSTYLE, GetMessageW, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW,
        GetWindowTextW, HWND_MESSAGE, HWND_NOTOPMOST, HWND_TOPMOST, IDC_ARROW, InSendMessage,
        LWA_ALPHA, LoadCursorW, MINMAXINFO, MSG, PostMessageW, PostQuitMessage, RegisterClassExW,
        SW_HIDE, SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_NOZORDER, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, SetWindowTextW,
        ShowWindow, TranslateMessage, ULW_ALPHA, UpdateLayeredWindow, WA_INACTIVE, WINDOW_EX_STYLE,
        WINDOW_STYLE, WINDOWPOS, WM_ACTIVATE, WM_APP, WM_DPICHANGED, WM_GETMINMAXINFO, WM_NCCREATE,
        WM_SETFOCUS, WM_WINDOWPOSCHANGED, WM_WINDOWPOSCHANGING, WNDCLASSEXW, WS_EX_LAYERED,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW, WS_POPUP,
    };
    use windows::core::{PCWSTR, w};

    /// Posted to the control window when commands are queued.
    const WM_APP_COMMAND: u32 = WM_APP + 1;
    /// Offset between consecutive windows created by `--count`.
    const CASCADE: i32 = 40;
    /// Background colour of the windows (`0x00BBGGRR`).
    const BACKGROUND: u32 = 0x00A0_6020;

    /// Command-line options (shared by all windows of the process).
    #[derive(Clone, Debug)]
    struct Options {
        count: usize,
        title: String,
        class: String,
        pos: (i32, i32),
        size: (i32, i32),
        min_size: Option<(i32, i32)>,
        max_size: Option<(i32, i32)>,
        layered_alpha: Option<u8>,
        per_pixel: bool,
        tool: bool,
        topmost: bool,
        dpi_resize: bool,
        refuse_topmost: Duration,
        trace: bool,
    }

    impl Default for Options {
        fn default() -> Self {
            Self {
                count: 1,
                title: "AWWM Test Window".to_owned(),
                class: "AWWM.TestApp".to_owned(),
                pos: (200, 200),
                size: (400, 300),
                min_size: None,
                max_size: None,
                layered_alpha: None,
                per_pixel: false,
                tool: false,
                topmost: false,
                dpi_resize: false,
                refuse_topmost: Duration::ZERO,
                trace: false,
            }
        }
    }

    /// Activation/focus counters of one window.
    #[derive(Clone, Copy, Debug, Default)]
    struct Stats {
        activations: u32,
        focus: u32,
    }

    static OPTIONS: OnceLock<Options> = OnceLock::new();
    static START: OnceLock<Instant> = OnceLock::new();
    /// Atom of the registered test-window class.
    static CLASS_ATOM: AtomicU16 = AtomicU16::new(0);
    /// Raw handle of the message-only control window.
    static CONTROL: AtomicIsize = AtomicIsize::new(0);
    /// Lines read from stdin, waiting for the UI thread.
    static PENDING: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

    thread_local! {
        /// Raw window handles by index (0 = closed).
        static WINDOWS: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
        /// Activation statistics by raw window handle.
        static STATS: RefCell<HashMap<isize, Stats>> = RefCell::new(HashMap::new());
        /// Creation time by raw window handle (for `--refuse-topmost-ms`).
        static CREATED: RefCell<HashMap<isize, Instant>> = RefCell::new(HashMap::new());
    }

    /// Whether `window` still vetoes moves into the topmost band (`--refuse-topmost-ms`).
    fn refuses_topmost(window: HWND, o: &Options) -> bool {
        !o.refuse_topmost.is_zero()
            && CREATED.with(|c| {
                c.try_borrow()
                    .ok()
                    .and_then(|c| c.get(&(window.0 as isize)).copied())
                    .is_some_and(|t| t.elapsed() < o.refuse_topmost)
            })
    }

    fn pair(value: &str) -> Result<(i32, i32), String> {
        let (a, b) = value
            .split_once(',')
            .ok_or_else(|| format!("expected X,Y but got `{value}`"))?;
        let a = a.trim().parse().map_err(|e| format!("`{value}`: {e}"))?;
        let b = b.trim().parse().map_err(|e| format!("`{value}`: {e}"))?;
        Ok((a, b))
    }

    fn parse_options(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
        let mut o = Options::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
            match arg.as_str() {
                "--count" => o.count = value()?.parse().map_err(|e| format!("--count: {e}"))?,
                "--title" => o.title = value()?,
                "--class" => o.class = value()?,
                "--pos" => o.pos = pair(&value()?)?,
                "--size" => o.size = pair(&value()?)?,
                "--min-size" => o.min_size = Some(pair(&value()?)?),
                "--max-size" => o.max_size = Some(pair(&value()?)?),
                "--layered-alpha" => {
                    o.layered_alpha = Some(
                        value()?
                            .parse()
                            .map_err(|e| format!("--layered-alpha: {e}"))?,
                    );
                }
                "--per-pixel" => o.per_pixel = true,
                "--tool" => o.tool = true,
                "--topmost" => o.topmost = true,
                "--dpi-resize" => o.dpi_resize = true,
                "--trace" => o.trace = true,
                "--refuse-topmost-ms" => {
                    let ms = value()?
                        .parse()
                        .map_err(|e| format!("--refuse-topmost-ms: {e}"))?;
                    o.refuse_topmost = Duration::from_millis(ms);
                }
                other => return Err(format!("unknown option `{other}`")),
            }
        }
        if o.per_pixel && o.layered_alpha.is_some() {
            return Err("--per-pixel and --layered-alpha are mutually exclusive".into());
        }
        Ok(o)
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn hwnd(raw: isize) -> HWND {
        HWND(raw as *mut core::ffi::c_void)
    }

    /// Writes one answer line and flushes it (errors mean the parent is gone: ignored).
    fn emit(line: &str) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }

    fn record(raw: isize, update: impl FnOnce(&mut Stats)) {
        STATS.with(|s| {
            if let Ok(mut map) = s.try_borrow_mut() {
                update(map.entry(raw).or_default());
            }
        });
    }

    /// `--trace`: logs Z-order/position messages to stderr.
    fn trace_window_pos(window: HWND, msg: u32, lparam: LPARAM) {
        let pos = lparam.0 as *const WINDOWPOS;
        if pos.is_null() {
            return;
        }
        // SAFETY: for WM_WINDOWPOSCHANGING/CHANGED, lParam points to a WINDOWPOS valid for
        // the duration of the call; it is copied.
        let pos = unsafe { *pos };
        // SAFETY: plain queries on the current thread / our own window.
        let (from_other_thread, ex) = unsafe {
            (
                InSendMessage().as_bool(),
                GetWindowLongPtrW(window, GWL_EXSTYLE) as u32,
            )
        };
        eprintln!(
            "trace {:?} {} after={:?} flags={:#x} from_other_thread={} topmost={} t={:?}",
            window.0,
            if msg == WM_WINDOWPOSCHANGING {
                "CHANGING"
            } else {
                "CHANGED "
            },
            pos.hwndInsertAfter.0,
            pos.flags.0,
            from_other_thread,
            ex & WS_EX_TOPMOST.0 != 0,
            START.get_or_init(Instant::now).elapsed(),
        );
    }

    unsafe extern "system" fn window_proc(
        window: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let opts = OPTIONS.get();
        if opts.is_some_and(|o| o.trace)
            && (msg == WM_WINDOWPOSCHANGING || msg == WM_WINDOWPOSCHANGED)
        {
            trace_window_pos(window, msg, lparam);
        }
        match msg {
            WM_GETMINMAXINFO => {
                let info = lparam.0 as *mut MINMAXINFO;
                if let (Some(o), false) = (opts, info.is_null()) {
                    // SAFETY: for WM_GETMINMAXINFO, lParam points to a MINMAXINFO owned by
                    // the system that is valid and writable for the duration of the call.
                    let info = unsafe { &mut *info };
                    if let Some((w, h)) = o.min_size {
                        info.ptMinTrackSize = POINT { x: w, y: h };
                    }
                    if let Some((w, h)) = o.max_size {
                        info.ptMaxTrackSize = POINT { x: w, y: h };
                    }
                }
                return LRESULT(0);
            }
            WM_ACTIVATE => {
                if (wparam.0 & 0xFFFF) as u32 != WA_INACTIVE {
                    record(window.0 as isize, |s| s.activations += 1);
                }
            }
            WM_SETFOCUS => record(window.0 as isize, |s| s.focus += 1),
            WM_NCCREATE => {
                CREATED.with(|c| {
                    if let Ok(mut c) = c.try_borrow_mut() {
                        c.insert(window.0 as isize, Instant::now());
                    }
                });
            }
            WM_WINDOWPOSCHANGING if opts.is_some_and(|o| refuses_topmost(window, o)) => {
                let pos = lparam.0 as *mut WINDOWPOS;
                if !pos.is_null() {
                    // SAFETY: for WM_WINDOWPOSCHANGING, lParam points to a WINDOWPOS owned by
                    // the system that is valid and writable for the duration of the call.
                    let pos = unsafe { &mut *pos };
                    // The system has already resolved HWND_TOPMOST into a concrete
                    // insert-after window here, so every Z-order change is vetoed.
                    pos.flags |= SWP_NOZORDER;
                }
            }
            WM_DPICHANGED if opts.is_some_and(|o| o.dpi_resize) => {
                let suggested = lparam.0 as *const RECT;
                if !suggested.is_null() {
                    // SAFETY: for WM_DPICHANGED, lParam points to the suggested RECT, valid
                    // for the duration of the call; it is copied before use.
                    let r = unsafe { *suggested };
                    // SAFETY: plain value arguments on our own window.
                    unsafe {
                        let _ = SetWindowPos(
                            window,
                            None,
                            r.left,
                            r.top,
                            r.right - r.left,
                            r.bottom - r.top,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
                return LRESULT(0);
            }
            _ => {}
        }
        // SAFETY: forwarding the original arguments to the default procedure.
        unsafe { DefWindowProcW(window, msg, wparam, lparam) }
    }

    unsafe extern "system" fn control_proc(
        window: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_APP_COMMAND {
            loop {
                let next = PENDING
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .pop_front();
                let Some(line) = next else { break };
                let (answer, quit) = execute(line.trim());
                emit(&answer);
                if quit {
                    destroy_all();
                    // SAFETY: ends this thread's message loop.
                    unsafe { PostQuitMessage(0) };
                    break;
                }
            }
            return LRESULT(0);
        }
        // SAFETY: forwarding the original arguments to the default procedure.
        unsafe { DefWindowProcW(window, msg, wparam, lparam) }
    }

    fn register_classes(o: &Options) -> Result<(), String> {
        // SAFETY: GetModuleHandleW(None) returns this executable's module; the class
        // structures are fully initialised and their strings outlive the calls (the system
        // copies the class name into an atom).
        unsafe {
            let instance = GetModuleHandleW(None).map_err(|e| e.message())?;
            let name = wide(&o.class);
            let class = WNDCLASSEXW {
                cbSize: u32::try_from(std::mem::size_of::<WNDCLASSEXW>()).unwrap_or(80),
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: CreateSolidBrush(COLORREF(BACKGROUND)),
                lpszClassName: PCWSTR(name.as_ptr()),
                ..WNDCLASSEXW::default()
            };
            let atom = RegisterClassExW(&raw const class);
            if atom == 0 {
                return Err(format!("RegisterClassExW({}) failed", o.class));
            }
            CLASS_ATOM.store(atom, Ordering::Release);

            let control = WNDCLASSEXW {
                cbSize: u32::try_from(std::mem::size_of::<WNDCLASSEXW>()).unwrap_or(80),
                lpfnWndProc: Some(control_proc),
                hInstance: instance.into(),
                lpszClassName: w!("AWWM.TestApp.Control"),
                ..WNDCLASSEXW::default()
            };
            if RegisterClassExW(&raw const control) == 0 {
                return Err("RegisterClassExW(control) failed".into());
            }
            let window = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("AWWM.TestApp.Control"),
                w!("AWWM test control"),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance.into()),
                None,
            )
            .map_err(|e| e.message())?;
            CONTROL.store(window.0 as isize, Ordering::Release);
        }
        Ok(())
    }

    /// Paints a per-pixel-alpha bitmap into a layered window with `UpdateLayeredWindow`
    /// (after which `GetLayeredWindowAttributes` fails for it).
    fn update_per_pixel(
        window: HWND,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<(), String> {
        let (width, height) = (width.max(1), height.max(1));
        let header = BITMAPINFOHEADER {
            biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).unwrap_or(40),
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..BITMAPINFOHEADER::default()
        };
        let info = BITMAPINFO {
            bmiHeader: header,
            ..BITMAPINFO::default()
        };
        // Premultiplied BGRA: alpha 0xE0, colour scaled by the same factor.
        let alpha = 0xE0u32;
        let channel = |shift: u32| (((BACKGROUND >> shift) & 0xFF) * alpha / 255) << (16 - shift);
        let pixel = (alpha << 24) | channel(0) | channel(8) | channel(16);
        // SAFETY: the DIB section is created with `width * height` 32-bit pixels which are
        // written through the returned pointer only while the bitmap exists; every GDI
        // object is selected out and deleted before returning; the POINT/SIZE/BLENDFUNCTION
        // locals outlive the UpdateLayeredWindow call.
        unsafe {
            let screen = GetDC(None);
            let memory = CreateCompatibleDC(Some(screen));
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let result = match CreateDIBSection(
                Some(memory),
                &raw const info,
                DIB_RGB_COLORS,
                &raw mut bits,
                None,
                0,
            ) {
                Ok(bitmap) if !bits.is_null() => {
                    let len =
                        usize::try_from(width).unwrap_or(1) * usize::try_from(height).unwrap_or(1);
                    std::slice::from_raw_parts_mut(bits.cast::<u32>(), len).fill(pixel);
                    let old = SelectObject(memory, HGDIOBJ(bitmap.0));
                    let dst = POINT { x, y };
                    let src = POINT { x: 0, y: 0 };
                    let size = SIZE {
                        cx: width,
                        cy: height,
                    };
                    let blend = BLENDFUNCTION {
                        BlendOp: u8::try_from(AC_SRC_OVER).unwrap_or(0),
                        BlendFlags: 0,
                        SourceConstantAlpha: 255,
                        AlphaFormat: u8::try_from(AC_SRC_ALPHA).unwrap_or(1),
                    };
                    let r = UpdateLayeredWindow(
                        window,
                        Some(screen),
                        Some(&raw const dst),
                        Some(&raw const size),
                        Some(memory),
                        Some(&raw const src),
                        COLORREF(0),
                        Some(&raw const blend),
                        ULW_ALPHA,
                    )
                    .map_err(|e| format!("UpdateLayeredWindow: {}", e.message()));
                    SelectObject(memory, old);
                    let _ = DeleteObject(HGDIOBJ(bitmap.0));
                    r
                }
                Ok(bitmap) => {
                    let _ = DeleteObject(HGDIOBJ(bitmap.0));
                    Err("CreateDIBSection returned no bits".into())
                }
                Err(e) => Err(format!("CreateDIBSection: {}", e.message())),
            };
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
            result
        }
    }

    /// Creates and shows (without activating) one test window.
    fn create_window(o: &Options, title: &str, rect: (i32, i32, i32, i32)) -> Result<HWND, String> {
        let (x, y, width, height) = rect;
        let mut ex = WINDOW_EX_STYLE(0);
        if o.tool {
            ex |= WS_EX_TOOLWINDOW;
        }
        if o.topmost {
            ex |= WS_EX_TOPMOST;
        }
        if o.per_pixel || o.layered_alpha.is_some() {
            ex |= WS_EX_LAYERED;
        }
        let style = if o.per_pixel {
            WS_POPUP
        } else {
            WS_OVERLAPPEDWINDOW
        };
        let title = wide(title);
        let atom = CLASS_ATOM.load(Ordering::Acquire);
        // SAFETY: the class atom is registered (MAKEINTATOM form) and `title` outlives the
        // call; every other argument is a plain value.
        let window = unsafe {
            CreateWindowExW(
                ex,
                PCWSTR(usize::from(atom) as *const u16),
                PCWSTR(title.as_ptr()),
                style,
                x,
                y,
                width,
                height,
                None,
                None,
                GetModuleHandleW(None).ok().map(Into::into),
                None,
            )
        }
        .map_err(|e| format!("CreateWindowExW: {}", e.message()))?;
        let prepared = if o.per_pixel {
            update_per_pixel(window, x, y, width, height)
        } else if let Some(alpha) = o.layered_alpha {
            // SAFETY: plain value arguments on our own window.
            unsafe { SetLayeredWindowAttributes(window, COLORREF(0), alpha, LWA_ALPHA) }
                .map_err(|e| format!("SetLayeredWindowAttributes: {}", e.message()))
        } else {
            Ok(())
        };
        if let Err(e) = prepared {
            // SAFETY: destroys the window created above on this thread.
            unsafe {
                let _ = DestroyWindow(window);
            }
            return Err(e);
        }
        // SAFETY: shows our own window without activating it (tests must not steal focus).
        unsafe {
            let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
        }
        Ok(window)
    }

    fn window_at(index: usize) -> Result<HWND, String> {
        let raw = WINDOWS
            .with(|w| w.borrow().get(index).copied())
            .ok_or_else(|| format!("no window {index}"))?;
        if raw == 0 {
            return Err(format!("window {index} is closed"));
        }
        Ok(hwnd(raw))
    }

    fn set_slot(index: usize, raw: isize) {
        WINDOWS.with(|w| {
            let mut w = w.borrow_mut();
            if index >= w.len() {
                w.resize(index + 1, 0);
            }
            w[index] = raw;
        });
    }

    fn destroy_all() {
        let all: Vec<isize> = WINDOWS.with(|w| std::mem::take(&mut *w.borrow_mut()));
        for raw in all.into_iter().filter(|&r| r != 0) {
            // SAFETY: destroys a window created by this thread (fails harmlessly if gone).
            unsafe {
                let _ = DestroyWindow(hwnd(raw));
            }
        }
    }

    fn current_title(window: HWND) -> String {
        // SAFETY: read-only queries on our own window; the buffer is writable.
        unsafe {
            let len = usize::try_from(GetWindowTextLengthW(window)).unwrap_or(0);
            let mut buf = vec![0u16; len + 1];
            let copied = usize::try_from(GetWindowTextW(window, &mut buf)).unwrap_or(0);
            String::from_utf16_lossy(&buf[..copied.min(buf.len())])
        }
    }

    fn index_arg(parts: &mut std::str::SplitWhitespace<'_>) -> Result<usize, String> {
        parts
            .next()
            .ok_or("missing window index")?
            .parse()
            .map_err(|e| format!("bad window index: {e}"))
    }

    fn int_arg(parts: &mut std::str::SplitWhitespace<'_>) -> Result<i32, String> {
        parts
            .next()
            .ok_or("missing number")?
            .parse()
            .map_err(|e| format!("bad number: {e}"))
    }

    /// Executes one command; returns the answer line and whether to quit.
    fn execute(line: &str) -> (String, bool) {
        match run_command(line) {
            Ok(Some(answer)) => (answer, false),
            Ok(None) => ("OK".to_owned(), line == "quit"),
            Err(e) => (format!("ERR {e}"), false),
        }
    }

    fn run_command(line: &str) -> Result<Option<String>, String> {
        let mut parts = line.split_whitespace();
        let Some(cmd) = parts.next() else {
            return Err("empty command".into());
        };
        let opts = OPTIONS.get().ok_or("not initialised")?;
        match cmd {
            "ping" | "quit" => Ok(None),
            "title" => {
                let index = index_arg(&mut parts)?;
                let window = window_at(index)?;
                // The title is everything after the index (it may contain spaces).
                let text = line
                    .splitn(3, char::is_whitespace)
                    .nth(2)
                    .unwrap_or("")
                    .trim();
                let text = wide(text);
                // SAFETY: `text` is NUL-terminated and outlives the call.
                unsafe { SetWindowTextW(window, PCWSTR(text.as_ptr())) }
                    .map_err(|e| e.message())?;
                Ok(None)
            }
            "recreate" => {
                let index = index_arg(&mut parts)?;
                let old = window_at(index)?;
                let title = current_title(old);
                let mut r = RECT::default();
                // SAFETY: `r` is a valid out pointer.
                unsafe { GetWindowRect(old, &raw mut r) }.map_err(|e| e.message())?;
                set_slot(index, 0);
                // SAFETY: destroys our own window.
                unsafe { DestroyWindow(old) }.map_err(|e| e.message())?;
                let new = create_window(
                    opts,
                    &title,
                    (r.left, r.top, r.right - r.left, r.bottom - r.top),
                )?;
                set_slot(index, new.0 as isize);
                Ok(Some(format!("HWND {}", new.0 as usize)))
            }
            "close" => {
                let index = index_arg(&mut parts)?;
                let window = window_at(index)?;
                set_slot(index, 0);
                // SAFETY: destroys our own window.
                unsafe { DestroyWindow(window) }.map_err(|e| e.message())?;
                Ok(None)
            }
            "topmost" => {
                let index = index_arg(&mut parts)?;
                let window = window_at(index)?;
                let after = match parts.next() {
                    Some("on") => HWND_TOPMOST,
                    Some("off") => HWND_NOTOPMOST,
                    other => return Err(format!("expected on|off, got {other:?}")),
                };
                // SAFETY: plain value arguments on our own window.
                unsafe {
                    SetWindowPos(
                        window,
                        Some(after),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    )
                }
                .map_err(|e| e.message())?;
                Ok(None)
            }
            "pos" => {
                let index = index_arg(&mut parts)?;
                let window = window_at(index)?;
                let (x, y, width, height) = (
                    int_arg(&mut parts)?,
                    int_arg(&mut parts)?,
                    int_arg(&mut parts)?,
                    int_arg(&mut parts)?,
                );
                // SAFETY: plain value arguments on our own window.
                unsafe {
                    SetWindowPos(
                        window,
                        None,
                        x,
                        y,
                        width,
                        height,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    )
                }
                .map_err(|e| e.message())?;
                Ok(None)
            }
            "minimize" | "restore" | "hide" | "show" => {
                let index = index_arg(&mut parts)?;
                let window = window_at(index)?;
                let show = match cmd {
                    "minimize" => SW_SHOWMINNOACTIVE,
                    "hide" => SW_HIDE,
                    _ => SW_SHOWNOACTIVATE,
                };
                // SAFETY: plain value arguments on our own window; never activates it.
                unsafe {
                    let _ = ShowWindow(window, show);
                }
                Ok(None)
            }
            "alpha" => {
                let index = index_arg(&mut parts)?;
                let window = window_at(index)?;
                let alpha: u8 = parts
                    .next()
                    .ok_or("missing alpha")?
                    .parse()
                    .map_err(|e| format!("bad alpha: {e}"))?;
                // SAFETY: style query/update and layered attributes on our own window.
                unsafe {
                    let ex = GetWindowLongPtrW(window, GWL_EXSTYLE);
                    if ex as u32 & WS_EX_LAYERED.0 == 0 {
                        SetWindowLongPtrW(window, GWL_EXSTYLE, ex | WS_EX_LAYERED.0 as isize);
                    }
                    SetLayeredWindowAttributes(window, COLORREF(0), alpha, LWA_ALPHA)
                        .map_err(|e| e.message())?;
                }
                Ok(None)
            }
            "stats" => {
                let index = index_arg(&mut parts)?;
                let window = window_at(index)?;
                let s = STATS.with(|s| {
                    s.borrow()
                        .get(&(window.0 as isize))
                        .copied()
                        .unwrap_or_default()
                });
                Ok(Some(format!("STATS {} {}", s.activations, s.focus)))
            }
            other => Err(format!("unknown command `{other}`")),
        }
    }

    /// Reads stdin lines and forwards them to the UI thread; `quit` on end of input.
    fn stdin_thread() {
        let post = |line: String| {
            PENDING
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push_back(line);
            let control = CONTROL.load(Ordering::Acquire);
            // SAFETY: posts a pointer-free message to our own control window.
            unsafe { PostMessageW(Some(hwnd(control)), WM_APP_COMMAND, WPARAM(0), LPARAM(0)) }
                .is_ok()
        };
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            if !post(line) {
                std::process::exit(1);
            }
        }
        if !post("quit".to_owned()) {
            std::process::exit(0);
        }
    }

    pub fn main() {
        let opts = match parse_options(std::env::args().skip(1)) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("awwm-testapp: {e}");
                std::process::exit(2);
            }
        };
        // SAFETY: process-wide setting made before any window exists; failure (already
        // set by a manifest) is harmless.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let opts = OPTIONS.get_or_init(|| opts);
        if let Err(e) = register_classes(opts) {
            eprintln!("awwm-testapp: {e}");
            std::process::exit(1);
        }
        for i in 0..opts.count {
            let offset = CASCADE * i32::try_from(i).unwrap_or(0);
            let rect = (
                opts.pos.0 + offset,
                opts.pos.1 + offset,
                opts.size.0,
                opts.size.1,
            );
            match create_window(opts, &opts.title, rect) {
                Ok(window) => {
                    set_slot(i, window.0 as isize);
                    emit(&format!("HWND {}", window.0 as usize));
                }
                Err(e) => {
                    eprintln!("awwm-testapp: {e}");
                    destroy_all();
                    std::process::exit(1);
                }
            }
        }
        if std::thread::Builder::new()
            .name("stdin".into())
            .spawn(stdin_thread)
            .is_err()
        {
            destroy_all();
            std::process::exit(1);
        }
        let mut msg = MSG::default();
        loop {
            // SAFETY: `msg` is a valid out buffer; the loop ends on WM_QUIT (0) or error (-1).
            let r = unsafe { GetMessageW(&raw mut msg, None, 0, 0) };
            if r.0 <= 0 {
                break;
            }
            // SAFETY: standard dispatch of a message retrieved by GetMessageW.
            unsafe {
                let _ = TranslateMessage(&raw const msg);
                DispatchMessageW(&raw const msg);
            }
        }
        destroy_all();
    }
}

fn main() {
    #[cfg(windows)]
    app::main();
}
