import { SHORTCUTS, describe } from "./shortcuts";

interface Props {
  undoDepth: number;
  redoDepth: number;
  dirty: boolean;
  onOpen: () => void;
  onSave: () => void;
  onExport: () => void;
  onUndo: () => void;
  onRedo: () => void;
  onGenerate: () => void;
  busy: boolean;
}

function hint(id: "edit.undo" | "edit.redo"): string {
  const shortcut = SHORTCUTS.find((entry) => entry.id === id);
  return shortcut ? describe(shortcut) : "";
}

export function Toolbar({
  undoDepth,
  redoDepth,
  dirty,
  onOpen,
  onSave,
  onExport,
  onUndo,
  onRedo,
  onGenerate,
  busy,
}: Props) {
  return (
    <div className="toolbar" role="toolbar" aria-label="编辑操作">
      <button type="button" className="toolbar-button" onClick={onOpen} aria-label="打开规划">
        打开
      </button>
      <button
        type="button"
        className="toolbar-button"
        onClick={onSave}
        aria-label="保存规划"
        title={dirty ? "有未保存的更改" : "已保存"}
      >
        保存{dirty ? " •" : ""}
      </button>
      <button type="button" className="toolbar-button" onClick={onExport} aria-label="导出规划">
        导出
      </button>
      <span className="toolbar-divider" aria-hidden="true" />
      <button
        type="button"
        className="toolbar-button"
        onClick={onUndo}
        disabled={undoDepth === 0}
        title={`撤销 ${hint("edit.undo")}`}
        aria-label="撤销"
      >
        ↶ 撤销
      </button>
      <button
        type="button"
        className="toolbar-button"
        onClick={onRedo}
        disabled={redoDepth === 0}
        title={`重做 ${hint("edit.redo")}`}
        aria-label="重做"
      >
        ↷ 重做
      </button>
      <span className="toolbar-depth" aria-live="polite">
        {undoDepth} 步可撤销
      </span>
      <button type="button" className="primary toolbar-generate" onClick={onGenerate} disabled={busy}>
        {busy ? "生成中…" : "生成规划"}
      </button>
    </div>
  );
}
