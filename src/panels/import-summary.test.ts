import { describe, expect, it } from "vitest";
import { summariseImport } from "./import-summary";
import type { ImportReport } from "../ipc/types";

const report = (over: Partial<ImportReport>): ImportReport => ({
  nodes: 12,
  groups: 1,
  dependencies: 11,
  undirected: 0,
  cycle_breaks: [],
  loose_text: [],
  edge_labels: [],
  untitled_nodes: 0,
  ...over,
});

describe("import summary", () => {
  it("lists every mapping", () => {
    const summary = summariseImport(report({}));
    expect(summary.mapped).toEqual([
      "方框 × 12 → 任务（副标题为备注）",
      "分组框 × 1 → 父任务",
      "箭头 × 11 → 依赖",
    ]);
    expect(summary.notices).toEqual([]);
  });

  it("surfaces every lossy decision", () => {
    const summary = summariseImport(
      report({
        undirected: 2,
        untitled_nodes: 1,
        cycle_breaks: ["A → B"],
        loose_text: ["Legend"],
        edge_labels: ["A → B：HTTPS"],
      }),
    );
    expect(summary.notices).toHaveLength(5);
    expect(summary.notices.join("\n")).toContain("A → B");
    expect(summary.notices.join("\n")).toContain("注释");
  });
});
