import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "../ipc/client";
import type { DiagramImport, DownloadProgress, OcrModel, OcrModelStatus } from "../ipc/types";
import { summariseImport } from "./import-summary";
import { megabytes, modelOptions, modelReady, progressLabel } from "./ocr-model";

interface Props {
  open: boolean;
  onClose: () => void;
  /** Loads the imported outline as a new plan; resolves false if the user backed out. */
  onLoad: (outline: string) => Promise<boolean>;
}

const FILTERS = [
  { name: "图片或 archify HTML", extensions: ["png", "jpg", "jpeg", "webp", "bmp", "html", "htm"] },
];

function messageOf(raw: unknown): string {
  return raw instanceof Object && "message" in raw ? String(raw.message) : String(raw);
}

export function ImportDialog({ open, onClose, onLoad }: Props) {
  const [result, setResult] = useState<DiagramImport | null>(null);
  const [source, setSource] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [model, setModel] = useState<OcrModel>("fast");
  const [status, setStatus] = useState<OcrModelStatus | null>(null);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);

  // The chosen model is a preference; the install state comes from the core.
  useEffect(() => {
    if (!open) return;
    void (async () => {
      try {
        const [prefs, current] = await Promise.all([ipc.prefsGet(), ipc.ocrModelStatus()]);
        setModel(prefs.ocr_model ?? "fast");
        setStatus(current);
      } catch (raw) {
        setError(messageOf(raw));
      }
    })();
  }, [open]);

  if (!open) return null;

  const choose = async (next: OcrModel) => {
    setModel(next);
    setResult(null);
    const prefs = await ipc.prefsGet();
    await ipc.prefsSet({ ...prefs, ocr_model: next });
  };

  const download = async () => {
    setError(null);
    setProgress({ done: 0, total: status?.accurate_download_bytes ?? 0 });
    const stop = await listen<DownloadProgress>("ocr-model-download", (event) => {
      setProgress(event.payload);
    });
    try {
      setStatus(await ipc.ocrModelDownload());
    } catch (raw) {
      setError(messageOf(raw));
    } finally {
      stop();
      setProgress(null);
    }
  };

  const remove = async () => {
    if (!window.confirm("删除已下载的高精度模型？之后可随时重新下载。")) return;
    try {
      setStatus(await ipc.ocrModelRemove());
      await choose("fast");
    } catch (raw) {
      setError(messageOf(raw));
    }
  };

  const pick = async () => {
    setError(null);
    const selected = await openDialog({ multiple: false, filters: FILTERS });
    if (typeof selected !== "string") return;
    setSource(selected);
    setResult(null);
    setBusy(true);
    try {
      setResult(await ipc.diagramImport(selected, model));
    } catch (raw) {
      setError(messageOf(raw));
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
  const downloading = progress !== null || status?.downloading === true;
  const accurateMissing = !modelReady("accurate", status);

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

          <fieldset className="format-picker">
            <legend>识别模型（仅用于截图）</legend>
            {modelOptions(status).map((option) => (
              <label key={option.id} className="format-option">
                <input
                  type="radio"
                  name="ocr-model"
                  value={option.id}
                  checked={model === option.id}
                  disabled={busy}
                  onChange={() => {
                    void choose(option.id);
                  }}
                />
                <span>
                  <strong>{option.label}</strong>
                  <em>{option.blurb}</em>
                </span>
              </label>
            ))}
          </fieldset>

          {model === "accurate" && accurateMissing && (
            <div className="model-download">
              <p>
                高精度模型不随安装包分发。点击下载后从 ModelScope 获取，失败时改用
                GitHub，完成后校验 SHA-256。只在你点击时联网。
              </p>
              {progress ? (
                <p className="empty-hint" aria-live="polite">
                  {progressLabel(progress)}
                </p>
              ) : (
                <button
                  type="button"
                  className="toolbar-button"
                  onClick={() => {
                    void download();
                  }}
                  disabled={downloading}
                >
                  下载高精度模型（{status ? megabytes(status.accurate_download_bytes) : "约 20 MB"}
                  ）
                </button>
              )}
              {status && (
                <p className="hint-block">
                  无法联网时，可把 pp-ocrv6_small_rec.onnx 与 ppocrv6_dict.txt 复制到：
                  <code>{status.accurate_dir}</code>
                </p>
              )}
            </div>
          )}

          {model === "accurate" && !accurateMissing && (
            <p className="hint-block">
              高精度模型已就绪。
              <button
                type="button"
                className="link-button"
                onClick={() => {
                  void remove();
                }}
              >
                删除模型
              </button>
            </p>
          )}

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
            disabled={busy || downloading}
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
