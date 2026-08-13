import { describe, expect, it } from "vitest";
import { clampTime, formatBytes, formatDurationCompact, formatTime } from "./time";

describe("time formatting", () => {
  it("formats multi-hour media", () => {
    expect(formatTime(3_661_234_000, true)).toBe("01:01:01.234");
  });

  it("clamps negative input", () => {
    expect(formatTime(-1)).toBe("00:00:00");
    expect(clampTime(-5, 100)).toBe(0);
    expect(clampTime(200, 100)).toBe(100);
  });

  it("uses compact Chinese duration and binary file sizes", () => {
    expect(formatDurationCompact(3_900_000_000)).toBe("1小时 5分");
    expect(formatBytes(4 * 1024 ** 3)).toBe("4.0 GB");
  });
});

