import { useCallback, useEffect, useState } from "react";
import type {
  EditCommand,
  ElementRef,
  SessionState,
  ValidationIssue,
  ViewKind,
} from "../ipc/types";
import { hasTauri, ipc } from "../ipc/client";
import type { SceneGraph } from "../canvas/scene-types";
import { emptyScene } from "../canvas/scene-types";
import { OutlineEditor } from "../panels/OutlineEditor";
import { IssuesPanel } from "../panels/IssuesPanel";
import { SearchBar } from "../panels/SearchBar";
import { ExportDialog } from "../panels/ExportDialog";
import { ShortcutsHelp } from "../panels/ShortcutsHelp";
import { WbsView } from "../views/wbs/WbsView";
import { GraphView } from "../views/graph/GraphView";
import { TimelineView } from "../views/timeline/TimelineView";
import { MilestonesView } from "../views/milestones/MilestonesView";
import { applyTheme, readTheme, toggleTheme } from "./theme";
import { registerShortcuts, type ShortcutId } from "./shortcuts";
import { PerfOverlay } from "./PerfOverlay";
import { Toolbar } from "./Toolbar";
import { askUnsavedChoice, openPlan, savePlan, savePlanAs } from "./files";

const VIEW_TABS: { id: ViewKind; label: string }[] = [
  { id: "wbs", label: "任务分解" },
  { id: "dep_graph", label: "依赖网络" },
  { id: "timeline", label: "时间线" },
  { id: "milestones", label: "里程碑" },
];

const STARTER_OUTLINE = `%mcm 1
%title 移动端改版
%start 2026-09-01

- 需求阶段 #t1 @王芳
  - [x] 用户访谈 #t2 [2026-09-01..2026-09-05]
  - 竞品分析 #t3 [3d] <-t2
  - 需求评审 #t4 [1d] <-t3
- 设计阶段 #t5
  - 交互稿 #t6 [5d] <-t4
! 需求冻结 #m1 [2026-09-25] <-t4
`;

function renderView(
  view: ViewKind,
  scene: SceneGraph,
  selected: ElementRef | null,
  onSelect: (target: ElementRef | null) => void,
  onCommand: (command: EditCommand) => void,
) {
  const props = { scene, selected, onSelect, onCommand };
  switch (view) {
    case "wbs":
      return <WbsView {...props} />;
    case "dep_graph":
      return <GraphView {...props} />;
    case "timeline":
      return <TimelineView {...props} />;
    case "milestones":
      return <MilestonesView {...props} />;
  }
}

export function App() {
  const [view, setView] = useState<ViewKind>("wbs");
  const [session, setSession] = useState<SessionState | null>(null);
  const [connected, setConnected] = useState(false);
  const [outline, setOutline] = useState(STARTER_OUTLINE);
  const [issues, setIssues] = useState<ValidationIssue[]>([]);
  const [scene, setScene] = useState<SceneGraph>(() => emptyScene("wbs"));
  const [selected, setSelected] = useState<ElementRef | null>(null);
  const [busy, setBusy] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [undoDepth, setUndoDepth] = useState(0);
  const [redoDepth, setRedoDepth] = useState(0);

  useEffect(() => {
    applyTheme(readTheme());
  }, []);

  const refreshScene = useCallback(async (target: ViewKind) => {
    if (!hasTauri()) return;
    const graph = await ipc.sceneGet(target);
    setScene(graph);
  }, []);

  useEffect(() => {
    if (!hasTauri()) return;
    void (async () => {
      try {
        const state = await ipc.sessionState();
        setSession(state);
        setIssues(state.issues);
        setConnected(true);
        const text = await ipc.outlineTextGet();
        if (state.counts.tasks > 0) setOutline(text.text);
      } catch {
        setConnected(false);
      }
    })();
  }, []);

  const generate = useCallback(async () => {
    if (!hasTauri()) return;
    setBusy(true);
    try {
      const result = await ipc.outlineTextApply(outline);
      setIssues(result.issues);
      setUndoDepth(result.undo_depth);
      setRedoDepth(result.redo_depth);
      const state = await ipc.sessionState();
      setSession(state);
      await refreshScene(view);
    } finally {
      setBusy(false);
    }
  }, [outline, view, refreshScene]);

  // Pulls text, session state and the active scene back into the UI after any
  // core-side change.
  const refreshAll = useCallback(async () => {
    const [text, state] = await Promise.all([ipc.outlineTextGet(), ipc.sessionState()]);
    setOutline(text.text);
    setSession(state);
    setIssues(state.issues);
    setUndoDepth(state.undo_depth);
    setRedoDepth(state.redo_depth);
    await refreshScene(view);
  }, [view, refreshScene]);

  const doOpen = useCallback(async () => {
    if (!hasTauri()) return;
    const opened = await openPlan();
    if (!opened) return;
    await refreshAll();
  }, [refreshAll]);

  const doSave = useCallback(
    async (mode: "save" | "save-as" = "save") => {
      if (!hasTauri()) return null;
      const path = mode === "save-as" ? await savePlanAs() : await savePlan(session?.path);
      if (path) await refreshAll();
      return path;
    },
    [session?.path, refreshAll],
  );

  // External-modification guard: never overwrite either side silently.
  useEffect(() => {
    if (!hasTauri()) return;
    const onFocus = () => {
      void (async () => {
        const check = await ipc.fileCheckExternal();
        if (check.status === "modified") {
          const reload = window.confirm(
            "磁盘上的文件已被其他程序修改。要重新加载吗？（取消则保留内存中的版本）",
          );
          if (reload && check.path) {
            await ipc.sessionOpen(check.path);
            await refreshAll();
          }
        } else if (check.status === "missing") {
          window.alert("原文件已不存在。下次保存时请重新选择路径。");
        }
      })();
    };
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("focus", onFocus);
    };
  }, [refreshAll]);

  // Unsaved-changes guard (spec FR-016): save / discard / cancel.
  const confirmClose = useCallback(async (): Promise<boolean> => {
    if (!hasTauri()) return true;
    const status = await ipc.appCloseCheck();
    if (!status.dirty) return true;
    const choice = askUnsavedChoice(session?.title ?? "未命名规划");
    if (choice === "cancel") return false;
    if (choice === "save") return (await doSave()) !== null;
    return true;
  }, [session?.title, doSave]);

  useEffect(() => {
    if (!hasTauri()) return;
    const onBeforeUnload = (event: BeforeUnloadEvent) => {
      if (!session?.dirty) return;
      event.preventDefault();
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => {
      window.removeEventListener("beforeunload", onBeforeUnload);
    };
  }, [session?.dirty]);

  // The shell's close handler consults this guard before destroying the window.
  useEffect(() => {
    const target = window as typeof window & { __mcmConfirmClose?: () => Promise<boolean> };
    target.__mcmConfirmClose = confirmClose;
    return () => {
      delete target.__mcmConfirmClose;
    };
  }, [confirmClose]);

  // Every in-view edit funnels through one command path so the core stays the
  // single source of truth (宪法 IV).
  const runCommand = useCallback(
    (command: EditCommand) => {
      if (!hasTauri()) return;
      void (async () => {
        const result = await ipc.editApply(command);
        setIssues(result.issues);
        setUndoDepth(result.undo_depth);
        setRedoDepth(result.redo_depth);
        const [text, state] = await Promise.all([ipc.outlineTextGet(), ipc.sessionState()]);
        setOutline(text.text);
        setSession(state);
        await refreshScene(view);
      })();
    },
    [view, refreshScene],
  );

  // Undo/redo share one path: apply, then refresh issues, text and scene.
  const history = useCallback(
    async (direction: "undo" | "redo") => {
      if (!hasTauri()) return;
      const result = direction === "undo" ? await ipc.undo() : await ipc.redo();
      setIssues(result.issues);
      setUndoDepth(result.undo_depth);
      setRedoDepth(result.redo_depth);
      const [text, state] = await Promise.all([ipc.outlineTextGet(), ipc.sessionState()]);
      setOutline(text.text);
      setSession(state);
      await refreshScene(view);
    },
    [view, refreshScene],
  );

  useEffect(() => {
    void refreshScene(view);
  }, [view, refreshScene]);

  const onShortcut = useCallback(
    (id: ShortcutId) => {
      switch (id) {
        case "view.wbs":
          setView("wbs");
          break;
        case "view.graph":
          setView("dep_graph");
          break;
        case "view.timeline":
          setView("timeline");
          break;
        case "view.milestones":
          setView("milestones");
          break;
        case "file.new":
          void (async () => {
            if (await confirmClose()) {
              await ipc.sessionNew();
              await refreshAll();
            }
          })();
          break;
        case "file.open":
          void doOpen();
          break;
        case "file.save":
          void doSave();
          break;
        case "file.saveAs":
          void doSave("save-as");
          break;
        case "file.export":
          setExportOpen(true);
          break;
        case "edit.generate":
          void generate();
          break;
        case "app.search":
          document.querySelector<HTMLInputElement>('input[aria-label="搜索任务"]')?.focus();
          break;
        case "app.shortcuts":
          setShortcutsOpen(true);
          break;
        case "edit.undo":
          void history("undo");
          break;
        case "edit.redo":
          void history("redo");
          break;
        case "app.theme":
          toggleTheme();
          break;
        default:
          break;
      }
    },
    [history, doOpen, doSave, generate, confirmClose, refreshAll],
  );

  useEffect(() => registerShortcuts(onShortcut), [onShortcut]);

  const errorCount = issues.filter((i) => i.severity === "error").length;

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="eyebrow">MCM</span>
          <strong>项目规划</strong>
        </div>
        <SearchBar revision={session?.revision ?? 0} onLocate={setSelected} />
        <nav aria-label="视图">
          {VIEW_TABS.map((tab) => (
            <button
              key={tab.id}
              type="button"
              className={tab.id === view ? "tab tab-active" : "tab"}
              aria-current={tab.id === view}
              onClick={() => {
                setView(tab.id);
              }}
            >
              {tab.label}
            </button>
          ))}
        </nav>
        <button
          type="button"
          className="tab theme-toggle"
          onClick={() => {
            toggleTheme();
          }}
        >
          切换主题
        </button>
      </aside>

      <main className="workspace">
        <Toolbar
          onExport={() => {
            setExportOpen(true);
          }}
          onOpen={() => {
            void doOpen();
          }}
          onSave={() => {
            void doSave();
          }}
          dirty={session?.dirty ?? false}
          undoDepth={undoDepth}
          redoDepth={redoDepth}
          onUndo={() => {
            void history("undo");
          }}
          onRedo={() => {
            void history("redo");
          }}
          onGenerate={() => {
            void generate();
          }}
          busy={busy}
        />
        <OutlineEditor text={outline} issues={issues} onChange={setOutline} />

        <div className="view-stage">
          {scene.nodes.length > 0 ? (
            renderView(view, scene, selected, setSelected, runCommand)
          ) : (
            <section className="empty-state">
              <span className="eyebrow">{VIEW_TABS.find((t) => t.id === view)?.label}</span>
              <h1>把项目描述，变成可执行的地图。</h1>
              <p>在左侧写下结构化大纲，点击「生成规划」即可看到经校验的规划视图。</p>
            </section>
          )}
        </div>

        <IssuesPanel issues={issues} onLocate={setSelected} />
      </main>

      <footer className="status-bar">
        <span>{session ? session.title : "未加载规划"}</span>
        <span>任务 {session?.counts.tasks ?? 0}</span>
        <span>问题 {errorCount}</span>
        {selected && <span>已选中 {selected.kind}</span>}
        <span className="status-conn">{connected ? "核心已连接" : "前端预览模式"}</span>
      </footer>

      <ShortcutsHelp
        open={shortcutsOpen}
        onClose={() => {
          setShortcutsOpen(false);
        }}
      />

      <ExportDialog
        open={exportOpen}
        onClose={() => {
          setExportOpen(false);
        }}
      />

      {import.meta.env.DEV && <PerfOverlay nodeCount={scene.nodes.length} />}
    </div>
  );
}
