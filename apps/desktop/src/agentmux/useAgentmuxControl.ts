import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  type AgentState,
  type AgentTelemetry,
  type BrowserClickTarget,
  type ControlClient,
  createControlClient,
  type DockControl,
  type NotificationSummary,
  type SidebarState,
  type SshProfile,
  type SshProfileInput,
  type SurfaceSummary,
  type TerminalSession,
  type WorkspaceDetail,
  type WorkspaceGroup,
  type WorkspaceGroupCreateInput,
  type WorkspaceGroupUpdateInput,
  type WorkspaceUpdateInput,
  type WorkspaceSummary,
  type WslDistribution
} from "../control/ControlClient";

const DEFAULT_PROJECT_ROOT = "D:\\Workspace\\irae\\agentmux";
const DEFAULT_WORKSPACE_NAME = "Workspace 1";
const SIGNAL_POLL_INTERVAL_MS = 1500;
const DETAIL_POLL_INTERVAL_MS = 5000;
const SIDEBAR_POLL_INTERVAL_MS = 5000;
const LAST_ACTIVE_WORKSPACE_STORAGE_KEY = "agentmux.ui.lastActiveWorkspaceId.v1";
const LOCAL_NOTIFICATION_PREFIX = "local_";
const WSL_REQUIRED_NOTIFICATION_ID = `${LOCAL_NOTIFICATION_PREFIX}wsl_required`;
const WSL_REQUIRED_MESSAGE =
  "AgentMux는 Windows에서 WSL을 기본 터미널 실행 환경으로 사용합니다. PowerShell에서 `wsl --install`로 WSL 배포판을 설치한 뒤 다시 시도하세요.";
const TMUX_REQUIRED_NOTIFICATION_ID = `${LOCAL_NOTIFICATION_PREFIX}tmux_required`;
const TMUX_REQUIRED_MESSAGE =
  "선택된 WSL 배포판에 tmux가 필요합니다. WSL에서 `sudo apt update && sudo apt install -y tmux`를 실행한 뒤 다시 시도하세요.";

// --- cheap structural-equality gates (PR-1) ---------------------------------
// The 1.2s poll re-fetches the same data on most ticks. Calling a state setter
// with a fresh-but-equal value still triggers a full re-render of the root
// component, so each setter is gated behind one of these cheap checks. They are
// intentionally shallow: they compare the fields the UI actually renders, so a
// no-op tick produces zero re-renders while real changes still propagate.

function setIfChanged<T>(
  setter: (updater: (previous: T) => T) => void,
  next: T,
  equal: (previous: T, next: T) => boolean
): void {
  setter((previous) => (equal(previous, next) ? previous : next));
}

function telemetryEqual(
  a: AgentTelemetry | null | undefined,
  b: AgentTelemetry | null | undefined
): boolean {
  if (a === b) return true;
  if (!a || !b) return !a && !b;
  return (
    a.activity === b.activity &&
    a.session === b.session &&
    a.cost === b.cost &&
    a.tokens === b.tokens &&
    a.cache === b.cache &&
    a.rate === b.rate &&
    a.ctx === b.ctx
  );
}

function agentStateEqual(a: AgentState, b: AgentState): boolean {
  return (
    a.sessionId === b.sessionId &&
    a.workspaceId === b.workspaceId &&
    a.state === b.state &&
    a.attention === b.attention &&
    a.reason === b.reason &&
    a.updatedAt === b.updatedAt &&
    telemetryEqual(a.telemetry, b.telemetry)
  );
}

function agentStatesEqual(a: AgentState[], b: AgentState[]): boolean {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  for (let index = 0; index < a.length; index += 1) {
    if (!agentStateEqual(a[index], b[index])) return false;
  }
  return true;
}

function notificationsEqual(
  a: NotificationSummary[],
  b: NotificationSummary[]
): boolean {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  for (let index = 0; index < a.length; index += 1) {
    const left = a[index];
    const right = b[index];
    if (
      left.notificationId !== right.notificationId ||
      left.dismissed !== right.dismissed ||
      left.severity !== right.severity ||
      left.title !== right.title ||
      left.message !== right.message
    ) {
      return false;
    }
  }
  return true;
}

function profilesEqual(a: SshProfile[], b: SshProfile[]): boolean {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  for (let index = 0; index < a.length; index += 1) {
    const left = a[index];
    const right = b[index];
    if (
      left.profileId !== right.profileId ||
      left.name !== right.name ||
      left.host !== right.host ||
      left.user !== right.user ||
      left.port !== right.port
    ) {
      return false;
    }
  }
  return true;
}

function sameIdList(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  for (let index = 0; index < a.length; index += 1) {
    if (a[index] !== b[index]) return false;
  }
  return true;
}

function detailEqual(
  a: WorkspaceDetail | null,
  b: WorkspaceDetail | null
): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  if (a.workspace.workspaceId !== b.workspace.workspaceId) return false;
  if (a.workspace.activePaneId !== b.workspace.activePaneId) return false;
  if (a.workspace.rootPaneId !== b.workspace.rootPaneId) return false;
  if (a.panes.length !== b.panes.length) return false;
  if (a.surfaces.length !== b.surfaces.length) return false;
  if (a.sessions.length !== b.sessions.length) return false;
  for (let index = 0; index < a.panes.length; index += 1) {
    const left = a.panes[index];
    const right = b.panes[index];
    if (
      left.paneId !== right.paneId ||
      left.mountedSurfaceId !== right.mountedSurfaceId ||
      left.splitRatio !== right.splitRatio ||
      left.splitAxis !== right.splitAxis ||
      left.parentPaneId !== right.parentPaneId ||
      left.kind !== right.kind
    ) {
      return false;
    }
  }
  for (let index = 0; index < a.surfaces.length; index += 1) {
    const left = a.surfaces[index];
    const right = b.surfaces[index];
    if (
      left.surfaceId !== right.surfaceId ||
      left.sessionId !== right.sessionId ||
      left.title !== right.title ||
      left.surfaceType !== right.surfaceType
    ) {
      return false;
    }
  }
  for (let index = 0; index < a.sessions.length; index += 1) {
    const left = a.sessions[index];
    const right = b.sessions[index];
    if (
      left.sessionId !== right.sessionId ||
      left.state !== right.state ||
      left.backendKind !== right.backendKind
    ) {
      return false;
    }
  }
  return true;
}

function sidebarStateEqual(
  a: SidebarState | null,
  b: SidebarState | null
): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  if (
    a.workspaceId !== b.workspaceId ||
    a.cwd !== b.cwd ||
    a.gitBranch !== b.gitBranch ||
    a.gitHash !== b.gitHash
  ) {
    return false;
  }
  if (!sameIdList(a.ports, b.ports)) return false;
  if (a.statuses.length !== b.statuses.length) return false;
  for (let index = 0; index < a.statuses.length; index += 1) {
    const left = a.statuses[index];
    const right = b.statuses[index];
    if (
      left.key !== right.key ||
      left.label !== right.label ||
      left.priority !== right.priority ||
      left.updatedAt !== right.updatedAt
    ) {
      return false;
    }
  }
  if ((a.progress?.value ?? null) !== (b.progress?.value ?? null)) return false;
  if ((a.progress?.label ?? null) !== (b.progress?.label ?? null)) return false;
  if (a.logs.length !== b.logs.length) return false;
  for (let index = 0; index < a.logs.length; index += 1) {
    if (a.logs[index].logId !== b.logs[index].logId) return false;
  }
  return true;
}

function nextWorkspaceName(workspaces: WorkspaceSummary[]): string {
  const usedNames = new Set(workspaces.map((workspace) => workspace.name));
  for (let index = 1; ; index += 1) {
    const candidate = `Workspace ${index}`;
    if (!usedNames.has(candidate)) {
      return candidate;
    }
  }
}

function resolveDockCwd(cwd: string | null | undefined, workspaceRoot: string): string {
  const value = cwd?.trim();
  if (!value || value === ".") {
    return workspaceRoot;
  }
  if (
    value.startsWith("/") ||
    value.startsWith("~") ||
    /^[A-Za-z]:[\\/]/.test(value) ||
    value.startsWith("\\\\")
  ) {
    return value;
  }
  if (workspaceRoot.startsWith("/") || workspaceRoot.startsWith("~")) {
    return `${workspaceRoot.replace(/\/+$/, "")}/${value.replace(/^[\\/]+/, "")}`;
  }
  return `${workspaceRoot.replace(/[\\/]+$/, "")}\\${value.replace(/^[\\/]+/, "").replace(/\//g, "\\")}`;
}

function readLastActiveWorkspaceId(): string | null {
  try {
    return window.localStorage.getItem(LAST_ACTIVE_WORKSPACE_STORAGE_KEY);
  } catch {
    return null;
  }
}

function persistLastActiveWorkspaceId(workspaceId: string | null): void {
  try {
    if (workspaceId) {
      window.localStorage.setItem(LAST_ACTIVE_WORKSPACE_STORAGE_KEY, workspaceId);
    } else {
      window.localStorage.removeItem(LAST_ACTIVE_WORKSPACE_STORAGE_KEY);
    }
  } catch {
    // Workspace restore should never block the control surface.
  }
}

function shouldAutoLaunchStartupTerminal(): boolean {
  return Boolean(window.__TAURI__?.core?.invoke || window.__AGENTMUX_SERVER__);
}

export interface AgentmuxControl {
  client: ControlClient;
  ready: boolean;
  error: string | null;
  workspaces: WorkspaceSummary[];
  workspaceGroups: WorkspaceGroup[];
  activeWorkspaceId: string | null;
  detail: WorkspaceDetail | null;
  attention: AgentState[];
  agentStates: AgentState[];
  notifications: NotificationSummary[];
  sidebarState: SidebarState | null;
  wslDistributions: WslDistribution[];
  profiles: SshProfile[];
  attentionByWorkspace: Map<string, number>;
  attentionBySession: Map<string, AgentState>;
  agentBySession: Map<string, AgentState>;
  selectWorkspace: (workspaceId: string) => Promise<void>;
  createWorkspace: (name?: string) => Promise<WorkspaceSummary>;
  renameWorkspace: (workspaceId: string, name: string) => Promise<void>;
  updateWorkspace: (workspaceId: string, input: WorkspaceUpdateInput) => Promise<void>;
  closeWorkspace: (workspaceId: string, policy: string) => Promise<void>;
  createWorkspaceGroup: (input: WorkspaceGroupCreateInput) => Promise<WorkspaceGroup | null>;
  updateWorkspaceGroup: (
    groupId: string,
    input: WorkspaceGroupUpdateInput
  ) => Promise<WorkspaceGroup | null>;
  deleteWorkspaceGroup: (groupId: string) => Promise<void>;
  addWorkspaceToGroup: (
    groupId: string,
    workspaceId: string,
    position?: number | null
  ) => Promise<WorkspaceGroup | null>;
  removeWorkspaceFromGroup: (
    groupId: string,
    workspaceId: string
  ) => Promise<WorkspaceGroup | null>;
  createWorkspaceInGroup: (groupId: string, name?: string) => Promise<WorkspaceSummary | null>;
  spawnDefaultTerminal: () => Promise<void>;
  spawnDefaultTerminalInPane: (paneId: string) => Promise<void>;
  spawnDurableTerminalInPane: (paneId: string) => Promise<void>;
  spawnWslTerminal: (distribution: string) => Promise<void>;
  spawnAgent: (command: string[]) => Promise<void>;
  spawnDockControl: (control: DockControl) => Promise<TerminalSession | null>;
  splitActivePane: (axis: "horizontal" | "vertical") => Promise<void>;
  resizePane: (paneId: string, ratio: number) => void;
  focusPane: (paneId: string) => Promise<void>;
  closePane: (paneId: string) => Promise<void>;
  closeSurface: (surfaceId: string) => Promise<void>;
  mountSurface: (surfaceId: string, paneId?: string) => Promise<void>;
  createBrowserSurface: (placement?: "new_tab" | "active_pane") => Promise<SurfaceSummary | null>;
  browserNavigate: (surfaceId: string, url: string) => Promise<void>;
  browserReload: (surfaceId: string) => Promise<void>;
  browserBack: (surfaceId: string) => Promise<void>;
  browserForward: (surfaceId: string) => Promise<void>;
  browserCurrentUrl: (surfaceId: string) => Promise<void>;
  browserScreenshot: (surfaceId: string, format?: string | null) => Promise<void>;
  browserDomSnapshot: (surfaceId: string, frameId?: string | null) => Promise<void>;
  browserClick: (surfaceId: string, target: BrowserClickTarget) => Promise<void>;
  browserType: (surfaceId: string, selector: string, text: string, frameId?: string | null) => Promise<void>;
  browserFill: (surfaceId: string, selector: string, text: string, frameId?: string | null) => Promise<void>;
  browserPress: (surfaceId: string, selector: string, key: string, frameId?: string | null) => Promise<void>;
  browserSelect: (surfaceId: string, selector: string, values: string[], frameId?: string | null) => Promise<void>;
  browserScroll: (
    surfaceId: string,
    options: { selector?: string | null; x?: number | null; y?: number | null; frameId?: string | null }
  ) => Promise<void>;
  browserHover: (surfaceId: string, selector: string, frameId?: string | null) => Promise<void>;
  browserCheck: (surfaceId: string, selector: string, checked?: boolean | null, frameId?: string | null) => Promise<void>;
  browserGet: (
    surfaceId: string,
    selector: string,
    options?: { kind?: string | null; attribute?: string | null; frameId?: string | null }
  ) => Promise<void>;
  browserFind: (
    surfaceId: string,
    query: string,
    options?: { selector?: string | null; limit?: number | null; frameId?: string | null }
  ) => Promise<void>;
  browserHighlight: (
    surfaceId: string,
    selector: string,
    durationMs?: number | null,
    frameId?: string | null
  ) => Promise<void>;
  browserFocus: (surfaceId: string, selector: string, frameId?: string | null) => Promise<void>;
  browserZoom: (surfaceId: string, percent: number) => Promise<void>;
  browserWaitForSelector: (
    surfaceId: string,
    selector: string,
    timeoutMs?: number | null,
    frameId?: string | null
  ) => Promise<void>;
  browserEvaluate: (surfaceId: string, script: string, frameId?: string | null) => Promise<void>;
  clearAttention: (sessionId: string) => Promise<void>;
  dismissNotification: (notificationId: string) => Promise<void>;
  createProfile: (input: SshProfileInput) => Promise<void>;
  updateProfile: (profileId: string, input: SshProfileInput) => Promise<void>;
  deleteProfile: (profileId: string) => Promise<void>;
  connectProfile: (profile: SshProfile) => Promise<void>;
  refresh: () => Promise<void>;
  refreshSidebar: () => Promise<void>;
}

// Bridges the design UI to the real control plane. Under Tauri this drives the
// Rust backend (real terminals); in a plain browser it drives the in-memory
// preview client. Either way the UI consumes the same shape.
export function useAgentmuxControl(): AgentmuxControl {
  const client = useMemo(() => createControlClient(), []);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceSummary[]>([]);
  const [workspaceGroups, setWorkspaceGroups] = useState<WorkspaceGroup[]>([]);
  const [activeWorkspaceId, setActiveWorkspaceId] = useState<string | null>(null);
  const [detail, setDetail] = useState<WorkspaceDetail | null>(null);
  const [attention, setAttention] = useState<AgentState[]>([]);
  const [agentStates, setAgentStates] = useState<AgentState[]>([]);
  const [notifications, setNotifications] = useState<NotificationSummary[]>([]);
  const [sidebarState, setSidebarState] = useState<SidebarState | null>(null);
  const [wslDistributions, setWslDistributions] = useState<WslDistribution[]>([]);
  const [wslChecked, setWslChecked] = useState(false);
  const [startupLaunchWorkspaceId, setStartupLaunchWorkspaceId] = useState<string | null>(null);
  const [profiles, setProfiles] = useState<SshProfile[]>([]);

  const activeRef = useRef<string | null>(null);
  const detailRef = useRef<WorkspaceDetail | null>(null);
  const lastDetailRefreshAtRef = useRef(0);
  const lastSidebarRefreshAtRef = useRef(0);
  const signalRefreshInFlightRef = useRef(false);
  activeRef.current = activeWorkspaceId;

  const pushLocalNotification = useCallback(
    (notification: Omit<NotificationSummary, "createdAt" | "dismissed">) => {
      setNotifications((current) => {
        if (current.some((candidate) => candidate.notificationId === notification.notificationId)) {
          return current;
        }
        return [
          {
            ...notification,
            createdAt: new Date().toISOString(),
            dismissed: false
          },
          ...current
        ];
      });
    },
    []
  );

  const mergeNotifications = useCallback((remoteNotifications: NotificationSummary[]) => {
    setNotifications((current) => {
      const locals = current.filter(
        (notification) =>
          notification.notificationId.startsWith(LOCAL_NOTIFICATION_PREFIX) &&
          !notification.dismissed
      );
      const remoteIds = new Set(remoteNotifications.map((notification) => notification.notificationId));
      const merged = [
        ...locals.filter((notification) => !remoteIds.has(notification.notificationId)),
        ...remoteNotifications
      ];
      // Keep the same reference when nothing changed so a no-op poll tick does
      // not trigger a re-render through the notifications dependency.
      return notificationsEqual(current, merged) ? current : merged;
    });
  }, []);

  const notifyWslRequired = useCallback(() => {
    pushLocalNotification({
      notificationId: WSL_REQUIRED_NOTIFICATION_ID,
      notificationType: "diagnostics.wsl_required",
      severity: "warning",
      workspaceId: activeRef.current,
      sessionId: null,
      title: "WSL 설치 필요",
      message: WSL_REQUIRED_MESSAGE
    });
    setError(WSL_REQUIRED_MESSAGE);
  }, [pushLocalNotification]);

  const notifyTmuxRequired = useCallback(
    (message = TMUX_REQUIRED_MESSAGE) => {
      pushLocalNotification({
        notificationId: TMUX_REQUIRED_NOTIFICATION_ID,
        notificationType: "diagnostics.tmux_required",
        severity: "warning",
        workspaceId: activeRef.current,
        sessionId: null,
        title: "tmux 설치 필요",
        message
      });
      setError(message);
    },
    [pushLocalNotification]
  );

  const loadDetail = useCallback(
    async (workspaceId: string) => {
      const next = await client.getWorkspace(workspaceId);
      if (activeRef.current === workspaceId) {
        lastDetailRefreshAtRef.current = Date.now();
        setDetail((previous) => {
          const resolved = detailEqual(previous, next) ? previous : next;
          detailRef.current = resolved;
          return resolved;
        });
      }
      return next;
    },
    [client]
  );

  const refresh = useCallback(async () => {
    const workspaceId = activeRef.current;
    if (!workspaceId) {
      return;
    }
    try {
      // PR-5: profiles and workspace groups change only on explicit user
      // mutation; they are hydrated once and reloaded by their mutators, so the
      // periodic poll no longer fetches them (per-tick IPC drops 7 -> 4).
      const [, nextAttention, nextStates, nextNotifications, nextSidebarState] =
        await Promise.all([
          loadDetail(workspaceId),
          client.listAgentAttention(null),
          client.listAgentStates(null),
          client.listNotifications({ workspaceId: null, severity: null, includeDismissed: false }),
          client.getSidebarState(workspaceId)
        ]);
      // PR-1: gate each setter behind a cheap equality check so a tick that
      // returns identical data produces zero re-renders. loadDetail and
      // mergeNotifications already self-gate above.
      setIfChanged(setAttention, nextAttention, agentStatesEqual);
      setIfChanged(setAgentStates, nextStates, agentStatesEqual);
      mergeNotifications(nextNotifications);
      lastSidebarRefreshAtRef.current = Date.now();
      setIfChanged(setSidebarState, nextSidebarState, sidebarStateEqual);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Control plane request failed.");
    }
  }, [client, loadDetail, mergeNotifications]);

  const refreshSignals = useCallback(async () => {
    const workspaceId = activeRef.current;
    if (!workspaceId || signalRefreshInFlightRef.current) {
      return;
    }
    signalRefreshInFlightRef.current = true;
    try {
      const now = Date.now();
      const shouldLoadDetail =
        now - lastDetailRefreshAtRef.current >= DETAIL_POLL_INTERVAL_MS;
      const shouldLoadSidebar =
        now - lastSidebarRefreshAtRef.current >= SIDEBAR_POLL_INTERVAL_MS;
      const [nextDetail, nextAttention, nextStates, nextNotifications, nextSidebarState] =
        await Promise.all([
          shouldLoadDetail ? loadDetail(workspaceId) : Promise.resolve(null),
          client.listAgentAttention(null),
          client.listAgentStates(null),
          client.listNotifications({ workspaceId: null, severity: null, includeDismissed: false }),
          shouldLoadSidebar ? client.getSidebarState(workspaceId) : Promise.resolve(null)
        ]);
      if (nextDetail) {
        lastDetailRefreshAtRef.current = now;
      }
      setIfChanged(setAttention, nextAttention, agentStatesEqual);
      setIfChanged(setAgentStates, nextStates, agentStatesEqual);
      mergeNotifications(nextNotifications);
      if (nextSidebarState) {
        lastSidebarRefreshAtRef.current = now;
        setIfChanged(setSidebarState, nextSidebarState, sidebarStateEqual);
      }
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Control plane request failed.");
    } finally {
      signalRefreshInFlightRef.current = false;
    }
  }, [client, loadDetail, mergeNotifications]);

  // Lightweight refetch of just the sidebar/footer state (git branch+hash for
  // the active pane's session cwd). Cheap enough to call whenever focus moves
  // between panes, so the footer git reflects the selected pane.
  const refreshSidebar = useCallback(async () => {
    const workspaceId = activeRef.current;
    if (!workspaceId) {
      return;
    }
    try {
      const next = await client.getSidebarState(workspaceId);
      lastSidebarRefreshAtRef.current = Date.now();
      setIfChanged(setSidebarState, next, sidebarStateEqual);
    } catch {
      /* leave the previous sidebar state in place on transient failures */
    }
  }, [client]);

  // Hydrate workspace list (create a default workspace when none exist).
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        let list = await client.listWorkspaces();
        if (cancelled) {
          return;
        }
        const shouldCreateStartupTerminal = list.length === 0 && shouldAutoLaunchStartupTerminal();
        const lastActiveWorkspaceId = readLastActiveWorkspaceId();
        let initial =
          (lastActiveWorkspaceId
            ? list.find((workspace) => workspace.workspaceId === lastActiveWorkspaceId)
            : undefined) ?? list[0];
        if (!initial) {
          initial = await client.createWorkspace(DEFAULT_WORKSPACE_NAME, DEFAULT_PROJECT_ROOT);
          if (cancelled) {
            return;
          }
          list = await client.listWorkspaces();
          if (cancelled) {
            return;
          }
          if (!list.some((workspace) => workspace.workspaceId === initial.workspaceId)) {
            list = [initial, ...list];
          }
        }
        setWorkspaces(list);
        setWorkspaceGroups(await client.listWorkspaceGroups());
        setActiveWorkspaceId(initial.workspaceId);
        activeRef.current = initial.workspaceId;
        persistLastActiveWorkspaceId(initial.workspaceId);
        await loadDetail(initial.workspaceId);
        if (shouldCreateStartupTerminal) {
          setStartupLaunchWorkspaceId(initial.workspaceId);
        }
        setReady(true);
      } catch (cause) {
        if (!cancelled) {
          setError(cause instanceof Error ? cause.message : "Failed to load workspaces.");
          setReady(true);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client, loadDetail]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const distributions = await client.listWslDistributions();
        if (!cancelled) {
          setWslDistributions(distributions);
          setWslChecked(true);
          if (distributions.length === 0) {
            notifyWslRequired();
          }
        }
      } catch (cause) {
        if (!cancelled) {
          setWslDistributions([]);
          setWslChecked(true);
          pushLocalNotification({
            notificationId: WSL_REQUIRED_NOTIFICATION_ID,
            notificationType: "diagnostics.wsl_required",
            severity: "warning",
            workspaceId: activeRef.current,
            sessionId: null,
            title: "WSL 확인 필요",
            message:
              cause instanceof Error
                ? `${WSL_REQUIRED_MESSAGE} (${cause.message})`
                : WSL_REQUIRED_MESSAGE
          });
          setError(WSL_REQUIRED_MESSAGE);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client, notifyWslRequired, pushLocalNotification]);

  // Poll active workspace + agent signals.
  useEffect(() => {
    if (!activeWorkspaceId) {
      return;
    }
    void refresh();
    const timer = window.setInterval(() => void refreshSignals(), SIGNAL_POLL_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [activeWorkspaceId, refresh, refreshSignals]);

  // PR-5: hydrate SSH profiles once on mount. They are no longer fetched by the
  // periodic poll; profile mutations (create/update/delete) keep them current.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const listed = await client.listProfiles();
        if (!cancelled) {
          setIfChanged(setProfiles, listed, profilesEqual);
        }
      } catch {
        /* profiles are non-critical; the next mutation will repopulate them */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client]);

  const reloadWorkspaces = useCallback(async () => {
    const listed = await client.listWorkspaces();
    setWorkspaces(listed);
    setWorkspaceGroups(await client.listWorkspaceGroups());
    return listed;
  }, [client]);

  const reloadWorkspaceGroups = useCallback(async () => {
    const groups = await client.listWorkspaceGroups();
    setWorkspaceGroups(groups);
    return groups;
  }, [client]);

  const selectWorkspace = useCallback(
    async (workspaceId: string) => {
      setActiveWorkspaceId(workspaceId);
      activeRef.current = workspaceId;
      lastDetailRefreshAtRef.current = 0;
      lastSidebarRefreshAtRef.current = 0;
      persistLastActiveWorkspaceId(workspaceId);
      await loadDetail(workspaceId);
    },
    [loadDetail]
  );

  const createWorkspace = useCallback(
    async (name?: string) => {
      const requestedName = name?.trim() || nextWorkspaceName(workspaces);
      const created = await client.createWorkspace(requestedName, DEFAULT_PROJECT_ROOT);
      await reloadWorkspaces();
      await selectWorkspace(created.workspaceId);
      return created;
    },
    [client, reloadWorkspaces, selectWorkspace, workspaces]
  );

  const renameWorkspace = useCallback(
    async (workspaceId: string, name: string) => {
      await client.renameWorkspace(workspaceId, name);
      await reloadWorkspaces();
      if (activeRef.current === workspaceId) {
        await loadDetail(workspaceId);
      }
    },
    [client, loadDetail, reloadWorkspaces]
  );

  const updateWorkspace = useCallback(
    async (workspaceId: string, input: WorkspaceUpdateInput) => {
      await client.updateWorkspace(workspaceId, input);
      await reloadWorkspaces();
      if (activeRef.current === workspaceId) {
        await loadDetail(workspaceId);
        const nextSidebarState = await client.getSidebarState(workspaceId);
        lastSidebarRefreshAtRef.current = Date.now();
        setIfChanged(setSidebarState, nextSidebarState, sidebarStateEqual);
      }
    },
    [client, loadDetail, reloadWorkspaces]
  );

  const closeWorkspace = useCallback(
    async (workspaceId: string, policy: string) => {
      await client.closeWorkspace(workspaceId, policy);
      const listed = await reloadWorkspaces();
      const next = listed[0]?.workspaceId ?? null;
      setActiveWorkspaceId(next);
      activeRef.current = next;
      lastDetailRefreshAtRef.current = 0;
      lastSidebarRefreshAtRef.current = 0;
      persistLastActiveWorkspaceId(next);
      if (next) {
        await loadDetail(next);
      } else {
        detailRef.current = null;
        setDetail(null);
        setSidebarState(null);
      }
    },
    [client, loadDetail, reloadWorkspaces]
  );

  const createWorkspaceGroup = useCallback(
    async (input: WorkspaceGroupCreateInput) => {
      try {
        const group = await client.createWorkspaceGroup(input);
        await reloadWorkspaceGroups();
        setError(null);
        return group;
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Workspace group create failed.");
        return null;
      }
    },
    [client, reloadWorkspaceGroups]
  );

  const updateWorkspaceGroup = useCallback(
    async (groupId: string, input: WorkspaceGroupUpdateInput) => {
      try {
        const group = await client.updateWorkspaceGroup(groupId, input);
        await reloadWorkspaceGroups();
        setError(null);
        return group;
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Workspace group update failed.");
        return null;
      }
    },
    [client, reloadWorkspaceGroups]
  );

  const deleteWorkspaceGroup = useCallback(
    async (groupId: string) => {
      try {
        await client.deleteWorkspaceGroup(groupId);
        await reloadWorkspaceGroups();
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Workspace group delete failed.");
      }
    },
    [client, reloadWorkspaceGroups]
  );

  const addWorkspaceToGroup = useCallback(
    async (groupId: string, workspaceId: string, position?: number | null) => {
      try {
        const group = await client.addWorkspaceToGroup(groupId, workspaceId, position ?? null);
        await reloadWorkspaceGroups();
        setError(null);
        return group;
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Workspace group membership failed.");
        return null;
      }
    },
    [client, reloadWorkspaceGroups]
  );

  const removeWorkspaceFromGroup = useCallback(
    async (groupId: string, workspaceId: string) => {
      try {
        const group = await client.removeWorkspaceFromGroup(groupId, workspaceId);
        await reloadWorkspaceGroups();
        setError(null);
        return group;
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Workspace group membership failed.");
        return null;
      }
    },
    [client, reloadWorkspaceGroups]
  );

  const createWorkspaceInGroup = useCallback(
    async (groupId: string, name?: string) => {
      try {
        const requestedName = name?.trim() || nextWorkspaceName(workspaces);
        const created = await client.createWorkspace(requestedName, DEFAULT_PROJECT_ROOT);
        await client.addWorkspaceToGroup(groupId, created.workspaceId, null);
        await reloadWorkspaces();
        await selectWorkspace(created.workspaceId);
        setError(null);
        return created;
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Workspace group create failed.");
        return null;
      }
    },
    [client, reloadWorkspaces, selectWorkspace, workspaces]
  );

  const withActive = useCallback(
    async (action: (workspaceId: string) => Promise<unknown>): Promise<void> => {
      const workspaceId = activeRef.current;
      if (!workspaceId) {
        return;
      }
      try {
        await action(workspaceId);
        await loadDetail(workspaceId);
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Control plane request failed.");
      }
    },
    [loadDetail]
  );

  const withActiveResult = useCallback(
    async <T,>(action: (workspaceId: string) => Promise<T>): Promise<T | null> => {
      const workspaceId = activeRef.current;
      if (!workspaceId) {
        return null;
      }
      try {
        const result = await action(workspaceId);
        await loadDetail(workspaceId);
        setError(null);
        return result;
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Control plane request failed.");
        return null;
      }
    },
    [loadDetail]
  );

  const getCurrentDetail = useCallback(
    async (workspaceId: string) => {
      const current = detailRef.current;
      if (current?.workspace.workspaceId === workspaceId) {
        return current;
      }
      return client.getWorkspace(workspaceId);
    },
    [client]
  );

  const workspaceProjectRoot = useCallback(
    (workspaceId: string) =>
      workspaces.find((workspace) => workspace.workspaceId === workspaceId)?.projectRoot ??
      DEFAULT_PROJECT_ROOT,
    [workspaces]
  );

  const defaultWslDistribution = useCallback(
    (workspaceId: string) => {
      const workspace = workspaces.find((candidate) => candidate.workspaceId === workspaceId);
      return (
        workspace?.defaultWslDistribution ??
        wslDistributions.find((distribution) => distribution.isDefault)?.name ??
        wslDistributions[0]?.name ??
        null
      );
    },
    [workspaces, wslDistributions]
  );

  const spawnDefaultTerminal = useCallback(
    () =>
      withActive(async (workspaceId) => {
        const distribution = defaultWslDistribution(workspaceId);
        if (!distribution) {
          notifyWslRequired();
          throw new Error(WSL_REQUIRED_MESSAGE);
        }
        return client.spawnWslTerminal(
          workspaceId,
          distribution,
          workspaceProjectRoot(workspaceId),
          "new_tab"
        );
      }),
    [client, defaultWslDistribution, notifyWslRequired, withActive, workspaceProjectRoot]
  );

  const spawnDefaultTerminalInPane = useCallback(
    (paneId: string) =>
      withActive(async (workspaceId) => {
        const distribution = defaultWslDistribution(workspaceId);
        if (!distribution) {
          notifyWslRequired();
          throw new Error(WSL_REQUIRED_MESSAGE);
        }
        return client.spawnWslTerminal(
          workspaceId,
          distribution,
          workspaceProjectRoot(workspaceId),
          "active_pane",
          paneId
        );
      }),
    [client, defaultWslDistribution, notifyWslRequired, withActive, workspaceProjectRoot]
  );

  const spawnDurableTerminalInPane = useCallback(
    (paneId: string) =>
      withActive(async (workspaceId) => {
        const distribution = defaultWslDistribution(workspaceId);
        if (!distribution) {
          notifyWslRequired();
          throw new Error(WSL_REQUIRED_MESSAGE);
        }
        return client.spawnDurableWslTerminal(
          workspaceId,
          distribution,
          workspaceProjectRoot(workspaceId),
          "active_pane",
          paneId
        );
      }),
    [client, defaultWslDistribution, notifyWslRequired, withActive, workspaceProjectRoot]
  );

  const spawnWslTerminal = useCallback(
    (distribution: string) =>
      withActive(async (workspaceId) => {
        return client.spawnWslTerminal(
          workspaceId,
          distribution || null,
          workspaceProjectRoot(workspaceId),
          "new_tab"
        );
      }),
    [client, withActive, workspaceProjectRoot]
  );

  useEffect(() => {
    if (!startupLaunchWorkspaceId || !ready || !wslChecked) {
      return;
    }
    if (activeRef.current !== startupLaunchWorkspaceId) {
      setStartupLaunchWorkspaceId(null);
      return;
    }
    const current = detailRef.current;
    if (current?.workspace.workspaceId !== startupLaunchWorkspaceId) {
      return;
    }
    if (current.surfaces.length > 0) {
      setStartupLaunchWorkspaceId(null);
      return;
    }

    let cancelled = false;
    (async () => {
      try {
        const distribution = defaultWslDistribution(startupLaunchWorkspaceId);
        if (!distribution) {
          notifyWslRequired();
          return;
        }
        await client.spawnWslTerminal(
          startupLaunchWorkspaceId,
          distribution,
          workspaceProjectRoot(startupLaunchWorkspaceId),
          "new_tab"
        );
        if (!cancelled) {
          await loadDetail(startupLaunchWorkspaceId);
          setError(null);
        }
      } catch (cause) {
        if (!cancelled) {
          setError(cause instanceof Error ? cause.message : "Startup terminal launch failed.");
        }
      } finally {
        if (!cancelled) {
          setStartupLaunchWorkspaceId(null);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [
    client,
    defaultWslDistribution,
    loadDetail,
    notifyWslRequired,
    ready,
    startupLaunchWorkspaceId,
    workspaceProjectRoot,
    wslChecked
  ]);

  const spawnAgent = useCallback(
    (command: string[]) =>
      withActive(async (workspaceId) => {
        // Launch the agent CLI in a durable WSL-tmux session so it survives
        // detach/restart. Uses the default WSL distribution.
        const distribution = defaultWslDistribution(workspaceId);
        if (!distribution) {
          notifyWslRequired();
          throw new Error(WSL_REQUIRED_MESSAGE);
        }

        const tmux = await client.checkTmux(distribution);
        if (!tmux.available) {
          notifyTmuxRequired(tmux.message || TMUX_REQUIRED_MESSAGE);
          throw new Error(tmux.message || TMUX_REQUIRED_MESSAGE);
        }

        return client.spawnAgentTerminal(workspaceId, command, distribution, "new_tab");
      }),
    [client, defaultWslDistribution, notifyTmuxRequired, notifyWslRequired, withActive]
  );

  const spawnDockControl = useCallback(
    (control: DockControl) =>
      withActiveResult(async (workspaceId) => {
        const distribution = defaultWslDistribution(workspaceId);
        if (!distribution) {
          notifyWslRequired();
          throw new Error(WSL_REQUIRED_MESSAGE);
        }
        const cwd = resolveDockCwd(control.cwd, workspaceProjectRoot(workspaceId));
        return client.spawnDockTerminal(workspaceId, control, distribution, cwd, "dock");
      }),
    [client, defaultWslDistribution, notifyWslRequired, withActiveResult, workspaceProjectRoot]
  );

  const splitActivePane = useCallback(
    (axis: "horizontal" | "vertical") =>
      withActive(async (workspaceId) => {
        const current = await getCurrentDetail(workspaceId);
        const paneId = current.workspace.activePaneId;
        if (!paneId) {
          return Promise.resolve();
        }
        return client.splitPane(workspaceId, paneId, axis);
      }),
    [client, getCurrentDetail, withActive]
  );

  const resizePane = useCallback(
    (paneId: string, ratio: number) => {
      const clamped = Math.min(0.9, Math.max(0.1, ratio));
      setDetail((current) => {
        const next = current
          ? {
              ...current,
              panes: current.panes.map((pane) =>
                pane.paneId === paneId ? { ...pane, splitRatio: clamped } : pane
              )
            }
          : current;
        detailRef.current = next;
        return next;
      });
      const workspaceId = activeRef.current;
      if (workspaceId) {
        void client.resizePaneLayout(workspaceId, paneId, clamped).catch(() => undefined);
      }
    },
    [client]
  );

  const createBrowserSurface = useCallback(
    (placement: "new_tab" | "active_pane" = "new_tab") =>
      withActiveResult(async (workspaceId) => {
        const current = await getCurrentDetail(workspaceId);
        const paneId = placement === "active_pane" ? current.workspace.activePaneId : undefined;
        return client.createBrowserSurface(workspaceId, paneId, "default", placement);
      }),
    [client, getCurrentDetail, withActiveResult]
  );

  const browserNavigate = useCallback(
    async (surfaceId: string, url: string) => {
      try {
        await client.browserNavigate(surfaceId, url);
        const workspaceId = activeRef.current;
        if (workspaceId) {
          await loadDetail(workspaceId);
        }
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser navigation failed.");
      }
    },
    [client, loadDetail]
  );

  const browserReload = useCallback(
    async (surfaceId: string) => {
      try {
        await client.browserReload(surfaceId);
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser reload failed.");
      }
    },
    [client]
  );

  const browserBack = useCallback(
    async (surfaceId: string) => {
      try {
        await client.browserBack(surfaceId);
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser back failed.");
      }
    },
    [client]
  );

  const browserForward = useCallback(
    async (surfaceId: string) => {
      try {
        await client.browserForward(surfaceId);
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser forward failed.");
      }
    },
    [client]
  );

  const browserCurrentUrl = useCallback(
    async (surfaceId: string) => {
      try {
        await client.browserCurrentUrl(surfaceId);
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser current URL failed.");
      }
    },
    [client]
  );

  const browserScreenshot = useCallback(
    async (surfaceId: string, format?: string | null) => {
      try {
        await client.browserScreenshot(surfaceId, format);
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser screenshot failed.");
      }
    },
    [client]
  );

  const browserDomSnapshot = useCallback(
    async (surfaceId: string, frameId?: string | null) => {
      try {
        await client.browserDomSnapshot(surfaceId, { frameId });
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser DOM snapshot failed.");
      }
    },
    [client]
  );

  const browserClick = useCallback(
    async (surfaceId: string, target: BrowserClickTarget) => {
      try {
        await client.browserClick(surfaceId, target);
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser click failed.");
      }
    },
    [client]
  );

  const browserType = useCallback(
    async (surfaceId: string, selector: string, text: string, frameId?: string | null) => {
      try {
        await client.browserType(surfaceId, selector, text, { frameId });
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser type failed.");
      }
    },
    [client]
  );

  const browserFill = useCallback(
    async (surfaceId: string, selector: string, text: string, frameId?: string | null) => {
      try {
        await client.browserFill(surfaceId, selector, text, { frameId });
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser fill failed.");
      }
    },
    [client]
  );

  const browserPress = useCallback(
    async (surfaceId: string, selector: string, key: string, frameId?: string | null) => {
      try {
        await client.browserPress(surfaceId, selector, key, { frameId });
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser press failed.");
      }
    },
    [client]
  );

  const browserSelect = useCallback(
    async (surfaceId: string, selector: string, values: string[], frameId?: string | null) => {
      try {
        await client.browserSelect(surfaceId, selector, values, { frameId });
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser select failed.");
      }
    },
    [client]
  );

  const browserScroll = useCallback(
    async (
      surfaceId: string,
      options: { selector?: string | null; x?: number | null; y?: number | null; frameId?: string | null }
    ) => {
      try {
        await client.browserScroll(surfaceId, options);
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser scroll failed.");
      }
    },
    [client]
  );

  const browserHover = useCallback(
    async (surfaceId: string, selector: string, frameId?: string | null) => {
      try {
        await client.browserHover(surfaceId, selector, { frameId });
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser hover failed.");
      }
    },
    [client]
  );

  const browserCheck = useCallback(
    async (surfaceId: string, selector: string, checked?: boolean | null, frameId?: string | null) => {
      try {
        await client.browserCheck(surfaceId, selector, checked, { frameId });
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser check failed.");
      }
    },
    [client]
  );

  const browserGet = useCallback(
    async (
      surfaceId: string,
      selector: string,
      options?: { kind?: string | null; attribute?: string | null }
    ) => {
      try {
        await client.browserGet(surfaceId, selector, options);
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser get failed.");
      }
    },
    [client]
  );

  const browserFind = useCallback(
    async (
      surfaceId: string,
      query: string,
      options?: { selector?: string | null; limit?: number | null }
    ) => {
      try {
        await client.browserFind(surfaceId, query, options);
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser find failed.");
      }
    },
    [client]
  );

  const browserHighlight = useCallback(
    async (surfaceId: string, selector: string, durationMs?: number | null, frameId?: string | null) => {
      try {
        await client.browserHighlight(surfaceId, selector, durationMs, { frameId });
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser highlight failed.");
      }
    },
    [client]
  );

  const browserFocus = useCallback(
    async (surfaceId: string, selector: string, frameId?: string | null) => {
      try {
        await client.browserFocus(surfaceId, selector, { frameId });
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser focus failed.");
      }
    },
    [client]
  );

  const browserZoom = useCallback(
    async (surfaceId: string, percent: number) => {
      try {
        await client.browserZoom(surfaceId, percent);
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser zoom failed.");
      }
    },
    [client]
  );

  const browserWaitForSelector = useCallback(
    async (surfaceId: string, selector: string, timeoutMs?: number | null, frameId?: string | null) => {
      try {
        await client.browserWaitForSelector(surfaceId, selector, timeoutMs, { frameId });
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser wait-for-selector failed.");
      }
    },
    [client]
  );

  const browserEvaluate = useCallback(
    async (surfaceId: string, script: string, frameId?: string | null) => {
      try {
        await client.browserEvaluate(surfaceId, script, { frameId });
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : "Browser evaluate failed.");
      }
    },
    [client]
  );

  const mountSurface = useCallback(
    (surfaceId: string, paneId?: string) =>
      withActive(async (workspaceId) => {
        if (paneId) {
          return client.mountSurface(workspaceId, paneId, surfaceId);
        }

        const current = await getCurrentDetail(workspaceId);
        return client.mountSurface(workspaceId, current.workspace.activePaneId, surfaceId);
      }),
    [client, getCurrentDetail, withActive]
  );

  const focusPane = useCallback(
    (paneId: string) => withActive((workspaceId) => client.focusPane(workspaceId, paneId)),
    [client, withActive]
  );

  const closePane = useCallback(
    (paneId: string) =>
      withActive((workspaceId) => client.closePane(workspaceId, paneId, "close_surface")),
    [client, withActive]
  );

  const closeSurface = useCallback(
    (surfaceId: string) =>
      withActive((workspaceId) => client.closeSurface(workspaceId, surfaceId)),
    [client, withActive]
  );

  const clearAttention = useCallback(
    async (sessionId: string) => {
      await client.clearAgentAttention(sessionId);
      await refresh();
    },
    [client, refresh]
  );

  const dismissNotification = useCallback(
    async (notificationId: string) => {
      if (notificationId.startsWith(LOCAL_NOTIFICATION_PREFIX)) {
        setNotifications((current) =>
          current.filter((notification) => notification.notificationId !== notificationId)
        );
        return;
      }
      await client.dismissNotification(notificationId);
      await refresh();
    },
    [client, refresh]
  );

  const createProfile = useCallback(
    async (input: SshProfileInput) => {
      await client.createProfile(input);
      setProfiles(await client.listProfiles());
    },
    [client]
  );

  const updateProfile = useCallback(
    async (profileId: string, input: SshProfileInput) => {
      await client.updateProfile(profileId, input);
      setProfiles(await client.listProfiles());
    },
    [client]
  );

  const deleteProfile = useCallback(
    async (profileId: string) => {
      await client.deleteProfile(profileId);
      setProfiles(await client.listProfiles());
    },
    [client]
  );

  const connectProfile = useCallback(
    (profile: SshProfile) =>
      withActive(async (workspaceId) => {
        const target = profile.port
          ? `${profile.user}@${profile.host}:${profile.port}`
          : `${profile.user}@${profile.host}`;
        return client.spawnSshTerminal(workspaceId, target, "new_tab");
      }),
    [client, withActive]
  );

  const attentionByWorkspace = useMemo(() => {
    const counts = new Map<string, number>();
    for (const state of attention) {
      counts.set(state.workspaceId, (counts.get(state.workspaceId) ?? 0) + 1);
    }
    return counts;
  }, [attention]);

  const attentionBySession = useMemo(
    () => new Map(attention.map((state) => [state.sessionId, state])),
    [attention]
  );

  const agentBySession = useMemo(
    () => new Map(agentStates.map((state) => [state.sessionId, state])),
    [agentStates]
  );

  return {
    client,
    ready,
    error,
    workspaces,
    workspaceGroups,
    activeWorkspaceId,
    detail,
    attention,
    agentStates,
    notifications,
    sidebarState,
    wslDistributions,
    profiles,
    attentionByWorkspace,
    attentionBySession,
    agentBySession,
    selectWorkspace,
    createWorkspace,
    renameWorkspace,
    updateWorkspace,
    closeWorkspace,
    createWorkspaceGroup,
    updateWorkspaceGroup,
    deleteWorkspaceGroup,
    addWorkspaceToGroup,
    removeWorkspaceFromGroup,
    createWorkspaceInGroup,
    spawnDefaultTerminal,
    spawnDefaultTerminalInPane,
    spawnDurableTerminalInPane,
    spawnWslTerminal,
    spawnAgent,
    spawnDockControl,
    splitActivePane,
    resizePane,
    focusPane,
    closePane,
    closeSurface,
    mountSurface,
    createBrowserSurface,
    browserNavigate,
    browserReload,
    browserBack,
    browserForward,
    browserCurrentUrl,
    browserScreenshot,
    browserDomSnapshot,
    browserClick,
    browserType,
    browserFill,
    browserPress,
    browserSelect,
    browserScroll,
    browserHover,
    browserCheck,
    browserGet,
    browserFind,
    browserHighlight,
    browserFocus,
    browserZoom,
    browserWaitForSelector,
    browserEvaluate,
    clearAttention,
    dismissNotification,
    createProfile,
    updateProfile,
    deleteProfile,
    connectProfile,
    refresh,
    refreshSidebar
  };
}
