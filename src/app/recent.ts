// Recent-file helpers. View state is keyed by path and stored in prefs, never
// inside the `.mcm` file (contracts/plan-file-format.md §视图状态).
import type { Prefs, ViewKind } from "../ipc/types";

export interface RecentEntry {
  path: string;
  /** Final path component, for display. */
  label: string;
}

export function recentEntries(prefs: Prefs): RecentEntry[] {
  return prefs.recent_files.map((path) => ({ path, label: basename(path) }));
}

export function basename(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

export interface FileViewState {
  view?: ViewKind;
  scale?: number;
}

export function readViewState(prefs: Prefs, path: string | undefined): FileViewState {
  if (!path) return {};
  const raw = prefs.view_state[path];
  if (typeof raw !== "object" || raw === null) return {};
  const record = raw as Record<string, unknown>;
  const state: FileViewState = {};
  if (typeof record.view === "string") state.view = record.view as ViewKind;
  if (typeof record.scale === "number") state.scale = record.scale;
  return state;
}

/** Returns a copy of prefs with this file's view state merged in. */
export function withViewState(prefs: Prefs, path: string, state: FileViewState): Prefs {
  return {
    ...prefs,
    view_state: { ...prefs.view_state, [path]: { ...readViewState(prefs, path), ...state } },
  };
}
