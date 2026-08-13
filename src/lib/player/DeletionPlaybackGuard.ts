import type { DeleteInterval } from "../types/contracts";

function contains(interval: DeleteInterval, timeUs: number): boolean {
  return timeUs >= interval.startUs && timeUs < interval.endUs;
}

export class DeletionPlaybackGuard {
  private previousUs: number | null = null;
  private manualIntervalId: number | null = null;

  reset(positionUs: number | null = null): void {
    this.previousUs = positionUs;
    this.manualIntervalId = null;
  }

  setManualPosition(positionUs: number, intervals: DeleteInterval[]): void {
    this.previousUs = positionUs;
    this.manualIntervalId = intervals.find((interval) => contains(interval, positionUs))?.id ?? null;
  }

  setAutomaticPosition(positionUs: number): void {
    this.previousUs = positionUs;
    this.manualIntervalId = null;
  }

  observePlayback(positionUs: number, playing: boolean, intervals: DeleteInterval[]): DeleteInterval | null {
    if (!Number.isFinite(positionUs)) return null;

    const previousUs = this.previousUs;
    const manualInterval = intervals.find((interval) => interval.id === this.manualIntervalId);
    if (!manualInterval || positionUs >= manualInterval.endUs) this.manualIntervalId = null;

    if (!playing || (previousUs !== null && positionUs < previousUs)) {
      this.previousUs = positionUs;
      return null;
    }

    const interval = intervals.find((candidate) => {
      if (candidate.id === this.manualIntervalId) return false;
      if (contains(candidate, positionUs)) return true;
      return previousUs !== null && previousUs < candidate.startUs && positionUs >= candidate.startUs;
    }) ?? null;

    this.previousUs = positionUs;
    return interval;
  }
}
