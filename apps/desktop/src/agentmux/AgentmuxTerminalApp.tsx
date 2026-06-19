import { type CSSProperties, type ReactNode, useEffect, useMemo, useState } from "react";
import "./agentmux.css";
import type {
  AgentTelemetry,
  NotificationSummary,
  PaneSummary,
  SshProfile,
  SshProfileInput,
  SurfaceSummary,
  TerminalSession,
  WorkspaceSummary
} from "../control/ControlClient";
import { KEYMAPS } from "./data";
import { BrowserSurfacePanel } from "./BrowserSurfacePanel";
import { Hov } from "./Hov";
import { LiveTerminal } from "./LiveTerminal";
import { SplitHandle } from "./SplitHandle";
import { useAgentmuxControl } from "./useAgentmuxControl";
import { ACCENTS, buildRootVars, THEMES, type ThemeName, type ThemeTokens } from "./theme";
import {
  BrandLogo,
  IconBranch,
  IconBubble,
  IconChevronDown,
  IconChevronRight,
  IconChevronUp,
  IconClose,
  IconFolder,
  IconGear,
  IconGrid,
  IconMoon,
  IconPlus,
  IconSearch,
  IconServer,
  IconShellArrow,
  IconSplitCols,
  IconSplitRows,
  IconSun
} from "./icons";

type Overlay = "palette" | "search" | "settings" | null;
type SettingsTab = "general" | "appearance" | "profiles" | "keys";

interface PaletteItem {
  title: string;
  hint: string;
  highlighted: boolean;
  onClick: () => void;
}

interface PaletteGroup {
  label: string;
  items: PaletteItem[];
}

const FONT_MONO = "'JetBrains Mono',monospace";
const FONT_SANS = "'Pretendard Variable'";

function sessionDotColor(theme: ThemeTokens, session: TerminalSession | undefined, attention: boolean): string {
  if (attention) return theme.warn;
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

function sessionLabel(session: TerminalSession | undefined, attention: boolean): string {
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

export function AgentmuxTerminalApp() {
  const ctl = useAgentmuxControl();
  const {
    client,
    ready,
    error,
    workspaces,
    activeWorkspaceId,
    detail,
    notifications,
    wslDistributions,
    profiles,
    attentionByWorkspace,
    attentionBySession,
    agentBySession
  } = ctl;

  const [theme, setTheme] = useState<ThemeName>("dark");
  const [accentKey, setAccentKey] = useState("orange");
  const [overlay, setOverlay] = useState<Overlay>(null);
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("appearance");
  const [query, setQuery] = useState("");
  const [fontSize, setFontSize] = useState(12.5);

  useEffect(() => {
    // Capture phase so global shortcuts win over a focused xterm terminal
    // (essential for a multiplexer). Escape is only intercepted while an
    // overlay is open, so it still reaches the terminal otherwise.
    function onKey(event: KeyboardEvent) {
      const mod = event.metaKey || event.ctrlKey;
      const key = (event.key || "").toLowerCase();
      if (mod && key === "k") {
        event.preventDefault();
        event.stopPropagation();
        setOverlay("palette");
        setQuery("");
      } else if (mod && key === "f") {
        event.preventDefault();
        event.stopPropagation();
        setOverlay("search");
      } else if (key === "escape" && overlay) {
        event.preventDefault();
        event.stopPropagation();
        setOverlay(null);
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [overlay]);

  const T = THEMES[theme];
  const accent = ACCENTS.find((a) => a.key === accentKey) ?? ACCENTS[0];
  const isDark = theme === "dark";
  const closeOverlay = () => setOverlay(null);
  const stop = (event: { stopPropagation: () => void }) => event.stopPropagation();

  const panes = useMemo(() => detail?.panes ?? [], [detail]);
  const surfaces = useMemo(() => detail?.surfaces ?? [], [detail]);
  const sessions = useMemo(() => detail?.sessions ?? [], [detail]);
  const activePaneId = detail?.workspace.activePaneId ?? null;

  const paneById = useMemo(() => new Map(panes.map((pane) => [pane.paneId, pane])), [panes]);
  const surfaceById = useMemo(
    () => new Map(surfaces.map((surface) => [surface.surfaceId, surface])),
    [surfaces]
  );
  const sessionById = useMemo(
    () => new Map(sessions.map((session) => [session.sessionId, session])),
    [sessions]
  );
  const childrenByParent = useMemo(() => {
    const map = new Map<string, PaneSummary[]>();
    for (const pane of panes) {
      if (!pane.parentPaneId) continue;
      const list = map.get(pane.parentPaneId) ?? [];
      list.push(pane);
      map.set(pane.parentPaneId, list);
    }
    return map;
  }, [panes]);
  const rootPaneId =
    detail?.workspace.rootPaneId ?? panes.find((pane) => !pane.parentPaneId)?.paneId ?? null;

  const surfaceForPane = (pane: PaneSummary): SurfaceSummary | undefined =>
    pane.mountedSurfaceId ? surfaceById.get(pane.mountedSurfaceId) : undefined;

  const terminalSurfaces = surfaces.filter((surface) => surface.surfaceType === "terminal");
  const paneHostingSurface = (surfaceId: string): PaneSummary | undefined =>
    panes.find((pane) => pane.mountedSurfaceId === surfaceId);

  const activeWorkspace = workspaces.find((ws) => ws.workspaceId === activeWorkspaceId);
  const activeSessionState = activePaneId
    ? (() => {
        const pane = paneById.get(activePaneId);
        const surface = pane ? surfaceForPane(pane) : undefined;
        return surface?.sessionId ? sessionById.get(surface.sessionId) : undefined;
      })()
    : undefined;
  const runningCount = sessions.filter((s) =>
    ["running", "starting", "recovering"].includes(s.state)
  ).length;

  // ---- actions ----
  const openTerminalInPane = async (paneId: string) => {
    await ctl.focusPane(paneId);
    await ctl.spawnNativeTerminal();
  };
  const addTerminal = async () => {
    const active = activePaneId ? paneById.get(activePaneId) : undefined;
    if (active && active.kind === "leaf" && !active.mountedSurfaceId) {
      await ctl.spawnNativeTerminal();
    } else {
      // Create an empty pane the user can fill (avoids replacing a live terminal).
      await ctl.splitActivePane("vertical");
    }
  };
  const splitPaneBy = async (paneId: string, axis: "horizontal" | "vertical") => {
    await ctl.focusPane(paneId);
    await ctl.splitActivePane(axis);
  };

  const rootStyle: CSSProperties = {
    ...buildRootVars(T, accent, fontSize),
    height: "100vh",
    width: "100vw",
    boxSizing: "border-box",
    background: "var(--canvas)",
    display: "flex",
    flexDirection: "column",
    overflow: "hidden",
    fontFamily: `${FONT_SANS},Pretendard,-apple-system,'Segoe UI',sans-serif`,
    color: T.fg1
  };
  const iconBtn: CSSProperties = {
    width: 30,
    height: 30,
    borderRadius: 7,
    border: 0,
    background: "transparent",
    cursor: "pointer",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    color: "var(--fg3)"
  };
  const iconBtnHover: CSSProperties = { background: "var(--s2)", color: "var(--fg1)" };

  // ---- command palette (live actions) ----
  const q = query.trim().toLowerCase();
  const promptCustomAgent = () => {
    const raw = window.prompt("durable 세션으로 실행할 에이전트 명령 (예: claude --resume)");
    const parts = (raw ?? "").trim().split(/\s+/).filter(Boolean);
    if (parts.length > 0) {
      void ctl.spawnAgent(parts);
    }
    closeOverlay();
  };
  const rawGroups: { label: string; items: PaletteItem[] }[] = [
    {
      label: "에이전트",
      items: [
        { title: "Claude Code 실행 (durable tmux)", hint: "claude", highlighted: false, onClick: () => { void ctl.spawnAgent(["claude"]); closeOverlay(); } },
        { title: "Codex 실행 (durable tmux)", hint: "codex", highlighted: false, onClick: () => { void ctl.spawnAgent(["codex"]); closeOverlay(); } },
        { title: "커스텀 에이전트 실행…", hint: "tmux", highlighted: false, onClick: promptCustomAgent }
      ]
    },
    {
      label: "터미널",
      items: [
        { title: "새 터미널", hint: "⌘ T", highlighted: false, onClick: () => { void addTerminal(); closeOverlay(); } },
        { title: "세로 분할", hint: "⌘ D", highlighted: false, onClick: () => { if (activePaneId) void splitPaneBy(activePaneId, "vertical"); closeOverlay(); } },
        { title: "가로 분할", hint: "⌘ G", highlighted: false, onClick: () => { if (activePaneId) void splitPaneBy(activePaneId, "horizontal"); closeOverlay(); } },
        { title: "브라우저 surface 열기", hint: "", highlighted: false, onClick: () => { void ctl.createBrowserSurface(); closeOverlay(); } }
      ]
    },
    {
      label: "워크스페이스",
      items: [
        { title: "새 워크스페이스", hint: "+", highlighted: false, onClick: () => { void ctl.createWorkspace(); closeOverlay(); } },
        ...workspaces.map((ws) => ({
          title: `이동: ${ws.name}`,
          hint: ws.projectRoot ?? "",
          highlighted: false,
          onClick: () => { void ctl.selectWorkspace(ws.workspaceId); closeOverlay(); }
        }))
      ]
    },
    {
      label: "보기",
      items: [
        { title: "테마 전환 (다크 / 라이트)", hint: "⌘⇧ L", highlighted: false, onClick: () => { setTheme(isDark ? "light" : "dark"); closeOverlay(); } },
        { title: "설정 열기", hint: "⌘ ,", highlighted: false, onClick: () => setOverlay("settings") },
        { title: "활성 창에서 검색", hint: "⌘ F", highlighted: false, onClick: () => setOverlay("search") }
      ]
    },
    {
      label: "원격 · WSL",
      items: wslDistributions.map((distribution) => ({
        title: `WSL 셸: ${distribution.name}`,
        hint: distribution.isDefault ? "default" : "",
        highlighted: false,
        onClick: () => { void ctl.spawnWslTerminal(distribution.name); closeOverlay(); }
      }))
    }
  ];
  const groups: PaletteGroup[] = [];
  let firstSet = false;
  for (const group of rawGroups) {
    const items = group.items.filter(
      (it) => !q || it.title.toLowerCase().includes(q) || it.hint.toLowerCase().includes(q)
    );
    if (items.length > 0) {
      for (const it of items) {
        it.highlighted = !firstSet;
        firstSet = true;
      }
      groups.push({ label: group.label, items });
    }
  }

  // ---- recursive pane-tree renderer (the real backend split tree) ----
  const renderPane = (paneId: string): ReactNode => {
    const pane = paneById.get(paneId);
    if (!pane) return null;

    if (pane.kind === "split") {
      const children = childrenByParent.get(pane.paneId) ?? [];
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
            flex: "1 1 0"
          }}
        >
          {first ? (
            <div style={{ flex: `${ratio} 1 0`, minWidth: 0, minHeight: 0, display: "flex" }}>
              {renderPane(first.paneId)}
            </div>
          ) : null}
          {first && second ? (
            <SplitHandle vertical={!column} onResize={(value) => ctl.resizePane(pane.paneId, value)} />
          ) : null}
          {second ? (
            <div style={{ flex: `${1 - ratio} 1 0`, minWidth: 0, minHeight: 0, display: "flex" }}>
              {renderPane(second.paneId)}
            </div>
          ) : null}
        </div>
      );
    }

    const surface = surfaceForPane(pane);
    const session = surface?.sessionId ? sessionById.get(surface.sessionId) : undefined;
    const active = pane.paneId === activePaneId;
    const attentionState = session ? attentionBySession.get(session.sessionId) : undefined;
    const hasAttention = Boolean(attentionState);
    const telemetry = session ? agentBySession.get(session.sessionId)?.telemetry ?? null : null;
    const isBrowser = surface?.surfaceType === "browser";
    const title = surface?.title ?? "빈 페인";
    const dot = sessionDotColor(T, session, hasAttention);
    const label = sessionLabel(session, hasAttention);
    const winBtn: CSSProperties = {
      width: 23,
      height: 23,
      borderRadius: 5,
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      color: "var(--fg4)",
      cursor: "pointer"
    };
    const winBtnHover: CSSProperties = { background: "var(--s2)", color: "var(--fg1)" };

    return (
      <div
        key={pane.paneId}
        onMouseDown={() => void ctl.focusPane(pane.paneId)}
        style={{
          minHeight: 0,
          minWidth: 0,
          flex: "1 1 0",
          background: "var(--term)",
          border: `1px solid ${active ? "var(--accent)" : "var(--border)"}`,
          borderRadius: 7,
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          boxShadow: active ? "0 0 0 1px var(--accent)" : "none"
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
            borderBottom: "1px solid var(--border)"
          }}
        >
          <span style={{ width: 7, height: 7, borderRadius: "50%", flex: "none", background: dot }} />
          <span style={{ font: `600 11.5px/1 ${FONT_MONO}`, color: "var(--fg1)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{title}</span>
          {label ? <span style={{ font: `500 9.5px/1 ${FONT_SANS}`, color: dot, background: "var(--s2)", borderRadius: 4, padding: "3px 6px", flex: "none", whiteSpace: "nowrap" }}>{label}</span> : null}
          {session ? <span style={{ font: `500 10px/1 ${FONT_MONO}`, color: "var(--fg3)", background: "var(--s2)", border: "1px solid var(--border)", borderRadius: 4, padding: "3px 6px", flex: "none" }}>{session.backendKind}</span> : null}
          <div style={{ flex: 1 }} />
          <div style={{ display: "flex", gap: 1, flex: "none" }}>
            <Hov tag="span" style={winBtn} hover={winBtnHover} onClick={(e) => { e.stopPropagation(); void splitPaneBy(pane.paneId, "vertical"); }}><IconSplitCols size={12} /></Hov>
            <Hov tag="span" style={winBtn} hover={winBtnHover} onClick={(e) => { e.stopPropagation(); void splitPaneBy(pane.paneId, "horizontal"); }}><IconSplitRows size={12} /></Hov>
            <Hov tag="span" style={winBtn} hover={winBtnHover} onClick={(e) => { e.stopPropagation(); if (pane.parentPaneId) void ctl.closePane(pane.paneId); }}><IconClose size={11} /></Hov>
          </div>
        </div>
        <div style={{ flex: 1, minHeight: 0, minWidth: 0, display: "flex", flexDirection: "column" }}>
          <div style={{ flex: 1, minHeight: 0, minWidth: 0, position: "relative" }}>
            {session && !isBrowser ? (
              <LiveTerminal
                key={session.sessionId}
                client={client}
                sessionId={session.sessionId}
                active={active}
                onFocus={() => void ctl.focusPane(pane.paneId)}
                onError={() => void ctl.refresh()}
              />
            ) : isBrowser && surface ? (
              <BrowserSurfacePanel client={client} surfaceId={surface.surfaceId} />
            ) : (
              <div style={{ height: "100%", display: "flex", flexDirection: "column", gap: 10, alignItems: "center", justifyContent: "center", color: "var(--fg4)" }}>
                <span style={{ font: `500 12px/1 ${FONT_SANS}` }}>빈 페인</span>
                <button
                  type="button"
                  onClick={(e) => { e.stopPropagation(); void openTerminalInPane(pane.paneId); }}
                  style={{ display: "flex", alignItems: "center", gap: 7, background: "var(--accent)", color: "#fff", border: 0, borderRadius: 8, padding: "8px 14px", cursor: "pointer", font: `600 12px/1 ${FONT_SANS}` }}
                >
                  <IconPlus size={13} /> 터미널 열기
                </button>
              </div>
            )}
          </div>
          {telemetry ? <OmcBar telemetry={telemetry} theme={T} /> : null}
        </div>
      </div>
    );
  };

  return (
    <div style={rootStyle}>
      {/* ============ APP SHELL (fills the OS window) ============ */}
      <div style={{ position: "relative", flex: "1 1 0", minHeight: 0, minWidth: 0, background: "var(--canvas)", overflow: "hidden", display: "flex", flexDirection: "column" }}>
        {/* titlebar — custom/frameless (decorations: false) */}
        <div data-tauri-drag-region style={{ height: 40, flex: "none", display: "flex", alignItems: "center", padding: "0 10px 0 12px", background: "var(--surface)", borderBottom: "1px solid var(--border)" }}>
          <BrandLogo size={17} radius={14} />
          <span style={{ font: `700 13px/1 ${FONT_MONO}`, letterSpacing: "-0.02em", color: "var(--fg1)", marginLeft: 9 }}>agentmux</span>
          <span style={{ color: "var(--fg4)", fontSize: 12, margin: "0 8px" }}>›</span>
          <span style={{ color: "var(--fg3)", display: "flex" }}><IconFolder /></span>
          <span style={{ font: `600 12.5px/1 ${FONT_SANS}`, color: "var(--fg2)", marginLeft: 7 }}>{activeWorkspace?.name ?? "—"}</span>
          <div style={{ flex: 1, height: "100%" }} />
          <button
            type="button"
            onClick={() => void ctl.spawnAgent(["claude"])}
            title="Claude Code를 durable WSL-tmux 세션으로 실행 (분리/재시작에도 유지)"
            style={{ display: "flex", alignItems: "center", gap: 6, background: "var(--accent)", color: "#fff", border: 0, borderRadius: 7, padding: "6px 11px", cursor: "pointer", font: `700 11.5px/1 ${FONT_SANS}`, marginRight: 8 }}
          >
            <span style={{ fontWeight: 700 }}>✳</span> 에이전트 실행
          </button>
          <Hov
            tag="button"
            style={{ height: 30, borderRadius: 7, border: 0, background: "transparent", cursor: "pointer", display: "flex", alignItems: "center", gap: 6, padding: "0 9px", color: "var(--fg2)", font: `600 11px/1 ${FONT_SANS}`, marginRight: 2 }}
            hover={iconBtnHover}
            onClick={() => setTheme(isDark ? "light" : "dark")}
          >
            {isDark ? <IconMoon /> : <IconSun />}
            {isDark ? "다크" : "라이트"}
          </Hov>
          <Hov tag="button" style={{ ...iconBtn, marginRight: 2 }} hover={iconBtnHover} onClick={() => setOverlay("search")}><IconSearch /></Hov>
          <Hov tag="button" style={{ ...iconBtn, marginRight: 2 }} hover={iconBtnHover} onClick={() => { setOverlay("palette"); setQuery(""); }}><IconGrid /></Hov>
          <Hov tag="button" style={iconBtn} hover={iconBtnHover} onClick={() => setOverlay("settings")}><IconGear /></Hov>
        </div>

        {/* body */}
        <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
          {/* sidebar */}
          <div style={{ width: 236, flex: "none", background: "var(--surface)", borderRight: "1px solid var(--border)", display: "flex", flexDirection: "column" }}>
            <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "10px 10px 8px" }}>
              <Hov tag="button" style={{ flex: 1, display: "flex", alignItems: "center", gap: 8, background: "var(--canvas)", border: "1px solid var(--border)", borderRadius: 8, padding: "8px 10px", cursor: "pointer", color: "var(--fg4)" }} hover={{ borderColor: "var(--border-strong)" }} onClick={() => { setOverlay("palette"); setQuery(""); }}>
                <IconSearch size={13} />
                <span style={{ font: `400 12px/1 ${FONT_SANS}`, flex: 1, textAlign: "left" }}>검색…</span>
                <span style={{ font: `600 10px/1 ${FONT_MONO}`, background: "var(--s2)", border: "1px solid var(--border)", borderRadius: 4, padding: "3px 5px", color: "var(--fg3)" }}>⌘K</span>
              </Hov>
              <Hov tag="button" style={{ width: 34, height: 34, flex: "none", display: "flex", alignItems: "center", justifyContent: "center", background: "var(--canvas)", border: "1px solid var(--border)", borderRadius: 8, cursor: "pointer", color: "var(--fg2)" }} hover={{ borderColor: "var(--border-strong)", color: "var(--fg1)" }} onClick={() => void ctl.createWorkspace()}>
                <IconPlus />
              </Hov>
            </div>
            <div style={{ font: `700 10px/1 ${FONT_SANS}`, letterSpacing: ".08em", textTransform: "uppercase", color: "var(--fg4)", padding: "8px 14px 6px" }}>워크스페이스</div>
            <div className="agentmux-scroll" style={{ flex: 1, overflow: "auto", padding: "0 8px 8px" }}>
              {workspaces.map((ws) => (
                <WorkspaceCard
                  key={ws.workspaceId}
                  ws={ws}
                  theme={T}
                  active={ws.workspaceId === activeWorkspaceId}
                  attentionCount={attentionByWorkspace.get(ws.workspaceId) ?? 0}
                  sessionCount={ws.workspaceId === activeWorkspaceId ? sessions.length : undefined}
                  running={ws.workspaceId === activeWorkspaceId && runningCount > 0}
                  onClick={() => void ctl.selectWorkspace(ws.workspaceId)}
                />
              ))}
              {workspaces.length === 0 ? (
                <div style={{ font: `400 11px/1.5 ${FONT_SANS}`, color: "var(--fg4)", padding: "6px 6px" }}>워크스페이스가 없습니다.</div>
              ) : null}

              <div style={{ font: `700 10px/1 ${FONT_SANS}`, letterSpacing: ".08em", textTransform: "uppercase", color: "var(--fg4)", padding: "16px 6px 6px" }}>원격 · SSH</div>
              {profiles.map((p) => (
                <Hov key={p.profileId} title={`접속: ${p.user}@${p.host}`} style={{ display: "flex", alignItems: "center", gap: 8, margin: "1px 0", padding: "7px 8px", borderRadius: 7, cursor: "pointer", color: "var(--fg2)" }} hover={{ background: "var(--s2)" }} onClick={() => void ctl.connectProfile(p)}>
                  <span style={{ color: "var(--fg4)", display: "flex" }}><IconServer /></span>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ font: `500 12px/1.3 ${FONT_SANS}` }}>{p.name}</div>
                    <div style={{ font: `400 10px/1.3 ${FONT_MONO}`, color: "var(--fg4)" }}>{p.user}@{p.host}</div>
                  </div>
                  <span style={{ width: 6, height: 6, borderRadius: "50%", background: T.fg4 }} />
                </Hov>
              ))}
              {profiles.length === 0 ? (
                <div style={{ font: `400 11px/1.5 ${FONT_SANS}`, color: "var(--fg4)", padding: "2px 8px" }}>등록된 프로필이 없습니다.</div>
              ) : null}
            </div>
            <Hov style={{ flex: "none", borderTop: "1px solid var(--border)", padding: "9px 14px", display: "flex", alignItems: "center", gap: 8, cursor: "pointer", color: "var(--fg3)" }} hover={{ background: "var(--s2)", color: "var(--fg1)" }} onClick={() => setOverlay("settings")}>
              <IconGear size={14} />
              <span style={{ font: `500 12px/1 ${FONT_SANS}` }}>설정</span>
            </Hov>
          </div>

          {/* right: tabs + mosaic */}
          <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", background: "var(--canvas)" }}>
            <div style={{ height: 38, flex: "none", display: "flex", alignItems: "stretch", background: "var(--surface)", borderBottom: "1px solid var(--border)", overflow: "hidden" }}>
              {terminalSurfaces.map((surface) => {
                const host = paneHostingSurface(surface.surfaceId);
                const on = host?.paneId === activePaneId;
                const session = surface.sessionId ? sessionById.get(surface.sessionId) : undefined;
                const att = session ? Boolean(attentionBySession.get(session.sessionId)) : false;
                return (
                  <Hov key={surface.surfaceId} style={{ display: "flex", alignItems: "center", gap: 7, padding: "0 11px 0 13px", maxWidth: 240, borderRight: "1px solid var(--border-subtle)", cursor: "pointer", background: on ? "var(--canvas)" : "transparent", boxShadow: on ? "inset 0 2px 0 var(--accent)" : "none" }} hover={on ? {} : { background: "var(--s2)" }} onClick={() => { if (host) void ctl.focusPane(host.paneId); }}>
                    <span style={{ color: "var(--fg4)", display: "flex", flex: "none" }}><IconShellArrow /></span>
                    <span style={{ font: `500 12px/1 ${FONT_SANS}`, color: on ? "var(--fg1)" : "var(--fg3)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{surface.title}</span>
                    {att ? <span style={{ width: 6, height: 6, borderRadius: "50%", background: T.warn, flex: "none" }} /> : null}
                    <Hov tag="span" style={{ width: 17, height: 17, borderRadius: 5, flex: "none", display: "flex", alignItems: "center", justifyContent: "center", color: "var(--fg4)" }} hover={{ background: "var(--s3)", color: "var(--fg1)" }} onClick={(e) => { e.stopPropagation(); if (host?.parentPaneId) void ctl.closePane(host.paneId); }}>
                      <IconClose size={10} />
                    </Hov>
                  </Hov>
                );
              })}
              <Hov style={{ width: 36, display: "flex", alignItems: "center", justifyContent: "center", cursor: "pointer", color: "var(--fg3)" }} hover={{ background: "var(--s2)", color: "var(--fg1)" }} onClick={() => void addTerminal()}>
                <IconPlus size={14} />
              </Hov>
              <div style={{ flex: 1 }} />
              <div style={{ display: "flex", alignItems: "center", gap: 1, padding: "0 8px" }}>
                <Hov tag="span" style={{ width: 26, height: 26, borderRadius: 6, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--fg4)", cursor: "pointer" }} hover={{ background: "var(--s2)", color: "var(--fg1)" }} onClick={() => { if (activePaneId) void splitPaneBy(activePaneId, "vertical"); }}><IconSplitCols size={13} /></Hov>
                <Hov tag="span" style={{ width: 26, height: 26, borderRadius: 6, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--fg4)", cursor: "pointer" }} hover={{ background: "var(--s2)", color: "var(--fg1)" }} onClick={() => { if (activePaneId) void splitPaneBy(activePaneId, "horizontal"); }}><IconSplitRows size={13} /></Hov>
              </div>
            </div>

            <div style={{ flex: 1, minHeight: 0, padding: 9, display: "flex" }}>
              {!ready ? (
                <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--fg4)", font: `500 13px/1 ${FONT_SANS}` }}>제어 플레인에 연결 중…</div>
              ) : rootPaneId ? (
                renderPane(rootPaneId)
              ) : (
                <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--fg4)", font: `500 13px/1 ${FONT_SANS}` }}>표시할 페인이 없습니다.</div>
              )}
            </div>
          </div>
        </div>

        {/* status bar */}
        <div style={{ height: 27, flex: "none", display: "flex", alignItems: "center", padding: "0 12px", background: "var(--surface)", borderTop: "1px solid var(--border)", fontFamily: FONT_MONO }}>
          <div style={{ display: "flex", alignItems: "center", gap: 6, color: "var(--fg3)" }}>
            <IconBranch size={12} />
            <span style={{ fontSize: 11, color: "var(--fg2)" }}>{activeWorkspace?.name ?? "—"}</span>
          </div>
          <div style={{ width: 1, height: 13, background: "var(--border)", margin: "0 12px" }} />
          <span style={{ fontSize: 10.5, color: "var(--fg4)" }}>{activeWorkspace?.projectRoot ?? ""}</span>
          <div style={{ flex: 1 }} />
          <span style={{ fontSize: 10.5, color: "var(--fg4)" }}>{terminalSurfaces.length}터미널 · {runningCount}실행</span>
          <div style={{ width: 1, height: 13, background: "var(--border)", margin: "0 12px" }} />
          <span style={{ fontSize: 11, color: "var(--fg3)" }}>{activeSessionState?.backendKind ?? "agentmux"}</span>
          <div style={{ width: 1, height: 13, background: "var(--border)", margin: "0 12px" }} />
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span style={{ width: 7, height: 7, borderRadius: "50%", background: sessionDotColor(T, activeSessionState, false), animation: "pulse 1.6s ease-in-out infinite" }} />
            <span style={{ fontSize: 11, color: sessionDotColor(T, activeSessionState, false) }}>{error ? "오류" : sessionLabel(activeSessionState, false) || "대기"}</span>
          </div>
        </div>

        {overlay === "palette" ? (
          <CommandPalette groups={groups} query={query} onQuery={setQuery} onClose={closeOverlay} stop={stop} />
        ) : null}
        {overlay === "search" ? <SearchOverlay onClose={closeOverlay} /> : null}
        {overlay === "settings" ? (
          <SettingsModal
            isDark={isDark}
            accentKey={accentKey}
            fontSize={fontSize}
            settingsTab={settingsTab}
            notifications={notifications}
            profiles={profiles}
            onClose={closeOverlay}
            stop={stop}
            setSettingsTab={setSettingsTab}
            setTheme={setTheme}
            setAccentKey={setAccentKey}
            setFontSize={setFontSize}
            onDismissNotification={(id) => void ctl.dismissNotification(id)}
            onCreateProfile={(input) => void ctl.createProfile(input)}
            onDeleteProfile={(id) => void ctl.deleteProfile(id)}
            onConnectProfile={(profile) => {
              void ctl.connectProfile(profile);
              closeOverlay();
            }}
          />
        ) : null}
      </div>
    </div>
  );
}

function WorkspaceCard({
  ws,
  theme,
  active,
  attentionCount,
  sessionCount,
  running,
  onClick
}: {
  ws: WorkspaceSummary;
  theme: ThemeTokens;
  active: boolean;
  attentionCount: number;
  sessionCount?: number;
  running: boolean;
  onClick: () => void;
}) {
  const needsInput = attentionCount > 0;
  const dot = needsInput ? theme.warn : running ? "var(--accent)" : theme.fg4;
  const statusText = needsInput
    ? "에이전트가 입력을 기다리는 중"
    : running
      ? "세션 실행 중"
      : sessionCount !== undefined
        ? `${sessionCount} 세션`
        : "대기 중";
  return (
    <Hov style={{ margin: "3px 0", padding: "10px 11px", borderRadius: 9, cursor: "pointer", background: active ? "var(--s2)" : "transparent", border: `1px solid ${active ? "var(--accent)" : "var(--border-subtle)"}` }} hover={active ? {} : { background: "var(--s2)" }} onClick={onClick}>
      <div style={{ display: "flex", alignItems: "center", gap: 7 }}>
        <span style={{ width: 8, height: 8, borderRadius: "50%", flex: "none", background: dot, boxShadow: running ? "0 0 6px var(--accent)" : "none" }} />
        <span style={{ font: `600 13px/1.2 ${FONT_SANS}`, color: "var(--fg1)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", flex: 1 }}>{ws.name}</span>
        {sessionCount !== undefined ? <span style={{ font: `500 9.5px/1 ${FONT_MONO}`, color: "var(--fg4)", flex: "none", whiteSpace: "nowrap" }}>{sessionCount}</span> : null}
      </div>
      <div style={{ font: `400 11px/1.35 ${FONT_SANS}`, color: needsInput ? theme.warn : "var(--fg2)", marginTop: 5 }}>{statusText}</div>
      {needsInput ? (
        <div style={{ display: "inline-flex", alignItems: "center", gap: 4, marginTop: 7, padding: "3px 7px", borderRadius: 5, background: "var(--accent-soft)" }}>
          <span style={{ color: theme.warn, display: "flex" }}><IconBubble /></span>
          <span style={{ font: `600 10px/1 ${FONT_SANS}`, color: theme.warn, whiteSpace: "nowrap" }}>입력 필요</span>
        </div>
      ) : null}
      {ws.projectRoot ? (
        <div style={{ display: "flex", alignItems: "center", gap: 5, marginTop: 7, font: `400 10px/1.2 ${FONT_MONO}`, color: "var(--fg4)" }}>
          <IconBranch size={10} />
          <span style={{ whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{ws.projectRoot}</span>
        </div>
      ) : null}
    </Hov>
  );
}

function Bar() {
  return <span style={{ color: "var(--border-strong)" }}>│</span>;
}

function OmcBar({ telemetry, theme }: { telemetry: AgentTelemetry; theme: ThemeTokens }) {
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
    </span>
  );
  if (activity) {
    push(
      <span key="activity" style={{ color: activityColor, fontWeight: 600 }}>
        {activity}
      </span>
    );
  }
  if (telemetry.session) push(<span key="session" style={{ color: "var(--fg3)" }}>session:{telemetry.session}</span>);
  if (telemetry.cost) push(<span key="cost" style={{ color: "var(--fg3)" }}>{telemetry.cost}</span>);
  if (telemetry.tokens) push(<span key="tokens" style={{ color: "var(--fg3)" }}>{telemetry.tokens}</span>);
  if (telemetry.cache) push(<span key="cache" style={{ color: "var(--fg3)" }}>Cache: {telemetry.cache}</span>);
  if (telemetry.rate) push(<span key="rate" style={{ color: "var(--fg3)" }}>{telemetry.rate}</span>);
  if (telemetry.ctx) push(<span key="ctx" style={{ color: "var(--accent)" }}>ctx:{telemetry.ctx}</span>);

  return (
    <div style={{ flex: "none", display: "flex", alignItems: "center", gap: 7, flexWrap: "wrap", padding: "6px 12px", borderTop: "1px solid var(--border-subtle)", background: "var(--surface)", fontFamily: FONT_MONO, fontSize: 11 }}>
      {parts}
      <div style={{ flex: 1 }} />
      <span style={{ color: "var(--fg4)" }}>/rc active</span>
    </div>
  );
}

function CommandPalette({ groups, query, onQuery, onClose, stop }: { groups: PaletteGroup[]; query: string; onQuery: (value: string) => void; onClose: () => void; stop: (e: { stopPropagation: () => void }) => void }) {
  return (
    <div onClick={onClose} style={{ position: "absolute", inset: 0, background: "rgba(0,0,0,0.5)", display: "flex", justifyContent: "center", paddingTop: 88, zIndex: 40, animation: "fadein .12s ease" }}>
      <div onClick={stop} style={{ width: 620, maxWidth: "90%", height: "max-content", maxHeight: 440, background: "var(--surface)", border: "1px solid var(--border-strong)", borderRadius: 13, boxShadow: "0 30px 80px rgba(0,0,0,0.45)", display: "flex", flexDirection: "column", overflow: "hidden" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 11, padding: "15px 17px", borderBottom: "1px solid var(--border)" }}>
          <span style={{ color: "var(--fg4)", display: "flex" }}><IconSearch size={17} /></span>
          <input value={query} onChange={(e) => onQuery(e.target.value)} autoFocus placeholder="명령 실행 또는 워크스페이스 검색…" style={{ flex: 1, border: 0, outline: "none", background: "transparent", font: `400 15px/1 ${FONT_SANS}`, color: "var(--fg1)" }} />
          <span style={{ font: `600 10px/1 ${FONT_MONO}`, background: "var(--s2)", border: "1px solid var(--border)", borderRadius: 4, padding: "4px 6px", color: "var(--fg3)" }}>esc</span>
        </div>
        <div className="agentmux-scroll" style={{ flex: 1, overflow: "auto", padding: 7 }}>
          {groups.map((g) => (
            <div key={g.label}>
              <div style={{ font: `700 9.5px/1 ${FONT_SANS}`, letterSpacing: ".09em", textTransform: "uppercase", color: "var(--fg4)", padding: "9px 10px 5px" }}>{g.label}</div>
              {g.items.map((it) => (
                <Hov key={it.title} style={{ display: "flex", alignItems: "center", gap: 10, padding: "9px 11px", borderRadius: 8, cursor: "pointer", background: it.highlighted ? "var(--accent-soft)" : "transparent", borderLeft: it.highlighted ? "2px solid var(--accent)" : "2px solid transparent" }} hover={{ background: "var(--s2)" }} onClick={it.onClick}>
                  <IconChevronRight />
                  <span style={{ flex: 1, font: `500 13.5px/1 ${FONT_SANS}`, color: "var(--fg1)" }}>{it.title}</span>
                  <span style={{ font: `500 10.5px/1 ${FONT_MONO}`, color: "var(--fg4)" }}>{it.hint}</span>
                </Hov>
              ))}
            </div>
          ))}
          {groups.length === 0 ? <div style={{ padding: "14px", color: "var(--fg4)", font: `400 13px/1 ${FONT_SANS}` }}>결과 없음</div> : null}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 16, padding: "9px 15px", borderTop: "1px solid var(--border)", background: "var(--bg)", font: `500 10.5px/1 ${FONT_SANS}`, color: "var(--fg4)" }}>
          <span>↑↓ 이동</span><span>↵ 실행</span><span>esc 닫기</span>
        </div>
      </div>
    </div>
  );
}

function SearchOverlay({ onClose }: { onClose: () => void }) {
  const navBtn: CSSProperties = { width: 26, height: 26, borderRadius: 6, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--fg3)", cursor: "pointer" };
  const navHover: CSSProperties = { background: "var(--s2)", color: "var(--fg1)" };
  return (
    <div style={{ position: "absolute", top: 48, right: 18, width: 430, maxWidth: "80%", background: "var(--surface)", border: "1px solid var(--border-strong)", borderRadius: 9, boxShadow: "0 18px 44px rgba(0,0,0,0.35)", display: "flex", alignItems: "center", gap: 8, padding: "9px 10px", zIndex: 40, animation: "fadein .12s ease" }}>
      <span style={{ color: "var(--fg4)", display: "flex" }}><IconSearch size={15} /></span>
      <input autoFocus placeholder="활성 창에서 검색" style={{ flex: 1, border: 0, outline: "none", background: "transparent", font: `400 13px/1 ${FONT_MONO}`, color: "var(--fg1)" }} />
      <div style={{ display: "flex", gap: 1 }}>
        <Hov tag="span" style={navBtn} hover={navHover}><IconChevronUp /></Hov>
        <Hov tag="span" style={navBtn} hover={navHover}><IconChevronDown /></Hov>
      </div>
      <div style={{ width: 1, height: 16, background: "var(--border)" }} />
      <Hov tag="span" style={navBtn} hover={navHover} onClick={onClose}><IconClose /></Hov>
    </div>
  );
}

interface SettingsModalProps {
  isDark: boolean;
  accentKey: string;
  fontSize: number;
  settingsTab: SettingsTab;
  notifications: NotificationSummary[];
  profiles: SshProfile[];
  onClose: () => void;
  stop: (e: { stopPropagation: () => void }) => void;
  setSettingsTab: (tab: SettingsTab) => void;
  setTheme: (theme: ThemeName) => void;
  setAccentKey: (key: string) => void;
  setFontSize: (size: number) => void;
  onDismissNotification: (id: string) => void;
  onCreateProfile: (input: SshProfileInput) => void;
  onDeleteProfile: (id: string) => void;
  onConnectProfile: (profile: SshProfile) => void;
}

function SettingsModal(props: SettingsModalProps) {
  const { isDark, accentKey, fontSize, settingsTab, notifications, profiles, onClose, stop, setSettingsTab, setTheme, setAccentKey, setFontSize, onDismissNotification, onCreateProfile, onDeleteProfile, onConnectProfile } = props;

  const promptNewProfile = () => {
    const name = window.prompt("프로필 이름")?.trim();
    if (!name) return;
    const host = window.prompt("호스트 (예: 10.0.0.1)")?.trim();
    if (!host) return;
    const user = window.prompt("사용자")?.trim();
    if (!user) return;
    onCreateProfile({ name, host, user, port: 22 });
  };
  const tabs: { key: SettingsTab; label: string }[] = [
    { key: "general", label: "일반" },
    { key: "appearance", label: "모양" },
    { key: "profiles", label: "프로필 · SSH" },
    { key: "keys", label: "단축키" }
  ];

  return (
    <div onClick={onClose} style={{ position: "absolute", inset: 0, background: "rgba(0,0,0,0.5)", display: "flex", alignItems: "center", justifyContent: "center", zIndex: 40, animation: "fadein .12s ease" }}>
      <div onClick={stop} style={{ width: 780, maxWidth: "92%", height: 560, maxHeight: "88%", background: "var(--surface)", border: "1px solid var(--border-strong)", borderRadius: 13, boxShadow: "0 30px 80px rgba(0,0,0,0.45)", display: "flex", overflow: "hidden" }}>
        <div style={{ width: 188, flex: "none", background: "var(--bg)", borderRight: "1px solid var(--border)", padding: "16px 10px", display: "flex", flexDirection: "column", gap: 2 }}>
          <div style={{ font: `700 14px/1 ${FONT_SANS}`, color: "var(--fg1)", padding: "4px 10px 14px" }}>설정</div>
          {tabs.map((t) => {
            const on = settingsTab === t.key;
            return (
              <Hov key={t.key} style={{ padding: "9px 11px", borderRadius: 8, cursor: "pointer", font: `500 13px/1 ${FONT_SANS}`, color: on ? "var(--fg1)" : "var(--fg3)", background: on ? "var(--s2)" : "transparent" }} hover={on ? {} : { background: "var(--s2)" }} onClick={() => setSettingsTab(t.key)}>
                {t.label}
              </Hov>
            );
          })}
        </div>
        <div className="agentmux-scroll" style={{ flex: 1, overflow: "auto", padding: "24px 28px", position: "relative" }}>
          <Hov tag="span" style={{ position: "absolute", top: 16, right: 16, width: 28, height: 28, borderRadius: 7, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--fg3)", cursor: "pointer" }} hover={{ background: "var(--s2)", color: "var(--fg1)" }} onClick={onClose}>
            <IconClose size={14} />
          </Hov>

          {settingsTab === "appearance" ? (
            <>
              <div style={{ font: `700 18px/1 ${FONT_SANS}`, color: "var(--fg1)", marginBottom: 22 }}>모양</div>
              <div style={{ font: `600 12px/1 ${FONT_SANS}`, color: "var(--fg2)", marginBottom: 8 }}>테마</div>
              <div style={{ display: "flex", gap: 10, marginBottom: 24 }}>
                <div onClick={() => setTheme("dark")} style={{ flex: 1, border: `1px solid ${isDark ? "var(--accent)" : "var(--border)"}`, borderRadius: 9, padding: 12, cursor: "pointer", background: isDark ? "var(--accent-soft)" : "transparent" }}>
                  <div style={{ height: 48, borderRadius: 6, background: "#0B0B0D", border: "1px solid #27272A", marginBottom: 9, display: "flex", alignItems: "center", padding: "0 8px", gap: 5 }}>
                    <span style={{ width: 7, height: 7, borderRadius: "50%", background: "var(--accent)" }} />
                    <span style={{ flex: 1, height: 5, borderRadius: 3, background: "#27272A" }} />
                  </div>
                  <span style={{ font: `600 12px/1 ${FONT_SANS}`, color: "var(--fg1)" }}>다크</span>
                </div>
                <div onClick={() => setTheme("light")} style={{ flex: 1, border: `1px solid ${!isDark ? "var(--accent)" : "var(--border)"}`, borderRadius: 9, padding: 12, cursor: "pointer", background: !isDark ? "var(--accent-soft)" : "transparent" }}>
                  <div style={{ height: 48, borderRadius: 6, background: "#FFFFFF", border: "1px solid #E4E4E7", marginBottom: 9, display: "flex", alignItems: "center", padding: "0 8px", gap: 5 }}>
                    <span style={{ width: 7, height: 7, borderRadius: "50%", background: "var(--accent)" }} />
                    <span style={{ flex: 1, height: 5, borderRadius: 3, background: "#E4E4E7" }} />
                  </div>
                  <span style={{ font: `600 12px/1 ${FONT_SANS}`, color: "var(--fg1)" }}>라이트</span>
                </div>
              </div>
              <div style={{ font: `600 12px/1 ${FONT_SANS}`, color: "var(--fg2)", marginBottom: 10 }}>액센트 색상</div>
              <div style={{ display: "flex", gap: 10, marginBottom: 24 }}>
                {ACCENTS.map((a) => (
                  <div key={a.key} onClick={() => setAccentKey(a.key)} style={{ display: "flex", alignItems: "center", gap: 8, border: `1px solid ${a.key === accentKey ? "var(--accent)" : "var(--border)"}`, borderRadius: 8, padding: "8px 12px", cursor: "pointer" }}>
                    <span style={{ width: 18, height: 18, borderRadius: 6, background: a.hex }} />
                    <span style={{ font: `500 12px/1 ${FONT_SANS}`, color: "var(--fg1)" }}>{a.label}</span>
                  </div>
                ))}
              </div>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 9 }}>
                <span style={{ font: `600 12px/1 ${FONT_SANS}`, color: "var(--fg2)" }}>UI 글자 크기</span>
                <span style={{ font: `600 12px/1 ${FONT_MONO}`, color: "var(--accent)" }}>{fontSize}px</span>
              </div>
              <input type="range" min={11} max={16} step={0.5} value={fontSize} onChange={(e) => setFontSize(parseFloat(e.target.value))} style={{ width: "100%", accentColor: "var(--accent)", marginBottom: 24 }} />
            </>
          ) : null}

          {settingsTab === "profiles" ? (
            <>
              <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 20 }}>
                <div style={{ font: `700 18px/1 ${FONT_SANS}`, color: "var(--fg1)" }}>프로필 · SSH</div>
                <button type="button" onClick={promptNewProfile} style={{ display: "flex", alignItems: "center", gap: 6, background: "var(--accent)", color: "#fff", border: 0, borderRadius: 8, padding: "8px 13px", cursor: "pointer", font: `600 12px/1 ${FONT_SANS}` }}>
                  <IconPlus size={13} />프로필 추가
                </button>
              </div>
              <div style={{ border: "1px solid var(--border)", borderRadius: 8, overflow: "hidden" }}>
                <div style={{ display: "grid", gridTemplateColumns: "1.2fr 1.4fr 0.9fr auto", padding: "10px 14px", background: "var(--bg)", borderBottom: "1px solid var(--border)", font: `700 10.5px/1 ${FONT_SANS}`, letterSpacing: ".05em", textTransform: "uppercase", color: "var(--fg4)" }}>
                  <span>이름</span><span>호스트</span><span>사용자</span><span style={{ textAlign: "right" }}>동작</span>
                </div>
                {profiles.map((p) => (
                  <div key={p.profileId} style={{ display: "grid", gridTemplateColumns: "1.2fr 1.4fr 0.9fr auto", alignItems: "center", padding: "12px 14px", borderBottom: "1px solid var(--border-subtle)" }}>
                    <span style={{ font: `600 12.5px/1 ${FONT_SANS}`, color: "var(--fg1)" }}>{p.name}</span>
                    <span style={{ font: `400 12px/1 ${FONT_MONO}`, color: "var(--fg3)" }}>{p.host}{p.port ? `:${p.port}` : ""}</span>
                    <span style={{ font: `400 12px/1 ${FONT_MONO}`, color: "var(--fg3)" }}>{p.user}</span>
                    <div style={{ justifySelf: "end", display: "flex", alignItems: "center", gap: 6 }}>
                      <button type="button" onClick={() => onConnectProfile(p)} style={{ background: "var(--accent-soft)", color: "var(--accent)", border: 0, borderRadius: 6, padding: "5px 10px", cursor: "pointer", font: `600 11px/1 ${FONT_SANS}` }}>접속</button>
                      <Hov tag="span" style={{ width: 26, height: 26, borderRadius: 6, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--fg4)", cursor: "pointer" }} hover={{ background: "var(--s2)", color: "var(--red, #F87171)" }} onClick={() => onDeleteProfile(p.profileId)}>
                        <IconClose size={12} />
                      </Hov>
                    </div>
                  </div>
                ))}
                {profiles.length === 0 ? (
                  <div style={{ padding: "14px", font: `400 12px/1 ${FONT_SANS}`, color: "var(--fg4)" }}>등록된 프로필이 없습니다.</div>
                ) : null}
              </div>
              <div style={{ marginTop: 12, font: `400 11px/1.5 ${FONT_SANS}`, color: "var(--fg4)" }}>프로필은 control plane에 저장됩니다. SSH 직접 연결(전송 백엔드)은 후속 작업입니다.</div>
            </>
          ) : null}

          {settingsTab === "keys" ? (
            <>
              <div style={{ font: `700 18px/1 ${FONT_SANS}`, color: "var(--fg1)", marginBottom: 20 }}>단축키</div>
              <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
                {KEYMAPS.map((km) => (
                  <div key={km.k} style={{ display: "flex", alignItems: "center", justifyContent: "space-between", padding: "11px 4px", borderBottom: "1px solid var(--border-subtle)" }}>
                    <span style={{ font: `500 13px/1 ${FONT_SANS}`, color: "var(--fg2)" }}>{km.k}</span>
                    <span style={{ font: `600 11px/1 ${FONT_MONO}`, background: "var(--s2)", border: "1px solid var(--border)", borderRadius: 5, padding: "5px 9px", color: "var(--fg2)" }}>{km.v}</span>
                  </div>
                ))}
              </div>
            </>
          ) : null}

          {settingsTab === "general" ? (
            <>
              <div style={{ font: `700 18px/1 ${FONT_SANS}`, color: "var(--fg1)", marginBottom: 20 }}>일반 · 알림</div>
              {notifications.length === 0 ? (
                <div style={{ font: `400 12px/1.5 ${FONT_SANS}`, color: "var(--fg4)" }}>활성 알림이 없습니다.</div>
              ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                  {notifications.map((n) => (
                    <div key={n.notificationId} style={{ display: "flex", alignItems: "center", justifyContent: "space-between", padding: "12px 14px", border: "1px solid var(--border)", borderRadius: 8 }}>
                      <div style={{ minWidth: 0 }}>
                        <div style={{ font: `600 12.5px/1.3 ${FONT_SANS}`, color: "var(--fg1)" }}>{n.title}</div>
                        <div style={{ font: `400 11.5px/1.4 ${FONT_SANS}`, color: "var(--fg4)", marginTop: 3, overflow: "hidden", textOverflow: "ellipsis" }}>{n.message}</div>
                      </div>
                      <button type="button" onClick={() => onDismissNotification(n.notificationId)} style={{ flex: "none", marginLeft: 12, background: "var(--s2)", border: "1px solid var(--border)", borderRadius: 7, padding: "6px 10px", cursor: "pointer", font: `500 11px/1 ${FONT_SANS}`, color: "var(--fg2)" }}>닫기</button>
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
