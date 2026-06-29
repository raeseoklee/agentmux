// Demo data model for the AgentMux terminal design prototype. Keep this file
// synthetic: no personal usernames, private hostnames, tokens, or real paths.
import type { ThemeTokens } from "./theme";

export type WindowStatus = "input" | "running" | "done" | "idle";
export type SplitMode = "single" | "two" | "mosaic";
export type TabKind = "agent" | "shell";

export type LineKind =
  | "user"
  | "h"
  | "sec"
  | "li"
  | "dim"
  | "recap"
  | "tool"
  | "res"
  | "cmd"
  | "boot"
  | "ok"
  | "text"
  | "run";

export type RawLine = [LineKind, string];

export interface OmcStatus {
  state: "thinking" | "building" | "running" | "done";
  session: string;
  cost: string;
  tokens: string;
  cache: string;
  rate: string;
  ctx: string;
}

export interface WindowModel {
  type: TabKind;
  title: string;
  status: WindowStatus;
  model?: string;
  agent?: string;
  user?: string;
  tty?: string;
  omc?: OmcStatus;
  lines: RawLine[];
}

export interface Workspace {
  id: string;
  name: string;
  branch: string;
  path: string;
  agent: string;
  status: WindowStatus;
  statusText: string;
  tabs: string[];
}

export interface TabModel {
  title: string;
  kind: TabKind;
  split: SplitMode;
  panes: string[];
}

export interface Profile {
  name: string;
  host: string;
  user: string;
  dot: string;
}

export const WORKSPACES: Workspace[] = [
  {
    id: "terminal-core",
    name: "terminal-core",
    branch: "main",
    path: "~/projects/terminal-core",
    agent: "Codex",
    status: "input",
    statusText: "Codex is waiting for input",
    tabs: ["t_review", "t_tests", "t_serve"],
  },
  {
    id: "browser-tools",
    name: "browser-tools",
    branch: "feature/cdp",
    path: "~/projects/browser-tools",
    agent: "Claude",
    status: "running",
    statusText: "Claude is checking browser automation",
    tabs: ["t_core", "t_build"],
  },
  {
    id: "release-lab",
    name: "release-lab",
    branch: "release/v0.1",
    path: "~/projects/release-lab",
    agent: "Gemini",
    status: "done",
    statusText: "Release checklist completed",
    tabs: ["t_logs"],
  },
];

export const WINDOWS: Record<string, WindowModel> = {
  w1: {
    type: "agent",
    title: "review terminal restore",
    status: "input",
    model: "gpt-5-codex",
    agent: "Codex",
    omc: {
      state: "thinking",
      session: "9m",
      cost: "~$0.12",
      tokens: "24k",
      cache: "92%",
      rate: "$0.80/h",
      ctx: "31%",
    },
    lines: [
      ["user", "Review terminal restore behavior and summarize remaining risks."],
      ["h", "Findings"],
      ["li", "Durable sessions reconnect through the saved tmux reference."],
      ["li", "Detached panes keep bounded scrollback without active rendering."],
      ["sec", "Next step"],
      ["recap", "Add one smoke test for a failed restore rollback path."],
    ],
  },
  w2: {
    type: "shell",
    title: "dev@workstation - pwsh",
    status: "idle",
    tty: "conpty-01",
    user: "dev@workstation",
    lines: [
      ["cmd", "git status -sb"],
      ["res", "## main...origin/main"],
      ["boot", "PowerShell 7 ready"],
    ],
  },
  w3: {
    type: "shell",
    title: "dev@workstation - wsl",
    status: "idle",
    tty: "wsl-01",
    user: "dev@workstation",
    lines: [["boot", "WSL shell ready"]],
  },
  w4: {
    type: "shell",
    title: "dev@workstation - logs",
    status: "idle",
    tty: "conpty-02",
    user: "dev@workstation",
    lines: [["cmd", "tail -f logs/agentmux.log"]],
  },
  w5: {
    type: "agent",
    title: "browser automation pass",
    status: "running",
    model: "claude-sonnet",
    agent: "Claude",
    omc: {
      state: "running",
      session: "4m",
      cost: "~$0.08",
      tokens: "18k",
      cache: "89%",
      rate: "$0.70/h",
      ctx: "22%",
    },
    lines: [
      ["user", "Check click, type, evaluate, and screenshot actions."],
      ["tool", "Read(crates/agentmux-browser/src/lib.rs)"],
      ["res", "Captured browser action definitions."],
      ["cmd", "npm run browser:cdp-smoke"],
      ["run", "Smoke test running"],
    ],
  },
  w6: {
    type: "shell",
    title: "dev@workstation - test",
    status: "idle",
    tty: "conpty-03",
    user: "dev@workstation",
    lines: [["cmd", "cargo test --workspace"]],
  },
  w_logs: {
    type: "shell",
    title: "release log",
    status: "done",
    user: "dev@workstation",
    lines: [
      ["cmd", "npm run version:check -- --tag v0.1.2"],
      ["ok", "Version metadata is consistent"],
      ["ok", "Release notes generated"],
    ],
  },
  w_test: {
    type: "shell",
    title: "ui tests",
    status: "idle",
    user: "dev@workstation",
    lines: [
      ["cmd", "npm --prefix apps/desktop run test:ui"],
      ["ok", "8 passed"],
      ["boot", "Watching for file changes"],
    ],
  },
  w_lint: {
    type: "shell",
    title: "lint",
    status: "idle",
    user: "dev@workstation",
    lines: [
      ["cmd", "cargo clippy --workspace --all-targets -- -D warnings"],
      ["ok", "0 warnings"],
    ],
  },
  w_serve: {
    type: "shell",
    title: "desktop dev server",
    status: "idle",
    user: "dev@workstation",
    lines: [
      ["cmd", "npm --prefix apps/desktop run dev"],
      ["res", "Vite ready on localhost"],
      ["boot", "press h + enter for help"],
    ],
  },
  w_build: {
    type: "shell",
    title: "desktop build",
    status: "idle",
    user: "dev@workstation",
    lines: [
      ["cmd", "npm --prefix apps/desktop run build"],
      ["res", "TypeScript and Vite build completed"],
    ],
  },
};

export const TABS: Record<string, TabModel> = {
  t_review: {
    title: "restore review",
    kind: "agent",
    split: "mosaic",
    panes: ["w1", "w2", "w3", "w4"],
  },
  t_tests: { title: "tests", kind: "shell", split: "two", panes: ["w_test", "w_lint"] },
  t_serve: { title: "dev server", kind: "shell", split: "single", panes: ["w_serve"] },
  t_core: { title: "browser work", kind: "agent", split: "two", panes: ["w5", "w6"] },
  t_build: { title: "build log", kind: "shell", split: "single", panes: ["w_build"] },
  t_logs: { title: "release log", kind: "shell", split: "single", panes: ["w_logs"] },
};

export const PROFILES: Profile[] = [
  { name: "local-dev", host: "localhost", user: "dev", dot: "#4ADE80" },
  { name: "staging", host: "staging.example.internal", user: "ops", dot: "#FBBF24" },
  { name: "build-runner", host: "runner.example.internal", user: "ci", dot: "#6B6B73" },
];

export const KEYMAPS: { k: string; v: string }[] = [
  { k: "Command palette", v: "Ctrl+K" },
  { k: "Search active pane", v: "Ctrl+F" },
  { k: "New tab", v: "Ctrl+T" },
  { k: "Close pane", v: "Ctrl+W" },
  { k: "Split pane", v: "Ctrl+D" },
  { k: "Toggle mosaic", v: "Ctrl+G" },
  { k: "Toggle theme", v: "Ctrl+Shift+L" },
  { k: "Settings", v: "Ctrl+," },
];

export function statusColor(theme: ThemeTokens, status: WindowStatus): string {
  if (status === "running") return "var(--accent)";
  if (status === "done") return theme.green;
  if (status === "input") return theme.warn;
  return theme.fg4;
}

export function statusLabel(status: WindowStatus): string {
  if (status === "running") return "Running";
  if (status === "done") return "Done";
  if (status === "input") return "Waiting";
  return "Idle";
}

export interface LineStyle {
  glyph: string;
  gc: string;
  tc: string;
  t: string;
  w: string;
  fs: string;
  mt: number;
  indent: number;
}

export function mapLine(theme: ThemeTokens, line: RawLine): LineStyle {
  const [kind, t] = line;
  const base: LineStyle = {
    glyph: "",
    gc: theme.fg4,
    tc: theme.fg2,
    t,
    w: "400",
    fs: "normal",
    mt: 0,
    indent: 0,
  };
  switch (kind) {
    case "user":
      return { ...base, glyph: ">", gc: "var(--accent)", tc: theme.fg1, w: "600", mt: 2 };
    case "h":
      return { ...base, tc: theme.fg1, w: "700", mt: 12 };
    case "sec":
      return { ...base, tc: theme.fg1, w: "700", mt: 14 };
    case "li":
      return { ...base, glyph: "-", gc: theme.fg4, tc: theme.fg2, indent: 2 };
    case "dim":
      return { ...base, tc: theme.fg4, mt: 8 };
    case "recap":
      return { ...base, tc: theme.fg4, fs: "italic", mt: 10 };
    case "tool":
      return { ...base, glyph: "*", gc: "var(--accent)", tc: theme.fg1, w: "500", mt: 2 };
    case "res":
      return { ...base, glyph: "=", gc: theme.fg4, tc: theme.fg3, indent: 14 };
    case "cmd":
      return { ...base, glyph: "$", gc: theme.fg4, tc: theme.fg1, w: "500", mt: 2 };
    case "boot":
      return { ...base, tc: theme.fg4 };
    case "ok":
      return { ...base, glyph: "+", gc: theme.green, tc: theme.fg2, indent: 14 };
    case "text":
      return { ...base, tc: theme.fg2, indent: 14, mt: 2 };
    case "run":
      return { ...base, tc: theme.fg2, indent: 14, mt: 2 };
    default:
      return base;
  }
}
