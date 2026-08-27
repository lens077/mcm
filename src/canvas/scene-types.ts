// TS mirror of crates/mcm-core scene projection (data-model.md §SceneGraph).
import type { ElementRef, ViewKind } from "../ipc/types";

/** Semantic role resolved to concrete colors by design tokens, never inline. */
export type StyleRole =
  | "task"
  | "task_done"
  | "task_error"
  | "task_warning"
  | "milestone"
  | "hierarchy_edge"
  | "dependency_edge"
  | "milestone_edge";

export type BadgeKind = "error" | "warning" | "done" | "milestone";

export interface SceneNode {
  ref: ElementRef;
  x: number;
  y: number;
  w: number;
  h: number;
  style_role: StyleRole;
  text: string;
  sub_text?: string;
  badges: BadgeKind[];
}

export interface SceneEdge {
  from: ElementRef;
  to: ElementRef;
  points: number[];
  style_role: StyleRole;
}

export interface SceneBounds {
  min_x: number;
  min_y: number;
  max_x: number;
  max_y: number;
}

export interface SceneGraph {
  view: ViewKind;
  nodes: SceneNode[];
  edges: SceneEdge[];
  bounds: SceneBounds;
}

export const EMPTY_BOUNDS: SceneBounds = { min_x: 0, min_y: 0, max_x: 0, max_y: 0 };

export function emptyScene(view: ViewKind): SceneGraph {
  return { view, nodes: [], edges: [], bounds: EMPTY_BOUNDS };
}

export function refKey(ref: ElementRef): string {
  switch (ref.kind) {
    case "plan":
      return "plan";
    case "task":
      return `t${String(ref.id)}`;
    case "milestone":
      return `m${String(ref.id)}`;
    case "dependency":
      return `d${String(ref.predecessor)}->${String(ref.successor)}`;
    case "line":
      return `l${String(ref.line)}`;
  }
}
