//! Snap engine: aligns a moving or resizing frame to nearby candidate lines.
//!
//! Candidate lines come from monitor bounds, work areas (taskbar-aware), the edges of other
//! visible windows, the work-area centre lines and an optional grid. Vertical lines (constant
//! `x`) snap the frame horizontally, horizontal lines (constant `y`) snap it vertically. Each
//! line carries the extent it covers on the perpendicular axis so that the gesture thread can
//! draw a guide for every snap that happened.
//!
//! All functions are pure and work in physical pixels with Win32 `RECT` semantics (right and
//! bottom exclusive): snapping a frame's `right` to another window's `left` makes the two
//! frames exactly adjacent.

use crate::geometry::{MonitorInfo, Rect, monitor_index_from_rect};
use crate::resize::{ResizeEdges, SizeConstraints};

/// Upper bound applied to [`GridSpec::columns`] and [`GridSpec::rows`] so that the work done
/// per mouse move stays bounded; larger values are clamped to it.
pub const MAX_GRID_DIVISIONS: u32 = 256;

/// An evenly spaced grid laid over a monitor's work area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSpec {
    /// Number of columns; yields `columns + 1` vertical lines (0 disables vertical lines).
    pub columns: u32,
    /// Number of rows; yields `rows + 1` horizontal lines (0 disables horizontal lines).
    pub rows: u32,
}

/// What to snap to and how far snapping reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapConfig {
    /// Maximum snap distance in physical pixels (inclusive). `<= 0` disables snapping.
    pub threshold: i32,
    /// Snap to the full bounds of every monitor.
    pub monitor_edges: bool,
    /// Snap to the work area of every monitor (the monitor minus taskbar and app bars).
    pub work_area_edges: bool,
    /// Snap to (and align with) the edges of other windows.
    pub window_edges: bool,
    /// Snap the frame's centre to the work-area centre lines (moves only).
    pub center_lines: bool,
    /// Snap to a grid over the work area of the monitor that holds most of the frame.
    pub grid: Option<GridSpec>,
}

/// The surroundings a frame can snap to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SnapEnvironment {
    /// All monitors.
    pub monitors: Vec<MonitorInfo>,
    /// Visible frames of other windows (the moving window is NOT included), top-most first.
    pub windows: Vec<Rect>,
}

/// Orientation of a snap guide line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Orientation {
    /// A vertical line (constant `x`); produced by horizontal snapping.
    Vertical,
    /// A horizontal line (constant `y`); produced by vertical snapping.
    Horizontal,
}

/// Where a snap line came from. The declaration order is also the tie-break priority (first
/// wins) when two lines are equally close.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SnapSource {
    /// An edge of a monitor's work area.
    WorkAreaEdge,
    /// An edge of a monitor's full bounds.
    MonitorEdge,
    /// An edge of another window.
    WindowEdge,
    /// A grid line.
    Grid,
    /// A work-area centre line.
    Center,
}

impl SnapSource {
    /// Tie-break rank: lower is preferred.
    const fn rank(self) -> u8 {
        match self {
            SnapSource::WorkAreaEdge => 0,
            SnapSource::MonitorEdge => 1,
            SnapSource::WindowEdge => 2,
            SnapSource::Grid => 3,
            SnapSource::Center => 4,
        }
    }
}

/// A guide line to draw for a snap that happened.
///
/// A vertical guide is the line `x = position` from `y = start` to `y = end` (end exclusive);
/// a horizontal guide is the line `y = position` from `x = start` to `x = end`. The range
/// covers both the snapped frame and the extent of the line it snapped to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapGuide {
    /// Whether the guide is a vertical or a horizontal line.
    pub orientation: Orientation,
    /// `x` of a vertical guide, `y` of a horizontal guide.
    pub position: i32,
    /// Start of the guide on the perpendicular axis (inclusive).
    pub start: i32,
    /// End of the guide on the perpendicular axis (exclusive).
    pub end: i32,
    /// What the frame snapped to.
    pub source: SnapSource,
}

/// The outcome of a snap computation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapResult {
    /// The snapped frame (the input frame when nothing snapped).
    pub rect: Rect,
    /// One guide per snap that happened (empty when nothing snapped).
    pub guides: Vec<SnapGuide>,
}

impl SnapResult {
    fn unchanged(rect: Rect) -> Self {
        Self {
            rect,
            guides: Vec::new(),
        }
    }
}

/// A candidate line: `pos` on its own axis, `[start, end)` on the perpendicular axis.
#[derive(Clone, Copy, Debug)]
struct Line {
    pos: i32,
    start: i32,
    end: i32,
    source: SnapSource,
}

impl Line {
    /// Guide for this line, extended to cover the frame range `[start, end)`.
    fn guide(&self, orientation: Orientation, start: i32, end: i32) -> SnapGuide {
        SnapGuide {
            orientation,
            position: self.pos,
            start: start.min(self.start),
            end: end.max(self.end),
            source: self.source,
        }
    }
}

/// Candidate lines of both axes: `x` holds vertical lines, `y` horizontal ones, each in input
/// order (monitors and windows in environment order).
#[derive(Debug, Default)]
struct Candidates {
    x: Vec<Line>,
    y: Vec<Line>,
}

impl Candidates {
    /// Collects every enabled candidate line for `frame`. Centre lines are only collected when
    /// `with_center` is set (moves).
    fn collect(frame: &Rect, env: &SnapEnvironment, cfg: &SnapConfig, with_center: bool) -> Self {
        let mut c = Candidates::default();
        // Lines of a rectangle only count when the frame overlaps (or comes within the snap
        // distance of) that rectangle on the perpendicular axis. `+ 1` makes the span test
        // inclusive, matching the `<=` used for the snap distance itself. Without this a
        // frame on one monitor could snap to the bottom edge of a far-away monitor that
        // merely happens to share the same y coordinate.
        let tolerance = cfg.threshold.saturating_add(1);
        for m in &env.monitors {
            let x = frame.vertical_spans_touch(&m.bounds, tolerance);
            let y = frame.horizontal_spans_touch(&m.bounds, tolerance);
            if cfg.work_area_edges {
                c.push_edges(&m.work_area, SnapSource::WorkAreaEdge, x, y);
            }
            if cfg.monitor_edges {
                c.push_edges(&m.bounds, SnapSource::MonitorEdge, x, y);
            }
        }
        if cfg.window_edges {
            for w in &env.windows {
                c.push_edges(
                    w,
                    SnapSource::WindowEdge,
                    frame.vertical_spans_touch(w, tolerance),
                    frame.horizontal_spans_touch(w, tolerance),
                );
            }
        }
        if let Some(grid) = cfg.grid {
            let dominant =
                monitor_index_from_rect(&env.monitors, frame).and_then(|i| env.monitors.get(i));
            if let Some(m) = dominant {
                c.push_grid(&m.work_area, grid);
            }
        }
        if with_center && cfg.center_lines {
            for m in &env.monitors {
                let w = &m.work_area;
                if !w.is_empty() && frame.intersection(&m.bounds).is_some() {
                    let centre = w.center();
                    c.x.push(Line {
                        pos: centre.x,
                        start: w.top,
                        end: w.bottom,
                        source: SnapSource::Center,
                    });
                    c.y.push(Line {
                        pos: centre.y,
                        start: w.left,
                        end: w.right,
                        source: SnapSource::Center,
                    });
                }
            }
        }
        c
    }

    /// Adds the left/right edges of `r` as vertical lines (when `x`) and its top/bottom edges
    /// as horizontal lines (when `y`). Empty rectangles contribute nothing.
    fn push_edges(&mut self, r: &Rect, source: SnapSource, x: bool, y: bool) {
        if r.is_empty() {
            return;
        }
        if x {
            self.x.push(Line {
                pos: r.left,
                start: r.top,
                end: r.bottom,
                source,
            });
            self.x.push(Line {
                pos: r.right,
                start: r.top,
                end: r.bottom,
                source,
            });
        }
        if y {
            self.y.push(Line {
                pos: r.top,
                start: r.left,
                end: r.right,
                source,
            });
            self.y.push(Line {
                pos: r.bottom,
                start: r.left,
                end: r.right,
                source,
            });
        }
    }

    /// Adds the grid lines of `work`: `work.left + round(i * work.width / columns)` for
    /// `i in 0..=columns`, rows analogous.
    fn push_grid(&mut self, work: &Rect, grid: GridSpec) {
        if work.is_empty() {
            return;
        }
        for pos in grid_lines(work.left, work.right, grid.columns) {
            self.x.push(Line {
                pos,
                start: work.top,
                end: work.bottom,
                source: SnapSource::Grid,
            });
        }
        for pos in grid_lines(work.top, work.bottom, grid.rows) {
            self.y.push(Line {
                pos,
                start: work.left,
                end: work.right,
                source: SnapSource::Grid,
            });
        }
    }
}

/// Positions `lo + round(i * (hi - lo) / divisions)` for `i in 0..=divisions` (halves round
/// up); nothing for zero divisions. `divisions` is clamped to [`MAX_GRID_DIVISIONS`].
fn grid_lines(lo: i32, hi: i32, divisions: u32) -> impl Iterator<Item = i32> {
    let n = i64::from(divisions.min(MAX_GRID_DIVISIONS));
    let len = i64::from(hi) - i64::from(lo);
    let count = if n > 0 { n + 1 } else { 0 };
    (0..count).map(move |i| {
        let offset = (2 * i * len + n) / (2 * n);
        i32::try_from(i64::from(lo) + offset).unwrap_or(hi)
    })
}

/// Signed distance `line - feature` when it is within `threshold`.
fn delta_within(line: i32, feature: i32, threshold: i32) -> Option<i32> {
    let delta = i64::from(line) - i64::from(feature);
    if delta.abs() > i64::from(threshold) {
        return None;
    }
    i32::try_from(delta).ok()
}

/// Ordering key of a move candidate: distance, source rank, edge rank (low before high),
/// input index. Smaller is better.
type MoveKey = (u32, u8, u8, usize);

/// Best line for a move on one axis, with the delta that aligns the frame to it.
///
/// Edge lines are compared with the frame's `low` (left/top) and `high` (right/bottom)
/// edges, centre lines only with its `centre`. Ties are broken by source priority, then
/// low edge before high edge, then input order.
fn best_move_snap(
    lines: &[Line],
    low: i32,
    high: i32,
    centre: i32,
    threshold: i32,
) -> Option<(Line, i32)> {
    let mut best: Option<(MoveKey, Line, i32)> = None;
    for (index, line) in lines.iter().enumerate() {
        let features: [Option<i32>; 2] = if line.source == SnapSource::Center {
            [Some(centre), None]
        } else {
            [Some(low), Some(high)]
        };
        for (feature_rank, feature) in (0u8..).zip(features) {
            let Some(delta) = feature.and_then(|f| delta_within(line.pos, f, threshold)) else {
                continue;
            };
            let key = (
                delta.unsigned_abs(),
                line.source.rank(),
                feature_rank,
                index,
            );
            if best.as_ref().is_none_or(|(k, _, _)| key < *k) {
                best = Some((key, *line, delta));
            }
        }
    }
    best.map(|(_, line, delta)| (line, delta))
}

/// Nearest line to `feature` within `threshold`; ties are broken by source priority, then
/// input order.
fn nearest_line(lines: &[Line], feature: i32, threshold: i32) -> Option<Line> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let delta = delta_within(line.pos, feature, threshold)?;
            Some(((delta.unsigned_abs(), line.source.rank(), index), *line))
        })
        .min_by_key(|(key, _)| *key)
        .map(|(_, line)| line)
}

/// Snaps a frame that is being moved. The size never changes.
///
/// X and Y are solved independently. On each axis the frame's left/top and right/bottom
/// edges are compared with every enabled edge line (work areas, monitors, other windows,
/// grid) and its centre with the work-area centre lines; the closest pair within
/// `cfg.threshold` wins and the frame is translated by that distance. Equal distances are
/// resolved by source priority ([`SnapSource::WorkAreaEdge`] > `MonitorEdge` > `WindowEdge` >
/// `Grid` > `Center`), then left/top edge before right/bottom edge, then the first candidate
/// in input order. Snapping on both axes yields corner snaps.
///
/// Window edges are only candidates when the frames (nearly) overlap on the perpendicular
/// axis, see [`Rect::vertical_spans_touch`]. Grid lines come from the monitor holding most of
/// the frame only.
///
/// Returns one guide per snapped axis, vertical first. A frame that is already aligned
/// (distance 0) still yields its guide. With `threshold <= 0` or no candidate in reach, the
/// input frame is returned without guides.
#[must_use]
pub fn snap_move(frame: Rect, env: &SnapEnvironment, cfg: &SnapConfig) -> SnapResult {
    if cfg.threshold <= 0 {
        return SnapResult::unchanged(frame);
    }
    let c = Candidates::collect(&frame, env, cfg, true);
    let centre = frame.center();
    let snap_x = best_move_snap(&c.x, frame.left, frame.right, centre.x, cfg.threshold);
    let snap_y = best_move_snap(&c.y, frame.top, frame.bottom, centre.y, cfg.threshold);
    let rect = frame.translate(snap_x.map_or(0, |(_, d)| d), snap_y.map_or(0, |(_, d)| d));

    let mut guides = Vec::new();
    if let Some((line, _)) = snap_x {
        guides.push(line.guide(Orientation::Vertical, rect.top, rect.bottom));
    }
    if let Some((line, _)) = snap_y {
        guides.push(line.guide(Orientation::Horizontal, rect.left, rect.right));
    }
    SnapResult { rect, guides }
}

/// Snaps the dragged edges of a frame that is being resized.
///
/// Only the edges in `edges` may move; all other edges keep their exact position. Each active
/// edge independently snaps to the nearest enabled line on its axis within `cfg.threshold`
/// (work areas, monitors, other windows, grid; centre lines are never used for resizing), with
/// ties resolved by source priority and then input order. A snap is applied only if the
/// resulting length on that axis is allowed by `constraints` (so it never shrinks below the
/// minimum, grows above the maximum or crosses the opposite edge); otherwise that edge stays
/// unsnapped. Edges are processed in the order left, right, top, bottom, and each check uses
/// the edges snapped before it.
///
/// Returns one guide per snapped edge, in the same order. With `threshold <= 0`, no active
/// edge or no candidate in reach, the input frame is returned without guides.
#[must_use]
pub fn snap_resize(
    frame: Rect,
    edges: ResizeEdges,
    env: &SnapEnvironment,
    cfg: &SnapConfig,
    constraints: &SizeConstraints,
) -> SnapResult {
    if cfg.threshold <= 0 || edges.is_empty() {
        return SnapResult::unchanged(frame);
    }
    let c = Candidates::collect(&frame, env, cfg, false);
    let width_ok = |lo: i32, hi: i32| length_allowed(lo, hi, constraints.width_range());
    let height_ok = |lo: i32, hi: i32| length_allowed(lo, hi, constraints.height_range());
    let t = cfg.threshold;

    let mut rect = frame;
    let mut left = None;
    let mut right = None;
    let mut top = None;
    let mut bottom = None;
    if edges.left {
        left = nearest_line(&c.x, rect.left, t).filter(|l| width_ok(l.pos, rect.right));
        if let Some(l) = left {
            rect.left = l.pos;
        }
    }
    if edges.right {
        right = nearest_line(&c.x, rect.right, t).filter(|l| width_ok(rect.left, l.pos));
        if let Some(l) = right {
            rect.right = l.pos;
        }
    }
    if edges.top {
        top = nearest_line(&c.y, rect.top, t).filter(|l| height_ok(l.pos, rect.bottom));
        if let Some(l) = top {
            rect.top = l.pos;
        }
    }
    if edges.bottom {
        bottom = nearest_line(&c.y, rect.bottom, t).filter(|l| height_ok(rect.top, l.pos));
        if let Some(l) = bottom {
            rect.bottom = l.pos;
        }
    }

    let vertical = [left, right]
        .into_iter()
        .flatten()
        .map(|l| l.guide(Orientation::Vertical, rect.top, rect.bottom));
    let horizontal = [top, bottom]
        .into_iter()
        .flatten()
        .map(|l| l.guide(Orientation::Horizontal, rect.left, rect.right));
    SnapResult {
        rect,
        guides: vertical.chain(horizontal).collect(),
    }
}

/// `true` when the span `[lo, hi)` has a length within the inclusive `range`.
fn length_allowed(lo: i32, hi: i32, range: (i32, i32)) -> bool {
    let len = i64::from(hi) - i64::from(lo);
    (i64::from(range.0)..=i64::from(range.1)).contains(&len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Size;

    const NO_EDGES: ResizeEdges = ResizeEdges {
        left: false,
        top: false,
        right: false,
        bottom: false,
    };
    const LEFT: ResizeEdges = ResizeEdges {
        left: true,
        ..NO_EDGES
    };
    const TOP: ResizeEdges = ResizeEdges {
        top: true,
        ..NO_EDGES
    };
    const RIGHT: ResizeEdges = ResizeEdges {
        right: true,
        ..NO_EDGES
    };
    const TOP_RIGHT: ResizeEdges = ResizeEdges {
        top: true,
        right: true,
        ..NO_EDGES
    };

    fn mon(bounds: Rect, work_area: Rect) -> MonitorInfo {
        MonitorInfo {
            bounds,
            work_area,
            dpi: 96,
            primary: false,
        }
    }

    /// One 1920x1080 monitor with a 40 px taskbar at the bottom.
    fn single() -> SnapEnvironment {
        SnapEnvironment {
            monitors: vec![mon(
                Rect::new(0, 0, 1920, 1080),
                Rect::new(0, 0, 1920, 1040),
            )],
            windows: Vec::new(),
        }
    }

    fn with_windows(windows: Vec<Rect>) -> SnapEnvironment {
        SnapEnvironment {
            windows,
            ..single()
        }
    }

    /// Left (-2560..0), centre (0..1680) and right (1680..3600, y offset 178) monitors, each
    /// with a 40 px bottom taskbar.
    fn triple() -> SnapEnvironment {
        SnapEnvironment {
            monitors: vec![
                mon(Rect::new(-2560, 0, 0, 1440), Rect::new(-2560, 0, 0, 1400)),
                mon(Rect::new(0, 0, 1680, 1050), Rect::new(0, 0, 1680, 1010)),
                mon(
                    Rect::new(1680, 178, 3600, 1258),
                    Rect::new(1680, 178, 3600, 1218),
                ),
            ],
            windows: Vec::new(),
        }
    }

    /// Monitor, work-area and window edges enabled; centre lines and grid off.
    fn cfg(threshold: i32) -> SnapConfig {
        SnapConfig {
            threshold,
            monitor_edges: true,
            work_area_edges: true,
            window_edges: true,
            center_lines: false,
            grid: None,
        }
    }

    /// Every source disabled.
    fn nothing(threshold: i32) -> SnapConfig {
        SnapConfig {
            threshold,
            monitor_edges: false,
            work_area_edges: false,
            window_edges: false,
            center_lines: false,
            grid: None,
        }
    }

    fn vguide(position: i32, start: i32, end: i32, source: SnapSource) -> SnapGuide {
        SnapGuide {
            orientation: Orientation::Vertical,
            position,
            start,
            end,
            source,
        }
    }

    fn hguide(position: i32, start: i32, end: i32, source: SnapSource) -> SnapGuide {
        SnapGuide {
            orientation: Orientation::Horizontal,
            position,
            start,
            end,
            source,
        }
    }

    fn unconstrained() -> SizeConstraints {
        SizeConstraints::default()
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

        fn rect(&mut self) -> Rect {
            Rect::from_xywh(
                self.range(-2700, 3500),
                self.range(-100, 1300),
                self.range(20, 1500),
                self.range(20, 1000),
            )
        }

        fn config(&mut self) -> SnapConfig {
            SnapConfig {
                threshold: self.range(1, 40),
                monitor_edges: self.chance(70),
                work_area_edges: self.chance(70),
                window_edges: self.chance(70),
                center_lines: self.chance(50),
                grid: if self.chance(40) {
                    Some(GridSpec {
                        columns: self.next_u32() % 7,
                        rows: self.next_u32() % 7,
                    })
                } else {
                    None
                },
            }
        }

        fn environment(&mut self) -> SnapEnvironment {
            let mut env = triple();
            let count = self.next_u32() % 10;
            for _ in 0..count {
                let r = self.rect();
                env.windows.push(r);
            }
            env
        }
    }

    // ---------------------------------------------------------------- disabled / empty

    #[test]
    fn non_positive_threshold_disables_snapping() {
        let frame = Rect::from_xywh(3, 4, 800, 600);
        for threshold in [0, -1, i32::MIN] {
            let c = SnapConfig {
                center_lines: true,
                grid: Some(GridSpec {
                    columns: 4,
                    rows: 4,
                }),
                ..cfg(threshold)
            };
            assert_eq!(
                snap_move(frame, &single(), &c),
                SnapResult::unchanged(frame)
            );
            let r = snap_resize(
                frame,
                ResizeEdges::BOTTOM_RIGHT,
                &single(),
                &c,
                &unconstrained(),
            );
            assert_eq!(r, SnapResult::unchanged(frame));
            let r = snap_resize(frame, LEFT, &single(), &c, &unconstrained());
            assert_eq!(r, SnapResult::unchanged(frame));
        }
    }

    #[test]
    fn no_candidates_returns_input() {
        let frame = Rect::from_xywh(3, 4, 800, 600);
        let env = SnapEnvironment::default();
        let c = SnapConfig {
            center_lines: true,
            grid: Some(GridSpec {
                columns: 3,
                rows: 3,
            }),
            ..cfg(10)
        };
        assert_eq!(snap_move(frame, &env, &c), SnapResult::unchanged(frame));
        assert_eq!(
            snap_resize(frame, TOP_RIGHT, &env, &c, &unconstrained()),
            SnapResult::unchanged(frame)
        );
        // Every source switched off.
        assert_eq!(
            snap_move(frame, &single(), &nothing(10)),
            SnapResult::unchanged(frame)
        );
        // Nothing in reach.
        let far = Rect::from_xywh(500, 300, 400, 300);
        assert_eq!(
            snap_move(far, &single(), &cfg(10)),
            SnapResult::unchanged(far)
        );
    }

    // ---------------------------------------------------------------- snap_move: monitors

    #[test]
    fn move_snaps_to_work_area_top_left_corner() {
        let r = snap_move(Rect::from_xywh(6, 9, 800, 600), &single(), &cfg(10));
        assert_eq!(r.rect, Rect::from_xywh(0, 0, 800, 600));
        assert_eq!(
            r.guides,
            vec![
                vguide(0, 0, 1040, SnapSource::WorkAreaEdge),
                hguide(0, 0, 1920, SnapSource::WorkAreaEdge)
            ]
        );
    }

    #[test]
    fn move_snaps_to_work_area_right_and_bottom() {
        // Right edge 1913 (7 px short of 1920), bottom edge 1032 (8 px above the taskbar).
        let r = snap_move(Rect::from_xywh(1113, 432, 800, 600), &single(), &cfg(10));
        assert_eq!(r.rect, Rect::from_xywh(1120, 440, 800, 600));
        assert_eq!(
            r.guides,
            vec![
                vguide(1920, 0, 1040, SnapSource::WorkAreaEdge),
                hguide(1040, 0, 1920, SnapSource::WorkAreaEdge)
            ]
        );
    }

    #[test]
    fn move_snaps_to_monitor_edge_below_taskbar() {
        // Bottom edge 1075: 5 px from the monitor bottom, 35 px from the work-area bottom.
        let r = snap_move(Rect::from_xywh(500, 475, 400, 600), &single(), &cfg(10));
        assert_eq!(r.rect, Rect::from_xywh(500, 480, 400, 600));
        assert_eq!(
            r.guides,
            vec![hguide(1080, 0, 1920, SnapSource::MonitorEdge)]
        );
    }

    #[test]
    fn move_prefers_work_area_over_monitor_edge_at_equal_distance() {
        // Bottom edge 1060: 20 px from both the work-area (1040) and monitor (1080) bottoms.
        let r = snap_move(Rect::from_xywh(500, 460, 400, 600), &single(), &cfg(20));
        assert_eq!(r.rect, Rect::from_xywh(500, 440, 400, 600));
        assert_eq!(
            r.guides,
            vec![hguide(1040, 0, 1920, SnapSource::WorkAreaEdge)]
        );
    }

    #[test]
    fn move_snaps_exactly_at_threshold_but_not_beyond() {
        let env = single();
        let c = cfg(10);
        // Left edge 10 px right of x = 0.
        assert_eq!(
            snap_move(Rect::from_xywh(10, 300, 400, 300), &env, &c).rect,
            Rect::from_xywh(0, 300, 400, 300)
        );
        let r = snap_move(Rect::from_xywh(11, 300, 400, 300), &env, &c);
        assert_eq!(r, SnapResult::unchanged(Rect::from_xywh(11, 300, 400, 300)));
        // Right edge 10 px beyond x = 1920 (negative delta).
        let r = snap_move(Rect::from_xywh(1530, 300, 400, 300), &env, &c);
        assert_eq!(r.rect, Rect::from_xywh(1520, 300, 400, 300));
        let r = snap_move(Rect::from_xywh(1531, 300, 400, 300), &env, &c);
        assert_eq!(
            r,
            SnapResult::unchanged(Rect::from_xywh(1531, 300, 400, 300))
        );
        // Top edge 10 px above y = 0.
        assert_eq!(
            snap_move(Rect::from_xywh(500, -10, 400, 300), &env, &c).rect,
            Rect::from_xywh(500, 0, 400, 300)
        );
        assert!(
            snap_move(Rect::from_xywh(500, -11, 400, 300), &env, &c)
                .guides
                .is_empty()
        );
    }

    #[test]
    fn move_with_frame_already_aligned_still_reports_guides() {
        let frame = Rect::from_xywh(0, 0, 800, 600);
        let r = snap_move(frame, &single(), &cfg(10));
        assert_eq!(r.rect, frame);
        assert_eq!(r.guides.len(), 2);
    }

    #[test]
    fn move_handles_multi_monitor_layout_with_negative_coordinates() {
        let env = triple();
        let c = cfg(10);

        // Top-left corner of the left monitor.
        let r = snap_move(Rect::from_xywh(-2554, 7, 1000, 700), &env, &c);
        assert_eq!(r.rect, Rect::from_xywh(-2560, 0, 1000, 700));
        assert_eq!(
            r.guides,
            vec![
                vguide(-2560, 0, 1400, SnapSource::WorkAreaEdge),
                hguide(0, -2560, 0, SnapSource::WorkAreaEdge)
            ]
        );

        // Right edge near the boundary x = 0 shared by four lines: the left monitor's work
        // area wins (highest priority, first in input order).
        let r = snap_move(Rect::from_xywh(-1006, 500, 1000, 400), &env, &c);
        assert_eq!(r.rect, Rect::from_xywh(-1000, 500, 1000, 400));
        assert_eq!(r.guides, vec![vguide(0, 0, 1400, SnapSource::WorkAreaEdge)]);

        // Top of the offset right monitor.
        let r = snap_move(Rect::from_xywh(2000, 183, 800, 600), &env, &c);
        assert_eq!(r.rect, Rect::from_xywh(2000, 178, 800, 600));
        assert_eq!(
            r.guides,
            vec![hguide(178, 1680, 3600, SnapSource::WorkAreaEdge)]
        );

        // Bottom-right corner of the right monitor: work-area bottom at 1218.
        let r = snap_move(Rect::from_xywh(2796, 614, 800, 600), &env, &c);
        assert_eq!(r.rect, Rect::from_xywh(2800, 618, 800, 600));
        assert_eq!(
            r.guides,
            vec![
                vguide(3600, 178, 1218, SnapSource::WorkAreaEdge),
                hguide(1218, 1680, 3600, SnapSource::WorkAreaEdge)
            ]
        );
    }

    // ---------------------------------------------------------------- snap_move: windows

    #[test]
    fn move_snaps_adjacent_to_other_window() {
        let w = Rect::new(1000, 300, 1400, 700);
        // Right edge 995, 5 px left of the window's left edge, overlapping vertically.
        let r = snap_move(
            Rect::from_xywh(395, 400, 600, 200),
            &with_windows(vec![w]),
            &cfg(10),
        );
        assert_eq!(r.rect, Rect::from_xywh(400, 400, 600, 200));
        assert_eq!(r.rect.right, w.left);
        assert_eq!(
            r.guides,
            vec![vguide(1000, 300, 700, SnapSource::WindowEdge)]
        );
    }

    #[test]
    fn move_aligns_with_other_window_edges() {
        let w = Rect::new(1000, 300, 1400, 700);
        // 4 px right of the window's left edge and 3 px below its bottom edge.
        let r = snap_move(
            Rect::from_xywh(1004, 703, 300, 200),
            &with_windows(vec![w]),
            &cfg(10),
        );
        assert_eq!(r.rect, Rect::from_xywh(1000, 700, 300, 200));
        assert_eq!(
            r.guides,
            vec![
                vguide(1000, 300, 900, SnapSource::WindowEdge),
                hguide(700, 1000, 1400, SnapSource::WindowEdge)
            ]
        );
    }

    #[test]
    fn move_ignores_window_edges_when_spans_do_not_touch() {
        let env = with_windows(vec![Rect::new(1000, 300, 1400, 700)]);
        // Left edge 4 px from the window's left edge, but 20 px below it: x lines ignored.
        let frame = Rect::from_xywh(1004, 720, 300, 200);
        assert_eq!(
            snap_move(frame, &env, &cfg(10)),
            SnapResult::unchanged(frame)
        );
        // Top edge 4 px from the window's top edge, but 100 px to its right: y lines ignored.
        let frame = Rect::from_xywh(1500, 304, 200, 200);
        assert_eq!(
            snap_move(frame, &env, &cfg(10)),
            SnapResult::unchanged(frame)
        );
    }

    #[test]
    fn move_window_span_tolerance_matches_spans_touch() {
        let w = Rect::new(1000, 300, 1400, 700);
        let env = with_windows(vec![w]);
        // Gap of 9 px (< threshold 10): the window's x lines count, so the left edge aligns.
        let r = snap_move(Rect::from_xywh(1003, 709, 300, 200), &env, &cfg(10));
        assert_eq!(r.rect.left, 1000);
        // Gap of exactly 10 px (== threshold): the span test is inclusive like the snap
        // distance, so the frame both aligns left and becomes adjacent below the window.
        let r = snap_move(Rect::from_xywh(1003, 710, 300, 200), &env, &cfg(10));
        assert_eq!(r.rect, Rect::from_xywh(1000, 700, 300, 200));
        // Gap of 11 px: spans no longer touch and the y distance exceeds the threshold.
        let frame = Rect::from_xywh(1003, 711, 300, 200);
        assert_eq!(
            snap_move(frame, &env, &cfg(10)),
            SnapResult::unchanged(frame)
        );
    }

    #[test]
    fn move_window_edges_can_be_disabled() {
        let env = with_windows(vec![Rect::new(1000, 300, 1400, 700)]);
        let frame = Rect::from_xywh(395, 400, 600, 200);
        let c = SnapConfig {
            window_edges: false,
            ..cfg(10)
        };
        assert_eq!(snap_move(frame, &env, &c), SnapResult::unchanged(frame));
    }

    #[test]
    fn move_ignores_empty_windows() {
        let env = with_windows(vec![Rect::new(1000, 300, 1000, 700)]);
        let frame = Rect::from_xywh(395, 400, 600, 200);
        assert_eq!(
            snap_move(frame, &env, &cfg(10)),
            SnapResult::unchanged(frame)
        );
    }

    // ---------------------------------------------------------------- snap_move: centre

    #[test]
    fn move_centres_frame_on_work_area_centre_lines() {
        let c = SnapConfig {
            center_lines: true,
            ..nothing(10)
        };
        // Work area 1920x1040 has its centre at (960, 520); the frame's centre is (963, 523).
        let r = snap_move(Rect::from_xywh(563, 223, 800, 600), &single(), &c);
        assert_eq!(r.rect, Rect::from_xywh(560, 220, 800, 600));
        assert_eq!(r.rect.center(), crate::geometry::Point::new(960, 520));
        assert_eq!(
            r.guides,
            vec![
                vguide(960, 0, 1040, SnapSource::Center),
                hguide(520, 0, 1920, SnapSource::Center)
            ]
        );
    }

    #[test]
    fn move_centre_lines_only_match_the_frame_centre() {
        let c = SnapConfig {
            center_lines: true,
            ..nothing(10)
        };
        // Left edge 3 px from the vertical centre line, top edge 3 px from the horizontal one.
        let frame = Rect::from_xywh(963, 523, 400, 200);
        assert_eq!(
            snap_move(frame, &single(), &c),
            SnapResult::unchanged(frame)
        );
    }

    #[test]
    fn move_centre_lines_require_the_option() {
        let frame = Rect::from_xywh(563, 223, 800, 600);
        assert_eq!(
            snap_move(frame, &single(), &cfg(10)),
            SnapResult::unchanged(frame)
        );
    }

    #[test]
    fn move_prefers_edges_over_centre_at_equal_distance() {
        // Work area 0..1000 with its vertical centre line at x = 500.
        let env = SnapEnvironment {
            monitors: vec![mon(Rect::new(0, 0, 1000, 800), Rect::new(0, 0, 1000, 800))],
            windows: Vec::new(),
        };
        let c = SnapConfig {
            center_lines: true,
            ..cfg(10)
        };
        // Width 992: left edge 4 px from x = 0, but the centre (4 + 496 = 500) is already on
        // the centre line, so the centre wins on distance (0 < 4).
        let r = snap_move(Rect::from_xywh(4, 300, 992, 100), &env, &c);
        assert_eq!(r.rect.left, 4);
        assert_eq!(r.guides[0].source, SnapSource::Center);
        // Frame width 1000: left 4, right 1004, centre 504 (all 4 px away): work-area left wins.
        let r = snap_move(Rect::from_xywh(4, 300, 1000, 100), &env, &c);
        assert_eq!(r.rect.left, 0);
        assert_eq!(r.guides[0], vguide(0, 0, 800, SnapSource::WorkAreaEdge));
    }

    // ---------------------------------------------------------------- snap_move: grid

    #[test]
    fn move_snaps_to_grid_of_dominant_monitor_only() {
        let env = SnapEnvironment {
            monitors: vec![
                mon(Rect::new(0, 0, 1920, 1080), Rect::new(0, 0, 1920, 1040)),
                mon(
                    Rect::new(1920, 0, 3840, 1080),
                    Rect::new(1920, 0, 3840, 1040),
                ),
            ],
            windows: Vec::new(),
        };
        // Columns at 0/640/1280/1920 (and 2560/3200 on the second monitor), rows at 0/520/1040.
        let c = SnapConfig {
            grid: Some(GridSpec {
                columns: 3,
                rows: 2,
            }),
            ..nothing(10)
        };

        // Mostly on the second monitor; the left edge is 4 px from the first monitor's 1280
        // column, which must not be used.
        let frame = Rect::from_xywh(1284, 300, 2000, 400);
        assert_eq!(snap_move(frame, &env, &c), SnapResult::unchanged(frame));

        // Entirely on the first monitor: snaps to its 1280 column and 520 row.
        let r = snap_move(Rect::from_xywh(1284, 516, 400, 300), &env, &c);
        assert_eq!(r.rect, Rect::from_xywh(1280, 520, 400, 300));
        assert_eq!(
            r.guides,
            vec![
                vguide(1280, 0, 1040, SnapSource::Grid),
                hguide(520, 0, 1920, SnapSource::Grid)
            ]
        );

        // On the second monitor: its 2560 column.
        let r = snap_move(Rect::from_xywh(2563, 100, 400, 300), &env, &c);
        assert_eq!(r.rect, Rect::from_xywh(2560, 100, 400, 300));
        assert_eq!(r.guides, vec![vguide(2560, 0, 1040, SnapSource::Grid)]);
    }

    #[test]
    fn grid_lines_are_rounded_and_handle_negative_offsets() {
        assert_eq!(
            grid_lines(0, 1000, 3).collect::<Vec<_>>(),
            vec![0, 333, 667, 1000]
        );
        assert_eq!(
            grid_lines(-1000, 0, 3).collect::<Vec<_>>(),
            vec![-1000, -667, -333, 0]
        );
        assert_eq!(
            grid_lines(48, 1920, 4).collect::<Vec<_>>(),
            vec![48, 516, 984, 1452, 1920]
        );
        assert_eq!(grid_lines(0, 1000, 0).count(), 0);
        assert_eq!(
            grid_lines(0, 1000, u32::MAX).count(),
            usize::try_from(MAX_GRID_DIVISIONS + 1).unwrap_or(0)
        );

        let env = SnapEnvironment {
            monitors: vec![mon(Rect::new(0, 0, 1000, 800), Rect::new(0, 0, 1000, 800))],
            windows: Vec::new(),
        };
        let c = SnapConfig {
            grid: Some(GridSpec {
                columns: 3,
                rows: 0,
            }),
            ..nothing(3)
        };
        assert_eq!(
            snap_move(Rect::from_xywh(669, 100, 100, 100), &env, &c)
                .rect
                .left,
            667
        );
        assert_eq!(
            snap_move(Rect::from_xywh(330, 100, 100, 100), &env, &c)
                .rect
                .left,
            333
        );
        // No row lines with rows = 0.
        let frame = Rect::from_xywh(500, 2, 100, 100);
        assert_eq!(snap_move(frame, &env, &c), SnapResult::unchanged(frame));
    }

    // ---------------------------------------------------------------- snap_move: tie-breaks

    #[test]
    fn move_tie_break_uses_source_priority() {
        // The window's right edge, the work-area left and the monitor left are all at x = 0.
        let env = with_windows(vec![Rect::new(-500, 200, 0, 800)]);
        let frame = Rect::from_xywh(4, 300, 400, 300);
        let source = |c: SnapConfig| snap_move(frame, &env, &c).guides[0].source;
        assert_eq!(source(cfg(10)), SnapSource::WorkAreaEdge);
        assert_eq!(
            source(SnapConfig {
                work_area_edges: false,
                ..cfg(10)
            }),
            SnapSource::MonitorEdge
        );
        assert_eq!(
            source(SnapConfig {
                window_edges: true,
                ..nothing(10)
            }),
            SnapSource::WindowEdge
        );
        // Window edge beats grid line at x = 0.
        let grid = Some(GridSpec {
            columns: 2,
            rows: 2,
        });
        assert_eq!(
            source(SnapConfig {
                window_edges: true,
                grid,
                ..nothing(10)
            }),
            SnapSource::WindowEdge
        );
        assert_eq!(
            source(SnapConfig {
                grid,
                ..nothing(10)
            }),
            SnapSource::Grid
        );
    }

    #[test]
    fn move_prefers_nearest_line_over_source_priority() {
        // Window edge at x = 2 (2 px away) beats the work-area edge at x = 0 (4 px away).
        let env = with_windows(vec![Rect::new(-500, 200, 2, 800)]);
        let r = snap_move(Rect::from_xywh(4, 300, 400, 300), &env, &cfg(10));
        assert_eq!(r.rect.left, 2);
        assert_eq!(r.guides, vec![vguide(2, 200, 800, SnapSource::WindowEdge)]);
    }

    #[test]
    fn move_tie_break_prefers_left_edge_then_input_order() {
        // Left edge 3 px right of W1.right, right edge 3 px left of W2.left.
        let w1 = Rect::new(100, 250, 500, 450);
        let w2 = Rect::new(906, 250, 1200, 450);
        let frame = Rect::from_xywh(503, 300, 400, 200);
        for windows in [vec![w1, w2], vec![w2, w1]] {
            let r = snap_move(frame, &with_windows(windows), &cfg(10));
            assert_eq!(r.rect, Rect::from_xywh(500, 300, 400, 200));
        }
        // Two lines 3 px either side of the left edge: the first window in input order wins.
        let w3 = Rect::new(506, 250, 700, 450);
        let r = snap_move(frame, &with_windows(vec![w1, w3]), &cfg(10));
        assert_eq!(r.rect.left, 500);
        let r = snap_move(frame, &with_windows(vec![w3, w1]), &cfg(10));
        assert_eq!(r.rect.left, 506);
        // Same tie on the vertical axis: top edge before bottom edge.
        let above = Rect::new(400, 100, 1000, 297);
        let below = Rect::new(400, 503, 1000, 700);
        let r = snap_move(frame, &with_windows(vec![below, above]), &cfg(10));
        assert_eq!(r.rect.top, 297);
    }

    #[test]
    fn move_is_deterministic() {
        let mut rng = Lcg(42);
        for _ in 0..200 {
            let env = rng.environment();
            let c = rng.config();
            let frame = rng.rect();
            assert_eq!(snap_move(frame, &env, &c), snap_move(frame, &env, &c));
        }
    }

    #[test]
    fn move_property_preserves_size_and_stays_within_threshold() {
        let mut rng = Lcg(0xC0FFEE);
        for _ in 0..3000 {
            let env = rng.environment();
            let c = rng.config();
            let frame = rng.rect();
            let r = snap_move(frame, &env, &c);
            assert_eq!(r.rect.size(), frame.size());
            assert!((r.rect.left - frame.left).abs() <= c.threshold);
            assert!((r.rect.top - frame.top).abs() <= c.threshold);
            let verticals: Vec<_> = r
                .guides
                .iter()
                .filter(|g| g.orientation == Orientation::Vertical)
                .collect();
            let horizontals: Vec<_> = r
                .guides
                .iter()
                .filter(|g| g.orientation == Orientation::Horizontal)
                .collect();
            assert!(verticals.len() <= 1 && horizontals.len() <= 1);
            if verticals.is_empty() {
                assert_eq!(r.rect.left, frame.left);
            }
            if horizontals.is_empty() {
                assert_eq!(r.rect.top, frame.top);
            }
            let centre = r.rect.center();
            for g in verticals {
                if g.source == SnapSource::Center {
                    assert_eq!(g.position, centre.x);
                } else {
                    assert!(g.position == r.rect.left || g.position == r.rect.right);
                }
                assert!(g.start <= r.rect.top && g.end >= r.rect.bottom);
            }
            for g in horizontals {
                if g.source == SnapSource::Center {
                    assert_eq!(g.position, centre.y);
                } else {
                    assert!(g.position == r.rect.top || g.position == r.rect.bottom);
                }
                assert!(g.start <= r.rect.left && g.end >= r.rect.right);
            }
        }
    }

    // ---------------------------------------------------------------- snap_resize

    #[test]
    fn resize_snaps_dragged_edges_to_work_area() {
        let r = snap_resize(
            Rect::new(100, 100, 1914, 1035),
            ResizeEdges::BOTTOM_RIGHT,
            &single(),
            &cfg(10),
            &unconstrained(),
        );
        assert_eq!(r.rect, Rect::new(100, 100, 1920, 1040));
        assert_eq!(
            r.guides,
            vec![
                vguide(1920, 0, 1040, SnapSource::WorkAreaEdge),
                hguide(1040, 0, 1920, SnapSource::WorkAreaEdge)
            ]
        );
    }

    #[test]
    fn resize_snaps_to_monitor_edge_beside_side_taskbar() {
        // Taskbar on the left: work area starts at x = 48.
        let env = SnapEnvironment {
            monitors: vec![mon(
                Rect::new(0, 0, 1920, 1080),
                Rect::new(48, 0, 1920, 1080),
            )],
            windows: Vec::new(),
        };
        let r = snap_resize(
            Rect::new(3, 200, 900, 700),
            LEFT,
            &env,
            &cfg(10),
            &unconstrained(),
        );
        assert_eq!(r.rect, Rect::new(0, 200, 900, 700));
        assert_eq!(r.guides, vec![vguide(0, 0, 1080, SnapSource::MonitorEdge)]);
        let r = snap_resize(
            Rect::new(51, 200, 900, 700),
            LEFT,
            &env,
            &cfg(10),
            &unconstrained(),
        );
        assert_eq!(r.rect, Rect::new(48, 200, 900, 700));
        assert_eq!(
            r.guides,
            vec![vguide(48, 0, 1080, SnapSource::WorkAreaEdge)]
        );
    }

    #[test]
    fn resize_snaps_to_window_edges() {
        let w = Rect::new(1000, 300, 1400, 700);
        let env = with_windows(vec![w]);
        let r = snap_resize(
            Rect::new(400, 296, 994, 650),
            TOP_RIGHT,
            &env,
            &cfg(10),
            &unconstrained(),
        );
        assert_eq!(r.rect, Rect::new(400, 300, 1000, 650));
        assert_eq!(
            r.guides,
            vec![
                vguide(1000, 300, 700, SnapSource::WindowEdge),
                hguide(300, 400, 1400, SnapSource::WindowEdge)
            ]
        );
    }

    #[test]
    fn resize_never_moves_inactive_edges() {
        // Every edge is within reach of a work-area edge, but only the right one is dragged.
        let frame = Rect::new(4, 6, 1914, 1036);
        let r = snap_resize(frame, RIGHT, &single(), &cfg(10), &unconstrained());
        assert_eq!(r.rect, Rect::new(4, 6, 1920, 1036));
        assert_eq!(
            r.guides,
            vec![vguide(1920, 0, 1040, SnapSource::WorkAreaEdge)]
        );
        let r = snap_resize(frame, TOP, &single(), &cfg(10), &unconstrained());
        assert_eq!(r.rect, Rect::new(4, 0, 1914, 1036));
        assert_eq!(r.guides, vec![hguide(0, 0, 1920, SnapSource::WorkAreaEdge)]);
    }

    #[test]
    fn resize_with_no_edges_is_identity() {
        let frame = Rect::new(4, 6, 1914, 1036);
        assert_eq!(
            snap_resize(frame, NO_EDGES, &single(), &cfg(10), &unconstrained()),
            SnapResult::unchanged(frame)
        );
    }

    #[test]
    fn resize_rejects_snap_below_minimum() {
        let env = with_windows(vec![Rect::new(597, 100, 900, 400)]);
        let frame = Rect::new(100, 100, 603, 400); // width 503, line 6 px to the left
        let min_500 = SizeConstraints {
            min: Size::new(500, 1),
            max: None,
        };
        assert_eq!(
            snap_resize(frame, RIGHT, &env, &cfg(10), &min_500),
            SnapResult::unchanged(frame)
        );
        let min_490 = SizeConstraints {
            min: Size::new(490, 1),
            max: None,
        };
        let r = snap_resize(frame, RIGHT, &env, &cfg(10), &min_490);
        assert_eq!(r.rect, Rect::new(100, 100, 597, 400));
        assert_eq!(r.guides.len(), 1);
    }

    #[test]
    fn resize_rejects_snap_above_maximum() {
        let env = with_windows(vec![Rect::new(905, 100, 1200, 400)]);
        let frame = Rect::new(100, 100, 895, 400); // width 795, line 10 px to the right
        let max_800 = SizeConstraints {
            min: Size::new(1, 1),
            max: Some(Size::new(800, 600)),
        };
        assert_eq!(
            snap_resize(frame, RIGHT, &env, &cfg(10), &max_800),
            SnapResult::unchanged(frame)
        );
        let max_805 = SizeConstraints {
            min: Size::new(1, 1),
            max: Some(Size::new(805, 600)),
        };
        assert_eq!(
            snap_resize(frame, RIGHT, &env, &cfg(10), &max_805).rect,
            Rect::new(100, 100, 905, 400)
        );
    }

    #[test]
    fn resize_rejects_snap_that_would_cross_the_opposite_edge() {
        // Width 4; the nearest line for the left edge lies beyond the right edge.
        let env = with_windows(vec![Rect::new(106, 100, 300, 400)]);
        let frame = Rect::new(100, 100, 104, 400);
        assert_eq!(
            snap_resize(frame, LEFT, &env, &cfg(10), &unconstrained()),
            SnapResult::unchanged(frame)
        );
    }

    #[test]
    fn resize_rejection_keeps_other_edge_snaps() {
        // The right snap (to x = 905) violates the maximum width; the top snap to y = 100 is
        // fine. Both windows offer a line at y = 100 (the right window is exactly `threshold`
        // away horizontally, which the inclusive span test accepts); the first one in input
        // order wins the tie, so the guide spans the frame and that window.
        let env = with_windows(vec![
            Rect::new(905, 100, 1200, 400),
            Rect::new(0, 50, 600, 100),
        ]);
        let frame = Rect::new(100, 104, 895, 400);
        let max = SizeConstraints {
            min: Size::new(1, 1),
            max: Some(Size::new(800, 600)),
        };
        let r = snap_resize(frame, TOP_RIGHT, &env, &cfg(10), &max);
        assert_eq!(r.rect, Rect::new(100, 100, 895, 400));
        assert_eq!(
            r.guides,
            vec![hguide(100, 100, 1200, SnapSource::WindowEdge)]
        );
    }

    #[test]
    fn resize_does_not_use_centre_lines() {
        let c = SnapConfig {
            center_lines: true,
            ..nothing(10)
        };
        let frame = Rect::new(100, 100, 963, 517);
        let r = snap_resize(
            frame,
            ResizeEdges::BOTTOM_RIGHT,
            &single(),
            &c,
            &unconstrained(),
        );
        assert_eq!(r, SnapResult::unchanged(frame));
    }

    #[test]
    fn resize_snaps_to_grid_lines() {
        let c = SnapConfig {
            grid: Some(GridSpec {
                columns: 3,
                rows: 2,
            }),
            ..nothing(10)
        };
        let r = snap_resize(
            Rect::new(100, 100, 636, 524),
            ResizeEdges::BOTTOM_RIGHT,
            &single(),
            &c,
            &unconstrained(),
        );
        assert_eq!(r.rect, Rect::new(100, 100, 640, 520));
        assert_eq!(
            r.guides,
            vec![
                vguide(640, 0, 1040, SnapSource::Grid),
                hguide(520, 0, 1920, SnapSource::Grid)
            ]
        );
    }

    #[test]
    fn resize_guides_follow_left_right_top_bottom_order() {
        let all = ResizeEdges {
            left: true,
            top: true,
            right: true,
            bottom: true,
        };
        let r = snap_resize(
            Rect::new(5, 7, 1912, 1033),
            all,
            &single(),
            &cfg(10),
            &unconstrained(),
        );
        assert_eq!(r.rect, Rect::new(0, 0, 1920, 1040));
        let positions: Vec<_> = r
            .guides
            .iter()
            .map(|g| (g.orientation, g.position))
            .collect();
        assert_eq!(
            positions,
            vec![
                (Orientation::Vertical, 0),
                (Orientation::Vertical, 1920),
                (Orientation::Horizontal, 0),
                (Orientation::Horizontal, 1040),
            ]
        );
    }

    #[test]
    fn resize_in_negative_coordinates() {
        let r = snap_resize(
            Rect::new(-2555, 500, -1000, 1396),
            ResizeEdges {
                left: true,
                bottom: true,
                ..NO_EDGES
            },
            &triple(),
            &cfg(10),
            &unconstrained(),
        );
        assert_eq!(r.rect, Rect::new(-2560, 500, -1000, 1400));
        assert_eq!(
            r.guides,
            vec![
                vguide(-2560, 0, 1400, SnapSource::WorkAreaEdge),
                hguide(1400, -2560, 0, SnapSource::WorkAreaEdge)
            ]
        );
    }

    #[test]
    fn resize_property_inactive_edges_never_move() {
        let mut rng = Lcg(0xDEAD_BEEF);
        for _ in 0..3000 {
            let env = rng.environment();
            let c = rng.config();
            let frame = rng.rect();
            let edges = ResizeEdges {
                left: rng.chance(50),
                top: rng.chance(50),
                right: rng.chance(50),
                bottom: rng.chance(50),
            };
            let min = Size::new(rng.range(1, 600), rng.range(1, 600));
            let max = if rng.chance(50) {
                Some(Size::new(rng.range(1, 2000), rng.range(1, 2000)))
            } else {
                None
            };
            let constraints = SizeConstraints { min, max };
            let r = snap_resize(frame, edges, &env, &c, &constraints);
            let out = r.rect;

            let sides = [
                (edges.left, frame.left, out.left),
                (edges.top, frame.top, out.top),
                (edges.right, frame.right, out.right),
                (edges.bottom, frame.bottom, out.bottom),
            ];
            for (active, before, after) in sides {
                if active {
                    assert!((after - before).abs() <= c.threshold);
                } else {
                    assert_eq!(after, before, "{frame:?} {edges:?} {c:?} -> {out:?}");
                }
            }
            // A changed axis always ends up within the constraints.
            let (min_w, max_w) = constraints.width_range();
            let (min_h, max_h) = constraints.height_range();
            if out.width() != frame.width() {
                assert!((min_w..=max_w).contains(&out.width()));
            }
            if out.height() != frame.height() {
                assert!((min_h..=max_h).contains(&out.height()));
            }
            // Guides: never centre lines, at most one per active edge, each on a dragged edge.
            let active = [edges.left, edges.top, edges.right, edges.bottom]
                .iter()
                .filter(|&&a| a)
                .count();
            assert!(r.guides.len() <= active);
            for g in &r.guides {
                assert_ne!(g.source, SnapSource::Center);
                match g.orientation {
                    Orientation::Vertical => {
                        assert!(
                            (edges.left && g.position == out.left)
                                || (edges.right && g.position == out.right)
                        );
                        assert!(g.start <= out.top && g.end >= out.bottom);
                    }
                    Orientation::Horizontal => {
                        assert!(
                            (edges.top && g.position == out.top)
                                || (edges.bottom && g.position == out.bottom)
                        );
                        assert!(g.start <= out.left && g.end >= out.right);
                    }
                }
            }
        }
    }
}
