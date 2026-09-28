import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ipc } from "../ipc/client";
import type { DiagramImport } from "../ipc/types";
import { summariseImport } from "./import-summary";

interface Props {
  open: boolean;
  onClose: () => void;
  /** Loads the imported outline as a new plan; resolves false if the user backed out. */
  onLoad: (outline: string) => Promise<boolean>;
}

const FILTERS = [
  { name: "图片或 archify HTML", extensions: ["png", "jpg", "jpeg", "webp", "bmp", "html", "htm"] },
];

export function ImportDialog({ open, onClose, onLoad }: Props) {
  const [result, setResult] = useState<DiagramImport | null>(null);
  const [source, setSource] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!open) return null;

  const pick = async () => {
    setError(null);
    const selected = await openDialog({ multiple: false, filters: FILTERS });
    if (typeof selected !== "string") return;
    setSource(selected);
    setResult(null);
    setBusy(true);
    try {
      setResult(await ipc.diagramImport(selected));
    } catch (raw) {
      const message = raw instanceof Object && "message" in raw ? String(raw.message) : String(raw);
      setError(message);
    } finally {
      setBusy(false);
    }
  };

  const load = async () => {
    if (!result) return;
    if (await onLoad(result.outline)) {
      setResult(null);
      setSource(null);
      onClose();
    }
  };

  const summary = result ? summariseImport(result.report) : null;

  return (
    <div className="modal-backdrop" role="presentation">
      <section className="modal" role="dialog" aria-modal="true" aria-label="导入图表">
        <header className="panel-head">
          <h2>导入图表</h2>
          <button type="button" className="toolbar-button" onClick={onClose} aria-label="关闭">
            ✕
          </button>
        </header>

        <div className="modal-body">
          <p className="import-intro">
            选择架构图 / 流程图的截图，或 archify 生成的
            HTML。截图在本地识别方框、分组、箭头与文字； HTML
            直接读取其中的结构标注，结果精确。载入后可继续修改，或导出为 XMind /
            Visio。文件不会离开本机。
          </p>

          {source && <p className="export-path">{source}</p>}
          {busy && <p className="empty-hint">识别中…</p>}
          {error && <p className="export-error">{error}</p>}

          {summary && result && (
            <div className="export-report">
              <h3>
                识别完成<span className="hint-inline">用时 {result.elapsed_ms} ms</span>
              </h3>
              <ul className="mapped-list">
                {summary.mapped.map((line) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>
              {summary.notices.length > 0 && (
                <>
                  <h4>请核对</h4>
                  <ul className="warning-list">
                    {summary.notices.map((notice) => (
                      <li key={notice}>{notice}</li>
                    ))}
                  </ul>
                </>
              )}
              <h4>大纲预览</h4>
              <pre className="import-preview">{result.outline}</pre>
            </div>
          )}
        </div>

        <footer className="modal-foot">
          <button type="button" className="toolbar-button" onClick={onClose}>
            取消
          </button>
          <button
            type="button"
            className="toolbar-button"
            onClick={() => {
              void pick();
            }}
            disabled={busy}
          >
            {result ? "换一个文件" : "选择文件…"}
          </button>
          <button
            type="button"
            className="primary"
            onClick={() => {
              void load();
            }}
            disabled={!result || busy}
          >
            载入为新规划
          </button>
        </footer>
      </section>
    </div>
  );
}
