import { describe, expect, it } from "vitest";
import { meetsFrameBudget, summarise } from "./perf";

describe("frame statistics", () => {
  it("computes fps from frame deltas", () => {
    // 16.67ms per frame is 60fps.
    const stats = summarise([16.67, 16.67, 16.67]);
    expect(stats.fps).toBeCloseTo(60, 0);
  });

  it("reports the worst frame, not just the average", () => {
    const stats = summarise([16, 16, 120]);
    expect(stats.worstFrameMs).toBe(120);
    // A single 120ms stall drags the average well below 60fps.
    expect(stats.fps).toBeLessThan(60);
  });

  it("ignores non-positive deltas", () => {
    const stats = summarise([0, -5, 16.67]);
    expect(stats.fps).toBeCloseTo(60, 0);
  });

  it("returns zeroes for an empty window", () => {
    expect(summarise([])).toEqual({ fps: 0, worstFrameMs: 0 });
  });

  it("checks the 60fps budget with tolerance", () => {
    expect(meetsFrameBudget({ fps: 60, worstFrameMs: 17 })).toBe(true);
    expect(meetsFrameBudget({ fps: 56, worstFrameMs: 20 })).toBe(true);
    expect(meetsFrameBudget({ fps: 30, worstFrameMs: 40 })).toBe(false);
  });
});
