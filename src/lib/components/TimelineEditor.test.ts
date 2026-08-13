import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DeleteInterval } from "../types/contracts";
import TimelineEditor from "./TimelineEditor.svelte";

const intervals: DeleteInterval[] = [
  { id: 1, startUs: 50_000_000, endUs: 70_000_000 },
  { id: 2, startUs: 105_000_000, endUs: 112_000_000 }
];

const props = {
  durationUs: 180_000_000,
  frameDurationUs: 33_333,
  playheadUs: 60_000_000,
  intervals,
  selectedId: 1,
  pendingStartUs: 55_000_000,
  canUndo: true,
  canRedo: false,
  playing: false,
  editLocked: false,
  markSaving: false,
  waveform: { samplesPerSecond: 2, peaks: [0, 32, 128, 255, 64, 0] },
  waveformState: "ready" as const,
  waveformMessage: ""
};

function setTrackGeometry(element: HTMLElement) {
  vi.spyOn(element, "getBoundingClientRect").mockReturnValue({
    x: 100,
    y: 0,
    left: 100,
    right: 1_100,
    top: 0,
    bottom: 80,
    width: 1_000,
    height: 80,
    toJSON: () => ({})
  });
}

describe("TimelineEditor", () => {
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, "setPointerCapture", { configurable: true, value: vi.fn() });
    Object.defineProperty(HTMLElement.prototype, "releasePointerCapture", { configurable: true, value: vi.fn() });
    Object.defineProperty(HTMLElement.prototype, "hasPointerCapture", { configurable: true, value: vi.fn(() => true) });
  });

  afterEach(() => vi.restoreAllMocks());

  it("renders one ruler, one playhead, and a navigator rather than a second timeline", () => {
    const { container } = render(TimelineEditor, { props });
    expect(container.querySelectorAll("[data-timeline-ruler]")).toHaveLength(1);
    expect(container.querySelectorAll(".timeline-playhead")).toHaveLength(1);
    expect(container.querySelector(".timeline-navigator .timeline-playhead")).toBeNull();
    expect(container.querySelectorAll(".timeline-delete-region")).toHaveLength(2);
    expect(screen.getByRole("img", { name: "源音频波形" })).toBeInTheDocument();
    expect(screen.getByText("删除区间轨道")).toBeInTheDocument();
    expect(container.querySelector(".timeline-pending-region")).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "源视频编辑时间轴" })).toBeInTheDocument();
    expect(screen.getByRole("scrollbar", { name: "浏览完整视频" })).toBeInTheDocument();
  });

  it("shows a non-blocking waveform analysis state", () => {
    render(TimelineEditor, {
      props: { ...props, waveform: null, waveformState: "loading", waveformMessage: "正在分析音频…" }
    });

    expect(screen.getByRole("status", { name: "音频波形状态" })).toHaveTextContent("正在分析音频");
    expect(screen.getByRole("button", { name: /完成删除区间/ })).toBeEnabled();
  });

  it("uses the waveform lane as part of the shared scrub track", async () => {
    const starts = vi.fn();
    const commits = vi.fn();
    const view = render(TimelineEditor, { props, events: { scrubStart: starts, scrubCommit: commits } });
    const track = view.getByTestId("timeline-track");
    const waveform = view.getByTestId("waveform-track");
    setTrackGeometry(track);

    await fireEvent.pointerDown(waveform, { button: 0, pointerId: 21, clientX: 650 });
    await fireEvent.pointerUp(waveform, { pointerId: 21, clientX: 650 });

    expect(starts).toHaveBeenCalledOnce();
    expect(commits).toHaveBeenCalledOnce();
  });

  it("scrubs continuously and commits once with pointer capture", async () => {
    const starts: number[] = [];
    const previews: number[] = [];
    const commits: number[] = [];
    const view = render(TimelineEditor, {
      props,
      events: {
        scrubStart: (event: CustomEvent<{ playheadUs: number }>) => starts.push(event.detail.playheadUs),
        scrubPreview: (event: CustomEvent<{ playheadUs: number }>) => previews.push(event.detail.playheadUs),
        scrubCommit: (event: CustomEvent<{ playheadUs: number }>) => commits.push(event.detail.playheadUs)
      }
    });
    const track = view.getByTestId("timeline-track");
    setTrackGeometry(track);

    await fireEvent.pointerDown(track, { button: 0, pointerId: 7, clientX: 200 });
    await fireEvent.pointerMove(track, { pointerId: 7, clientX: 600 });
    await fireEvent.pointerMove(track, { pointerId: 7, clientX: 800 });
    await fireEvent.pointerUp(track, { pointerId: 7, clientX: 800 });

    expect(track.setPointerCapture).toHaveBeenCalledWith(7);
    expect(starts).toHaveLength(1);
    expect(previews.length).toBeGreaterThanOrEqual(3);
    expect(commits).toHaveLength(1);
    expect(commits[0]).toBe(previews.at(-1));
  });

  it("cancels an active scrub without committing", async () => {
    const cancels = vi.fn();
    const commits = vi.fn();
    const view = render(TimelineEditor, { props, events: { scrubCancel: cancels, scrubCommit: commits } });
    const track = view.getByTestId("timeline-track");
    setTrackGeometry(track);

    await fireEvent.pointerDown(track, { button: 0, pointerId: 2, clientX: 300 });
    await fireEvent.pointerMove(track, { pointerId: 2, clientX: 500 });
    await fireEvent.pointerCancel(track, { pointerId: 2 });

    expect(cancels).toHaveBeenCalledOnce();
    expect(commits).not.toHaveBeenCalled();
  });

  it("cancels an active scrub when pointer capture is lost", async () => {
    const cancels = vi.fn();
    const commits = vi.fn();
    const view = render(TimelineEditor, { props, events: { scrubCancel: cancels, scrubCommit: commits } });
    const track = view.getByTestId("timeline-track");
    setTrackGeometry(track);

    await fireEvent.pointerDown(track, { button: 0, pointerId: 9, clientX: 300 });
    await fireEvent.pointerMove(track, { pointerId: 9, clientX: 500 });
    await fireEvent.lostPointerCapture(track, { pointerId: 9 });

    expect(cancels).toHaveBeenCalledOnce();
    expect(commits).not.toHaveBeenCalled();
  });

  it("selects an interval while treating its body drag as scrub, never resize", async () => {
    const selections: number[] = [];
    const starts = vi.fn();
    const resizes = vi.fn();
    const view = render(TimelineEditor, {
      props,
      events: {
        select: (event: CustomEvent<{ id: number }>) => selections.push(event.detail.id),
        scrubStart: starts,
        resize: resizes
      }
    });
    const track = view.getByTestId("timeline-track");
    const region = screen.getByRole("button", { name: /删除区间 1/ });
    setTrackGeometry(track);
    setTrackGeometry(region);

    await fireEvent.pointerDown(region, { button: 0, pointerId: 4, clientX: 400 });
    await fireEvent.pointerMove(region, { pointerId: 4, clientX: 500 });
    await fireEvent.pointerUp(region, { pointerId: 4, clientX: 500 });

    expect(selections).toEqual([1]);
    expect(starts).toHaveBeenCalledOnce();
    expect(resizes).not.toHaveBeenCalled();
  });

  it("previews a boundary drag and dispatches one resize only on release", async () => {
    const resizes: Array<{ id: number; startUs: number; endUs: number }> = [];
    const view = render(TimelineEditor, {
      props,
      events: { resize: (event: CustomEvent<(typeof resizes)[number]>) => resizes.push(event.detail) }
    });
    const track = view.getByTestId("timeline-track");
    const handle = screen.getByRole("slider", { name: "调整删除起点" });
    setTrackGeometry(track);
    setTrackGeometry(handle);

    await fireEvent.pointerDown(handle, { button: 0, pointerId: 5, clientX: 400 });
    await fireEvent.pointerMove(handle, { pointerId: 5, clientX: 500 });
    expect(resizes).toEqual([]);
    await fireEvent.pointerUp(handle, { pointerId: 5, clientX: 500 });

    expect(resizes).toHaveLength(1);
    expect(resizes[0].id).toBe(1);
    expect(resizes[0].startUs).toBeLessThan(resizes[0].endUs);
  });

  it("cancels a boundary drag with Escape without dispatching resize", async () => {
    const resizes = vi.fn();
    const view = render(TimelineEditor, { props, events: { resize: resizes } });
    const track = view.getByTestId("timeline-track");
    const handle = screen.getByRole("slider", { name: "调整删除起点" });
    setTrackGeometry(track);

    await fireEvent.pointerDown(handle, { button: 0, pointerId: 12, clientX: 400 });
    await fireEvent.pointerMove(handle, { pointerId: 12, clientX: 500 });
    await fireEvent.keyDown(window, { key: "Escape" });
    await fireEvent.pointerUp(handle, { pointerId: 12, clientX: 500 });

    expect(resizes).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: /删除区间 1，00:00:50\.000 到 00:01:10\.000/ })).toBeInTheDocument();
  });

  it("fits the whole source and supports anchored wheel zoom", async () => {
    const view = render(TimelineEditor, { props });
    const track = view.getByTestId("timeline-track");
    const navigatorWindow = view.getByTestId("navigator-window");
    setTrackGeometry(track);

    await fireEvent.click(screen.getByRole("button", { name: "适配全片" }));
    expect(navigatorWindow).toHaveStyle({ left: "0%", width: "100%" });
    const deleteRegion = view.container.querySelector<HTMLElement>(".timeline-delete-wrap");
    expect(deleteRegion).not.toBeNull();
    const initialLeft = deleteRegion?.style.left;
    const initialWidth = deleteRegion?.style.width;

    const wheel = new WheelEvent("wheel", { bubbles: true, cancelable: true, altKey: true, deltaY: -300, clientX: 600 });
    track.dispatchEvent(wheel);
    expect(wheel.defaultPrevented).toBe(true);
    await waitFor(() => expect(Number.parseFloat(navigatorWindow.style.width)).toBeLessThan(100));
    expect(deleteRegion?.style.left).not.toBe(initialLeft);
    expect(deleteRegion?.style.width).not.toBe(initialWidth);
  });

  it("keeps navigation available while export locking interval edits", () => {
    const { container } = render(TimelineEditor, { props: { ...props, editLocked: true } });
    expect(screen.getByRole("button", { name: /重设删除起点/ })).toBeDisabled();
    expect(screen.getByRole("scrollbar", { name: "浏览完整视频" })).toHaveAttribute("aria-disabled", "false");
    expect(container.querySelector(".timeline-edge-handle")).toBeNull();
  });
});
