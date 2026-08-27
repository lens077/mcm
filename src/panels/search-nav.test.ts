import { describe, expect, it } from "vitest";
import { nextIndex } from "./search-nav";

describe("search cursor navigation", () => {
  it("advances forward through the list", () => {
    expect(nextIndex(0, 1, 3)).toBe(1);
    expect(nextIndex(1, 1, 3)).toBe(2);
  });

  it("wraps around at both ends", () => {
    expect(nextIndex(2, 1, 3)).toBe(0);
    expect(nextIndex(0, -1, 3)).toBe(2);
  });

  it("is a no-op when there are no matches", () => {
    expect(nextIndex(0, 1, 0)).toBe(0);
    expect(nextIndex(5, -1, 0)).toBe(0);
  });

  it("handles a single match", () => {
    expect(nextIndex(0, 1, 1)).toBe(0);
    expect(nextIndex(0, -1, 1)).toBe(0);
  });
});
