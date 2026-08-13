import { describe, expect, it } from "vitest";
import type { DeleteInterval } from "../types/contracts";
import { DeletionPlaybackGuard } from "./DeletionPlaybackGuard";

const intervals: DeleteInterval[] = [
  { id: 1, startUs: 5_000_000, endUs: 10_000_000 },
  { id: 2, startUs: 15_000_000, endUs: 18_000_000 }
];

describe("DeletionPlaybackGuard", () => {
  it("requests a skip when continuous playback enters a deletion interval", () => {
    const guard = new DeletionPlaybackGuard();
    guard.setManualPosition(4_000_000, intervals);

    expect(guard.observePlayback(5_100_000, true, intervals)).toEqual(intervals[0]);
  });

  it("detects an interval whose start and end were crossed in one update", () => {
    const guard = new DeletionPlaybackGuard();
    guard.setAutomaticPosition(4_000_000);

    expect(guard.observePlayback(11_000_000, true, intervals)).toEqual(intervals[0]);
  });

  it("allows a manually selected deletion interval and skips the next one", () => {
    const guard = new DeletionPlaybackGuard();
    guard.setManualPosition(6_000_000, intervals);

    expect(guard.observePlayback(9_000_000, true, intervals)).toBeNull();
    expect(guard.observePlayback(10_000_000, true, intervals)).toBeNull();
    expect(guard.observePlayback(15_100_000, true, intervals)).toEqual(intervals[1]);
  });

  it("restores a manual exemption after an old playback observation", () => {
    const guard = new DeletionPlaybackGuard();
    guard.setAutomaticPosition(20_000_000);
    guard.setManualPosition(6_000_000, intervals);
    expect(guard.observePlayback(20_100_000, true, intervals)).toEqual(intervals[1]);

    guard.setManualPosition(6_000_000, intervals);
    expect(guard.observePlayback(6_100_000, true, intervals)).toBeNull();
  });

  it("treats the start as inside and the end as outside for manual positioning", () => {
    const atStart = new DeletionPlaybackGuard();
    atStart.setManualPosition(5_000_000, intervals);
    expect(atStart.observePlayback(4_999_000, true, intervals)).toBeNull();
    expect(atStart.observePlayback(6_000_000, true, intervals)).toBeNull();

    const atEnd = new DeletionPlaybackGuard();
    atEnd.setManualPosition(10_000_000, intervals);
    expect(atEnd.observePlayback(15_100_000, true, intervals)).toEqual(intervals[1]);
  });

  it("does not request skips while playback is paused", () => {
    const guard = new DeletionPlaybackGuard();
    guard.setAutomaticPosition(4_000_000);

    expect(guard.observePlayback(6_000_000, false, intervals)).toBeNull();
  });
});
