import {
  type CSSProperties,
  type DragEvent as ReactDragEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type {
  DownloadEvent,
  Update as TauriUpdate,
} from "@tauri-apps/plugin-updater";
import "./agentmux.css";
import type {
  AgentState,
  AgentTelemetry,
  AppConfigCustomAction,
  AppConfigDiagnosticEntry,
  AppConfigNotificationAction,
  AppConfigUpdates,
  AppConfigUi,
  ControlClient,
  DockControl,
  DockConfig,
  NotificationSummary,
  PaneSummary,
  SidebarState,
  SshProfile,
  SshProfileInput,
  SurfaceSummary,
  TeamMessage,
  TeamTask,
  TerminalSession,
  TerminalProfile,
  TerminalStartDirectory,
  TerminalSplitBehavior,
  TmuxDiagnostics,
  AppConfig,
  AppConfigScope,
  AppLocaleLanguage,
  WorkspaceGroup,
  WorkspaceUpdateInput,
  WorkspaceDetail,
  WorkspaceSummary,
} from "../control/ControlClient";
import {
  buildResolvedShortcutBindings,
  buildShortcutIndex,
  chordKey,
  keyboardEventToStroke,
  normalizeShortcutBinding,
  parseShortcutBindingInput,
  shortcutLabelForAction,
  type ActionGroup,
  type ActionDescriptor,
  type ResolvedShortcutBindings,
  type ShortcutBindingValue,
  type ShortcutBindingMap,
} from "./actions";
import { BrowserSurfacePanel } from "./BrowserSurfacePanel";
import { Hov } from "./Hov";
import { LiveTerminal, TerminalRestorePreview } from "./LiveTerminal";
import { SplitHandle } from "./SplitHandle";
import { useAgentmuxControl } from "./useAgentmuxControl";
import {
  ACCENTS,
  buildRootVars,
  THEMES,
  type ThemeName,
  type ThemeTokens,
} from "./theme";
import {
  IconBranch,
  IconBubble,
  IconChevronDown,
  IconChevronRight,
  IconChevronUp,
  IconClose,
  IconDuplicate,
  IconFolder,
  IconGear,
  IconGrid,
  IconMoon,
  IconPlus,
  IconSearch,
  IconBalance,
  IconServer,
  IconShellArrow,
  IconSidebar,
  IconSplitCols,
  IconSplitRows,
  IconSun,
  IconWinMaximize,
  IconWinMinimize,
  IconWinRestore,
} from "./icons";
import {
  closeWindow,
  minimizeWindow,
  toggleMaximizeWindow,
  watchMaximized,
} from "./windowControls";
import { initZoom, nudgeZoom, resetZoom, ZOOM_STEP } from "./uiZoom";
import {
  createTranslator,
  SUPPORTED_LANGUAGES,
  type Translator,
} from "./i18n";
import desktopPackage from "../../package.json";

type Overlay = "palette" | "search" | "settings" | "setup" | null;
type SettingsTab =
  | "general"
  | "workspace"
  | "appearance"
  | "profiles"
  | "keys"
  | "diagnostics";

const SSH_UI_ENABLED = false;
const APP_VERSION =
  typeof desktopPackage.version === "string" ? desktopPackage.version : "0.0.0";

type AppUpdateStatus =
  | "idle"
  | "checking"
  | "available"
  | "not_available"
  | "downloading"
  | "installed"
  | "error"
  | "unsupported";

interface AppUpdateState {
  status: AppUpdateStatus;
  currentVersion?: string | null;
  version?: string | null;
  date?: string | null;
  body?: string | null;
  message?: string | null;
  downloadedBytes?: number;
  contentLength?: number | null;
  lastCheckedAt?: string | null;
}

const DEFAULT_UPDATES_CONFIG: AppConfigUpdates = {
  autoCheck: true,
};

const DEFAULT_UPDATE_STATE: AppUpdateState = {
  status: "idle",
  lastCheckedAt: null,
};

function isTauriDesktopRuntime(): boolean {
  const runtime = window as Window & {
    __TAURI__?: { core?: { invoke?: unknown } };
    __TAURI_INTERNALS__?: unknown;
  };
  return Boolean(runtime.__TAURI__?.core?.invoke || runtime.__TAURI_INTERNALS__);
}

function updateErrorMessage(cause: unknown): string {
  if (cause instanceof Error && cause.message.trim()) {
    return cause.message;
  }
  if (typeof cause === "string" && cause.trim()) {
    return cause;
  }
  return "unknown updater error";
}

function updateProgressText(state: AppUpdateState): string {
  const downloaded = state.downloadedBytes ?? 0;
  const total = state.contentLength ?? 0;
  if (total > 0) {
    const pct = Math.max(0, Math.min(100, Math.round((downloaded / total) * 100)));
    return `${pct}%`;
  }
  if (downloaded > 0) {
    return `${Math.round(downloaded / 1024)} KB`;
  }
  return "";
}

interface PaletteItem {
  id: string;
  title: string;
  hint: string;
  highlighted: boolean;
  disabled?: boolean;
  onClick: () => void;
}

interface PaletteGroup {
  label: string;
  items: PaletteItem[];
}

interface TerminalProfileMenuItem {
  id: string;
  profile: TerminalProfile;
  distribution?: string | null;
  title: string;
  description: string;
  disabled?: boolean;
}

interface NotificationActionBinding {
  hook: AppConfigNotificationAction;
  action: ActionDescriptor;
}

type BrowserCustomActionPreset =
  | {
      kind: "open";
      placement: "new_tab" | "active_pane";
      url: string | null;
    }
  | {
      kind: "screenshot";
      placement: "new_tab" | "active_pane";
      format: string | null;
    }
  | {
      kind: "dom_snapshot";
      placement: "new_tab" | "active_pane";
      frameId?: string | null;
    }
  | {
      kind: "evaluate";
      placement: "new_tab" | "active_pane";
      script: string;
      frameId?: string | null;
    }
  | {
      kind: "click";
      placement: "new_tab" | "active_pane";
      selector: string;
      frameId?: string | null;
    }
  | {
      kind: "type";
      placement: "new_tab" | "active_pane";
      selector: string;
      text: string;
      frameId?: string | null;
    }
  | {
      kind: "fill";
      placement: "new_tab" | "active_pane";
      selector: string;
      text: string;
      frameId?: string | null;
    }
  | {
      kind: "press";
      placement: "new_tab" | "active_pane";
      selector: string;
      key: string;
      frameId?: string | null;
    }
  | {
      kind: "select";
      placement: "new_tab" | "active_pane";
      selector: string;
      values: string[];
      frameId?: string | null;
    }
  | {
      kind: "scroll";
      placement: "new_tab" | "active_pane";
      selector: string | null;
      x: number;
      y: number;
      frameId?: string | null;
    }
  | {
      kind: "hover";
      placement: "new_tab" | "active_pane";
      selector: string;
      frameId?: string | null;
    }
  | {
      kind: "check";
      placement: "new_tab" | "active_pane";
      selector: string;
      checked: boolean;
      frameId?: string | null;
    }
  | {
      kind: "highlight";
      placement: "new_tab" | "active_pane";
      selector: string;
      durationMs: number | null;
      frameId?: string | null;
    }
  | {
      kind: "wait_for_selector";
      placement: "new_tab" | "active_pane";
      selector: string;
      timeoutMs: number | null;
      frameId?: string | null;
    }
  | {
      kind: "navigation_control";
      operation: "reload" | "back" | "forward" | "current_url";
      placement: "new_tab" | "active_pane";
    }
  | {
      kind: "focus";
      placement: "new_tab" | "active_pane";
      selector: string;
      frameId?: string | null;
    }
  | {
      kind: "zoom";
      placement: "new_tab" | "active_pane";
      percent: number;
    };

// Unified to Pretendard across the UI chrome (the terminal content keeps its own
// monospace font in XtermTerminalRenderer). Bundled locally via src/fonts.css —
// no CDN, robust fallbacks so it degrades gracefully if the face ever fails.
const FONT_MONO =
  "'Pretendard Variable',Pretendard,-apple-system,'Segoe UI','Malgun Gothic',system-ui,sans-serif";
const FONT_SANS = FONT_MONO;
const DEFAULT_WORKSPACE_PLUS_ACTION = "workspace.new";
const DEFAULT_SURFACE_TAB_PLUS_ACTION = "terminal.newWsl";
const DEFAULT_SURFACE_TAB_ACTIONS = ["pane.splitRight", "pane.splitDown"];
const NATIVE_TERMINAL_COMMANDS: Record<Exclude<TerminalProfile, "wsl">, string[]> = {
  powershell: ["powershell.exe", "-NoLogo"],
  cmd: ["cmd.exe", "/d", "/q"],
};
const ACTION_GROUP_ORDER: ActionGroup[] = [
  "agent",
  "terminal",
  "workspace",
  "view",
  "remote",
];
const TEXT_BOX_DEFAULT_MAX_LINES = 7;
const TEXT_BOX_MIN_LINES = 2;
const TEXT_BOX_MAX_LINES = 12;
const TERMINAL_INNER_MARGIN_DEFAULT = 0;
const TERMINAL_INNER_MARGIN_MIN = 0;
const TERMINAL_INNER_MARGIN_MAX = 32;
const WORKSPACE_GROUP_DRAG_TYPE = "application/x-agentmux-workspace-group";
const WORKSPACE_MEMBER_DRAG_TYPE = "application/x-agentmux-workspace-member";
const WORKSPACE_CARD_DRAG_TYPE = "application/x-agentmux-workspace-card";
const SURFACE_TAB_DRAG_TYPE = "application/x-agentmux-surface-tab";
const PANE_SURFACE_DRAG_TYPE = "application/x-agentmux-pane-surface";
const WORKSPACE_ORDER_STORAGE_KEY = "agentmux.workspaceOrder.v1";
const SURFACE_TAB_ORDER_STORAGE_PREFIX = "agentmux.surfaceTabOrder.v1:";

type DropPlacement = "before" | "after";

interface WorkspaceGroupDragPayload {
  groupId: string;
}

interface WorkspaceMemberDragPayload {
  groupId: string;
  workspaceId: string;
}

interface WorkspaceCardDragPayload {
  workspaceId: string;
}

interface SurfaceTabDragPayload {
  workspaceId: string;
  surfaceId: string;
}

interface PaneSurfaceDragPayload {
  workspaceId: string;
  paneId: string;
  surfaceId: string;
}

interface AttentionPaneTarget {
  state: AgentState;
  pane: PaneSummary;
  surface: SurfaceSummary;
}

function dropPlacementFromEvent(
  event: ReactDragEvent<HTMLElement>,
): DropPlacement {
  const rect = event.currentTarget.getBoundingClientRect();
  return event.clientY > rect.top + rect.height / 2 ? "after" : "before";
}

function parseDragPayload<T>(
  event: ReactDragEvent<HTMLElement>,
  type: string,
): T | null {
  const raw = event.dataTransfer.getData(type);
  if (!raw) {
    return null;
  }
  try {
    return JSON.parse(raw) as T;
  } catch {
    return null;
  }
}

function readStoredOrder(key: string): string[] {
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed)
      ? parsed.filter((value): value is string => typeof value === "string")
      : [];
  } catch {
    return [];
  }
}

function writeStoredOrder(key: string, order: string[]): void {
  try {
    window.localStorage.setItem(key, JSON.stringify(order));
  } catch {
    // Ordering is a UX preference; storage failures should not block work.
  }
}

function applyStoredOrder<T>(
  items: T[],
  getId: (item: T) => string,
  storedOrder: string[],
): T[] {
  if (storedOrder.length === 0) return items;
  const rank = new Map(storedOrder.map((id, index) => [id, index]));
  return [...items].sort((left, right) => {
    const leftRank = rank.get(getId(left)) ?? Number.MAX_SAFE_INTEGER;
    const rightRank = rank.get(getId(right)) ?? Number.MAX_SAFE_INTEGER;
    return leftRank - rightRank;
  });
}

function reorderIds(
  current: string[],
  sourceId: string,
  targetId: string,
  placement: DropPlacement,
): string[] {
  if (sourceId === targetId) return current;
  const next = current.filter((id) => id !== sourceId);
  const targetIndex = next.indexOf(targetId);
  if (targetIndex < 0) return current;
  next.splice(targetIndex + (placement === "after" ? 1 : 0), 0, sourceId);
  return next;
}

function moveIdByDirection(
  current: string[],
  sourceId: string,
  direction: -1 | 1,
): string[] {
  const index = current.indexOf(sourceId);
  const nextIndex = index + direction;
  if (index < 0 || nextIndex < 0 || nextIndex >= current.length) {
    return current;
  }
  const next = [...current];
  const [moved] = next.splice(index, 1);
  next.splice(nextIndex, 0, moved);
  return next;
}

const URL_PATTERN = /\bhttps?:\/\/[^\s<>"')\]]+/i;

function normalizeHttpUrl(value: string): string | null {
  const trimmed = value.trim().replace(/[),.;:!?\]}]+$/, "");
  try {
    const parsed = new URL(trimmed);
    return parsed.protocol === "http:" || parsed.protocol === "https:"
      ? parsed.href
      : null;
  } catch {
    return null;
  }
}

function extractFirstUrl(value: string | null | undefined): string | null {
  const match = value?.match(URL_PATTERN);
  return match ? normalizeHttpUrl(match[0]) : null;
}

// Where terminal links (e.g. Claude Code / OAuth login URLs) open. The system
// browser is the default because the embedded browser surface cannot service a
// CLI's localhost loopback auth callback. Persisted client-side so the choice
// survives restarts without a backend config round-trip.
export type TerminalLinkOpenMode = "system" | "in-app";
const TERMINAL_LINK_OPEN_MODE_KEY = "agentmux.terminal.linkOpenMode";
const DEFAULT_TERMINAL_LINK_OPEN_MODE: TerminalLinkOpenMode = "system";

function readTerminalLinkOpenMode(): TerminalLinkOpenMode {
  try {
    return window.localStorage?.getItem(TERMINAL_LINK_OPEN_MODE_KEY) === "in-app"
      ? "in-app"
      : DEFAULT_TERMINAL_LINK_OPEN_MODE;
  } catch {
    return DEFAULT_TERMINAL_LINK_OPEN_MODE;
  }
}

function writeTerminalLinkOpenMode(mode: TerminalLinkOpenMode): void {
  try {
    window.localStorage?.setItem(TERMINAL_LINK_OPEN_MODE_KEY, mode);
  } catch {
    // Preference persistence is best-effort; storage may be unavailable.
  }
}

// Open a URL in the OS default browser. In the Tauri host this calls the
// dependency-free `open_external_url` command; on vite preview / server mode
// there is no host, so fall back to a new browser tab.
async function openUrlInSystemBrowser(url: string): Promise<boolean> {
  const invoke = window.__TAURI__?.core?.invoke;
  if (invoke) {
    try {
      await invoke("open_external_url", { url });
      return true;
    } catch (error) {
      console.warn("[agentmux] system browser open failed", { error, url });
      return false;
    }
  }
  return window.open(url, "_blank", "noopener,noreferrer") !== null;
}

function targetPaneForSplitBrowser(
  detail: WorkspaceDetail,
  splitPaneId: string,
): string | null {
  const children = detail.panes.filter(
    (pane) => pane.parentPaneId === splitPaneId && pane.kind === "leaf",
  );
  const emptyChild = children.find((pane) => !pane.mountedSurfaceId);
  if (emptyChild) {
    return emptyChild.paneId;
  }
  const secondChild = children[1];
  if (secondChild) {
    return secondChild.paneId;
  }
  const activePane = detail.panes.find(
    (pane) => pane.paneId === detail.workspace.activePaneId && pane.kind === "leaf",
  );
  return activePane?.paneId ?? null;
}

function targetPaneForMovedSurfaceSplit(
  detail: WorkspaceDetail,
  splitPaneId: string,
  movingSurfaceId: string,
): string | null {
  const children = detail.panes.filter(
    (pane) => pane.parentPaneId === splitPaneId && pane.kind === "leaf",
  );
  const emptyChild = children.find((pane) => !pane.mountedSurfaceId);
  if (emptyChild) {
    return emptyChild.paneId;
  }
  const nonMovingChild = children.find(
    (pane) => pane.mountedSurfaceId !== movingSurfaceId,
  );
  if (nonMovingChild) {
    return nonMovingChild.paneId;
  }
  return children[1]?.paneId ?? children[0]?.paneId ?? null;
}

function emptyTargetPaneForSplit(
  detail: WorkspaceDetail,
  splitPaneId: string,
): string | null {
  const children = detail.panes.filter(
    (pane) => pane.parentPaneId === splitPaneId && pane.kind === "leaf",
  );
  const emptyChild = children.find((pane) => !pane.mountedSurfaceId);
  if (emptyChild) {
    return emptyChild.paneId;
  }
  return children[1]?.paneId ?? children[0]?.paneId ?? null;
}

function agentCommandFromTelemetry(
  agentState: AgentState | null | undefined,
): string[] {
  const command = splitCommandText(agentState?.telemetry?.session);
  if (command.length === 0) {
    return [];
  }
  if (command[0].toLowerCase().startsWith("session:")) {
    command[0] = command[0].slice("session:".length);
  }
  const executable = command[0].split(/[\\/]/).pop()?.toLowerCase() ?? "";
  if (["claude", "codex", "opencode", "gemini"].includes(executable)) {
    return command;
  }
  return [];
}

function sessionDotColor(
  theme: ThemeTokens,
  session: TerminalSession | undefined,
  attention: boolean,
): string {
  if (attention) return "var(--accent)";
  if (!session) return theme.fg4;
  switch (session.state) {
    case "running":
    case "starting":
    case "recovering":
      return "var(--accent)";
    case "exited":
      return theme.green;
    case "failed":
    case "lost":
      return theme.red;
    default:
      return theme.fg4;
  }
}

function sessionLabel(
  session: TerminalSession | undefined,
  attention: boolean,
): string {
  if (attention) return "입력 대기";
  if (!session) return "";
  switch (session.state) {
    case "running":
      return "실행 중";
    case "starting":
      return "시작 중";
    case "recovering":
      return "복구 중";
    case "detached":
      return "분리됨";
    case "disconnected":
      return "연결 끊김";
    case "exited":
      return "종료됨";
    case "failed":
      return "실패";
    case "lost":
      return "유실";
    default:
      return session.state;
  }
}

function translatedSessionLabel(
  t: Translator,
  session: TerminalSession | undefined,
  attention: boolean,
): string {
  if (attention) return t("session.status.attention");
  if (!session) return "";
  switch (session.state) {
    case "running":
      return t("session.status.running");
    case "starting":
      return t("session.status.starting");
    case "recovering":
      return t("session.status.recovering");
    case "detached":
      return t("session.status.detached");
    case "disconnected":
      return t("session.status.disconnected");
    case "exited":
      return t("session.status.exited");
    case "failed":
      return t("session.status.failed");
    case "lost":
      return t("session.status.lost");
    default:
      return session.state;
  }
}

function actionGroupLabel(t: Translator, group: ActionGroup): string {
  switch (group) {
    case "agent":
      return t("action.group.agent");
    case "terminal":
      return t("action.group.terminal");
    case "workspace":
      return t("action.group.workspace");
    case "view":
      return t("action.group.view");
    case "remote":
      return t("action.group.remote");
  }
}

void sessionLabel;

// A session backs a live terminal only while running/starting. Store-only
// recovery sessions must render placeholders until the backend reattaches.
// or detached sessions (disconnected, detached, exited, failed, lost) — e.g. an
// ephemeral terminal whose process did not survive an app restart — must NOT
// mount a LiveTerminal: its snapshot poll would only ever return SessionNotFound
// and the pane would sit on the "starting…" overlay forever. Such panes render
// the reopenable empty-pane state instead.
function isLiveSession(session: TerminalSession | undefined): boolean {
  return (
    !!session &&
    (session.state === "running" ||
      session.state === "starting" ||
      session.state === "preview")
  );
}

function isClosedTerminalState(state: string | null | undefined): boolean {
  return (
    state === "exited" ||
    state === "failed" ||
    state === "lost" ||
    state === "disconnected"
  );
}

function isRecoveringPlaceholder(
  session: TerminalSession | undefined,
  isBrowser: boolean,
): boolean {
  return Boolean(
    session && !isBrowser && session.state === "recovering" && !isLiveSession(session),
  );
}

function isRestorableAgentPlaceholder(
  session: TerminalSession | undefined,
  agentState: AgentState | null | undefined,
  isBrowser: boolean,
): boolean {
  return Boolean(
    session &&
      !isBrowser &&
      (session.state === "disconnected" || session.state === "recovering") &&
      agentState &&
      !isLiveSession(session),
  );
}

function agentRestoreLabel(agentState: AgentState | null | undefined): string {
  return (
    agentState?.telemetry?.session?.trim() ||
    agentState?.telemetry?.activity?.trim() ||
    "agent"
  );
}

function terminalAgentKind(
  session: TerminalSession | undefined,
  telemetry: AgentTelemetry | null | undefined,
): "claude" | "codex" | null {
  const text = [
    telemetry?.session,
    telemetry?.activity,
    session?.backendKind,
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
  if (/\bcodex\b/.test(text)) {
    return "codex";
  }
  if (/\bclaude\b/.test(text)) {
    return "claude";
  }
  return null;
}

function surfaceTabActionIcon(actionId: string): ReactNode {
  if (actionId === "pane.splitRight") {
    return <IconSplitCols size={13} />;
  }
  if (actionId === "pane.splitDown") {
    return <IconSplitRows size={13} />;
  }
  if (actionId.startsWith("browser.")) {
    return <IconGrid size={13} />;
  }
  if (actionId.startsWith("workspace.")) {
    return <IconFolder size={13} />;
  }
  if (actionId.startsWith("agent.") || actionId.startsWith("custom.")) {
    return <IconShellArrow size={13} />;
  }
  return <IconPlus size={13} />;
}

// --- module-level style constants (PR-2 / PR-3) -----------------------------
// These inline style objects reference only CSS variables (resolved at paint
// from the themed root vars) or static literals, so they never depend on
// component state. Hoisting them to module scope keeps their references stable
// across renders, so style props no longer churn every poll tick.
const ICON_BTN_STYLE: CSSProperties = {
  width: 30,
  height: 30,
  borderRadius: 7,
  border: 0,
  background: "transparent",
  cursor: "pointer",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  color: "var(--fg3)",
};
const ICON_BTN_HOVER_STYLE: CSSProperties = {
  background: "var(--s2)",
  color: "var(--fg1)",
};
// Frameless-window controls (decorations:false): full-height, flush to the
// top-right corner like a native caption.
const WIN_CTL_BTN_STYLE: CSSProperties = {
  width: 46,
  height: "100%",
  border: 0,
  // Override the global `button { border-radius: 6px }` rule — caption controls
  // are sharp rectangles flush to the window edge; the window's own rounded
  // top-right corner clips the close button for a native look.
  borderRadius: 0,
  margin: 0,
  padding: 0,
  background: "transparent",
  cursor: "pointer",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  color: "var(--fg3)",
};
const WIN_CTL_BTN_HOVER_STYLE: CSSProperties = {
  background: "var(--s2)",
  color: "var(--fg1)",
};
const GROUP_ACTION_BTN_STYLE: CSSProperties = {
  width: 22,
  height: 22,
  flex: "none",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  background: "transparent",
  border: "1px solid transparent",
  borderRadius: 6,
  color: "var(--fg4)",
  cursor: "pointer",
};
const GROUP_ACTION_HOVER_STYLE: CSSProperties = {
  background: "var(--s2)",
  borderColor: "var(--border)",
  color: "var(--fg1)",
};
const GROUP_MENU_ITEM_STYLE: CSSProperties = {
  width: "100%",
  display: "flex",
  alignItems: "center",
  gap: 8,
  padding: "8px 10px",
  border: 0,
  borderRadius: 6,
  background: "transparent",
  color: "var(--fg2)",
  cursor: "pointer",
  textAlign: "left",
  font: `600 12px/1 ${FONT_SANS}`,
};
const GROUP_MENU_ITEM_HOVER_STYLE: CSSProperties = {
  background: "var(--s2)",
  color: "var(--fg1)",
};
// PR-2: per-leaf pane caption-button styles, hoisted so PaneView receives the
// same references each render.
const PANE_WIN_BTN_STYLE: CSSProperties = {
  width: 23,
  height: 23,
  borderRadius: 5,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  color: "var(--fg4)",
  cursor: "pointer",
};
const PANE_WIN_BTN_HOVER_STYLE: CSSProperties = {
  background: "var(--s2)",
  color: "var(--fg1)",
};

function surfaceTabActionClassName(actionId: string): string {
  if (actionId === "pane.splitRight") {
    return "agentmux-tab-action agentmux-top-split-vertical";
  }
  if (actionId === "pane.splitDown") {
    return "agentmux-tab-action agentmux-top-split-horizontal";
  }
  return `agentmux-tab-action agentmux-tab-action-${actionClassFragment(actionId)}`;
}

function actionClassFragment(actionId: string): string {
  return actionId.replace(/[^A-Za-z0-9_-]+/g, "-");
}

function matchesNotificationAction(
  hook: AppConfigNotificationAction,
  notification: NotificationSummary,
): boolean {
  const typeMatches =
    !hook.notificationType ||
    hook.notificationType === notification.notificationType;
  const severityMatches =
    !hook.severity || hook.severity === notification.severity;
  return typeMatches && severityMatches;
}

function browserCustomActionPreset(
  command: string[],
): BrowserCustomActionPreset {
  if (command.length === 0) {
    return { kind: "open", placement: "new_tab", url: null };
  }
  const operation = command[0].toLowerCase();
  if (operation === "open" || operation === "navigate") {
    return {
      kind: "open",
      url: command[1] ?? null,
      placement: browserPlacementFromCommand(command[2]),
    };
  }
  if (operation === "active-pane" || operation === "active_pane") {
    return { kind: "open", placement: "active_pane", url: command[1] ?? null };
  }
  if (operation === "screenshot") {
    return {
      kind: "screenshot",
      format: command[1] ?? null,
      placement: browserPlacementFromCommand(command[2], "active_pane"),
    };
  }
  if (operation === "dom-snapshot" || operation === "dom_snapshot") {
    return {
      kind: "dom_snapshot",
      placement: browserPlacementFromCommand(command[1], "active_pane"),
      frameId: browserFrameIdFromCommand(command[2]),
    };
  }
  if (operation === "evaluate") {
    return {
      kind: "evaluate",
      script: command[1] ?? "",
      placement: browserPlacementFromCommand(command[2], "active_pane"),
      frameId: browserFrameIdFromCommand(command[3]),
    };
  }
  if (operation === "click") {
    return {
      kind: "click",
      selector: command[1] ?? "",
      placement: browserPlacementFromCommand(command[2], "active_pane"),
      frameId: browserFrameIdFromCommand(command[3]),
    };
  }
  if (operation === "type") {
    return {
      kind: "type",
      selector: command[1] ?? "",
      text: command[2] ?? "",
      placement: browserPlacementFromCommand(command[3], "active_pane"),
      frameId: browserFrameIdFromCommand(command[4]),
    };
  }
  if (operation === "fill") {
    return {
      kind: "fill",
      selector: command[1] ?? "",
      text: command[2] ?? "",
      placement: browserPlacementFromCommand(command[3], "active_pane"),
      frameId: browserFrameIdFromCommand(command[4]),
    };
  }
  if (operation === "press") {
    return {
      kind: "press",
      selector: command[1] ?? "",
      key: command[2] ?? "",
      placement: browserPlacementFromCommand(command[3], "active_pane"),
      frameId: browserFrameIdFromCommand(command[4]),
    };
  }
  if (operation === "select") {
    const framePlacementToken = command[command.length - 2];
    const hasFrameSlot =
      command.length >= 5 &&
      browserPlacementFromCommand(framePlacementToken, "new_tab") ===
        framePlacementToken;
    const placementToken = hasFrameSlot
      ? command[command.length - 2]
      : command[command.length - 1];
    const placement = browserPlacementFromCommand(
      placementToken,
      "active_pane",
    );
    const hasPlacement =
      placementToken === "new_tab" ||
      placementToken === "new-tab" ||
      placementToken === "active_pane" ||
      placementToken === "active-pane";
    return {
      kind: "select",
      selector: command[1] ?? "",
      values: command.slice(
        2,
        hasFrameSlot ? -2 : hasPlacement ? -1 : undefined,
      ),
      placement,
      frameId: browserFrameIdFromCommand(
        hasFrameSlot ? command[command.length - 1] : undefined,
      ),
    };
  }
  if (operation === "scroll") {
    return {
      kind: "scroll",
      selector: command[1]?.trim() ? command[1] : null,
      x: parseOptionalInteger(command[2]) ?? 0,
      y: parseOptionalInteger(command[3]) ?? 0,
      placement: browserPlacementFromCommand(command[4], "active_pane"),
      frameId: browserFrameIdFromCommand(command[5]),
    };
  }
  if (operation === "hover") {
    return {
      kind: "hover",
      selector: command[1] ?? "",
      placement: browserPlacementFromCommand(command[2], "active_pane"),
      frameId: browserFrameIdFromCommand(command[3]),
    };
  }
  if (operation === "check" || operation === "uncheck") {
    return {
      kind: "check",
      selector: command[1] ?? "",
      checked:
        operation === "uncheck"
          ? false
          : (parseOptionalBoolean(command[2]) ?? true),
      placement: browserPlacementFromCommand(command[3], "active_pane"),
      frameId: browserFrameIdFromCommand(command[4]),
    };
  }
  if (operation === "highlight") {
    const thirdArgIsDuration = isPositiveIntegerText(command[2]);
    return {
      kind: "highlight",
      selector: command[1] ?? "",
      placement: thirdArgIsDuration
        ? browserPlacementFromCommand(command[3], "active_pane")
        : browserPlacementFromCommand(command[2], "active_pane"),
      durationMs: parseOptionalPositiveInteger(
        thirdArgIsDuration ? command[2] : command[3],
      ),
      frameId: browserFrameIdFromCommand(command[4]),
    };
  }
  if (operation === "focus") {
    return {
      kind: "focus",
      selector: command[1] ?? "",
      placement: browserPlacementFromCommand(command[2], "active_pane"),
      frameId: browserFrameIdFromCommand(command[3]),
    };
  }
  if (operation === "zoom") {
    return {
      kind: "zoom",
      percent: parseOptionalPositiveInteger(command[1]) ?? 100,
      placement: browserPlacementFromCommand(command[2], "active_pane"),
    };
  }
  if (
    operation === "wait" ||
    operation === "wait-for-selector" ||
    operation === "wait_for_selector"
  ) {
    const placementToken = command[2];
    const thirdArgIsTimeout = isPositiveIntegerText(placementToken);
    return {
      kind: "wait_for_selector",
      selector: command[1] ?? "",
      placement: thirdArgIsTimeout
        ? "active_pane"
        : browserPlacementFromCommand(placementToken, "active_pane"),
      timeoutMs: parseOptionalPositiveInteger(
        thirdArgIsTimeout ? placementToken : command[3],
      ),
      frameId: browserFrameIdFromCommand(command[4]),
    };
  }
  if (
    operation === "reload" ||
    operation === "refresh" ||
    operation === "back" ||
    operation === "go-back" ||
    operation === "go_back" ||
    operation === "forward" ||
    operation === "go-forward" ||
    operation === "go_forward" ||
    operation === "current-url" ||
    operation === "current_url" ||
    operation === "url"
  ) {
    return {
      kind: "navigation_control",
      operation: browserNavigationOperation(operation),
      placement: browserPlacementFromCommand(command[1], "active_pane"),
    };
  }
  return { kind: "open", placement: "new_tab", url: command[1] ?? null };
}

function browserNavigationOperation(
  operation: string,
): "reload" | "back" | "forward" | "current_url" {
  if (
    operation === "back" ||
    operation === "go-back" ||
    operation === "go_back"
  ) {
    return "back";
  }
  if (
    operation === "forward" ||
    operation === "go-forward" ||
    operation === "go_forward"
  ) {
    return "forward";
  }
  if (
    operation === "current-url" ||
    operation === "current_url" ||
    operation === "url"
  ) {
    return "current_url";
  }
  return "reload";
}

function isPositiveIntegerText(value: string | undefined): boolean {
  return parseOptionalPositiveInteger(value) !== null;
}

function parseOptionalPositiveInteger(
  value: string | undefined,
): number | null {
  if (value === undefined || value.trim().length === 0) {
    return null;
  }
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) {
    return null;
  }
  const parsed = Number.parseInt(trimmed, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

function parseOptionalInteger(value: string | undefined): number | null {
  if (value === undefined || value.trim().length === 0) {
    return null;
  }
  const trimmed = value.trim();
  if (!/^-?\d+$/.test(trimmed)) {
    return null;
  }
  const parsed = Number.parseInt(trimmed, 10);
  return Number.isFinite(parsed) ? parsed : null;
}

function parseOptionalBoolean(value: string | undefined): boolean | null {
  const normalized = value?.trim().toLowerCase();
  if (!normalized) {
    return null;
  }
  if (["true", "1", "yes", "on", "checked"].includes(normalized)) {
    return true;
  }
  if (["false", "0", "no", "off", "unchecked"].includes(normalized)) {
    return false;
  }
  return null;
}

function browserPlacementFromCommand(
  value: string | undefined,
  fallback: "new_tab" | "active_pane" = "new_tab",
): "new_tab" | "active_pane" {
  const placement = value?.toLowerCase();
  if (placement === "new-tab" || placement === "new_tab") {
    return "new_tab";
  }
  if (placement === "active-pane" || placement === "active_pane") {
    return "active_pane";
  }
  return fallback;
}

function browserFrameIdFromCommand(value: string | undefined): string | null {
  const frameId = value?.trim();
  return frameId ? frameId : null;
}

function searchableText(parts: Array<string | null | undefined>): string {
  return parts
    .map((part) => part?.trim() ?? "")
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
}

function searchableWorkspaceText(workspace: WorkspaceSummary): string {
  return searchableText([
    workspace.name,
    workspace.projectRoot,
    workspace.description,
    workspace.icon,
    workspace.color,
    workspace.defaultWslDistribution,
    workspace.defaultTerminalProfile,
    workspace.defaultAgentCommand,
  ]);
}

function searchableWorkspaceGroupText(group: WorkspaceGroup): string {
  return searchableText([group.name, group.icon, group.color]);
}

function splitCommandText(raw: string | null | undefined): string[] {
  const text = raw?.trim();
  if (!text) {
    return [];
  }
  const parts = text.match(/"([^"\\]|\\.)*"|'([^'\\]|\\.)*'|\S+/g) ?? [];
  return parts
    .map((part) => {
      if (
        (part.startsWith('"') && part.endsWith('"')) ||
        (part.startsWith("'") && part.endsWith("'"))
      ) {
        return part.slice(1, -1).replace(/\\(["'\\])/g, "$1");
      }
      return part;
    })
    .filter(Boolean);
}

function isEditableShortcutTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  if (target.closest(".xterm")) {
    return false;
  }
  const tagName = target.tagName.toLowerCase();
  return (
    target.isContentEditable ||
    tagName === "input" ||
    tagName === "textarea" ||
    tagName === "select"
  );
}

function groupForCustomAction(
  action: AppConfigCustomAction,
): ActionDescriptor["group"] {
  switch (action.group) {
    case "agent":
    case "terminal":
    case "workspace":
    case "view":
    case "remote":
      return action.group;
    default:
      return action.target === "agent"
        ? "agent"
        : action.target === "browser"
          ? "terminal"
          : "remote";
  }
}

function nextWorkspaceGroupName(groups: WorkspaceGroup[]): string {
  const usedNames = new Set(groups.map((group) => group.name));
  for (let index = 1; ; index += 1) {
    const candidate = `Group ${index}`;
    if (!usedNames.has(candidate)) {
      return candidate;
    }
  }
}

function normalizeGroupIcon(value: string | null | undefined): string | null {
  const text = value?.trim().toUpperCase() ?? "";
  return text ? text.slice(0, 2) : null;
}

function normalizeGroupColor(value: string | null | undefined): string | null {
  const text = value?.trim() ?? "";
  if (!text) {
    return null;
  }
  return text.startsWith("#") ? text : `#${text}`;
}

function workspaceSessionSurfaceCount(detail: WorkspaceDetail): number {
  return detail.surfaces.filter((surface) => surface.sessionId).length;
}

function isWorkspaceRunningCloseError(cause: unknown): boolean {
  if (!cause || typeof cause !== "object") {
    return false;
  }
  const message = cause instanceof Error ? cause.message : "";
  const code =
    "code" in cause && typeof cause.code === "string" ? cause.code : null;
  return (
    (code === "conflict" && /running sessions/i.test(message)) ||
    /workspace has running sessions/i.test(message)
  );
}

type AppConfirmVariant = "default" | "danger";

interface AppConfirmOptions {
  title: string;
  message: string;
  detail?: string;
  confirmLabel: string;
  cancelLabel?: string;
  variant?: AppConfirmVariant;
}

interface AppConfirmDialog extends AppConfirmOptions {
  variant: AppConfirmVariant;
}

// PR-2: the leaf (terminal/browser/empty) pane renderer, extracted into a
// `React.memo` component so panes whose inputs are unchanged do not reconcile on
// every 1.2s poll tick. The recursive split wrapper stays in the parent and
// renders one <PaneView/> per leaf. All derived data is computed in the parent
// and passed in; callbacks are the parent's stable useCallback handlers so the
// memo comparison holds across no-op ticks. Behaviour is identical to the
// previous inline leaf branch.
interface PaneViewProps {
  pane: PaneSummary;
  surface: SurfaceSummary | undefined;
  session: TerminalSession | undefined;
  active: boolean;
  isBrowser: boolean;
  agentState: AgentState | null;
  telemetry: AgentTelemetry | null;
  hasAttention: boolean;
  attentionReason?: string | null;
  title: string;
  dot: string;
  label: string;
  theme: ThemeTokens;
  client: ControlClient;
  terminalInnerMargin: number;
  fontSize: number;
  terminalLaunchPending: boolean;
  t: Translator;
  focusPane: (paneId: string) => void;
  splitPaneBy: (paneId: string, axis: "horizontal" | "vertical") => void;
  closePane: (paneId: string) => void;
  closeSurface: (surfaceId: string) => void;
  openTerminalInPane: (paneId: string) => void;
  openTerminalProfileMenu: (
    event: ReactMouseEvent<HTMLElement>,
    paneId?: string | null,
  ) => void;
  openDurableTerminalInPane: (paneId: string) => void;
  onOpenTerminalLink: (url: string, paneId: string) => void;
  onTerminalExitIntent: (sessionId: string) => void;
  onPaneDragStart: (
    event: ReactDragEvent<HTMLElement>,
    paneId: string,
    surfaceId?: string | null,
  ) => void;
  onPaneDragOver: (event: ReactDragEvent<HTMLElement>) => void;
  onPaneDrop: (
    event: ReactDragEvent<HTMLElement>,
    pane: PaneSummary,
  ) => void;
  onMovePaneSurface: (paneId: string, direction: -1 | 1) => void;
  onTerminalError: () => void;
}

const PaneView = memo(function PaneView({
  pane,
  surface,
  session,
  active,
  isBrowser,
  agentState,
  telemetry,
  hasAttention,
  attentionReason,
  title,
  dot,
  label,
  theme,
  client,
  terminalInnerMargin,
  fontSize,
  terminalLaunchPending,
  t,
  focusPane,
  splitPaneBy,
  closePane,
  closeSurface,
  openTerminalInPane,
  openTerminalProfileMenu,
  openDurableTerminalInPane,
  onOpenTerminalLink,
  onTerminalExitIntent,
  onPaneDragStart,
  onPaneDragOver,
  onPaneDrop,
  onMovePaneSurface,
  onTerminalError,
}: PaneViewProps) {
  const restoringAgent = isRestorableAgentPlaceholder(
    session,
    agentState,
    isBrowser,
  );
  const restoringTerminal = isRecoveringPlaceholder(session, isBrowser);
  const restoringAgentLabel = agentRestoreLabel(agentState);
  const restoringLabel = restoringAgent
    ? restoringAgentLabel
    : session?.backendKind ?? "terminal";
  const restoreFallback = (
    <div
      style={{
        height: "100%",
        display: "flex",
        flexDirection: "column",
        gap: 10,
        alignItems: "center",
        justifyContent: "center",
        color: "var(--fg3)",
        background: "var(--term)",
      }}
    >
      <span className="agentmux-term-booting-spinner" />
      <span style={{ font: `600 12px/1 ${FONT_SANS}` }}>
        {t("pane.restoring")}
      </span>
      <span
        style={{
          font: `600 10px/1 ${FONT_MONO}`,
          color: "var(--accent)",
          background: "var(--accent-soft)",
          border: "1px solid rgba(88, 166, 255, 0.28)",
          borderRadius: 4,
          padding: "4px 7px",
        }}
      >
        {restoringLabel}
      </span>
    </div>
  );

  return (
    <div
      key={pane.paneId}
      data-agentmux-pane={pane.paneId}
      data-agentmux-mounted={surface ? "true" : "false"}
      data-agentmux-mounted-surface={surface?.surfaceId ?? ""}
      data-agentmux-active={active ? "true" : "false"}
      data-agentmux-attention={hasAttention ? "true" : "false"}
      onDragOver={onPaneDragOver}
      onDrop={(event) => onPaneDrop(event, pane)}
      onMouseDown={() => focusPane(pane.paneId)}
      style={{
        minHeight: 0,
        minWidth: 0,
        flex: "1 1 0",
        background: "var(--term)",
        // Active highlight is a 1px accent border — same thickness as the
        // inactive 1px border, so focus never shifts layout. No extra inset
        // shadow: that doubled the edge to 2px, which showed through on empty
        // panes (no terminal content to paint over the inset ring).
        border: `1px solid ${
          hasAttention ? "var(--accent)" : active ? "var(--accent)" : "var(--border)"
        }`,
        borderRadius: 7,
        boxShadow: hasAttention
          ? "0 0 0 1px rgba(88, 166, 255, 0.58), 0 0 0 4px rgba(88, 166, 255, 0.16)"
          : "none",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
      }}
    >
      <div
        style={{
          height: 32,
          flex: "none",
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "0 9px",
          background: "var(--surface)",
          borderBottom: "1px solid var(--border)",
        }}
        draggable={Boolean(surface?.surfaceId)}
        onDragStart={(event) =>
          onPaneDragStart(event, pane.paneId, surface?.surfaceId)
        }
      >
        <span
          style={{
            width: 7,
            height: 7,
            borderRadius: "50%",
            flex: "none",
            background: dot,
          }}
        />
        <span
          style={{
            font: `600 11.5px/1 ${FONT_MONO}`,
            color: "var(--fg1)",
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {title}
        </span>
        {label ? (
          <span
            className={hasAttention ? "attention-pill" : undefined}
            title={hasAttention ? attentionReason ?? "Agent needs input" : undefined}
            style={{
              font: `500 9.5px/1 ${FONT_SANS}`,
              color: hasAttention ? "var(--accent)" : dot,
              background: hasAttention ? "var(--accent-soft)" : "var(--s2)",
              border: hasAttention ? "1px solid rgba(88, 166, 255, 0.38)" : 0,
              borderRadius: 4,
              padding: "3px 6px",
              flex: "none",
              whiteSpace: "nowrap",
            }}
          >
            {label}
          </span>
        ) : null}
        {session ? (
          <span
            style={{
              font: `500 10px/1 ${FONT_MONO}`,
              color: "var(--fg3)",
              background: "var(--s2)",
              border: "1px solid var(--border)",
              borderRadius: 4,
              padding: "3px 6px",
              flex: "none",
            }}
          >
            {session.backendKind}
          </span>
        ) : null}
        <div style={{ flex: 1 }} />
        <div style={{ display: "flex", gap: 1, flex: "none" }}>
          <Hov
            tag="span"
            className="agentmux-pane-surface-move-prev"
            ariaLabel="Move pane surface earlier"
            title="Move pane surface earlier"
            style={{
              ...PANE_WIN_BTN_STYLE,
              opacity: surface ? 1 : 0.35,
              cursor: surface ? "pointer" : "default",
            }}
            hover={surface ? PANE_WIN_BTN_HOVER_STYLE : {}}
            onClick={(e) => {
              e.stopPropagation();
              if (surface) {
                onMovePaneSurface(pane.paneId, -1);
              }
            }}
          >
            <span style={{ display: "flex", transform: "rotate(180deg)" }}>
              <IconChevronRight size={12} />
            </span>
          </Hov>
          <Hov
            tag="span"
            className="agentmux-pane-surface-move-next"
            ariaLabel="Move pane surface later"
            title="Move pane surface later"
            style={{
              ...PANE_WIN_BTN_STYLE,
              opacity: surface ? 1 : 0.35,
              cursor: surface ? "pointer" : "default",
            }}
            hover={surface ? PANE_WIN_BTN_HOVER_STYLE : {}}
            onClick={(e) => {
              e.stopPropagation();
              if (surface) {
                onMovePaneSurface(pane.paneId, 1);
              }
            }}
          >
            <IconChevronRight size={12} />
          </Hov>
          <Hov
            tag="span"
            className="agentmux-pane-split-vertical"
            style={PANE_WIN_BTN_STYLE}
            hover={PANE_WIN_BTN_HOVER_STYLE}
            onClick={(e) => {
              e.stopPropagation();
              splitPaneBy(pane.paneId, "vertical");
            }}
          >
            <IconSplitCols size={12} />
          </Hov>
          <Hov
            tag="span"
            className="agentmux-pane-split-horizontal"
            style={PANE_WIN_BTN_STYLE}
            hover={PANE_WIN_BTN_HOVER_STYLE}
            onClick={(e) => {
              e.stopPropagation();
              splitPaneBy(pane.paneId, "horizontal");
            }}
          >
            <IconSplitRows size={12} />
          </Hov>
          <Hov
            tag="span"
            className="agentmux-pane-close"
            style={PANE_WIN_BTN_STYLE}
            hover={PANE_WIN_BTN_HOVER_STYLE}
            onClick={(e) => {
              e.stopPropagation();
              if (pane.parentPaneId) closePane(pane.paneId);
              else if (surface) closeSurface(surface.surfaceId);
            }}
          >
            <IconClose size={11} />
          </Hov>
        </div>
      </div>
      <div
        style={{
          flex: 1,
          minHeight: 0,
          minWidth: 0,
          display: "flex",
          flexDirection: "column",
        }}
      >
        <div
          style={{
            flex: "1 1 0",
            minHeight: 0,
            minWidth: 0,
            position: "relative",
            display: "flex",
            flexDirection: "column",
            overflow: "hidden",
          }}
        >
          {session && isLiveSession(session) && !isBrowser ? (
            <LiveTerminal
              key={session.sessionId}
              client={client}
              sessionId={session.sessionId}
              active={active}
              agentKind={terminalAgentKind(session, telemetry)}
              innerMargin={terminalInnerMargin}
              fontSize={fontSize}
              onFocus={() => focusPane(pane.paneId)}
              onError={onTerminalError}
              onOpenLink={(url) => onOpenTerminalLink(url, pane.paneId)}
              onExitIntent={() => onTerminalExitIntent(session.sessionId)}
            />
          ) : isBrowser && surface ? (
            <BrowserSurfacePanel client={client} surfaceId={surface.surfaceId} />
          ) : (restoringAgent || restoringTerminal) && session ? (
            <TerminalRestorePreview
              sessionId={session.sessionId}
              innerMargin={terminalInnerMargin}
              fontSize={fontSize}
              fallback={restoreFallback}
            />
          ) : (
            <div
              style={{
                height: "100%",
                display: "flex",
                flexDirection: "column",
                gap: 10,
                alignItems: "center",
                justifyContent: "center",
                color: "var(--fg4)",
              }}
            >
              <span style={{ font: `500 12px/1 ${FONT_SANS}` }}>
                {t("pane.empty")}
              </span>
              <button
                type="button"
                disabled={terminalLaunchPending}
                onClick={(e) => {
                  e.stopPropagation();
                  openTerminalInPane(pane.paneId);
                }}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 7,
                  background: "var(--accent)",
                  color: "#fff",
                  border: 0,
                  borderRadius: 8,
                  padding: "8px 14px",
                  cursor: terminalLaunchPending ? "wait" : "pointer",
                  opacity: terminalLaunchPending ? 0.72 : 1,
                  font: `600 12px/1 ${FONT_SANS}`,
                }}
              >
                <IconPlus size={13} /> Open terminal
              </button>
              <button
                type="button"
                className="agentmux-pane-terminal-profile-menu-button"
                disabled={terminalLaunchPending}
                onClick={(e) => {
                  e.stopPropagation();
                  openTerminalProfileMenu(e, pane.paneId);
                }}
                title="Choose terminal profile"
                aria-label="Choose terminal profile for pane"
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  background: "transparent",
                  color: "var(--fg3)",
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  padding: "6px 10px",
                  cursor: terminalLaunchPending ? "wait" : "pointer",
                  opacity: terminalLaunchPending ? 0.72 : 1,
                  font: `600 11px/1 ${FONT_SANS}`,
                }}
              >
                <IconChevronDown size={12} /> Choose shell
              </button>
              <button
                type="button"
                disabled={terminalLaunchPending}
                onClick={(e) => {
                  e.stopPropagation();
                  openDurableTerminalInPane(pane.paneId);
                }}
                title="Durable WSL session (tmux) survives restarts and reconnects after disconnects."
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  background: "transparent",
                  color: "var(--fg3)",
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  padding: "6px 12px",
                  cursor: terminalLaunchPending ? "wait" : "pointer",
                  opacity: terminalLaunchPending ? 0.72 : 1,
                  font: `600 11px/1 ${FONT_SANS}`,
                }}
              >
                <IconServer size={12} /> Durable terminal (tmux)
              </button>
            </div>
          )}
        </div>
        {telemetry ? <OmcBar telemetry={telemetry} theme={theme} /> : null}
      </div>
    </div>
  );
});

export function AgentmuxTerminalApp() {
  const ctl = useAgentmuxControl();
  const {
    client,
    ready,
    error,
    workspaces,
    workspaceGroups,
    activeWorkspaceId,
    detail,
    attention,
    notifications,
    teamTasks,
    teamMessages,
    sidebarState,
    wslDistributions,
    profiles,
    attentionByWorkspace,
    attentionBySession,
    agentBySession,
  } = ctl;
  const autoClosingExitedSessionsRef = useRef<Set<string>>(new Set());
  const exitIntentSessionIdsRef = useRef<Set<string>>(new Set());
  const exitIntentRefreshTimersRef = useRef<number[]>([]);

  const [theme, setTheme] = useState<ThemeName>("dark");
  const [language, setLanguage] = useState<AppLocaleLanguage>("en");
  const [accentKey, setAccentKey] = useState("blue");
  const [overlay, setOverlay] = useState<Overlay>(null);
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("appearance");
  const [query, setQuery] = useState("");
  const [paletteSelectedIndex, setPaletteSelectedIndex] = useState(0);
  const [fontSize, setFontSize] = useState(12.5);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [windowMaximized, setWindowMaximized] = useState(false);
  useEffect(() => watchMaximized(setWindowMaximized), []);
  useEffect(() => {
    // Apply the saved zoom or the adaptive default for this display (not
    // persisted — the default is recomputed each launch). Ctrl +/-/0 adjust it.
    initZoom();
  }, []);
  const [configLoaded, setConfigLoaded] = useState(false);
  const [configPath, setConfigPath] = useState("");
  const [projectConfigPath, setProjectConfigPath] = useState<string | null>(
    null,
  );
  const [projectConfigLoaded, setProjectConfigLoaded] = useState(false);
  const [configDiagnostics, setConfigDiagnostics] = useState<
    AppConfigDiagnosticEntry[]
  >([]);
  const [configReloadMessage, setConfigReloadMessage] = useState("");
  const [tmuxProbe, setTmuxProbe] = useState<TmuxDiagnostics | null>(null);
  const [tmuxProbeBusy, setTmuxProbeBusy] = useState(false);
  const [shortcutOverrides, setShortcutOverrides] =
    useState<ShortcutBindingMap>({});
  const [customActions, setCustomActions] = useState<AppConfigCustomAction[]>(
    [],
  );
  const [uiConfig, setUiConfig] = useState<AppConfigUi>({});
  const terminalSplitBehavior: TerminalSplitBehavior =
    uiConfig.terminalSplitBehavior ?? "clone_current";
  const [terminalLinkOpenMode, setTerminalLinkOpenModeState] =
    useState<TerminalLinkOpenMode>(readTerminalLinkOpenMode);
  const setTerminalLinkOpenMode = useCallback((mode: TerminalLinkOpenMode) => {
    setTerminalLinkOpenModeState(mode);
    writeTerminalLinkOpenMode(mode);
  }, []);
  const [updatesConfig, setUpdatesConfig] = useState<AppConfigUpdates>(
    DEFAULT_UPDATES_CONFIG,
  );
  const [updateState, setUpdateState] = useState<AppUpdateState>(
    DEFAULT_UPDATE_STATE,
  );
  const [notificationActions, setNotificationActions] = useState<
    AppConfigNotificationAction[]
  >([]);
  const [dockConfig, setDockConfig] = useState<DockConfig | null>(null);
  const [dockTrusted, setDockTrusted] = useState(false);
  const [dockRunMessage, setDockRunMessage] = useState("");
  const [shortcutEditMessage, setShortcutEditMessage] = useState("");
  const [editingWorkspaceId, setEditingWorkspaceId] = useState<string | null>(
    null,
  );
  const [workspaceNameDraft, setWorkspaceNameDraft] = useState("");
  const [workspaceFilterText, setWorkspaceFilterText] = useState("");
  const [workspaceOrder, setWorkspaceOrder] = useState<string[]>(() =>
    readStoredOrder(WORKSPACE_ORDER_STORAGE_KEY),
  );
  const [surfaceTabOrderByWorkspace, setSurfaceTabOrderByWorkspace] = useState<
    Record<string, string[]>
  >({});
  const [selectedWorkspaceIds, setSelectedWorkspaceIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [workspaceGroupMenu, setWorkspaceGroupMenu] = useState<{
    groupId: string;
    x: number;
    y: number;
  } | null>(null);
  const [workspaceMenu, setWorkspaceMenu] = useState<{
    workspaceId: string;
    x: number;
    y: number;
  } | null>(null);
  const [surfaceTabMenu, setSurfaceTabMenu] = useState<{
    surfaceId: string;
    x: number;
    y: number;
  } | null>(null);
  const [terminalProfileMenu, setTerminalProfileMenu] = useState<{
    x: number;
    y: number;
    paneId?: string | null;
  } | null>(null);
  const [textBoxOpen, setTextBoxOpen] = useState(false);
  const [textBoxDraft, setTextBoxDraft] = useState("");
  const [activeDockSessionId, setActiveDockSessionId] = useState<string | null>(
    null,
  );
  const [terminalLaunchPending, setTerminalLaunchPending] = useState(false);
  const [confirmDialog, setConfirmDialog] = useState<AppConfirmDialog | null>(
    null,
  );
  const terminalLaunchPendingRef = useRef(false);
  const autoUpdateCheckStartedRef = useRef(false);
  const updateResourceRef = useRef<TauriUpdate | null>(null);
  const pendingShortcutRef = useRef<string | null>(null);
  const pendingShortcutTimerRef = useRef<number | null>(null);
  const confirmResolverRef = useRef<((confirmed: boolean) => void) | null>(
    null,
  );

  const applyConfig = useCallback((config: AppConfig) => {
    setTheme(config.appearance.theme);
    setAccentKey(
      ACCENTS.some((candidate) => candidate.key === config.appearance.accentKey)
        ? config.appearance.accentKey
        : "blue",
    );
    setFontSize(Math.min(16, Math.max(11, config.appearance.fontSize)));
    setLanguage(config.locale.language);
    setUpdatesConfig(config.updates ?? DEFAULT_UPDATES_CONFIG);
    setShortcutOverrides(config.shortcuts.bindings);
    setCustomActions(config.actions.custom);
    setUiConfig(config.ui ?? {});
    setNotificationActions(config.notifications?.actions ?? []);
    setConfigPath(config.configPath);
    setProjectConfigPath(config.projectConfigPath ?? null);
    setProjectConfigLoaded(config.projectConfigLoaded);
  }, []);

  const refreshConfigDiagnostics = useCallback(async () => {
    try {
      setConfigDiagnostics(await client.configDiagnostics(activeWorkspaceId));
    } catch {
      setConfigDiagnostics([]);
    }
  }, [activeWorkspaceId, client]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const config = await client.getConfig(activeWorkspaceId);
        if (cancelled) {
          return;
        }
        applyConfig(config);
      } catch {
        // Config should not block the terminal UI; defaults remain usable.
      }
      try {
        const diagnostics = await client.configDiagnostics(activeWorkspaceId);
        if (!cancelled) {
          setConfigDiagnostics(diagnostics);
        }
      } catch {
        if (!cancelled) {
          setConfigDiagnostics([]);
        }
      } finally {
        if (!cancelled) {
          setConfigLoaded(true);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeWorkspaceId, applyConfig, client]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const dock = await client.getDock(activeWorkspaceId);
        if (!cancelled) {
          setDockConfig(dock);
        }
      } catch {
        if (!cancelled) {
          setDockConfig(null);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeWorkspaceId, client]);

  useEffect(() => {
    setDockTrusted(Boolean(dockConfig?.trusted));
    setDockRunMessage("");
  }, [dockConfig]);

  useEffect(() => {
    if (!configLoaded) {
      return;
    }
    void client
      .updateConfig(
        {
          appearance: {
            theme,
            accentKey,
            fontSize,
          },
        },
        activeWorkspaceId,
      )
      .catch(() => undefined);
  }, [accentKey, activeWorkspaceId, client, configLoaded, fontSize, theme]);

  useEffect(() => {
    if (!configLoaded) {
      return;
    }
    void client
      .updateConfig(
        {
          locale: {
            language,
          },
        },
        activeWorkspaceId,
      )
      .catch(() => undefined);
  }, [activeWorkspaceId, client, configLoaded, language]);

  useEffect(() => {
    if (!configLoaded) {
      return;
    }
    void client
      .updateConfig(
        {
          updates: updatesConfig,
        },
        activeWorkspaceId,
      )
      .catch(() => undefined);
  }, [activeWorkspaceId, client, configLoaded, updatesConfig]);

  const T = THEMES[theme];
  const t = useMemo(() => createTranslator(language), [language]);
  const accent = ACCENTS.find((a) => a.key === accentKey) ?? ACCENTS[0];
  const isDark = theme === "dark";
  const closeOverlay = useCallback(() => setOverlay(null), []);
  const stop = useCallback(
    (event: { stopPropagation: () => void }) => event.stopPropagation(),
    [],
  );
  const resolveConfirmDialog = useCallback((confirmed: boolean) => {
    confirmResolverRef.current?.(confirmed);
    confirmResolverRef.current = null;
    setConfirmDialog(null);
  }, []);
  const requestConfirm = useCallback((options: AppConfirmOptions) => {
    confirmResolverRef.current?.(false);
    return new Promise<boolean>((resolve) => {
      confirmResolverRef.current = resolve;
      setConfirmDialog({
        ...options,
        cancelLabel: options.cancelLabel ?? t("common.cancel"),
        variant: options.variant ?? "default",
      });
    });
  }, [t]);
  useEffect(
    () => () => {
      confirmResolverRef.current?.(false);
      confirmResolverRef.current = null;
    },
    [],
  );
  useEffect(
    () => () => {
      void updateResourceRef.current?.close().catch(() => undefined);
      updateResourceRef.current = null;
    },
    [],
  );
  const setAutoUpdateCheck = useCallback((autoCheck: boolean) => {
    setUpdatesConfig((current) => ({ ...current, autoCheck }));
  }, []);
  const checkForUpdates = useCallback(
    async (options: { background?: boolean } = {}) => {
      const background = options.background ?? false;
      if (!isTauriDesktopRuntime()) {
        if (!background) {
          setUpdateState({
            status: "unsupported",
            lastCheckedAt: new Date().toISOString(),
          });
        }
        return null;
      }

      try {
        if (!background) {
          setUpdateState((current) => ({
            ...current,
            status: "checking",
            message: null,
          }));
        }
        const updater = await import("@tauri-apps/plugin-updater");
        await updateResourceRef.current?.close().catch(() => undefined);
        updateResourceRef.current = null;
        const update = await updater.check({ timeout: 15_000 });
        const checkedAt = new Date().toISOString();
        if (!update) {
          setUpdateState({
            status: "not_available",
            lastCheckedAt: checkedAt,
          });
          return null;
        }
        updateResourceRef.current = update;
        setUpdateState({
          status: "available",
          currentVersion: update.currentVersion,
          version: update.version,
          date: update.date ?? null,
          body: update.body ?? null,
          lastCheckedAt: checkedAt,
        });
        return update;
      } catch (cause) {
        if (!background) {
          setUpdateState({
            status: "error",
            message: updateErrorMessage(cause),
            lastCheckedAt: new Date().toISOString(),
          });
        }
        return null;
      }
    },
    [],
  );
  const installAvailableUpdate = useCallback(async () => {
    try {
      let update = updateResourceRef.current;
      if (!update) {
        update = await checkForUpdates();
      }
      if (!update) {
        return;
      }

      let downloadedBytes = 0;
      let contentLength: number | null = null;
      setUpdateState((current) => ({
        ...current,
        status: "downloading",
        message: null,
        downloadedBytes,
        contentLength,
      }));
      await update.downloadAndInstall((event: DownloadEvent) => {
        if (event.event === "Started") {
          downloadedBytes = 0;
          contentLength = event.data.contentLength ?? null;
        } else if (event.event === "Progress") {
          downloadedBytes += event.data.chunkLength;
        }
        setUpdateState((current) => ({
          ...current,
          status: "downloading",
          downloadedBytes,
          contentLength,
        }));
      });
      await update.close().catch(() => undefined);
      updateResourceRef.current = null;
      setUpdateState((current) => ({
        ...current,
        status: "installed",
      }));
      const process = await import("@tauri-apps/plugin-process");
      await process.relaunch();
    } catch (cause) {
      setUpdateState((current) => ({
        ...current,
        status: "error",
        message: updateErrorMessage(cause),
      }));
    }
  }, [checkForUpdates]);
  useEffect(() => {
    if (
      !configLoaded ||
      !updatesConfig.autoCheck ||
      autoUpdateCheckStartedRef.current
    ) {
      return;
    }
    autoUpdateCheckStartedRef.current = true;
    void checkForUpdates({ background: true });
  }, [checkForUpdates, configLoaded, updatesConfig.autoCheck]);
  const reloadConfig = useCallback(async () => {
    try {
      const config = await client.reloadConfig(activeWorkspaceId);
      applyConfig(config);
      await refreshConfigDiagnostics();
      setConfigReloadMessage("Config reloaded.");
    } catch (cause) {
      setConfigReloadMessage(
        cause instanceof Error ? cause.message : "Config reload failed.",
      );
    }
  }, [activeWorkspaceId, applyConfig, client, refreshConfigDiagnostics]);
  const exportConfig = useCallback(
    async (scope: AppConfigScope = "global") => {
      try {
        const result = await client.exportConfig({
          workspaceId: activeWorkspaceId,
          scope,
        });
        if (navigator.clipboard?.writeText) {
          await navigator.clipboard.writeText(result.json);
          setConfigReloadMessage(
            scope === "project"
              ? "Project config JSON copied."
              : "Config JSON copied.",
          );
        } else {
          window.prompt("Config JSON", result.json);
          setConfigReloadMessage(
            scope === "project"
              ? "Project config JSON exported."
              : "Config JSON exported.",
          );
        }
      } catch (cause) {
        setConfigReloadMessage(
          cause instanceof Error ? cause.message : "Config export failed.",
        );
      }
    },
    [activeWorkspaceId, client],
  );
  const importConfig = useCallback(
    async (scope: AppConfigScope = "global") => {
      const json = window.prompt("Paste config JSON");
      if (!json) {
        return;
      }
      try {
        const config = await client.importConfig(json, {
          workspaceId: activeWorkspaceId,
          scope,
        });
        applyConfig(config);
        await refreshConfigDiagnostics();
        setConfigReloadMessage(
          scope === "project" ? "Project config imported." : "Config imported.",
        );
      } catch (cause) {
        setConfigReloadMessage(
          cause instanceof Error ? cause.message : "Config import failed.",
        );
      }
    },
    [activeWorkspaceId, applyConfig, client, refreshConfigDiagnostics],
  );
  const resetConfig = useCallback(
    async (scope: AppConfigScope = "global") => {
      if (
        !window.confirm(
          scope === "project"
            ? t("config.resetProjectConfirm")
            : t("config.resetGlobalConfirm"),
        )
      ) {
        return;
      }
      try {
        const config = await client.resetConfig({
          workspaceId: activeWorkspaceId,
          scope,
        });
        applyConfig(config);
        await refreshConfigDiagnostics();
        setConfigReloadMessage(
          scope === "project" ? "Project config reset." : "Config reset.",
        );
      } catch (cause) {
        setConfigReloadMessage(
          cause instanceof Error ? cause.message : "Config reset failed.",
        );
      }
    },
    [activeWorkspaceId, applyConfig, client, refreshConfigDiagnostics, t],
  );
  const migrateProjectConfig = useCallback(async () => {
    try {
      const result = await client.migrateProjectConfig({
        workspaceId: activeWorkspaceId,
        overwrite: false,
      });
      applyConfig(result.config);
      await refreshConfigDiagnostics();
      setConfigReloadMessage(
        result.overwritten
          ? ".cmux config replaced project config."
          : ".cmux config migrated.",
      );
    } catch (cause) {
      setConfigReloadMessage(
        cause instanceof Error ? cause.message : ".cmux migration failed.",
      );
    }
  }, [activeWorkspaceId, applyConfig, client, refreshConfigDiagnostics]);
  const updateShortcutBinding = useCallback(
    async (actionId: string, binding: ShortcutBindingValue) => {
      try {
        const config = await client.updateConfig(
          {
            shortcuts: {
              bindings: {
                [actionId]: binding,
              },
            },
          },
          activeWorkspaceId,
        );
        applyConfig(config);
        setShortcutEditMessage(
          binding === null ? "Shortcut cleared." : "Shortcut saved.",
        );
      } catch (cause) {
        setShortcutEditMessage(
          cause instanceof Error ? cause.message : "Shortcut save failed.",
        );
      }
    },
    [activeWorkspaceId, applyConfig, client],
  );
  const startWorkspaceRename = useCallback((workspace: WorkspaceSummary) => {
    setEditingWorkspaceId(workspace.workspaceId);
    setWorkspaceNameDraft(workspace.name);
  }, []);
  const commitWorkspaceRename = () => {
    if (!editingWorkspaceId) {
      return;
    }
    const workspace = workspaces.find(
      (candidate) => candidate.workspaceId === editingWorkspaceId,
    );
    const nextName = workspaceNameDraft.trim();
    setEditingWorkspaceId(null);
    setWorkspaceNameDraft("");
    if (workspace && nextName && nextName !== workspace.name) {
      void ctl.renameWorkspace(workspace.workspaceId, nextName);
    }
  };
  const cancelWorkspaceRename = () => {
    setEditingWorkspaceId(null);
    setWorkspaceNameDraft("");
  };
  const createWorkspace = useCallback(async () => {
    const created = await ctl.createWorkspace();
    startWorkspaceRename(created);
    closeOverlay();
  }, [closeOverlay, ctl, startWorkspaceRename]);

  const panes = useMemo(() => detail?.panes ?? [], [detail]);
  const surfaces = useMemo(() => detail?.surfaces ?? [], [detail]);
  const sessions = useMemo(() => detail?.sessions ?? [], [detail]);
  const activePaneId = detail?.workspace.activePaneId ?? null;

  useEffect(() => {
    // Footer git reflects the active pane's session cwd (computed backend-side
    // from the active pane). Refetch it whenever focus moves between panes so
    // the branch/hash tracks the selected pane, not just the periodic poll.
    // PR-4: debounce so rapid focus changes coalesce into a single
    // getSidebarState call instead of one per intermediate pane.
    const timer = window.setTimeout(() => {
      void ctl.refreshSidebar();
    }, 150);
    return () => window.clearTimeout(timer);
  }, [activePaneId, ctl]);

  const paneById = useMemo(
    () => new Map(panes.map((pane) => [pane.paneId, pane])),
    [panes],
  );
  const surfaceById = useMemo(
    () => new Map(surfaces.map((surface) => [surface.surfaceId, surface])),
    [surfaces],
  );
  const sessionById = useMemo(
    () => new Map(sessions.map((session) => [session.sessionId, session])),
    [sessions],
  );
  const childrenByParent = useMemo(() => {
    const map = new Map<string, PaneSummary[]>();
    for (const pane of panes) {
      if (!pane.parentPaneId) continue;
      const parent = paneById.get(pane.parentPaneId);
      if (!parent || parent.kind !== "split" || parent.paneId === pane.paneId) {
        continue;
      }
      const list = map.get(pane.parentPaneId) ?? [];
      if (list.length < 2) {
        list.push(pane);
        map.set(pane.parentPaneId, list);
      }
    }
    return map;
  }, [paneById, panes]);
  const rootPaneId =
    (() => {
      const candidate = detail?.workspace.rootPaneId
        ? paneById.get(detail.workspace.rootPaneId)
        : undefined;
      if (candidate && !candidate.parentPaneId) {
        return candidate.paneId;
      }
      return null;
    })() ??
    panes.find((pane) => !pane.parentPaneId)?.paneId ??
    null;

  const activeRootIsSplit = rootPaneId
    ? paneById.get(rootPaneId)?.kind === "split"
    : false;
  const orderedLeafPaneIds = useMemo(() => {
    if (!rootPaneId) {
      return panes
        .filter((pane) => pane.kind === "leaf")
        .map((pane) => pane.paneId);
    }
    const ordered: string[] = [];
    const visit = (paneId: string, visited = new Set<string>()) => {
      if (visited.has(paneId)) return;
      visited.add(paneId);
      const pane = paneById.get(paneId);
      if (!pane) return;
      if (pane.kind !== "split") {
        ordered.push(pane.paneId);
        return;
      }
      for (const child of childrenByParent.get(pane.paneId) ?? []) {
        visit(child.paneId, visited);
      }
    };
    visit(rootPaneId);
    return ordered;
  }, [childrenByParent, paneById, panes, rootPaneId]);

  // Balance: reset every split ratio in the active tab's subtree back to 0.5 so
  // panes return to even sizes after manual drag-resizing.
  const balanceActivePanes = useCallback(() => {
    if (!rootPaneId) return;
    const stack = [rootPaneId];
    const seen = new Set<string>();
    while (stack.length) {
      const id = stack.pop();
      if (!id || seen.has(id)) continue;
      seen.add(id);
      const pane = paneById.get(id);
      if (!pane || pane.kind !== "split") continue;
      if ((pane.splitRatio ?? 0.5) !== 0.5) {
        void ctl.resizePane(pane.paneId, 0.5);
      }
      for (const child of childrenByParent.get(pane.paneId) ?? []) {
        stack.push(child.paneId);
      }
    }
  }, [rootPaneId, paneById, childrenByParent, ctl]);

  const surfaceForPane = (pane: PaneSummary): SurfaceSummary | undefined =>
    pane.mountedSurfaceId ? surfaceById.get(pane.mountedSurfaceId) : undefined;

  const terminalSurfaces = surfaces.filter(
    (surface) => surface.surfaceType === "terminal",
  );
  const dockTerminalSurfaces = surfaces.filter(
    (surface) => surface.surfaceType === "dock-terminal",
  );
  const dockSurfaceByControlId = useMemo(
    () =>
      new Map(
        dockTerminalSurfaces
          .map((surface) => [surface.browserId ?? "", surface] as const)
          .filter(([controlId]) => controlId.length > 0),
      ),
    [dockTerminalSurfaces],
  );
  const paneHostingSurface = (surfaceId: string): PaneSummary | undefined =>
    panes.find((pane) => pane.mountedSurfaceId === surfaceId);

  useEffect(() => {
    if (!activeWorkspaceId) {
      return;
    }

    const paneByMountedSurfaceId = new Map(
      panes
        .filter((pane) => pane.mountedSurfaceId)
        .map((pane) => [pane.mountedSurfaceId ?? "", pane] as const),
    );

    for (const surface of surfaces) {
      if (surface.surfaceType !== "terminal" || !surface.sessionId) {
        continue;
      }
      const sessionId = surface.sessionId;
      const session = sessionById.get(sessionId);
      const exitIntent = exitIntentSessionIdsRef.current.has(sessionId);
      const shouldClose =
        session?.state === "exited" ||
        (exitIntent && (!session || isClosedTerminalState(session.state)));
      if (!shouldClose) {
        continue;
      }
      const pane = paneByMountedSurfaceId.get(surface.surfaceId);
      if (!pane) {
        continue;
      }
      const key = `${activeWorkspaceId}:${sessionId}`;
      if (autoClosingExitedSessionsRef.current.has(key)) {
        continue;
      }
      autoClosingExitedSessionsRef.current.add(key);
      void (async () => {
        try {
          if (pane.parentPaneId) {
            await ctl.closePane(pane.paneId);
          } else {
            await ctl.closeSurface(surface.surfaceId);
          }
          exitIntentSessionIdsRef.current.delete(sessionId);
        } catch (error) {
          console.warn("[agentmux] failed to auto-close exited terminal pane", {
            error,
            paneId: pane.paneId,
            sessionId,
            surfaceId: surface.surfaceId,
          });
          window.setTimeout(() => {
            autoClosingExitedSessionsRef.current.delete(key);
          }, 1500);
        }
      })();
    }
  }, [
    activeWorkspaceId,
    ctl.closePane,
    ctl.closeSurface,
    panes,
    sessionById,
    surfaces,
  ]);
  const attentionPaneQueue = useMemo<AttentionPaneTarget[]>(() => {
    const surfaceBySession = new Map(
      surfaces
        .filter((surface) => surface.sessionId)
        .map((surface) => [surface.sessionId ?? "", surface] as const),
    );
    return attention
      .map((state) => {
        const surface = surfaceBySession.get(state.sessionId);
        const pane = surface ? paneHostingSurface(surface.surfaceId) : undefined;
        return surface && pane ? { state, pane, surface } : null;
      })
      .filter((target): target is AttentionPaneTarget => Boolean(target))
      .sort((left, right) => {
        const leftTime = Date.parse(left.state.updatedAt ?? "") || 0;
        const rightTime = Date.parse(right.state.updatedAt ?? "") || 0;
        return leftTime - rightTime;
      });
  }, [attention, panes, surfaces]);
  const rootPaneForPane = useCallback(
    (pane: PaneSummary): PaneSummary => {
      let current = pane;
      const visited = new Set<string>();
      while (current.parentPaneId && !visited.has(current.paneId)) {
        visited.add(current.paneId);
        const parent = paneById.get(current.parentPaneId);
        if (!parent) break;
        current = parent;
      }
      return current;
    },
    [paneById],
  );
  const tabSurfaces = useMemo(() => {
    const firstSurfaceByRoot = new Map<string, string>();
    const hostedSurfaceIds = new Set<string>();
    for (const pane of panes) {
      if (!pane.mountedSurfaceId) continue;
      hostedSurfaceIds.add(pane.mountedSurfaceId);
      const root = rootPaneForPane(pane);
      if (!firstSurfaceByRoot.has(root.paneId)) {
        firstSurfaceByRoot.set(root.paneId, pane.mountedSurfaceId);
      }
    }
    const tabSurfaceIds = new Set(firstSurfaceByRoot.values());
    const visible = surfaces.filter((surface) => {
      if (surface.surfaceType === "dock-terminal") return false;
      // The representative surface of each root pane (tab) always shows.
      if (tabSurfaceIds.has(surface.surfaceId)) return true;
      // A non-first surface inside a split tab is part of that tab's tree, not a
      // tab of its own.
      if (hostedSurfaceIds.has(surface.surfaceId)) return false;
      // A tab IS a root pane (represented by its first surface, handled above).
      // An UNMOUNTED terminal surface is therefore NOT a tab — it's a detached or
      // leaked surface, and showing it as a phantom tab lets a click mount it into
      // the active split pane, which makes the other split panes look "shared"
      // across tabs. Only top-level browser surfaces may legitimately appear
      // unmounted.
      return surface.surfaceType === "browser";
    });
    const activeOrder = activeWorkspaceId
      ? (surfaceTabOrderByWorkspace[activeWorkspaceId] ?? [])
      : [];
    return applyStoredOrder(visible, (surface) => surface.surfaceId, activeOrder);
  }, [
    activeWorkspaceId,
    panes,
    rootPaneForPane,
    surfaces,
    surfaceTabOrderByWorkspace,
  ]);

  const activeWorkspace = workspaces.find(
    (ws) => ws.workspaceId === activeWorkspaceId,
  );
  const gitStatusLabel = useMemo(() => {
    const branch = sidebarState?.gitBranch?.trim();
    const hash = sidebarState?.gitHash?.trim();
    if (branch && hash) {
      return `${branch} @ ${hash}`;
    }
    return branch || hash || "no git";
  }, [sidebarState?.gitBranch, sidebarState?.gitHash]);
  const teamTaskStats = useMemo(() => {
    const total = teamTasks.length;
    const completed = teamTasks.filter((task) => task.status === "completed").length;
    const blocked = teamTasks.filter((task) => task.status === "blocked").length;
    const claimed = teamTasks.filter((task) => task.status === "claimed").length;
    return { total, completed, blocked, claimed };
  }, [teamTasks]);
  const unreadTeamMessageCount = useMemo(
    () => teamMessages.filter((message) => !message.readAt).length,
    [teamMessages],
  );
  const selectedWorkspaces = useMemo(
    () =>
      workspaces.filter((workspace) =>
        selectedWorkspaceIds.has(workspace.workspaceId),
      ),
    [selectedWorkspaceIds, workspaces],
  );
  const selectedWorkspaceCount = selectedWorkspaces.length;
  useEffect(() => {
    const existingWorkspaceIds = new Set(
      workspaces.map((workspace) => workspace.workspaceId),
    );
    setSelectedWorkspaceIds((previous) => {
      const next = new Set(
        [...previous].filter((workspaceId) =>
          existingWorkspaceIds.has(workspaceId),
        ),
      );
      return next.size === previous.size ? previous : next;
    });
  }, [workspaces]);
  useEffect(() => {
    const ids = new Set(workspaces.map((workspace) => workspace.workspaceId));
    setWorkspaceOrder((previous) => {
      const next = [
        ...previous.filter((workspaceId) => ids.has(workspaceId)),
        ...workspaces
          .map((workspace) => workspace.workspaceId)
          .filter((workspaceId) => !previous.includes(workspaceId)),
      ];
      if (next.join("\0") !== previous.join("\0")) {
        writeStoredOrder(WORKSPACE_ORDER_STORAGE_KEY, next);
        return next;
      }
      return previous;
    });
  }, [workspaces]);
  useEffect(() => {
    if (!activeWorkspaceId) {
      return;
    }
    const key = `${SURFACE_TAB_ORDER_STORAGE_PREFIX}${activeWorkspaceId}`;
    setSurfaceTabOrderByWorkspace((previous) => ({
      ...previous,
      [activeWorkspaceId]: readStoredOrder(key),
    }));
  }, [activeWorkspaceId]);
  const workspaceById = useMemo(
    () =>
      new Map(
        workspaces.map((workspace) => [workspace.workspaceId, workspace]),
      ),
    [workspaces],
  );
  const workspaceGroupsView = useMemo(
    () =>
      [...workspaceGroups]
        .sort((left, right) => {
          if (left.pinned !== right.pinned) {
            return left.pinned ? -1 : 1;
          }
          return (
            left.sortOrder - right.sortOrder ||
            left.name.localeCompare(right.name)
          );
        })
        .map((group) => ({
          group,
          workspaces: [...group.members]
            .sort((left, right) => left.position - right.position)
            .map((member) => workspaceById.get(member.workspaceId))
            .filter((workspace): workspace is WorkspaceSummary =>
              Boolean(workspace),
            ),
        })),
    [workspaceById, workspaceGroups],
  );
  const workspaceGroupMenuGroup = useMemo(
    () =>
      workspaceGroupMenu
        ? (workspaceGroups.find(
            (group) => group.groupId === workspaceGroupMenu.groupId,
          ) ?? null)
        : null,
    [workspaceGroupMenu, workspaceGroups],
  );
  const workspaceMenuWorkspace = useMemo(
    () =>
      workspaceMenu
        ? (workspaces.find(
            (workspace) => workspace.workspaceId === workspaceMenu.workspaceId,
          ) ?? null)
        : null,
    [workspaceMenu, workspaces],
  );
  const surfaceTabMenuSurface = useMemo(
    () =>
      surfaceTabMenu
        ? (surfaces.find(
            (surface) => surface.surfaceId === surfaceTabMenu.surfaceId,
          ) ?? null)
        : null,
    [surfaceTabMenu, surfaces],
  );
  const workspaceAnchorGroups = useMemo(
    () =>
      workspaceMenu
        ? workspaceGroups.filter(
            (group) => group.anchorWorkspaceId === workspaceMenu.workspaceId,
          )
        : [],
    [workspaceGroups, workspaceMenu],
  );
  const groupedWorkspaceIds = useMemo(() => {
    const ids = new Set<string>();
    for (const group of workspaceGroups) {
      for (const member of group.members) {
        ids.add(member.workspaceId);
      }
    }
    return ids;
  }, [workspaceGroups]);
  const ungroupedWorkspaces = useMemo(
    () =>
      applyStoredOrder(
        workspaces.filter(
          (workspace) => !groupedWorkspaceIds.has(workspace.workspaceId),
        ),
        (workspace) => workspace.workspaceId,
        workspaceOrder,
      ),
    [groupedWorkspaceIds, workspaceOrder, workspaces],
  );
  const normalizedWorkspaceFilter = workspaceFilterText.trim().toLowerCase();
  const workspaceFilterActive = normalizedWorkspaceFilter.length > 0;
  const visibleWorkspaceGroupsView = useMemo(() => {
    if (!workspaceFilterActive) {
      return workspaceGroupsView;
    }
    return workspaceGroupsView.flatMap((entry) => {
      const groupMatches = searchableWorkspaceGroupText(entry.group).includes(
        normalizedWorkspaceFilter,
      );
      const visibleWorkspaces = groupMatches
        ? entry.workspaces
        : entry.workspaces.filter((workspace) =>
            searchableWorkspaceText(workspace).includes(
              normalizedWorkspaceFilter,
            ),
          );
      return groupMatches || visibleWorkspaces.length > 0
        ? [{ group: entry.group, workspaces: visibleWorkspaces }]
        : [];
    });
  }, [normalizedWorkspaceFilter, workspaceFilterActive, workspaceGroupsView]);
  const visibleUngroupedWorkspaces = useMemo(
    () =>
      workspaceFilterActive
        ? ungroupedWorkspaces.filter((workspace) =>
            searchableWorkspaceText(workspace).includes(
              normalizedWorkspaceFilter,
            ),
          )
        : ungroupedWorkspaces,
    [normalizedWorkspaceFilter, ungroupedWorkspaces, workspaceFilterActive],
  );
  const visibleWorkspaceCount =
    visibleUngroupedWorkspaces.length +
    visibleWorkspaceGroupsView.reduce(
      (count, entry) => count + entry.workspaces.length,
      0,
    );
  const toggleWorkspaceSelection = useCallback((workspaceId: string) => {
    setSelectedWorkspaceIds((previous) => {
      const next = new Set(previous);
      if (next.has(workspaceId)) {
        next.delete(workspaceId);
      } else {
        next.add(workspaceId);
      }
      return next;
    });
  }, []);
  const clearWorkspaceSelection = useCallback(() => {
    setSelectedWorkspaceIds(new Set());
  }, []);
  const closeWorkspaceGroupMenu = useCallback(() => {
    setWorkspaceGroupMenu(null);
  }, []);
  const closeWorkspaceMenu = useCallback(() => {
    setWorkspaceMenu(null);
  }, []);
  const closeSurfaceTabMenu = useCallback(() => {
    setSurfaceTabMenu(null);
  }, []);
  const closeTerminalProfileMenu = useCallback(() => {
    setTerminalProfileMenu(null);
  }, []);
  const openWorkspaceGroupMenu = useCallback(
    (event: ReactMouseEvent<HTMLElement>, group: WorkspaceGroup) => {
      event.preventDefault();
      event.stopPropagation();
      const width = 230;
      const height = 292;
      setWorkspaceMenu(null);
      setSurfaceTabMenu(null);
      setWorkspaceGroupMenu({
        groupId: group.groupId,
        x: Math.min(event.clientX, Math.max(8, window.innerWidth - width - 8)),
        y: Math.min(
          event.clientY,
          Math.max(8, window.innerHeight - height - 8),
        ),
      });
    },
    [],
  );
  const openWorkspaceMenu = useCallback(
    (event: ReactMouseEvent<HTMLElement>, workspace: WorkspaceSummary) => {
      event.preventDefault();
      event.stopPropagation();
      const width = 224;
      const height = 170;
      setWorkspaceGroupMenu(null);
      setSurfaceTabMenu(null);
      setWorkspaceMenu({
        workspaceId: workspace.workspaceId,
        x: Math.min(event.clientX, Math.max(8, window.innerWidth - width - 8)),
        y: Math.min(
          event.clientY,
          Math.max(8, window.innerHeight - height - 8),
        ),
      });
    },
    [],
  );
  const openSurfaceTabMenu = useCallback(
    (event: ReactMouseEvent<HTMLElement>, surface: SurfaceSummary) => {
      event.preventDefault();
      event.stopPropagation();
      const rect = event.currentTarget.getBoundingClientRect();
      const width = 250;
      const height = Math.min(360, 92 + Math.max(1, workspaces.length) * 30);
      setWorkspaceGroupMenu(null);
      setWorkspaceMenu(null);
      setSurfaceTabMenu({
        surfaceId: surface.surfaceId,
        x: Math.min(rect.left, Math.max(8, window.innerWidth - width - 8)),
        y: Math.min(
          rect.bottom + 4,
          Math.max(8, window.innerHeight - height - 8),
        ),
      });
    },
    [workspaces.length],
  );
  const openTerminalProfileMenu = useCallback(
    (event: ReactMouseEvent<HTMLElement>, paneId?: string | null) => {
      event.preventDefault();
      event.stopPropagation();
      const rect = event.currentTarget.getBoundingClientRect();
      const width = 300;
      const height = 270;
      setWorkspaceGroupMenu(null);
      setWorkspaceMenu(null);
      setSurfaceTabMenu(null);
      setTerminalProfileMenu({
        x: Math.min(rect.left, Math.max(8, window.innerWidth - width - 8)),
        y: Math.min(
          rect.bottom + 4,
          Math.max(8, window.innerHeight - height - 8),
        ),
        paneId: paneId ?? null,
      });
    },
    [],
  );
  useEffect(() => {
    if (
      !workspaceGroupMenu &&
      !workspaceMenu &&
      !surfaceTabMenu &&
      !terminalProfileMenu
    ) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setWorkspaceGroupMenu(null);
        setWorkspaceMenu(null);
        setSurfaceTabMenu(null);
        setTerminalProfileMenu(null);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [surfaceTabMenu, terminalProfileMenu, workspaceGroupMenu, workspaceMenu]);
  const toggleWorkspaceGroup = useCallback(
    (group: WorkspaceGroup) => {
      void ctl.updateWorkspaceGroup(group.groupId, {
        collapsed: !group.collapsed,
      });
    },
    [ctl],
  );
  const createWorkspaceGroup = useCallback(async () => {
    const defaultName = nextWorkspaceGroupName(workspaceGroups);
    const rawName = window.prompt("Workspace group name", defaultName);
    if (rawName === null) {
      return;
    }
    const name = rawName.trim() || defaultName;
    const selectedIds = selectedWorkspaces.map(
      (workspace) => workspace.workspaceId,
    );
    const anchorWorkspaceId = selectedIds[0] ?? activeWorkspaceId ?? null;
    const workspaceIds =
      selectedIds.length > 0
        ? selectedIds
        : anchorWorkspaceId
          ? [anchorWorkspaceId]
          : [];
    await ctl.createWorkspaceGroup({
      name,
      anchorWorkspaceId,
      workspaceIds,
      color: activeWorkspace?.color ?? ACCENTS[0].hex,
      icon: normalizeGroupIcon(activeWorkspace?.icon ?? name),
      collapsed: false,
      pinned: false,
    });
    clearWorkspaceSelection();
  }, [
    activeWorkspace,
    activeWorkspaceId,
    clearWorkspaceSelection,
    ctl,
    selectedWorkspaces,
    workspaceGroups,
  ]);
  const editWorkspaceGroup = useCallback(
    async (group: WorkspaceGroup) => {
      const rawName = window.prompt("Workspace group name", group.name);
      if (rawName === null) {
        return;
      }
      const rawIcon = window.prompt(
        "Group icon (1-2 letters)",
        group.icon ?? "",
      );
      if (rawIcon === null) {
        return;
      }
      const rawColor = window.prompt(
        "Group color (#RRGGBB)",
        group.color ?? "",
      );
      if (rawColor === null) {
        return;
      }
      await ctl.updateWorkspaceGroup(group.groupId, {
        name: rawName.trim() || group.name,
        icon: normalizeGroupIcon(rawIcon),
        color: normalizeGroupColor(rawColor),
      });
    },
    [ctl],
  );
  const toggleWorkspaceGroupPin = useCallback(
    (group: WorkspaceGroup) => {
      void ctl.updateWorkspaceGroup(group.groupId, { pinned: !group.pinned });
    },
    [ctl],
  );
  const deleteWorkspaceGroup = useCallback(
    (group: WorkspaceGroup) => {
      if (
        window.confirm(
          `Delete workspace group "${group.name}"? Workspaces will remain.`,
        )
      ) {
        void ctl.deleteWorkspaceGroup(group.groupId);
      }
    },
    [ctl],
  );
  const closeWorkspaceFromMenu = useCallback(
    async (workspace: WorkspaceSummary) => {
      const anchorGroups = workspaceGroups.filter(
        (group) => group.anchorWorkspaceId === workspace.workspaceId,
      );
      const anchorWarning =
        anchorGroups.length > 0
          ? `This workspace anchors ${anchorGroups.length} group(s): ${anchorGroups.map((group) => group.name).join(", ")}. Closing it will remove the workspace from its group and clear those group anchors.`
          : "";

      try {
        const workspaceDetail = await client.getWorkspace(workspace.workspaceId);
        const sessionCount = workspaceSessionSurfaceCount(workspaceDetail);
        const hasOpenSessions = sessionCount > 0;
        const confirmed = await requestConfirm({
          title: hasOpenSessions
            ? "Close workspace with running terminals?"
            : "Close workspace?",
          message: hasOpenSessions
            ? `"${workspace.name}" has ${sessionCount} open terminal session(s). Closing it will terminate those sessions.`
            : `Close "${workspace.name}"?`,
          detail: anchorWarning || undefined,
          confirmLabel: hasOpenSessions
            ? "Terminate and close"
            : "Close workspace",
          variant: hasOpenSessions ? "danger" : "default",
        });
        if (!confirmed) {
          return;
        }
        await ctl.closeWorkspace(
          workspace.workspaceId,
          hasOpenSessions ? "terminate_sessions" : "fail_if_running",
        );
        clearWorkspaceSelection();
      } catch (cause) {
        if (isWorkspaceRunningCloseError(cause)) {
          const confirmed = await requestConfirm({
            title: "Close workspace with running terminals?",
            message: `"${workspace.name}" has running terminal session(s). Closing it will terminate those sessions.`,
            detail: anchorWarning || undefined,
            confirmLabel: "Terminate and close",
            variant: "danger",
          });
          if (!confirmed) {
            return;
          }
          await ctl.closeWorkspace(workspace.workspaceId, "terminate_sessions");
          clearWorkspaceSelection();
          return;
        }
        window.alert(
          cause instanceof Error ? cause.message : "Workspace close failed.",
        );
      }
    },
    [clearWorkspaceSelection, client, ctl, requestConfirm, workspaceGroups],
  );
  const createWorkspaceInGroup = useCallback(
    async (group: WorkspaceGroup) => {
      if (group.collapsed) {
        await ctl.updateWorkspaceGroup(group.groupId, { collapsed: false });
      }
      const created = await ctl.createWorkspaceInGroup(group.groupId);
      if (created) {
        startWorkspaceRename(created);
      }
    },
    [ctl, startWorkspaceRename],
  );
  const addSelectedWorkspacesToGroup = useCallback(
    (group: WorkspaceGroup) => {
      const selectedIds = selectedWorkspaces.map(
        (workspace) => workspace.workspaceId,
      );
      const workspaceIds =
        selectedIds.length > 0
          ? selectedIds
          : activeWorkspaceId
            ? [activeWorkspaceId]
            : [];
      const existingIds = new Set(
        group.members.map((member) => member.workspaceId),
      );
      const targetIds = workspaceIds.filter(
        (workspaceId) => !existingIds.has(workspaceId),
      );
      if (targetIds.length === 0) {
        return;
      }
      void Promise.all(
        targetIds.map((workspaceId) =>
          ctl.addWorkspaceToGroup(group.groupId, workspaceId),
        ),
      ).then(() => clearWorkspaceSelection());
    },
    [activeWorkspaceId, clearWorkspaceSelection, ctl, selectedWorkspaces],
  );
  const reorderWorkspaceGroup = useCallback(
    async (
      sourceGroupId: string,
      targetGroupId: string,
      placement: DropPlacement,
    ) => {
      if (sourceGroupId === targetGroupId) {
        return;
      }
      const source = workspaceGroups.find(
        (group) => group.groupId === sourceGroupId,
      );
      const target = workspaceGroups.find(
        (group) => group.groupId === targetGroupId,
      );
      if (!source || !target || source.pinned !== target.pinned) {
        return;
      }
      const peers = workspaceGroupsView
        .map((item) => item.group)
        .filter(
          (candidate) =>
            candidate.pinned === target.pinned &&
            candidate.groupId !== sourceGroupId,
        );
      const targetIndex = peers.findIndex(
        (candidate) => candidate.groupId === targetGroupId,
      );
      if (targetIndex < 0) {
        return;
      }
      const insertIndex = targetIndex + (placement === "after" ? 1 : 0);
      const ordered = [...peers];
      ordered.splice(insertIndex, 0, source);
      for (const [sortOrder, candidate] of ordered.entries()) {
        await ctl.updateWorkspaceGroup(candidate.groupId, { sortOrder });
      }
    },
    [ctl, workspaceGroups, workspaceGroupsView],
  );
  const moveWorkspaceGroup = useCallback(
    async (group: WorkspaceGroup, direction: -1 | 1) => {
      const peers = workspaceGroupsView
        .map((item) => item.group)
        .filter((candidate) => candidate.pinned === group.pinned);
      const index = peers.findIndex(
        (candidate) => candidate.groupId === group.groupId,
      );
      const nextIndex = index + direction;
      if (index < 0 || nextIndex < 0 || nextIndex >= peers.length) {
        return;
      }
      const ordered = [...peers];
      const [moved] = ordered.splice(index, 1);
      ordered.splice(nextIndex, 0, moved);
      for (const [sortOrder, candidate] of ordered.entries()) {
        await ctl.updateWorkspaceGroup(candidate.groupId, { sortOrder });
      }
    },
    [ctl, workspaceGroupsView],
  );
  const reorderWorkspaceInGroup = useCallback(
    async (
      group: WorkspaceGroup,
      sourceWorkspaceId: string,
      targetWorkspaceId: string,
      placement: DropPlacement,
    ) => {
      if (sourceWorkspaceId === targetWorkspaceId) {
        return;
      }
      const source = group.members.find(
        (member) => member.workspaceId === sourceWorkspaceId,
      );
      if (!source) {
        return;
      }
      const peers = [...group.members]
        .sort((left, right) => left.position - right.position)
        .filter((member) => member.workspaceId !== sourceWorkspaceId);
      const targetIndex = peers.findIndex(
        (member) => member.workspaceId === targetWorkspaceId,
      );
      if (targetIndex < 0) {
        return;
      }
      const insertIndex = targetIndex + (placement === "after" ? 1 : 0);
      const ordered = [...peers];
      ordered.splice(insertIndex, 0, source);
      for (const [position, member] of ordered.entries()) {
        await ctl.addWorkspaceToGroup(
          group.groupId,
          member.workspaceId,
          position,
        );
      }
    },
    [ctl],
  );
  const moveWorkspaceInGroup = useCallback(
    async (group: WorkspaceGroup, workspaceId: string, direction: -1 | 1) => {
      const ordered = [...group.members].sort(
        (left, right) => left.position - right.position,
      );
      const index = ordered.findIndex(
        (member) => member.workspaceId === workspaceId,
      );
      const nextIndex = index + direction;
      if (index < 0 || nextIndex < 0 || nextIndex >= ordered.length) {
        return;
      }
      const [moved] = ordered.splice(index, 1);
      ordered.splice(nextIndex, 0, moved);
      for (const [position, member] of ordered.entries()) {
        await ctl.addWorkspaceToGroup(
          group.groupId,
          member.workspaceId,
          position,
        );
      }
    },
    [ctl],
  );
  const beginWorkspaceGroupDrag = useCallback(
    (event: ReactDragEvent<HTMLElement>, group: WorkspaceGroup) => {
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData(
        WORKSPACE_GROUP_DRAG_TYPE,
        JSON.stringify({
          groupId: group.groupId,
        } satisfies WorkspaceGroupDragPayload),
      );
    },
    [],
  );
  const allowWorkspaceGroupDrop = useCallback(
    (event: ReactDragEvent<HTMLElement>) => {
      event.preventDefault();
      event.dataTransfer.dropEffect = "move";
    },
    [],
  );
  const dropWorkspaceGroup = useCallback(
    (event: ReactDragEvent<HTMLElement>, targetGroup: WorkspaceGroup) => {
      const payload = parseDragPayload<WorkspaceGroupDragPayload>(
        event,
        WORKSPACE_GROUP_DRAG_TYPE,
      );
      if (!payload) {
        return;
      }
      event.preventDefault();
      void reorderWorkspaceGroup(
        payload.groupId,
        targetGroup.groupId,
        dropPlacementFromEvent(event),
      );
    },
    [reorderWorkspaceGroup],
  );
  const beginWorkspaceMemberDrag = useCallback(
    (
      event: ReactDragEvent<HTMLElement>,
      group: WorkspaceGroup,
      workspaceId: string,
    ) => {
      event.stopPropagation();
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData(
        WORKSPACE_MEMBER_DRAG_TYPE,
        JSON.stringify({
          groupId: group.groupId,
          workspaceId,
        } satisfies WorkspaceMemberDragPayload),
      );
    },
    [],
  );
  const allowWorkspaceMemberDrop = useCallback(
    (event: ReactDragEvent<HTMLElement>) => {
      event.preventDefault();
      event.stopPropagation();
      event.dataTransfer.dropEffect = "move";
    },
    [],
  );
  const dropWorkspaceMember = useCallback(
    (
      event: ReactDragEvent<HTMLElement>,
      targetGroup: WorkspaceGroup,
      targetWorkspaceId: string,
    ) => {
      const payload = parseDragPayload<WorkspaceMemberDragPayload>(
        event,
        WORKSPACE_MEMBER_DRAG_TYPE,
      );
      if (!payload || payload.groupId !== targetGroup.groupId) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      void reorderWorkspaceInGroup(
        targetGroup,
        payload.workspaceId,
        targetWorkspaceId,
        dropPlacementFromEvent(event),
      );
    },
    [reorderWorkspaceInGroup],
  );
  const persistWorkspaceOrder = useCallback((next: string[]) => {
    setWorkspaceOrder(next);
    writeStoredOrder(WORKSPACE_ORDER_STORAGE_KEY, next);
  }, []);
  const moveUngroupedWorkspace = useCallback(
    (workspaceId: string, direction: -1 | 1) => {
      if (workspaceFilterActive) {
        return;
      }
      const visibleIds = visibleUngroupedWorkspaces.map(
        (workspace) => workspace.workspaceId,
      );
      if (!visibleIds.includes(workspaceId)) {
        return;
      }
      const nextVisible = moveIdByDirection(visibleIds, workspaceId, direction);
      if (nextVisible === visibleIds) {
        return;
      }
      const nextVisibleRank = new Map(
        nextVisible.map((id, index) => [id, index]),
      );
      const current = [
        ...workspaceOrder.filter((id) =>
          workspaces.some((workspace) => workspace.workspaceId === id),
        ),
        ...workspaces
          .map((workspace) => workspace.workspaceId)
          .filter((id) => !workspaceOrder.includes(id)),
      ];
      const next = [...current].sort((left, right) => {
        const leftRank = nextVisibleRank.get(left);
        const rightRank = nextVisibleRank.get(right);
        if (leftRank === undefined || rightRank === undefined) {
          return 0;
        }
        return leftRank - rightRank;
      });
      persistWorkspaceOrder(next);
    },
    [
      persistWorkspaceOrder,
      visibleUngroupedWorkspaces,
      workspaceFilterActive,
      workspaceOrder,
      workspaces,
    ],
  );
  const beginWorkspaceCardDrag = useCallback(
    (event: ReactDragEvent<HTMLElement>, workspaceId: string) => {
      event.stopPropagation();
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData(
        WORKSPACE_CARD_DRAG_TYPE,
        JSON.stringify({ workspaceId } satisfies WorkspaceCardDragPayload),
      );
    },
    [],
  );
  const allowWorkspaceCardDrop = useCallback(
    (event: ReactDragEvent<HTMLElement>) => {
      if (
        event.dataTransfer.types.includes(WORKSPACE_CARD_DRAG_TYPE) ||
        event.dataTransfer.types.includes(SURFACE_TAB_DRAG_TYPE)
      ) {
        event.preventDefault();
        event.stopPropagation();
        event.dataTransfer.dropEffect = "move";
      }
    },
    [],
  );
  const dropWorkspaceCard = useCallback(
    (event: ReactDragEvent<HTMLElement>, targetWorkspaceId: string) => {
      const surfacePayload = parseDragPayload<SurfaceTabDragPayload>(
        event,
        SURFACE_TAB_DRAG_TYPE,
      );
      if (surfacePayload && surfacePayload.workspaceId !== targetWorkspaceId) {
        event.preventDefault();
        event.stopPropagation();
        void client
          .moveSurfaceToWorkspace(
            surfacePayload.workspaceId,
            targetWorkspaceId,
            surfacePayload.surfaceId,
          )
          .then(() => ctl.selectWorkspace(targetWorkspaceId));
        return;
      }

      const workspacePayload = parseDragPayload<WorkspaceCardDragPayload>(
        event,
        WORKSPACE_CARD_DRAG_TYPE,
      );
      if (!workspacePayload || workspaceFilterActive) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      const current = [
        ...workspaceOrder.filter((workspaceId) =>
          workspaces.some((workspace) => workspace.workspaceId === workspaceId),
        ),
        ...workspaces
          .map((workspace) => workspace.workspaceId)
          .filter((workspaceId) => !workspaceOrder.includes(workspaceId)),
      ];
      persistWorkspaceOrder(
        reorderIds(
          current,
          workspacePayload.workspaceId,
          targetWorkspaceId,
          dropPlacementFromEvent(event),
        ),
      );
    },
    [
      client,
      ctl,
      persistWorkspaceOrder,
      workspaceFilterActive,
      workspaceOrder,
      workspaces,
    ],
  );
  const persistSurfaceTabOrder = useCallback(
    (workspaceId: string, next: string[]) => {
      setSurfaceTabOrderByWorkspace((previous) => ({
        ...previous,
        [workspaceId]: next,
      }));
      writeStoredOrder(`${SURFACE_TAB_ORDER_STORAGE_PREFIX}${workspaceId}`, next);
    },
    [],
  );
  const currentSurfaceTabOrder = useCallback(
    (workspaceId: string) => {
      const storedOrder = surfaceTabOrderByWorkspace[workspaceId] ?? [];
      return [
        ...storedOrder.filter((surfaceId) =>
          tabSurfaces.some((surface) => surface.surfaceId === surfaceId),
        ),
        ...tabSurfaces
          .map((surface) => surface.surfaceId)
          .filter((surfaceId) => !storedOrder.includes(surfaceId)),
      ];
    },
    [surfaceTabOrderByWorkspace, tabSurfaces],
  );
  const moveSurfaceTabByDirection = useCallback(
    (surfaceId: string, direction: -1 | 1) => {
      if (!activeWorkspaceId) {
        return;
      }
      const current = currentSurfaceTabOrder(activeWorkspaceId);
      const next = moveIdByDirection(current, surfaceId, direction);
      if (next !== current) {
        persistSurfaceTabOrder(activeWorkspaceId, next);
      }
    },
    [activeWorkspaceId, currentSurfaceTabOrder, persistSurfaceTabOrder],
  );
  const moveSurfaceTabToWorkspace = useCallback(
    async (surfaceId: string, targetWorkspaceId: string) => {
      if (!activeWorkspaceId || activeWorkspaceId === targetWorkspaceId) {
        return;
      }
      await client.moveSurfaceToWorkspace(
        activeWorkspaceId,
        targetWorkspaceId,
        surfaceId,
      );
      closeSurfaceTabMenu();
      await ctl.selectWorkspace(targetWorkspaceId);
    },
    [activeWorkspaceId, client, closeSurfaceTabMenu, ctl],
  );
  const beginSurfaceTabDrag = useCallback(
    (event: ReactDragEvent<HTMLElement>, surface: SurfaceSummary) => {
      if (!activeWorkspaceId) return;
      event.stopPropagation();
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData(
        SURFACE_TAB_DRAG_TYPE,
        JSON.stringify({
          workspaceId: activeWorkspaceId,
          surfaceId: surface.surfaceId,
        } satisfies SurfaceTabDragPayload),
      );
    },
    [activeWorkspaceId],
  );
  const allowSurfaceTabDrop = useCallback(
    (event: ReactDragEvent<HTMLElement>) => {
      if (event.dataTransfer.types.includes(SURFACE_TAB_DRAG_TYPE)) {
        event.preventDefault();
        event.stopPropagation();
        event.dataTransfer.dropEffect = "move";
      }
    },
    [],
  );
  const dropSurfaceTab = useCallback(
    (event: ReactDragEvent<HTMLElement>, targetSurfaceId: string) => {
      if (!activeWorkspaceId) return;
      const payload = parseDragPayload<SurfaceTabDragPayload>(
        event,
        SURFACE_TAB_DRAG_TYPE,
      );
      if (!payload) return;
      event.preventDefault();
      event.stopPropagation();
      if (payload.workspaceId !== activeWorkspaceId) {
        void client
          .moveSurfaceToWorkspace(
            payload.workspaceId,
            activeWorkspaceId,
            payload.surfaceId,
          )
          .then(() => ctl.selectWorkspace(activeWorkspaceId));
        return;
      }
      const current = currentSurfaceTabOrder(activeWorkspaceId);
      persistSurfaceTabOrder(
        activeWorkspaceId,
        reorderIds(
          current,
          payload.surfaceId,
          targetSurfaceId,
          dropPlacementFromEvent(event),
        ),
      );
    },
    [
      activeWorkspaceId,
      client,
      currentSurfaceTabOrder,
      ctl,
      persistSurfaceTabOrder,
    ],
  );
  const beginPaneSurfaceDrag = useCallback(
    (
      event: ReactDragEvent<HTMLElement>,
      paneId: string,
      surfaceId: string | null | undefined,
    ) => {
      if (!activeWorkspaceId || !surfaceId) return;
      event.stopPropagation();
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData(
        PANE_SURFACE_DRAG_TYPE,
        JSON.stringify({
          workspaceId: activeWorkspaceId,
          paneId,
          surfaceId,
        } satisfies PaneSurfaceDragPayload),
      );
    },
    [activeWorkspaceId],
  );
  const allowPaneSurfaceDrop = useCallback(
    (event: ReactDragEvent<HTMLElement>) => {
      if (event.dataTransfer.types.includes(PANE_SURFACE_DRAG_TYPE)) {
        event.preventDefault();
        event.stopPropagation();
        event.dataTransfer.dropEffect = "move";
      }
    },
    [],
  );
  const dropPaneSurface = useCallback(
    async (event: ReactDragEvent<HTMLElement>, targetPane: PaneSummary) => {
      if (!activeWorkspaceId) return;
      const payload = parseDragPayload<PaneSurfaceDragPayload>(
        event,
        PANE_SURFACE_DRAG_TYPE,
      );
      if (
        !payload ||
        payload.workspaceId !== activeWorkspaceId ||
        payload.paneId === targetPane.paneId ||
        targetPane.kind !== "leaf"
      ) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      const targetSurfaceId = targetPane.mountedSurfaceId ?? null;
      await ctl.mountSurface(payload.surfaceId, targetPane.paneId);
      if (targetSurfaceId) {
        await ctl.mountSurface(targetSurfaceId, payload.paneId);
      }
    },
    [activeWorkspaceId, ctl],
  );
  const swapPaneSurfaceByDirection = useCallback(
    async (paneId: string, direction: -1 | 1) => {
      if (!activeWorkspaceId) {
        return;
      }
      const currentIndex = orderedLeafPaneIds.indexOf(paneId);
      const targetPaneId = orderedLeafPaneIds[currentIndex + direction];
      if (currentIndex < 0 || !targetPaneId || paneId === targetPaneId) {
        return;
      }
      const sourcePane = paneById.get(paneId);
      const targetPane = paneById.get(targetPaneId);
      if (!sourcePane || !targetPane || targetPane.kind !== "leaf") {
        return;
      }
      const sourceSurfaceId = sourcePane.mountedSurfaceId ?? null;
      const targetSurfaceId = targetPane.mountedSurfaceId ?? null;
      if (!sourceSurfaceId && !targetSurfaceId) {
        return;
      }
      if (sourceSurfaceId && targetSurfaceId) {
        await ctl.mountSurface(sourceSurfaceId, targetPaneId);
        await ctl.mountSurface(targetSurfaceId, paneId);
      } else if (sourceSurfaceId) {
        await ctl.mountSurface(sourceSurfaceId, targetPaneId);
      } else if (targetSurfaceId) {
        await ctl.mountSurface(targetSurfaceId, paneId);
      }
      await ctl.focusPane(targetPaneId);
    },
    [activeWorkspaceId, ctl, orderedLeafPaneIds, paneById],
  );
  const setupWarning = notifications.find(
    (notification) =>
      notification.notificationType === "diagnostics.wsl_required" ||
      notification.notificationType === "diagnostics.tmux_required",
  );
  const activeSessionState = activePaneId
    ? (() => {
        const pane = paneById.get(activePaneId);
        const surface = pane ? surfaceForPane(pane) : undefined;
        return surface?.sessionId
          ? sessionById.get(surface.sessionId)
          : undefined;
      })()
    : undefined;
  const activeTerminalSession = activePaneId
    ? (() => {
        const pane = paneById.get(activePaneId);
        const surface = pane ? surfaceForPane(pane) : undefined;
        return surface?.surfaceType === "terminal" && surface.sessionId
          ? sessionById.get(surface.sessionId)
          : undefined;
      })()
    : undefined;
  const textBoxDraftKey = activeTerminalSession
    ? textBoxDraftStorageKey(activeTerminalSession.sessionId)
    : null;
  const runningCount = sessions.filter((s) =>
    ["running", "starting", "recovering"].includes(s.state),
  ).length;

  useEffect(() => {
    if (textBoxOpen && !activeTerminalSession) {
      setTextBoxOpen(false);
    }
  }, [activeTerminalSession, textBoxOpen]);

  useEffect(() => {
    setTextBoxDraft(readTextBoxDraft(textBoxDraftKey));
  }, [textBoxDraftKey]);

  // ---- actions ----
  const runTerminalLaunch = useCallback(async (launch: () => Promise<void>) => {
    if (terminalLaunchPendingRef.current) {
      return;
    }

    terminalLaunchPendingRef.current = true;
    setTerminalLaunchPending(true);
    try {
      await launch();
    } finally {
      terminalLaunchPendingRef.current = false;
      setTerminalLaunchPending(false);
    }
  }, []);
  const splitSurfaceTabToPane = useCallback(
    async (surface: SurfaceSummary, axis: "horizontal" | "vertical") => {
      closeSurfaceTabMenu();
      if (!activeWorkspaceId) {
        return;
      }

      const host = paneHostingSurface(surface.surfaceId);
      const activePane = activePaneId ? paneById.get(activePaneId) : undefined;
      const activeRoot = activePane ? rootPaneForPane(activePane) : undefined;
      const hostRoot = host ? rootPaneForPane(host) : undefined;
      const sameRoot = Boolean(
        activeRoot && hostRoot && activeRoot.paneId === hostRoot.paneId,
      );
      const fallbackPaneId = orderedLeafPaneIds[0];
      const basePane =
        sameRoot && host?.kind === "leaf"
          ? host
          : activePane?.kind === "leaf"
            ? activePane
            : host?.kind === "leaf"
              ? host
              : fallbackPaneId
                ? paneById.get(fallbackPaneId)
                : undefined;
      if (!basePane) {
        return;
      }

      try {
        const splitDetail = await client.splitPane(
          activeWorkspaceId,
          basePane.paneId,
          axis,
        );
        if (!sameRoot) {
          const targetPaneId = targetPaneForMovedSurfaceSplit(
            splitDetail,
            basePane.paneId,
            surface.surfaceId,
          );
          if (!targetPaneId) {
            throw new Error("Could not find a split pane for the tab.");
          }
          await client.mountSurface(activeWorkspaceId, targetPaneId, surface.surfaceId);
          await client.focusPane(activeWorkspaceId, targetPaneId);
        }
        await ctl.refresh();
      } catch (error) {
        console.warn("[agentmux] failed to split tab into pane", {
          error,
          surfaceId: surface.surfaceId,
          axis,
        });
      }
    },
    [
      activePaneId,
      activeWorkspaceId,
      client,
      closeSurfaceTabMenu,
      ctl,
      orderedLeafPaneIds,
      paneById,
      paneHostingSurface,
      rootPaneForPane,
    ],
  );
  const duplicateSurfaceTab = useCallback(
    async (surface: SurfaceSummary) => {
      closeSurfaceTabMenu();
      if (!activeWorkspaceId) {
        return;
      }

      try {
        if (surface.surfaceType === "browser") {
          const duplicated = await client.createBrowserSurface(
            activeWorkspaceId,
            null,
            "default",
            "new_tab",
          );
          try {
            const current = await client.browserCurrentUrl(surface.surfaceId);
            if (current.url && current.url !== "about:blank") {
              await client.browserNavigate(duplicated.surfaceId, current.url);
            }
          } catch (error) {
            console.warn("[agentmux] failed to copy browser URL for duplicate", {
              error,
              surfaceId: surface.surfaceId,
            });
          }
          await ctl.refresh();
          return;
        }

        const session = surface.sessionId
          ? sessionById.get(surface.sessionId)
          : undefined;
        const agentCommand = surface.sessionId
          ? agentCommandFromTelemetry(agentBySession.get(surface.sessionId))
          : [];
        if (agentCommand.length > 0) {
          await runTerminalLaunch(() => ctl.spawnAgent(agentCommand));
          return;
        }

        const fallbackDistribution =
          activeWorkspace?.defaultWslDistribution ||
          wslDistributions.find((distribution) => distribution.isDefault)?.name ||
          wslDistributions[0]?.name ||
          null;
        if (session?.backendKind === "wsl-direct") {
          await runTerminalLaunch(() =>
            ctl.spawnTerminalProfile("wsl", fallbackDistribution),
          );
          return;
        }
        if (session?.backendKind === "wsl-tmux-control") {
          if (!fallbackDistribution) {
            await runTerminalLaunch(() => ctl.spawnDefaultTerminal());
            return;
          }
          await runTerminalLaunch(async () => {
            await client.spawnDurableWslTerminal(
              activeWorkspaceId,
              fallbackDistribution,
              activeWorkspace?.projectRoot ?? null,
              "new_tab",
            );
            await ctl.refresh();
          });
          return;
        }

        const title = surface.title.toLowerCase();
        const profile: TerminalProfile =
          title.includes("cmd") && !title.includes("powershell")
            ? "cmd"
            : "powershell";
        await runTerminalLaunch(() => ctl.spawnTerminalProfile(profile));
      } catch (error) {
        console.warn("[agentmux] failed to duplicate tab", {
          error,
          surfaceId: surface.surfaceId,
        });
      }
    },
    [
      activeWorkspace,
      activeWorkspaceId,
      agentBySession,
      client,
      closeSurfaceTabMenu,
      ctl,
      runTerminalLaunch,
      sessionById,
      wslDistributions,
    ],
  );

  const openTerminalInPane = useCallback(
    async (paneId: string) => {
      await runTerminalLaunch(() => ctl.spawnDefaultTerminalInPane(paneId));
    },
    // PR-2: keep a stable identity for the PaneView memo (ctl.* methods and
    // runTerminalLaunch are themselves stable).
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ctl.spawnDefaultTerminalInPane, runTerminalLaunch],
  );
  const openDurableTerminalInPane = useCallback(
    async (paneId: string) => {
      await runTerminalLaunch(() => ctl.spawnDurableTerminalInPane(paneId));
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ctl.spawnDurableTerminalInPane, runTerminalLaunch],
  );
  const addTerminal = useCallback(async () => {
    await runTerminalLaunch(() => ctl.spawnDefaultTerminal());
  }, [ctl, runTerminalLaunch]);
  const openContextLink = useCallback(async () => {
    const selectedText = window.getSelection()?.toString() ?? "";
    const url =
      extractFirstUrl(selectedText) ??
      notifications
        .map((notification) =>
          extractFirstUrl(`${notification.title}\n${notification.message}`),
        )
        .find((candidate): candidate is string => Boolean(candidate)) ??
      teamMessages
        .map((message) => extractFirstUrl(message.body))
        .find((candidate): candidate is string => Boolean(candidate)) ??
      teamTasks
        .map((task) => extractFirstUrl(`${task.title}\n${task.description ?? ""}`))
        .find((candidate): candidate is string => Boolean(candidate)) ??
      attention
        .map((state) => extractFirstUrl(state.reason))
        .find((candidate): candidate is string => Boolean(candidate)) ??
      null;
    const surface = await ctl.createBrowserSurface("new_tab");
    if (surface && url) {
      await ctl.browserNavigate(surface.surfaceId, url);
    }
    closeOverlay();
  }, [attention, closeOverlay, ctl, notifications, teamMessages, teamTasks]);
  const openTerminalLinkInBrowserSplit = useCallback(
    async (rawUrl: string, paneId: string) => {
      const url = normalizeHttpUrl(rawUrl);
      if (!url) {
        return;
      }

      // Default: hand off to the OS browser so OAuth/login loopback callbacks
      // (Claude Code, etc.) complete. The in-app split browser is opt-in.
      if (terminalLinkOpenMode === "system") {
        const opened = await openUrlInSystemBrowser(url);
        if (opened) {
          return;
        }
        // System open unavailable (or refused): fall through to the in-app
        // browser so the link is never silently dropped.
      }

      if (!activeWorkspaceId) {
        return;
      }

      try {
        const splitDetail = await client.splitPane(
          activeWorkspaceId,
          paneId,
          "vertical",
        );
        const browserPaneId = targetPaneForSplitBrowser(splitDetail, paneId);
        if (!browserPaneId) {
          throw new Error("Could not find a split pane for the browser.");
        }
        const surface = await client.createBrowserSurface(
          activeWorkspaceId,
          browserPaneId,
          "default",
          "active_pane",
        );
        await client.browserNavigate(surface.surfaceId, url);
        await ctl.refresh();
      } catch (error) {
        console.warn("[agentmux] failed to open terminal link in split browser", {
          error,
          paneId,
          url,
        });
        const surface = await ctl.createBrowserSurface("new_tab");
        if (surface) {
          await ctl.browserNavigate(surface.surfaceId, url);
        }
      }
    },
    [
      terminalLinkOpenMode,
      activeWorkspaceId,
      client,
      ctl.browserNavigate,
      ctl.createBrowserSurface,
      ctl.refresh,
    ],
  );
  const addTerminalProfile = useCallback(
    async (item: TerminalProfileMenuItem) => {
      if (item.disabled) {
        return;
      }
      const targetPaneId = terminalProfileMenu?.paneId ?? null;
      closeTerminalProfileMenu();
      await runTerminalLaunch(() =>
        targetPaneId
          ? ctl.spawnTerminalProfileInPane(
              targetPaneId,
              item.profile,
              item.distribution ?? null,
            )
          : ctl.spawnTerminalProfile(item.profile, item.distribution ?? null),
      );
    },
    [closeTerminalProfileMenu, ctl, runTerminalLaunch, terminalProfileMenu?.paneId],
  );
  const openTextBoxComposer = useCallback(() => {
    if (!activeTerminalSession) {
      return;
    }
    setTextBoxOpen(true);
    closeOverlay();
  }, [activeTerminalSession, closeOverlay]);
  const updateTextBoxDraft = useCallback(
    (value: string) => {
      setTextBoxDraft(value);
      writeTextBoxDraft(textBoxDraftKey, value);
    },
    [textBoxDraftKey],
  );
  const sendTextBoxDraft = useCallback(async () => {
    if (!activeTerminalSession || textBoxDraft.trim().length === 0) {
      return;
    }
    const payload = `${textBoxDraft.replace(/\r?\n/g, "\r")}\r`;
    await client.sendText(activeTerminalSession.sessionId, payload);
    clearTextBoxDraft(textBoxDraftKey);
    setTextBoxDraft("");
    setTextBoxOpen(false);
    void ctl.refresh();
  }, [activeTerminalSession, client, ctl, textBoxDraft, textBoxDraftKey]);
  const trustDock = useCallback(async () => {
    if (!activeWorkspaceId || !dockConfig) {
      return;
    }
    setDockRunMessage("Trusting Dock...");
    try {
      const trustedDock = await client.trustDock(activeWorkspaceId);
      setDockConfig(trustedDock);
      setDockTrusted(Boolean(trustedDock.trusted));
      setDockRunMessage(
        trustedDock.trusted ? "Dock trusted." : "Dock trust unchanged.",
      );
    } catch (error) {
      setDockRunMessage(
        error instanceof Error ? error.message : "Failed to trust Dock.",
      );
    }
  }, [activeWorkspaceId, client, dockConfig]);
  const runDockControl = useCallback(
    async (control: DockControl) => {
      if (!dockConfig) {
        return;
      }
      if (dockConfig.requiresTrust && !dockTrusted) {
        setDockRunMessage("Review Dock before running.");
        return;
      }
      const existing = dockSurfaceByControlId.get(control.id);
      setDockRunMessage(`Starting ${control.title}...`);
      if (existing) {
        await ctl.closeSurface(existing.surfaceId);
      }
      const session = await ctl.spawnDockControl(control);
      if (session) {
        setActiveDockSessionId(session.sessionId);
      }
      setDockRunMessage(`Started ${control.title}.`);
    },
    [ctl, dockConfig, dockSurfaceByControlId, dockTrusted],
  );
  const closeDockSurface = useCallback(
    async (surfaceId: string) => {
      await ctl.closeSurface(surfaceId);
      setActiveDockSessionId((current) => {
        const closed = dockTerminalSurfaces.find(
          (surface) => surface.surfaceId === surfaceId,
        );
        return closed?.sessionId === current ? null : current;
      });
      setDockRunMessage("Dock terminal closed.");
    },
    [ctl, dockTerminalSurfaces],
  );
  const resolveBrowserActionSurface = useCallback(
    async (placement: "new_tab" | "active_pane") => {
      if (placement === "new_tab") {
        return ctl.createBrowserSurface("new_tab");
      }
      const activePane = activePaneId ? paneById.get(activePaneId) : undefined;
      const activeSurface = activePane ? surfaceForPane(activePane) : undefined;
      if (activeSurface?.surfaceType === "browser") {
        return activeSurface;
      }
      const existingBrowser = surfaces.find(
        (surface) => surface.surfaceType === "browser",
      );
      if (existingBrowser) {
        return existingBrowser;
      }
      return ctl.createBrowserSurface("active_pane");
    },
    [activePaneId, ctl, paneById, surfaces],
  );
  const runBrowserCustomAction = useCallback(
    async (command: string[]) => {
      const preset = browserCustomActionPreset(command);
      if (preset.kind === "open") {
        const surface = await ctl.createBrowserSurface(preset.placement);
        if (surface && preset.url) {
          await ctl.browserNavigate(surface.surfaceId, preset.url);
        }
        return;
      }
      const surface = await resolveBrowserActionSurface(preset.placement);
      if (!surface) {
        return;
      }
      if (preset.kind === "screenshot") {
        await ctl.browserScreenshot(surface.surfaceId, preset.format);
      } else if (preset.kind === "dom_snapshot") {
        await ctl.browserDomSnapshot(surface.surfaceId, preset.frameId);
      } else if (preset.kind === "evaluate") {
        await ctl.browserEvaluate(
          surface.surfaceId,
          preset.script,
          preset.frameId,
        );
      } else if (preset.kind === "click") {
        await ctl.browserClick(surface.surfaceId, {
          selector: preset.selector,
          frameId: preset.frameId,
        });
      } else if (preset.kind === "type") {
        await ctl.browserType(
          surface.surfaceId,
          preset.selector,
          preset.text,
          preset.frameId,
        );
      } else if (preset.kind === "fill") {
        await ctl.browserFill(
          surface.surfaceId,
          preset.selector,
          preset.text,
          preset.frameId,
        );
      } else if (preset.kind === "press") {
        await ctl.browserPress(
          surface.surfaceId,
          preset.selector,
          preset.key,
          preset.frameId,
        );
      } else if (preset.kind === "select") {
        await ctl.browserSelect(
          surface.surfaceId,
          preset.selector,
          preset.values,
          preset.frameId,
        );
      } else if (preset.kind === "scroll") {
        await ctl.browserScroll(surface.surfaceId, {
          selector: preset.selector,
          x: preset.x,
          y: preset.y,
          frameId: preset.frameId,
        });
      } else if (preset.kind === "hover") {
        await ctl.browserHover(
          surface.surfaceId,
          preset.selector,
          preset.frameId,
        );
      } else if (preset.kind === "check") {
        await ctl.browserCheck(
          surface.surfaceId,
          preset.selector,
          preset.checked,
          preset.frameId,
        );
      } else if (preset.kind === "highlight") {
        await ctl.browserHighlight(
          surface.surfaceId,
          preset.selector,
          preset.durationMs,
          preset.frameId,
        );
      } else if (preset.kind === "focus") {
        await ctl.browserFocus(
          surface.surfaceId,
          preset.selector,
          preset.frameId,
        );
      } else if (preset.kind === "zoom") {
        await ctl.browserZoom(surface.surfaceId, preset.percent);
      } else if (preset.kind === "wait_for_selector") {
        await ctl.browserWaitForSelector(
          surface.surfaceId,
          preset.selector,
          preset.timeoutMs,
          preset.frameId,
        );
      } else if (preset.kind === "navigation_control") {
        if (preset.operation === "reload") {
          await ctl.browserReload(surface.surfaceId);
        } else if (preset.operation === "back") {
          await ctl.browserBack(surface.surfaceId);
        } else if (preset.operation === "forward") {
          await ctl.browserForward(surface.surfaceId);
        } else {
          await ctl.browserCurrentUrl(surface.surfaceId);
        }
      }
    },
    [ctl, resolveBrowserActionSurface],
  );
  // PR-2: the control-plane methods are individually stable (memoized inside
  // useAgentmuxControl), but `ctl` itself is a fresh object each render. Depend
  // on the specific methods so these handlers keep a stable identity and the
  // PaneView memo holds across no-op poll ticks.
  const splitPaneBy = useCallback(
    async (paneId: string, axis: "horizontal" | "vertical") => {
      if (!activeWorkspaceId || terminalSplitBehavior === "empty") {
        await ctl.focusPane(paneId);
        await ctl.splitActivePane(axis);
        return;
      }

      const pane = paneById.get(paneId);
      const surface = pane?.mountedSurfaceId
        ? surfaceById.get(pane.mountedSurfaceId)
        : undefined;
      const session =
        surface?.surfaceType === "terminal" && surface.sessionId
          ? sessionById.get(surface.sessionId)
          : undefined;
      if (!pane || pane.kind !== "leaf" || !surface || !session) {
        await ctl.focusPane(paneId);
        await ctl.splitActivePane(axis);
        return;
      }

      const fallbackDistribution =
        activeWorkspace?.defaultWslDistribution ||
        wslDistributions.find((distribution) => distribution.isDefault)?.name ||
        wslDistributions[0]?.name ||
        null;

      try {
        await ctl.focusPane(paneId);
        const latestDetail = await client.getWorkspace(activeWorkspaceId);
        const latestPane =
          latestDetail.panes.find((candidate) => candidate.paneId === paneId) ??
          pane;
        const latestSurface = latestPane.mountedSurfaceId
          ? latestDetail.surfaces.find(
              (candidate) => candidate.surfaceId === latestPane.mountedSurfaceId,
            )
          : undefined;
        const latestSession =
          latestSurface?.surfaceType === "terminal" && latestSurface.sessionId
            ? latestDetail.sessions.find(
                (candidate) => candidate.sessionId === latestSurface.sessionId,
              )
            : undefined;
        const sourceSession = latestSession ?? session;
        const sourceSurface = latestSurface ?? surface;
        const cwd =
          sourceSession.cwd?.trim() ||
          sidebarState?.cwd?.trim() ||
          activeWorkspace?.projectRoot?.trim() ||
          null;

        const splitDetail = await client.splitPane(
          activeWorkspaceId,
          paneId,
          axis,
        );
        const targetPaneId = emptyTargetPaneForSplit(splitDetail, paneId);
        if (!targetPaneId) {
          throw new Error("Could not find a target pane for the split.");
        }

        if (sourceSession.backendKind === "wsl-direct") {
          await client.spawnWslTerminal(
            activeWorkspaceId,
            fallbackDistribution,
            cwd,
            "active_pane",
            targetPaneId,
          );
        } else if (sourceSession.backendKind === "wsl-tmux-control") {
          await client.spawnDurableWslTerminal(
            activeWorkspaceId,
            fallbackDistribution,
            cwd,
            "active_pane",
            targetPaneId,
          );
        } else if (sourceSession.backendKind === "conpty") {
          const title = sourceSurface.title.toLowerCase();
          const profile: Exclude<TerminalProfile, "wsl"> =
            title.includes("cmd") && !title.includes("powershell")
              ? "cmd"
              : "powershell";
          await client.spawnNativeTerminal(
            activeWorkspaceId,
            NATIVE_TERMINAL_COMMANDS[profile],
            "active_pane",
            targetPaneId,
            cwd,
          );
        } else {
          await client.focusPane(activeWorkspaceId, targetPaneId);
        }
        await ctl.refresh();
      } catch (error) {
        console.warn("[agentmux] failed to clone terminal into split pane", {
          error,
          paneId,
          axis,
          sessionId: session.sessionId,
          backendKind: session.backendKind,
        });
        await ctl.refresh();
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [
      activeWorkspace,
      activeWorkspaceId,
      client,
      ctl.focusPane,
      ctl.refresh,
      ctl.splitActivePane,
      paneById,
      sessionById,
      sidebarState?.cwd,
      surfaceById,
      terminalSplitBehavior,
      wslDistributions,
    ],
  );
  const focusPaneStable = useCallback(
    (paneId: string) => {
      void ctl.focusPane(paneId);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ctl.focusPane],
  );
  const focusSessionPane = useCallback(
    (sessionId: string | null | undefined): boolean => {
      if (!sessionId) {
        return false;
      }
      const surface = surfaces.find(
        (candidate) => candidate.sessionId === sessionId,
      );
      const pane = surface
        ? panes.find((candidate) => candidate.mountedSurfaceId === surface.surfaceId)
        : undefined;
      if (!pane) {
        return false;
      }
      void ctl.focusPane(pane.paneId);
      setOverlay(null);
      return true;
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ctl.focusPane, panes, surfaces],
  );
  const focusAttentionTarget = useCallback(
    (target: AttentionPaneTarget | undefined): boolean => {
      if (!target) {
        return false;
      }
      void ctl.focusPane(target.pane.paneId);
      setOverlay(null);
      return true;
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ctl.focusPane],
  );
  const jumpNextAttention = useCallback((): boolean => {
    if (attentionPaneQueue.length === 0) {
      return false;
    }
    const activeIndex = activePaneId
      ? attentionPaneQueue.findIndex(
          (target) => target.pane.paneId === activePaneId,
        )
      : -1;
    const nextTarget =
      attentionPaneQueue[
        activeIndex >= 0 ? (activeIndex + 1) % attentionPaneQueue.length : 0
      ];
    return focusAttentionTarget(nextTarget);
  }, [activePaneId, attentionPaneQueue, focusAttentionTarget]);
  const openNotificationPanel = useCallback(() => {
    setSettingsTab("general");
    setOverlay("settings");
  }, []);
  const closePaneStable = useCallback(
    (paneId: string) => {
      void ctl.closePane(paneId);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ctl.closePane],
  );
  const closeSurfaceStable = useCallback(
    (surfaceId: string) => {
      void ctl.closeSurface(surfaceId);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ctl.closeSurface],
  );
  // PR-2: stable wrappers so the PaneView memo identity holds across poll ticks.
  const onPaneDropStable = useCallback(
    (event: import("react").DragEvent<HTMLElement>, targetPane: PaneSummary) => {
      void dropPaneSurface(event, targetPane);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [dropPaneSurface],
  );
  const onMovePaneSurfaceStable = useCallback(
    (paneId: string, direction: -1 | 1) => {
      void swapPaneSurfaceByDirection(paneId, direction);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [swapPaneSurfaceByDirection],
  );
  const queueTerminalExitRefresh = useCallback((sessionId: string) => {
    exitIntentSessionIdsRef.current.add(sessionId);
    for (const delayMs of [150, 300, 700, 1200, 2500, 5000]) {
      const timer = window.setTimeout(() => {
        exitIntentRefreshTimersRef.current =
          exitIntentRefreshTimersRef.current.filter((candidate) => candidate !== timer);
        void (async () => {
          try {
            await client.getSession(sessionId);
          } catch (error) {
            const code =
              error && typeof error === "object" && "code" in error
                ? String((error as { code?: unknown }).code ?? "")
                : "";
            if (code === "session_not_found") {
              // Session is fully gone — clean up the intent entry so the
              // auto-close loop does not act on it, and skip the refresh
              // (there is nothing new to observe).
              exitIntentSessionIdsRef.current.delete(sessionId);
              return;
            }
            if (code) {
              console.warn("[agentmux] failed to refresh exited terminal state", {
                error,
                sessionId,
              });
            }
          }
          await ctl.refresh();
        })();
      }, delayMs);
      exitIntentRefreshTimersRef.current.push(timer);
    }
  }, [client, ctl.refresh]);
  useEffect(() => {
    return () => {
      for (const timer of exitIntentRefreshTimersRef.current) {
        window.clearTimeout(timer);
      }
      exitIntentRefreshTimersRef.current = [];
    };
  }, []);
  const refreshStable = useCallback(
    () => {
      void ctl.refresh();
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ctl.refresh],
  );
  const closeSurfaceTab = useCallback(
    async (surfaceId: string, _host?: PaneSummary) => {
      await ctl.closeSurface(surfaceId);
    },
    [ctl],
  );
  const runTmuxProbe = useCallback(
    async (distributionOverride?: string | null) => {
      const fallbackDistribution =
        wslDistributions.find((candidate) => candidate.isDefault)?.name ??
        wslDistributions[0]?.name ??
        null;
      const distribution =
        distributionOverride?.trim() ||
        activeWorkspace?.defaultWslDistribution ||
        fallbackDistribution;
      if (!distribution) {
        setTmuxProbe({
          available: false,
          distribution: null,
          version: null,
          message:
            "No WSL distribution is available. Install WSL with `wsl --install` first.",
        });
        return;
      }

      setTmuxProbeBusy(true);
      try {
        setTmuxProbe(await client.checkTmux(distribution));
      } catch (cause) {
        setTmuxProbe({
          available: false,
          distribution,
          version: null,
          message:
            cause instanceof Error ? cause.message : "tmux diagnostics failed.",
        });
      } finally {
        setTmuxProbeBusy(false);
      }
    },
    [activeWorkspace, client, wslDistributions],
  );

  // PR-3: the root style only changes when theme/accent/fontSize change, so it
  // is memoized on those inputs to keep its reference (and the themed CSS vars)
  // stable across the frequent re-renders driven by the poll loop.
  const rootStyle = useMemo<CSSProperties>(
    () => ({
      ...buildRootVars(T, accent, fontSize),
      height: "100vh",
      width: "100vw",
      boxSizing: "border-box",
      background: "var(--canvas)",
      display: "flex",
      flexDirection: "column",
      overflow: "hidden",
      fontFamily: `${FONT_SANS},Pretendard,-apple-system,'Segoe UI',sans-serif`,
      color: T.fg1,
    }),
    [T, accent, fontSize],
  );
  // PR-3: alias the hoisted module-level style constants so the JSX below keeps
  // its existing names while the references stay stable across renders.
  const iconBtn = ICON_BTN_STYLE;
  const iconBtnHover = ICON_BTN_HOVER_STYLE;
  const winCtlBtn = WIN_CTL_BTN_STYLE;
  const winCtlBtnHover = WIN_CTL_BTN_HOVER_STYLE;
  const groupActionBtn = GROUP_ACTION_BTN_STYLE;
  const groupActionHover = GROUP_ACTION_HOVER_STYLE;
  const groupMenuItemStyle = GROUP_MENU_ITEM_STYLE;
  const groupMenuItemHover = GROUP_MENU_ITEM_HOVER_STYLE;
  const terminalProfileMenuItems = useMemo<TerminalProfileMenuItem[]>(() => {
    const nativeItems: TerminalProfileMenuItem[] = [
      {
        id: "powershell",
        profile: "powershell",
        title: "Windows PowerShell",
        description: "powershell.exe -NoLogo",
      },
      {
        id: "cmd",
        profile: "cmd",
        title: "Command Prompt",
        description: "cmd.exe /d /q",
      },
    ];
    const wslItems =
      wslDistributions.length > 0
        ? wslDistributions.map<TerminalProfileMenuItem>((distribution) => ({
            id: `wsl:${distribution.name}`,
            profile: "wsl",
            distribution: distribution.name,
            title: distribution.name,
            description: distribution.isDefault
              ? "WSL distribution · default"
              : "WSL distribution",
          }))
        : [
            {
              id: "wsl:missing",
              profile: "wsl" as const,
              title: "Install WSL",
              description: "No WSL distribution detected",
              disabled: true,
            },
          ];
    return [...nativeItems, ...wslItems];
  }, [wslDistributions]);

  // ---- action registry, shortcuts, and command palette ----
  const shortcutBindings = useMemo(
    () => buildResolvedShortcutBindings(shortcutOverrides),
    [shortcutOverrides],
  );
  const shortcutIndex = useMemo(
    () => buildShortcutIndex(shortcutBindings),
    [shortcutBindings],
  );
  const promptCustomAgent = useCallback(() => {
    const raw = window.prompt(
      "Agent command to run in a durable session (for example: claude)",
    );
    const parts = (raw ?? "").trim().split(/\s+/).filter(Boolean);
    if (parts.length > 0) {
      void ctl.spawnAgent(parts);
    }
    closeOverlay();
  }, [closeOverlay, ctl.spawnAgent]);

  // PR-7: keep source-list-derived command descriptors separate from the core
  // command list so workspace/WSL/custom maps do not rebuild on unrelated UI
  // state changes.
  const customActionDescriptors = useMemo<ActionDescriptor[]>(
    () =>
      customActions.map<ActionDescriptor>((customAction) => ({
        id: customAction.id,
        group: groupForCustomAction(customAction),
        title: customAction.title,
        keywords: [
          customAction.target,
          ...customAction.command,
          ...customAction.keywords,
        ],
        run: () => {
          switch (customAction.target) {
            case "agent":
              void ctl.spawnAgent(customAction.command);
              break;
            case "wsl-terminal":
              void ctl.spawnDefaultTerminal();
              break;
            case "browser":
              void runBrowserCustomAction(customAction.command);
              break;
          }
          closeOverlay();
        },
      })),
    [
      closeOverlay,
      customActions,
      ctl.spawnAgent,
      ctl.spawnDefaultTerminal,
      runBrowserCustomAction,
    ],
  );
  const workspaceActionDescriptors = useMemo<ActionDescriptor[]>(
    () =>
      workspaces.map<ActionDescriptor>((ws) => ({
        id: `workspace.select.${ws.workspaceId}`,
        group: "workspace",
        title: `Switch to ${ws.name}`,
        keywords: [ws.projectRoot ?? "", ws.name],
        run: () => {
          void ctl.selectWorkspace(ws.workspaceId);
          closeOverlay();
        },
      })),
    [closeOverlay, ctl.selectWorkspace, workspaces],
  );
  const wslActionDescriptors = useMemo<ActionDescriptor[]>(
    () =>
      wslDistributions.map<ActionDescriptor>((distribution) => ({
        id: `wsl.open.${distribution.name}`,
        group: "remote",
        title: `WSL shell: ${distribution.name}`,
        keywords: [distribution.name, distribution.isDefault ? "default" : ""],
        run: () => {
          void ctl.spawnWslTerminal(distribution.name);
          closeOverlay();
        },
      })),
    [closeOverlay, ctl.spawnWslTerminal, wslDistributions],
  );
  const actions = useMemo<ActionDescriptor[]>(
    () => [
      {
        id: "app.commandPalette",
        group: "view",
        title: t("app.commandPalette.open"),
        visibleInPalette: false,
        run: () => {
          setOverlay("palette");
          setQuery("");
          setPaletteSelectedIndex(0);
        },
      },
      {
        id: "app.commandPalette.legacy",
        group: "view",
        title: t("app.commandPalette.open"),
        visibleInPalette: false,
        run: () => {
          setOverlay("palette");
          setQuery("");
          setPaletteSelectedIndex(0);
        },
      },
      {
        id: "agent.launchClaude",
        group: "agent",
        title: "Run Claude Code (durable tmux)",
        keywords: ["claude", "tmux"],
        run: () => {
          void ctl.spawnAgent(["claude"]);
          closeOverlay();
        },
      },
      {
        id: "agent.launchCodex",
        group: "agent",
        title: "Run Codex (durable tmux)",
        keywords: ["codex", "tmux", "no-alt-screen"],
        run: () => {
          void ctl.spawnAgent(["codex", "--no-alt-screen"]);
          closeOverlay();
        },
      },
      {
        id: "agent.launchCustom",
        group: "agent",
        title: "Run custom agent...",
        keywords: ["custom", "tmux"],
        run: promptCustomAgent,
      },
      {
        id: "agent.jumpNextAttention",
        group: "agent",
        title: "Jump to next waiting agent",
        keywords: ["attention", "waiting", "agent", "jump"],
        disabled: attentionPaneQueue.length === 0,
        run: () => {
          void jumpNextAttention();
        },
      },
      {
        id: "terminal.newWsl",
        group: "terminal",
        title: "New terminal",
        keywords: ["terminal", "wsl", "powershell", "cmd", "shell"],
        run: () => {
          void addTerminal();
          closeOverlay();
        },
      },
      {
        id: "terminal.openInActivePane",
        group: "terminal",
        title: "Open terminal in current pane",
        keywords: ["terminal", "wsl", "powershell", "cmd", "pane"],
        disabled: activePaneId === null,
        run: () => {
          if (activePaneId) {
            void openTerminalInPane(activePaneId);
          }
          closeOverlay();
        },
      },
      {
        id: "terminal.textBox",
        group: "terminal",
        title: "TextBox",
        keywords: ["prompt", "composer", "send"],
        disabled: !activeTerminalSession,
        run: openTextBoxComposer,
      },
      {
        id: "pane.splitRight",
        group: "terminal",
        title: "세로 분할",
        keywords: ["split", "right"],
        disabled: activePaneId === null,
        run: () => {
          if (activePaneId) {
            void splitPaneBy(activePaneId, "vertical");
          }
          closeOverlay();
        },
      },
      {
        id: "pane.splitDown",
        group: "terminal",
        title: "가로 분할",
        keywords: ["split", "down"],
        disabled: activePaneId === null,
        run: () => {
          if (activePaneId) {
            void splitPaneBy(activePaneId, "horizontal");
          }
          closeOverlay();
        },
      },
      {
        id: "browser.openNewTab",
        group: "terminal",
        title: "브라우저 새 탭 열기",
        keywords: ["browser", "surface", "tab"],
        run: () => {
          void ctl.createBrowserSurface("new_tab");
          closeOverlay();
        },
      },
      {
        id: "browser.openActivePane",
        group: "terminal",
        title: "현재 페인에 브라우저 열기",
        keywords: ["browser", "surface", "pane"],
        disabled: activePaneId === null,
        run: () => {
          void ctl.createBrowserSurface("active_pane");
          closeOverlay();
        },
      },
      {
        id: "workspace.new",
        group: "workspace",
        title: t("workspace.add"),
        keywords: ["workspace"],
        run: () => {
          void createWorkspace();
          closeOverlay();
        },
      },
      {
        id: "browser.openContextLink",
        group: "terminal",
        title: "Open context link",
        keywords: ["browser", "url", "link", "context", "docs", "pr"],
        run: () => {
          void openContextLink();
        },
      },
      ...customActionDescriptors,
      ...workspaceActionDescriptors,
      {
        id: "view.toggleTheme",
        group: "view",
        title: `${t("settings.theme")}: ${t("appearance.dark")} / ${t("appearance.light")}`,
        keywords: ["theme", "dark", "light"],
        run: () => {
          setTheme(isDark ? "light" : "dark");
          closeOverlay();
        },
      },
      {
        id: "app.settings",
        group: "view",
        title: t("app.settings.open"),
        keywords: ["settings"],
        run: () => setOverlay("settings"),
      },
      {
        id: "notification.openPanel",
        group: "view",
        title: "Open notifications",
        keywords: ["notification", "attention", "waiting"],
        run: openNotificationPanel,
      },
      {
        id: "app.setup",
        group: "remote",
        title: "Windows setup",
        keywords: ["setup", "wsl", "tmux", "cmux", "first run"],
        run: () => setOverlay("setup"),
      },
      {
        id: "app.search",
        group: "view",
        title: t("app.search.activeWindow"),
        keywords: ["find", "search"],
        run: () => setOverlay("search"),
      },
      ...wslActionDescriptors,
    ],
    [
      activePaneId,
      activeTerminalSession,
      addTerminal,
      attentionPaneQueue.length,
      closeOverlay,
      createWorkspace,
      ctl.createBrowserSurface,
      ctl.spawnAgent,
      customActionDescriptors,
      isDark,
      jumpNextAttention,
      openContextLink,
      openNotificationPanel,
      openTerminalInPane,
      openTextBoxComposer,
      promptCustomAgent,
      splitPaneBy,
      t,
      workspaceActionDescriptors,
      wslActionDescriptors,
    ],
  );
  const actionsById = useMemo(
    () => new Map(actions.map((action) => [action.id, action])),
    [actions],
  );
  const executeAction = useCallback(
    (actionId: string): boolean => {
      const action = actionsById.get(actionId);
      if (!action || action.disabled) {
        return false;
      }
      void action.run();
      return true;
    },
    [actionsById],
  );
  const notificationActionsFor = useCallback(
    (notification: NotificationSummary): NotificationActionBinding[] =>
      notificationActions
        .map((hook) => ({ hook, action: actionsById.get(hook.action) }))
        .filter((binding): binding is NotificationActionBinding =>
          Boolean(
            binding.action &&
            !binding.action.disabled &&
            matchesNotificationAction(binding.hook, notification),
          ),
        ),
    [actionsById, notificationActions],
  );
  const runNotificationAction = useCallback(
    (hook: AppConfigNotificationAction, notification: NotificationSummary) => {
      if (executeAction(hook.action) && hook.dismissOnRun) {
        void ctl.dismissNotification(notification.notificationId);
      }
    },
    [ctl, executeAction],
  );
  const workspacePlusActionId =
    uiConfig.workspacePlusAction ?? DEFAULT_WORKSPACE_PLUS_ACTION;
  const surfaceTabPlusActionId =
    uiConfig.surfaceTabPlusAction ?? DEFAULT_SURFACE_TAB_PLUS_ACTION;
  const surfaceTabActionIds = useMemo(
    () => uiConfig.surfaceTabActions ?? DEFAULT_SURFACE_TAB_ACTIONS,
    [uiConfig.surfaceTabActions],
  );
  const textBoxMaxLines = clampTextBoxMaxLines(uiConfig.textBoxMaxLines);
  const terminalInnerMargin = clampTerminalInnerMargin(
    uiConfig.terminalInnerMargin,
  );
  const terminalStartDirectory =
    uiConfig.terminalStartDirectory ?? "home";
  const terminalStartCustomCwd = uiConfig.terminalStartCustomCwd ?? "";
  const updateTerminalInnerMargin = useCallback(
    (value: number) => {
      const nextMargin = clampTerminalInnerMargin(value);
      setUiConfig((current) => ({
        ...current,
        terminalInnerMargin: nextMargin,
      }));
      void client
        .updateConfig(
          {
            ui: {
              terminalInnerMargin: nextMargin,
            },
          },
          activeWorkspaceId,
        )
        .then((config) => applyConfig(config))
        .catch(() => undefined);
    },
    [activeWorkspaceId, applyConfig, client],
  );
  const updateTerminalStartDirectory = useCallback(
    (value: TerminalStartDirectory) => {
      setUiConfig((current) => ({
        ...current,
        terminalStartDirectory: value,
      }));
      void client
        .updateConfig(
          {
            ui: {
              terminalStartDirectory: value,
            },
          },
          activeWorkspaceId,
        )
        .then((config) => applyConfig(config))
        .catch(() => undefined);
    },
    [activeWorkspaceId, applyConfig, client],
  );
  const updateTerminalStartCustomCwd = useCallback(
    (value: string) => {
      setUiConfig((current) => ({
        ...current,
        terminalStartCustomCwd: value,
      }));
      void client
        .updateConfig(
          {
            ui: {
              terminalStartCustomCwd: value,
            },
          },
          activeWorkspaceId,
        )
        .then((config) => applyConfig(config))
        .catch(() => undefined);
    },
    [activeWorkspaceId, applyConfig, client],
  );
  const updateTerminalSplitBehavior = useCallback(
    (value: TerminalSplitBehavior) => {
      setUiConfig((current) => ({
        ...current,
        terminalSplitBehavior: value,
      }));
      void client
        .updateConfig(
          {
            ui: {
              terminalSplitBehavior: value,
            },
          },
          activeWorkspaceId,
        )
        .then((config) => applyConfig(config))
        .catch(() => undefined);
    },
    [activeWorkspaceId, applyConfig, client],
  );
  const surfaceTabActions = useMemo(
    () =>
      surfaceTabActionIds
        .map((actionId) => actionsById.get(actionId))
        .filter((action): action is ActionDescriptor =>
          Boolean(action && !action.disabled),
        ),
    [actionsById, surfaceTabActionIds],
  );
  const runConfiguredAction = useCallback(
    (actionId: string, fallback: () => void | Promise<void>) => {
      if (!executeAction(actionId)) {
        void fallback();
      }
    },
    [executeAction],
  );
  useEffect(() => {
    function clearPendingShortcut() {
      pendingShortcutRef.current = null;
      if (pendingShortcutTimerRef.current !== null) {
        window.clearTimeout(pendingShortcutTimerRef.current);
        pendingShortcutTimerRef.current = null;
      }
    }

    function onKey(event: KeyboardEvent) {
      if (confirmDialog) {
        return;
      }
      const key = (event.key || "").toLowerCase();
      if (key === "escape" && overlay) {
        event.preventDefault();
        event.stopPropagation();
        clearPendingShortcut();
        setOverlay(null);
        return;
      }
      if (overlay || isEditableShortcutTarget(event.target)) {
        return;
      }

      const stroke = keyboardEventToStroke(event);

      // ⌘/Ctrl+B — toggle the workspace sidebar (VS Code convention).
      if (
        (event.metaKey || event.ctrlKey) &&
        !event.altKey &&
        !event.shiftKey &&
        key === "b" &&
        (!stroke ||
          (!shortcutIndex.chordPrefix.has(stroke) &&
            !shortcutIndex.single.has(stroke)))
      ) {
        event.preventDefault();
        event.stopPropagation();
        setSidebarCollapsed((collapsed) => !collapsed);
        return;
      }

      // ⌘/Ctrl +/-/0 — UI zoom (persisted; browser / VS Code convention).
      if ((event.metaKey || event.ctrlKey) && !event.altKey) {
        if (key === "=" || key === "+") {
          event.preventDefault();
          nudgeZoom(ZOOM_STEP);
          return;
        }
        if (key === "-" || key === "_") {
          event.preventDefault();
          nudgeZoom(-ZOOM_STEP);
          return;
        }
        if (key === "0") {
          event.preventDefault();
          resetZoom();
          return;
        }
      }

      if (!stroke) {
        return;
      }

      const pendingStroke = pendingShortcutRef.current;
      if (pendingStroke) {
        const actionId = shortcutIndex.chord.get(
          chordKey(pendingStroke, stroke),
        );
        clearPendingShortcut();
        if (actionId && executeAction(actionId)) {
          event.preventDefault();
          event.stopPropagation();
        }
        return;
      }

      if (shortcutIndex.chordPrefix.has(stroke)) {
        event.preventDefault();
        event.stopPropagation();
        pendingShortcutRef.current = stroke;
        pendingShortcutTimerRef.current = window.setTimeout(
          clearPendingShortcut,
          1400,
        );
        return;
      }

      const actionId = shortcutIndex.single.get(stroke);
      if (actionId && executeAction(actionId)) {
        event.preventDefault();
        event.stopPropagation();
      }
    }

    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      clearPendingShortcut();
    };
  }, [confirmDialog, executeAction, overlay, shortcutIndex]);

  const q = query.trim().toLowerCase();
  const rawGroups = ACTION_GROUP_ORDER.map((group) => ({
      label: actionGroupLabel(t, group),
      items: actions
        .filter(
          (action) =>
            action.group === group && action.visibleInPalette !== false,
        )
        .map((action) => ({
          action,
          id: action.id,
          title: action.title,
          hint: shortcutLabelForAction(shortcutBindings, action.id),
          disabled: action.disabled,
          highlighted: false,
          onClick: () => {
            void executeAction(action.id);
          },
        })),
    }),
  );
  const groups: PaletteGroup[] = [];
  let itemIndex = 0;
  for (const group of rawGroups) {
    const items = group.items.filter(
      (it) =>
        !q ||
        it.title.toLowerCase().includes(q) ||
        it.hint.toLowerCase().includes(q) ||
        it.action.id.toLowerCase().includes(q) ||
        (it.action.keywords ?? []).some((keyword) =>
          keyword.toLowerCase().includes(q),
        ),
    );
    if (items.length > 0) {
      for (const it of items) {
        it.highlighted = itemIndex === paletteSelectedIndex;
        itemIndex += 1;
      }
      groups.push({
        label: group.label,
        items: items.map(
          ({ id, title, hint, highlighted, disabled, onClick }) => ({
            id,
            title,
            hint,
            highlighted,
            disabled,
            onClick,
          }),
        ),
      });
    }
  }
  const paletteItemCount = itemIndex;

  useEffect(() => {
    if (overlay !== "palette") {
      return;
    }
    setPaletteSelectedIndex((current) => {
      if (paletteItemCount <= 0) {
        return 0;
      }
      return Math.min(current, paletteItemCount - 1);
    });
  }, [overlay, paletteItemCount]);

  const movePaletteSelection = useCallback(
    (delta: number) => {
      setPaletteSelectedIndex((current) => {
        if (paletteItemCount <= 0) {
          return 0;
        }
        return (current + delta + paletteItemCount) % paletteItemCount;
      });
    },
    [paletteItemCount],
  );

  const runSelectedPaletteItem = useCallback(() => {
    let index = 0;
    for (const group of groups) {
      for (const item of group.items) {
        if (index === paletteSelectedIndex) {
          item.onClick();
          return;
        }
        index += 1;
      }
    }
  }, [groups, paletteSelectedIndex]);

  // ---- recursive pane-tree renderer (the real backend split tree) ----
  const renderPane = (paneId: string, visited = new Set<string>()): ReactNode => {
    if (visited.has(paneId)) {
      return (
        <div key={`${paneId}-cycle`} className="agentmux-empty-pane">
          {t("pane.invalidLayout")}
        </div>
      );
    }
    visited.add(paneId);
    const pane = paneById.get(paneId);
    try {
      if (!pane) return null;

    if (pane.kind === "split") {
      const children = (childrenByParent.get(pane.paneId) ?? []).filter(
        (child) => child.paneId !== pane.paneId,
      );
      const ratio = pane.splitRatio ?? 0.5;
      const column = pane.splitAxis === "horizontal";
      const [first, second] = children;
      return (
        <div
          key={pane.paneId}
          style={{
            display: "flex",
            flexDirection: column ? "column" : "row",
            gap: 2,
            minWidth: 0,
            minHeight: 0,
            flex: "1 1 0",
          }}
        >
          {first ? (
            <div
              style={{
                flex: `${ratio} 1 0`,
                minWidth: 0,
                minHeight: 0,
                display: "flex",
              }}
            >
              {renderPane(first.paneId, visited)}
            </div>
          ) : null}
          {first && second ? (
            <SplitHandle
              vertical={!column}
              onResize={(value) => ctl.resizePane(pane.paneId, value)}
            />
          ) : null}
          {second ? (
            <div
              style={{
                flex: `${1 - ratio} 1 0`,
                minWidth: 0,
                minHeight: 0,
                display: "flex",
              }}
            >
              {renderPane(second.paneId, visited)}
            </div>
          ) : null}
        </div>
      );
    }

    const surface = surfaceForPane(pane);
    const session = surface?.sessionId
      ? sessionById.get(surface.sessionId)
      : undefined;
    const active = pane.paneId === activePaneId;
    const attentionState = session
      ? attentionBySession.get(session.sessionId)
      : undefined;
    const hasAttention = Boolean(attentionState);
    const agentState = session
      ? (agentBySession.get(session.sessionId) ?? null)
      : null;
    const restoringAgent = isRestorableAgentPlaceholder(
      session,
      agentState,
      surface?.surfaceType === "browser",
    );
    const telemetry = session
      ? (agentState?.telemetry ?? null)
      : null;
    const isBrowser = surface?.surfaceType === "browser";
    const title = surface?.title ?? t("pane.empty");
    const dot = restoringAgent
      ? "var(--accent)"
      : sessionDotColor(T, session, hasAttention);
    const label = restoringAgent
      ? t("pane.restoring")
      : translatedSessionLabel(t, session, hasAttention);

      return (
      <PaneView
        key={pane.paneId}
        pane={pane}
        surface={surface}
        session={session}
        active={active}
        isBrowser={isBrowser}
        agentState={agentState}
        telemetry={telemetry}
        hasAttention={hasAttention}
        attentionReason={attentionState?.reason ?? null}
        title={title}
        dot={dot}
        label={label}
        theme={T}
        client={client}
        terminalInnerMargin={terminalInnerMargin}
        fontSize={fontSize}
        terminalLaunchPending={terminalLaunchPending}
        t={t}
        focusPane={focusPaneStable}
        splitPaneBy={splitPaneBy}
        closePane={closePaneStable}
        closeSurface={closeSurfaceStable}
        openTerminalInPane={openTerminalInPane}
        openTerminalProfileMenu={openTerminalProfileMenu}
        openDurableTerminalInPane={openDurableTerminalInPane}
        onOpenTerminalLink={openTerminalLinkInBrowserSplit}
        onTerminalExitIntent={queueTerminalExitRefresh}
        onPaneDragStart={beginPaneSurfaceDrag}
        onPaneDragOver={allowPaneSurfaceDrop}
        onPaneDrop={onPaneDropStable}
        onMovePaneSurface={onMovePaneSurfaceStable}
        onTerminalError={refreshStable}
      />
    );
    } finally {
      visited.delete(paneId);
    }
  };

  return (
    <div data-agentmux-root style={rootStyle}>
      {/* ============ APP SHELL (fills the OS window) ============ */}
      <div
        style={{
          position: "relative",
          flex: "1 1 0",
          minHeight: 0,
          minWidth: 0,
          background: "var(--canvas)",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
        }}
      >
        {/* titlebar — custom/frameless (decorations: false) */}
        <div
          data-tauri-drag-region
          style={{
            height: 40,
            flex: "none",
            display: "flex",
            alignItems: "center",
            padding: "0 10px 0 12px",
            background: "var(--surface)",
            borderBottom: "1px solid var(--border)",
          }}
        >
          <Hov
            tag="button"
            ariaLabel={t("app.sidebar.toggle")}
            title={`${t("app.sidebar.toggle")} (⌘B)`}
            style={{
              ...iconBtn,
              marginRight: 6,
              color: sidebarCollapsed ? "var(--accent)" : "var(--fg2)",
            }}
            hover={iconBtnHover}
            onClick={() => setSidebarCollapsed((c) => !c)}
          >
            <IconSidebar />
          </Hov>
          {/* Non-interactive breadcrumb cluster — pointer-events:none lets
             clicks fall through to the parent drag region so the titlebar
             center drags the window. */}
          <div
            data-tauri-drag-region
            style={{
              display: "flex",
              alignItems: "center",
              pointerEvents: "none",
            }}
          >
            <span
              style={{
                font: `700 13px/1 ${FONT_MONO}`,
                letterSpacing: "-0.02em",
                color: "var(--fg1)",
              }}
            >
              AgentMux
            </span>
            <span style={{ color: "var(--fg4)", fontSize: 12, margin: "0 8px" }}>
              ›
            </span>
            <span style={{ color: "var(--fg3)", display: "flex" }}>
              <IconFolder />
            </span>
            <span
              style={{
                font: `600 12.5px/1 ${FONT_SANS}`,
                color: "var(--fg2)",
                marginLeft: 7,
              }}
            >
              {activeWorkspace?.name ?? "—"}
            </span>
          </div>
          <div data-tauri-drag-region style={{ flex: 1, height: "100%" }} />
          <Hov
            tag="button"
            className="agentmux-theme-toggle"
            style={{
              height: 30,
              borderRadius: 7,
              border: 0,
              background: "transparent",
              cursor: "pointer",
              display: "flex",
              alignItems: "center",
              gap: 6,
              padding: "0 9px",
              color: "var(--fg2)",
              font: `600 11px/1 ${FONT_SANS}`,
              marginRight: 2,
            }}
            hover={iconBtnHover}
            onClick={() => setTheme(isDark ? "light" : "dark")}
          >
            {isDark ? <IconMoon /> : <IconSun />}
            {isDark ? t("appearance.dark") : t("appearance.light")}
          </Hov>
          {activeRootIsSplit ? (
            <Hov
              tag="button"
              ariaLabel={t("app.panes.balance")}
              title={t("app.panes.balance")}
              style={{ ...iconBtn, marginRight: 2 }}
              hover={iconBtnHover}
              onClick={balanceActivePanes}
            >
              <IconBalance />
            </Hov>
          ) : null}
          <Hov
            tag="button"
            ariaLabel={t("app.search.activeWindow")}
            title={`${t("app.search.activeWindow")} (⌘P)`}
            style={{ ...iconBtn, marginRight: 2 }}
            hover={iconBtnHover}
            onClick={() => setOverlay("search")}
          >
            <IconSearch />
          </Hov>
          <Hov
            tag="button"
            ariaLabel={t("app.commandPalette.open")}
            title={t("app.commandPalette.open")}
            style={{ ...iconBtn, marginRight: 2 }}
            hover={iconBtnHover}
            onClick={() => {
              setOverlay("palette");
              setQuery("");
            }}
          >
            <IconGrid />
          </Hov>
          <Hov
            tag="button"
            className="agentmux-settings-open"
            ariaLabel={t("app.settings.open")}
            title={t("app.settings.open")}
            style={iconBtn}
            hover={iconBtnHover}
            onClick={() => setOverlay("settings")}
          >
            <IconGear />
          </Hov>
          {/* window controls — frameless caption (decorations:false), flush to
              the top-right corner */}
          <div
            style={{
              display: "flex",
              alignSelf: "stretch",
              marginLeft: 6,
              marginRight: -10,
            }}
          >
            <Hov
              tag="button"
              ariaLabel={t("app.window.minimize")}
              title={t("app.window.minimize")}
              style={winCtlBtn}
              hover={winCtlBtnHover}
              onClick={minimizeWindow}
            >
              <IconWinMinimize />
            </Hov>
            <Hov
              tag="button"
              ariaLabel={
                windowMaximized
                  ? t("app.window.restore")
                  : t("app.window.maximize")
              }
              title={
                windowMaximized
                  ? t("app.window.restore")
                  : t("app.window.maximize")
              }
              style={winCtlBtn}
              hover={winCtlBtnHover}
              onClick={toggleMaximizeWindow}
            >
              {windowMaximized ? <IconWinRestore /> : <IconWinMaximize />}
            </Hov>
            <Hov
              tag="button"
              ariaLabel={t("app.window.close")}
              title={t("app.window.close")}
              style={winCtlBtn}
              hover={{ background: "#e81123", color: "#fff" }}
              onClick={closeWindow}
            >
              <IconClose />
            </Hov>
          </div>
        </div>

        {/* body */}
        <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
          {/* sidebar */}
          <div
            style={{
              width: sidebarCollapsed ? 0 : 236,
              flex: "none",
              background: "var(--surface)",
              borderRight: sidebarCollapsed ? "none" : "1px solid var(--border)",
              display: "flex",
              flexDirection: "column",
              overflow: "hidden",
              transition: "width 160ms ease",
            }}
          >
            {/* Sidebar search box removed — redundant with the titlebar search
               button. Workspace add (+) and group-create moved into the filter
               row below, per the requested layout. */}
            <div
              style={{
                font: `700 10px/1 ${FONT_SANS}`,
                letterSpacing: ".08em",
                textTransform: "uppercase",
                color: "var(--fg4)",
                padding: "8px 14px 6px",
              }}
            >
              {t("workspace.section")}
            </div>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                margin: "0 10px 7px",
              }}
            >
              <div
                className="agentmux-workspace-filter"
                style={{
                  flex: 1,
                  minWidth: 0,
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  padding: "6px 7px",
                  height: 32,
                  background: "var(--canvas)",
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  color: "var(--fg4)",
                }}
              >
              <IconSearch size={13} />
              <input
                className="agentmux-workspace-filter-input"
                aria-label={t("workspace.filter")}
                placeholder={t("workspace.filter")}
                value={workspaceFilterText}
                onChange={(event) =>
                  setWorkspaceFilterText(event.currentTarget.value)
                }
                onKeyDown={(event: ReactKeyboardEvent<HTMLInputElement>) => {
                  event.stopPropagation();
                  if (event.key === "Escape") {
                    setWorkspaceFilterText("");
                  }
                }}
                style={{
                  flex: 1,
                  minWidth: 0,
                  background: "transparent",
                  border: 0,
                  outline: "none",
                  color: "var(--fg1)",
                  font: `500 11px/1 ${FONT_SANS}`,
                }}
              />
              {workspaceFilterActive ? (
                <Hov
                  tag="button"
                  className="agentmux-workspace-filter-clear"
                  ariaLabel={t("common.clear")}
                  title={t("common.clear")}
                  style={{
                    width: 20,
                    height: 20,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    background: "transparent",
                    border: 0,
                    color: "var(--fg4)",
                    cursor: "pointer",
                    padding: 0,
                  }}
                  hover={{ color: "var(--fg1)" }}
                  onClick={() => setWorkspaceFilterText("")}
                >
                  <IconClose size={11} />
                </Hov>
              ) : null}
              </div>
              <Hov
                tag="button"
                className="agentmux-workspace-plus"
                ariaLabel={t("workspace.add")}
                title={t("workspace.add")}
                style={{
                  width: 32,
                  height: 32,
                  flex: "none",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  background: "var(--canvas)",
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  cursor: "pointer",
                  color: "var(--fg2)",
                }}
                hover={{
                  borderColor: "var(--border-strong)",
                  color: "var(--fg1)",
                }}
                onClick={() => {
                  runConfiguredAction(workspacePlusActionId, createWorkspace);
                }}
              >
                <IconPlus />
              </Hov>
              <Hov
                tag="button"
                className="agentmux-workspace-group-create"
                ariaLabel={t("workspace.createGroup")}
                title={t("workspace.createGroup")}
                style={{
                  width: 32,
                  height: 32,
                  flex: "none",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  background: "var(--canvas)",
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  cursor: "pointer",
                  color: "var(--fg2)",
                }}
                hover={{
                  borderColor: "var(--border-strong)",
                  color: "var(--fg1)",
                }}
                onClick={() => {
                  void createWorkspaceGroup();
                }}
              >
                <IconFolder />
              </Hov>
            </div>
            <div
              className="agentmux-scroll"
              style={{ flex: 1, overflow: "auto", padding: "0 8px 8px" }}
            >
              {selectedWorkspaceCount > 0 ? (
                <div
                  className="agentmux-workspace-selection-bar"
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 6,
                    margin: "3px 0 7px",
                    padding: "7px 8px",
                    background: "var(--accent-soft)",
                    border: "1px solid var(--border)",
                    borderRadius: 8,
                  }}
                >
                  <span
                    style={{
                      flex: 1,
                      minWidth: 0,
                      font: `700 11px/1 ${FONT_SANS}`,
                      color: "var(--fg1)",
                      whiteSpace: "nowrap",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                    }}
                  >
                    {t("workspace.selectedCount", {
                      count: selectedWorkspaceCount,
                    })}
                  </span>
                  <Hov
                    tag="button"
                    className="agentmux-workspace-selection-create-group"
                    ariaLabel={t("workspace.createGroupFromSelection")}
                    title={t("workspace.createGroupFromSelection")}
                    style={groupActionBtn}
                    hover={groupActionHover}
                    onClick={() => {
                      void createWorkspaceGroup();
                    }}
                  >
                    <IconFolder size={12} />
                  </Hov>
                  <Hov
                    tag="button"
                    className="agentmux-workspace-selection-clear"
                    ariaLabel={t("workspace.clearSelection")}
                    title={t("workspace.clearSelection")}
                    style={groupActionBtn}
                    hover={groupActionHover}
                    onClick={clearWorkspaceSelection}
                  >
                    <IconClose size={11} />
                  </Hov>
                </div>
              ) : null}
              {visibleWorkspaceGroupsView.map(
                ({ group, workspaces: groupedWorkspaces }) => (
                  <div
                    key={group.groupId}
                    className="agentmux-workspace-group"
                    data-agentmux-workspace-group={group.groupId}
                    draggable={!workspaceFilterActive}
                    onDragStart={(event) =>
                      beginWorkspaceGroupDrag(event, group)
                    }
                    onDragOver={allowWorkspaceGroupDrop}
                    onDrop={(event) => dropWorkspaceGroup(event, group)}
                    style={{ margin: "4px 0 7px" }}
                  >
                    <div
                      onContextMenu={(event) =>
                        openWorkspaceGroupMenu(event, group)
                      }
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 4,
                        minHeight: 30,
                        background: group.pinned ? "var(--s2)" : "transparent",
                        border: "1px solid var(--border-subtle)",
                        borderRadius: 7,
                        padding: "3px 4px 3px 7px",
                      }}
                    >
                      <Hov
                        tag="button"
                        className="agentmux-workspace-group-toggle"
                        draggable={!workspaceFilterActive}
                        onDragStart={(event) =>
                          beginWorkspaceGroupDrag(event, group)
                        }
                        ariaLabel={`${group.collapsed ? "Expand" : "Collapse"} ${group.name}`}
                        title={group.name}
                        style={{
                          flex: 1,
                          minWidth: 0,
                          display: "flex",
                          alignItems: "center",
                          gap: 7,
                          height: 22,
                          background: "transparent",
                          border: 0,
                          color: "var(--fg3)",
                          cursor: workspaceFilterActive ? "pointer" : "grab",
                          padding: 0,
                          textAlign: "left",
                        }}
                        hover={{ color: "var(--fg1)" }}
                        onClick={() => toggleWorkspaceGroup(group)}
                      >
                        {group.collapsed ? (
                          <IconChevronRight size={13} />
                        ) : (
                          <IconChevronDown size={13} />
                        )}
                        <span
                          style={{
                            width: 7,
                            height: 7,
                            borderRadius: "50%",
                            background: group.color ?? "var(--accent)",
                            flex: "none",
                          }}
                        />
                        <span
                          style={{
                            flex: 1,
                            minWidth: 0,
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                            font: `700 11px/1 ${FONT_SANS}`,
                            color: "var(--fg2)",
                          }}
                        >
                          {group.name}
                        </span>
                        <span
                          style={{
                            font: `600 10px/1 ${FONT_MONO}`,
                            color: "var(--fg4)",
                          }}
                        >
                          {groupedWorkspaces.length}
                        </span>
                      </Hov>
                      <Hov
                        tag="button"
                        className="agentmux-workspace-group-new-workspace"
                        ariaLabel={`${group.name}: ${t("workspace.group.addWorkspace")}`}
                        title={t("workspace.group.addWorkspace")}
                        style={groupActionBtn}
                        hover={groupActionHover}
                        onClick={(event) => {
                          event.stopPropagation();
                          void createWorkspaceInGroup(group);
                        }}
                      >
                        <IconPlus size={12} />
                      </Hov>
                      <Hov
                        tag="button"
                        className="agentmux-workspace-group-move-up"
                        ariaLabel={`${group.name}: ${t("workspace.group.moveUp")}`}
                        title={t("workspace.group.moveUp")}
                        style={groupActionBtn}
                        hover={groupActionHover}
                        onClick={(event) => {
                          event.stopPropagation();
                          void moveWorkspaceGroup(group, -1);
                        }}
                      >
                        <IconChevronUp size={12} />
                      </Hov>
                      <Hov
                        tag="button"
                        className="agentmux-workspace-group-move-down"
                        ariaLabel={`${group.name}: ${t("workspace.group.moveDown")}`}
                        title={t("workspace.group.moveDown")}
                        style={groupActionBtn}
                        hover={groupActionHover}
                        onClick={(event) => {
                          event.stopPropagation();
                          void moveWorkspaceGroup(group, 1);
                        }}
                      >
                        <IconChevronDown size={12} />
                      </Hov>
                      {(
                        selectedWorkspaceCount > 0
                          ? selectedWorkspaces.some(
                              (workspace) =>
                                !group.members.some(
                                  (member) =>
                                    member.workspaceId ===
                                    workspace.workspaceId,
                                ),
                            )
                          : activeWorkspaceId &&
                            !group.members.some(
                              (member) =>
                                member.workspaceId === activeWorkspaceId,
                            )
                      ) ? (
                        <Hov
                          tag="button"
                          className={
                            selectedWorkspaceCount > 0
                              ? "agentmux-workspace-group-add-selected"
                              : "agentmux-workspace-group-add-active"
                          }
                          ariaLabel={`${group.name}: ${
                            selectedWorkspaceCount > 0
                              ? t("workspace.addSelectedToGroup")
                              : t("workspace.addToGroup")
                          }`}
                          title={
                            selectedWorkspaceCount > 0
                              ? t("workspace.addSelectedToGroup")
                              : t("workspace.addToGroup")
                          }
                          style={groupActionBtn}
                          hover={groupActionHover}
                          onClick={(event) => {
                            event.stopPropagation();
                            addSelectedWorkspacesToGroup(group);
                          }}
                        >
                          <IconFolder size={12} />
                        </Hov>
                      ) : null}
                      <Hov
                        tag="button"
                        className="agentmux-workspace-group-pin"
                        ariaLabel={`${group.pinned ? "Unpin" : "Pin"} ${group.name}`}
                        title={group.pinned ? "고정 해제" : "고정"}
                        style={{
                          ...groupActionBtn,
                          color: group.pinned ? "var(--accent)" : "var(--fg4)",
                        }}
                        hover={groupActionHover}
                        onClick={(event) => {
                          event.stopPropagation();
                          toggleWorkspaceGroupPin(group);
                        }}
                      >
                        <IconChevronUp size={12} />
                      </Hov>
                      <Hov
                        tag="button"
                        className="agentmux-workspace-group-edit"
                        ariaLabel={`${group.name} 편집`}
                        title="그룹 편집"
                        style={groupActionBtn}
                        hover={groupActionHover}
                        onClick={(event) => {
                          event.stopPropagation();
                          void editWorkspaceGroup(group);
                        }}
                      >
                        <IconGear size={12} />
                      </Hov>
                      <Hov
                        tag="button"
                        className="agentmux-workspace-group-delete"
                        ariaLabel={`${group.name} 삭제`}
                        title="그룹 삭제"
                        style={groupActionBtn}
                        hover={{ ...groupActionHover, color: T.red }}
                        onClick={(event) => {
                          event.stopPropagation();
                          deleteWorkspaceGroup(group);
                        }}
                      >
                        <IconClose size={11} />
                      </Hov>
                    </div>
                    {!group.collapsed || workspaceFilterActive
                      ? groupedWorkspaces.map((ws, index) => (
                          <WorkspaceCard
                            key={ws.workspaceId}
                            ws={ws}
                            theme={T}
                            t={t}
                            active={ws.workspaceId === activeWorkspaceId}
                            attentionCount={
                              attentionByWorkspace.get(ws.workspaceId) ?? 0
                            }
                            sessionCount={
                              ws.workspaceId === activeWorkspaceId
                                ? sessions.length
                                : undefined
                            }
                            running={
                              ws.workspaceId === activeWorkspaceId &&
                              runningCount > 0
                            }
                            editing={editingWorkspaceId === ws.workspaceId}
                            selected={selectedWorkspaceIds.has(ws.workspaceId)}
                            draftName={workspaceNameDraft}
                            onDraftNameChange={setWorkspaceNameDraft}
                            onToggleSelected={() =>
                              toggleWorkspaceSelection(ws.workspaceId)
                            }
                            onMoveUp={
                              !workspaceFilterActive && index > 0
                                ? () => {
                                    void moveWorkspaceInGroup(
                                      group,
                                      ws.workspaceId,
                                      -1,
                                    );
                                  }
                                : undefined
                            }
                            onMoveDown={
                              !workspaceFilterActive &&
                              index < groupedWorkspaces.length - 1
                                ? () => {
                                    void moveWorkspaceInGroup(
                                      group,
                                      ws.workspaceId,
                                      1,
                                    );
                                  }
                                : undefined
                            }
                            draggable={!workspaceFilterActive}
                            onDragStart={(event) =>
                              beginWorkspaceMemberDrag(
                                event,
                                group,
                                ws.workspaceId,
                              )
                            }
                            onDragOver={(event) => {
                              allowWorkspaceMemberDrop(event);
                              allowWorkspaceCardDrop(event);
                            }}
                            onDrop={(event) => {
                              dropWorkspaceMember(event, group, ws.workspaceId);
                              dropWorkspaceCard(event, ws.workspaceId);
                            }}
                            onStartRename={() => startWorkspaceRename(ws)}
                            onCommitRename={commitWorkspaceRename}
                            onCancelRename={cancelWorkspaceRename}
                            onContextMenu={(event) =>
                              openWorkspaceMenu(event, ws)
                            }
                            onClick={() =>
                              void ctl.selectWorkspace(ws.workspaceId)
                            }
                          />
                        ))
                      : null}
                  </div>
                ),
              )}
              {visibleUngroupedWorkspaces.map((ws, index) => (
                <WorkspaceCard
                  key={ws.workspaceId}
                  ws={ws}
                  theme={T}
                  t={t}
                  active={ws.workspaceId === activeWorkspaceId}
                  attentionCount={attentionByWorkspace.get(ws.workspaceId) ?? 0}
                  sessionCount={
                    ws.workspaceId === activeWorkspaceId
                      ? sessions.length
                      : undefined
                  }
                  running={
                    ws.workspaceId === activeWorkspaceId && runningCount > 0
                  }
                  editing={editingWorkspaceId === ws.workspaceId}
                  selected={selectedWorkspaceIds.has(ws.workspaceId)}
                  draftName={workspaceNameDraft}
                  onDraftNameChange={setWorkspaceNameDraft}
                  onToggleSelected={() =>
                    toggleWorkspaceSelection(ws.workspaceId)
                  }
                  onMoveUp={
                    !workspaceFilterActive && index > 0
                      ? () => moveUngroupedWorkspace(ws.workspaceId, -1)
                      : undefined
                  }
                  onMoveDown={
                    !workspaceFilterActive &&
                    index < visibleUngroupedWorkspaces.length - 1
                      ? () => moveUngroupedWorkspace(ws.workspaceId, 1)
                      : undefined
                  }
                  draggable={!workspaceFilterActive}
                  onDragStart={(event) =>
                    beginWorkspaceCardDrag(event, ws.workspaceId)
                  }
                  onDragOver={allowWorkspaceCardDrop}
                  onDrop={(event) => dropWorkspaceCard(event, ws.workspaceId)}
                  onStartRename={() => startWorkspaceRename(ws)}
                  onCommitRename={commitWorkspaceRename}
                  onCancelRename={cancelWorkspaceRename}
                  onContextMenu={(event) => openWorkspaceMenu(event, ws)}
                  onClick={() => void ctl.selectWorkspace(ws.workspaceId)}
                />
              ))}
              {workspaceFilterActive && visibleWorkspaceCount === 0 ? (
                <div
                  className="agentmux-workspace-filter-empty"
                  style={{
                    font: `500 11px/1.5 ${FONT_SANS}`,
                    color: "var(--fg4)",
                    padding: "8px 8px",
                  }}
                >
                  No matching workspaces.
                </div>
              ) : null}
              {workspaces.length === 0 ? (
                <div
                  style={{
                    font: `400 11px/1.5 ${FONT_SANS}`,
                    color: "var(--fg4)",
                    padding: "6px 6px",
                  }}
                >
                  {t("workspace.none")}
                </div>
              ) : null}

              <SidebarMetadataPanel sidebarState={sidebarState} />
              <TeamCollaborationPanel
                tasks={teamTasks}
                messages={teamMessages}
                activeSessionId={activeTerminalSession?.sessionId ?? null}
                onFocusSession={focusSessionPane}
                onClaimTask={(taskId, sessionId) =>
                  void ctl.claimTeamTask(taskId, sessionId)
                }
                onCompleteTask={(taskId) => void ctl.completeTeamTask(taskId)}
                onUnblockTask={(taskId) => void ctl.unblockTeamTask(taskId)}
                onMarkMessageRead={(messageId) =>
                  void ctl.markTeamMessageRead(messageId)
                }
              />

              {SSH_UI_ENABLED ? (
                <>
                  <div
                    style={{
                      font: `700 10px/1 ${FONT_SANS}`,
                      letterSpacing: ".08em",
                      textTransform: "uppercase",
                      color: "var(--fg4)",
                      padding: "16px 6px 6px",
                    }}
                  >
                    원격 · SSH
                  </div>
                  {profiles.map((p) => (
                    <Hov
                      key={p.profileId}
                      title={`접속: ${p.user}@${p.host}`}
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 8,
                        margin: "1px 0",
                        padding: "7px 8px",
                        borderRadius: 7,
                        cursor: "pointer",
                        color: "var(--fg2)",
                      }}
                      hover={{ background: "var(--s2)" }}
                      onClick={() => void ctl.connectProfile(p)}
                    >
                      <span style={{ color: "var(--fg4)", display: "flex" }}>
                        <IconServer />
                      </span>
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ font: `500 12px/1.3 ${FONT_SANS}` }}>
                          {p.name}
                        </div>
                        <div
                          style={{
                            font: `400 10px/1.3 ${FONT_MONO}`,
                            color: "var(--fg4)",
                          }}
                        >
                          {p.user}@{p.host}
                        </div>
                      </div>
                      <span
                        style={{
                          width: 6,
                          height: 6,
                          borderRadius: "50%",
                          background: T.fg4,
                        }}
                      />
                    </Hov>
                  ))}
                  {profiles.length === 0 ? (
                    <div
                      style={{
                        font: `400 11px/1.5 ${FONT_SANS}`,
                        color: "var(--fg4)",
                        padding: "2px 8px",
                      }}
                    >
                      등록된 프로필이 없습니다.
                    </div>
                  ) : null}
                </>
              ) : null}
            </div>
            <Hov
              style={{
                flex: "none",
                borderTop: "1px solid var(--border)",
                padding: "9px 14px",
                display: "flex",
                alignItems: "center",
                gap: 8,
                cursor: "pointer",
                color: "var(--fg3)",
              }}
              hover={{ background: "var(--s2)", color: "var(--fg1)" }}
              onClick={() => setOverlay("settings")}
            >
              <IconGear size={14} />
              <span style={{ font: `500 12px/1 ${FONT_SANS}` }}>
                {t("common.settings")}
              </span>
            </Hov>
          </div>

          {workspaceGroupMenu && workspaceGroupMenuGroup
            ? (() => {
                const group = workspaceGroupMenuGroup;
                const canAddWorkspace =
                  selectedWorkspaceCount > 0
                    ? selectedWorkspaces.some(
                        (workspace) =>
                          !group.members.some(
                            (member) =>
                              member.workspaceId === workspace.workspaceId,
                          ),
                      )
                    : Boolean(
                        activeWorkspaceId &&
                        !group.members.some(
                          (member) => member.workspaceId === activeWorkspaceId,
                        ),
                      );
                return (
                  <>
                    <div
                      className="agentmux-workspace-group-menu-backdrop"
                      onClick={closeWorkspaceGroupMenu}
                      style={{ position: "fixed", inset: 0, zIndex: 58 }}
                    />
                    <div
                      className="agentmux-workspace-group-menu"
                      onClick={(event) => event.stopPropagation()}
                      onMouseDown={(event) => event.stopPropagation()}
                      style={{
                        position: "fixed",
                        left: workspaceGroupMenu.x,
                        top: workspaceGroupMenu.y,
                        width: 230,
                        zIndex: 59,
                        background: "var(--surface)",
                        border: "1px solid var(--border-strong)",
                        borderRadius: 8,
                        boxShadow: "0 18px 45px rgba(0,0,0,0.35)",
                        padding: 6,
                      }}
                    >
                      <div
                        style={{
                          padding: "7px 10px 8px",
                          borderBottom: "1px solid var(--border)",
                          marginBottom: 4,
                        }}
                      >
                        <div
                          style={{
                            font: `700 12px/1 ${FONT_SANS}`,
                            color: "var(--fg1)",
                            whiteSpace: "nowrap",
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                          }}
                        >
                          {group.name}
                        </div>
                        <div
                          style={{
                            font: `500 10px/1 ${FONT_MONO}`,
                            color: "var(--fg4)",
                            marginTop: 4,
                          }}
                        >
                          {group.members.length} workspaces
                        </div>
                      </div>
                      <Hov
                        tag="button"
                        className="agentmux-workspace-group-menu-new-workspace"
                        style={groupMenuItemStyle}
                        hover={groupMenuItemHover}
                        onClick={() => {
                          closeWorkspaceGroupMenu();
                          void createWorkspaceInGroup(group);
                        }}
                      >
                        <IconPlus size={12} />
                        그룹 안에 워크스페이스 추가
                      </Hov>
                      {canAddWorkspace ? (
                        <Hov
                          tag="button"
                          className="agentmux-workspace-group-menu-add"
                          style={groupMenuItemStyle}
                          hover={groupMenuItemHover}
                          onClick={() => {
                            closeWorkspaceGroupMenu();
                            addSelectedWorkspacesToGroup(group);
                          }}
                        >
                          <IconFolder size={12} />
                          {selectedWorkspaceCount > 0
                            ? "선택 워크스페이스 추가"
                            : "현재 워크스페이스 추가"}
                        </Hov>
                      ) : null}
                      <Hov
                        tag="button"
                        className="agentmux-workspace-group-menu-move-up"
                        style={groupMenuItemStyle}
                        hover={groupMenuItemHover}
                        onClick={() => {
                          closeWorkspaceGroupMenu();
                          void moveWorkspaceGroup(group, -1);
                        }}
                      >
                        <IconChevronUp size={12} />
                        위로 이동
                      </Hov>
                      <Hov
                        tag="button"
                        className="agentmux-workspace-group-menu-move-down"
                        style={groupMenuItemStyle}
                        hover={groupMenuItemHover}
                        onClick={() => {
                          closeWorkspaceGroupMenu();
                          void moveWorkspaceGroup(group, 1);
                        }}
                      >
                        <IconChevronDown size={12} />
                        아래로 이동
                      </Hov>
                      <Hov
                        tag="button"
                        className="agentmux-workspace-group-menu-pin"
                        style={groupMenuItemStyle}
                        hover={groupMenuItemHover}
                        onClick={() => {
                          closeWorkspaceGroupMenu();
                          toggleWorkspaceGroupPin(group);
                        }}
                      >
                        <IconChevronUp size={12} />
                        {group.pinned ? "고정 해제" : "고정"}
                      </Hov>
                      <Hov
                        tag="button"
                        className="agentmux-workspace-group-menu-edit"
                        style={groupMenuItemStyle}
                        hover={groupMenuItemHover}
                        onClick={() => {
                          closeWorkspaceGroupMenu();
                          void editWorkspaceGroup(group);
                        }}
                      >
                        <IconGear size={12} />
                        그룹 편집
                      </Hov>
                      <Hov
                        tag="button"
                        className="agentmux-workspace-group-menu-delete"
                        style={{ ...groupMenuItemStyle, color: T.red }}
                        hover={{ ...groupMenuItemHover, color: T.red }}
                        onClick={() => {
                          closeWorkspaceGroupMenu();
                          deleteWorkspaceGroup(group);
                        }}
                      >
                        <IconClose size={11} />
                        그룹 삭제
                      </Hov>
                    </div>
                  </>
                );
              })()
            : null}

          {workspaceMenu && workspaceMenuWorkspace
            ? (() => {
                const workspace = workspaceMenuWorkspace;
                return (
                  <>
                    <div
                      className="agentmux-workspace-menu-backdrop"
                      onClick={closeWorkspaceMenu}
                      style={{ position: "fixed", inset: 0, zIndex: 58 }}
                    />
                    <div
                      className="agentmux-workspace-menu"
                      onClick={(event) => event.stopPropagation()}
                      onMouseDown={(event) => event.stopPropagation()}
                      style={{
                        position: "fixed",
                        left: workspaceMenu.x,
                        top: workspaceMenu.y,
                        width: 224,
                        zIndex: 59,
                        background: "var(--surface)",
                        border: "1px solid var(--border-strong)",
                        borderRadius: 8,
                        boxShadow: "0 18px 45px rgba(0,0,0,0.35)",
                        padding: 6,
                      }}
                    >
                      <div
                        style={{
                          padding: "7px 10px 8px",
                          borderBottom: "1px solid var(--border)",
                          marginBottom: 4,
                        }}
                      >
                        <div
                          style={{
                            font: `700 12px/1 ${FONT_SANS}`,
                            color: "var(--fg1)",
                            whiteSpace: "nowrap",
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                          }}
                        >
                          {workspace.name}
                        </div>
                        {workspaceAnchorGroups.length > 0 ? (
                          <div
                            className="agentmux-workspace-menu-anchor-warning"
                            style={{
                              font: `500 10px/1.35 ${FONT_SANS}`,
                              color: T.warn,
                              marginTop: 5,
                            }}
                          >
                            {workspaceAnchorGroups.length}개 그룹 anchor
                          </div>
                        ) : (
                          <div
                            style={{
                              font: `500 10px/1 ${FONT_MONO}`,
                              color: "var(--fg4)",
                              marginTop: 4,
                            }}
                          >
                            {workspace.projectRoot ?? "No project root"}
                          </div>
                        )}
                      </div>
                      <Hov
                        tag="button"
                        className="agentmux-workspace-menu-rename"
                        style={groupMenuItemStyle}
                        hover={groupMenuItemHover}
                        onClick={() => {
                          closeWorkspaceMenu();
                          startWorkspaceRename(workspace);
                        }}
                      >
                        <IconGear size={12} />
                        이름 변경
                      </Hov>
                      <Hov
                        tag="button"
                        className="agentmux-workspace-menu-close"
                        style={{ ...groupMenuItemStyle, color: T.red }}
                        hover={{ ...groupMenuItemHover, color: T.red }}
                        onClick={() => {
                          closeWorkspaceMenu();
                          void closeWorkspaceFromMenu(workspace);
                        }}
                      >
                        <IconClose size={11} />
                        워크스페이스 닫기
                      </Hov>
                    </div>
                  </>
                );
              })()
            : null}

          {surfaceTabMenu && surfaceTabMenuSurface
            ? (() => {
                const surface = surfaceTabMenuSurface;
                return (
                  <>
                    <div
                      className="agentmux-surface-tab-menu-backdrop"
                      onClick={closeSurfaceTabMenu}
                      style={{ position: "fixed", inset: 0, zIndex: 58 }}
                    />
                    <div
                      className="agentmux-surface-tab-menu"
                      onClick={(event) => event.stopPropagation()}
                      onMouseDown={(event) => event.stopPropagation()}
                      style={{
                        position: "fixed",
                        left: surfaceTabMenu.x,
                        top: surfaceTabMenu.y,
                        width: 250,
                        maxHeight: "min(360px, calc(100vh - 16px))",
                        overflowY: "auto",
                        zIndex: 59,
                        background: "var(--surface)",
                        border: "1px solid var(--border-strong)",
                        borderRadius: 8,
                        boxShadow: "0 18px 45px rgba(0,0,0,0.35)",
                        padding: 6,
                      }}
                    >
                      <div
                        style={{
                          padding: "7px 10px 8px",
                          borderBottom: "1px solid var(--border)",
                          marginBottom: 4,
                        }}
                      >
                        <div
                          style={{
                            font: `700 12px/1 ${FONT_SANS}`,
                            color: "var(--fg1)",
                            whiteSpace: "nowrap",
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                          }}
                        >
                          {surface.title}
                        </div>
                        <div
                          style={{
                            font: `500 10px/1 ${FONT_MONO}`,
                            color: "var(--fg4)",
                            marginTop: 4,
                          }}
                        >
                          Tab actions
                        </div>
                      </div>
                      <Hov
                        tag="button"
                        className="agentmux-surface-tab-menu-split-right"
                        style={groupMenuItemStyle}
                        hover={groupMenuItemHover}
                        onClick={() => {
                          void splitSurfaceTabToPane(surface, "vertical");
                        }}
                      >
                        <IconSplitCols size={12} />
                        Split right
                      </Hov>
                      <Hov
                        tag="button"
                        className="agentmux-surface-tab-menu-split-down"
                        style={groupMenuItemStyle}
                        hover={groupMenuItemHover}
                        onClick={() => {
                          void splitSurfaceTabToPane(surface, "horizontal");
                        }}
                      >
                        <IconSplitRows size={12} />
                        Split down
                      </Hov>
                      <Hov
                        tag="button"
                        className="agentmux-surface-tab-menu-duplicate"
                        style={groupMenuItemStyle}
                        hover={groupMenuItemHover}
                        onClick={() => {
                          void duplicateSurfaceTab(surface);
                        }}
                      >
                        <IconDuplicate size={12} />
                        Duplicate tab
                      </Hov>
                      <div
                        style={{
                          padding: "8px 10px 5px",
                          borderTop: "1px solid var(--border)",
                          marginTop: 5,
                          color: "var(--fg4)",
                          font: `600 10px/1 ${FONT_MONO}`,
                          textTransform: "uppercase",
                        }}
                      >
                        Move to workspace
                      </div>
                      {workspaces.map((workspace) => {
                        const current = workspace.workspaceId === activeWorkspaceId;
                        return (
                          <Hov
                            key={workspace.workspaceId}
                            tag="button"
                            className="agentmux-surface-tab-menu-workspace"
                            style={{
                              ...groupMenuItemStyle,
                              opacity: current ? 0.45 : 1,
                              cursor: current ? "default" : "pointer",
                            }}
                            hover={current ? {} : groupMenuItemHover}
                            onClick={() => {
                              if (!current) {
                                void moveSurfaceTabToWorkspace(
                                  surface.surfaceId,
                                  workspace.workspaceId,
                                );
                              }
                            }}
                          >
                            <IconFolder size={12} />
                            {workspace.name}
                          </Hov>
                        );
                      })}
                    </div>
                  </>
                );
              })()
            : null}

          {/* right: tabs + mosaic */}
          <div
            style={{
              flex: 1,
              minWidth: 0,
              display: "flex",
              flexDirection: "column",
              background: "var(--canvas)",
            }}
          >
            <div
              style={{
                height: 38,
                flex: "none",
                display: "flex",
                alignItems: "stretch",
                background: "var(--surface)",
                borderBottom: "1px solid var(--border)",
                overflow: "hidden",
              }}
            >
              {tabSurfaces.map((surface, index) => {
                const host = paneHostingSurface(surface.surfaceId);
                // A tab stays active while ANY pane in its tab (root-pane tree)
                // is focused — not only the tab's first/host pane. Compare roots.
                const tabRoot = host ? rootPaneForPane(host) : undefined;
                const activePane = activePaneId
                  ? paneById.get(activePaneId)
                  : undefined;
                const activeRoot = activePane
                  ? rootPaneForPane(activePane)
                  : undefined;
                const on = Boolean(
                  tabRoot && activeRoot && tabRoot.paneId === activeRoot.paneId,
                );
                const session = surface.sessionId
                  ? sessionById.get(surface.sessionId)
                  : undefined;
                const att = session
                  ? Boolean(attentionBySession.get(session.sessionId))
                  : false;
                return (
                  <Hov
                    key={surface.surfaceId}
                    className="agentmux-surface-tab"
                    data-agentmux-surface-tab={surface.surfaceId}
                    draggable
                    onDragStart={(event) => beginSurfaceTabDrag(event, surface)}
                    onDragOver={allowSurfaceTabDrop}
                    onDrop={(event) => dropSurfaceTab(event, surface.surfaceId)}
                    onContextMenu={(event) => openSurfaceTabMenu(event, surface)}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 7,
                      padding: "0 11px 0 13px",
                      minWidth: 220,
                      maxWidth: 240,
                      borderRight: "1px solid var(--border-subtle)",
                      cursor: "pointer",
                      background: on ? "var(--canvas)" : "transparent",
                      boxShadow: on ? "inset 0 2px 0 var(--accent)" : "none",
                    }}
                    hover={on ? {} : { background: "var(--s2)" }}
                    onClick={() => {
                      if (host) void ctl.focusPane(host.paneId);
                      else void ctl.mountSurface(surface.surfaceId);
                    }}
                  >
                    <span
                      style={{
                        color: "var(--fg4)",
                        display: "flex",
                        flex: "none",
                      }}
                    >
                      {surface.surfaceType === "browser" ? (
                        <IconGrid size={12} />
                      ) : (
                        <IconShellArrow />
                      )}
                    </span>
                    <span
                      style={{
                        font: `500 12px/1 ${FONT_SANS}`,
                        color: on ? "var(--fg1)" : "var(--fg3)",
                        whiteSpace: "nowrap",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        flex: 1,
                        minWidth: 0,
                      }}
                    >
                        {surface.title}
                      </span>
                    <Hov
                      tag="span"
                      className="agentmux-surface-tab-move-left"
                      ariaLabel={`Move ${surface.title} left`}
                      title="Move tab left"
                      style={{
                        width: 17,
                        height: 17,
                        borderRadius: 5,
                        flex: "none",
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "center",
                        color: "var(--fg4)",
                        opacity: index > 0 ? 1 : 0.35,
                        cursor: index > 0 ? "pointer" : "default",
                      }}
                      hover={
                        index > 0
                          ? { background: "var(--s3)", color: "var(--fg1)" }
                          : {}
                      }
                      onClick={(e) => {
                        e.stopPropagation();
                        if (index > 0) {
                          moveSurfaceTabByDirection(surface.surfaceId, -1);
                        }
                      }}
                    >
                      <span style={{ display: "flex", transform: "rotate(180deg)" }}>
                        <IconChevronRight size={11} />
                      </span>
                    </Hov>
                    <Hov
                      tag="span"
                      className="agentmux-surface-tab-move-right"
                      ariaLabel={`Move ${surface.title} right`}
                      title="Move tab right"
                      style={{
                        width: 17,
                        height: 17,
                        borderRadius: 5,
                        flex: "none",
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "center",
                        color: "var(--fg4)",
                        opacity: index < tabSurfaces.length - 1 ? 1 : 0.35,
                        cursor:
                          index < tabSurfaces.length - 1 ? "pointer" : "default",
                      }}
                      hover={
                        index < tabSurfaces.length - 1
                          ? { background: "var(--s3)", color: "var(--fg1)" }
                          : {}
                      }
                      onClick={(e) => {
                        e.stopPropagation();
                        if (index < tabSurfaces.length - 1) {
                          moveSurfaceTabByDirection(surface.surfaceId, 1);
                        }
                      }}
                    >
                      <IconChevronRight size={11} />
                    </Hov>
                    <Hov
                      tag="span"
                      className="agentmux-surface-tab-workspace-menu"
                      ariaLabel={`Move ${surface.title} to workspace`}
                      title="Move tab to workspace"
                      style={{
                        width: 17,
                        height: 17,
                        borderRadius: 5,
                        flex: "none",
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "center",
                        color: surfaceTabMenu?.surfaceId === surface.surfaceId
                          ? "var(--accent)"
                          : "var(--fg4)",
                        cursor: "pointer",
                      }}
                      hover={{ background: "var(--s3)", color: "var(--fg1)" }}
                      onClick={(e) => openSurfaceTabMenu(e, surface)}
                    >
                      <IconChevronDown size={11} />
                    </Hov>
                    {att ? (
                      <span
                        style={{
                          width: 6,
                          height: 6,
                          borderRadius: "50%",
                          background: T.warn,
                          flex: "none",
                        }}
                      />
                    ) : null}
                    <Hov
                      tag="span"
                      className="agentmux-surface-tab-close"
                      ariaLabel={`Close ${surface.title}`}
                      title="Close tab"
                      style={{
                        width: 17,
                        height: 17,
                        borderRadius: 5,
                        flex: "none",
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "center",
                        color: "var(--fg4)",
                      }}
                      hover={{ background: "var(--s3)", color: "var(--fg1)" }}
                      onClick={(e) => {
                        e.stopPropagation();
                        void closeSurfaceTab(surface.surfaceId, host);
                      }}
                    >
                      <IconClose size={10} />
                    </Hov>
                  </Hov>
                );
              })}
              <Hov
                className="agentmux-new-terminal-tab"
                style={{
                  width: 34,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  cursor: terminalLaunchPending ? "wait" : "pointer",
                  color: "var(--fg3)",
                  opacity: terminalLaunchPending ? 0.55 : 1,
                }}
                hover={{ background: "var(--s2)", color: "var(--fg1)" }}
                onClick={() => {
                  if (terminalLaunchPending) {
                    return;
                  }
                  runConfiguredAction(surfaceTabPlusActionId, addTerminal);
                }}
              >
                <IconPlus size={14} />
              </Hov>
              <Hov
                tag="button"
                className="agentmux-terminal-profile-menu-button"
                ariaLabel="Choose terminal profile"
                title="Choose terminal profile"
                style={{
                  width: 24,
                  height: 36,
                  border: 0,
                  borderLeft: "1px solid var(--border)",
                  borderRadius: 0,
                  background: "transparent",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  cursor: terminalLaunchPending ? "wait" : "pointer",
                  color: terminalProfileMenu ? "var(--accent)" : "var(--fg4)",
                  opacity: terminalLaunchPending ? 0.55 : 1,
                  padding: 0,
                }}
                hover={{ background: "var(--s2)", color: "var(--fg1)" }}
                onClick={(event) => {
                  if (terminalLaunchPending) {
                    return;
                  }
                  openTerminalProfileMenu(event);
                }}
              >
                <IconChevronDown size={11} />
              </Hov>
              <div style={{ flex: 1 }} />
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 1,
                  padding: "0 8px",
                }}
              >
                {surfaceTabActions.map((action) => (
                  <Hov
                    key={action.id}
                    tag="span"
                    className={surfaceTabActionClassName(action.id)}
                    ariaLabel={action.title}
                    title={action.title}
                    style={{
                      width: 26,
                      height: 26,
                      borderRadius: 6,
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      color: "var(--fg4)",
                      cursor: "pointer",
                    }}
                    hover={{ background: "var(--s2)", color: "var(--fg1)" }}
                    onClick={() => {
                      void action.run();
                    }}
                  >
                    {surfaceTabActionIcon(action.id)}
                  </Hov>
                ))}
              </div>
            </div>

            {terminalProfileMenu ? (
              <>
                <div
                  className="agentmux-terminal-profile-menu-backdrop"
                  onClick={closeTerminalProfileMenu}
                  style={{ position: "fixed", inset: 0, zIndex: 58 }}
                />
                <div
                  className="agentmux-terminal-profile-menu"
                  onClick={(event) => event.stopPropagation()}
                  onMouseDown={(event) => event.stopPropagation()}
                  style={{
                    position: "fixed",
                    left: terminalProfileMenu.x,
                    top: terminalProfileMenu.y,
                    width: 300,
                    zIndex: 59,
                    background: "var(--surface)",
                    border: "1px solid var(--border-strong)",
                    borderRadius: 8,
                    boxShadow: "0 18px 45px rgba(0,0,0,0.35)",
                    padding: 6,
                  }}
                >
                  <div
                    style={{
                      padding: "8px 10px 9px",
                      borderBottom: "1px solid var(--border)",
                      marginBottom: 4,
                    }}
                  >
                    <div
                      style={{
                        font: `700 12px/1 ${FONT_SANS}`,
                        color: "var(--fg1)",
                      }}
                    >
                      {terminalProfileMenu.paneId ? "Open in pane" : "New terminal"}
                    </div>
                    <div
                      style={{
                        font: `500 10px/1 ${FONT_MONO}`,
                        color: "var(--fg4)",
                        marginTop: 4,
                      }}
                    >
                      {terminalProfileMenu.paneId
                        ? "Choose a shell for this split pane"
                        : "Choose a shell for this tab"}
                    </div>
                  </div>
                  {terminalProfileMenuItems.map((item) => (
                    <Hov
                      key={item.id}
                      tag="button"
                      className={`agentmux-terminal-profile-menu-item agentmux-terminal-profile-${item.profile}`}
                      style={{
                        ...groupMenuItemStyle,
                        padding: "8px 9px",
                        opacity: item.disabled ? 0.55 : 1,
                        cursor: item.disabled ? "default" : "pointer",
                      }}
                      hover={item.disabled ? undefined : groupMenuItemHover}
                      onClick={() => {
                        void addTerminalProfile(item);
                      }}
                    >
                      <span
                        aria-hidden="true"
                        style={{
                          width: 22,
                          height: 22,
                          borderRadius: 5,
                          flex: "none",
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "center",
                          background:
                            item.profile === "wsl"
                              ? "rgba(34,197,94,0.13)"
                              : "var(--accent-soft)",
                          color:
                            item.profile === "wsl" ? "#22C55E" : "var(--accent)",
                        }}
                      >
                        {item.profile === "wsl" ? (
                          <IconServer size={12} />
                        ) : (
                          <IconShellArrow size={12} />
                        )}
                      </span>
                      <span style={{ minWidth: 0, flex: 1 }}>
                        <span
                          style={{
                            display: "block",
                            color: "var(--fg1)",
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                          }}
                        >
                          {item.title}
                        </span>
                        <span
                          style={{
                            display: "block",
                            color: "var(--fg4)",
                            font: `500 10px/1.25 ${FONT_MONO}`,
                            marginTop: 3,
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                          }}
                        >
                          {item.description}
                        </span>
                      </span>
                    </Hov>
                  ))}
                </div>
              </>
            ) : null}

            {setupWarning ? (
              <div
                role="status"
                style={{
                  flex: "none",
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  padding: "8px 12px",
                  background: "var(--accent-soft)",
                  borderBottom: "1px solid var(--border)",
                  color: "var(--fg2)",
                  font: `500 12px/1.35 ${FONT_SANS}`,
                }}
              >
                <span style={{ color: T.warn, display: "flex", flex: "none" }}>
                  <IconBubble size={12} />
                </span>
                <span
                  style={{
                    minWidth: 0,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {setupWarning.message}
                </span>
                <button
                  type="button"
                  className="agentmux-setup-open"
                  onClick={() => setOverlay("setup")}
                  style={{
                    flex: "none",
                    background: "var(--accent)",
                    color: "#fff",
                    border: 0,
                    borderRadius: 7,
                    padding: "5px 10px",
                    cursor: "pointer",
                    font: `700 11px/1 ${FONT_SANS}`,
                  }}
                >
                  Setup
                </button>
              </div>
            ) : null}

            <div style={{ flex: 1, minHeight: 0, padding: 9, display: "flex" }}>
              {!ready ? (
                <div
                  style={{
                    flex: 1,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    color: "var(--fg4)",
                    font: `500 13px/1 ${FONT_SANS}`,
                  }}
                >
                  제어 플레인에 연결 중…
                </div>
              ) : rootPaneId ? (
                renderPane(rootPaneId)
              ) : (
                <div
                  style={{
                    flex: 1,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    color: "var(--fg4)",
                    font: `500 13px/1 ${FONT_SANS}`,
                  }}
                >
                  표시할 페인이 없습니다.
                </div>
              )}
            </div>
            {textBoxOpen ? (
              <TextBoxComposer
                draft={textBoxDraft}
                disabled={!activeTerminalSession}
                maxLines={textBoxMaxLines}
                onDraftChange={updateTextBoxDraft}
                onClose={() => setTextBoxOpen(false)}
                onSend={() => void sendTextBoxDraft()}
              />
            ) : null}
          </div>
          <DockPanel
            client={client}
            dock={dockConfig}
            workspaceId={activeWorkspaceId}
            trusted={dockTrusted}
            status={dockRunMessage}
            surfaceByControlId={dockSurfaceByControlId}
            sessionById={sessionById}
            activeSessionId={activeDockSessionId}
            terminalInnerMargin={terminalInnerMargin}
            fontSize={fontSize}
            onTrust={trustDock}
            onRun={runDockControl}
            onCloseSurface={(surfaceId) => void closeDockSurface(surfaceId)}
            onFocusSession={setActiveDockSessionId}
          />
        </div>

        {/* status bar */}
        <div
          style={{
            height: 27,
            flex: "none",
            display: "flex",
            alignItems: "center",
            padding: "0 12px",
            background: "var(--surface)",
            borderTop: "1px solid var(--border)",
            fontFamily: FONT_MONO,
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              color: "var(--fg3)",
            }}
          >
            <IconBranch size={12} />
            <span
              className="agentmux-status-git"
              style={{ fontSize: 11, color: "var(--fg2)" }}
            >
              {gitStatusLabel}
            </span>
          </div>
          <div
            style={{
              width: 1,
              height: 13,
              background: "var(--border)",
              margin: "0 12px",
            }}
          />
          <span style={{ fontSize: 10.5, color: "var(--fg4)" }}>
            {sidebarState?.cwd ?? activeWorkspace?.projectRoot ?? ""}
          </span>
          <div style={{ flex: 1 }} />
          {teamTaskStats.total > 0 || unreadTeamMessageCount > 0 ? (
            <>
              <span
                title={`${teamTaskStats.blocked} blocked, ${teamTaskStats.claimed} claimed`}
                style={{ fontSize: 10.5, color: "var(--fg4)" }}
              >
                Tasks {teamTaskStats.completed}/{teamTaskStats.total}
                {unreadTeamMessageCount > 0
                  ? ` - Mail ${unreadTeamMessageCount}`
                  : ""}
              </span>
              <div
                style={{
                  width: 1,
                  height: 13,
                  background: "var(--border)",
                  margin: "0 12px",
                }}
              />
            </>
          ) : null}
          <span style={{ fontSize: 10.5, color: "var(--fg4)" }}>
            {t("statusbar.surfaceSummary", {
              surfaces: surfaces.length,
              terminals: terminalSurfaces.length,
              running: runningCount,
            })}
          </span>
          <div
            style={{
              width: 1,
              height: 13,
              background: "var(--border)",
              margin: "0 12px",
            }}
          />
          <span style={{ fontSize: 11, color: "var(--fg3)" }}>
            {activeSessionState?.backendKind ?? "agentmux"}
          </span>
          <div
            style={{
              width: 1,
              height: 13,
              background: "var(--border)",
              margin: "0 12px",
            }}
          />
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span
              style={{
                width: 7,
                height: 7,
                borderRadius: "50%",
                background: sessionDotColor(T, activeSessionState, false),
                animation: "pulse 1.6s ease-in-out infinite",
              }}
            />
            <span
              style={{
                fontSize: 11,
                color: sessionDotColor(T, activeSessionState, false),
              }}
            >
              {error
                ? t("common.invalid")
                : translatedSessionLabel(t, activeSessionState, false) ||
                  t("common.idle")}
            </span>
          </div>
        </div>

        {overlay === "palette" ? (
          <CommandPalette
            groups={groups}
            query={query}
            onQuery={(value) => {
              setQuery(value);
              setPaletteSelectedIndex(0);
            }}
            onClose={closeOverlay}
            onMoveSelection={movePaletteSelection}
            onRunSelected={runSelectedPaletteItem}
            stop={stop}
            t={t}
          />
        ) : null}
        {overlay === "search" ? (
          <SearchOverlay onClose={closeOverlay} t={t} />
        ) : null}
        {overlay === "setup" ? (
          <SetupModal
            activeWorkspace={activeWorkspace ?? null}
            wslDistributions={wslDistributions}
            setupWarning={setupWarning ?? null}
            tmuxProbe={tmuxProbe}
            tmuxProbeBusy={tmuxProbeBusy}
            onClose={closeOverlay}
            stop={stop}
            onRunTmuxProbe={(distribution) => void runTmuxProbe(distribution)}
            onUpdateWorkspace={(workspaceId, input) =>
              void ctl.updateWorkspace(workspaceId, input)
            }
          />
        ) : null}
        {overlay === "settings" ? (
          <SettingsModal
            isDark={isDark}
            language={language}
            accentKey={accentKey}
            fontSize={fontSize}
            terminalInnerMargin={terminalInnerMargin}
            terminalStartDirectory={terminalStartDirectory}
            terminalStartCustomCwd={terminalStartCustomCwd}
            terminalSplitBehavior={terminalSplitBehavior}
            settingsTab={settingsTab}
            notifications={notifications}
            updatesConfig={updatesConfig}
            updateState={updateState}
            configPath={configPath}
            projectConfigPath={projectConfigPath}
            projectConfigLoaded={projectConfigLoaded}
            configDiagnostics={configDiagnostics}
            configReloadMessage={configReloadMessage}
            tmuxProbe={tmuxProbe}
            tmuxProbeBusy={tmuxProbeBusy}
            profiles={profiles}
            activeWorkspace={activeWorkspace ?? null}
            wslDistributions={wslDistributions}
            actions={actions}
            notificationActionsFor={notificationActionsFor}
            shortcutBindings={shortcutBindings}
            shortcutEditMessage={shortcutEditMessage}
            onClose={closeOverlay}
            stop={stop}
            setSettingsTab={setSettingsTab}
            setLanguage={setLanguage}
            setTheme={setTheme}
            setAccentKey={setAccentKey}
            setFontSize={setFontSize}
            setTerminalInnerMargin={updateTerminalInnerMargin}
            setTerminalStartDirectory={updateTerminalStartDirectory}
            setTerminalStartCustomCwd={updateTerminalStartCustomCwd}
            setTerminalSplitBehavior={updateTerminalSplitBehavior}
            terminalLinkOpenMode={terminalLinkOpenMode}
            setTerminalLinkOpenMode={setTerminalLinkOpenMode}
            setAutoUpdateCheck={setAutoUpdateCheck}
            onDismissNotification={(id) => void ctl.dismissNotification(id)}
            onFocusNotificationSession={focusSessionPane}
            onRunNotificationAction={runNotificationAction}
            onReloadConfig={() => void reloadConfig()}
            onExportConfig={(scope) => void exportConfig(scope)}
            onImportConfig={(scope) => void importConfig(scope)}
            onResetConfig={(scope) => void resetConfig(scope)}
            onMigrateProjectConfig={() => void migrateProjectConfig()}
            onCheckForUpdates={() => void checkForUpdates()}
            onInstallUpdate={() => void installAvailableUpdate()}
            onUpdateShortcut={(actionId, binding) =>
              void updateShortcutBinding(actionId, binding)
            }
            onRunTmuxProbe={() => void runTmuxProbe()}
            onUpdateWorkspace={(workspaceId, input) =>
              void ctl.updateWorkspace(workspaceId, input)
            }
            onCreateProfile={(input) => void ctl.createProfile(input)}
            onUpdateProfile={(profileId, input) =>
              void ctl.updateProfile(profileId, input)
            }
            onDeleteProfile={(id) => void ctl.deleteProfile(id)}
            onConnectProfile={(profile) => {
              void ctl.connectProfile(profile);
              closeOverlay();
            }}
            t={t}
          />
        ) : null}
        {confirmDialog ? (
          <AppConfirmModal
            dialog={confirmDialog}
            onCancel={() => resolveConfirmDialog(false)}
            onConfirm={() => resolveConfirmDialog(true)}
            stop={stop}
          />
        ) : null}
      </div>
    </div>
  );
}

function TextBoxComposer({
  draft,
  disabled,
  maxLines,
  onDraftChange,
  onClose,
  onSend,
}: {
  draft: string;
  disabled: boolean;
  maxLines: number;
  onDraftChange: (value: string) => void;
  onClose: () => void;
  onSend: () => void;
}) {
  const inputRef = useRef<HTMLTextAreaElement | null>(null);
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const sendDisabled = disabled || draft.trim().length === 0;
  const onKeyDown = (event: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    event.stopPropagation();
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      if (!sendDisabled) {
        onSend();
      }
    }
  };
  const rows = Math.min(3, maxLines);
  const maxHeight = textBoxMaxHeight(maxLines);

  return (
    <div
      className="agentmux-textbox"
      onMouseDown={(event) => event.stopPropagation()}
      style={{
        flex: "none",
        margin: "0 9px 9px",
        background: "var(--surface)",
        border: "1px solid var(--border-strong)",
        borderRadius: 8,
        boxShadow: "0 14px 34px rgba(0,0,0,0.22)",
        overflow: "hidden",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "8px 10px",
          borderBottom: "1px solid var(--border)",
        }}
      >
        <span style={{ font: `700 11px/1 ${FONT_SANS}`, color: "var(--fg1)" }}>
          TextBox
        </span>
        <div style={{ flex: 1 }} />
        <Hov
          tag="button"
          className="agentmux-textbox-close"
          ariaLabel="Close TextBox"
          title="Close"
          style={{
            width: 24,
            height: 24,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            border: 0,
            borderRadius: 6,
            background: "transparent",
            color: "var(--fg4)",
            cursor: "pointer",
          }}
          hover={{ background: "var(--s2)", color: "var(--fg1)" }}
          onClick={onClose}
        >
          <IconClose size={11} />
        </Hov>
      </div>
      <div
        style={{ display: "flex", alignItems: "flex-end", gap: 8, padding: 10 }}
      >
        <textarea
          ref={inputRef}
          className="agentmux-textbox-input"
          data-agentmux-textbox-max-lines={maxLines}
          aria-label="TextBox draft"
          value={draft}
          placeholder="Draft terminal input"
          onChange={(event) => onDraftChange(event.currentTarget.value)}
          onKeyDown={onKeyDown}
          disabled={disabled}
          rows={rows}
          style={{
            flex: 1,
            minWidth: 0,
            maxHeight,
            resize: "vertical",
            background: "var(--canvas)",
            color: "var(--fg1)",
            border: "1px solid var(--border)",
            borderRadius: 7,
            outline: "none",
            padding: "9px 10px",
            font: `500 12px/1.45 ${FONT_MONO}`,
          }}
        />
        <button
          type="button"
          className="agentmux-textbox-send"
          disabled={sendDisabled}
          onClick={onSend}
          style={{
            flex: "none",
            minWidth: 72,
            height: 34,
            border: 0,
            borderRadius: 7,
            background: sendDisabled ? "var(--s3)" : "var(--accent)",
            color: sendDisabled ? "var(--fg4)" : "#fff",
            cursor: sendDisabled ? "default" : "pointer",
            font: `700 12px/1 ${FONT_SANS}`,
          }}
        >
          Send
        </button>
      </div>
    </div>
  );
}

function WorkspaceCard({
  ws,
  theme,
  t,
  active,
  attentionCount,
  sessionCount,
  running,
  editing,
  selected,
  draftName,
  onDraftNameChange,
  onToggleSelected,
  onMoveUp,
  onMoveDown,
  draggable,
  onDragStart,
  onDragOver,
  onDrop,
  onContextMenu,
  onStartRename,
  onCommitRename,
  onCancelRename,
  onClick,
}: {
  ws: WorkspaceSummary;
  theme: ThemeTokens;
  t: Translator;
  active: boolean;
  attentionCount: number;
  sessionCount?: number;
  running: boolean;
  editing: boolean;
  selected: boolean;
  draftName: string;
  onDraftNameChange: (name: string) => void;
  onToggleSelected: () => void;
  onMoveUp?: () => void;
  onMoveDown?: () => void;
  draggable?: boolean;
  onDragStart?: (event: ReactDragEvent<HTMLElement>) => void;
  onDragOver?: (event: ReactDragEvent<HTMLElement>) => void;
  onDrop?: (event: ReactDragEvent<HTMLElement>) => void;
  onContextMenu?: (event: ReactMouseEvent<HTMLElement>) => void;
  onStartRename: () => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  onClick: () => void;
}) {
  const needsInput = attentionCount > 0;
  const dot = needsInput ? "var(--accent)" : running ? "var(--accent)" : theme.fg4;
  const workspaceColor = ws.color?.trim() || "var(--accent)";
  const workspaceIcon = ws.icon?.trim() || ws.name.slice(0, 1).toUpperCase();
  const statusText = needsInput
    ? "에이전트가 입력을 기다리는 중"
    : running
      ? "세션 실행 중"
      : sessionCount !== undefined
        ? `${sessionCount} 세션`
        : "대기 중";
  const displayStatusText = needsInput
    ? t("workspace.status.needsInput")
    : running
      ? t("workspace.status.running")
      : sessionCount !== undefined
        ? t("workspace.status.sessionCount", { count: sessionCount })
        : t("workspace.status.idle");
  void statusText;
  const moveButtonStyle: CSSProperties = {
    width: 18,
    height: 18,
    border: "1px solid var(--border)",
    borderRadius: 5,
    background: "transparent",
    color: "var(--fg4)",
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    cursor: "pointer",
    padding: 0,
  };
  return (
    <Hov
      className="agentmux-workspace-card"
      data-agentmux-workspace={ws.workspaceId}
      data-agentmux-active={active ? "true" : "false"}
      data-agentmux-attention={needsInput ? "true" : "false"}
      draggable={draggable}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDrop={onDrop}
      onContextMenu={onContextMenu}
      style={{
        margin: "3px 0",
        padding: "10px 11px",
        borderRadius: 9,
        cursor: editing ? "default" : draggable ? "grab" : "pointer",
        background: active ? "var(--s2)" : "transparent",
        border: `1px solid ${
          needsInput ? "var(--accent)" : active ? workspaceColor : "var(--border-subtle)"
        }`,
        boxShadow: needsInput ? "0 0 0 3px rgba(88, 166, 255, 0.14)" : "none",
      }}
      hover={active || editing ? {} : { background: "var(--s2)" }}
      onClick={editing ? undefined : onClick}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 7 }}>
        <input
          type="checkbox"
          className="agentmux-workspace-select"
          aria-label={`${ws.name} 선택`}
          checked={selected}
          onChange={onToggleSelected}
          onClick={(event) => event.stopPropagation()}
          style={{
            width: 14,
            height: 14,
            flex: "none",
            accentColor: "var(--accent)",
            cursor: "pointer",
          }}
        />
        <span
          aria-hidden="true"
          style={{
            width: 20,
            height: 20,
            borderRadius: 6,
            flex: "none",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            background: workspaceColor,
            color: "#fff",
            font: `800 10px/1 ${FONT_SANS}`,
          }}
        >
          {workspaceIcon.slice(0, 2)}
        </span>
        <span
          style={{
            width: 8,
            height: 8,
            borderRadius: "50%",
            flex: "none",
            background: dot,
            boxShadow: needsInput || running ? "0 0 6px var(--accent)" : "none",
          }}
        />
        {editing ? (
          <input
            className="agentmux-workspace-inline-name-input"
            aria-label="워크스페이스 이름"
            autoFocus
            value={draftName}
            onBlur={onCommitRename}
            onChange={(event) => onDraftNameChange(event.currentTarget.value)}
            onClick={(event) => event.stopPropagation()}
            onFocus={(event) => event.currentTarget.select()}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                onCommitRename();
              } else if (event.key === "Escape") {
                event.preventDefault();
                onCancelRename();
              }
            }}
            style={{
              flex: 1,
              minWidth: 0,
              height: 22,
              background: "var(--canvas)",
              color: "var(--fg1)",
              border: "1px solid var(--accent)",
              borderRadius: 5,
              padding: "0 6px",
              font: `600 13px/1 ${FONT_SANS}`,
              outline: "none",
            }}
          />
        ) : (
          <span
            title="더블클릭해서 이름 변경"
            onDoubleClick={(event) => {
              event.stopPropagation();
              onStartRename();
            }}
            style={{
              font: `600 13px/1.2 ${FONT_SANS}`,
              color: "var(--fg1)",
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
              flex: 1,
            }}
          >
            {ws.name}
          </span>
        )}
        {sessionCount !== undefined ? (
          <span
            style={{
              font: `500 9.5px/1 ${FONT_MONO}`,
              color: "var(--fg4)",
              flex: "none",
              whiteSpace: "nowrap",
            }}
          >
            {sessionCount}
          </span>
        ) : null}
        {onMoveUp || onMoveDown ? (
          <span
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 2,
              flex: "none",
            }}
          >
            <button
              type="button"
              className="agentmux-workspace-member-move-up"
              aria-label={`${ws.name} 위로 이동`}
              title="워크스페이스 위로 이동"
              disabled={!onMoveUp}
              onClick={(event) => {
                event.stopPropagation();
                onMoveUp?.();
              }}
              style={{
                ...moveButtonStyle,
                opacity: onMoveUp ? 1 : 0.35,
                cursor: onMoveUp ? "pointer" : "default",
              }}
            >
              <IconChevronUp size={11} />
            </button>
            <button
              type="button"
              className="agentmux-workspace-member-move-down"
              aria-label={`${ws.name} 아래로 이동`}
              title="워크스페이스 아래로 이동"
              disabled={!onMoveDown}
              onClick={(event) => {
                event.stopPropagation();
                onMoveDown?.();
              }}
              style={{
                ...moveButtonStyle,
                opacity: onMoveDown ? 1 : 0.35,
                cursor: onMoveDown ? "pointer" : "default",
              }}
            >
              <IconChevronDown size={11} />
            </button>
          </span>
        ) : null}
      </div>
      {ws.description ? (
        <div
          style={{
            font: `400 10.5px/1.35 ${FONT_SANS}`,
            color: "var(--fg4)",
            marginTop: 5,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {ws.description}
        </div>
      ) : null}
      <div
        style={{
          font: `400 11px/1.35 ${FONT_SANS}`,
          color: needsInput ? "var(--accent)" : "var(--fg2)",
          marginTop: 5,
        }}
      >
        {displayStatusText}
      </div>
      {needsInput ? (
        <div
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 4,
            marginTop: 7,
            padding: "3px 7px",
            borderRadius: 5,
            background: "var(--accent-soft)",
          }}
        >
          <span style={{ color: "var(--accent)", display: "flex" }}>
            <IconBubble />
          </span>
          <span
            style={{
              font: `600 10px/1 ${FONT_SANS}`,
              color: "var(--accent)",
              whiteSpace: "nowrap",
            }}
          >
            입력 필요
          </span>
        </div>
      ) : null}
      {ws.projectRoot ? (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 5,
            marginTop: 7,
            font: `400 10px/1.2 ${FONT_MONO}`,
            color: "var(--fg4)",
          }}
        >
          <IconBranch size={10} />
          <span
            style={{
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
          >
            {ws.projectRoot}
          </span>
        </div>
      ) : null}
    </Hov>
  );
}

function SidebarMetadataPanel({
  sidebarState,
}: {
  sidebarState: SidebarState | null;
}) {
  const statuses = sidebarState?.statuses ?? [];
  const progress = sidebarState?.progress ?? null;
  const logs = sidebarState?.logs ?? [];
  if (statuses.length === 0 && !progress && logs.length === 0) {
    return null;
  }

  return (
    <div
      data-agentmux-sidebar-state
      style={{ margin: "12px 0 2px", padding: "0 6px" }}
    >
      <div
        style={{
          font: `700 10px/1 ${FONT_SANS}`,
          letterSpacing: ".08em",
          textTransform: "uppercase",
          color: "var(--fg4)",
          padding: "4px 0 7px",
        }}
      >
        상태
      </div>
      {statuses.length > 0 ? (
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: 6,
            marginBottom: progress || logs.length > 0 ? 8 : 0,
          }}
        >
          {statuses.map((status) => (
            <span
              key={status.key}
              title={status.key}
              style={{
                maxWidth: "100%",
                display: "inline-flex",
                alignItems: "center",
                gap: 5,
                padding: "4px 7px",
                borderRadius: 6,
                border: "1px solid var(--border)",
                background: "var(--s2)",
                color: status.color ?? "var(--fg2)",
                font: `600 10.5px/1 ${FONT_SANS}`,
              }}
            >
              {status.icon ? (
                <span style={{ flex: "none" }}>{status.icon}</span>
              ) : null}
              <span
                style={{
                  minWidth: 0,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {status.label}
              </span>
            </span>
          ))}
        </div>
      ) : null}
      {progress ? (
        <div style={{ marginBottom: logs.length > 0 ? 8 : 0 }}>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              gap: 8,
              marginBottom: 5,
            }}
          >
            <span
              style={{
                minWidth: 0,
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
                font: `500 11px/1 ${FONT_SANS}`,
                color: "var(--fg3)",
              }}
            >
              {progress.label ?? "progress"}
            </span>
            <span
              style={{ font: `600 10px/1 ${FONT_MONO}`, color: "var(--fg4)" }}
            >
              {Math.round(progress.value * 100)}%
            </span>
          </div>
          <div
            style={{
              height: 5,
              borderRadius: 999,
              background: "var(--s2)",
              overflow: "hidden",
            }}
          >
            <div
              style={{
                width: `${Math.round(progress.value * 100)}%`,
                height: "100%",
                background: "var(--accent)",
                borderRadius: 999,
              }}
            />
          </div>
        </div>
      ) : null}
      {logs.length > 0 ? (
        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          {logs.slice(0, 3).map((log) => (
            <div
              key={log.logId}
              style={{
                display: "grid",
                gridTemplateColumns: "auto 1fr",
                gap: 6,
                alignItems: "center",
                minWidth: 0,
              }}
            >
              <span
                style={{
                  font: `700 9px/1 ${FONT_MONO}`,
                  color: sidebarLogColor(log.level),
                  textTransform: "uppercase",
                }}
              >
                {log.level}
              </span>
              <span
                style={{
                  minWidth: 0,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                  font: `400 10.5px/1.3 ${FONT_SANS}`,
                  color: "var(--fg4)",
                }}
              >
                {log.source ? `${log.source}: ` : ""}
                {log.message}
              </span>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function TeamCollaborationPanel({
  tasks,
  messages,
  activeSessionId,
  onFocusSession,
  onClaimTask,
  onCompleteTask,
  onUnblockTask,
  onMarkMessageRead,
}: {
  tasks: TeamTask[];
  messages: TeamMessage[];
  activeSessionId: string | null;
  onFocusSession: (sessionId: string | null | undefined) => boolean;
  onClaimTask: (taskId: string, sessionId: string) => void;
  onCompleteTask: (taskId: string) => void;
  onUnblockTask: (taskId: string) => void;
  onMarkMessageRead: (messageId: string) => void;
}) {
  if (tasks.length === 0 && messages.length === 0) {
    return null;
  }
  const completed = tasks.filter((task) => task.status === "completed").length;
  const blocked = tasks.filter((task) => task.status === "blocked").length;
  const unread = messages.filter((message) => !message.readAt).length;
  const progress = tasks.length > 0 ? completed / tasks.length : 0;
  const visibleTasks = tasks
    .filter((task) => task.status !== "completed")
    .slice(0, 5);
  const visibleMessages = messages.slice(0, 4);

  return (
    <div
      data-agentmux-team-panel
      style={{ margin: "12px 0 2px", padding: "0 6px" }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 8,
          padding: "4px 0 7px",
        }}
      >
        <span
          style={{
            font: `700 10px/1 ${FONT_SANS}`,
            letterSpacing: ".08em",
            textTransform: "uppercase",
            color: "var(--fg4)",
          }}
        >
          Team
        </span>
        <span
          style={{ font: `600 10px/1 ${FONT_MONO}`, color: "var(--fg4)" }}
        >
          {tasks.length > 0 ? `${completed}/${tasks.length}` : "mailbox"}
        </span>
      </div>

      {tasks.length > 0 ? (
        <div style={{ marginBottom: 8 }}>
          <div
            style={{
              height: 5,
              borderRadius: 999,
              background: "var(--s2)",
              overflow: "hidden",
            }}
          >
            <div
              style={{
                width: `${Math.round(progress * 100)}%`,
                height: "100%",
                background: blocked > 0 ? "var(--warn, #FBBF24)" : "var(--accent)",
              }}
            />
          </div>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              gap: 8,
              marginTop: 6,
              font: `500 10px/1 ${FONT_SANS}`,
              color: "var(--fg4)",
            }}
          >
            <span>Tasks {completed}/{tasks.length}</span>
            {blocked > 0 ? <span>{blocked} blocked</span> : null}
          </div>
        </div>
      ) : null}

      {visibleTasks.length > 0 ? (
        <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
          {visibleTasks.map((task) => (
            <div
              key={task.taskId}
              data-agentmux-team-task={task.taskId}
              style={{
                border: "1px solid var(--border)",
                borderRadius: 7,
                background: "var(--s1)",
                padding: "7px 7px 6px",
              }}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  minWidth: 0,
                }}
              >
                <span
                  style={{
                    width: 7,
                    height: 7,
                    borderRadius: "50%",
                    flex: "none",
                    background: teamTaskStatusColor(task.status),
                  }}
                />
                <span
                  title={task.title}
                  style={{
                    minWidth: 0,
                    flex: 1,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                    font: `600 11px/1.25 ${FONT_SANS}`,
                    color: "var(--fg2)",
                  }}
                >
                  {task.title}
                </span>
                <span
                  style={{
                    flex: "none",
                    font: `600 9px/1 ${FONT_MONO}`,
                    color: teamTaskStatusColor(task.status),
                  }}
                >
                  {task.status}
                </span>
              </div>
              {task.blockedReason ? (
                <div
                  style={{
                    marginTop: 5,
                    font: `400 10px/1.25 ${FONT_SANS}`,
                    color: "var(--warn, #FBBF24)",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {task.blockedReason}
                </div>
              ) : null}
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 5,
                  marginTop: 6,
                }}
              >
                {task.assignedSessionId ? (
                  <button
                    type="button"
                    className="agentmux-team-button"
                    onClick={() => onFocusSession(task.assignedSessionId)}
                  >
                    {shortSessionId(task.assignedSessionId)}
                  </button>
                ) : activeSessionId ? (
                  <button
                    type="button"
                    className="agentmux-team-button"
                    onClick={() => onClaimTask(task.taskId, activeSessionId)}
                  >
                    Claim
                  </button>
                ) : null}
                {task.status === "blocked" ? (
                  <button
                    type="button"
                    className="agentmux-team-button"
                    onClick={() => onUnblockTask(task.taskId)}
                  >
                    Unblock
                  </button>
                ) : null}
                <button
                  type="button"
                  className="agentmux-team-button"
                  onClick={() => onCompleteTask(task.taskId)}
                >
                  Done
                </button>
              </div>
            </div>
          ))}
        </div>
      ) : null}

      {visibleMessages.length > 0 ? (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 5,
            marginTop: visibleTasks.length > 0 ? 9 : 0,
          }}
        >
          <div
            style={{
              font: `600 10px/1 ${FONT_SANS}`,
              color: unread > 0 ? "var(--accent)" : "var(--fg4)",
            }}
          >
            Mailbox {unread > 0 ? `${unread} unread` : "all read"}
          </div>
          {visibleMessages.map((message) => {
            const focusSessionId = message.toSessionId ?? message.fromSessionId;
            return (
              <div
                key={message.messageId}
                data-agentmux-team-message={message.messageId}
                style={{
                  border: "1px solid var(--border)",
                  borderRadius: 7,
                  background: message.readAt ? "var(--s1)" : "var(--accent-soft)",
                  padding: "7px",
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 6,
                    marginBottom: 4,
                  }}
                >
                  <span
                    style={{
                      width: 7,
                      height: 7,
                      borderRadius: "50%",
                      flex: "none",
                      background: message.readAt ? "var(--fg4)" : "var(--accent)",
                    }}
                  />
                  <span
                    style={{
                      minWidth: 0,
                      flex: 1,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                      font: `600 10px/1 ${FONT_MONO}`,
                      color: "var(--fg3)",
                    }}
                  >
                    {message.kind}
                    {focusSessionId ? ` - ${shortSessionId(focusSessionId)}` : ""}
                  </span>
                </div>
                <div
                  title={message.body}
                  style={{
                    font: `400 10.5px/1.35 ${FONT_SANS}`,
                    color: "var(--fg2)",
                    display: "-webkit-box",
                    WebkitLineClamp: 2,
                    WebkitBoxOrient: "vertical",
                    overflow: "hidden",
                  }}
                >
                  {message.body}
                </div>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 5,
                    marginTop: 6,
                  }}
                >
                  {focusSessionId ? (
                    <button
                      type="button"
                      className="agentmux-team-button"
                      onClick={() => onFocusSession(focusSessionId)}
                    >
                      Focus
                    </button>
                  ) : null}
                  {!message.readAt ? (
                    <button
                      type="button"
                      className="agentmux-team-button"
                      onClick={() => onMarkMessageRead(message.messageId)}
                    >
                      Read
                    </button>
                  ) : null}
                </div>
              </div>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}

function teamTaskStatusColor(status: string): string {
  if (status === "completed") return "var(--ok, #4ADE80)";
  if (status === "blocked") return "var(--warn, #FBBF24)";
  if (status === "claimed") return "var(--accent)";
  return "var(--fg4)";
}

function shortSessionId(sessionId: string | null | undefined): string {
  if (!sessionId) return "session";
  return sessionId.length > 10 ? sessionId.slice(-8) : sessionId;
}

function DockPanel({
  client,
  dock,
  workspaceId,
  trusted,
  status,
  surfaceByControlId,
  sessionById,
  activeSessionId,
  terminalInnerMargin,
  fontSize,
  onTrust,
  onRun,
  onCloseSurface,
  onFocusSession,
}: {
  client: ControlClient;
  dock: DockConfig | null;
  workspaceId: string | null;
  trusted: boolean;
  status: string;
  surfaceByControlId: Map<string, SurfaceSummary>;
  sessionById: Map<string, TerminalSession>;
  activeSessionId: string | null;
  terminalInnerMargin: number;
  fontSize: number;
  onTrust: () => void;
  onRun: (control: DockControl) => void;
  onCloseSurface: (surfaceId: string) => void;
  onFocusSession: (sessionId: string) => void;
}) {
  const controls = dock?.controls ?? [];
  const dockIdentity = dockStorageIdentity(workspaceId, dock);
  const controlIds = controls.map((control) => control.id).join("\u0000");
  const [heightOverrides, setHeightOverrides] = useState<
    Record<string, number>
  >({});

  useEffect(() => {
    const next: Record<string, number> = {};
    for (const control of controls) {
      const stored = readDockHeightOverride(dockIdentity, control.id);
      if (stored !== null) {
        next[control.id] = stored;
      }
    }
    setHeightOverrides(next);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dockIdentity, controlIds]);

  if (controls.length === 0) {
    return null;
  }
  const locked = Boolean(dock?.requiresTrust && !trusted);
  const updateDockHeight = (controlId: string, value: number) => {
    const height = clampDockHeight(value);
    setHeightOverrides((current) => ({ ...current, [controlId]: height }));
    writeDockHeightOverride(dockIdentity, controlId, height);
  };
  return (
    <aside
      className="agentmux-dock-panel"
      aria-label="Dock"
      style={{
        width: 280,
        flex: "none",
        display: "flex",
        flexDirection: "column",
        minHeight: 0,
        background: "var(--surface)",
        borderLeft: "1px solid var(--border)",
      }}
    >
      <div
        style={{
          height: 38,
          flex: "none",
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "0 10px",
          borderBottom: "1px solid var(--border)",
        }}
      >
        <span
          style={{
            font: `800 11px/1 ${FONT_SANS}`,
            color: "var(--fg1)",
            letterSpacing: ".04em",
            textTransform: "uppercase",
          }}
        >
          Dock
        </span>
        <span
          className="agentmux-dock-source"
          style={{
            minWidth: 0,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            font: `600 9.5px/1 ${FONT_MONO}`,
            color: "var(--fg4)",
            background: "var(--s2)",
            border: "1px solid var(--border)",
            borderRadius: 5,
            padding: "3px 5px",
          }}
        >
          {dockSourceLabel(dock?.source)}
        </span>
        {dock?.requiresTrust ? (
          <span
            className="agentmux-dock-trust"
            style={{
              flex: "none",
              font: `700 9px/1 ${FONT_MONO}`,
              color: trusted ? "#4ADE80" : "var(--accent)",
              background: trusted
                ? "rgba(74,222,128,.10)"
                : "var(--accent-soft)",
              border: "1px solid var(--border)",
              borderRadius: 5,
              padding: "3px 5px",
            }}
          >
            {trusted ? "trusted" : "review"}
          </span>
        ) : null}
        <div style={{ flex: 1 }} />
        {dock?.requiresTrust && !trusted ? (
          <button
            type="button"
            className="agentmux-dock-trust-approve"
            onClick={onTrust}
            style={{
              flex: "none",
              border: 0,
              borderRadius: 6,
              background: "var(--accent)",
              color: "#fff",
              padding: "5px 8px",
              cursor: "pointer",
              font: `800 10px/1 ${FONT_SANS}`,
            }}
          >
            Trust
          </button>
        ) : null}
      </div>
      {status ? (
        <div
          className="agentmux-dock-status"
          role="status"
          style={{
            flex: "none",
            padding: "7px 10px",
            borderBottom: "1px solid var(--border)",
            color: "var(--fg3)",
            font: `500 11px/1.3 ${FONT_SANS}`,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {status}
        </div>
      ) : null}
      <div
        className="agentmux-scroll"
        style={{ flex: 1, minHeight: 0, overflow: "auto", padding: 8 }}
      >
        {controls.map((control) => {
          const surface = surfaceByControlId.get(control.id);
          const session = surface?.sessionId
            ? sessionById.get(surface.sessionId)
            : undefined;
          const terminalHeight =
            heightOverrides[control.id] ?? clampDockHeight(control.height);
          return (
            <div
              key={control.id}
              className="agentmux-dock-control"
              data-agentmux-dock-control={control.id}
              style={{
                marginBottom: 8,
                border: "1px solid var(--border)",
                borderRadius: 8,
                background: "var(--canvas)",
                overflow: "hidden",
              }}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  padding: "8px 9px",
                  borderBottom: "1px solid var(--border)",
                }}
              >
                <span
                  style={{
                    flex: 1,
                    minWidth: 0,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                    font: `700 12px/1 ${FONT_SANS}`,
                    color: "var(--fg1)",
                  }}
                >
                  {control.title}
                </span>
                {control.height ? (
                  <span
                    style={{
                      flex: "none",
                      font: `600 9px/1 ${FONT_MONO}`,
                      color: "var(--fg4)",
                      background: "var(--s2)",
                      borderRadius: 4,
                      padding: "3px 5px",
                    }}
                  >
                    {control.height}px
                  </span>
                ) : null}
                <button
                  type="button"
                  className="agentmux-dock-run"
                  aria-label={`${session ? "Restart" : "Run"} ${control.title}`}
                  disabled={locked}
                  onClick={() => onRun(control)}
                  style={{
                    width: 23,
                    height: 23,
                    flex: "none",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    border: "1px solid var(--border)",
                    borderRadius: 6,
                    background: locked ? "var(--s2)" : "var(--accent)",
                    color: locked ? "var(--fg4)" : "#fff",
                    cursor: locked ? "not-allowed" : "pointer",
                  }}
                >
                  <IconShellArrow size={11} />
                </button>
                {surface ? (
                  <button
                    type="button"
                    className="agentmux-dock-close"
                    aria-label={`Close ${control.title}`}
                    onClick={() => onCloseSurface(surface.surfaceId)}
                    style={{
                      width: 23,
                      height: 23,
                      flex: "none",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      border: "1px solid var(--border)",
                      borderRadius: 6,
                      background: "var(--s2)",
                      color: "var(--fg4)",
                      cursor: "pointer",
                    }}
                  >
                    <IconClose size={10} />
                  </button>
                ) : null}
              </div>
              <div
                style={{
                  padding: 9,
                  display: "flex",
                  flexDirection: "column",
                  gap: 7,
                }}
              >
                <div
                  className="agentmux-dock-command"
                  style={{
                    font: `500 11px/1.35 ${FONT_MONO}`,
                    color: "var(--fg2)",
                    whiteSpace: "pre-wrap",
                    overflowWrap: "anywhere",
                  }}
                >
                  {control.command}
                </div>
                <div style={{ display: "flex", gap: 5, flexWrap: "wrap" }}>
                  {control.cwd ? (
                    <span
                      style={{
                        maxWidth: "100%",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                        font: `600 9px/1 ${FONT_MONO}`,
                        color: "var(--fg4)",
                        background: "var(--s2)",
                        borderRadius: 4,
                        padding: "3px 5px",
                      }}
                    >
                      {control.cwd}
                    </span>
                  ) : null}
                  {Object.keys(control.env).length > 0 ? (
                    <span
                      style={{
                        font: `600 9px/1 ${FONT_MONO}`,
                        color: "var(--fg4)",
                        background: "var(--s2)",
                        borderRadius: 4,
                        padding: "3px 5px",
                      }}
                    >
                      {Object.keys(control.env).length} env
                    </span>
                  ) : null}
                </div>
                {session ? (
                  <div
                    style={{
                      display: "grid",
                      gridTemplateColumns: "1fr auto",
                      gap: 7,
                      alignItems: "center",
                    }}
                  >
                    <input
                      className="agentmux-dock-height"
                      aria-label={`Resize ${control.title}`}
                      type="range"
                      min={130}
                      max={420}
                      step={10}
                      value={terminalHeight}
                      onInput={(event) =>
                        updateDockHeight(
                          control.id,
                          Number(event.currentTarget.value),
                        )
                      }
                      onChange={(event) =>
                        updateDockHeight(
                          control.id,
                          Number(event.currentTarget.value),
                        )
                      }
                      style={{ width: "100%", accentColor: "var(--accent)" }}
                    />
                    <span
                      className="agentmux-dock-height-value"
                      style={{
                        font: `600 9px/1 ${FONT_MONO}`,
                        color: "var(--fg4)",
                        minWidth: 36,
                        textAlign: "right",
                      }}
                    >
                      {terminalHeight}px
                    </span>
                  </div>
                ) : null}
              </div>
              {session && isLiveSession(session) ? (
                <div
                  className="agentmux-dock-terminal"
                  data-agentmux-dock-terminal={control.id}
                  style={{
                    height: terminalHeight,
                    borderTop: "1px solid var(--border)",
                    background: "#050505",
                  }}
                >
                  <LiveTerminal
                    client={client}
                    sessionId={session.sessionId}
                    active={activeSessionId === session.sessionId}
                    innerMargin={terminalInnerMargin}
                    fontSize={fontSize}
                    onFocus={() => onFocusSession(session.sessionId)}
                  />
                </div>
              ) : null}
            </div>
          );
        })}
      </div>
    </aside>
  );
}

function dockSourceLabel(source: string | undefined): string {
  switch (source) {
    case "project_agentmux":
      return ".agentmux";
    case "project_cmux":
      return ".cmux";
    case "global_agentmux":
      return "global";
    case "global_cmux":
      return "cmux global";
    default:
      return "none";
  }
}

function clampDockHeight(value: number | null | undefined): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return 180;
  }
  return Math.min(420, Math.max(130, Math.round(value / 10) * 10));
}

function textBoxDraftStorageKey(sessionId: string): string {
  return `agentmux.textbox.draft.v1.${encodeURIComponent(sessionId)}`;
}

function clampTextBoxMaxLines(value: number | null | undefined): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return TEXT_BOX_DEFAULT_MAX_LINES;
  }
  return Math.min(
    TEXT_BOX_MAX_LINES,
    Math.max(TEXT_BOX_MIN_LINES, Math.round(value)),
  );
}

function clampTerminalInnerMargin(value: number | null | undefined): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return TERMINAL_INNER_MARGIN_DEFAULT;
  }
  return Math.min(
    TERMINAL_INNER_MARGIN_MAX,
    Math.max(TERMINAL_INNER_MARGIN_MIN, Math.round(value)),
  );
}

function textBoxMaxHeight(maxLines: number): number {
  return Math.ceil(maxLines * 17.4 + 20);
}

function readTextBoxDraft(key: string | null): string {
  if (!key) {
    return "";
  }
  try {
    return window.localStorage.getItem(key) ?? "";
  } catch {
    return "";
  }
}

function writeTextBoxDraft(key: string | null, value: string): void {
  if (!key) {
    return;
  }
  try {
    if (value.length === 0) {
      window.localStorage.removeItem(key);
    } else {
      window.localStorage.setItem(key, value);
    }
  } catch {
    // Draft persistence should never block terminal input.
  }
}

function clearTextBoxDraft(key: string | null): void {
  if (!key) {
    return;
  }
  try {
    window.localStorage.removeItem(key);
  } catch {
    // Ignore storage failures; the sent text has already reached the session.
  }
}

function dockStorageIdentity(
  workspaceId: string | null,
  dock: DockConfig | null,
): string | null {
  if (!workspaceId || !dock) {
    return null;
  }
  return [workspaceId, dock.source, dock.configPath ?? ""].join("|");
}

function dockHeightStorageKey(
  identity: string | null,
  controlId: string,
): string | null {
  if (!identity || !controlId.trim()) {
    return null;
  }
  return `agentmux.dock.height.v1.${encodeURIComponent(`${identity}|${controlId}`)}`;
}

function readDockHeightOverride(
  identity: string | null,
  controlId: string,
): number | null {
  const key = dockHeightStorageKey(identity, controlId);
  if (!key) {
    return null;
  }
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) {
      return null;
    }
    const value = Number(raw);
    return Number.isFinite(value) ? clampDockHeight(value) : null;
  } catch {
    return null;
  }
}

function writeDockHeightOverride(
  identity: string | null,
  controlId: string,
  height: number,
): void {
  const key = dockHeightStorageKey(identity, controlId);
  if (!key) {
    return;
  }
  try {
    window.localStorage.setItem(key, String(clampDockHeight(height)));
  } catch {
    // Height persistence should not block the terminal session.
  }
}

function sidebarLogColor(level: string): string {
  switch (level) {
    case "success":
      return "#4ADE80";
    case "warning":
      return "#FBBF24";
    case "error":
      return "#F87171";
    case "progress":
      return "var(--accent)";
    default:
      return "var(--fg4)";
  }
}

function Bar() {
  return <span style={{ color: "var(--border-strong)" }}>│</span>;
}

function OmcBar({
  telemetry,
  theme,
}: {
  telemetry: AgentTelemetry;
  theme: ThemeTokens;
}) {
  const activity = telemetry.activity ?? undefined;
  const activityColor =
    activity === "thinking"
      ? theme.info
      : activity === "building"
        ? theme.warn
        : activity === "done"
          ? theme.green
          : "var(--accent)";
  const parts: ReactNode[] = [];
  const push = (node: ReactNode) => {
    if (parts.length > 0) {
      parts.push(<Bar key={`bar-${parts.length}`} />);
    }
    parts.push(node);
  };
  push(
    <span key="omc" style={{ color: "var(--fg4)" }}>
      [OMC]
    </span>,
  );
  if (activity) {
    push(
      <span key="activity" style={{ color: activityColor, fontWeight: 600 }}>
        {activity}
      </span>,
    );
  }
  if (telemetry.session)
    push(
      <span key="session" style={{ color: "var(--fg3)" }}>
        session:{telemetry.session}
      </span>,
    );
  if (telemetry.cost)
    push(
      <span key="cost" style={{ color: "var(--fg3)" }}>
        {telemetry.cost}
      </span>,
    );
  if (telemetry.tokens)
    push(
      <span key="tokens" style={{ color: "var(--fg3)" }}>
        {telemetry.tokens}
      </span>,
    );
  if (telemetry.cache)
    push(
      <span key="cache" style={{ color: "var(--fg3)" }}>
        Cache: {telemetry.cache}
      </span>,
    );
  if (telemetry.rate)
    push(
      <span key="rate" style={{ color: "var(--fg3)" }}>
        {telemetry.rate}
      </span>,
    );
  if (telemetry.ctx)
    push(
      <span key="ctx" style={{ color: "var(--accent)" }}>
        ctx:{telemetry.ctx}
      </span>,
    );

  return (
    <div
      style={{
        flex: "none",
        display: "flex",
        alignItems: "center",
        gap: 7,
        flexWrap: "wrap",
        padding: "6px 12px",
        borderTop: "1px solid var(--border-subtle)",
        background: "var(--surface)",
        fontFamily: FONT_MONO,
        fontSize: 11,
      }}
    >
      {parts}
      <div style={{ flex: 1 }} />
      <span style={{ color: "var(--fg4)" }}>/rc active</span>
    </div>
  );
}

function CommandPalette({
  groups,
  query,
  onQuery,
  onClose,
  onMoveSelection,
  onRunSelected,
  stop,
  t,
}: {
  groups: PaletteGroup[];
  query: string;
  onQuery: (value: string) => void;
  onClose: () => void;
  onMoveSelection: (delta: number) => void;
  onRunSelected: () => void;
  stop: (e: { stopPropagation: () => void }) => void;
  t: Translator;
}) {
  const onKeyDown = (event: ReactKeyboardEvent) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      event.stopPropagation();
      onMoveSelection(1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      event.stopPropagation();
      onMoveSelection(-1);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      onRunSelected();
    }
  };

  return (
    <div
      onClick={onClose}
      style={{
        position: "absolute",
        inset: 0,
        background: "rgba(0,0,0,0.5)",
        display: "flex",
        justifyContent: "center",
        paddingTop: 88,
        zIndex: 40,
        animation: "fadein .12s ease",
      }}
    >
      <div
        onClick={stop}
        onKeyDown={onKeyDown}
        style={{
          width: 620,
          maxWidth: "90%",
          height: "max-content",
          maxHeight: 440,
          background: "var(--surface)",
          border: "1px solid var(--border-strong)",
          borderRadius: 13,
          boxShadow: "0 30px 80px rgba(0,0,0,0.45)",
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 11,
            padding: "15px 17px",
            borderBottom: "1px solid var(--border)",
          }}
        >
          <span style={{ color: "var(--fg4)", display: "flex" }}>
            <IconSearch size={17} />
          </span>
          <input
            value={query}
            onChange={(e) => onQuery(e.target.value)}
            autoFocus
            placeholder={t("app.commandPalette.placeholder")}
            style={{
              flex: 1,
              border: 0,
              outline: "none",
              background: "transparent",
              font: `400 15px/1 ${FONT_SANS}`,
              color: "var(--fg1)",
            }}
          />
          <span
            style={{
              font: `600 10px/1 ${FONT_MONO}`,
              background: "var(--s2)",
              border: "1px solid var(--border)",
              borderRadius: 4,
              padding: "4px 6px",
              color: "var(--fg3)",
            }}
          >
            esc
          </span>
        </div>
        <div
          className="agentmux-scroll"
          style={{ flex: 1, overflow: "auto", padding: 7 }}
        >
          {groups.map((g) => (
            <div key={g.label}>
              <div
                style={{
                  font: `700 9.5px/1 ${FONT_SANS}`,
                  letterSpacing: ".09em",
                  textTransform: "uppercase",
                  color: "var(--fg4)",
                  padding: "9px 10px 5px",
                }}
              >
                {g.label}
              </div>
              {g.items.map((it) => (
                <Hov
                  key={it.id}
                  className={`agentmux-palette-item${it.highlighted ? " agentmux-palette-item-selected" : ""}${it.disabled ? " agentmux-palette-item-disabled" : ""}`}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                    padding: "9px 11px",
                    borderRadius: 8,
                    cursor: it.disabled ? "default" : "pointer",
                    opacity: it.disabled ? 0.55 : 1,
                    background: it.highlighted
                      ? "var(--accent-soft)"
                      : "transparent",
                    borderLeft: it.highlighted
                      ? "2px solid var(--accent)"
                      : "2px solid transparent",
                  }}
                  hover={{
                    background: it.disabled ? "transparent" : "var(--s2)",
                  }}
                  onClick={it.onClick}
                >
                  <IconChevronRight />
                  <span
                    style={{
                      flex: 1,
                      font: `500 13.5px/1 ${FONT_SANS}`,
                      color: "var(--fg1)",
                    }}
                  >
                    {it.title}
                  </span>
                  <span
                    style={{
                      font: `500 10.5px/1 ${FONT_MONO}`,
                      color: "var(--fg4)",
                    }}
                  >
                    {it.hint}
                  </span>
                </Hov>
              ))}
            </div>
          ))}
          {groups.length === 0 ? (
            <div
              style={{
                padding: "14px",
                color: "var(--fg4)",
                font: `400 13px/1 ${FONT_SANS}`,
              }}
            >
              {t("app.commandPalette.noResults")}
            </div>
          ) : null}
        </div>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 16,
            padding: "9px 15px",
            borderTop: "1px solid var(--border)",
            background: "var(--bg)",
            font: `500 10.5px/1 ${FONT_SANS}`,
            color: "var(--fg4)",
          }}
        >
          <span>{t("app.commandPalette.shortcutMove")}</span>
          <span>{t("app.commandPalette.shortcutRun")}</span>
          <span>{t("app.commandPalette.shortcutClose")}</span>
        </div>
      </div>
    </div>
  );
}

function collectShortcutConflicts(
  actions: ActionDescriptor[],
  bindings: ResolvedShortcutBindings,
): Array<{ key: string; label: string; actions: string[] }> {
  const byShortcut = new Map<string, { label: string; actions: string[] }>();
  for (const action of actions) {
    const binding = bindings[action.id];
    if (!binding) {
      continue;
    }
    const key = binding.strokes.join(" ");
    const current = byShortcut.get(key) ?? {
      label: binding.label,
      actions: [],
    };
    current.actions.push(action.id);
    byShortcut.set(key, current);
  }
  return Array.from(byShortcut.entries())
    .filter(([, value]) => value.actions.length > 1)
    .map(([key, value]) => ({
      key,
      label: value.label,
      actions: value.actions,
    }));
}

function SearchOverlay({
  onClose,
  t,
}: {
  onClose: () => void;
  t: Translator;
}) {
  const navBtn: CSSProperties = {
    width: 26,
    height: 26,
    borderRadius: 6,
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    color: "var(--fg3)",
    cursor: "pointer",
  };
  const navHover: CSSProperties = {
    background: "var(--s2)",
    color: "var(--fg1)",
  };
  return (
    <div
      style={{
        position: "absolute",
        top: 48,
        right: 18,
        width: 430,
        maxWidth: "80%",
        background: "var(--surface)",
        border: "1px solid var(--border-strong)",
        borderRadius: 9,
        boxShadow: "0 18px 44px rgba(0,0,0,0.35)",
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "9px 10px",
        zIndex: 40,
        animation: "fadein .12s ease",
      }}
    >
      <span style={{ color: "var(--fg4)", display: "flex" }}>
        <IconSearch size={15} />
      </span>
      <input
        autoFocus
        placeholder={t("app.search.placeholder")}
        style={{
          flex: 1,
          border: 0,
          outline: "none",
          background: "transparent",
          font: `400 13px/1 ${FONT_MONO}`,
          color: "var(--fg1)",
        }}
      />
      <div style={{ display: "flex", gap: 1 }}>
        <Hov tag="span" style={navBtn} hover={navHover}>
          <IconChevronUp />
        </Hov>
        <Hov tag="span" style={navBtn} hover={navHover}>
          <IconChevronDown />
        </Hov>
      </div>
      <div style={{ width: 1, height: 16, background: "var(--border)" }} />
      <Hov tag="span" style={navBtn} hover={navHover} onClick={onClose}>
        <IconClose />
      </Hov>
    </div>
  );
}

interface SettingsModalProps {
  isDark: boolean;
  language: AppLocaleLanguage;
  accentKey: string;
  fontSize: number;
  terminalInnerMargin: number;
  terminalStartDirectory: TerminalStartDirectory;
  terminalStartCustomCwd: string;
  terminalSplitBehavior: TerminalSplitBehavior;
  terminalLinkOpenMode: TerminalLinkOpenMode;
  settingsTab: SettingsTab;
  notifications: NotificationSummary[];
  updatesConfig: AppConfigUpdates;
  updateState: AppUpdateState;
  configPath: string;
  projectConfigPath: string | null;
  projectConfigLoaded: boolean;
  configDiagnostics: AppConfigDiagnosticEntry[];
  configReloadMessage: string;
  tmuxProbe: TmuxDiagnostics | null;
  tmuxProbeBusy: boolean;
  profiles: SshProfile[];
  activeWorkspace: WorkspaceSummary | null;
  wslDistributions: { name: string; isDefault: boolean }[];
  actions: ActionDescriptor[];
  notificationActionsFor: (
    notification: NotificationSummary,
  ) => NotificationActionBinding[];
  shortcutBindings: ResolvedShortcutBindings;
  shortcutEditMessage: string;
  onClose: () => void;
  stop: (e: { stopPropagation: () => void }) => void;
  setSettingsTab: (tab: SettingsTab) => void;
  setLanguage: (language: AppLocaleLanguage) => void;
  setTheme: (theme: ThemeName) => void;
  setAccentKey: (key: string) => void;
  setFontSize: (size: number) => void;
  setTerminalInnerMargin: (size: number) => void;
  setTerminalStartDirectory: (value: TerminalStartDirectory) => void;
  setTerminalStartCustomCwd: (value: string) => void;
  setTerminalSplitBehavior: (value: TerminalSplitBehavior) => void;
  setTerminalLinkOpenMode: (mode: TerminalLinkOpenMode) => void;
  setAutoUpdateCheck: (enabled: boolean) => void;
  onDismissNotification: (id: string) => void;
  onFocusNotificationSession: (sessionId: string | null | undefined) => boolean;
  onRunNotificationAction: (
    hook: AppConfigNotificationAction,
    notification: NotificationSummary,
  ) => void;
  onReloadConfig: () => void;
  onExportConfig: (scope?: AppConfigScope) => void;
  onImportConfig: (scope?: AppConfigScope) => void;
  onResetConfig: (scope?: AppConfigScope) => void;
  onMigrateProjectConfig: () => void;
  onCheckForUpdates: () => void;
  onInstallUpdate: () => void;
  onUpdateShortcut: (actionId: string, binding: ShortcutBindingValue) => void;
  onRunTmuxProbe: (distribution?: string | null) => void;
  onUpdateWorkspace: (workspaceId: string, input: WorkspaceUpdateInput) => void;
  onCreateProfile: (input: SshProfileInput) => void;
  onUpdateProfile: (profileId: string, input: SshProfileInput) => void;
  onDeleteProfile: (id: string) => void;
  onConnectProfile: (profile: SshProfile) => void;
  t: Translator;
}

interface SetupModalProps {
  activeWorkspace: WorkspaceSummary | null;
  wslDistributions: { name: string; isDefault: boolean }[];
  setupWarning: NotificationSummary | null;
  tmuxProbe: TmuxDiagnostics | null;
  tmuxProbeBusy: boolean;
  onClose: () => void;
  stop: (e: { stopPropagation: () => void }) => void;
  onRunTmuxProbe: (distribution?: string | null) => void;
  onUpdateWorkspace: (workspaceId: string, input: WorkspaceUpdateInput) => void;
}

function AppConfirmModal({
  dialog,
  onCancel,
  onConfirm,
  stop,
}: {
  dialog: AppConfirmDialog;
  onCancel: () => void;
  onConfirm: () => void;
  stop: (e: { stopPropagation: () => void }) => void;
}) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCancel();
      }
      if (event.key === "Enter") {
        event.preventDefault();
        onConfirm();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onCancel, onConfirm]);

  const danger = dialog.variant === "danger";
  const messageParts = dialog.message.split("\n").filter(Boolean);
  const detailParts = dialog.detail?.split("\n").filter(Boolean) ?? [];

  return (
    <div
      className="agentmux-confirm-backdrop"
      onMouseDown={onCancel}
      style={{
        position: "absolute",
        inset: 0,
        zIndex: 90,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: 20,
        background: "rgba(0,0,0,0.58)",
        backdropFilter: "blur(10px)",
        animation: "fadein .12s ease",
      }}
    >
      <div
        className={`agentmux-confirm-modal is-${dialog.variant}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="agentmux-confirm-title"
        onMouseDown={stop}
        style={{
          width: 430,
          maxWidth: "min(430px, 94vw)",
          background: "var(--surface)",
          border: "1px solid var(--border-strong)",
          borderRadius: 10,
          boxShadow: "0 28px 80px rgba(0,0,0,0.5)",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            display: "flex",
            gap: 12,
            padding: "18px 18px 14px",
            borderBottom: "1px solid var(--border)",
          }}
        >
          <div
            aria-hidden="true"
            style={{
              width: 34,
              height: 34,
              borderRadius: 8,
              flex: "none",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              background: danger
                ? "rgba(248,113,113,0.13)"
                : "var(--accent-soft)",
              border: danger
                ? "1px solid rgba(248,113,113,0.32)"
                : "1px solid var(--border)",
              color: danger ? "var(--red, #F87171)" : "var(--accent)",
              font: `800 18px/1 ${FONT_SANS}`,
            }}
          >
            !
          </div>
          <div style={{ minWidth: 0 }}>
            <div
              id="agentmux-confirm-title"
              className="agentmux-confirm-title"
              style={{
                color: "var(--fg1)",
                font: `700 14px/1.3 ${FONT_SANS}`,
              }}
            >
              {dialog.title}
            </div>
            <div
              className="agentmux-confirm-message"
              style={{
                marginTop: 8,
                color: "var(--fg3)",
                font: `400 12.5px/1.55 ${FONT_SANS}`,
              }}
            >
              {messageParts.map((part) => (
                <p key={part} style={{ margin: "0 0 6px" }}>
                  {part}
                </p>
              ))}
              {detailParts.length > 0 ? (
                <div
                  className="agentmux-confirm-detail"
                  style={{
                    marginTop: 10,
                    padding: "9px 10px",
                    borderRadius: 7,
                    border: "1px solid var(--border)",
                    background: "var(--canvas)",
                    color: danger ? "var(--fg2)" : "var(--fg4)",
                  }}
                >
                  {detailParts.map((part) => (
                    <p key={part} style={{ margin: "0 0 5px" }}>
                      {part}
                    </p>
                  ))}
                </div>
              ) : null}
            </div>
          </div>
        </div>
        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 8,
            padding: "12px 14px",
            background: "var(--canvas)",
          }}
        >
          <button
            type="button"
            className="agentmux-confirm-cancel"
            onClick={onCancel}
            autoFocus
            style={{
              height: 32,
              borderRadius: 7,
              border: "1px solid var(--border)",
              background: "var(--surface)",
              color: "var(--fg2)",
              cursor: "pointer",
              padding: "0 12px",
              font: `600 12px/1 ${FONT_SANS}`,
            }}
          >
            {dialog.cancelLabel ?? "Cancel"}
          </button>
          <button
            type="button"
            className="agentmux-confirm-confirm"
            onClick={onConfirm}
            style={{
              height: 32,
              borderRadius: 7,
              border: danger ? "1px solid rgba(248,113,113,0.42)" : 0,
              background: danger ? "var(--red, #EF4444)" : "var(--accent)",
              color: "#fff",
              cursor: "pointer",
              padding: "0 13px",
              font: `700 12px/1 ${FONT_SANS}`,
              boxShadow: danger
                ? "0 8px 22px rgba(239,68,68,0.24)"
                : "0 8px 22px rgba(37,99,235,0.22)",
            }}
          >
            {dialog.confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

function SetupModal(props: SetupModalProps) {
  const {
    activeWorkspace,
    wslDistributions,
    setupWarning,
    tmuxProbe,
    tmuxProbeBusy,
    onClose,
    stop,
    onRunTmuxProbe,
    onUpdateWorkspace,
  } = props;
  const preferredDistribution =
    activeWorkspace?.defaultWslDistribution ||
    wslDistributions.find((distribution) => distribution.isDefault)?.name ||
    wslDistributions[0]?.name ||
    "";
  const [selectedDistribution, setSelectedDistribution] = useState(
    preferredDistribution,
  );
  const [projectRootDraft, setProjectRootDraft] = useState(
    activeWorkspace?.projectRoot ?? "",
  );
  const [saveMessage, setSaveMessage] = useState("");

  useEffect(() => {
    setSelectedDistribution(preferredDistribution);
    setProjectRootDraft(activeWorkspace?.projectRoot ?? "");
    setSaveMessage("");
  }, [activeWorkspace, preferredDistribution]);

  const wslReady = wslDistributions.length > 0;
  const selectedTmuxProbe =
    tmuxProbe &&
    (!selectedDistribution ||
      !tmuxProbe.distribution ||
      tmuxProbe.distribution === selectedDistribution)
      ? tmuxProbe
      : null;
  const tmuxReady = Boolean(selectedTmuxProbe?.available);
  const rowStyle: CSSProperties = {
    border: "1px solid var(--border)",
    borderRadius: 8,
    padding: 14,
    background: "var(--surface)",
  };
  const labelStyle: CSSProperties = {
    display: "block",
    font: `700 11px/1 ${FONT_SANS}`,
    color: "var(--fg4)",
    textTransform: "uppercase",
    marginBottom: 7,
  };
  const inputStyle: CSSProperties = {
    width: "100%",
    background: "var(--canvas)",
    border: "1px solid var(--border)",
    borderRadius: 7,
    color: "var(--fg1)",
    padding: "8px 10px",
    outline: "none",
    font: `500 12px/1.35 ${FONT_SANS}`,
  };
  const codeStyle: CSSProperties = {
    display: "block",
    marginTop: 9,
    padding: "9px 10px",
    borderRadius: 7,
    background: "var(--canvas)",
    border: "1px solid var(--border)",
    color: "var(--fg2)",
    font: `500 11.5px/1.5 ${FONT_MONO}`,
    overflowWrap: "anywhere",
  };
  const statusPill = (label: string, ok: boolean, pending = false) => (
    <span
      style={{
        flex: "none",
        borderRadius: 999,
        padding: "4px 8px",
        background: pending
          ? "var(--s2)"
          : ok
            ? "rgba(16,185,129,0.14)"
            : "rgba(248,113,113,0.14)",
        color: pending
          ? "var(--fg3)"
          : ok
            ? "var(--green, #34D399)"
            : "var(--red, #F87171)",
        font: `700 10.5px/1 ${FONT_SANS}`,
      }}
    >
      {label}
    </span>
  );

  const saveSetup = () => {
    if (!activeWorkspace) {
      return;
    }
    onUpdateWorkspace(activeWorkspace.workspaceId, {
      name: activeWorkspace.name,
      projectRoot: projectRootDraft.trim() || null,
      environmentProfileId: activeWorkspace.environmentProfileId ?? null,
      description: activeWorkspace.description ?? null,
      icon: activeWorkspace.icon ?? null,
      color: activeWorkspace.color ?? null,
      defaultWslDistribution: selectedDistribution.trim() || null,
      defaultTerminalProfile: activeWorkspace.defaultTerminalProfile ?? "wsl",
      defaultAgentCommand: activeWorkspace.defaultAgentCommand ?? null,
    });
    setSaveMessage("Setup saved.");
  };

  return (
    <div
      onClick={onClose}
      style={{
        position: "absolute",
        inset: 0,
        background: "rgba(0,0,0,0.5)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 42,
        animation: "fadein .12s ease",
      }}
    >
      <div
        className="agentmux-setup-modal"
        onClick={stop}
        role="dialog"
        aria-label="Windows setup"
        style={{
          width: 840,
          maxWidth: "94%",
          height: 590,
          maxHeight: "90%",
          background: "var(--surface)",
          border: "1px solid var(--border-strong)",
          borderRadius: 13,
          boxShadow: "0 30px 80px rgba(0,0,0,0.45)",
          display: "grid",
          gridTemplateColumns: "220px minmax(0, 1fr)",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            background: "var(--bg)",
            borderRight: "1px solid var(--border)",
            padding: 18,
            display: "flex",
            flexDirection: "column",
            gap: 12,
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 9,
              color: "var(--fg1)",
              font: `800 17px/1.15 ${FONT_SANS}`,
            }}
          >
            <IconServer size={18} />
            Windows setup
          </div>
          <div
            style={{ color: "var(--fg4)", font: `500 12px/1.55 ${FONT_SANS}` }}
          >
            Prepare the Windows AgentMux runtime for WSL, tmux, workspace
            defaults, and cmux-style CLI checks.
          </div>
          <div style={{ display: "grid", gap: 8, marginTop: 4 }}>
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                gap: 8,
              }}
            >
              <span
                style={{ color: "var(--fg3)", font: `600 12px/1 ${FONT_SANS}` }}
              >
                WSL
              </span>
              {statusPill(wslReady ? "ready" : "missing", wslReady)}
            </div>
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                gap: 8,
              }}
            >
              <span
                style={{ color: "var(--fg3)", font: `600 12px/1 ${FONT_SANS}` }}
              >
                tmux
              </span>
              {selectedTmuxProbe
                ? statusPill(tmuxReady ? "ready" : "missing", tmuxReady)
                : statusPill("unchecked", false, true)}
            </div>
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                gap: 8,
              }}
            >
              <span
                style={{ color: "var(--fg3)", font: `600 12px/1 ${FONT_SANS}` }}
              >
                Workspace
              </span>
              {statusPill(
                activeWorkspace?.projectRoot ? "set" : "unset",
                Boolean(activeWorkspace?.projectRoot),
                !activeWorkspace,
              )}
            </div>
          </div>
          <div style={{ flex: 1 }} />
          <button
            type="button"
            className="agentmux-setup-save"
            onClick={saveSetup}
            disabled={!activeWorkspace}
            style={{
              width: "100%",
              background: activeWorkspace ? "var(--accent)" : "var(--s2)",
              color: activeWorkspace ? "#fff" : "var(--fg4)",
              border: 0,
              borderRadius: 8,
              padding: "9px 12px",
              cursor: activeWorkspace ? "pointer" : "default",
              font: `800 12px/1 ${FONT_SANS}`,
            }}
          >
            Save setup
          </button>
          {saveMessage ? (
            <div
              style={{
                color: "var(--accent)",
                font: `600 11.5px/1.35 ${FONT_SANS}`,
              }}
            >
              {saveMessage}
            </div>
          ) : null}
        </div>
        <div
          className="agentmux-scroll"
          style={{
            minWidth: 0,
            overflow: "auto",
            padding: "22px 26px",
            position: "relative",
          }}
        >
          <Hov
            tag="span"
            style={{
              position: "absolute",
              top: 15,
              right: 15,
              width: 28,
              height: 28,
              borderRadius: 7,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              color: "var(--fg3)",
              cursor: "pointer",
            }}
            hover={{ background: "var(--s2)", color: "var(--fg1)" }}
            onClick={onClose}
          >
            <IconClose size={14} />
          </Hov>
          <div
            style={{
              font: `800 20px/1 ${FONT_SANS}`,
              color: "var(--fg1)",
              marginBottom: 18,
            }}
          >
            First-run checklist
          </div>

          {setupWarning ? (
            <div
              style={{
                ...rowStyle,
                borderColor: "rgba(251,191,36,0.45)",
                background: "rgba(251,191,36,0.08)",
                marginBottom: 12,
              }}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  color: "var(--fg1)",
                  font: `700 12.5px/1.3 ${FONT_SANS}`,
                }}
              >
                <IconBubble size={13} />
                {setupWarning.title}
              </div>
              <div
                style={{
                  color: "var(--fg3)",
                  font: `500 12px/1.5 ${FONT_SANS}`,
                  marginTop: 6,
                }}
              >
                {setupWarning.message}
              </div>
            </div>
          ) : null}

          <div style={{ display: "grid", gap: 12 }}>
            <section style={rowStyle}>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: 12,
                  marginBottom: 12,
                }}
              >
                <div
                  style={{
                    color: "var(--fg1)",
                    font: `800 14px/1 ${FONT_SANS}`,
                  }}
                >
                  1. WSL distribution
                </div>
                {statusPill(wslReady ? "ready" : "install required", wslReady)}
              </div>
              <label>
                <span style={labelStyle}>Default distribution</span>
                <select
                  className="agentmux-setup-wsl-select"
                  value={selectedDistribution}
                  onChange={(event) =>
                    setSelectedDistribution(event.currentTarget.value)
                  }
                  disabled={!wslReady}
                  style={inputStyle}
                >
                  {wslReady ? null : (
                    <option value="">No distribution found</option>
                  )}
                  {wslDistributions.map((distribution) => (
                    <option key={distribution.name} value={distribution.name}>
                      {distribution.name}
                      {distribution.isDefault ? " (default)" : ""}
                    </option>
                  ))}
                </select>
              </label>
              {!wslReady ? <code style={codeStyle}>wsl --install</code> : null}
            </section>

            <section style={rowStyle}>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: 12,
                  marginBottom: 12,
                }}
              >
                <div
                  style={{
                    color: "var(--fg1)",
                    font: `800 14px/1 ${FONT_SANS}`,
                  }}
                >
                  2. tmux for durable panes
                </div>
                {selectedTmuxProbe
                  ? statusPill(
                      tmuxReady ? "ready" : "install required",
                      tmuxReady,
                    )
                  : statusPill("unchecked", false, true)}
              </div>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 10,
                  flexWrap: "wrap",
                }}
              >
                <button
                  type="button"
                  className="agentmux-setup-tmux-probe"
                  onClick={() => onRunTmuxProbe(selectedDistribution)}
                  disabled={!selectedDistribution || tmuxProbeBusy}
                  style={{
                    background: selectedDistribution
                      ? "var(--accent)"
                      : "var(--s2)",
                    color: selectedDistribution ? "#fff" : "var(--fg4)",
                    border: 0,
                    borderRadius: 8,
                    padding: "8px 12px",
                    cursor:
                      selectedDistribution && !tmuxProbeBusy
                        ? "pointer"
                        : "default",
                    opacity: tmuxProbeBusy ? 0.72 : 1,
                    font: `800 12px/1 ${FONT_SANS}`,
                  }}
                >
                  {tmuxProbeBusy ? "Checking..." : "Run tmux probe"}
                </button>
                <span
                  style={{
                    color: "var(--fg4)",
                    font: `500 11.5px/1.45 ${FONT_SANS}`,
                  }}
                >
                  {selectedTmuxProbe
                    ? selectedTmuxProbe.message
                    : "Probe the selected WSL distribution."}
                </span>
              </div>
              {selectedTmuxProbe && !tmuxReady ? (
                <code style={codeStyle}>
                  {selectedDistribution
                    ? `wsl -d ${selectedDistribution} -- sudo apt update && sudo apt install -y tmux`
                    : "sudo apt update && sudo apt install -y tmux"}
                </code>
              ) : null}
            </section>

            <section style={rowStyle}>
              <div
                style={{
                  color: "var(--fg1)",
                  font: `800 14px/1 ${FONT_SANS}`,
                  marginBottom: 12,
                }}
              >
                3. Workspace defaults
              </div>
              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "1fr 1fr",
                  gap: 12,
                }}
              >
                <label>
                  <span style={labelStyle}>Project root</span>
                  <input
                    className="agentmux-setup-root-input"
                    aria-label="Setup project root"
                    value={projectRootDraft}
                    onChange={(event) =>
                      setProjectRootDraft(event.currentTarget.value)
                    }
                    style={{ ...inputStyle, fontFamily: FONT_MONO }}
                  />
                </label>
                <label>
                  <span style={labelStyle}>Agent command</span>
                  <input
                    aria-label="Setup agent command"
                    value={activeWorkspace?.defaultAgentCommand ?? ""}
                    readOnly
                    style={{
                      ...inputStyle,
                      fontFamily: FONT_MONO,
                      color: "var(--fg4)",
                    }}
                  />
                </label>
              </div>
            </section>

            <section style={rowStyle}>
              <div
                style={{
                  color: "var(--fg1)",
                  font: `800 14px/1 ${FONT_SANS}`,
                  marginBottom: 10,
                }}
              >
                4. CLI smoke checks
              </div>
              <div
                style={{
                  color: "var(--fg4)",
                  font: `500 12px/1.5 ${FONT_SANS}`,
                }}
              >
                After install, the bundled sidecars should answer the same
                control pipe.
              </div>
              <code style={codeStyle}>cmux list-workspaces</code>
              <code style={codeStyle}>agentmux diagnostics</code>
            </section>
          </div>
        </div>
      </div>
    </div>
  );
}

function SettingsModal(props: SettingsModalProps) {
  const {
    isDark,
    language,
    accentKey,
    fontSize,
    terminalInnerMargin,
    terminalStartDirectory,
    terminalStartCustomCwd,
    terminalSplitBehavior,
    terminalLinkOpenMode,
    settingsTab,
    notifications,
    updatesConfig,
    updateState,
    configPath,
    projectConfigPath,
    projectConfigLoaded,
    configDiagnostics,
    configReloadMessage,
    tmuxProbe,
    tmuxProbeBusy,
    profiles,
    activeWorkspace,
    wslDistributions,
    actions,
    notificationActionsFor,
    shortcutBindings,
    shortcutEditMessage,
    onClose,
    stop,
    setSettingsTab,
    setLanguage,
    setTheme,
    setAccentKey,
    setFontSize,
    setTerminalInnerMargin,
    setTerminalStartDirectory,
    setTerminalStartCustomCwd,
    setTerminalSplitBehavior,
    setTerminalLinkOpenMode,
    setAutoUpdateCheck,
    onDismissNotification,
    onFocusNotificationSession,
    onRunNotificationAction,
    onReloadConfig,
    onExportConfig,
    onImportConfig,
    onResetConfig,
    onMigrateProjectConfig,
    onCheckForUpdates,
    onInstallUpdate,
    onUpdateShortcut,
    onRunTmuxProbe,
    onUpdateWorkspace,
    onCreateProfile,
    onUpdateProfile,
    onDeleteProfile,
    onConnectProfile,
    t,
  } = props;
  const [workspaceDraft, setWorkspaceDraft] = useState<WorkspaceUpdateInput>({
    name: activeWorkspace?.name ?? "",
    projectRoot: activeWorkspace?.projectRoot ?? "",
    environmentProfileId: activeWorkspace?.environmentProfileId ?? null,
    description: activeWorkspace?.description ?? "",
    icon: activeWorkspace?.icon ?? "",
    color: activeWorkspace?.color ?? ACCENTS[0].hex,
    defaultWslDistribution: activeWorkspace?.defaultWslDistribution ?? "",
    defaultTerminalProfile: activeWorkspace?.defaultTerminalProfile ?? "wsl",
    defaultAgentCommand: activeWorkspace?.defaultAgentCommand ?? "",
  });

  useEffect(() => {
    setWorkspaceDraft({
      name: activeWorkspace?.name ?? "",
      projectRoot: activeWorkspace?.projectRoot ?? "",
      environmentProfileId: activeWorkspace?.environmentProfileId ?? null,
      description: activeWorkspace?.description ?? "",
      icon: activeWorkspace?.icon ?? "",
      color: activeWorkspace?.color ?? ACCENTS[0].hex,
      defaultWslDistribution: activeWorkspace?.defaultWslDistribution ?? "",
      defaultTerminalProfile: activeWorkspace?.defaultTerminalProfile ?? "wsl",
      defaultAgentCommand: activeWorkspace?.defaultAgentCommand ?? "",
    });
  }, [activeWorkspace]);

  const updateWorkspaceDraft = (patch: Partial<WorkspaceUpdateInput>) => {
    setWorkspaceDraft((current) => ({ ...current, ...patch }));
  };

  const saveWorkspaceDraft = () => {
    if (!activeWorkspace) {
      return;
    }
    onUpdateWorkspace(activeWorkspace.workspaceId, {
      name: workspaceDraft.name.trim() || activeWorkspace.name,
      projectRoot: workspaceDraft.projectRoot?.trim() || null,
      environmentProfileId: workspaceDraft.environmentProfileId?.trim() || null,
      description: workspaceDraft.description?.trim() || null,
      icon: workspaceDraft.icon?.trim() || null,
      color: workspaceDraft.color?.trim() || null,
      defaultWslDistribution:
        workspaceDraft.defaultWslDistribution?.trim() || null,
      defaultTerminalProfile: workspaceDraft.defaultTerminalProfile ?? "wsl",
      defaultAgentCommand: workspaceDraft.defaultAgentCommand?.trim() || null,
    });
  };

  const visibleActions = actions.filter(
    (action) => action.visibleInPalette !== false,
  );
  const shortcutConflicts = collectShortcutConflicts(actions, shortcutBindings);
  const editShortcut = (action: ActionDescriptor) => {
    const current = shortcutLabelForAction(shortcutBindings, action.id);
    const raw = window.prompt(
      "Shortcut (examples: ctrl+shift+p or ctrl+b, c). Leave empty to clear.",
      current,
    );
    if (raw === null) {
      return;
    }
    const binding = parseShortcutBindingInput(raw);
    if (binding !== null && !normalizeShortcutBinding(binding)) {
      window.alert(
        "Invalid shortcut. Use a single shortcut or a two-step chord separated by a comma.",
      );
      return;
    }
    onUpdateShortcut(action.id, binding);
  };

  const promptNewProfile = () => {
    const name = window.prompt("프로필 이름")?.trim();
    if (!name) return;
    const host = window.prompt("호스트 (예: 10.0.0.1)")?.trim();
    if (!host) return;
    const user = window.prompt("사용자")?.trim();
    if (!user) return;
    onCreateProfile({ name, host, user, port: 22 });
  };
  const promptEditProfile = (profile: SshProfile) => {
    const name = window.prompt("프로필 이름", profile.name)?.trim();
    if (!name) return;
    const host = window.prompt("호스트", profile.host)?.trim();
    if (!host) return;
    const user = window.prompt("사용자", profile.user)?.trim();
    if (!user) return;
    const portText =
      window.prompt("포트", String(profile.port ?? 22))?.trim() ?? "";
    const port = Number.parseInt(portText, 10);
    onUpdateProfile(profile.profileId, {
      name,
      host,
      user,
      port: Number.isFinite(port) ? port : 22,
    });
  };
  const tabs: { key: SettingsTab; label: string }[] = [
    { key: "general", label: t("settings.tabs.general") },
    { key: "workspace", label: t("settings.tabs.workspace") },
    { key: "diagnostics", label: t("settings.tabs.diagnostics") },
    { key: "appearance", label: t("settings.tabs.appearance") },
    ...(SSH_UI_ENABLED
      ? ([{ key: "profiles", label: t("settings.tabs.profiles") }] as const)
      : []),
    { key: "keys", label: t("settings.tabs.keys") },
  ];
  const fieldLabel: CSSProperties = {
    display: "block",
    font: `600 11.5px/1 ${FONT_SANS}`,
    color: "var(--fg3)",
    marginBottom: 7,
  };
  const fieldInput: CSSProperties = {
    width: "100%",
    boxSizing: "border-box",
    background: "var(--canvas)",
    color: "var(--fg1)",
    border: "1px solid var(--border)",
    borderRadius: 7,
    padding: "8px 10px",
    font: `500 12px/1.35 ${FONT_SANS}`,
    outline: "none",
  };
  const updateProgress = updateProgressText(updateState);
  const updateStatusText =
    updateState.status === "available"
      ? t("updates.status.available", {
          version: updateState.version ?? "",
        })
      : updateState.status === "checking"
        ? t("updates.status.checking")
        : updateState.status === "downloading"
          ? t("updates.status.downloading", {
              progress: updateProgress || "...",
            })
          : updateState.status === "installed"
            ? t("updates.status.installed")
            : updateState.status === "not_available"
              ? t("updates.status.notAvailable")
              : updateState.status === "unsupported"
                ? t("updates.status.unsupported")
                : updateState.status === "error"
                  ? t("updates.status.error", {
                      message: updateState.message ?? "unknown",
                    })
                  : t("updates.status.idle");
  const updateBusy =
    updateState.status === "checking" || updateState.status === "downloading";

  return (
    <div
      onClick={onClose}
      style={{
        position: "absolute",
        inset: 0,
        background: "rgba(0,0,0,0.5)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 40,
        animation: "fadein .12s ease",
      }}
    >
      <div
        onClick={stop}
        style={{
          width: 780,
          maxWidth: "92%",
          height: 560,
          maxHeight: "88%",
          background: "var(--surface)",
          border: "1px solid var(--border-strong)",
          borderRadius: 13,
          boxShadow: "0 30px 80px rgba(0,0,0,0.45)",
          display: "flex",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            width: 188,
            flex: "none",
            background: "var(--bg)",
            borderRight: "1px solid var(--border)",
            padding: "16px 10px",
            display: "flex",
            flexDirection: "column",
            gap: 2,
          }}
        >
          <div
            style={{
              font: `700 14px/1 ${FONT_SANS}`,
              color: "var(--fg1)",
              padding: "4px 10px 14px",
            }}
          >
            {t("common.settings")}
          </div>
          {tabs.map((t) => {
            const on = settingsTab === t.key;
            return (
              <Hov
                key={t.key}
                className={`agentmux-settings-tab-${t.key}`}
                style={{
                  padding: "9px 11px",
                  borderRadius: 8,
                  cursor: "pointer",
                  font: `500 13px/1 ${FONT_SANS}`,
                  color: on ? "var(--fg1)" : "var(--fg3)",
                  background: on ? "var(--s2)" : "transparent",
                }}
                hover={on ? {} : { background: "var(--s2)" }}
                onClick={() => setSettingsTab(t.key)}
              >
                {t.label}
              </Hov>
            );
          })}
        </div>
        <div
          className="agentmux-scroll"
          style={{
            flex: 1,
            overflow: "auto",
            padding: "24px 28px",
            position: "relative",
          }}
        >
          <Hov
            tag="span"
            className="agentmux-settings-close"
            style={{
              position: "absolute",
              top: 16,
              right: 16,
              width: 28,
              height: 28,
              borderRadius: 7,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              color: "var(--fg3)",
              cursor: "pointer",
            }}
            hover={{ background: "var(--s2)", color: "var(--fg1)" }}
            onClick={onClose}
          >
            <IconClose size={14} />
          </Hov>

          {settingsTab === "appearance" ? (
            <>
              <div
                style={{
                  font: `700 18px/1 ${FONT_SANS}`,
                  color: "var(--fg1)",
                  marginBottom: 22,
                }}
              >
                {t("settings.appearance")}
              </div>
              <div
                style={{
                  font: `600 12px/1 ${FONT_SANS}`,
                  color: "var(--fg2)",
                  marginBottom: 8,
                }}
              >
                {t("settings.theme")}
              </div>
              <div style={{ display: "flex", gap: 10, marginBottom: 24 }}>
                <div
                  onClick={() => setTheme("dark")}
                  style={{
                    flex: 1,
                    border: `1px solid ${isDark ? "var(--accent)" : "var(--border)"}`,
                    borderRadius: 9,
                    padding: 12,
                    cursor: "pointer",
                    background: isDark ? "var(--accent-soft)" : "transparent",
                  }}
                >
                  <div
                    style={{
                      height: 48,
                      borderRadius: 6,
                      background: "#0B0B0D",
                      border: "1px solid #27272A",
                      marginBottom: 9,
                      display: "flex",
                      alignItems: "center",
                      padding: "0 8px",
                      gap: 5,
                    }}
                  >
                    <span
                      style={{
                        width: 7,
                        height: 7,
                        borderRadius: "50%",
                        background: "var(--accent)",
                      }}
                    />
                    <span
                      style={{
                        flex: 1,
                        height: 5,
                        borderRadius: 3,
                        background: "#27272A",
                      }}
                    />
                  </div>
                  <span
                    style={{
                      font: `600 12px/1 ${FONT_SANS}`,
                      color: "var(--fg1)",
                    }}
                  >
                    {t("appearance.dark")}
                  </span>
                </div>
                <div
                  onClick={() => setTheme("light")}
                  style={{
                    flex: 1,
                    border: `1px solid ${!isDark ? "var(--accent)" : "var(--border)"}`,
                    borderRadius: 9,
                    padding: 12,
                    cursor: "pointer",
                    background: !isDark ? "var(--accent-soft)" : "transparent",
                  }}
                >
                  <div
                    style={{
                      height: 48,
                      borderRadius: 6,
                      background: "#FFFFFF",
                      border: "1px solid #E4E4E7",
                      marginBottom: 9,
                      display: "flex",
                      alignItems: "center",
                      padding: "0 8px",
                      gap: 5,
                    }}
                  >
                    <span
                      style={{
                        width: 7,
                        height: 7,
                        borderRadius: "50%",
                        background: "var(--accent)",
                      }}
                    />
                    <span
                      style={{
                        flex: 1,
                        height: 5,
                        borderRadius: 3,
                        background: "#E4E4E7",
                      }}
                    />
                  </div>
                  <span
                    style={{
                      font: `600 12px/1 ${FONT_SANS}`,
                      color: "var(--fg1)",
                    }}
                  >
                    {t("appearance.light")}
                  </span>
                </div>
              </div>
              <div
                style={{
                  font: `600 12px/1 ${FONT_SANS}`,
                  color: "var(--fg2)",
                  marginBottom: 10,
                }}
              >
                {t("settings.accentColor")}
              </div>
              <div style={{ display: "flex", gap: 10, marginBottom: 24 }}>
                {ACCENTS.map((a) => (
                  <div
                    key={a.key}
                    onClick={() => setAccentKey(a.key)}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 8,
                      border: `1px solid ${a.key === accentKey ? "var(--accent)" : "var(--border)"}`,
                      borderRadius: 8,
                      padding: "8px 12px",
                      cursor: "pointer",
                    }}
                  >
                    <span
                      style={{
                        width: 18,
                        height: 18,
                        borderRadius: 6,
                        background: a.hex,
                      }}
                    />
                    <span
                      style={{
                        font: `500 12px/1 ${FONT_SANS}`,
                        color: "var(--fg1)",
                      }}
                    >
                      {a.label}
                    </span>
                  </div>
                ))}
              </div>
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  marginBottom: 9,
                }}
              >
                <span
                  style={{
                    font: `600 12px/1 ${FONT_SANS}`,
                    color: "var(--fg2)",
                  }}
                >
                  {t("settings.uiFontSize")}
                </span>
                <span
                  style={{
                    font: `600 12px/1 ${FONT_MONO}`,
                    color: "var(--accent)",
                  }}
                >
                  {fontSize}px
                </span>
              </div>
              <input
                type="range"
                min={11}
                max={16}
                step={0.5}
                value={fontSize}
                onChange={(e) => setFontSize(parseFloat(e.target.value))}
                style={{
                  width: "100%",
                  accentColor: "var(--accent)",
                  marginBottom: 24,
                }}
              />
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  marginBottom: 9,
                }}
              >
                <span
                  style={{
                    font: `600 12px/1 ${FONT_SANS}`,
                    color: "var(--fg2)",
                  }}
                >
                  {t("settings.terminalInnerMargin")}
                </span>
                <span
                  style={{
                    font: `600 12px/1 ${FONT_MONO}`,
                    color: "var(--accent)",
                  }}
                >
                  {terminalInnerMargin}px
                </span>
              </div>
              <input
                className="agentmux-terminal-inner-margin"
                aria-label={t("settings.terminalInnerMargin")}
                type="range"
                min={TERMINAL_INNER_MARGIN_MIN}
                max={TERMINAL_INNER_MARGIN_MAX}
                step={1}
                value={terminalInnerMargin}
                onChange={(e) =>
                  setTerminalInnerMargin(Number(e.currentTarget.value))
                }
                style={{
                  width: "100%",
                  accentColor: "var(--accent)",
                  marginBottom: 24,
                }}
              />
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  gap: 12,
                  marginBottom: 9,
                }}
              >
                <div style={{ minWidth: 0 }}>
                  <div
                    style={{
                      font: `600 12px/1 ${FONT_SANS}`,
                      color: "var(--fg2)",
                    }}
                  >
                    {t("settings.terminalStartDirectory")}
                  </div>
                  <div
                    style={{
                      font: `400 11.5px/1.45 ${FONT_SANS}`,
                      color: "var(--fg4)",
                      marginTop: 5,
                    }}
                  >
                    {t("settings.terminalStartDirectoryHint")}
                  </div>
                </div>
                <select
                  className="agentmux-terminal-start-directory"
                  aria-label={t("settings.terminalStartDirectory")}
                  value={terminalStartDirectory}
                  onChange={(e) =>
                    setTerminalStartDirectory(
                      e.currentTarget.value === "workspace"
                        ? "workspace"
                        : e.currentTarget.value === "custom"
                          ? "custom"
                          : "home",
                    )
                  }
                  style={{
                    flex: "none",
                    background: "var(--bg2)",
                    color: "var(--fg1)",
                    border: "1px solid var(--border)",
                    borderRadius: 6,
                    padding: "6px 8px",
                    font: `500 11.5px/1 ${FONT_SANS}`,
                  }}
                >
                  <option value="home">
                    {t("settings.terminalStartDirectory.home")}
                  </option>
                  <option value="workspace">
                    {t("settings.terminalStartDirectory.workspace")}
                  </option>
                  <option value="custom">
                    {t("settings.terminalStartDirectory.custom")}
                  </option>
                </select>
              </div>
              {terminalStartDirectory === "custom" ? (
                <input
                  className="agentmux-terminal-start-custom-cwd"
                  aria-label={t("settings.terminalStartCustomCwd")}
                  value={terminalStartCustomCwd}
                  placeholder={t("settings.terminalStartCustomCwdPlaceholder")}
                  onChange={(event) =>
                    setTerminalStartCustomCwd(event.currentTarget.value)
                  }
                  style={{
                    width: "100%",
                    boxSizing: "border-box",
                    background: "var(--bg2)",
                    color: "var(--fg1)",
                    border: "1px solid var(--border)",
                    borderRadius: 6,
                    padding: "7px 9px",
                    font: `500 11.5px/1 ${FONT_MONO}`,
                    marginBottom: 24,
                  }}
                />
              ) : (
                <div style={{ marginBottom: 24 }} />
              )}
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  gap: 12,
                  marginBottom: 24,
                }}
              >
                <div style={{ minWidth: 0 }}>
                  <div
                    style={{
                      font: `600 12px/1 ${FONT_SANS}`,
                      color: "var(--fg2)",
                    }}
                  >
                    {t("settings.terminalSplitBehavior")}
                  </div>
                  <div
                    style={{
                      font: `400 11.5px/1.45 ${FONT_SANS}`,
                      color: "var(--fg4)",
                      marginTop: 5,
                    }}
                  >
                    {t("settings.terminalSplitBehaviorHint")}
                  </div>
                </div>
                <select
                  className="agentmux-terminal-split-behavior"
                  aria-label={t("settings.terminalSplitBehavior")}
                  value={terminalSplitBehavior}
                  onChange={(event) =>
                    setTerminalSplitBehavior(
                      event.currentTarget.value === "empty"
                        ? "empty"
                        : "clone_current",
                    )
                  }
                  style={{
                    flex: "none",
                    background: "var(--bg2)",
                    color: "var(--fg1)",
                    border: "1px solid var(--border)",
                    borderRadius: 6,
                    padding: "6px 8px",
                    font: `500 11.5px/1 ${FONT_SANS}`,
                  }}
                >
                  <option value="clone_current">
                    {t("settings.terminalSplitBehavior.cloneCurrent")}
                  </option>
                  <option value="empty">
                    {t("settings.terminalSplitBehavior.empty")}
                  </option>
                </select>
              </div>
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  gap: 12,
                  marginBottom: 9,
                }}
              >
                <div style={{ minWidth: 0 }}>
                  <div
                    style={{
                      font: `600 12px/1 ${FONT_SANS}`,
                      color: "var(--fg2)",
                    }}
                  >
                    {t("settings.terminalLinkOpen")}
                  </div>
                  <div
                    style={{
                      font: `400 11.5px/1.45 ${FONT_SANS}`,
                      color: "var(--fg4)",
                      marginTop: 5,
                    }}
                  >
                    {t("settings.terminalLinkOpenHint")}
                  </div>
                </div>
                <select
                  className="agentmux-terminal-link-open-mode"
                  aria-label={t("settings.terminalLinkOpen")}
                  value={terminalLinkOpenMode}
                  onChange={(e) =>
                    setTerminalLinkOpenMode(
                      e.currentTarget.value === "in-app" ? "in-app" : "system",
                    )
                  }
                  style={{
                    flex: "none",
                    background: "var(--bg2)",
                    color: "var(--fg1)",
                    border: "1px solid var(--border)",
                    borderRadius: 6,
                    padding: "6px 8px",
                    font: `500 11.5px/1 ${FONT_SANS}`,
                  }}
                >
                  <option value="system">
                    {t("settings.terminalLinkOpen.system")}
                  </option>
                  <option value="in-app">
                    {t("settings.terminalLinkOpen.inApp")}
                  </option>
                </select>
              </div>
            </>
          ) : null}

          {settingsTab === "workspace" ? (
            <form
              data-agentmux-workspace-settings
              onSubmit={(event) => {
                event.preventDefault();
                saveWorkspaceDraft();
              }}
            >
              <div
                style={{
                  font: `700 18px/1 ${FONT_SANS}`,
                  color: "var(--fg1)",
                  marginBottom: 20,
                }}
              >
                {t("settings.workspace.title")}
              </div>
              {activeWorkspace ? (
                <>
                  <div
                    style={{
                      display: "grid",
                      gridTemplateColumns: "1fr 1fr",
                      gap: 14,
                      marginBottom: 14,
                    }}
                  >
                    <label>
                      <span style={fieldLabel}>Name</span>
                      <input
                        className="agentmux-workspace-name-input"
                        aria-label="Workspace name"
                        value={workspaceDraft.name}
                        onChange={(event) =>
                          updateWorkspaceDraft({
                            name: event.currentTarget.value,
                          })
                        }
                        style={fieldInput}
                      />
                    </label>
                    <label>
                      <span style={fieldLabel}>Project root</span>
                      <input
                        className="agentmux-workspace-root-input"
                        aria-label="Project root"
                        value={workspaceDraft.projectRoot ?? ""}
                        onChange={(event) =>
                          updateWorkspaceDraft({
                            projectRoot: event.currentTarget.value,
                          })
                        }
                        style={{ ...fieldInput, fontFamily: FONT_MONO }}
                      />
                    </label>
                  </div>
                  <label style={{ display: "block", marginBottom: 14 }}>
                    <span style={fieldLabel}>Description</span>
                    <textarea
                      className="agentmux-workspace-description-input"
                      aria-label="Workspace description"
                      value={workspaceDraft.description ?? ""}
                      onChange={(event) =>
                        updateWorkspaceDraft({
                          description: event.currentTarget.value,
                        })
                      }
                      rows={3}
                      style={{
                        ...fieldInput,
                        resize: "vertical",
                        minHeight: 74,
                      }}
                    />
                  </label>
                  <div
                    style={{
                      display: "grid",
                      gridTemplateColumns: "120px 1fr",
                      gap: 14,
                      marginBottom: 14,
                    }}
                  >
                    <label>
                      <span style={fieldLabel}>Icon</span>
                      <input
                        className="agentmux-workspace-icon-input"
                        aria-label="Workspace icon"
                        maxLength={2}
                        value={workspaceDraft.icon ?? ""}
                        onChange={(event) =>
                          updateWorkspaceDraft({
                            icon: event.currentTarget.value
                              .toUpperCase()
                              .slice(0, 2),
                          })
                        }
                        style={fieldInput}
                      />
                    </label>
                    <div>
                      <span style={fieldLabel}>Color</span>
                      <div
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: 8,
                          flexWrap: "wrap",
                        }}
                      >
                        {ACCENTS.map((candidate) => (
                          <button
                            key={candidate.key}
                            type="button"
                            aria-label={`Workspace color ${candidate.label}`}
                            className={`agentmux-workspace-color-${candidate.key}`}
                            onClick={() =>
                              updateWorkspaceDraft({ color: candidate.hex })
                            }
                            style={{
                              width: 28,
                              height: 28,
                              borderRadius: 7,
                              border: `2px solid ${workspaceDraft.color === candidate.hex ? "var(--fg1)" : "var(--border)"}`,
                              background: candidate.hex,
                              cursor: "pointer",
                            }}
                          />
                        ))}
                      </div>
                    </div>
                  </div>
                  <div
                    style={{
                      display: "grid",
                      gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))",
                      gap: 14,
                      marginBottom: 18,
                    }}
                  >
                    <label>
                      <span style={fieldLabel}>Default terminal</span>
                      <select
                        className="agentmux-workspace-terminal-profile-select"
                        aria-label="Default terminal profile"
                        value={workspaceDraft.defaultTerminalProfile ?? "wsl"}
                        onChange={(event) =>
                          updateWorkspaceDraft({
                            defaultTerminalProfile: event.currentTarget
                              .value as TerminalProfile,
                          })
                        }
                        style={fieldInput}
                      >
                        <option value="wsl">WSL</option>
                        <option value="powershell">PowerShell</option>
                        <option value="cmd">Command Prompt</option>
                      </select>
                    </label>
                    <label>
                      <span style={fieldLabel}>Default WSL</span>
                      <select
                        className="agentmux-workspace-wsl-select"
                        aria-label="Default WSL distribution"
                        value={workspaceDraft.defaultWslDistribution ?? ""}
                        onChange={(event) =>
                          updateWorkspaceDraft({
                            defaultWslDistribution: event.currentTarget.value,
                          })
                        }
                        style={fieldInput}
                      >
                        <option value="">System default</option>
                        {wslDistributions.map((distribution) => (
                          <option
                            key={distribution.name}
                            value={distribution.name}
                          >
                            {distribution.name}
                            {distribution.isDefault ? " (default)" : ""}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label>
                      <span style={fieldLabel}>Default agent command</span>
                      <input
                        className="agentmux-workspace-agent-input"
                        aria-label="Default agent command"
                        value={workspaceDraft.defaultAgentCommand ?? ""}
                        onChange={(event) =>
                          updateWorkspaceDraft({
                            defaultAgentCommand: event.currentTarget.value,
                          })
                        }
                        placeholder="claude"
                        style={{ ...fieldInput, fontFamily: FONT_MONO }}
                      />
                    </label>
                  </div>
                  <button
                    type="submit"
                    className="agentmux-workspace-save"
                    style={{
                      background: "var(--accent)",
                      color: "#fff",
                      border: 0,
                      borderRadius: 8,
                      padding: "9px 14px",
                      cursor: "pointer",
                      font: `700 12px/1 ${FONT_SANS}`,
                    }}
                  >
                    {t("settings.workspace.saveProject")}
                  </button>
                </>
              ) : (
                <div
                  style={{
                    font: `400 12px/1.5 ${FONT_SANS}`,
                    color: "var(--fg4)",
                  }}
                >
                  {t("settings.workspace.noActiveProject")}
                </div>
              )}
            </form>
          ) : null}

          {SSH_UI_ENABLED && settingsTab === "profiles" ? (
            <>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  marginBottom: 20,
                }}
              >
                <div
                  style={{
                    font: `700 18px/1 ${FONT_SANS}`,
                    color: "var(--fg1)",
                  }}
                >
                  프로필 · SSH
                </div>
                <button
                  type="button"
                  onClick={promptNewProfile}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 6,
                    background: "var(--accent)",
                    color: "#fff",
                    border: 0,
                    borderRadius: 8,
                    padding: "8px 13px",
                    cursor: "pointer",
                    font: `600 12px/1 ${FONT_SANS}`,
                  }}
                >
                  <IconPlus size={13} />
                  프로필 추가
                </button>
              </div>
              <div
                style={{
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  overflow: "hidden",
                }}
              >
                <div
                  style={{
                    display: "grid",
                    gridTemplateColumns: "1.2fr 1.4fr 0.9fr auto",
                    padding: "10px 14px",
                    background: "var(--bg)",
                    borderBottom: "1px solid var(--border)",
                    font: `700 10.5px/1 ${FONT_SANS}`,
                    letterSpacing: ".05em",
                    textTransform: "uppercase",
                    color: "var(--fg4)",
                  }}
                >
                  <span>이름</span>
                  <span>호스트</span>
                  <span>사용자</span>
                  <span style={{ textAlign: "right" }}>동작</span>
                </div>
                {profiles.map((p) => (
                  <div
                    key={p.profileId}
                    style={{
                      display: "grid",
                      gridTemplateColumns: "1.2fr 1.4fr 0.9fr auto",
                      alignItems: "center",
                      padding: "12px 14px",
                      borderBottom: "1px solid var(--border-subtle)",
                    }}
                  >
                    <span
                      style={{
                        font: `600 12.5px/1 ${FONT_SANS}`,
                        color: "var(--fg1)",
                      }}
                    >
                      {p.name}
                    </span>
                    <span
                      style={{
                        font: `400 12px/1 ${FONT_MONO}`,
                        color: "var(--fg3)",
                      }}
                    >
                      {p.host}
                      {p.port ? `:${p.port}` : ""}
                    </span>
                    <span
                      style={{
                        font: `400 12px/1 ${FONT_MONO}`,
                        color: "var(--fg3)",
                      }}
                    >
                      {p.user}
                    </span>
                    <div
                      style={{
                        justifySelf: "end",
                        display: "flex",
                        alignItems: "center",
                        gap: 6,
                      }}
                    >
                      <button
                        type="button"
                        className="agentmux-profile-edit"
                        onClick={() => promptEditProfile(p)}
                        style={{
                          background: "var(--s2)",
                          color: "var(--fg2)",
                          border: "1px solid var(--border)",
                          borderRadius: 6,
                          padding: "5px 10px",
                          cursor: "pointer",
                          font: `600 11px/1 ${FONT_SANS}`,
                        }}
                      >
                        {t("common.edit")}
                      </button>
                      <button
                        type="button"
                        onClick={() => onConnectProfile(p)}
                        style={{
                          background: "var(--accent-soft)",
                          color: "var(--accent)",
                          border: 0,
                          borderRadius: 6,
                          padding: "5px 10px",
                          cursor: "pointer",
                          font: `600 11px/1 ${FONT_SANS}`,
                        }}
                      >
                        {t("common.connect")}
                      </button>
                      <Hov
                        tag="span"
                        style={{
                          width: 26,
                          height: 26,
                          borderRadius: 6,
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "center",
                          color: "var(--fg4)",
                          cursor: "pointer",
                        }}
                        hover={{
                          background: "var(--s2)",
                          color: "var(--red, #F87171)",
                        }}
                        onClick={() => onDeleteProfile(p.profileId)}
                      >
                        <IconClose size={12} />
                      </Hov>
                    </div>
                  </div>
                ))}
                {profiles.length === 0 ? (
                  <div
                    style={{
                      padding: "14px",
                      font: `400 12px/1 ${FONT_SANS}`,
                      color: "var(--fg4)",
                    }}
                  >
                    등록된 프로필이 없습니다.
                  </div>
                ) : null}
              </div>
              <div
                style={{
                  marginTop: 12,
                  font: `400 11px/1.5 ${FONT_SANS}`,
                  color: "var(--fg4)",
                }}
              >
                프로필은 control plane에 저장됩니다. SSH 직접 연결(전송
                백엔드)은 후속 작업입니다.
              </div>
            </>
          ) : null}

          {settingsTab === "keys" ? (
            <>
              <div
                style={{
                  font: `700 18px/1 ${FONT_SANS}`,
                  color: "var(--fg1)",
                  marginBottom: 20,
                }}
              >
                {t("settings.keys")}
              </div>
              {shortcutConflicts.length > 0 ? (
                <div
                  className="agentmux-shortcut-conflicts"
                  style={{
                    border: "1px solid var(--warn, #FBBF24)",
                    background: "rgba(251,191,36,0.1)",
                    borderRadius: 8,
                    padding: 12,
                    marginBottom: 12,
                  }}
                >
                  <div
                    style={{
                      font: `700 12px/1.2 ${FONT_SANS}`,
                      color: "var(--fg1)",
                      marginBottom: 8,
                    }}
                  >
                    Shortcut conflicts
                  </div>
                  {shortcutConflicts.map((conflict) => (
                    <div
                      key={conflict.key}
                      className="agentmux-shortcut-conflict"
                      style={{
                        font: `500 11.5px/1.45 ${FONT_MONO}`,
                        color: "var(--fg2)",
                        marginTop: 4,
                      }}
                    >
                      {conflict.label}: {conflict.actions.join(", ")}
                    </div>
                  ))}
                </div>
              ) : null}
              {shortcutEditMessage ? (
                <div
                  className="agentmux-shortcut-edit-message"
                  style={{
                    font: `500 11.5px/1.4 ${FONT_SANS}`,
                    color: "var(--fg3)",
                    marginBottom: 10,
                  }}
                >
                  {shortcutEditMessage}
                </div>
              ) : null}
              <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
                {visibleActions.map((action) => (
                  <div
                    key={action.id}
                    data-agentmux-shortcut-row={action.id}
                    style={{
                      display: "grid",
                      gridTemplateColumns: "1.2fr 1fr auto auto auto",
                      alignItems: "center",
                      gap: 8,
                      padding: "11px 4px",
                      borderBottom: "1px solid var(--border-subtle)",
                    }}
                  >
                    <span
                      style={{
                        font: `500 13px/1 ${FONT_SANS}`,
                        color: "var(--fg2)",
                        minWidth: 0,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {action.title}
                    </span>
                    <span
                      style={{
                        font: `400 10.5px/1 ${FONT_MONO}`,
                        color: "var(--fg4)",
                        minWidth: 0,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {action.id}
                    </span>
                    <span
                      style={{
                        font: `600 11px/1 ${FONT_MONO}`,
                        background: "var(--s2)",
                        border: "1px solid var(--border)",
                        borderRadius: 5,
                        padding: "5px 9px",
                        color: "var(--fg2)",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {shortcutLabelForAction(shortcutBindings, action.id) ||
                        t("common.unassigned")}
                    </span>
                    <button
                      type="button"
                      className="agentmux-shortcut-edit"
                      onClick={() => editShortcut(action)}
                      style={{
                        background: "var(--s2)",
                        color: "var(--fg2)",
                        border: "1px solid var(--border)",
                        borderRadius: 6,
                        padding: "5px 9px",
                        cursor: "pointer",
                        font: `600 11px/1 ${FONT_SANS}`,
                      }}
                    >
                      {t("common.edit")}
                    </button>
                    <button
                      type="button"
                      className="agentmux-shortcut-clear"
                      onClick={() => onUpdateShortcut(action.id, null)}
                      style={{
                        background: "transparent",
                        color: "var(--fg4)",
                        border: "1px solid var(--border)",
                        borderRadius: 6,
                        padding: "5px 9px",
                        cursor: "pointer",
                        font: `600 11px/1 ${FONT_SANS}`,
                      }}
                    >
                      {t("common.clear")}
                    </button>
                  </div>
                ))}
              </div>
            </>
          ) : null}

          {settingsTab === "diagnostics" ? (
            <div data-agentmux-diagnostics>
              <div
                style={{
                  font: `700 18px/1 ${FONT_SANS}`,
                  color: "var(--fg1)",
                  marginBottom: 20,
                }}
              >
                진단
              </div>
              <div
                style={{
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  padding: 14,
                  marginBottom: 14,
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 12,
                    marginBottom: 12,
                  }}
                >
                  <div>
                    <div
                      style={{
                        font: `700 13px/1.2 ${FONT_SANS}`,
                        color: "var(--fg1)",
                      }}
                    >
                      WSL tmux probe
                    </div>
                    <div
                      style={{
                        font: `400 11.5px/1.5 ${FONT_SANS}`,
                        color: "var(--fg4)",
                        marginTop: 4,
                      }}
                    >
                      durable agent 실행에 필요한 WSL/tmux 상태를 확인합니다.
                    </div>
                  </div>
                  <button
                    type="button"
                    className="agentmux-tmux-probe"
                    onClick={() => onRunTmuxProbe()}
                    disabled={tmuxProbeBusy}
                    style={{
                      flex: "none",
                      background: "var(--accent)",
                      color: "#fff",
                      border: 0,
                      borderRadius: 8,
                      padding: "8px 13px",
                      cursor: tmuxProbeBusy ? "default" : "pointer",
                      opacity: tmuxProbeBusy ? 0.72 : 1,
                      font: `600 12px/1 ${FONT_SANS}`,
                    }}
                  >
                    {tmuxProbeBusy ? "Checking..." : "Run probe"}
                  </button>
                </div>
                {tmuxProbe ? (
                  <div
                    style={{
                      display: "grid",
                      gridTemplateColumns: "110px 1fr",
                      rowGap: 8,
                      columnGap: 10,
                      font: `500 12px/1.4 ${FONT_SANS}`,
                    }}
                  >
                    <span style={{ color: "var(--fg4)" }}>status</span>
                    <span
                      style={{
                        color: tmuxProbe.available
                          ? "var(--green, #34D399)"
                          : "var(--red, #F87171)",
                      }}
                    >
                      {tmuxProbe.available ? "available" : "unavailable"}
                    </span>
                    <span style={{ color: "var(--fg4)" }}>distribution</span>
                    <span style={{ color: "var(--fg2)" }}>
                      {tmuxProbe.distribution ?? "-"}
                    </span>
                    <span style={{ color: "var(--fg4)" }}>version</span>
                    <span style={{ color: "var(--fg2)" }}>
                      {tmuxProbe.version ?? "-"}
                    </span>
                    <span style={{ color: "var(--fg4)" }}>message</span>
                    <span style={{ color: "var(--fg2)" }}>
                      {tmuxProbe.message}
                    </span>
                  </div>
                ) : (
                  <div
                    style={{
                      font: `400 12px/1.5 ${FONT_SANS}`,
                      color: "var(--fg4)",
                    }}
                  >
                    Probe를 실행하면 결과가 여기에 표시됩니다.
                  </div>
                )}
              </div>
            </div>
          ) : null}

          {settingsTab === "general" ? (
            <>
              <div
                style={{
                  font: `700 18px/1 ${FONT_SANS}`,
                  color: "var(--fg1)",
                  marginBottom: 20,
                }}
              >
                {t("settings.general")}
              </div>
              <div
                data-agentmux-app-version
                style={{
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  padding: 14,
                  marginBottom: 14,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: 14,
                }}
              >
                <div style={{ minWidth: 0 }}>
                  <div
                    style={{
                      font: `700 13px/1.2 ${FONT_SANS}`,
                      color: "var(--fg1)",
                    }}
                  >
                    {t("app.version")}
                  </div>
                  <div
                    style={{
                      font: `400 11.5px/1.45 ${FONT_SANS}`,
                      color: "var(--fg4)",
                      marginTop: 5,
                    }}
                  >
                    {t("app.version.current")}
                  </div>
                </div>
                <code
                  style={{
                    flex: "none",
                    border: "1px solid var(--border)",
                    borderRadius: 7,
                    padding: "7px 10px",
                    background: "var(--canvas)",
                    color: "var(--fg2)",
                    font: `700 12px/1 ${FONT_MONO}`,
                  }}
                >
                  v{APP_VERSION}
                </code>
              </div>
              <div
                data-agentmux-language-settings
                style={{
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  padding: 14,
                  marginBottom: 14,
                }}
              >
                <label
                  style={{
                    display: "grid",
                    gridTemplateColumns: "minmax(0, 1fr) 180px",
                    alignItems: "center",
                    gap: 14,
                  }}
                >
                  <span style={{ minWidth: 0 }}>
                    <span
                      style={{
                        display: "block",
                        font: `700 13px/1.2 ${FONT_SANS}`,
                        color: "var(--fg1)",
                      }}
                    >
                      {t("language.label")}
                    </span>
                    <span
                      style={{
                        display: "block",
                        font: `400 11.5px/1.45 ${FONT_SANS}`,
                        color: "var(--fg4)",
                        marginTop: 5,
                      }}
                    >
                      {t("language.savedGlobally")}
                    </span>
                  </span>
                  <select
                    className="agentmux-language-select"
                    aria-label={t("language.label")}
                    value={language}
                    onChange={(event) =>
                      setLanguage(event.currentTarget.value as AppLocaleLanguage)
                    }
                    style={fieldInput}
                  >
                    {SUPPORTED_LANGUAGES.map((option) => (
                      <option key={option.code} value={option.code}>
                        {t(option.labelKey)}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
              <div
                data-agentmux-update-settings
                style={{
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  padding: 14,
                  marginBottom: 14,
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 14,
                    marginBottom: 12,
                  }}
                >
                  <div style={{ minWidth: 0 }}>
                    <div
                      style={{
                        font: `700 13px/1.2 ${FONT_SANS}`,
                        color: "var(--fg1)",
                      }}
                    >
                      {t("updates.title")}
                    </div>
                    <div
                      style={{
                        font: `400 11.5px/1.45 ${FONT_SANS}`,
                        color: "var(--fg4)",
                        marginTop: 5,
                      }}
                    >
                      {t("updates.autoCheckHint")}
                    </div>
                  </div>
                  <label
                    style={{
                      flex: "none",
                      display: "flex",
                      alignItems: "center",
                      gap: 8,
                      color: "var(--fg2)",
                      font: `600 11.5px/1 ${FONT_SANS}`,
                    }}
                  >
                    <input
                      type="checkbox"
                      checked={updatesConfig.autoCheck}
                      onChange={(event) =>
                        setAutoUpdateCheck(event.currentTarget.checked)
                      }
                      style={{ accentColor: "var(--accent)" }}
                    />
                    {t("updates.autoCheck")}
                  </label>
                </div>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 12,
                  }}
                >
                  <div
                    className="agentmux-update-status"
                    style={{
                      minWidth: 0,
                      color:
                        updateState.status === "error"
                          ? "#ef4444"
                          : updateState.status === "available"
                            ? "var(--accent)"
                            : "var(--fg3)",
                      font: `500 11.5px/1.45 ${FONT_SANS}`,
                      overflowWrap: "anywhere",
                    }}
                  >
                    {updateStatusText}
                  </div>
                  <div
                    style={{
                      flex: "none",
                      display: "flex",
                      alignItems: "center",
                      gap: 8,
                    }}
                  >
                    {updateState.status === "available" ? (
                      <button
                        type="button"
                        className="agentmux-update-install"
                        onClick={onInstallUpdate}
                        disabled={updateBusy}
                        style={{
                          background: "var(--accent)",
                          color: "#fff",
                          border: 0,
                          borderRadius: 8,
                          padding: "8px 13px",
                          cursor: updateBusy ? "default" : "pointer",
                          opacity: updateBusy ? 0.55 : 1,
                          font: `600 12px/1 ${FONT_SANS}`,
                        }}
                      >
                        {t("updates.install")}
                      </button>
                    ) : null}
                    <button
                      type="button"
                      className="agentmux-update-check"
                      onClick={onCheckForUpdates}
                      disabled={updateBusy}
                      style={{
                        background: "var(--s2)",
                        color: "var(--fg2)",
                        border: "1px solid var(--border)",
                        borderRadius: 8,
                        padding: "8px 11px",
                        cursor: updateBusy ? "default" : "pointer",
                        opacity: updateBusy ? 0.55 : 1,
                        font: `600 12px/1 ${FONT_SANS}`,
                      }}
                    >
                      {t("updates.check")}
                    </button>
                  </div>
                </div>
                {updateState.body ? (
                  <details
                    style={{
                      marginTop: 10,
                      color: "var(--fg3)",
                      font: `400 11.5px/1.45 ${FONT_SANS}`,
                    }}
                  >
                    <summary style={{ cursor: "pointer", color: "var(--fg2)" }}>
                      {t("updates.releaseNotes")}
                    </summary>
                    <pre
                      style={{
                        margin: "8px 0 0",
                        whiteSpace: "pre-wrap",
                        overflowWrap: "anywhere",
                        font: `400 11px/1.45 ${FONT_MONO}`,
                        color: "var(--fg4)",
                      }}
                    >
                      {updateState.body}
                    </pre>
                  </details>
                ) : null}
              </div>
              <div
                data-agentmux-config-reload
                style={{
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  padding: 14,
                  marginBottom: 14,
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 12,
                    marginBottom: 10,
                  }}
                >
                  <div style={{ minWidth: 0 }}>
                    <div
                      style={{
                        font: `700 13px/1.2 ${FONT_SANS}`,
                        color: "var(--fg1)",
                      }}
                    >
                      {t("config.configuration")}
                    </div>
                    <div
                      data-agentmux-global-config-path
                      style={{
                        font: `400 11.5px/1.45 ${FONT_MONO}`,
                        color: "var(--fg4)",
                        marginTop: 5,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {t("config.globalPath", { path: configPath || "-" })}
                    </div>
                    <div
                      data-agentmux-project-config-path
                      style={{
                        font: `400 11.5px/1.45 ${FONT_MONO}`,
                        color: projectConfigLoaded
                          ? "var(--accent)"
                          : "var(--fg4)",
                        marginTop: 3,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {t("config.projectPath", {
                        path: projectConfigPath || "-",
                      })}
                    </div>
                  </div>
                  <div
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 8,
                      flex: "none",
                    }}
                  >
                    <button
                      type="button"
                      className="agentmux-config-export"
                      onClick={() => onExportConfig("global")}
                      style={{
                        background: "var(--s2)",
                        color: "var(--fg2)",
                        border: "1px solid var(--border)",
                        borderRadius: 8,
                        padding: "8px 11px",
                        cursor: "pointer",
                        font: `600 12px/1 ${FONT_SANS}`,
                      }}
                    >
                      {t("config.export")}
                    </button>
                    <button
                      type="button"
                      className="agentmux-config-import"
                      onClick={() => onImportConfig("global")}
                      style={{
                        background: "var(--s2)",
                        color: "var(--fg2)",
                        border: "1px solid var(--border)",
                        borderRadius: 8,
                        padding: "8px 11px",
                        cursor: "pointer",
                        font: `600 12px/1 ${FONT_SANS}`,
                      }}
                    >
                      {t("config.import")}
                    </button>
                    <button
                      type="button"
                      className="agentmux-config-reset"
                      onClick={() => onResetConfig("global")}
                      style={{
                        background: "var(--s2)",
                        color: "var(--fg2)",
                        border: "1px solid var(--border)",
                        borderRadius: 8,
                        padding: "8px 11px",
                        cursor: "pointer",
                        font: `600 12px/1 ${FONT_SANS}`,
                      }}
                    >
                      {t("common.reset")}
                    </button>
                    <button
                      type="button"
                      className="agentmux-config-reload"
                      onClick={onReloadConfig}
                      style={{
                        background: "var(--accent)",
                        color: "#fff",
                        border: 0,
                        borderRadius: 8,
                        padding: "8px 13px",
                        cursor: "pointer",
                        font: `600 12px/1 ${FONT_SANS}`,
                      }}
                    >
                      {t("config.reload")}
                    </button>
                  </div>
                </div>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "flex-end",
                    gap: 8,
                    flexWrap: "wrap",
                    marginTop: 8,
                  }}
                >
                  <button
                    type="button"
                    className="agentmux-project-config-export"
                    onClick={() => onExportConfig("project")}
                    disabled={!projectConfigPath}
                    style={{
                      background: "var(--s2)",
                      color: projectConfigPath ? "var(--fg2)" : "var(--fg4)",
                      border: "1px solid var(--border)",
                      borderRadius: 8,
                      padding: "7px 10px",
                      cursor: projectConfigPath ? "pointer" : "default",
                      opacity: projectConfigPath ? 1 : 0.55,
                      font: `600 11px/1 ${FONT_SANS}`,
                    }}
                  >
                    {t("config.exportProject")}
                  </button>
                  <button
                    type="button"
                    className="agentmux-project-config-import"
                    onClick={() => onImportConfig("project")}
                    disabled={!projectConfigPath}
                    style={{
                      background: "var(--s2)",
                      color: projectConfigPath ? "var(--fg2)" : "var(--fg4)",
                      border: "1px solid var(--border)",
                      borderRadius: 8,
                      padding: "7px 10px",
                      cursor: projectConfigPath ? "pointer" : "default",
                      opacity: projectConfigPath ? 1 : 0.55,
                      font: `600 11px/1 ${FONT_SANS}`,
                    }}
                  >
                    {t("config.importProject")}
                  </button>
                  <button
                    type="button"
                    className="agentmux-project-config-migrate-cmux"
                    onClick={onMigrateProjectConfig}
                    disabled={!projectConfigPath}
                    style={{
                      background: "var(--s2)",
                      color: projectConfigPath ? "var(--fg2)" : "var(--fg4)",
                      border: "1px solid var(--border)",
                      borderRadius: 8,
                      padding: "7px 10px",
                      cursor: projectConfigPath ? "pointer" : "default",
                      opacity: projectConfigPath ? 1 : 0.55,
                      font: `600 11px/1 ${FONT_SANS}`,
                    }}
                  >
                    {t("config.migrateCmux")}
                  </button>
                  <button
                    type="button"
                    className="agentmux-project-config-reset"
                    onClick={() => onResetConfig("project")}
                    disabled={!projectConfigPath}
                    style={{
                      background: "var(--s2)",
                      color: projectConfigPath ? "var(--fg2)" : "var(--fg4)",
                      border: "1px solid var(--border)",
                      borderRadius: 8,
                      padding: "7px 10px",
                      cursor: projectConfigPath ? "pointer" : "default",
                      opacity: projectConfigPath ? 1 : 0.55,
                      font: `600 11px/1 ${FONT_SANS}`,
                    }}
                  >
                    {t("config.resetProject")}
                  </button>
                </div>
                <div
                  style={{
                    font: `400 11.5px/1.45 ${FONT_SANS}`,
                    color: "var(--fg4)",
                    marginTop: 10,
                  }}
                >
                  {t("config.jsonOnlyHint")}
                </div>
                {configDiagnostics.length > 0 ? (
                  <div
                    data-agentmux-config-diagnostics
                    style={{ display: "grid", gap: 6, marginTop: 12 }}
                  >
                    {configDiagnostics.map((entry) => (
                      <div
                        key={entry.source}
                        className="agentmux-config-diagnostic-row"
                        data-agentmux-config-diagnostic-source={entry.source}
                        style={{
                          display: "grid",
                          gridTemplateColumns: "92px 70px 70px minmax(0, 1fr)",
                          gap: 8,
                          alignItems: "center",
                          font: `500 11.5px/1.35 ${FONT_SANS}`,
                          color: "var(--fg3)",
                        }}
                      >
                        <span
                          style={{
                            color: "var(--fg2)",
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                          }}
                        >
                          {entry.source}
                        </span>
                        <span
                          style={{
                            color: entry.valid ? "var(--accent)" : "#ef4444",
                          }}
                        >
                          {entry.valid ? t("common.ok") : t("common.invalid")}
                        </span>
                        <span
                          style={{
                            color: entry.active ? "var(--fg2)" : "var(--fg4)",
                          }}
                        >
                          {entry.active ? t("common.active") : t("common.idle")}
                        </span>
                        <span
                          title={entry.path ?? ""}
                          style={{
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                            color: entry.exists ? "var(--fg3)" : "var(--fg4)",
                            fontFamily: FONT_MONO,
                          }}
                        >
                          {entry.path ?? "-"}
                        </span>
                        <span
                          style={{
                            gridColumn: "1 / -1",
                            color: "var(--fg4)",
                            overflowWrap: "anywhere",
                          }}
                        >
                          {entry.message}
                        </span>
                      </div>
                    ))}
                  </div>
                ) : null}
                {configReloadMessage ? (
                  <div
                    className="agentmux-config-reload-message"
                    style={{
                      font: `500 11.5px/1.4 ${FONT_SANS}`,
                      color: "var(--fg3)",
                    }}
                  >
                    {configReloadMessage}
                  </div>
                ) : null}
              </div>
              {notifications.length === 0 ? (
                <div
                  style={{
                    font: `400 12px/1.5 ${FONT_SANS}`,
                    color: "var(--fg4)",
                  }}
                >
                  {t("notifications.empty")}
                </div>
              ) : (
                <div
                  style={{ display: "flex", flexDirection: "column", gap: 8 }}
                >
                  {notifications.map((n) => (
                    <div
                      key={n.notificationId}
                      data-agentmux-notification={n.notificationId}
                      data-agentmux-notification-type={n.notificationType}
                      data-agentmux-notification-severity={n.severity}
                      style={{
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "space-between",
                        padding: "12px 14px",
                        border: "1px solid var(--border)",
                        borderRadius: 8,
                      }}
                    >
                      <div style={{ minWidth: 0, flex: "1 1 auto" }}>
                        <div
                          style={{
                            font: `600 12.5px/1.3 ${FONT_SANS}`,
                            color: "var(--fg1)",
                          }}
                        >
                          {n.title}
                        </div>
                        <div
                          style={{
                            font: `400 11.5px/1.4 ${FONT_SANS}`,
                            color: "var(--fg4)",
                            marginTop: 3,
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                          }}
                        >
                          {n.message}
                        </div>
                      </div>
                      {n.sessionId ? (
                        <button
                          type="button"
                          className="agentmux-notification-focus"
                          onClick={() => onFocusNotificationSession(n.sessionId)}
                          style={{
                            flex: "none",
                            marginLeft: 12,
                            background: "var(--accent-soft)",
                            border: "1px solid rgba(88, 166, 255, 0.38)",
                            borderRadius: 7,
                            padding: "6px 10px",
                            cursor: "pointer",
                            font: `600 11px/1 ${FONT_SANS}`,
                            color: "var(--accent)",
                          }}
                        >
                          Focus
                        </button>
                      ) : null}
                      {notificationActionsFor(n).map(
                        ({ hook, action }, index) => (
                          <button
                            key={`${n.notificationId}-${hook.action}-${index}`}
                            type="button"
                            className={`agentmux-notification-action agentmux-notification-action-${actionClassFragment(hook.action)}`}
                            onClick={() => onRunNotificationAction(hook, n)}
                            style={{
                              background: "var(--accent)",
                              border: 0,
                              borderRadius: 7,
                              padding: "6px 10px",
                              cursor: "pointer",
                              font: `600 11px/1 ${FONT_SANS}`,
                              color: "#fff",
                            }}
                          >
                            {hook.label ?? action.title}
                          </button>
                        ),
                      )}
                      <button
                        type="button"
                        onClick={() => onDismissNotification(n.notificationId)}
                        style={{
                          flex: "none",
                          marginLeft: 12,
                          background: "var(--s2)",
                          border: "1px solid var(--border)",
                          borderRadius: 7,
                          padding: "6px 10px",
                          cursor: "pointer",
                          font: `500 11px/1 ${FONT_SANS}`,
                          color: "var(--fg2)",
                        }}
                      >
                        {t("common.close")}
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </>
          ) : null}
        </div>
      </div>
    </div>
  );
}
