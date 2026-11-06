//! The visible pixel grid shown during gestures.
//!
//! The grid has a spacing of `size` logical pixels and is anchored at the top-left corner
//! of each monitor. On a monitor with an effective DPI of `dpi`, line `k` lies at the
//! physical coordinate `origin + round(k * size * dpi / 96)` (halves round up). Rounding
//! each line from its index (instead of accumulating a rounded step) keeps the grid exact
//! in logical pixels at fractional scale factors such as 125 %, and every line falls on a
//! whole physical pixel. The same positions are used for drawing, for snapping and for the
//! position read-out, so what is drawn is exactly what a window snaps to.

use crate::geometry::BASE_DPI;

/// Every `MAJOR_EVERY`-th line, counted from the monitor's top-left corner, is a major
/// line (drawn more prominently).
pub const MAJOR_EVERY: u32 = 5;

/// One grid line on one axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridLine {
    /// Physical `x` of a vertical line or `y` of a horizontal line.
    pub position: i32,
    /// Index of the line, counted from the monitor origin (0 = the origin itself).
    pub index: u32,
    /// Every [`MAJOR_EVERY`]-th line.
    pub major: bool,
}

/// Size and DPI as used by the arithmetic, clamped so `2 * index * size * dpi` always fits
/// in an `i64` (callers sanitize far below these bounds anyway).
fn effective(size: u32, dpi: u32) -> (i64, i64) {
    (
        i64::from(size.clamp(1, 10_000)),
        i64::from(dpi.clamp(1, 10_000)),
    )
}

/// Physical position of line `index` for a grid anchored at `origin`.
#[must_use]
pub fn line_position(origin: i32, index: u32, size: u32, dpi: u32) -> i32 {
    let (size, dpi) = effective(size, dpi);
    let base = i64::from(BASE_DPI);
    // floor(k * size * dpi / 96 + 1/2) for non-negative k.
    let offset = (2 * i64::from(index) * size * dpi + base) / (2 * base);
    i32::try_from(i64::from(origin) + offset).unwrap_or(i32::MAX)
}

/// Upper bound on the number of lines per axis (a 16K-wide monitor at the smallest grid
/// size needs about 4 000), so a bogus monitor rectangle can never allocate unbounded memory.
pub const MAX_LINES: usize = 16_384;

/// All lines in `[origin, origin + extent)`, in increasing order (at most [`MAX_LINES`]).
#[must_use]
pub fn lines(origin: i32, extent: i32, size: u32, dpi: u32) -> Vec<GridLine> {
    let end = i64::from(origin) + i64::from(extent.max(0));
    let mut out = Vec::new();
    for index in 0u32.. {
        let position = line_position(origin, index, size, dpi);
        if i64::from(position) >= end || position == i32::MAX || out.len() >= MAX_LINES {
            break;
        }
        out.push(GridLine {
            position,
            index,
            major: index % MAJOR_EVERY == 0,
        });
    }
    out
}

/// The (at most two) lines that bracket `feature` on a grid anchored at `origin` and
/// limited to `[origin, origin + extent]`: the last line at or before it and the first
/// line after it. One of them is always the nearest line to `feature`. A feature before
/// the origin yields the origin line; one past the end yields the last line.
#[must_use]
pub fn bracketing_lines(origin: i32, extent: i32, feature: i32, size: u32, dpi: u32) -> Vec<i32> {
    let (s, d) = effective(size, dpi);
    let end = i64::from(origin) + i64::from(extent.max(0));
    let relative = i64::from(feature) - i64::from(origin);
    // floor(relative * 96 / (size * dpi)); lines k and k + 1 bracket the feature because
    // round(x) <= x + 1/2 rounds no line index past it (see the module docs).
    let k = (relative * i64::from(BASE_DPI)).div_euclid(s * d).max(0);
    let Ok(k) = u32::try_from(k) else {
        return Vec::new();
    };
    let mut out: Vec<i32> = [k, k.saturating_add(1)]
        .into_iter()
        .map(|i| line_position(origin, i, size, dpi))
        .filter(|&p| i64::from(p) <= end)
        .collect();
    if out.is_empty() && extent >= 0 {
        // Past the end: the last line at or before the end, computed directly (this runs on
        // every mouse move while an edge is beyond the monitor, so no full line list).
        let estimate = (i64::from(extent) * i64::from(BASE_DPI)).div_euclid(s * d);
        let mut last = u32::try_from(estimate).unwrap_or(u32::MAX);
        let at = |i: u32| i64::from(line_position(origin, i, size, dpi));
        if last < u32::MAX && at(last + 1) <= end {
            last += 1;
        }
        while last > 0 && at(last) > end {
            last -= 1;
        }
        out.push(line_position(origin, last, size, dpi));
    }
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_at_100_percent_are_multiples_of_the_size() {
        assert_eq!(line_position(0, 0, 20, 96), 0);
        assert_eq!(line_position(0, 1, 20, 96), 20);
        assert_eq!(line_position(0, 7, 20, 96), 140);
        assert_eq!(line_position(-2560, 3, 20, 96), -2500);
    }

    #[test]
    fn positions_scale_with_dpi_and_round_per_line() {
        // 150 %: 20 logical px = 30 physical px.
        assert_eq!(line_position(0, 1, 20, 144), 30);
        assert_eq!(line_position(0, 3, 20, 144), 90);
        // 125 %: 10 logical px = 12.5 physical px; each line rounds on its own, so the
        // grid never drifts: 12.5 -> 13, 25, 37.5 -> 38, 50.
        let p: Vec<i32> = (0..5).map(|k| line_position(0, k, 10, 120)).collect();
        assert_eq!(p, vec![0, 13, 25, 38, 50]);
        // 200 %.
        assert_eq!(line_position(100, 2, 8, 192), 132);
        // 175 %: 4 logical px = 7 physical px.
        assert_eq!(line_position(0, 10, 4, 168), 70);
    }

    #[test]
    fn lines_cover_the_extent_and_mark_every_fifth_line_major() {
        let l = lines(1920, 1920, 20, 96);
        assert_eq!(l.len(), 96);
        assert_eq!(l[0].position, 1920);
        assert_eq!(l.last().map(|g| g.position), Some(1920 + 95 * 20));
        assert!(l.iter().all(|g| g.position >= 1920 && g.position < 3840));
        let majors: Vec<u32> = l.iter().filter(|g| g.major).map(|g| g.index).collect();
        assert_eq!(&majors[..4], &[0, 5, 10, 15]);
        assert!(lines(0, 0, 20, 96).is_empty());
        assert!(lines(0, -5, 20, 96).is_empty());
    }

    #[test]
    fn lines_on_a_scaled_monitor_with_negative_origin() {
        // A 150 % monitor left of the primary one.
        let l = lines(-2880, 2880, 20, 144);
        assert_eq!(l.len(), 96);
        assert_eq!(l[1].position, -2850);
        assert_eq!(l.last().map(|g| g.position), Some(-30));
    }

    #[test]
    fn degenerate_sizes_do_not_panic() {
        assert_eq!(line_position(0, 3, 0, 0), 0);
        assert!(!lines(0, 100, 0, 96).is_empty());
        assert_eq!(line_position(i32::MAX - 1, 1_000, 200, 192), i32::MAX);
    }

    #[test]
    fn bracketing_lines_contain_the_nearest_line() {
        for dpi in [96, 120, 144, 168, 192] {
            for size in [4, 7, 10, 20, 33, 200] {
                // Bracketing includes a line exactly at the end of the extent.
                let all = lines(-300, 2001, size, dpi);
                for feature in (-300..1700).step_by(7) {
                    let nearest = all
                        .iter()
                        .map(|g| g.position)
                        .min_by_key(|p| (p - feature).abs())
                        .expect("lines");
                    let b = bracketing_lines(-300, 2000, feature, size, dpi);
                    let best = b
                        .iter()
                        .map(|p| (p - feature).abs())
                        .min()
                        .expect("bracket");
                    assert_eq!(
                        best,
                        (nearest - feature).abs(),
                        "dpi {dpi} size {size} feature {feature}: {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_last_line_past_the_end_matches_the_full_line_list() {
        for dpi in [96, 120, 144, 168, 192] {
            for size in [4, 7, 10, 20, 33, 200] {
                for extent in [0, 1, 999, 1000, 1001, 1920, 2559] {
                    let expected = lines(-40, extent + 1, size, dpi).last().map(|l| l.position);
                    let got = bracketing_lines(-40, extent, i32::MAX / 2, size, dpi);
                    assert_eq!(
                        got.first().copied(),
                        expected,
                        "dpi {dpi} size {size} extent {extent}"
                    );
                }
            }
        }
    }

    #[test]
    fn huge_inputs_do_not_overflow() {
        assert!(line_position(0, u32::MAX, u32::MAX, u32::MAX) > 0);
        assert_eq!(
            bracketing_lines(0, i32::MAX, i32::MAX, u32::MAX, u32::MAX).len(),
            2
        );
    }

    #[test]
    fn bracketing_lines_at_the_edges() {
        assert_eq!(bracketing_lines(0, 1000, -50, 20, 96), vec![0, 20]);
        assert_eq!(bracketing_lines(0, 1000, 40, 20, 96), vec![40, 60]);
        // The right edge of the monitor is a line position too (1000 = 50 * 20).
        assert_eq!(bracketing_lines(0, 1000, 995, 20, 96), vec![980, 1000]);
        assert_eq!(bracketing_lines(0, 1000, 5000, 20, 96), vec![1000]);
    }
}
