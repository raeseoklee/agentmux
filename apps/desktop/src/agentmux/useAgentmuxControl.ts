import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  type AgentState,
  type ControlClient,
  createControlClient,
  type NotificationSummary,
  type SshProfile,
  type SshProfileInput,
  type WorkspaceDetail,
  type WorkspaceSummary,
  type WslDistribution
} from "../control/ControlClient";

const DEFAULT_PROJECT_ROOT = "D:\\Workspace\\irae\\agentmux";
const DEFAULT_WORKSPACE_NAME = "Local project";
const POLL_INTERVAL_MS = 1200;

export interface AgentmuxControl {
  client: ControlClient;
  ready: boolean;
  error: string | null;
  workspaces: WorkspaceSummary[];
  activeWorkspaceId: string | null;
  detail: WorkspaceDetail | null;
  attention: AgentState[];
  agentStates: AgentState[];
  notifications: NotificationSummary[];
  wslDistributions: WslDistribution[];
  profiles: SshProfile[];
  attentionByWorkspace: Map<string, number>;
  attentionBySession: Map<string, AgentState>;
  agentBySession: Map<string, AgentState>;
  selectWorkspace: (workspaceId: string) => Promise<void>;
  createWorkspace: (name?: string) => Promise<void>;
  renameWorkspace: (workspaceId: string, name: string) => Promise<void>;
  closeWorkspace: (workspaceId: string, policy: string) => Promise<void>;
  spawnNativeTerminal: () => Promise<void>;
  spawnWslTerminal: (distribution: string) => Promise<void>;
  splitActivePane: (axis: "horizontal" | "vertical") => Promise<void>;
  resizePane: (paneId: string, ratio: number) => void;
  focusPane: (paneId: string) => Promise<void>;
  closePane: (paneId: string) => Promise<void>;
  createBrowserSurface: () => Promise<void>;
  clearAttention: (sessionId: string) => Promise<void>;
  dismissNotification: (notificationId: string) => Promise<void>;
  createProfile: (input: SshProfileInput) => Promise<void>;
  deleteProfile: (profileId: string) => Promise<void>;
  connectProfile: (profile: SshProfile) => Promise<void>;
  refresh: () => Promise<void>;
}

// Bridges the design UI to the real control plane. Under Tauri this drives the
// Rust backend (real terminals); in a plain browser it drives the in-memory
// preview client. Either way the UI consumes the same shape.
export function useAgentmuxControl(): AgentmuxControl {
  const client = useMemo(() => createControlClient(), []);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceSummary[]>([]);
  const [activeWorkspaceId, setActiveWorkspaceId] = useState<string | null>(null);
  const [detail, setDetail] = useState<WorkspaceDetail | null>(null);
  const [attention, setAttention] = useState<AgentState[]>([]);
  const [agentStates, setAgentStates] = useState<AgentState[]>([]);
  const [notifications, setNotifications] = useState<NotificationSummary[]>([]);
  const [wslDistributions, setWslDistributions] = useState<WslDistribution[]>([]);
  const [profiles, setProfiles] = useState<SshProfile[]>([]);

  const activeRef = useRef<string | null>(null);
  activeRef.current = activeWorkspaceId;

  const loadDetail = useCallback(
    async (workspaceId: string) => {
      const next = await client.getWorkspace(workspaceId);
      if (activeRef.current === workspaceId) {
        setDetail(next);
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
      const [, nextAttention, nextStates, nextNotifications, nextProfiles] = await Promise.all([
        loadDetail(workspaceId),
        client.listAgentAttention(null),
        client.listAgentStates(null),
        client.listNotifications({ workspaceId: null, severity: null, includeDismissed: false }),
        client.listProfiles()
      ]);
      setAttention(nextAttention);
      setAgentStates(nextStates);
      setNotifications(nextNotifications);
      setProfiles(nextProfiles);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Control plane request failed.");
    }
  }, [client, loadDetail]);

  // Hydrate workspace list (create a default workspace when none exist).
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const listed = await client.listWorkspaces();
        const initial =
          listed[0] ?? (await client.createWorkspace(DEFAULT_WORKSPACE_NAME, DEFAULT_PROJECT_ROOT));
        const list = listed.length > 0 ? listed : [initial];
        if (cancelled) {
          return;
        }
        setWorkspaces(list);
        setActiveWorkspaceId(initial.workspaceId);
        activeRef.current = initial.workspaceId;
        await loadDetail(initial.workspaceId);
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
        }
      } catch {
        if (!cancelled) {
          setWslDistributions([]);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client]);

  // Poll active workspace + agent signals.
  useEffect(() => {
    if (!activeWorkspaceId) {
      return;
    }
    void refresh();
    const timer = window.setInterval(() => void refresh(), POLL_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [activeWorkspaceId, refresh]);

  const reloadWorkspaces = useCallback(async () => {
    const listed = await client.listWorkspaces();
    setWorkspaces(listed);
    return listed;
  }, [client]);

  const selectWorkspace = useCallback(
    async (workspaceId: string) => {
      setActiveWorkspaceId(workspaceId);
      activeRef.current = workspaceId;
      await loadDetail(workspaceId);
    },
    [loadDetail]
  );

  const createWorkspace = useCallback(
    async (name?: string) => {
      const created = await client.createWorkspace(
        name?.trim() || `Workspace ${workspaces.length + 1}`,
        DEFAULT_PROJECT_ROOT
      );
      await reloadWorkspaces();
      await selectWorkspace(created.workspaceId);
    },
    [client, reloadWorkspaces, selectWorkspace, workspaces.length]
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

  const closeWorkspace = useCallback(
    async (workspaceId: string, policy: string) => {
      await client.closeWorkspace(workspaceId, policy);
      const listed = await reloadWorkspaces();
      const next = listed[0]?.workspaceId ?? null;
      setActiveWorkspaceId(next);
      activeRef.current = next;
      if (next) {
        await loadDetail(next);
      } else {
        setDetail(null);
      }
    },
    [client, loadDetail, reloadWorkspaces]
  );

  const withActive = useCallback(
    async (action: (workspaceId: string) => Promise<unknown>) => {
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

  const spawnNativeTerminal = useCallback(
    () => withActive((workspaceId) => client.spawnNativeTerminal(workspaceId, ["cmd.exe", "/d", "/q"])),
    [client, withActive]
  );

  const spawnWslTerminal = useCallback(
    (distribution: string) =>
      withActive((workspaceId) =>
        client.spawnWslTerminal(workspaceId, distribution || null, DEFAULT_PROJECT_ROOT)
      ),
    [client, withActive]
  );

  const splitActivePane = useCallback(
    (axis: "horizontal" | "vertical") =>
      withActive((workspaceId) => {
        const paneId = detail?.workspace.activePaneId;
        if (!paneId) {
          return Promise.resolve();
        }
        return client.splitPane(workspaceId, paneId, axis);
      }),
    [client, detail?.workspace.activePaneId, withActive]
  );

  const resizePane = useCallback(
    (paneId: string, ratio: number) => {
      const clamped = Math.min(0.9, Math.max(0.1, ratio));
      setDetail((current) =>
        current
          ? {
              ...current,
              panes: current.panes.map((pane) =>
                pane.paneId === paneId ? { ...pane, splitRatio: clamped } : pane
              )
            }
          : current
      );
      const workspaceId = activeRef.current;
      if (workspaceId) {
        void client.resizePaneLayout(workspaceId, paneId, clamped).catch(() => undefined);
      }
    },
    [client]
  );

  const createBrowserSurface = useCallback(
    () =>
      // pane omitted -> backend mounts the browser surface on the active pane.
      withActive((workspaceId) => client.createBrowserSurface(workspaceId, undefined, "default")),
    [client, withActive]
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

  const clearAttention = useCallback(
    async (sessionId: string) => {
      await client.clearAgentAttention(sessionId);
      await refresh();
    },
    [client, refresh]
  );

  const dismissNotification = useCallback(
    async (notificationId: string) => {
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

  const deleteProfile = useCallback(
    async (profileId: string) => {
      await client.deleteProfile(profileId);
      setProfiles(await client.listProfiles());
    },
    [client]
  );

  const connectProfile = useCallback(
    (profile: SshProfile) =>
      withActive((workspaceId) => {
        const target = profile.port
          ? `${profile.user}@${profile.host}:${profile.port}`
          : `${profile.user}@${profile.host}`;
        return client.spawnSshTerminal(workspaceId, target);
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
    activeWorkspaceId,
    detail,
    attention,
    agentStates,
    notifications,
    wslDistributions,
    profiles,
    attentionByWorkspace,
    attentionBySession,
    agentBySession,
    selectWorkspace,
    createWorkspace,
    renameWorkspace,
    closeWorkspace,
    spawnNativeTerminal,
    spawnWslTerminal,
    splitActivePane,
    resizePane,
    focusPane,
    closePane,
    createBrowserSurface,
    clearAttention,
    dismissNotification,
    createProfile,
    deleteProfile,
    connectProfile,
    refresh
  };
}
