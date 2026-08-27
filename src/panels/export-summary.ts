// Pure formatting for the export report, kept separate so the zero-silent-loss
// rule (spec FR-021 / SC-008) is directly testable.
import type { DegradedItem, ExportReport } from "../ipc/types";

export interface ExportSummary {
  mapped: string[];
  degraded: DegradedItem[];
  degradedCount: number;
  warnings: string[];
}

export function summariseReport(report: ExportReport): ExportSummary {
  return {
    mapped: report.mapped.map(
      (item) => `${item.kind} × ${String(item.count)} → ${item.representation}`,
    ),
    degraded: report.degraded,
    degradedCount: report.degraded.length,
    warnings: report.warnings,
  };
}

/** Groups degraded entries by their fallback representation, for compact display. */
export function groupDegraded(report: ExportReport): Map<string, DegradedItem[]> {
  const grouped = new Map<string, DegradedItem[]>();
  for (const item of report.degraded) {
    const bucket = grouped.get(item.fallback) ?? [];
    bucket.push(item);
    grouped.set(item.fallback, bucket);
  }
  return grouped;
}

/** True when the export finished but the user should still read the report. */
export function needsAttention(report: ExportReport): boolean {
  return report.warnings.length > 0 || report.degraded.length > 0;
}
