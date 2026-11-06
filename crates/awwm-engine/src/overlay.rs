//! overlay_manager: click-through, non-activating overlay windows (pin frames, resize
//! outline, size and position labels, snap and alignment guides, the pixel grid).
//!
//! Overlays are `WS_POPUP` windows with `WS_EX_LAYERED | WS_EX_TRANSPARENT |
//! WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST`: they never take focus, never
//! appear in Alt+Tab or the taskbar, and mouse input passes straight through them. Frames
//! are shaped with `SetWindowRgn` (outer rectangle minus inner rectangle) so only the
//! border pixels exist. Overlay windows are thread-affine (`!Send`): each is created,
//! updated and destroyed on the thread that runs its message loop.

use std::cell::RefCell;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::OnceLock;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CombineRgn, CreateCompatibleBitmap,
    CreateCompatibleDC, CreateFontW, CreatePatternBrush, CreateRectRgn, CreateRoundRectRgn,
    CreateSolidBrush, DEFAULT_CHARSET, DT_CENTER, DT_SINGLELINE, DT_VCENTER, DeleteDC,
    DeleteObject, DrawTextW, EndPaint, FF_DONTCARE, FW_SEMIBOLD, FillRect, GetDC,
    GetTextExtentPoint32W, HBRUSH, HDC, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS, PAINTSTRUCT, RGN_DIFF,
    ReleaseDC, SelectObject, SetBkMode, SetPixel, SetTextColor, SetWindowRgn, TRANSPARENT,
    UpdateWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, HTTRANSPARENT,
    LWA_ALPHA, LWA_COLORKEY, MA_NOACTIVATE, RegisterClassExW, SW_HIDE, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SetLayeredWindowAttributes, ShowWindow, WM_ERASEBKGND, WM_MOUSEACTIVATE,
    WM_NCHITTEST, WM_PAINT, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use awwm_core::geometry::logical_to_physical;
use awwm_core::pixel_grid;
use awwm_core::snap::{Orientation, SnapGuide};
use awwm_core::{MonitorInfo, Point, Rect};

use crate::win::WindowHandle;
use crate::win::window::{self, ZOrder};

const CLASS_NAME: PCWSTR = w!("AWWM.Overlay");

/// An `(r, g, b)` colour.
pub type Rgb = (u8, u8, u8);

fn colorref((r, g, b): Rgb) -> COLORREF {
    COLORREF(u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16))
}

/// Pixel-grid lines of one monitor in client coordinates.
struct GridPaint {
    /// `(x, major)` of vertical lines.
    vertical: Vec<(i32, bool)>,
    /// `(y, major)` of horizontal lines.
    horizontal: Vec<(i32, bool)>,
    line: COLORREF,
    /// Transparent colour key (never equal to `line`).
    key: COLORREF,
}

/// Per-window paint state, owned by the overlay's thread.
#[derive(Default)]
struct PaintState {
    background: COLORREF,
    foreground: COLORREF,
    text: Vec<u16>,
    font: Option<HFONT>,
    grid: Option<GridPaint>,
}

thread_local! {
    static PAINT: RefCell<HashMap<isize, PaintState>> = RefCell::new(HashMap::new());
}

fn ensure_class() -> bool {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    *REGISTERED.get_or_init(|| {
        // SAFETY: GetModuleHandleW(None) returns this executable's module; the class
        // structure is fully initialised and the class name is a static string.
        unsafe {
            let Ok(instance) = GetModuleHandleW(None) else {
                return false;
            };
            let class = WNDCLASSEXW {
                cbSize: u32::try_from(std::mem::size_of::<WNDCLASSEXW>()).unwrap_or(80),
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(overlay_proc),
                hInstance: instance.into(),
                lpszClassName: CLASS_NAME,
                ..WNDCLASSEXW::default()
            };
            RegisterClassExW(&raw const class) != 0
        }
    })
}

unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        // SAFETY: forwarding unhandled messages with the original arguments.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: BeginPaint/EndPaint are paired on the window's own thread; GDI objects
    // created here are deleted before returning; the paint state borrow is short-lived and
    // no code inside it can re-enter this window procedure.
    unsafe {
        let hdc = BeginPaint(hwnd, &raw mut ps);
        if hdc.is_invalid() {
            return;
        }
        PAINT.with(|map| {
            let map = map.borrow();
            let state = map.get(&(hwnd.0 as isize));
            if let Some(grid) = state.and_then(|s| s.grid.as_ref()) {
                let mut client = RECT::default();
                let _ =
                    windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &raw mut client);
                paint_grid(hdc, &client, grid);
                return;
            }
            let bg = state.map_or(COLORREF(0), |s| s.background);
            let brush = CreateSolidBrush(bg);
            let mut client = RECT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &raw mut client);
            FillRect(hdc, &raw const client, brush);
            let _ = DeleteObject(HGDIOBJ(brush.0));
            if let Some(state) = state.filter(|s| !s.text.is_empty()) {
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, state.foreground);
                let old = state.font.map(|f| SelectObject(hdc, HGDIOBJ(f.0)));
                let mut text = state.text.clone();
                DrawTextW(
                    hdc,
                    &mut text,
                    &raw mut client,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
                if let Some(old) = old {
                    SelectObject(hdc, old);
                }
            }
        });
        let _ = EndPaint(hwnd, &raw const ps);
    }
}

/// A 2x2 checkerboard brush (`line` where `x + y` is even, `key` elsewhere) that draws
/// 1 px lines dotted: every other pixel of the line is transparent.
///
/// # Safety
/// `hdc` must be a valid device context of the calling thread. The caller owns the
/// returned brush and bitmap and must delete both.
unsafe fn checker_brush(hdc: HDC, line: COLORREF, key: COLORREF) -> Option<(HBRUSH, HGDIOBJ)> {
    // SAFETY: (caller contract) valid DC; every object created here is either returned to
    // the caller or deleted before returning.
    unsafe {
        let bitmap = CreateCompatibleBitmap(hdc, 2, 2);
        if bitmap.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(hdc));
        if mem.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            return None;
        }
        let old = SelectObject(mem, HGDIOBJ(bitmap.0));
        SetPixel(mem, 0, 0, line);
        SetPixel(mem, 1, 1, line);
        SetPixel(mem, 1, 0, key);
        SetPixel(mem, 0, 1, key);
        SelectObject(mem, old);
        let _ = DeleteDC(mem);
        let brush = CreatePatternBrush(bitmap);
        if brush.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            return None;
        }
        Some((brush, HGDIOBJ(bitmap.0)))
    }
}

/// Paints the colour key over the whole client area, then the minor lines dotted and the
/// major lines solid on top (so majors stay continuous where lines cross).
fn paint_grid(hdc: HDC, client: &RECT, grid: &GridPaint) {
    // SAFETY: called from WM_PAINT with the window's paint DC; every brush and bitmap is
    // deleted before returning and none stays selected into the DC.
    unsafe {
        let key = CreateSolidBrush(grid.key);
        FillRect(hdc, client, key);
        let _ = DeleteObject(HGDIOBJ(key.0));
        let solid = CreateSolidBrush(grid.line);
        let dotted = checker_brush(hdc, grid.line, grid.key);
        for major in [false, true] {
            let brush = if major {
                solid
            } else {
                dotted.map_or(solid, |(b, _)| b)
            };
            for &(x, _) in grid.vertical.iter().filter(|(_, m)| *m == major) {
                let r = RECT {
                    left: x,
                    top: client.top,
                    right: x + 1,
                    bottom: client.bottom,
                };
                FillRect(hdc, &raw const r, brush);
            }
            for &(y, _) in grid.horizontal.iter().filter(|(_, m)| *m == major) {
                let r = RECT {
                    left: client.left,
                    top: y,
                    right: client.right,
                    bottom: y + 1,
                };
                FillRect(hdc, &raw const r, brush);
            }
        }
        let _ = DeleteObject(HGDIOBJ(solid.0));
        if let Some((brush, bitmap)) = dotted {
            let _ = DeleteObject(HGDIOBJ(brush.0));
            let _ = DeleteObject(bitmap);
        }
    }
}

/// One overlay window. Not `Send`: it must stay on the thread that created it.
#[derive(Debug)]
pub struct OverlayWindow {
    handle: WindowHandle,
    visible: bool,
    _thread_affine: PhantomData<*const ()>,
}

impl OverlayWindow {
    /// Creates a hidden overlay window with the given uniform alpha.
    #[must_use]
    pub fn create(alpha: u8, color: Rgb) -> Option<Self> {
        if !ensure_class() {
            return None;
        }
        // SAFETY: the class is registered; all pointers are static strings or null.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(None).ok()?;
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOPMOST,
                CLASS_NAME,
                w!("AWWM overlay"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .ok()?
        };
        // SAFETY: the window was just created on this thread.
        unsafe {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
        }
        let handle = WindowHandle::from_hwnd(hwnd);
        PAINT.with(|m| {
            m.borrow_mut().insert(
                handle.raw(),
                PaintState {
                    background: colorref(color),
                    ..PaintState::default()
                },
            );
        });
        Some(Self {
            handle,
            visible: false,
            _thread_affine: PhantomData,
        })
    }

    /// The window handle.
    #[must_use]
    pub const fn handle(&self) -> WindowHandle {
        self.handle
    }

    /// Changes the fill colour (repaints only when it actually changes: guides and outlines
    /// call this on every mouse move).
    pub fn set_color(&self, color: Rgb) {
        let changed = PAINT.with(|m| {
            m.borrow_mut().get_mut(&self.handle.raw()).is_some_and(|s| {
                let changed = s.background != colorref(color);
                s.background = colorref(color);
                changed
            })
        });
        if changed {
            self.invalidate();
        }
    }

    fn invalidate(&self) {
        // SAFETY: plain call on our own window.
        unsafe {
            let _ =
                windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.handle.hwnd()), None, true);
        }
    }

    /// Positions and shows the window.
    pub fn show_at(&mut self, rect: Rect, z: ZOrder) {
        let _ = window::set_window_pos(self.handle, z, rect, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        self.visible = true;
    }

    /// Hides the window.
    pub fn hide(&mut self) {
        if self.visible {
            // SAFETY: plain call on our own window.
            unsafe {
                let _ = ShowWindow(self.handle.hwnd(), SW_HIDE);
            }
            self.visible = false;
        }
    }

    /// Whether the window is currently shown.
    #[must_use]
    pub const fn is_visible(&self) -> bool {
        self.visible
    }

    /// Sets the uniform alpha and, optionally, a colour that is fully transparent.
    fn set_layered(&self, key: Option<COLORREF>, alpha: u8) {
        let flags = if key.is_some() {
            LWA_COLORKEY | LWA_ALPHA
        } else {
            LWA_ALPHA
        };
        // SAFETY: plain call on our own layered window.
        unsafe {
            let _ = SetLayeredWindowAttributes(
                self.handle.hwnd(),
                key.unwrap_or(COLORREF(0)),
                alpha,
                flags,
            );
        }
    }

    /// Paints pending invalid regions now (synchronous `WM_PAINT`).
    fn paint_now(&self) {
        // SAFETY: plain call on our own window, on its thread.
        unsafe {
            let _ = UpdateWindow(self.handle.hwnd());
        }
    }

    /// Applies a window region in window-relative coordinates. Ownership of the region
    /// passes to the system.
    fn set_region(&self, region: windows::Win32::Graphics::Gdi::HRGN) {
        // SAFETY: on success the system owns `region`; on failure we delete it.
        unsafe {
            if SetWindowRgn(self.handle.hwnd(), Some(region), true) == 0 {
                let _ = DeleteObject(HGDIOBJ(region.0));
            }
        }
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        PAINT.with(|m| {
            if let Some(state) = m.borrow_mut().remove(&self.handle.raw())
                && let Some(font) = state.font
            {
                // SAFETY: the font was created by us and is no longer selected anywhere.
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(font.0));
                }
            }
        });
        // SAFETY: the window was created on this thread (OverlayWindow is !Send).
        unsafe {
            let _ = DestroyWindow(self.handle.hwnd());
        }
    }
}

/// A rectangular border drawn around a target rectangle.
#[derive(Debug)]
pub struct FrameOverlay {
    window: OverlayWindow,
    shape: Option<(i32, i32, i32)>,
}

impl FrameOverlay {
    /// Creates a hidden frame overlay.
    #[must_use]
    pub fn create(color: Rgb) -> Option<Self> {
        Some(Self {
            window: OverlayWindow::create(255, color)?,
            shape: None,
        })
    }

    /// Shows a border of `thickness` physical pixels around `frame` (outside it).
    pub fn show(&mut self, frame: Rect, thickness: i32, z: ZOrder) {
        let t = thickness.max(1);
        let outer = frame.inflate(t, t, t, t);
        let shape = (outer.width(), outer.height(), t);
        if self.shape != Some(shape) {
            // SAFETY: regions are created here and handed to the system by set_region; the
            // temporary inner region is deleted after combining.
            unsafe {
                let region = CreateRectRgn(0, 0, outer.width(), outer.height());
                let inner = CreateRectRgn(t, t, outer.width() - t, outer.height() - t);
                CombineRgn(Some(region), Some(region), Some(inner), RGN_DIFF);
                let _ = DeleteObject(HGDIOBJ(inner.0));
                self.window.set_region(region);
            }
            self.shape = Some(shape);
        }
        self.window.show_at(outer, z);
    }

    /// Changes the border colour.
    pub fn set_color(&self, color: Rgb) {
        self.window.set_color(color);
    }

    /// Hides the frame.
    pub fn hide(&mut self) {
        self.window.hide();
    }

    /// Whether the frame is shown.
    #[must_use]
    pub const fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    /// The frame's window handle.
    #[must_use]
    pub const fn handle(&self) -> WindowHandle {
        self.window.handle()
    }
}

/// A small rounded label (e.g. "1280 × 720").
#[derive(Debug)]
pub struct LabelOverlay {
    window: OverlayWindow,
    font_dpi: u32,
    /// `(width, height, radius)` of the current window region.
    shape: Option<(i32, i32, i32)>,
}

impl LabelOverlay {
    /// Creates a hidden label with dark background and white text.
    #[must_use]
    pub fn create() -> Option<Self> {
        let window = OverlayWindow::create(235, (32, 32, 36))?;
        PAINT.with(|m| {
            if let Some(s) = m.borrow_mut().get_mut(&window.handle().raw()) {
                s.foreground = colorref((255, 255, 255));
            }
        });
        Some(Self {
            window,
            font_dpi: 0,
            shape: None,
        })
    }

    fn ensure_font(&mut self, dpi: u32) {
        if self.font_dpi == dpi {
            return;
        }
        let height = logical_to_physical(15, dpi);
        // SAFETY: creates a GDI font owned by this label (deleted on replacement or drop).
        let font = unsafe {
            CreateFontW(
                -height,
                0,
                0,
                0,
                FW_SEMIBOLD.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                FF_DONTCARE.0 as u32,
                w!("Segoe UI"),
            )
        };
        PAINT.with(|m| {
            if let Some(s) = m.borrow_mut().get_mut(&self.window.handle().raw())
                && let Some(old) = s.font.replace(font)
            {
                // SAFETY: the previous font is ours and not selected into any DC.
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(old.0));
                }
            }
        });
        self.font_dpi = dpi;
    }

    fn measure(&self, text: &[u16]) -> (i32, i32) {
        let hwnd = self.window.handle().hwnd();
        let font = PAINT.with(|m| {
            m.borrow()
                .get(&self.window.handle().raw())
                .and_then(|s| s.font)
        });
        let mut size = SIZE::default();
        // SAFETY: the DC is obtained and released on this thread; `size` is a valid out ptr.
        unsafe {
            let hdc = GetDC(Some(hwnd));
            let old = font.map(|f| SelectObject(hdc, HGDIOBJ(f.0)));
            let _ = GetTextExtentPoint32W(hdc, text, &raw mut size);
            if let Some(old) = old {
                SelectObject(hdc, old);
            }
            ReleaseDC(Some(hwnd), hdc);
        }
        (size.cx, size.cy)
    }

    /// Shows `text` centred on `center`, kept inside `bounds`.
    pub fn show(&mut self, text: &str, center: Point, bounds: Rect, dpi: u32) {
        self.ensure_font(dpi);
        let utf16: Vec<u16> = text.encode_utf16().collect();
        let (tw, th) = self.measure(&utf16);
        let pad_x = logical_to_physical(12, dpi);
        let pad_y = logical_to_physical(6, dpi);
        let (w, h) = (tw + 2 * pad_x, th + 2 * pad_y);
        let mut x = center.x - w / 2;
        let mut y = center.y - h / 2;
        if !bounds.is_empty() {
            x = x.clamp(bounds.left, (bounds.right - w).max(bounds.left));
            y = y.clamp(bounds.top, (bounds.bottom - h).max(bounds.top));
        }
        let changed = PAINT.with(|m| {
            m.borrow_mut()
                .get_mut(&self.window.handle().raw())
                .is_some_and(|s| {
                    let changed = s.text != utf16;
                    s.text = utf16;
                    changed
                })
        });
        let radius = logical_to_physical(8, dpi);
        if self.shape != Some((w, h, radius)) {
            // Only when the size changes: the label is updated on every mouse move.
            // SAFETY: the region is handed to the system by set_region.
            let region = unsafe { CreateRoundRectRgn(0, 0, w + 1, h + 1, radius, radius) };
            self.window.set_region(region);
            self.shape = Some((w, h, radius));
        }
        self.window
            .show_at(Rect::from_xywh(x, y, w, h), ZOrder::Topmost);
        if changed {
            self.window.invalidate();
        }
    }

    /// Hides the label.
    pub fn hide(&mut self) {
        self.window.hide();
    }
}

/// A pool of thin line windows rendering snap guides.
#[derive(Debug, Default)]
pub struct GuideOverlays {
    lines: Vec<OverlayWindow>,
}

impl GuideOverlays {
    /// Shows the given guides (hiding unused lines).
    pub fn show(&mut self, guides: &[SnapGuide], thickness: i32, color: Rgb) {
        let t = thickness.max(1);
        while self.lines.len() < guides.len().min(8) {
            match OverlayWindow::create(255, color) {
                Some(w) => self.lines.push(w),
                None => break,
            }
        }
        for (i, line) in self.lines.iter_mut().enumerate() {
            match guides.get(i) {
                Some(g) => {
                    line.set_color(color);
                    let rect = match g.orientation {
                        Orientation::Vertical => {
                            Rect::new(g.position - t / 2, g.start, g.position - t / 2 + t, g.end)
                        }
                        Orientation::Horizontal => {
                            Rect::new(g.start, g.position - t / 2, g.end, g.position - t / 2 + t)
                        }
                    };
                    line.show_at(rect, ZOrder::Topmost);
                }
                None => line.hide(),
            }
        }
    }

    /// Hides every guide line.
    pub fn hide(&mut self) {
        for line in &mut self.lines {
            line.hide();
        }
    }
}

/// Opacity of the pixel grid windows (about 38 %).
pub const GRID_ALPHA: u8 = 96;

/// The visible pixel grid: one colour-keyed, click-through overlay window per monitor,
/// placed directly below the window being moved so it never covers it. The windows exist
/// only while a gesture runs; [`GridOverlay::hide`] destroys them, so an idle AWWM keeps
/// no monitor-sized surfaces. Building it is slow, so it lives on its own thread
/// (`crate::grid_thread`), never on the gesture thread.
#[derive(Debug, Default)]
pub struct GridOverlay {
    windows: Vec<OverlayWindow>,
}

/// A colour key that differs from `line` (pure magenta, or pure green for a magenta line).
fn grid_key(line: Rgb) -> COLORREF {
    if line == (255, 0, 255) {
        colorref((0, 255, 0))
    } else {
        colorref((255, 0, 255))
    }
}

impl GridOverlay {
    /// Shows a grid of `size` logical pixels on every monitor, below `below` in Z order.
    /// Each window is painted while fully transparent and only then given its alpha, so a
    /// half-painted monitor-sized window never flashes. `cancelled` is checked before each
    /// monitor; returns `false` when it stopped early (the windows built so far stay).
    pub fn show(
        &mut self,
        monitors: &[MonitorInfo],
        size: u32,
        color: Rgb,
        below: WindowHandle,
        cancelled: impl Fn() -> bool,
    ) -> bool {
        self.hide();
        let key = grid_key(color);
        for m in monitors.iter().filter(|m| !m.bounds.is_empty()) {
            if cancelled() {
                return false;
            }
            let Some(mut window) = OverlayWindow::create(0, color) else {
                continue;
            };
            window.set_layered(Some(key), 0);
            let b = m.bounds;
            let axis = |origin: i32, extent: i32| -> Vec<(i32, bool)> {
                pixel_grid::lines(origin, extent, size, m.dpi)
                    .into_iter()
                    .map(|l| (l.position - origin, l.major))
                    .collect()
            };
            let paint = GridPaint {
                vertical: axis(b.left, b.width()),
                horizontal: axis(b.top, b.height()),
                line: colorref(color),
                key,
            };
            PAINT.with(|p| {
                if let Some(state) = p.borrow_mut().get_mut(&window.handle().raw()) {
                    state.grid = Some(paint);
                }
            });
            window.show_at(b, ZOrder::After(below));
            window.paint_now();
            window.set_layered(Some(key), GRID_ALPHA);
            self.windows.push(window);
        }
        true
    }

    /// Removes the grid immediately (destroys its windows).
    pub fn hide(&mut self) {
        self.windows.clear();
    }
}

/// Text of the live size label, e.g. `1280 × 720`.
#[must_use]
pub fn size_label(width: i32, height: i32) -> String {
    format!("{width} \u{00d7} {height}")
}

/// `n` with a typographic minus sign for negative values.
fn number(n: i32) -> String {
    if n < 0 {
        format!("\u{2212}{}", n.unsigned_abs())
    } else {
        n.to_string()
    }
}

/// `n` with an explicit sign (`+12`, `−12`, `0`).
fn signed(n: i32) -> String {
    if n > 0 { format!("+{n}") } else { number(n) }
}

/// Text of the position read-out while moving, e.g. `X 640 · Y 320 · Δ +120, −45`:
/// the position of the frame's top-left corner relative to its monitor and, while the
/// window is still on the monitor where the drag started, the distance moved (there is no
/// common logical unit across monitors with different scaling). Logical pixels.
#[must_use]
pub fn position_label(x: i32, y: i32, moved: Option<(i32, i32)>) -> String {
    let at = format!("X {} \u{00b7} Y {}", number(x), number(y));
    match moved {
        Some((dx, dy)) => format!("{at} \u{00b7} \u{0394} {}, {}", signed(dx), signed(dy)),
        None => at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorref_packs_bgr() {
        assert_eq!(colorref((0x11, 0x22, 0x33)).0, 0x0033_2211);
    }

    #[test]
    fn size_label_format() {
        assert_eq!(size_label(1280, 720), "1280 \u{00d7} 720");
    }

    #[test]
    fn position_label_format() {
        assert_eq!(
            position_label(640, 320, Some((120, -45))),
            "X 640 \u{00b7} Y 320 \u{00b7} \u{0394} +120, \u{2212}45"
        );
        assert_eq!(
            position_label(-20, 0, Some((0, 0))),
            "X \u{2212}20 \u{00b7} Y 0 \u{00b7} \u{0394} 0, 0"
        );
        assert_eq!(position_label(5, 7, None), "X 5 \u{00b7} Y 7");
        assert_eq!(
            position_label(i32::MIN, 1, Some((1, i32::MIN))),
            "X \u{2212}2147483648 \u{00b7} Y 1 \u{00b7} \u{0394} +1, \u{2212}2147483648"
        );
    }

    #[test]
    fn grid_key_never_equals_the_line_colour() {
        assert_ne!(grid_key((255, 0, 255)), colorref((255, 0, 255)));
        assert_ne!(grid_key((59, 130, 246)), colorref((59, 130, 246)));
    }
}
