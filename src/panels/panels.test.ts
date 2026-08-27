import { describe, expect, it } from "vitest";
import { describeTarget, formatCyclePath, parseErrorLines } from "./format";
import type { ValidationIssue } from "../ipc/types";

const issue = (over: Partial<ValidationIssue>): ValidationIssue => ({
  severity: "error",
  code: "P-003",
  target: { kind: "line", line: 4 },
  message: "无法解析日期",
  fix_hint: "使用 YYYY-MM-DD",
  ...over,
});

describe("outline editor gutter", () => {
  it("maps parse issues to their source lines", () => {
    const map = parseErrorLines([issue({}), issue({ code: "P-005", target: { kind: "line", line: 7 } })]);
    expect(map.get(4)?.code).toBe("P-003");
    expect(map.get(7)?.code).toBe("P-005");
  });

  it("ignores issues that target model elements", () => {
    const map = parseErrorLines([issue({ code: "V-REF", target: { kind: "task", id: 3 } })]);
    expect(map.size).toBe(0);
  });

  it("keeps the first issue per line", () => {
    const map = parseErrorLines([
      issue({ code: "P-002" }),
      issue({ code: "P-003" }),
    ]);
    expect(map.get(4)?.code).toBe("P-002");
  });
});

describe("issues panel formatting", () => {
  it("describes every element ref kind", () => {
    expect(describeTarget({ kind: "plan" })).toBe("规划");
    expect(describeTarget({ kind: "task", id: 3 })).toBe("任务 t3");
    expect(describeTarget({ kind: "milestone", id: 1 })).toBe("里程碑 m1");
    expect(describeTarget({ kind: "dependency", predecessor: 1, successor: 2 })).toBe(
      "依赖 t1 → t2",
    );
    expect(describeTarget({ kind: "line", line: 9 })).toBe("第 9 行");
  });

  it("renders a full cycle path", () => {
    expect(formatCyclePath([1, 2, 3, 1])).toBe("t1 → t2 → t3 → t1");
  });

  it("returns null when there is no cycle", () => {
    expect(formatCyclePath(undefined)).toBeNull();
    expect(formatCyclePath([])).toBeNull();
  });
});
