import { describe, expect, it } from "vitest";
import {
  ACTION_GROUP_LABELS,
  DEFAULT_SHORTCUT_BINDINGS,
  analyzeShortcutConflicts,
  buildResolvedShortcutBindings,
  buildShortcutIndex,
  findShortcutConflicts,
  isShortcutEditorVisible,
  normalizeShortcutBinding,
  normalizeShortcutStroke,
  parseShortcutBindingInput,
  resolveShortcutBinding,
  shortcutLabelForAction,
  validateShortcutBindingInput,
} from "./actions";

describe("shortcut core", () => {
  it("normalizes aliases, modifier order, named keys, and labels", () => {
    expect(normalizeShortcutStroke("Shift+Command+ArrowLeft")).toBe("ctrl+shift+left");
    expect(normalizeShortcutStroke("ctrl+page-down")).toBe("ctrl+pagedown");
    expect(normalizeShortcutBinding(["ctrl+b", "c"])).toEqual({
      strokes: ["ctrl+b", "c"],
      label: "Ctrl+B C",
    });
    expect(
      shortcutLabelForAction(buildResolvedShortcutBindings(), "app.commandPalette"),
    ).toBe("Ctrl+Shift+P");
    expect(ACTION_GROUP_LABELS.remote).toBe("Remote / WSL");
  });

  it("rejects unknown modifiers and key tokens instead of dropping them", () => {
    expect(normalizeShortcutStroke("hyper+x")).toBeNull();
    expect(normalizeShortcutStroke("ctrl+wat")).toBeNull();
    expect(normalizeShortcutStroke("ctrl+shift")).toBeNull();
    expect(parseShortcutBindingInput("hyper+x")).toBeNull();
    expect(validateShortcutBindingInput("ctrl+wat")).toMatchObject({
      valid: false,
      error: "invalid-stroke",
    });
    expect(validateShortcutBindingInput("none")).toEqual({
      valid: true,
      disabled: true,
      value: null,
      error: "empty",
    });
  });

  it("preserves one-stroke bindings, two-stroke chords, and comma keys", () => {
    expect(parseShortcutBindingInput("ctrl+b, c")).toEqual(["ctrl+b", "c"]);
    expect(parseShortcutBindingInput("ctrl+,")).toBe("ctrl+,");
    expect(parseShortcutBindingInput("ctrl+b,")).toBeNull();
    expect(parseShortcutBindingInput("ctrl+b, c, d")).toBeNull();
    expect(normalizeShortcutBinding(["ctrl+b", "c"])).toEqual({
      strokes: ["ctrl+b", "c"],
      label: "Ctrl+B C",
    });
  });

  it("rejects unmodified terminal input except as a chord continuation", () => {
    expect(normalizeShortcutBinding("c")).toBeNull();
    expect(normalizeShortcutBinding("enter")).toBeNull();
    expect(normalizeShortcutBinding(["c", "ctrl+x"])).toBeNull();
    expect(normalizeShortcutBinding(["ctrl+b", "c"])).toEqual({
      strokes: ["ctrl+b", "c"],
      label: "Ctrl+B C",
    });
    expect(normalizeShortcutBinding("f2")?.strokes).toEqual(["f2"]);
  });

  it("indexes the first deterministic action and exposes exact/prefix collisions", () => {
    const bindings = buildResolvedShortcutBindings({
      "action.first": "ctrl+x",
      "action.second": "ctrl+x",
      "action.chord": ["ctrl+x", "c"],
    });
    const index = buildShortcutIndex(bindings);
    expect(index.single.get("ctrl+x")).toBe("action.first");
    expect(index.chord.get("ctrl+x c")).toBe("action.chord");
    expect(index.singleConflicts.get("ctrl+x")).toEqual([
      "action.first",
      "action.second",
    ]);
    expect(index.prefixConflicts.get("ctrl+x")).toEqual([
      "action.first",
      "action.second",
      "action.chord",
    ]);
    const analysis = analyzeShortcutConflicts(bindings);
    expect(analysis.conflicts).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ key: "ctrl+x", kind: "exact" }),
        expect.objectContaining({ key: "ctrl+x", kind: "prefix" }),
      ]),
    );
  });

  it("finds candidate conflicts without mutating the current map", () => {
    const bindings = buildResolvedShortcutBindings({
      "action.first": "ctrl+x",
      "action.second": "ctrl+y",
    });
    const conflicts = findShortcutConflicts(bindings, "action.second", "ctrl+x");
    expect(conflicts).toEqual([
      expect.objectContaining({ key: "ctrl+x", kind: "exact" }),
    ]);
    expect(bindings["action.second"]?.strokes).toEqual(["ctrl+y"]);
  });

  it("rejects conflicting replacements and can explicitly replace them", () => {
    const bindings = buildResolvedShortcutBindings({
      "action.first": "ctrl+x",
      "action.second": ["ctrl+x", "c"],
      "action.third": "ctrl+y",
    });
    const rejected = resolveShortcutBinding(bindings, "action.third", "ctrl+x", "reject");
    expect(rejected.accepted).toBe(false);
    expect(rejected.error).toBe("conflict");
    expect(rejected.replacedActionIds).toEqual([]);

    const replaced = resolveShortcutBinding(bindings, "action.third", "ctrl+x", "replace");
    expect(replaced.accepted).toBe(true);
    expect(replaced.replacedActionIds).toEqual(["action.first", "action.second"]);
    expect(replaced.bindings["action.first"]).toBeNull();
    expect(replaced.bindings["action.second"]).toBeNull();
    expect(replaced.bindings["action.third"]?.strokes).toEqual(["ctrl+x"]);
  });

  it("rejects invalid replacements and supports clearing a binding", () => {
    const bindings = buildResolvedShortcutBindings({ "action.first": "ctrl+x" });
    expect(resolveShortcutBinding(bindings, "action.first", "hyper+x").error).toBe("invalid");
    const cleared = resolveShortcutBinding(bindings, "action.first", null);
    expect(cleared.accepted).toBe(true);
    expect(cleared.bindings["action.first"]).toBeNull();
    expect(resolveShortcutBinding(bindings, "action.first", "").accepted).toBe(true);
  });

  it("keeps palette visibility independent from shortcut-editor visibility", () => {
    expect(isShortcutEditorVisible({})).toBe(true);
    expect(isShortcutEditorVisible({ visibleInShortcutEditor: false })).toBe(false);
    expect(
      isShortcutEditorVisible({ visibleInShortcutEditor: true }),
    ).toBe(true);
  });

  it("uses terminal-safe Windows defaults for Ctrl+D, Ctrl+I, and Ctrl+F", () => {
    expect(DEFAULT_SHORTCUT_BINDINGS["pane.splitRight"]).toBe("ctrl+alt+d");
    expect(DEFAULT_SHORTCUT_BINDINGS["notification.openPanel"]).toBe("ctrl+shift+i");
    expect(DEFAULT_SHORTCUT_BINDINGS["app.search"]).toBe("ctrl+shift+f");
    // Ctrl+B remains a parent integration concern because the legacy fallback lives in App.tsx.
    expect(DEFAULT_SHORTCUT_BINDINGS["pane.splitRight"]).not.toBe("ctrl+b");
  });
});
