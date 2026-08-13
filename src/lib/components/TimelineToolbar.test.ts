import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import TimelineToolbar from "./TimelineToolbar.svelte";

const baseProps = {
  playheadUs: 75_000_000,
  durationUs: 180_000_000,
  frameDurationUs: 33_333,
  pendingStartUs: null,
  canUndo: true,
  canRedo: false,
  markSaving: false,
  editLocked: false,
  zoomValue: 0.5
};

describe("TimelineToolbar", () => {
  it("shows the discoverable idle marking state", () => {
    render(TimelineToolbar, { props: baseProps });
    expect(screen.getByRole("button", { name: /设删除起点/ })).toBeEnabled();
    expect(screen.getByRole("button", { name: /完成删除区间/ })).toBeDisabled();
    expect(screen.queryByRole("button", { name: /取消标记/ })).not.toBeInTheDocument();
  });

  it("shows a pending start and dispatches marking actions", async () => {
    const start = vi.fn();
    const end = vi.fn();
    const cancel = vi.fn();
    render(TimelineToolbar, {
      props: { ...baseProps, pendingStartUs: 60_000_000 },
      events: { markStart: start, markEnd: end, cancelMark: cancel }
    });

    expect(screen.getByText(/删除起点 00:01:00\.000 已设置/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: /重设删除起点/ }));
    await fireEvent.click(screen.getByRole("button", { name: /完成删除区间/ }));
    await fireEvent.click(screen.getByRole("button", { name: /取消标记/ }));
    expect(start).toHaveBeenCalledOnce();
    expect(end).toHaveBeenCalledOnce();
    expect(cancel).toHaveBeenCalledOnce();
  });

  it("disables invalid, saving, and export-locked edits", () => {
    const { rerender } = render(TimelineToolbar, {
      props: { ...baseProps, pendingStartUs: 90_000_000, playheadUs: 80_000_000 }
    });
    expect(screen.getByRole("button", { name: /完成删除区间/ })).toBeDisabled();
    expect(screen.getByText("请将播放头移到删除起点之后")).toBeInTheDocument();

    rerender({ ...baseProps, pendingStartUs: 60_000_000, markSaving: true });
    expect(screen.getByRole("button", { name: /保存删除区间/ })).toBeDisabled();

    rerender({ ...baseProps, editLocked: true });
    expect(screen.getByRole("button", { name: /设删除起点/ })).toBeDisabled();
    expect(screen.getByText("导出期间区间编辑已锁定")).toBeInTheDocument();
  });
});
