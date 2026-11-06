import { describe, expect, it } from 'vitest';
import {
  alignedSpan,
  fillCoverage,
  GRID_OPACITY,
  GRID_SIZE_MAX,
  GRID_SIZE_MIN,
  gridCoverage,
  gridLinePosition,
  gridLines,
  gridPreviewCaption,
  isMajorLine,
  MAJOR_LINE_INTERVAL,
  parseHexColor,
} from './pixelGrid';

const positions = (count: number, gridSize: number, scale: number): number[] =>
  Array.from({ length: count }, (_, index) => gridLinePosition(index, gridSize, scale));

/** Renders a coverage mask as rows of `#` (covered) and `.` (not covered). */
const picture = (coverage: Uint8Array, width: number): string[] => {
  const rows: string[] = [];
  for (let start = 0; start < coverage.length; start += width) {
    rows.push(Array.from(coverage.subarray(start, start + width), (value) => (value ? '#' : '.')).join(''));
  }
  return rows;
};

describe('grid line positions', () => {
  it('are multiples of the grid size at 100 %', () => {
    expect(positions(5, 20, 1)).toEqual([0, 20, 40, 60, 80]);
    expect(gridLinePosition(7, 20, 1)).toBe(140);
  });

  it('round each line on its own at 125 % so the grid never drifts', () => {
    // 10 logical px = 12.5 device px: 12.5 -> 13, 25, 37.5 -> 38, 50 (same as the engine).
    expect(positions(5, 10, 1.25)).toEqual([0, 13, 25, 38, 50]);
    expect(positions(4, 20, 1.25)).toEqual([0, 25, 50, 75]);
  });

  it('scale with 150 % and round halves up', () => {
    expect(positions(4, 20, 1.5)).toEqual([0, 30, 60, 90]);
    // 5 logical px = 7.5 device px.
    expect(positions(5, 5, 1.5)).toEqual([0, 8, 15, 23, 30]);
  });

  it('double at 200 %', () => {
    expect(positions(4, 8, 2)).toEqual([0, 16, 32, 48]);
    expect(gridLinePosition(5, 200, 2)).toBe(2000);
  });

  it('stay whole device pixels with steps of floor or ceil of the scaled size', () => {
    for (const scale of [1, 1.25, 1.5, 1.75, 2]) {
      const lines = positions(50, 7, scale);
      for (let i = 1; i < lines.length; i += 1) {
        const step = (lines[i] ?? 0) - (lines[i - 1] ?? 0);
        expect(Number.isInteger(lines[i])).toBe(true);
        expect([Math.floor(7 * scale), Math.ceil(7 * scale)]).toContain(step);
      }
    }
  });
});

describe('gridLines', () => {
  it('lists the lines inside the extent and marks every fifth one major', () => {
    expect(MAJOR_LINE_INTERVAL).toBe(5);
    expect(gridLines(100, 20, 1)).toEqual([
      { index: 0, position: 0, major: true },
      { index: 1, position: 20, major: false },
      { index: 2, position: 40, major: false },
      { index: 3, position: 60, major: false },
      { index: 4, position: 80, major: false },
    ]);
    const lines = gridLines(1920, 20, 1);
    expect(lines).toHaveLength(96);
    expect(lines.filter((line) => line.major).map((line) => line.index).slice(0, 4)).toEqual([0, 5, 10, 15]);
    expect(isMajorLine(0)).toBe(true);
    expect(isMajorLine(10)).toBe(true);
    expect(isMajorLine(7)).toBe(false);
  });

  it('places the major lines at the scaled major interval', () => {
    const majors = (scale: number): number[] =>
      gridLines(Math.round(600 * scale), 20, scale)
        .filter((line) => line.major)
        .map((line) => line.position);
    expect(majors(1)).toEqual([0, 100, 200, 300, 400, 500]);
    expect(majors(1.25)).toEqual([0, 125, 250, 375, 500, 625]);
    expect(majors(1.5)).toEqual([0, 150, 300, 450, 600, 750]);
    expect(majors(2)).toEqual([0, 200, 400, 600, 800, 1000]);
  });

  it('returns nothing for empty extents or invalid sizes', () => {
    expect(gridLines(0, 20, 1)).toEqual([]);
    expect(gridLines(-5, 20, 1)).toEqual([]);
    expect(gridLines(100, 0, 1)).toEqual([]);
    expect(gridLines(100, 20, 0)).toEqual([]);
    expect(gridLines(100, Number.NaN, 1)).toEqual([]);
  });

  it('accepts the backend range of grid sizes', () => {
    expect(GRID_SIZE_MIN).toBe(4);
    expect(GRID_SIZE_MAX).toBe(200);
  });
});

describe('gridCoverage', () => {
  it('draws major lines solid and minor lines on every other device pixel', () => {
    // Lines every 2 px: x/y = 0 and 10 are major (index 0 and 5), the others minor.
    expect(picture(gridCoverage(12, 4, 2, 1), 12)).toEqual([
      '############',
      '#.........#.',
      '#.#.#.#.#.#.',
      '#.........#.',
    ]);
  });

  it('uses the scaled positions', () => {
    // 150 %: lines at 0, 3 (minor), 6 (minor); a horizontal major line at y = 0. Minor
    // lines are dotted where x + y is even, like the engine's checkerboard brush.
    expect(picture(gridCoverage(8, 3, 2, 1.5), 8)).toEqual(['########', '#..#....', '#.....#.']);
  });

  it('covers each pixel at most once', () => {
    const coverage = gridCoverage(50, 50, 4, 1.25);
    expect(coverage.every((value) => value === 0 || value === 1)).toBe(true);
    expect(gridCoverage(0, 10, 4, 1)).toHaveLength(0);
  });
});

describe('fillCoverage', () => {
  it('paints covered pixels in the grid colour at the grid opacity and leaves the rest', () => {
    const pixels = new Uint8ClampedArray(8);
    fillCoverage(pixels, Uint8Array.of(1, 0), { r: 59, g: 130, b: 246 }, GRID_OPACITY);
    expect(Array.from(pixels)).toEqual([59, 130, 246, 96, 0, 0, 0, 0]);
  });

  it('parses only #RRGGBB colours', () => {
    expect(parseHexColor('#3B82F6')).toEqual({ r: 59, g: 130, b: 246 });
    expect(parseHexColor('#ff0000')).toEqual({ r: 255, g: 0, b: 0 });
    expect(parseHexColor('#FFF')).toBeNull();
    expect(parseHexColor('red')).toBeNull();
  });
});

describe('alignedSpan', () => {
  it('starts and ends on grid lines inside the margins', () => {
    expect(alignedSpan(600, 20, 240, 12)).toEqual({ first: 1, last: 13 });
    expect(alignedSpan(160, 20, 100, 12)).toEqual({ first: 1, last: 6 });
    expect(alignedSpan(160, 4, 100, 12)).toEqual({ first: 3, last: 28 });
    // Large grids: at least one cell, or nothing when no cell fits.
    expect(alignedSpan(160, 50, 100, 12)).toEqual({ first: 1, last: 2 });
    expect(alignedSpan(160, 80, 100, 12)).toBeNull();
    expect(alignedSpan(0, 20, 100, 12)).toBeNull();
  });
});

describe('grid preview text', () => {
  it('states the line spacing and the major-line interval', () => {
    expect(gridPreviewCaption(20)).toBe('Actual size: lines every 20 px, a solid line every 100 px.');
    expect(gridPreviewCaption(200)).toBe('Actual size: lines every 200 px, a solid line every 1000 px.');
  });
});
