import { formatTime } from "../utils/time";

export interface TimelineViewport {
  startUs: number;
  spanUs: number;
}

export interface TimelineBounds {
  durationUs: number;
  frameDurationUs: number;
}

export interface TimelineTick {
  timeUs: number;
  major: boolean;
  label: string | null;
}

const TIME_TICK_CANDIDATES_US = [
  100_000,
  250_000,
  500_000,
  1_000_000,
  2_000_000,
  5_000_000,
  10_000_000,
  15_000_000,
  30_000_000,
  60_000_000,
  120_000_000,
  300_000_000,
  600_000_000,
  900_000_000,
  1_800_000_000,
  3_600_000_000
];

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}

export function minimumSpanUs(bounds: TimelineBounds): number {
  if (bounds.durationUs <= 0) return 0;
  return Math.min(bounds.durationUs, Math.max(1, bounds.frameDurationUs) * 20);
}

export function clampViewport(viewport: TimelineViewport, bounds: TimelineBounds): TimelineViewport {
  if (bounds.durationUs <= 0) return { startUs: 0, spanUs: 0 };
  const spanUs = clamp(viewport.spanUs, minimumSpanUs(bounds), bounds.durationUs);
  const startUs = clamp(viewport.startUs, 0, Math.max(0, bounds.durationUs - spanUs));
  return { startUs, spanUs };
}

export function timeAtClientX(
  clientX: number,
  rectLeft: number,
  rectWidth: number,
  viewport: TimelineViewport
): number {
  if (rectWidth <= 0 || viewport.spanUs <= 0) return viewport.startUs;
  const ratio = clamp((clientX - rectLeft) / rectWidth, 0, 1);
  return viewport.startUs + ratio * viewport.spanUs;
}

export function xAtTime(
  timeUs: number,
  rectLeft: number,
  rectWidth: number,
  viewport: TimelineViewport
): number {
  if (rectWidth <= 0 || viewport.spanUs <= 0) return rectLeft;
  return rectLeft + ((timeUs - viewport.startUs) / viewport.spanUs) * rectWidth;
}

export function snapToFrame(timeUs: number, bounds: TimelineBounds): number {
  if (bounds.durationUs <= 0) return 0;
  const frameDurationUs = Math.max(1, bounds.frameDurationUs);
  return clamp(Math.round(timeUs / frameDurationUs) * frameDurationUs, 0, bounds.durationUs);
}

export function zoomAtAnchor(
  viewport: TimelineViewport,
  normalizedDelta: number,
  anchorRatio: number,
  bounds: TimelineBounds
): TimelineViewport {
  if (bounds.durationUs <= 0) return { startUs: 0, spanUs: 0 };
  const current = clampViewport(viewport, bounds);
  const ratio = clamp(anchorRatio, 0, 1);
  const anchorUs = current.startUs + ratio * current.spanUs;
  const scale = Math.exp(clamp(normalizedDelta, -2_000, 2_000) * 0.0015);
  const spanUs = clamp(current.spanUs * scale, minimumSpanUs(bounds), bounds.durationUs);
  return clampViewport({ startUs: anchorUs - ratio * spanUs, spanUs }, bounds);
}

export function zoomToSpan(
  viewport: TimelineViewport,
  spanUs: number,
  anchorUs: number,
  bounds: TimelineBounds
): TimelineViewport {
  const current = clampViewport(viewport, bounds);
  const ratio = current.spanUs > 0 ? clamp((anchorUs - current.startUs) / current.spanUs, 0, 1) : 0.5;
  const nextSpanUs = clamp(spanUs, minimumSpanUs(bounds), bounds.durationUs);
  return clampViewport({ startUs: anchorUs - ratio * nextSpanUs, spanUs: nextSpanUs }, bounds);
}

export function panViewport(
  viewport: TimelineViewport,
  deltaUs: number,
  bounds: TimelineBounds
): TimelineViewport {
  return clampViewport({ ...viewport, startUs: viewport.startUs + deltaUs }, bounds);
}

export function spanFromSlider(value: number, bounds: TimelineBounds): number {
  if (bounds.durationUs <= 0) return 0;
  const minimum = minimumSpanUs(bounds);
  if (minimum >= bounds.durationUs) return bounds.durationUs;
  return minimum * Math.pow(bounds.durationUs / minimum, 1 - clamp(value, 0, 1));
}

export function sliderFromViewport(viewport: TimelineViewport, bounds: TimelineBounds): number {
  if (bounds.durationUs <= 0) return 0;
  const minimum = minimumSpanUs(bounds);
  if (minimum >= bounds.durationUs) return 0;
  const spanUs = clamp(viewport.spanUs, minimum, bounds.durationUs);
  return clamp(1 - Math.log(spanUs / minimum) / Math.log(bounds.durationUs / minimum), 0, 1);
}

export function buildTimelineTicks(
  viewport: TimelineViewport,
  widthPx: number,
  bounds: TimelineBounds
): TimelineTick[] {
  const current = clampViewport(viewport, bounds);
  if (current.spanUs <= 0 || widthPx <= 0) return [];

  const frameDurationUs = Math.max(1, bounds.frameDurationUs);
  const candidates = [
    frameDurationUs,
    frameDurationUs * 2,
    frameDurationUs * 5,
    frameDurationUs * 10,
    ...TIME_TICK_CANDIDATES_US
  ].filter((value, index, values) => value > 0 && values.indexOf(value) === index).sort((a, b) => a - b);

  const majorTargetUs = (current.spanUs * 90) / widthPx;
  const majorStepUs = candidates.find((candidate) => candidate >= majorTargetUs) ?? candidates.at(-1) ?? current.spanUs;
  const minorStepUs = majorStepUs >= frameDurationUs * 5 ? majorStepUs / 5 : majorStepUs;
  const firstTickUs = Math.ceil(current.startUs / minorStepUs) * minorStepUs;
  const endUs = current.startUs + current.spanUs;
  const ticks: TimelineTick[] = [];

  for (let timeUs = firstTickUs; timeUs <= endUs + minorStepUs * 0.001 && ticks.length < 200; timeUs += minorStepUs) {
    const majorIndex = Math.round(timeUs / majorStepUs);
    const major = Math.abs(timeUs - majorIndex * majorStepUs) < Math.max(1, frameDurationUs / 10);
    ticks.push({
      timeUs,
      major,
      label: major ? formatTime(timeUs, current.spanUs < 60_000_000) : null
    });
  }
  return ticks;
}
