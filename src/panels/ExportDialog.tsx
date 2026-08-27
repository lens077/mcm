import { useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { ipc } from "../ipc/client";
import type { ExportFormat, ExportReport } from "../ipc/types";
import { summariseReport } from "./export-summary";

interface Props {
  open: boolean;
  onClose: () => void;
}

const FORMATS: { id: ExportFormat; label: string; extension: string; blurb: string }[] = [
  {
    id: "xmind",
    label: "XMind 脑图",
    extension: "xmind",
    blurb: "任务层级为可编辑节点，依赖为真实连线。",
  },
  {
    id: "vsdx",
    label: "Visio 图表",
    extension: "vsdx",
    blurb: "任务为形状，依赖为保持粘连的连接线。",
  },
];

export function ExportDialog({ open, onClose }: Props) {
  const [format, setFormat] = useState<ExportFormat>("xmind");
  const [report, setReport] = useState<ExportReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!open) return null;

  const run = async () => {
    setError(null);
    const chosen = FORMATS.find((entry) => entry.id === format);
    if (!chosen) return;

    // Exporting with validation errors is allowed, but never silently.
    const precheck = await ipc.exportPrecheck();
    if (!precheck.ok) {
      const proceed = window.confirm(
        `规划仍有 ${String(precheck.error_count)} 个校验错误。仍要导出吗？`,
      );
      if (!proceed) return;
    }

    const path = await saveDialog({
      defaultPath: `规划.${chosen.extension}`,
      filters: [{ name: chosen.label, extensions: [chosen.extension] }],
    });
    if (typeof path !== "string") return;

    setBusy(true);
    try {
      setReport(await ipc.exportRun(format, path));
    } catch (raw) {
      const message = raw instanceof Object && "message" in raw ? String(raw.message) : String(raw);
      setError(message);
    } finally {
      setBusy(false);
    }
  };

  const summary = report ? summariseReport(report) : null;

  return (
    <div className="modal-backdrop" role="presentation">
      <section className="modal" role="dialog" aria-modal="true" aria-label="导出规划">
        <header className="panel-head">
          <h2>导出规划</h2>
          <button type="button" className="toolbar-button" onClick={onClose} aria-label="关闭">
            ✕
          </button>
        </header>

        <div className="modal-body">
          <fieldset className="format-picker">
            <legend>目标格式</legend>
            {FORMATS.map((entry) => (
              <label key={entry.id} className="format-option">
                <input
                  type="radio"
                  name="export-format"
                  value={entry.id}
                  checked={format === entry.id}
                  onChange={() => {
                    setFormat(entry.id);
                    setReport(null);
                  }}
                />
                <span>
                  <strong>{entry.label}</strong>
                  <em>{entry.blurb}</em>
                </span>
              </label>
            ))}
          </fieldset>

          {error && <p className="export-error">{error}</p>}

          {summary && (
            <div className="export-report">
              <h3>导出完成</h3>
              <p className="export-path">{report?.output_path}</p>
              <ul className="mapped-list">
                {summary.mapped.map((line) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>

              <h4>
                降级内容（{summary.degradedCount}）
                <span className="hint-inline">目标格式无法原生表达，已保留为下列形式</span>
              </h4>
              {summary.degradedCount === 0 ? (
                <p className="empty-hint">没有降级内容。</p>
              ) : (
                <ul className="degraded-list">
                  {summary.degraded.map((item) => (
                    <li key={`${item.element}-${item.original}`}>
                      <span className="target">{item.element}</span>
                      <span>{item.original}</span>
                      <span className="fallback">→ {item.fallback}</span>
                    </li>
                  ))}
                </ul>
              )}

              {summary.warnings.length > 0 && (
                <ul className="warning-list">
                  {summary.warnings.map((warning) => (
                    <li key={warning}>{warning}</li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </div>

        <footer className="modal-foot">
          <button type="button" className="toolbar-button" onClick={onClose}>
            关闭
          </button>
          <button
            type="button"
            className="primary"
            onClick={() => {
              void run();
            }}
            disabled={busy}
          >
            {busy ? "导出中…" : "选择位置并导出"}
          </button>
        </footer>
      </section>
    </div>
  );
}
