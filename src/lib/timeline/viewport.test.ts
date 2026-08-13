import { describe, expect, it } from "vitest";
import {
  buildTimelineTicks,
  clampViewport,
  panViewport,
  sliderFromViewport,
  snapToFrame,
  spanFromSlider,
  timeAtClientX,
  xAtTime,
  zoomAtAnchor
} from "./viewport";

const bounds = { durationUs: 10_800_000_000, frameDurationUs: 33_367 };

describe("timeline viewport", () => {
  it("maps time and pixels in both directions and clamps outside the track", () => {
    const viewport = { startUs: 60_000_000, spanUs: 120_000_000 };
    expect(timeAtClientX(100, 100, 800, viewport)).toBe(60_000_000);
    expect(timeAtClientX(500, 100, 800, viewport)).toBe(120_000_000);
    expect(timeAtClientX(900, 100, 800, viewport)).toBe(180_000_000);
    expect(timeAtClientX(-50, 100, 800, viewport)).toBe(60_000_000);
    expect(timeAtClientX(2_000, 100, 800, viewport)).toBe(180_000_000);
    expect(timeAtClientX(300, 100, 0, viewport)).toBe(60_000_000);

    expect(xAtTime(60_000_000, 100, 800, viewport)).toBe(100);
    expect(xAtTime(120_000_000, 100, 800, viewport)).toBe(500);
    expect(xAtTime(180_000_000, 100, 800, viewport)).toBe(900);
  });

  it.each([
    [17.12, Math.round(1_000_000 / 17.12)],
    [29.97, Math.round(1_000_000 / 29.97)],
    [60, Math.round(1_000_000 / 60)]
  ])("snaps %.2f fps media to its source-frame grid", (_fps, frameDurationUs) => {
    const localBounds = { durationUs: 30_000_000, frameDurationUs };
    expect(snapToFrame(frameDurationUs * 4 + frameDurationUs * 0.49, localBounds)).toBe(frameDurationUs * 4);
    expect(snapToFrame(frameDurationUs * 4 + frameDurationUs * 0.51, localBounds)).toBe(frameDurationUs * 5);
    expect(snapToFrame(-100, localBounds)).toBe(0);
    expect(snapToFrame(31_000_000, localBounds)).toBe(30_000_000);
  });

  it("clamps spans and starts to valid media bounds", () => {
    expect(clampViewport({ startUs: -20, spanUs: 1 }, bounds)).toEqual({ startUs: 0, spanUs: bounds.frameDurationUs * 20 });
    expect(clampViewport({ startUs: bounds.durationUs, spanUs: 120_000_000 }, bounds)).toEqual({
      startUs: bounds.durationUs - 120_000_000,
      spanUs: 120_000_000
    });
    expect(clampViewport({ startUs: 50, spanUs: bounds.durationUs * 2 }, bounds)).toEqual({ startUs: 0, spanUs: bounds.durationUs });
  });

  it("zooms continuously without moving the time beneath the pointer", () => {
    const viewport = { startUs: 2_000_000_000, spanUs: 600_000_000 };
    const anchorRatio = 0.72;
    const anchorBefore = viewport.startUs + viewport.spanUs * anchorRatio;
    const zoomed = zoomAtAnchor(viewport, -360, anchorRatio, bounds);
    const anchorAfter = zoomed.startUs + zoomed.spanUs * anchorRatio;

    expect(zoomed.spanUs).toBeLessThan(viewport.spanUs);
    expect(Math.abs(anchorAfter - anchorBefore)).toBeLessThanOrEqual(bounds.frameDurationUs);
  });

  it("pans without crossing the start or end of the source", () => {
    const viewport = { startUs: 60_000_000, spanUs: 300_000_000 };
    expect(panViewport(viewport, -90_000_000, bounds).startUs).toBe(0);
    expect(panViewport(viewport, bounds.durationUs, bounds).startUs).toBe(bounds.durationUs - viewport.spanUs);
  });

  it("maps the logarithmic zoom slider to the full and minimum spans", () => {
    const minSpan = bounds.frameDurationUs * 20;
    expect(spanFromSlider(0, bounds)).toBe(bounds.durationUs);
    expect(spanFromSlider(1, bounds)).toBe(minSpan);
    expect(sliderFromViewport({ startUs: 0, spanUs: bounds.durationUs }, bounds)).toBeCloseTo(0);
    expect(sliderFromViewport({ startUs: 0, spanUs: minSpan }, bounds)).toBeCloseTo(1);
  });

  it("uses bounded coarse ticks for a three-hour view and frame ticks when zoomed in", () => {
    const overviewTicks = buildTimelineTicks({ startUs: 0, spanUs: bounds.durationUs }, 1_200, bounds);
    const frameTicks = buildTimelineTicks({ startUs: 0, spanUs: bounds.frameDurationUs * 30 }, 1_200, bounds);

    expect(overviewTicks.length).toBeLessThan(100);
    expect(overviewTicks.some((tick) => tick.label?.includes(":"))).toBe(true);
    expect(frameTicks.length).toBeGreaterThan(10);
    expect(frameTicks.length).toBeLessThan(100);
  });
});
