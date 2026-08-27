import { describe, expect, it } from "vitest";
import type { SceneGraph, SceneNode } from "../canvas/scene-types";
import {
  daysFromDelta,
  deleteCommandFor,
  dropModeFor,
  gripFor,
  isDescendant,
  isOnLinkAnchor,
  linkCommandFor,
  moveCommandFor,
  parentKeyOf,
  scheduleCommandFor,
  shiftIsoDate,
  siblingIndex,
  unlinkCommandFor,
} from "./interaction-model";

function node(id: number, x = 0, y = 0, sub?: string): SceneNode {
  return {
    ref: { kind: "task", id },
    x,
    y,
    w: 200,
    h: 40,
    style_role: "task",
    text: `t${String(id)}`,
    ...(sub === undefined ? {} : { sub_text: sub }),
    badges: [],
  };
}

/** t1 with children t2, t3; t4 is a separate root. */
const tree: SceneGraph = {
  view: "wbs",
  nodes: [node(1, 0, 0), node(2, 40, 60), node(3, 40, 120), node(4, 0, 180)],
  edges: [
    {
      from: { kind: "task", id: 1 },
      to: { kind: "task", id: 2 },
      points: [0, 0, 40, 60],
      style_role: "hierarchy_edge",
    },
    {
      from: { kind: "task", id: 1 },
      to: { kind: "task", id: 3 },
      points: [0, 0, 40, 120],
      style_role: "hierarchy_edge",
    },
  ],
  bounds: { min_x: 0, min_y: 0, max_x: 240, max_y: 220 },
};

describe("wbs drop targeting", () => {
  const target = node(2, 40, 60);

  it("maps vertical thirds to before/child/after", () => {
    expect(dropModeFor(target, { x: 50, y: 62 })).toBe("before");
    expect(dropModeFor(target, { x: 50, y: 80 })).toBe("child");
    expect(dropModeFor(target, { x: 50, y: 98 })).toBe("after");
  });

  it("derives the parent from hierarchy edges", () => {
    const child = tree.nodes[1];
    expect(child).toBeDefined();
    if (child) expect(parentKeyOf(tree, child)).toBe("t1");
    const root = tree.nodes[0];
    expect(root).toBeDefined();
    if (root) expect(parentKeyOf(tree, root)).toBeNull();
  });

  it("computes the sibling index", () => {
    expect(siblingIndex(tree, "t1", "t2")).toBe(0);
    expect(siblingIndex(tree, "t1", "t3")).toBe(1);
  });

  it("detects descendants", () => {
    expect(isDescendant(tree, "t2", "t1")).toBe(true);
    expect(isDescendant(tree, "t1", "t2")).toBe(false);
    expect(isDescendant(tree, "t4", "t1")).toBe(false);
  });
});

describe("wbs move commands", () => {
  it("re-parents when dropped in the middle", () => {
    const dragged = tree.nodes[3];
    const target = tree.nodes[0];
    expect(dragged && target).toBeTruthy();
    if (!dragged || !target) return;
    expect(moveCommandFor(tree, dragged, target, "child")).toEqual({
      kind: "move_task",
      id: 4,
      new_parent: 1,
      index: 0,
    });
  });

  it("inserts before and after a sibling", () => {
    const dragged = tree.nodes[3];
    const target = tree.nodes[2];
    if (!dragged || !target) return;
    expect(moveCommandFor(tree, dragged, target, "before")).toEqual({
      kind: "move_task",
      id: 4,
      new_parent: 1,
      index: 1,
    });
    expect(moveCommandFor(tree, dragged, target, "after")).toEqual({
      kind: "move_task",
      id: 4,
      new_parent: 1,
      index: 2,
    });
  });

  it("refuses to drop a task on itself", () => {
    const dragged = tree.nodes[0];
    if (!dragged) return;
    expect(moveCommandFor(tree, dragged, dragged, "child")).toBeNull();
  });

  it("refuses to drop a task into its own subtree", () => {
    const parent = tree.nodes[0];
    const child = tree.nodes[1];
    if (!parent || !child) return;
    expect(moveCommandFor(tree, parent, child, "child")).toBeNull();
  });
});

describe("graph link gestures", () => {
  const source = node(1, 0, 0);

  it("recognises the right-edge anchor", () => {
    expect(isOnLinkAnchor(source, { x: 200, y: 20 })).toBe(true);
    expect(isOnLinkAnchor(source, { x: 100, y: 20 })).toBe(false);
  });

  it("builds an AddDependency command", () => {
    expect(linkCommandFor(source, node(2))).toEqual({
      kind: "add_dependency",
      predecessor: 1,
      successor: 2,
    });
  });

  it("refuses self-links", () => {
    expect(linkCommandFor(source, source)).toBeNull();
  });

  it("builds a RemoveDependency command from an edge", () => {
    expect(
      unlinkCommandFor({ from: { kind: "task", id: 1 }, to: { kind: "task", id: 2 } }),
    ).toEqual({ kind: "remove_dependency", predecessor: 1, successor: 2 });
  });
});

describe("timeline drag gestures", () => {
  const bar = node(1, 100, 0, "2026-09-01..2026-09-05");

  it("detects which grip the press used", () => {
    expect(gripFor(bar, { x: 103, y: 10 })).toBe("start");
    expect(gripFor(bar, { x: 297, y: 10 })).toBe("end");
    expect(gripFor(bar, { x: 200, y: 10 })).toBe("move");
  });

  it("converts pixel deltas into whole days", () => {
    expect(daysFromDelta(26, 26)).toBe(1);
    expect(daysFromDelta(-52, 26)).toBe(-2);
    expect(daysFromDelta(10, 26)).toBe(0);
    expect(daysFromDelta(10, 0)).toBe(0);
  });

  it("shifts ISO dates across month boundaries", () => {
    expect(shiftIsoDate("2026-09-30", 1)).toBe("2026-10-01");
    expect(shiftIsoDate("2026-01-01", -1)).toBe("2025-12-31");
    expect(shiftIsoDate("not-a-date", 1)).toBe("not-a-date");
  });

  it("moves the whole window", () => {
    expect(scheduleCommandFor(bar, "move", 2)).toEqual({
      kind: "set_schedule",
      id: 1,
      schedule: { kind: "explicit", start: "2026-09-03", end: "2026-09-07" },
    });
  });

  it("drags only the grabbed edge", () => {
    expect(scheduleCommandFor(bar, "start", 1)).toEqual({
      kind: "set_schedule",
      id: 1,
      schedule: { kind: "explicit", start: "2026-09-02", end: "2026-09-05" },
    });
    expect(scheduleCommandFor(bar, "end", -1)).toEqual({
      kind: "set_schedule",
      id: 1,
      schedule: { kind: "explicit", start: "2026-09-01", end: "2026-09-04" },
    });
  });

  it("refuses inverted windows and no-op drags", () => {
    expect(scheduleCommandFor(bar, "start", 10)).toBeNull();
    expect(scheduleCommandFor(bar, "move", 0)).toBeNull();
    expect(scheduleCommandFor(node(2, 0, 0, "无日期"), "move", 1)).toBeNull();
  });
});

describe("delete gesture", () => {
  it("maps each selection kind to its removal command", () => {
    expect(deleteCommandFor({ kind: "task", id: 3 })).toEqual({ kind: "delete_task", id: 3 });
    expect(deleteCommandFor({ kind: "milestone", id: 1 })).toEqual({
      kind: "remove_milestone",
      id: 1,
    });
    expect(deleteCommandFor({ kind: "dependency", predecessor: 1, successor: 2 })).toEqual({
      kind: "remove_dependency",
      predecessor: 1,
      successor: 2,
    });
  });

  it("returns null when nothing deletable is selected", () => {
    expect(deleteCommandFor(null)).toBeNull();
    expect(deleteCommandFor({ kind: "plan" })).toBeNull();
    expect(deleteCommandFor({ kind: "line", line: 3 })).toBeNull();
  });
});
