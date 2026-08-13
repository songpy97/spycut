import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createDemoSession } from "./lib/types/contracts";

const api = vi.hoisted(() => ({
  tauri: false,
  addDeleteInterval: vi.fn(),
  getAudioWaveform: vi.fn(),
  getLaunchSource: vi.fn(),
  setPlayhead: vi.fn(),
  getSession: vi.fn(),
  chooseSource: vi.fn(),
  openSource: vi.fn(),
  recordFrontendDiagnostic: vi.fn(),
  revealDiagnosticLog: vi.fn()
}));

vi.mock("./lib/api/tauri", () => ({
  runningInTauri: () => api.tauri,
  getLaunchSource: (...args: unknown[]) => api.getLaunchSource(...args),
  getSession: (...args: unknown[]) => api.getSession(...args),
  listRecoverableExports: vi.fn(async () => []),
  onExportProgress: vi.fn(async () => () => {}),
  onExportResult: vi.fn(async () => () => {}),
  addDeleteInterval: (...args: unknown[]) => api.addDeleteInterval(...args),
  getAudioWaveform: (...args: unknown[]) => api.getAudioWaveform(...args),
  setPlayhead: (...args: unknown[]) => api.setPlayhead(...args),
  chooseSource: (...args: unknown[]) => api.chooseSource(...args),
  chooseDestination: vi.fn(async () => null),
  openSource: (...args: unknown[]) => api.openSource(...args),
  cancelExport: vi.fn(),
  cleanupRecoverableExport: vi.fn(),
  diagnosePlayback: vi.fn(),
  redoEdit: vi.fn(),
  removeDeleteInterval: vi.fn(),
  resizeDeleteInterval: vi.fn(),
  revealOutput: vi.fn(),
  revealDiagnosticLog: (...args: unknown[]) => api.revealDiagnosticLog(...args),
  recordFrontendDiagnostic: (...args: unknown[]) => api.recordFrontendDiagnostic(...args),
  setJoinReviewed: vi.fn(),
  startExport: vi.fn(),
  undoEdit: vi.fn(),
  commandMessage: (error: unknown) => {
    if (error && typeof error === "object" && "message" in error) return String(error.message);
    return String(error);
  }
}));

import App from "./App.svelte";

describe("App timeline workflow", () => {
  beforeEach(() => {
    api.tauri = false;
    api.addDeleteInterval.mockReset();
    api.getAudioWaveform.mockReset();
    api.getAudioWaveform.mockResolvedValue({ samplesPerSecond: 50, peaks: [0, 80, 160, 0] });
    api.getLaunchSource.mockReset();
    api.getLaunchSource.mockResolvedValue(null);
    api.setPlayhead.mockReset();
    api.getSession.mockReset();
    api.chooseSource.mockReset();
    api.openSource.mockReset();
    api.recordFrontendDiagnostic.mockReset();
    api.recordFrontendDiagnostic.mockResolvedValue(undefined);
    api.revealDiagnosticLog.mockReset();
    api.revealDiagnosticLog.mockResolvedValue(undefined);
  });

  it("starts with an empty workspace instead of restoring the recent-project cache", async () => {
    api.tauri = true;
    api.getSession.mockResolvedValue({ session: createDemoSession(), resumed: true, previewUrl: "" });

    render(App);

    expect(await screen.findByRole("button", { name: /打开 MP4 录屏/ })).toBeEnabled();
    expect(api.getSession).not.toHaveBeenCalled();
    expect(screen.queryByRole("group", { name: "源视频编辑时间轴" })).not.toBeInTheDocument();
  });

  it("ignores a waveform result that arrives after switching projects", async () => {
    api.tauri = true;
    const first = createDemoSession();
    first.project.lastPlayheadUs = 0;
    const second = structuredClone(first);
    second.project.projectId = "second-project";
    let resolveFirst!: (waveform: { samplesPerSecond: number; peaks: number[] }) => void;
    const delayedFirst = new Promise<{ samplesPerSecond: number; peaks: number[] }>((resolve) => {
      resolveFirst = resolve;
    });
    api.getLaunchSource.mockResolvedValue("/tmp/first.mp4");
    api.chooseSource.mockResolvedValue("/tmp/second.mp4");
    api.openSource
      .mockResolvedValueOnce({ session: first, resumed: true, previewUrl: "" })
      .mockResolvedValueOnce({ session: second, resumed: false, previewUrl: "" });
    api.getAudioWaveform
      .mockImplementationOnce(() => delayedFirst)
      .mockResolvedValueOnce({ samplesPerSecond: 50, peaks: [0, 20, 20, 0] });
    const view = render(App);

    await waitFor(() => expect(api.getAudioWaveform).toHaveBeenCalledWith(first.project.projectId));
    await fireEvent.click(screen.getByRole("button", { name: "更换录屏" }));
    await waitFor(() => expect(api.getAudioWaveform).toHaveBeenCalledWith(second.project.projectId));
    await screen.findByRole("img", { name: "源音频波形" });
    const secondPath = view.container.querySelector<SVGPathElement>(".timeline-waveform path")?.getAttribute("d");

    resolveFirst({ samplesPerSecond: 50, peaks: [0, 255, 255, 0] });
    await Promise.resolve();
    await Promise.resolve();

    expect(view.container.querySelector<SVGPathElement>(".timeline-waveform path")?.getAttribute("d")).toBe(secondPath);
  });

  it("automatically requests and displays the active project's audio waveform", async () => {
    api.tauri = true;
    const session = createDemoSession();
    session.project.lastPlayheadUs = 0;
    api.getLaunchSource.mockResolvedValue("/tmp/launch.mp4");
    api.openSource.mockResolvedValue({ session, resumed: true, previewUrl: "" });
    render(App);

    expect(await screen.findByRole("img", { name: "源音频波形" })).toBeInTheDocument();
    await waitFor(() => expect(api.getAudioWaveform).toHaveBeenCalledWith(session.project.projectId));
  });

  it("records uncaught frontend errors without interrupting the editor", async () => {
    api.tauri = true;
    render(App);

    window.dispatchEvent(new ErrorEvent("error", {
      message: "renderer broke",
      error: new Error("renderer broke")
    }));

    await waitFor(() => expect(api.recordFrontendDiagnostic).toHaveBeenCalledWith(
      "frontend_error",
      expect.stringContaining("renderer broke")
    ));
    expect(screen.getByRole("button", { name: "打开诊断日志" })).toBeEnabled();
  });

  it("reveals the diagnostic log from the empty workspace", async () => {
    api.tauri = true;
    render(App);

    await fireEvent.click(await screen.findByRole("button", { name: "打开诊断日志" }));

    expect(api.revealDiagnosticLog).toHaveBeenCalledOnce();
  });

  it("replaces the duplicate timelines with one editor and discoverable marking controls", async () => {
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: "查看界面演示" }));

    expect(await screen.findByRole("group", { name: "源视频编辑时间轴" })).toBeInTheDocument();
    expect(screen.queryByText("精确时间轴")).not.toBeInTheDocument();
    expect(screen.queryByText("全局概览")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "设删除起点" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "完成删除区间" })).toBeDisabled();
  });

  it("keeps a pending start when the backend cannot save the interval", async () => {
    api.tauri = true;
    api.getLaunchSource.mockResolvedValue("/tmp/launch.mp4");
    api.openSource.mockResolvedValue({ session: createDemoSession(), resumed: true, previewUrl: "" });
    api.addDeleteInterval.mockRejectedValue({ code: "save_failed", message: "测试保存失败" });
    api.setPlayhead.mockResolvedValue(createDemoSession());
    render(App);

    await screen.findByRole("group", { name: "源视频编辑时间轴" });
    await fireEvent.click(screen.getByRole("button", { name: "设删除起点" }));
    const playhead = screen.getByRole("slider", { name: "当前播放位置" });
    await fireEvent.keyDown(playhead, { key: "ArrowRight", shiftKey: true });
    await waitFor(() => expect(screen.getByRole("button", { name: "完成删除区间" })).toBeEnabled());
    await fireEvent.click(screen.getByRole("button", { name: "完成删除区间" }));

    expect(await screen.findAllByText("测试保存失败")).toHaveLength(2);
    expect(screen.getByText(/删除起点 .* 已设置/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重设删除起点" })).toBeEnabled();
  });
});
