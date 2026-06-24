import { type CSSProperties, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  type AgentState,
  type BrowserClickTarget,
  createControlClient,
  type NotificationSummary,
  type PaneSummary,
  type SurfaceSummary,
  type TerminalSession,
  type WorkspaceDetail,
  type WorkspaceSummary,
  type WslDistribution
} from "./control/ControlClient";
import { XtermTerminalRenderer } from "./terminal/XtermTerminalRenderer";

const encoder = new TextEncoder();
const DEFAULT_PROJECT_ROOT = "D:\\Workspace\\irae\\agentmux";
const DEFAULT_WORKSPACE_NAME = "Workspace 1";

interface BrowserPaneState {
  url: string;
  selector: string;
  text: string;
  x: string;
  y: string;
  script: string;
  currentUrl?: string;
  snapshot?: string;
  screenshotHandle?: string;
  screenshotBytes?: number;
  evalValue?: string;
  lastAction?: string;
  error?: string;
}

const DEFAULT_BROWSER_STATE: BrowserPaneState = {
  url: "https://example.invalid",
  selector: "#q",
  text: "agentmux",
  x: "12",
  y: "24",
  script: "document.title"
};

export function App() {
  const rendererRef = useRef<XtermTerminalRenderer | null>(null);
  const sessionRef = useRef<TerminalSession | null>(null);
  const activeWorkspaceRef = useRef<WorkspaceSummary | null>(null);
  const renderedLengthRef = useRef(0);
  const pollTimerRef = useRef<number | undefined>();
  const client = useMemo(() => createControlClient(), []);
  const [terminalHost, setTerminalHost] = useState<HTMLDivElement | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceSummary[]>([]);
  const [activeWorkspace, setActiveWorkspaceState] = useState<WorkspaceSummary | null>(null);
  const [panes, setPanes] = useState<PaneSummary[]>([]);
  const [surfaces, setSurfaces] = useState<SurfaceSummary[]>([]);
  const [wslDistributions, setWslDistributions] = useState<WslDistribution[]>([]);
  const [selectedWslDistribution, setSelectedWslDistribution] = useState("");
  const [selectedSurfaceId, setSelectedSurfaceId] = useState("");
  const [workspaceClosePolicy, setWorkspaceClosePolicy] = useState("fail_if_running");
  const [paneSurfacePolicy, setPaneSurfacePolicy] = useState("fail_if_session_running");
  const [attentionSessions, setAttentionSessions] = useState<AgentState[]>([]);
  const [notifications, setNotifications] = useState<NotificationSummary[]>([]);
  const [notificationSeverity, setNotificationSeverity] = useState("");
  const [browserPaneStates, setBrowserPaneStates] = useState<Record<string, BrowserPaneState>>({});
  const [session, setSession] = useState<TerminalSession>({
    sessionId: "ses_pending",
    backendKind: "conpty",
    state: "idle"
  });

  const setTerminalSurfaceElement = useCallback((node: HTMLDivElement | null) => {
    setTerminalHost(node);
  }, []);

  useEffect(() => {
    if (!terminalHost) {
      rendererRef.current?.dispose();
      rendererRef.current = null;
      return;
    }

    const renderer = new XtermTerminalRenderer();
    renderer.mount(terminalHost, {
      columns: 120,
      rows: 30,
      bytes: encoder.encode("")
    });
    rendererRef.current = renderer;

    const unsubscribeInput = renderer.onData((data) => {
      const activeSession = sessionRef.current;
      if (!activeSession) {
        renderer.write(encoder.encode(data));
        return;
      }

      client.sendText(activeSession.sessionId, data).catch(() => {
        setSession((current) => ({ ...current, state: "failed" }));
      });
    });
    const unsubscribeResize = renderer.onResize((columns, rows) => {
      const activeSession = sessionRef.current;
      if (!activeSession) {
        return;
      }

      client.resize(activeSession.sessionId, columns, rows).catch(() => {
        setSession((current) => ({ ...current, state: "failed" }));
      });
    });

    const resizeObserver = new ResizeObserver(() => renderer.fit());
    resizeObserver.observe(terminalHost);

    const activeSession = sessionRef.current;
    if (activeSession) {
      renderedLengthRef.current = 0;
      refreshTerminalOutput(activeSession.sessionId).catch(() => {
        setSession((current) => ({ ...current, state: "failed" }));
      });
    }

    return () => {
      unsubscribeInput();
      unsubscribeResize();
      resizeObserver.disconnect();
      renderer.dispose();
      if (rendererRef.current === renderer) {
        rendererRef.current = null;
      }
    };
  }, [client, terminalHost]);

  useEffect(() => {
    return () => {
      stopPolling();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;

    async function hydrateWorkspaceList() {
      try {
        const listed = await client.listWorkspaces();
        const workspace =
          listed[0] ?? (await client.createWorkspace(DEFAULT_WORKSPACE_NAME, DEFAULT_PROJECT_ROOT));
        const detail = await client.getWorkspace(workspace.workspaceId);

        if (cancelled) {
          return;
        }

        setWorkspaces(listed.length > 0 ? listed : [workspace]);
        applyWorkspaceDetail(detail);
      } catch {
        if (!cancelled) {
          setSession((current) => ({ ...current, state: "failed" }));
        }
      }
    }

    hydrateWorkspaceList();

    return () => {
      cancelled = true;
    };
  }, [client]);

  useEffect(() => {
    let cancelled = false;

    async function hydrateWslDistributions() {
      try {
        const distributions = await client.listWslDistributions();
        if (cancelled) {
          return;
        }

        setWslDistributions(distributions);
        const selected =
          distributions.find((distribution) => distribution.isDefault)?.name ??
          distributions[0]?.name ??
          "";
        setSelectedWslDistribution(selected);
      } catch {
        if (!cancelled) {
          setWslDistributions([]);
          setSelectedWslDistribution("");
        }
      }
    }

    hydrateWslDistributions();

    return () => {
      cancelled = true;
    };
  }, [client]);

  useEffect(() => {
    let cancelled = false;

    async function refresh() {
      if (cancelled) {
        return;
      }
      const refreshed = await refreshAgentSignals();
      if (!refreshed && !cancelled) {
        setAttentionSessions([]);
        setNotifications([]);
      }
    }

    void refresh();
    const timer = window.setInterval(() => {
      void refresh();
    }, 1500);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [client, activeWorkspace?.workspaceId, notificationSeverity]);

  async function refreshAgentSignals(): Promise<boolean> {
    try {
      const workspaceId = activeWorkspaceRef.current?.workspaceId ?? null;
      const [nextAttention, nextNotifications] = await Promise.all([
        client.listAgentAttention(null),
        client.listNotifications({
          workspaceId,
          severity: notificationSeverity || null,
          includeDismissed: false
        })
      ]);

      setAttentionSessions(nextAttention);
      setNotifications(nextNotifications);
      return true;
    } catch {
      return false;
    }
  }

  async function clearAttention(sessionId: string) {
    try {
      await client.clearAgentAttention(sessionId);
      await refreshAgentSignals();
    } catch (error) {
      window.alert(error instanceof Error ? error.message : "Attention clear failed.");
    }
  }

  async function dismissNotification(notificationId: string) {
    try {
      await client.dismissNotification(notificationId);
      await refreshAgentSignals();
    } catch (error) {
      window.alert(error instanceof Error ? error.message : "Notification dismiss failed.");
    }
  }

  async function openNativeShell() {
    try {
      const workspace = await ensureWorkspace();
      const nextSession = await client.spawnNativeTerminal(workspace.workspaceId, [
        "cmd.exe",
        "/d",
        "/q"
      ]);
      const detail = await client.getWorkspace(workspace.workspaceId);
      applyWorkspaceDetail(detail, nextSession.sessionId);
      await refreshTerminalOutput(nextSession.sessionId);
      rendererRef.current?.focus();
    } catch {
      stopPolling();
      setSession((current) => ({ ...current, state: "failed" }));
    }
  }

  async function openWslShell() {
    if (!selectedWslDistribution) {
      return;
    }

    try {
      const workspace = await ensureWorkspace();
      const nextSession = await client.spawnWslTerminal(
        workspace.workspaceId,
        selectedWslDistribution,
        workspace.projectRoot ?? DEFAULT_PROJECT_ROOT
      );
      const detail = await client.getWorkspace(workspace.workspaceId);
      applyWorkspaceDetail(detail, nextSession.sessionId);
      await refreshTerminalOutput(nextSession.sessionId);
      rendererRef.current?.focus();
    } catch {
      stopPolling();
      setSession((current) => ({ ...current, state: "failed" }));
    }
  }

  async function createBrowserSurface() {
    try {
      const workspace = await ensureWorkspace();
      const paneId = workspace.activePaneId;
      const surface = await client.createBrowserSurface(workspace.workspaceId, paneId, "default");
      const detail = await client.getWorkspace(workspace.workspaceId);
      setBrowserPaneState(surface.surfaceId, {
        currentUrl: "about:blank",
        lastAction: "Created"
      });
      applyWorkspaceDetail(detail);
    } catch (error) {
      window.alert(error instanceof Error ? error.message : "Browser surface create failed.");
    }
  }

  async function createWorkspace() {
    try {
      const workspace = await client.createWorkspace(
        `Workspace ${workspaces.length + 1}`,
        DEFAULT_PROJECT_ROOT
      );
      const detail = await client.getWorkspace(workspace.workspaceId);
      applyWorkspaceDetail(detail);
    } catch {
      setSession((current) => ({ ...current, state: "failed" }));
    }
  }

  async function selectWorkspace(workspaceId: string) {
    try {
      const detail = await client.getWorkspace(workspaceId);
      applyWorkspaceDetail(detail);
    } catch {
      setSession((current) => ({ ...current, state: "failed" }));
    }
  }

  async function renameActiveWorkspace() {
    const workspace = activeWorkspaceRef.current;
    if (!workspace) {
      return;
    }

    const name = window.prompt("Workspace name", workspace.name)?.trim();
    if (!name) {
      return;
    }

    try {
      const renamed = await client.renameWorkspace(workspace.workspaceId, name);
      const detail = await client.getWorkspace(renamed.workspaceId);
      applyWorkspaceDetail(detail);
    } catch (error) {
      window.alert(error instanceof Error ? error.message : "Workspace rename failed.");
    }
  }

  async function closeActiveWorkspace() {
    const workspace = activeWorkspaceRef.current;
    if (!workspace || !window.confirm("Close workspace?")) {
      return;
    }

    try {
      const closed = await client.closeWorkspace(workspace.workspaceId, workspaceClosePolicy);
      if (!closed) {
        return;
      }

      stopPolling();
      sessionRef.current = null;
      activeWorkspaceRef.current = null;
      setWorkspaces((current) =>
        current.filter((candidate) => candidate.workspaceId !== workspace.workspaceId)
      );

      const listed = await client.listWorkspaces();
      const next =
        listed[0] ?? (await client.createWorkspace(DEFAULT_WORKSPACE_NAME, DEFAULT_PROJECT_ROOT));
      const detail = await client.getWorkspace(next.workspaceId);
      setWorkspaces(listed.length > 0 ? listed : [next]);
      applyWorkspaceDetail(detail);
    } catch (error) {
      window.alert(error instanceof Error ? error.message : "Workspace close failed.");
    }
  }

  async function splitActivePane(axis: "horizontal" | "vertical") {
    const workspace = activeWorkspaceRef.current;
    if (!workspace) {
      return;
    }

    try {
      const detail = await client.splitPane(workspace.workspaceId, workspace.activePaneId, axis);
      applyWorkspaceDetail(detail);
      rendererRef.current?.fit();
    } catch {
      setSession((current) => ({ ...current, state: "failed" }));
    }
  }

  async function closeActivePane() {
    const workspace = activeWorkspaceRef.current;
    if (!workspace) {
      return;
    }

    try {
      const detail = await client.closePane(
        workspace.workspaceId,
        workspace.activePaneId,
        paneSurfacePolicy
      );
      applyWorkspaceDetail(detail);
    } catch (error) {
      window.alert(error instanceof Error ? error.message : "Pane close failed.");
    }
  }

  async function resizeSplitPane(paneId: string, ratio: number) {
    const workspace = activeWorkspaceRef.current;
    if (!workspace) {
      return;
    }

    try {
      const detail = await client.resizePaneLayout(workspace.workspaceId, paneId, ratio);
      applyWorkspaceDetail(detail, sessionRef.current?.sessionId, false);
      rendererRef.current?.fit();
    } catch {
      setSession((current) => ({ ...current, state: "failed" }));
    }
  }

  async function mountSelectedSurface() {
    const workspace = activeWorkspaceRef.current;
    const surfaceId = selectedSurfaceId || unmountedSurfaces[0]?.surfaceId;
    if (!workspace || !surfaceId) {
      return;
    }

    try {
      const detail = await client.mountSurface(workspace.workspaceId, workspace.activePaneId, surfaceId);
      const surface = detail.surfaces.find((candidate) => candidate.surfaceId === surfaceId);
      applyWorkspaceDetail(detail, surface?.sessionId ?? undefined);
      rendererRef.current?.focus();
    } catch (error) {
      window.alert(error instanceof Error ? error.message : "Surface mount failed.");
    }
  }

  async function unmountActiveSurface() {
    const workspace = activeWorkspaceRef.current;
    if (!workspace) {
      return;
    }

    try {
      const detail = await client.unmountSurface(workspace.workspaceId, workspace.activePaneId);
      applyWorkspaceDetail(detail);
    } catch (error) {
      window.alert(error instanceof Error ? error.message : "Surface unmount failed.");
    }
  }

  async function focusPane(paneId: string) {
    const workspace = activeWorkspaceRef.current;
    if (!workspace || workspace.activePaneId === paneId) {
      rendererRef.current?.focus();
      return;
    }

    try {
      const detail = await client.focusPane(workspace.workspaceId, paneId);
      applyWorkspaceDetail(detail);
      rendererRef.current?.focus();
    } catch {
      setSession((current) => ({ ...current, state: "failed" }));
    }
  }

  function setBrowserPaneState(surfaceId: string, patch: Partial<BrowserPaneState>) {
    setBrowserPaneStates((current) => ({
      ...current,
      [surfaceId]: {
        ...DEFAULT_BROWSER_STATE,
        ...current[surfaceId],
        ...patch
      }
    }));
  }

  async function runBrowserAction(
    surfaceId: string,
    action: () => Promise<Partial<BrowserPaneState>>
  ) {
    try {
      setBrowserPaneState(surfaceId, { error: undefined });
      const patch = await action();
      setBrowserPaneState(surfaceId, {
        ...patch,
        error: undefined
      });
    } catch (error) {
      setBrowserPaneState(surfaceId, {
        error: error instanceof Error ? error.message : "Browser action failed."
      });
      await refreshAgentSignals();
    }
  }

  async function navigateBrowser(surfaceId: string) {
    const state = browserPaneStates[surfaceId] ?? DEFAULT_BROWSER_STATE;
    const url = state.url.trim() || "about:blank";
    await runBrowserAction(surfaceId, async () => {
      const result = await client.browserNavigate(surfaceId, url);
      return {
        currentUrl: result.url,
        lastAction: "Navigated"
      };
    });
  }

  async function snapshotBrowser(surfaceId: string) {
    await runBrowserAction(surfaceId, async () => {
      const result = await client.browserDomSnapshot(surfaceId);
      return {
        snapshot: result.html,
        lastAction: "Snapshot"
      };
    });
  }

  async function screenshotBrowser(surfaceId: string) {
    await runBrowserAction(surfaceId, async () => {
      const result = await client.browserScreenshot(surfaceId, "png");
      return {
        screenshotHandle: result.imageHandle,
        screenshotBytes: result.byteCount,
        lastAction: "Screenshot"
      };
    });
  }

  async function clickBrowser(surfaceId: string) {
    const state = browserPaneStates[surfaceId] ?? DEFAULT_BROWSER_STATE;
    const selector = state.selector.trim();
    const target: BrowserClickTarget = selector
      ? { selector }
      : { x: Number(state.x), y: Number(state.y) };
    await runBrowserAction(surfaceId, async () => {
      await client.browserClick(surfaceId, target);
      return {
        lastAction: "Clicked"
      };
    });
  }

  async function clickBrowserPoint(surfaceId: string) {
    const state = browserPaneStates[surfaceId] ?? DEFAULT_BROWSER_STATE;
    await runBrowserAction(surfaceId, async () => {
      await client.browserClick(surfaceId, {
        x: Number(state.x),
        y: Number(state.y)
      });
      return {
        lastAction: "Clicked point"
      };
    });
  }

  async function typeBrowser(surfaceId: string) {
    const state = browserPaneStates[surfaceId] ?? DEFAULT_BROWSER_STATE;
    await runBrowserAction(surfaceId, async () => {
      await client.browserType(surfaceId, state.selector, state.text);
      return {
        lastAction: "Typed"
      };
    });
  }

  async function evaluateBrowser(surfaceId: string) {
    const state = browserPaneStates[surfaceId] ?? DEFAULT_BROWSER_STATE;
    await runBrowserAction(surfaceId, async () => {
      const result = await client.browserEvaluate(surfaceId, state.script);
      return {
        evalValue: result.valueJson,
        lastAction: "Evaluated"
      };
    });
  }

  async function ensureWorkspace(): Promise<WorkspaceSummary> {
    const active = activeWorkspaceRef.current;
    if (active) {
      return active;
    }

    const listed = await client.listWorkspaces();
    const workspace =
      listed[0] ?? (await client.createWorkspace(DEFAULT_WORKSPACE_NAME, DEFAULT_PROJECT_ROOT));
    const detail = await client.getWorkspace(workspace.workspaceId);
    setWorkspaces(listed.length > 0 ? listed : [workspace]);
    applyWorkspaceDetail(detail);
    return detail.workspace;
  }

  function applyWorkspaceDetail(
    detail: WorkspaceDetail,
    preferredSessionId?: string,
    resetOutput = true
  ) {
    const previousSessionId = sessionRef.current?.sessionId;
    setWorkspaces((current) => upsertWorkspace(current, detail.workspace));
    setActiveWorkspace(detail.workspace);
    setPanes(detail.panes);
    setSurfaces(detail.surfaces);

    const nextSession = selectSession(detail, preferredSessionId);
    const sessionChanged = nextSession?.sessionId !== previousSessionId;
    sessionRef.current = nextSession;
    if (resetOutput || sessionChanged) {
      renderedLengthRef.current = 0;
    }

    if (nextSession) {
      setSession(nextSession);
      if (resetOutput || sessionChanged || pollTimerRef.current === undefined) {
        startPolling(nextSession.sessionId);
      }
    } else {
      stopPolling();
      setSession({
        sessionId: "ses_pending",
        backendKind: "conpty",
        state: "idle"
      });
    }
  }

  function setActiveWorkspace(workspace: WorkspaceSummary) {
    activeWorkspaceRef.current = workspace;
    setActiveWorkspaceState(workspace);
  }

  async function refreshTerminalOutput(sessionId: string) {
    const [nextSession, output] = await Promise.all([
      client.getSession(sessionId),
      client.readRecent(sessionId, 65536)
    ]);
    if (sessionRef.current?.sessionId !== sessionId) {
      return;
    }

    sessionRef.current = nextSession;
    setSession(nextSession);
    const nextText = output.slice(renderedLengthRef.current);
    renderedLengthRef.current = output.length;
    if (nextText.length > 0) {
      rendererRef.current?.write(encoder.encode(nextText));
    }
  }

  function startPolling(sessionId: string) {
    stopPolling();
    pollTimerRef.current = window.setInterval(() => {
      refreshTerminalOutput(sessionId).catch(() => {
        stopPolling();
        setSession((current) => ({ ...current, state: "failed" }));
      });
    }, 75);
  }

  function stopPolling() {
    if (pollTimerRef.current !== undefined) {
      window.clearInterval(pollTimerRef.current);
      pollTimerRef.current = undefined;
    }
  }

  function renderPaneNode(paneId: string): JSX.Element | null {
    const pane = paneById.get(paneId);
    if (!pane) {
      return null;
    }

    if (pane.kind === "split") {
      const children = childrenByParent.get(pane.paneId) ?? [];
      const ratio = Math.round((pane.splitRatio ?? 0.5) * 100);
      const [firstChild, ...remainingChildren] = children;
      return (
        <div
          className={`pane-split is-${pane.splitAxis ?? "vertical"}`}
          key={pane.paneId}
          style={{ "--split-ratio": `${ratio}%` } as CSSProperties}
        >
          {firstChild ? renderPaneNode(firstChild.paneId) : null}
          <input
            aria-label="Split ratio"
            className="split-ratio"
            max={90}
            min={10}
            type="range"
            value={ratio}
            onChange={(event) =>
              void resizeSplitPane(pane.paneId, Number(event.currentTarget.value) / 100)
            }
            onMouseDown={(event) => event.stopPropagation()}
          />
          {remainingChildren.map((child) => renderPaneNode(child.paneId))}
        </div>
      );
    }

    const isActive = pane.paneId === activeWorkspace?.activePaneId;
    const surface = surfaceForPane(pane);
    const paneTitle = surface?.title ?? "Terminal";
    const attention = surface?.sessionId ? attentionBySessionId.get(surface.sessionId) : undefined;
    const isBrowserSurface = surface?.surfaceType === "browser";
    return (
      <article
        className={`terminal-pane${isActive ? " is-active" : ""}`}
        aria-label={isBrowserSurface ? "Browser pane" : "Terminal pane"}
        key={pane.paneId}
        onMouseDown={() => void focusPane(pane.paneId)}
      >
        <header className="pane-titlebar">
          <div>
            <strong>{paneTitle}</strong>
            <span>{surface?.surfaceType ?? "empty"}</span>
          </div>
          <div className="pane-status">
            {attention ? (
              <button
                className="attention-pill"
                type="button"
                onClick={(event) => {
                  event.stopPropagation();
                  void clearAttention(attention.sessionId);
                }}
              >
                Attention
              </button>
            ) : null}
            {isActive && !isBrowserSurface ? <span className="state-pill">{session.state}</span> : null}
          </div>
        </header>
        {isActive && isBrowserSurface && surface ? (
          renderBrowserSurface(surface)
        ) : isActive && surface?.sessionId ? (
          <div
            className="terminal-surface"
            role="application"
            tabIndex={0}
            ref={setTerminalSurfaceElement}
          />
        ) : (
          <div className="empty-pane" />
        )}
      </article>
    );
  }

  function renderBrowserSurface(surface: SurfaceSummary): JSX.Element {
    const state = browserPaneStates[surface.surfaceId] ?? DEFAULT_BROWSER_STATE;
    const viewportSource = browserViewportSource(state.currentUrl);
    return (
      <section className="browser-surface" aria-label="Browser surface">
        <form
          className="browser-address"
          onSubmit={(event) => {
            event.preventDefault();
            void navigateBrowser(surface.surfaceId);
          }}
        >
          <input
            aria-label="Browser URL"
            value={state.url}
            onChange={(event) =>
              setBrowserPaneState(surface.surfaceId, { url: event.currentTarget.value })
            }
          />
          <button type="submit">Go</button>
          <button type="button" onClick={() => void snapshotBrowser(surface.surfaceId)}>
            Snapshot
          </button>
          <button type="button" onClick={() => void screenshotBrowser(surface.surfaceId)}>
            Screenshot
          </button>
        </form>

        <div className="browser-controls">
          <input
            aria-label="Browser selector"
            value={state.selector}
            onChange={(event) =>
              setBrowserPaneState(surface.surfaceId, { selector: event.currentTarget.value })
            }
          />
          <input
            aria-label="Browser text"
            value={state.text}
            onChange={(event) =>
              setBrowserPaneState(surface.surfaceId, { text: event.currentTarget.value })
            }
          />
          <button type="button" onClick={() => void clickBrowser(surface.surfaceId)}>
            Click
          </button>
          <button type="button" onClick={() => void typeBrowser(surface.surfaceId)}>
            Type
          </button>
          <input
            aria-label="Browser x"
            inputMode="numeric"
            value={state.x}
            onChange={(event) =>
              setBrowserPaneState(surface.surfaceId, { x: event.currentTarget.value })
            }
          />
          <input
            aria-label="Browser y"
            inputMode="numeric"
            value={state.y}
            onChange={(event) =>
              setBrowserPaneState(surface.surfaceId, { y: event.currentTarget.value })
            }
          />
          <button type="button" onClick={() => void clickBrowserPoint(surface.surfaceId)}>
            Click point
          </button>
          <input
            aria-label="Browser script"
            value={state.script}
            onChange={(event) =>
              setBrowserPaneState(surface.surfaceId, { script: event.currentTarget.value })
            }
          />
          <button type="button" onClick={() => void evaluateBrowser(surface.surfaceId)}>
            Evaluate
          </button>
        </div>

        <div className="browser-viewport" role="region" aria-label="Browser preview">
          <div className="browser-url">{state.currentUrl ?? "about:blank"}</div>
          <div className="browser-frame-shell">
            <iframe
              className="browser-frame"
              title={`Browser viewport ${surface.surfaceId}`}
              src={viewportSource}
              sandbox="allow-forms allow-modals allow-pointer-lock allow-popups allow-scripts"
            />
            <div className="browser-page" aria-live="polite">
              <strong>{surface.browserId ?? surface.surfaceId}</strong>
              <span>{state.lastAction ?? "Ready"}</span>
            </div>
          </div>
        </div>

        <div className="browser-output" aria-label="Browser output">
          {state.screenshotHandle ? (
            <p>
              <strong>Screenshot</strong>
              <span>{state.screenshotHandle}</span>
              <span>{state.screenshotBytes ?? 0} bytes</span>
            </p>
          ) : null}
          {state.evalValue ? (
            <p>
              <strong>Eval</strong>
              <span>{state.evalValue}</span>
            </p>
          ) : null}
          {state.snapshot ? (
            <pre>{state.snapshot}</pre>
          ) : null}
          {state.error ? <p className="browser-error">{state.error}</p> : null}
        </div>
      </section>
    );
  }

  function browserViewportSource(url?: string): string {
    const candidate = url?.trim() || "about:blank";
    if (candidate === "about:blank") {
      return candidate;
    }

    try {
      const parsed = new URL(candidate);
      if (["http:", "https:", "data:"].includes(parsed.protocol)) {
        return candidate;
      }
    } catch {
      return "about:blank";
    }

    return "about:blank";
  }

  function surfaceForPane(pane: PaneSummary): SurfaceSummary | undefined {
    if (!pane.mountedSurfaceId) {
      return undefined;
    }
    return surfaces.find((surface) => surface.surfaceId === pane.mountedSurfaceId);
  }

  const workspaceName = activeWorkspace?.name ?? DEFAULT_WORKSPACE_NAME;
  const workspaceRoot = activeWorkspace?.projectRoot ?? DEFAULT_PROJECT_ROOT;
  const paneById = useMemo(() => new Map(panes.map((pane) => [pane.paneId, pane])), [panes]);
  const childrenByParent = useMemo(() => {
    const children = new Map<string, PaneSummary[]>();
    for (const pane of panes) {
      if (!pane.parentPaneId) {
        continue;
      }
      const siblings = children.get(pane.parentPaneId) ?? [];
      siblings.push(pane);
      children.set(pane.parentPaneId, siblings);
    }
    return children;
  }, [panes]);
  const mountedSurfaceIds = useMemo(
    () =>
      new Set(
        panes
          .map((pane) => pane.mountedSurfaceId)
          .filter((surfaceId): surfaceId is string => Boolean(surfaceId))
      ),
    [panes]
  );
  const unmountedSurfaces = useMemo(
    () => surfaces.filter((surface) => !mountedSurfaceIds.has(surface.surfaceId)),
    [mountedSurfaceIds, surfaces]
  );
  const attentionBySessionId = useMemo(
    () => new Map(attentionSessions.map((state) => [state.sessionId, state])),
    [attentionSessions]
  );
  const attentionCountByWorkspaceId = useMemo(() => {
    const counts = new Map<string, number>();
    for (const state of attentionSessions) {
      counts.set(state.workspaceId, (counts.get(state.workspaceId) ?? 0) + 1);
    }
    return counts;
  }, [attentionSessions]);
  const activeWorkspaceAttention = useMemo(
    () =>
      attentionSessions.filter(
        (state) => !activeWorkspace || state.workspaceId === activeWorkspace.workspaceId
      ),
    [activeWorkspace, attentionSessions]
  );
  useEffect(() => {
    if (selectedSurfaceId && unmountedSurfaces.some((surface) => surface.surfaceId === selectedSurfaceId)) {
      return;
    }

    setSelectedSurfaceId(unmountedSurfaces[0]?.surfaceId ?? "");
  }, [selectedSurfaceId, unmountedSurfaces]);
  const rootPaneId = activeWorkspace?.rootPaneId ?? panes.find((pane) => !pane.parentPaneId)?.paneId;
  const canCloseActivePane = panes.some(
    (pane) => pane.paneId === activeWorkspace?.activePaneId && Boolean(pane.parentPaneId)
  );
  const activePane = panes.find((pane) => pane.paneId === activeWorkspace?.activePaneId);
  const canUnmountActiveSurface = Boolean(activePane?.mountedSurfaceId);

  return (
    <main className="app-shell">
      <aside className="sidebar" aria-label="Workspaces">
        <div className="brand">AgentMux</div>
        <button className="primary-action" type="button" onClick={createWorkspace}>
          New workspace
        </button>
        <nav className="workspace-list" aria-label="Workspace list">
          {workspaces.map((workspace, index) => (
            <button
              className={`workspace-item${
                workspace.workspaceId === activeWorkspace?.workspaceId ? " is-active" : ""
              }`}
              type="button"
              key={workspace.workspaceId}
              onClick={() => void selectWorkspace(workspace.workspaceId)}
            >
              <span>{workspace.name}</span>
              <span
                className={`badge${
                  (attentionCountByWorkspaceId.get(workspace.workspaceId) ?? 0) > 0
                    ? " is-attention"
                    : ""
                }`}
              >
                {attentionCountByWorkspaceId.get(workspace.workspaceId) ?? index + 1}
              </span>
            </button>
          ))}
        </nav>
        <section className="notification-panel" aria-label="Notifications">
          <header className="panel-header">
            <h2>Notifications</h2>
            <select
              aria-label="Notification severity"
              value={notificationSeverity}
              onChange={(event) => setNotificationSeverity(event.currentTarget.value)}
            >
              <option value="">All</option>
              <option value="info">Info</option>
              <option value="warning">Warning</option>
              <option value="error">Error</option>
            </select>
          </header>
          <div className="attention-list">
            {activeWorkspaceAttention.map((state) => (
              <div className="attention-row" key={state.sessionId}>
                <div>
                  <strong>{shortId(state.sessionId)}</strong>
                  <span>{state.reason ?? state.state}</span>
                </div>
                <button type="button" onClick={() => void clearAttention(state.sessionId)}>
                  Clear
                </button>
              </div>
            ))}
          </div>
          <div className="notification-list">
            {notifications.map((notification) => (
              <article
                className={`notification-row is-${notification.severity}`}
                key={notification.notificationId}
              >
                <div>
                  <strong>{notification.title}</strong>
                  <span>{formatNotificationTime(notification.createdAt)}</span>
                </div>
                <p>{notification.message}</p>
                <button
                  type="button"
                  onClick={() => void dismissNotification(notification.notificationId)}
                >
                  Dismiss
                </button>
              </article>
            ))}
          </div>
        </section>
      </aside>

      <section className="workspace">
        <header className="workspace-header">
          <div>
            <h1>{workspaceName}</h1>
            <p>{workspaceRoot}</p>
          </div>
          <div className="toolbar" aria-label="Workspace actions">
            <button type="button" onClick={openNativeShell}>
              Native shell
            </button>
            <button type="button" onClick={renameActiveWorkspace} disabled={!activeWorkspace}>
              Rename
            </button>
            <select
              aria-label="Workspace close policy"
              value={workspaceClosePolicy}
              onChange={(event) => setWorkspaceClosePolicy(event.target.value)}
            >
              <option value="fail_if_running">Fail if running</option>
              <option value="detach_sessions">Detach sessions</option>
              <option value="terminate_sessions">Terminate sessions</option>
            </select>
            <button type="button" onClick={closeActiveWorkspace} disabled={!activeWorkspace}>
              Close workspace
            </button>
            <select
              aria-label="WSL distribution"
              value={selectedWslDistribution}
              disabled={wslDistributions.length === 0}
              onChange={(event) => setSelectedWslDistribution(event.target.value)}
            >
              {wslDistributions.length === 0 ? (
                <option value="">WSL</option>
              ) : (
                wslDistributions.map((distribution) => (
                  <option key={distribution.name} value={distribution.name}>
                    {distribution.name}
                  </option>
                ))
              )}
            </select>
            <button type="button" onClick={openWslShell} disabled={wslDistributions.length === 0}>
              WSL shell
            </button>
            <button type="button" onClick={createBrowserSurface} disabled={!activeWorkspace}>
              Browser
            </button>
            <button
              type="button"
              onClick={() => void splitActivePane("vertical")}
              disabled={!activeWorkspace}
            >
              Split vertical
            </button>
            <button
              type="button"
              onClick={() => void splitActivePane("horizontal")}
              disabled={!activeWorkspace}
            >
              Split horizontal
            </button>
            <select
              aria-label="Pane close policy"
              value={paneSurfacePolicy}
              onChange={(event) => setPaneSurfacePolicy(event.target.value)}
            >
              <option value="fail_if_session_running">Fail if running</option>
              <option value="detach_surface">Detach surface</option>
              <option value="close_surface">Close surface</option>
            </select>
            <button type="button" onClick={closeActivePane} disabled={!canCloseActivePane}>
              Close pane
            </button>
            <select
              aria-label="Detached surface"
              value={selectedSurfaceId}
              disabled={unmountedSurfaces.length === 0}
              onChange={(event) => setSelectedSurfaceId(event.target.value)}
            >
              {unmountedSurfaces.length === 0 ? (
                <option value="">No detached surfaces</option>
              ) : (
                unmountedSurfaces.map((surface) => (
                  <option key={surface.surfaceId} value={surface.surfaceId}>
                    {surface.title}
                  </option>
                ))
              )}
            </select>
            <button
              type="button"
              onClick={mountSelectedSurface}
              disabled={!activeWorkspace || unmountedSurfaces.length === 0}
            >
              Mount
            </button>
            <button
              type="button"
              onClick={unmountActiveSurface}
              disabled={!canUnmountActiveSurface}
            >
              Unmount
            </button>
          </div>
        </header>

        <section className="pane-canvas" aria-label="Pane canvas">
          <div className="pane-tree">
            {rootPaneId ? renderPaneNode(rootPaneId) : <div className="empty-pane" />}
          </div>
        </section>
      </section>
    </main>
  );
}

function upsertWorkspace(
  current: WorkspaceSummary[],
  workspace: WorkspaceSummary
): WorkspaceSummary[] {
  const index = current.findIndex((candidate) => candidate.workspaceId === workspace.workspaceId);
  if (index < 0) {
    return [workspace, ...current];
  }

  const next = [...current];
  next[index] = workspace;
  return next;
}

function selectSession(detail: WorkspaceDetail, preferredSessionId?: string): TerminalSession | null {
  if (preferredSessionId) {
    const preferred = detail.sessions.find((session) => session.sessionId === preferredSessionId);
    if (preferred) {
      return preferred;
    }
  }

  const activePane = detail.panes.find(
    (pane) => pane.paneId === detail.workspace.activePaneId && pane.mountedSurfaceId
  );
  const surface = detail.surfaces.find(
    (candidate) => candidate.surfaceId === activePane?.mountedSurfaceId
  );
  if (!surface?.sessionId) {
    return null;
  }

  return detail.sessions.find((session) => session.sessionId === surface.sessionId) ?? null;
}

function shortId(value: string): string {
  return value.length <= 12 ? value : value.slice(0, 12);
}

function formatNotificationTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return date.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit"
  });
}
