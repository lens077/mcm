import type { SceneEdge, SceneGraph, SceneNode } from "./scene-types";

export interface Point {
  x: number;
  y: number;
}

export function nodeContains(node: SceneNode, point: Point): boolean {
  return (
    point.x >= node.x &&
    point.x <= node.x + node.w &&
    point.y >= node.y &&
    point.y <= node.y + node.h
  );
}

/** Topmost node under the point; later nodes win, matching draw order. */
export function hitNode(scene: SceneGraph, point: Point): SceneNode | null {
  for (let i = scene.nodes.length - 1; i >= 0; i -= 1) {
    const node = scene.nodes[i];
    if (node && nodeContains(node, point)) return node;
  }
  return null;
}

function distanceToSegment(point: Point, ax: number, ay: number, bx: number, by: number): number {
  const dx = bx - ax;
  const dy = by - ay;
  const lengthSq = dx * dx + dy * dy;
  if (lengthSq === 0) return Math.hypot(point.x - ax, point.y - ay);
  let t = ((point.x - ax) * dx + (point.y - ay) * dy) / lengthSq;
  t = Math.max(0, Math.min(1, t));
  return Math.hypot(point.x - (ax + t * dx), point.y - (ay + t * dy));
}

/** Nearest edge within `tolerance` logical units, or null. */
export function hitEdge(scene: SceneGraph, point: Point, tolerance = 6): SceneEdge | null {
  let best: SceneEdge | null = null;
  let bestDistance = tolerance;
  for (const edge of scene.edges) {
    for (let i = 0; i + 3 < edge.points.length; i += 2) {
      const ax = edge.points[i];
      const ay = edge.points[i + 1];
      const bx = edge.points[i + 2];
      const by = edge.points[i + 3];
      if (ax === undefined || ay === undefined || bx === undefined || by === undefined) continue;
      const distance = distanceToSegment(point, ax, ay, bx, by);
      if (distance <= bestDistance) {
        bestDistance = distance;
        best = edge;
      }
    }
  }
  return best;
}
