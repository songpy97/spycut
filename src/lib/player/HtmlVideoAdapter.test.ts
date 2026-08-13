import { afterEach, describe, expect, it, vi } from "vitest";
import { HtmlVideoAdapter } from "./HtmlVideoAdapter";

describe("HtmlVideoAdapter seeking", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("updates preview seeks synchronously without waiting", () => {
    const video = document.createElement("video");
    const adapter = new HtmlVideoAdapter(video);
    adapter.previewSeekTo(12.5);
    expect(video.currentTime).toBe(12.5);
  });

  it("resolves immediately when already at the exact target", async () => {
    const video = document.createElement("video");
    video.currentTime = 8;
    const addListener = vi.spyOn(video, "addEventListener");
    await new HtmlVideoAdapter(video).seekTo(8.0004);
    expect(addListener).not.toHaveBeenCalled();
  });

  it("resolves on seeked and removes all listeners", async () => {
    const video = document.createElement("video");
    const removeListener = vi.spyOn(video, "removeEventListener");
    const pending = new HtmlVideoAdapter(video).seekTo(4);
    video.dispatchEvent(new Event("seeked"));
    await pending;
    expect(removeListener).toHaveBeenCalledWith("seeked", expect.any(Function));
    expect(removeListener).toHaveBeenCalledWith("error", expect.any(Function));
  });

  it("rejects on media error", async () => {
    const video = document.createElement("video");
    const pending = new HtmlVideoAdapter(video).seekTo(4);
    video.dispatchEvent(new Event("error"));
    await expect(pending).rejects.toThrow("视频定位失败");
  });

  it("times out instead of waiting forever", async () => {
    vi.useFakeTimers();
    const video = document.createElement("video");
    const pending = new HtmlVideoAdapter(video).seekTo(4);
    const expectation = expect(pending).rejects.toThrow("视频定位超时");
    await vi.advanceTimersByTimeAsync(5_000);
    await expectation;
  });
});
