// Single source of truth for the selected element (spec FR-007): the selection
// survives view switches and each view re-locates it in its own geometry.
import type { ElementRef } from "../ipc/types";
import type { SceneGraph, SceneNode } from "../canvas/scene-types";
import { refKey } from "../canvas/scene-types";
import { type Viewport, clampScale } from "../canvas/renderer";

export function sameRef(a: ElementRef | null, b: ElementRef | null): boolean {
  if (a === null || b === null) return a === b;
  return refKey(a) === refKey(b);
}

/** Finds the selected element inside a scene, if that view shows it. */
export function findSelectedNode(scene: SceneGraph, selected: ElementRef | null): SceneNode | null {
  if (!selected) return null;
  const key = refKey(selected);
  return scene.nodes.find((node) => refKey(node.ref) === key) ?? null;
}

/**
 * Viewport that keeps the current scale but centres the selected node, so
 * switching views auto-locates the selection instead of losing it.
 */
export function centreOn(
  node: SceneNode,
  width: number,
  height: number,
  scale: number,
): Viewport {
  const safeScale = clampScale(scale);
  return {
    scale: safeScale,
    offsetX: width / 2 - (node.x + node.w / 2) * safeScale,
    offsetY: height / 2 - (node.y + node.h / 2) * safeScale,
  };
}

/** True when the node is fully outside the visible rectangle. */
export function isOffscreen(
  node: SceneNode,
  viewport: Viewport,
  width: number,
  height: number,
): boolean {
  const left = node.x * viewport.scale + viewport.offsetX;
  const top = node.y * viewport.scale + viewport.offsetY;
  const right = left + node.w * viewport.scale;
  const bottom = top + node.h * viewport.scale;
  return right < 0 || bottom < 0 || left > width || top > height;
}
