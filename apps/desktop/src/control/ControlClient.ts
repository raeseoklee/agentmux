export interface TerminalSession {
  sessionId: string;
  state: string;
  backendKind: string;
  backendNativeId?: string | null;
}

export class ControlClientError extends Error {
  constructor(
    message: string,
    readonly code?: string | null,
    readonly detailsJson?: string | null,
  ) {
    super(message);
    this.name = "ControlClientError";
  }
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

export interface SidebarStatus {
  workspaceId: string;
  key: string;
  label: string;
  icon?: string | null;
  color?: string | null;
  priority: number;
  updatedAt: string;
}

export interface SidebarProgress {
  workspaceId: string;
  value: number;
  label?: string | null;
  updatedAt: string;
}

export interface SidebarLogEntry {
  logId: string;
  workspaceId: string;
  level: string;
  source?: string | null;
  message: string;
  createdAt: string;
}

export interface SidebarState {
  workspaceId: string;
  cwd?: string | null;
  gitBranch?: string | null;
  gitHash?: string | null;
  ports: string[];
  statuses: SidebarStatus[];
  progress?: SidebarProgress | null;
  logs: SidebarLogEntry[];
}

export interface WorkspaceSummary {
  workspaceId: string;
  name: string;
  rootPaneId: string;
  activePaneId: string;
  projectRoot?: string | null;
  environmentProfileId?: string | null;
  description?: string | null;
  icon?: string | null;
  color?: string | null;
  defaultWslDistribution?: string | null;
  defaultAgentCommand?: string | null;
}

export interface WorkspaceGroupMember {
  workspaceId: string;
  position: number;
}

export interface WorkspaceGroup {
  groupId: string;
  name: string;
  anchorWorkspaceId?: string | null;
  collapsed: boolean;
  pinned: boolean;
  color?: string | null;
  icon?: string | null;
  sortOrder: number;
  createdAt: string;
  updatedAt: string;
  members: WorkspaceGroupMember[];
}

export interface WorkspaceGroupCreateInput {
  name: string;
  anchorWorkspaceId?: string | null;
  workspaceIds?: string[] | null;
  collapsed?: boolean;
  pinned?: boolean;
  color?: string | null;
  icon?: string | null;
}

export interface WorkspaceGroupUpdateInput {
  name?: string;
  anchorWorkspaceId?: string | null;
  collapsed?: boolean;
  pinned?: boolean;
  color?: string | null;
  icon?: string | null;
  sortOrder?: number;
}

export interface WorkspaceUpdateInput {
  name: string;
  projectRoot?: string | null;
  environmentProfileId?: string | null;
  description?: string | null;
  icon?: string | null;
  color?: string | null;
  defaultWslDistribution?: string | null;
  defaultAgentCommand?: string | null;
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

export interface TmuxDiagnostics {
  available: boolean;
  distribution?: string | null;
  version?: string | null;
  message: string;
}

export interface AppConfigAppearance {
  theme: "dark" | "light";
  accentKey: string;
  fontSize: number;
}

export type ShortcutBindingValue = string | [string, string] | null;

export interface AppConfigShortcuts {
  bindings: Record<string, ShortcutBindingValue>;
}

export type CustomActionTarget = "agent" | "wsl-terminal" | "browser";

export interface AppConfigCustomAction {
  id: string;
  title: string;
  group?: string | null;
  target: CustomActionTarget;
  command: string[];
  keywords: string[];
}

export interface AppConfigActions {
  custom: AppConfigCustomAction[];
}

export interface AppConfigUi {
  workspacePlusAction?: string | null;
  surfaceTabPlusAction?: string | null;
  surfaceTabActions?: string[] | null;
  textBoxMaxLines?: number | null;
  terminalInnerMargin?: number | null;
}

export interface AppConfigNotificationAction {
  action: string;
  label?: string | null;
  notificationType?: string | null;
  severity?: string | null;
  dismissOnRun?: boolean | null;
}

export interface AppConfigNotifications {
  actions: AppConfigNotificationAction[];
}

export interface AppConfig {
  formatVersion: string;
  configPath: string;
  projectConfigPath?: string | null;
  projectConfigLoaded: boolean;
  appearance: AppConfigAppearance;
  shortcuts: AppConfigShortcuts;
  actions: AppConfigActions;
  ui: AppConfigUi;
  notifications: AppConfigNotifications;
}

export type AppConfigScope = "global" | "project";

export interface AppConfigExport {
  json: string;
  config: AppConfig;
}

export interface AppConfigMigration {
  sourcePath: string;
  targetPath: string;
  overwritten: boolean;
  config: AppConfig;
}

export interface AppConfigDiagnosticEntry {
  source: string;
  path?: string | null;
  exists: boolean;
  valid: boolean;
  active: boolean;
  message: string;
}

export interface DockControl {
  id: string;
  title: string;
  command: string;
  cwd?: string | null;
  height?: number | null;
  env: Record<string, string>;
}

export interface DockConfig {
  source: string;
  configPath?: string | null;
  requiresTrust: boolean;
  trusted: boolean;
  controls: DockControl[];
}

export interface AppConfigUpdate {
  appearance?: Partial<AppConfigAppearance>;
  shortcuts?: {
    bindings?: Record<string, ShortcutBindingValue>;
  };
  ui?: Partial<AppConfigUi>;
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

export interface BrowserGetResult {
  surfaceId: string;
  selector: string;
  kind: string;
  value: string;
}

export interface BrowserFindResult {
  surfaceId: string;
  query: string;
  count: number;
  matches: string[];
}

export interface BrowserWaitForSelectorResult {
  surfaceId: string;
  selector: string;
  elapsedMs: number;
}

export interface BrowserEvaluateResult {
  surfaceId: string;
  valueJson: string;
}

export type TerminalPlacement = "new_tab" | "active_pane";
type SessionPlacement = TerminalPlacement | "dock";

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
  frameId?: string | null;
}

export interface ControlClient {
  listWorkspaces(): Promise<WorkspaceSummary[]>;
  createWorkspace(
    name: string,
    projectRoot?: string | null,
  ): Promise<WorkspaceSummary>;
  getWorkspace(workspaceId: string): Promise<WorkspaceDetail>;
  renameWorkspace(workspaceId: string, name: string): Promise<WorkspaceSummary>;
  updateWorkspace(
    workspaceId: string,
    input: WorkspaceUpdateInput,
  ): Promise<WorkspaceSummary>;
  closeWorkspace(workspaceId: string, closePolicy: string): Promise<boolean>;
  listWorkspaceGroups(): Promise<WorkspaceGroup[]>;
  createWorkspaceGroup(
    input: WorkspaceGroupCreateInput,
  ): Promise<WorkspaceGroup>;
  updateWorkspaceGroup(
    groupId: string,
    input: WorkspaceGroupUpdateInput,
  ): Promise<WorkspaceGroup>;
  deleteWorkspaceGroup(groupId: string): Promise<void>;
  addWorkspaceToGroup(
    groupId: string,
    workspaceId: string,
    position?: number | null,
  ): Promise<WorkspaceGroup>;
  removeWorkspaceFromGroup(
    groupId: string,
    workspaceId: string,
  ): Promise<WorkspaceGroup>;
  splitPane(
    workspaceId: string,
    paneId: string,
    axis: "horizontal" | "vertical",
    ratio?: number,
  ): Promise<WorkspaceDetail>;
  focusPane(workspaceId: string, paneId: string): Promise<WorkspaceDetail>;
  closePane(
    workspaceId: string,
    paneId: string,
    surfacePolicy: string,
  ): Promise<WorkspaceDetail>;
  resizePaneLayout(
    workspaceId: string,
    paneId: string,
    ratio: number,
  ): Promise<WorkspaceDetail>;
  mountSurface(
    workspaceId: string,
    paneId: string,
    surfaceId: string,
  ): Promise<WorkspaceDetail>;
  unmountSurface(workspaceId: string, paneId: string): Promise<WorkspaceDetail>;
  createBrowserSurface(
    workspaceId: string,
    paneId?: string | null,
    profile?: string | null,
    placement?: TerminalPlacement,
  ): Promise<SurfaceSummary>;
  closeSurface(
    workspaceId: string,
    surfaceId: string,
  ): Promise<WorkspaceDetail>;
  browserNavigate(
    surfaceId: string,
    url: string,
  ): Promise<BrowserNavigationResult>;
  browserReload(surfaceId: string): Promise<BrowserNavigationResult>;
  browserBack(surfaceId: string): Promise<BrowserNavigationResult>;
  browserForward(surfaceId: string): Promise<BrowserNavigationResult>;
  browserCurrentUrl(surfaceId: string): Promise<BrowserNavigationResult>;
  browserScreenshot(
    surfaceId: string,
    format?: string | null,
  ): Promise<BrowserScreenshotResult>;
  browserDomSnapshot(
    surfaceId: string,
    options?: { frameId?: string | null },
  ): Promise<BrowserDomSnapshotResult>;
  browserClick(
    surfaceId: string,
    target: BrowserClickTarget,
  ): Promise<BrowserActionResult>;
  browserType(
    surfaceId: string,
    selector: string,
    text: string,
    options?: { frameId?: string | null },
  ): Promise<BrowserActionResult>;
  browserFill(
    surfaceId: string,
    selector: string,
    text: string,
    options?: { frameId?: string | null },
  ): Promise<BrowserActionResult>;
  browserPress(
    surfaceId: string,
    selector: string,
    key: string,
    options?: { frameId?: string | null },
  ): Promise<BrowserActionResult>;
  browserSelect(
    surfaceId: string,
    selector: string,
    values: string[],
    options?: { frameId?: string | null },
  ): Promise<BrowserActionResult>;
  browserScroll(
    surfaceId: string,
    options: {
      selector?: string | null;
      x?: number | null;
      y?: number | null;
      frameId?: string | null;
    },
  ): Promise<BrowserActionResult>;
  browserHover(
    surfaceId: string,
    selector: string,
    options?: { frameId?: string | null },
  ): Promise<BrowserActionResult>;
  browserCheck(
    surfaceId: string,
    selector: string,
    checked?: boolean | null,
    options?: { frameId?: string | null },
  ): Promise<BrowserActionResult>;
  browserGet(
    surfaceId: string,
    selector: string,
    options?: {
      kind?: string | null;
      attribute?: string | null;
      frameId?: string | null;
    },
  ): Promise<BrowserGetResult>;
  browserFind(
    surfaceId: string,
    query: string,
    options?: {
      selector?: string | null;
      limit?: number | null;
      frameId?: string | null;
    },
  ): Promise<BrowserFindResult>;
  browserHighlight(
    surfaceId: string,
    selector: string,
    durationMs?: number | null,
    options?: { frameId?: string | null },
  ): Promise<BrowserActionResult>;
  browserFocus(
    surfaceId: string,
    selector: string,
    options?: { frameId?: string | null },
  ): Promise<BrowserActionResult>;
  browserZoom(surfaceId: string, percent: number): Promise<BrowserActionResult>;
  browserWaitForSelector(
    surfaceId: string,
    selector: string,
    timeoutMs?: number | null,
    options?: { frameId?: string | null },
  ): Promise<BrowserWaitForSelectorResult>;
  browserEvaluate(
    surfaceId: string,
    script: string,
    options?: { frameId?: string | null },
  ): Promise<BrowserEvaluateResult>;
  browserDiagnostics(options?: {
    workspaceId?: string | null;
    surfaceId?: string | null;
  }): Promise<BrowserDiagnostic[]>;
  recoveryDiagnostics(): Promise<RecoveryDiagnostics>;
  listWslDistributions(): Promise<WslDistribution[]>;
  checkTmux(distribution?: string | null): Promise<TmuxDiagnostics>;
  getConfig(workspaceId?: string | null): Promise<AppConfig>;
  reloadConfig(workspaceId?: string | null): Promise<AppConfig>;
  updateConfig(
    update: AppConfigUpdate,
    workspaceId?: string | null,
  ): Promise<AppConfig>;
  exportConfig(options?: {
    workspaceId?: string | null;
    scope?: AppConfigScope;
  }): Promise<AppConfigExport>;
  importConfig(
    json: string,
    options?: { workspaceId?: string | null; scope?: AppConfigScope },
  ): Promise<AppConfig>;
  resetConfig(options?: {
    workspaceId?: string | null;
    scope?: AppConfigScope;
  }): Promise<AppConfig>;
  migrateProjectConfig(options?: {
    workspaceId?: string | null;
    overwrite?: boolean;
  }): Promise<AppConfigMigration>;
  configDiagnostics(
    workspaceId?: string | null,
  ): Promise<AppConfigDiagnosticEntry[]>;
  getDock(workspaceId?: string | null): Promise<DockConfig>;
  trustDock(workspaceId: string): Promise<DockConfig>;
  listProfiles(): Promise<SshProfile[]>;
  createProfile(input: SshProfileInput): Promise<SshProfile>;
  updateProfile(profileId: string, input: SshProfileInput): Promise<SshProfile>;
  deleteProfile(profileId: string): Promise<void>;
  spawnNativeTerminal(
    workspaceId: string,
    command: string[],
    placement?: TerminalPlacement,
    paneId?: string | null,
  ): Promise<TerminalSession>;
  spawnWslTerminal(
    workspaceId: string,
    distribution: string | null,
    cwd: string | null,
    placement?: TerminalPlacement,
    paneId?: string | null,
  ): Promise<TerminalSession>;
  spawnDurableWslTerminal(
    workspaceId: string,
    distribution: string | null,
    cwd: string | null,
    placement?: TerminalPlacement,
    paneId?: string | null,
  ): Promise<TerminalSession>;
  spawnDockTerminal(
    workspaceId: string,
    control: DockControl,
    distribution: string | null,
    cwd: string | null,
    placement?: SessionPlacement,
  ): Promise<TerminalSession>;
  spawnSshTerminal(
    workspaceId: string,
    target: string,
    placement?: TerminalPlacement,
    paneId?: string | null,
  ): Promise<TerminalSession>;
  spawnAgentTerminal(
    workspaceId: string,
    command: string[],
    distribution: string | null,
    placement?: TerminalPlacement,
    paneId?: string | null,
  ): Promise<TerminalSession>;
  getSession(sessionId: string): Promise<TerminalSession>;
  readRecent(sessionId: string, maxBytes: number): Promise<string>;
  /**
   * Optional live-output stream (real Tauri host only). When present, the
   * renderer cold-starts from `snapshot`, then writes raw bytes pushed through
   * `subscribeOutput` instead of polling `readRecent`. Absent on the preview and
   * server clients, where the renderer falls back to polling.
   */
  snapshot?(sessionId: string, sinceOffset?: number): Promise<OutputSnapshot>;
  subscribeOutput?(
    sessionId: string,
    onFrame: (fromOffset: number, bytes: Uint8Array) => void,
  ): Promise<() => void>;
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
  getSidebarState(workspaceId?: string | null): Promise<SidebarState>;
}

interface TauriChannel<T> {
  onmessage: ((message: T) => void) | null;
}

interface TauriChannelConstructor {
  new <T>(): TauriChannel<T>;
}

interface TauriCore {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  // Present under withGlobalTauri; used for the live terminal-output stream.
  Channel?: TauriChannelConstructor;
}

/** A cold-start snapshot of a session's recent output for a stream-first renderer. */
export interface OutputSnapshot {
  baseOffset: number;
  endOffset: number;
  bytes: Uint8Array;
}

const EMPTY_BYTES = new Uint8Array(0);

function base64ToBytes(base64: string): Uint8Array {
  // PR-6: an empty (or absent) payload is the steady-state no-output snapshot;
  // skip decoding entirely. Otherwise decode natively via atob + a single
  // Uint8Array.from pass instead of a per-character assignment loop.
  if (!base64) {
    return EMPTY_BYTES;
  }
  const binary = atob(base64);
  return Uint8Array.from(binary, (char) => char.charCodeAt(0));
}

interface AgentmuxServerBootstrap {
  baseUrl?: string;
  mode?: string;
  defaults?: {
    workspace_id?: string | null;
    backend?: string | null;
    backend_profile?: string | null;
    cwd?: string | null;
    command?: string[];
    command_line?: string | null;
    columns?: number | null;
    rows?: number | null;
    max_recent_bytes?: number | null;
  };
}

declare global {
  interface Window {
    __TAURI__?: {
      core?: TauriCore;
    };
    __AGENTMUX_SERVER__?: AgentmuxServerBootstrap;
    __AGENTMUX_PREVIEW__?: BrowserPreviewApi;
    __AGENTMUX_PREVIEW_READY__?: boolean;
    __AGENTMUX_PREVIEW_WSL_DISTRIBUTIONS__?: WslDistribution[];
    __AGENTMUX_PREVIEW_TMUX_AVAILABLE__?: boolean;
  }
}

export function createControlClient(): ControlClient {
  const invoke = window.__TAURI__?.core?.invoke;
  if (invoke) {
    return new TauriControlClient(invoke);
  }
  if (window.__AGENTMUX_SERVER__) {
    return new ServerControlClient(window.__AGENTMUX_SERVER__);
  }

  return new BrowserPreviewControlClient();
}

class TauriControlClient implements ControlClient {
  private requestCounter = 0;
  private controlToken?: Promise<string>;

  constructor(private readonly invoke: TauriCore["invoke"]) {}

  async listWorkspaces(): Promise<WorkspaceSummary[]> {
    const result = await this.call<{ workspaces: WorkspaceSummaryWire[] }>(
      "workspace.list",
      {},
    );
    return result.workspaces.map(mapWorkspace);
  }

  async createWorkspace(
    name: string,
    projectRoot?: string | null,
  ): Promise<WorkspaceSummary> {
    const result = await this.call<WorkspaceSummaryWire>("workspace.create", {
      name,
      project_root: projectRoot ?? null,
      backend_profile: null,
    });
    return mapWorkspace(result);
  }

  async getWorkspace(workspaceId: string): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>("workspace.get", {
      workspace_id: workspaceId,
    });
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession),
    };
  }

  async renameWorkspace(
    workspaceId: string,
    name: string,
  ): Promise<WorkspaceSummary> {
    const result = await this.call<WorkspaceSummaryWire>("workspace.rename", {
      workspace_id: workspaceId,
      name,
    });
    return mapWorkspace(result);
  }

  async updateWorkspace(
    workspaceId: string,
    input: WorkspaceUpdateInput,
  ): Promise<WorkspaceSummary> {
    const result = await this.call<WorkspaceSummaryWire>("workspace.update", {
      workspace_id: workspaceId,
      name: input.name,
      project_root: input.projectRoot ?? null,
      environment_profile_id: input.environmentProfileId ?? null,
      description: input.description ?? null,
      icon: input.icon ?? null,
      color: input.color ?? null,
      default_wsl_distribution: input.defaultWslDistribution ?? null,
      default_agent_command: input.defaultAgentCommand ?? null,
    });
    return mapWorkspace(result);
  }

  async closeWorkspace(
    workspaceId: string,
    closePolicy: string,
  ): Promise<boolean> {
    const result = await this.call<{ closed: boolean }>("workspace.close", {
      workspace_id: workspaceId,
      close_policy: closePolicy,
    });
    return result.closed;
  }

  async listWorkspaceGroups(): Promise<WorkspaceGroup[]> {
    const result = await this.call<{ groups: WorkspaceGroupWire[] }>(
      "workspace_group.list",
      {},
    );
    return result.groups.map(mapWorkspaceGroup);
  }

  async createWorkspaceGroup(
    input: WorkspaceGroupCreateInput,
  ): Promise<WorkspaceGroup> {
    const result = await this.call<WorkspaceGroupWire>(
      "workspace_group.create",
      {
        name: input.name,
        anchor_workspace_id: input.anchorWorkspaceId ?? null,
        workspace_ids: input.workspaceIds ?? null,
        collapsed: input.collapsed ?? null,
        pinned: input.pinned ?? null,
        color: input.color ?? null,
        icon: input.icon ?? null,
      },
    );
    return mapWorkspaceGroup(result);
  }

  async updateWorkspaceGroup(
    groupId: string,
    input: WorkspaceGroupUpdateInput,
  ): Promise<WorkspaceGroup> {
    const result = await this.call<WorkspaceGroupWire>(
      "workspace_group.update",
      {
        group_id: groupId,
        name: input.name,
        anchor_workspace_id: input.anchorWorkspaceId,
        collapsed: input.collapsed,
        pinned: input.pinned,
        color: input.color,
        icon: input.icon,
        sort_order: input.sortOrder,
      },
    );
    return mapWorkspaceGroup(result);
  }

  async deleteWorkspaceGroup(groupId: string): Promise<void> {
    await this.call("workspace_group.delete", {
      group_id: groupId,
    });
  }

  async addWorkspaceToGroup(
    groupId: string,
    workspaceId: string,
    position?: number | null,
  ): Promise<WorkspaceGroup> {
    const result = await this.call<WorkspaceGroupWire>(
      "workspace_group.add_workspace",
      {
        group_id: groupId,
        workspace_id: workspaceId,
        position: position ?? null,
      },
    );
    return mapWorkspaceGroup(result);
  }

  async removeWorkspaceFromGroup(
    groupId: string,
    workspaceId: string,
  ): Promise<WorkspaceGroup> {
    const result = await this.call<WorkspaceGroupWire>(
      "workspace_group.remove_workspace",
      {
        group_id: groupId,
        workspace_id: workspaceId,
        position: null,
      },
    );
    return mapWorkspaceGroup(result);
  }

  async splitPane(
    workspaceId: string,
    paneId: string,
    axis: "horizontal" | "vertical",
    ratio = 0.5,
  ): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>("pane.split", {
      workspace_id: workspaceId,
      pane_id: paneId,
      axis,
      ratio,
    });
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession),
    };
  }

  async focusPane(
    workspaceId: string,
    paneId: string,
  ): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>("pane.focus", {
      workspace_id: workspaceId,
      pane_id: paneId,
    });
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession),
    };
  }

  async closePane(
    workspaceId: string,
    paneId: string,
    surfacePolicy: string,
  ): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>("pane.close", {
      workspace_id: workspaceId,
      pane_id: paneId,
      surface_policy: surfacePolicy,
    });
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession),
    };
  }

  async resizePaneLayout(
    workspaceId: string,
    paneId: string,
    ratio: number,
  ): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>("pane.resize_layout", {
      workspace_id: workspaceId,
      pane_id: paneId,
      ratio,
    });
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession),
    };
  }

  async mountSurface(
    workspaceId: string,
    paneId: string,
    surfaceId: string,
  ): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>("pane.mount_surface", {
      workspace_id: workspaceId,
      pane_id: paneId,
      surface_id: surfaceId,
    });
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession),
    };
  }

  async unmountSurface(
    workspaceId: string,
    paneId: string,
  ): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>(
      "pane.unmount_surface",
      {
        workspace_id: workspaceId,
        pane_id: paneId,
      },
    );
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession),
    };
  }

  async createBrowserSurface(
    workspaceId: string,
    paneId?: string | null,
    profile?: string | null,
    placement?: TerminalPlacement,
  ): Promise<SurfaceSummary> {
    const result = await this.call<SurfaceSummaryWire>(
      "surface.create_browser",
      {
        workspace_id: workspaceId,
        pane_id: paneId ?? null,
        profile: profile ?? null,
        placement: placement ?? null,
      },
    );
    return mapSurface(result);
  }

  async closeSurface(
    workspaceId: string,
    surfaceId: string,
  ): Promise<WorkspaceDetail> {
    const result = await this.call<WorkspaceDetailWire>("surface.close", {
      workspace_id: workspaceId,
      surface_id: surfaceId,
    });
    return {
      workspace: mapWorkspace(result.workspace),
      panes: result.panes.map(mapPane),
      surfaces: result.surfaces.map(mapSurface),
      sessions: result.sessions.map(mapSession),
    };
  }

  async browserNavigate(
    surfaceId: string,
    url: string,
  ): Promise<BrowserNavigationResult> {
    const result = await this.call<BrowserNavigationResultWire>(
      "browser.navigate",
      {
        surface_id: surfaceId,
        url,
      },
    );
    return mapBrowserNavigation(result);
  }

  async browserReload(surfaceId: string): Promise<BrowserNavigationResult> {
    const result = await this.call<BrowserNavigationResultWire>(
      "browser.reload",
      {
        surface_id: surfaceId,
      },
    );
    return mapBrowserNavigation(result);
  }

  async browserBack(surfaceId: string): Promise<BrowserNavigationResult> {
    const result = await this.call<BrowserNavigationResultWire>(
      "browser.back",
      {
        surface_id: surfaceId,
      },
    );
    return mapBrowserNavigation(result);
  }

  async browserForward(surfaceId: string): Promise<BrowserNavigationResult> {
    const result = await this.call<BrowserNavigationResultWire>(
      "browser.forward",
      {
        surface_id: surfaceId,
      },
    );
    return mapBrowserNavigation(result);
  }

  async browserCurrentUrl(surfaceId: string): Promise<BrowserNavigationResult> {
    const result = await this.call<BrowserNavigationResultWire>(
      "browser.current_url",
      {
        surface_id: surfaceId,
      },
    );
    return mapBrowserNavigation(result);
  }

  async browserScreenshot(
    surfaceId: string,
    format?: string | null,
  ): Promise<BrowserScreenshotResult> {
    const result = await this.call<BrowserScreenshotResultWire>(
      "browser.screenshot",
      {
        surface_id: surfaceId,
        format: format ?? null,
      },
    );
    return mapBrowserScreenshot(result);
  }

  async browserDomSnapshot(
    surfaceId: string,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserDomSnapshotResult> {
    const result = await this.call<BrowserDomSnapshotResultWire>(
      "browser.dom_snapshot",
      {
        surface_id: surfaceId,
        frame_id: options.frameId ?? null,
      },
    );
    return mapBrowserDomSnapshot(result);
  }

  async browserClick(
    surfaceId: string,
    target: BrowserClickTarget,
  ): Promise<BrowserActionResult> {
    const result = await this.call<BrowserActionResultWire>("browser.click", {
      surface_id: surfaceId,
      selector: target.selector ?? null,
      x: target.x ?? null,
      y: target.y ?? null,
      frame_id: target.frameId ?? null,
    });
    return mapBrowserAction(result);
  }

  async browserType(
    surfaceId: string,
    selector: string,
    text: string,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    const result = await this.call<BrowserActionResultWire>("browser.type", {
      surface_id: surfaceId,
      selector,
      text,
      frame_id: options.frameId ?? null,
    });
    return mapBrowserAction(result);
  }

  async browserFill(
    surfaceId: string,
    selector: string,
    text: string,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    const result = await this.call<BrowserActionResultWire>("browser.fill", {
      surface_id: surfaceId,
      selector,
      text,
      frame_id: options.frameId ?? null,
    });
    return mapBrowserAction(result);
  }

  async browserPress(
    surfaceId: string,
    selector: string,
    key: string,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    const result = await this.call<BrowserActionResultWire>("browser.press", {
      surface_id: surfaceId,
      selector,
      key,
      frame_id: options.frameId ?? null,
    });
    return mapBrowserAction(result);
  }

  async browserSelect(
    surfaceId: string,
    selector: string,
    values: string[],
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    const result = await this.call<BrowserActionResultWire>("browser.select", {
      surface_id: surfaceId,
      selector,
      values,
      frame_id: options.frameId ?? null,
    });
    return mapBrowserAction(result);
  }

  async browserScroll(
    surfaceId: string,
    options: {
      selector?: string | null;
      x?: number | null;
      y?: number | null;
      frameId?: string | null;
    },
  ): Promise<BrowserActionResult> {
    const result = await this.call<BrowserActionResultWire>("browser.scroll", {
      surface_id: surfaceId,
      selector: options.selector ?? null,
      x: options.x ?? null,
      y: options.y ?? null,
      frame_id: options.frameId ?? null,
    });
    return mapBrowserAction(result);
  }

  async browserHover(
    surfaceId: string,
    selector: string,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    const result = await this.call<BrowserActionResultWire>("browser.hover", {
      surface_id: surfaceId,
      selector,
      frame_id: options.frameId ?? null,
    });
    return mapBrowserAction(result);
  }

  async browserCheck(
    surfaceId: string,
    selector: string,
    checked?: boolean | null,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    const result = await this.call<BrowserActionResultWire>("browser.check", {
      surface_id: surfaceId,
      selector,
      checked: checked ?? null,
      frame_id: options.frameId ?? null,
    });
    return mapBrowserAction(result);
  }

  async browserGet(
    surfaceId: string,
    selector: string,
    options: {
      kind?: string | null;
      attribute?: string | null;
      frameId?: string | null;
    } = {},
  ): Promise<BrowserGetResult> {
    const result = await this.call<BrowserGetResultWire>("browser.get", {
      surface_id: surfaceId,
      selector,
      kind: options.kind ?? null,
      attribute: options.attribute ?? null,
      frame_id: options.frameId ?? null,
    });
    return mapBrowserGet(result);
  }

  async browserFind(
    surfaceId: string,
    query: string,
    options: {
      selector?: string | null;
      limit?: number | null;
      frameId?: string | null;
    } = {},
  ): Promise<BrowserFindResult> {
    const result = await this.call<BrowserFindResultWire>("browser.find", {
      surface_id: surfaceId,
      query,
      selector: options.selector ?? null,
      limit: options.limit ?? null,
      frame_id: options.frameId ?? null,
    });
    return mapBrowserFind(result);
  }

  async browserHighlight(
    surfaceId: string,
    selector: string,
    durationMs?: number | null,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    const result = await this.call<BrowserActionResultWire>(
      "browser.highlight",
      {
        surface_id: surfaceId,
        selector,
        duration_ms: durationMs ?? null,
        frame_id: options.frameId ?? null,
      },
    );
    return mapBrowserAction(result);
  }

  async browserFocus(
    surfaceId: string,
    selector: string,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    const result = await this.call<BrowserActionResultWire>("browser.focus", {
      surface_id: surfaceId,
      selector,
      frame_id: options.frameId ?? null,
    });
    return mapBrowserAction(result);
  }

  async browserZoom(
    surfaceId: string,
    percent: number,
  ): Promise<BrowserActionResult> {
    const result = await this.call<BrowserActionResultWire>("browser.zoom", {
      surface_id: surfaceId,
      percent,
    });
    return mapBrowserAction(result);
  }

  async browserWaitForSelector(
    surfaceId: string,
    selector: string,
    timeoutMs?: number | null,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserWaitForSelectorResult> {
    const result = await this.call<BrowserWaitForSelectorResultWire>(
      "browser.wait_for_selector",
      {
        surface_id: surfaceId,
        selector,
        timeout_ms: timeoutMs ?? null,
        frame_id: options.frameId ?? null,
      },
    );
    return mapBrowserWaitForSelector(result);
  }

  async browserEvaluate(
    surfaceId: string,
    script: string,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserEvaluateResult> {
    const result = await this.call<BrowserEvaluateResultWire>(
      "browser.evaluate",
      {
        surface_id: surfaceId,
        script,
        frame_id: options.frameId ?? null,
      },
    );
    return mapBrowserEvaluate(result);
  }

  async browserDiagnostics(options?: {
    workspaceId?: string | null;
    surfaceId?: string | null;
  }): Promise<BrowserDiagnostic[]> {
    const result = await this.call<{ failures: BrowserDiagnosticWire[] }>(
      "diagnostics.browser",
      {
        workspace_id: options?.workspaceId ?? null,
        surface_id: options?.surfaceId ?? null,
      },
    );
    return result.failures.map(mapBrowserDiagnostic);
  }

  async recoveryDiagnostics(): Promise<RecoveryDiagnostics> {
    const result = await this.call<RecoveryDiagnosticsWire>(
      "diagnostics.recovery",
      {},
    );
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
        backendNativeId: session.backend_native_id,
      })),
    };
  }

  async listWslDistributions(): Promise<WslDistribution[]> {
    const result = await this.call<{ distributions: WslDistributionWire[] }>(
      "diagnostics.wsl_distributions",
      {},
    );
    return result.distributions.map(mapWslDistribution);
  }

  async checkTmux(distribution?: string | null): Promise<TmuxDiagnostics> {
    const result = await this.call<TmuxDiagnosticsWire>("diagnostics.tmux", {
      distribution: distribution ?? null,
    });
    return mapTmuxDiagnostics(result);
  }

  async getConfig(workspaceId?: string | null): Promise<AppConfig> {
    const result = await this.call<AppConfigWire>("config.get", {
      workspace_id: workspaceId ?? null,
    });
    return mapAppConfig(result);
  }

  async reloadConfig(workspaceId?: string | null): Promise<AppConfig> {
    const result = await this.call<AppConfigWire>("config.reload", {
      workspace_id: workspaceId ?? null,
    });
    return mapAppConfig(result);
  }

  async updateConfig(
    update: AppConfigUpdate,
    workspaceId?: string | null,
  ): Promise<AppConfig> {
    const result = await this.call<AppConfigWire>("config.update", {
      workspace_id: workspaceId ?? null,
      appearance: update.appearance
        ? {
            theme: update.appearance.theme,
            accent_key: update.appearance.accentKey,
            font_size: update.appearance.fontSize,
          }
        : undefined,
      shortcuts: update.shortcuts
        ? {
            bindings: update.shortcuts.bindings,
          }
        : undefined,
      ui: update.ui
        ? {
            workspace_plus_action: update.ui.workspacePlusAction,
            surface_tab_plus_action: update.ui.surfaceTabPlusAction,
            surface_tab_actions: update.ui.surfaceTabActions,
            text_box_max_lines: update.ui.textBoxMaxLines,
            terminal_inner_margin: update.ui.terminalInnerMargin,
          }
        : undefined,
    });
    return mapAppConfig(result);
  }

  async exportConfig(
    options: { workspaceId?: string | null; scope?: AppConfigScope } = {},
  ): Promise<AppConfigExport> {
    const result = await this.call<AppConfigExportWire>("config.export", {
      workspace_id: options.workspaceId ?? null,
      scope: options.scope ?? "global",
    });
    return {
      json: result.json,
      config: mapAppConfig(result.config),
    };
  }

  async importConfig(
    json: string,
    options: { workspaceId?: string | null; scope?: AppConfigScope } = {},
  ): Promise<AppConfig> {
    const result = await this.call<AppConfigWire>("config.import", {
      workspace_id: options.workspaceId ?? null,
      scope: options.scope ?? "global",
      json,
    });
    return mapAppConfig(result);
  }

  async resetConfig(
    options: { workspaceId?: string | null; scope?: AppConfigScope } = {},
  ): Promise<AppConfig> {
    const result = await this.call<AppConfigWire>("config.reset", {
      workspace_id: options.workspaceId ?? null,
      scope: options.scope ?? "global",
    });
    return mapAppConfig(result);
  }

  async migrateProjectConfig(
    options: { workspaceId?: string | null; overwrite?: boolean } = {},
  ): Promise<AppConfigMigration> {
    const result = await this.call<AppConfigMigrationWire>(
      "config.migrate_project",
      {
        workspace_id: options.workspaceId ?? null,
        overwrite: options.overwrite ?? false,
      },
    );
    return mapAppConfigMigration(result);
  }

  async configDiagnostics(
    workspaceId?: string | null,
  ): Promise<AppConfigDiagnosticEntry[]> {
    const result = await this.call<AppConfigDiagnosticsWire>(
      "config.diagnostics",
      {
        workspace_id: workspaceId ?? null,
      },
    );
    return result.entries.map(mapAppConfigDiagnosticEntry);
  }

  async getDock(workspaceId?: string | null): Promise<DockConfig> {
    const result = await this.call<DockConfigWire>("dock.get", {
      workspace_id: workspaceId ?? null,
    });
    return mapDockConfig(result);
  }

  async trustDock(workspaceId: string): Promise<DockConfig> {
    const result = await this.call<DockConfigWire>("dock.trust", {
      workspace_id: workspaceId,
    });
    return mapDockConfig(result);
  }

  async listProfiles(): Promise<SshProfile[]> {
    const result = await this.call<{ profiles: SshProfileWire[] }>(
      "profile.list",
      {},
    );
    return result.profiles.map(mapProfile);
  }

  async createProfile(input: SshProfileInput): Promise<SshProfile> {
    const result = await this.call<SshProfileWire>("profile.create", {
      name: input.name,
      host: input.host,
      user: input.user,
      port: input.port ?? null,
    });
    return mapProfile(result);
  }

  async updateProfile(
    profileId: string,
    input: SshProfileInput,
  ): Promise<SshProfile> {
    const result = await this.call<SshProfileWire>("profile.update", {
      profile_id: profileId,
      name: input.name,
      host: input.host,
      user: input.user,
      port: input.port ?? null,
    });
    return mapProfile(result);
  }

  async deleteProfile(profileId: string): Promise<void> {
    await this.call("profile.delete", { profile_id: profileId });
  }

  async spawnNativeTerminal(
    workspaceId: string,
    command: string[],
    placement?: TerminalPlacement,
    paneId?: string | null,
  ): Promise<TerminalSession> {
    const result = await this.call<{ session_id: string }>("session.spawn", {
      workspace_id: workspaceId,
      backend: "conpty",
      command,
      cwd: null,
      columns: 120,
      rows: 30,
      durability: "ephemeral",
      placement: placement ?? null,
      pane_id: paneId ?? null,
    });

    return {
      sessionId: result.session_id,
      backendKind: "conpty",
      state: "running",
    };
  }

  async spawnWslTerminal(
    workspaceId: string,
    distribution: string | null,
    cwd: string | null,
    placement?: TerminalPlacement,
    paneId?: string | null,
  ): Promise<TerminalSession> {
    const result = await this.call<{ session_id: string }>("session.spawn", {
      workspace_id: workspaceId,
      backend: "wsl-direct",
      backend_profile: distribution,
      // Launch the user's login shell (e.g. zsh) so their configured prompt and
      // theme (powerlevel10k, oh-my-zsh, ...) load, instead of forcing bash.
      // Falls back to $SHELL, then /bin/bash, when the passwd lookup is empty.
      command: [
        "sh",
        "-c",
        'login_shell="$(getent passwd "$(id -un)" 2>/dev/null | cut -d: -f7)"; exec "${login_shell:-${SHELL:-/bin/bash}}" -l',
      ],
      cwd,
      columns: 120,
      rows: 30,
      durability: "ephemeral",
      placement: placement ?? null,
      pane_id: paneId ?? null,
    });

    return {
      sessionId: result.session_id,
      backendKind: "wsl-direct",
      state: "running",
    };
  }

  // Same login-shell as spawnWslTerminal, but on the durable WSL-tmux backend so
  // the session survives app restarts (and is reattached, not respawned).
  async spawnDurableWslTerminal(
    workspaceId: string,
    distribution: string | null,
    cwd: string | null,
    placement?: TerminalPlacement,
    paneId?: string | null,
  ): Promise<TerminalSession> {
    const result = await this.call<{ session_id: string }>("session.spawn", {
      workspace_id: workspaceId,
      backend: "wsl-tmux-control",
      backend_profile: distribution,
      // Non-nested command substitution: the tmux backend runs this through an
      // extra `/bin/sh -c "sh -c '…'"` layer, where the original nested
      // "$(getent passwd "$(id -un)" …)" was fragile and the pane died on
      // launch (server "exited unexpectedly"). Splitting into separate
      // assignments keeps the login-shell detection but survives the re-quoting.
      command: [
        "sh",
        "-c",
        'u=$(id -un); s=$(getent passwd "$u" 2>/dev/null | cut -d: -f7); exec "${s:-${SHELL:-/bin/bash}}" -l',
      ],
      cwd,
      columns: 120,
      rows: 30,
      durability: "durable",
      placement: placement ?? null,
      pane_id: paneId ?? null,
    });

    return {
      sessionId: result.session_id,
      backendKind: "wsl-tmux-control",
      state: "running",
    };
  }

  async spawnDockTerminal(
    workspaceId: string,
    control: DockControl,
    distribution: string | null,
    cwd: string | null,
    placement: SessionPlacement = "dock",
  ): Promise<TerminalSession> {
    const rows = Math.min(80, Math.max(6, Math.round(control.height ?? 30)));
    const result = await this.call<{ session_id: string }>("session.spawn", {
      workspace_id: workspaceId,
      backend: "wsl-direct",
      backend_profile: distribution,
      command: ["bash", "-lc", control.command],
      cwd,
      env: dockControlEnv(control),
      columns: 120,
      rows,
      durability: "ephemeral",
      placement,
      pane_id: null,
    });

    return {
      sessionId: result.session_id,
      backendKind: "wsl-direct",
      state: "running",
    };
  }

  async spawnSshTerminal(
    workspaceId: string,
    target: string,
    placement?: TerminalPlacement,
    paneId?: string | null,
  ): Promise<TerminalSession> {
    const result = await this.call<{ session_id: string }>("session.spawn", {
      workspace_id: workspaceId,
      backend: "ssh",
      backend_profile: target,
      command: [],
      cwd: null,
      columns: 120,
      rows: 30,
      durability: "ephemeral",
      placement: placement ?? null,
      pane_id: paneId ?? null,
    });

    return {
      sessionId: result.session_id,
      backendKind: "ssh",
      state: "running",
    };
  }

  async spawnAgentTerminal(
    workspaceId: string,
    command: string[],
    distribution: string | null,
    placement?: TerminalPlacement,
    paneId?: string | null,
  ): Promise<TerminalSession> {
    const result = await this.call<{ session_id: string }>("session.spawn", {
      workspace_id: workspaceId,
      backend: "wsl-tmux-control",
      backend_profile: distribution,
      command,
      cwd: null,
      columns: 120,
      rows: 30,
      durability: "durable",
      placement: placement ?? null,
      pane_id: paneId ?? null,
    });

    return {
      sessionId: result.session_id,
      backendKind: "wsl-tmux-control",
      state: "running",
    };
  }

  async readRecent(sessionId: string, maxBytes: number): Promise<string> {
    const result = await this.call<{ text: string }>("session.read_recent", {
      session_id: sessionId,
      max_bytes: maxBytes,
    });
    return result.text;
  }

  async snapshot(
    sessionId: string,
    sinceOffset?: number,
  ): Promise<OutputSnapshot> {
    const result = await this.call<{
      base_offset: number;
      end_offset: number;
      bytes_base64: string;
    }>("session.snapshot", {
      session_id: sessionId,
      since_offset: sinceOffset ?? null,
    });
    return {
      baseOffset: result.base_offset,
      endOffset: result.end_offset,
      bytes: base64ToBytes(result.bytes_base64),
    };
  }

  async subscribeOutput(
    sessionId: string,
    onFrame: (fromOffset: number, bytes: Uint8Array) => void,
  ): Promise<() => void> {
    const core = window.__TAURI__?.core;
    if (!core?.Channel) {
      throw new Error("Tauri Channel is unavailable for output streaming.");
    }
    const channel = new core.Channel<{
      fromOffset: number;
      bytesBase64: string;
    }>();
    channel.onmessage = (frame) => {
      onFrame(frame.fromOffset, base64ToBytes(frame.bytesBase64));
    };
    // Await registration so the host holds the channel before the renderer
    // takes its cold-start snapshot: output produced after this resolves is
    // streamed, not dropped by the pump for an unregistered session.
    await this.invoke("session_subscribe_output", {
      session_id: sessionId,
      on_event: channel,
    });
    return () => {
      void this.invoke("session_unsubscribe_output", { session_id: sessionId });
    };
  }

  async getSession(sessionId: string): Promise<TerminalSession> {
    const result = await this.call<SessionSummaryWire>("session.get", {
      session_id: sessionId,
    });
    return mapSession(result);
  }

  async sendText(sessionId: string, text: string): Promise<void> {
    await this.call("session.send_text", {
      session_id: sessionId,
      text,
    });
  }

  async sendKey(sessionId: string, key: string): Promise<void> {
    await this.call("session.send_key", {
      session_id: sessionId,
      key,
    });
  }

  async resize(
    sessionId: string,
    columns: number,
    rows: number,
  ): Promise<void> {
    await this.call("session.resize", {
      session_id: sessionId,
      columns,
      rows,
    });
  }

  async listAgentAttention(workspaceId?: string | null): Promise<AgentState[]> {
    const result = await this.call<{ sessions: AgentStateWire[] }>(
      "agent.list_attention",
      {
        workspace_id: workspaceId ?? null,
      },
    );
    return result.sessions.map(mapAgentState);
  }

  async listAgentStates(workspaceId?: string | null): Promise<AgentState[]> {
    const result = await this.call<{ sessions: AgentStateWire[] }>(
      "agent.list",
      {
        workspace_id: workspaceId ?? null,
      },
    );
    return result.sessions.map(mapAgentState);
  }

  async clearAgentAttention(sessionId: string): Promise<void> {
    await this.call("agent.clear_attention", {
      session_id: sessionId,
    });
  }

  async listNotifications(options: {
    workspaceId?: string | null;
    severity?: string | null;
    includeDismissed?: boolean;
  }): Promise<NotificationSummary[]> {
    const result = await this.call<{
      notifications: NotificationSummaryWire[];
    }>("notification.list", {
      workspace_id: options.workspaceId ?? null,
      severity: options.severity ?? null,
      include_dismissed: options.includeDismissed ?? false,
    });
    return result.notifications.map(mapNotification);
  }

  async dismissNotification(notificationId: string): Promise<void> {
    await this.call("notification.dismiss", {
      notification_id: notificationId,
    });
  }

  async getSidebarState(workspaceId?: string | null): Promise<SidebarState> {
    const result = await this.call<SidebarStateWire>("sidebar.state", {
      workspace_id: workspaceId ?? null,
    });
    return mapSidebarState(result);
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
          token,
        },
      },
    });

    if ("Error" in response.outcome) {
      const error = response.outcome.Error;
      throw new ControlClientError(
        error.message,
        error.code,
        error.details_json,
      );
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
          code?: string;
          details_json?: string | null;
          message: string;
        };
      };
}

class BrowserPreviewControlClient implements ControlClient {
  private readonly configStorageKey = "agentmux.preview.config.v1";
  private readonly projectConfigStoragePrefix =
    "agentmux.preview.project.config.v1.";
  private readonly cmuxProjectConfigStoragePrefix =
    "agentmux.preview.cmux.project.config.v1.";
  private readonly dockStorageKey = "agentmux.preview.dock.v1";
  private readonly projectDockStoragePrefix =
    "agentmux.preview.project.dock.v1.";
  private readonly cmuxProjectDockStoragePrefix =
    "agentmux.preview.cmux.project.dock.v1.";
  private workspaceCounter = 0;
  private readonly workspaces: WorkspaceSummary[] = [];
  private readonly workspaceGroups: WorkspaceGroup[] = [];
  private readonly panes = new Map<string, PaneSummary[]>();
  private readonly agentStates = new Map<string, AgentState>();
  private readonly notifications: NotificationSummary[] = [];
  private readonly sidebarStates = new Map<string, SidebarState>();
  private readonly terminalSurfaces: SurfaceSummary[] = [];
  private readonly sessions = new Map<string, TerminalSession>();
  private readonly outputs = new Map<string, string>();
  private readonly browserSurfaces: SurfaceSummary[] = [];
  private readonly browserUrls = new Map<string, string>();
  private readonly browserActionLog: string[] = [];
  private readonly wslDistributions: WslDistribution[];
  private workspaceGroupCounter = 0;
  private terminalCounter = 0;
  private lastSessionId?: string;
  private profileCounter = 3;
  private readonly profiles: SshProfile[] = [
    {
      profileId: "prof_preview_1",
      name: "prod-server",
      host: "10.0.4.12",
      user: "deploy",
      port: 22,
    },
    {
      profileId: "prof_preview_2",
      name: "staging-db",
      host: "10.0.7.3",
      user: "ops",
      port: 22,
    },
    {
      profileId: "prof_preview_3",
      name: "gpu-box",
      host: "gpu.lan",
      user: "ml",
      port: 22,
    },
  ];

  constructor() {
    this.wslDistributions = window.__AGENTMUX_PREVIEW_WSL_DISTRIBUTIONS__ ?? [
      {
        name: "Ubuntu",
        isDefault: true,
      },
      {
        name: "Debian",
        isDefault: false,
      },
    ];
    const previewApi: BrowserPreviewApi = {
      syntheticAgentState: (detail) => this.applySyntheticAgentState(detail),
      sidebarState: (detail) => this.applySyntheticSidebarState(detail),
      browserUrl: (surfaceId?: string) => {
        const id =
          surfaceId ??
          this.browserSurfaces[this.browserSurfaces.length - 1]?.surfaceId;
        return id ? (this.browserUrls.get(id) ?? null) : null;
      },
      browserActions: () => [...this.browserActionLog],
      terminalOutput: (sessionId?: string) => {
        const id = sessionId ?? this.lastSessionId;
        return id ? (this.outputs.get(id) ?? null) : null;
      },
    };
    window.__AGENTMUX_PREVIEW__ = previewApi;
    window.__AGENTMUX_PREVIEW_READY__ = true;
    window.addEventListener("agentmux:synthetic-agent-state", (event) => {
      if (window.__AGENTMUX_PREVIEW__ !== previewApi) {
        return;
      }
      this.applySyntheticAgentState(
        (event as CustomEvent<SyntheticAgentStateDetail>).detail,
      );
    });
  }

  async listWorkspaces(): Promise<WorkspaceSummary[]> {
    return [...this.workspaces];
  }

  async createWorkspace(
    name: string,
    projectRoot?: string | null,
  ): Promise<WorkspaceSummary> {
    const suffix = ++this.workspaceCounter;
    const workspace: WorkspaceSummary = {
      workspaceId: `ws_browser_preview_${suffix}`,
      name,
      rootPaneId: `pane_browser_preview_${suffix}`,
      activePaneId: `pane_browser_preview_${suffix}`,
      projectRoot: projectRoot ?? null,
      environmentProfileId: null,
      description: null,
      icon: null,
      color: null,
      defaultWslDistribution: null,
      defaultAgentCommand: null,
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
        mountedSurfaceId: null,
      },
    ]);
    return workspace;
  }

  async getWorkspace(workspaceId: string): Promise<WorkspaceDetail> {
    const workspace = this.findWorkspace(workspaceId);
    const terminalSurfaces = this.terminalSurfaces.filter(
      (surface) => surface.workspaceId === workspaceId,
    );
    const sessionIds = new Set(
      terminalSurfaces.map((surface) => surface.sessionId).filter(Boolean),
    );
    return {
      workspace,
      panes: [...(this.panes.get(workspaceId) ?? [])],
      surfaces: [
        ...terminalSurfaces,
        ...this.browserSurfaces.filter(
          (surface) => surface.workspaceId === workspaceId,
        ),
      ],
      sessions: [...this.sessions.values()].filter((session) =>
        sessionIds.has(session.sessionId),
      ),
    };
  }

  async renameWorkspace(
    workspaceId: string,
    name: string,
  ): Promise<WorkspaceSummary> {
    const workspace = this.findWorkspace(workspaceId);
    workspace.name = name;
    return workspace;
  }

  async updateWorkspace(
    workspaceId: string,
    input: WorkspaceUpdateInput,
  ): Promise<WorkspaceSummary> {
    const workspace = this.findWorkspace(workspaceId);
    workspace.name = input.name;
    workspace.projectRoot = input.projectRoot ?? null;
    workspace.environmentProfileId = input.environmentProfileId ?? null;
    workspace.description = input.description ?? null;
    workspace.icon = input.icon ?? null;
    workspace.color = input.color ?? null;
    workspace.defaultWslDistribution = input.defaultWslDistribution ?? null;
    workspace.defaultAgentCommand = input.defaultAgentCommand ?? null;
    return workspace;
  }

  async closeWorkspace(
    workspaceId: string,
    closePolicy = "terminate_sessions",
  ): Promise<boolean> {
    const index = this.workspaces.findIndex(
      (workspace) => workspace.workspaceId === workspaceId,
    );
    if (index < 0) {
      return false;
    }
    if (
      closePolicy === "fail_if_running" &&
      this.terminalSurfaces.some(
        (surface) => surface.workspaceId === workspaceId && surface.sessionId,
      )
    ) {
      throw new ControlClientError(
        "Workspace has running sessions.",
        "conflict",
      );
    }
    this.workspaces.splice(index, 1);
    this.panes.delete(workspaceId);
    this.sidebarStates.delete(workspaceId);
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
    for (const group of this.workspaceGroups) {
      group.members = group.members.filter(
        (member) => member.workspaceId !== workspaceId,
      );
      if (group.anchorWorkspaceId === workspaceId) {
        group.anchorWorkspaceId = null;
      }
      group.updatedAt = new Date().toISOString();
    }
    return true;
  }

  async listWorkspaceGroups(): Promise<WorkspaceGroup[]> {
    return this.workspaceGroups.map(cloneWorkspaceGroup);
  }

  async createWorkspaceGroup(
    input: WorkspaceGroupCreateInput,
  ): Promise<WorkspaceGroup> {
    const now = new Date().toISOString();
    const workspaceIds = input.workspaceIds?.length
      ? input.workspaceIds
      : input.anchorWorkspaceId
        ? [input.anchorWorkspaceId]
        : [];
    if (input.anchorWorkspaceId) {
      this.findWorkspace(input.anchorWorkspaceId);
    }
    for (const workspaceId of workspaceIds) {
      this.findWorkspace(workspaceId);
    }
    const group: WorkspaceGroup = {
      groupId: `wsg_browser_preview_${++this.workspaceGroupCounter}`,
      name: input.name.trim() || "Workspace group",
      anchorWorkspaceId: input.anchorWorkspaceId ?? null,
      collapsed: input.collapsed ?? false,
      pinned: input.pinned ?? false,
      color: input.color ?? null,
      icon: input.icon ?? null,
      sortOrder: this.workspaceGroups.length,
      createdAt: now,
      updatedAt: now,
      members: workspaceIds.map((workspaceId, index) => ({
        workspaceId,
        position: index,
      })),
    };
    this.workspaceGroups.push(group);
    return cloneWorkspaceGroup(group);
  }

  async updateWorkspaceGroup(
    groupId: string,
    input: WorkspaceGroupUpdateInput,
  ): Promise<WorkspaceGroup> {
    const group = this.findWorkspaceGroup(groupId);
    if (input.name !== undefined) {
      group.name = input.name.trim() || group.name;
    }
    if (input.anchorWorkspaceId !== undefined) {
      if (input.anchorWorkspaceId) {
        this.findWorkspace(input.anchorWorkspaceId);
      }
      group.anchorWorkspaceId = input.anchorWorkspaceId ?? null;
    }
    if (input.collapsed !== undefined) {
      group.collapsed = input.collapsed;
    }
    if (input.pinned !== undefined) {
      group.pinned = input.pinned;
    }
    if (input.color !== undefined) {
      group.color = input.color?.trim() || null;
    }
    if (input.icon !== undefined) {
      group.icon = input.icon?.trim() || null;
    }
    if (input.sortOrder !== undefined) {
      group.sortOrder = input.sortOrder;
    }
    group.updatedAt = new Date().toISOString();
    return cloneWorkspaceGroup(group);
  }

  async deleteWorkspaceGroup(groupId: string): Promise<void> {
    const index = this.workspaceGroups.findIndex(
      (group) => group.groupId === groupId,
    );
    if (index >= 0) {
      this.workspaceGroups.splice(index, 1);
    }
  }

  async addWorkspaceToGroup(
    groupId: string,
    workspaceId: string,
    position?: number | null,
  ): Promise<WorkspaceGroup> {
    this.findWorkspace(workspaceId);
    const group = this.findWorkspaceGroup(groupId);
    for (const candidate of this.workspaceGroups) {
      candidate.members = candidate.members.filter(
        (member) => member.workspaceId !== workspaceId,
      );
    }
    group.members.push({
      workspaceId,
      position: position ?? group.members.length,
    });
    group.members.sort((a, b) => a.position - b.position);
    group.updatedAt = new Date().toISOString();
    return cloneWorkspaceGroup(group);
  }

  async removeWorkspaceFromGroup(
    groupId: string,
    workspaceId: string,
  ): Promise<WorkspaceGroup> {
    const group = this.findWorkspaceGroup(groupId);
    group.members = group.members.filter(
      (member) => member.workspaceId !== workspaceId,
    );
    group.updatedAt = new Date().toISOString();
    return cloneWorkspaceGroup(group);
  }

  async splitPane(
    workspaceId: string,
    paneId: string,
    axis: "horizontal" | "vertical",
    ratio = 0.5,
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
        mountedSurfaceId,
      },
      {
        paneId: secondChildId,
        workspaceId,
        parentPaneId: paneId,
        kind: "leaf",
        splitAxis: null,
        splitRatio: null,
        mountedSurfaceId: null,
      },
    );
    if (workspace.activePaneId === paneId) {
      workspace.activePaneId = firstChildId;
    }
    return this.getWorkspace(workspaceId);
  }

  async focusPane(
    workspaceId: string,
    paneId: string,
  ): Promise<WorkspaceDetail> {
    const workspace = this.findWorkspace(workspaceId);
    const pane = (this.panes.get(workspaceId) ?? []).find(
      (candidate) => candidate.paneId === paneId,
    );
    if (!pane || pane.kind !== "leaf") {
      throw new Error(`Pane '${paneId}' cannot be focused.`);
    }
    workspace.activePaneId = paneId;
    workspace.rootPaneId =
      this.findRootPaneId(workspaceId, paneId) ?? workspace.rootPaneId;
    return this.getWorkspace(workspaceId);
  }

  async closePane(
    workspaceId: string,
    paneId: string,
    surfacePolicy: string,
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
    const siblings = panes.filter(
      (pane) => pane.parentPaneId === target.parentPaneId,
    );
    const remaining = siblings.find((pane) => pane.paneId !== paneId);
    this.panes.set(
      workspaceId,
      panes.filter(
        (pane) => pane.paneId !== paneId && pane.paneId !== remaining?.paneId,
      ),
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
      if (
        workspace.activePaneId === paneId ||
        workspace.activePaneId === remaining.paneId
      ) {
        workspace.activePaneId = parent.paneId;
      }
    }

    if (
      closedSurfaceId &&
      (surfacePolicy === "close_surface" ||
        surfacePolicy === "fail_if_session_running")
    ) {
      this.removePreviewSurface(closedSurfaceId);
    }

    return this.getWorkspace(workspaceId);
  }

  async resizePaneLayout(
    workspaceId: string,
    paneId: string,
    ratio: number,
  ): Promise<WorkspaceDetail> {
    const pane = (this.panes.get(workspaceId) ?? []).find(
      (candidate) => candidate.paneId === paneId,
    );
    if (!pane || pane.kind !== "split") {
      throw new Error(`Pane '${paneId}' cannot be resized.`);
    }
    pane.splitRatio = Math.min(0.9, Math.max(0.1, ratio));
    return this.getWorkspace(workspaceId);
  }

  async mountSurface(
    workspaceId: string,
    paneId: string,
    surfaceId: string,
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

  async unmountSurface(
    workspaceId: string,
    paneId: string,
  ): Promise<WorkspaceDetail> {
    this.findWorkspace(workspaceId);
    const pane = (this.panes.get(workspaceId) ?? []).find(
      (candidate) => candidate.paneId === paneId,
    );
    if (!pane || pane.kind !== "leaf") {
      throw new Error(`Pane '${paneId}' cannot unmount surfaces.`);
    }
    pane.mountedSurfaceId = null;
    return this.getWorkspace(workspaceId);
  }

  async createBrowserSurface(
    workspaceId: string,
    paneId?: string | null,
    profile?: string | null,
    placement: TerminalPlacement = "active_pane",
  ): Promise<SurfaceSummary> {
    const workspace = this.findWorkspace(workspaceId);
    let targetPaneId = paneId ?? workspace.activePaneId;
    if (placement === "new_tab") {
      const suffix = this.browserSurfaces.length + 1;
      targetPaneId = `pane_browser_preview_browser_tab_${suffix}`;
      const panes = this.panes.get(workspaceId) ?? [];
      panes.push({
        paneId: targetPaneId,
        workspaceId,
        parentPaneId: null,
        kind: "leaf",
        splitAxis: null,
        splitRatio: null,
        mountedSurfaceId: null,
      });
      this.panes.set(workspaceId, panes);
      workspace.rootPaneId = targetPaneId;
      workspace.activePaneId = targetPaneId;
    }
    const pane = (this.panes.get(workspaceId) ?? []).find(
      (candidate) => candidate.paneId === targetPaneId,
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
      browserId: `browser_preview_${suffix}`,
    };
    this.browserSurfaces.push(surface);
    this.browserUrls.set(surface.surfaceId, "about:blank");
    await this.mountSurface(workspaceId, targetPaneId, surface.surfaceId);
    return surface;
  }

  async closeSurface(
    workspaceId: string,
    surfaceId: string,
  ): Promise<WorkspaceDetail> {
    const workspace = this.findWorkspace(workspaceId);
    const terminalIndex = this.terminalSurfaces.findIndex(
      (surface) =>
        surface.surfaceId === surfaceId && surface.workspaceId === workspaceId,
    );
    const browserIndex = this.browserSurfaces.findIndex(
      (surface) =>
        surface.surfaceId === surfaceId && surface.workspaceId === workspaceId,
    );
    if (terminalIndex < 0 && browserIndex < 0) {
      throw new Error(`Surface '${surfaceId}' was not found.`);
    }

    const panes = this.panes.get(workspaceId) ?? [];
    const hostPane = panes.find((pane) => pane.mountedSurfaceId === surfaceId);
    const rootPaneIds = panes
      .filter((pane) => !pane.parentPaneId)
      .map((pane) => pane.paneId);
    if (hostPane && rootPaneIds.length > 1) {
      const rootPaneId = this.findRootPaneId(workspaceId, hostPane.paneId);
      if (rootPaneId) {
        const subtreePaneIds = this.previewPaneSubtreeIds(
          workspaceId,
          rootPaneId,
        );
        const subtreeSurfaceIds = panes
          .filter((pane) => subtreePaneIds.includes(pane.paneId))
          .map((pane) => pane.mountedSurfaceId)
          .filter((candidate): candidate is string => Boolean(candidate));
        if (!subtreeSurfaceIds.includes(surfaceId)) {
          subtreeSurfaceIds.push(surfaceId);
        }
        for (const candidate of subtreeSurfaceIds) {
          this.removePreviewSurface(candidate);
        }
        this.panes.set(
          workspaceId,
          panes.filter((pane) => !subtreePaneIds.includes(pane.paneId)),
        );
        const fallbackRootId =
          rootPaneIds[rootPaneIds.indexOf(rootPaneId) - 1] ??
          rootPaneIds[rootPaneIds.indexOf(rootPaneId) + 1] ??
          null;
        if (fallbackRootId) {
          workspace.rootPaneId = fallbackRootId;
          workspace.activePaneId =
            this.firstPreviewLeafId(workspaceId, fallbackRootId) ??
            fallbackRootId;
        }
        return this.getWorkspace(workspaceId);
      }
    }

    const surfaces = [
      ...this.terminalSurfaces.filter(
        (surface) => surface.workspaceId === workspaceId,
      ),
      ...this.browserSurfaces.filter(
        (surface) => surface.workspaceId === workspaceId,
      ),
    ];
    const closedIndex = surfaces.findIndex(
      (surface) => surface.surfaceId === surfaceId,
    );
    const replacement =
      surfaces.length <= 1
        ? null
        : (surfaces[Math.max(0, closedIndex - 1)] ?? surfaces[0] ?? null);
    const mountedPanes = panes.filter(
      (pane) => pane.mountedSurfaceId === surfaceId,
    );

    this.removePreviewSurface(surfaceId);
    for (const pane of mountedPanes) {
      pane.mountedSurfaceId = replacement?.surfaceId ?? null;
    }
    if (replacement && mountedPanes.length > 0) {
      for (const pane of panes) {
        if (
          pane.mountedSurfaceId === replacement.surfaceId &&
          !mountedPanes.some((target) => target.paneId === pane.paneId)
        ) {
          pane.mountedSurfaceId = null;
        }
      }
    }

    return this.getWorkspace(workspaceId);
  }

  async browserNavigate(
    surfaceId: string,
    url: string,
  ): Promise<BrowserNavigationResult> {
    this.findBrowserSurface(surfaceId);
    this.browserUrls.set(surfaceId, url);
    this.browserActionLog.push(`navigate:${surfaceId}:${url}`);
    return {
      surfaceId,
      url,
    };
  }

  async browserReload(surfaceId: string): Promise<BrowserNavigationResult> {
    this.findBrowserSurface(surfaceId);
    const url = this.browserUrls.get(surfaceId) ?? "about:blank";
    this.browserActionLog.push(`reload:${surfaceId}`);
    return {
      surfaceId,
      url,
    };
  }

  async browserBack(surfaceId: string): Promise<BrowserNavigationResult> {
    this.findBrowserSurface(surfaceId);
    const url = this.browserUrls.get(surfaceId) ?? "about:blank";
    this.browserActionLog.push(`back:${surfaceId}`);
    return {
      surfaceId,
      url,
    };
  }

  async browserForward(surfaceId: string): Promise<BrowserNavigationResult> {
    this.findBrowserSurface(surfaceId);
    const url = this.browserUrls.get(surfaceId) ?? "about:blank";
    this.browserActionLog.push(`forward:${surfaceId}`);
    return {
      surfaceId,
      url,
    };
  }

  async browserCurrentUrl(surfaceId: string): Promise<BrowserNavigationResult> {
    this.findBrowserSurface(surfaceId);
    const url = this.browserUrls.get(surfaceId) ?? "about:blank";
    this.browserActionLog.push(`current-url:${surfaceId}`);
    return {
      surfaceId,
      url,
    };
  }

  async browserScreenshot(
    surfaceId: string,
    format?: string | null,
  ): Promise<BrowserScreenshotResult> {
    this.findBrowserSurface(surfaceId);
    const resolvedFormat = format || "png";
    const imageHandle = `memory://browser-preview/${surfaceId}/${resolvedFormat}`;
    this.browserActionLog.push(`screenshot:${surfaceId}:${resolvedFormat}`);
    return {
      surfaceId,
      format: resolvedFormat,
      imageHandle,
      byteCount: imageHandle.length,
    };
  }

  async browserDomSnapshot(
    surfaceId: string,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserDomSnapshotResult> {
    this.findBrowserSurface(surfaceId);
    const url = this.browserUrls.get(surfaceId) ?? "about:blank";
    this.browserActionLog.push(
      `dom-snapshot:${surfaceId}${browserFrameLog(options.frameId)}`,
    );
    return {
      surfaceId,
      html: `<html data-agentmux-surface="${surfaceId}"><body>${url}</body></html>`,
    };
  }

  async browserClick(
    surfaceId: string,
    target: BrowserClickTarget,
  ): Promise<BrowserActionResult> {
    this.findBrowserSurface(surfaceId);
    this.browserActionLog.push(
      `click:${surfaceId}:${target.selector ?? `${target.x},${target.y}`}${browserFrameLog(target.frameId)}`,
    );
    return {
      surfaceId,
      ok: true,
    };
  }

  async browserType(
    surfaceId: string,
    selector: string,
    text: string,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    this.findBrowserSurface(surfaceId);
    this.browserActionLog.push(
      `type:${surfaceId}:${selector}:${text}${browserFrameLog(options.frameId)}`,
    );
    return {
      surfaceId,
      ok: true,
    };
  }

  async browserFill(
    surfaceId: string,
    selector: string,
    text: string,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    this.findBrowserSurface(surfaceId);
    this.browserActionLog.push(
      `fill:${surfaceId}:${selector}:${text}${browserFrameLog(options.frameId)}`,
    );
    return {
      surfaceId,
      ok: true,
    };
  }

  async browserPress(
    surfaceId: string,
    selector: string,
    key: string,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    this.findBrowserSurface(surfaceId);
    this.browserActionLog.push(
      `press:${surfaceId}:${selector}:${key}${browserFrameLog(options.frameId)}`,
    );
    return {
      surfaceId,
      ok: true,
    };
  }

  async browserSelect(
    surfaceId: string,
    selector: string,
    values: string[],
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    this.findBrowserSurface(surfaceId);
    this.browserActionLog.push(
      `select:${surfaceId}:${selector}:${values.join(",")}${browserFrameLog(options.frameId)}`,
    );
    return {
      surfaceId,
      ok: true,
    };
  }

  async browserScroll(
    surfaceId: string,
    options: {
      selector?: string | null;
      x?: number | null;
      y?: number | null;
      frameId?: string | null;
    },
  ): Promise<BrowserActionResult> {
    this.findBrowserSurface(surfaceId);
    this.browserActionLog.push(
      `scroll:${surfaceId}:${options.selector ?? "window"}:${options.x ?? 0}:${options.y ?? 0}${browserFrameLog(options.frameId)}`,
    );
    return {
      surfaceId,
      ok: true,
    };
  }

  async browserHover(
    surfaceId: string,
    selector: string,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    this.findBrowserSurface(surfaceId);
    this.browserActionLog.push(
      `hover:${surfaceId}:${selector}${browserFrameLog(options.frameId)}`,
    );
    return {
      surfaceId,
      ok: true,
    };
  }

  async browserCheck(
    surfaceId: string,
    selector: string,
    checked?: boolean | null,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    this.findBrowserSurface(surfaceId);
    this.browserActionLog.push(
      `check:${surfaceId}:${selector}:${checked ?? true}${browserFrameLog(options.frameId)}`,
    );
    return {
      surfaceId,
      ok: true,
    };
  }

  async browserGet(
    surfaceId: string,
    selector: string,
    options: {
      kind?: string | null;
      attribute?: string | null;
      frameId?: string | null;
    } = {},
  ): Promise<BrowserGetResult> {
    this.findBrowserSurface(surfaceId);
    const kind = options.attribute
      ? `attribute:${options.attribute}`
      : options.kind || "text";
    this.browserActionLog.push(
      `get:${surfaceId}:${selector}:${kind}${browserFrameLog(options.frameId)}`,
    );
    return {
      surfaceId,
      selector,
      kind,
      value: `${kind}:${selector}`,
    };
  }

  async browserFind(
    surfaceId: string,
    query: string,
    options: {
      selector?: string | null;
      limit?: number | null;
      frameId?: string | null;
    } = {},
  ): Promise<BrowserFindResult> {
    this.findBrowserSurface(surfaceId);
    const selector = options.selector ?? "body";
    this.browserActionLog.push(
      `find:${surfaceId}:${selector}:${query}:${options.limit ?? 10}${browserFrameLog(options.frameId)}`,
    );
    return {
      surfaceId,
      query,
      count: 1,
      matches: [`${selector}:${query}`],
    };
  }

  async browserHighlight(
    surfaceId: string,
    selector: string,
    durationMs?: number | null,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    this.findBrowserSurface(surfaceId);
    this.browserActionLog.push(
      `highlight:${surfaceId}:${selector}:${durationMs ?? 1200}${browserFrameLog(options.frameId)}`,
    );
    return {
      surfaceId,
      ok: true,
    };
  }

  async browserFocus(
    surfaceId: string,
    selector: string,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserActionResult> {
    this.findBrowserSurface(surfaceId);
    this.browserActionLog.push(
      `focus:${surfaceId}:${selector}${browserFrameLog(options.frameId)}`,
    );
    return {
      surfaceId,
      ok: true,
    };
  }

  async browserZoom(
    surfaceId: string,
    percent: number,
  ): Promise<BrowserActionResult> {
    this.findBrowserSurface(surfaceId);
    this.browserActionLog.push(`zoom:${surfaceId}:${percent}`);
    return {
      surfaceId,
      ok: true,
    };
  }

  async browserWaitForSelector(
    surfaceId: string,
    selector: string,
    timeoutMs?: number | null,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserWaitForSelectorResult> {
    this.findBrowserSurface(surfaceId);
    this.browserActionLog.push(
      `wait-for-selector:${surfaceId}:${selector}:${timeoutMs ?? 5000}${browserFrameLog(options.frameId)}`,
    );
    return {
      surfaceId,
      selector,
      elapsedMs: 1,
    };
  }

  async browserEvaluate(
    surfaceId: string,
    script: string,
    options: { frameId?: string | null } = {},
  ): Promise<BrowserEvaluateResult> {
    this.findBrowserSurface(surfaceId);
    this.browserActionLog.push(
      `evaluate:${surfaceId}:${script}${browserFrameLog(options.frameId)}`,
    );
    return {
      surfaceId,
      valueJson: '{"ok":true}',
    };
  }

  async browserDiagnostics(): Promise<BrowserDiagnostic[]> {
    return [];
  }

  async recoveryDiagnostics(): Promise<RecoveryDiagnostics> {
    const sessions = [...this.sessions.values()];
    return {
      workspaceCount: this.workspaces.length,
      paneCount: [...this.panes.values()].reduce(
        (count, panes) => count + panes.length,
        0,
      ),
      surfaceCount: this.terminalSurfaces.length + this.browserSurfaces.length,
      sessionCount: sessions.length,
      sessions: sessions.map((session) => {
        const surface = this.terminalSurfaces.find(
          (candidate) => candidate.sessionId === session.sessionId,
        );
        return {
          sessionId: session.sessionId,
          workspaceId:
            surface?.workspaceId ??
            this.workspaces[0]?.workspaceId ??
            "ws_browser_preview",
          backendKind: session.backendKind,
          state: session.state,
          durability: "ephemeral",
          backendNativeId: null,
        };
      }),
    };
  }

  async listWslDistributions(): Promise<WslDistribution[]> {
    return this.wslDistributions.map((distribution) => ({ ...distribution }));
  }

  async checkTmux(distribution?: string | null): Promise<TmuxDiagnostics> {
    const available = window.__AGENTMUX_PREVIEW_TMUX_AVAILABLE__ ?? true;
    return {
      available,
      distribution:
        distribution ??
        this.wslDistributions.find((candidate) => candidate.isDefault)?.name ??
        null,
      version: available ? "tmux 3.4-preview" : null,
      message: available
        ? "tmux is available in the preview WSL distribution."
        : "tmux was not found in the selected WSL distribution. Install it with `sudo apt update && sudo apt install -y tmux`.",
    };
  }

  async getConfig(workspaceId?: string | null): Promise<AppConfig> {
    return this.readPreviewConfig(workspaceId);
  }

  async reloadConfig(workspaceId?: string | null): Promise<AppConfig> {
    return this.readPreviewConfig(workspaceId);
  }

  async updateConfig(
    update: AppConfigUpdate,
    workspaceId?: string | null,
  ): Promise<AppConfig> {
    const current = this.readPreviewConfig(workspaceId);
    const next: AppConfig = {
      ...current,
      appearance: {
        ...current.appearance,
        ...(update.appearance ?? {}),
      },
      shortcuts: {
        bindings: {
          ...current.shortcuts.bindings,
          ...(update.shortcuts?.bindings ?? {}),
        },
      },
      ui: sanitizeAppConfigUi({
        ...current.ui,
        ...(update.ui ?? {}),
      }),
    };
    window.localStorage.setItem(this.configStorageKey, JSON.stringify(next));
    return this.readPreviewConfig(workspaceId);
  }

  async exportConfig(
    options: { workspaceId?: string | null; scope?: AppConfigScope } = {},
  ): Promise<AppConfigExport> {
    const config = this.readPreviewConfig(options.workspaceId);
    const projectConfig = this.readPreviewProjectConfig(options.workspaceId);
    return {
      json: JSON.stringify(
        (options.scope ?? "global") === "project"
          ? previewProjectConfigExportSnapshot(projectConfig)
          : appConfigExportSnapshot(config, "global"),
        null,
        2,
      ),
      config,
    };
  }

  async importConfig(
    json: string,
    options: { workspaceId?: string | null; scope?: AppConfigScope } = {},
  ): Promise<AppConfig> {
    const parsed = previewConfigFromImport(JSON.parse(json));
    if ((options.scope ?? "global") === "project") {
      const key = this.previewProjectConfigStorageKey(options.workspaceId);
      if (key) {
        window.localStorage.setItem(key, JSON.stringify(parsed));
      }
    } else {
      window.localStorage.setItem(
        this.configStorageKey,
        JSON.stringify(parsed),
      );
    }
    return this.readPreviewConfig(options.workspaceId);
  }

  async resetConfig(
    options: { workspaceId?: string | null; scope?: AppConfigScope } = {},
  ): Promise<AppConfig> {
    if ((options.scope ?? "global") === "project") {
      const key = this.previewProjectConfigStorageKey(options.workspaceId);
      if (key) {
        window.localStorage.removeItem(key);
      }
    } else {
      window.localStorage.removeItem(this.configStorageKey);
    }
    return this.readPreviewConfig(options.workspaceId);
  }

  async migrateProjectConfig(
    options: { workspaceId?: string | null; overwrite?: boolean } = {},
  ): Promise<AppConfigMigration> {
    const sourceKey = this.previewCmuxProjectConfigStorageKey(
      options.workspaceId,
    );
    const targetKey = this.previewProjectConfigStorageKey(options.workspaceId);
    if (!sourceKey || !targetKey) {
      throw new Error("Project config migration requires an active workspace.");
    }
    if (window.localStorage.getItem(targetKey) && !options.overwrite) {
      throw new Error("Preview AgentMux project config already exists.");
    }
    const raw = window.localStorage.getItem(sourceKey);
    if (!raw) {
      throw new Error("No preview .cmux/cmux.json config is available.");
    }
    const parsed = previewConfigFromImport(JSON.parse(raw));
    const overwritten = window.localStorage.getItem(targetKey) !== null;
    window.localStorage.setItem(targetKey, JSON.stringify(parsed));
    return {
      sourcePath: sourceKey,
      targetPath: targetKey,
      overwritten,
      config: this.readPreviewConfig(options.workspaceId),
    };
  }

  async configDiagnostics(
    workspaceId?: string | null,
  ): Promise<AppConfigDiagnosticEntry[]> {
    const projectPath = this.previewProjectConfigPath(workspaceId);
    const projectKey = this.previewProjectConfigStorageKey(workspaceId);
    const cmuxKey = this.previewCmuxProjectConfigStorageKey(workspaceId);
    const projectExists = Boolean(
      projectKey && window.localStorage.getItem(projectKey),
    );
    const cmuxExists = Boolean(cmuxKey && window.localStorage.getItem(cmuxKey));
    return [
      {
        source: "global",
        path: "localStorage://agentmux.preview.config.v1",
        exists: window.localStorage.getItem(this.configStorageKey) !== null,
        valid: true,
        active: true,
        message: "Preview global config is readable.",
      },
      {
        source: "project",
        path: projectPath,
        exists: projectExists,
        valid: true,
        active: projectExists,
        message: projectExists
          ? "Preview AgentMux project config is readable."
          : "Preview AgentMux project config is absent.",
      },
      {
        source: "cmux_project",
        path: cmuxKey,
        exists: cmuxExists,
        valid: true,
        active: !projectExists && cmuxExists,
        message: cmuxExists
          ? projectExists
            ? "Preview .cmux config is ignored because project config exists."
            : "Preview .cmux config is available as fallback."
          : "Preview .cmux config is absent.",
      },
    ];
  }

  async getDock(workspaceId?: string | null): Promise<DockConfig> {
    return this.readPreviewDock(workspaceId);
  }

  async trustDock(workspaceId: string): Promise<DockConfig> {
    const dock = this.readPreviewDock(workspaceId);
    if (dock.requiresTrust) {
      this.writePreviewDockTrust(workspaceId, dock);
    }
    return this.readPreviewDock(workspaceId);
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
      port: input.port ?? null,
    };
    this.profiles.push(profile);
    return { ...profile };
  }

  async updateProfile(
    profileId: string,
    input: SshProfileInput,
  ): Promise<SshProfile> {
    const profile = this.profiles.find(
      (candidate) => candidate.profileId === profileId,
    );
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
    const index = this.profiles.findIndex(
      (candidate) => candidate.profileId === profileId,
    );
    if (index >= 0) {
      this.profiles.splice(index, 1);
    }
  }

  async spawnNativeTerminal(
    workspaceId: string,
    command: string[],
    placement: TerminalPlacement = "active_pane",
    paneId?: string | null,
  ): Promise<TerminalSession> {
    const commandText = command.join(" ");
    return this.createPreviewTerminal(
      workspaceId,
      "conpty",
      "conpty",
      ["\r\n$ " + commandText, "\r\nagentmux desktop preview", "\r\n"].join(""),
      placement,
      paneId,
    );
  }

  async spawnWslTerminal(
    workspaceId: string,
    distribution: string | null,
    cwd: string | null,
    placement: TerminalPlacement = "active_pane",
    paneId?: string | null,
  ): Promise<TerminalSession> {
    return this.createPreviewTerminal(
      workspaceId,
      "wsl-direct",
      "wsl-direct",
      [
        "\r\n$ wsl " + (distribution ?? "default") + " " + (cwd ?? "~"),
        "\r\nagentmux WSL desktop preview",
        "\r\n",
      ].join(""),
      placement,
      paneId,
    );
  }

  async spawnDurableWslTerminal(
    workspaceId: string,
    distribution: string | null,
    cwd: string | null,
    placement: TerminalPlacement = "active_pane",
    paneId?: string | null,
  ): Promise<TerminalSession> {
    return this.createPreviewTerminal(
      workspaceId,
      "wsl-tmux-control",
      "wsl-tmux-control",
      [
        "\r\n$ wsl " + (distribution ?? "default") + " " + (cwd ?? "~") + "  (durable · tmux)",
        "\r\nagentmux durable WSL desktop preview",
        "\r\n",
      ].join(""),
      placement,
      paneId,
    );
  }

  async spawnDockTerminal(
    workspaceId: string,
    control: DockControl,
    distribution: string | null,
    cwd: string | null,
    placement: SessionPlacement = "dock",
  ): Promise<TerminalSession> {
    const envKeys = Object.keys(control.env);
    return this.createPreviewTerminal(
      workspaceId,
      "wsl-direct",
      control.title,
      [
        "\r\n$ " + control.command,
        "\r\nagentmux Dock preview · " + (distribution ?? "WSL"),
        "\r\ncwd " + (cwd || "~"),
        envKeys.length > 0 ? "\r\nenv " + envKeys.join(",") : "",
        "\r\n",
      ].join(""),
      placement,
      null,
      "dock-terminal",
      control.id,
    );
  }

  async spawnSshTerminal(
    workspaceId: string,
    target: string,
    placement: TerminalPlacement = "active_pane",
    paneId?: string | null,
  ): Promise<TerminalSession> {
    return this.createPreviewTerminal(
      workspaceId,
      "ssh",
      "ssh",
      [
        "\r\n$ ssh " + target,
        "\r\nagentmux SSH desktop preview (실제 접속은 Tauri 실행에서 동작)",
        "\r\n",
      ].join(""),
      placement,
      paneId,
    );
  }

  async spawnAgentTerminal(
    workspaceId: string,
    command: string[],
    distribution: string | null,
    placement: TerminalPlacement = "active_pane",
    paneId?: string | null,
  ): Promise<TerminalSession> {
    const label = command.join(" ") || "agent";
    return this.createPreviewTerminal(
      workspaceId,
      "wsl-tmux-control",
      "wsl-tmux-control",
      [
        "\r\n$ " +
          label +
          "   (durable tmux · " +
          (distribution ?? "WSL") +
          ")",
        "\r\nagentmux 에이전트 세션 preview — 실제 실행/durable 복원은 Tauri에서 동작",
        "\r\n",
      ].join(""),
      placement,
      paneId,
    );
  }

  async readRecent(sessionId: string, _maxBytes: number): Promise<string> {
    return this.outputs.get(sessionId) ?? "";
  }

  async getSession(sessionId: string): Promise<TerminalSession> {
    return (
      this.sessions.get(sessionId) ?? {
        sessionId,
        backendKind: "conpty",
        state: "lost",
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

  async resize(
    _sessionId: string,
    _columns: number,
    _rows: number,
  ): Promise<void> {
    return;
  }

  async listAgentAttention(workspaceId?: string | null): Promise<AgentState[]> {
    return [...this.agentStates.values()].filter(
      (state) =>
        state.attention && (!workspaceId || state.workspaceId === workspaceId),
    );
  }

  async listAgentStates(workspaceId?: string | null): Promise<AgentState[]> {
    return [...this.agentStates.values()].filter(
      (state) => !workspaceId || state.workspaceId === workspaceId,
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
      if (
        options.workspaceId &&
        notification.workspaceId !== options.workspaceId
      ) {
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
      (candidate) => candidate.notificationId === notificationId,
    );
    if (notification) {
      notification.dismissed = true;
    }
  }

  async getSidebarState(workspaceId?: string | null): Promise<SidebarState> {
    const workspace = workspaceId
      ? this.findWorkspace(workspaceId)
      : this.workspaces[0];
    if (!workspace) {
      return {
        workspaceId: workspaceId ?? "ws_browser_preview",
        cwd: null,
        gitBranch: null,
        gitHash: null,
        ports: [],
        statuses: [],
        progress: null,
        logs: [],
      };
    }
    return this.readSidebarState(workspace.workspaceId);
  }

  private findWorkspace(workspaceId: string): WorkspaceSummary {
    const workspace = this.workspaces.find(
      (candidate) => candidate.workspaceId === workspaceId,
    );
    if (!workspace) {
      throw new Error(`Workspace '${workspaceId}' was not found.`);
    }
    return workspace;
  }

  private findWorkspaceGroup(groupId: string): WorkspaceGroup {
    const group = this.workspaceGroups.find(
      (candidate) => candidate.groupId === groupId,
    );
    if (!group) {
      throw new Error(`Workspace group '${groupId}' was not found.`);
    }
    return group;
  }

  private findBrowserSurface(surfaceId: string): SurfaceSummary {
    const surface = this.browserSurfaces.find(
      (candidate) => candidate.surfaceId === surfaceId,
    );
    if (!surface) {
      throw new Error(`Browser surface '${surfaceId}' was not found.`);
    }
    return surface;
  }

  private createPreviewTerminal(
    workspaceId: string,
    backendKind: string,
    title: string,
    output: string,
    placement: SessionPlacement = "active_pane",
    paneId?: string | null,
    surfaceType = "terminal",
    browserId?: string | null,
  ): TerminalSession {
    const workspace = this.findWorkspace(workspaceId);
    const suffix = ++this.terminalCounter;
    const sessionId = `ses_browser_preview_${suffix}`;
    const surfaceId = `surf_browser_preview_terminal_${suffix}`;
    const session: TerminalSession = {
      sessionId,
      backendKind,
      state: "preview",
    };
    this.sessions.set(sessionId, session);
    this.terminalSurfaces.push({
      surfaceId,
      workspaceId,
      surfaceType,
      title,
      sessionId,
      browserId: browserId ?? null,
    });
    this.outputs.set(sessionId, output);
    this.lastSessionId = sessionId;
    if (placement === "dock") {
      return session;
    }
    if (placement === "new_tab") {
      const rootPaneId = `pane_browser_preview_tab_${suffix}`;
      const panes = this.panes.get(workspaceId) ?? [];
      panes.push({
        paneId: rootPaneId,
        workspaceId,
        parentPaneId: null,
        kind: "leaf",
        splitAxis: null,
        splitRatio: null,
        mountedSurfaceId: surfaceId,
      });
      this.panes.set(workspaceId, panes);
      workspace.rootPaneId = rootPaneId;
      workspace.activePaneId = rootPaneId;
    } else {
      this.mountPreviewSurface(
        workspaceId,
        surfaceId,
        paneId ?? workspace.activePaneId,
      );
    }
    return session;
  }

  private mountPreviewSurface(
    workspaceId: string,
    surfaceId: string,
    paneId?: string | null,
  ): void {
    const workspace = this.findWorkspace(workspaceId);
    const pane = (this.panes.get(workspaceId) ?? []).find(
      (candidate) => candidate.paneId === (paneId ?? workspace.activePaneId),
    );
    if (pane) {
      for (const candidate of this.panes.get(workspaceId) ?? []) {
        if (candidate.mountedSurfaceId === surfaceId) {
          candidate.mountedSurfaceId = null;
        }
      }
      pane.mountedSurfaceId = surfaceId;
      workspace.activePaneId = pane.paneId;
      workspace.rootPaneId =
        this.findRootPaneId(workspaceId, pane.paneId) ?? workspace.rootPaneId;
    }
  }

  private findRootPaneId(workspaceId: string, paneId: string): string | null {
    const panes = this.panes.get(workspaceId) ?? [];
    let pane = panes.find((candidate) => candidate.paneId === paneId);
    let guard = 0;
    while (pane?.parentPaneId && guard < 100) {
      const parentPaneId = pane.parentPaneId;
      pane = panes.find((candidate) => candidate.paneId === parentPaneId);
      guard += 1;
    }
    return pane?.paneId ?? null;
  }

  private previewPaneSubtreeIds(workspaceId: string, paneId: string): string[] {
    const panes = this.panes.get(workspaceId) ?? [];
    const ids = [paneId];
    for (const child of panes.filter((pane) => pane.parentPaneId === paneId)) {
      ids.push(...this.previewPaneSubtreeIds(workspaceId, child.paneId));
    }
    return ids;
  }

  private firstPreviewLeafId(
    workspaceId: string,
    paneId: string,
  ): string | null {
    const panes = this.panes.get(workspaceId) ?? [];
    const pane = panes.find((candidate) => candidate.paneId === paneId);
    if (!pane) {
      return null;
    }
    if (pane.kind === "leaf") {
      return pane.paneId;
    }
    for (const child of panes.filter(
      (candidate) => candidate.parentPaneId === paneId,
    )) {
      const leafId = this.firstPreviewLeafId(workspaceId, child.paneId);
      if (leafId) {
        return leafId;
      }
    }
    return null;
  }

  private surfaceHasRunningSession(surfaceId: string): boolean {
    const surface = this.terminalSurfaces.find(
      (candidate) => candidate.surfaceId === surfaceId,
    );
    if (!surface?.sessionId) {
      return false;
    }
    const session = this.sessions.get(surface.sessionId);
    return Boolean(
      session &&
      !["exited", "failed", "lost", "disconnected"].includes(session.state),
    );
  }

  private removePreviewSurface(surfaceId: string): void {
    for (const panes of this.panes.values()) {
      for (const pane of panes) {
        if (pane.mountedSurfaceId === surfaceId) {
          pane.mountedSurfaceId = null;
        }
      }
    }

    const terminalIndex = this.terminalSurfaces.findIndex(
      (surface) => surface.surfaceId === surfaceId,
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
      (surface) => surface.surfaceId === surfaceId,
    );
    if (browserIndex >= 0) {
      this.browserUrls.delete(surfaceId);
      this.browserSurfaces.splice(browserIndex, 1);
    }
  }

  private readSidebarState(workspaceId: string): SidebarState {
    const workspace = this.findWorkspace(workspaceId);
    const current = this.sidebarStates.get(workspaceId);
    return {
      workspaceId,
      cwd: current?.cwd ?? workspace.projectRoot ?? null,
      gitBranch: current?.gitBranch ?? null,
      gitHash: current?.gitHash ?? null,
      ports: current?.ports ?? [],
      statuses: current?.statuses ?? [],
      progress: current?.progress ?? null,
      logs: current?.logs ?? [],
    };
  }

  private previewProjectConfigPath(workspaceId?: string | null): string | null {
    const workspace = workspaceId
      ? this.workspaces.find(
          (candidate) => candidate.workspaceId === workspaceId,
        )
      : (this.workspaces[0] ?? null);
    const root = workspace?.projectRoot?.trim();
    if (!root) {
      return null;
    }
    const separator = root.includes("\\") ? "\\" : "/";
    return `${root.replace(/[\\/]+$/, "")}${separator}.agentmux${separator}agentmux.json`;
  }

  private previewProjectConfigStorageKey(
    workspaceId?: string | null,
  ): string | null {
    const workspace = workspaceId
      ? this.workspaces.find(
          (candidate) => candidate.workspaceId === workspaceId,
        )
      : (this.workspaces[0] ?? null);
    return workspace
      ? `${this.projectConfigStoragePrefix}${workspace.workspaceId}`
      : null;
  }

  private previewCmuxProjectConfigStorageKey(
    workspaceId?: string | null,
  ): string | null {
    const workspace = workspaceId
      ? this.workspaces.find(
          (candidate) => candidate.workspaceId === workspaceId,
        )
      : (this.workspaces[0] ?? null);
    return workspace
      ? `${this.cmuxProjectConfigStoragePrefix}${workspace.workspaceId}`
      : null;
  }

  private previewDockPath(
    workspaceId: string | null | undefined,
    source:
      | "project_agentmux"
      | "project_cmux"
      | "global_agentmux"
      | "global_cmux",
  ): string {
    if (source === "global_agentmux") {
      return "localStorage://agentmux.preview.dock.v1";
    }
    if (source === "global_cmux") {
      return "localStorage://cmux.preview.dock.v1";
    }
    const workspace = workspaceId
      ? this.workspaces.find(
          (candidate) => candidate.workspaceId === workspaceId,
        )
      : (this.workspaces[0] ?? null);
    const root = workspace?.projectRoot?.trim() || "preview-project";
    const separator = root.includes("\\") ? "\\" : "/";
    const folder = source === "project_agentmux" ? ".agentmux" : ".cmux";
    return `${root.replace(/[\\/]+$/, "")}${separator}${folder}${separator}dock.json`;
  }

  private previewProjectDockStorageKey(
    workspaceId?: string | null,
  ): string | null {
    const workspace = workspaceId
      ? this.workspaces.find(
          (candidate) => candidate.workspaceId === workspaceId,
        )
      : (this.workspaces[0] ?? null);
    return workspace
      ? `${this.projectDockStoragePrefix}${workspace.workspaceId}`
      : null;
  }

  private previewCmuxProjectDockStorageKey(
    workspaceId?: string | null,
  ): string | null {
    const workspace = workspaceId
      ? this.workspaces.find(
          (candidate) => candidate.workspaceId === workspaceId,
        )
      : (this.workspaces[0] ?? null);
    return workspace
      ? `${this.cmuxProjectDockStoragePrefix}${workspace.workspaceId}`
      : null;
  }

  private previewDockTrustStorageKey(
    workspaceId: string | null | undefined,
    source: string,
    configPath: string | null | undefined,
  ): string | null {
    if (!workspaceId || !configPath) {
      return null;
    }
    return `agentmux.dock.trust.v1.${encodeURIComponent([workspaceId, source, configPath].join("|"))}`;
  }

  private readPreviewDockTrust(
    workspaceId: string | null | undefined,
    source: string,
    configPath: string,
  ): boolean {
    const key = this.previewDockTrustStorageKey(
      workspaceId,
      source,
      configPath,
    );
    if (!key) {
      return false;
    }
    try {
      return window.localStorage.getItem(key) === "trusted";
    } catch {
      return false;
    }
  }

  private writePreviewDockTrust(workspaceId: string, dock: DockConfig): void {
    const key = this.previewDockTrustStorageKey(
      workspaceId,
      dock.source,
      dock.configPath,
    );
    if (!key) {
      return;
    }
    try {
      window.localStorage.setItem(key, "trusted");
    } catch {
      // Preview trust persistence should not block the local run path.
    }
  }

  private readPreviewDock(workspaceId?: string | null): DockConfig {
    const candidates: Array<{
      source: DockConfig["source"];
      key: string | null;
      path: string;
      requiresTrust: boolean;
    }> = [
      {
        source: "project_agentmux",
        key: this.previewProjectDockStorageKey(workspaceId),
        path: this.previewDockPath(workspaceId, "project_agentmux"),
        requiresTrust: true,
      },
      {
        source: "project_cmux",
        key: this.previewCmuxProjectDockStorageKey(workspaceId),
        path: this.previewDockPath(workspaceId, "project_cmux"),
        requiresTrust: true,
      },
      {
        source: "global_agentmux",
        key: this.dockStorageKey,
        path: this.previewDockPath(workspaceId, "global_agentmux"),
        requiresTrust: false,
      },
    ];

    for (const candidate of candidates) {
      const raw = candidate.key
        ? window.localStorage.getItem(candidate.key)
        : null;
      if (!raw) {
        continue;
      }
      try {
        const parsed = sanitizeDockConfig(JSON.parse(raw));
        return {
          source: candidate.source,
          configPath: candidate.path,
          requiresTrust: candidate.requiresTrust,
          trusted: candidate.requiresTrust
            ? this.readPreviewDockTrust(
                workspaceId,
                candidate.source,
                candidate.path,
              )
            : true,
          controls: parsed.controls,
        };
      } catch {
        continue;
      }
    }
    return {
      source: "none",
      configPath: null,
      requiresTrust: false,
      trusted: false,
      controls: [],
    };
  }

  private readPreviewProjectConfig(
    workspaceId?: string | null,
  ): Partial<AppConfig> | null {
    const key = this.previewProjectConfigStorageKey(workspaceId);
    if (!key) {
      return null;
    }
    const raw = window.localStorage.getItem(key);
    if (!raw) {
      return null;
    }
    try {
      return previewConfigFromImport(JSON.parse(raw));
    } catch {
      return null;
    }
  }

  private readPreviewConfig(workspaceId?: string | null): AppConfig {
    const projectConfigPath = this.previewProjectConfigPath(workspaceId);
    const fallback: AppConfig = {
      formatVersion: "agentmux.config.v1",
      configPath: "localStorage://agentmux.preview.config.v1",
      projectConfigPath,
      projectConfigLoaded: false,
      appearance: {
        theme: "dark",
        accentKey: "blue",
        fontSize: 12.5,
      },
      shortcuts: {
        bindings: {},
      },
      actions: {
        custom: [],
      },
      ui: {
        workspacePlusAction: null,
        surfaceTabPlusAction: null,
        surfaceTabActions: null,
        textBoxMaxLines: null,
        terminalInnerMargin: null,
      },
      notifications: {
        actions: [],
      },
    };
    const raw = window.localStorage.getItem(this.configStorageKey);
    if (!raw) {
      return mergePreviewProjectConfig(
        fallback,
        this.readPreviewProjectConfig(workspaceId),
      );
    }
    try {
      const parsed = JSON.parse(raw) as Partial<AppConfig>;
      const appearance: Partial<AppConfigAppearance> = parsed.appearance ?? {};
      const shortcuts = parsed.shortcuts ?? fallback.shortcuts;
      const actions = parsed.actions ?? fallback.actions;
      const ui = parsed.ui ?? fallback.ui;
      const notifications = parsed.notifications ?? fallback.notifications;
      const base: AppConfig = {
        formatVersion: parsed.formatVersion ?? fallback.formatVersion,
        configPath: parsed.configPath ?? fallback.configPath,
        projectConfigPath:
          projectConfigPath ??
          parsed.projectConfigPath ??
          fallback.projectConfigPath,
        projectConfigLoaded: false,
        appearance: {
          theme: appearance.theme === "light" ? "light" : "dark",
          accentKey:
            typeof appearance.accentKey === "string" &&
            appearance.accentKey.trim()
              ? appearance.accentKey
              : fallback.appearance.accentKey,
          fontSize:
            typeof appearance.fontSize === "number" &&
            Number.isFinite(appearance.fontSize)
              ? Math.min(16, Math.max(11, appearance.fontSize))
              : fallback.appearance.fontSize,
        },
        shortcuts: {
          bindings: sanitizeShortcutBindings(shortcuts.bindings),
        },
        actions: {
          custom: sanitizeCustomActions(actions.custom),
        },
        ui: sanitizeAppConfigUi(ui),
        notifications: {
          actions: sanitizeNotificationActions(notifications.actions),
        },
      };
      return mergePreviewProjectConfig(
        base,
        this.readPreviewProjectConfig(workspaceId),
      );
    } catch {
      return mergePreviewProjectConfig(
        fallback,
        this.readPreviewProjectConfig(workspaceId),
      );
    }
  }

  private applySyntheticAgentState(
    detail: SyntheticAgentStateDetail = {},
  ): void {
    const sessionId = detail.sessionId ?? this.lastSessionId;
    const session = sessionId ? this.sessions.get(sessionId) : undefined;
    const surface = sessionId
      ? this.terminalSurfaces.find(
          (candidate) => candidate.sessionId === sessionId,
        )
      : undefined;
    const workspaceId = detail.workspaceId ?? surface?.workspaceId;
    const workspace = workspaceId
      ? this.findWorkspace(workspaceId)
      : (this.workspaces[0] ?? null);
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
      telemetry: detail.telemetry ?? null,
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
    const severity =
      state === "failed" ? "error" : state === "completed" ? "info" : "warning";
    const title =
      state === "waiting_for_input"
        ? "Agent needs input"
        : state === "completed"
          ? "Agent completed"
          : "Agent failed";
    const notificationId =
      detail.notificationId ??
      `not_browser_preview_${this.notifications.length + 1}`;
    if (
      this.notifications.some(
        (notification) => notification.notificationId === notificationId,
      )
    ) {
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
      dismissed: false,
    });
  }

  private applySyntheticSidebarState(
    detail: SyntheticSidebarStateDetail = {},
  ): void {
    const workspace = detail.workspaceId
      ? this.findWorkspace(detail.workspaceId)
      : this.workspaces[0];
    if (!workspace) {
      return;
    }
    const now = new Date().toISOString();
    const current = this.readSidebarState(workspace.workspaceId);
    this.sidebarStates.set(workspace.workspaceId, {
      workspaceId: workspace.workspaceId,
      cwd: detail.cwd ?? current.cwd ?? workspace.projectRoot ?? null,
      gitBranch: detail.gitBranch ?? current.gitBranch ?? null,
      gitHash: detail.gitHash ?? current.gitHash ?? null,
      ports: detail.ports ?? current.ports,
      statuses:
        detail.statuses?.map((status, index) => ({
          workspaceId: workspace.workspaceId,
          key: status.key,
          label: status.label,
          icon: status.icon ?? null,
          color: status.color ?? null,
          priority: status.priority ?? 0,
          updatedAt: status.updatedAt ?? now,
        })) ?? current.statuses,
      progress:
        detail.progress === null
          ? null
          : detail.progress
            ? {
                workspaceId: workspace.workspaceId,
                value: Math.min(1, Math.max(0, detail.progress.value)),
                label: detail.progress.label ?? null,
                updatedAt: detail.progress.updatedAt ?? now,
              }
            : current.progress,
      logs:
        detail.logs?.map((log, index) => ({
          logId: log.logId ?? `preview_log_${Date.now()}_${index}`,
          workspaceId: workspace.workspaceId,
          level: log.level ?? "info",
          source: log.source ?? null,
          message: log.message,
          createdAt: log.createdAt ?? now,
        })) ?? current.logs,
    });
  }
}

interface ServerApiEnvelope<T> {
  ok: boolean;
  result?: T;
  error?: string;
}

interface ServerStateResult {
  mode: string;
  control_pipe?: string | null;
  default_workspace_id?: string | null;
  workspaces: WorkspaceSummaryWire[];
  sessions: SessionSummaryWire[];
  defaults: NonNullable<AgentmuxServerBootstrap["defaults"]>;
}

class ServerControlClient extends BrowserPreviewControlClient {
  private readonly serverBaseUrl: string;
  private readonly serverDefaults: NonNullable<AgentmuxServerBootstrap["defaults"]>;
  private readonly serverWorkspaces = new Map<string, WorkspaceSummary>();
  private readonly serverPanes = new Map<string, PaneSummary[]>();
  private readonly serverSurfaces = new Map<string, SurfaceSummary[]>();
  private readonly serverSessions = new Map<string, TerminalSession>();
  private serverWorkspaceCounter = 0;
  private serverPaneCounter = 0;

  constructor(bootstrap: AgentmuxServerBootstrap) {
    super();
    this.serverBaseUrl = (bootstrap.baseUrl ?? "").replace(/\/+$/, "");
    this.serverDefaults = bootstrap.defaults ?? {};
  }

  async listWorkspaces(): Promise<WorkspaceSummary[]> {
    await this.hydrateServerState();
    return [...this.serverWorkspaces.values()];
  }

  async createWorkspace(
    name: string,
    projectRoot?: string | null,
  ): Promise<WorkspaceSummary> {
    const suffix = ++this.serverWorkspaceCounter;
    const workspaceId = `ws_server_local_${suffix}`;
    const rootPaneId = `pane_server_local_${suffix}`;
    const workspace: WorkspaceSummary = {
      workspaceId,
      name: name.trim() || `Workspace ${suffix}`,
      rootPaneId,
      activePaneId: rootPaneId,
      projectRoot: projectRoot ?? this.serverDefaults.cwd ?? null,
      environmentProfileId: null,
      description: null,
      icon: null,
      color: null,
      defaultWslDistribution: this.serverDefaults.backend_profile ?? null,
      defaultAgentCommand: null,
    };
    this.serverWorkspaces.set(workspaceId, workspace);
    this.ensureServerPaneRoot(workspace);
    return { ...workspace };
  }

  async getWorkspace(workspaceId: string): Promise<WorkspaceDetail> {
    await this.hydrateServerState();
    const workspace = this.findServerWorkspace(workspaceId);
    const sessionsResult = await this.serverApi<{ sessions: SessionSummaryWire[] }>(
      `/api/sessions?workspace=${encodeURIComponent(workspaceId)}`,
    );
    const sessions = sessionsResult.sessions.map(mapSession);
    const surfaces = this.syncServerSessions(workspaceId, sessions);
    return {
      workspace: { ...workspace },
      panes: this.serverPanes.get(workspaceId)?.map((pane) => ({ ...pane })) ?? [],
      surfaces: surfaces.map((surface) => ({ ...surface })),
      sessions: sessions.map((session) => ({ ...session })),
    };
  }

  async renameWorkspace(
    workspaceId: string,
    name: string,
  ): Promise<WorkspaceSummary> {
    const workspace = this.findServerWorkspace(workspaceId);
    workspace.name = name.trim() || workspace.name;
    return { ...workspace };
  }

  async updateWorkspace(
    workspaceId: string,
    input: WorkspaceUpdateInput,
  ): Promise<WorkspaceSummary> {
    const workspace = this.findServerWorkspace(workspaceId);
    workspace.name = input.name.trim() || workspace.name;
    workspace.projectRoot = input.projectRoot ?? workspace.projectRoot ?? null;
    workspace.environmentProfileId = input.environmentProfileId ?? null;
    workspace.description = input.description ?? null;
    workspace.icon = input.icon ?? null;
    workspace.color = input.color ?? null;
    workspace.defaultWslDistribution =
      input.defaultWslDistribution ?? workspace.defaultWslDistribution ?? null;
    workspace.defaultAgentCommand = input.defaultAgentCommand ?? null;
    return { ...workspace };
  }

  async closeWorkspace(
    workspaceId: string,
    closePolicy = "terminate_sessions",
  ): Promise<boolean> {
    if (!this.serverWorkspaces.has(workspaceId)) {
      return false;
    }
    const detail = await this.getWorkspace(workspaceId);
    const sessionSurfaces = detail.surfaces.filter((surface) => surface.sessionId);
    if (closePolicy === "fail_if_running" && sessionSurfaces.length > 0) {
      throw new ControlClientError(
        "Workspace has running sessions.",
        "conflict",
      );
    }
    if (closePolicy !== "detach_sessions") {
      await Promise.all(
        sessionSurfaces.map((surface) =>
          surface.sessionId
            ? this.terminateServerSession(surface.sessionId).catch(() => undefined)
            : Promise.resolve(),
        ),
      );
    }
    this.serverWorkspaces.delete(workspaceId);
    this.serverPanes.delete(workspaceId);
    this.serverSurfaces.delete(workspaceId);
    return true;
  }

  async splitPane(
    workspaceId: string,
    paneId: string,
    axis: "horizontal" | "vertical",
    ratio = 0.5,
  ): Promise<WorkspaceDetail> {
    const workspace = this.findServerWorkspace(workspaceId);
    const panes = this.serverPanes.get(workspaceId) ?? [];
    const target = panes.find((pane) => pane.paneId === paneId);
    if (!target || target.kind !== "leaf") {
      throw new Error(`Pane '${paneId}' cannot be split.`);
    }

    const firstChildId = `${paneId}_a_${++this.serverPaneCounter}`;
    const secondChildId = `${paneId}_b_${this.serverPaneCounter}`;
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
        mountedSurfaceId,
      },
      {
        paneId: secondChildId,
        workspaceId,
        parentPaneId: paneId,
        kind: "leaf",
        splitAxis: null,
        splitRatio: null,
        mountedSurfaceId: null,
      },
    );
    workspace.activePaneId = firstChildId;
    return this.getWorkspace(workspaceId);
  }

  async focusPane(
    workspaceId: string,
    paneId: string,
  ): Promise<WorkspaceDetail> {
    const workspace = this.findServerWorkspace(workspaceId);
    const pane = this.serverPanes
      .get(workspaceId)
      ?.find((candidate) => candidate.paneId === paneId);
    if (!pane || pane.kind !== "leaf") {
      throw new Error(`Pane '${paneId}' cannot be focused.`);
    }
    workspace.activePaneId = paneId;
    workspace.rootPaneId =
      this.findServerRootPaneId(workspaceId, paneId) ?? workspace.rootPaneId;
    return this.getWorkspace(workspaceId);
  }

  async closePane(
    workspaceId: string,
    paneId: string,
    surfacePolicy: string,
  ): Promise<WorkspaceDetail> {
    const workspace = this.findServerWorkspace(workspaceId);
    const panes = this.serverPanes.get(workspaceId) ?? [];
    const target = panes.find((pane) => pane.paneId === paneId);
    if (!target || target.kind !== "leaf" || !target.parentPaneId) {
      throw new Error(`Pane '${paneId}' cannot be closed.`);
    }
    const parent = panes.find((pane) => pane.paneId === target.parentPaneId);
    const sibling = panes.find(
      (pane) => pane.parentPaneId === target.parentPaneId && pane.paneId !== paneId,
    );
    const closedSurfaceId = target.mountedSurfaceId ?? null;
    this.serverPanes.set(
      workspaceId,
      panes.filter(
        (pane) => pane.paneId !== paneId && pane.paneId !== sibling?.paneId,
      ),
    );
    if (parent && sibling) {
      parent.kind = sibling.kind;
      parent.splitAxis = sibling.splitAxis;
      parent.splitRatio = sibling.splitRatio;
      parent.mountedSurfaceId = sibling.mountedSurfaceId;
      for (const pane of this.serverPanes.get(workspaceId) ?? []) {
        if (pane.parentPaneId === sibling.paneId) {
          pane.parentPaneId = parent.paneId;
        }
      }
      workspace.activePaneId = parent.paneId;
    }
    if (closedSurfaceId && surfacePolicy === "close_surface") {
      await this.removeServerSurface(workspaceId, closedSurfaceId, true);
    }
    return this.getWorkspace(workspaceId);
  }

  async resizePaneLayout(
    workspaceId: string,
    paneId: string,
    ratio: number,
  ): Promise<WorkspaceDetail> {
    const pane = this.serverPanes
      .get(workspaceId)
      ?.find((candidate) => candidate.paneId === paneId);
    if (!pane || pane.kind !== "split") {
      throw new Error(`Pane '${paneId}' cannot be resized.`);
    }
    pane.splitRatio = Math.min(0.9, Math.max(0.1, ratio));
    return this.getWorkspace(workspaceId);
  }

  async mountSurface(
    workspaceId: string,
    paneId: string,
    surfaceId: string,
  ): Promise<WorkspaceDetail> {
    this.mountServerSurface(workspaceId, surfaceId, paneId);
    return this.focusPane(workspaceId, paneId);
  }

  async unmountSurface(
    workspaceId: string,
    paneId: string,
  ): Promise<WorkspaceDetail> {
    const pane = this.serverPanes
      .get(workspaceId)
      ?.find((candidate) => candidate.paneId === paneId);
    if (pane) {
      pane.mountedSurfaceId = null;
    }
    return this.getWorkspace(workspaceId);
  }

  async closeSurface(
    workspaceId: string,
    surfaceId: string,
  ): Promise<WorkspaceDetail> {
    await this.removeServerSurface(workspaceId, surfaceId, true);
    return this.getWorkspace(workspaceId);
  }

  async createBrowserSurface(): Promise<SurfaceSummary> {
    throw new Error("Browser surfaces are not available in server mode yet.");
  }

  async spawnNativeTerminal(
    workspaceId: string,
    command: string[],
    placement: TerminalPlacement = "active_pane",
    paneId?: string | null,
  ): Promise<TerminalSession> {
    return this.spawnServerTerminal({
      workspaceId,
      backend: "conpty",
      backendProfile: null,
      command,
      cwd: null,
      placement,
      paneId,
      title: command.join(" ") || "ConPTY",
    });
  }

  async spawnWslTerminal(
    workspaceId: string,
    distribution: string | null,
    cwd: string | null,
    placement: TerminalPlacement = "active_pane",
    paneId?: string | null,
  ): Promise<TerminalSession> {
    return this.spawnServerTerminal({
      workspaceId,
      backend: "wsl-direct",
      backendProfile: distribution ?? this.serverDefaults.backend_profile ?? null,
      command: ["bash", "-l"],
      cwd: cwd ?? this.serverDefaults.cwd ?? null,
      placement,
      paneId,
      title: distribution ? `WSL ${distribution}` : "WSL",
    });
  }

  async spawnDurableWslTerminal(
    workspaceId: string,
    distribution: string | null,
    cwd: string | null,
    placement: TerminalPlacement = "active_pane",
    paneId?: string | null,
  ): Promise<TerminalSession> {
    // Server mode doesn't model durability separately yet — fall back to a
    // regular WSL terminal so the call still resolves.
    return this.spawnWslTerminal(workspaceId, distribution, cwd, placement, paneId);
  }

  async spawnDockTerminal(
    workspaceId: string,
    control: DockControl,
    distribution: string | null,
    cwd: string | null,
    placement: SessionPlacement = "dock",
  ): Promise<TerminalSession> {
    return this.spawnServerTerminal({
      workspaceId,
      backend: "wsl-direct",
      backendProfile: distribution ?? this.serverDefaults.backend_profile ?? null,
      command: ["bash", "-lc", control.command],
      cwd: cwd ?? this.serverDefaults.cwd ?? null,
      placement,
      paneId: null,
      title: control.title,
      surfaceType: "dock-terminal",
      browserId: control.id,
    });
  }

  async spawnSshTerminal(
    workspaceId: string,
    target: string,
    placement: TerminalPlacement = "active_pane",
    paneId?: string | null,
  ): Promise<TerminalSession> {
    return this.spawnServerTerminal({
      workspaceId,
      backend: "ssh",
      backendProfile: target,
      command: [],
      cwd: null,
      placement,
      paneId,
      title: `SSH ${target}`,
    });
  }

  async spawnAgentTerminal(
    workspaceId: string,
    command: string[],
    distribution: string | null,
    placement: TerminalPlacement = "active_pane",
    paneId?: string | null,
  ): Promise<TerminalSession> {
    return this.spawnServerTerminal({
      workspaceId,
      backend: "wsl-tmux-control",
      backendProfile: distribution ?? this.serverDefaults.backend_profile ?? null,
      command,
      cwd: this.serverDefaults.cwd ?? null,
      placement,
      paneId,
      title: command.join(" ") || "Agent",
    });
  }

  async readRecent(sessionId: string, maxBytes: number): Promise<string> {
    const result = await this.serverApi<{ text: string }>(
      `/api/session/${encodeURIComponent(sessionId)}/recent?max_bytes=${maxBytes}`,
    );
    return result.text ?? "";
  }

  async getSession(sessionId: string): Promise<TerminalSession> {
    for (const workspace of await this.listWorkspaces()) {
      const detail = await this.getWorkspace(workspace.workspaceId);
      const session = detail.sessions.find((candidate) => candidate.sessionId === sessionId);
      if (session) {
        return session;
      }
    }
    return {
      sessionId,
      backendKind: "unknown",
      state: "lost",
    };
  }

  async sendText(sessionId: string, text: string): Promise<void> {
    await this.serverApi(`/api/session/${encodeURIComponent(sessionId)}/send`, {
      method: "POST",
      body: JSON.stringify({ text }),
    });
  }

  async sendKey(sessionId: string, key: string): Promise<void> {
    await this.serverApi(`/api/session/${encodeURIComponent(sessionId)}/key`, {
      method: "POST",
      body: JSON.stringify({ key }),
    });
  }

  async resize(
    sessionId: string,
    columns: number,
    rows: number,
  ): Promise<void> {
    await this.serverApi(`/api/session/${encodeURIComponent(sessionId)}/resize`, {
      method: "POST",
      body: JSON.stringify({ columns, rows }),
    });
  }

  async recoveryDiagnostics(): Promise<RecoveryDiagnostics> {
    const workspaces = await this.listWorkspaces();
    const details = await Promise.all(
      workspaces.map((workspace) => this.getWorkspace(workspace.workspaceId)),
    );
    const sessions = details.flatMap((detail) =>
      detail.sessions.map((session) => ({
        sessionId: session.sessionId,
        workspaceId: detail.workspace.workspaceId,
        backendKind: session.backendKind,
        state: session.state,
        durability: "ephemeral",
        backendNativeId: session.backendNativeId ?? null,
      })),
    );
    return {
      workspaceCount: workspaces.length,
      paneCount: details.reduce((count, detail) => count + detail.panes.length, 0),
      surfaceCount: details.reduce(
        (count, detail) => count + detail.surfaces.length,
        0,
      ),
      sessionCount: sessions.length,
      sessions,
    };
  }

  async listWslDistributions(): Promise<WslDistribution[]> {
    const result = await this.serverApi<{ distributions: WslDistributionWire[] }>(
      "/api/wsl/distributions",
    );
    return result.distributions.map(mapWslDistribution);
  }

  async checkTmux(distribution?: string | null): Promise<TmuxDiagnostics> {
    const result = await this.serverApi<TmuxDiagnosticsWire>("/api/tmux/check", {
      method: "POST",
      body: JSON.stringify({ distribution: distribution ?? null }),
    });
    return mapTmuxDiagnostics(result);
  }

  async getSidebarState(workspaceId?: string | null): Promise<SidebarState> {
    await this.hydrateServerState();
    const workspace = workspaceId
      ? this.findServerWorkspace(workspaceId)
      : [...this.serverWorkspaces.values()][0];
    return {
      workspaceId: workspace?.workspaceId ?? workspaceId ?? "ws_server",
      cwd: workspace?.projectRoot ?? this.serverDefaults.cwd ?? null,
      gitBranch: null,
      gitHash: null,
      ports: [],
      statuses: [],
      progress: null,
      logs: [],
    };
  }

  private async hydrateServerState(): Promise<ServerStateResult> {
    const state = await this.serverApi<ServerStateResult>("/api/state");
    for (const wire of state.workspaces ?? []) {
      this.ensureServerWorkspace(mapWorkspace(wire));
    }
    return state;
  }

  private ensureServerWorkspace(workspace: WorkspaceSummary): WorkspaceSummary {
    const existing = this.serverWorkspaces.get(workspace.workspaceId);
    const next = existing
      ? {
          ...existing,
          ...workspace,
          rootPaneId: existing.rootPaneId || workspace.rootPaneId,
          activePaneId: existing.activePaneId || workspace.activePaneId,
        }
      : workspace;
    next.defaultWslDistribution =
      next.defaultWslDistribution ?? this.serverDefaults.backend_profile ?? null;
    this.serverWorkspaces.set(next.workspaceId, next);
    this.ensureServerPaneRoot(next);
    return next;
  }

  private ensureServerPaneRoot(workspace: WorkspaceSummary): void {
    if (this.serverPanes.has(workspace.workspaceId)) {
      return;
    }
    const rootPaneId = workspace.rootPaneId || `pane_server_${workspace.workspaceId}`;
    workspace.rootPaneId = rootPaneId;
    workspace.activePaneId = workspace.activePaneId || rootPaneId;
    this.serverPanes.set(workspace.workspaceId, [
      {
        paneId: rootPaneId,
        workspaceId: workspace.workspaceId,
        parentPaneId: null,
        kind: "leaf",
        splitAxis: null,
        splitRatio: null,
        mountedSurfaceId: null,
      },
    ]);
  }

  private findServerWorkspace(workspaceId: string): WorkspaceSummary {
    const workspace = this.serverWorkspaces.get(workspaceId);
    if (!workspace) {
      throw new Error(`Workspace '${workspaceId}' was not found.`);
    }
    return workspace;
  }

  private syncServerSessions(
    workspaceId: string,
    sessions: TerminalSession[],
  ): SurfaceSummary[] {
    for (const session of sessions) {
      this.serverSessions.set(session.sessionId, session);
    }
    const sessionIds = new Set(sessions.map((session) => session.sessionId));
    let surfaces = this.serverSurfaces.get(workspaceId) ?? [];
    surfaces = surfaces.filter(
      (surface) => !surface.sessionId || sessionIds.has(surface.sessionId),
    );
    for (const session of sessions) {
      if (!surfaces.some((surface) => surface.sessionId === session.sessionId)) {
        surfaces.push(this.createServerSurface(workspaceId, session));
      }
    }
    this.serverSurfaces.set(workspaceId, surfaces);
    this.clearMissingServerSurfaceMounts(workspaceId);
    if (
      surfaces.length > 0 &&
      !(this.serverPanes.get(workspaceId) ?? []).some((pane) => pane.mountedSurfaceId)
    ) {
      const workspace = this.findServerWorkspace(workspaceId);
      this.mountServerSurface(workspaceId, surfaces[0].surfaceId, workspace.activePaneId);
    }
    return surfaces;
  }

  private createServerSurface(
    workspaceId: string,
    session: TerminalSession,
    surfaceType = "terminal",
    title = session.backendKind,
    browserId?: string | null,
  ): SurfaceSummary {
    return {
      surfaceId: `surf_server_${session.sessionId.replace(/[^A-Za-z0-9_-]/g, "_")}`,
      workspaceId,
      surfaceType,
      title,
      sessionId: session.sessionId,
      browserId: browserId ?? null,
    };
  }

  private async spawnServerTerminal(options: {
    workspaceId: string;
    backend: string;
    backendProfile: string | null;
    command: string[];
    cwd: string | null;
    placement: SessionPlacement;
    paneId?: string | null;
    title: string;
    surfaceType?: string;
    browserId?: string | null;
  }): Promise<TerminalSession> {
    await this.hydrateServerState();
    this.findServerWorkspace(options.workspaceId);
    const result = await this.serverApi<{ session_id: string }>("/api/spawn", {
      method: "POST",
      body: JSON.stringify({
        workspace_id: options.workspaceId,
        backend: options.backend,
        backend_profile: options.backendProfile,
        command: options.command,
        cwd: options.cwd,
      }),
    });
    const session: TerminalSession = {
      sessionId: result.session_id,
      backendKind: options.backend,
      state: "running",
    };
    this.serverSessions.set(session.sessionId, session);
    const surfaces = this.serverSurfaces.get(options.workspaceId) ?? [];
    const surface = this.createServerSurface(
      options.workspaceId,
      session,
      options.surfaceType ?? "terminal",
      options.title,
      options.browserId,
    );
    surfaces.push(surface);
    this.serverSurfaces.set(options.workspaceId, surfaces);
    if (options.placement !== "dock") {
      if (options.placement === "new_tab") {
        this.createServerTabPane(options.workspaceId, surface.surfaceId);
      } else {
        this.mountServerSurface(options.workspaceId, surface.surfaceId, options.paneId);
      }
    }
    return session;
  }

  private createServerTabPane(workspaceId: string, surfaceId: string): void {
    const workspace = this.findServerWorkspace(workspaceId);
    const paneId = `pane_server_tab_${++this.serverPaneCounter}`;
    const panes = this.serverPanes.get(workspaceId) ?? [];
    panes.push({
      paneId,
      workspaceId,
      parentPaneId: null,
      kind: "leaf",
      splitAxis: null,
      splitRatio: null,
      mountedSurfaceId: surfaceId,
    });
    this.serverPanes.set(workspaceId, panes);
    workspace.rootPaneId = paneId;
    workspace.activePaneId = paneId;
  }

  private mountServerSurface(
    workspaceId: string,
    surfaceId: string,
    paneId?: string | null,
  ): void {
    const workspace = this.findServerWorkspace(workspaceId);
    const panes = this.serverPanes.get(workspaceId) ?? [];
    const target =
      panes.find((pane) => pane.paneId === (paneId ?? workspace.activePaneId)) ??
      panes.find((pane) => pane.kind === "leaf");
    if (!target) {
      return;
    }
    for (const pane of panes) {
      if (pane.mountedSurfaceId === surfaceId) {
        pane.mountedSurfaceId = null;
      }
    }
    target.mountedSurfaceId = surfaceId;
    workspace.activePaneId = target.paneId;
    workspace.rootPaneId =
      this.findServerRootPaneId(workspaceId, target.paneId) ?? workspace.rootPaneId;
  }

  private async removeServerSurface(
    workspaceId: string,
    surfaceId: string,
    terminate: boolean,
  ): Promise<void> {
    const surfaces = this.serverSurfaces.get(workspaceId) ?? [];
    const surface = surfaces.find((candidate) => candidate.surfaceId === surfaceId);
    if (terminate && surface?.sessionId) {
      await this.terminateServerSession(surface.sessionId).catch(() => undefined);
    }
    this.serverSurfaces.set(
      workspaceId,
      surfaces.filter((candidate) => candidate.surfaceId !== surfaceId),
    );
    for (const pane of this.serverPanes.get(workspaceId) ?? []) {
      if (pane.mountedSurfaceId === surfaceId) {
        pane.mountedSurfaceId = null;
      }
    }
  }

  private async terminateServerSession(sessionId: string): Promise<void> {
    await this.serverApi(`/api/session/${encodeURIComponent(sessionId)}/terminate`, {
      method: "POST",
      body: "{}",
    });
    this.serverSessions.delete(sessionId);
  }

  private clearMissingServerSurfaceMounts(workspaceId: string): void {
    const surfaceIds = new Set(
      (this.serverSurfaces.get(workspaceId) ?? []).map((surface) => surface.surfaceId),
    );
    for (const pane of this.serverPanes.get(workspaceId) ?? []) {
      if (pane.mountedSurfaceId && !surfaceIds.has(pane.mountedSurfaceId)) {
        pane.mountedSurfaceId = null;
      }
    }
  }

  private findServerRootPaneId(workspaceId: string, paneId: string): string | null {
    const panes = this.serverPanes.get(workspaceId) ?? [];
    let pane = panes.find((candidate) => candidate.paneId === paneId);
    let guard = 0;
    while (pane?.parentPaneId && guard < 100) {
      const parentPaneId = pane.parentPaneId;
      pane = panes.find((candidate) => candidate.paneId === parentPaneId);
      guard += 1;
    }
    return pane?.paneId ?? null;
  }

  private async serverApi<T = unknown>(
    path: string,
    options: RequestInit = {},
  ): Promise<T> {
    const headers = new Headers(options.headers);
    if (options.body) {
      headers.set("Content-Type", "application/json");
    }
    const response = await fetch(`${this.serverBaseUrl}${path}`, {
      ...options,
      headers,
    });
    const text = await response.text();
    let data: ServerApiEnvelope<T>;
    try {
      data = JSON.parse(text) as ServerApiEnvelope<T>;
    } catch {
      throw new Error(text || response.statusText);
    }
    if (!response.ok || data.ok === false) {
      throw new Error(data.error || response.statusText);
    }
    return data.result as T;
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

interface SyntheticSidebarStateDetail {
  workspaceId?: string;
  cwd?: string | null;
  gitBranch?: string | null;
  gitHash?: string | null;
  ports?: string[];
  statuses?: Array<Partial<SidebarStatus> & { key: string; label: string }>;
  progress?: (Partial<SidebarProgress> & { value: number }) | null;
  logs?: Array<Partial<SidebarLogEntry> & { message: string }>;
}

interface BrowserPreviewApi {
  syntheticAgentState(detail?: SyntheticAgentStateDetail): void;
  sidebarState(detail?: SyntheticSidebarStateDetail): void;
  browserUrl(surfaceId?: string): string | null;
  browserActions(): string[];
  terminalOutput(sessionId?: string): string | null;
}

interface WorkspaceSummaryWire {
  workspace_id: string;
  name: string;
  root_pane_id: string;
  active_pane_id: string;
  project_root?: string | null;
  environment_profile_id?: string | null;
  description?: string | null;
  icon?: string | null;
  color?: string | null;
  default_wsl_distribution?: string | null;
  default_agent_command?: string | null;
}

interface WorkspaceGroupMemberWire {
  workspace_id: string;
  position: number;
}

interface WorkspaceGroupWire {
  group_id: string;
  name: string;
  anchor_workspace_id?: string | null;
  collapsed: boolean;
  pinned: boolean;
  color?: string | null;
  icon?: string | null;
  sort_order: number;
  created_at: string;
  updated_at: string;
  members: WorkspaceGroupMemberWire[];
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

interface SidebarStatusWire {
  workspace_id: string;
  key: string;
  label: string;
  icon?: string | null;
  color?: string | null;
  priority: number;
  updated_at: string;
}

interface SidebarProgressWire {
  workspace_id: string;
  value: number;
  label?: string | null;
  updated_at: string;
}

interface SidebarLogWire {
  log_id: string;
  workspace_id: string;
  level: string;
  source?: string | null;
  message: string;
  created_at: string;
}

interface SidebarStateWire {
  workspace_id: string;
  cwd?: string | null;
  git_branch?: string | null;
  git_hash?: string | null;
  ports: string[];
  statuses: SidebarStatusWire[];
  progress?: SidebarProgressWire | null;
  logs: SidebarLogWire[];
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

interface TmuxDiagnosticsWire {
  available: boolean;
  distribution?: string | null;
  version?: string | null;
  message: string;
}

interface AppConfigWire {
  format_version: string;
  config_path: string;
  project_config_path?: string | null;
  project_config_loaded?: boolean;
  appearance: {
    theme: string;
    accent_key: string;
    font_size: number;
  };
  shortcuts?: {
    bindings?: Record<string, ShortcutBindingValue>;
  };
  actions?: {
    custom?: AppConfigCustomActionWire[];
  };
  ui?: {
    workspace_plus_action?: string | null;
    surface_tab_plus_action?: string | null;
    surface_tab_actions?: string[] | null;
    text_box_max_lines?: number | null;
    terminal_inner_margin?: number | null;
  };
  notifications?: {
    actions?: AppConfigNotificationActionWire[];
  };
}

interface AppConfigExportWire {
  json: string;
  config: AppConfigWire;
}

interface AppConfigMigrationWire {
  source_path: string;
  target_path: string;
  overwritten: boolean;
  config: AppConfigWire;
}

interface AppConfigDiagnosticsWire {
  entries: AppConfigDiagnosticEntryWire[];
}

interface DockConfigWire {
  source: string;
  config_path?: string | null;
  requires_trust?: boolean;
  trusted?: boolean;
  controls?: DockControlWire[];
}

interface DockControlWire {
  id: string;
  title: string;
  command: string;
  cwd?: string | null;
  height?: number | null;
  env?: Record<string, string>;
}

interface AppConfigDiagnosticEntryWire {
  source: string;
  path?: string | null;
  exists: boolean;
  valid: boolean;
  active: boolean;
  message: string;
}

interface AppConfigCustomActionWire {
  id: string;
  title: string;
  group?: string | null;
  target: string;
  command?: string[];
  keywords?: string[];
}

interface AppConfigNotificationActionWire {
  action: string;
  label?: string | null;
  notification_type?: string | null;
  severity?: string | null;
  dismiss_on_run?: boolean | null;
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

interface BrowserGetResultWire {
  surface_id: string;
  selector: string;
  kind: string;
  value: string;
}

interface BrowserFindResultWire {
  surface_id: string;
  query: string;
  count: number;
  matches: string[];
}

interface BrowserWaitForSelectorResultWire {
  surface_id: string;
  selector: string;
  elapsed_ms: number;
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
    environmentProfileId: value.environment_profile_id,
    description: value.description,
    icon: value.icon,
    color: value.color,
    defaultWslDistribution: value.default_wsl_distribution,
    defaultAgentCommand: value.default_agent_command,
  };
}

function mapWorkspaceGroup(value: WorkspaceGroupWire): WorkspaceGroup {
  return {
    groupId: value.group_id,
    name: value.name,
    anchorWorkspaceId: value.anchor_workspace_id ?? null,
    collapsed: value.collapsed,
    pinned: value.pinned,
    color: value.color ?? null,
    icon: value.icon ?? null,
    sortOrder: value.sort_order,
    createdAt: value.created_at,
    updatedAt: value.updated_at,
    members: value.members.map((member) => ({
      workspaceId: member.workspace_id,
      position: member.position,
    })),
  };
}

function cloneWorkspaceGroup(group: WorkspaceGroup): WorkspaceGroup {
  return {
    ...group,
    members: group.members.map((member) => ({ ...member })),
  };
}

function browserFrameLog(frameId?: string | null): string {
  const frame = frameId?.trim();
  return frame ? `:frame=${frame}` : "";
}

function mapPane(value: PaneSummaryWire): PaneSummary {
  return {
    paneId: value.pane_id,
    workspaceId: value.workspace_id,
    parentPaneId: value.parent_pane_id,
    kind: value.kind,
    splitAxis: value.split_axis,
    splitRatio: value.split_ratio,
    mountedSurfaceId: value.mounted_surface_id,
  };
}

function mapSurface(value: SurfaceSummaryWire): SurfaceSummary {
  return {
    surfaceId: value.surface_id,
    workspaceId: value.workspace_id,
    surfaceType: value.surface_type,
    title: value.title,
    sessionId: value.session_id,
    browserId: value.browser_id,
  };
}

function mapSession(value: SessionSummaryWire): TerminalSession {
  return {
    sessionId: value.session_id,
    backendKind: value.backend_kind,
    state: value.state,
    backendNativeId: value.backend_native_id,
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
    telemetry: value.telemetry ?? null,
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
    dismissed: value.dismissed,
  };
}

function mapSidebarStatus(value: SidebarStatusWire): SidebarStatus {
  return {
    workspaceId: value.workspace_id,
    key: value.key,
    label: value.label,
    icon: value.icon,
    color: value.color,
    priority: value.priority,
    updatedAt: value.updated_at,
  };
}

function mapSidebarProgress(value: SidebarProgressWire): SidebarProgress {
  return {
    workspaceId: value.workspace_id,
    value: value.value,
    label: value.label,
    updatedAt: value.updated_at,
  };
}

function mapSidebarLog(value: SidebarLogWire): SidebarLogEntry {
  return {
    logId: value.log_id,
    workspaceId: value.workspace_id,
    level: value.level,
    source: value.source,
    message: value.message,
    createdAt: value.created_at,
  };
}

function mapSidebarState(value: SidebarStateWire): SidebarState {
  return {
    workspaceId: value.workspace_id,
    cwd: value.cwd,
    gitBranch: value.git_branch,
    gitHash: value.git_hash,
    ports: value.ports,
    statuses: value.statuses.map(mapSidebarStatus),
    progress: value.progress ? mapSidebarProgress(value.progress) : null,
    logs: value.logs.map(mapSidebarLog),
  };
}

function mapWslDistribution(value: WslDistributionWire): WslDistribution {
  return {
    name: value.name,
    isDefault: value.is_default,
  };
}

function mapTmuxDiagnostics(value: TmuxDiagnosticsWire): TmuxDiagnostics {
  return {
    available: value.available,
    distribution: value.distribution,
    version: value.version,
    message: value.message,
  };
}

function mapAppConfig(value: AppConfigWire): AppConfig {
  return {
    formatVersion: value.format_version,
    configPath: value.config_path,
    projectConfigPath: value.project_config_path ?? null,
    projectConfigLoaded: value.project_config_loaded ?? false,
    appearance: {
      theme: value.appearance.theme === "light" ? "light" : "dark",
      accentKey: value.appearance.accent_key,
      fontSize: value.appearance.font_size,
    },
    shortcuts: {
      bindings: sanitizeShortcutBindings(value.shortcuts?.bindings),
    },
    actions: {
      custom: sanitizeCustomActions(value.actions?.custom),
    },
    ui: sanitizeAppConfigUi({
      workspacePlusAction: value.ui?.workspace_plus_action ?? null,
      surfaceTabPlusAction: value.ui?.surface_tab_plus_action ?? null,
      surfaceTabActions: value.ui?.surface_tab_actions ?? null,
      textBoxMaxLines: value.ui?.text_box_max_lines ?? null,
      terminalInnerMargin: value.ui?.terminal_inner_margin ?? null,
    }),
    notifications: {
      actions: sanitizeNotificationActions(
        value.notifications?.actions?.map((action) => ({
          action: action.action,
          label: action.label,
          notificationType: action.notification_type,
          severity: action.severity,
          dismissOnRun: action.dismiss_on_run,
        })),
      ),
    },
  };
}

function mapAppConfigMigration(
  value: AppConfigMigrationWire,
): AppConfigMigration {
  return {
    sourcePath: value.source_path,
    targetPath: value.target_path,
    overwritten: value.overwritten,
    config: mapAppConfig(value.config),
  };
}

function mapAppConfigDiagnosticEntry(
  value: AppConfigDiagnosticEntryWire,
): AppConfigDiagnosticEntry {
  return {
    source: value.source,
    path: value.path ?? null,
    exists: value.exists,
    valid: value.valid,
    active: value.active,
    message: value.message,
  };
}

function mapDockConfig(value: DockConfigWire): DockConfig {
  const requiresTrust = value.requires_trust ?? false;
  return {
    source: value.source,
    configPath: value.config_path ?? null,
    requiresTrust,
    trusted: value.trusted ?? !requiresTrust,
    controls: sanitizeDockControls(value.controls),
  };
}

function appConfigExportSnapshot(
  config: AppConfig,
  scope: AppConfigScope,
): unknown {
  const shared = {
    shortcuts: config.shortcuts,
    actions: config.actions,
    ui: {
      workspace_plus_action: config.ui.workspacePlusAction ?? null,
      surface_tab_plus_action: config.ui.surfaceTabPlusAction ?? null,
      surface_tab_actions: config.ui.surfaceTabActions ?? null,
      text_box_max_lines: config.ui.textBoxMaxLines ?? null,
      terminal_inner_margin: config.ui.terminalInnerMargin ?? null,
    },
    notifications: {
      actions: config.notifications.actions.map((action) => ({
        action: action.action,
        label: action.label ?? null,
        notification_type: action.notificationType ?? null,
        severity: action.severity ?? null,
        dismiss_on_run: action.dismissOnRun ?? null,
      })),
    },
  };
  if (scope === "project") {
    return shared;
  }
  return {
    format_version: config.formatVersion,
    appearance: {
      theme: config.appearance.theme,
      accent_key: config.appearance.accentKey,
      font_size: config.appearance.fontSize,
    },
    ...shared,
  };
}

function previewProjectConfigExportSnapshot(
  config: Partial<AppConfig> | null,
): unknown {
  return {
    shortcuts: config?.shortcuts ?? { bindings: {} },
    actions: config?.actions ?? { custom: [] },
    ui: {
      workspace_plus_action: config?.ui?.workspacePlusAction ?? null,
      surface_tab_plus_action: config?.ui?.surfaceTabPlusAction ?? null,
      surface_tab_actions: config?.ui?.surfaceTabActions ?? null,
      text_box_max_lines: config?.ui?.textBoxMaxLines ?? null,
      terminal_inner_margin: config?.ui?.terminalInnerMargin ?? null,
    },
    notifications: {
      actions: (config?.notifications?.actions ?? []).map((action) => ({
        action: action.action,
        label: action.label ?? null,
        notification_type: action.notificationType ?? null,
        severity: action.severity ?? null,
        dismiss_on_run: action.dismissOnRun ?? null,
      })),
    },
  };
}

function mergePreviewProjectConfig(
  base: AppConfig,
  project: Partial<AppConfig> | null,
): AppConfig {
  if (!project) {
    return base;
  }
  return {
    ...base,
    projectConfigLoaded: true,
    shortcuts: {
      bindings: {
        ...base.shortcuts.bindings,
        ...sanitizeShortcutBindings(project.shortcuts?.bindings),
      },
    },
    actions: {
      custom: mergePreviewCustomActions(
        base.actions.custom,
        sanitizeCustomActions(project.actions?.custom),
      ),
    },
    ui: {
      workspacePlusAction:
        project.ui?.workspacePlusAction ?? base.ui.workspacePlusAction ?? null,
      surfaceTabPlusAction:
        project.ui?.surfaceTabPlusAction ??
        base.ui.surfaceTabPlusAction ??
        null,
      surfaceTabActions:
        project.ui?.surfaceTabActions ?? base.ui.surfaceTabActions ?? null,
      textBoxMaxLines:
        project.ui?.textBoxMaxLines ?? base.ui.textBoxMaxLines ?? null,
      terminalInnerMargin:
        project.ui?.terminalInnerMargin ?? base.ui.terminalInnerMargin ?? null,
    },
    notifications: {
      actions: mergePreviewNotificationActions(
        base.notifications.actions,
        sanitizeNotificationActions(project.notifications?.actions),
      ),
    },
  };
}

function mergePreviewCustomActions(
  base: AppConfigCustomAction[],
  project: AppConfigCustomAction[],
): AppConfigCustomAction[] {
  const actions = [...base];
  for (const action of project) {
    const index = actions.findIndex((candidate) => candidate.id === action.id);
    if (index >= 0) {
      actions[index] = action;
    } else {
      actions.push(action);
    }
  }
  return actions;
}

function mergePreviewNotificationActions(
  base: AppConfigNotificationAction[],
  project: AppConfigNotificationAction[],
): AppConfigNotificationAction[] {
  const actions = [...base];
  for (const action of project) {
    const index = actions.findIndex(
      (candidate) =>
        candidate.action === action.action &&
        candidate.notificationType === action.notificationType &&
        candidate.severity === action.severity,
    );
    if (index >= 0) {
      actions[index] = action;
    } else {
      actions.push(action);
    }
  }
  return actions;
}

function previewConfigFromImport(value: unknown): Partial<AppConfig> {
  if (!value || typeof value !== "object") {
    return {};
  }
  const raw = value as Record<string, unknown>;
  const appearance =
    raw.appearance && typeof raw.appearance === "object"
      ? (raw.appearance as Record<string, unknown>)
      : {};
  const ui =
    raw.ui && typeof raw.ui === "object"
      ? (raw.ui as Record<string, unknown>)
      : {};
  const notifications =
    raw.notifications && typeof raw.notifications === "object"
      ? (raw.notifications as Record<string, unknown>)
      : {};
  const hasSnakeCase =
    "format_version" in raw ||
    "project_config_loaded" in raw ||
    "project_config_path" in raw ||
    "accent_key" in appearance ||
    "font_size" in appearance ||
    "workspace_plus_action" in ui ||
    "surface_tab_plus_action" in ui ||
    "surface_tab_actions" in ui ||
    "text_box_max_lines" in ui ||
    "terminal_inner_margin" in ui;
  if (!hasSnakeCase) {
    return raw as Partial<AppConfig>;
  }
  return {
    formatVersion:
      typeof raw.format_version === "string"
        ? raw.format_version
        : "agentmux.config.v1",
    configPath: "localStorage://agentmux.preview.config.v1",
    projectConfigPath:
      typeof raw.project_config_path === "string"
        ? raw.project_config_path
        : null,
    projectConfigLoaded:
      typeof raw.project_config_loaded === "boolean"
        ? raw.project_config_loaded
        : false,
    appearance: {
      theme: appearance.theme === "light" ? "light" : "dark",
      accentKey:
        typeof appearance.accent_key === "string"
          ? appearance.accent_key
          : "blue",
      fontSize:
        typeof appearance.font_size === "number" ? appearance.font_size : 12.5,
    },
    shortcuts: raw.shortcuts as AppConfigShortcuts | undefined,
    actions: raw.actions as AppConfigActions | undefined,
    ui: {
      workspacePlusAction:
        typeof ui.workspace_plus_action === "string"
          ? ui.workspace_plus_action
          : null,
      surfaceTabPlusAction:
        typeof ui.surface_tab_plus_action === "string"
          ? ui.surface_tab_plus_action
          : null,
      surfaceTabActions: Array.isArray(ui.surface_tab_actions)
        ? (ui.surface_tab_actions as string[])
        : null,
      textBoxMaxLines:
        typeof ui.text_box_max_lines === "number"
          ? ui.text_box_max_lines
          : null,
      terminalInnerMargin:
        typeof ui.terminal_inner_margin === "number"
          ? ui.terminal_inner_margin
          : null,
    },
    notifications: {
      actions: Array.isArray(notifications.actions)
        ? notifications.actions.map((item) => {
            const action =
              item && typeof item === "object"
                ? (item as Record<string, unknown>)
                : {};
            return {
              action: typeof action.action === "string" ? action.action : "",
              label: typeof action.label === "string" ? action.label : null,
              notificationType:
                typeof action.notification_type === "string"
                  ? action.notification_type
                  : null,
              severity:
                typeof action.severity === "string" ? action.severity : null,
              dismissOnRun:
                typeof action.dismiss_on_run === "boolean"
                  ? action.dismiss_on_run
                  : null,
            };
          })
        : [],
    },
  };
}

function sanitizeShortcutBindings(
  bindings: Record<string, unknown> | undefined,
): Record<string, ShortcutBindingValue> {
  const sanitized: Record<string, ShortcutBindingValue> = {};
  if (!bindings) {
    return sanitized;
  }
  for (const [actionId, value] of Object.entries(bindings)) {
    if (!actionId.trim()) {
      continue;
    }
    if (value === null) {
      sanitized[actionId] = null;
    } else if (typeof value === "string") {
      sanitized[actionId] = value;
    } else if (
      Array.isArray(value) &&
      value.length === 2 &&
      typeof value[0] === "string" &&
      typeof value[1] === "string"
    ) {
      sanitized[actionId] = [value[0], value[1]];
    }
  }
  return sanitized;
}

function sanitizeCustomActions(values: unknown): AppConfigCustomAction[] {
  if (!Array.isArray(values)) {
    return [];
  }
  const actions: AppConfigCustomAction[] = [];
  const seen = new Set<string>();
  for (const value of values) {
    if (!value || typeof value !== "object") {
      continue;
    }
    const raw = value as Record<string, unknown>;
    const id = typeof raw.id === "string" ? raw.id.trim() : "";
    const title = typeof raw.title === "string" ? raw.title.trim() : "";
    const target = typeof raw.target === "string" ? raw.target.trim() : "";
    if (
      !isValidCustomActionId(id) ||
      !title ||
      seen.has(id) ||
      !isCustomActionTarget(target)
    ) {
      continue;
    }
    const group =
      typeof raw.group === "string" && raw.group.trim()
        ? raw.group.trim()
        : null;
    const command = Array.isArray(raw.command)
      ? raw.command
          .filter(
            (part): part is string =>
              typeof part === "string" && part.trim() !== "",
          )
          .map((part) => part.trim())
      : [];
    if (target === "agent" && command.length === 0) {
      continue;
    }
    const normalizedCommand =
      target === "browser"
        ? normalizeBrowserCustomActionCommand(command)
        : command;
    if (!normalizedCommand) {
      continue;
    }
    if (
      target !== "agent" &&
      target !== "browser" &&
      normalizedCommand.length > 0
    ) {
      continue;
    }
    const keywords = Array.isArray(raw.keywords)
      ? raw.keywords
          .filter(
            (part): part is string =>
              typeof part === "string" && part.trim() !== "",
          )
          .map((part) => part.trim())
      : [];
    seen.add(id);
    actions.push({
      id,
      title,
      group,
      target,
      command: normalizedCommand,
      keywords,
    });
  }
  return actions;
}

function normalizeBrowserCustomActionCommand(
  command: string[],
): string[] | null {
  if (command.length === 0) {
    return [];
  }
  const rawOperation = command[0].trim().toLowerCase();
  const extractedFrame = extractBrowserActionFrameId(rawOperation, command);
  if (!extractedFrame || extractedFrame.command.length === 0) {
    return null;
  }
  const parts = extractedFrame.command;
  const frameId = extractedFrame.frameId ?? "";
  const hasFrameId = frameId.length > 0;
  const operation = parts[0].trim().toLowerCase();
  if (operation === "open" || operation === "navigate") {
    if (
      hasFrameId ||
      parts.length < 2 ||
      parts.length > 3 ||
      !parts[1].trim()
    ) {
      return null;
    }
    const placement = normalizeBrowserActionPlacement(parts[2] ?? "new_tab");
    return placement ? ["open", parts[1].trim(), placement] : null;
  }
  if (operation === "new-tab" || operation === "new_tab") {
    return !hasFrameId && parts.length === 2 && parts[1].trim()
      ? ["open", parts[1].trim(), "new_tab"]
      : null;
  }
  if (operation === "active-pane" || operation === "active_pane") {
    return !hasFrameId && parts.length === 2 && parts[1].trim()
      ? ["open", parts[1].trim(), "active_pane"]
      : null;
  }
  if (operation === "screenshot") {
    if (hasFrameId || parts.length > 3) {
      return null;
    }
    const first = parts[1]?.trim();
    const placementFromFirst = first
      ? normalizeBrowserActionPlacement(first)
      : null;
    const format = first && !placementFromFirst ? first : "png";
    const placement =
      normalizeBrowserActionPlacement(parts[2] ?? "") ??
      placementFromFirst ??
      "active_pane";
    return ["screenshot", format, placement];
  }
  if (operation === "dom-snapshot" || operation === "dom_snapshot") {
    if (parts.length > 2) {
      return null;
    }
    const placement =
      normalizeBrowserActionPlacement(parts[1] ?? "") ?? "active_pane";
    return ["dom-snapshot", placement, frameId];
  }
  if (operation === "evaluate") {
    if (parts.length < 2 || parts.length > 3 || !parts[1].trim()) {
      return null;
    }
    const placement =
      normalizeBrowserActionPlacement(parts[2] ?? "") ?? "active_pane";
    return ["evaluate", parts[1].trim(), placement, frameId];
  }
  if (operation === "click") {
    if (parts.length < 2 || parts.length > 3 || !parts[1].trim()) {
      return null;
    }
    const placement =
      normalizeBrowserActionPlacement(parts[2] ?? "") ?? "active_pane";
    return ["click", parts[1].trim(), placement, frameId];
  }
  if (operation === "type") {
    if (
      parts.length < 3 ||
      parts.length > 4 ||
      !parts[1].trim() ||
      !parts[2].trim()
    ) {
      return null;
    }
    const placement =
      normalizeBrowserActionPlacement(parts[3] ?? "") ?? "active_pane";
    return ["type", parts[1].trim(), parts[2].trim(), placement, frameId];
  }
  if (operation === "fill") {
    if (
      parts.length < 3 ||
      parts.length > 4 ||
      !parts[1].trim() ||
      !parts[2].trim()
    ) {
      return null;
    }
    const placement =
      normalizeBrowserActionPlacement(parts[3] ?? "") ?? "active_pane";
    return ["fill", parts[1].trim(), parts[2].trim(), placement, frameId];
  }
  if (operation === "press") {
    if (
      parts.length < 3 ||
      parts.length > 4 ||
      !parts[1].trim() ||
      !parts[2].trim()
    ) {
      return null;
    }
    const placement =
      normalizeBrowserActionPlacement(parts[3] ?? "") ?? "active_pane";
    return ["press", parts[1].trim(), parts[2].trim(), placement, frameId];
  }
  if (operation === "select") {
    if (parts.length < 3 || !parts[1].trim()) {
      return null;
    }
    const last = parts[parts.length - 1];
    const placementFromLast = normalizeBrowserActionPlacement(last);
    const values = parts
      .slice(2, placementFromLast ? -1 : undefined)
      .map((value) => value.trim())
      .filter(Boolean);
    if (values.length === 0) {
      return null;
    }
    return [
      "select",
      parts[1].trim(),
      ...values,
      placementFromLast ?? "active_pane",
      frameId,
    ];
  }
  if (operation === "scroll") {
    if (parts.length < 2 || parts.length > 5) {
      return null;
    }
    const last = parts[parts.length - 1];
    const placementFromLast = normalizeBrowserActionPlacement(last);
    const args = parts
      .slice(1, placementFromLast ? -1 : undefined)
      .map((value) => value.trim());
    let selector = "";
    let x = 0;
    let y: number | null = null;
    if (args.length === 1) {
      y = parseBrowserInteger(args[0]);
    } else if (args.length === 2) {
      x = parseBrowserInteger(args[0]) ?? Number.NaN;
      y = parseBrowserInteger(args[1]);
    } else if (args.length === 3) {
      selector = args[0];
      x = parseBrowserInteger(args[1]) ?? Number.NaN;
      y = parseBrowserInteger(args[2]);
    }
    if (y === null || !Number.isFinite(x) || selector === undefined) {
      return null;
    }
    return [
      "scroll",
      selector,
      x.toString(),
      y.toString(),
      placementFromLast ?? "active_pane",
      frameId,
    ];
  }
  if (operation === "hover") {
    if (parts.length < 2 || parts.length > 3 || !parts[1].trim()) {
      return null;
    }
    const placement =
      normalizeBrowserActionPlacement(parts[2] ?? "") ?? "active_pane";
    return ["hover", parts[1].trim(), placement, frameId];
  }
  if (operation === "check" || operation === "uncheck") {
    if (parts.length < 2 || parts.length > 4 || !parts[1].trim()) {
      return null;
    }
    const thirdArgPlacement = normalizeBrowserActionPlacement(parts[2] ?? "");
    const checked =
      operation === "uncheck"
        ? false
        : thirdArgPlacement
          ? true
          : (parseBrowserBoolean(parts[2]) ?? true);
    const placement = thirdArgPlacement
      ? thirdArgPlacement
      : (normalizeBrowserActionPlacement(parts[3] ?? "") ?? "active_pane");
    return ["check", parts[1].trim(), checked.toString(), placement, frameId];
  }
  if (operation === "focus") {
    if (parts.length < 2 || parts.length > 3 || !parts[1].trim()) {
      return null;
    }
    const placement =
      normalizeBrowserActionPlacement(parts[2] ?? "") ?? "active_pane";
    return ["focus", parts[1].trim(), placement, frameId];
  }
  if (operation === "zoom") {
    if (hasFrameId || parts.length < 2 || parts.length > 3) {
      return null;
    }
    const percent = parseBrowserPositiveInteger(parts[1]);
    if (percent === null || percent < 25 || percent > 500) {
      return null;
    }
    const placement =
      normalizeBrowserActionPlacement(parts[2] ?? "") ?? "active_pane";
    return ["zoom", percent.toString(), placement];
  }
  if (
    operation === "wait" ||
    operation === "wait-for-selector" ||
    operation === "wait_for_selector"
  ) {
    if (parts.length < 2 || parts.length > 4 || !parts[1].trim()) {
      return null;
    }
    const thirdArgIsTimeout = parseBrowserPositiveInteger(parts[2]) !== null;
    const placement = thirdArgIsTimeout
      ? "active_pane"
      : (normalizeBrowserActionPlacement(parts[2] ?? "") ?? "active_pane");
    const timeout =
      parseBrowserPositiveInteger(thirdArgIsTimeout ? parts[2] : parts[3]) ??
      5000;
    return [
      "wait-for-selector",
      parts[1].trim(),
      placement,
      timeout.toString(),
      frameId,
    ];
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
    if (hasFrameId || parts.length > 2) {
      return null;
    }
    const placement =
      normalizeBrowserActionPlacement(parts[1] ?? "") ?? "active_pane";
    return [normalizeBrowserNavigationCommand(operation), placement];
  }
  if (operation === "highlight") {
    if (parts.length < 2 || parts.length > 4 || !parts[1].trim()) {
      return null;
    }
    const thirdArgIsDuration = parseBrowserPositiveInteger(parts[2]) !== null;
    const placement = thirdArgIsDuration
      ? (normalizeBrowserActionPlacement(parts[3] ?? "") ?? "active_pane")
      : (normalizeBrowserActionPlacement(parts[2] ?? "") ?? "active_pane");
    const duration =
      parseBrowserPositiveInteger(thirdArgIsDuration ? parts[2] : parts[3]) ??
      1200;
    return [
      "highlight",
      parts[1].trim(),
      duration.toString(),
      placement,
      frameId,
    ];
  }
  return null;
}

function extractBrowserActionFrameId(
  operation: string,
  command: string[],
): { command: string[]; frameId: string | null } | null {
  const normalized: string[] = [];
  let frameId: string | null = null;
  for (let index = 0; index < command.length; index += 1) {
    const part = command[index];
    if (index > 0) {
      const parsed = parseBrowserActionFrameToken(part);
      if (parsed !== undefined) {
        if (!parsed || frameId) {
          return null;
        }
        frameId = parsed;
        continue;
      }
    }
    normalized.push(part);
  }
  if (
    frameId === null &&
    browserActionHasNormalizedFrameSlot(operation, normalized)
  ) {
    const value = normalized.pop()?.trim() ?? "";
    frameId = value || null;
  }
  return { command: normalized, frameId };
}

function browserActionHasNormalizedFrameSlot(
  operation: string,
  command: string[],
): boolean {
  if (operation === "dom-snapshot" || operation === "dom_snapshot") {
    return (
      command.length === 3 &&
      normalizeBrowserActionPlacement(command[1]) !== null
    );
  }
  if (operation === "evaluate" || operation === "click") {
    return (
      command.length === 4 &&
      normalizeBrowserActionPlacement(command[2]) !== null
    );
  }
  if (
    operation === "type" ||
    operation === "fill" ||
    operation === "press" ||
    operation === "check" ||
    operation === "highlight"
  ) {
    return (
      command.length === 5 &&
      normalizeBrowserActionPlacement(command[3]) !== null
    );
  }
  if (operation === "select") {
    return (
      command.length >= 5 &&
      normalizeBrowserActionPlacement(command[command.length - 2]) !== null
    );
  }
  if (operation === "scroll") {
    return (
      command.length === 6 &&
      normalizeBrowserActionPlacement(command[4]) !== null
    );
  }
  if (operation === "hover" || operation === "focus") {
    return (
      command.length === 4 &&
      normalizeBrowserActionPlacement(command[2]) !== null
    );
  }
  if (
    operation === "wait" ||
    operation === "wait-for-selector" ||
    operation === "wait_for_selector"
  ) {
    return (
      command.length === 5 &&
      normalizeBrowserActionPlacement(command[2]) !== null &&
      parseBrowserPositiveInteger(command[3]) !== null
    );
  }
  return false;
}

function parseBrowserActionFrameToken(
  value: string,
): string | null | undefined {
  const trimmed = value.trim();
  const lower = trimmed.toLowerCase();
  for (const prefix of [
    "frame:",
    "frame=",
    "frame-id:",
    "frame-id=",
    "frame_id:",
    "frame_id=",
  ]) {
    if (lower.startsWith(prefix)) {
      return trimmed.slice(prefix.length).trim() || null;
    }
  }
  return undefined;
}

function normalizeBrowserActionPlacement(
  value: string,
): "new_tab" | "active_pane" | null {
  const placement = value.trim().toLowerCase();
  if (placement === "new-tab" || placement === "new_tab") {
    return "new_tab";
  }
  if (placement === "active-pane" || placement === "active_pane") {
    return "active_pane";
  }
  return null;
}

function normalizeBrowserNavigationCommand(operation: string): string {
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
    return "current-url";
  }
  return "reload";
}

function parseBrowserInteger(value: string | undefined): number | null {
  const trimmed = value?.trim();
  if (!trimmed || !/^-?\d+$/.test(trimmed)) {
    return null;
  }
  const parsed = Number.parseInt(trimmed, 10);
  return Number.isFinite(parsed) ? parsed : null;
}

function parseBrowserPositiveInteger(value: string | undefined): number | null {
  const parsed = parseBrowserInteger(value);
  return parsed !== null && parsed > 0 ? parsed : null;
}

function parseBrowserBoolean(value: string | undefined): boolean | null {
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

function sanitizeTextBoxMaxLines(value: unknown): number | null {
  if (value === null || value === undefined) {
    return null;
  }
  const lines =
    typeof value === "number"
      ? value
      : typeof value === "string"
        ? Number(value.trim())
        : Number.NaN;
  if (!Number.isFinite(lines)) {
    return null;
  }
  const rounded = Math.round(lines);
  return Math.min(12, Math.max(2, rounded));
}

function sanitizeTerminalInnerMargin(value: unknown): number | null {
  if (value === null || value === undefined) {
    return null;
  }
  const margin =
    typeof value === "number"
      ? value
      : typeof value === "string"
        ? Number(value.trim())
        : Number.NaN;
  if (!Number.isFinite(margin)) {
    return null;
  }
  const rounded = Math.round(margin);
  return Math.min(32, Math.max(0, rounded));
}

function sanitizeAppConfigUi(
  value: Partial<AppConfigUi> | undefined,
): AppConfigUi {
  return {
    workspacePlusAction: sanitizeActionReference(value?.workspacePlusAction),
    surfaceTabPlusAction: sanitizeActionReference(value?.surfaceTabPlusAction),
    surfaceTabActions: sanitizeActionReferenceList(value?.surfaceTabActions),
    textBoxMaxLines: sanitizeTextBoxMaxLines(value?.textBoxMaxLines),
    terminalInnerMargin: sanitizeTerminalInnerMargin(
      value?.terminalInnerMargin,
    ),
  };
}

function sanitizeDockConfig(value: unknown): { controls: DockControl[] } {
  if (!value || typeof value !== "object") {
    return { controls: [] };
  }
  return {
    controls: sanitizeDockControls((value as { controls?: unknown }).controls),
  };
}

function sanitizeDockControls(values: unknown): DockControl[] {
  if (!Array.isArray(values)) {
    return [];
  }
  const controls: DockControl[] = [];
  const seen = new Set<string>();
  for (const value of values) {
    if (!value || typeof value !== "object") {
      continue;
    }
    const raw = value as Record<string, unknown>;
    const id = typeof raw.id === "string" ? raw.id.trim() : "";
    const title = typeof raw.title === "string" ? raw.title.trim() : "";
    const command = typeof raw.command === "string" ? raw.command.trim() : "";
    if (
      !id ||
      !sanitizeActionReference(id) ||
      !title ||
      !command ||
      seen.has(id)
    ) {
      continue;
    }
    const env: Record<string, string> = {};
    if (raw.env && typeof raw.env === "object" && !Array.isArray(raw.env)) {
      for (const [key, envValue] of Object.entries(
        raw.env as Record<string, unknown>,
      )) {
        const envKey = key.trim();
        if (envKey && typeof envValue === "string") {
          env[envKey] = envValue;
        }
      }
    }
    const height =
      typeof raw.height === "number" && raw.height > 0 ? raw.height : null;
    seen.add(id);
    controls.push({
      id,
      title,
      command,
      cwd:
        typeof raw.cwd === "string" && raw.cwd.trim() ? raw.cwd.trim() : null,
      height,
      env,
    });
  }
  return controls;
}

function dockControlEnv(
  control: DockControl,
): Array<{ key: string; value: string }> {
  const env = [
    { key: "AGENTMUX_SURFACE_TITLE", value: control.title },
    { key: "CMUX_SURFACE_TITLE", value: control.title },
    { key: "AGENTMUX_SURFACE_TYPE", value: "dock-terminal" },
    { key: "AGENTMUX_DOCK_CONTROL_ID", value: control.id },
    { key: "CMUX_DOCK_CONTROL_ID", value: control.id },
  ];
  for (const [key, value] of Object.entries(control.env)) {
    if (!key.trim()) {
      continue;
    }
    env.push({ key: key.trim(), value });
  }
  return env;
}

function sanitizeNotificationActions(
  values: unknown,
): AppConfigNotificationAction[] {
  if (!Array.isArray(values)) {
    return [];
  }
  const actions: AppConfigNotificationAction[] = [];
  const seen = new Set<string>();
  for (const value of values) {
    if (!value || typeof value !== "object") {
      continue;
    }
    const raw = value as Record<string, unknown>;
    const action = sanitizeActionReference(raw.action);
    if (!action) {
      continue;
    }
    const notificationType = sanitizeNotificationType(
      raw.notificationType ?? raw.notification_type,
    );
    const severity = sanitizeNotificationSeverity(raw.severity);
    const key = `${action}\n${notificationType ?? ""}\n${severity ?? ""}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    actions.push({
      action,
      label:
        typeof raw.label === "string" && raw.label.trim()
          ? raw.label.trim()
          : null,
      notificationType,
      severity,
      dismissOnRun:
        typeof (raw.dismissOnRun ?? raw.dismiss_on_run) === "boolean"
          ? ((raw.dismissOnRun ?? raw.dismiss_on_run) as boolean)
          : null,
    });
  }
  return actions;
}

function sanitizeNotificationType(value: unknown): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const notificationType = value.trim();
  return /^[A-Za-z0-9._-]+$/.test(notificationType) ? notificationType : null;
}

function sanitizeNotificationSeverity(value: unknown): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const severity = value.trim().toLowerCase();
  return ["info", "progress", "success", "warning", "error"].includes(severity)
    ? severity
    : null;
}

function sanitizeActionReference(value: unknown): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const actionId = value.trim();
  return /^[A-Za-z0-9._-]+$/.test(actionId) ? actionId : null;
}

function sanitizeActionReferenceList(value: unknown): string[] | null {
  if (!Array.isArray(value)) {
    return null;
  }
  const actions: string[] = [];
  for (const item of value) {
    const actionId = sanitizeActionReference(item);
    if (actionId && !actions.includes(actionId)) {
      actions.push(actionId);
    }
  }
  return actions;
}

function isValidCustomActionId(value: string): boolean {
  return /^custom\.[A-Za-z0-9._-]+$/.test(value);
}

function isCustomActionTarget(value: string): value is CustomActionTarget {
  return value === "agent" || value === "wsl-terminal" || value === "browser";
}

function mapProfile(value: SshProfileWire): SshProfile {
  return {
    profileId: value.profile_id,
    name: value.name,
    host: value.host,
    user: value.user,
    port: value.port ?? null,
  };
}

function mapBrowserNavigation(
  value: BrowserNavigationResultWire,
): BrowserNavigationResult {
  return {
    surfaceId: value.surface_id,
    url: value.url,
  };
}

function mapBrowserScreenshot(
  value: BrowserScreenshotResultWire,
): BrowserScreenshotResult {
  return {
    surfaceId: value.surface_id,
    format: value.format,
    imageHandle: value.image_handle,
    byteCount: value.byte_count,
  };
}

function mapBrowserDomSnapshot(
  value: BrowserDomSnapshotResultWire,
): BrowserDomSnapshotResult {
  return {
    surfaceId: value.surface_id,
    html: value.html,
  };
}

function mapBrowserAction(value: BrowserActionResultWire): BrowserActionResult {
  return {
    surfaceId: value.surface_id,
    ok: value.ok,
  };
}

function mapBrowserGet(value: BrowserGetResultWire): BrowserGetResult {
  return {
    surfaceId: value.surface_id,
    selector: value.selector,
    kind: value.kind,
    value: value.value,
  };
}

function mapBrowserFind(value: BrowserFindResultWire): BrowserFindResult {
  return {
    surfaceId: value.surface_id,
    query: value.query,
    count: value.count,
    matches: value.matches,
  };
}

function mapBrowserWaitForSelector(
  value: BrowserWaitForSelectorResultWire,
): BrowserWaitForSelectorResult {
  return {
    surfaceId: value.surface_id,
    selector: value.selector,
    elapsedMs: value.elapsed_ms,
  };
}

function mapBrowserEvaluate(
  value: BrowserEvaluateResultWire,
): BrowserEvaluateResult {
  return {
    surfaceId: value.surface_id,
    valueJson: value.value_json,
  };
}

function mapBrowserDiagnostic(value: BrowserDiagnosticWire): BrowserDiagnostic {
  return {
    surfaceId: value.surface_id,
    workspaceId: value.workspace_id,
    operation: value.operation,
    code: value.code,
    message: value.message,
    occurredAt: value.occurred_at,
  };
}
