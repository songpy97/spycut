import { render, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

const adapterMocks = vi.hoisted(() => ({
  load: vi.fn(async () => {}),
  dispose: vi.fn(),
  setRate: vi.fn(),
  previewSeekTo: vi.fn(),
  seekTo: vi.fn(async () => {}),
  play: vi.fn(async () => {}),
  pause: vi.fn()
}));

vi.mock("../player/HtmlVideoAdapter", () => ({
  HtmlVideoAdapter: class {
    load = adapterMocks.load;
    dispose = adapterMocks.dispose;
    setRate = adapterMocks.setRate;
    previewSeekTo = adapterMocks.previewSeekTo;
    seekTo = adapterMocks.seekTo;
    play = adapterMocks.play;
    pause = adapterMocks.pause;
  }
}));

import PlayerPane from "./PlayerPane.svelte";

describe("PlayerPane preview seeking", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("coalesces multiple preview positions into the latest animation frame", async () => {
    const callbacks: FrameRequestCallback[] = [];
    vi.stubGlobal("requestAnimationFrame", vi.fn((callback: FrameRequestCallback) => {
      callbacks.push(callback);
      return callbacks.length;
    }));
    vi.stubGlobal("cancelAnimationFrame", vi.fn());

    const { component } = render(PlayerPane, { props: { sourceUrl: "http://localhost/media/test", demo: false } });
    await waitFor(() => expect(adapterMocks.load).toHaveBeenCalledOnce());
    component.previewSeekTo(1_000_000);
    component.previewSeekTo(2_000_000);
    component.previewSeekTo(3_000_000);

    expect(callbacks).toHaveLength(1);
    callbacks[0](16);
    expect(adapterMocks.previewSeekTo).toHaveBeenCalledOnce();
    expect(adapterMocks.previewSeekTo).toHaveBeenCalledWith(3);
  });
});
