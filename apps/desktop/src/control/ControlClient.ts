export interface TerminalSession {
  sessionId: string;
  state: string;
  backendKind: string;
  backendNativeId?: string | null;
}

export interface AgentTelemetry {
  activity?: string | null;
  session?: string | null;
  cost?: string | null;
  tokens?: string | null;
  cache?: string | null;
  rate?: string | null;
  ctx?: string | null;
}

export interface AgentState {
  sessionId: string;
  workspaceId: string;
  state: string;
  attention: boolean;
  reason?: string | null;
  updatedAt?: string | null;
  telemetry?: AgentTelemetry | null;
}

export interface NotificationSummary {
  notificationId: string;
  notificationType: string;
  severity: string;
  workspaceId?: string | null;
  sessionId?: string | null;
  title: string;
  message: string;
  createdAt: string;
  dismissed: boolean;
}

export interface WorkspaceSummary {
  workspaceId: string;
  name: string;
  rootPaneId: string;
  activePaneId: string;
  projectRoot?: string | null;
  environmentProfileId?: string | null;
}

export interface PaneSummary {
  paneId: string;
  workspaceId: string;
  parentPaneId?: string | null;
  kind: string;
  splitAxis?: string | null;
  splitRatio?: number | null;
  mountedSurfaceId?: string | null;
}

export interface SurfaceSummary {
  surfaceId: string;
  workspaceId: string;
  surfaceType: string;
  title: string;
  sessionId?: string | null;
  browserId?: string | null;
}

export interface WorkspaceDetail {
  workspace: WorkspaceSummary;
  panes: PaneSummary[];
  surfaces: SurfaceSummary[];
  sessions: TerminalSession[];
}

export interface RecoveryDiagnostics {
  workspaceCount: number;
  paneCount: number;
  surfaceCount: number;
  sessionCount: number;
  sessions: Array<{
    sessionId: string;
    workspaceId: string;
    backendKind: string;
    state: string;
    durability: string;
    backendNativeId?: string | null;
  }>;
}

export interface WslDistribution {
  name: string;
  isDefault: boolean;
}

export interface SshProfile {
  profileId: string;
  name: string;
  host: string;
  user: string;
  port?: number | null;
}

export interface SshProfileInput {
  name: string;
  host: string;
  user: string;
  port?: number | null;
}

export interface BrowserNavigationResult {
  surfaceId: string;
  url: string;
}

export interface BrowserScreenshotResult {
  surfaceId: string;
  format: string;
  imageHandle: string;
  byteCount: number;
}

export interface BrowserDomSnapshotResult {
  surfaceId: string;
  html: string;
}

export interface BrowserActionResult {
  surfaceId: string;
  ok: boolean;
}

export interface BrowserEvaluateResult {
  surfaceId: string;
  valueJson: string;
}

export interface BrowserDiagnostic {
  surfaceId?: string | null;
  workspaceId?: string | null;
  operation: string;
  code: string;
  message: string;
  occurredAt: string;
}

export interface BrowserClickTarget {
  selector?: string;
  x?: number;
  y?: number;
}

export interface ControlClient {
  listWorkspaces(): Promise<WorkspaceSummary[]>;
  createWorkspace(name: string, projectRoot?: string | null): Promise<WorkspaceSummary>;
  getWorkspace(workspaceId: string): Promise<WorkspaceDetail>;
  renameWorkspace(workspaceId: string, name: string): Promise<WorkspaceSummary>;
  closeWorkspace(workspaceId: string, closePolicy: string): Promise<boolean>;
  splitPane(
    workspaceId: string,
    paneId: string,
    axis: "horizontal" | "vertical",
    ratio?: number
  ): Promise<WorkspaceDetail>;
  focusPane(workspaceId: string, paneId: string): Promise<WorkspaceDetail>;
  closePane(workspaceId: string, paneId: string, surfacePolicy: string): Promise<WorkspaceDetail>;
  resizePaneLayout(workspaceId: string, paneId: string, ratio: number): Promise<WorkspaceDetail>;
  mountSurface(workspaceId: string, paneId: string, surfaceId: string): Promise<WorkspaceDetail>;
  unmountSurface(workspaceId: string, paneId: string): Promise<WorkspaceDetail>;
  createBrowserSurface(
    workspaceId: string,
    paneId?: string | null,
    profile?: string | null
  ): Promise<SurfaceSummary>;
  browserNavigate(surfaceId: string, url: string): Promise<BrowserNavigationResult>;
  browserScreenshot(surfaceId: string, format?: string | null): Promise<BrowserScreenshotResult>;
  browserDomSnapshot(surfaceId: string): Promise<BrowserDomSnapshotResult>;
  browserClick(surfaceId: string, target: BrowserClickTarget): Promise<BrowserActionResult>;
  browserType(surfaceId: string, selector: string, text: string): Promise<BrowserActionResult>;
  browserEvaluate(surfaceId: string, script: string): Promise<BrowserEvaluateResult>;
  browserDiagnostics(options?: {
    workspaceId?: string | null;
    surfaceId?: string | null;
  }): Promise<BrowserDiagnostic[]>;
  recoveryDiagnostics(): Promise<RecoveryDiagnostics>;
  listWslDistributions(): Promise<WslDistribution[]>;
  listProfiles(): Promise<SshProfile[]>;
  createProfile(input: SshProfileInput): Promise<SshProfile>;
  updateProfile(profileId: string, input: SshProfileInput): Promise<SshProfile>;
  deleteProfile(profileId: string): Promise<void>;
  spawnNativeTerminal(workspaceId: string, command: string[]): Promise<TerminalSession>;
  spawnWslTerminal(
    workspaceId: string,
    distribution: string | null,
    cwd: string | null
  ): Promise<TerminalSession>;
  spawnSshTerminal(workspaceId: string, target: string): Promise<TerminalSession>;
  spawnAgentTerminal(
    workspaceId: string,
    command: string[],
    distribution: string | null
  ): Promise<TerminalSession>;
  getSession(sessionId: string): Promise<TerminalSession>;
  readRecent(sessionId: string, maxBytes: number): Promise<string>;
  sendText(sessionId: string, text: string): Promise<void>;
  sendKey(sessionId: string, key: string): Promise<void>;
  resize(sessionId: string, columns: number, rows: number): Promise<void>;
  listAgentAttention(workspaceId?: string | null): Promise<AgentState[]>;
  listAgentStates(workspaceId?: string | null): Promise<AgentState[]>;
  clearAgentAttention(sessionId: string): Promise<void>;
  listNotifications(options: {
    workspaceId?: string | null;
    severity?: string | null;
    includeDismissed?: boolean;
  }): Promise<NotificationSummary[]>;
  dismissNotification(notificationId: string): Promise<void>;
}

interface TauriCore {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
}

declare global {
  interface Window {
    __TAURI__?: {
      core?: TauriCore;
    };
    __AGENTMUX_PREVIEW__?: BrowserPreviewApi;
    __AGENTMUX_PREVIEW_READY__?: boolean;
  }
}

export function createControlClient(): ControlClient {
  const invoke = window.__TAURI__?.core?.invoke;
  if (invoke) {
    return new TauriControlClient(invoke);
  }

  return new BrowserPreviewControlClient();
}

class TauriControlClient implements ControlClient {
  private requestCounter = 0;
  private controlToken?: Promise<string>;

  constructor(private readonly invoke: TauriCore["invoke"]) {}

  async listWorkspaces(): Promise<WorkspaceSummary[]> {
    const result = await this.call<{ workspaces: WorkspaceSummaryWire[] }>("workspace.list", {});
    return result.workspaces.map(mapWorkspace);
  }

  async createWorkspace(name: string, projectRoot?: string | null): Promise<WorkspaceSummary> {
    const result = await this.call<WorkspaceSummaryWire>("workspace.create", {
      name,
      project_root: projectRoot ?? null,
      backend_profile: null
    });
    return mapWorkspace(result);
  }

  async getWorkspace(workspaceId: string): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>("workspace.get", {
      workspace_id: workspaceId
    });
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession)
    };
  }

  async renameWorkspace(workspaceId: string, name: string): Promise<WorkspaceSummary> {
    const result = await this.call<WorkspaceSummaryWire>("workspace.rename", {
      workspace_id: workspaceId,
      name
    });
    return mapWorkspace(result);
  }

  async closeWorkspace(workspaceId: string, closePolicy: string): Promise<boolean> {
    const result = await this.call<{ closed: boolean }>("workspace.close", {
      workspace_id: workspaceId,
      close_policy: closePolicy
    });
    return result.closed;
  }

  async splitPane(
    workspaceId: string,
    paneId: string,
    axis: "horizontal" | "vertical",
    ratio = 0.5
  ): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>("pane.split", {
      workspace_id: workspaceId,
      pane_id: paneId,
      axis,
      ratio
    });
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession)
    };
  }

  async focusPane(workspaceId: string, paneId: string): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>("pane.focus", {
      workspace_id: workspaceId,
      pane_id: paneId
    });
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession)
    };
  }

  async closePane(
    workspaceId: string,
    paneId: string,
    surfacePolicy: string
  ): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>("pane.close", {
      workspace_id: workspaceId,
      pane_id: paneId,
      surface_policy: surfacePolicy
    });
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession)
    };
  }

  async resizePaneLayout(
    workspaceId: string,
    paneId: string,
    ratio: number
  ): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>("pane.resize_layout", {
      workspace_id: workspaceId,
      pane_id: paneId,
      ratio
    });
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession)
    };
  }

  async mountSurface(
    workspaceId: string,
    paneId: string,
    surfaceId: string
  ): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>("pane.mount_surface", {
      workspace_id: workspaceId,
      pane_id: paneId,
      surface_id: surfaceId
    });
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession)
    };
  }

  async unmountSurface(workspaceId: string, paneId: string): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>("pane.unmount_surface", {
      workspace_id: workspaceId,
      pane_id: paneId
    });
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession)
    };
  }

  async createBrowserSurface(
    workspaceId: string,
    paneId?: string | null,
    profile?: string | null
  ): Promise<SurfaceSummary> {
    const result = await this.call<SurfaceSummaryWire>("surface.create_browser", {
      workspace_id: workspaceId,
      pane_id: paneId ?? null,
      profile: profile ?? null
    });
    return mapSurface(result);
  }

  async browserNavigate(surfaceId: string, url: string): Promise<BrowserNavigationResult> {
    const result = await this.call<BrowserNavigationResultWire>("browser.navigate", {
      surface_id: surfaceId,
      url
    });
    return mapBrowserNavigation(result);
  }

  async browserScreenshot(
    surfaceId: string,
    format?: string | null
  ): Promise<BrowserScreenshotResult> {
    const result = await this.call<BrowserScreenshotResultWire>("browser.screenshot", {
      surface_id: surfaceId,
      format: format ?? null
    });
    return mapBrowserScreenshot(result);
  }

  async browserDomSnapshot(surfaceId: string): Promise<BrowserDomSnapshotResult> {
    const result = await this.call<BrowserDomSnapshotResultWire>("browser.dom_snapshot", {
      surface_id: surfaceId
    });
    return mapBrowserDomSnapshot(result);
  }

  async browserClick(
    surfaceId: string,
    target: BrowserClickTarget
  ): Promise<BrowserActionResult> {
    const result = await this.call<BrowserActionResultWire>("browser.click", {
      surface_id: surfaceId,
      selector: target.selector ?? null,
      x: target.x ?? null,
      y: target.y ?? null
    });
    return mapBrowserAction(result);
  }

  async browserType(
    surfaceId: string,
    selector: string,
    text: string
  ): Promise<BrowserActionResult> {
    const result = await this.call<BrowserActionResultWire>("browser.type", {
      surface_id: surfaceId,
      selector,
      text
    });
    return mapBrowserAction(result);
  }

  async browserEvaluate(surfaceId: string, script: string): Promise<BrowserEvaluateResult> {
    const result = await this.call<BrowserEvaluateResultWire>("browser.evaluate", {
      surface_id: surfaceId,
      script
    });
    return mapBrowserEvaluate(result);
  }

  async browserDiagnostics(options?: {
    workspaceId?: string | null;
    surfaceId?: string | null;
  }): Promise<BrowserDiagnostic[]> {
    const result = await this.call<{ failures: BrowserDiagnosticWire[] }>("diagnostics.browser", {
      workspace_id: options?.workspaceId ?? null,
      surface_id: options?.surfaceId ?? null
    });
    return result.failures.map(mapBrowserDiagnostic);
  }

  async recoveryDiagnostics(): Promise<RecoveryDiagnostics> {
    const result = await this.call<RecoveryDiagnosticsWire>("diagnostics.recovery", {});
    return {
      workspaceCount: result.workspace_count,
      paneCount: result.pane_count,
      surfaceCount: result.surface_count,
      sessionCount: result.session_count,
      sessions: result.sessions.map((session) => ({
        sessionId: session.session_id,
        workspaceId: session.workspace_id,
        backendKind: session.backend_kind,
        state: session.state,
        durability: session.durability,
        backendNativeId: session.backend_native_id
      }))
    };
  }

  async listWslDistributions(): Promise<WslDistribution[]> {
    const result = await this.call<{ distributions: WslDistributionWire[] }>(
      "diagnostics.wsl_distributions",
      {}
    );
    return result.distributions.map(mapWslDistribution);
  }

  async listProfiles(): Promise<SshProfile[]> {
    const result = await this.call<{ profiles: SshProfileWire[] }>("profile.list", {});
    return result.profiles.map(mapProfile);
  }

  async createProfile(input: SshProfileInput): Promise<SshProfile> {
    const result = await this.call<SshProfileWire>("profile.create", {
      name: input.name,
      host: input.host,
      user: input.user,
      port: input.port ?? null
    });
    return mapProfile(result);
  }

  async updateProfile(profileId: string, input: SshProfileInput): Promise<SshProfile> {
    const result = await this.call<SshProfileWire>("profile.update", {
      profile_id: profileId,
      name: input.name,
      host: input.host,
      user: input.user,
      port: input.port ?? null
    });
    return mapProfile(result);
  }

  async deleteProfile(profileId: string): Promise<void> {
    await this.call("profile.delete", { profile_id: profileId });
  }

  async spawnNativeTerminal(workspaceId: string, command: string[]): Promise<TerminalSession> {
    const result = await this.call<{ session_id: string }>("session.spawn", {
      workspace_id: workspaceId,
      backend: "conpty",
      command,
      cwd: null,
      columns: 120,
      rows: 30,
      durability: "ephemeral"
    });

    return {
      sessionId: result.session_id,
      backendKind: "conpty",
      state: "running"
    };
  }

  async spawnWslTerminal(
    workspaceId: string,
    distribution: string | null,
    cwd: string | null
  ): Promise<TerminalSession> {
    const result = await this.call<{ session_id: string }>("session.spawn", {
      workspace_id: workspaceId,
      backend: "wsl-direct",
      backend_profile: distribution,
      command: ["bash", "-l"],
      cwd,
      columns: 120,
      rows: 30,
      durability: "ephemeral"
    });

    return {
      sessionId: result.session_id,
      backendKind: "wsl-direct",
      state: "running"
    };
  }

  async spawnSshTerminal(workspaceId: string, target: string): Promise<TerminalSession> {
    const result = await this.call<{ session_id: string }>("session.spawn", {
      workspace_id: workspaceId,
      backend: "ssh",
      backend_profile: target,
      command: [],
      cwd: null,
      columns: 120,
      rows: 30,
      durability: "ephemeral"
    });

    return {
      sessionId: result.session_id,
      backendKind: "ssh",
      state: "running"
    };
  }

  async spawnAgentTerminal(
    workspaceId: string,
    command: string[],
    distribution: string | null
  ): Promise<TerminalSession> {
    const result = await this.call<{ session_id: string }>("session.spawn", {
      workspace_id: workspaceId,
      backend: "wsl-tmux-control",
      backend_profile: distribution,
      command,
      cwd: null,
      columns: 120,
      rows: 30,
      durability: "durable"
    });

    return {
      sessionId: result.session_id,
      backendKind: "wsl-tmux-control",
      state: "running"
    };
  }

  async readRecent(sessionId: string, maxBytes: number): Promise<string> {
    const result = await this.call<{ text: string }>("session.read_recent", {
      session_id: sessionId,
      max_bytes: maxBytes
    });
    return result.text;
  }

  async getSession(sessionId: string): Promise<TerminalSession> {
    const result = await this.call<SessionSummaryWire>("session.get", {
      session_id: sessionId
    });
    return mapSession(result);
  }

  async sendText(sessionId: string, text: string): Promise<void> {
    await this.call("session.send_text", {
      session_id: sessionId,
      text
    });
  }

  async sendKey(sessionId: string, key: string): Promise<void> {
    await this.call("session.send_key", {
      session_id: sessionId,
      key
    });
  }

  async resize(sessionId: string, columns: number, rows: number): Promise<void> {
    await this.call("session.resize", {
      session_id: sessionId,
      columns,
      rows
    });
  }

  async listAgentAttention(workspaceId?: string | null): Promise<AgentState[]> {
    const result = await this.call<{ sessions: AgentStateWire[] }>("agent.list_attention", {
      workspace_id: workspaceId ?? null
    });
    return result.sessions.map(mapAgentState);
  }

  async listAgentStates(workspaceId?: string | null): Promise<AgentState[]> {
    const result = await this.call<{ sessions: AgentStateWire[] }>("agent.list", {
      workspace_id: workspaceId ?? null
    });
    return result.sessions.map(mapAgentState);
  }

  async clearAgentAttention(sessionId: string): Promise<void> {
    await this.call("agent.clear_attention", {
      session_id: sessionId
    });
  }

  async listNotifications(options: {
    workspaceId?: string | null;
    severity?: string | null;
    includeDismissed?: boolean;
  }): Promise<NotificationSummary[]> {
    const result = await this.call<{ notifications: NotificationSummaryWire[] }>(
      "notification.list",
      {
        workspace_id: options.workspaceId ?? null,
        severity: options.severity ?? null,
        include_dismissed: options.includeDismissed ?? false
      }
    );
    return result.notifications.map(mapNotification);
  }

  async dismissNotification(notificationId: string): Promise<void> {
    await this.call("notification.dismiss", {
      notification_id: notificationId
    });
  }

  private async call<T = unknown>(method: string, params: unknown): Promise<T> {
    const token = await this.getControlToken();
    const response = await this.invoke<ControlResponse>("agentmux_control", {
      request: {
        schema: "agentmux.control.v1",
        id: `ui_${++this.requestCounter}`,
        method,
        params_json: JSON.stringify(params),
        auth: {
          token
        }
      }
    });

    if ("Error" in response.outcome) {
      throw new Error(response.outcome.Error.message);
    }

    return JSON.parse(response.outcome.Ok.result_json) as T;
  }

  private getControlToken(): Promise<string> {
    this.controlToken ??= this.invoke<string>("agentmux_control_token");
    return this.controlToken;
  }
}

interface ControlResponse {
  schema: string;
  id: string;
  outcome:
    | {
        Ok: {
          result_json: string;
        };
      }
    | {
        Error: {
          message: string;
        };
      };
}

class BrowserPreviewControlClient implements ControlClient {
  private workspaceCounter = 0;
  private readonly workspaces: WorkspaceSummary[] = [];
  private readonly panes = new Map<string, PaneSummary[]>();
  private readonly agentStates = new Map<string, AgentState>();
  private readonly notifications: NotificationSummary[] = [];
  private readonly terminalSurfaces: SurfaceSummary[] = [];
  private readonly sessions = new Map<string, TerminalSession>();
  private readonly outputs = new Map<string, string>();
  private readonly browserSurfaces: SurfaceSummary[] = [];
  private readonly browserUrls = new Map<string, string>();
  private terminalCounter = 0;
  private lastSessionId?: string;
  private profileCounter = 3;
  private readonly profiles: SshProfile[] = [
    { profileId: "prof_preview_1", name: "prod-server", host: "10.0.4.12", user: "deploy", port: 22 },
    { profileId: "prof_preview_2", name: "staging-db", host: "10.0.7.3", user: "ops", port: 22 },
    { profileId: "prof_preview_3", name: "gpu-box", host: "gpu.lan", user: "ml", port: 22 }
  ];

  constructor() {
    const previewApi: BrowserPreviewApi = {
      syntheticAgentState: (detail) => this.applySyntheticAgentState(detail)
    };
    window.__AGENTMUX_PREVIEW__ = previewApi;
    window.__AGENTMUX_PREVIEW_READY__ = true;
    window.addEventListener("agentmux:synthetic-agent-state", (event) => {
      if (window.__AGENTMUX_PREVIEW__ !== previewApi) {
        return;
      }
      this.applySyntheticAgentState((event as CustomEvent<SyntheticAgentStateDetail>).detail);
    });
  }

  async listWorkspaces(): Promise<WorkspaceSummary[]> {
    return [...this.workspaces];
  }

  async createWorkspace(name: string, projectRoot?: string | null): Promise<WorkspaceSummary> {
    const suffix = ++this.workspaceCounter;
    const workspace: WorkspaceSummary = {
      workspaceId: `ws_browser_preview_${suffix}`,
      name,
      rootPaneId: `pane_browser_preview_${suffix}`,
      activePaneId: `pane_browser_preview_${suffix}`,
      projectRoot: projectRoot ?? null,
      environmentProfileId: null
    };
    this.workspaces.push(workspace);
    this.panes.set(workspace.workspaceId, [
      {
        paneId: workspace.rootPaneId,
        workspaceId: workspace.workspaceId,
        parentPaneId: null,
        kind: "leaf",
        splitAxis: null,
        splitRatio: null,
        mountedSurfaceId: null
      }
    ]);
    return workspace;
  }

  async getWorkspace(workspaceId: string): Promise<WorkspaceDetail> {
    const workspace = this.findWorkspace(workspaceId);
    const terminalSurfaces = this.terminalSurfaces.filter(
      (surface) => surface.workspaceId === workspaceId
    );
    const sessionIds = new Set(
      terminalSurfaces.map((surface) => surface.sessionId).filter(Boolean)
    );
    return {
      workspace,
      panes: [...(this.panes.get(workspaceId) ?? [])],
      surfaces: [
        ...terminalSurfaces,
        ...this.browserSurfaces.filter((surface) => surface.workspaceId === workspaceId)
      ],
      sessions: [...this.sessions.values()].filter((session) => sessionIds.has(session.sessionId))
    };
  }

  async renameWorkspace(workspaceId: string, name: string): Promise<WorkspaceSummary> {
    const workspace = this.findWorkspace(workspaceId);
    workspace.name = name;
    return workspace;
  }

  async closeWorkspace(workspaceId: string): Promise<boolean> {
    const index = this.workspaces.findIndex((workspace) => workspace.workspaceId === workspaceId);
    if (index < 0) {
      return false;
    }
    this.workspaces.splice(index, 1);
    this.panes.delete(workspaceId);
    for (let i = this.terminalSurfaces.length - 1; i >= 0; i -= 1) {
      if (this.terminalSurfaces[i].workspaceId === workspaceId) {
        const sessionId = this.terminalSurfaces[i].sessionId;
        if (sessionId) {
          this.sessions.delete(sessionId);
          this.outputs.delete(sessionId);
          this.agentStates.delete(sessionId);
        }
        this.terminalSurfaces.splice(i, 1);
      }
    }
    for (let i = this.browserSurfaces.length - 1; i >= 0; i -= 1) {
      if (this.browserSurfaces[i].workspaceId === workspaceId) {
        this.browserUrls.delete(this.browserSurfaces[i].surfaceId);
        this.browserSurfaces.splice(i, 1);
      }
    }
    return true;
  }

  async splitPane(
    workspaceId: string,
    paneId: string,
    axis: "horizontal" | "vertical",
    ratio = 0.5
  ): Promise<WorkspaceDetail> {
    const workspace = this.findWorkspace(workspaceId);
    const panes = this.panes.get(workspaceId) ?? [];
    const target = panes.find((pane) => pane.paneId === paneId);
    if (!target || target.kind !== "leaf") {
      throw new Error(`Pane '${paneId}' cannot be split.`);
    }

    const firstChildId = `${paneId}_a`;
    const secondChildId = `${paneId}_b`;
    const mountedSurfaceId = target.mountedSurfaceId ?? null;
    target.kind = "split";
    target.splitAxis = axis;
    target.splitRatio = ratio;
    target.mountedSurfaceId = null;
    panes.push(
      {
        paneId: firstChildId,
        workspaceId,
        parentPaneId: paneId,
        kind: "leaf",
        splitAxis: null,
        splitRatio: null,
        mountedSurfaceId
      },
      {
        paneId: secondChildId,
        workspaceId,
        parentPaneId: paneId,
        kind: "leaf",
        splitAxis: null,
        splitRatio: null,
        mountedSurfaceId: null
      }
    );
    if (workspace.activePaneId === paneId) {
      workspace.activePaneId = firstChildId;
    }
    return this.getWorkspace(workspaceId);
  }

  async focusPane(workspaceId: string, paneId: string): Promise<WorkspaceDetail> {
    const workspace = this.findWorkspace(workspaceId);
    const pane = (this.panes.get(workspaceId) ?? []).find((candidate) => candidate.paneId === paneId);
    if (!pane || pane.kind !== "leaf") {
      throw new Error(`Pane '${paneId}' cannot be focused.`);
    }
    workspace.activePaneId = paneId;
    return this.getWorkspace(workspaceId);
  }

  async closePane(
    workspaceId: string,
    paneId: string,
    surfacePolicy: string
  ): Promise<WorkspaceDetail> {
    const workspace = this.findWorkspace(workspaceId);
    const panes = this.panes.get(workspaceId) ?? [];
    const target = panes.find((pane) => pane.paneId === paneId);
    if (!target || target.kind !== "leaf" || !target.parentPaneId) {
      throw new Error(`Pane '${paneId}' cannot be closed.`);
    }
    if (
      surfacePolicy === "fail_if_session_running" &&
      target.mountedSurfaceId &&
      this.surfaceHasRunningSession(target.mountedSurfaceId)
    ) {
      throw new Error("Pane has a running session.");
    }
    const closedSurfaceId = target.mountedSurfaceId ?? null;

    const parent = panes.find((pane) => pane.paneId === target.parentPaneId);
    const siblings = panes.filter((pane) => pane.parentPaneId === target.parentPaneId);
    const remaining = siblings.find((pane) => pane.paneId !== paneId);
    this.panes.set(
      workspaceId,
      panes.filter((pane) => pane.paneId !== paneId && pane.paneId !== remaining?.paneId)
    );

    if (parent && remaining) {
      parent.kind = remaining.kind;
      parent.splitAxis = remaining.splitAxis;
      parent.splitRatio = remaining.splitRatio;
      parent.mountedSurfaceId = remaining.mountedSurfaceId;
      for (const pane of this.panes.get(workspaceId) ?? []) {
        if (pane.parentPaneId === remaining.paneId) {
          pane.parentPaneId = parent.paneId;
        }
      }
      if (workspace.activePaneId === paneId || workspace.activePaneId === remaining.paneId) {
        workspace.activePaneId = parent.paneId;
      }
    }

    if (
      closedSurfaceId &&
      (surfacePolicy === "close_surface" || surfacePolicy === "fail_if_session_running")
    ) {
      this.closeSurface(closedSurfaceId);
    }

    return this.getWorkspace(workspaceId);
  }

  async resizePaneLayout(
    workspaceId: string,
    paneId: string,
    ratio: number
  ): Promise<WorkspaceDetail> {
    const pane = (this.panes.get(workspaceId) ?? []).find((candidate) => candidate.paneId === paneId);
    if (!pane || pane.kind !== "split") {
      throw new Error(`Pane '${paneId}' cannot be resized.`);
    }
    pane.splitRatio = Math.min(0.9, Math.max(0.1, ratio));
    return this.getWorkspace(workspaceId);
  }

  async mountSurface(
    workspaceId: string,
    paneId: string,
    surfaceId: string
  ): Promise<WorkspaceDetail> {
    this.findWorkspace(workspaceId);
    const panes = this.panes.get(workspaceId) ?? [];
    const pane = panes.find((candidate) => candidate.paneId === paneId);
    if (!pane || pane.kind !== "leaf") {
      throw new Error(`Pane '${paneId}' cannot mount surfaces.`);
    }
    for (const candidate of panes) {
      if (candidate.mountedSurfaceId === surfaceId) {
        candidate.mountedSurfaceId = null;
      }
    }
    pane.mountedSurfaceId = surfaceId;
    return this.focusPane(workspaceId, paneId);
  }

  async unmountSurface(workspaceId: string, paneId: string): Promise<WorkspaceDetail> {
    this.findWorkspace(workspaceId);
    const pane = (this.panes.get(workspaceId) ?? []).find((candidate) => candidate.paneId === paneId);
    if (!pane || pane.kind !== "leaf") {
      throw new Error(`Pane '${paneId}' cannot unmount surfaces.`);
    }
    pane.mountedSurfaceId = null;
    return this.getWorkspace(workspaceId);
  }

  async createBrowserSurface(
    workspaceId: string,
    paneId?: string | null,
    profile?: string | null
  ): Promise<SurfaceSummary> {
    const workspace = this.findWorkspace(workspaceId);
    const targetPaneId = paneId ?? workspace.activePaneId;
    const pane = (this.panes.get(workspaceId) ?? []).find(
      (candidate) => candidate.paneId === targetPaneId
    );
    if (!pane || pane.kind !== "leaf") {
      throw new Error(`Pane '${targetPaneId}' cannot mount browser surfaces.`);
    }
    const suffix = this.browserSurfaces.length + 1;
    const surface: SurfaceSummary = {
      surfaceId: `surf_browser_preview_${suffix}`,
      workspaceId,
      surfaceType: "browser",
      title: profile ? `Browser ${profile}` : "Browser",
      sessionId: null,
      browserId: `browser_preview_${suffix}`
    };
    this.browserSurfaces.push(surface);
    this.browserUrls.set(surface.surfaceId, "about:blank");
    await this.mountSurface(workspaceId, targetPaneId, surface.surfaceId);
    return surface;
  }

  async browserNavigate(surfaceId: string, url: string): Promise<BrowserNavigationResult> {
    this.findBrowserSurface(surfaceId);
    this.browserUrls.set(surfaceId, url);
    return {
      surfaceId,
      url
    };
  }

  async browserScreenshot(
    surfaceId: string,
    format?: string | null
  ): Promise<BrowserScreenshotResult> {
    this.findBrowserSurface(surfaceId);
    const resolvedFormat = format || "png";
    const imageHandle = `memory://browser-preview/${surfaceId}/${resolvedFormat}`;
    return {
      surfaceId,
      format: resolvedFormat,
      imageHandle,
      byteCount: imageHandle.length
    };
  }

  async browserDomSnapshot(surfaceId: string): Promise<BrowserDomSnapshotResult> {
    this.findBrowserSurface(surfaceId);
    const url = this.browserUrls.get(surfaceId) ?? "about:blank";
    return {
      surfaceId,
      html: `<html data-agentmux-surface="${surfaceId}"><body>${url}</body></html>`
    };
  }

  async browserClick(surfaceId: string, _target: BrowserClickTarget): Promise<BrowserActionResult> {
    this.findBrowserSurface(surfaceId);
    return {
      surfaceId,
      ok: true
    };
  }

  async browserType(
    surfaceId: string,
    _selector: string,
    _text: string
  ): Promise<BrowserActionResult> {
    this.findBrowserSurface(surfaceId);
    return {
      surfaceId,
      ok: true
    };
  }

  async browserEvaluate(surfaceId: string, _script: string): Promise<BrowserEvaluateResult> {
    this.findBrowserSurface(surfaceId);
    return {
      surfaceId,
      valueJson: '{"ok":true}'
    };
  }

  async browserDiagnostics(): Promise<BrowserDiagnostic[]> {
    return [];
  }

  async recoveryDiagnostics(): Promise<RecoveryDiagnostics> {
    const sessions = [...this.sessions.values()];
    return {
      workspaceCount: this.workspaces.length,
      paneCount: [...this.panes.values()].reduce((count, panes) => count + panes.length, 0),
      surfaceCount: this.terminalSurfaces.length + this.browserSurfaces.length,
      sessionCount: sessions.length,
      sessions: sessions.map((session) => {
        const surface = this.terminalSurfaces.find(
          (candidate) => candidate.sessionId === session.sessionId
        );
        return {
          sessionId: session.sessionId,
          workspaceId: surface?.workspaceId ?? this.workspaces[0]?.workspaceId ?? "ws_browser_preview",
          backendKind: session.backendKind,
          state: session.state,
          durability: "ephemeral",
          backendNativeId: null
        };
      })
    };
  }

  async listWslDistributions(): Promise<WslDistribution[]> {
    return [
      {
        name: "Ubuntu",
        isDefault: true
      },
      {
        name: "Debian",
        isDefault: false
      }
    ];
  }

  async listProfiles(): Promise<SshProfile[]> {
    return this.profiles.map((profile) => ({ ...profile }));
  }

  async createProfile(input: SshProfileInput): Promise<SshProfile> {
    const profile: SshProfile = {
      profileId: `prof_preview_${++this.profileCounter}`,
      name: input.name,
      host: input.host,
      user: input.user,
      port: input.port ?? null
    };
    this.profiles.push(profile);
    return { ...profile };
  }

  async updateProfile(profileId: string, input: SshProfileInput): Promise<SshProfile> {
    const profile = this.profiles.find((candidate) => candidate.profileId === profileId);
    if (!profile) {
      throw new Error(`Profile '${profileId}' was not found.`);
    }
    profile.name = input.name;
    profile.host = input.host;
    profile.user = input.user;
    profile.port = input.port ?? null;
    return { ...profile };
  }

  async deleteProfile(profileId: string): Promise<void> {
    const index = this.profiles.findIndex((candidate) => candidate.profileId === profileId);
    if (index >= 0) {
      this.profiles.splice(index, 1);
    }
  }

  async spawnNativeTerminal(workspaceId: string, command: string[]): Promise<TerminalSession> {
    const commandText = command.join(" ");
    return this.createPreviewTerminal(workspaceId, "conpty", "conpty", [
      "\r\n$ " + commandText,
      "\r\nagentmux desktop preview",
      "\r\n"
    ].join(""));
  }

  async spawnWslTerminal(
    workspaceId: string,
    distribution: string | null,
    cwd: string | null
  ): Promise<TerminalSession> {
    return this.createPreviewTerminal(workspaceId, "wsl-direct", "wsl-direct", [
      "\r\n$ wsl " + (distribution ?? "default") + " " + (cwd ?? "~"),
      "\r\nagentmux WSL desktop preview",
      "\r\n"
    ].join(""));
  }

  async spawnSshTerminal(workspaceId: string, target: string): Promise<TerminalSession> {
    return this.createPreviewTerminal(workspaceId, "ssh", "ssh", [
      "\r\n$ ssh " + target,
      "\r\nagentmux SSH desktop preview (실제 접속은 Tauri 실행에서 동작)",
      "\r\n"
    ].join(""));
  }

  async spawnAgentTerminal(
    workspaceId: string,
    command: string[],
    distribution: string | null
  ): Promise<TerminalSession> {
    const label = command.join(" ") || "agent";
    return this.createPreviewTerminal(workspaceId, "wsl-tmux-control", "wsl-tmux-control", [
      "\r\n$ " + label + "   (durable tmux · " + (distribution ?? "WSL") + ")",
      "\r\nagentmux 에이전트 세션 preview — 실제 실행/durable 복원은 Tauri에서 동작",
      "\r\n"
    ].join(""));
  }

  async readRecent(sessionId: string, _maxBytes: number): Promise<string> {
    return this.outputs.get(sessionId) ?? "";
  }

  async getSession(sessionId: string): Promise<TerminalSession> {
    return (
      this.sessions.get(sessionId) ?? {
        sessionId,
        backendKind: "conpty",
        state: "lost"
      }
    );
  }

  async sendText(sessionId: string, text: string): Promise<void> {
    let output = this.outputs.get(sessionId) ?? "";
    output += text;
    if (text.includes("\r")) {
      output += "C:\\agentmux> ";
    }
    this.outputs.set(sessionId, output);
  }

  async sendKey(sessionId: string, key: string): Promise<void> {
    if (key === "enter") {
      this.outputs.set(sessionId, (this.outputs.get(sessionId) ?? "") + "\r\n");
    }
  }

  async resize(_sessionId: string, _columns: number, _rows: number): Promise<void> {
    return;
  }

  async listAgentAttention(workspaceId?: string | null): Promise<AgentState[]> {
    return [...this.agentStates.values()].filter(
      (state) => state.attention && (!workspaceId || state.workspaceId === workspaceId)
    );
  }

  async listAgentStates(workspaceId?: string | null): Promise<AgentState[]> {
    return [...this.agentStates.values()].filter(
      (state) => !workspaceId || state.workspaceId === workspaceId
    );
  }

  async clearAgentAttention(sessionId: string): Promise<void> {
    const state = this.agentStates.get(sessionId);
    if (state) {
      state.attention = false;
      state.updatedAt = new Date().toISOString();
    }
  }

  async listNotifications(options: {
    workspaceId?: string | null;
    severity?: string | null;
    includeDismissed?: boolean;
  }): Promise<NotificationSummary[]> {
    return this.notifications.filter((notification) => {
      if (!options.includeDismissed && notification.dismissed) {
        return false;
      }
      if (options.workspaceId && notification.workspaceId !== options.workspaceId) {
        return false;
      }
      if (options.severity && notification.severity !== options.severity) {
        return false;
      }
      return true;
    });
  }

  async dismissNotification(notificationId: string): Promise<void> {
    const notification = this.notifications.find(
      (candidate) => candidate.notificationId === notificationId
    );
    if (notification) {
      notification.dismissed = true;
    }
  }

  private findWorkspace(workspaceId: string): WorkspaceSummary {
    const workspace = this.workspaces.find((candidate) => candidate.workspaceId === workspaceId);
    if (!workspace) {
      throw new Error(`Workspace '${workspaceId}' was not found.`);
    }
    return workspace;
  }

  private findBrowserSurface(surfaceId: string): SurfaceSummary {
    const surface = this.browserSurfaces.find((candidate) => candidate.surfaceId === surfaceId);
    if (!surface) {
      throw new Error(`Browser surface '${surfaceId}' was not found.`);
    }
    return surface;
  }

  private createPreviewTerminal(
    workspaceId: string,
    backendKind: string,
    title: string,
    output: string
  ): TerminalSession {
    this.findWorkspace(workspaceId);
    const suffix = ++this.terminalCounter;
    const sessionId = `ses_browser_preview_${suffix}`;
    const surfaceId = `surf_browser_preview_terminal_${suffix}`;
    const session: TerminalSession = {
      sessionId,
      backendKind,
      state: "preview"
    };
    this.sessions.set(sessionId, session);
    this.terminalSurfaces.push({
      surfaceId,
      workspaceId,
      surfaceType: "terminal",
      title,
      sessionId,
      browserId: null
    });
    this.outputs.set(sessionId, output);
    this.lastSessionId = sessionId;
    this.mountPreviewSurface(workspaceId, surfaceId);
    return session;
  }

  private mountPreviewSurface(workspaceId: string, surfaceId: string): void {
    const workspace = this.findWorkspace(workspaceId);
    const pane = (this.panes.get(workspaceId) ?? []).find(
      (candidate) => candidate.paneId === workspace.activePaneId
    );
    if (pane) {
      for (const candidate of this.panes.get(workspaceId) ?? []) {
        if (candidate.mountedSurfaceId === surfaceId) {
          candidate.mountedSurfaceId = null;
        }
      }
      pane.mountedSurfaceId = surfaceId;
    }
  }

  private surfaceHasRunningSession(surfaceId: string): boolean {
    const surface = this.terminalSurfaces.find((candidate) => candidate.surfaceId === surfaceId);
    if (!surface?.sessionId) {
      return false;
    }
    const session = this.sessions.get(surface.sessionId);
    return Boolean(session && !["exited", "failed", "lost", "disconnected"].includes(session.state));
  }

  private closeSurface(surfaceId: string): void {
    for (const panes of this.panes.values()) {
      for (const pane of panes) {
        if (pane.mountedSurfaceId === surfaceId) {
          pane.mountedSurfaceId = null;
        }
      }
    }

    const terminalIndex = this.terminalSurfaces.findIndex(
      (surface) => surface.surfaceId === surfaceId
    );
    if (terminalIndex >= 0) {
      const sessionId = this.terminalSurfaces[terminalIndex].sessionId;
      if (sessionId) {
        this.sessions.delete(sessionId);
        this.outputs.delete(sessionId);
        this.agentStates.delete(sessionId);
      }
      this.terminalSurfaces.splice(terminalIndex, 1);
      return;
    }

    const browserIndex = this.browserSurfaces.findIndex(
      (surface) => surface.surfaceId === surfaceId
    );
    if (browserIndex >= 0) {
      this.browserUrls.delete(surfaceId);
      this.browserSurfaces.splice(browserIndex, 1);
    }
  }

  private applySyntheticAgentState(detail: SyntheticAgentStateDetail = {}): void {
    const sessionId = detail.sessionId ?? this.lastSessionId;
    const session = sessionId ? this.sessions.get(sessionId) : undefined;
    const surface = sessionId
      ? this.terminalSurfaces.find((candidate) => candidate.sessionId === sessionId)
      : undefined;
    const workspaceId = detail.workspaceId ?? surface?.workspaceId;
    const workspace = workspaceId ? this.findWorkspace(workspaceId) : this.workspaces[0] ?? null;
    if (!workspace || !session || !sessionId) {
      return;
    }

    const state = detail.state ?? "waiting_for_input";
    const reason = detail.reason ?? "Synthetic attention requested.";
    const now = new Date().toISOString();
    const attention = state === "waiting_for_input" || state === "failed";
    this.agentStates.set(sessionId, {
      sessionId,
      workspaceId: workspace.workspaceId,
      state,
      attention,
      reason,
      updatedAt: now,
      telemetry: detail.telemetry ?? null
    });

    if (!["waiting_for_input", "completed", "failed"].includes(state)) {
      return;
    }

    const notificationType =
      state === "waiting_for_input"
        ? "agent.needs_input"
        : state === "completed"
          ? "agent.completed"
          : "agent.failed";
    const severity = state === "failed" ? "error" : state === "completed" ? "info" : "warning";
    const title =
      state === "waiting_for_input"
        ? "Agent needs input"
        : state === "completed"
          ? "Agent completed"
          : "Agent failed";
    const notificationId = detail.notificationId ?? `not_browser_preview_${this.notifications.length + 1}`;
    if (this.notifications.some((notification) => notification.notificationId === notificationId)) {
      return;
    }
    this.notifications.unshift({
      notificationId,
      notificationType,
      severity,
      workspaceId: workspace.workspaceId,
      sessionId,
      title,
      message: reason,
      createdAt: now,
      dismissed: false
    });
  }
}

interface SyntheticAgentStateDetail {
  workspaceId?: string;
  sessionId?: string;
  state?: string;
  reason?: string;
  notificationId?: string;
  telemetry?: AgentTelemetry;
}

interface BrowserPreviewApi {
  syntheticAgentState(detail?: SyntheticAgentStateDetail): void;
}

interface WorkspaceSummaryWire {
  workspace_id: string;
  name: string;
  root_pane_id: string;
  active_pane_id: string;
  project_root?: string | null;
  environment_profile_id?: string | null;
}

interface WorkspaceDetailWire {
  workspace: WorkspaceSummaryWire;
  panes: PaneSummaryWire[];
  surfaces: SurfaceSummaryWire[];
  sessions: SessionSummaryWire[];
}

interface PaneSummaryWire {
  pane_id: string;
  workspace_id: string;
  parent_pane_id?: string | null;
  kind: string;
  split_axis?: string | null;
  split_ratio?: number | null;
  mounted_surface_id?: string | null;
}

interface SurfaceSummaryWire {
  surface_id: string;
  workspace_id: string;
  surface_type: string;
  title: string;
  session_id?: string | null;
  browser_id?: string | null;
}

interface SessionSummaryWire {
  session_id: string;
  backend_kind: string;
  state: string;
  backend_native_id?: string | null;
}

interface AgentStateWire {
  session_id: string;
  workspace_id: string;
  state: string;
  attention: boolean;
  reason?: string | null;
  updated_at?: string | null;
  telemetry?: AgentTelemetry | null;
}

interface NotificationSummaryWire {
  notification_id: string;
  notification_type: string;
  severity: string;
  workspace_id?: string | null;
  session_id?: string | null;
  title: string;
  message: string;
  created_at: string;
  dismissed: boolean;
}

interface RecoveryDiagnosticsWire {
  workspace_count: number;
  pane_count: number;
  surface_count: number;
  session_count: number;
  sessions: Array<{
    session_id: string;
    workspace_id: string;
    backend_kind: string;
    state: string;
    durability: string;
    backend_native_id?: string | null;
  }>;
}

interface WslDistributionWire {
  name: string;
  is_default: boolean;
}

interface SshProfileWire {
  profile_id: string;
  name: string;
  host: string;
  user: string;
  port?: number | null;
}

interface BrowserNavigationResultWire {
  surface_id: string;
  url: string;
}

interface BrowserScreenshotResultWire {
  surface_id: string;
  format: string;
  image_handle: string;
  byte_count: number;
}

interface BrowserDomSnapshotResultWire {
  surface_id: string;
  html: string;
}

interface BrowserActionResultWire {
  surface_id: string;
  ok: boolean;
}

interface BrowserEvaluateResultWire {
  surface_id: string;
  value_json: string;
}

interface BrowserDiagnosticWire {
  surface_id?: string | null;
  workspace_id?: string | null;
  operation: string;
  code: string;
  message: string;
  occurred_at: string;
}

function mapWorkspace(value: WorkspaceSummaryWire): WorkspaceSummary {
  return {
    workspaceId: value.workspace_id,
    name: value.name,
    rootPaneId: value.root_pane_id,
    activePaneId: value.active_pane_id,
    projectRoot: value.project_root,
    environmentProfileId: value.environment_profile_id
  };
}

function mapPane(value: PaneSummaryWire): PaneSummary {
  return {
    paneId: value.pane_id,
    workspaceId: value.workspace_id,
    parentPaneId: value.parent_pane_id,
    kind: value.kind,
    splitAxis: value.split_axis,
    splitRatio: value.split_ratio,
    mountedSurfaceId: value.mounted_surface_id
  };
}

function mapSurface(value: SurfaceSummaryWire): SurfaceSummary {
  return {
    surfaceId: value.surface_id,
    workspaceId: value.workspace_id,
    surfaceType: value.surface_type,
    title: value.title,
    sessionId: value.session_id,
    browserId: value.browser_id
  };
}

function mapSession(value: SessionSummaryWire): TerminalSession {
  return {
    sessionId: value.session_id,
    backendKind: value.backend_kind,
    state: value.state,
    backendNativeId: value.backend_native_id
  };
}

function mapAgentState(value: AgentStateWire): AgentState {
  return {
    sessionId: value.session_id,
    workspaceId: value.workspace_id,
    state: value.state,
    attention: value.attention,
    reason: value.reason,
    updatedAt: value.updated_at,
    telemetry: value.telemetry ?? null
  };
}

function mapNotification(value: NotificationSummaryWire): NotificationSummary {
  return {
    notificationId: value.notification_id,
    notificationType: value.notification_type,
    severity: value.severity,
    workspaceId: value.workspace_id,
    sessionId: value.session_id,
    title: value.title,
    message: value.message,
    createdAt: value.created_at,
    dismissed: value.dismissed
  };
}

function mapWslDistribution(value: WslDistributionWire): WslDistribution {
  return {
    name: value.name,
    isDefault: value.is_default
  };
}

function mapProfile(value: SshProfileWire): SshProfile {
  return {
    profileId: value.profile_id,
    name: value.name,
    host: value.host,
    user: value.user,
    port: value.port ?? null
  };
}

function mapBrowserNavigation(value: BrowserNavigationResultWire): BrowserNavigationResult {
  return {
    surfaceId: value.surface_id,
    url: value.url
  };
}

function mapBrowserScreenshot(value: BrowserScreenshotResultWire): BrowserScreenshotResult {
  return {
    surfaceId: value.surface_id,
    format: value.format,
    imageHandle: value.image_handle,
    byteCount: value.byte_count
  };
}

function mapBrowserDomSnapshot(value: BrowserDomSnapshotResultWire): BrowserDomSnapshotResult {
  return {
    surfaceId: value.surface_id,
    html: value.html
  };
}

function mapBrowserAction(value: BrowserActionResultWire): BrowserActionResult {
  return {
    surfaceId: value.surface_id,
    ok: value.ok
  };
}

function mapBrowserEvaluate(value: BrowserEvaluateResultWire): BrowserEvaluateResult {
  return {
    surfaceId: value.surface_id,
    valueJson: value.value_json
  };
}

function mapBrowserDiagnostic(value: BrowserDiagnosticWire): BrowserDiagnostic {
  return {
    surfaceId: value.surface_id,
    workspaceId: value.workspace_id,
    operation: value.operation,
    code: value.code,
    message: value.message,
    occurredAt: value.occurred_at
  };
}
