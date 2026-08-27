import { describe, expect, it } from "vitest";
import { centreOn, findSelectedNode, isOffscreen, sameRef } from "./selection";
import type { SceneGraph, SceneNode } from "../canvas/scene-types";

const node = (id: number, x: number, y: number): SceneNode => ({
  ref: { kind: "task", id },
  x,
  y,
  w: 100,
  h: 40,
  style_role: "task",
  text: `t${String(id)}`,
  badges: [],
});

const scene: SceneGraph = {
  view: "wbs",
  nodes: [node(1, 0, 0), node(2, 4000, 3000)],
  edges: [],
  bounds: { min_x: 0, min_y: 0, max_x: 4100, max_y: 3040 },
};

describe("selection identity", () => {
  it("compares refs structurally", () => {
    expect(sameRef({ kind: "task", id: 1 }, { kind: "task", id: 1 })).toBe(true);
    expect(sameRef({ kind: "task", id: 1 }, { kind: "task", id: 2 })).toBe(false);
    expect(sameRef({ kind: "task", id: 1 }, { kind: "milestone", id: 1 })).toBe(false);
    expect(sameRef(null, null)).toBe(true);
    expect(sameRef(null, { kind: "task", id: 1 })).toBe(false);
  });
});

describe("cross-view location", () => {
  it("finds the selected node in a scene that shows it", () => {
    const found = findSelectedNode(scene, { kind: "task", id: 2 });
    expect(found?.x).toBe(4000);
  });

  it("returns null when this view does not show the selection", () => {
    expect(findSelectedNode(scene, { kind: "task", id: 99 })).toBeNull();
    expect(findSelectedNode(scene, null)).toBeNull();
  });

  it("detects off-screen nodes", () => {
    const viewport = { scale: 1, offsetX: 0, offsetY: 0 };
    const far = scene.nodes[1];
    expect(far).toBeDefined();
    if (far) expect(isOffscreen(far, viewport, 800, 600)).toBe(true);
    const near = scene.nodes[0];
    expect(near).toBeDefined();
    if (near) expect(isOffscreen(near, viewport, 800, 600)).toBe(false);
  });

  it("centres the viewport on a node without changing scale", () => {
    const target = scene.nodes[1];
    expect(target).toBeDefined();
    if (!target) return;
    const viewport = centreOn(target, 800, 600, 1);
    expect(viewport.scale).toBe(1);
    // The node centre lands in the middle of the viewport.
    const centreX = (target.x + target.w / 2) * viewport.scale + viewport.offsetX;
    const centreY = (target.y + target.h / 2) * viewport.scale + viewport.offsetY;
    expect(centreX).toBeCloseTo(400);
    expect(centreY).toBeCloseTo(300);
    // And it is no longer off-screen.
    expect(isOffscreen(target, viewport, 800, 600)).toBe(false);
  });
});
