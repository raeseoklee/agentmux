export type ShortcutBindingValue = string | [string, string] | null;

export interface ShortcutBinding {
  strokes: [string] | [string, string];
  label: string;
}

export type ShortcutBindingMap = Record<string, ShortcutBindingValue>;
export type ResolvedShortcutBindings = Record<string, ShortcutBinding | null>;

export type ActionGroup =
  | "agent"
  | "terminal"
  | "workspace"
  | "view"
  | "remote";

export interface ActionDescriptor {
  id: string;
  group: ActionGroup;
  title: string;
  keywords?: string[];
  visibleInPalette?: boolean;
  /** Whether this action should have a row in the keyboard shortcut editor. */
  visibleInShortcutEditor?: boolean;
  disabled?: boolean;
  run: () => void | Promise<void>;
}

export interface ShortcutIndex {
  single: Map<string, string>;
  chordPrefix: Set<string>;
  chord: Map<string, string>;
  singleConflicts: Map<string, string[]>;
  chordConflicts: Map<string, string[]>;
  prefixConflicts: Map<string, string[]>;
}

export type ShortcutConflictKind = "exact" | "prefix";

export interface ShortcutConflict {
  key: string;
  label: string;
  actions: string[];
  kind: ShortcutConflictKind;
}

export interface ShortcutConflictAnalysis {
  conflicts: ShortcutConflict[];
  exact: Map<string, string[]>;
  prefixes: Map<string, string[]>;
}

export type ShortcutConflictPolicy = "reject" | "replace";

export interface ShortcutResolution {
  accepted: boolean;
  binding: ShortcutBinding | null;
  bindings: ResolvedShortcutBindings;
  conflicts: ShortcutConflict[];
  replacedActionIds: string[];
  error?: "invalid" | "conflict";
}

export interface ShortcutInputValidation {
  valid: boolean;
  disabled: boolean;
  value: ShortcutBindingValue;
  error?: "empty" | "invalid-stroke" | "invalid-chord";
}

const DISABLED_SHORTCUTS = new Set(["", "none", "clear", "unbound", "disabled"]);
const MODIFIER_ORDER = ["ctrl", "alt", "shift", "meta"] as const;

export const ACTION_GROUP_LABELS: Record<ActionGroup, string> = {
  agent: "Agent",
  terminal: "Terminal",
  workspace: "Workspace",
  view: "View",
  remote: "Remote / WSL"
};

export const DEFAULT_SHORTCUT_BINDINGS: ShortcutBindingMap = {
  "app.commandPalette": "ctrl+shift+p",
  "app.commandPalette.legacy": "ctrl+k",
  "app.search": "ctrl+shift+f",
  "app.settings": "ctrl+,",
  "notification.openPanel": "ctrl+shift+i",
  "view.toggleTheme": "ctrl+alt+l",
  "workspace.new": "ctrl+n",
  "agent.jumpNextAttention": "ctrl+shift+u",
  "terminal.newWsl": "ctrl+t",
  "terminal.textBox": "ctrl+alt+i",
  "pane.splitRight": "ctrl+alt+d",
  "pane.splitDown": "ctrl+shift+d",
  "browser.openContextLink": "ctrl+shift+l",
  "surface.nextTab": "ctrl+tab",
  "surface.prevTab": "ctrl+shift+tab",
  "surface.jumpTab1": "ctrl+alt+1",
  "surface.jumpTab2": "ctrl+alt+2",
  "surface.jumpTab3": "ctrl+alt+3",
  "surface.jumpTab4": "ctrl+alt+4",
  "surface.jumpTab5": "ctrl+alt+5",
  "surface.jumpTab6": "ctrl+alt+6",
  "surface.jumpTab7": "ctrl+alt+7",
  "surface.jumpTab8": "ctrl+alt+8",
  "surface.jumpTab9": "ctrl+alt+9",
  "surface.closeTab": "ctrl+shift+w",
  "surface.renameTab": "f2",
  "terminal.fontSizeUp": "ctrl+=",
  "terminal.fontSizeDown": "ctrl+-",
  "terminal.fontSizeReset": "ctrl+0",
  "pane.focusLeft": "alt+left",
  "pane.focusRight": "alt+right",
  "pane.focusUp": "alt+up",
  "pane.focusDown": "alt+down",
  "pane.growLeft": "ctrl+alt+shift+left",
  "pane.growRight": "ctrl+alt+shift+right",
  "pane.growUp": "ctrl+alt+shift+up",
  "pane.growDown": "ctrl+alt+shift+down",
  "app.fullscreen": "f11",
  "workspace.next": "ctrl+alt+down",
  "workspace.prev": "ctrl+alt+up",
  "pane.zoomToggle": "ctrl+shift+z"
};

export function buildResolvedShortcutBindings(
  overrides: ShortcutBindingMap = {}
): ResolvedShortcutBindings {
  const merged: ShortcutBindingMap = { ...DEFAULT_SHORTCUT_BINDINGS, ...overrides };
  const resolved: ResolvedShortcutBindings = {};
  for (const [actionId, value] of Object.entries(merged)) {
    resolved[actionId] = normalizeShortcutBinding(value);
  }
  return resolved;
}

export function buildShortcutIndex(bindings: ResolvedShortcutBindings): ShortcutIndex {
  const single = new Map<string, string>();
  const chordPrefix = new Set<string>();
  const chord = new Map<string, string>();
  const singleConflicts = new Map<string, string[]>();
  const chordConflicts = new Map<string, string[]>();
  const prefixConflicts = new Map<string, string[]>();
  for (const [actionId, binding] of Object.entries(bindings)) {
    if (!binding) {
      continue;
    }
    if (binding.strokes.length === 1) {
      const key = binding.strokes[0];
      if (!single.has(key)) {
        single.set(key, actionId);
      }
      appendConflictEntry(singleConflicts, key, actionId);
    } else {
      const prefix = binding.strokes[0];
      const key = chordKey(prefix, binding.strokes[1]);
      chordPrefix.add(prefix);
      if (!chord.has(key)) {
        chord.set(key, actionId);
      }
      appendConflictEntry(chordConflicts, key, actionId);
    }
  }
  for (const [key, actionIds] of [...singleConflicts]) {
    if (actionIds.length < 2) {
      singleConflicts.delete(key);
    }
  }
  for (const [key, actionIds] of [...chordConflicts]) {
    if (actionIds.length < 2) {
      chordConflicts.delete(key);
    }
  }
  for (const prefix of chordPrefix) {
    const singleActions =
      singleConflicts.get(prefix) ?? (single.has(prefix) ? [single.get(prefix)!] : []);
    if (singleActions.length === 0) {
      continue;
    }
    const actions = [...singleActions];
    for (const [actionId, binding] of Object.entries(bindings)) {
      if (binding?.strokes.length === 2 && binding.strokes[0] === prefix) {
        actions.push(actionId);
      }
    }
    prefixConflicts.set(prefix, [...new Set(actions)]);
  }
  return {
    single,
    chordPrefix,
    chord,
    singleConflicts,
    chordConflicts,
    prefixConflicts,
  };
}

export function isShortcutEditorVisible(
  action: Pick<ActionDescriptor, "visibleInShortcutEditor">,
): boolean {
  return action.visibleInShortcutEditor !== false;
}

export function shortcutLabelForAction(
  bindings: ResolvedShortcutBindings,
  actionId: string
): string {
  return bindings[actionId]?.label ?? "";
}

export function normalizeShortcutBinding(value: ShortcutBindingValue | unknown): ShortcutBinding | null {
  if (value === null || value === undefined) {
    return null;
  }
  if (typeof value === "string") {
    if (DISABLED_SHORTCUTS.has(value.trim().toLowerCase())) {
      return null;
    }
    const stroke = normalizeShortcutStroke(value);
    return stroke && !isUnsafeUnmodifiedShortcutStroke(stroke)
      ? { strokes: [stroke], label: formatShortcutLabel([stroke]) }
      : null;
  }
  if (Array.isArray(value) && value.length === 2) {
    const first = normalizeShortcutStroke(value[0]);
    const second = normalizeShortcutStroke(value[1]);
    return first && second && !isUnsafeUnmodifiedShortcutStroke(first)
      ? { strokes: [first, second], label: formatShortcutLabel([first, second]) }
      : null;
  }
  return null;
}

export function parseShortcutBindingInput(value: string): ShortcutBindingValue {
  const validation = validateShortcutBindingInput(value);
  return validation.valid ? validation.value : null;
}

export function validateShortcutBindingInput(value: string): ShortcutInputValidation {
  const text = value.trim();
  if (!text || DISABLED_SHORTCUTS.has(text.toLowerCase())) {
    return { valid: true, disabled: true, value: null, error: "empty" };
  }
  const chordParts = splitChordInput(text);
  if (!chordParts || chordParts.length > 2) {
    return { valid: false, disabled: false, value: null, error: "invalid-chord" };
  }
  const strokes = chordParts.map((part) => normalizeShortcutStroke(part));
  if (strokes.some((stroke) => !stroke)) {
    return { valid: false, disabled: false, value: null, error: "invalid-stroke" };
  }
  if (isUnsafeUnmodifiedShortcutStroke(strokes[0] ?? "")) {
    return { valid: false, disabled: false, value: null, error: "invalid-stroke" };
  }
  return {
    valid: true,
    disabled: false,
    value: chordParts.length === 2 ? [chordParts[0], chordParts[1]] : text,
  };
}

export function keyboardEventToStroke(event: KeyboardEvent): string | null {
  const key = normalizeKey(event.key);
  if (!key || isModifierKey(key)) {
    return null;
  }
  const modifiers: string[] = [];
  if (event.ctrlKey) modifiers.push("ctrl");
  if (event.altKey) modifiers.push("alt");
  if (event.shiftKey) modifiers.push("shift");
  if (event.metaKey) modifiers.push("meta");
  return [...modifiers, key].join("+");
}

export function chordKey(first: string, second: string): string {
  return `${first} ${second}`;
}

export function normalizeShortcutStroke(value: unknown): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const rawParts = value
    .trim()
    .toLowerCase()
    .replace(/command/g, "cmd")
    .replace(/option/g, "alt")
    .replace(/control/g, "ctrl")
    .replace(/⌘|command/g, "cmd")
    .replace(/⌥|option/g, "alt")
    .replace(/⌃|control/g, "ctrl")
    .replace(/⇧/g, "shift")
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean);
  if (rawParts.length === 0) {
    return null;
  }
  const key = normalizeKey(rawParts[rawParts.length - 1]);
  if (!key || isModifierKey(key)) {
    return null;
  }
  const modifiers = new Set<string>();
  for (const part of rawParts.slice(0, -1)) {
    const modifier = normalizeModifier(part);
    if (!modifier) {
      return null;
    }
    modifiers.add(modifier);
  }
  return [...MODIFIER_ORDER.filter((modifier) => modifiers.has(modifier)), key].join("+");
}

function normalizeModifier(value: string): string | null {
  switch (value) {
    case "cmd":
    case "win":
    case "windows":
      return "ctrl";
    case "ctrl":
    case "alt":
    case "shift":
    case "meta":
      return value;
    default:
      return null;
  }
}

function normalizeKey(value: string): string {
  const key = value.trim().toLowerCase();
  switch (key) {
    case "":
      return "";
    case " ":
    case "spacebar":
      return "space";
    case "escape":
      return "esc";
    case "arrowleft":
      return "left";
    case "arrowright":
      return "right";
    case "arrowup":
      return "up";
    case "arrowdown":
      return "down";
    case "return":
      return "enter";
    case "tab":
      return "tab";
    case "backspace":
      return "backspace";
    case "del":
      return "delete";
    case "page-up":
    case "page_up":
      return "pageup";
    case "page-down":
    case "page_down":
      return "pagedown";
    case "plus":
      return "+";
    case "minus":
      return "-";
    case "comma":
      return ",";
    case "period":
      return ".";
    case "slash":
      return "/";
    case "semicolon":
      return ";";
    case "quote":
      return "'";
    case "backslash":
      return "\\";
    case "bracketleft":
      return "[";
    case "bracketright":
      return "]";
    case "grave":
      return "`";
    default:
      if (/^f(?:[1-9]|1[0-9]|2[0-4])$/.test(key)) {
        return key;
      }
      if (/^key[a-z]$/.test(key)) {
        return key.slice(3);
      }
      if (key.length === 1 || KNOWN_KEYS.has(key)) {
        return key;
      }
      return "";
  }
}

export function isUnsafeUnmodifiedShortcutStroke(stroke: string): boolean {
  return !stroke.includes("+") && !/^f(?:[1-9]|1[0-9]|2[0-4])$/.test(stroke);
}

function isModifierKey(key: string): boolean {
  return (
    key === "ctrl" ||
    key === "control" ||
    key === "cmd" ||
    key === "command" ||
    key === "win" ||
    key === "windows" ||
    key === "alt" ||
    key === "option" ||
    key === "shift" ||
    key === "meta"
  );
}

const KNOWN_KEYS = new Set([
  "space",
  "esc",
  "tab",
  "enter",
  "backspace",
  "delete",
  "insert",
  "home",
  "end",
  "pageup",
  "pagedown",
  "left",
  "right",
  "up",
  "down",
]);

function appendConflictEntry(map: Map<string, string[]>, key: string, actionId: string): void {
  const actions = map.get(key) ?? [];
  actions.push(actionId);
  map.set(key, actions);
}

function splitChordInput(value: string): string[] | null {
  // A trailing comma after a plus is the comma key, not a chord separator.
  if (value.endsWith(",") && value.slice(0, -1).trim().endsWith("+")) {
    return [value];
  }
  const parts = value.split(",").map((part) => part.trim());
  if (parts.some((part) => !part) || parts.length > 2) {
    return null;
  }
  return parts;
}

export function analyzeShortcutConflicts(
  bindings: ResolvedShortcutBindings,
): ShortcutConflictAnalysis {
  const exact = new Map<string, string[]>();
  const labels = new Map<string, string>();
  const prefixes = new Map<string, string[]>();
  for (const [actionId, binding] of Object.entries(bindings)) {
    if (!binding) continue;
    const key = binding.strokes.join(" ");
    appendConflictEntry(exact, key, actionId);
    labels.set(key, binding.label);
    if (binding.strokes.length === 2) {
      appendConflictEntry(prefixes, binding.strokes[0], actionId);
    }
  }
  for (const [key, actionIds] of [...exact]) {
    if (actionIds.length < 2) exact.delete(key);
  }
  for (const [prefix, chordActionIds] of [...prefixes]) {
    const singleActionIds = Object.entries(bindings)
      .filter(([, binding]) => binding?.strokes.length === 1 && binding.strokes[0] === prefix)
      .map(([actionId]) => actionId);
    if (singleActionIds.length === 0) {
      prefixes.delete(prefix);
      continue;
    }
    prefixes.set(prefix, [...singleActionIds, ...chordActionIds]);
  }
  const conflicts: ShortcutConflict[] = [];
  for (const [key, actions] of exact) {
    conflicts.push({
      key,
      label: labels.get(key) ?? formatShortcutLabel(key.split(" ") as [string] | [string, string]),
      actions,
      kind: "exact",
    });
  }
  for (const [prefix, actions] of prefixes) {
    conflicts.push({
      key: prefix,
      label: formatStrokeLabel(prefix),
      actions: [...new Set(actions)],
      kind: "prefix",
    });
  }
  return { conflicts, exact, prefixes };
}

export function collectShortcutConflicts(
  bindings: ResolvedShortcutBindings,
): ShortcutConflict[] {
  return analyzeShortcutConflicts(bindings).conflicts;
}

export function findShortcutConflicts(
  bindings: ResolvedShortcutBindings,
  actionId: string,
  value: ShortcutBindingValue | unknown,
): ShortcutConflict[] {
  const candidate = normalizeShortcutBinding(value);
  if (!candidate) return [];
  const next = { ...bindings, [actionId]: candidate };
  return analyzeShortcutConflicts(next).conflicts.filter((conflict) =>
    conflict.actions.includes(actionId),
  );
}

export function resolveShortcutBinding(
  bindings: ResolvedShortcutBindings,
  actionId: string,
  value: ShortcutBindingValue | unknown,
  policy: ShortcutConflictPolicy = "reject",
): ShortcutResolution {
  const binding = normalizeShortcutBinding(value);
  const isDisabledValue =
    value === null ||
    value === undefined ||
    (typeof value === "string" && DISABLED_SHORTCUTS.has(value.trim().toLowerCase()));
  if (!isDisabledValue && binding === null) {
    return {
      accepted: false,
      binding: null,
      bindings,
      conflicts: [],
      replacedActionIds: [],
      error: "invalid",
    };
  }
  const candidateBindings = { ...bindings, [actionId]: binding };
  const conflicts = analyzeShortcutConflicts(candidateBindings).conflicts.filter((conflict) =>
    conflict.actions.includes(actionId),
  );
  if (conflicts.length > 0 && policy === "reject") {
    return {
      accepted: false,
      binding,
      bindings,
      conflicts,
      replacedActionIds: [],
      error: "conflict",
    };
  }
  const replacedActionIds =
    policy === "replace"
      ? [...new Set(conflicts.flatMap((conflict) => conflict.actions))].filter(
          (candidateActionId) => candidateActionId !== actionId,
        )
      : [];
  const resolvedBindings = { ...candidateBindings };
  for (const replacedActionId of replacedActionIds) {
    resolvedBindings[replacedActionId] = null;
  }
  return {
    accepted: true,
    binding,
    bindings: resolvedBindings,
    conflicts,
    replacedActionIds,
  };
}

function formatShortcutLabel(strokes: [string] | [string, string]): string {
  return strokes.map(formatStrokeLabel).join(" ");
}

function formatStrokeLabel(stroke: string): string {
  return stroke
    .split("+")
    .map((part) => {
      switch (part) {
        case "ctrl":
          return "Ctrl";
        case "alt":
          return "Alt";
        case "shift":
          return "Shift";
        case "meta":
          return "Win";
        case "space":
          return "Space";
        case ",":
          return ",";
        default:
          return part.length === 1 ? part.toUpperCase() : part[0].toUpperCase() + part.slice(1);
      }
    })
    .join("+");
}
