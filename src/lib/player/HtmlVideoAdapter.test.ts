import { afterEach, describe, expect, it, vi } from "vitest";
import { HtmlVideoAdapter } from "./HtmlVideoAdapter";

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("HtmlVideoAdapter playback", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("treats a pending play interrupted by an explicit pause as cancellation", async () => {
    const video = document.createElement("video");
    const browserPlay = deferred<void>();
    vi.spyOn(video, "play").mockReturnValue(browserPlay.promise);
    vi.spyOn(video, "pause").mockImplementation(() => {});
    const adapter = new HtmlVideoAdapter(video);

    const playback = adapter.play();
    const expectation = expect(playback).resolves.toBeUndefined();
    adapter.pause();
    browserPlay.reject(new DOMException("The play() request was interrupted by a call to pause().", "AbortError"));

    await expectation;
  });

  it("still reports a current playback failure", async () => {
    const video = document.createElement("video");
    const browserPlay = deferred<void>();
    const failure = new DOMException("Playback requires a user gesture", "NotAllowedError");
    vi.spyOn(video, "play").mockReturnValue(browserPlay.promise);
    const adapter = new HtmlVideoAdapter(video);

    const playback = adapter.play();
    const expectation = expect(playback).rejects.toBe(failure);
    browserPlay.reject(failure);

    await expectation;
  });
});

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
    await expect(new HtmlVideoAdapter(video).seekTo(8.0004)).resolves.toBe(true);
    expect(addListener).not.toHaveBeenCalled();
  });

  it("resolves on seeked and removes all listeners", async () => {
    const video = document.createElement("video");
    const removeListener = vi.spyOn(video, "removeEventListener");
    const pending = new HtmlVideoAdapter(video).seekTo(4);
    video.dispatchEvent(new Event("seeked"));
    await expect(pending).resolves.toBe(true);
    expect(removeListener).toHaveBeenCalledWith("seeked", expect.any(Function));
    expect(removeListener).toHaveBeenCalledWith("error", expect.any(Function));
  });

  it("lets the latest exact seek supersede an older request", async () => {
    const video = document.createElement("video");
    let seeking = true;
    Object.defineProperty(video, "seeking", { configurable: true, get: () => seeking });
    const adapter = new HtmlVideoAdapter(video);

    const first = adapter.seekTo(4);
    const second = adapter.seekTo(8);
    const firstExpectation = expect(first).resolves.toBe(false);
    seeking = false;
    video.dispatchEvent(new Event("seeked"));

    await firstExpectation;
    await expect(second).resolves.toBe(true);
  });

  it("lets a preview seek supersede a pending exact request", async () => {
    const video = document.createElement("video");
    Object.defineProperty(video, "seeking", { configurable: true, value: true });
    const adapter = new HtmlVideoAdapter(video);

    const exact = adapter.seekTo(4);
    adapter.previewSeekTo(8);

    await expect(exact).resolves.toBe(false);
    expect(video.currentTime).toBe(8);
  });

  it("rejects on media error", async () => {
    const video = document.createElement("video");
    const pending = new HtmlVideoAdapter(video).seekTo(4);
    video.dispatchEvent(new Event("error"));
    await expect(pending).rejects.toThrow("视频定位失败");
  });

  it("accepts a completed seek when the seeked event was missed", async () => {
    vi.useFakeTimers();
    const video = document.createElement("video");
    let seeking = true;
    Object.defineProperty(video, "seeking", { configurable: true, get: () => seeking });
    const pending = new HtmlVideoAdapter(video).seekTo(4);
    const outcome = pending.then(
      (value) => ({ status: "fulfilled" as const, value }),
      (reason) => ({ status: "rejected" as const, reason })
    );
    seeking = false;

    await vi.advanceTimersByTimeAsync(10_000);
    expect(await outcome).toEqual({ status: "fulfilled", value: true });
  });

  it("uses a finite timeout while the media element is still seeking", async () => {
    vi.useFakeTimers();
    const video = document.createElement("video");
    Object.defineProperty(video, "seeking", { configurable: true, value: true });
    let settled = false;
    const pending = new HtmlVideoAdapter(video).seekTo(4).finally(() => { settled = true; });
    const outcome = pending.then(
      (value) => ({ status: "fulfilled" as const, value }),
      (reason) => ({ status: "rejected" as const, reason })
    );

    await vi.advanceTimersByTimeAsync(9_999);
    expect(settled).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    const result = await outcome;
    expect(result.status).toBe("rejected");
    if (result.status === "rejected") expect(result.reason).toEqual(new Error("视频定位超时"));
  });
});
