import { describe, expect, it } from "vitest";
import { SHORTCUTS, describe as describeShortcut, grouped, resolve } from "./shortcuts";

function keyEvent(init: Partial<KeyboardEvent> & { key: string }): KeyboardEvent {
  return new KeyboardEvent("keydown", {
    key: init.key,
    ctrlKey: init.ctrlKey ?? false,
    metaKey: init.metaKey ?? false,
    shiftKey: init.shiftKey ?? false,
  });
}

describe("shortcut registry", () => {
  it("has unique ids", () => {
    const ids = SHORTCUTS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("resolves undo and prefers redo when shift is held", () => {
    const mod = /mac/i.test(navigator.userAgent) ? { metaKey: true } : { ctrlKey: true };
    expect(resolve(keyEvent({ key: "z", ...mod }))).toBe("edit.undo");
    expect(resolve(keyEvent({ key: "z", shiftKey: true, ...mod }))).toBe("edit.redo");
  });

  it("ignores plain keys without the platform modifier", () => {
    expect(resolve(keyEvent({ key: "z" }))).toBeNull();
    expect(resolve(keyEvent({ key: "1" }))).toBeNull();
  });

  it("renders a platform-appropriate label", () => {
    const undo = SHORTCUTS.find((s) => s.id === "edit.undo");
    expect(undo).toBeDefined();
    if (undo) expect(describeShortcut(undo)).toMatch(/Z$/);
  });

  it("covers every core action group", () => {
    const groups = grouped();
    expect(groups.map((entry) => entry.group)).toEqual(["文件", "编辑", "视图", "应用"]);
    for (const entry of groups) {
      expect(entry.items.length).toBeGreaterThan(0);
    }
  });

  it("assigns every shortcut to exactly one group", () => {
    const total = grouped().reduce((sum, entry) => sum + entry.items.length, 0);
    expect(total).toBe(SHORTCUTS.length);
  });

  it("has no duplicate key combinations", () => {
    const combos = SHORTCUTS.map(
      (s) => `${s.mod ? "mod+" : ""}${s.shift ? "shift+" : ""}${s.key}`,
    );
    expect(new Set(combos).size).toBe(combos.length);
  });

  it("distinguishes save from save-as by the shift modifier", () => {
    const mod = /mac/i.test(navigator.userAgent) ? { metaKey: true } : { ctrlKey: true };
    expect(resolve(keyEvent({ key: "s", ...mod }))).toBe("file.save");
    expect(resolve(keyEvent({ key: "s", shiftKey: true, ...mod }))).toBe("file.saveAs");
  });

  it("renders friendly names for special keys", () => {
    const generate = SHORTCUTS.find((s) => s.id === "edit.generate");
    expect(generate).toBeDefined();
    if (generate) expect(describeShortcut(generate)).toContain("↵");
  });
});
