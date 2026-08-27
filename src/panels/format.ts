// Pure presentation helpers shared by the panels (kept out of component files
// so React Fast Refresh stays effective).
import type { ElementRef, ValidationIssue } from "../ipc/types";

/** Line numbers of parse issues, so the editor gutter can mark them. */
export function parseErrorLines(issues: ValidationIssue[]): Map<number, ValidationIssue> {
  const byLine = new Map<number, ValidationIssue>();
  for (const issue of issues) {
    if (issue.target.kind === "line" && !byLine.has(issue.target.line)) {
      byLine.set(issue.target.line, issue);
    }
  }
  return byLine;
}

export function describeTarget(target: ElementRef): string {
  switch (target.kind) {
    case "plan":
      return "规划";
    case "task":
      return `任务 t${String(target.id)}`;
    case "milestone":
      return `里程碑 m${String(target.id)}`;
    case "dependency":
      return `依赖 t${String(target.predecessor)} → t${String(target.successor)}`;
    case "line":
      return `第 ${String(target.line)} 行`;
  }
}

export function formatCyclePath(path: number[] | undefined): string | null {
  if (!path || path.length === 0) return null;
  return path.map((id) => `t${String(id)}`).join(" → ");
}
