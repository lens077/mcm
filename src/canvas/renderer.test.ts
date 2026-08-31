import { describe, expect, it } from "vitest";
import {
  DEFAULT_VIEWPORT,
  MAX_SCALE,
  MIN_SCALE,
  badgeToken,
  clampScale,
  fitToBounds,
  panBy,
  readToken,
  screenToWorld,
  tokensFor,
  worldToScreen,
  zoomAt,
} from "./renderer";
import { hitEdge, hitNode } from "./hit";
import type { SceneGraph } from "./scene-types";
import { emptyScene, refKey } from "./scene-types";

const scene: SceneGraph = {
  view: "wbs",
  nodes: [
    {
      ref: { kind: "task", id: 1 },
      x: 0,
      y: 0,
      w: 100,
      h: 40,
      style_role: "task",
      text: "A",
      badges: [],
    },
    {
      ref: { kind: "task", id: 2 },
      x: 200,
      y: 100,
      w: 100,
      h: 40,
      style_role: "task",
      text: "B",
      badges: ["error"],
    },
  ],
  edges: [
    {
      from: { kind: "task", id: 1 },
      to: { kind: "task", id: 2 },
      points: [100, 20, 200, 120],
      style_role: "dependency_edge",
    },
  ],
  bounds: { min_x: 0, min_y: 0, max_x: 300, max_y: 140 },
};

describe("viewport math", () => {
  it("round-trips screen and world coordinates", () => {
    const viewport = { scale: 1.5, offsetX: 30, offsetY: -12 };
    const world = screenToWorld(viewport, { x: 120, y: 60 });
    const screen = worldToScreen(viewport, world);
    expect(screen.x).toBeCloseTo(120);
    expect(screen.y).toBeCloseTo(60);
  });

  it("keeps the anchor point stable while zooming", () => {
    const anchor = { x: 200, y: 150 };
    const before = screenToWorld(DEFAULT_VIEWPORT, anchor);
    const zoomed = zoomAt(DEFAULT_VIEWPORT, anchor, 2);
    const after = screenToWorld(zoomed, anchor);
    expect(after.x).toBeCloseTo(before.x);
    expect(after.y).toBeCloseTo(before.y);
    expect(zoomed.scale).toBeCloseTo(2);
  });

  it("clamps scale into the supported range", () => {
    expect(clampScale(1000)).toBe(MAX_SCALE);
    expect(clampScale(0.0001)).toBe(MIN_SCALE);
  });

  it("pans by a delta without touching scale", () => {
    const panned = panBy({ scale: 2, offsetX: 10, offsetY: 10 }, 5, -5);
    expect(panned).toEqual({ scale: 2, offsetX: 15, offsetY: 5 });
  });

  it("fits bounds inside the viewport", () => {
    const viewport = fitToBounds(scene, 800, 600);
    expect(viewport.scale).toBeGreaterThan(0);
    expect(viewport.scale).toBeLessThanOrEqual(MAX_SCALE);
  });
});

describe("hit testing", () => {
  it("finds the node under a point", () => {
    expect(hitNode(scene, { x: 50, y: 20 })?.ref).toEqual({ kind: "task", id: 1 });
    expect(hitNode(scene, { x: 250, y: 120 })?.ref).toEqual({ kind: "task", id: 2 });
    expect(hitNode(scene, { x: 500, y: 500 })).toBeNull();
  });

  it("finds an edge within tolerance and rejects far points", () => {
    expect(hitEdge(scene, { x: 150, y: 70 })).not.toBeNull();
    expect(hitEdge(scene, { x: 150, y: 400 })).toBeNull();
  });
});

describe("style role tokens", () => {
  const roles = [
    "task",
    "task_done",
    "task_error",
    "task_warning",
    "milestone",
    "hierarchy_edge",
    "dependency_edge",
    "milestone_edge",
  ] as const;

  it("maps every style role to fill and stroke tokens", () => {
    for (const role of roles) {
      const tokens = tokensFor(role);
      expect(tokens.fill.startsWith("--"), `${role} fill`).toBe(true);
      expect(tokens.stroke.startsWith("--"), `${role} stroke`).toBe(true);
    }
  });

  it("gives task states visually distinct fills", () => {
    const fills = new Set(
      (["task", "task_done", "task_error", "task_warning", "milestone"] as const).map(
        (role) => tokensFor(role).fill,
      ),
    );
    expect(fills.size).toBe(5);
  });

  it("maps every badge kind to its own token", () => {
    const badges = ["error", "warning", "done", "milestone"] as const;
    const tokens = badges.map(badgeToken);
    expect(new Set(tokens).size).toBe(badges.length);
    for (const token of tokens) expect(token.startsWith("--")).toBe(true);
  });

  it("falls back to a visible color when a token is undefined", () => {
    const root = document.documentElement;
    expect(readToken(root, "--definitely-not-defined")).toBe("#888");
  });
});

describe("scene helpers", () => {
  it("creates an empty scene per view", () => {
    expect(emptyScene("timeline")).toEqual({
      view: "timeline",
      nodes: [],
      edges: [],
      bounds: { min_x: 0, min_y: 0, max_x: 0, max_y: 0 },
    });
  });

  it("derives stable keys per element ref", () => {
    expect(refKey({ kind: "task", id: 3 })).toBe("t3");
    expect(refKey({ kind: "milestone", id: 1 })).toBe("m1");
    expect(refKey({ kind: "dependency", predecessor: 1, successor: 2 })).toBe("d1->2");
    expect(refKey({ kind: "plan" })).toBe("plan");
  });
});

describe("fitToBounds 居中", () => {
  const scene = (w: number, h: number): SceneGraph => ({
    view: "dep_graph",
    nodes: [],
    edges: [],
    bounds: { min_x: 0, min_y: 0, max_x: w, max_y: h },
  });

  it("宽扁内容在纵向居中，而不是贴顶", () => {
    // 回归：依赖网络等宽高比大的内容曾被顶到左上角，画布下方大片留白
    const vp = fitToBounds(scene(1200, 200), 800, 600);
    const drawnH = 200 * vp.scale;
    const top = vp.offsetY;
    const bottom = 600 - (top + drawnH);
    expect(Math.abs(top - bottom)).toBeLessThan(1);
  });

  it("高窄内容在横向居中", () => {
    const vp = fitToBounds(scene(200, 1200), 800, 600);
    const drawnW = 200 * vp.scale;
    const left = vp.offsetX;
    const right = 800 - (left + drawnW);
    expect(Math.abs(left - right)).toBeLessThan(1);
  });

  it("内容仍完整落在画布内", () => {
    const vp = fitToBounds(scene(1200, 200), 800, 600);
    expect(vp.offsetX).toBeGreaterThanOrEqual(0);
    expect(vp.offsetY).toBeGreaterThanOrEqual(0);
    expect(vp.offsetX + 1200 * vp.scale).toBeLessThanOrEqual(800 + 1);
  });
});
