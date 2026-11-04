//! Pure resize and move math for mouse gestures.
//!
//! All rectangles are window frames in physical pixels with Win32 `RECT` semantics (right
//! and bottom exclusive). A resize gesture only moves the edges being dragged; the opposite
//! edges stay where they are, even when the size has to be clamped to the window's limits.

use crate::geometry::{Point, Rect, Size};

/// The edges of a frame that follow the cursor during a resize gesture.
///
/// The [`Default`] value selects no edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ResizeEdges {
    /// The left edge follows the cursor.
    pub left: bool,
    /// The top edge follows the cursor.
    pub top: bool,
    /// The right edge follows the cursor.
    pub right: bool,
    /// The bottom edge follows the cursor.
    pub bottom: bool,
}

impl ResizeEdges {
    /// The bottom-right corner: the right and bottom edges move, the top-left corner is fixed.
    pub const BOTTOM_RIGHT: ResizeEdges = ResizeEdges {
        left: false,
        top: false,
        right: true,
        bottom: true,
    };

    /// Chooses the edges to drag from the cursor position at the start of a resize gesture.
    ///
    /// The frame is divided into 3x3 zones by thirds of its width and height. A corner zone
    /// selects its two adjacent edges, a side zone selects that single edge and the centre
    /// zone selects [`ResizeEdges::BOTTOM_RIGHT`]. A cursor outside the frame is clamped into
    /// it first (right/bottom are exclusive, so `x >= frame.right` falls into the last
    /// column). An axis without extent counts as its middle third.
    #[must_use]
    pub fn from_cursor(frame: Rect, cursor: Point) -> ResizeEdges {
        let column = third(frame.left, frame.right, cursor.x);
        let row = third(frame.top, frame.bottom, cursor.y);
        let edges = ResizeEdges {
            left: column == 0,
            top: row == 0,
            right: column == 2,
            bottom: row == 2,
        };
        if edges.is_empty() {
            Self::BOTTOM_RIGHT
        } else {
            edges
        }
    }

    /// `true` when no edge is selected.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        !(self.left || self.top || self.right || self.bottom)
    }

    /// `true` when the left or the right edge is selected (the width can change).
    #[must_use]
    pub const fn horizontal(&self) -> bool {
        self.left || self.right
    }

    /// `true` when the top or the bottom edge is selected (the height can change).
    #[must_use]
    pub const fn vertical(&self) -> bool {
        self.top || self.bottom
    }
}

/// Index (0, 1 or 2) of the third of `[lo, hi)` that contains `p` after clamping `p` into the
/// span. Returns 1 (the middle) for an empty span.
fn third(lo: i32, hi: i32, p: i32) -> i64 {
    let len = i64::from(hi) - i64::from(lo);
    if len <= 0 {
        return 1;
    }
    let offset = (i64::from(p) - i64::from(lo)).clamp(0, len - 1);
    offset * 3 / len
}

/// Minimum and optional maximum frame size in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeConstraints {
    /// Smallest allowed size. Values below 1 are treated as 1, so a frame never collapses
    /// and its edges never cross.
    pub min: Size,
    /// Largest allowed size, if any. On an axis where it is smaller than `min`, `min` wins.
    pub max: Option<Size>,
}

impl Default for SizeConstraints {
    /// A 1x1 minimum and no maximum.
    fn default() -> Self {
        Self {
            min: Size::new(1, 1),
            max: None,
        }
    }
}

impl SizeConstraints {
    /// Effective inclusive `(min, max)` width range: `min >= 1`, `max >= min`, and
    /// `max == i32::MAX` when there is no maximum.
    #[must_use]
    pub fn width_range(&self) -> (i32, i32) {
        effective_range(self.min.width, self.max.map(|m| m.width))
    }

    /// Effective inclusive `(min, max)` height range: `min >= 1`, `max >= min`, and
    /// `max == i32::MAX` when there is no maximum.
    #[must_use]
    pub fn height_range(&self) -> (i32, i32) {
        effective_range(self.min.height, self.max.map(|m| m.height))
    }

    /// Clamps `size` to `[min, max]` on each axis. The minimum is always at least 1x1; if the
    /// maximum is smaller than the minimum on an axis, the minimum wins.
    #[must_use]
    pub fn clamp(&self, size: Size) -> Size {
        let (min_w, max_w) = self.width_range();
        let (min_h, max_h) = self.height_range();
        Size::new(
            size.width.clamp(min_w, max_w),
            size.height.clamp(min_h, max_h),
        )
    }

    /// `true` when `size` already lies within the constraints (clamping would not change it).
    #[must_use]
    pub fn allows(&self, size: Size) -> bool {
        self.clamp(size) == size
    }
}

fn effective_range(min: i32, max: Option<i32>) -> (i32, i32) {
    let lo = min.max(1);
    (lo, max.map_or(i32::MAX, |m| m.max(lo)))
}

/// Resizes `start` by moving only its active edges by `delta` and clamping the size.
///
/// The left edge moves to `left + dx`, the right edge to `right + dx`, the top edge to
/// `top + dy` and the bottom edge to `bottom + dy`. Inactive edges never move. On an axis with
/// an active edge the length is clamped to `constraints`, keeping the edge opposite to the
/// dragged one fixed (dragging the left edge keeps `right` and sets `left = right - width`).
/// When both edges of an axis are active the axis is translated and, if clamping is needed,
/// the (moved) left/top edge is kept. An axis without an active edge is left untouched, even
/// if its length violates the constraints. Because the minimum length is at least 1, edges can
/// never cross, whatever the delta.
#[must_use]
pub fn resize_frame(
    start: Rect,
    edges: ResizeEdges,
    delta: Point,
    constraints: &SizeConstraints,
) -> Rect {
    let (left, right) = resize_axis(
        start.left,
        start.right,
        edges.left,
        edges.right,
        delta.x,
        constraints.width_range(),
    );
    let (top, bottom) = resize_axis(
        start.top,
        start.bottom,
        edges.top,
        edges.bottom,
        delta.y,
        constraints.height_range(),
    );
    Rect::new(left, top, right, bottom)
}

/// Moves the active ends of the span `[lo, hi)` by `d` and clamps its length to `range`,
/// keeping the end opposite to the dragged one fixed.
fn resize_axis(
    lo: i32,
    hi: i32,
    move_lo: bool,
    move_hi: bool,
    d: i32,
    range: (i32, i32),
) -> (i32, i32) {
    let (min, max) = range;
    let clamped_len = |a: i32, b: i32| -> i32 {
        let len = (i64::from(b) - i64::from(a)).clamp(i64::from(min), i64::from(max));
        i32::try_from(len).unwrap_or(max)
    };
    match (move_lo, move_hi) {
        (false, false) => (lo, hi),
        (true, false) => {
            let len = clamped_len(lo.saturating_add(d), hi);
            (hi.saturating_sub(len), hi)
        }
        (false, true) => {
            let len = clamped_len(lo, hi.saturating_add(d));
            (lo, lo.saturating_add(len))
        }
        (true, true) => {
            let new_lo = lo.saturating_add(d);
            let len = clamped_len(new_lo, hi.saturating_add(d));
            (new_lo, new_lo.saturating_add(len))
        }
    }
}

/// Returns a rectangle of exactly `size` that keeps the fixed (non-dragged) edges of `frame`.
///
/// Horizontally, when only the left edge is dragged (`edges.left && !edges.right`) the right
/// edge stays and `left = right - width`; otherwise the left edge stays and
/// `right = left + width`. The same rule applies vertically to top/bottom. Used to re-anchor a
/// frame after the target application enforced a different size than requested.
#[must_use]
pub fn anchor_size(frame: Rect, edges: ResizeEdges, size: Size) -> Rect {
    let (left, right) = if edges.left && !edges.right {
        (frame.right.saturating_sub(size.width), frame.right)
    } else {
        (frame.left, frame.left.saturating_add(size.width))
    };
    let (top, bottom) = if edges.top && !edges.bottom {
        (frame.bottom.saturating_sub(size.height), frame.bottom)
    } else {
        (frame.top, frame.top.saturating_add(size.height))
    };
    Rect::new(left, top, right, bottom)
}

/// Translates `start` by the cursor movement `cursor - start_cursor`, preserving its size.
#[must_use]
pub fn move_frame(start: Rect, start_cursor: Point, cursor: Point) -> Rect {
    let d = cursor.delta_from(start_cursor);
    start.translate(d.x, d.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: ResizeEdges = ResizeEdges {
        left: false,
        top: false,
        right: false,
        bottom: false,
    };
    const LEFT: ResizeEdges = ResizeEdges { left: true, ..NONE };
    const TOP: ResizeEdges = ResizeEdges { top: true, ..NONE };
    const RIGHT: ResizeEdges = ResizeEdges {
        right: true,
        ..NONE
    };
    const BOTTOM: ResizeEdges = ResizeEdges {
        bottom: true,
        ..NONE
    };
    const TOP_LEFT: ResizeEdges = ResizeEdges {
        left: true,
        top: true,
        ..NONE
    };
    const TOP_RIGHT: ResizeEdges = ResizeEdges {
        right: true,
        top: true,
        ..NONE
    };
    const BOTTOM_LEFT: ResizeEdges = ResizeEdges {
        left: true,
        bottom: true,
        ..NONE
    };
    const BOTTOM_RIGHT: ResizeEdges = ResizeEdges::BOTTOM_RIGHT;

    /// 300x300 frame: columns x 100..=199 / 200..=299 / 300..=399, rows y 200..=299 /
    /// 300..=399 / 400..=499.
    const FRAME: Rect = Rect::new(100, 200, 400, 500);

    fn zone(x: i32, y: i32) -> ResizeEdges {
        ResizeEdges::from_cursor(FRAME, Point::new(x, y))
    }

    /// Tiny deterministic LCG for property-style tests.
    struct Lcg(u64);

    impl Lcg {
        fn next_u32(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            u32::try_from(self.0 >> 32).unwrap_or(0)
        }

        /// Value in `lo..hi` (`hi > lo`).
        fn range(&mut self, lo: i32, hi: i32) -> i32 {
            let span = i64::from(hi) - i64::from(lo);
            i32::try_from(i64::from(lo) + i64::from(self.next_u32()) % span).unwrap_or(lo)
        }

        fn chance(&mut self, percent: u32) -> bool {
            self.next_u32() % 100 < percent
        }
    }

    #[test]
    fn from_cursor_selects_corner_zones() {
        assert_eq!(zone(150, 250), TOP_LEFT);
        assert_eq!(zone(350, 250), TOP_RIGHT);
        assert_eq!(zone(150, 450), BOTTOM_LEFT);
        assert_eq!(zone(350, 450), BOTTOM_RIGHT);
    }

    #[test]
    fn from_cursor_selects_side_zones() {
        assert_eq!(zone(250, 250), TOP);
        assert_eq!(zone(250, 450), BOTTOM);
        assert_eq!(zone(150, 350), LEFT);
        assert_eq!(zone(350, 350), RIGHT);
    }

    #[test]
    fn from_cursor_centre_zone_uses_bottom_right() {
        assert_eq!(zone(250, 350), BOTTOM_RIGHT);
        assert_eq!(zone(200, 300), BOTTOM_RIGHT);
        assert_eq!(zone(299, 399), BOTTOM_RIGHT);
    }

    #[test]
    fn from_cursor_zone_boundaries() {
        assert_eq!(zone(100, 350), LEFT);
        assert_eq!(zone(199, 350), LEFT);
        assert_eq!(zone(200, 350), BOTTOM_RIGHT);
        assert_eq!(zone(299, 350), BOTTOM_RIGHT);
        assert_eq!(zone(300, 350), RIGHT);
        assert_eq!(zone(399, 350), RIGHT);

        assert_eq!(zone(250, 200), TOP);
        assert_eq!(zone(250, 299), TOP);
        assert_eq!(zone(250, 300), BOTTOM_RIGHT);
        assert_eq!(zone(250, 399), BOTTOM_RIGHT);
        assert_eq!(zone(250, 400), BOTTOM);
        assert_eq!(zone(250, 499), BOTTOM);

        assert_eq!(zone(199, 299), TOP_LEFT);
        assert_eq!(zone(300, 400), BOTTOM_RIGHT);
        assert_eq!(zone(300, 299), TOP_RIGHT);
        assert_eq!(zone(199, 400), BOTTOM_LEFT);
    }

    #[test]
    fn from_cursor_handles_sizes_not_divisible_by_three() {
        // Width 100: offsets 0..=33 left, 34..=66 middle, 67..=99 right.
        let frame = Rect::from_xywh(0, 0, 100, 100);
        let at = |x: i32| ResizeEdges::from_cursor(frame, Point::new(x, 50));
        assert_eq!(at(33), LEFT);
        assert_eq!(at(34), BOTTOM_RIGHT);
        assert_eq!(at(66), BOTTOM_RIGHT);
        assert_eq!(at(67), RIGHT);
    }

    #[test]
    fn from_cursor_clamps_cursor_outside_frame() {
        assert_eq!(zone(-1000, -1000), TOP_LEFT);
        assert_eq!(zone(5000, -1000), TOP_RIGHT);
        assert_eq!(zone(-1000, 5000), BOTTOM_LEFT);
        assert_eq!(zone(5000, 5000), BOTTOM_RIGHT);
        assert_eq!(zone(250, -50), TOP);
        assert_eq!(zone(250, 900), BOTTOM);
        assert_eq!(zone(50, 350), LEFT);
        // Right/bottom are exclusive: the first pixel outside maps to the last column/row.
        assert_eq!(zone(400, 350), RIGHT);
        assert_eq!(zone(250, 500), BOTTOM);
        assert_eq!(zone(i32::MIN, i32::MAX), BOTTOM_LEFT);
    }

    #[test]
    fn from_cursor_on_degenerate_frames() {
        assert_eq!(
            ResizeEdges::from_cursor(Rect::new(10, 10, 10, 10), Point::new(0, 0)),
            BOTTOM_RIGHT
        );
        // Zero width: the horizontal axis counts as the middle third.
        assert_eq!(
            ResizeEdges::from_cursor(Rect::new(0, 0, 0, 300), Point::new(-5, 10)),
            TOP
        );
        assert_eq!(
            ResizeEdges::from_cursor(Rect::new(0, 0, 300, 0), Point::new(10, 10)),
            LEFT
        );
    }

    #[test]
    fn edge_predicates() {
        assert!(ResizeEdges::default().is_empty());
        assert_eq!(ResizeEdges::default(), NONE);
        assert!(!LEFT.is_empty());
        assert!(LEFT.horizontal() && !LEFT.vertical());
        assert!(RIGHT.horizontal() && !RIGHT.vertical());
        assert!(TOP.vertical() && !TOP.horizontal());
        assert!(BOTTOM.vertical() && !BOTTOM.horizontal());
        assert!(BOTTOM_RIGHT.horizontal() && BOTTOM_RIGHT.vertical());
        assert_eq!(
            BOTTOM_RIGHT,
            ResizeEdges {
                right: true,
                bottom: true,
                ..NONE
            }
        );
        assert!(!NONE.horizontal() && !NONE.vertical());
    }

    #[test]
    fn constraints_default_is_one_pixel_minimum_without_maximum() {
        let c = SizeConstraints::default();
        assert_eq!(c.min, Size::new(1, 1));
        assert_eq!(c.max, None);
        assert_eq!(c.clamp(Size::new(0, -10)), Size::new(1, 1));
        assert_eq!(c.clamp(Size::new(i32::MAX, 5)), Size::new(i32::MAX, 5));
        assert_eq!(c.width_range(), (1, i32::MAX));
    }

    #[test]
    fn constraints_clamp_to_min_and_max() {
        let c = SizeConstraints {
            min: Size::new(100, 50),
            max: Some(Size::new(800, 600)),
        };
        assert_eq!(c.clamp(Size::new(10, 10)), Size::new(100, 50));
        assert_eq!(c.clamp(Size::new(1000, 1000)), Size::new(800, 600));
        assert_eq!(c.clamp(Size::new(400, 300)), Size::new(400, 300));
        assert_eq!(c.clamp(Size::new(10, 1000)), Size::new(100, 600));
        // A non-positive minimum is raised to 1.
        let c = SizeConstraints {
            min: Size::new(0, -4),
            max: None,
        };
        assert_eq!(c.clamp(Size::new(-7, 0)), Size::new(1, 1));
    }

    #[test]
    fn constraints_min_wins_over_smaller_max() {
        let c = SizeConstraints {
            min: Size::new(300, 200),
            max: Some(Size::new(100, 400)),
        };
        assert_eq!(c.width_range(), (300, 300));
        assert_eq!(c.height_range(), (200, 400));
        assert_eq!(c.clamp(Size::new(50, 50)), Size::new(300, 200));
        assert_eq!(c.clamp(Size::new(1000, 1000)), Size::new(300, 400));
        let zero_max = SizeConstraints {
            min: Size::new(0, 0),
            max: Some(Size::new(0, 0)),
        };
        assert_eq!(zero_max.clamp(Size::new(50, 50)), Size::new(1, 1));
    }

    #[test]
    fn constraints_allows() {
        let c = SizeConstraints {
            min: Size::new(100, 50),
            max: Some(Size::new(800, 600)),
        };
        assert!(c.allows(Size::new(100, 50)));
        assert!(c.allows(Size::new(800, 600)));
        assert!(!c.allows(Size::new(99, 50)));
        assert!(!c.allows(Size::new(100, 601)));
        assert!(!SizeConstraints::default().allows(Size::new(0, 10)));
        assert!(SizeConstraints::default().allows(Size::new(1, 1)));
    }

    #[test]
    fn resize_frame_moves_single_edges() {
        let c = SizeConstraints::default();
        let start = Rect::new(100, 100, 500, 400);
        assert_eq!(
            resize_frame(start, LEFT, Point::new(-30, 77), &c),
            Rect::new(70, 100, 500, 400)
        );
        assert_eq!(
            resize_frame(start, LEFT, Point::new(30, 0), &c),
            Rect::new(130, 100, 500, 400)
        );
        assert_eq!(
            resize_frame(start, RIGHT, Point::new(40, -9), &c),
            Rect::new(100, 100, 540, 400)
        );
        assert_eq!(
            resize_frame(start, RIGHT, Point::new(-40, 0), &c),
            Rect::new(100, 100, 460, 400)
        );
        assert_eq!(
            resize_frame(start, TOP, Point::new(55, -20), &c),
            Rect::new(100, 80, 500, 400)
        );
        assert_eq!(
            resize_frame(start, TOP, Point::new(0, 20), &c),
            Rect::new(100, 120, 500, 400)
        );
        assert_eq!(
            resize_frame(start, BOTTOM, Point::new(3, 10), &c),
            Rect::new(100, 100, 500, 410)
        );
        assert_eq!(
            resize_frame(start, BOTTOM, Point::new(0, -10), &c),
            Rect::new(100, 100, 500, 390)
        );
    }

    #[test]
    fn resize_frame_moves_corners() {
        let c = SizeConstraints::default();
        let start = Rect::new(100, 100, 500, 400);
        let d = Point::new(-25, 35);
        assert_eq!(
            resize_frame(start, TOP_LEFT, d, &c),
            Rect::new(75, 135, 500, 400)
        );
        assert_eq!(
            resize_frame(start, TOP_RIGHT, d, &c),
            Rect::new(100, 135, 475, 400)
        );
        assert_eq!(
            resize_frame(start, BOTTOM_LEFT, d, &c),
            Rect::new(75, 100, 500, 435)
        );
        assert_eq!(
            resize_frame(start, BOTTOM_RIGHT, d, &c),
            Rect::new(100, 100, 475, 435)
        );
    }

    #[test]
    fn resize_frame_min_clamp_keeps_opposite_edge_fixed() {
        let c = SizeConstraints {
            min: Size::new(200, 150),
            max: None,
        };
        let start = Rect::new(100, 100, 500, 400);
        // Dragging the left edge past the minimum: right stays, left = right - min.
        assert_eq!(
            resize_frame(start, LEFT, Point::new(350, 0), &c),
            Rect::new(300, 100, 500, 400)
        );
        // Dragging the right edge: left stays.
        assert_eq!(
            resize_frame(start, RIGHT, Point::new(-350, 0), &c),
            Rect::new(100, 100, 300, 400)
        );
        // Top edge: bottom stays.
        assert_eq!(
            resize_frame(start, TOP, Point::new(0, 280), &c),
            Rect::new(100, 250, 500, 400)
        );
        // Bottom edge: top stays.
        assert_eq!(
            resize_frame(start, BOTTOM, Point::new(0, -280), &c),
            Rect::new(100, 100, 500, 250)
        );
        // Top-left corner: bottom-right corner stays.
        assert_eq!(
            resize_frame(start, TOP_LEFT, Point::new(390, 290), &c),
            Rect::new(300, 250, 500, 400)
        );
    }

    #[test]
    fn resize_frame_max_clamp_keeps_opposite_edge_fixed() {
        let c = SizeConstraints {
            min: Size::new(1, 1),
            max: Some(Size::new(600, 500)),
        };
        let start = Rect::new(100, 100, 500, 400);
        assert_eq!(
            resize_frame(start, LEFT, Point::new(-900, 0), &c),
            Rect::new(-100, 100, 500, 400)
        );
        assert_eq!(
            resize_frame(start, RIGHT, Point::new(900, 0), &c),
            Rect::new(100, 100, 700, 400)
        );
        assert_eq!(
            resize_frame(start, TOP, Point::new(0, -900), &c),
            Rect::new(100, -100, 500, 400)
        );
        assert_eq!(
            resize_frame(start, BOTTOM, Point::new(0, 900), &c),
            Rect::new(100, 100, 500, 600)
        );
        assert_eq!(
            resize_frame(start, BOTTOM_LEFT, Point::new(-900, 900), &c),
            Rect::new(-100, 100, 500, 600)
        );
    }

    #[test]
    fn resize_frame_edges_never_cross_with_extreme_deltas() {
        let c = SizeConstraints::default();
        let start = Rect::new(100, 100, 500, 400);
        assert_eq!(
            resize_frame(start, LEFT, Point::new(i32::MAX, 0), &c),
            Rect::new(499, 100, 500, 400)
        );
        assert_eq!(
            resize_frame(start, RIGHT, Point::new(i32::MIN, 0), &c),
            Rect::new(100, 100, 101, 400)
        );
        assert_eq!(
            resize_frame(start, TOP, Point::new(0, i32::MAX), &c),
            Rect::new(100, 399, 500, 400)
        );
        assert_eq!(
            resize_frame(start, BOTTOM, Point::new(0, i32::MIN), &c),
            Rect::new(100, 100, 500, 101)
        );
        let r = resize_frame(start, TOP_LEFT, Point::new(1_000_000, 1_000_000), &c);
        assert_eq!(r, Rect::new(499, 399, 500, 400));
        let r = resize_frame(start, BOTTOM_RIGHT, Point::new(-1_000_000, -1_000_000), &c);
        assert_eq!(r, Rect::new(100, 100, 101, 101));
        // Growing towards the coordinate limits saturates instead of overflowing.
        let r = resize_frame(start, LEFT, Point::new(i32::MIN, 0), &c);
        assert_eq!(r.right, 500);
        assert!(r.left < r.right);
    }

    #[test]
    fn resize_frame_leaves_inactive_axes_untouched() {
        let c = SizeConstraints {
            min: Size::new(1000, 1000),
            max: None,
        };
        let start = Rect::new(100, 100, 500, 400);
        assert_eq!(resize_frame(start, NONE, Point::new(50, 50), &c), start);
        // Only the vertical axis is active: the too-narrow width is not clamped.
        let r = resize_frame(start, BOTTOM, Point::new(50, 10), &c);
        assert_eq!((r.left, r.right), (100, 500));
        assert_eq!((r.top, r.bottom), (100, 1100));
    }

    #[test]
    fn resize_frame_with_both_edges_of_an_axis_translates_it() {
        let c = SizeConstraints {
            min: Size::new(1, 1),
            max: Some(Size::new(300, 1000)),
        };
        let both = ResizeEdges {
            left: true,
            right: true,
            ..NONE
        };
        let start = Rect::new(100, 100, 300, 400);
        assert_eq!(
            resize_frame(start, both, Point::new(25, 9), &c),
            Rect::new(125, 100, 325, 400)
        );
        // Clamping keeps the moved left edge.
        let wide = Rect::new(100, 100, 600, 400);
        assert_eq!(
            resize_frame(wide, both, Point::new(10, 0), &c),
            Rect::new(110, 100, 410, 400)
        );
    }

    #[test]
    fn resize_frame_property_fixed_edges_and_constrained_lengths() {
        let mut rng = Lcg(0x5EED_1234);
        for _ in 0..5000 {
            let start = Rect::from_xywh(
                rng.range(-4000, 4000),
                rng.range(-2000, 2000),
                rng.range(1, 2500),
                rng.range(1, 1500),
            );
            let edges = ResizeEdges {
                left: rng.chance(50),
                top: rng.chance(50),
                right: rng.chance(50),
                bottom: rng.chance(50),
            };
            let min = Size::new(rng.range(-10, 500), rng.range(-10, 500));
            let max = if rng.chance(50) {
                Some(Size::new(rng.range(0, 3000), rng.range(0, 3000)))
            } else {
                None
            };
            let c = SizeConstraints { min, max };
            let delta = Point::new(rng.range(-6000, 6000), rng.range(-6000, 6000));
            let r = resize_frame(start, edges, delta, &c);

            let axes = [
                (
                    start.left,
                    start.right,
                    r.left,
                    r.right,
                    edges.left,
                    edges.right,
                    delta.x,
                    c.width_range(),
                ),
                (
                    start.top,
                    start.bottom,
                    r.top,
                    r.bottom,
                    edges.top,
                    edges.bottom,
                    delta.y,
                    c.height_range(),
                ),
            ];
            for (lo, hi, new_lo, new_hi, move_lo, move_hi, d, (min, max)) in axes {
                match (move_lo, move_hi) {
                    (false, false) => assert_eq!((new_lo, new_hi), (lo, hi)),
                    (true, false) => {
                        assert_eq!(new_hi, hi);
                        if (min..=max).contains(&(hi - (lo + d))) {
                            assert_eq!(new_lo, lo + d);
                        }
                    }
                    (false, true) => {
                        assert_eq!(new_lo, lo);
                        if (min..=max).contains(&(hi + d - lo)) {
                            assert_eq!(new_hi, hi + d);
                        }
                    }
                    (true, true) => assert_eq!(new_lo, lo + d),
                }
                if move_lo || move_hi {
                    assert!(
                        (min..=max).contains(&(new_hi - new_lo)),
                        "{start:?} {edges:?} {delta:?} {c:?} -> {r:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn anchor_size_bottom_right_keeps_top_left() {
        let frame = Rect::new(100, 100, 500, 400);
        let size = Size::new(250, 120);
        assert_eq!(
            anchor_size(frame, BOTTOM_RIGHT, size),
            Rect::new(100, 100, 350, 220)
        );
        assert_eq!(
            anchor_size(frame, RIGHT, size),
            Rect::new(100, 100, 350, 220)
        );
        assert_eq!(
            anchor_size(frame, BOTTOM, size),
            Rect::new(100, 100, 350, 220)
        );
        assert_eq!(
            anchor_size(frame, NONE, size),
            Rect::new(100, 100, 350, 220)
        );
    }

    #[test]
    fn anchor_size_left_and_top_keep_right_and_bottom() {
        let frame = Rect::new(100, 100, 500, 400);
        let size = Size::new(250, 120);
        assert_eq!(
            anchor_size(frame, LEFT, size),
            Rect::new(250, 100, 500, 220)
        );
        assert_eq!(anchor_size(frame, TOP, size), Rect::new(100, 280, 350, 400));
        // Growing works the same way.
        assert_eq!(
            anchor_size(frame, LEFT, Size::new(700, 300)),
            Rect::new(-200, 100, 500, 400)
        );
    }

    #[test]
    fn anchor_size_corners() {
        let frame = Rect::new(100, 100, 500, 400);
        let size = Size::new(250, 120);
        assert_eq!(
            anchor_size(frame, TOP_LEFT, size),
            Rect::new(250, 280, 500, 400)
        );
        assert_eq!(
            anchor_size(frame, TOP_RIGHT, size),
            Rect::new(100, 280, 350, 400)
        );
        assert_eq!(
            anchor_size(frame, BOTTOM_LEFT, size),
            Rect::new(250, 100, 500, 220)
        );
        // Both horizontal edges dragged: the left edge is kept.
        let both = ResizeEdges {
            left: true,
            right: true,
            top: true,
            bottom: true,
        };
        assert_eq!(
            anchor_size(frame, both, size),
            Rect::new(100, 100, 350, 220)
        );
    }

    #[test]
    fn move_frame_translates_by_cursor_delta() {
        let start = Rect::new(100, 100, 500, 400);
        assert_eq!(
            move_frame(start, Point::new(300, 200), Point::new(310, 180)),
            Rect::new(110, 80, 510, 380)
        );
        assert_eq!(
            move_frame(start, Point::new(300, 200), Point::new(300, 200)),
            start
        );
        let r = move_frame(start, Point::new(0, 0), Point::new(-3000, -500));
        assert_eq!(r, Rect::new(-2900, -400, -2500, -100));
        assert_eq!(r.size(), start.size());
    }
}
