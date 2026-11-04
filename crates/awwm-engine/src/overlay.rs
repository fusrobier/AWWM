//! overlay_manager: click-through, non-activating overlay windows (pin frames, resize
//! outline, size label, snap guides).
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
    BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CombineRgn, CreateFontW, CreateRectRgn,
    CreateRoundRectRgn, CreateSolidBrush, DEFAULT_CHARSET, DT_CENTER, DT_SINGLELINE, DT_VCENTER,
    DeleteObject, DrawTextW, EndPaint, FF_DONTCARE, FW_SEMIBOLD, FillRect, GetDC,
    GetTextExtentPoint32W, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS, PAINTSTRUCT, RGN_DIFF, ReleaseDC,
    SelectObject, SetBkMode, SetTextColor, SetWindowRgn, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, HTTRANSPARENT,
    LWA_ALPHA, MA_NOACTIVATE, RegisterClassExW, SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
    SetLayeredWindowAttributes, ShowWindow, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NCHITTEST,
    WM_PAINT, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use awwm_core::geometry::logical_to_physical;
use awwm_core::snap::{Orientation, SnapGuide};
use awwm_core::{Point, Rect};

use crate::win::WindowHandle;
use crate::win::window::{self, ZOrder};

const CLASS_NAME: PCWSTR = w!("AWWM.Overlay");

/// An `(r, g, b)` colour.
pub type Rgb = (u8, u8, u8);

fn colorref((r, g, b): Rgb) -> COLORREF {
    COLORREF(u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16))
}

/// Per-window paint state, owned by the overlay's thread.
#[derive(Default)]
struct PaintState {
    background: COLORREF,
    foreground: COLORREF,
    text: Vec<u16>,
    font: Option<HFONT>,
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

    /// Changes the fill colour.
    pub fn set_color(&self, color: Rgb) {
        PAINT.with(|m| {
            if let Some(s) = m.borrow_mut().get_mut(&self.handle.raw()) {
                s.background = colorref(color);
            }
        });
        self.invalidate();
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
        // SAFETY: the region is handed to the system by set_region.
        let region = unsafe { CreateRoundRectRgn(0, 0, w + 1, h + 1, radius, radius) };
        self.window.set_region(region);
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

/// Text of the live size label, e.g. `1280 × 720`.
#[must_use]
pub fn size_label(width: i32, height: i32) -> String {
    format!("{width} \u{00d7} {height}")
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
}
