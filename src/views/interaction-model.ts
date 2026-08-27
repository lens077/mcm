// Pure interaction logic shared by the view components. Keeping it out of the
// React files makes every gesture rule directly testable.
import type { EditCommand, ElementRef } from "../ipc/types";
import type { SceneGraph, SceneNode } from "../canvas/scene-types";
import { refKey } from "../canvas/scene-types";

export interface Point {
  x: number;
  y: number;
}

// ----------------------------------------------------------------- WBS ---

/** Where a dragged task lands relative to the node under the cursor. */
export type DropMode = "before" | "after" | "child";

/**
 * Vertical thirds decide the drop: top → before, bottom → after, middle →
 * child, which is the convention outline editors use.
 */
export function dropModeFor(target: SceneNode, point: Point): DropMode {
  const offset = (point.y - target.y) / Math.max(1, target.h);
  if (offset < 0.28) return "before";
  if (offset > 0.72) return "after";
  return "child";
}

/** Sibling index of `id` among the children of `parent` in a WBS scene. */
export function siblingIndex(scene: SceneGraph, parentKey: string | null, id: string): number {
  const siblings = scene.nodes.filter((node) => parentKeyOf(scene, node) === parentKey);
  return siblings.findIndex((node) => refKey(node.ref) === id);
}

/** Parent of a node, derived from the hierarchy edges in the scene. */
export function parentKeyOf(scene: SceneGraph, node: SceneNode): string | null {
  const key = refKey(node.ref);
  const edge = scene.edges.find(
    (candidate) => candidate.style_role === "hierarchy_edge" && refKey(candidate.to) === key,
  );
  return edge ? refKey(edge.from) : null;
}

function taskId(ref: ElementRef): number | null {
  return ref.kind === "task" ? ref.id : null;
}

/** Builds the MoveTask command for a drag-and-drop gesture, or null. */
export function moveCommandFor(
  scene: SceneGraph,
  dragged: SceneNode,
  target: SceneNode,
  mode: DropMode,
): EditCommand | null {
  const draggedId = taskId(dragged.ref);
  const targetId = taskId(target.ref);
  if (draggedId === null || targetId === null) return null;
  if (draggedId === targetId) return null;
  // Dropping a task into its own subtree would orphan it.
  if (isDescendant(scene, refKey(target.ref), refKey(dragged.ref))) return null;

  if (mode === "child") {
    return { kind: "move_task", id: draggedId, new_parent: targetId, index: 0 };
  }
  const parentKey = parentKeyOf(scene, target);
  const parentNode = parentKey
    ? scene.nodes.find((node) => refKey(node.ref) === parentKey)
    : undefined;
  const parentId = parentNode ? taskId(parentNode.ref) : null;
  const index = siblingIndex(scene, parentKey, refKey(target.ref));
  return {
    kind: "move_task",
    id: draggedId,
    new_parent: parentId,
    index: mode === "before" ? Math.max(0, index) : index + 1,
  };
}

/** True when `candidateKey` sits inside the subtree rooted at `rootKey`. */
export function isDescendant(scene: SceneGraph, candidateKey: string, rootKey: string): boolean {
  let cursor: string | null = candidateKey;
  // Guard against malformed scenes.
  let guard = scene.nodes.length + 1;
  while (cursor && guard > 0) {
    if (cursor === rootKey) return true;
    guard -= 1;
    const node = scene.nodes.find((candidate) => refKey(candidate.ref) === cursor);
    cursor = node ? parentKeyOf(scene, node) : null;
  }
  return false;
}

// --------------------------------------------------------------- graph ---

/** Anchor hot-zone on the right edge of a node, used to start a link drag. */
export function isOnLinkAnchor(node: SceneNode, point: Point, radius = 12): boolean {
  const anchorX = node.x + node.w;
  const anchorY = node.y + node.h / 2;
  return Math.hypot(point.x - anchorX, point.y - anchorY) <= radius;
}

/** Builds the AddDependency command for a link gesture, or null. */
export function linkCommandFor(from: SceneNode, to: SceneNode): EditCommand | null {
  const predecessor = taskId(from.ref);
  const successor = taskId(to.ref);
  if (predecessor === null || successor === null) return null;
  if (predecessor === successor) return null;
  return { kind: "add_dependency", predecessor, successor };
}

/** Builds the RemoveDependency command for a selected edge, or null. */
export function unlinkCommandFor(edge: {
  from: ElementRef;
  to: ElementRef;
}): EditCommand | null {
  const predecessor = taskId(edge.from);
  const successor = taskId(edge.to);
  if (predecessor === null || successor === null) return null;
  return { kind: "remove_dependency", predecessor, successor };
}

// ------------------------------------------------------------ timeline ---

/** Which part of a timeline bar a press grabbed. */
export type BarGrip = "move" | "start" | "end";

export function gripFor(bar: SceneNode, point: Point, edgeWidth = 8): BarGrip {
  if (point.x - bar.x <= edgeWidth) return "start";
  if (bar.x + bar.w - point.x <= edgeWidth) return "end";
  return "move";
}

/** Converts a horizontal pixel delta into whole days. */
export function daysFromDelta(deltaX: number, dayWidth: number): number {
  if (dayWidth <= 0) return 0;
  return Math.round(deltaX / dayWidth);
}

/** Shifts an ISO date by `days`, staying in UTC to avoid DST drift. */
export function shiftIsoDate(iso: string, days: number): string {
  const date = new Date(`${iso}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return iso;
  date.setUTCDate(date.getUTCDate() + days);
  return date.toISOString().slice(0, 10);
}

/** Builds the SetSchedule command for a bar drag, or null when nothing moved. */
export function scheduleCommandFor(
  bar: SceneNode,
  grip: BarGrip,
  days: number,
): EditCommand | null {
  const id = taskId(bar.ref);
  if (id === null || days === 0) return null;
  // sub_text carries the derived window as `start..end`.
  const range = bar.sub_text?.split("..");
  if (!range || range.length !== 2) return null;
  const [start, end] = range;
  if (!start || !end) return null;

  const nextStart = grip === "end" ? start : shiftIsoDate(start, days);
  const nextEnd = grip === "start" ? end : shiftIsoDate(end, days);
  // Refuse inversions; the user can drag the other edge instead.
  if (nextStart > nextEnd) return null;
  return {
    kind: "set_schedule",
    id,
    schedule: { kind: "explicit", start: nextStart, end: nextEnd },
  };
}

// --------------------------------------------------------------- shared ---

/** Delete gesture for whatever is selected, or null when nothing applies. */
export function deleteCommandFor(selected: ElementRef | null): EditCommand | null {
  if (!selected) return null;
  if (selected.kind === "task") return { kind: "delete_task", id: selected.id };
  if (selected.kind === "milestone") return { kind: "remove_milestone", id: selected.id };
  if (selected.kind === "dependency") {
    return {
      kind: "remove_dependency",
      predecessor: selected.predecessor,
      successor: selected.successor,
    };
  }
  return null;
}
