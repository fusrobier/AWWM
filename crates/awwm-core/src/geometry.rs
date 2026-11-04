//! Integer geometry primitives in physical pixels plus DPI conversion helpers.
//!
//! [`Rect`] follows the Win32 `RECT` convention: `right` and `bottom` are exclusive, so
//! `width = right - left`.

use serde::{Deserialize, Serialize};

/// The DPI value that corresponds to 100 % scaling.
pub const BASE_DPI: u32 = 96;

/// A point in physical screen coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Component-wise difference `self - other`.
    #[must_use]
    pub const fn delta_from(self, other: Point) -> Point {
        Point::new(
            self.x.saturating_sub(other.x),
            self.y.saturating_sub(other.y),
        )
    }
}

/// A width/height pair in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

impl Size {
    #[must_use]
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }
}

/// An axis-aligned rectangle with exclusive right/bottom edges (Win32 `RECT` semantics).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    #[must_use]
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// Creates a rectangle from its origin and size.
    #[must_use]
    pub const fn from_xywh(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self::new(x, y, x.saturating_add(width), y.saturating_add(height))
    }

    #[must_use]
    pub const fn width(&self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    #[must_use]
    pub const fn height(&self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }

    #[must_use]
    pub const fn size(&self) -> Size {
        Size::new(self.width(), self.height())
    }

    #[must_use]
    pub const fn origin(&self) -> Point {
        Point::new(self.left, self.top)
    }

    /// Centre point (rounded towards the top-left).
    #[must_use]
    pub const fn center(&self) -> Point {
        Point::new(self.left + self.width() / 2, self.top + self.height() / 2)
    }

    /// `true` when the rectangle has no area.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width() <= 0 || self.height() <= 0
    }

    /// Area in square pixels (0 for empty rectangles).
    #[must_use]
    pub fn area(&self) -> i64 {
        if self.is_empty() {
            0
        } else {
            i64::from(self.width()) * i64::from(self.height())
        }
    }

    /// `true` when `p` lies inside the rectangle (right/bottom exclusive).
    #[must_use]
    pub const fn contains(&self, p: Point) -> bool {
        p.x >= self.left && p.x < self.right && p.y >= self.top && p.y < self.bottom
    }

    /// Returns the rectangle moved by `(dx, dy)`.
    #[must_use]
    pub const fn translate(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(
            self.left.saturating_add(dx),
            self.top.saturating_add(dy),
            self.right.saturating_add(dx),
            self.bottom.saturating_add(dy),
        )
    }

    /// Returns the rectangle moved so that its top-left corner is at `origin`.
    #[must_use]
    pub const fn with_origin(&self, origin: Point) -> Rect {
        Rect::from_xywh(origin.x, origin.y, self.width(), self.height())
    }

    /// Returns the rectangle grown by the given amounts on each side (negative shrinks).
    #[must_use]
    pub const fn inflate(&self, left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect::new(
            self.left.saturating_sub(left),
            self.top.saturating_sub(top),
            self.right.saturating_add(right),
            self.bottom.saturating_add(bottom),
        )
    }

    /// Intersection of two rectangles, or `None` when they do not overlap.
    #[must_use]
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let r = Rect::new(
            self.left.max(other.left),
            self.top.max(other.top),
            self.right.min(other.right),
            self.bottom.min(other.bottom),
        );
        (!r.is_empty()).then_some(r)
    }

    /// Smallest rectangle containing both rectangles.
    #[must_use]
    pub fn union(&self, other: &Rect) -> Rect {
        Rect::new(
            self.left.min(other.left),
            self.top.min(other.top),
            self.right.max(other.right),
            self.bottom.max(other.bottom),
        )
    }

    /// Area shared by both rectangles.
    #[must_use]
    pub fn overlap_area(&self, other: &Rect) -> i64 {
        self.intersection(other).map_or(0, |r| r.area())
    }

    /// `true` when the vertical extents overlap or are closer than `tolerance` pixels.
    #[must_use]
    pub const fn vertical_spans_touch(&self, other: &Rect, tolerance: i32) -> bool {
        self.top < other.bottom.saturating_add(tolerance)
            && other.top < self.bottom.saturating_add(tolerance)
    }

    /// `true` when the horizontal extents overlap or are closer than `tolerance` pixels.
    #[must_use]
    pub const fn horizontal_spans_touch(&self, other: &Rect, tolerance: i32) -> bool {
        self.left < other.right.saturating_add(tolerance)
            && other.left < self.right.saturating_add(tolerance)
    }
}

/// Description of one display.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorInfo {
    /// Full monitor rectangle in physical pixels.
    pub bounds: Rect,
    /// Work area (monitor minus taskbar and app bars) in physical pixels.
    pub work_area: Rect,
    /// Effective DPI of the monitor (96 = 100 %).
    pub dpi: u32,
    /// Whether this is the primary monitor.
    pub primary: bool,
}

impl MonitorInfo {
    /// Scale factor of this monitor (1.0 = 100 %).
    #[must_use]
    pub fn scale(&self) -> f64 {
        scale_for_dpi(self.dpi)
    }
}

/// Scale factor for a DPI value (`96 → 1.0`, `144 → 1.5`). Invalid DPI values map to 1.0.
#[must_use]
pub fn scale_for_dpi(dpi: u32) -> f64 {
    if dpi == 0 {
        1.0
    } else {
        f64::from(dpi) / f64::from(BASE_DPI)
    }
}

/// Converts a logical (96-DPI) length to physical pixels for the given DPI.
#[must_use]
pub fn logical_to_physical(logical: i32, dpi: u32) -> i32 {
    round_to_i32(f64::from(logical) * scale_for_dpi(dpi))
}

/// Converts a physical length to logical (96-DPI) pixels for the given DPI.
#[must_use]
pub fn physical_to_logical(physical: i32, dpi: u32) -> i32 {
    round_to_i32(f64::from(physical) / scale_for_dpi(dpi))
}

/// Rounds and saturates an `f64` into the `i32` range.
#[must_use]
pub fn round_to_i32(v: f64) -> i32 {
    if v.is_nan() {
        return 0;
    }
    let r = v.round();
    if r >= f64::from(i32::MAX) {
        i32::MAX
    } else if r <= f64::from(i32::MIN) {
        i32::MIN
    } else {
        // Exact: `r` is already rounded and within the i32 range.
        #[allow(clippy::cast_possible_truncation)]
        let out = r as i32;
        out
    }
}

/// Returns the index of the monitor that contains `p`, or the nearest one when `p` lies
/// outside every monitor. Returns `None` only for an empty slice.
#[must_use]
pub fn monitor_index_from_point(monitors: &[MonitorInfo], p: Point) -> Option<usize> {
    if let Some(i) = monitors.iter().position(|m| m.bounds.contains(p)) {
        return Some(i);
    }
    monitors
        .iter()
        .enumerate()
        .min_by_key(|(_, m)| distance_sq_to_rect(&m.bounds, p))
        .map(|(i, _)| i)
}

/// Returns the index of the monitor with the largest overlap with `r` (nearest to its
/// centre when it overlaps none). Returns `None` only for an empty slice.
#[must_use]
pub fn monitor_index_from_rect(monitors: &[MonitorInfo], r: &Rect) -> Option<usize> {
    let best = monitors
        .iter()
        .enumerate()
        .map(|(i, m)| (i, m.bounds.overlap_area(r)))
        .max_by_key(|&(_, a)| a)?;
    if best.1 > 0 {
        Some(best.0)
    } else {
        monitor_index_from_point(monitors, r.center())
    }
}

fn distance_sq_to_rect(r: &Rect, p: Point) -> i64 {
    let dx = if p.x < r.left {
        i64::from(r.left) - i64::from(p.x)
    } else if p.x >= r.right {
        i64::from(p.x) - i64::from(r.right) + 1
    } else {
        0
    };
    let dy = if p.y < r.top {
        i64::from(r.top) - i64::from(p.y)
    } else if p.y >= r.bottom {
        i64::from(p.y) - i64::from(r.bottom) + 1
    } else {
        0
    };
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(bounds: Rect, dpi: u32) -> MonitorInfo {
        MonitorInfo {
            bounds,
            work_area: bounds,
            dpi,
            primary: false,
        }
    }

    #[test]
    fn rect_basics() {
        let r = Rect::from_xywh(10, 20, 100, 50);
        assert_eq!(r.width(), 100);
        assert_eq!(r.height(), 50);
        assert_eq!(r.center(), Point::new(60, 45));
        assert!(r.contains(Point::new(10, 20)));
        assert!(!r.contains(Point::new(110, 20)));
        assert_eq!(r.translate(5, -5), Rect::from_xywh(15, 15, 100, 50));
        assert_eq!(r.area(), 5000);
        assert!(Rect::new(0, 0, 0, 10).is_empty());
    }

    #[test]
    fn intersection_and_union() {
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(50, 50, 150, 150);
        assert_eq!(a.intersection(&b), Some(Rect::new(50, 50, 100, 100)));
        assert_eq!(a.union(&b), Rect::new(0, 0, 150, 150));
        assert_eq!(a.intersection(&Rect::new(100, 0, 200, 100)), None);
        assert_eq!(a.overlap_area(&b), 2500);
    }

    #[test]
    fn spans_touch_with_tolerance() {
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(0, 110, 100, 200);
        assert!(!a.vertical_spans_touch(&b, 0));
        assert!(a.vertical_spans_touch(&b, 11));
        assert!(a.horizontal_spans_touch(&b, 0));
    }

    #[test]
    fn dpi_conversion_round_trips() {
        assert_eq!(logical_to_physical(100, 96), 100);
        assert_eq!(logical_to_physical(100, 144), 150);
        assert_eq!(physical_to_logical(150, 144), 100);
        assert_eq!(physical_to_logical(1500, 144), 1000);
        assert_eq!(logical_to_physical(10, 120), 13);
        assert_eq!(physical_to_logical(5, 0), 5);
        assert_eq!(round_to_i32(f64::NAN), 0);
        assert_eq!(round_to_i32(1e20), i32::MAX);
    }

    #[test]
    fn monitor_lookup_handles_gaps_and_negative_coordinates() {
        let monitors = [
            mon(Rect::new(-2560, 0, 0, 1440), 96),
            mon(Rect::new(0, 0, 1680, 1440), 96),
            mon(Rect::new(1680, 178, 3600, 1258), 144),
        ];
        assert_eq!(
            monitor_index_from_point(&monitors, Point::new(-10, 10)),
            Some(0)
        );
        assert_eq!(
            monitor_index_from_point(&monitors, Point::new(1700, 200)),
            Some(2)
        );
        // A point in the dead zone above monitor 2 maps to the nearest monitor.
        assert_eq!(
            monitor_index_from_point(&monitors, Point::new(3000, 10)),
            Some(2)
        );
        let r = Rect::new(1600, 300, 2000, 600);
        assert_eq!(monitor_index_from_rect(&monitors, &r), Some(2));
        assert_eq!(monitor_index_from_point(&[], Point::new(0, 0)), None);
    }
}
