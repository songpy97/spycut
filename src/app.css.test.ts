import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const appCss = readFileSync(resolve("src/app.css"), "utf8");

function ruleFor(selector: string): string {
  const escapedSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = appCss.match(new RegExp(`${escapedSelector}\\s*\\{([^}]*)\\}`));
  expect(match, `missing CSS rule for ${selector}`).not.toBeNull();
  return match?.[1] ?? "";
}

describe("editor layout sizing", () => {
  it("keeps high-resolution video from forcing the editor beyond the window", () => {
    expect(ruleFor(".video-column")).toMatch(/min-height\s*:\s*0\s*;/);
    expect(ruleFor(".video-column")).toMatch(/overflow\s*:\s*hidden\s*;/);

    const videoRule = ruleFor(".player-pane video");
    expect(videoRule).toMatch(/position\s*:\s*absolute\s*;/);
    expect(videoRule).toMatch(/inset\s*:\s*0\s*;/);
  });
});
