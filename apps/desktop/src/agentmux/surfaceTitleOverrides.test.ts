import { describe, expect, it } from "vitest";
import {
  readSurfaceTitleOverrides,
  SURFACE_TITLE_OVERRIDE_STORAGE_KEY,
  surfaceTitleOverride,
  writeSurfaceTitleOverrides,
} from "./surfaceTitleOverrides";

function createStorage(initial: Record<string, string> = {}): Storage {
  const values = new Map(Object.entries(initial));
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => values.delete(key),
    setItem: (key, value) => values.set(key, value),
  };
}

describe("surface title overrides", () => {
  it("persists a custom title across application restarts", () => {
    const storage = createStorage();

    writeSurfaceTitleOverrides({ surf_1: "Deploy API" }, storage);

    expect(readSurfaceTitleOverrides(storage)).toEqual({ surf_1: "Deploy API" });
  });

  it("ignores malformed, blank, and non-string persisted entries", () => {
    const storage = createStorage({
      [SURFACE_TITLE_OVERRIDE_STORAGE_KEY]: JSON.stringify({
        surf_valid: " Review ",
        surf_blank: "   ",
        surf_number: 42,
        "": "Missing id",
      }),
    });

    expect(readSurfaceTitleOverrides(storage)).toEqual({ surf_valid: " Review " });
  });

  it("treats a cleared title as an automatic-title reset", () => {
    expect(surfaceTitleOverride("  ")).toBeNull();
    expect(surfaceTitleOverride(undefined)).toBeNull();
    expect(surfaceTitleOverride("  Codex review  ")).toBe("Codex review");
  });
});
