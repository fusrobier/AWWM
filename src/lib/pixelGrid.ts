// Geometry of the visible pixel grid, used by the settings preview. Mirrors the engine
// (crates/awwm-core/src/pixel_grid.rs): lines every `gridSize` logical pixels, anchored at
// the top-left corner of the surface, with line k at the device pixel
// `round(k * gridSize * scale)` (scale = DPI / 96 = devicePixelRatio, halves round up).
// Every MAJOR_LINE_INTERVAL-th line is solid, the others are dotted on every other device
// pixel (where x + y is even), and the whole grid is drawn in one colour at GRID_OPACITY.
import { formatPixels } from './format';

/** Grid spacing limits in logical pixels (the backend clamps to the same range). */
export const GRID_SIZE_MIN = 4;
export const GRID_SIZE_MAX = 200;

/** Every fifth line, counted from the origin (line 0), is a major (solid) line. */
export const MAJOR_LINE_INTERVAL = 5;

/** Opacity of the grid colour over whatever lies below it. */
export const GRID_OPACITY = 96 / 255; // the engine's GRID_ALPHA (about 38 %)

export interface GridLine {
  /** Line number counted from the origin (0 = the origin itself). */
  index: number;
  /** Offset from the origin in device pixels. */
  position: number;
  /** Major lines are solid, minor lines are dotted. */
  major: boolean;
}

/** Device-pixel offset of line `index` from the origin. */
export function gridLinePosition(index: number, gridSize: number, scale: number): number {
  return Math.round(index * gridSize * scale);
}

export function isMajorLine(index: number): boolean {
  return index % MAJOR_LINE_INTERVAL === 0;
}

/** Every line whose device-pixel position lies in `[0, extent)`, in increasing order. */
export function gridLines(extent: number, gridSize: number, scale: number): GridLine[] {
  const lines: GridLine[] = [];
  if (!(extent > 0) || !(gridSize > 0) || !(scale > 0)) return lines;
  for (let index = 0; ; index += 1) {
    const position = gridLinePosition(index, gridSize, scale);
    if (position >= extent) break;
    lines.push({ index, position, major: isMajorLine(index) });
  }
  return lines;
}

/**
 * Pixels covered by the grid on a `width` × `height` device-pixel surface whose top-left
 * corner is the grid origin: 1 where a line is drawn, 0 elsewhere (row-major). Minor lines
 * are dotted like the engine's checkerboard brush: they cover the pixels whose x + y is
 * even. A pixel where lines cross is covered once, so crossings are not darker.
 */
export function gridCoverage(width: number, height: number, gridSize: number, scale: number): Uint8Array {
  const w = Math.max(0, Math.floor(width));
  const h = Math.max(0, Math.floor(height));
  const coverage = new Uint8Array(w * h);
  for (const line of gridLines(w, gridSize, scale)) {
    const step = line.major ? 1 : 2;
    for (let y = line.major ? 0 : line.position % 2; y < h; y += step) coverage[y * w + line.position] = 1;
  }
  for (const line of gridLines(h, gridSize, scale)) {
    const step = line.major ? 1 : 2;
    const row = line.position * w;
    for (let x = line.major ? 0 : line.position % 2; x < w; x += step) coverage[row + x] = 1;
  }
  return coverage;
}

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

/** Parses `#RRGGBB` (the backend's colour format); null for anything else. */
export function parseHexColor(value: string): Rgb | null {
  const match = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(value.trim());
  if (!match) return null;
  const [, r = '0', g = '0', b = '0'] = match;
  return { r: Number.parseInt(r, 16), g: Number.parseInt(g, 16), b: Number.parseInt(b, 16) };
}

/**
 * Writes the grid layer into `pixels` (non-premultiplied RGBA, as in ImageData): every
 * covered pixel becomes `color` at `opacity`; other pixels are left as they are
 * (transparent in a fresh ImageData), so the surface below shows through.
 */
export function fillCoverage(pixels: Uint8ClampedArray, coverage: Uint8Array, color: Rgb, opacity: number): void {
  const alpha = Math.round(Math.min(1, Math.max(0, opacity)) * 255);
  const count = Math.min(coverage.length, Math.floor(pixels.length / 4));
  for (let i = 0; i < count; i += 1) {
    if (coverage[i] !== 1) continue;
    const o = i * 4;
    pixels[o] = color.r;
    pixels[o + 1] = color.g;
    pixels[o + 2] = color.b;
    pixels[o + 3] = alpha;
  }
}

export interface LineSpan {
  /** Index of the grid line where the span starts. */
  first: number;
  /** Index of the grid line where the span ends (> first). */
  last: number;
}

/**
 * A span between two grid lines on an axis of `extent` logical pixels that keeps at least
 * `margin` logical pixels from both ends (never starting at the origin line) and is about
 * `target` logical pixels long. Null when not even one grid cell fits.
 */
export function alignedSpan(extent: number, gridSize: number, target: number, margin: number): LineSpan | null {
  if (!(gridSize > 0) || !(extent > 0)) return null;
  const first = Math.max(1, Math.ceil(margin / gridSize));
  const lastAllowed = Math.floor((extent - margin) / gridSize);
  if (lastAllowed <= first) return null;
  const cells = Math.min(lastAllowed - first, Math.max(1, Math.round(target / gridSize)));
  return { first, last: first + cells };
}

/** Caption under the grid preview; it also names the figure for screen readers. */
export function gridPreviewCaption(gridSize: number): string {
  return `Actual size: lines every ${formatPixels(gridSize)}, a solid line every ${formatPixels(gridSize * MAJOR_LINE_INTERVAL)}.`;
}
