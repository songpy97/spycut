import { fireEvent, render } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";
import { createDemoSession, type SessionProjection } from "../types/contracts";
import ReviewPanel from "./ReviewPanel.svelte";

function reviewedSession(bitDepth = 8): SessionProjection {
  const session = createDemoSession();
  session.project.reviewedIntervalIds = session.project.deleteIntervals.map((item) => item.id);
  session.project.media.bitDepth = bitDepth;
  return session;
}

describe("review export safeguards", () => {
  it("requires a second explicit action for unreviewed joins", async () => {
    const events: Array<{ allowUnreviewed: boolean; allowBitDepthFallback: boolean }> = [];
    const view = render(ReviewPanel, {
      props: { session: createDemoSession(), activeIndex: 0 },
      events: { export: (event: CustomEvent<(typeof events)[number]>) => events.push(event.detail) }
    });

    const firstAction = view.getByRole("button", { name: /需要确认：2 处未复核/ });
    await fireEvent.click(firstAction);
    expect(events).toEqual([]);
    const confirmation = view.getByRole("button", { name: "确认仍然导出" });
    await fireEvent.click(confirmation);
    expect(events).toEqual([{ allowUnreviewed: true, allowBitDepthFallback: false }]);
  });

  it("requires confirmation before converting Main10 to 8-bit", async () => {
    const events: Array<{ allowUnreviewed: boolean; allowBitDepthFallback: boolean }> = [];
    const view = render(ReviewPanel, {
      props: { session: reviewedSession(10), activeIndex: 0 },
      events: { export: (event: CustomEvent<(typeof events)[number]>) => events.push(event.detail) }
    });

    await fireEvent.click(view.getByRole("button", { name: /10-bit 将转为 8-bit/ }));
    expect(events).toEqual([]);
    await fireEvent.click(view.getByRole("button", { name: "确认仍然导出" }));
    expect(events).toEqual([{ allowUnreviewed: false, allowBitDepthFallback: true }]);
  });

  it("exports a fully reviewed 8-bit project in one action", async () => {
    const events: Array<{ allowUnreviewed: boolean; allowBitDepthFallback: boolean }> = [];
    const view = render(ReviewPanel, {
      props: { session: reviewedSession(), activeIndex: 0 },
      events: { export: (event: CustomEvent<(typeof events)[number]>) => events.push(event.detail) }
    });

    await fireEvent.click(view.getByRole("button", { name: "选择位置并导出 MP4" }));
    expect(events).toEqual([{ allowUnreviewed: false, allowBitDepthFallback: false }]);
  });
});
