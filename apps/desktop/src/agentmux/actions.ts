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
  disabled?: boolean;
  run: () => void | Promise<void>;
}

export interface ShortcutIndex {
  single: Map<string, string>;
  chordPrefix: Set<string>;
  chord: Map<string, string>;
}

const DISABLED_SHORTCUTS = new Set(["", "none", "clear", "unbound", "disabled"]);
const MODIFIER_ORDER = ["ctrl", "alt", "shift", "meta"] as const;

export const ACTION_GROUP_LABELS: Record<ActionGroup, string> = {
  agent: "Agent",
  terminal: "Terminal",
  workspace: "Workspace",
  view: "View",
  remote: "Remote · WSL"
};

export const DEFAULT_SHORTCUT_BINDINGS: ShortcutBindingMap = {
  "app.commandPalette": "ctrl+shift+p",
  "app.commandPalette.legacy": "ctrl+k",
  "app.search": "ctrl+f",
  "app.settings": "ctrl+,",
  "notification.openPanel": "ctrl+i",
  "view.toggleTheme": "ctrl+alt+l",
  "workspace.new": "ctrl+n",
  "agent.jumpNextAttention": "ctrl+shift+u",
  "terminal.newWsl": "ctrl+t",
  "terminal.textBox": "ctrl+alt+i",
  "pane.splitRight": "ctrl+d",
  "pane.splitDown": "ctrl+shift+d",
  "browser.openContextLink": "ctrl+shift+l"
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
  for (const [actionId, binding] of Object.entries(bindings)) {
    if (!binding) {
      continue;
    }
    if (binding.strokes.length === 1) {
      single.set(binding.strokes[0], actionId);
    } else {
      chordPrefix.add(binding.strokes[0]);
      chord.set(chordKey(binding.strokes[0], binding.strokes[1]), actionId);
    }
  }
  return { single, chordPrefix, chord };
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
    return stroke ? { strokes: [stroke], label: formatShortcutLabel([stroke]) } : null;
  }
  if (Array.isArray(value) && value.length === 2) {
    const first = normalizeShortcutStroke(value[0]);
    const second = normalizeShortcutStroke(value[1]);
    return first && second
      ? { strokes: [first, second], label: formatShortcutLabel([first, second]) }
      : null;
  }
  return null;
}

export function parseShortcutBindingInput(value: string): ShortcutBindingValue {
  const text = value.trim();
  if (!text || DISABLED_SHORTCUTS.has(text.toLowerCase())) {
    return null;
  }
  const chordParts = text
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);
  if (chordParts.length === 2) {
    return [chordParts[0], chordParts[1]];
  }
  return text;
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

function normalizeShortcutStroke(value: unknown): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const rawParts = value
    .trim()
    .toLowerCase()
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
    if (modifier) {
      modifiers.add(modifier);
    }
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
    case "del":
      return "delete";
    default:
      return key.length === 1 ? key : key.replace(/^key/, "");
  }
}

function isModifierKey(key: string): boolean {
  return key === "ctrl" || key === "control" || key === "alt" || key === "shift" || key === "meta";
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
