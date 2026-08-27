import { describe, expect, it } from "vitest";
import { basename, readViewState, recentEntries, withViewState } from "./recent";
import type { Prefs } from "../ipc/types";

const prefs: Prefs = {
  theme: "dark",
  recent_files: ["/home/u/plans/alpha.mcm", "C:\\Users\\u\\beta.mcm"],
  view_state: { "/home/u/plans/alpha.mcm": { view: "timeline", scale: 1.5 } },
};

describe("recent files", () => {
  it("derives display labels on both path styles", () => {
    expect(basename("/home/u/plans/alpha.mcm")).toBe("alpha.mcm");
    expect(basename("C:\\Users\\u\\beta.mcm")).toBe("beta.mcm");
    expect(basename("plain.mcm")).toBe("plain.mcm");
  });

  it("maps prefs into display entries preserving order", () => {
    const entries = recentEntries(prefs);
    expect(entries).toHaveLength(2);
    expect(entries[0]?.label).toBe("alpha.mcm");
    expect(entries[1]?.label).toBe("beta.mcm");
  });
});

describe("per-file view state", () => {
  it("reads stored state", () => {
    expect(readViewState(prefs, "/home/u/plans/alpha.mcm")).toEqual({
      view: "timeline",
      scale: 1.5,
    });
  });

  it("returns empty state for unknown or missing paths", () => {
    expect(readViewState(prefs, "/unknown.mcm")).toEqual({});
    expect(readViewState(prefs, undefined)).toEqual({});
  });

  it("ignores malformed entries instead of throwing", () => {
    const broken: Prefs = { ...prefs, view_state: { "/x.mcm": "not an object" } };
    expect(readViewState(broken, "/x.mcm")).toEqual({});
  });

  it("merges new state without dropping other files", () => {
    const next = withViewState(prefs, "/home/u/plans/alpha.mcm", { view: "wbs" });
    expect(readViewState(next, "/home/u/plans/alpha.mcm")).toEqual({ view: "wbs", scale: 1.5 });
    // The original object is untouched.
    expect(readViewState(prefs, "/home/u/plans/alpha.mcm").view).toBe("timeline");
  });

  it("adds state for a file that had none", () => {
    const next = withViewState(prefs, "/new.mcm", { view: "milestones" });
    expect(readViewState(next, "/new.mcm")).toEqual({ view: "milestones" });
  });
});
