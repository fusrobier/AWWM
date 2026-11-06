import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useDevicePixelRatio } from '../hooks/useDevicePixelRatio';
import { useAppliedTheme } from '../hooks/useDocumentTheme';
import { cx } from '../lib/format';
import {
  alignedSpan,
  fillCoverage,
  GRID_OPACITY,
  gridCoverage,
  gridLinePosition,
  gridPreviewCaption,
  parseHexColor,
} from '../lib/pixelGrid';

interface GridPreviewProps {
  /** Grid spacing in logical (CSS) pixels. */
  gridSize: number;
  /** Line colour, `#RRGGBB`. */
  color: string;
  /** Shown dimmed, like other disabled content. */
  disabled?: boolean;
}

interface CanvasBox {
  cssWidth: number;
  cssHeight: number;
  /** Exact backing-store size when the browser reports the device-pixel content box. */
  deviceWidth: number | null;
  deviceHeight: number | null;
}

/** Mock window placement, in logical pixels. */
const WINDOW_MARGIN = 12;
const WINDOW_WIDTH = 240;
const WINDOW_HEIGHT = 100;
const TITLE_BAR_HEIGHT = 24;

function token(styles: CSSStyleDeclaration, name: string): string | null {
  const value = styles.getPropertyValue(name).trim();
  return value === '' ? null : value;
}

/** A window whose edges lie on grid lines, drawn above the grid like a dragged window. */
function paintWindow(
  context: CanvasRenderingContext2D,
  styles: CSSStyleDeclaration,
  box: CanvasBox,
  gridSize: number,
  scale: number,
): void {
  const columns = alignedSpan(box.cssWidth, gridSize, WINDOW_WIDTH, WINDOW_MARGIN);
  const rows = alignedSpan(box.cssHeight, gridSize, WINDOW_HEIGHT, WINDOW_MARGIN);
  const fill = token(styles, '--bg-card');
  const titleFill = token(styles, '--bg-card-secondary');
  const border = token(styles, '--kbd-border');
  if (!columns || !rows || !fill || !titleFill || !border) return;

  // Edges are exclusive, like a window rectangle: the line at `right`/`bottom` stays visible.
  const left = gridLinePosition(columns.first, gridSize, scale);
  const right = gridLinePosition(columns.last, gridSize, scale);
  const top = gridLinePosition(rows.first, gridSize, scale);
  const bottom = gridLinePosition(rows.last, gridSize, scale);
  const width = right - left;
  const height = bottom - top;
  if (width < 3 || height < 3) return;

  context.fillStyle = fill;
  context.fillRect(left, top, width, height);
  const titleHeight = Math.round(TITLE_BAR_HEIGHT * scale);
  if (height >= titleHeight * 2) {
    context.fillStyle = titleFill;
    context.fillRect(left, top, width, titleHeight);
  }
  context.fillStyle = border;
  context.fillRect(left, top, width, 1);
  context.fillRect(left, bottom - 1, width, 1);
  context.fillRect(left, top + 1, 1, height - 2);
  context.fillRect(right - 1, top + 1, 1, height - 2);
  if (height >= titleHeight * 2) context.fillRect(left + 1, top + titleHeight, width - 2, 1);
}

function paint(canvas: HTMLCanvasElement, box: CanvasBox, gridSize: number, color: string, scale: number): void {
  const width = box.deviceWidth ?? Math.round(box.cssWidth * scale);
  const height = box.deviceHeight ?? Math.round(box.cssHeight * scale);
  if (width <= 0 || height <= 0) return;
  // Resizing the backing store also clears it.
  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;
  const context = canvas.getContext('2d');
  if (!context) return;
  context.clearRect(0, 0, width, height);

  // The neutral surface is the canvas's CSS background; the grid layer is painted over it
  // with uniform opacity, exactly one device pixel per line.
  const rgb = parseHexColor(color);
  if (rgb) {
    const image = context.createImageData(width, height);
    fillCoverage(image.data, gridCoverage(width, height, gridSize, scale), rgb, GRID_OPACITY);
    context.putImageData(image, 0, 0);
  }
  paintWindow(context, getComputedStyle(canvas), box, gridSize, scale);
}

/**
 * The pixel grid at its true size (1 CSS pixel = 1 logical pixel): line k at device pixel
 * round(k × gridSize × devicePixelRatio), every fifth line solid, the others dotted, in
 * `color` at the grid's opacity, as the engine draws it while a window is dragged.
 */
export function GridPreview({ gridSize, color, disabled = false }: GridPreviewProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [box, setBox] = useState<CanvasBox | null>(null);
  const scale = useDevicePixelRatio();
  const theme = useAppliedTheme();

  useLayoutEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return undefined;
    const update = (next: CanvasBox): void => {
      setBox((current) =>
        current &&
        current.cssWidth === next.cssWidth &&
        current.cssHeight === next.cssHeight &&
        current.deviceWidth === next.deviceWidth &&
        current.deviceHeight === next.deviceHeight
          ? current
          : next,
      );
    };
    // Paint right away; the observer then reports the exact device-pixel size and changes.
    // The canvas has no border or padding, so its bounding box is its content box.
    const initial = canvas.getBoundingClientRect();
    update({ cssWidth: initial.width, cssHeight: initial.height, deviceWidth: null, deviceHeight: null });
    const observer = new ResizeObserver((entries) => {
      const entry = entries[entries.length - 1];
      if (!entry) return;
      const device = entry.devicePixelContentBoxSize?.[0];
      update({
        cssWidth: entry.contentRect.width,
        cssHeight: entry.contentRect.height,
        deviceWidth: device ? device.inlineSize : null,
        deviceHeight: device ? device.blockSize : null,
      });
    });
    try {
      observer.observe(canvas, { box: 'device-pixel-content-box' });
    } catch {
      observer.observe(canvas);
    }
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas && box) paint(canvas, box, gridSize, color, scale);
  }, [box, gridSize, color, scale, theme]);

  return (
    <figure className={cx('grid-preview', disabled && 'is-disabled')}>
      {/* Decorative for screen readers: the caption states the same facts once. */}
      <canvas ref={canvasRef} className="grid-preview-canvas" aria-hidden="true" />
      <figcaption className="grid-preview-caption">{gridPreviewCaption(gridSize)}</figcaption>
    </figure>
  );
}
