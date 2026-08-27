import { describe, expect, it } from "vitest";
import { groupDegraded, needsAttention, summariseReport } from "./export-summary";
import type { ExportReport } from "../ipc/types";

const report: ExportReport = {
  format: "xmind",
  output_path: "/tmp/plan.xmind",
  mapped: [
    { kind: "任务", count: 12, representation: "脑图 topic" },
    { kind: "依赖", count: 3, representation: "relationship 连线" },
  ],
  degraded: [
    { element: "任务 t1", original: "日期 2026-09-01..2026-09-05", fallback: "标签文本" },
    { element: "任务 t2", original: "负责人 王芳", fallback: "标签 @负责人" },
    { element: "任务 t3", original: "工期 3d", fallback: "标签文本" },
  ],
  warnings: ["规划仍有 1 个校验错误"],
};

describe("export summary", () => {
  it("renders each mapped category with its count and representation", () => {
    const summary = summariseReport(report);
    expect(summary.mapped).toEqual([
      "任务 × 12 → 脑图 topic",
      "依赖 × 3 → relationship 连线",
    ]);
  });

  it("surfaces every degraded item, never a subset", () => {
    // SC-008: zero silent loss — the count must match the report exactly.
    const summary = summariseReport(report);
    expect(summary.degradedCount).toBe(report.degraded.length);
    expect(summary.degraded).toHaveLength(3);
  });

  it("passes warnings straight through", () => {
    expect(summariseReport(report).warnings).toEqual(["规划仍有 1 个校验错误"]);
  });

  it("handles a clean report", () => {
    const clean: ExportReport = {
      format: "vsdx",
      output_path: "/tmp/a.vsdx",
      mapped: [],
      degraded: [],
      warnings: [],
    };
    const summary = summariseReport(clean);
    expect(summary.mapped).toEqual([]);
    expect(summary.degradedCount).toBe(0);
    expect(needsAttention(clean)).toBe(false);
  });
});

describe("degraded grouping", () => {
  it("buckets items by their fallback representation", () => {
    const grouped = groupDegraded(report);
    expect(grouped.get("标签文本")).toHaveLength(2);
    expect(grouped.get("标签 @负责人")).toHaveLength(1);
  });

  it("keeps every item across buckets", () => {
    const grouped = groupDegraded(report);
    const total = [...grouped.values()].reduce((sum, items) => sum + items.length, 0);
    expect(total).toBe(report.degraded.length);
  });
});

describe("attention flag", () => {
  it("is raised by warnings or degraded content", () => {
    expect(needsAttention(report)).toBe(true);
    expect(
      needsAttention({ ...report, warnings: [], degraded: [] }),
    ).toBe(false);
    expect(needsAttention({ ...report, warnings: [] })).toBe(true);
  });
});
