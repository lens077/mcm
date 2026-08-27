// File actions shared by the menu, the toolbar and the close guard.
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { ipc } from "../ipc/client";
import type { SessionState } from "../ipc/types";

const FILTERS = [{ name: "MCM 规划", extensions: ["mcm"] }];

/** Prompts for a `.mcm` file and opens it; returns null when cancelled. */
export async function openPlan(): Promise<SessionState | null> {
  const selected = await openDialog({ multiple: false, filters: FILTERS });
  if (typeof selected !== "string") return null;
  return await ipc.sessionOpen(selected);
}

/**
 * Saves to the current path, or asks for one when the document is new.
 * Returns the path, or null when the user cancelled the dialog.
 */
export async function savePlan(currentPath: string | undefined): Promise<string | null> {
  if (currentPath) {
    const result = await ipc.sessionSave(currentPath);
    return result.path;
  }
  const chosen = await saveDialog({ defaultPath: "规划.mcm", filters: FILTERS });
  if (typeof chosen !== "string") return null;
  const result = await ipc.sessionSave(chosen);
  return result.path;
}

/** Save-as always asks, even when the document already has a path. */
export async function savePlanAs(): Promise<string | null> {
  const chosen = await saveDialog({ defaultPath: "规划.mcm", filters: FILTERS });
  if (typeof chosen !== "string") return null;
  const result = await ipc.sessionSave(chosen);
  return result.path;
}

export type CloseChoice = "save" | "discard" | "cancel";

/**
 * Three-way unsaved-changes guard (spec FR-016). Uses the browser dialogs the
 * WebView provides: confirm → save, cancel → ask whether to discard.
 */
export function askUnsavedChoice(title: string): CloseChoice {
  const save = window.confirm(`「${title}」有未保存的更改。要先保存吗？`);
  if (save) return "save";
  return window.confirm("放弃未保存的更改？") ? "discard" : "cancel";
}
