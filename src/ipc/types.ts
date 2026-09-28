// Mirrors contracts/ipc-commands.md and the Rust serde representations.
// Keep field names snake_case: they cross the IPC boundary verbatim.

export type ViewKind = "wbs" | "dep_graph" | "timeline" | "milestones";

export type Severity = "error" | "warning";

export type ElementRef =
  | { kind: "plan" }
  | { kind: "task"; id: number }
  | { kind: "dependency"; predecessor: number; successor: number }
  | { kind: "milestone"; id: number }
  | { kind: "line"; line: number };

export interface ValidationIssue {
  severity: Severity;
  code: string;
  target: ElementRef;
  message: string;
  fix_hint: string;
  cycle_path?: number[];
}

export interface PlanCounts {
  tasks: number;
  dependencies: number;
  milestones: number;
}

export interface SessionState {
  path?: string;
  dirty: boolean;
  title: string;
  revision: number;
  counts: PlanCounts;
  issues: ValidationIssue[];
  undo_depth: number;
  redo_depth: number;
}

/** Mirrors mcm_core::edit::EditCommand (serde tag = "kind"). */
export type EditCommand =
  | { kind: "add_task"; parent: number | null; index: number; title: string }
  | { kind: "rename_task"; id: number; title: string }
  | { kind: "delete_task"; id: number }
  | { kind: "move_task"; id: number; new_parent: number | null; index: number }
  | { kind: "set_schedule"; id: number; schedule: Schedule }
  | { kind: "set_assignee"; id: number; assignee: string | null }
  | { kind: "set_notes"; id: number; notes: string | null }
  | { kind: "set_done"; id: number; done: boolean }
  | { kind: "add_dependency"; predecessor: number; successor: number }
  | { kind: "remove_dependency"; predecessor: number; successor: number }
  | { kind: "add_milestone"; name: string; date: string; linked_tasks: number[] }
  | { kind: "update_milestone"; id: number; name: string; date: string; linked_tasks: number[] }
  | { kind: "remove_milestone"; id: number }
  | {
      kind: "set_plan_meta";
      title: string;
      description: string | null;
      project_start: string | null;
    }
  | { kind: "replace_from_outline"; text: string };

export type Schedule =
  | { kind: "none" }
  | { kind: "explicit"; start: string; end: string }
  | { kind: "duration"; days: number };

export type ExportFormat = "xmind" | "vsdx";

export interface MappedItem {
  kind: string;
  count: number;
  representation: string;
}

export interface DegradedItem {
  element: string;
  original: string;
  fallback: string;
}

export interface ExportReport {
  format: ExportFormat;
  output_path: string;
  mapped: MappedItem[];
  degraded: DegradedItem[];
  warnings: string[];
}

export interface ExportPrecheck {
  ok: boolean;
  error_count: number;
}

export type ExternalChange = "none" | "modified" | "missing";

export interface ExternalCheck {
  status: ExternalChange;
  path?: string;
}

export interface SaveResult {
  path: string;
  saved: boolean;
}

/** OCR recognizer for image import (mirrors `mcm_import::OcrModel`). */
export type OcrModel = "fast" | "accurate";

export interface Prefs {
  theme?: string | null;
  recent_files: string[];
  view_state: Record<string, unknown>;
  ocr_model?: OcrModel;
}

export interface OcrModelStatus {
  accurate_installed: boolean;
  accurate_download_bytes: number;
  /** Where the model files live; offline users can copy them here. */
  accurate_dir: string;
  downloading: boolean;
}

/** Payload of the `ocr-model-download` event. */
export interface DownloadProgress {
  done: number;
  total: number;
}

export interface SearchMatch {
  ref: ElementRef;
  title: string;
  snippet: string;
}

/** Mirrors `mcm_import::ImportReport`. */
export interface ImportReport {
  nodes: number;
  groups: number;
  dependencies: number;
  undirected: number;
  cycle_breaks: string[];
  loose_text: string[];
  edge_labels: string[];
  /** Arrows between a group and its own member, which a plan cannot hold. */
  hierarchy_links: string[];
  untitled_nodes: number;
}

/** Mirrors `mcm_import::GraphSpec`: a graph parsed in the webview (Mermaid). */
export interface GraphSpec {
  title: string;
  nodes: { id: string; label: string; detail: string[] }[];
  groups: { id: string; label: string; members: string[] }[];
  edges: { from: string; to: string; directed: boolean; label: string | null }[];
}

/** Payload of `diagram_source_read`. */
export interface DiagramSource {
  text: string;
  /** File name without extension. */
  name: string;
}

export interface DiagramImport {
  outline: string;
  report: ImportReport;
  elapsed_ms: number;
}

export interface ApplyResult {
  revision: number;
  issues: ValidationIssue[];
  dirty: boolean;
  scene_stale: ViewKind[];
  undo_depth: number;
  redo_depth: number;
}

export type ErrorCode =
  | "E_NEED_PATH"
  | "E_FILE_IO"
  | "E_VERSION_TOO_NEW"
  | "E_BAD_TARGET"
  | "E_EXPORT_IO"
  | "E_IMPORT"
  | "E_MODEL_MISSING"
  | "E_DOWNLOAD"
  | "E_BUSY"
  | "E_INTERNAL";

export interface CommandError {
  code: ErrorCode;
  message: string;
  details?: unknown;
}

export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as CommandError).code === "string" &&
    typeof (value as CommandError).message === "string"
  );
}
