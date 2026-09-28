// Pure formatting for the diagram-import report. Kept apart from the dialog so
// the "nothing dropped silently" rule (宪法 VI) is directly testable.
import type { ImportReport } from "../ipc/types";

export interface ImportSummary {
  /** One line per mapped element kind, e.g. "方框 × 12 → 任务". */
  mapped: string[];
  /** Everything the user should double-check, in plain language. */
  notices: string[];
}

export function summariseImport(report: ImportReport): ImportSummary {
  const mapped = [
    `方框 × ${String(report.nodes)} → 任务（副标题为备注）`,
    `分组框 × ${String(report.groups)} → 父任务`,
    `箭头 × ${String(report.dependencies)} → 依赖`,
  ];
  const notices: string[] = [];
  if (report.undirected > 0) {
    notices.push(
      `${String(report.undirected)} 条连线没有箭头，已按从上到下、从左到右的顺序设定方向`,
    );
  }
  if (report.untitled_nodes > 0) {
    notices.push(
      `${String(report.untitled_nodes)} 个方框没有识别到文字，标题暂为「（未识别文字）」`,
    );
  }
  if (report.edge_labels.length > 0) {
    notices.push(
      `${String(report.edge_labels.length)} 条连线带有说明文字，规划模型无法承载，已作为注释保留在大纲末尾`,
    );
  }
  for (const arrow of report.cycle_breaks) {
    notices.push(`循环依赖未导入：${arrow}（已作为注释保留）`);
  }
  if (report.loose_text.length > 0) {
    notices.push(
      `${String(report.loose_text.length)} 段文字不在任何方框内（图例、标注等），已作为注释保留在大纲末尾`,
    );
  }
  return { mapped, notices };
}
