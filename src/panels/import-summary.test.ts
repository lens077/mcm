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
  hierarchy_links: [],
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

  it("speaks Mermaid's vocabulary for Mermaid sources", () => {
    const summary = summariseImport(
      report({ undirected: 1, hierarchy_links: ["核心域 → 订单"] }),
      "mermaid",
    );
    expect(summary.mapped[0]).toBe("节点 × 12 → 任务（多行文字的其余行为备注）");
    expect(summary.mapped[1]).toBe("子图 × 1 → 父任务");
    expect(summary.notices[0]).toContain("书写顺序");
    expect(summary.notices[1]).toContain("核心域 → 订单");
  });
});
