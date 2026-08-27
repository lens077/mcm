export type ShortcutId =
  | "view.wbs"
  | "view.graph"
  | "view.timeline"
  | "view.milestones"
  | "edit.undo"
  | "edit.redo"
  | "edit.generate"
  | "file.new"
  | "file.open"
  | "file.save"
  | "file.saveAs"
  | "file.export"
  | "app.search"
  | "app.theme"
  | "app.shortcuts";

/** Grouping used by the shortcut reference dialog. */
export type ShortcutGroup = "文件" | "编辑" | "视图" | "应用";

export interface Shortcut {
  id: ShortcutId;
  /** Lower-case `event.key`. */
  key: string;
  /** Cmd on macOS, Ctrl elsewhere. */
  mod?: boolean;
  shift?: boolean;
  label: string;
  group: ShortcutGroup;
}

export const isMac = (): boolean =>
  typeof navigator !== "undefined" && /mac/i.test(navigator.platform || navigator.userAgent);

export const SHORTCUTS: Shortcut[] = [
  { id: "file.new", key: "n", mod: true, label: "新建规划", group: "文件" },
  { id: "file.open", key: "o", mod: true, label: "打开规划", group: "文件" },
  { id: "file.save", key: "s", mod: true, label: "保存规划", group: "文件" },
  { id: "file.saveAs", key: "s", mod: true, shift: true, label: "另存为", group: "文件" },
  { id: "file.export", key: "e", mod: true, label: "导出规划", group: "文件" },
  { id: "edit.generate", key: "enter", mod: true, label: "生成规划", group: "编辑" },
  { id: "edit.undo", key: "z", mod: true, label: "撤销", group: "编辑" },
  { id: "edit.redo", key: "z", mod: true, shift: true, label: "重做", group: "编辑" },
  { id: "view.wbs", key: "1", mod: true, label: "任务分解视图", group: "视图" },
  { id: "view.graph", key: "2", mod: true, label: "依赖网络视图", group: "视图" },
  { id: "view.timeline", key: "3", mod: true, label: "时间线视图", group: "视图" },
  { id: "view.milestones", key: "4", mod: true, label: "里程碑视图", group: "视图" },
  { id: "app.search", key: "f", mod: true, label: "搜索任务", group: "应用" },
  { id: "app.theme", key: "j", mod: true, label: "切换深浅主题", group: "应用" },
  { id: "app.shortcuts", key: "/", mod: true, label: "快捷键速查", group: "应用" },
];

/** Shortcuts grouped for display, preserving declaration order. */
export function grouped(): { group: ShortcutGroup; items: Shortcut[] }[] {
  const order: ShortcutGroup[] = ["文件", "编辑", "视图", "应用"];
  return order.map((group) => ({
    group,
    items: SHORTCUTS.filter((shortcut) => shortcut.group === group),
  }));
}

/** Human-readable names for keys whose raw value reads poorly. */
const KEY_NAMES: Record<string, string> = {
  enter: "↵",
  "/": "/",
};

/** Renders a platform-correct label such as `⌘1` or `Ctrl+1`. */
export function describe(shortcut: Shortcut): string {
  const parts: string[] = [];
  if (shortcut.mod) parts.push(isMac() ? "⌘" : "Ctrl");
  if (shortcut.shift) parts.push(isMac() ? "⇧" : "Shift");
  parts.push(KEY_NAMES[shortcut.key] ?? shortcut.key.toUpperCase());
  return isMac() ? parts.join("") : parts.join("+");
}

export function matches(shortcut: Shortcut, event: KeyboardEvent): boolean {
  const modPressed = isMac() ? event.metaKey : event.ctrlKey;
  return (
    event.key.toLowerCase() === shortcut.key &&
    modPressed === Boolean(shortcut.mod) &&
    event.shiftKey === Boolean(shortcut.shift)
  );
}

export function resolve(event: KeyboardEvent): ShortcutId | null {
  // Longest-specific first: redo (with shift) must win over undo.
  const ordered = [...SHORTCUTS].sort((a, b) => Number(b.shift ?? false) - Number(a.shift ?? false));
  for (const shortcut of ordered) {
    if (matches(shortcut, event)) return shortcut.id;
  }
  return null;
}

/** Registers a global handler; returns the disposer. */
export function registerShortcuts(handler: (id: ShortcutId) => void): () => void {
  const onKeyDown = (event: KeyboardEvent) => {
    const id = resolve(event);
    if (!id) return;
    event.preventDefault();
    handler(id);
  };
  window.addEventListener("keydown", onKeyDown);
  return () => {
    window.removeEventListener("keydown", onKeyDown);
  };
}
