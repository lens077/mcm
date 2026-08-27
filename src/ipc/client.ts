import { invoke } from "@tauri-apps/api/core";
import type {
  ApplyResult,
  EditCommand,
  ExportFormat,
  ExportPrecheck,
  ExportReport,
  ExternalCheck,
  Prefs,
  SaveResult,
  SearchMatch,
  SessionState,
  ValidationIssue,
  ViewKind,
} from "./types";
import { isCommandError } from "./types";
import type { SceneGraph } from "../canvas/scene-types";

/** True when running inside the Tauri shell (vs. a plain browser dev server). */
export function hasTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    if (isCommandError(raw)) throw raw;
    throw { code: "E_INTERNAL", message: String(raw) };
  }
}

export const ipc = {
  sessionNew: () => call<SessionState>("session_new"),
  sessionState: () => call<SessionState>("session_state"),
  issuesGet: () => call<ValidationIssue[]>("issues_get"),
  appCloseCheck: () => call<{ dirty: boolean }>("app_close_check"),
  outlineTextGet: () => call<{ text: string }>("outline_text_get"),
  outlineTextApply: (text: string) => call<ApplyResult>("outline_text_apply", { text }),
  sceneGet: (view: ViewKind) => call<SceneGraph>("scene_get", { view }),
  search: (query: string) => call<{ matches: SearchMatch[] }>("search", { query }),
  editApply: (command: EditCommand) => call<ApplyResult>("edit_apply", { command }),
  undo: () => call<ApplyResult>("undo"),
  redo: () => call<ApplyResult>("redo"),
  sessionOpen: (path: string) => call<SessionState>("session_open", { path }),
  sessionSave: (path?: string) => call<SaveResult>("session_save", { path: path ?? null }),
  fileCheckExternal: () => call<ExternalCheck>("file_check_external"),
  prefsGet: () => call<Prefs>("prefs_get"),
  prefsSet: (prefs: Prefs) => call<Prefs>("prefs_set", { prefs }),
  exportPrecheck: () => call<ExportPrecheck>("export_precheck"),
  exportRun: (format: ExportFormat, path: string) =>
    call<ExportReport>("export_run", { format, path }),
};
