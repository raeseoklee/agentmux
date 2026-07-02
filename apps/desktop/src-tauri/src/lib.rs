use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::env;
use std::fmt;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use agentmux_backend::{
    AttachRequest, BackendError, BackendEvent, BackendKind as BackendTraitKind, BackendResult,
    InputEvent, SessionBackend, SessionHandle, SpawnRequest, TerminalSize, TerminationMode,
};
use agentmux_backend_conpty::ConptyBackend;
use agentmux_backend_ssh::SshDirectBackend;
use agentmux_backend_tmux::{posix_shell_quote, TmuxControlBackend};
use agentmux_backend_wsl::{
    discover_wsl_distributions as discover_wsl_distributions_from_backend, PipeBackend,
    WslDiagnosticCode, WslDirectBackend, WslDirectConfig, WslDistribution,
};
use agentmux_browser::{
    BrowserAutomation, BrowserAutomationError, BrowserAutomationErrorCode, BrowserCommand,
    BrowserCommandResult, BrowserConsoleMessage, BrowserCookieInfo, BrowserDialogMessage,
    BrowserDownloadInfo, BrowserErrorEvent, BrowserFrameInfo, BrowserHistoryEntry,
    BrowserStorageEntry, BrowserSurface, CdpBrowserAutomation, InMemoryBrowserAutomation,
};
use agentmux_core::{
    PaneId, RuntimeControlPlane, SessionId, SurfaceId, TerminalRuntime, WorkspaceId,
};
use agentmux_ipc::{
    AckResult, ActionListParams, ActionListResult, ActionRunParams, ActionRunResult,
    ActionSummaryResult, AgentAttentionListResult, AgentListAttentionParams, AgentStateResult,
    AgentTelemetry, AppConfigActions, AppConfigAppearance, AppConfigCustomAction,
    AppConfigDiagnosticsEntry, AppConfigDiagnosticsParams, AppConfigDiagnosticsResult,
    AppConfigExportParams, AppConfigExportResult, AppConfigGetParams, AppConfigImportParams,
    AppConfigLocale, AppConfigMigrateProjectParams, AppConfigMigrateProjectResult,
    AppConfigNotifications, AppConfigResetParams, AppConfigResult, AppConfigShortcuts, AppConfigUi,
    AppConfigUpdateParams, AppConfigUpdates, BrowserActionResult, BrowserCheckParams,
    BrowserClickParams, BrowserConsoleMessageResult, BrowserConsoleParams, BrowserConsoleResult,
    BrowserCookieResult, BrowserCookiesResult, BrowserDiagnosticResult, BrowserDiagnosticsParams,
    BrowserDiagnosticsResult, BrowserDialogMessageResult, BrowserDialogsParams,
    BrowserDialogsResult, BrowserDomSnapshotParams, BrowserDomSnapshotResult,
    BrowserDownloadResult, BrowserDownloadsParams, BrowserDownloadsResult, BrowserErrorEventResult,
    BrowserErrorsParams, BrowserErrorsResult, BrowserEvaluateParams, BrowserEvaluateResult,
    BrowserFillParams, BrowserFindParams, BrowserFindResult, BrowserFocusParams,
    BrowserFrameResult, BrowserFramesResult, BrowserGetParams, BrowserGetResult,
    BrowserHighlightParams, BrowserHistoryEntryResult, BrowserHistoryResult, BrowserHoverParams,
    BrowserNavigateParams, BrowserNavigationResult, BrowserPressParams, BrowserScreenshotParams,
    BrowserScreenshotResult, BrowserScrollParams, BrowserSelectParams, BrowserStorageEntryResult,
    BrowserStorageResult, BrowserSurfaceParams, BrowserTypeParams, BrowserWaitForSelectorParams,
    BrowserWaitForSelectorResult, BrowserZoomParams, ControlError, ControlPipeConnection,
    DiagnosticsBackendHealthResult, DiagnosticsExportResult, DiagnosticsOutputStreamResult,
    DiagnosticsQueuePressureResult, DockConfigResult, DockControlResult, DockGetParams,
    DockTrustParams, EnvVarParam, ErrorCode, EventSubscribeParams, EventSubscribeResult,
    NotificationClearParams, NotificationClearResult, NotificationCreateParams,
    NotificationDismissParams, NotificationListParams, NotificationListResult,
    NotificationSummaryResult, PaneCloseParams, PaneFocusParams, PaneMountSurfaceParams,
    PaneResizeLayoutParams, PaneSplitParams, PaneSummaryResult, PaneUnmountSurfaceParams,
    ProfileCreateParams, ProfileIdParams, ProfileListResult, ProfileSummaryResult,
    ProfileUpdateParams, RecoveryDiagnosticsResult, RecoverySessionResult, RequestEnvelope,
    ResponseEnvelope, ResponseOutcome, SessionAttachParams, SessionIdParams, SessionSendTextParams,
    SessionSpawnParams, SessionSpawnResult, SessionSummaryResult, SidebarLogAddParams,
    SidebarLogListParams, SidebarLogListResult, SidebarLogResult, SidebarProgressResult,
    SidebarProgressSetParams, SidebarStateResult, SidebarStatusKeyParams, SidebarStatusListResult,
    SidebarStatusResult, SidebarStatusSetParams, SidebarWorkspaceParams, SurfaceCloseParams,
    SurfaceCreateBrowserParams, SurfaceMoveWorkspaceParams, SurfaceMoveWorkspaceResult,
    SurfaceSummaryResult, SystemCapabilitiesResult, SystemIdentifyParams, SystemIdentifyResult,
    TeamMessageListParams, TeamMessageListResult, TeamMessageMarkReadParams, TeamMessageResult,
    TeamMessageSendParams, TeamTaskBlockParams, TeamTaskClaimParams, TeamTaskCreateParams,
    TeamTaskDependencyParams, TeamTaskIdParams, TeamTaskListParams, TeamTaskListResult,
    TeamTaskResult, TmuxDiagnosticsParams, TmuxDiagnosticsResult, WorkspaceCloseParams,
    WorkspaceCloseResult, WorkspaceCreateParams, WorkspaceDetailResult, WorkspaceGroupCreateParams,
    WorkspaceGroupIdParams, WorkspaceGroupListParams, WorkspaceGroupListResult,
    WorkspaceGroupMemberParams, WorkspaceGroupMemberResult, WorkspaceGroupResult,
    WorkspaceGroupUpdateParams, WorkspaceIdParams, WorkspaceListResult, WorkspaceRenameParams,
    WorkspaceSummaryResult, WorkspaceUpdateParams, WslDistributionListResult,
    WslDistributionResult, DEFAULT_CONTROL_PIPE_NAME, DEFAULT_LOCAL_CONTROL_TOKEN,
};
use agentmux_store::{
    PersistedAgentState, PersistedDockTrust, PersistedNotification, PersistedPane,
    PersistedProfile, PersistedSession, PersistedSidebarLog, PersistedSidebarProgress,
    PersistedSidebarStatus, PersistedSurface, PersistedTeamMessage, PersistedTeamTask,
    PersistedWorkspace, PersistedWorkspaceGroup, PersistedWorkspaceGroupMember, RecoverySnapshot,
    SqliteStore, StoreError, WorkspaceBundle,
};
use base64::prelude::{Engine as _, BASE64_STANDARD};
use tauri::ipc::Channel;

pub const DESKTOP_CONTROL_TOKEN: &str = DEFAULT_LOCAL_CONTROL_TOKEN;
const MAX_BROWSER_FAILURES: usize = 100;
const APP_CONFIG_FILE_NAME: &str = "agentmux.json";
const APP_CONFIG_FORMAT_VERSION: &str = "agentmux.config.v1";
const DOCK_CONFIG_FILE_NAME: &str = "dock.json";
const PROJECT_CONFIG_DIR_NAME: &str = ".agentmux";
const CMUX_PROJECT_CONFIG_DIR_NAME: &str = ".cmux";
const CMUX_PROJECT_CONFIG_FILE_NAME: &str = "cmux.json";
const TEXT_BOX_MIN_LINES: u8 = 2;
const TEXT_BOX_MAX_LINES: u8 = 12;
const TERMINAL_INNER_MARGIN_MIN: u8 = 0;
const TERMINAL_INNER_MARGIN_MAX: u8 = 32;
const GIT_STATUS_CACHE_TTL: Duration = Duration::from_secs(3);
const TMUX_SESSION_EXISTS_TIMEOUT_MS: u64 = 1_500;

#[derive(Clone)]
struct GitStatusCacheEntry {
    captured_at: Instant,
    branch: Option<String>,
    hash: Option<String>,
}

static GIT_STATUS_CACHE: OnceLock<Mutex<HashMap<String, GitStatusCacheEntry>>> = OnceLock::new();

type DesktopRuntimeControl = RuntimeControlPlane<DesktopBackendRouter>;

pub struct DesktopBackendRouter {
    conpty: ConptyBackend,
    wsl_direct: WslDirectBackend,
    tmux_control: TmuxControlBackend<WslDirectBackend<PipeBackend>>,
    ssh: SshDirectBackend,
    routes: HashMap<String, BackendTraitKind>,
}

impl DesktopBackendRouter {
    pub fn new() -> Self {
        Self {
            conpty: ConptyBackend::new(),
            wsl_direct: WslDirectBackend::with_config(WslDirectConfig::for_interactive_terminal()),
            tmux_control: TmuxControlBackend::new(),
            ssh: SshDirectBackend::new(),
            routes: HashMap::new(),
        }
    }

    fn backend_for_session(&mut self, session_id: &str) -> BackendResult<&mut dyn SessionBackend> {
        match self.routes.get(session_id).copied() {
            Some(BackendTraitKind::Conpty) => Ok(&mut self.conpty),
            Some(BackendTraitKind::WslDirect) => Ok(&mut self.wsl_direct),
            Some(BackendTraitKind::WslTmuxControl) => Ok(&mut self.tmux_control),
            Some(BackendTraitKind::Ssh) => Ok(&mut self.ssh),
            None => Err(BackendError::session_not_found(session_id)),
        }
    }
}

impl Default for DesktopBackendRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionBackend for DesktopBackendRouter {
    fn kind(&self) -> BackendTraitKind {
        BackendTraitKind::Conpty
    }

    fn spawn(&mut self, mut request: SpawnRequest) -> BackendResult<SessionHandle> {
        let requested = request.backend.unwrap_or(BackendTraitKind::Conpty);
        request.backend = Some(requested);
        let handle = match requested {
            BackendTraitKind::Conpty => self.conpty.spawn(request)?,
            BackendTraitKind::WslDirect => self.wsl_direct.spawn(request)?,
            BackendTraitKind::WslTmuxControl => self.tmux_control.spawn(request)?,
            BackendTraitKind::Ssh => self.ssh.spawn(request)?,
        };
        self.routes
            .insert(handle.session_id.clone(), handle.backend_kind);
        Ok(handle)
    }

    fn attach(&mut self, request: AttachRequest) -> BackendResult<SessionHandle> {
        let requested = request.backend;
        let handle = match requested {
            BackendTraitKind::Conpty => self.conpty.attach(request)?,
            BackendTraitKind::WslDirect => self.wsl_direct.attach(request)?,
            BackendTraitKind::WslTmuxControl => self.tmux_control.attach(request)?,
            BackendTraitKind::Ssh => self.ssh.attach(request)?,
        };
        self.routes
            .insert(handle.session_id.clone(), handle.backend_kind);
        Ok(handle)
    }

    fn send_input(&mut self, session_id: &str, input: InputEvent) -> BackendResult<()> {
        self.backend_for_session(session_id)?
            .send_input(session_id, input)
    }

    fn resize(&mut self, session_id: &str, size: TerminalSize) -> BackendResult<()> {
        self.backend_for_session(session_id)?
            .resize(session_id, size)
    }

    fn terminate(&mut self, session_id: &str, mode: TerminationMode) -> BackendResult<()> {
        self.backend_for_session(session_id)?
            .terminate(session_id, mode)?;
        if !matches!(mode, TerminationMode::Interrupt) {
            self.routes.remove(session_id);
        }
        Ok(())
    }

    fn drain_events(&mut self) -> Vec<BackendEvent> {
        let mut events = self.conpty.drain_events();
        events.extend(self.wsl_direct.drain_events());
        events.extend(self.tmux_control.drain_events());
        events.extend(self.ssh.drain_events());
        events
    }
}

/// A live terminal-output frame pushed to a per-session Tauri `Channel`. The
/// raw bytes (base64-encoded) begin at the absolute offset `from_offset`.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputStreamFrame {
    pub from_offset: u64,
    pub bytes_base64: String,
}

const DURABLE_TMUX_ATTACH_READY_TIMEOUT_MS: u64 = 2_000;
const DURABLE_TMUX_ATTACH_READY_POLL_MS: u64 = 40;
const OUTPUT_STREAM_IDLE_PUMP_MS: u64 = 12;
const OUTPUT_STREAM_HOT_PUMP_MS: u64 = 3;
const OUTPUT_STREAM_HOT_WINDOW_MS: u64 = 160;
const OUTPUT_FLOW_CONTROL_PAUSE_BYTES: u64 = 1024 * 1024;
const OUTPUT_FLOW_CONTROL_RESUME_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, Default)]
struct OutputStreamMetrics {
    frames_sent: u64,
    bytes_sent: u64,
    send_failures: u64,
    closed_channels: u64,
    pump_runs: u64,
    pump_active_runs: u64,
    pump_idle_runs: u64,
    last_frame_at: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct OutputPressureRecord {
    queued_bytes: u64,
    max_queued_bytes: u64,
    backpressure_events: u64,
    write_in_flight: bool,
}

pub struct DesktopControlState {
    control: Mutex<DesktopRuntimeControl>,
    store: Mutex<SqliteStore>,
    browser: Mutex<Box<dyn BrowserAutomation>>,
    browser_failures: Mutex<VecDeque<BrowserFailureRecord>>,
    browser_failure_counter: Mutex<u64>,
    config_path: PathBuf,
    control_token: String,
    desktop_notifications: Mutex<DesktopNotificationState>,
    // Per-session live-output Tauri channels for the stream-first renderer.
    // Subscriptions are keyed independently so a late unsubscribe from an old
    // remount cannot remove the current renderer's channel.
    output_channels: Mutex<HashMap<String, HashMap<String, Channel<OutputStreamFrame>>>>,
    output_pump_hot_until: Mutex<Option<Instant>>,
    output_stream_metrics: Mutex<OutputStreamMetrics>,
    output_pressure: Mutex<HashMap<String, OutputPressureRecord>>,
    input_command_buffers: Mutex<HashMap<String, String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserFailureRecord {
    surface_id: Option<String>,
    workspace_id: Option<String>,
    operation: String,
    code: String,
    message: String,
    occurred_at: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct AppConfigFile {
    #[serde(default = "default_app_config_format_version")]
    format_version: String,
    #[serde(default = "default_app_config_appearance")]
    appearance: AppConfigAppearance,
    #[serde(default = "default_app_config_locale")]
    locale: AppConfigLocale,
    #[serde(default = "default_app_config_updates")]
    updates: AppConfigUpdates,
    #[serde(default)]
    shortcuts: AppConfigShortcuts,
    #[serde(default)]
    actions: AppConfigActions,
    #[serde(default)]
    ui: AppConfigUi,
    #[serde(default)]
    notifications: AppConfigNotifications,
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
struct ProjectAppConfigFile {
    #[serde(default)]
    shortcuts: AppConfigShortcuts,
    #[serde(default)]
    actions: AppConfigActions,
    #[serde(default)]
    ui: AppConfigUi,
    #[serde(default)]
    notifications: AppConfigNotifications,
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
struct DockConfigFile {
    #[serde(default)]
    controls: Vec<DockControlFile>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct DockControlFile {
    id: String,
    title: String,
    command: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    height: Option<u16>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopNotification {
    pub notification_id: String,
    pub notification_type: String,
    pub severity: String,
    pub title: String,
    pub body: String,
}

pub trait DesktopNotificationAdapter: Send + Sync {
    fn notify(&self, notification: DesktopNotification);
}

const MAX_DELIVERED_NOTIFICATION_IDS: usize = 1000;

#[derive(Default)]
struct DesktopNotificationState {
    adapter: Option<Arc<dyn DesktopNotificationAdapter>>,
    /// Tracks recently delivered OS notification IDs to prevent duplicate toasts.
    /// Capped at MAX_DELIVERED_NOTIFICATION_IDS; oldest entries are evicted first.
    delivered_notification_ids: HashSet<String>,
    delivered_notification_order: VecDeque<String>,
}

impl DesktopControlState {
    pub fn new() -> Self {
        Self::new_in_memory().expect("failed to initialize in-memory desktop state")
    }

    /// Open a store with the well-known bootstrap token. Only available in
    /// tests; production callers must use [`open_with_token`] with a randomly
    /// generated token so the footgun cannot be hit silently.
    ///
    /// [`open_with_token`]: DesktopControlState::open_with_token
    #[cfg(test)]
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DesktopHostError> {
        Self::open_with_token(path, DESKTOP_CONTROL_TOKEN)
    }

    pub fn open_with_token(
        path: impl AsRef<Path>,
        token: impl Into<String>,
    ) -> Result<Self, DesktopHostError> {
        Self::open_with_token_and_config(path, token, default_app_config_path()?)
    }

    pub fn open_with_token_and_config(
        path: impl AsRef<Path>,
        token: impl Into<String>,
        config_path: impl Into<PathBuf>,
    ) -> Result<Self, DesktopHostError> {
        let token = token.into();
        let runtime = TerminalRuntime::new(DesktopBackendRouter::new());
        let state = Self {
            control: Mutex::new(RuntimeControlPlane::new(runtime, token.clone())),
            store: Mutex::new(SqliteStore::open(path)?),
            browser: Mutex::new(browser_automation_from_environment()?),
            browser_failures: Mutex::new(VecDeque::new()),
            browser_failure_counter: Mutex::new(0),
            config_path: config_path.into(),
            control_token: token,
            desktop_notifications: Mutex::new(DesktopNotificationState::default()),
            output_channels: Mutex::new(HashMap::new()),
            output_pump_hot_until: Mutex::new(None),
            output_stream_metrics: Mutex::new(OutputStreamMetrics::default()),
            output_pressure: Mutex::new(HashMap::new()),
            input_command_buffers: Mutex::new(HashMap::new()),
        };
        // Durable-session recovery is deliberately NOT run here: it probes
        // wsl.exe/tmux and can block for seconds. The desktop host runs it on a
        // background thread after the control pipe and window are up (main.rs);
        // other callers invoke recover_durable_sessions() explicitly if needed.
        Ok(state)
    }

    pub fn new_in_memory() -> Result<Self, DesktopHostError> {
        Self::new_in_memory_with_token(DESKTOP_CONTROL_TOKEN)
    }

    pub fn new_in_memory_with_token(token: impl Into<String>) -> Result<Self, DesktopHostError> {
        let token = token.into();
        let runtime = TerminalRuntime::new(DesktopBackendRouter::new());
        let state = Self {
            control: Mutex::new(RuntimeControlPlane::new(runtime, token.clone())),
            store: Mutex::new(SqliteStore::in_memory()?),
            browser: Mutex::new(Box::new(InMemoryBrowserAutomation::new())),
            browser_failures: Mutex::new(VecDeque::new()),
            browser_failure_counter: Mutex::new(0),
            config_path: unique_temp_config_path(),
            control_token: token,
            desktop_notifications: Mutex::new(DesktopNotificationState::default()),
            output_channels: Mutex::new(HashMap::new()),
            output_pump_hot_until: Mutex::new(None),
            output_stream_metrics: Mutex::new(OutputStreamMetrics::default()),
            output_pressure: Mutex::new(HashMap::new()),
            input_command_buffers: Mutex::new(HashMap::new()),
        };
        Ok(state)
    }

    pub fn control_token(&self) -> &str {
        &self.control_token
    }

    pub fn set_desktop_notification_adapter(&self, adapter: Arc<dyn DesktopNotificationAdapter>) {
        if let Ok(mut state) = self.desktop_notifications.lock() {
            state.adapter = Some(adapter);
        }
    }

    /// Registers a live-output channel for one renderer subscription.
    /// Multiple subscriptions can coexist for a session during remount races.
    pub fn register_output_channel(
        &self,
        session_id: String,
        subscription_id: String,
        channel: Channel<OutputStreamFrame>,
    ) {
        if let Ok(mut channels) = self.output_channels.lock() {
            channels
                .entry(session_id)
                .or_default()
                .insert(subscription_id, channel);
        }
    }

    /// Removes a session's live-output channel (renderer unmounted / session
    /// closed). Idempotent.
    pub fn unregister_output_channel(&self, session_id: &str, subscription_id: &str) {
        if let Ok(mut channels) = self.output_channels.lock() {
            let should_remove_session = channels
                .get_mut(session_id)
                .map(|session_channels| {
                    session_channels.remove(subscription_id);
                    session_channels.is_empty()
                })
                .unwrap_or(false);
            if should_remove_session {
                channels.remove(session_id);
            }
        }
        if let Ok(mut pressure) = self.output_pressure.lock() {
            pressure.remove(session_id);
        }
        if let Ok(mut buffers) = self.input_command_buffers.lock() {
            buffers.remove(session_id);
        }
    }

    pub fn report_output_pressure(
        &self,
        session_id: String,
        queued_bytes: u64,
        max_queued_bytes: u64,
        backpressure_events: u64,
        write_in_flight: bool,
    ) {
        if let Ok(mut pressure) = self.output_pressure.lock() {
            pressure.insert(
                session_id.clone(),
                OutputPressureRecord {
                    queued_bytes,
                    max_queued_bytes,
                    backpressure_events,
                    write_in_flight,
                },
            );
        }
        let pause = write_in_flight && queued_bytes >= OUTPUT_FLOW_CONTROL_PAUSE_BYTES;
        let resume = !write_in_flight || queued_bytes <= OUTPUT_FLOW_CONTROL_RESUME_BYTES;
        if pause || resume {
            if let Ok(mut control) = self.control.lock() {
                let _ = control
                    .runtime_mut()
                    .set_output_paused(&SessionId::from_string(&session_id), pause && !resume);
            }
        }
    }

    fn mark_output_pump_hot(&self) {
        if let Ok(mut hot_until) = self.output_pump_hot_until.lock() {
            *hot_until = Some(Instant::now() + Duration::from_millis(OUTPUT_STREAM_HOT_WINDOW_MS));
        }
    }

    pub fn output_stream_pump_delay(&self, had_output: bool) -> Duration {
        if had_output {
            self.mark_output_pump_hot();
            return Duration::from_millis(OUTPUT_STREAM_HOT_PUMP_MS);
        }
        let Ok(mut hot_until) = self.output_pump_hot_until.lock() else {
            return Duration::from_millis(OUTPUT_STREAM_IDLE_PUMP_MS);
        };
        let Some(deadline) = *hot_until else {
            return Duration::from_millis(OUTPUT_STREAM_IDLE_PUMP_MS);
        };
        if deadline > Instant::now() {
            Duration::from_millis(OUTPUT_STREAM_HOT_PUMP_MS)
        } else {
            *hot_until = None;
            Duration::from_millis(OUTPUT_STREAM_IDLE_PUMP_MS)
        }
    }

    fn record_output_pump_metrics(
        &self,
        had_activity: bool,
        frames_sent: u64,
        bytes_sent: u64,
        send_failures: u64,
        closed_channels: u64,
    ) {
        if let Ok(mut metrics) = self.output_stream_metrics.lock() {
            metrics.pump_runs = metrics.pump_runs.saturating_add(1);
            if had_activity {
                metrics.pump_active_runs = metrics.pump_active_runs.saturating_add(1);
            } else {
                metrics.pump_idle_runs = metrics.pump_idle_runs.saturating_add(1);
            }
            metrics.frames_sent = metrics.frames_sent.saturating_add(frames_sent);
            metrics.bytes_sent = metrics.bytes_sent.saturating_add(bytes_sent);
            metrics.send_failures = metrics.send_failures.saturating_add(send_failures);
            metrics.closed_channels = metrics.closed_channels.saturating_add(closed_channels);
            if frames_sent > 0 {
                metrics.last_frame_at = Some(timestamp());
            }
        }
    }

    fn output_stream_diagnostics(&self) -> DiagnosticsOutputStreamResult {
        let (active_sessions, active_subscriptions) = self
            .output_channels
            .lock()
            .map(|channels| {
                (
                    channels.len(),
                    channels
                        .values()
                        .map(|session_channels| session_channels.len())
                        .sum::<usize>(),
                )
            })
            .unwrap_or((0, 0));
        let metrics = self
            .output_stream_metrics
            .lock()
            .map(|metrics| metrics.clone())
            .unwrap_or_default();
        let (
            renderer_queued_bytes,
            renderer_max_queued_bytes,
            renderer_backpressure_events,
            renderer_write_in_flight_sessions,
        ) = self
            .output_pressure
            .lock()
            .map(|pressure| {
                pressure.values().fold(
                    (0_u64, 0_u64, 0_u64, 0_usize),
                    |(queued, max_queued, backpressure, in_flight), record| {
                        (
                            queued.saturating_add(record.queued_bytes),
                            max_queued.max(record.max_queued_bytes),
                            backpressure.saturating_add(record.backpressure_events),
                            in_flight + usize::from(record.write_in_flight),
                        )
                    },
                )
            })
            .unwrap_or((0, 0, 0, 0));
        DiagnosticsOutputStreamResult {
            active_sessions,
            active_subscriptions,
            frames_sent: metrics.frames_sent,
            bytes_sent: metrics.bytes_sent,
            send_failures: metrics.send_failures,
            closed_channels: metrics.closed_channels,
            pump_runs: metrics.pump_runs,
            pump_active_runs: metrics.pump_active_runs,
            pump_idle_runs: metrics.pump_idle_runs,
            last_frame_at: metrics.last_frame_at,
            renderer_queued_bytes,
            renderer_max_queued_bytes,
            renderer_backpressure_events,
            renderer_write_in_flight_sessions,
        }
    }

    /// Fast path for interactive terminal input from the desktop WebView. This
    /// avoids building a full control-plane JSON envelope for every keystroke.
    pub fn send_text_direct(&self, session_id: &str, text: String) -> Result<(), DesktopHostError> {
        let Ok(mut control) = self.control.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop control state is unavailable".to_string(),
            ));
        };
        control
            .runtime_mut()
            .send_text(&SessionId::from_string(session_id), text.clone())
            .map_err(|error| DesktopHostError::StateUnavailable(error.to_string()))?;
        control.collect_events();
        drop(control);
        self.mark_output_pump_hot();
        self.detect_agent_launch_from_terminal_input(session_id, &text);
        Ok(())
    }

    pub fn send_paste_direct(
        &self,
        session_id: &str,
        text: String,
        bracketed: bool,
    ) -> Result<(), DesktopHostError> {
        let Ok(mut control) = self.control.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop control state is unavailable".to_string(),
            ));
        };
        control
            .runtime_mut()
            .send_paste(&SessionId::from_string(session_id), text.clone(), bracketed)
            .map_err(|error| DesktopHostError::StateUnavailable(error.to_string()))?;
        control.collect_events();
        drop(control);
        self.mark_output_pump_hot();
        if text.contains('\r') || text.contains('\n') {
            self.detect_agent_launch_from_terminal_input(session_id, &text);
        }
        Ok(())
    }

    /// Drains coalesced terminal-output deltas from the control plane and pushes
    /// them to each session's registered channel as base64 frames. Run on a
    /// short timer by the host's background pump, so a stream-first renderer
    /// needs no output polling. Channels that fail to send (renderer gone) are
    /// dropped.
    pub fn pump_output_stream(&self) -> bool {
        let (deltas, cwd_updates) = {
            let Ok(mut control) = self.control.lock() else {
                self.record_output_pump_metrics(false, 0, 0, 0, 0);
                return false;
            };
            control.collect_events();
            (control.drain_output_stream(), control.drain_cwd_updates())
        };
        let had_activity = !deltas.is_empty() || !cwd_updates.is_empty();
        // Persist live cwd updates (OSC 7) so the footer git status tracks the
        // directory the shell has cd'd into. Best-effort; skip on contention.
        if !cwd_updates.is_empty() {
            if let Ok(mut store) = self.store.lock() {
                let now = timestamp();
                for (session_id, cwd) in &cwd_updates {
                    let _ = store.update_session_cwd(session_id, Some(cwd), &now);
                }
            }
        }
        if deltas.is_empty() {
            self.record_output_pump_metrics(had_activity, 0, 0, 0, 0);
            return had_activity;
        }
        // Snapshot (session_id, subscription_id, channel, frame) tuples while
        // holding the lock, then release the lock before calling channel.send().
        // This prevents a blocking send from stalling register_output_channel /
        // unregister_output_channel, which also need the same lock.
        let to_send: Vec<(String, String, Channel<OutputStreamFrame>, OutputStreamFrame)> = {
            let Ok(channels) = self.output_channels.lock() else {
                self.record_output_pump_metrics(had_activity, 0, 0, 0, 0);
                return had_activity;
            };
            let mut snapshot = Vec::new();
            for delta in &deltas {
                let session_id = delta.session_id.to_string();
                let Some(session_channels) = channels.get(&session_id) else {
                    continue;
                };
                let frame = OutputStreamFrame {
                    from_offset: delta.from_offset,
                    bytes_base64: BASE64_STANDARD.encode(&delta.bytes),
                };
                for (subscription_id, channel) in session_channels {
                    snapshot.push((
                        session_id.clone(),
                        subscription_id.clone(),
                        channel.clone(),
                        frame.clone(),
                    ));
                }
            }
            snapshot
        }; // lock released here
        let mut closed: Vec<(String, String)> = Vec::new();
        let mut frames_sent = 0_u64;
        let mut bytes_sent = 0_u64;
        let mut send_failures = 0_u64;
        for (session_id, subscription_id, channel, frame) in to_send {
            let byte_len = frame.bytes_base64.len() as u64;
            if channel.send(frame).is_err() {
                send_failures = send_failures.saturating_add(1);
                closed.push((session_id, subscription_id));
            } else {
                frames_sent = frames_sent.saturating_add(1);
                bytes_sent = bytes_sent.saturating_add(byte_len);
            }
        }
        let closed_channels = closed.len() as u64;
        if !closed.is_empty() {
            if let Ok(mut channels) = self.output_channels.lock() {
                for (session_id, subscription_id) in closed {
                    let should_remove_session = channels
                        .get_mut(&session_id)
                        .map(|session_channels| {
                            session_channels.remove(&subscription_id);
                            session_channels.is_empty()
                        })
                        .unwrap_or(false);
                    if should_remove_session {
                        channels.remove(&session_id);
                        // Also clear the stale pressure record so a future session
                        // with the same id is not incorrectly flow-controlled.
                        if let Ok(mut pressure) = self.output_pressure.lock() {
                            pressure.remove(&session_id);
                        }
                    }
                }
            }
        }
        self.record_output_pump_metrics(
            had_activity,
            frames_sent,
            bytes_sent,
            send_failures,
            closed_channels,
        );
        had_activity
    }

    pub fn recovery_snapshot(&self) -> Result<RecoverySnapshot, DesktopHostError> {
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        store
            .load_recovery_snapshot()
            .map_err(DesktopHostError::from)
    }

    pub fn handle_request(&self, request: RequestEnvelope) -> ResponseEnvelope {
        if request.method == "actions.run" {
            return self.handle_actions_run_request(request);
        }

        if is_desktop_store_method(&request.method) {
            return self.handle_desktop_store_request(request);
        }

        let id = request.id.clone();
        let request = match self.prepare_runtime_request(request) {
            Ok(request) => request,
            Err(error) => return ResponseEnvelope::error(id, error),
        };
        let request_for_persistence = request.clone();
        let Ok(mut control) = self.control.lock() else {
            return ResponseEnvelope::error(
                id,
                ControlError::new(ErrorCode::Conflict, "Desktop control state is unavailable."),
            );
        };

        if runtime_request_needs_pre_dispatch_collect(&request.method) {
            control.collect_events();
            if let Some(error) = self.persist_agent_snapshots(&control, &id) {
                return error;
            }
        }
        let response = control.handle_request(request);
        control.collect_events();
        if let Some(error) = self.persist_agent_snapshots(&control, &id) {
            return error;
        }
        if let Some(error) =
            self.persist_after_request(&mut control, &request_for_persistence, &response)
        {
            return error;
        }
        response
    }

    fn prepare_runtime_request(
        &self,
        request: RequestEnvelope,
    ) -> Result<RequestEnvelope, ControlError> {
        if request.method != "session.spawn" {
            return Ok(request);
        }

        let mut params: SessionSpawnParams = request.parse_params()?;
        let surface_id = SurfaceId::new().to_string();
        let pane_id = self.resolve_spawn_pane_id(&params)?;
        if params.pane_id.is_none() {
            params.pane_id = Some(pane_id.clone());
        }
        if Self::should_default_conpty_cwd_to_home(&params) {
            params.cwd = default_windows_shell_cwd();
        }
        let mut env = std::mem::take(&mut params.env);
        let extra_wsl_env_keys = env
            .iter()
            .filter_map(|entry| wsl_env_key(&entry.key))
            .collect::<Vec<_>>();
        env.extend(managed_terminal_env(
            &params.workspace_id,
            &self.control_token,
            &surface_id,
            &pane_id,
            &extra_wsl_env_keys,
        ));
        params.env = env;
        let params_json = serde_json::to_string(&params).map_err(|error| {
            ControlError::new(
                ErrorCode::InvalidRequest,
                format!("Failed to encode session.spawn params: {error}"),
            )
        })?;
        Ok(RequestEnvelope {
            params_json,
            ..request
        })
    }

    fn should_default_conpty_cwd_to_home(params: &SessionSpawnParams) -> bool {
        let backend = params.backend.as_deref().unwrap_or("conpty").trim();
        backend == "conpty"
            && params
                .cwd
                .as_deref()
                .map(str::trim)
                .filter(|cwd| !cwd.is_empty())
                .is_none()
    }

    fn resolve_spawn_pane_id(&self, params: &SessionSpawnParams) -> Result<String, ControlError> {
        if let Some(pane_id) = params
            .pane_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(pane_id.to_string());
        }

        if params.placement.as_deref() == Some("new_tab") {
            return Ok(PaneId::new().to_string());
        }

        let store = self.store.lock().map_err(|_| {
            ControlError::new(ErrorCode::Conflict, "Desktop store state is unavailable.")
        })?;
        if let Some(bundle) =
            store
                .load_workspace_bundle(&params.workspace_id)
                .map_err(|error| {
                    ControlError::new(
                        ErrorCode::InvalidRequest,
                        format!("Failed to load workspace context: {error}"),
                    )
                })?
        {
            return Ok(bundle.workspace.active_pane_id);
        }

        Ok(PaneId::new().to_string())
    }

    pub fn handle_pipe_connection(
        &self,
        request: RequestEnvelope,
        mut connection: ControlPipeConnection,
    ) -> io::Result<()> {
        if request.method == "events.subscribe" {
            return self.stream_event_subscription(request, &mut connection);
        }

        let response = self.handle_request(request);
        connection.write_response(&response)
    }

    fn stream_event_subscription(
        &self,
        request: RequestEnvelope,
        connection: &mut ControlPipeConnection,
    ) -> io::Result<()> {
        let (params, mut cursor, response) = match self.begin_event_subscription(&request) {
            Ok(subscription) => subscription,
            Err(error) => {
                connection.write_response(&ResponseEnvelope::error(request.id, error))?;
                return Ok(());
            }
        };
        connection.write_response(&response)?;

        loop {
            let events = match self.event_subscription_batch(&params, &cursor) {
                Ok(events) => events,
                Err(error) => {
                    connection
                        .write_response(&ResponseEnvelope::error(request.id.clone(), error))?;
                    return Ok(());
                }
            };

            for event in events {
                cursor = event.event_id.clone();
                connection.write_event(&event)?;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn begin_event_subscription(
        &self,
        request: &RequestEnvelope,
    ) -> Result<(EventSubscribeParams, String, ResponseEnvelope), ControlError> {
        validate_desktop_request(request, &self.control_token)?;
        let params: EventSubscribeParams = request.parse_params()?;
        let Ok(mut control) = self.control.lock() else {
            return Err(ControlError::new(
                ErrorCode::Conflict,
                "Desktop control state is unavailable.",
            ));
        };
        control.collect_events();
        let cursor = if let Some(after_event_id) = params.after_event_id.as_deref() {
            control.validate_event_cursor(after_event_id)?;
            after_event_id.to_string()
        } else {
            control.current_event_cursor()
        };
        let response = ResponseEnvelope::ok_typed(
            request.id.clone(),
            &EventSubscribeResult {
                subscribed: true,
                cursor: cursor.clone(),
                dropped_count: control.dropped_event_count(),
            },
        );
        Ok((params, cursor, response))
    }

    fn event_subscription_batch(
        &self,
        params: &EventSubscribeParams,
        cursor: &str,
    ) -> Result<Vec<agentmux_ipc::EventFrame>, ControlError> {
        let Ok(mut control) = self.control.lock() else {
            return Err(ControlError::new(
                ErrorCode::Conflict,
                "Desktop control state is unavailable.",
            ));
        };
        control.event_subscription_batch(params, cursor, 256)
    }

    pub fn recover_durable_sessions(&self) {
        let Ok(snapshot) = self.recovery_snapshot() else {
            return;
        };
        let recoverable_agents = self.recoverable_agent_states_by_session();
        let mounted_terminals = mounted_terminal_surfaces_by_session(&snapshot);
        let workspace_profiles = workspace_backend_profiles(&snapshot);

        for session in snapshot.sessions.iter() {
            if !should_attach_recovering_session(session) {
                continue;
            }
            let Some(backend_ref) = session.backend_native_id.clone() else {
                continue;
            };
            let backend_profile = workspace_profiles
                .get(&session.workspace_id)
                .cloned()
                .flatten();
            let should_try_attach =
                tmux_session_exists(backend_profile.as_deref(), &backend_ref) != Some(false);
            let params_json = serde_json::json!({
                "session_id": session.session_id.clone(),
                "workspace_id": session.workspace_id.clone(),
                "backend": session.backend_kind.clone(),
                "backend_profile": backend_profile.clone(),
                "backend_ref": backend_ref,
                "columns": 120,
                "rows": 30,
                "durability": "durable",
            })
            .to_string();

            if should_try_attach {
                let response = self.handle_request(RequestEnvelope::new(
                    "desktop_startup_recover_durable_session",
                    "session.attach",
                    params_json,
                    self.control_token.clone(),
                ));
                if matches!(response.outcome, ResponseOutcome::Ok { .. }) {
                    let Ok(result) = response_result_json::<SessionSpawnResult>(&response) else {
                        continue;
                    };
                    if self.wait_for_recovered_tmux_attach(&result.session_id) {
                        let _ = self.persist_session_summary_from_id(result.session_id.clone());
                        if let Some(agent_state) = recoverable_agents.get(&session.session_id) {
                            self.replay_restored_agent_state(
                                &result.session_id,
                                &session.command,
                                agent_state,
                                None,
                            );
                        }
                        continue;
                    }
                    self.terminate_runtime_session(&result.session_id, TerminationMode::Kill);
                    self.mark_persisted_session_disconnected(&session.session_id);
                }
            }

            let Some((pane_id, surface_id)) = mounted_terminals.get(&session.session_id) else {
                continue;
            };
            if session.command.is_empty() {
                continue;
            }
            let agent_state = recoverable_agents.get(&session.session_id);
            if let Some(result) = self.respawn_persisted_terminal_into_pane(
                session,
                pane_id,
                backend_profile,
                "durable",
                "desktop_startup_respawn_durable_session",
                agent_state,
            ) {
                if self.wait_for_recovered_tmux_attach(&result.session_id) {
                    let _ = self.persist_session_summary_from_id(result.session_id.clone());
                    if let Some(agent_state) = agent_state {
                        self.replay_restored_agent_launch_command(
                            &result.session_id,
                            session,
                            agent_state,
                        );
                        self.replay_restored_agent_state(
                            &result.session_id,
                            &session.command,
                            agent_state,
                            Some("running"),
                        );
                    }
                    self.delete_superseded_terminal(surface_id, &session.session_id);
                } else {
                    self.terminate_runtime_session(&result.session_id, TerminationMode::Kill);
                    self.discard_failed_recovery_spawn(
                        &session.workspace_id,
                        pane_id,
                        surface_id,
                        &session.session_id,
                        &result.session_id,
                    );
                }
            } else {
                self.mark_persisted_session_disconnected(&session.session_id);
            }
        }
    }

    fn wait_for_recovered_tmux_attach(&self, session_id: &str) -> bool {
        let deadline = Instant::now() + Duration::from_millis(DURABLE_TMUX_ATTACH_READY_TIMEOUT_MS);
        loop {
            let state = {
                let Ok(mut control) = self.control.lock() else {
                    return false;
                };
                control.collect_events();
                try_session_summary(&mut control, session_id, &self.control_token)
                    .ok()
                    .flatten()
                    .map(|summary| summary.state)
            };
            match state.as_deref() {
                Some("running") => return true,
                Some("failed" | "exited" | "lost" | "disconnected") | None => return false,
                _ if Instant::now() >= deadline => return false,
                _ => thread::sleep(Duration::from_millis(DURABLE_TMUX_ATTACH_READY_POLL_MS)),
            }
        }
    }

    fn terminate_runtime_session(&self, session_id: &str, mode: TerminationMode) {
        let mode = match mode {
            TerminationMode::Soft => "soft",
            TerminationMode::Interrupt => "interrupt",
            TerminationMode::Kill => "kill",
        };
        let params_json = serde_json::json!({
            "session_id": session_id,
            "mode": mode,
        })
        .to_string();
        let _ = self.handle_request(RequestEnvelope::new(
            "desktop_startup_discard_unhealthy_tmux_attach",
            "session.terminate",
            params_json,
            self.control_token.clone(),
        ));
    }

    fn mark_persisted_session_disconnected(&self, session_id: &str) {
        let Ok(mut store) = self.store.lock() else {
            return;
        };
        let _ = store.update_session_state(session_id, "disconnected", None, &timestamp());
    }

    fn discard_failed_recovery_spawn(
        &self,
        workspace_id: &str,
        pane_id: &str,
        previous_surface_id: &str,
        previous_session_id: &str,
        failed_session_id: &str,
    ) {
        let Ok(mut store) = self.store.lock() else {
            return;
        };
        let Ok(Some(mut bundle)) = store.load_workspace_bundle(workspace_id) else {
            return;
        };
        let now = timestamp();
        let failed_surface_ids = bundle
            .surfaces
            .iter()
            .filter(|surface| surface.session_id.as_deref() == Some(failed_session_id))
            .map(|surface| surface.surface_id.clone())
            .collect::<HashSet<_>>();
        bundle
            .surfaces
            .retain(|surface| !failed_surface_ids.contains(&surface.surface_id));
        bundle
            .sessions
            .retain(|session| session.session_id != failed_session_id);
        if let Some(pane) = bundle.panes.iter_mut().find(|pane| pane.pane_id == pane_id) {
            pane.mounted_surface_id = Some(previous_surface_id.to_string());
            pane.updated_at = now.clone();
        }
        if let Some(session) = bundle
            .sessions
            .iter_mut()
            .find(|session| session.session_id == previous_session_id)
        {
            session.state = "disconnected".to_string();
            session.exit_code = None;
            session.updated_at = now.clone();
        }
        bundle.workspace.updated_at = now;
        let _ = store.save_workspace_bundle(&bundle);
    }

    /// Reconcile persisted ephemeral sessions on startup.
    ///
    /// Ephemeral terminals (ConPTY / direct WSL) cannot outlive the app: when a
    /// previous run exits, their backend processes die, but the store still has
    /// them at `state='running'`. On the next launch the UI would render a live
    /// terminal for each, whose `session.snapshot` can only ever return
    /// SessionNotFound — a pane stuck "starting…" forever. Mark them
    /// `disconnected` so the UI shows a reopenable empty pane instead. Durable
    /// (tmux) sessions are untouched; `recover_durable_sessions` re-attaches them.
    pub fn reconcile_orphaned_ephemeral_sessions(&self) {
        let Ok(mut store) = self.store.lock() else {
            return;
        };
        let Ok(sessions) = store.list_sessions() else {
            return;
        };
        let now = timestamp();
        for session in sessions {
            if session.durability != "durable"
                && !matches!(
                    session.state.as_str(),
                    "exited" | "failed" | "lost" | "disconnected"
                )
            {
                let _ = store.update_session_state(
                    &session.session_id,
                    "disconnected",
                    session.exit_code,
                    &now,
                );
            }
        }
    }

    /// Restore ephemeral terminals after a restart.
    ///
    /// Ephemeral terminal processes (ConPTY / direct WSL) don't survive app exit;
    /// `reconcile_orphaned_ephemeral_sessions` marks their sessions 'disconnected'.
    /// Re-spawn each persisted terminal command into its original leaf pane, then
    /// delete the superseded dead surface/session so repeated app restarts remain
    /// idempotent.
    pub fn restore_ephemeral_terminals(&self) {
        let Ok(snapshot) = self.recovery_snapshot() else {
            return;
        };
        let recoverable_agents = self.recoverable_agent_states_by_session();
        let workspace_profiles = workspace_backend_profiles(&snapshot);
        const MAX_RESTORE: usize = 32;
        let mut restored = 0usize;
        for pane in &snapshot.panes {
            if restored >= MAX_RESTORE {
                break;
            }
            if pane.kind != "leaf" {
                continue;
            }
            let Some(surface_id) = pane.mounted_surface_id.as_ref() else {
                continue;
            };
            let Some(surface) = snapshot
                .surfaces
                .iter()
                .find(|s| &s.surface_id == surface_id && s.surface_type == "terminal")
            else {
                continue;
            };
            let Some(session_id) = surface.session_id.as_ref() else {
                continue;
            };
            // Restore disconnected ephemeral terminal processes. If the session
            // had an active agent, restore it even if the last persisted state
            // was not normalized as disconnected.
            let Some(session) = snapshot.sessions.iter().find(|s| {
                &s.session_id == session_id
                    && s.durability != "durable"
                    && matches!(s.backend_kind.as_str(), "conpty" | "wsl-direct")
                    && !s.command.is_empty()
                    && (s.state == "disconnected"
                        || recoverable_agents.contains_key(s.session_id.as_str()))
            }) else {
                continue;
            };
            let backend_profile = workspace_profiles
                .get(&session.workspace_id)
                .cloned()
                .flatten();
            if let Some(result) = self.respawn_persisted_terminal_into_pane(
                session,
                &pane.pane_id,
                backend_profile,
                "ephemeral",
                "desktop_startup_restore_terminal",
                recoverable_agents.get(&session.session_id),
            ) {
                if let Some(agent_state) = recoverable_agents.get(&session.session_id) {
                    self.replay_restored_agent_launch_command(
                        &result.session_id,
                        session,
                        agent_state,
                    );
                    self.replay_restored_agent_state(
                        &result.session_id,
                        &session.command,
                        agent_state,
                        Some("running"),
                    );
                }
                self.delete_superseded_terminal(&surface.surface_id, &session.session_id);
                restored += 1;
            }
        }
    }

    fn recoverable_agent_states_by_session(&self) -> HashMap<String, PersistedAgentState> {
        let Ok(store) = self.store.lock() else {
            return HashMap::new();
        };
        let Ok(states) = store.list_agent_states(None) else {
            return HashMap::new();
        };
        states
            .into_iter()
            .filter(should_restore_agent_state)
            .map(|state| (state.session_id.clone(), state))
            .collect()
    }

    fn restore_cwd_for_session(&self, session: &PersistedSession) -> Option<String> {
        let project_root = self.workspace_project_root_for_restore(&session.workspace_id);
        if let Some(cwd) =
            clean_optional_text(session.cwd.clone()).filter(|cwd| !is_host_process_working_dir(cwd))
        {
            if persisted_command_already_launches_agent(&session.command)
                && is_probably_home_directory(&cwd)
            {
                if let Some(context_cwd) = project_root
                    .clone()
                    .or_else(|| self.workspace_context_cwd_for_restore(session))
                {
                    return Some(context_cwd);
                }
            }
            return Some(cwd);
        }
        project_root.or_else(|| {
            if session.backend_kind == "conpty" {
                default_windows_shell_cwd()
            } else {
                None
            }
        })
    }

    fn workspace_project_root_for_restore(&self, workspace_id: &str) -> Option<String> {
        let Ok(store) = self.store.lock() else {
            return None;
        };
        store
            .load_workspace_bundle(workspace_id)
            .ok()
            .flatten()
            .and_then(|bundle| clean_optional_text(bundle.workspace.project_root))
    }

    fn workspace_context_cwd_for_restore(&self, session: &PersistedSession) -> Option<String> {
        let Ok(store) = self.store.lock() else {
            return None;
        };
        let bundle = store
            .load_workspace_bundle(&session.workspace_id)
            .ok()
            .flatten()?;
        let mut seen = HashSet::new();
        let mut candidate_session_ids = Vec::new();
        if let Some(active_id) = active_session_id(&bundle) {
            candidate_session_ids.push(active_id);
        }
        candidate_session_ids.extend(bundle.sessions.iter().map(|value| value.session_id.clone()));

        for session_id in candidate_session_ids {
            if !seen.insert(session_id.clone()) || session_id == session.session_id {
                continue;
            }
            let Some(cwd) = bundle
                .sessions
                .iter()
                .find(|value| value.session_id == session_id)
                .and_then(|value| clean_optional_text(value.cwd.clone()))
            else {
                continue;
            };
            if !is_host_process_working_dir(&cwd) && !is_probably_home_directory(&cwd) {
                return Some(cwd);
            }
        }

        None
    }

    fn respawn_persisted_terminal_into_pane(
        &self,
        session: &PersistedSession,
        pane_id: &str,
        backend_profile: Option<String>,
        durability: &str,
        request_id: &str,
        agent_state: Option<&PersistedAgentState>,
    ) -> Option<SessionSpawnResult> {
        let command = restored_spawn_command_for_session(session, agent_state);
        let params = serde_json::json!({
            "workspace_id": session.workspace_id.clone(),
            "backend": session.backend_kind.clone(),
            "backend_profile": backend_profile,
            "command": command,
            "cwd": self.restore_cwd_for_session(session),
            "columns": 120,
            "rows": 30,
            "durability": durability,
            "placement": "active_pane",
            "pane_id": pane_id,
        })
        .to_string();
        let response = self.handle_request(RequestEnvelope::new(
            request_id,
            "session.spawn",
            params,
            self.control_token.clone(),
        ));
        response_result_json::<SessionSpawnResult>(&response).ok()
    }

    fn replay_restored_agent_state(
        &self,
        session_id: &str,
        command: &[String],
        previous: &PersistedAgentState,
        state_override: Option<&str>,
    ) {
        let command_label = command.join(" ");
        let restored_label = normalized_restored_agent_command_label(previous)
            .filter(|_| !persisted_command_already_launches_agent(command));
        let label = restored_label.as_deref().unwrap_or(command_label.trim());
        let label = if label.is_empty() { "agent" } else { label };
        let state = state_override.unwrap_or(previous.state.as_str());
        let mut telemetry = previous
            .telemetry_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<AgentTelemetry>(json).ok())
            .unwrap_or_default();
        if telemetry.activity.is_none() {
            telemetry.activity = Some("agent".to_string());
        }
        if restored_label.is_some() || telemetry.session.is_none() {
            telemetry.session = Some(label.to_string());
        }
        let reason = if restored_label.is_some() || state_override.is_some() {
            Some(format!("Agent restored: {label}"))
        } else {
            previous
                .reason
                .clone()
                .or_else(|| Some(format!("Agent restored: {label}")))
        };
        let params = serde_json::json!({
            "session_id": session_id,
            "state": state,
            "reason": reason,
            "telemetry": telemetry,
        })
        .to_string();
        let _ = self.handle_request(RequestEnvelope::new(
            "desktop_startup_replay_agent_state",
            "agent.set_state",
            params,
            self.control_token.clone(),
        ));
    }

    fn replay_restored_agent_launch_command(
        &self,
        session_id: &str,
        session: &PersistedSession,
        previous: &PersistedAgentState,
    ) {
        let Some(command_line) = restored_agent_launch_line(session, previous) else {
            return;
        };
        let params = serde_json::json!({
            "session_id": session_id,
            "text": format!("{command_line}\r"),
        })
        .to_string();
        let _ = self.handle_request(RequestEnvelope::new(
            "desktop_startup_replay_agent_command",
            "session.send_text",
            params,
            self.control_token.clone(),
        ));
    }

    fn delete_superseded_terminal(&self, surface_id: &str, session_id: &str) {
        if let Ok(mut store) = self.store.lock() {
            let _ = store.delete_surface(surface_id);
            let _ = store.delete_session(session_id);
        }
    }

    /// Seed the runtime id counter from the persisted high-water mark so ids
    /// minted this run can't collide with ids already on disk and silently
    /// overwrite them. Must run at startup before anything mints a new id
    /// (spawns, splits, restores). See `agentmux_core::seed_next_id`.
    pub fn seed_id_counter(&self) {
        let Ok(snapshot) = self.recovery_snapshot() else {
            return;
        };
        let max_seq = snapshot
            .workspaces
            .iter()
            .map(|workspace| id_sequence(&workspace.workspace_id))
            .chain(snapshot.panes.iter().map(|pane| id_sequence(&pane.pane_id)))
            .chain(
                snapshot
                    .surfaces
                    .iter()
                    .map(|surface| id_sequence(&surface.surface_id)),
            )
            .chain(
                snapshot
                    .sessions
                    .iter()
                    .map(|session| id_sequence(&session.session_id)),
            )
            .max()
            .unwrap_or(0);
        if max_seq > 0 {
            agentmux_core::seed_next_id(max_seq + 1);
        }
    }

    fn handle_desktop_store_request(&self, request: RequestEnvelope) -> ResponseEnvelope {
        let id = request.id.clone();
        if let Err(error) = validate_desktop_request(&request, &self.control_token) {
            return ResponseEnvelope::error(id, error);
        }

        let response = match request.method.as_str() {
            "system.ping" => self.handle_system_ping(&request),
            "system.capabilities" => self.handle_system_capabilities(&request),
            "system.identify" => self.handle_system_identify(&request),
            "workspace.create" => self.handle_workspace_create(&request),
            "workspace.list" => self.handle_workspace_list(&request),
            "workspace.get" => self.handle_workspace_get(&request),
            "workspace.rename" => self.handle_workspace_rename(&request),
            "workspace.update" => self.handle_workspace_update(&request),
            "workspace.close" => self.handle_workspace_close(&request),
            "workspace_group.list" => self.handle_workspace_group_list(&request),
            "workspace_group.create" => self.handle_workspace_group_create(&request),
            "workspace_group.update" => self.handle_workspace_group_update(&request),
            "workspace_group.delete" => self.handle_workspace_group_delete(&request),
            "workspace_group.add_workspace" => self.handle_workspace_group_add_workspace(&request),
            "workspace_group.remove_workspace" => {
                self.handle_workspace_group_remove_workspace(&request)
            }
            "pane.split" => self.handle_pane_split(&request),
            "pane.focus" => self.handle_pane_focus(&request),
            "pane.close" => self.handle_pane_close(&request),
            "pane.resize_layout" => self.handle_pane_resize_layout(&request),
            "pane.mount_surface" => self.handle_pane_mount_surface(&request),
            "pane.unmount_surface" => self.handle_pane_unmount_surface(&request),
            "surface.create_browser" => self.handle_surface_create_browser(&request),
            "surface.close" => self.handle_surface_close(&request),
            "surface.move_workspace" => self.handle_surface_move_workspace(&request),
            "browser.navigate" => self.handle_browser_navigate(&request),
            "browser.reload" => self.handle_browser_reload(&request),
            "browser.back" => self.handle_browser_back(&request),
            "browser.forward" => self.handle_browser_forward(&request),
            "browser.current_url" => self.handle_browser_current_url(&request),
            "browser.screenshot" => self.handle_browser_screenshot(&request),
            "browser.dom_snapshot" => self.handle_browser_dom_snapshot(&request),
            "browser.frames" => self.handle_browser_frames(&request),
            "browser.storage" => self.handle_browser_storage(&request),
            "browser.cookies" => self.handle_browser_cookies(&request),
            "browser.downloads" => self.handle_browser_downloads(&request),
            "browser.history" => self.handle_browser_history(&request),
            "browser.console" => self.handle_browser_console(&request),
            "browser.dialogs" => self.handle_browser_dialogs(&request),
            "browser.errors" => self.handle_browser_errors(&request),
            "browser.click" => self.handle_browser_click(&request),
            "browser.type" => self.handle_browser_type(&request),
            "browser.fill" => self.handle_browser_fill(&request),
            "browser.press" => self.handle_browser_press(&request),
            "browser.select" => self.handle_browser_select(&request),
            "browser.scroll" => self.handle_browser_scroll(&request),
            "browser.hover" => self.handle_browser_hover(&request),
            "browser.check" => self.handle_browser_check(&request),
            "browser.get" => self.handle_browser_get(&request),
            "browser.find" => self.handle_browser_find(&request),
            "browser.highlight" => self.handle_browser_highlight(&request),
            "browser.focus" => self.handle_browser_focus(&request),
            "browser.zoom" => self.handle_browser_zoom(&request),
            "browser.wait_for_selector" => self.handle_browser_wait_for_selector(&request),
            "browser.evaluate" => self.handle_browser_evaluate(&request),
            "agent.get_state" => self.handle_agent_get_state(&request),
            "agent.list_attention" => self.handle_agent_list_attention(&request),
            "agent.list" => self.handle_agent_list(&request),
            "actions.list" => self.handle_actions_list(&request),
            "notification.create" => self.handle_notification_create(&request),
            "notification.list" => self.handle_notification_list(&request),
            "notification.dismiss" => self.handle_notification_dismiss(&request),
            "notification.clear" => self.handle_notification_clear(&request),
            "team.task.list" => self.handle_team_task_list(&request),
            "team.task.create" => self.handle_team_task_create(&request),
            "team.task.claim" => self.handle_team_task_claim(&request),
            "team.task.complete" => self.handle_team_task_complete(&request),
            "team.task.block" => self.handle_team_task_block(&request),
            "team.task.unblock" => self.handle_team_task_unblock(&request),
            "team.task.set_dependency" => self.handle_team_task_set_dependency(&request),
            "team.message.list" => self.handle_team_message_list(&request),
            "team.message.send" => self.handle_team_message_send(&request),
            "team.message.mark_read" => self.handle_team_message_mark_read(&request),
            "sidebar.set_status" => self.handle_sidebar_set_status(&request),
            "sidebar.clear_status" => self.handle_sidebar_clear_status(&request),
            "sidebar.list_status" => self.handle_sidebar_list_status(&request),
            "sidebar.set_progress" => self.handle_sidebar_set_progress(&request),
            "sidebar.clear_progress" => self.handle_sidebar_clear_progress(&request),
            "sidebar.log" => self.handle_sidebar_log(&request),
            "sidebar.clear_log" => self.handle_sidebar_clear_log(&request),
            "sidebar.list_log" => self.handle_sidebar_list_log(&request),
            "sidebar.state" => self.handle_sidebar_state(&request),
            "profile.list" => self.handle_profile_list(&request),
            "profile.create" => self.handle_profile_create(&request),
            "profile.update" => self.handle_profile_update(&request),
            "profile.delete" => self.handle_profile_delete(&request),
            "config.get" => self.handle_config_get(&request),
            "config.reload" => self.handle_config_reload(&request),
            "config.update" => self.handle_config_update(&request),
            "config.export" => self.handle_config_export(&request),
            "config.import" => self.handle_config_import(&request),
            "config.reset" => self.handle_config_reset(&request),
            "config.migrate_project" => self.handle_config_migrate_project(&request),
            "config.diagnostics" => self.handle_config_diagnostics(&request),
            "dock.get" => self.handle_dock_get(&request),
            "dock.trust" => self.handle_dock_trust(&request),
            "diagnostics.browser" => self.handle_browser_diagnostics(&request),
            "diagnostics.export" => self.handle_diagnostics_export(&request),
            "diagnostics.recovery" => self.handle_recovery_diagnostics(&request),
            "diagnostics.wsl_distributions" => self.handle_wsl_distributions(&request),
            "diagnostics.tmux" => self.handle_tmux_diagnostics(&request),
            _ => Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::UnsupportedMethod,
                format!("Unsupported method '{}'.", request.method),
            ))),
        };

        match response {
            Ok(response) => response,
            Err(error) => ResponseEnvelope::error(id, control_error_from_host(error)),
        }
    }

    fn persist_after_request(
        &self,
        control: &mut DesktopRuntimeControl,
        request: &RequestEnvelope,
        response: &ResponseEnvelope,
    ) -> Option<ResponseEnvelope> {
        if !matches!(response.outcome, ResponseOutcome::Ok { .. }) {
            return None;
        }

        let result = match request.method.as_str() {
            "session.spawn" => self.persist_spawn(control, request, response),
            "session.attach" => self.persist_attach(control, request, response),
            "session.get" => self.persist_session_summary(response),
            "session.send_text" => {
                self.persist_detected_agent_launch_from_send_text(control, request)
            }
            "agent.set_state" => self.persist_agent_set_state(control, response),
            "agent.clear_attention" => self.persist_agent_clear_attention(request),
            _ => Ok(()),
        };

        match result {
            Ok(()) => None,
            Err(error) => Some(ResponseEnvelope::error(
                response.id.clone(),
                ControlError::new(
                    ErrorCode::Conflict,
                    format!("Failed to persist desktop session metadata: {error}"),
                ),
            )),
        }
    }

    fn persist_spawn(
        &self,
        control: &mut DesktopRuntimeControl,
        request: &RequestEnvelope,
        response: &ResponseEnvelope,
    ) -> Result<(), DesktopHostError> {
        let params: SessionSpawnParams = request.parse_params()?;
        let result: SessionSpawnResult = response_result_json(response)?;
        let summary = session_summary(control, &result.session_id, &self.control_token)?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let existing = store.load_workspace_bundle(&params.workspace_id)?;
        let bundle = workspace_bundle_from_spawn(&params, &result, &summary, existing);
        store.save_workspace_bundle(&bundle)?;
        drop(store);
        let command_label = params.command.join(" ");
        if is_known_agent_launch(&command_label) {
            let _ = self.apply_detected_agent_launch(control, &result.session_id, &command_label);
        }
        Ok(())
    }

    fn persist_attach(
        &self,
        control: &mut DesktopRuntimeControl,
        request: &RequestEnvelope,
        response: &ResponseEnvelope,
    ) -> Result<(), DesktopHostError> {
        let params: SessionAttachParams = request.parse_params()?;
        let result: SessionSpawnResult = response_result_json(response)?;
        let summary = session_summary(control, &result.session_id, &self.control_token)?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let Some(mut bundle) = store.load_workspace_bundle(&params.workspace_id)? else {
            return Ok(());
        };
        let now = timestamp();
        if let Some(session) = bundle
            .sessions
            .iter_mut()
            .find(|session| session.session_id == result.session_id)
        {
            session.backend_kind = summary.backend_kind;
            session.backend_native_id = summary
                .backend_native_id
                .or_else(|| Some(params.backend_ref.clone()));
            session.state = summary.state;
            session.exit_code = summary.exit_code;
            session.durability = params
                .durability
                .clone()
                .unwrap_or_else(|| session.durability.clone());
            session.last_seen_at = Some(now.clone());
            session.updated_at = now.clone();
            if session.command.is_empty() {
                session.command = vec!["attach".to_string(), params.backend_ref.clone()];
            }
        } else {
            bundle.sessions.push(PersistedSession {
                session_id: result.session_id.clone(),
                workspace_id: params.workspace_id.clone(),
                backend_kind: summary.backend_kind,
                backend_attachment_id: None,
                backend_native_id: summary
                    .backend_native_id
                    .or_else(|| Some(params.backend_ref.clone())),
                cwd: None,
                command: vec!["attach".to_string(), params.backend_ref.clone()],
                state: summary.state,
                exit_code: summary.exit_code,
                durability: params
                    .durability
                    .clone()
                    .unwrap_or_else(|| "durable".to_string()),
                created_at: now.clone(),
                last_seen_at: Some(now.clone()),
                updated_at: now.clone(),
            });
        }
        bundle.workspace.updated_at = now;
        store.save_workspace_bundle(&bundle)?;
        Ok(())
    }

    fn persist_session_summary(&self, response: &ResponseEnvelope) -> Result<(), DesktopHostError> {
        let summary: SessionSummaryResult = response_result_json(response)?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        store.update_session_state(
            &summary.session_id,
            &summary.state,
            summary.exit_code,
            &timestamp(),
        )?;
        store.update_session_cwd(&summary.session_id, summary.cwd.as_deref(), &timestamp())?;
        Ok(())
    }

    fn persist_detected_agent_launch_from_send_text(
        &self,
        control: &mut DesktopRuntimeControl,
        request: &RequestEnvelope,
    ) -> Result<(), DesktopHostError> {
        let params: SessionSendTextParams = request.parse_params()?;
        for label in self.completed_terminal_input_lines(&params.session_id, &params.text) {
            let _ = self.apply_detected_agent_launch(control, &params.session_id, &label);
        }
        Ok(())
    }

    fn detect_agent_launch_from_terminal_input(&self, session_id: &str, text: &str) {
        let labels = self.completed_terminal_input_lines(session_id, text);
        if labels.is_empty() {
            return;
        }
        let Ok(mut control) = self.control.lock() else {
            return;
        };
        for label in labels {
            let _ = self.apply_detected_agent_launch(&mut control, session_id, &label);
        }
    }

    fn completed_terminal_input_lines(&self, session_id: &str, text: &str) -> Vec<String> {
        const MAX_TRACKED_COMMAND_BYTES: usize = 4096;
        let Ok(mut buffers) = self.input_command_buffers.lock() else {
            return Vec::new();
        };
        let buffer = buffers.entry(session_id.to_string()).or_default();
        let mut completed = Vec::new();
        let mut previous_was_newline = false;

        for ch in text.chars() {
            match ch {
                '\r' | '\n' => {
                    if previous_was_newline && buffer.is_empty() {
                        continue;
                    }
                    let line = buffer.trim();
                    if !line.is_empty() && is_known_agent_launch(line) {
                        completed.push(line.to_string());
                    }
                    buffer.clear();
                    previous_was_newline = true;
                }
                '\u{8}' | '\u{7f}' => {
                    buffer.pop();
                    previous_was_newline = false;
                }
                '\u{3}' | '\u{4}' => {
                    buffer.clear();
                    previous_was_newline = false;
                }
                '\t' => {
                    if buffer.len() < MAX_TRACKED_COMMAND_BYTES {
                        buffer.push(ch);
                    } else {
                        buffer.clear();
                    }
                    previous_was_newline = false;
                }
                other if !other.is_control() => {
                    if buffer.len() < MAX_TRACKED_COMMAND_BYTES {
                        buffer.push(other);
                    } else {
                        buffer.clear();
                    }
                    previous_was_newline = false;
                }
                _ => {
                    previous_was_newline = false;
                }
            }
        }

        completed
    }

    fn apply_detected_agent_launch(
        &self,
        control: &mut DesktopRuntimeControl,
        session_id: &str,
        label: &str,
    ) -> Result<(), DesktopHostError> {
        let trimmed = label.trim();
        if trimmed.is_empty() || !is_known_agent_launch(trimmed) {
            return Ok(());
        }
        let agent = agent_command_name(first_command_word(trimmed).unwrap_or(trimmed));
        let params = serde_json::json!({
            "session_id": session_id,
            "state": "running",
            "reason": format!("Agent started: {trimmed}"),
            "telemetry": {
                "activity": "agent",
                "session": trimmed,
                "ctx": agent,
            },
        })
        .to_string();
        let response = control.handle_request(RequestEnvelope::new(
            "desktop_detect_agent_launch_from_input",
            "agent.set_state",
            params,
            self.control_token.clone(),
        ));
        self.persist_agent_set_state(control, &response)
    }

    fn persist_session_summary_from_id(&self, session_id: String) -> Result<(), DesktopHostError> {
        let Ok(mut control) = self.control.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop control state is unavailable".to_string(),
            ));
        };
        control.collect_events();
        let summary = session_summary(&mut control, &session_id, &self.control_token)?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        store.update_session_state(
            &summary.session_id,
            &summary.state,
            summary.exit_code,
            &timestamp(),
        )?;
        store.update_session_cwd(&summary.session_id, summary.cwd.as_deref(), &timestamp())?;
        Ok(())
    }

    fn persist_agent_set_state(
        &self,
        control: &mut DesktopRuntimeControl,
        response: &ResponseEnvelope,
    ) -> Result<(), DesktopHostError> {
        let result: AgentStateResult = response_result_json(response)?;
        {
            let Ok(mut store) = self.store.lock() else {
                return Err(DesktopHostError::StateUnavailable(
                    "desktop store state is unavailable".to_string(),
                ));
            };
            store.upsert_agent_state(&persisted_agent_state_from_result(&result, &timestamp()))?;
        }
        self.persist_notifications_from_control(control, Some(&result.workspace_id))
    }

    fn persist_agent_clear_attention(
        &self,
        request: &RequestEnvelope,
    ) -> Result<(), DesktopHostError> {
        let params: SessionIdParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        store.clear_agent_attention(&params.session_id, &timestamp())?;
        Ok(())
    }

    fn persist_notifications_from_control(
        &self,
        control: &mut DesktopRuntimeControl,
        workspace_id: Option<&str>,
    ) -> Result<(), DesktopHostError> {
        let params_json = serde_json::json!({
            "workspace_id": workspace_id,
            "severity": null,
            "include_dismissed": true,
        })
        .to_string();
        let response = control.handle_request(RequestEnvelope::new(
            "desktop_persist_notifications",
            "notification.list",
            params_json,
            self.control_token.clone(),
        ));
        let result: NotificationListResult = response_result_json(&response)?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        for notification in result.notifications {
            store.upsert_notification(&persisted_notification_from_result(&notification))?;
            self.dispatch_desktop_notification(&notification);
        }
        Ok(())
    }

    fn persist_agent_snapshots(
        &self,
        control: &DesktopRuntimeControl,
        response_id: &str,
    ) -> Option<ResponseEnvelope> {
        match self.persist_agent_snapshots_result(control) {
            Ok(()) => None,
            Err(error) => Some(ResponseEnvelope::error(
                response_id.to_string(),
                ControlError::new(
                    ErrorCode::Conflict,
                    format!("Failed to persist desktop agent metadata: {error}"),
                ),
            )),
        }
    }

    fn persist_agent_snapshots_result(
        &self,
        control: &DesktopRuntimeControl,
    ) -> Result<(), DesktopHostError> {
        let states = control.agent_state_snapshot();
        let notifications = control.notification_snapshot();
        if states.is_empty() && notifications.is_empty() {
            return Ok(());
        }

        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let now = timestamp();
        for state in states {
            store.upsert_agent_state(&persisted_agent_state_from_result(&state, &now))?;
        }
        for notification in notifications {
            store.upsert_notification(&persisted_notification_from_result(&notification))?;
            self.dispatch_desktop_notification(&notification);
        }
        Ok(())
    }

    fn dispatch_desktop_notification(&self, notification: &NotificationSummaryResult) {
        if notification.dismissed || !desktop_notification_type_enabled(notification) {
            return;
        }
        let adapter = {
            let Ok(mut state) = self.desktop_notifications.lock() else {
                return;
            };
            if state
                .delivered_notification_ids
                .contains(&notification.notification_id)
            {
                return;
            }
            let Some(adapter) = state.adapter.clone() else {
                return;
            };
            state
                .delivered_notification_ids
                .insert(notification.notification_id.clone());
            state
                .delivered_notification_order
                .push_back(notification.notification_id.clone());
            // Evict oldest entry when the cap is exceeded.
            if state.delivered_notification_order.len() > MAX_DELIVERED_NOTIFICATION_IDS {
                if let Some(oldest) = state.delivered_notification_order.pop_front() {
                    state.delivered_notification_ids.remove(&oldest);
                }
            }
            adapter
        };

        adapter.notify(desktop_notification_from_summary(notification));
    }

    fn handle_workspace_create(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: WorkspaceCreateParams = request.parse_params()?;
        if params.name.trim().is_empty() {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "workspace.create requires a non-empty name.",
            )));
        }

        let now = timestamp();
        let workspace_id = WorkspaceId::new().to_string();
        let pane_id = PaneId::new().to_string();
        let bundle = WorkspaceBundle {
            workspace: PersistedWorkspace {
                workspace_id: workspace_id.clone(),
                name: params.name,
                root_pane_id: pane_id.clone(),
                active_pane_id: pane_id.clone(),
                project_root: params.project_root,
                environment_profile_id: params.backend_profile,
                description: None,
                icon: None,
                color: None,
                default_wsl_distribution: None,
                default_terminal_profile: None,
                default_agent_command: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            },
            panes: vec![PersistedPane {
                pane_id,
                workspace_id: workspace_id.clone(),
                parent_pane_id: None,
                kind: "leaf".to_string(),
                split_axis: None,
                split_ratio: None,
                mounted_surface_id: None,
                last_focused_at: Some(now.clone()),
                created_at: now.clone(),
                updated_at: now,
            }],
            surfaces: Vec::new(),
            sessions: Vec::new(),
        };
        let summary = workspace_summary(&bundle.workspace);
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        store.save_workspace_bundle(&bundle)?;

        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &summary))
    }

    fn handle_workspace_list(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let workspaces = store
            .list_workspaces()?
            .iter()
            .map(workspace_summary)
            .collect();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &WorkspaceListResult { workspaces },
        ))
    }

    fn handle_workspace_get(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: WorkspaceIdParams = request.parse_params()?;
        let bundle = self.load_workspace_or_not_found(&params.workspace_id)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_detail(bundle),
        ))
    }

    fn handle_workspace_rename(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: WorkspaceRenameParams = request.parse_params()?;
        if params.name.trim().is_empty() {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "workspace.rename requires a non-empty name.",
            )));
        }

        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        if !store.rename_workspace(&params.workspace_id, &params.name, &timestamp())? {
            return Err(workspace_not_found(&params.workspace_id).into());
        }
        let bundle = store
            .load_workspace_bundle(&params.workspace_id)?
            .ok_or_else(|| workspace_not_found(&params.workspace_id))?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_summary(&bundle.workspace),
        ))
    }

    fn handle_workspace_update(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: WorkspaceUpdateParams = request.parse_params()?;
        let name = params.name.trim();
        if name.is_empty() {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "workspace.update requires a non-empty name.",
            )));
        }

        let mut bundle = self.load_workspace_or_not_found(&params.workspace_id)?;
        bundle.workspace.name = name.to_string();
        bundle.workspace.project_root = clean_optional_text(params.project_root);
        bundle.workspace.environment_profile_id =
            clean_optional_text(params.environment_profile_id);
        bundle.workspace.description = clean_optional_text(params.description);
        bundle.workspace.icon = clean_optional_text(params.icon);
        bundle.workspace.color = clean_optional_text(params.color);
        bundle.workspace.default_wsl_distribution =
            clean_optional_text(params.default_wsl_distribution);
        bundle.workspace.default_terminal_profile =
            clean_optional_text(params.default_terminal_profile);
        bundle.workspace.default_agent_command = clean_optional_text(params.default_agent_command);
        bundle.workspace.updated_at = timestamp();

        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        store.save_workspace_bundle(&bundle)?;

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_summary(&bundle.workspace),
        ))
    }

    fn handle_workspace_close(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: WorkspaceCloseParams = request.parse_params()?;
        if !matches!(
            params.close_policy.as_str(),
            "detach_sessions" | "terminate_sessions" | "fail_if_running"
        ) {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                format!("Unsupported close policy '{}'.", params.close_policy),
            )));
        }

        let bundle = self.load_workspace_or_not_found(&params.workspace_id)?;
        self.coordinate_workspace_close(&bundle, &params.close_policy)?;

        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let closed = store.delete_workspace(&params.workspace_id)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &WorkspaceCloseResult {
                workspace_id: params.workspace_id,
                closed,
            },
        ))
    }

    fn handle_workspace_group_list(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let _params: WorkspaceGroupListParams = request.parse_params()?;
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_group_list_result(&store)?,
        ))
    }

    fn handle_workspace_group_create(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: WorkspaceGroupCreateParams = request.parse_params()?;
        let name = normalize_workspace_group_name(&params.name)?;
        let now = timestamp();
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        validate_optional_workspace_exists(&store, params.anchor_workspace_id.as_deref())?;
        let mut workspace_ids = params.workspace_ids.unwrap_or_default();
        if workspace_ids.is_empty() {
            if let Some(anchor_workspace_id) = params.anchor_workspace_id.clone() {
                workspace_ids.push(anchor_workspace_id);
            }
        }
        for workspace_id in &workspace_ids {
            validate_workspace_exists(&store, workspace_id)?;
        }
        let group_id = workspace_group_id();
        let sort_order = store.list_workspace_groups()?.len() as i64;
        let group = PersistedWorkspaceGroup {
            group_id: group_id.clone(),
            name,
            anchor_workspace_id: params.anchor_workspace_id,
            collapsed: params.collapsed.unwrap_or(false),
            pinned: params.pinned.unwrap_or(false),
            color: normalize_optional_workspace_group_text(params.color),
            icon: normalize_optional_workspace_group_text(params.icon),
            sort_order,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.upsert_workspace_group(&group)?;
        for (index, workspace_id) in workspace_ids.into_iter().enumerate() {
            store.upsert_workspace_group_member(&PersistedWorkspaceGroupMember {
                group_id: group_id.clone(),
                workspace_id,
                position: index as i64,
                created_at: now.clone(),
                updated_at: now.clone(),
            })?;
        }
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_group_result_for_id(&store, &group_id)?,
        ))
    }

    fn handle_workspace_group_update(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: WorkspaceGroupUpdateParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let mut group = store
            .load_workspace_group(&params.group_id)?
            .ok_or_else(|| workspace_group_not_found(&params.group_id))?;
        if let Some(name) = params.name {
            group.name = normalize_workspace_group_name(&name)?;
        }
        if let Some(anchor_workspace_id) = params.anchor_workspace_id {
            validate_workspace_exists(&store, &anchor_workspace_id)?;
            group.anchor_workspace_id = Some(anchor_workspace_id);
        }
        if let Some(collapsed) = params.collapsed {
            group.collapsed = collapsed;
        }
        if let Some(pinned) = params.pinned {
            group.pinned = pinned;
        }
        if params.color.is_some() {
            group.color = normalize_optional_workspace_group_text(params.color);
        }
        if params.icon.is_some() {
            group.icon = normalize_optional_workspace_group_text(params.icon);
        }
        if let Some(sort_order) = params.sort_order {
            group.sort_order = sort_order;
        }
        group.updated_at = timestamp();
        store.upsert_workspace_group(&group)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_group_result_for_id(&store, &params.group_id)?,
        ))
    }

    fn handle_workspace_group_delete(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: WorkspaceGroupIdParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        if !store.delete_workspace_group(&params.group_id)? {
            return Err(workspace_group_not_found(&params.group_id));
        }
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_workspace_group_add_workspace(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: WorkspaceGroupMemberParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        if store.load_workspace_group(&params.group_id)?.is_none() {
            return Err(workspace_group_not_found(&params.group_id));
        }
        validate_workspace_exists(&store, &params.workspace_id)?;
        let position = params.position.unwrap_or_else(|| {
            store
                .list_workspace_group_members(Some(&params.group_id))
                .map(|members| members.len() as i64)
                .unwrap_or(0)
        });
        let now = timestamp();
        store.upsert_workspace_group_member(&PersistedWorkspaceGroupMember {
            group_id: params.group_id.clone(),
            workspace_id: params.workspace_id,
            position,
            created_at: now.clone(),
            updated_at: now,
        })?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_group_result_for_id(&store, &params.group_id)?,
        ))
    }

    fn handle_workspace_group_remove_workspace(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: WorkspaceGroupMemberParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        if store.load_workspace_group(&params.group_id)?.is_none() {
            return Err(workspace_group_not_found(&params.group_id));
        }
        store.remove_workspace_group_member(&params.group_id, &params.workspace_id)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_group_result_for_id(&store, &params.group_id)?,
        ))
    }

    fn handle_pane_split(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: PaneSplitParams = request.parse_params()?;
        if !matches!(params.axis.as_str(), "horizontal" | "vertical") {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "pane.split axis must be 'horizontal' or 'vertical'.",
            )));
        }

        let ratio = params.ratio.unwrap_or(0.5);
        if !(0.1..=0.9).contains(&ratio) {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "pane.split ratio must be between 0.1 and 0.9.",
            )));
        }

        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let mut bundle = store
            .load_workspace_bundle(&params.workspace_id)?
            .ok_or_else(|| workspace_not_found(&params.workspace_id))?;
        split_pane_in_bundle(&mut bundle, &params.pane_id, &params.axis, ratio)?;
        store.save_workspace_bundle(&bundle)?;

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_detail(bundle),
        ))
    }

    fn handle_pane_focus(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: PaneFocusParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let mut bundle = store
            .load_workspace_bundle(&params.workspace_id)?
            .ok_or_else(|| workspace_not_found(&params.workspace_id))?;
        focus_pane_in_bundle(&mut bundle, &params.pane_id)?;
        store.save_workspace_bundle(&bundle)?;

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_detail(bundle),
        ))
    }

    fn handle_pane_close(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: PaneCloseParams = request.parse_params()?;
        if !matches!(
            params.surface_policy.as_str(),
            "detach_surface" | "close_surface" | "fail_if_session_running"
        ) {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                format!("Unsupported surface policy '{}'.", params.surface_policy),
            )));
        }

        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let mut bundle = store
            .load_workspace_bundle(&params.workspace_id)?
            .ok_or_else(|| workspace_not_found(&params.workspace_id))?;
        self.coordinate_pane_close(&bundle, &params.pane_id, &params.surface_policy)?;
        close_pane_in_bundle(&mut bundle, &params.pane_id, &params.surface_policy)?;
        store.save_workspace_bundle(&bundle)?;

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_detail(bundle),
        ))
    }

    fn handle_pane_resize_layout(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: PaneResizeLayoutParams = request.parse_params()?;
        if !(0.1..=0.9).contains(&params.ratio) {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "pane.resize_layout ratio must be between 0.1 and 0.9.",
            )));
        }

        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let mut bundle = store
            .load_workspace_bundle(&params.workspace_id)?
            .ok_or_else(|| workspace_not_found(&params.workspace_id))?;
        resize_pane_layout_in_bundle(&mut bundle, &params.pane_id, params.ratio)?;
        store.save_workspace_bundle(&bundle)?;

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_detail(bundle),
        ))
    }

    fn handle_pane_mount_surface(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: PaneMountSurfaceParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let mut bundle = store
            .load_workspace_bundle(&params.workspace_id)?
            .ok_or_else(|| workspace_not_found(&params.workspace_id))?;
        mount_surface_in_bundle(&mut bundle, &params.pane_id, &params.surface_id)?;
        store.save_workspace_bundle(&bundle)?;

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_detail(bundle),
        ))
    }

    fn handle_pane_unmount_surface(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: PaneUnmountSurfaceParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let mut bundle = store
            .load_workspace_bundle(&params.workspace_id)?
            .ok_or_else(|| workspace_not_found(&params.workspace_id))?;
        unmount_surface_in_bundle(&mut bundle, &params.pane_id)?;
        store.save_workspace_bundle(&bundle)?;

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_detail(bundle),
        ))
    }

    fn handle_surface_create_browser(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: SurfaceCreateBrowserParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let mut bundle = store
            .load_workspace_bundle(&params.workspace_id)?
            .ok_or_else(|| workspace_not_found(&params.workspace_id))?;
        let now = timestamp();
        let pane_id = match params.placement.as_deref() {
            Some("new_tab") => {
                let pane_id = PaneId::new().to_string();
                bundle.panes.push(PersistedPane {
                    pane_id: pane_id.clone(),
                    workspace_id: params.workspace_id.clone(),
                    parent_pane_id: None,
                    kind: "leaf".to_string(),
                    split_axis: None,
                    split_ratio: None,
                    mounted_surface_id: None,
                    last_focused_at: Some(now.clone()),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                });
                bundle.workspace.root_pane_id = pane_id.clone();
                bundle.workspace.active_pane_id = pane_id.clone();
                pane_id
            }
            Some("active_pane") | None => params
                .pane_id
                .clone()
                .unwrap_or_else(|| bundle.workspace.active_pane_id.clone()),
            Some(value) => {
                return Err(DesktopHostError::Control(ControlError::new(
                    ErrorCode::InvalidRequest,
                    format!("Unsupported browser surface placement '{value}'."),
                )));
            }
        };
        validate_browser_mount_target(&bundle, &pane_id)?;

        let surface_id = SurfaceId::new().to_string();
        let browser_surface = {
            let Ok(mut browser) = self.browser.lock() else {
                return Err(DesktopHostError::StateUnavailable(
                    "browser automation state is unavailable".to_string(),
                ));
            };
            browser
                .create_surface(
                    surface_id.clone(),
                    params.workspace_id.clone(),
                    params.profile.clone(),
                )
                .map_err(browser_error_from_automation)?
        };
        let surface = persisted_browser_surface(&browser_surface, &now);
        bundle.surfaces.push(surface.clone());
        mount_surface_in_bundle(&mut bundle, &pane_id, &surface_id)?;
        store.save_workspace_bundle(&bundle)?;

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &surface_summary(&surface),
        ))
    }

    fn handle_surface_close(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: SurfaceCloseParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let mut bundle = store
            .load_workspace_bundle(&params.workspace_id)?
            .ok_or_else(|| workspace_not_found(&params.workspace_id))?;
        let surface_ids = surface_ids_for_close(&bundle, &params.surface_id)?;
        let surfaces_to_close = bundle
            .surfaces
            .iter()
            .filter(|surface| surface_ids.contains(&surface.surface_id))
            .cloned()
            .collect::<Vec<_>>();

        for surface in &surfaces_to_close {
            if let Some(session_id) = surface.session_id.as_deref() {
                self.close_live_surface_session(&bundle, session_id)?;
            }
            if surface.surface_type == "browser" {
                self.close_browser_surface_if_present(&surface.surface_id)?;
            }
        }

        close_surface_in_bundle(&mut bundle, &params.surface_id)?;
        store.save_workspace_bundle(&bundle)?;

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &workspace_detail(bundle),
        ))
    }

    fn handle_surface_move_workspace(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: SurfaceMoveWorkspaceParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let mut source = store
            .load_workspace_bundle(&params.source_workspace_id)?
            .ok_or_else(|| workspace_not_found(&params.source_workspace_id))?;
        if params.source_workspace_id == params.target_workspace_id {
            if !source
                .surfaces
                .iter()
                .any(|surface| surface.surface_id == params.surface_id)
            {
                return Err(surface_not_found(&params.surface_id).into());
            }
            let detail = workspace_detail(source);
            return Ok(ResponseEnvelope::ok_typed(
                request.id.clone(),
                &SurfaceMoveWorkspaceResult {
                    source: detail.clone(),
                    target: detail,
                },
            ));
        }

        let mut target = store
            .load_workspace_bundle(&params.target_workspace_id)?
            .ok_or_else(|| workspace_not_found(&params.target_workspace_id))?;
        let moved_session_ids =
            move_surface_tab_between_workspaces(&mut source, &mut target, &params.surface_id)?;
        let mut moved_agent_states = Vec::new();
        for session_id in &moved_session_ids {
            if let Some(mut state) = store.load_agent_state(session_id)? {
                state.workspace_id = params.target_workspace_id.clone();
                state.updated_at = timestamp();
                moved_agent_states.push(state);
            }
        }

        store.save_workspace_bundle(&target)?;
        store.save_workspace_bundle(&source)?;
        for state in &moved_agent_states {
            store.upsert_agent_state(state)?;
        }

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &SurfaceMoveWorkspaceResult {
                source: workspace_detail(source),
                target: workspace_detail(target),
            },
        ))
    }

    fn handle_browser_navigate(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserNavigateParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.navigate",
            BrowserCommand::Navigate {
                surface_id: params.surface_id,
                url: params.url,
            },
        )?;
        let BrowserCommandResult::Navigated { surface_id, url } = result else {
            unreachable!("navigate command returns navigated result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserNavigationResult { surface_id, url },
        ))
    }

    fn handle_browser_reload(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserSurfaceParams = request.parse_params()?;
        self.handle_browser_navigation_command(
            request,
            "browser.reload",
            BrowserCommand::Reload {
                surface_id: params.surface_id,
            },
        )
    }

    fn handle_browser_back(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserSurfaceParams = request.parse_params()?;
        self.handle_browser_navigation_command(
            request,
            "browser.back",
            BrowserCommand::GoBack {
                surface_id: params.surface_id,
            },
        )
    }

    fn handle_browser_forward(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserSurfaceParams = request.parse_params()?;
        self.handle_browser_navigation_command(
            request,
            "browser.forward",
            BrowserCommand::GoForward {
                surface_id: params.surface_id,
            },
        )
    }

    fn handle_browser_current_url(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserSurfaceParams = request.parse_params()?;
        self.handle_browser_navigation_command(
            request,
            "browser.current_url",
            BrowserCommand::CurrentUrl {
                surface_id: params.surface_id,
            },
        )
    }

    fn handle_browser_navigation_command(
        &self,
        request: &RequestEnvelope,
        operation: &'static str,
        command: BrowserCommand,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let result = self.execute_browser_command(operation, command)?;
        let BrowserCommandResult::Navigated { surface_id, url } = result else {
            unreachable!("browser navigation command returns navigated result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserNavigationResult { surface_id, url },
        ))
    }

    fn handle_browser_screenshot(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserScreenshotParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.screenshot",
            BrowserCommand::Screenshot {
                surface_id: params.surface_id,
                format: params.format.unwrap_or_else(|| "png".to_string()),
            },
        )?;
        let BrowserCommandResult::Screenshot {
            surface_id,
            format,
            bytes,
        } = result
        else {
            unreachable!("screenshot command returns screenshot result")
        };
        let byte_count = bytes.len();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserScreenshotResult {
                image_handle: format!("memory://browser/{surface_id}/{format}/{byte_count}"),
                surface_id,
                format,
                byte_count,
            },
        ))
    }

    fn handle_browser_dom_snapshot(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserDomSnapshotParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.dom_snapshot",
            BrowserCommand::DomSnapshot {
                surface_id: params.surface_id,
                frame_id: params.frame_id,
            },
        )?;
        let BrowserCommandResult::DomSnapshot { surface_id, html } = result else {
            unreachable!("dom snapshot command returns dom snapshot result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserDomSnapshotResult { surface_id, html },
        ))
    }

    fn handle_browser_frames(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserSurfaceParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.frames",
            BrowserCommand::Frames {
                surface_id: params.surface_id,
            },
        )?;
        let BrowserCommandResult::Frames { surface_id, frames } = result else {
            unreachable!("frames command returns frames result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserFramesResult {
                surface_id,
                frames: frames.into_iter().map(browser_frame_result).collect(),
            },
        ))
    }

    fn handle_browser_storage(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserSurfaceParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.storage",
            BrowserCommand::StorageSnapshot {
                surface_id: params.surface_id,
            },
        )?;
        let BrowserCommandResult::StorageSnapshot {
            surface_id,
            local_storage,
            session_storage,
        } = result
        else {
            unreachable!("storage command returns storage snapshot result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserStorageResult {
                surface_id,
                local_storage: local_storage
                    .into_iter()
                    .map(browser_storage_entry_result)
                    .collect(),
                session_storage: session_storage
                    .into_iter()
                    .map(browser_storage_entry_result)
                    .collect(),
            },
        ))
    }

    fn handle_browser_cookies(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserSurfaceParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.cookies",
            BrowserCommand::Cookies {
                surface_id: params.surface_id,
            },
        )?;
        let BrowserCommandResult::Cookies {
            surface_id,
            cookies,
        } = result
        else {
            unreachable!("cookies command returns cookies result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserCookiesResult {
                surface_id,
                cookies: cookies.into_iter().map(browser_cookie_result).collect(),
            },
        ))
    }

    fn handle_browser_downloads(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserDownloadsParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.downloads",
            BrowserCommand::Downloads {
                surface_id: params.surface_id,
                limit: params.limit.unwrap_or(100),
            },
        )?;
        let BrowserCommandResult::Downloads {
            surface_id,
            directory,
            downloads,
        } = result
        else {
            unreachable!("downloads command returns downloads result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserDownloadsResult {
                surface_id,
                directory,
                downloads: downloads.into_iter().map(browser_download_result).collect(),
            },
        ))
    }

    fn handle_browser_history(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserSurfaceParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.history",
            BrowserCommand::History {
                surface_id: params.surface_id,
            },
        )?;
        let BrowserCommandResult::History {
            surface_id,
            current_index,
            entries,
        } = result
        else {
            unreachable!("history command returns history result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserHistoryResult {
                surface_id,
                current_index,
                entries: entries
                    .into_iter()
                    .map(browser_history_entry_result)
                    .collect(),
            },
        ))
    }

    fn handle_browser_console(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserConsoleParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.console",
            BrowserCommand::ConsoleMessages {
                surface_id: params.surface_id,
                limit: params.limit.unwrap_or(100),
            },
        )?;
        let BrowserCommandResult::ConsoleMessages {
            surface_id,
            messages,
        } = result
        else {
            unreachable!("console command returns console messages result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserConsoleResult {
                surface_id,
                messages: messages
                    .into_iter()
                    .map(browser_console_message_result)
                    .collect(),
            },
        ))
    }

    fn handle_browser_dialogs(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserDialogsParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.dialogs",
            BrowserCommand::DialogMessages {
                surface_id: params.surface_id,
                limit: params.limit.unwrap_or(100),
            },
        )?;
        let BrowserCommandResult::DialogMessages {
            surface_id,
            messages,
        } = result
        else {
            unreachable!("dialogs command returns dialog messages result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserDialogsResult {
                surface_id,
                messages: messages
                    .into_iter()
                    .map(browser_dialog_message_result)
                    .collect(),
            },
        ))
    }

    fn handle_browser_errors(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserErrorsParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.errors",
            BrowserCommand::ErrorEvents {
                surface_id: params.surface_id,
                limit: params.limit.unwrap_or(100),
            },
        )?;
        let BrowserCommandResult::ErrorEvents { surface_id, events } = result else {
            unreachable!("errors command returns error events result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserErrorsResult {
                surface_id,
                events: events.into_iter().map(browser_error_event_result).collect(),
            },
        ))
    }

    fn handle_browser_click(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserClickParams = request.parse_params()?;
        let command = if let Some(selector) = params.selector {
            BrowserCommand::ClickSelector {
                surface_id: params.surface_id,
                selector,
                frame_id: params.frame_id,
            }
        } else {
            let x = browser_coordinate_to_i32(params.x, "x")?;
            let y = browser_coordinate_to_i32(params.y, "y")?;
            BrowserCommand::ClickPoint {
                surface_id: params.surface_id,
                x,
                y,
            }
        };
        let result = self.execute_browser_command("browser.click", command)?;
        let BrowserCommandResult::Clicked { surface_id, .. } = result else {
            unreachable!("click command returns clicked result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserActionResult {
                surface_id,
                ok: true,
            },
        ))
    }

    fn handle_browser_type(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserTypeParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.type",
            BrowserCommand::TypeText {
                surface_id: params.surface_id,
                selector: params.selector,
                text: params.text,
                frame_id: params.frame_id,
            },
        )?;
        let BrowserCommandResult::Typed { surface_id, .. } = result else {
            unreachable!("type command returns typed result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserActionResult {
                surface_id,
                ok: true,
            },
        ))
    }

    fn handle_browser_fill(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserFillParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.fill",
            BrowserCommand::FillText {
                surface_id: params.surface_id,
                selector: params.selector,
                text: params.text,
                frame_id: params.frame_id,
            },
        )?;
        let BrowserCommandResult::Filled { surface_id, .. } = result else {
            unreachable!("fill command returns filled result")
        };
        Ok(browser_action_ok_response(request, surface_id))
    }

    fn handle_browser_press(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserPressParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.press",
            BrowserCommand::PressKey {
                surface_id: params.surface_id,
                selector: params.selector,
                key: params.key,
                frame_id: params.frame_id,
            },
        )?;
        let BrowserCommandResult::Pressed { surface_id, .. } = result else {
            unreachable!("press command returns pressed result")
        };
        Ok(browser_action_ok_response(request, surface_id))
    }

    fn handle_browser_select(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserSelectParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.select",
            BrowserCommand::SelectValues {
                surface_id: params.surface_id,
                selector: params.selector,
                values: params.values,
                frame_id: params.frame_id,
            },
        )?;
        let BrowserCommandResult::Selected { surface_id, .. } = result else {
            unreachable!("select command returns selected result")
        };
        Ok(browser_action_ok_response(request, surface_id))
    }

    fn handle_browser_scroll(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserScrollParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.scroll",
            BrowserCommand::ScrollBy {
                surface_id: params.surface_id,
                selector: params.selector,
                x: params.x.unwrap_or(0),
                y: params.y.unwrap_or(0),
                frame_id: params.frame_id,
            },
        )?;
        let BrowserCommandResult::Scrolled { surface_id, .. } = result else {
            unreachable!("scroll command returns scrolled result")
        };
        Ok(browser_action_ok_response(request, surface_id))
    }

    fn handle_browser_hover(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserHoverParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.hover",
            BrowserCommand::HoverSelector {
                surface_id: params.surface_id,
                selector: params.selector,
                frame_id: params.frame_id,
            },
        )?;
        let BrowserCommandResult::Hovered { surface_id, .. } = result else {
            unreachable!("hover command returns hovered result")
        };
        Ok(browser_action_ok_response(request, surface_id))
    }

    fn handle_browser_check(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserCheckParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.check",
            BrowserCommand::CheckSelector {
                surface_id: params.surface_id,
                selector: params.selector,
                checked: params.checked.unwrap_or(true),
                frame_id: params.frame_id,
            },
        )?;
        let BrowserCommandResult::Checked { surface_id, .. } = result else {
            unreachable!("check command returns checked result")
        };
        Ok(browser_action_ok_response(request, surface_id))
    }

    fn handle_browser_get(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserGetParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.get",
            BrowserCommand::GetElement {
                surface_id: params.surface_id,
                selector: params.selector,
                kind: params.kind.unwrap_or_else(|| "text".to_string()),
                attribute: params.attribute,
                frame_id: params.frame_id,
            },
        )?;
        let BrowserCommandResult::Got {
            surface_id,
            selector,
            kind,
            value,
        } = result
        else {
            unreachable!("get command returns got result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserGetResult {
                surface_id,
                selector,
                kind,
                value,
            },
        ))
    }

    fn handle_browser_find(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserFindParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.find",
            BrowserCommand::FindText {
                surface_id: params.surface_id,
                query: params.query,
                selector: params.selector,
                limit: params.limit.unwrap_or(10),
                frame_id: params.frame_id,
            },
        )?;
        let BrowserCommandResult::Found {
            surface_id,
            query,
            count,
            matches,
        } = result
        else {
            unreachable!("find command returns found result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserFindResult {
                surface_id,
                query,
                count,
                matches,
            },
        ))
    }

    fn handle_browser_highlight(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserHighlightParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.highlight",
            BrowserCommand::HighlightSelector {
                surface_id: params.surface_id,
                selector: params.selector,
                duration_ms: params.duration_ms.unwrap_or(1200),
                frame_id: params.frame_id,
            },
        )?;
        let BrowserCommandResult::Highlighted { surface_id, .. } = result else {
            unreachable!("highlight command returns highlighted result")
        };
        Ok(browser_action_ok_response(request, surface_id))
    }

    fn handle_browser_focus(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserFocusParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.focus",
            BrowserCommand::FocusSelector {
                surface_id: params.surface_id,
                selector: params.selector,
                frame_id: params.frame_id,
            },
        )?;
        let BrowserCommandResult::Focused { surface_id, .. } = result else {
            unreachable!("focus command returns focused result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserActionResult {
                surface_id,
                ok: true,
            },
        ))
    }

    fn handle_browser_zoom(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserZoomParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.zoom",
            BrowserCommand::SetZoom {
                surface_id: params.surface_id,
                percent: params.percent,
            },
        )?;
        let BrowserCommandResult::Zoomed { surface_id, .. } = result else {
            unreachable!("zoom command returns zoomed result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserActionResult {
                surface_id,
                ok: true,
            },
        ))
    }

    fn handle_browser_wait_for_selector(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserWaitForSelectorParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.wait_for_selector",
            BrowserCommand::WaitForSelector {
                surface_id: params.surface_id,
                selector: params.selector,
                timeout_ms: params.timeout_ms.unwrap_or(5000),
                frame_id: params.frame_id,
            },
        )?;
        let BrowserCommandResult::WaitedForSelector {
            surface_id,
            selector,
            elapsed_ms,
        } = result
        else {
            unreachable!("wait-for-selector command returns waited result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserWaitForSelectorResult {
                surface_id,
                selector,
                elapsed_ms,
            },
        ))
    }

    fn handle_browser_evaluate(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserEvaluateParams = request.parse_params()?;
        let result = self.execute_browser_command(
            "browser.evaluate",
            BrowserCommand::Evaluate {
                surface_id: params.surface_id,
                script: params.script,
                frame_id: params.frame_id,
            },
        )?;
        let BrowserCommandResult::Evaluated {
            surface_id,
            value_json,
        } = result
        else {
            unreachable!("evaluate command returns evaluated result")
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserEvaluateResult {
                surface_id,
                value_json,
            },
        ))
    }

    fn handle_agent_get_state(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: SessionIdParams = request.parse_params()?;
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };

        if let Some(state) = store.load_agent_state(&params.session_id)? {
            return Ok(ResponseEnvelope::ok_typed(
                request.id.clone(),
                &agent_state_result_from_persisted(&state),
            ));
        }

        let Some(session) = store.load_session(&params.session_id)? else {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::SessionNotFound,
                "Session not found.",
            )));
        };

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AgentStateResult {
                session_id: session.session_id,
                workspace_id: session.workspace_id,
                state: "unknown".to_string(),
                attention: false,
                reason: None,
                updated_at: None,
                telemetry: None,
            },
        ))
    }

    fn handle_agent_list_attention(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: AgentListAttentionParams = request.parse_params()?;
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let sessions = store
            .list_agent_attention(params.workspace_id.as_deref())?
            .iter()
            .map(agent_state_result_from_persisted)
            .collect();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AgentAttentionListResult { sessions },
        ))
    }

    fn handle_agent_list(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: AgentListAttentionParams = request.parse_params()?;
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let sessions = store
            .list_agent_states(params.workspace_id.as_deref())?
            .iter()
            .map(agent_state_result_from_persisted)
            .collect();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AgentAttentionListResult { sessions },
        ))
    }

    fn handle_actions_list(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: ActionListParams = request.parse_params()?;
        let config = self.load_effective_app_config(params.workspace_id.as_deref())?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &ActionListResult {
                workspace_id: params.workspace_id,
                actions: action_list_from_config(&config),
            },
        ))
    }

    fn handle_actions_run_request(&self, request: RequestEnvelope) -> ResponseEnvelope {
        let id = request.id.clone();
        let result = (|| {
            validate_desktop_request(&request, &self.control_token)?;
            let params: ActionRunParams = request.parse_params()?;
            self.run_action(params)
        })();
        match result {
            Ok(result) => ResponseEnvelope::ok_typed(id, &result),
            Err(error) => ResponseEnvelope::error(id, control_error_from_host(error)),
        }
    }

    fn run_action(&self, params: ActionRunParams) -> Result<ActionRunResult, DesktopHostError> {
        let action_id = params.action_id.trim().to_string();
        if action_id.is_empty() {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "actions.run requires a non-empty action_id.",
            )));
        }

        let config = self.load_effective_app_config(params.workspace_id.as_deref())?;
        if let Some(custom_action) = config
            .actions
            .custom
            .iter()
            .find(|action| action.id == action_id)
            .cloned()
        {
            return self.run_custom_action(&custom_action, params);
        }

        self.run_builtin_action(&action_id, params)
    }

    fn run_builtin_action(
        &self,
        action_id: &str,
        params: ActionRunParams,
    ) -> Result<ActionRunResult, DesktopHostError> {
        match action_id {
            "workspace.new" => {
                let workspace: WorkspaceSummaryResult = self.call_internal_control(
                    "workspace.create",
                    &WorkspaceCreateParams {
                        name: "Workspace".to_string(),
                        project_root: None,
                        backend_profile: None,
                    },
                )?;
                Ok(action_workspace_result(action_id, workspace.workspace_id))
            }
            "terminal.newWsl" => {
                let workspace = self.action_workspace(params.workspace_id.as_deref())?;
                self.run_wsl_terminal_action(action_id, &workspace, None, "new_tab")
            }
            "terminal.openInActivePane" => {
                let workspace = self.action_workspace(params.workspace_id.as_deref())?;
                let pane_id = params
                    .pane_id
                    .clone()
                    .unwrap_or_else(|| workspace.active_pane_id.clone());
                self.run_wsl_terminal_action(action_id, &workspace, Some(pane_id), "active_pane")
            }
            "pane.splitRight" => {
                let workspace = self.action_workspace(params.workspace_id.as_deref())?;
                let pane_id = params
                    .pane_id
                    .clone()
                    .unwrap_or_else(|| workspace.active_pane_id.clone());
                self.run_pane_split_action(action_id, workspace.workspace_id, pane_id, "vertical")
            }
            "pane.splitDown" => {
                let workspace = self.action_workspace(params.workspace_id.as_deref())?;
                let pane_id = params
                    .pane_id
                    .clone()
                    .unwrap_or_else(|| workspace.active_pane_id.clone());
                self.run_pane_split_action(action_id, workspace.workspace_id, pane_id, "horizontal")
            }
            "browser.openNewTab" => {
                let workspace = self.action_workspace(params.workspace_id.as_deref())?;
                self.run_browser_open_action(action_id, &workspace, None, "new_tab", None)
            }
            "browser.openActivePane" => {
                let workspace = self.action_workspace(params.workspace_id.as_deref())?;
                let pane_id = params
                    .pane_id
                    .clone()
                    .unwrap_or_else(|| workspace.active_pane_id.clone());
                self.run_browser_open_action(
                    action_id,
                    &workspace,
                    Some(pane_id),
                    "active_pane",
                    None,
                )
            }
            "agent.launchClaude" => {
                let workspace = self.action_workspace(params.workspace_id.as_deref())?;
                self.run_agent_action(action_id, &workspace, vec!["claude".to_string()])
            }
            "agent.launchCodex" => {
                let workspace = self.action_workspace(params.workspace_id.as_deref())?;
                self.run_agent_action(
                    action_id,
                    &workspace,
                    vec!["codex".to_string(), "--no-alt-screen".to_string()],
                )
            }
            "app.commandPalette"
            | "app.commandPalette.legacy"
            | "app.search"
            | "app.settings"
            | "view.toggleTheme"
            | "terminal.textBox"
            | "agent.launchCustom" => Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                format!("Action '{action_id}' is UI-only and cannot be run through actions.run."),
            ))),
            _ => Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                format!("Unknown action '{action_id}'."),
            ))),
        }
    }

    fn run_custom_action(
        &self,
        action: &AppConfigCustomAction,
        params: ActionRunParams,
    ) -> Result<ActionRunResult, DesktopHostError> {
        let workspace = self.action_workspace(params.workspace_id.as_deref())?;
        match action.target.as_str() {
            "agent" => self.run_agent_action(&action.id, &workspace, action.command.clone()),
            "wsl-terminal" => self.run_wsl_terminal_action(&action.id, &workspace, None, "new_tab"),
            "browser" => self.run_browser_custom_action(
                &action.id,
                &workspace,
                params.pane_id,
                browser_custom_action_runtime(&action.command)?,
            ),
            _ => Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                format!("Unsupported custom action target '{}'.", action.target),
            ))),
        }
    }

    fn run_wsl_terminal_action(
        &self,
        action_id: &str,
        workspace: &WorkspaceSummaryResult,
        pane_id: Option<String>,
        placement: &str,
    ) -> Result<ActionRunResult, DesktopHostError> {
        let result: SessionSpawnResult = self.call_internal_control(
            "session.spawn",
            &SessionSpawnParams {
                workspace_id: workspace.workspace_id.clone(),
                backend: Some("wsl-direct".to_string()),
                backend_profile: workspace.default_wsl_distribution.clone(),
                command: vec!["bash".to_string(), "-l".to_string()],
                cwd: workspace.project_root.clone(),
                env: Vec::new(),
                columns: 120,
                rows: 30,
                durability: Some("ephemeral".to_string()),
                placement: Some(placement.to_string()),
                pane_id,
            },
        )?;
        Ok(action_session_result(
            action_id,
            workspace.workspace_id.clone(),
            result.session_id,
            "terminal session spawned",
        ))
    }

    fn run_agent_action(
        &self,
        action_id: &str,
        workspace: &WorkspaceSummaryResult,
        command: Vec<String>,
    ) -> Result<ActionRunResult, DesktopHostError> {
        if command.is_empty() {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                format!("Action '{action_id}' does not define an agent command."),
            )));
        }
        let result: SessionSpawnResult = self.call_internal_control(
            "session.spawn",
            &SessionSpawnParams {
                workspace_id: workspace.workspace_id.clone(),
                backend: Some("wsl-tmux-control".to_string()),
                backend_profile: workspace.default_wsl_distribution.clone(),
                command,
                cwd: self.agent_action_cwd(workspace)?,
                env: Vec::new(),
                columns: 120,
                rows: 30,
                durability: Some("durable".to_string()),
                placement: Some("new_tab".to_string()),
                pane_id: None,
            },
        )?;
        Ok(action_session_result(
            action_id,
            workspace.workspace_id.clone(),
            result.session_id,
            "agent session spawned",
        ))
    }

    fn agent_action_cwd(
        &self,
        workspace: &WorkspaceSummaryResult,
    ) -> Result<Option<String>, DesktopHostError> {
        if let Some(project_root) = clean_optional_text(workspace.project_root.clone()) {
            return Ok(Some(project_root));
        }

        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let Some(bundle) = store.load_workspace_bundle(&workspace.workspace_id)? else {
            return Ok(None);
        };
        Ok(active_session_id(&bundle)
            .as_deref()
            .and_then(|session_id| {
                bundle
                    .sessions
                    .iter()
                    .find(|session| session.session_id == session_id)
            })
            .and_then(|session| clean_optional_text(session.cwd.clone()))
            .filter(|cwd| !is_host_process_working_dir(cwd))
            .or_else(|| clean_optional_text(bundle.workspace.project_root)))
    }

    fn run_pane_split_action(
        &self,
        action_id: &str,
        workspace_id: String,
        pane_id: String,
        axis: &str,
    ) -> Result<ActionRunResult, DesktopHostError> {
        let detail: WorkspaceDetailResult = self.call_internal_control(
            "pane.split",
            &PaneSplitParams {
                workspace_id: workspace_id.clone(),
                pane_id: pane_id.clone(),
                axis: axis.to_string(),
                ratio: None,
            },
        )?;
        Ok(ActionRunResult {
            action_id: action_id.to_string(),
            workspace_id: Some(workspace_id),
            result_type: "pane".to_string(),
            session_id: None,
            surface_id: None,
            pane_id: Some(detail.workspace.active_pane_id),
            message: Some("pane split created".to_string()),
        })
    }

    fn run_browser_open_action(
        &self,
        action_id: &str,
        workspace: &WorkspaceSummaryResult,
        pane_id: Option<String>,
        placement: &str,
        url: Option<String>,
    ) -> Result<ActionRunResult, DesktopHostError> {
        let surface: SurfaceSummaryResult = self.call_internal_control(
            "surface.create_browser",
            &SurfaceCreateBrowserParams {
                workspace_id: workspace.workspace_id.clone(),
                pane_id,
                profile: Some("default".to_string()),
                placement: Some(placement.to_string()),
            },
        )?;
        if let Some(url) = url {
            let _: BrowserNavigationResult = self.call_internal_control(
                "browser.navigate",
                &BrowserNavigateParams {
                    surface_id: surface.surface_id.clone(),
                    url,
                },
            )?;
        }
        Ok(ActionRunResult {
            action_id: action_id.to_string(),
            workspace_id: Some(workspace.workspace_id.clone()),
            result_type: "surface".to_string(),
            session_id: None,
            surface_id: Some(surface.surface_id),
            pane_id: None,
            message: Some("browser surface opened".to_string()),
        })
    }

    fn run_browser_custom_action(
        &self,
        action_id: &str,
        workspace: &WorkspaceSummaryResult,
        requested_pane_id: Option<String>,
        runtime: BrowserCustomActionRuntime,
    ) -> Result<ActionRunResult, DesktopHostError> {
        match runtime {
            BrowserCustomActionRuntime::Open { url, placement } => {
                let pane_id = browser_action_pane_id(workspace, requested_pane_id, &placement);
                self.run_browser_open_action(action_id, workspace, pane_id, &placement, url)
            }
            BrowserCustomActionRuntime::Screenshot { format, placement } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserScreenshotResult = self.call_internal_control(
                    "browser.screenshot",
                    &BrowserScreenshotParams {
                        surface_id: surface.surface_id.clone(),
                        format: Some(format),
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser screenshot captured",
                ))
            }
            BrowserCustomActionRuntime::DomSnapshot {
                placement,
                frame_id,
            } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserDomSnapshotResult = self.call_internal_control(
                    "browser.dom_snapshot",
                    &BrowserDomSnapshotParams {
                        surface_id: surface.surface_id.clone(),
                        frame_id,
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser DOM snapshot captured",
                ))
            }
            BrowserCustomActionRuntime::Evaluate {
                script,
                placement,
                frame_id,
            } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserEvaluateResult = self.call_internal_control(
                    "browser.evaluate",
                    &BrowserEvaluateParams {
                        surface_id: surface.surface_id.clone(),
                        script,
                        frame_id,
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser script evaluated",
                ))
            }
            BrowserCustomActionRuntime::Click {
                selector,
                placement,
                frame_id,
            } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserActionResult = self.call_internal_control(
                    "browser.click",
                    &BrowserClickParams {
                        surface_id: surface.surface_id.clone(),
                        selector: Some(selector),
                        x: None,
                        y: None,
                        frame_id,
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser click executed",
                ))
            }
            BrowserCustomActionRuntime::Type {
                selector,
                text,
                placement,
                frame_id,
            } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserActionResult = self.call_internal_control(
                    "browser.type",
                    &BrowserTypeParams {
                        surface_id: surface.surface_id.clone(),
                        selector,
                        text,
                        frame_id,
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser text typed",
                ))
            }
            BrowserCustomActionRuntime::Fill {
                selector,
                text,
                placement,
                frame_id,
            } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserActionResult = self.call_internal_control(
                    "browser.fill",
                    &BrowserFillParams {
                        surface_id: surface.surface_id.clone(),
                        selector,
                        text,
                        frame_id,
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser text filled",
                ))
            }
            BrowserCustomActionRuntime::Press {
                selector,
                key,
                placement,
                frame_id,
            } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserActionResult = self.call_internal_control(
                    "browser.press",
                    &BrowserPressParams {
                        surface_id: surface.surface_id.clone(),
                        selector,
                        key,
                        frame_id,
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser key pressed",
                ))
            }
            BrowserCustomActionRuntime::Select {
                selector,
                values,
                placement,
                frame_id,
            } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserActionResult = self.call_internal_control(
                    "browser.select",
                    &BrowserSelectParams {
                        surface_id: surface.surface_id.clone(),
                        selector,
                        values,
                        frame_id,
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser option selected",
                ))
            }
            BrowserCustomActionRuntime::Scroll {
                selector,
                x,
                y,
                placement,
                frame_id,
            } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserActionResult = self.call_internal_control(
                    "browser.scroll",
                    &BrowserScrollParams {
                        surface_id: surface.surface_id.clone(),
                        selector,
                        x: Some(x),
                        y: Some(y),
                        frame_id,
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser scrolled",
                ))
            }
            BrowserCustomActionRuntime::Hover {
                selector,
                placement,
                frame_id,
            } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserActionResult = self.call_internal_control(
                    "browser.hover",
                    &BrowserHoverParams {
                        surface_id: surface.surface_id.clone(),
                        selector,
                        frame_id,
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser element hovered",
                ))
            }
            BrowserCustomActionRuntime::Check {
                selector,
                checked,
                placement,
                frame_id,
            } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserActionResult = self.call_internal_control(
                    "browser.check",
                    &BrowserCheckParams {
                        surface_id: surface.surface_id.clone(),
                        selector,
                        checked: Some(checked),
                        frame_id,
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser check state set",
                ))
            }
            BrowserCustomActionRuntime::Highlight {
                selector,
                duration_ms,
                placement,
                frame_id,
            } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserActionResult = self.call_internal_control(
                    "browser.highlight",
                    &BrowserHighlightParams {
                        surface_id: surface.surface_id.clone(),
                        selector,
                        duration_ms: Some(duration_ms),
                        frame_id,
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser element highlighted",
                ))
            }
            BrowserCustomActionRuntime::Focus {
                selector,
                placement,
                frame_id,
            } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserActionResult = self.call_internal_control(
                    "browser.focus",
                    &BrowserFocusParams {
                        surface_id: surface.surface_id.clone(),
                        selector,
                        frame_id,
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser element focused",
                ))
            }
            BrowserCustomActionRuntime::Zoom { percent, placement } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserActionResult = self.call_internal_control(
                    "browser.zoom",
                    &BrowserZoomParams {
                        surface_id: surface.surface_id.clone(),
                        percent,
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser zoom set",
                ))
            }
            BrowserCustomActionRuntime::WaitForSelector {
                selector,
                placement,
                timeout_ms,
                frame_id,
            } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let _: BrowserWaitForSelectorResult = self.call_internal_control(
                    "browser.wait_for_selector",
                    &BrowserWaitForSelectorParams {
                        surface_id: surface.surface_id.clone(),
                        selector,
                        timeout_ms: Some(timeout_ms),
                        frame_id,
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser selector appeared",
                ))
            }
            BrowserCustomActionRuntime::NavigationControl {
                operation,
                placement,
            } => {
                let surface = self.create_browser_action_surface(
                    workspace,
                    browser_action_pane_id(workspace, requested_pane_id, &placement),
                    &placement,
                )?;
                let method = match operation.as_str() {
                    "reload" => "browser.reload",
                    "back" => "browser.back",
                    "forward" => "browser.forward",
                    "current-url" => "browser.current_url",
                    _ => unreachable!("normalized browser navigation operation"),
                };
                let _: BrowserNavigationResult = self.call_internal_control(
                    method,
                    &BrowserSurfaceParams {
                        surface_id: surface.surface_id.clone(),
                    },
                )?;
                Ok(browser_action_result(
                    action_id,
                    workspace,
                    surface,
                    "browser navigation executed",
                ))
            }
        }
    }

    fn create_browser_action_surface(
        &self,
        workspace: &WorkspaceSummaryResult,
        pane_id: Option<String>,
        placement: &str,
    ) -> Result<SurfaceSummaryResult, DesktopHostError> {
        self.call_internal_control(
            "surface.create_browser",
            &SurfaceCreateBrowserParams {
                workspace_id: workspace.workspace_id.clone(),
                pane_id,
                profile: Some("default".to_string()),
                placement: Some(placement.to_string()),
            },
        )
    }

    fn action_workspace(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<WorkspaceSummaryResult, DesktopHostError> {
        let workspace_id = workspace_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                DesktopHostError::Control(ControlError::new(
                    ErrorCode::InvalidRequest,
                    "actions.run requires workspace_id for this action.",
                ))
            })?;
        let bundle = self.load_workspace_or_not_found(workspace_id)?;
        Ok(workspace_summary(&bundle.workspace))
    }

    fn call_internal_control<P, R>(&self, method: &str, params: &P) -> Result<R, DesktopHostError>
    where
        P: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        let params_json = serde_json::to_string(params)?;
        let response = self.handle_request(RequestEnvelope::new(
            format!("desktop_actions_run_{method}"),
            method,
            params_json,
            self.control_token.clone(),
        ));
        response_result_json(&response)
    }

    fn handle_system_ping(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &serde_json::json!({ "pong": true }),
        ))
    }

    fn handle_system_capabilities(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &SystemCapabilitiesResult {
                product: "agentmux".to_string(),
                control_schema: agentmux_ipc::CONTROL_SCHEMA.to_string(),
                access_mode: "local_token".to_string(),
                pipe_name: default_control_pipe_name(),
                cmux_compat: true,
                methods: desktop_control_methods()
                    .iter()
                    .map(|method| (*method).to_string())
                    .collect(),
            },
        ))
    }

    fn handle_system_identify(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: SystemIdentifyParams = request.parse_params()?;
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let Some(workspace_id) =
            resolve_optional_workspace_id(&store, params.workspace_id.as_deref())?
        else {
            return Ok(ResponseEnvelope::ok_typed(
                request.id.clone(),
                &SystemIdentifyResult {
                    in_agentmux: false,
                    workspace_id: None,
                    pane_id: None,
                    surface_id: None,
                    session_id: None,
                    cwd: None,
                    backend_kind: None,
                    control_pipe: default_control_pipe_name(),
                },
            ));
        };
        let bundle = store
            .load_workspace_bundle(&workspace_id)?
            .ok_or_else(|| workspace_not_found(&workspace_id))?;
        let pane_id = bundle.workspace.active_pane_id.clone();
        let active_pane = bundle.panes.iter().find(|pane| pane.pane_id == pane_id);
        let surface_id = active_pane.and_then(|pane| pane.mounted_surface_id.clone());
        let surface = surface_id.as_deref().and_then(|surface_id| {
            bundle
                .surfaces
                .iter()
                .find(|surface| surface.surface_id == surface_id)
        });
        let session = surface
            .and_then(|surface| surface.session_id.as_deref())
            .and_then(|session_id| {
                bundle
                    .sessions
                    .iter()
                    .find(|session| session.session_id == session_id)
            });
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &SystemIdentifyResult {
                in_agentmux: true,
                workspace_id: Some(workspace_id),
                pane_id: Some(pane_id),
                surface_id,
                session_id: session.map(|session| session.session_id.clone()),
                cwd: session
                    .and_then(|session| session.cwd.clone())
                    .or(bundle.workspace.project_root),
                backend_kind: session.map(|session| session.backend_kind.clone()),
                control_pipe: default_control_pipe_name(),
            },
        ))
    }

    fn handle_notification_create(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: NotificationCreateParams = request.parse_params()?;
        let title = non_empty(params.title, "notification title")?;
        let message = params
            .body
            .or(params.subtitle)
            .unwrap_or_else(|| title.clone());
        let severity = normalize_sidebar_level(params.severity.as_deref().unwrap_or("info"))?;
        let now = timestamp();
        let notification = PersistedNotification {
            notification_id: format!("notif_cli_{}", unique_time_id()),
            notification_type: "cli.notification".to_string(),
            severity,
            workspace_id: params.workspace_id,
            session_id: params.session_id,
            title,
            message,
            created_at: now,
            dismissed: false,
        };
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        store.upsert_notification(&notification)?;
        let result = notification_result_from_persisted(&notification);
        self.dispatch_desktop_notification(&result);
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn handle_notification_list(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: NotificationListParams = request.parse_params()?;
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let notifications = store
            .list_notifications(
                params.workspace_id.as_deref(),
                params.severity.as_deref(),
                params.include_dismissed.unwrap_or(false),
            )?
            .iter()
            .map(notification_result_from_persisted)
            .collect();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &NotificationListResult { notifications },
        ))
    }

    fn handle_notification_dismiss(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: NotificationDismissParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        if !store.dismiss_notification(&params.notification_id)? {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "Notification not found.",
            )));
        }

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_notification_clear(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: NotificationClearParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let cleared = store
            .clear_notifications(params.workspace_id.as_deref(), params.severity.as_deref())?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &NotificationClearResult { cleared },
        ))
    }

    fn handle_team_task_list(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: TeamTaskListParams = request.parse_params()?;
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let tasks = team_task_results_from_store(&store, params.workspace_id.as_deref())?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &TeamTaskListResult { tasks },
        ))
    }

    fn handle_team_task_create(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: TeamTaskCreateParams = request.parse_params()?;
        let title = non_empty(params.title, "task title")?;
        let now = timestamp();
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let workspace_id = resolve_workspace_id(&store, Some(&params.workspace_id))?;
        let dependency_ready = team_dependencies_satisfied(&store, &params.depends_on)?;
        let task = PersistedTeamTask {
            task_id: format!("task_{}", unique_time_id()),
            workspace_id,
            title,
            description: trim_optional(params.description),
            status: if dependency_ready { "ready" } else { "blocked" }.to_string(),
            assigned_session_id: trim_optional(params.assigned_session_id),
            blocked_reason: (!dependency_ready).then(|| "waiting_on_dependency".to_string()),
            created_at: now.clone(),
            updated_at: now.clone(),
            completed_at: None,
        };
        store.upsert_team_task(&task)?;
        store.replace_team_task_dependencies(&task.task_id, &params.depends_on, &now)?;
        let dependencies = store.list_team_task_dependencies(Some(&task.workspace_id))?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &team_task_result(&task, &dependencies),
        ))
    }

    fn handle_team_task_claim(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: TeamTaskClaimParams = request.parse_params()?;
        let now = timestamp();
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let task = load_team_task_or_not_found(&store, &params.task_id)?;
        store.set_team_task_status(
            &params.task_id,
            "claimed",
            params.session_id.as_deref(),
            None,
            None,
            &now,
        )?;
        let task = store.load_team_task(&params.task_id)?.unwrap_or(task);
        let dependencies = store.list_team_task_dependencies(Some(&task.workspace_id))?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &team_task_result(&task, &dependencies),
        ))
    }

    fn handle_team_task_complete(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: TeamTaskIdParams = request.parse_params()?;
        let now = timestamp();
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let task = load_team_task_or_not_found(&store, &params.task_id)?;
        store.set_team_task_status(&params.task_id, "completed", None, None, Some(&now), &now)?;
        unblock_dependency_ready_tasks(&mut store, &task.workspace_id, &now)?;
        let task = store.load_team_task(&params.task_id)?.unwrap_or(task);
        let dependencies = store.list_team_task_dependencies(Some(&task.workspace_id))?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &team_task_result(&task, &dependencies),
        ))
    }

    fn handle_team_task_block(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: TeamTaskBlockParams = request.parse_params()?;
        let now = timestamp();
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let task = load_team_task_or_not_found(&store, &params.task_id)?;
        let reason = trim_optional(params.reason);
        store.set_team_task_status(
            &params.task_id,
            "blocked",
            None,
            reason.as_deref(),
            None,
            &now,
        )?;
        let task = store.load_team_task(&params.task_id)?.unwrap_or(task);
        let dependencies = store.list_team_task_dependencies(Some(&task.workspace_id))?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &team_task_result(&task, &dependencies),
        ))
    }

    fn handle_team_task_unblock(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: TeamTaskIdParams = request.parse_params()?;
        let now = timestamp();
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let task = load_team_task_or_not_found(&store, &params.task_id)?;
        store.set_team_task_status(&params.task_id, "ready", None, None, None, &now)?;
        let task = store.load_team_task(&params.task_id)?.unwrap_or(task);
        let dependencies = store.list_team_task_dependencies(Some(&task.workspace_id))?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &team_task_result(&task, &dependencies),
        ))
    }

    fn handle_team_task_set_dependency(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: TeamTaskDependencyParams = request.parse_params()?;
        let now = timestamp();
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let task = load_team_task_or_not_found(&store, &params.task_id)?;
        store.replace_team_task_dependencies(&params.task_id, &params.depends_on, &now)?;
        let dependency_ready = team_dependencies_satisfied(&store, &params.depends_on)?;
        if !dependency_ready {
            store.set_team_task_status(
                &params.task_id,
                "blocked",
                None,
                Some("waiting_on_dependency"),
                None,
                &now,
            )?;
        } else if task.status == "blocked"
            && task.blocked_reason.as_deref() == Some("waiting_on_dependency")
        {
            store.set_team_task_status(&params.task_id, "ready", None, None, None, &now)?;
        }
        let task = store.load_team_task(&params.task_id)?.unwrap_or(task);
        let dependencies = store.list_team_task_dependencies(Some(&task.workspace_id))?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &team_task_result(&task, &dependencies),
        ))
    }

    fn handle_team_message_list(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: TeamMessageListParams = request.parse_params()?;
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let messages = store
            .list_team_messages(
                params.workspace_id.as_deref(),
                params.include_read.unwrap_or(true),
            )?
            .iter()
            .map(team_message_result)
            .collect();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &TeamMessageListResult { messages },
        ))
    }

    fn handle_team_message_send(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: TeamMessageSendParams = request.parse_params()?;
        let body = non_empty(params.body, "message body")?;
        let now = timestamp();
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let workspace_id = resolve_workspace_id(&store, Some(&params.workspace_id))?;
        let message = PersistedTeamMessage {
            message_id: format!("msg_{}", unique_time_id()),
            workspace_id: workspace_id.clone(),
            thread_id: trim_optional(params.thread_id),
            from_session_id: trim_optional(params.from_session_id),
            to_session_id: trim_optional(params.to_session_id),
            body,
            kind: trim_optional(params.kind).unwrap_or_else(|| "mailbox".to_string()),
            created_at: now.clone(),
            read_at: None,
        };
        store.upsert_team_message(&message)?;
        let notification = PersistedNotification {
            notification_id: format!("not_team_message_{}", unique_time_id()),
            notification_type: "team.message".to_string(),
            severity: "info".to_string(),
            workspace_id: Some(workspace_id),
            session_id: message.to_session_id.clone(),
            title: "Agent message".to_string(),
            message: message.body.clone(),
            created_at: now,
            dismissed: false,
        };
        store.upsert_notification(&notification)?;
        let result = team_message_result(&message);
        self.dispatch_desktop_notification(&notification_result_from_persisted(&notification));
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn handle_team_message_mark_read(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: TeamMessageMarkReadParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        if !store.mark_team_message_read(&params.message_id, &timestamp())? {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "Team message not found.",
            )));
        }
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_sidebar_set_status(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: SidebarStatusSetParams = request.parse_params()?;
        let key = non_empty(params.key, "status key")?;
        let label = non_empty(params.label, "status label")?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let workspace_id = resolve_workspace_id(&store, params.workspace_id.as_deref())?;
        let status = PersistedSidebarStatus {
            workspace_id,
            key,
            label,
            icon: trim_optional(params.icon),
            color: trim_optional(params.color),
            priority: params.priority.unwrap_or(0),
            updated_at: timestamp(),
        };
        store.upsert_sidebar_status(&status)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &sidebar_status_result(&status),
        ))
    }

    fn handle_sidebar_clear_status(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: SidebarStatusKeyParams = request.parse_params()?;
        let key = non_empty(params.key, "status key")?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let workspace_id = resolve_workspace_id(&store, params.workspace_id.as_deref())?;
        store.delete_sidebar_status(&workspace_id, &key)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_sidebar_list_status(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: SidebarWorkspaceParams = request.parse_params()?;
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let workspace_id = resolve_workspace_id(&store, params.workspace_id.as_deref())?;
        let statuses = store
            .list_sidebar_status(&workspace_id)?
            .iter()
            .map(sidebar_status_result)
            .collect();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &SidebarStatusListResult { statuses },
        ))
    }

    fn handle_sidebar_set_progress(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: SidebarProgressSetParams = request.parse_params()?;
        if !params.value.is_finite() {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "Progress value must be finite.",
            )));
        }
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let workspace_id = resolve_workspace_id(&store, params.workspace_id.as_deref())?;
        let progress = PersistedSidebarProgress {
            workspace_id,
            value: params.value.clamp(0.0, 1.0),
            label: trim_optional(params.label),
            updated_at: timestamp(),
        };
        store.upsert_sidebar_progress(&progress)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &sidebar_progress_result(&progress),
        ))
    }

    fn handle_sidebar_clear_progress(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: SidebarWorkspaceParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let workspace_id = resolve_workspace_id(&store, params.workspace_id.as_deref())?;
        store.delete_sidebar_progress(&workspace_id)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_sidebar_log(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: SidebarLogAddParams = request.parse_params()?;
        let message = non_empty(params.message, "log message")?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let workspace_id = resolve_workspace_id(&store, params.workspace_id.as_deref())?;
        let log = PersistedSidebarLog {
            log_id: format!("log_{}", unique_time_id()),
            workspace_id,
            level: normalize_sidebar_level(params.level.as_deref().unwrap_or("info"))?,
            source: trim_optional(params.source),
            message,
            created_at: timestamp(),
        };
        store.append_sidebar_log(&log)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &sidebar_log_result(&log),
        ))
    }

    fn handle_sidebar_clear_log(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: SidebarWorkspaceParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let workspace_id = resolve_workspace_id(&store, params.workspace_id.as_deref())?;
        store.clear_sidebar_logs(&workspace_id)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_sidebar_list_log(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: SidebarLogListParams = request.parse_params()?;
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let workspace_id = resolve_workspace_id(&store, params.workspace_id.as_deref())?;
        let logs = store
            .list_sidebar_logs(&workspace_id, params.limit)?
            .iter()
            .map(sidebar_log_result)
            .collect();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &SidebarLogListResult { logs },
        ))
    }

    fn handle_sidebar_state(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: SidebarWorkspaceParams = request.parse_params()?;
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let workspace_id = resolve_workspace_id(&store, params.workspace_id.as_deref())?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &sidebar_state_result(&store, &workspace_id)?,
        ))
    }

    fn handle_profile_list(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let profiles = store.list_profiles()?.iter().map(profile_summary).collect();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &ProfileListResult { profiles },
        ))
    }

    fn handle_profile_create(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: ProfileCreateParams = request.parse_params()?;
        validate_profile_fields(&params.name, &params.host, &params.user)?;
        let now = timestamp();
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let profile = PersistedProfile {
            profile_id: format!("prof_{millis}"),
            name: params.name,
            host: params.host,
            user: params.user,
            port: params.port,
            created_at: now.clone(),
            updated_at: now,
        };
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        store.upsert_profile(&profile)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &profile_summary(&profile),
        ))
    }

    fn handle_profile_update(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: ProfileUpdateParams = request.parse_params()?;
        validate_profile_fields(&params.name, &params.host, &params.user)?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let existing = store.load_profile(&params.profile_id)?.ok_or_else(|| {
            DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "Profile not found.",
            ))
        })?;
        let profile = PersistedProfile {
            profile_id: existing.profile_id,
            name: params.name,
            host: params.host,
            user: params.user,
            port: params.port,
            created_at: existing.created_at,
            updated_at: timestamp(),
        };
        store.upsert_profile(&profile)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &profile_summary(&profile),
        ))
    }

    fn handle_profile_delete(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: ProfileIdParams = request.parse_params()?;
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        if !store.delete_profile(&params.profile_id)? {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "Profile not found.",
            )));
        }
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_browser_diagnostics(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserDiagnosticsParams = request.parse_params()?;
        let Ok(failures) = self.browser_failures.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "browser diagnostics state is unavailable".to_string(),
            ));
        };
        let failures = failures
            .iter()
            .filter(|failure| {
                let Some(workspace_id) = params.workspace_id.as_deref() else {
                    return true;
                };
                failure.workspace_id.as_deref() == Some(workspace_id)
            })
            .filter(|failure| {
                let Some(surface_id) = params.surface_id.as_deref() else {
                    return true;
                };
                failure.surface_id.as_deref() == Some(surface_id)
            })
            .map(browser_diagnostic_result_from_record)
            .collect();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &BrowserDiagnosticsResult { failures },
        ))
    }

    fn handle_config_get(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: AppConfigGetParams = request.parse_params()?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &self.load_effective_app_config(params.workspace_id.as_deref())?,
        ))
    }

    fn handle_config_reload(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        self.handle_config_get(request)
    }

    fn handle_config_update(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: AppConfigUpdateParams = request.parse_params()?;
        let mut config = load_app_config(&self.config_path)?;

        if let Some(appearance) = params.appearance {
            if let Some(theme) = appearance.theme {
                config.appearance.theme = normalize_theme(&theme)?;
            }
            if let Some(accent_key) = appearance.accent_key {
                config.appearance.accent_key = normalize_accent_key(&accent_key)?;
            }
            if let Some(font_size) = appearance.font_size {
                config.appearance.font_size = normalize_font_size(font_size)?;
            }
        }
        if let Some(locale) = params.locale {
            if let Some(language) = locale.language {
                config.locale.language = normalize_locale_language(&language)?;
            }
        }
        if let Some(updates) = params.updates {
            if let Some(auto_check) = updates.auto_check {
                config.updates.auto_check = auto_check;
            }
        }
        if let Some(shortcuts) = params.shortcuts {
            if let Some(mut bindings) = shortcuts.bindings {
                normalize_shortcut_bindings(&mut bindings)?;
                config.shortcuts.bindings.extend(bindings);
            }
        }
        if let Some(ui) = params.ui {
            if ui.workspace_plus_action.is_some() {
                config.ui.workspace_plus_action = normalize_optional_action_reference(
                    ui.workspace_plus_action,
                    "workspace_plus_action",
                )?;
            }
            if ui.surface_tab_plus_action.is_some() {
                config.ui.surface_tab_plus_action = normalize_optional_action_reference(
                    ui.surface_tab_plus_action,
                    "surface_tab_plus_action",
                )?;
            }
            if let Some(mut actions) = ui.surface_tab_actions {
                let mut normalized = Vec::new();
                for action in actions.drain(..) {
                    let action = normalize_action_reference(&action, "surface_tab_actions")?;
                    if !normalized.contains(&action) {
                        normalized.push(action);
                    }
                }
                config.ui.surface_tab_actions = Some(normalized);
            }
            if let Some(lines) = ui.text_box_max_lines {
                if !(TEXT_BOX_MIN_LINES..=TEXT_BOX_MAX_LINES).contains(&lines) {
                    return Err(invalid_text_box_max_lines(lines));
                }
                config.ui.text_box_max_lines = Some(lines);
            }
            if let Some(margin) = ui.terminal_inner_margin {
                config.ui.terminal_inner_margin = Some(normalize_terminal_inner_margin(margin)?);
            }
            if let Some(directory) = ui.terminal_start_directory {
                config.ui.terminal_start_directory =
                    Some(normalize_terminal_start_directory(&directory)?);
            }
            if let Some(cwd) = ui.terminal_start_custom_cwd {
                config.ui.terminal_start_custom_cwd = normalize_terminal_start_custom_cwd(cwd);
            }
            if let Some(behavior) = ui.terminal_split_behavior {
                config.ui.terminal_split_behavior =
                    Some(normalize_terminal_split_behavior(&behavior)?);
            }
        }

        save_app_config(&self.config_path, &config)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &self.effective_app_config_result(config, params.workspace_id.as_deref())?,
        ))
    }

    fn handle_config_export(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: AppConfigExportParams = request.parse_params()?;
        let scope = normalize_config_scope(params.scope.as_deref())?;
        let config = self.load_effective_app_config(params.workspace_id.as_deref())?;
        let json = match scope {
            AppConfigScope::Global => export_app_config_json(&config)?,
            AppConfigScope::Project => {
                let path = self.required_project_config_path(params.workspace_id.as_deref())?;
                let project_config = load_project_app_config(&path)?.unwrap_or_default();
                export_project_app_config_json(&project_config)?
            }
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AppConfigExportResult { json, config },
        ))
    }

    fn handle_config_import(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: AppConfigImportParams = request.parse_params()?;
        let scope = normalize_config_scope(params.scope.as_deref())?;
        match scope {
            AppConfigScope::Global => {
                let mut config: AppConfigFile = serde_json::from_str(&params.json)?;
                normalize_app_config_file(&mut config)?;
                save_app_config(&self.config_path, &config)?;
            }
            AppConfigScope::Project => {
                let path = self.required_project_config_path(params.workspace_id.as_deref())?;
                let mut config: ProjectAppConfigFile = serde_json::from_str(&params.json)?;
                normalize_project_app_config_file(&mut config)?;
                save_project_app_config(&path, &config)?;
            }
        }
        let config = load_app_config(&self.config_path)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &self.effective_app_config_result(config, params.workspace_id.as_deref())?,
        ))
    }

    fn handle_config_reset(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: AppConfigResetParams = request.parse_params()?;
        let scope = normalize_config_scope(params.scope.as_deref())?;
        match scope {
            AppConfigScope::Global => {
                save_app_config(&self.config_path, &default_app_config())?;
            }
            AppConfigScope::Project => {
                let path = self.required_project_config_path(params.workspace_id.as_deref())?;
                match fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(DesktopHostError::StateUnavailable(format!(
                            "failed to remove AgentMux project config '{}': {error}",
                            path.display()
                        )));
                    }
                }
            }
        }
        let config = load_app_config(&self.config_path)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &self.effective_app_config_result(config, params.workspace_id.as_deref())?,
        ))
    }

    fn handle_config_migrate_project(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: AppConfigMigrateProjectParams = request.parse_params()?;
        let Some(workspace_id) = params.workspace_id.as_deref() else {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "Project config migration requires a workspace_id.",
            )));
        };
        let target_path = self.required_project_config_path(Some(workspace_id))?;
        let Some(source_path) = self.cmux_project_config_path_for_workspace(workspace_id)? else {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "Workspace has no project_root for .cmux project config migration.",
            )));
        };
        let mut project_config = load_project_app_config(&source_path)?.ok_or_else(|| {
            DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                format!(
                    "No cmux project config found at '{}'.",
                    source_path.display()
                ),
            ))
        })?;
        normalize_project_app_config_file(&mut project_config)?;

        let overwritten = target_path.exists();
        if overwritten && !params.overwrite.unwrap_or(false) {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                format!(
                    "AgentMux project config already exists at '{}'. Pass overwrite=true to replace it.",
                    target_path.display()
                ),
            )));
        }

        save_project_app_config(&target_path, &project_config)?;
        let config = load_app_config(&self.config_path)?;
        let config = self.effective_app_config_result(config, Some(workspace_id))?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AppConfigMigrateProjectResult {
                source_path: source_path.to_string_lossy().to_string(),
                target_path: target_path.to_string_lossy().to_string(),
                overwritten,
                config,
            },
        ))
    }

    fn handle_config_diagnostics(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: AppConfigDiagnosticsParams = request.parse_params()?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &self.config_diagnostics(params.workspace_id.as_deref())?,
        ))
    }

    fn config_diagnostics(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<AppConfigDiagnosticsResult, DesktopHostError> {
        let mut entries = Vec::new();
        entries.push(diagnose_global_app_config(&self.config_path));

        if let Some(workspace_id) = workspace_id {
            let project_path = self.project_config_path_for_workspace(workspace_id)?;
            let cmux_path = self.cmux_project_config_path_for_workspace(workspace_id)?;
            let project_exists = project_path.as_ref().is_some_and(|path| path.exists());
            if let Some(path) = project_path {
                entries.push(diagnose_project_app_config(
                    "project",
                    &path,
                    project_exists,
                    project_exists,
                    if project_exists {
                        None
                    } else {
                        Some("AgentMux project config is absent.")
                    },
                ));
            } else {
                entries.push(config_diagnostics_entry(
                    "project",
                    None,
                    false,
                    true,
                    false,
                    "Workspace has no project_root for AgentMux project config.",
                ));
            }

            if let Some(path) = cmux_path {
                let cmux_exists = path.exists();
                if cmux_exists {
                    entries.push(diagnose_project_app_config(
                        "cmux_project",
                        &path,
                        true,
                        !project_exists,
                        if project_exists {
                            Some("Legacy cmux config is ignored because AgentMux project config exists.")
                        } else {
                            Some("Legacy cmux config is available for migration.")
                        },
                    ));
                }
            }
        }

        Ok(AppConfigDiagnosticsResult { entries })
    }

    fn handle_dock_get(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: DockGetParams = request.parse_params()?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &self.load_dock_config(params.workspace_id.as_deref())?,
        ))
    }

    fn handle_dock_trust(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: DockTrustParams = request.parse_params()?;
        self.trust_current_dock_config(&params.workspace_id)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &self.load_dock_config(Some(&params.workspace_id))?,
        ))
    }

    fn load_dock_config(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<DockConfigResult, DesktopHostError> {
        for candidate in self.dock_config_candidates(workspace_id)? {
            if !candidate.path.exists() {
                continue;
            }
            let (mut config, config_hash) = load_dock_config_file_with_hash(&candidate.path)?;
            normalize_dock_config_file(&mut config)?;
            let trusted = self.dock_config_trusted(workspace_id, &candidate, &config_hash)?;
            return Ok(DockConfigResult {
                source: candidate.source.to_string(),
                config_path: Some(candidate.path.to_string_lossy().to_string()),
                requires_trust: candidate.requires_trust,
                trusted,
                controls: config
                    .controls
                    .into_iter()
                    .map(dock_control_result)
                    .collect(),
            });
        }

        Ok(DockConfigResult {
            source: "none".to_string(),
            config_path: None,
            requires_trust: false,
            trusted: false,
            controls: Vec::new(),
        })
    }

    fn trust_current_dock_config(&self, workspace_id: &str) -> Result<(), DesktopHostError> {
        for candidate in self.dock_config_candidates(Some(workspace_id))? {
            if !candidate.path.exists() {
                continue;
            }
            let (mut config, config_hash) = load_dock_config_file_with_hash(&candidate.path)?;
            normalize_dock_config_file(&mut config)?;
            if !candidate.requires_trust {
                return Ok(());
            }

            let now = timestamp();
            let config_path = candidate.path.to_string_lossy().to_string();
            let Ok(mut store) = self.store.lock() else {
                return Err(DesktopHostError::StateUnavailable(
                    "desktop store state is unavailable".to_string(),
                ));
            };
            store.upsert_dock_trust(&PersistedDockTrust {
                workspace_id: workspace_id.to_string(),
                source: candidate.source.to_string(),
                config_path,
                config_hash,
                trusted_at: now.clone(),
                updated_at: now,
            })?;
            return Ok(());
        }

        Ok(())
    }

    fn dock_config_trusted(
        &self,
        workspace_id: Option<&str>,
        candidate: &DockConfigCandidate,
        config_hash: &str,
    ) -> Result<bool, DesktopHostError> {
        if !candidate.requires_trust {
            return Ok(true);
        }
        let Some(workspace_id) = workspace_id else {
            return Ok(false);
        };
        let config_path = candidate.path.to_string_lossy().to_string();
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        store
            .dock_trust_matches(workspace_id, candidate.source, &config_path, config_hash)
            .map_err(DesktopHostError::from)
    }

    fn dock_config_candidates(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<DockConfigCandidate>, DesktopHostError> {
        let mut candidates = Vec::new();
        if let Some(workspace_id) = workspace_id {
            let bundle = self.load_workspace_or_not_found(workspace_id)?;
            if let Some(project_root) = bundle
                .workspace
                .project_root
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                let root = PathBuf::from(project_root);
                candidates.push(DockConfigCandidate {
                    source: "project_agentmux",
                    path: root
                        .join(PROJECT_CONFIG_DIR_NAME)
                        .join(DOCK_CONFIG_FILE_NAME),
                    requires_trust: true,
                });
                candidates.push(DockConfigCandidate {
                    source: "project_cmux",
                    path: root
                        .join(CMUX_PROJECT_CONFIG_DIR_NAME)
                        .join(DOCK_CONFIG_FILE_NAME),
                    requires_trust: true,
                });
            }
        }
        candidates.push(DockConfigCandidate {
            source: "global_agentmux",
            path: global_agentmux_dock_config_path(&self.config_path),
            requires_trust: false,
        });
        if let Some(path) = global_cmux_dock_config_path() {
            candidates.push(DockConfigCandidate {
                source: "global_cmux",
                path,
                requires_trust: false,
            });
        }
        Ok(candidates)
    }

    fn load_effective_app_config(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<AppConfigResult, DesktopHostError> {
        let config = load_app_config(&self.config_path)?;
        self.effective_app_config_result(config, workspace_id)
    }

    fn effective_app_config_result(
        &self,
        mut config: AppConfigFile,
        workspace_id: Option<&str>,
    ) -> Result<AppConfigResult, DesktopHostError> {
        let mut project_config_path = None;
        let mut project_config_loaded = false;

        if let Some(workspace_id) = workspace_id {
            if let Some(path) = self.project_config_path_for_workspace(workspace_id)? {
                project_config_path = Some(path.to_string_lossy().to_string());
                if let Some(project_config) =
                    self.load_project_app_config_with_cmux_fallback(workspace_id, &path)?
                {
                    config
                        .shortcuts
                        .bindings
                        .extend(project_config.shortcuts.bindings);
                    merge_custom_actions(&mut config.actions.custom, project_config.actions.custom);
                    merge_app_config_ui(&mut config.ui, project_config.ui);
                    merge_app_config_notifications(
                        &mut config.notifications,
                        project_config.notifications,
                    );
                    project_config_loaded = true;
                }
            }
        }

        Ok(app_config_result(
            &self.config_path,
            config,
            project_config_path,
            project_config_loaded,
        ))
    }

    fn load_project_app_config_with_cmux_fallback(
        &self,
        workspace_id: &str,
        agentmux_path: &Path,
    ) -> Result<Option<ProjectAppConfigFile>, DesktopHostError> {
        if let Some(config) = load_project_app_config(agentmux_path)? {
            return Ok(Some(config));
        }

        let Some(cmux_path) = self.cmux_project_config_path_for_workspace(workspace_id)? else {
            return Ok(None);
        };
        load_project_app_config(&cmux_path)
    }

    fn project_config_path_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Option<PathBuf>, DesktopHostError> {
        let bundle = self.load_workspace_or_not_found(workspace_id)?;
        let Some(project_root) = bundle
            .workspace
            .project_root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };
        Ok(Some(
            PathBuf::from(project_root)
                .join(PROJECT_CONFIG_DIR_NAME)
                .join(APP_CONFIG_FILE_NAME),
        ))
    }

    fn cmux_project_config_path_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Option<PathBuf>, DesktopHostError> {
        let bundle = self.load_workspace_or_not_found(workspace_id)?;
        let Some(project_root) = bundle
            .workspace
            .project_root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };
        Ok(Some(
            PathBuf::from(project_root)
                .join(CMUX_PROJECT_CONFIG_DIR_NAME)
                .join(CMUX_PROJECT_CONFIG_FILE_NAME),
        ))
    }

    fn required_project_config_path(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<PathBuf, DesktopHostError> {
        let Some(workspace_id) = workspace_id else {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "Project config operations require a workspace_id.",
            )));
        };
        self.project_config_path_for_workspace(workspace_id)?
            .ok_or_else(|| {
                DesktopHostError::Control(ControlError::new(
                    ErrorCode::InvalidRequest,
                    "Workspace has no project_root for project config operations.",
                ))
            })
    }

    fn handle_diagnostics_export(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &self.diagnostics_export()?,
        ))
    }

    fn handle_recovery_diagnostics(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let snapshot = self.recovery_snapshot()?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &recovery_diagnostics(snapshot),
        ))
    }

    fn handle_wsl_distributions(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let distributions = discover_wsl_distributions()?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &WslDistributionListResult {
                distributions: distributions
                    .into_iter()
                    .map(|distribution| WslDistributionResult {
                        name: distribution.name,
                        is_default: distribution.is_default,
                    })
                    .collect(),
            },
        ))
    }

    fn handle_tmux_diagnostics(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: TmuxDiagnosticsParams = request.parse_params()?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &tmux_diagnostics(params.distribution.as_deref()),
        ))
    }

    fn diagnostics_export(&self) -> Result<DiagnosticsExportResult, DesktopHostError> {
        let recovery = recovery_diagnostics(self.recovery_snapshot()?);
        let backend_health = backend_health_from_recovery(&recovery);
        let (browser, browser_depth, browser_dropped) = self.browser_diagnostics_snapshot()?;
        let notifications = {
            let Ok(store) = self.store.lock() else {
                return Err(DesktopHostError::StateUnavailable(
                    "desktop store state is unavailable".to_string(),
                ));
            };
            store
                .list_notifications(None, None, true)?
                .iter()
                .map(notification_result_from_persisted)
                .collect()
        };
        let mut queue_pressure = {
            let Ok(mut control) = self.control.lock() else {
                return Err(DesktopHostError::StateUnavailable(
                    "desktop control state is unavailable".to_string(),
                ));
            };
            control.collect_events();
            vec![
                queue_pressure_result(
                    "runtime.events.pending",
                    control.event_queue_depth(),
                    control.event_backlog_limit(),
                    control.dropped_event_count(),
                ),
                queue_pressure_result(
                    "runtime.events.history",
                    control.event_history_depth(),
                    control.event_backlog_limit(),
                    control.dropped_event_count(),
                ),
                queue_pressure_result(
                    "runtime.notifications",
                    control.notification_depth(),
                    control.notification_limit(),
                    0,
                ),
            ]
        };
        queue_pressure.push(queue_pressure_result(
            "desktop.browser_failures",
            browser_depth,
            MAX_BROWSER_FAILURES,
            browser_dropped,
        ));
        let output_stream = self.output_stream_diagnostics();

        Ok(DiagnosticsExportResult {
            generated_at: timestamp(),
            format_version: "agentmux.diagnostics.v1".to_string(),
            recovery,
            browser,
            notifications,
            backend_health,
            queue_pressure,
            output_stream,
        })
    }

    fn browser_diagnostics_snapshot(
        &self,
    ) -> Result<(BrowserDiagnosticsResult, usize, usize), DesktopHostError> {
        let Ok(failures) = self.browser_failures.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "browser diagnostics state is unavailable".to_string(),
            ));
        };
        let depth = failures.len();
        let failures = failures
            .iter()
            .map(browser_diagnostic_result_from_record)
            .collect();
        let dropped = {
            let Ok(counter) = self.browser_failure_counter.lock() else {
                return Err(DesktopHostError::StateUnavailable(
                    "browser diagnostics counter is unavailable".to_string(),
                ));
            };
            (*counter as usize).saturating_sub(MAX_BROWSER_FAILURES)
        };
        Ok((BrowserDiagnosticsResult { failures }, depth, dropped))
    }

    fn execute_browser_command(
        &self,
        operation: &str,
        command: BrowserCommand,
    ) -> Result<BrowserCommandResult, DesktopHostError> {
        let surface_id = command.surface_id().to_string();
        let result = self.execute_browser_command_inner(command);
        if let Err(error) = &result {
            self.record_browser_failure_from_error(operation, Some(surface_id), error);
        }
        result
    }

    fn execute_browser_command_inner(
        &self,
        command: BrowserCommand,
    ) -> Result<BrowserCommandResult, DesktopHostError> {
        let surface = self.load_browser_surface_or_not_found(command.surface_id())?;
        let Ok(mut browser) = self.browser.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "browser automation state is unavailable".to_string(),
            ));
        };
        if matches!(
            browser.surface(&surface.surface_id),
            Err(BrowserAutomationError {
                code: BrowserAutomationErrorCode::SurfaceNotFound,
                ..
            })
        ) {
            browser
                .create_surface(
                    surface.surface_id.clone(),
                    surface.workspace_id.clone(),
                    None,
                )
                .map_err(browser_error_from_automation)?;
        }
        browser
            .execute(command)
            .map_err(browser_error_from_automation)
    }

    fn record_browser_failure_from_error(
        &self,
        operation: &str,
        surface_id: Option<String>,
        error: &DesktopHostError,
    ) {
        let workspace_id = surface_id
            .as_deref()
            .and_then(|surface_id| self.browser_failure_workspace_id(surface_id).ok().flatten());
        let record = BrowserFailureRecord {
            surface_id,
            workspace_id,
            operation: operation.to_string(),
            code: browser_failure_error_code(error).as_str().to_string(),
            message: browser_failure_error_message(error),
            occurred_at: timestamp(),
        };
        let _ = self.record_browser_failure(record);
    }

    fn record_browser_failure(&self, record: BrowserFailureRecord) -> Result<(), DesktopHostError> {
        let sequence = {
            let Ok(mut counter) = self.browser_failure_counter.lock() else {
                return Err(DesktopHostError::StateUnavailable(
                    "browser diagnostics counter is unavailable".to_string(),
                ));
            };
            *counter += 1;
            *counter
        };

        {
            let Ok(mut failures) = self.browser_failures.lock() else {
                return Err(DesktopHostError::StateUnavailable(
                    "browser diagnostics state is unavailable".to_string(),
                ));
            };
            failures.push_back(record.clone());
            while failures.len() > MAX_BROWSER_FAILURES {
                failures.pop_front();
            }
        }

        let notification = browser_failure_notification(&record, sequence);
        {
            let Ok(mut store) = self.store.lock() else {
                return Err(DesktopHostError::StateUnavailable(
                    "desktop store state is unavailable".to_string(),
                ));
            };
            store.upsert_notification(&notification)?;
        }
        self.dispatch_desktop_notification(&notification_result_from_persisted(&notification));
        Ok(())
    }

    fn browser_failure_workspace_id(
        &self,
        surface_id: &str,
    ) -> Result<Option<String>, DesktopHostError> {
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        for workspace in store.list_workspaces()? {
            let Some(bundle) = store.load_workspace_bundle(&workspace.workspace_id)? else {
                continue;
            };
            if let Some(surface) = bundle
                .surfaces
                .into_iter()
                .find(|surface| surface.surface_id == surface_id)
            {
                return Ok(Some(surface.workspace_id));
            }
        }
        Ok(None)
    }

    fn load_browser_surface_or_not_found(
        &self,
        surface_id: &str,
    ) -> Result<PersistedSurface, DesktopHostError> {
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        for workspace in store.list_workspaces()? {
            let Some(bundle) = store.load_workspace_bundle(&workspace.workspace_id)? else {
                continue;
            };
            if let Some(surface) = bundle
                .surfaces
                .into_iter()
                .find(|surface| surface.surface_id == surface_id)
            {
                if surface.surface_type != "browser" {
                    return Err(DesktopHostError::Control(ControlError::new(
                        ErrorCode::InvalidRequest,
                        format!("Surface '{surface_id}' is not a browser surface."),
                    )));
                }
                return Ok(surface);
            }
        }
        Err(surface_not_found(surface_id).into())
    }

    fn load_workspace_or_not_found(
        &self,
        workspace_id: &str,
    ) -> Result<WorkspaceBundle, DesktopHostError> {
        let Ok(mut store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        let mut bundle = store
            .load_workspace_bundle(workspace_id)?
            .ok_or_else(|| DesktopHostError::from(workspace_not_found(workspace_id)))?;
        if normalize_workspace_pane_tree(&mut bundle) {
            store.save_workspace_bundle(&bundle)?;
        }
        Ok(bundle)
    }

    fn coordinate_workspace_close(
        &self,
        bundle: &WorkspaceBundle,
        close_policy: &str,
    ) -> Result<(), DesktopHostError> {
        let Ok(mut control) = self.control.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop control state is unavailable".to_string(),
            ));
        };
        control.collect_events();

        match close_policy {
            "fail_if_running" => {
                if workspace_has_active_sessions(&mut control, bundle, &self.control_token)? {
                    return Err(DesktopHostError::Control(ControlError::new(
                        ErrorCode::Conflict,
                        "Workspace has running sessions.",
                    )));
                }
            }
            "detach_sessions" => {
                close_live_workspace_sessions(&mut control, bundle, "soft", &self.control_token)?;
            }
            "terminate_sessions" => {
                close_live_workspace_sessions(&mut control, bundle, "kill", &self.control_token)?;
            }
            _ => unreachable!("workspace close policy is validated before coordination"),
        }

        control.collect_events();
        Ok(())
    }

    fn coordinate_pane_close(
        &self,
        bundle: &WorkspaceBundle,
        pane_id: &str,
        surface_policy: &str,
    ) -> Result<(), DesktopHostError> {
        if surface_policy != "fail_if_session_running" {
            return Ok(());
        }

        let Some(surface_id) = bundle
            .panes
            .iter()
            .find(|pane| pane.pane_id == pane_id)
            .ok_or_else(|| DesktopHostError::Control(pane_not_found(pane_id)))?
            .mounted_surface_id
            .as_deref()
        else {
            return Ok(());
        };
        let Some(session_id) = bundle
            .surfaces
            .iter()
            .find(|surface| surface.surface_id == surface_id)
            .and_then(|surface| surface.session_id.as_deref())
        else {
            return Ok(());
        };

        let Ok(mut control) = self.control.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop control state is unavailable".to_string(),
            ));
        };
        control.collect_events();
        if session_is_active(&mut control, bundle, session_id, &self.control_token)? {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::Conflict,
                "Pane has a running session.",
            )));
        }
        Ok(())
    }

    fn close_live_surface_session(
        &self,
        bundle: &WorkspaceBundle,
        session_id: &str,
    ) -> Result<(), DesktopHostError> {
        let Ok(mut control) = self.control.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop control state is unavailable".to_string(),
            ));
        };
        control.collect_events();
        if session_is_active(&mut control, bundle, session_id, &self.control_token)? {
            terminate_live_session(&mut control, session_id, "kill", &self.control_token)?;
            control.collect_events();
        }
        Ok(())
    }

    fn close_browser_surface_if_present(&self, surface_id: &str) -> Result<(), DesktopHostError> {
        let Ok(mut browser) = self.browser.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "browser automation state is unavailable".to_string(),
            ));
        };
        match browser.close_surface(surface_id) {
            Ok(_) => Ok(()),
            Err(error) if error.code == BrowserAutomationErrorCode::SurfaceNotFound => Ok(()),
            Err(error) => Err(browser_error_from_automation(error)),
        }
    }
}

impl Default for DesktopControlState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn agentmux_control(state: &DesktopControlState, request: RequestEnvelope) -> ResponseEnvelope {
    state.handle_request(request)
}

pub fn start_control_pipe_server(state: Arc<DesktopControlState>, pipe_name: impl Into<String>) {
    let pipe_name = pipe_name.into();
    std::thread::spawn(move || {
        if let Err(error) =
            agentmux_ipc::serve_named_pipe_streaming_requests(&pipe_name, move |request, stream| {
                state.handle_pipe_connection(request, stream)
            })
        {
            eprintln!("agentmux: control pipe server stopped: {error}");
        }
    });
}

pub fn default_control_pipe_name() -> String {
    std::env::var("AGENTMUX_CONTROL_PIPE").unwrap_or_else(|_| DEFAULT_CONTROL_PIPE_NAME.to_string())
}

fn desktop_control_methods() -> &'static [&'static str] {
    &[
        "system.ping",
        "system.capabilities",
        "system.identify",
        "workspace.create",
        "workspace.list",
        "workspace.get",
        "workspace.rename",
        "workspace.update",
        "workspace.close",
        "workspace_group.list",
        "workspace_group.create",
        "workspace_group.update",
        "workspace_group.delete",
        "workspace_group.add_workspace",
        "workspace_group.remove_workspace",
        "pane.split",
        "pane.focus",
        "pane.close",
        "pane.resize_layout",
        "pane.mount_surface",
        "pane.unmount_surface",
        "surface.create_browser",
        "surface.close",
        "surface.move_workspace",
        "session.spawn",
        "session.attach",
        "session.list",
        "session.get",
        "session.send_text",
        "session.send_key",
        "session.resize",
        "session.terminate",
        "session.read_recent",
        "agent.set_state",
        "agent.get_state",
        "agent.list_attention",
        "agent.clear_attention",
        "agent.list",
        "actions.list",
        "actions.run",
        "notification.create",
        "notification.list",
        "notification.dismiss",
        "notification.clear",
        "team.task.list",
        "team.task.create",
        "team.task.claim",
        "team.task.complete",
        "team.task.block",
        "team.task.unblock",
        "team.task.set_dependency",
        "team.message.list",
        "team.message.send",
        "team.message.mark_read",
        "sidebar.set_status",
        "sidebar.clear_status",
        "sidebar.list_status",
        "sidebar.set_progress",
        "sidebar.clear_progress",
        "sidebar.log",
        "sidebar.clear_log",
        "sidebar.list_log",
        "sidebar.state",
        "browser.navigate",
        "browser.screenshot",
        "browser.dom_snapshot",
        "browser.frames",
        "browser.storage",
        "browser.cookies",
        "browser.downloads",
        "browser.history",
        "browser.console",
        "browser.dialogs",
        "browser.errors",
        "browser.click",
        "browser.type",
        "browser.evaluate",
        "profile.list",
        "profile.create",
        "profile.update",
        "profile.delete",
        "config.get",
        "config.reload",
        "config.update",
        "config.export",
        "config.import",
        "config.reset",
        "config.migrate_project",
        "config.diagnostics",
        "dock.get",
        "dock.trust",
        "diagnostics.browser",
        "diagnostics.export",
        "diagnostics.recovery",
        "diagnostics.wsl_distributions",
        "diagnostics.tmux",
        "events.poll",
        "events.subscribe",
    ]
}

fn managed_terminal_env(
    workspace_id: &str,
    token: &str,
    surface_id: &str,
    pane_id: &str,
    extra_wsl_env_keys: &[String],
) -> Vec<EnvVarParam> {
    let pipe = default_control_pipe_name();
    let wsl_env = managed_wsl_env_value(extra_wsl_env_keys);
    let tmux_pane = agentmux_pane_to_tmux_pane(pane_id);
    let tmux = format!("agentmux,{workspace_id},{pane_id}");
    vec![
        EnvVarParam {
            key: "AGENTMUX_CONTROL_PIPE".to_string(),
            value: pipe.clone(),
        },
        EnvVarParam {
            key: "AGENTMUX_CONTROL_TOKEN".to_string(),
            value: token.to_string(),
        },
        EnvVarParam {
            key: "AGENTMUX_WORKSPACE_ID".to_string(),
            value: workspace_id.to_string(),
        },
        EnvVarParam {
            key: "AGENTMUX_SURFACE_ID".to_string(),
            value: surface_id.to_string(),
        },
        EnvVarParam {
            key: "AGENTMUX_PANE_ID".to_string(),
            value: pane_id.to_string(),
        },
        EnvVarParam {
            key: "CMUX_SOCKET_PATH".to_string(),
            value: pipe,
        },
        EnvVarParam {
            key: "CMUX_WORKSPACE_ID".to_string(),
            value: workspace_id.to_string(),
        },
        EnvVarParam {
            key: "CMUX_SURFACE_ID".to_string(),
            value: surface_id.to_string(),
        },
        EnvVarParam {
            key: "CMUX_PANE_ID".to_string(),
            value: pane_id.to_string(),
        },
        EnvVarParam {
            key: "TMUX".to_string(),
            value: tmux,
        },
        EnvVarParam {
            key: "TMUX_PANE".to_string(),
            value: tmux_pane,
        },
        EnvVarParam {
            key: "WSLENV".to_string(),
            value: wsl_env,
        },
    ]
}

fn managed_wsl_env_value(extra_keys: &[String]) -> String {
    let required = [
        "AGENTMUX_CONTROL_PIPE",
        "AGENTMUX_CONTROL_TOKEN",
        "AGENTMUX_WORKSPACE_ID",
        "AGENTMUX_SURFACE_ID",
        "AGENTMUX_PANE_ID",
        "CMUX_SOCKET_PATH",
        "CMUX_WORKSPACE_ID",
        "CMUX_SURFACE_ID",
        "CMUX_PANE_ID",
        "TMUX",
        "TMUX_PANE",
    ];
    let mut parts = std::env::var("WSLENV")
        .unwrap_or_default()
        .split(':')
        .filter(|part| !part.trim().is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    for key in required {
        push_wsl_env_key(&mut parts, key);
    }
    for key in extra_keys {
        if let Some(key) = wsl_env_key(key) {
            push_wsl_env_key(&mut parts, &key);
        }
    }
    parts.join(":")
}

fn push_wsl_env_key(parts: &mut Vec<String>, key: &str) {
    if !parts.iter().any(|part| part.split('/').next() == Some(key)) {
        parts.push(key.to_string());
    }
}

fn wsl_env_key(key: &str) -> Option<String> {
    let key = key.trim();
    if key.is_empty() || key == "WSLENV" {
        return None;
    }
    let mut chars = key.chars();
    let first = chars.next()?;
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return None;
    }
    if chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
        Some(key.to_string())
    } else {
        None
    }
}

fn agentmux_pane_to_tmux_pane(pane_id: &str) -> String {
    format!("%{pane_id}")
}

#[derive(Debug)]
pub enum DesktopHostError {
    Control(ControlError),
    Store(StoreError),
    Json(serde_json::Error),
    StateUnavailable(String),
}

impl fmt::Display for DesktopHostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DesktopHostError::Control(error) => {
                write!(f, "{}: {}", error.code.as_str(), error.message)
            }
            DesktopHostError::Store(error) => write!(f, "{error}"),
            DesktopHostError::Json(error) => write!(f, "{error}"),
            DesktopHostError::StateUnavailable(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for DesktopHostError {}

impl From<ControlError> for DesktopHostError {
    fn from(value: ControlError) -> Self {
        DesktopHostError::Control(value)
    }
}

impl From<StoreError> for DesktopHostError {
    fn from(value: StoreError) -> Self {
        DesktopHostError::Store(value)
    }
}

impl From<serde_json::Error> for DesktopHostError {
    fn from(value: serde_json::Error) -> Self {
        DesktopHostError::Json(value)
    }
}

pub fn default_store_path() -> Result<PathBuf, DesktopHostError> {
    if let Some(path) = std::env::var_os("AGENTMUX_STORE_PATH") {
        return Ok(PathBuf::from(path));
    }

    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| {
            DesktopHostError::StateUnavailable("unable to resolve desktop store path".to_string())
        })?;
    let directory = base.join("AgentMux");
    std::fs::create_dir_all(&directory).map_err(|error| {
        DesktopHostError::StateUnavailable(format!("failed to create store directory: {error}"))
    })?;
    Ok(directory.join("agentmux.sqlite3"))
}

pub fn default_control_token_path() -> Result<PathBuf, DesktopHostError> {
    agentmux_ipc::default_control_token_path().map_err(|error| {
        DesktopHostError::StateUnavailable(format!(
            "failed to resolve AgentMux control token path: {error}"
        ))
    })
}

pub fn default_app_config_path() -> Result<PathBuf, DesktopHostError> {
    if let Some(path) = std::env::var_os("AGENTMUX_CONFIG_PATH") {
        return Ok(PathBuf::from(path));
    }

    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| {
            DesktopHostError::StateUnavailable("unable to resolve AgentMux config path".to_string())
        })?;
    Ok(base.join("AgentMux").join(APP_CONFIG_FILE_NAME))
}

fn default_app_config() -> AppConfigFile {
    AppConfigFile {
        format_version: default_app_config_format_version(),
        appearance: default_app_config_appearance(),
        locale: default_app_config_locale(),
        updates: default_app_config_updates(),
        shortcuts: AppConfigShortcuts::default(),
        actions: AppConfigActions::default(),
        ui: AppConfigUi::default(),
        notifications: AppConfigNotifications::default(),
    }
}

fn default_app_config_format_version() -> String {
    APP_CONFIG_FORMAT_VERSION.to_string()
}

fn default_app_config_appearance() -> AppConfigAppearance {
    AppConfigAppearance {
        theme: "dark".to_string(),
        accent_key: "blue".to_string(),
        font_size: 12.5,
    }
}

fn default_app_config_locale() -> AppConfigLocale {
    AppConfigLocale {
        language: "en".to_string(),
    }
}

fn default_app_config_updates() -> AppConfigUpdates {
    AppConfigUpdates { auto_check: true }
}

enum AppConfigScope {
    Global,
    Project,
}

fn normalize_config_scope(value: Option<&str>) -> Result<AppConfigScope, DesktopHostError> {
    let value = value.unwrap_or("global").trim().to_ascii_lowercase();
    match value.as_str() {
        "" | "global" => Ok(AppConfigScope::Global),
        "project" => Ok(AppConfigScope::Project),
        other => Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            format!("Unsupported config scope '{other}'."),
        ))),
    }
}

fn normalize_app_config_file(config: &mut AppConfigFile) -> Result<(), DesktopHostError> {
    config.appearance.theme = normalize_theme(&config.appearance.theme)?;
    config.appearance.accent_key = normalize_accent_key(&config.appearance.accent_key)?;
    config.appearance.font_size = normalize_font_size(config.appearance.font_size)?;
    config.locale.language = normalize_locale_language(&config.locale.language)?;
    normalize_shortcut_bindings(&mut config.shortcuts.bindings)?;
    normalize_custom_actions(&mut config.actions.custom)?;
    normalize_app_config_ui(&mut config.ui)?;
    normalize_app_config_notifications(&mut config.notifications)?;
    if config.format_version.trim().is_empty() {
        config.format_version = APP_CONFIG_FORMAT_VERSION.to_string();
    }
    Ok(())
}

fn normalize_project_app_config_file(
    config: &mut ProjectAppConfigFile,
) -> Result<(), DesktopHostError> {
    normalize_shortcut_bindings(&mut config.shortcuts.bindings)?;
    normalize_custom_actions(&mut config.actions.custom)?;
    normalize_app_config_ui(&mut config.ui)?;
    normalize_app_config_notifications(&mut config.notifications)?;
    Ok(())
}

fn load_app_config(path: &Path) -> Result<AppConfigFile, DesktopHostError> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let mut config: AppConfigFile = serde_json::from_str(&contents)?;
            normalize_app_config_file(&mut config)?;
            Ok(config)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(default_app_config()),
        Err(error) => Err(DesktopHostError::StateUnavailable(format!(
            "failed to read AgentMux config: {error}"
        ))),
    }
}

fn load_project_app_config(path: &Path) -> Result<Option<ProjectAppConfigFile>, DesktopHostError> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let mut config: ProjectAppConfigFile = serde_json::from_str(&contents)?;
            normalize_project_app_config_file(&mut config)?;
            Ok(Some(config))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(DesktopHostError::StateUnavailable(format!(
            "failed to read AgentMux project config '{}': {error}",
            path.display()
        ))),
    }
}

struct DockConfigCandidate {
    source: &'static str,
    path: PathBuf,
    requires_trust: bool,
}

fn global_agentmux_dock_config_path(app_config_path: &Path) -> PathBuf {
    app_config_path
        .parent()
        .map(|parent| parent.join(DOCK_CONFIG_FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(DOCK_CONFIG_FILE_NAME))
}

fn global_cmux_dock_config_path() -> Option<PathBuf> {
    std::env::var_os("CMUX_DOCK_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .map(|home| {
                    home.join(".config")
                        .join("cmux")
                        .join(DOCK_CONFIG_FILE_NAME)
                })
        })
}

fn load_dock_config_file_with_hash(
    path: &Path,
) -> Result<(DockConfigFile, String), DesktopHostError> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let config_hash = dock_config_hash(&contents);
            let config = serde_json::from_str(&contents).map_err(DesktopHostError::from)?;
            Ok((config, config_hash))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Ok((DockConfigFile::default(), dock_config_hash("")))
        }
        Err(error) => Err(DesktopHostError::StateUnavailable(format!(
            "failed to read Dock config '{}': {error}",
            path.display()
        ))),
    }
}

fn dock_config_hash(contents: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in contents.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("fnv1a64:{hash:016x}")
}

fn normalize_dock_config_file(config: &mut DockConfigFile) -> Result<(), DesktopHostError> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for mut control in config.controls.drain(..) {
        control.id = control.id.trim().to_string();
        if control.id.is_empty() || !is_valid_action_reference_id(&control.id) {
            return Err(invalid_dock_config(
                "Dock control id must be non-empty and contain only letters, numbers, '.', '_', or '-'.",
            ));
        }
        if !seen.insert(control.id.clone()) {
            return Err(invalid_dock_config(
                "Dock control ids must be unique within a dock.json file.",
            ));
        }
        control.title = control.title.trim().to_string();
        if control.title.is_empty() {
            return Err(invalid_dock_config("Dock control title cannot be empty."));
        }
        control.command = control.command.trim().to_string();
        if control.command.is_empty() {
            return Err(invalid_dock_config("Dock control command cannot be empty."));
        }
        control.cwd = control
            .cwd
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        if control.height == Some(0) {
            return Err(invalid_dock_config(
                "Dock control height must be a positive integer when provided.",
            ));
        }
        control.env = control
            .env
            .into_iter()
            .map(|(key, value)| (key.trim().to_string(), value))
            .filter(|(key, _)| !key.is_empty())
            .collect();
        normalized.push(control);
    }
    config.controls = normalized;
    Ok(())
}

fn invalid_dock_config(message: &str) -> DesktopHostError {
    DesktopHostError::Control(ControlError::new(ErrorCode::InvalidRequest, message))
}

fn dock_control_result(control: DockControlFile) -> DockControlResult {
    DockControlResult {
        id: control.id,
        title: control.title,
        command: control.command,
        cwd: control.cwd,
        height: control.height,
        env: control.env,
    }
}

fn diagnose_global_app_config(path: &Path) -> AppConfigDiagnosticsEntry {
    let exists = path.exists();
    if !exists {
        return config_diagnostics_entry(
            "global",
            Some(path),
            false,
            true,
            true,
            "Global config is absent; defaults will be used.",
        );
    }

    match load_app_config(path) {
        Ok(_) => config_diagnostics_entry(
            "global",
            Some(path),
            true,
            true,
            true,
            "Global config is valid.",
        ),
        Err(error) => config_diagnostics_entry(
            "global",
            Some(path),
            true,
            false,
            true,
            format!("Global config is invalid: {error}"),
        ),
    }
}

fn diagnose_project_app_config(
    source: &str,
    path: &Path,
    exists: bool,
    active: bool,
    absent_or_override_message: Option<&str>,
) -> AppConfigDiagnosticsEntry {
    if !exists {
        return config_diagnostics_entry(
            source,
            Some(path),
            false,
            true,
            active,
            absent_or_override_message.unwrap_or("Project config is absent."),
        );
    }

    match load_project_app_config(path) {
        Ok(Some(_)) => config_diagnostics_entry(
            source,
            Some(path),
            true,
            true,
            active,
            absent_or_override_message.unwrap_or("Project config is valid."),
        ),
        Ok(None) => config_diagnostics_entry(
            source,
            Some(path),
            false,
            true,
            active,
            absent_or_override_message.unwrap_or("Project config is absent."),
        ),
        Err(error) => config_diagnostics_entry(
            source,
            Some(path),
            true,
            false,
            active,
            format!("Project config is invalid: {error}"),
        ),
    }
}

fn config_diagnostics_entry(
    source: &str,
    path: Option<&Path>,
    exists: bool,
    valid: bool,
    active: bool,
    message: impl Into<String>,
) -> AppConfigDiagnosticsEntry {
    AppConfigDiagnosticsEntry {
        source: source.to_string(),
        path: path.map(|path| path.to_string_lossy().to_string()),
        exists,
        valid,
        active,
        message: message.into(),
    }
}

fn save_app_config(path: &Path, config: &AppConfigFile) -> Result<(), DesktopHostError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            DesktopHostError::StateUnavailable(format!(
                "failed to create AgentMux config directory: {error}"
            ))
        })?;
    }
    let text = serde_json::to_string_pretty(config)?;
    fs::write(path, format!("{text}\n")).map_err(|error| {
        DesktopHostError::StateUnavailable(format!("failed to write AgentMux config: {error}"))
    })
}

fn save_project_app_config(
    path: &Path,
    config: &ProjectAppConfigFile,
) -> Result<(), DesktopHostError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            DesktopHostError::StateUnavailable(format!(
                "failed to create AgentMux project config directory: {error}"
            ))
        })?;
    }
    let text = serde_json::to_string_pretty(config)?;
    fs::write(path, format!("{text}\n")).map_err(|error| {
        DesktopHostError::StateUnavailable(format!(
            "failed to write AgentMux project config '{}': {error}",
            path.display()
        ))
    })
}

fn export_app_config_json(config: &AppConfigResult) -> Result<String, DesktopHostError> {
    let value = serde_json::json!({
        "format_version": config.format_version,
        "appearance": config.appearance,
        "locale": config.locale,
        "updates": config.updates,
        "shortcuts": config.shortcuts,
        "actions": config.actions,
        "ui": config.ui,
        "notifications": config.notifications,
    });
    Ok(serde_json::to_string_pretty(&value)?)
}

fn export_project_app_config_json(
    config: &ProjectAppConfigFile,
) -> Result<String, DesktopHostError> {
    let value = serde_json::json!({
        "shortcuts": config.shortcuts,
        "actions": config.actions,
        "ui": config.ui,
        "notifications": config.notifications,
    });
    Ok(serde_json::to_string_pretty(&value)?)
}

fn app_config_result(
    path: &Path,
    config: AppConfigFile,
    project_config_path: Option<String>,
    project_config_loaded: bool,
) -> AppConfigResult {
    AppConfigResult {
        format_version: config.format_version,
        config_path: path.to_string_lossy().to_string(),
        project_config_path,
        project_config_loaded,
        appearance: config.appearance,
        locale: config.locale,
        updates: config.updates,
        shortcuts: config.shortcuts,
        actions: config.actions,
        ui: config.ui,
        notifications: config.notifications,
    }
}

fn action_list_from_config(config: &AppConfigResult) -> Vec<ActionSummaryResult> {
    let mut actions = builtin_action_results();
    actions.extend(config.actions.custom.iter().map(custom_action_result));
    actions
}

fn builtin_action_results() -> Vec<ActionSummaryResult> {
    vec![
        builtin_action(
            "app.commandPalette",
            "Command palette",
            "view",
            "ui",
            &[],
            &["palette", "commands"],
        ),
        builtin_action(
            "app.commandPalette.legacy",
            "Command palette",
            "view",
            "ui",
            &[],
            &["palette", "commands"],
        ),
        builtin_action(
            "app.search",
            "Search active pane",
            "view",
            "ui",
            &[],
            &["find"],
        ),
        builtin_action(
            "app.settings",
            "Open settings",
            "view",
            "ui",
            &[],
            &["settings"],
        ),
        builtin_action(
            "view.toggleTheme",
            "Toggle theme",
            "view",
            "ui",
            &[],
            &["theme", "dark", "light"],
        ),
        builtin_action(
            "workspace.new",
            "New workspace",
            "workspace",
            "workspace",
            &[],
            &["workspace"],
        ),
        builtin_action(
            "terminal.newWsl",
            "New WSL terminal",
            "terminal",
            "wsl-terminal",
            &[],
            &["terminal", "wsl", "shell"],
        ),
        builtin_action(
            "terminal.openInActivePane",
            "Open WSL terminal in active pane",
            "terminal",
            "wsl-terminal",
            &[],
            &["terminal", "wsl", "pane"],
        ),
        builtin_action(
            "terminal.textBox",
            "Open TextBox composer",
            "terminal",
            "ui",
            &[],
            &["textbox", "prompt", "composer", "send"],
        ),
        builtin_action(
            "pane.splitRight",
            "Split right",
            "terminal",
            "pane",
            &[],
            &["split", "right"],
        ),
        builtin_action(
            "pane.splitDown",
            "Split down",
            "terminal",
            "pane",
            &[],
            &["split", "down"],
        ),
        builtin_action(
            "browser.openNewTab",
            "Open browser in new tab",
            "terminal",
            "browser",
            &[],
            &["browser", "surface", "tab"],
        ),
        builtin_action(
            "browser.openActivePane",
            "Open browser in active pane",
            "terminal",
            "browser",
            &[],
            &["browser", "surface", "pane"],
        ),
        builtin_action(
            "agent.launchClaude",
            "Launch Claude Code",
            "agent",
            "agent",
            &["claude"],
            &["claude", "tmux"],
        ),
        builtin_action(
            "agent.launchCodex",
            "Launch Codex",
            "agent",
            "agent",
            &["codex", "--no-alt-screen"],
            &["codex", "tmux", "--no-alt-screen"],
        ),
        builtin_action(
            "agent.launchCustom",
            "Launch custom agent command",
            "agent",
            "agent",
            &[],
            &["custom", "tmux"],
        ),
    ]
}

fn builtin_action(
    id: &str,
    title: &str,
    group: &str,
    target: &str,
    command: &[&str],
    keywords: &[&str],
) -> ActionSummaryResult {
    ActionSummaryResult {
        id: id.to_string(),
        title: title.to_string(),
        group: group.to_string(),
        source: "builtin".to_string(),
        target: Some(target.to_string()),
        command: command.iter().map(|part| (*part).to_string()).collect(),
        keywords: keywords
            .iter()
            .map(|keyword| (*keyword).to_string())
            .collect(),
    }
}

fn custom_action_result(action: &AppConfigCustomAction) -> ActionSummaryResult {
    let mut keywords = Vec::new();
    keywords.push(action.target.clone());
    keywords.extend(action.command.clone());
    keywords.extend(action.keywords.clone());
    ActionSummaryResult {
        id: action.id.clone(),
        title: action.title.clone(),
        group: custom_action_group(action).to_string(),
        source: "custom".to_string(),
        target: Some(action.target.clone()),
        command: action.command.clone(),
        keywords,
    }
}

fn custom_action_group(action: &AppConfigCustomAction) -> &'static str {
    match action.group.as_deref() {
        Some("agent") => "agent",
        Some("terminal") => "terminal",
        Some("workspace") => "workspace",
        Some("view") => "view",
        Some("remote") => "remote",
        _ if action.target == "agent" => "agent",
        _ if action.target == "browser" => "terminal",
        _ => "remote",
    }
}

fn action_workspace_result(action_id: &str, workspace_id: String) -> ActionRunResult {
    ActionRunResult {
        action_id: action_id.to_string(),
        workspace_id: Some(workspace_id),
        result_type: "workspace".to_string(),
        session_id: None,
        surface_id: None,
        pane_id: None,
        message: Some("workspace created".to_string()),
    }
}

fn action_session_result(
    action_id: &str,
    workspace_id: String,
    session_id: String,
    message: &str,
) -> ActionRunResult {
    ActionRunResult {
        action_id: action_id.to_string(),
        workspace_id: Some(workspace_id),
        result_type: "session".to_string(),
        session_id: Some(session_id),
        surface_id: None,
        pane_id: None,
        message: Some(message.to_string()),
    }
}

enum BrowserCustomActionRuntime {
    Open {
        url: Option<String>,
        placement: String,
    },
    Screenshot {
        format: String,
        placement: String,
    },
    DomSnapshot {
        placement: String,
        frame_id: Option<String>,
    },
    Evaluate {
        script: String,
        placement: String,
        frame_id: Option<String>,
    },
    Click {
        selector: String,
        placement: String,
        frame_id: Option<String>,
    },
    Type {
        selector: String,
        text: String,
        placement: String,
        frame_id: Option<String>,
    },
    Fill {
        selector: String,
        text: String,
        placement: String,
        frame_id: Option<String>,
    },
    Press {
        selector: String,
        key: String,
        placement: String,
        frame_id: Option<String>,
    },
    Select {
        selector: String,
        values: Vec<String>,
        placement: String,
        frame_id: Option<String>,
    },
    Scroll {
        selector: Option<String>,
        x: i32,
        y: i32,
        placement: String,
        frame_id: Option<String>,
    },
    Hover {
        selector: String,
        placement: String,
        frame_id: Option<String>,
    },
    Check {
        selector: String,
        checked: bool,
        placement: String,
        frame_id: Option<String>,
    },
    Highlight {
        selector: String,
        duration_ms: u64,
        placement: String,
        frame_id: Option<String>,
    },
    Focus {
        selector: String,
        placement: String,
        frame_id: Option<String>,
    },
    Zoom {
        percent: u16,
        placement: String,
    },
    WaitForSelector {
        selector: String,
        placement: String,
        timeout_ms: u64,
        frame_id: Option<String>,
    },
    NavigationControl {
        operation: String,
        placement: String,
    },
}

fn browser_custom_action_runtime(
    command: &[String],
) -> Result<BrowserCustomActionRuntime, DesktopHostError> {
    if command.is_empty() {
        return Ok(BrowserCustomActionRuntime::Open {
            url: None,
            placement: "new_tab".to_string(),
        });
    }
    if command.len() == 3 && command[0] == "open" {
        return Ok(BrowserCustomActionRuntime::Open {
            url: Some(command[1].clone()),
            placement: command[2].clone(),
        });
    }
    match command[0].as_str() {
        "screenshot" if command.len() == 3 => Ok(BrowserCustomActionRuntime::Screenshot {
            format: command[1].clone(),
            placement: command[2].clone(),
        }),
        "dom-snapshot" if (2..=3).contains(&command.len()) => {
            Ok(BrowserCustomActionRuntime::DomSnapshot {
                placement: command[1].clone(),
                frame_id: normalized_browser_action_frame_id(command.get(2)),
            })
        }
        "evaluate" if (3..=4).contains(&command.len()) => {
            Ok(BrowserCustomActionRuntime::Evaluate {
                script: command[1].clone(),
                placement: command[2].clone(),
                frame_id: normalized_browser_action_frame_id(command.get(3)),
            })
        }
        "click" if (3..=4).contains(&command.len()) => Ok(BrowserCustomActionRuntime::Click {
            selector: command[1].clone(),
            placement: command[2].clone(),
            frame_id: normalized_browser_action_frame_id(command.get(3)),
        }),
        "type" if (4..=5).contains(&command.len()) => Ok(BrowserCustomActionRuntime::Type {
            selector: command[1].clone(),
            text: command[2].clone(),
            placement: command[3].clone(),
            frame_id: normalized_browser_action_frame_id(command.get(4)),
        }),
        "fill" if (4..=5).contains(&command.len()) => Ok(BrowserCustomActionRuntime::Fill {
            selector: command[1].clone(),
            text: command[2].clone(),
            placement: command[3].clone(),
            frame_id: normalized_browser_action_frame_id(command.get(4)),
        }),
        "press" if (4..=5).contains(&command.len()) => Ok(BrowserCustomActionRuntime::Press {
            selector: command[1].clone(),
            key: command[2].clone(),
            placement: command[3].clone(),
            frame_id: normalized_browser_action_frame_id(command.get(4)),
        }),
        "select" if command.len() >= 4 => {
            let (placement_index, frame_id) = if command.len() >= 5
                && maybe_browser_action_placement(&command[command.len() - 2]).is_some()
            {
                (
                    command.len() - 2,
                    normalized_browser_action_frame_id(command.last()),
                )
            } else {
                (command.len() - 1, None)
            };
            Ok(BrowserCustomActionRuntime::Select {
                selector: command[1].clone(),
                values: command[2..placement_index].to_vec(),
                placement: command[placement_index].clone(),
                frame_id,
            })
        }
        "scroll" if (5..=6).contains(&command.len()) => Ok(BrowserCustomActionRuntime::Scroll {
            selector: if command[1].trim().is_empty() {
                None
            } else {
                Some(command[1].clone())
            },
            x: command[2].parse::<i32>().map_err(|_| {
                DesktopHostError::Control(ControlError::new(
                    ErrorCode::InvalidRequest,
                    "Browser scroll x must be an integer.",
                ))
            })?,
            y: command[3].parse::<i32>().map_err(|_| {
                DesktopHostError::Control(ControlError::new(
                    ErrorCode::InvalidRequest,
                    "Browser scroll y must be an integer.",
                ))
            })?,
            placement: command[4].clone(),
            frame_id: normalized_browser_action_frame_id(command.get(5)),
        }),
        "hover" if (3..=4).contains(&command.len()) => Ok(BrowserCustomActionRuntime::Hover {
            selector: command[1].clone(),
            placement: command[2].clone(),
            frame_id: normalized_browser_action_frame_id(command.get(3)),
        }),
        "check" if (4..=5).contains(&command.len()) => Ok(BrowserCustomActionRuntime::Check {
            selector: command[1].clone(),
            checked: parse_browser_action_bool(&command[2]).map_err(|_| {
                DesktopHostError::Control(ControlError::new(
                    ErrorCode::InvalidRequest,
                    "Browser check value must be true or false.",
                ))
            })?,
            placement: command[3].clone(),
            frame_id: normalized_browser_action_frame_id(command.get(4)),
        }),
        "highlight" if (4..=5).contains(&command.len()) => {
            Ok(BrowserCustomActionRuntime::Highlight {
                selector: command[1].clone(),
                duration_ms: command[2].parse::<u64>().map_err(|_| {
                    DesktopHostError::Control(ControlError::new(
                        ErrorCode::InvalidRequest,
                        "Browser highlight duration must be a positive integer.",
                    ))
                })?,
                placement: command[3].clone(),
                frame_id: normalized_browser_action_frame_id(command.get(4)),
            })
        }
        "focus" if (3..=4).contains(&command.len()) => Ok(BrowserCustomActionRuntime::Focus {
            selector: command[1].clone(),
            placement: command[2].clone(),
            frame_id: normalized_browser_action_frame_id(command.get(3)),
        }),
        "zoom" if command.len() == 3 => Ok(BrowserCustomActionRuntime::Zoom {
            percent: command[1].parse::<u16>().map_err(|_| {
                DesktopHostError::Control(ControlError::new(
                    ErrorCode::InvalidRequest,
                    "Browser zoom percent must be a positive integer.",
                ))
            })?,
            placement: command[2].clone(),
        }),
        "wait-for-selector" if (4..=5).contains(&command.len()) => {
            Ok(BrowserCustomActionRuntime::WaitForSelector {
                selector: command[1].clone(),
                placement: command[2].clone(),
                timeout_ms: command[3].parse::<u64>().map_err(|_| {
                    DesktopHostError::Control(ControlError::new(
                        ErrorCode::InvalidRequest,
                        "Browser wait-for-selector timeout must be a positive integer.",
                    ))
                })?,
                frame_id: normalized_browser_action_frame_id(command.get(4)),
            })
        }
        "reload" | "back" | "forward" | "current-url" if command.len() == 2 => {
            Ok(BrowserCustomActionRuntime::NavigationControl {
                operation: command[0].clone(),
                placement: command[1].clone(),
            })
        }
        _ => Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            "Browser custom action command is not normalized for runtime execution.",
        ))),
    }
}

fn normalized_browser_action_frame_id(value: Option<&String>) -> Option<String> {
    value
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn browser_action_pane_id(
    workspace: &WorkspaceSummaryResult,
    requested_pane_id: Option<String>,
    placement: &str,
) -> Option<String> {
    if placement == "active_pane" {
        Some(requested_pane_id.unwrap_or_else(|| workspace.active_pane_id.clone()))
    } else {
        None
    }
}

fn browser_action_result(
    action_id: &str,
    workspace: &WorkspaceSummaryResult,
    surface: SurfaceSummaryResult,
    message: &str,
) -> ActionRunResult {
    ActionRunResult {
        action_id: action_id.to_string(),
        workspace_id: Some(workspace.workspace_id.clone()),
        result_type: "browser".to_string(),
        session_id: None,
        surface_id: Some(surface.surface_id),
        pane_id: None,
        message: Some(message.to_string()),
    }
}

fn normalize_theme(value: &str) -> Result<String, DesktopHostError> {
    match value.trim() {
        "dark" => Ok("dark".to_string()),
        "light" => Ok("light".to_string()),
        other => Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            format!("Unsupported theme '{other}'."),
        ))),
    }
}

fn normalize_accent_key(value: &str) -> Result<String, DesktopHostError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            "Accent key cannot be empty.",
        )));
    }
    Ok(value.to_string())
}

fn normalize_font_size(value: f64) -> Result<f64, DesktopHostError> {
    if !value.is_finite() {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            "Font size must be finite.",
        )));
    }
    Ok(value.clamp(11.0, 16.0))
}

fn normalize_locale_language(value: &str) -> Result<String, DesktopHostError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "en" | "en-us" | "en_us" => Ok("en".to_string()),
        "ko" | "ko-kr" | "ko_kr" => Ok("ko".to_string()),
        other => Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            format!("Unsupported locale language '{other}'."),
        ))),
    }
}

fn normalize_shortcut_bindings(
    bindings: &mut std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<(), DesktopHostError> {
    for (action_id, value) in bindings.iter_mut() {
        if action_id.trim().is_empty() {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::InvalidRequest,
                "Shortcut action id cannot be empty.",
            )));
        }
        match value {
            serde_json::Value::Null => {}
            serde_json::Value::String(text) => {
                *text = text.trim().to_string();
            }
            serde_json::Value::Array(parts) if parts.len() == 2 => {
                for part in parts {
                    let Some(text) = part.as_str() else {
                        return Err(invalid_shortcut_binding(action_id));
                    };
                    *part = serde_json::Value::String(text.trim().to_string());
                }
            }
            _ => return Err(invalid_shortcut_binding(action_id)),
        }
    }
    Ok(())
}

fn invalid_shortcut_binding(action_id: &str) -> DesktopHostError {
    DesktopHostError::Control(ControlError::new(
        ErrorCode::InvalidRequest,
        format!(
            "Shortcut binding for '{action_id}' must be a string, a two-item string array, or null."
        ),
    ))
}

fn normalize_app_config_ui(ui: &mut AppConfigUi) -> Result<(), DesktopHostError> {
    ui.workspace_plus_action = normalize_optional_action_reference(
        ui.workspace_plus_action.take(),
        "workspace_plus_action",
    )?;
    ui.surface_tab_plus_action = normalize_optional_action_reference(
        ui.surface_tab_plus_action.take(),
        "surface_tab_plus_action",
    )?;

    if let Some(actions) = &mut ui.surface_tab_actions {
        let mut normalized = Vec::new();
        for action in actions.iter() {
            let action = normalize_action_reference(action, "surface_tab_actions")?;
            if !normalized.contains(&action) {
                normalized.push(action);
            }
        }
        *actions = normalized;
    }

    if let Some(lines) = ui.text_box_max_lines {
        if !(TEXT_BOX_MIN_LINES..=TEXT_BOX_MAX_LINES).contains(&lines) {
            return Err(invalid_text_box_max_lines(lines));
        }
    }
    if let Some(margin) = ui.terminal_inner_margin {
        ui.terminal_inner_margin = Some(normalize_terminal_inner_margin(margin)?);
    }
    if let Some(directory) = ui.terminal_start_directory.take() {
        ui.terminal_start_directory = Some(normalize_terminal_start_directory(&directory)?);
    }
    if let Some(cwd) = ui.terminal_start_custom_cwd.take() {
        ui.terminal_start_custom_cwd = normalize_terminal_start_custom_cwd(cwd);
    }
    if let Some(behavior) = ui.terminal_split_behavior.take() {
        ui.terminal_split_behavior = Some(normalize_terminal_split_behavior(&behavior)?);
    }

    Ok(())
}

fn normalize_app_config_notifications(
    notifications: &mut AppConfigNotifications,
) -> Result<(), DesktopHostError> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();

    for mut action in notifications.actions.drain(..) {
        action.action = normalize_notification_action_reference(&action.action)?;
        action.label = action
            .label
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        action.notification_type = normalize_optional_notification_type(
            action.notification_type.take(),
            "notifications.actions.notification_type",
        )?;
        action.severity = action
            .severity
            .as_deref()
            .map(normalize_sidebar_level)
            .transpose()?;

        let key = (
            action.action.clone(),
            action.notification_type.clone(),
            action.severity.clone(),
        );
        if seen.insert(key) {
            normalized.push(action);
        }
    }

    notifications.actions = normalized;
    Ok(())
}

fn normalize_notification_action_reference(value: &str) -> Result<String, DesktopHostError> {
    let action_id = value.trim();
    if action_id.is_empty() || !is_valid_action_reference_id(action_id) {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            format!(
                "Config notifications.actions.action contains invalid action id '{action_id}'. Action ids may contain only letters, numbers, '.', '_', or '-'."
            ),
        )));
    }
    Ok(action_id.to_string())
}

fn normalize_optional_notification_type(
    value: Option<String>,
    field: &str,
) -> Result<Option<String>, DesktopHostError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let notification_type = value.trim();
    if notification_type.is_empty() {
        return Ok(None);
    }
    if !is_valid_action_reference_id(notification_type) {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            format!(
                "Config {field} contains invalid notification type '{notification_type}'. Notification types may contain only letters, numbers, '.', '_', or '-'."
            ),
        )));
    }
    Ok(Some(notification_type.to_string()))
}

fn normalize_optional_action_reference(
    value: Option<String>,
    field: &str,
) -> Result<Option<String>, DesktopHostError> {
    value
        .as_deref()
        .map(|action_id| normalize_action_reference(action_id, field))
        .transpose()
}

fn normalize_action_reference(value: &str, field: &str) -> Result<String, DesktopHostError> {
    let action_id = value.trim();
    if action_id.is_empty() {
        return Err(invalid_action_reference(field, value));
    }
    if !is_valid_action_reference_id(action_id) {
        return Err(invalid_action_reference(field, action_id));
    }
    Ok(action_id.to_string())
}

fn is_valid_action_reference_id(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn invalid_action_reference(field: &str, value: &str) -> DesktopHostError {
    DesktopHostError::Control(ControlError::new(
        ErrorCode::InvalidRequest,
        format!(
            "Config ui.{field} contains invalid action id '{value}'. Action ids may contain only letters, numbers, '.', '_', or '-'."
        ),
    ))
}

fn invalid_text_box_max_lines(value: u8) -> DesktopHostError {
    DesktopHostError::Control(ControlError::new(
        ErrorCode::InvalidRequest,
        format!(
            "Config ui.text_box_max_lines must be between {TEXT_BOX_MIN_LINES} and {TEXT_BOX_MAX_LINES}; got {value}."
        ),
    ))
}

fn normalize_terminal_inner_margin(value: u8) -> Result<u8, DesktopHostError> {
    if (TERMINAL_INNER_MARGIN_MIN..=TERMINAL_INNER_MARGIN_MAX).contains(&value) {
        Ok(value)
    } else {
        Err(invalid_terminal_inner_margin(value))
    }
}

fn invalid_terminal_inner_margin(value: u8) -> DesktopHostError {
    DesktopHostError::Control(ControlError::new(
        ErrorCode::InvalidRequest,
        format!(
            "Config ui.terminal_inner_margin must be between {TERMINAL_INNER_MARGIN_MIN} and {TERMINAL_INNER_MARGIN_MAX}; got {value}."
        ),
    ))
}

fn normalize_terminal_start_directory(value: &str) -> Result<String, DesktopHostError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "home" => Ok("home".to_string()),
        "workspace" => Ok("workspace".to_string()),
        "custom" => Ok("custom".to_string()),
        other => Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            format!(
                "Config ui.terminal_start_directory must be 'home', 'workspace', or 'custom'; got '{other}'."
            ),
        ))),
    }
}

fn normalize_terminal_split_behavior(value: &str) -> Result<String, DesktopHostError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "clone_current" => Ok("clone_current".to_string()),
        "empty" => Ok("empty".to_string()),
        other => Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            format!(
                "Config ui.terminal_split_behavior must be 'clone_current' or 'empty'; got '{other}'."
            ),
        ))),
    }
}

fn normalize_terminal_start_custom_cwd(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_custom_actions(actions: &mut [AppConfigCustomAction]) -> Result<(), DesktopHostError> {
    let mut seen = HashSet::new();
    for action in actions.iter_mut() {
        action.id = action.id.trim().to_string();
        if !is_valid_custom_action_id(&action.id) {
            return Err(invalid_custom_action(
                &action.id,
                "Custom action id must start with 'custom.' and contain only letters, numbers, '.', '_', or '-'.",
            ));
        }
        if !seen.insert(action.id.clone()) {
            return Err(invalid_custom_action(
                &action.id,
                "Custom action ids must be unique.",
            ));
        }

        action.title = action.title.trim().to_string();
        if action.title.is_empty() {
            return Err(invalid_custom_action(
                &action.id,
                "Custom action title cannot be empty.",
            ));
        }

        action.group = action
            .group
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        if let Some(group) = action.group.as_deref() {
            if !matches!(
                group,
                "agent" | "terminal" | "workspace" | "view" | "remote"
            ) {
                return Err(invalid_custom_action(
                    &action.id,
                    "Custom action group must be agent, terminal, workspace, view, or remote.",
                ));
            }
        }

        action.target = action.target.trim().to_ascii_lowercase();
        if !matches!(action.target.as_str(), "agent" | "wsl-terminal" | "browser") {
            return Err(invalid_custom_action(
                &action.id,
                "Custom action target must be agent, wsl-terminal, or browser.",
            ));
        }

        action.command = action
            .command
            .iter()
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty())
            .collect();
        if action.target == "agent" && action.command.is_empty() {
            return Err(invalid_custom_action(
                &action.id,
                "Agent custom actions require a non-empty command array.",
            ));
        }
        if action.target == "browser" {
            action.command = normalize_browser_custom_action_command(&action.id, &action.command)?;
        } else if action.target != "agent" && !action.command.is_empty() {
            return Err(invalid_custom_action(
                &action.id,
                "Only agent and browser custom actions may define a command array.",
            ));
        }

        action.keywords = action
            .keywords
            .iter()
            .map(|keyword| keyword.trim().to_string())
            .filter(|keyword| !keyword.is_empty())
            .collect();
    }
    Ok(())
}

fn normalize_browser_custom_action_command(
    action_id: &str,
    command: &[String],
) -> Result<Vec<String>, DesktopHostError> {
    if command.is_empty() {
        return Ok(Vec::new());
    }
    let operation = command[0].trim().to_ascii_lowercase();
    let (url, placement) = match operation.as_str() {
        "open" | "navigate" => {
            if !(2..=3).contains(&command.len()) {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser custom actions use ['open', url, optional placement].",
                ));
            }
            let placement = command
                .get(2)
                .map(|value| normalize_browser_action_placement(action_id, value))
                .transpose()?
                .unwrap_or_else(|| "new_tab".to_string());
            (command[1].trim().to_string(), placement)
        }
        "new-tab" | "new_tab" => {
            if command.len() != 2 {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser new-tab custom actions use ['new-tab', url].",
                ));
            }
            (command[1].trim().to_string(), "new_tab".to_string())
        }
        "active-pane" | "active_pane" => {
            if command.len() != 2 {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser active-pane custom actions use ['active-pane', url].",
                ));
            }
            (command[1].trim().to_string(), "active_pane".to_string())
        }
        _ => {
            return normalize_browser_automation_action_command(
                action_id,
                operation.as_str(),
                command,
            );
        }
    };

    if url.is_empty() {
        return Err(invalid_custom_action(
            action_id,
            "Browser custom action URL cannot be empty.",
        ));
    }

    Ok(vec!["open".to_string(), url, placement])
}

fn normalize_browser_automation_action_command(
    action_id: &str,
    operation: &str,
    command: &[String],
) -> Result<Vec<String>, DesktopHostError> {
    let (command, frame_id) = extract_browser_action_frame_id(action_id, operation, command)?;
    let command = command.as_slice();
    let has_frame_id = frame_id.is_some();
    let frame_id = frame_id.unwrap_or_default();
    match operation {
        "screenshot" => {
            if has_frame_id {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser screenshot custom actions do not accept a frame target.",
                ));
            }
            if command.len() > 3 {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser screenshot custom actions use ['screenshot', optional format, optional placement].",
                ));
            }
            let mut format = "png".to_string();
            let mut placement = "active_pane".to_string();
            if let Some(first) = command.get(1) {
                if let Some(value) = maybe_browser_action_placement(first) {
                    placement = value;
                } else {
                    format = first.trim().to_string();
                }
            }
            if let Some(second) = command.get(2) {
                placement = normalize_browser_action_placement(action_id, second)?;
            }
            Ok(vec!["screenshot".to_string(), format, placement])
        }
        "dom-snapshot" | "dom_snapshot" => {
            if command.len() > 2 {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser DOM snapshot custom actions use ['dom-snapshot', optional placement].",
                ));
            }
            let placement = command
                .get(1)
                .map(|value| normalize_browser_action_placement(action_id, value))
                .transpose()?
                .unwrap_or_else(|| "active_pane".to_string());
            Ok(vec!["dom-snapshot".to_string(), placement, frame_id])
        }
        "evaluate" => {
            if !(2..=3).contains(&command.len()) {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser evaluate custom actions use ['evaluate', script, optional placement].",
                ));
            }
            let placement = command
                .get(2)
                .map(|value| normalize_browser_action_placement(action_id, value))
                .transpose()?
                .unwrap_or_else(|| "active_pane".to_string());
            Ok(vec![
                "evaluate".to_string(),
                command[1].trim().to_string(),
                placement,
                frame_id,
            ])
        }
        "click" => {
            if !(2..=3).contains(&command.len()) {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser click custom actions use ['click', selector, optional placement].",
                ));
            }
            let placement = command
                .get(2)
                .map(|value| normalize_browser_action_placement(action_id, value))
                .transpose()?
                .unwrap_or_else(|| "active_pane".to_string());
            Ok(vec![
                "click".to_string(),
                command[1].trim().to_string(),
                placement,
                frame_id,
            ])
        }
        "type" => {
            if !(3..=4).contains(&command.len()) {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser type custom actions use ['type', selector, text, optional placement].",
                ));
            }
            let placement = command
                .get(3)
                .map(|value| normalize_browser_action_placement(action_id, value))
                .transpose()?
                .unwrap_or_else(|| "active_pane".to_string());
            Ok(vec![
                "type".to_string(),
                command[1].trim().to_string(),
                command[2].trim().to_string(),
                placement,
                frame_id,
            ])
        }
        "fill" => {
            if !(3..=4).contains(&command.len()) {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser fill custom actions use ['fill', selector, text, optional placement].",
                ));
            }
            let placement = command
                .get(3)
                .map(|value| normalize_browser_action_placement(action_id, value))
                .transpose()?
                .unwrap_or_else(|| "active_pane".to_string());
            Ok(vec![
                "fill".to_string(),
                command[1].trim().to_string(),
                command[2].trim().to_string(),
                placement,
                frame_id,
            ])
        }
        "press" => {
            if !(3..=4).contains(&command.len()) {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser press custom actions use ['press', selector, key, optional placement].",
                ));
            }
            let placement = command
                .get(3)
                .map(|value| normalize_browser_action_placement(action_id, value))
                .transpose()?
                .unwrap_or_else(|| "active_pane".to_string());
            Ok(vec![
                "press".to_string(),
                command[1].trim().to_string(),
                command[2].trim().to_string(),
                placement,
                frame_id,
            ])
        }
        "select" => {
            if command.len() < 3 {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser select custom actions use ['select', selector, value..., optional placement].",
                ));
            }
            let last_index = command.len() - 1;
            let placement_from_last = maybe_browser_action_placement(&command[last_index]);
            let (value_end, placement) = if let Some(placement) = placement_from_last {
                (last_index, placement)
            } else {
                (command.len(), "active_pane".to_string())
            };
            if value_end <= 2 {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser select custom actions require at least one value.",
                ));
            }
            let mut normalized = vec![
                "select".to_string(),
                command[1].trim().to_string(),
            ];
            normalized.extend(command[2..value_end].iter().map(|value| value.trim().to_string()));
            normalized.push(placement);
            normalized.push(frame_id);
            Ok(normalized)
        }
        "scroll" => {
            if !(2..=5).contains(&command.len()) {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser scroll custom actions use ['scroll', y], ['scroll', x, y], or ['scroll', selector, x, y], each with optional placement.",
                ));
            }
            let last_index = command.len() - 1;
            let placement_from_last = maybe_browser_action_placement(&command[last_index]);
            let (arg_end, placement) = if let Some(placement) = placement_from_last {
                (last_index, placement)
            } else {
                (command.len(), "active_pane".to_string())
            };
            let args = &command[1..arg_end];
            let (selector, x, y) = match args.len() {
                1 => (String::new(), 0, parse_browser_action_i32(&args[0]).map_err(|_| {
                    invalid_custom_action(action_id, "Browser scroll y must be an integer.")
                })?),
                2 => (
                    String::new(),
                    parse_browser_action_i32(&args[0]).map_err(|_| {
                        invalid_custom_action(action_id, "Browser scroll x must be an integer.")
                    })?,
                    parse_browser_action_i32(&args[1]).map_err(|_| {
                        invalid_custom_action(action_id, "Browser scroll y must be an integer.")
                    })?,
                ),
                3 => (
                    args[0].trim().to_string(),
                    parse_browser_action_i32(&args[1]).map_err(|_| {
                        invalid_custom_action(action_id, "Browser scroll x must be an integer.")
                    })?,
                    parse_browser_action_i32(&args[2]).map_err(|_| {
                        invalid_custom_action(action_id, "Browser scroll y must be an integer.")
                    })?,
                ),
                _ => {
                    return Err(invalid_custom_action(
                        action_id,
                        "Browser scroll custom actions use ['scroll', y], ['scroll', x, y], or ['scroll', selector, x, y], each with optional placement.",
                    ))
                }
            };
            Ok(vec![
                "scroll".to_string(),
                selector,
                x.to_string(),
                y.to_string(),
                placement,
                frame_id,
            ])
        }
        "hover" => {
            if !(2..=3).contains(&command.len()) {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser hover custom actions use ['hover', selector, optional placement].",
                ));
            }
            let placement = command
                .get(2)
                .map(|value| normalize_browser_action_placement(action_id, value))
                .transpose()?
                .unwrap_or_else(|| "active_pane".to_string());
            Ok(vec![
                "hover".to_string(),
                command[1].trim().to_string(),
                placement,
                frame_id,
            ])
        }
        "check" | "uncheck" => {
            if !(2..=4).contains(&command.len()) {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser check custom actions use ['check', selector, optional checked, optional placement].",
                ));
            }
            let third_arg_is_placement = command
                .get(2)
                .and_then(|value| maybe_browser_action_placement(value))
                .is_some();
            let checked = if operation == "uncheck" {
                false
            } else if third_arg_is_placement {
                true
            } else {
                command
                    .get(2)
                    .map(|value| {
                        parse_browser_action_bool(value).map_err(|_| {
                            invalid_custom_action(
                                action_id,
                                "Browser check value must be true or false.",
                            )
                        })
                    })
                    .transpose()?
                    .unwrap_or(true)
            };
            let placement_source = if third_arg_is_placement {
                command.get(2)
            } else {
                command.get(3)
            };
            let placement = placement_source
                .map(|value| normalize_browser_action_placement(action_id, value))
                .transpose()?
                .unwrap_or_else(|| "active_pane".to_string());
            Ok(vec![
                "check".to_string(),
                command[1].trim().to_string(),
                checked.to_string(),
                placement,
                frame_id,
            ])
        }
        "highlight" => {
            if !(2..=4).contains(&command.len()) {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser highlight custom actions use ['highlight', selector, optional durationMs, optional placement].",
                ));
            }
            let third_arg_is_duration = command
                .get(2)
                .map(|value| is_browser_action_timeout_token(value))
                .unwrap_or(false);
            let placement_source = if third_arg_is_duration {
                command.get(3)
            } else {
                command.get(2)
            };
            let placement = placement_source
                .map(|value| normalize_browser_action_placement(action_id, value))
                .transpose()?
                .unwrap_or_else(|| "active_pane".to_string());
            let duration_source = if third_arg_is_duration {
                command.get(2)
            } else {
                command.get(3)
            };
            let duration_ms = duration_source
                .map(|value| {
                    parse_browser_action_timeout(value).map_err(|_| {
                        invalid_custom_action(
                            action_id,
                            "Browser highlight duration must be a positive integer.",
                        )
                    })
                })
                .transpose()?
                .unwrap_or(1200);
            Ok(vec![
                "highlight".to_string(),
                command[1].trim().to_string(),
                duration_ms.to_string(),
                placement,
                frame_id,
            ])
        }
        "focus" => {
            if !(2..=3).contains(&command.len()) {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser focus custom actions use ['focus', selector, optional placement].",
                ));
            }
            let placement = command
                .get(2)
                .map(|value| normalize_browser_action_placement(action_id, value))
                .transpose()?
                .unwrap_or_else(|| "active_pane".to_string());
            Ok(vec![
                "focus".to_string(),
                command[1].trim().to_string(),
                placement,
                frame_id,
            ])
        }
        "zoom" => {
            if has_frame_id {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser zoom custom actions do not accept a frame target.",
                ));
            }
            if !(2..=3).contains(&command.len()) {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser zoom custom actions use ['zoom', percent, optional placement].",
                ));
            }
            let percent = command[1].trim().parse::<u16>().map_err(|_| {
                invalid_custom_action(action_id, "Browser zoom percent must be a positive integer.")
            })?;
            if !(25..=500).contains(&percent) {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser zoom percent must be between 25 and 500.",
                ));
            }
            let placement = command
                .get(2)
                .map(|value| normalize_browser_action_placement(action_id, value))
                .transpose()?
                .unwrap_or_else(|| "active_pane".to_string());
            Ok(vec!["zoom".to_string(), percent.to_string(), placement])
        }
        "reload" | "refresh" | "back" | "go-back" | "go_back" | "forward" | "go-forward"
        | "go_forward" | "current-url" | "current_url" | "url" => {
            if has_frame_id {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser navigation custom actions do not accept a frame target.",
                ));
            }
            if command.len() > 2 {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser navigation custom actions use ['reload'|'back'|'forward'|'current-url', optional placement].",
                ));
            }
            let placement = command
                .get(1)
                .map(|value| normalize_browser_action_placement(action_id, value))
                .transpose()?
                .unwrap_or_else(|| "active_pane".to_string());
            Ok(vec![
                normalize_browser_navigation_operation(command[0].as_str()).to_string(),
                placement,
            ])
        }
        "wait" | "wait-for-selector" | "wait_for_selector" => {
            if !(2..=4).contains(&command.len()) {
                return Err(invalid_custom_action(
                    action_id,
                    "Browser wait-for-selector custom actions use ['wait-for-selector', selector, optional placement, optional timeoutMs].",
                ));
            }
            let third_arg_is_timeout = command
                .get(2)
                .map(|value| is_browser_action_timeout_token(value))
                .unwrap_or(false);
            let placement = if third_arg_is_timeout {
                "active_pane".to_string()
            } else {
                command
                    .get(2)
                    .map(|value| normalize_browser_action_placement(action_id, value))
                    .transpose()?
                    .unwrap_or_else(|| "active_pane".to_string())
            };
            let timeout_source = if third_arg_is_timeout {
                command.get(2)
            } else {
                command.get(3)
            };
            let timeout_ms = timeout_source
                .map(|value| {
                    parse_browser_action_timeout(value).map_err(|_| {
                        invalid_custom_action(
                            action_id,
                            "Browser wait-for-selector timeout must be a positive integer.",
                        )
                    })
                })
                .transpose()?
                .unwrap_or(5000);
            Ok(vec![
                "wait-for-selector".to_string(),
                command[1].trim().to_string(),
                placement,
                timeout_ms.to_string(),
                frame_id,
            ])
        }
        _ => Err(invalid_custom_action(
            action_id,
            "Browser custom action command must start with open, navigate, new-tab, active-pane, screenshot, dom-snapshot, evaluate, click, type, fill, press, select, scroll, hover, check, highlight, focus, zoom, wait-for-selector, reload, back, forward, or current-url.",
        )),
    }
}

fn extract_browser_action_frame_id(
    action_id: &str,
    operation: &str,
    command: &[String],
) -> Result<(Vec<String>, Option<String>), DesktopHostError> {
    let mut normalized = Vec::with_capacity(command.len());
    let mut frame_id = None;
    for (index, part) in command.iter().enumerate() {
        if index > 0 {
            if let Some(value) = browser_action_frame_id_token(part) {
                if value.trim().is_empty() {
                    return Err(invalid_custom_action(
                        action_id,
                        "Browser frame target cannot be empty.",
                    ));
                }
                if frame_id.replace(value).is_some() {
                    return Err(invalid_custom_action(
                        action_id,
                        "Browser custom actions accept at most one frame target.",
                    ));
                }
                continue;
            }
        }
        normalized.push(part.clone());
    }
    if frame_id.is_none() && browser_action_has_normalized_frame_slot(operation, &normalized) {
        if let Some(value) = normalized.pop() {
            let value = value.trim();
            if !value.is_empty() {
                frame_id = Some(value.to_string());
            }
        }
    }
    Ok((normalized, frame_id))
}

fn browser_action_has_normalized_frame_slot(operation: &str, command: &[String]) -> bool {
    match operation {
        "dom-snapshot" | "dom_snapshot" => {
            command.len() == 3 && maybe_browser_action_placement(&command[1]).is_some()
        }
        "evaluate" | "click" => {
            command.len() == 4 && maybe_browser_action_placement(&command[2]).is_some()
        }
        "type" | "fill" | "press" | "check" | "highlight" => {
            command.len() == 5 && maybe_browser_action_placement(&command[3]).is_some()
        }
        "select" => {
            command.len() >= 5
                && maybe_browser_action_placement(&command[command.len() - 2]).is_some()
        }
        "scroll" => command.len() == 6 && maybe_browser_action_placement(&command[4]).is_some(),
        "hover" | "focus" => {
            command.len() == 4 && maybe_browser_action_placement(&command[2]).is_some()
        }
        "wait" | "wait-for-selector" | "wait_for_selector" => {
            command.len() == 5
                && maybe_browser_action_placement(&command[2]).is_some()
                && is_browser_action_timeout_token(&command[3])
        }
        _ => false,
    }
}

fn browser_action_frame_id_token(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    for prefix in [
        "frame:",
        "frame=",
        "frame-id:",
        "frame-id=",
        "frame_id:",
        "frame_id=",
    ] {
        if lower.starts_with(prefix) {
            return Some(trimmed[prefix.len()..].trim().to_string());
        }
    }
    None
}

fn normalize_browser_navigation_operation(operation: &str) -> &'static str {
    match operation {
        "back" | "go-back" | "go_back" => "back",
        "forward" | "go-forward" | "go_forward" => "forward",
        "current-url" | "current_url" | "url" => "current-url",
        _ => "reload",
    }
}

fn parse_browser_action_timeout(value: &str) -> Result<u64, ()> {
    let parsed = value.trim().parse::<u64>().map_err(|_| ())?;
    if parsed == 0 {
        return Err(());
    }
    Ok(parsed)
}

fn parse_browser_action_i32(value: &str) -> Result<i32, ()> {
    value.trim().parse::<i32>().map_err(|_| ())
}

fn parse_browser_action_bool(value: &str) -> Result<bool, ()> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" | "checked" => Ok(true),
        "false" | "0" | "no" | "off" | "unchecked" => Ok(false),
        _ => Err(()),
    }
}

fn is_browser_action_timeout_token(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.chars().all(|candidate| candidate.is_ascii_digit())
}

fn maybe_browser_action_placement(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "new-tab" | "new_tab" => Some("new_tab".to_string()),
        "active-pane" | "active_pane" => Some("active_pane".to_string()),
        _ => None,
    }
}

fn normalize_browser_action_placement(
    action_id: &str,
    value: &str,
) -> Result<String, DesktopHostError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "new-tab" | "new_tab" => Ok("new_tab".to_string()),
        "active-pane" | "active_pane" => Ok("active_pane".to_string()),
        other => Err(invalid_custom_action(
            action_id,
            &format!("Unsupported browser placement '{other}'."),
        )),
    }
}

fn is_valid_custom_action_id(value: &str) -> bool {
    value.starts_with("custom.")
        && value.len() > "custom.".len()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn invalid_custom_action(action_id: &str, message: &str) -> DesktopHostError {
    DesktopHostError::Control(ControlError::new(
        ErrorCode::InvalidRequest,
        format!("Invalid custom action '{action_id}': {message}"),
    ))
}

fn merge_custom_actions(
    actions: &mut Vec<AppConfigCustomAction>,
    overrides: Vec<AppConfigCustomAction>,
) {
    for override_action in overrides {
        if let Some(index) = actions
            .iter()
            .position(|candidate| candidate.id == override_action.id)
        {
            actions[index] = override_action;
        } else {
            actions.push(override_action);
        }
    }
}

fn merge_app_config_ui(ui: &mut AppConfigUi, overrides: AppConfigUi) {
    if overrides.workspace_plus_action.is_some() {
        ui.workspace_plus_action = overrides.workspace_plus_action;
    }
    if overrides.surface_tab_plus_action.is_some() {
        ui.surface_tab_plus_action = overrides.surface_tab_plus_action;
    }
    if overrides.surface_tab_actions.is_some() {
        ui.surface_tab_actions = overrides.surface_tab_actions;
    }
    if overrides.text_box_max_lines.is_some() {
        ui.text_box_max_lines = overrides.text_box_max_lines;
    }
    if overrides.terminal_inner_margin.is_some() {
        ui.terminal_inner_margin = overrides.terminal_inner_margin;
    }
    if overrides.terminal_start_directory.is_some() {
        ui.terminal_start_directory = overrides.terminal_start_directory;
    }
    if overrides.terminal_start_custom_cwd.is_some() {
        ui.terminal_start_custom_cwd = overrides.terminal_start_custom_cwd;
    }
    if overrides.terminal_split_behavior.is_some() {
        ui.terminal_split_behavior = overrides.terminal_split_behavior;
    }
}

fn merge_app_config_notifications(
    notifications: &mut AppConfigNotifications,
    overrides: AppConfigNotifications,
) {
    for override_action in overrides.actions {
        if let Some(index) = notifications.actions.iter().position(|candidate| {
            candidate.action == override_action.action
                && candidate.notification_type == override_action.notification_type
                && candidate.severity == override_action.severity
        }) {
            notifications.actions[index] = override_action;
        } else {
            notifications.actions.push(override_action);
        }
    }
}

fn unique_temp_config_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("agentmux-config-{nanos}.json"))
}

pub fn load_or_create_control_token(path: impl AsRef<Path>) -> Result<String, DesktopHostError> {
    let path = path.as_ref();
    match agentmux_ipc::read_control_token(path) {
        Ok(token) => return Ok(token),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(DesktopHostError::StateUnavailable(format!(
                "failed to read AgentMux control token: {error}"
            )))
        }
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            DesktopHostError::StateUnavailable(format!(
                "failed to create AgentMux config directory: {error}"
            ))
        })?;
    }

    let token = generate_control_token()?;
    match create_control_token_file(path, &token) {
        Ok(()) => Ok(token),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            agentmux_ipc::read_control_token(path).map_err(|error| {
                DesktopHostError::StateUnavailable(format!(
                    "failed to read AgentMux control token: {error}"
                ))
            })
        }
        Err(error) => Err(DesktopHostError::StateUnavailable(format!(
            "failed to create AgentMux control token: {error}"
        ))),
    }
}

#[cfg(windows)]
fn create_control_token_file(path: &Path, token: &str) -> std::io::Result<()> {
    windows_token_file::create(path, token)
}

#[cfg(not(windows))]
fn create_control_token_file(path: &Path, token: &str) -> std::io::Result<()> {
    use std::io::Write;

    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    writeln!(file, "{token}")?;
    Ok(())
}

#[cfg(windows)]
mod windows_token_file {
    use std::ffi::OsStr;
    use std::fs::File;
    use std::io::{self, Write};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use std::path::Path;
    use std::ptr::null_mut;

    #[cfg(test)]
    use windows_sys::core::PWSTR;
    #[cfg(test)]
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::Foundation::{LocalFree, GENERIC_WRITE, HLOCAL, INVALID_HANDLE_VALUE};
    #[cfg(test)]
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    #[cfg(test)]
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;
    use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL};

    const TOKEN_FILE_SDDL: &str = "D:P(A;;FA;;;OW)";

    pub fn create(path: &Path, token: &str) -> io::Result<()> {
        let descriptor = SecurityDescriptor::from_sddl(TOKEN_FILE_SDDL)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.as_ptr(),
            bInheritHandle: 0,
        };
        let path = wide_path(path);
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_WRITE,
                0,
                &attributes,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        let mut file = unsafe { File::from_raw_handle(handle as RawHandle) };
        writeln!(file, "{token}")?;
        file.flush()
    }

    #[cfg(test)]
    pub fn file_dacl_sddl(path: &Path) -> io::Result<String> {
        let path = wide_path(path);
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        let result = unsafe {
            GetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut descriptor,
            )
        };
        if result != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(result as i32));
        }
        let descriptor = SecurityDescriptor { ptr: descriptor };

        let mut sddl: PWSTR = null_mut();
        let ok = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor.as_ptr(),
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut sddl,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let sddl = LocalWideString { ptr: sddl };
        sddl.to_string()
    }

    struct SecurityDescriptor {
        ptr: PSECURITY_DESCRIPTOR,
    }

    impl SecurityDescriptor {
        fn from_sddl(sddl: &str) -> io::Result<Self> {
            let mut ptr = null_mut();
            let sddl = wide_null(sddl);
            let ok = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut ptr,
                    null_mut(),
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self { ptr })
        }

        fn as_ptr(&self) -> PSECURITY_DESCRIPTOR {
            self.ptr
        }
    }

    impl Drop for SecurityDescriptor {
        fn drop(&mut self) {
            if !self.ptr.is_null() {
                free_local(self.ptr as HLOCAL);
            }
        }
    }

    #[cfg(test)]
    struct LocalWideString {
        ptr: PWSTR,
    }

    #[cfg(test)]
    impl LocalWideString {
        fn to_string(&self) -> io::Result<String> {
            if self.ptr.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "empty security descriptor string",
                ));
            }

            let mut len = 0;
            unsafe {
                while *self.ptr.add(len) != 0 {
                    len += 1;
                }
                String::from_utf16(std::slice::from_raw_parts(self.ptr, len))
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
            }
        }
    }

    #[cfg(test)]
    impl Drop for LocalWideString {
        fn drop(&mut self) {
            if !self.ptr.is_null() {
                free_local(self.ptr as HLOCAL);
            }
        }
    }

    fn free_local(value: HLOCAL) {
        unsafe {
            LocalFree(value);
        }
    }

    fn wide_path(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    fn wide_null(value: &str) -> Vec<u16> {
        OsStr::new(value).encode_wide().chain(Some(0)).collect()
    }
}

fn generate_control_token() -> Result<String, DesktopHostError> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|error| {
        DesktopHostError::StateUnavailable(format!(
            "failed to generate AgentMux control token: {error}"
        ))
    })?;
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut token, "{byte:02x}");
    }
    Ok(token)
}

fn session_summary(
    control: &mut DesktopRuntimeControl,
    session_id: &str,
    token: &str,
) -> Result<SessionSummaryResult, DesktopHostError> {
    try_session_summary(control, session_id, token)?.ok_or_else(|| {
        DesktopHostError::Control(ControlError::new(
            ErrorCode::SessionNotFound,
            "Session not found.",
        ))
    })
}

fn try_session_summary(
    control: &mut DesktopRuntimeControl,
    session_id: &str,
    token: &str,
) -> Result<Option<SessionSummaryResult>, DesktopHostError> {
    let params_json = format!(r#"{{"session_id":"{session_id}"}}"#);
    let response = control.handle_request(RequestEnvelope::new(
        "desktop_persist_session_get",
        "session.get",
        params_json,
        token,
    ));
    match &response.outcome {
        ResponseOutcome::Ok { .. } => response_result_json(&response).map(Some),
        ResponseOutcome::Error(error) if error.code == ErrorCode::SessionNotFound => Ok(None),
        ResponseOutcome::Error(error) => Err(error.clone().into()),
    }
}

fn response_result_json<T>(response: &ResponseEnvelope) -> Result<T, DesktopHostError>
where
    T: serde::de::DeserializeOwned,
{
    match &response.outcome {
        ResponseOutcome::Ok { result_json } => {
            serde_json::from_str(result_json).map_err(DesktopHostError::from)
        }
        ResponseOutcome::Error(error) => Err(error.clone().into()),
    }
}

fn discover_wsl_distributions() -> Result<Vec<WslDistribution>, DesktopHostError> {
    discover_wsl_distributions_from_backend().map_err(|diagnostic| {
        let code = match diagnostic.code {
            WslDiagnosticCode::WslUnavailable | WslDiagnosticCode::NoDistributions => {
                ErrorCode::BackendUnavailable
            }
            WslDiagnosticCode::MissingDistribution | WslDiagnosticCode::InvalidCwd => {
                ErrorCode::InvalidRequest
            }
            WslDiagnosticCode::LaunchTimeout => ErrorCode::Timeout,
        };
        DesktopHostError::Control(ControlError::new(code, diagnostic.message))
    })
}

fn tmux_diagnostics(distribution: Option<&str>) -> TmuxDiagnosticsResult {
    let mut command = Command::new("wsl.exe");
    hide_console_window(&mut command);
    if let Some(distribution) = distribution.filter(|value| !value.trim().is_empty()) {
        command.arg("--distribution").arg(distribution);
    }
    command.args([
        "--exec",
        "sh",
        "-lc",
        "command -v tmux >/dev/null 2>&1 && tmux -V",
    ]);

    match command.output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string();
            TmuxDiagnosticsResult {
                available: true,
                distribution: distribution.map(str::to_string),
                version: if version.is_empty() {
                    None
                } else {
                    Some(version.clone())
                },
                message: if version.is_empty() {
                    "tmux is available in WSL.".to_string()
                } else {
                    format!("tmux is available in WSL: {version}")
                },
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            TmuxDiagnosticsResult {
                available: false,
                distribution: distribution.map(str::to_string),
                version: None,
                message: if stderr.is_empty() {
                    "tmux was not found in the selected WSL distribution. Install it with `sudo apt update && sudo apt install -y tmux`.".to_string()
                } else {
                    format!(
                        "tmux was not found in the selected WSL distribution. Install it with `sudo apt update && sudo apt install -y tmux`. ({stderr})"
                    )
                },
            }
        }
        Err(error) => TmuxDiagnosticsResult {
            available: false,
            distribution: distribution.map(str::to_string),
            version: None,
            message: format!(
                "Could not check tmux through wsl.exe. Install WSL first, then install tmux with `sudo apt update && sudo apt install -y tmux`. ({error})"
            ),
        },
    }
}

fn tmux_session_exists(distribution: Option<&str>, session_name: &str) -> Option<bool> {
    if session_name.trim().is_empty() {
        return Some(false);
    }
    let mut command = Command::new("wsl.exe");
    hide_console_window(&mut command);
    if let Some(distribution) = distribution.filter(|value| !value.trim().is_empty()) {
        command.arg("--distribution").arg(distribution);
    }
    let target = posix_shell_quote(session_name.to_string());
    command.args([
        "--exec",
        "sh",
        "-lc",
        &format!("tmux has-session -t {target} >/dev/null 2>&1"),
    ]);
    command_status_success_with_timeout(
        command,
        Duration::from_millis(TMUX_SESSION_EXISTS_TIMEOUT_MS),
    )
}

fn command_status_success_with_timeout(mut command: Command, timeout: Duration) -> Option<bool> {
    let mut child = command.spawn().ok()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.success()),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

/// Keep the WSL2 VM warm so terminals open fast.
///
/// Measured: a freshly-booted WSL2 VM costs ~5s on the first `wsl.exe` launch,
/// during which a spawned shell emits nothing and the pane looks blank; once the
/// VM is resident, subsequent shells start in ~0.35s. WSL2 also auto-shuts the
/// VM down after a short idle, so without an anchor the 5s cost recurs after
/// every lull — which is what makes terminals intermittently open blank.
///
/// This holds one blocking no-op (`cat`) open in the default distribution, which
/// keeps the VM resident. Its stdin is an OS pipe held by this process: if
/// AgentMux exits or crashes, the pipe closes, `cat` reaches EOF and exits, and
/// the VM is free to idle out — so we never leak a pinned VM. If the VM is shut
/// down out from under us (e.g. `wsl --shutdown`), the anchor exits and we
/// re-warm after a short backoff.
///
/// Best-effort and side-effect-only: it spawns nothing the user interacts with.
/// Opt out with `AGENTMUX_DISABLE_WSL_PREWARM`. Loops forever; run on its own
/// thread.
#[cfg(windows)]
pub fn run_wsl_prewarm_keepalive() {
    if std::env::var_os("AGENTMUX_DISABLE_WSL_PREWARM").is_some() {
        return;
    }

    loop {
        let mut command = Command::new("wsl.exe");
        hide_console_window(&mut command);
        // Default distribution (no --distribution): the one new terminals use
        // unless a workspace pins a specific distro. `cat` blocks on an empty
        // stdin pipe, holding the distro — and thus the VM — resident.
        command
            .args(["--", "cat"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        match command.spawn() {
            Ok(mut child) => {
                // Hold the write end of the pipe for the anchor's lifetime.
                // Dropping it would EOF `cat` immediately; instead we keep it and
                // block on the child so the anchor lives until the VM (or we) die.
                let stdin = child.stdin.take();
                let _ = child.wait();
                drop(stdin);
                // Anchor exited (VM shutdown / killed); re-warm shortly.
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
            Err(_) => {
                // wsl.exe unavailable (WSL not installed?). This is an
                // optimization, not a requirement — back off hard and retry.
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
        }
    }
}

#[cfg(not(windows))]
pub fn run_wsl_prewarm_keepalive() {}

fn workspace_bundle_from_spawn(
    params: &SessionSpawnParams,
    result: &SessionSpawnResult,
    summary: &SessionSummaryResult,
    existing: Option<WorkspaceBundle>,
) -> WorkspaceBundle {
    let now = timestamp();
    let surface_id = terminal_surface_id_from_env(params)
        .unwrap_or_else(|| format!("surf_{}", result.session_id));
    let durability = params
        .durability
        .clone()
        .unwrap_or_else(|| "ephemeral".to_string());

    let surface = persisted_terminal_surface(params, result, &surface_id, &now);
    let session = persisted_terminal_session(params, result, summary, durability, &now);

    if let Some(mut bundle) = existing {
        bundle.workspace.updated_at = now.clone();
        if params.placement.as_deref() == Some("dock") {
            bundle.surfaces.push(surface);
            bundle.sessions.push(session);
            return bundle;
        } else if params.placement.as_deref() == Some("new_tab") {
            let pane_id = params
                .pane_id
                .clone()
                .unwrap_or_else(|| PaneId::new().to_string());
            bundle.panes.push(PersistedPane {
                pane_id: pane_id.clone(),
                workspace_id: params.workspace_id.clone(),
                parent_pane_id: None,
                kind: "leaf".to_string(),
                split_axis: None,
                split_ratio: None,
                mounted_surface_id: Some(surface_id.clone()),
                last_focused_at: Some(now.clone()),
                created_at: now.clone(),
                updated_at: now.clone(),
            });
            bundle.workspace.root_pane_id = pane_id.clone();
            bundle.workspace.active_pane_id = pane_id;
        } else {
            let target_pane_id = params
                .pane_id
                .as_deref()
                .unwrap_or(bundle.workspace.active_pane_id.as_str())
                .to_string();
            let mounted_pane_id = if let Some(active_pane) = bundle
                .panes
                .iter_mut()
                .find(|pane| pane.pane_id == target_pane_id)
            {
                active_pane.mounted_surface_id = Some(surface_id.clone());
                active_pane.last_focused_at = Some(now.clone());
                active_pane.updated_at = now.clone();
                Some(active_pane.pane_id.clone())
            } else {
                None
            };
            if let Some(mounted_pane_id) = mounted_pane_id {
                bundle.workspace.active_pane_id = mounted_pane_id.clone();
                if let Some(root_pane_id) = root_pane_id_for_pane(&bundle, &mounted_pane_id) {
                    bundle.workspace.root_pane_id = root_pane_id;
                }
            }
        }
        bundle.surfaces.push(surface);
        bundle.sessions.push(session);
        return bundle;
    }

    let pane_id = params
        .pane_id
        .clone()
        .unwrap_or_else(|| PaneId::new().to_string());
    WorkspaceBundle {
        workspace: PersistedWorkspace {
            workspace_id: params.workspace_id.clone(),
            name: params.workspace_id.clone(),
            root_pane_id: pane_id.clone(),
            active_pane_id: pane_id.clone(),
            project_root: params.cwd.clone(),
            environment_profile_id: None,
            description: None,
            icon: None,
            color: None,
            default_wsl_distribution: None,
            default_terminal_profile: None,
            default_agent_command: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        },
        panes: vec![PersistedPane {
            pane_id,
            workspace_id: params.workspace_id.clone(),
            parent_pane_id: None,
            kind: "leaf".to_string(),
            split_axis: None,
            split_ratio: None,
            mounted_surface_id: Some(surface_id),
            last_focused_at: Some(now.clone()),
            created_at: now.clone(),
            updated_at: now,
        }],
        surfaces: vec![surface],
        sessions: vec![session],
    }
}

fn terminal_surface_id_from_env(params: &SessionSpawnParams) -> Option<String> {
    params
        .env
        .iter()
        .find(|item| item.key == "AGENTMUX_SURFACE_ID")
        .or_else(|| params.env.iter().find(|item| item.key == "CMUX_SURFACE_ID"))
        .map(|item| item.value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn terminal_surface_title_from_env(params: &SessionSpawnParams) -> Option<String> {
    params
        .env
        .iter()
        .find(|item| item.key == "AGENTMUX_SURFACE_TITLE")
        .or_else(|| {
            params
                .env
                .iter()
                .find(|item| item.key == "CMUX_SURFACE_TITLE")
        })
        .map(|item| item.value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn terminal_surface_type_from_env(params: &SessionSpawnParams) -> String {
    params
        .env
        .iter()
        .find(|item| item.key == "AGENTMUX_SURFACE_TYPE")
        .map(|item| item.value.trim())
        .filter(|value| *value == "dock-terminal")
        .unwrap_or("terminal")
        .to_string()
}

fn terminal_dock_control_id_from_env(params: &SessionSpawnParams) -> Option<String> {
    params
        .env
        .iter()
        .find(|item| item.key == "AGENTMUX_DOCK_CONTROL_ID")
        .or_else(|| {
            params
                .env
                .iter()
                .find(|item| item.key == "CMUX_DOCK_CONTROL_ID")
        })
        .map(|item| item.value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn persisted_terminal_surface(
    params: &SessionSpawnParams,
    result: &SessionSpawnResult,
    surface_id: &str,
    now: &str,
) -> PersistedSurface {
    let surface_type = terminal_surface_type_from_env(params);
    let browser_id = if surface_type == "dock-terminal" {
        terminal_dock_control_id_from_env(params)
    } else {
        None
    };
    PersistedSurface {
        surface_id: surface_id.to_string(),
        workspace_id: params.workspace_id.clone(),
        surface_type,
        title: terminal_surface_title_from_env(params).unwrap_or_else(|| {
            params
                .command
                .first()
                .cloned()
                .unwrap_or_else(|| "terminal".to_string())
        }),
        session_id: Some(result.session_id.clone()),
        browser_id,
        created_at: now.to_string(),
        last_visible_at: Some(now.to_string()),
        updated_at: now.to_string(),
    }
}

fn persisted_browser_surface(surface: &BrowserSurface, now: &str) -> PersistedSurface {
    PersistedSurface {
        surface_id: surface.surface_id.clone(),
        workspace_id: surface.workspace_id.clone(),
        surface_type: "browser".to_string(),
        title: surface
            .current_url
            .clone()
            .unwrap_or_else(|| "Browser".to_string()),
        session_id: None,
        browser_id: Some(surface.browser_id.clone()),
        created_at: now.to_string(),
        last_visible_at: Some(now.to_string()),
        updated_at: now.to_string(),
    }
}

fn persisted_terminal_session(
    params: &SessionSpawnParams,
    result: &SessionSpawnResult,
    summary: &SessionSummaryResult,
    durability: String,
    now: &str,
) -> PersistedSession {
    PersistedSession {
        session_id: result.session_id.clone(),
        workspace_id: params.workspace_id.clone(),
        backend_kind: summary.backend_kind.clone(),
        backend_attachment_id: None,
        backend_native_id: summary.backend_native_id.clone(),
        cwd: params.cwd.clone(),
        command: params.command.clone(),
        state: summary.state.clone(),
        exit_code: summary.exit_code,
        durability,
        created_at: now.to_string(),
        last_seen_at: Some(now.to_string()),
        updated_at: now.to_string(),
    }
}

fn is_desktop_store_method(method: &str) -> bool {
    matches!(
        method,
        "system.ping"
            | "system.capabilities"
            | "system.identify"
            | "workspace.create"
            | "workspace.list"
            | "workspace.get"
            | "workspace.rename"
            | "workspace.update"
            | "workspace.close"
            | "workspace_group.list"
            | "workspace_group.create"
            | "workspace_group.update"
            | "workspace_group.delete"
            | "workspace_group.add_workspace"
            | "workspace_group.remove_workspace"
            | "pane.split"
            | "pane.focus"
            | "pane.close"
            | "pane.resize_layout"
            | "pane.mount_surface"
            | "pane.unmount_surface"
            | "surface.create_browser"
            | "surface.close"
            | "surface.move_workspace"
            | "browser.navigate"
            | "browser.reload"
            | "browser.back"
            | "browser.forward"
            | "browser.current_url"
            | "browser.screenshot"
            | "browser.dom_snapshot"
            | "browser.frames"
            | "browser.storage"
            | "browser.cookies"
            | "browser.downloads"
            | "browser.history"
            | "browser.console"
            | "browser.dialogs"
            | "browser.errors"
            | "browser.click"
            | "browser.type"
            | "browser.fill"
            | "browser.press"
            | "browser.select"
            | "browser.scroll"
            | "browser.hover"
            | "browser.check"
            | "browser.get"
            | "browser.find"
            | "browser.highlight"
            | "browser.focus"
            | "browser.zoom"
            | "browser.wait_for_selector"
            | "browser.evaluate"
            | "agent.get_state"
            | "agent.list_attention"
            | "agent.list"
            | "actions.list"
            | "notification.create"
            | "notification.list"
            | "notification.dismiss"
            | "notification.clear"
            | "team.task.list"
            | "team.task.create"
            | "team.task.claim"
            | "team.task.complete"
            | "team.task.block"
            | "team.task.unblock"
            | "team.task.set_dependency"
            | "team.message.list"
            | "team.message.send"
            | "team.message.mark_read"
            | "sidebar.set_status"
            | "sidebar.clear_status"
            | "sidebar.list_status"
            | "sidebar.set_progress"
            | "sidebar.clear_progress"
            | "sidebar.log"
            | "sidebar.clear_log"
            | "sidebar.list_log"
            | "sidebar.state"
            | "profile.list"
            | "profile.create"
            | "profile.update"
            | "profile.delete"
            | "config.get"
            | "config.reload"
            | "config.update"
            | "config.export"
            | "config.import"
            | "config.reset"
            | "config.migrate_project"
            | "config.diagnostics"
            | "dock.get"
            | "dock.trust"
            | "diagnostics.browser"
            | "diagnostics.export"
            | "diagnostics.recovery"
            | "diagnostics.wsl_distributions"
            | "diagnostics.tmux"
    )
}

fn validate_desktop_request(
    request: &RequestEnvelope,
    expected_token: &str,
) -> Result<(), ControlError> {
    if request.schema != agentmux_ipc::CONTROL_SCHEMA {
        return Err(ControlError::new(
            ErrorCode::InvalidRequest,
            "Unsupported control schema.",
        ));
    }

    if !constant_time_eq(request.auth.token.as_bytes(), expected_token.as_bytes()) {
        return Err(ControlError::new(
            ErrorCode::Unauthorized,
            "Invalid local control token.",
        ));
    }

    Ok(())
}

/// Constant-time byte-slice equality. Returns `true` iff `a == b` in all
/// bytes, without short-circuiting on the first mismatch. This prevents a
/// local timing oracle from recovering the control token one byte at a time.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn control_error_from_host(error: DesktopHostError) -> ControlError {
    match error {
        DesktopHostError::Control(error) => error,
        DesktopHostError::Json(error) => {
            ControlError::new(ErrorCode::InvalidRequest, error.to_string())
        }
        DesktopHostError::Store(error) => ControlError::new(ErrorCode::Conflict, error.to_string()),
        DesktopHostError::StateUnavailable(message) => {
            ControlError::new(ErrorCode::Conflict, message)
        }
    }
}

fn browser_automation_from_environment() -> Result<Box<dyn BrowserAutomation>, DesktopHostError> {
    match env::var("AGENTMUX_BROWSER_AUTOMATION")
        .unwrap_or_else(|_| "auto".to_string())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "memory" | "in-memory" | "in_memory" => Ok(Box::new(InMemoryBrowserAutomation::new())),
        "cdp" | "chrome" | "edge" => CdpBrowserAutomation::new()
            .map(|browser| Box::new(browser) as Box<dyn BrowserAutomation>)
            .map_err(browser_error_from_automation),
        "auto" | "" => match CdpBrowserAutomation::new() {
            Ok(browser) => Ok(Box::new(browser)),
            Err(_) => Ok(Box::new(InMemoryBrowserAutomation::new())),
        },
        mode => Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            format!("Unsupported browser automation mode '{mode}'. Use auto, cdp, or memory."),
        ))),
    }
}

fn browser_failure_error_code(error: &DesktopHostError) -> ErrorCode {
    match error {
        DesktopHostError::Control(error) => error.code,
        DesktopHostError::Json(_) => ErrorCode::InvalidRequest,
        DesktopHostError::Store(_) | DesktopHostError::StateUnavailable(_) => ErrorCode::Conflict,
    }
}

fn browser_failure_error_message(error: &DesktopHostError) -> String {
    match error {
        DesktopHostError::Control(error) => error.message.clone(),
        DesktopHostError::Json(error) => error.to_string(),
        DesktopHostError::Store(error) => error.to_string(),
        DesktopHostError::StateUnavailable(message) => message.clone(),
    }
}

fn workspace_not_found(workspace_id: &str) -> ControlError {
    ControlError::new(
        ErrorCode::WorkspaceNotFound,
        format!("Workspace '{workspace_id}' does not exist."),
    )
}

fn pane_not_found(pane_id: &str) -> ControlError {
    ControlError::new(
        ErrorCode::PaneNotFound,
        format!("Pane '{pane_id}' does not exist."),
    )
}

fn surface_not_found(surface_id: &str) -> ControlError {
    ControlError::new(
        ErrorCode::SurfaceNotFound,
        format!("Surface '{surface_id}' does not exist."),
    )
}

fn validate_browser_mount_target(
    bundle: &WorkspaceBundle,
    pane_id: &str,
) -> Result<(), DesktopHostError> {
    let pane = bundle
        .panes
        .iter()
        .find(|pane| pane.pane_id == pane_id)
        .ok_or_else(|| pane_not_found(pane_id))?;
    if pane.kind != "leaf" {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            "Browser surfaces can only be mounted into leaf panes.",
        )));
    }
    Ok(())
}

fn browser_coordinate_to_i32(value: Option<f64>, field: &str) -> Result<i32, DesktopHostError> {
    let value = value.ok_or_else(|| {
        DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            format!("browser.click requires either selector or both {field} coordinate values."),
        ))
    })?;
    if !value.is_finite() || value < 0.0 || value > i32::MAX as f64 {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            format!("browser.click coordinate {field} must be a non-negative finite integer."),
        )));
    }
    Ok(value.round() as i32)
}

fn browser_action_ok_response(request: &RequestEnvelope, surface_id: String) -> ResponseEnvelope {
    ResponseEnvelope::ok_typed(
        request.id.clone(),
        &BrowserActionResult {
            surface_id,
            ok: true,
        },
    )
}

fn browser_frame_result(frame: BrowserFrameInfo) -> BrowserFrameResult {
    BrowserFrameResult {
        frame_id: frame.frame_id,
        parent_frame_id: frame.parent_frame_id,
        url: frame.url,
        name: frame.name,
        security_origin: frame.security_origin,
    }
}

fn browser_storage_entry_result(entry: BrowserStorageEntry) -> BrowserStorageEntryResult {
    BrowserStorageEntryResult {
        key: entry.key,
        value: entry.value,
    }
}

fn browser_cookie_result(cookie: BrowserCookieInfo) -> BrowserCookieResult {
    BrowserCookieResult {
        name: cookie.name,
        value: cookie.value,
        domain: cookie.domain,
        path: cookie.path,
        expires: cookie.expires,
        http_only: cookie.http_only,
        secure: cookie.secure,
        same_site: cookie.same_site,
    }
}

fn browser_download_result(download: BrowserDownloadInfo) -> BrowserDownloadResult {
    BrowserDownloadResult {
        file_name: download.file_name,
        path: download.path,
        byte_count: download.byte_count,
        modified_at: download.modified_at,
        complete: download.complete,
    }
}

fn browser_console_message_result(message: BrowserConsoleMessage) -> BrowserConsoleMessageResult {
    BrowserConsoleMessageResult {
        level: message.level,
        text: message.text,
        timestamp: message.timestamp,
    }
}

fn browser_history_entry_result(entry: BrowserHistoryEntry) -> BrowserHistoryEntryResult {
    BrowserHistoryEntryResult {
        id: entry.id,
        url: entry.url,
        title: entry.title,
    }
}

fn browser_dialog_message_result(message: BrowserDialogMessage) -> BrowserDialogMessageResult {
    BrowserDialogMessageResult {
        dialog_type: message.dialog_type,
        message: message.message,
        default_value: message.default_value,
        response: message.response,
        timestamp: message.timestamp,
    }
}

fn browser_error_event_result(event: BrowserErrorEvent) -> BrowserErrorEventResult {
    BrowserErrorEventResult {
        kind: event.kind,
        message: event.message,
        source: event.source,
        line: event.line,
        column: event.column,
        stack: event.stack,
        timestamp: event.timestamp,
    }
}

fn browser_error_from_automation(error: BrowserAutomationError) -> DesktopHostError {
    let code = match error.code {
        BrowserAutomationErrorCode::SurfaceNotFound => ErrorCode::SurfaceNotFound,
        BrowserAutomationErrorCode::InvalidRequest => ErrorCode::InvalidRequest,
        BrowserAutomationErrorCode::AutomationFailed => ErrorCode::BackendDegraded,
    };
    DesktopHostError::Control(ControlError::new(code, error.message))
}

fn normalize_workspace_pane_tree(bundle: &mut WorkspaceBundle) -> bool {
    let mut changed = false;
    let now = timestamp();

    if bundle.panes.is_empty() {
        let pane_id = if bundle.workspace.root_pane_id.is_empty() {
            PaneId::new().to_string()
        } else {
            bundle.workspace.root_pane_id.clone()
        };
        bundle.workspace.root_pane_id = pane_id.clone();
        bundle.workspace.active_pane_id = pane_id.clone();
        bundle.panes.push(PersistedPane {
            pane_id,
            workspace_id: bundle.workspace.workspace_id.clone(),
            parent_pane_id: None,
            kind: "leaf".to_string(),
            split_axis: None,
            split_ratio: None,
            mounted_surface_id: None,
            last_focused_at: Some(now.clone()),
            created_at: now.clone(),
            updated_at: now.clone(),
        });
        bundle.workspace.updated_at = now;
        return true;
    }

    let pane_ids = bundle
        .panes
        .iter()
        .map(|pane| pane.pane_id.clone())
        .collect::<HashSet<_>>();

    for index in 0..bundle.panes.len() {
        let pane_id = bundle.panes[index].pane_id.clone();
        let mut parent = bundle.panes[index].parent_pane_id.clone();
        let mut seen = HashSet::from([pane_id.clone()]);
        let mut invalid_parent = false;

        while let Some(parent_id) = parent {
            if !pane_ids.contains(&parent_id) || !seen.insert(parent_id.clone()) {
                invalid_parent = true;
                break;
            }
            parent = bundle
                .panes
                .iter()
                .find(|candidate| candidate.pane_id == parent_id)
                .and_then(|candidate| candidate.parent_pane_id.clone());
        }

        if invalid_parent || bundle.panes[index].parent_pane_id.as_deref() == Some(pane_id.as_str())
        {
            bundle.panes[index].parent_pane_id = None;
            bundle.panes[index].updated_at = now.clone();
            changed = true;
        }
    }

    let parent_kinds = bundle
        .panes
        .iter()
        .map(|pane| (pane.pane_id.clone(), pane.kind.clone()))
        .collect::<HashMap<_, _>>();
    let mut child_counts: HashMap<String, usize> = HashMap::new();

    for pane in &mut bundle.panes {
        let Some(parent_id) = pane.parent_pane_id.clone() else {
            continue;
        };
        if parent_kinds.get(&parent_id).map(String::as_str) != Some("split") {
            pane.parent_pane_id = None;
            pane.updated_at = now.clone();
            changed = true;
            continue;
        }

        let count = child_counts.entry(parent_id).or_insert(0);
        *count += 1;
        if *count > 2 {
            pane.parent_pane_id = None;
            pane.updated_at = now.clone();
            changed = true;
        }
    }

    if !bundle
        .panes
        .iter()
        .any(|pane| pane.parent_pane_id.is_none())
    {
        if let Some(first) = bundle.panes.first_mut() {
            first.parent_pane_id = None;
            first.updated_at = now.clone();
            changed = true;
        }
    }

    // Dissolve degenerate split panes. A split must have exactly two children;
    // a half-closed split left with a single child (or none) would otherwise
    // render as a split with one blank half. With one child, promote it to the
    // split's parent and drop the split; with none, turn the split into an empty
    // leaf. Loop, because dissolving a split can leave its parent degenerate too.
    loop {
        let mut child_counts: HashMap<String, usize> = HashMap::new();
        for pane in &bundle.panes {
            if let Some(parent_id) = &pane.parent_pane_id {
                *child_counts.entry(parent_id.clone()).or_insert(0) += 1;
            }
        }
        let Some((split_id, split_parent, child_count)) = bundle
            .panes
            .iter()
            .find(|pane| {
                pane.kind == "split" && child_counts.get(&pane.pane_id).copied().unwrap_or(0) < 2
            })
            .map(|pane| {
                (
                    pane.pane_id.clone(),
                    pane.parent_pane_id.clone(),
                    child_counts.get(&pane.pane_id).copied().unwrap_or(0),
                )
            })
        else {
            break;
        };

        if child_count == 0 {
            if let Some(pane) = bundle.panes.iter_mut().find(|p| p.pane_id == split_id) {
                pane.kind = "leaf".to_string();
                pane.split_axis = None;
                pane.split_ratio = None;
                pane.updated_at = now.clone();
            }
        } else {
            for pane in &mut bundle.panes {
                if pane.parent_pane_id.as_deref() == Some(split_id.as_str()) {
                    pane.parent_pane_id = split_parent.clone();
                    pane.updated_at = now.clone();
                }
            }
            bundle.panes.retain(|pane| pane.pane_id != split_id);
        }
        changed = true;
    }

    let root_is_valid = bundle
        .panes
        .iter()
        .any(|pane| pane.pane_id == bundle.workspace.root_pane_id && pane.parent_pane_id.is_none());
    if !root_is_valid {
        let root_pane_id = root_pane_id_for_pane(bundle, &bundle.workspace.active_pane_id)
            .or_else(|| {
                bundle
                    .panes
                    .iter()
                    .find(|pane| pane.parent_pane_id.is_none())
                    .map(|pane| pane.pane_id.clone())
            })
            .unwrap_or_else(|| bundle.panes[0].pane_id.clone());
        bundle.workspace.root_pane_id = root_pane_id;
        changed = true;
    }

    let active_is_leaf = bundle
        .panes
        .iter()
        .any(|pane| pane.pane_id == bundle.workspace.active_pane_id && pane.kind == "leaf");
    if !active_is_leaf {
        if let Some(active_pane_id) =
            first_leaf_id(bundle, &bundle.workspace.root_pane_id).or_else(|| {
                bundle
                    .panes
                    .iter()
                    .find(|pane| pane.kind == "leaf")
                    .map(|pane| pane.pane_id.clone())
            })
        {
            bundle.workspace.active_pane_id = active_pane_id;
            changed = true;
        }
    }

    // Heal surface mounts: a surface may be mounted by at most one pane, and only
    // surfaces that belong to this workspace's bundle. Clear dangling mounts (the
    // surface was closed or belongs to another workspace) and de-duplicate, so one
    // surface never renders in two panes at once.
    let valid_surface_ids: HashSet<String> = bundle
        .surfaces
        .iter()
        .map(|surface| surface.surface_id.clone())
        .collect();
    let mut mounted_seen: HashSet<String> = HashSet::new();
    for pane in &mut bundle.panes {
        let Some(surface_id) = pane.mounted_surface_id.clone() else {
            continue;
        };
        if !valid_surface_ids.contains(&surface_id) || !mounted_seen.insert(surface_id) {
            pane.mounted_surface_id = None;
            pane.updated_at = now.clone();
            changed = true;
        }
    }

    if changed {
        bundle.workspace.updated_at = now;
    }
    changed
}

fn split_pane_in_bundle(
    bundle: &mut WorkspaceBundle,
    pane_id: &str,
    axis: &str,
    ratio: f64,
) -> Result<(), DesktopHostError> {
    let now = timestamp();
    let Some(index) = bundle.panes.iter().position(|pane| pane.pane_id == pane_id) else {
        return Err(pane_not_found(pane_id).into());
    };
    if bundle.panes[index].kind != "leaf" {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            "Only leaf panes can be split.",
        )));
    }

    let workspace_id = bundle.workspace.workspace_id.clone();
    let first_child_id = PaneId::new().to_string();
    let second_child_id = PaneId::new().to_string();
    let was_active = bundle.workspace.active_pane_id == pane_id;
    let previous_surface_id = bundle.panes[index].mounted_surface_id.take();
    let previous_focus = bundle.panes[index].last_focused_at.take();

    {
        let pane = &mut bundle.panes[index];
        pane.kind = "split".to_string();
        pane.split_axis = Some(axis.to_string());
        pane.split_ratio = Some(ratio);
        pane.mounted_surface_id = None;
        pane.last_focused_at = None;
        pane.updated_at = now.clone();
    }

    bundle.panes.push(PersistedPane {
        pane_id: first_child_id.clone(),
        workspace_id: workspace_id.clone(),
        parent_pane_id: Some(pane_id.to_string()),
        kind: "leaf".to_string(),
        split_axis: None,
        split_ratio: None,
        mounted_surface_id: previous_surface_id,
        last_focused_at: if was_active {
            Some(now.clone())
        } else {
            previous_focus
        },
        created_at: now.clone(),
        updated_at: now.clone(),
    });
    bundle.panes.push(PersistedPane {
        pane_id: second_child_id,
        workspace_id,
        parent_pane_id: Some(pane_id.to_string()),
        kind: "leaf".to_string(),
        split_axis: None,
        split_ratio: None,
        mounted_surface_id: None,
        last_focused_at: None,
        created_at: now.clone(),
        updated_at: now.clone(),
    });

    if was_active {
        bundle.workspace.active_pane_id = first_child_id;
    }
    bundle.workspace.updated_at = now;
    Ok(())
}

fn focus_pane_in_bundle(
    bundle: &mut WorkspaceBundle,
    pane_id: &str,
) -> Result<(), DesktopHostError> {
    let now = timestamp();
    let Some(pane) = bundle.panes.iter_mut().find(|pane| pane.pane_id == pane_id) else {
        return Err(pane_not_found(pane_id).into());
    };
    if pane.kind != "leaf" {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            "Only leaf panes can be focused.",
        )));
    }

    pane.last_focused_at = Some(now.clone());
    pane.updated_at = now.clone();
    bundle.workspace.active_pane_id = pane_id.to_string();
    if let Some(root_pane_id) = root_pane_id_for_pane(bundle, pane_id) {
        bundle.workspace.root_pane_id = root_pane_id;
    }
    bundle.workspace.updated_at = now;
    Ok(())
}

fn resize_pane_layout_in_bundle(
    bundle: &mut WorkspaceBundle,
    pane_id: &str,
    ratio: f64,
) -> Result<(), DesktopHostError> {
    let now = timestamp();
    let Some(pane) = bundle.panes.iter_mut().find(|pane| pane.pane_id == pane_id) else {
        return Err(pane_not_found(pane_id).into());
    };
    if pane.kind != "split" {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            "Only split panes can be resized.",
        )));
    }

    pane.split_ratio = Some(ratio);
    pane.updated_at = now.clone();
    bundle.workspace.updated_at = now;
    Ok(())
}

fn mount_surface_in_bundle(
    bundle: &mut WorkspaceBundle,
    pane_id: &str,
    surface_id: &str,
) -> Result<(), DesktopHostError> {
    if !bundle.surfaces.iter().any(|surface| {
        surface.surface_id == surface_id && surface.workspace_id == bundle.workspace.workspace_id
    }) {
        return Err(surface_not_found(surface_id).into());
    }

    let now = timestamp();
    let Some(target_index) = bundle.panes.iter().position(|pane| pane.pane_id == pane_id) else {
        return Err(pane_not_found(pane_id).into());
    };
    if bundle.panes[target_index].kind != "leaf" {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            "Only leaf panes can mount surfaces.",
        )));
    }

    for pane in &mut bundle.panes {
        if pane.mounted_surface_id.as_deref() == Some(surface_id) {
            pane.mounted_surface_id = None;
            pane.updated_at = now.clone();
        }
    }

    let pane = &mut bundle.panes[target_index];
    pane.mounted_surface_id = Some(surface_id.to_string());
    pane.last_focused_at = Some(now.clone());
    pane.updated_at = now.clone();
    bundle.workspace.active_pane_id = pane_id.to_string();
    if let Some(root_pane_id) = root_pane_id_for_pane(bundle, pane_id) {
        bundle.workspace.root_pane_id = root_pane_id;
    }
    bundle.workspace.updated_at = now;
    Ok(())
}

fn unmount_surface_in_bundle(
    bundle: &mut WorkspaceBundle,
    pane_id: &str,
) -> Result<(), DesktopHostError> {
    let now = timestamp();
    let Some(pane) = bundle.panes.iter_mut().find(|pane| pane.pane_id == pane_id) else {
        return Err(pane_not_found(pane_id).into());
    };
    if pane.kind != "leaf" {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            "Only leaf panes can unmount surfaces.",
        )));
    }

    pane.mounted_surface_id = None;
    pane.updated_at = now.clone();
    bundle.workspace.updated_at = now;
    Ok(())
}

fn surface_ids_for_close(
    bundle: &WorkspaceBundle,
    surface_id: &str,
) -> Result<Vec<String>, DesktopHostError> {
    if !bundle
        .surfaces
        .iter()
        .any(|surface| surface.surface_id == surface_id)
    {
        return Err(surface_not_found(surface_id).into());
    }

    let root_count = bundle
        .panes
        .iter()
        .filter(|pane| pane.parent_pane_id.is_none())
        .count();
    let host_pane_id = bundle
        .panes
        .iter()
        .find(|pane| pane.mounted_surface_id.as_deref() == Some(surface_id))
        .map(|pane| pane.pane_id.clone());

    if root_count <= 1 {
        return Ok(vec![surface_id.to_string()]);
    }

    let Some(host_pane_id) = host_pane_id else {
        return Ok(vec![surface_id.to_string()]);
    };
    let Some(root_pane_id) = root_pane_id_for_pane(bundle, &host_pane_id) else {
        return Ok(vec![surface_id.to_string()]);
    };

    let subtree_pane_ids = pane_subtree_ids(bundle, &root_pane_id);
    let mut surface_ids = bundle
        .panes
        .iter()
        .filter(|pane| subtree_pane_ids.contains(&pane.pane_id))
        .filter_map(|pane| pane.mounted_surface_id.clone())
        .collect::<Vec<_>>();
    if !surface_ids.iter().any(|candidate| candidate == surface_id) {
        surface_ids.push(surface_id.to_string());
    }
    Ok(surface_ids)
}

fn move_surface_tab_between_workspaces(
    source: &mut WorkspaceBundle,
    target: &mut WorkspaceBundle,
    surface_id: &str,
) -> Result<Vec<String>, DesktopHostError> {
    let now = timestamp();
    let host_pane_id = source
        .panes
        .iter()
        .find(|pane| pane.mounted_surface_id.as_deref() == Some(surface_id))
        .map(|pane| pane.pane_id.clone());
    let root_pane_id = host_pane_id
        .as_deref()
        .and_then(|pane_id| root_pane_id_for_pane(source, pane_id));
    let moved_pane_ids = root_pane_id
        .as_deref()
        .map(|pane_id| pane_subtree_ids(source, pane_id))
        .unwrap_or_default();
    let moved_pane_id_set = moved_pane_ids.iter().cloned().collect::<HashSet<_>>();

    if !source
        .surfaces
        .iter()
        .any(|surface| surface.surface_id == surface_id)
    {
        return Err(surface_not_found(surface_id).into());
    }

    let mut moved_surface_ids = if moved_pane_id_set.is_empty() {
        vec![surface_id.to_string()]
    } else {
        source
            .panes
            .iter()
            .filter(|pane| moved_pane_id_set.contains(&pane.pane_id))
            .filter_map(|pane| pane.mounted_surface_id.clone())
            .collect::<Vec<_>>()
    };
    if !moved_surface_ids
        .iter()
        .any(|candidate| candidate == surface_id)
    {
        moved_surface_ids.push(surface_id.to_string());
    }
    moved_surface_ids.sort();
    moved_surface_ids.dedup();
    let moved_surface_id_set = moved_surface_ids.iter().cloned().collect::<HashSet<_>>();

    let moved_session_ids = source
        .surfaces
        .iter()
        .filter(|surface| moved_surface_id_set.contains(&surface.surface_id))
        .filter_map(|surface| surface.session_id.clone())
        .collect::<Vec<_>>();
    let moved_session_id_set = moved_session_ids.iter().cloned().collect::<HashSet<_>>();

    let mut moved_panes = source
        .panes
        .iter()
        .filter(|pane| moved_pane_id_set.contains(&pane.pane_id))
        .cloned()
        .collect::<Vec<_>>();
    let mut moved_surfaces = source
        .surfaces
        .iter()
        .filter(|surface| moved_surface_id_set.contains(&surface.surface_id))
        .cloned()
        .collect::<Vec<_>>();
    let mut moved_sessions = source
        .sessions
        .iter()
        .filter(|session| moved_session_id_set.contains(&session.session_id))
        .cloned()
        .collect::<Vec<_>>();

    if moved_surfaces.is_empty() {
        return Err(surface_not_found(surface_id).into());
    }

    source
        .panes
        .retain(|pane| !moved_pane_id_set.contains(&pane.pane_id));
    source
        .surfaces
        .retain(|surface| !moved_surface_id_set.contains(&surface.surface_id));
    source
        .sessions
        .retain(|session| !moved_session_id_set.contains(&session.session_id));

    let moved_root_id = if let Some(root_pane_id) = root_pane_id {
        root_pane_id
    } else {
        let pane_id = PaneId::new().to_string();
        moved_panes.push(PersistedPane {
            pane_id: pane_id.clone(),
            workspace_id: target.workspace.workspace_id.clone(),
            parent_pane_id: None,
            kind: "leaf".to_string(),
            split_axis: None,
            split_ratio: None,
            mounted_surface_id: Some(surface_id.to_string()),
            last_focused_at: Some(now.clone()),
            created_at: now.clone(),
            updated_at: now.clone(),
        });
        pane_id
    };

    for pane in &mut moved_panes {
        pane.workspace_id = target.workspace.workspace_id.clone();
        if pane.parent_pane_id.as_ref().is_some_and(|parent| {
            !moved_pane_id_set.contains(parent) || pane.pane_id == moved_root_id
        }) {
            pane.parent_pane_id = None;
        }
        if pane.pane_id == moved_root_id {
            pane.parent_pane_id = None;
        }
        pane.updated_at = now.clone();
    }
    for surface in &mut moved_surfaces {
        surface.workspace_id = target.workspace.workspace_id.clone();
        surface.updated_at = now.clone();
        surface.last_visible_at = Some(now.clone());
    }
    for session in &mut moved_sessions {
        session.workspace_id = target.workspace.workspace_id.clone();
        session.updated_at = now.clone();
    }

    target.panes.extend(moved_panes);
    target.surfaces.extend(moved_surfaces);
    target.sessions.extend(moved_sessions);
    target.workspace.root_pane_id = moved_root_id.clone();
    target.workspace.active_pane_id =
        first_leaf_id(target, &moved_root_id).unwrap_or(moved_root_id);
    target.workspace.updated_at = now.clone();
    normalize_workspace_pane_tree(source);
    normalize_workspace_pane_tree(target);
    Ok(moved_session_ids)
}

fn pane_subtree_ids(bundle: &WorkspaceBundle, pane_id: &str) -> Vec<String> {
    let mut ids = vec![pane_id.to_string()];
    let children = bundle
        .panes
        .iter()
        .filter(|pane| pane.parent_pane_id.as_deref() == Some(pane_id))
        .map(|pane| pane.pane_id.clone())
        .collect::<Vec<_>>();
    for child_id in children {
        ids.extend(pane_subtree_ids(bundle, &child_id));
    }
    ids
}

fn close_surface_in_bundle(
    bundle: &mut WorkspaceBundle,
    surface_id: &str,
) -> Result<(), DesktopHostError> {
    let surface_ids = surface_ids_for_close(bundle, surface_id)?;
    let Some(surface_index) = bundle
        .surfaces
        .iter()
        .position(|surface| surface.surface_id == surface_id)
    else {
        return Err(surface_not_found(surface_id).into());
    };

    let now = timestamp();
    let root_count = bundle
        .panes
        .iter()
        .filter(|pane| pane.parent_pane_id.is_none())
        .count();
    let host_pane_id = bundle
        .panes
        .iter()
        .find(|pane| pane.mounted_surface_id.as_deref() == Some(surface_id))
        .map(|pane| pane.pane_id.clone());

    if root_count > 1 {
        if let Some(host_pane_id) = host_pane_id {
            if let Some(root_pane_id) = root_pane_id_for_pane(bundle, &host_pane_id) {
                let roots = bundle
                    .panes
                    .iter()
                    .filter(|pane| pane.parent_pane_id.is_none())
                    .map(|pane| pane.pane_id.clone())
                    .collect::<Vec<_>>();
                let fallback_root_id = roots
                    .iter()
                    .position(|pane_id| pane_id == &root_pane_id)
                    .and_then(|index| {
                        if index > 0 {
                            roots.get(index - 1)
                        } else {
                            roots.get(index + 1)
                        }
                    })
                    .cloned();
                let subtree_pane_ids = pane_subtree_ids(bundle, &root_pane_id);
                bundle
                    .panes
                    .retain(|pane| !subtree_pane_ids.contains(&pane.pane_id));
                bundle
                    .surfaces
                    .retain(|surface| !surface_ids.contains(&surface.surface_id));
                let remaining_session_ids = bundle
                    .surfaces
                    .iter()
                    .filter_map(|surface| surface.session_id.clone())
                    .collect::<Vec<_>>();
                bundle
                    .sessions
                    .retain(|session| remaining_session_ids.contains(&session.session_id));
                if let Some(fallback_root_id) = fallback_root_id {
                    bundle.workspace.root_pane_id = fallback_root_id.clone();
                    bundle.workspace.active_pane_id =
                        first_leaf_id(bundle, &fallback_root_id).unwrap_or(fallback_root_id);
                }
                bundle.workspace.updated_at = now;
                return Ok(());
            }
        }
    }

    let closed_session_ids = bundle
        .surfaces
        .iter()
        .filter(|surface| surface_ids.contains(&surface.surface_id))
        .filter_map(|surface| surface.session_id.clone())
        .collect::<Vec<_>>();
    bundle
        .surfaces
        .retain(|surface| !surface_ids.contains(&surface.surface_id));
    bundle
        .sessions
        .retain(|session| !closed_session_ids.contains(&session.session_id));

    let replacement_surface_id = if bundle.surfaces.is_empty() {
        None
    } else if surface_index == 0 {
        Some(bundle.surfaces[0].surface_id.clone())
    } else {
        Some(bundle.surfaces[surface_index - 1].surface_id.clone())
    };

    let mut replacement_target_pane_ids = Vec::new();
    for pane in &mut bundle.panes {
        if pane.mounted_surface_id.as_deref() == Some(surface_id) {
            pane.mounted_surface_id = replacement_surface_id.clone();
            pane.last_focused_at = Some(now.clone());
            pane.updated_at = now.clone();
            replacement_target_pane_ids.push(pane.pane_id.clone());
        }
    }

    if !replacement_target_pane_ids.is_empty() {
        if let Some(surface_id) = replacement_surface_id.as_deref() {
            for pane in &mut bundle.panes {
                if pane.mounted_surface_id.as_deref() == Some(surface_id)
                    && !replacement_target_pane_ids.contains(&pane.pane_id)
                {
                    pane.mounted_surface_id = None;
                    pane.updated_at = now.clone();
                }
            }
        }
    }

    bundle.workspace.updated_at = now;
    Ok(())
}

fn close_pane_in_bundle(
    bundle: &mut WorkspaceBundle,
    pane_id: &str,
    surface_policy: &str,
) -> Result<(), DesktopHostError> {
    let Some(target_index) = bundle.panes.iter().position(|pane| pane.pane_id == pane_id) else {
        return Err(pane_not_found(pane_id).into());
    };
    if bundle.panes[target_index].kind != "leaf" {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            "Only leaf panes can be closed.",
        )));
    }
    let Some(parent_id) = bundle.panes[target_index].parent_pane_id.clone() else {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::Conflict,
            "Cannot close the last pane in a workspace.",
        )));
    };

    let now = timestamp();
    let closed_surface_id = bundle.panes[target_index].mounted_surface_id.clone();
    bundle.panes.remove(target_index);

    if matches!(surface_policy, "close_surface" | "fail_if_session_running") {
        if let Some(surface_id) = closed_surface_id {
            bundle
                .surfaces
                .retain(|surface| surface.surface_id != surface_id);
        }
    }

    let sibling_ids = bundle
        .panes
        .iter()
        .filter(|pane| pane.parent_pane_id.as_deref() == Some(parent_id.as_str()))
        .map(|pane| pane.pane_id.clone())
        .collect::<Vec<_>>();

    if sibling_ids.len() == 1 {
        promote_single_child_into_parent(bundle, &parent_id, &sibling_ids[0], &now)?;
    }

    if !bundle
        .panes
        .iter()
        .any(|pane| pane.pane_id == bundle.workspace.active_pane_id)
    {
        let active = first_leaf_id(bundle, &bundle.workspace.root_pane_id).ok_or_else(|| {
            DesktopHostError::Control(pane_not_found(&bundle.workspace.root_pane_id))
        })?;
        bundle.workspace.active_pane_id = active;
    }

    bundle.workspace.updated_at = now;
    Ok(())
}

fn promote_single_child_into_parent(
    bundle: &mut WorkspaceBundle,
    parent_id: &str,
    child_id: &str,
    now: &str,
) -> Result<(), DesktopHostError> {
    let Some(parent_index) = bundle
        .panes
        .iter()
        .position(|pane| pane.pane_id == parent_id)
    else {
        return Err(pane_not_found(parent_id).into());
    };
    let Some(child_index) = bundle
        .panes
        .iter()
        .position(|pane| pane.pane_id == child_id)
    else {
        return Err(pane_not_found(child_id).into());
    };

    let child = bundle.panes[child_index].clone();
    {
        let parent = &mut bundle.panes[parent_index];
        parent.kind = child.kind;
        parent.split_axis = child.split_axis;
        parent.split_ratio = child.split_ratio;
        parent.mounted_surface_id = child.mounted_surface_id;
        parent.last_focused_at = child.last_focused_at;
        parent.updated_at = now.to_string();
    }

    for pane in &mut bundle.panes {
        if pane.parent_pane_id.as_deref() == Some(child_id) {
            pane.parent_pane_id = Some(parent_id.to_string());
            pane.updated_at = now.to_string();
        }
    }

    bundle.panes.retain(|pane| pane.pane_id != child_id);
    if bundle.workspace.active_pane_id == child_id {
        bundle.workspace.active_pane_id = parent_id.to_string();
    }
    Ok(())
}

fn first_leaf_id(bundle: &WorkspaceBundle, pane_id: &str) -> Option<String> {
    let pane = bundle.panes.iter().find(|pane| pane.pane_id == pane_id)?;
    if pane.kind == "leaf" {
        return Some(pane.pane_id.clone());
    }

    bundle
        .panes
        .iter()
        .filter(|candidate| candidate.parent_pane_id.as_deref() == Some(pane_id))
        .find_map(|child| first_leaf_id(bundle, &child.pane_id))
}

fn root_pane_id_for_pane(bundle: &WorkspaceBundle, pane_id: &str) -> Option<String> {
    let mut pane = bundle.panes.iter().find(|pane| pane.pane_id == pane_id)?;
    let mut guard = 0;
    while let Some(parent_pane_id) = pane.parent_pane_id.as_deref() {
        pane = bundle
            .panes
            .iter()
            .find(|candidate| candidate.pane_id == parent_pane_id)?;
        guard += 1;
        if guard > 100 {
            return None;
        }
    }
    Some(pane.pane_id.clone())
}

fn workspace_summary(workspace: &PersistedWorkspace) -> WorkspaceSummaryResult {
    WorkspaceSummaryResult {
        workspace_id: workspace.workspace_id.clone(),
        name: workspace.name.clone(),
        root_pane_id: workspace.root_pane_id.clone(),
        active_pane_id: workspace.active_pane_id.clone(),
        project_root: workspace.project_root.clone(),
        environment_profile_id: workspace.environment_profile_id.clone(),
        description: workspace.description.clone(),
        icon: workspace.icon.clone(),
        color: workspace.color.clone(),
        default_wsl_distribution: workspace.default_wsl_distribution.clone(),
        default_terminal_profile: workspace.default_terminal_profile.clone(),
        default_agent_command: workspace.default_agent_command.clone(),
    }
}

fn runtime_request_needs_pre_dispatch_collect(method: &str) -> bool {
    !matches!(
        method,
        "session.list"
            | "session.get"
            | "session.read_recent"
            | "session.snapshot"
            | "events.poll"
            | "events.subscribe"
            | "agent.get_state"
            | "agent.list_attention"
            | "notification.list"
    )
}

fn clean_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn workspace_group_list_result(
    store: &SqliteStore,
) -> Result<WorkspaceGroupListResult, DesktopHostError> {
    let groups = store
        .list_workspace_groups()?
        .into_iter()
        .map(|group| workspace_group_result(store, group))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(WorkspaceGroupListResult { groups })
}

fn workspace_group_result_for_id(
    store: &SqliteStore,
    group_id: &str,
) -> Result<WorkspaceGroupResult, DesktopHostError> {
    let group = store
        .load_workspace_group(group_id)?
        .ok_or_else(|| workspace_group_not_found(group_id))?;
    workspace_group_result(store, group)
}

fn workspace_group_result(
    store: &SqliteStore,
    group: PersistedWorkspaceGroup,
) -> Result<WorkspaceGroupResult, DesktopHostError> {
    let members = store
        .list_workspace_group_members(Some(&group.group_id))?
        .into_iter()
        .map(workspace_group_member_result)
        .collect();
    Ok(WorkspaceGroupResult {
        group_id: group.group_id,
        name: group.name,
        anchor_workspace_id: group.anchor_workspace_id,
        collapsed: group.collapsed,
        pinned: group.pinned,
        color: group.color,
        icon: group.icon,
        sort_order: group.sort_order,
        created_at: group.created_at,
        updated_at: group.updated_at,
        members,
    })
}

fn workspace_group_member_result(
    member: PersistedWorkspaceGroupMember,
) -> WorkspaceGroupMemberResult {
    WorkspaceGroupMemberResult {
        workspace_id: member.workspace_id,
        position: member.position,
    }
}

fn workspace_group_not_found(group_id: &str) -> DesktopHostError {
    DesktopHostError::Control(ControlError::new(
        ErrorCode::InvalidRequest,
        format!("Workspace group '{group_id}' was not found."),
    ))
}

fn validate_optional_workspace_exists(
    store: &SqliteStore,
    workspace_id: Option<&str>,
) -> Result<(), DesktopHostError> {
    if let Some(workspace_id) = workspace_id {
        validate_workspace_exists(store, workspace_id)?;
    }
    Ok(())
}

fn validate_workspace_exists(
    store: &SqliteStore,
    workspace_id: &str,
) -> Result<(), DesktopHostError> {
    if store.load_workspace_bundle(workspace_id)?.is_some() {
        return Ok(());
    }
    Err(DesktopHostError::Control(ControlError::new(
        ErrorCode::WorkspaceNotFound,
        format!("Workspace '{workspace_id}' was not found."),
    )))
}

fn normalize_workspace_group_name(name: &str) -> Result<String, DesktopHostError> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            "workspace group name cannot be empty.",
        )));
    }
    Ok(name)
}

fn normalize_optional_workspace_group_text(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn workspace_detail(bundle: WorkspaceBundle) -> WorkspaceDetailResult {
    WorkspaceDetailResult {
        workspace: workspace_summary(&bundle.workspace),
        panes: bundle.panes.iter().map(pane_summary).collect(),
        surfaces: bundle.surfaces.iter().map(surface_summary).collect(),
        sessions: bundle
            .sessions
            .iter()
            .map(persisted_session_summary)
            .collect(),
    }
}

fn pane_summary(pane: &PersistedPane) -> PaneSummaryResult {
    PaneSummaryResult {
        pane_id: pane.pane_id.clone(),
        workspace_id: pane.workspace_id.clone(),
        parent_pane_id: pane.parent_pane_id.clone(),
        kind: pane.kind.clone(),
        split_axis: pane.split_axis.clone(),
        split_ratio: pane.split_ratio,
        mounted_surface_id: pane.mounted_surface_id.clone(),
    }
}

fn surface_summary(surface: &PersistedSurface) -> SurfaceSummaryResult {
    SurfaceSummaryResult {
        surface_id: surface.surface_id.clone(),
        workspace_id: surface.workspace_id.clone(),
        surface_type: surface.surface_type.clone(),
        title: surface.title.clone(),
        session_id: surface.session_id.clone(),
        browser_id: surface.browser_id.clone(),
    }
}

fn profile_summary(profile: &PersistedProfile) -> ProfileSummaryResult {
    ProfileSummaryResult {
        profile_id: profile.profile_id.clone(),
        name: profile.name.clone(),
        host: profile.host.clone(),
        user: profile.user.clone(),
        port: profile.port,
    }
}

fn validate_profile_fields(name: &str, host: &str, user: &str) -> Result<(), DesktopHostError> {
    if name.trim().is_empty() || host.trim().is_empty() || user.trim().is_empty() {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            "profile requires non-empty name, host, and user.",
        )));
    }
    Ok(())
}

fn non_empty(value: String, name: &str) -> Result<String, DesktopHostError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            format!("{name} cannot be empty."),
        )));
    }
    Ok(value)
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_sidebar_level(value: &str) -> Result<String, DesktopHostError> {
    let value = value.trim().to_ascii_lowercase();
    match value.as_str() {
        "info" | "progress" | "success" | "warning" | "error" => Ok(value),
        other => Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            format!("Unsupported sidebar level '{other}'."),
        ))),
    }
}

fn resolve_workspace_id(
    store: &SqliteStore,
    requested: Option<&str>,
) -> Result<String, DesktopHostError> {
    resolve_optional_workspace_id(store, requested)?.ok_or_else(|| {
        DesktopHostError::Control(ControlError::new(
            ErrorCode::WorkspaceNotFound,
            "No workspace is available.",
        ))
    })
}

fn resolve_optional_workspace_id(
    store: &SqliteStore,
    requested: Option<&str>,
) -> Result<Option<String>, DesktopHostError> {
    if let Some(workspace_id) = requested.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    }) {
        if store.load_workspace_bundle(workspace_id)?.is_none() {
            return Err(workspace_not_found(workspace_id).into());
        }
        return Ok(Some(workspace_id.to_string()));
    }
    Ok(store
        .list_workspaces()?
        .first()
        .map(|workspace| workspace.workspace_id.clone()))
}

fn persisted_session_summary(session: &PersistedSession) -> SessionSummaryResult {
    SessionSummaryResult {
        session_id: session.session_id.clone(),
        workspace_id: session.workspace_id.clone(),
        backend_kind: session.backend_kind.clone(),
        state: session.state.clone(),
        exit_code: session.exit_code,
        backend_native_id: session.backend_native_id.clone(),
        cwd: session.cwd.clone(),
    }
}

fn persisted_agent_state_from_result(
    state: &AgentStateResult,
    fallback_updated_at: &str,
) -> PersistedAgentState {
    PersistedAgentState {
        session_id: state.session_id.clone(),
        workspace_id: state.workspace_id.clone(),
        state: state.state.clone(),
        attention: state.attention,
        reason: state.reason.clone(),
        updated_at: state
            .updated_at
            .clone()
            .unwrap_or_else(|| fallback_updated_at.to_string()),
        telemetry_json: state
            .telemetry
            .as_ref()
            .filter(|telemetry| !telemetry.is_empty())
            .and_then(|telemetry| serde_json::to_string(telemetry).ok()),
    }
}

fn agent_state_result_from_persisted(state: &PersistedAgentState) -> AgentStateResult {
    AgentStateResult {
        session_id: state.session_id.clone(),
        workspace_id: state.workspace_id.clone(),
        state: state.state.clone(),
        attention: state.attention,
        reason: state.reason.clone(),
        updated_at: Some(state.updated_at.clone()),
        telemetry: state
            .telemetry_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<AgentTelemetry>(json).ok()),
    }
}

fn persisted_notification_from_result(
    notification: &NotificationSummaryResult,
) -> PersistedNotification {
    PersistedNotification {
        notification_id: notification.notification_id.clone(),
        notification_type: notification.notification_type.clone(),
        severity: notification.severity.clone(),
        workspace_id: notification.workspace_id.clone(),
        session_id: notification.session_id.clone(),
        title: notification.title.clone(),
        message: notification.message.clone(),
        created_at: notification.created_at.clone(),
        dismissed: notification.dismissed,
    }
}

fn sidebar_status_result(status: &PersistedSidebarStatus) -> SidebarStatusResult {
    SidebarStatusResult {
        workspace_id: status.workspace_id.clone(),
        key: status.key.clone(),
        label: status.label.clone(),
        icon: status.icon.clone(),
        color: status.color.clone(),
        priority: status.priority,
        updated_at: status.updated_at.clone(),
    }
}

fn sidebar_progress_result(progress: &PersistedSidebarProgress) -> SidebarProgressResult {
    SidebarProgressResult {
        workspace_id: progress.workspace_id.clone(),
        value: progress.value,
        label: progress.label.clone(),
        updated_at: progress.updated_at.clone(),
    }
}

fn sidebar_log_result(log: &PersistedSidebarLog) -> SidebarLogResult {
    SidebarLogResult {
        log_id: log.log_id.clone(),
        workspace_id: log.workspace_id.clone(),
        level: log.level.clone(),
        source: log.source.clone(),
        message: log.message.clone(),
        created_at: log.created_at.clone(),
    }
}

fn team_task_result(task: &PersistedTeamTask, dependencies: &[(String, String)]) -> TeamTaskResult {
    TeamTaskResult {
        task_id: task.task_id.clone(),
        workspace_id: task.workspace_id.clone(),
        title: task.title.clone(),
        description: task.description.clone(),
        status: task.status.clone(),
        assigned_session_id: task.assigned_session_id.clone(),
        blocked_reason: task.blocked_reason.clone(),
        depends_on: dependencies
            .iter()
            .filter(|(task_id, _)| task_id == &task.task_id)
            .map(|(_, depends_on)| depends_on.clone())
            .collect(),
        created_at: task.created_at.clone(),
        updated_at: task.updated_at.clone(),
        completed_at: task.completed_at.clone(),
    }
}

fn team_task_results_from_store(
    store: &SqliteStore,
    workspace_id: Option<&str>,
) -> Result<Vec<TeamTaskResult>, DesktopHostError> {
    let tasks = store.list_team_tasks(workspace_id)?;
    let dependencies = store.list_team_task_dependencies(workspace_id)?;
    Ok(tasks
        .iter()
        .map(|task| team_task_result(task, &dependencies))
        .collect())
}

fn team_message_result(message: &PersistedTeamMessage) -> TeamMessageResult {
    TeamMessageResult {
        message_id: message.message_id.clone(),
        workspace_id: message.workspace_id.clone(),
        thread_id: message.thread_id.clone(),
        from_session_id: message.from_session_id.clone(),
        to_session_id: message.to_session_id.clone(),
        body: message.body.clone(),
        kind: message.kind.clone(),
        created_at: message.created_at.clone(),
        read_at: message.read_at.clone(),
    }
}

fn load_team_task_or_not_found(
    store: &SqliteStore,
    task_id: &str,
) -> Result<PersistedTeamTask, DesktopHostError> {
    store.load_team_task(task_id)?.ok_or_else(|| {
        DesktopHostError::Control(ControlError::new(
            ErrorCode::InvalidRequest,
            format!("Team task '{task_id}' was not found."),
        ))
    })
}

fn team_dependencies_satisfied(
    store: &SqliteStore,
    depends_on: &[String],
) -> Result<bool, DesktopHostError> {
    for task_id in depends_on {
        let task = load_team_task_or_not_found(store, task_id)?;
        if task.status != "completed" {
            return Ok(false);
        }
    }
    Ok(true)
}

fn unblock_dependency_ready_tasks(
    store: &mut SqliteStore,
    workspace_id: &str,
    updated_at: &str,
) -> Result<(), DesktopHostError> {
    let tasks = store.list_team_tasks(Some(workspace_id))?;
    let dependencies = store.list_team_task_dependencies(Some(workspace_id))?;
    let completed: HashSet<&str> = tasks
        .iter()
        .filter(|task| task.status == "completed")
        .map(|task| task.task_id.as_str())
        .collect();
    for task in tasks.iter().filter(|task| {
        task.status == "blocked" && task.blocked_reason.as_deref() == Some("waiting_on_dependency")
    }) {
        let ready = dependencies
            .iter()
            .filter(|(task_id, _)| task_id == &task.task_id)
            .all(|(_, depends_on)| completed.contains(depends_on.as_str()));
        if ready {
            store.set_team_task_status(&task.task_id, "ready", None, None, None, updated_at)?;
        }
    }
    Ok(())
}

fn sidebar_state_result(
    store: &SqliteStore,
    workspace_id: &str,
) -> Result<SidebarStateResult, DesktopHostError> {
    let bundle = store
        .load_workspace_bundle(workspace_id)?
        .ok_or_else(|| workspace_not_found(workspace_id))?;
    let cwd = bundle
        .sessions
        .iter()
        .find(|session| Some(session.session_id.as_str()) == active_session_id(&bundle).as_deref())
        .and_then(|session| session.cwd.clone())
        .or(bundle.workspace.project_root);
    let (git_branch, git_hash) = git_status_for_cwd(cwd.as_deref());
    Ok(SidebarStateResult {
        workspace_id: workspace_id.to_string(),
        cwd,
        git_branch,
        git_hash,
        ports: Vec::new(),
        statuses: store
            .list_sidebar_status(workspace_id)?
            .iter()
            .map(sidebar_status_result)
            .collect(),
        progress: store
            .load_sidebar_progress(workspace_id)?
            .as_ref()
            .map(sidebar_progress_result),
        logs: store
            .list_sidebar_logs(workspace_id, Some(8))?
            .iter()
            .map(sidebar_log_result)
            .collect(),
    })
}

fn git_status_for_cwd(cwd: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(cwd) = cwd.map(str::trim).filter(|value| !value.is_empty()) else {
        return (None, None);
    };
    // OSC 7 from a WSL shell reports a Linux path; native git needs a Windows
    // path, so translate /mnt/<drive>/… → <DRIVE>:\… (the common project-on-D:
    // case). Other paths pass through unchanged.
    let cwd = translate_wsl_path(cwd);
    let cwd = cwd.as_str();
    if let Some(cached) = cached_git_status(cwd) {
        return cached;
    }
    let branch = git_output(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).map(|value| {
        if value == "HEAD" {
            "detached".to_string()
        } else {
            value
        }
    });
    let hash = git_output(cwd, &["rev-parse", "--short", "HEAD"]);
    store_git_status(cwd, branch.clone(), hash.clone());
    (branch, hash)
}

fn git_status_cache() -> &'static Mutex<HashMap<String, GitStatusCacheEntry>> {
    GIT_STATUS_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached_git_status(cwd: &str) -> Option<(Option<String>, Option<String>)> {
    let Ok(cache) = git_status_cache().lock() else {
        return None;
    };
    let entry = cache.get(cwd)?;
    if entry.captured_at.elapsed() > GIT_STATUS_CACHE_TTL {
        return None;
    }
    Some((entry.branch.clone(), entry.hash.clone()))
}

fn store_git_status(cwd: &str, branch: Option<String>, hash: Option<String>) {
    let Ok(mut cache) = git_status_cache().lock() else {
        return;
    };
    cache.insert(
        cwd.to_string(),
        GitStatusCacheEntry {
            captured_at: Instant::now(),
            branch,
            hash,
        },
    );
}

/// Translate a WSL `/mnt/<drive>/…` path into a Windows path (`D:\…`) so native
/// git can resolve it. Paths that aren't under /mnt (pure WSL-filesystem paths)
/// and already-Windows paths are returned unchanged.
fn translate_wsl_path(path: &str) -> String {
    let Some(rest) = path.strip_prefix("/mnt/") else {
        return path.to_string();
    };
    let mut chars = rest.chars();
    let Some(drive) = chars.next() else {
        return path.to_string();
    };
    let after = chars.as_str();
    if !drive.is_ascii_alphabetic() || !(after.is_empty() || after.starts_with('/')) {
        return path.to_string();
    }
    let win_rest = if after.is_empty() {
        "\\".to_string()
    } else {
        after.replace('/', "\\")
    };
    format!("{}:{}", drive.to_ascii_uppercase(), win_rest)
}

fn is_host_process_working_dir(path: &str) -> bool {
    let path = translate_wsl_path(path.trim());
    let Ok(current_dir) = env::current_dir() else {
        return false;
    };
    paths_equivalent(Path::new(&path), &current_dir)
}

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    normalized_path_text(&left) == normalized_path_text(&right)
}

fn normalized_path_text(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn is_probably_home_directory(path: &str) -> bool {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return false;
    }
    if let Some(home) = default_windows_shell_cwd() {
        let translated = translate_wsl_path(trimmed);
        if paths_equivalent(Path::new(&translated), Path::new(&home)) {
            return true;
        }
    }

    let normalized = trimmed
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase();
    if normalized == "~" {
        return true;
    }
    // Try POSIX `USER` first (WSL/Linux), then fall back to Windows `USERNAME`.
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .ok()
        .map(|user| normalized == format!("/home/{}", user.to_ascii_lowercase()))
        .unwrap_or(false)
}

fn default_windows_shell_cwd() -> Option<String> {
    env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(
            || match (env::var_os("HOMEDRIVE"), env::var_os("HOMEPATH")) {
                (Some(drive), Some(path)) => {
                    let mut home = PathBuf::from(drive);
                    home.push(path);
                    Some(home)
                }
                _ => None,
            },
        )
        .map(|path| path.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
}

fn git_output(cwd: &str, args: &[&str]) -> Option<String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(cwd).args(args);
    hide_console_window(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn active_session_id(bundle: &WorkspaceBundle) -> Option<String> {
    let active_pane = bundle
        .panes
        .iter()
        .find(|pane| pane.pane_id == bundle.workspace.active_pane_id)?;
    let surface_id = active_pane.mounted_surface_id.as_deref()?;
    bundle
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == surface_id)?
        .session_id
        .clone()
}

fn notification_result_from_persisted(
    notification: &PersistedNotification,
) -> NotificationSummaryResult {
    NotificationSummaryResult {
        notification_id: notification.notification_id.clone(),
        notification_type: notification.notification_type.clone(),
        severity: notification.severity.clone(),
        workspace_id: notification.workspace_id.clone(),
        session_id: notification.session_id.clone(),
        title: notification.title.clone(),
        message: notification.message.clone(),
        created_at: notification.created_at.clone(),
        dismissed: notification.dismissed,
    }
}

fn browser_diagnostic_result_from_record(record: &BrowserFailureRecord) -> BrowserDiagnosticResult {
    BrowserDiagnosticResult {
        surface_id: record.surface_id.clone(),
        workspace_id: record.workspace_id.clone(),
        operation: record.operation.clone(),
        code: record.code.clone(),
        message: record.message.clone(),
        occurred_at: record.occurred_at.clone(),
    }
}

fn browser_failure_notification(
    record: &BrowserFailureRecord,
    sequence: u64,
) -> PersistedNotification {
    PersistedNotification {
        notification_id: format!("not_browser_failure_{sequence}"),
        notification_type: "browser.action_failed".to_string(),
        severity: "error".to_string(),
        workspace_id: record.workspace_id.clone(),
        session_id: None,
        title: "Browser action failed".to_string(),
        message: format!("{}: {}", record.operation, record.message),
        created_at: record.occurred_at.clone(),
        dismissed: false,
    }
}

fn desktop_notification_type_enabled(notification: &NotificationSummaryResult) -> bool {
    matches!(
        notification.notification_type.as_str(),
        "agent.needs_input"
            | "agent.completed"
            | "agent.failed"
            | "browser.action_failed"
            | "cli.notification"
    )
}

fn desktop_notification_from_summary(
    notification: &NotificationSummaryResult,
) -> DesktopNotification {
    DesktopNotification {
        notification_id: notification.notification_id.clone(),
        notification_type: notification.notification_type.clone(),
        severity: notification.severity.clone(),
        title: notification.title.clone(),
        body: notification.message.clone(),
    }
}

fn recovery_diagnostics(snapshot: RecoverySnapshot) -> RecoveryDiagnosticsResult {
    RecoveryDiagnosticsResult {
        workspace_count: snapshot.workspaces.len(),
        pane_count: snapshot.panes.len(),
        surface_count: snapshot.surfaces.len(),
        session_count: snapshot.sessions.len(),
        sessions: snapshot
            .sessions
            .into_iter()
            .map(|session| RecoverySessionResult {
                session_id: session.session_id,
                workspace_id: session.workspace_id,
                backend_kind: session.backend_kind,
                state: session.state,
                durability: session.durability,
                backend_native_id: session.backend_native_id,
            })
            .collect(),
    }
}

fn backend_health_from_recovery(
    recovery: &RecoveryDiagnosticsResult,
) -> Vec<DiagnosticsBackendHealthResult> {
    let mut by_backend = HashMap::<String, DiagnosticsBackendHealthResult>::new();
    for session in &recovery.sessions {
        let entry = by_backend
            .entry(session.backend_kind.clone())
            .or_insert_with(|| DiagnosticsBackendHealthResult {
                backend_kind: session.backend_kind.clone(),
                health: "healthy".to_string(),
                active_sessions: 0,
                recovering_sessions: 0,
                failed_sessions: 0,
            });

        if session.state == "recovering" {
            entry.recovering_sessions += 1;
        } else if matches!(session.state.as_str(), "failed" | "lost" | "disconnected") {
            entry.failed_sessions += 1;
        } else if !is_terminal_state(&session.state) {
            entry.active_sessions += 1;
        }
    }

    let mut health = by_backend.into_values().collect::<Vec<_>>();
    for backend in &mut health {
        backend.health = if backend.failed_sessions > 0 {
            "degraded".to_string()
        } else if backend.recovering_sessions > 0 {
            "recovering".to_string()
        } else {
            "healthy".to_string()
        };
    }
    health.sort_by(|left, right| left.backend_kind.cmp(&right.backend_kind));
    health
}

fn queue_pressure_result(
    queue: &str,
    depth: usize,
    capacity: usize,
    dropped_count: usize,
) -> DiagnosticsQueuePressureResult {
    let state = if capacity == 0 {
        "unknown"
    } else if depth >= capacity {
        "full"
    } else if depth.saturating_mul(4) >= capacity.saturating_mul(3) || dropped_count > 0 {
        "pressure"
    } else {
        "nominal"
    };
    DiagnosticsQueuePressureResult {
        queue: queue.to_string(),
        depth,
        capacity,
        dropped_count,
        state: state.to_string(),
    }
}

fn should_attach_recovering_session(session: &PersistedSession) -> bool {
    session.durability == "durable"
        && session.state == "recovering"
        && session.backend_kind == "wsl-tmux-control"
        && session.backend_native_id.is_some()
}

fn workspace_backend_profiles(snapshot: &RecoverySnapshot) -> HashMap<String, Option<String>> {
    snapshot
        .workspaces
        .iter()
        .map(|workspace| {
            (
                workspace.workspace_id.clone(),
                workspace
                    .default_wsl_distribution
                    .clone()
                    .or_else(|| workspace.environment_profile_id.clone()),
            )
        })
        .collect()
}

fn mounted_terminal_surfaces_by_session(
    snapshot: &RecoverySnapshot,
) -> HashMap<String, (String, String)> {
    let terminal_surfaces = snapshot
        .surfaces
        .iter()
        .filter(|surface| surface.surface_type == "terminal")
        .filter_map(|surface| {
            surface
                .session_id
                .as_ref()
                .map(|session_id| (surface.surface_id.as_str(), session_id.as_str()))
        })
        .collect::<HashMap<_, _>>();
    let mut mounted = HashMap::new();
    for pane in &snapshot.panes {
        let Some(surface_id) = pane.mounted_surface_id.as_deref() else {
            continue;
        };
        let Some(session_id) = terminal_surfaces.get(surface_id) else {
            continue;
        };
        mounted.insert(
            (*session_id).to_string(),
            (pane.pane_id.clone(), surface_id.to_string()),
        );
    }
    mounted
}

fn should_restore_agent_state(state: &PersistedAgentState) -> bool {
    if !matches!(
        state.state.as_str(),
        "running" | "waiting_for_input" | "idle"
    ) {
        return false;
    }

    if let Some(activity) = state
        .telemetry_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<AgentTelemetry>(json).ok())
        .and_then(|telemetry| telemetry.activity)
    {
        return matches!(activity.as_str(), "agent" | "agent_team")
            || activity.starts_with("agent.");
    }

    state.reason.as_deref().is_some_and(|reason| {
        reason.starts_with("Agent started:") || reason.starts_with("Agent restored:")
    })
}

fn restored_agent_launch_line(
    session: &PersistedSession,
    state: &PersistedAgentState,
) -> Option<String> {
    if persisted_command_already_launches_agent(&session.command) {
        return None;
    }
    normalized_restored_agent_command_label(state)
}

fn restored_spawn_command_for_session(
    session: &PersistedSession,
    state: Option<&PersistedAgentState>,
) -> Vec<String> {
    if !persisted_command_already_launches_agent(&session.command) {
        return session.command.clone();
    }
    state
        .and_then(normalized_restored_agent_command_label)
        .or_else(|| normalize_restored_agent_launch(&join_command_tokens(&session.command)))
        .and_then(|command_line| split_command_line(&command_line))
        .filter(|command| !command.is_empty())
        .unwrap_or_else(|| session.command.clone())
}

fn normalized_restored_agent_command_label(state: &PersistedAgentState) -> Option<String> {
    let command_line = restored_agent_command_label(state)?;
    normalize_restored_agent_launch(&command_line)
}

fn restored_agent_command_label(state: &PersistedAgentState) -> Option<String> {
    state
        .telemetry_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<AgentTelemetry>(json).ok())
        .and_then(|telemetry| telemetry.session)
        .or_else(|| {
            state
                .reason
                .as_deref()
                .and_then(|reason| reason.strip_prefix("Agent started:"))
                .or_else(|| {
                    state
                        .reason
                        .as_deref()
                        .and_then(|reason| reason.strip_prefix("Agent restored:"))
                })
                .map(str::trim)
                .map(ToString::to_string)
        })
}

fn persisted_command_already_launches_agent(command: &[String]) -> bool {
    command
        .iter()
        .any(|part| is_known_agent_launch(part.trim()))
}

fn is_known_agent_launch(command_line: &str) -> bool {
    first_command_word(command_line)
        .map(agent_command_name)
        .is_some_and(|name| matches!(name.as_str(), "claude" | "codex"))
}

fn normalize_restored_agent_launch(command_line: &str) -> Option<String> {
    let command_line = command_line.trim();
    if command_line.is_empty() || command_line.chars().any(|ch| ch.is_control() && ch != '\t') {
        return None;
    }
    let tokens = split_command_line(command_line)?;
    let launcher = tokens.first().map(|word| agent_command_name(word))?;
    match launcher.as_str() {
        "claude" => Some(normalize_claude_restore_args(&tokens)),
        "codex" => Some(normalize_codex_restore_args(&tokens)),
        _ => None,
    }
}

fn normalize_claude_restore_args(tokens: &[String]) -> String {
    let mut restored = vec!["claude".to_string()];
    let mut index = 1;
    while index < tokens.len() {
        let token = tokens[index].as_str();
        if is_resume_selector_flag(token) {
            index += 1;
            if index < tokens.len() && !tokens[index].starts_with('-') {
                index += 1;
            }
            continue;
        }
        if is_resume_selector_assignment(token) {
            index += 1;
            continue;
        }
        restored.push(tokens[index].clone());
        index += 1;
    }
    join_command_tokens(&restored)
}

fn normalize_codex_restore_args(tokens: &[String]) -> String {
    let mut restored = vec!["codex".to_string()];
    let mut index = 1;
    let mut resume_index = None;
    let has_no_alt_screen = tokens.iter().any(|token| token == "--no-alt-screen");
    while index < tokens.len() {
        let token = tokens[index].as_str();
        if token.eq_ignore_ascii_case("resume") || token.eq_ignore_ascii_case("continue") {
            resume_index = Some(index);
            break;
        }
        if token.starts_with('-') {
            restored.push(tokens[index].clone());
            index += 1;
            if codex_option_takes_value(token) && index < tokens.len() {
                restored.push(tokens[index].clone());
                index += 1;
            }
        } else {
            // A plain `codex "prompt"` launch creates a new session. On app
            // restore, resume the most recent saved session instead of replaying
            // the initial prompt and creating another conversation.
            break;
        }
    }
    if !has_no_alt_screen {
        restored.push("--no-alt-screen".to_string());
    }
    restored.push("resume".to_string());

    if let Some(mut index) = resume_index.map(|value| value + 1) {
        let mut has_selector = false;
        let mut has_last = false;
        while index < tokens.len() {
            let token = tokens[index].as_str();
            if token == "--last" {
                has_last = true;
                restored.push(tokens[index].clone());
                index += 1;
                continue;
            }
            if token.starts_with('-') {
                restored.push(tokens[index].clone());
                index += 1;
                if codex_option_takes_value(token) && index < tokens.len() {
                    restored.push(tokens[index].clone());
                    index += 1;
                }
                continue;
            }
            if !has_selector {
                restored.push(tokens[index].clone());
                has_selector = true;
            }
            index += 1;
        }
        if !has_selector && !has_last {
            restored.push("--last".to_string());
        }
    } else {
        restored.push("--last".to_string());
    }

    join_command_tokens(&restored)
}

fn codex_option_takes_value(token: &str) -> bool {
    if token.contains('=') {
        return false;
    }
    matches!(
        token,
        "-c" | "--config"
            | "--remote"
            | "--remote-auth-token-env"
            | "-i"
            | "--image"
            | "-m"
            | "--model"
            | "--local-provider"
            | "-p"
            | "--profile"
            | "-s"
            | "--sandbox"
            | "-C"
            | "--cd"
            | "--add-dir"
            | "-a"
            | "--ask-for-approval"
    )
}

fn is_resume_selector_flag(token: &str) -> bool {
    matches!(token, "--resume" | "-r" | "--continue" | "-c")
}

fn is_resume_selector_assignment(token: &str) -> bool {
    token.starts_with("--resume=") || token.starts_with("--continue=")
}

fn split_command_line(command_line: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in command_line.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if quote == Some('"') && ch == '\\' {
            escaped = true;
            continue;
        }
        if quote.is_some_and(|value| value == ch) {
            quote = None;
            continue;
        }
        if quote.is_none() && matches!(ch, '"' | '\'') {
            quote = Some(ch);
            continue;
        }
        if quote.is_none() && ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
    }

    if escaped {
        current.push('\\');
    }
    if quote.is_some() {
        return None;
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Some(tokens)
}

fn join_command_tokens(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|token| quote_command_token(token))
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote_command_token(token: &str) -> String {
    if token.is_empty() || token.chars().any(|ch| ch.is_whitespace()) {
        format!("\"{}\"", token.replace('"', "\\\""))
    } else {
        token.to_string()
    }
}

fn first_command_word(command_line: &str) -> Option<&str> {
    let command_line = command_line.trim();
    if command_line.is_empty() {
        return None;
    }
    if let Some(rest) = command_line.strip_prefix('"') {
        return rest
            .find('"')
            .map(|index| &rest[..index])
            .filter(|value| !value.is_empty());
    }
    if let Some(rest) = command_line.strip_prefix('\'') {
        return rest
            .find('\'')
            .map(|index| &rest[..index])
            .filter(|value| !value.is_empty());
    }
    command_line.split_whitespace().next()
}

fn agent_command_name(word: &str) -> String {
    let file_name = word
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(word)
        .trim_matches(|ch| ch == '"' || ch == '\'');
    file_name
        .strip_suffix(".exe")
        .unwrap_or(file_name)
        .to_ascii_lowercase()
}

/// The numeric sequence embedded in a domain id like `pane_00000042` -> 42.
/// Ids are minted as `{prefix}_{value:08}`, so the value is the final
/// underscore-separated segment. Unparseable ids contribute 0.
fn id_sequence(id: &str) -> u64 {
    id.rsplit('_')
        .next()
        .and_then(|suffix| suffix.parse::<u64>().ok())
        .unwrap_or(0)
}

fn workspace_has_active_sessions(
    control: &mut DesktopRuntimeControl,
    bundle: &WorkspaceBundle,
    token: &str,
) -> Result<bool, DesktopHostError> {
    for session in &bundle.sessions {
        if let Some(summary) = try_session_summary(control, &session.session_id, token)? {
            if !is_terminal_state(&summary.state) {
                return Ok(true);
            }
            continue;
        }

        if session.durability == "durable" && !is_terminal_state(&session.state) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn session_is_active(
    control: &mut DesktopRuntimeControl,
    bundle: &WorkspaceBundle,
    session_id: &str,
    token: &str,
) -> Result<bool, DesktopHostError> {
    if let Some(summary) = try_session_summary(control, session_id, token)? {
        return Ok(!is_terminal_state(&summary.state));
    }

    let Some(session) = bundle
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
    else {
        return Ok(false);
    };
    Ok(session.durability == "durable" && !is_terminal_state(&session.state))
}

fn close_live_workspace_sessions(
    control: &mut DesktopRuntimeControl,
    bundle: &WorkspaceBundle,
    mode: &str,
    token: &str,
) -> Result<(), DesktopHostError> {
    for session in &bundle.sessions {
        let Some(summary) = try_session_summary(control, &session.session_id, token)? else {
            if mode == "kill"
                && session.durability == "durable"
                && !is_terminal_state(&session.state)
            {
                return Err(DesktopHostError::Control(ControlError::new(
                    ErrorCode::Conflict,
                    format!(
                        "Durable session '{}' is not attached to the desktop runtime.",
                        session.session_id
                    ),
                )));
            }
            continue;
        };

        if !is_terminal_state(&summary.state) {
            terminate_live_session(control, &session.session_id, mode, token)?;
        }
    }

    Ok(())
}

fn terminate_live_session(
    control: &mut DesktopRuntimeControl,
    session_id: &str,
    mode: &str,
    token: &str,
) -> Result<(), DesktopHostError> {
    let params_json = serde_json::json!({
        "session_id": session_id,
        "mode": mode,
    })
    .to_string();
    let response = control.handle_request(RequestEnvelope::new(
        "desktop_workspace_close_session_terminate",
        "session.terminate",
        params_json,
        token,
    ));

    match response.outcome {
        ResponseOutcome::Ok { .. } => Ok(()),
        ResponseOutcome::Error(error) if error.code == ErrorCode::SessionNotFound => Ok(()),
        ResponseOutcome::Error(error) => Err(error.into()),
    }
}

fn is_terminal_state(state: &str) -> bool {
    matches!(state, "exited" | "failed" | "lost" | "disconnected")
}

fn timestamp() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}Z", duration.as_secs(), duration.subsec_millis())
}

fn unique_time_id() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn workspace_group_id() -> String {
    format!("wsg_{}", unique_time_id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentmux_ipc::ResponseOutcome;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn desktop_control_state_is_shareable() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DesktopControlState>();
    }

    #[test]
    fn read_only_runtime_requests_skip_pre_dispatch_collect() {
        assert!(!runtime_request_needs_pre_dispatch_collect(
            "session.snapshot"
        ));
        assert!(!runtime_request_needs_pre_dispatch_collect("session.get"));
        assert!(!runtime_request_needs_pre_dispatch_collect(
            "notification.list"
        ));
        assert!(runtime_request_needs_pre_dispatch_collect(
            "session.send_text"
        ));
        assert!(runtime_request_needs_pre_dispatch_collect("session.spawn"));
    }

    #[test]
    fn translate_wsl_path_maps_mnt_to_windows_drive() {
        assert_eq!(
            translate_wsl_path("/mnt/d/workspace/agentmux"),
            "D:\\workspace\\agentmux"
        );
        assert_eq!(translate_wsl_path("/mnt/c"), "C:\\");
        // Non-/mnt and already-Windows paths pass through unchanged.
        assert_eq!(translate_wsl_path("/home/dev/project"), "/home/dev/project");
        assert_eq!(
            translate_wsl_path("D:\\already\\windows"),
            "D:\\already\\windows"
        );
    }

    #[test]
    fn agentmux_control_rejects_invalid_token() {
        let state = DesktopControlState::new();
        let response = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_bad_auth",
                "session.get",
                r#"{"session_id":"ses_missing"}"#,
                "wrong-token",
            ),
        );

        assert!(matches!(
            response.outcome,
            ResponseOutcome::Error(ControlError {
                code: ErrorCode::Unauthorized,
                ..
            })
        ));
    }

    #[test]
    fn desktop_control_state_accepts_configured_token() {
        let state = DesktopControlState::new_in_memory_with_token("configured-token").unwrap();
        let bad = agentmux_control(
            &state,
            RequestEnvelope::new("req_bad", "workspace.list", "{}", DESKTOP_CONTROL_TOKEN),
        );
        assert_eq!(response_error_code(&bad), ErrorCode::Unauthorized);

        let good = agentmux_control(
            &state,
            RequestEnvelope::new("req_good", "workspace.list", "{}", "configured-token"),
        );
        assert!(response_value(&good)["workspaces"].is_array());
    }

    #[test]
    fn load_or_create_control_token_persists_random_token() {
        let path = unique_temp_db_path("control-token").with_extension("token");
        let token = load_or_create_control_token(&path).unwrap();
        assert_eq!(token.len(), 64);
        assert_ne!(token, DESKTOP_CONTROL_TOKEN);

        let reread = load_or_create_control_token(&path).unwrap();
        assert_eq!(reread, token);

        cleanup_temp_db(&path);
    }

    #[test]
    fn desktop_config_update_persists_appearance_settings() {
        let store_path = unique_temp_db_path("desktop-config-store");
        let config_path = unique_temp_db_path("desktop-config").with_extension("json");
        let state = DesktopControlState::open_with_token_and_config(
            &store_path,
            "configured-token",
            &config_path,
        )
        .unwrap();

        let initial = agentmux_control(
            &state,
            RequestEnvelope::new("req_config_get", "config.get", "{}", "configured-token"),
        );
        assert_eq!(response_value(&initial)["appearance"]["theme"], "dark");
        assert_eq!(
            response_value(&initial)["shortcuts"]["bindings"],
            serde_json::json!({})
        );

        let update = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_update",
                "config.update",
                r#"{"appearance":{"theme":"light","accent_key":"blue","font_size":14.5},"shortcuts":{"bindings":{"workspace.new":["ctrl+b","c"],"app.search":null}},"ui":{"terminal_inner_margin":9}}"#,
                "configured-token",
            ),
        );
        let value = response_value(&update);
        assert_eq!(value["appearance"]["theme"], "light");
        assert_eq!(value["appearance"]["accent_key"], "blue");
        assert_eq!(value["appearance"]["font_size"], 14.5);
        assert_eq!(
            value["shortcuts"]["bindings"]["workspace.new"],
            serde_json::json!(["ctrl+b", "c"])
        );
        assert_eq!(
            value["shortcuts"]["bindings"]["app.search"],
            serde_json::Value::Null
        );
        assert_eq!(value["ui"]["terminal_inner_margin"], serde_json::json!(9));

        let state = DesktopControlState::open_with_token_and_config(
            &store_path,
            "configured-token",
            &config_path,
        )
        .unwrap();
        let reloaded = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_reload",
                "config.reload",
                "{}",
                "configured-token",
            ),
        );
        let value = response_value(&reloaded);
        assert_eq!(value["appearance"]["theme"], "light");
        assert_eq!(value["appearance"]["accent_key"], "blue");
        assert_eq!(value["appearance"]["font_size"], 14.5);
        assert_eq!(
            value["shortcuts"]["bindings"]["workspace.new"],
            serde_json::json!(["ctrl+b", "c"])
        );
        assert_eq!(
            value["shortcuts"]["bindings"]["app.search"],
            serde_json::Value::Null
        );
        assert_eq!(value["ui"]["terminal_inner_margin"], serde_json::json!(9));

        cleanup_temp_db(&store_path);
        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn desktop_config_import_export_and_reset_round_trip() {
        let store_path = unique_temp_db_path("desktop-config-import-store");
        let config_path = unique_temp_db_path("desktop-config-import").with_extension("json");
        let state = DesktopControlState::open_with_token_and_config(
            &store_path,
            "configured-token",
            &config_path,
        )
        .unwrap();

        let imported = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_import",
                "config.import",
                serde_json::json!({
                    "scope": "global",
                    "json": r#"{"format_version":"agentmux.config.v1","appearance":{"theme":"light","accent_key":"blue","font_size":14},"shortcuts":{"bindings":{"workspace.new":"ctrl+j"}},"actions":{"custom":[{"id":"custom.openDocs","title":"Open docs","target":"browser","command":["new-tab","https://example.com/docs"],"keywords":["docs"]}]},"ui":{"workspace_plus_action":"terminal.newWsl","text_box_max_lines":5,"terminal_inner_margin":11},"notifications":{"actions":[{"action":"browser.openNewTab","notification_type":"diagnostics.wsl_required","severity":"warning"}]}}"#
                })
                .to_string(),
                "configured-token",
            ),
        );
        let value = response_value(&imported);
        assert_eq!(value["appearance"]["theme"], "light");
        assert_eq!(
            value["ui"]["workspace_plus_action"],
            serde_json::json!("terminal.newWsl")
        );
        assert_eq!(value["ui"]["text_box_max_lines"], serde_json::json!(5));
        assert_eq!(value["ui"]["terminal_inner_margin"], serde_json::json!(11));
        assert_eq!(
            value["notifications"]["actions"][0]["action"],
            serde_json::json!("browser.openNewTab")
        );
        assert_eq!(
            value["actions"]["custom"][0]["command"],
            serde_json::json!(["open", "https://example.com/docs", "new_tab"])
        );

        let exported = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_export",
                "config.export",
                "{}",
                "configured-token",
            ),
        );
        let exported_value = response_value(&exported);
        let exported_json = exported_value["json"].as_str().unwrap();
        assert!(exported_json.contains("\"appearance\""));
        assert!(exported_json.contains("\"terminal_inner_margin\""));
        assert!(!exported_json.contains("config_path"));

        let reset = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_reset",
                "config.reset",
                r#"{"scope":"global"}"#,
                "configured-token",
            ),
        );
        let value = response_value(&reset);
        assert_eq!(value["appearance"]["theme"], "dark");
        assert_eq!(value["shortcuts"]["bindings"], serde_json::json!({}));

        cleanup_temp_db(&store_path);
        let _ = fs::remove_file(config_path);
    }

    #[test]
    fn desktop_config_get_merges_project_shortcut_overrides() {
        let store_path = unique_temp_db_path("desktop-project-config-store");
        let config_path = unique_temp_db_path("desktop-project-config").with_extension("json");
        let project_root = unique_temp_db_path("desktop-project-config-root");
        let project_config_dir = project_root.join(PROJECT_CONFIG_DIR_NAME);
        fs::create_dir_all(&project_config_dir).unwrap();
        let project_config_path = project_config_dir.join(APP_CONFIG_FILE_NAME);
        fs::write(
            &project_config_path,
            r#"{"shortcuts":{"bindings":{"workspace.new":"ctrl+j","app.search":null}},"actions":{"custom":[{"id":"custom.runTests","title":"Run project tests","target":"agent","command":["npm","test"],"keywords":["verify"]}]},"ui":{"workspace_plus_action":"custom.runTests","surface_tab_plus_action":"browser.openNewTab","surface_tab_actions":["pane.splitRight","custom.runTests"],"text_box_max_lines":4,"terminal_inner_margin":6},"notifications":{"actions":[{"action":"browser.openNewTab","label":"Open details","notification_type":"diagnostics.wsl_required","severity":"warning","dismiss_on_run":true}]}}"#,
        )
        .unwrap();

        let state = DesktopControlState::open_with_token_and_config(
            &store_path,
            "configured-token",
            &config_path,
        )
        .unwrap();
        let created = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace",
                "workspace.create",
                serde_json::json!({
                    "name": "Project config",
                    "project_root": project_root.to_string_lossy(),
                    "backend_profile": null
                })
                .to_string(),
                "configured-token",
            ),
        );
        let workspace_id = response_value(&created)["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();

        let update = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_update",
                "config.update",
                r#"{"shortcuts":{"bindings":{"workspace.new":["ctrl+b","c"],"terminal.newWsl":"ctrl+t"}}}"#,
                "configured-token",
            ),
        );
        assert_eq!(
            response_value(&update)["shortcuts"]["bindings"]["workspace.new"],
            serde_json::json!(["ctrl+b", "c"])
        );

        let effective = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_get_project",
                "config.get",
                serde_json::json!({ "workspace_id": workspace_id }).to_string(),
                "configured-token",
            ),
        );
        let value = response_value(&effective);
        assert_eq!(value["project_config_loaded"], true);
        assert_eq!(
            value["project_config_path"],
            serde_json::json!(project_config_path.to_string_lossy())
        );
        assert_eq!(
            value["shortcuts"]["bindings"]["workspace.new"],
            serde_json::json!("ctrl+j")
        );
        assert_eq!(
            value["shortcuts"]["bindings"]["terminal.newWsl"],
            serde_json::json!("ctrl+t")
        );
        assert_eq!(
            value["shortcuts"]["bindings"]["app.search"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["actions"]["custom"][0]["id"],
            serde_json::json!("custom.runTests")
        );
        assert_eq!(
            value["actions"]["custom"][0]["target"],
            serde_json::json!("agent")
        );
        assert_eq!(
            value["actions"]["custom"][0]["command"],
            serde_json::json!(["npm", "test"])
        );
        assert_eq!(
            value["ui"]["workspace_plus_action"],
            serde_json::json!("custom.runTests")
        );
        assert_eq!(
            value["ui"]["surface_tab_plus_action"],
            serde_json::json!("browser.openNewTab")
        );
        assert_eq!(
            value["ui"]["surface_tab_actions"],
            serde_json::json!(["pane.splitRight", "custom.runTests"])
        );
        assert_eq!(value["ui"]["text_box_max_lines"], serde_json::json!(4));
        assert_eq!(value["ui"]["terminal_inner_margin"], serde_json::json!(6));
        assert_eq!(
            value["notifications"]["actions"][0]["action"],
            serde_json::json!("browser.openNewTab")
        );
        assert_eq!(
            value["notifications"]["actions"][0]["label"],
            serde_json::json!("Open details")
        );
        assert_eq!(
            value["notifications"]["actions"][0]["notification_type"],
            serde_json::json!("diagnostics.wsl_required")
        );
        assert_eq!(
            value["notifications"]["actions"][0]["severity"],
            serde_json::json!("warning")
        );
        assert_eq!(
            value["notifications"]["actions"][0]["dismiss_on_run"],
            serde_json::json!(true)
        );

        let actions = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_actions_list_project",
                "actions.list",
                serde_json::json!({ "workspace_id": workspace_id }).to_string(),
                "configured-token",
            ),
        );
        let actions_value = response_value(&actions);
        let actions = actions_value["actions"].as_array().unwrap();
        assert!(actions.iter().any(|action| action["id"]
            == serde_json::json!("browser.openNewTab")
            && action["source"] == serde_json::json!("builtin")));
        assert!(actions.iter().any(
            |action| action["id"] == serde_json::json!("custom.runTests")
                && action["group"] == serde_json::json!("agent")
                && action["source"] == serde_json::json!("custom")
        ));

        let exported_project = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_export_project",
                "config.export",
                serde_json::json!({ "workspace_id": workspace_id, "scope": "project" }).to_string(),
                "configured-token",
            ),
        );
        let exported_json = response_value(&exported_project)["json"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(exported_json.contains("\"custom.runTests\""));
        assert!(exported_json.contains("\"notifications\""));
        assert!(!exported_json.contains("\"appearance\""));

        let reset_project = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_reset_project",
                "config.reset",
                serde_json::json!({ "workspace_id": workspace_id, "scope": "project" }).to_string(),
                "configured-token",
            ),
        );
        let reset_value = response_value(&reset_project);
        assert_eq!(reset_value["project_config_loaded"], false);
        assert_eq!(
            reset_value["shortcuts"]["bindings"]["workspace.new"],
            serde_json::json!(["ctrl+b", "c"])
        );
        assert!(reset_value["actions"]["custom"]
            .as_array()
            .unwrap()
            .is_empty());

        cleanup_temp_db(&store_path);
        let _ = fs::remove_file(config_path);
        let _ = fs::remove_dir_all(project_root);
    }

    #[test]
    fn desktop_actions_run_executes_control_safe_actions() {
        let state = DesktopControlState::new_in_memory_with_token("configured-token").unwrap();
        let created = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace",
                "workspace.create",
                serde_json::json!({
                    "name": "Action workspace",
                    "project_root": null,
                    "backend_profile": null
                })
                .to_string(),
                "configured-token",
            ),
        );
        let workspace_id = response_value(&created)["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();

        let import = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_import_actions",
                "config.import",
                serde_json::json!({
                    "json": r##"{"actions":{"custom":[{"id":"custom.openDocs","title":"Open docs","target":"browser","command":["new-tab","https://example.com/docs"]},{"id":"custom.capture","title":"Capture browser","target":"browser","command":["screenshot","jpeg","active-pane"]},{"id":"custom.waitReady","title":"Wait ready","target":"browser","command":["wait-for-selector","#ready","frame:frame_1","1500"]},{"id":"custom.reloadBrowser","title":"Reload browser","target":"browser","command":["reload"]},{"id":"custom.zoomBrowser","title":"Zoom browser","target":"browser","command":["zoom","125"]},{"id":"custom.fillBrowser","title":"Fill browser","target":"browser","command":["fill","#q","agentmux","frame:frame_1"]},{"id":"custom.highlightBrowser","title":"Highlight browser","target":"browser","command":["highlight","#q","750","frame:frame_1"]}]}}"##
                })
                .to_string(),
                "configured-token",
            ),
        );
        assert!(
            matches!(import.outcome, ResponseOutcome::Ok { .. }),
            "config import failed: {:?}",
            import.outcome
        );

        let browser_actions = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_actions_list_browser_frame",
                "actions.list",
                serde_json::json!({ "workspace_id": workspace_id }).to_string(),
                "configured-token",
            ),
        );
        let browser_actions = response_value(&browser_actions);
        let browser_actions = browser_actions["actions"].as_array().unwrap();
        assert!(browser_actions.iter().any(|action| {
            action["id"] == serde_json::json!("custom.waitReady")
                && action["command"]
                    == serde_json::json!([
                        "wait-for-selector",
                        "#ready",
                        "active_pane",
                        "1500",
                        "frame_1"
                    ])
        }));
        assert!(browser_actions.iter().any(|action| {
            action["id"] == serde_json::json!("custom.fillBrowser")
                && action["command"]
                    == serde_json::json!(["fill", "#q", "agentmux", "active_pane", "frame_1"])
        }));

        let run_workspace = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_actions_run_workspace",
                "actions.run",
                serde_json::json!({ "action_id": "workspace.new" }).to_string(),
                "configured-token",
            ),
        );
        let run_workspace_value = response_value(&run_workspace);
        assert_eq!(run_workspace_value["result_type"], "workspace");
        assert!(run_workspace_value["workspace_id"]
            .as_str()
            .unwrap()
            .starts_with("ws_"));

        let run_browser = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_actions_run_browser",
                "actions.run",
                serde_json::json!({
                    "workspace_id": workspace_id,
                    "action_id": "custom.openDocs"
                })
                .to_string(),
                "configured-token",
            ),
        );
        let run_browser_value = response_value(&run_browser);
        assert_eq!(run_browser_value["action_id"], "custom.openDocs");
        assert_eq!(run_browser_value["result_type"], "surface");
        let surface_id = run_browser_value["surface_id"]
            .as_str()
            .unwrap()
            .to_string();

        let detail = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_detail",
                "workspace.get",
                serde_json::json!({ "workspace_id": workspace_id }).to_string(),
                "configured-token",
            ),
        );
        let detail_value = response_value(&detail);
        assert!(detail_value["surfaces"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |surface| surface["surface_id"] == serde_json::json!(surface_id)
                    && surface["surface_type"] == serde_json::json!("browser")
            ));

        let run_browser_recipe = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_actions_run_browser_recipe",
                "actions.run",
                serde_json::json!({
                    "workspace_id": workspace_id,
                    "action_id": "custom.capture"
                })
                .to_string(),
                "configured-token",
            ),
        );
        let run_browser_recipe_value = response_value(&run_browser_recipe);
        assert_eq!(run_browser_recipe_value["action_id"], "custom.capture");
        assert_eq!(run_browser_recipe_value["result_type"], "browser");
        assert_eq!(
            run_browser_recipe_value["message"],
            serde_json::json!("browser screenshot captured")
        );
        assert!(run_browser_recipe_value["surface_id"].is_string());

        let run_browser_wait = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_actions_run_browser_wait",
                "actions.run",
                serde_json::json!({
                    "workspace_id": workspace_id,
                    "action_id": "custom.waitReady"
                })
                .to_string(),
                "configured-token",
            ),
        );
        let run_browser_wait_value = response_value(&run_browser_wait);
        assert_eq!(run_browser_wait_value["action_id"], "custom.waitReady");
        assert_eq!(run_browser_wait_value["result_type"], "browser");
        assert_eq!(
            run_browser_wait_value["message"],
            serde_json::json!("browser selector appeared")
        );
        assert!(run_browser_wait_value["surface_id"].is_string());

        let run_browser_reload = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_actions_run_browser_reload",
                "actions.run",
                serde_json::json!({
                    "workspace_id": workspace_id,
                    "action_id": "custom.reloadBrowser"
                })
                .to_string(),
                "configured-token",
            ),
        );
        let run_browser_reload_value = response_value(&run_browser_reload);
        assert_eq!(
            run_browser_reload_value["action_id"],
            "custom.reloadBrowser"
        );
        assert_eq!(run_browser_reload_value["result_type"], "browser");
        assert_eq!(
            run_browser_reload_value["message"],
            serde_json::json!("browser navigation executed")
        );
        assert!(run_browser_reload_value["surface_id"].is_string());

        let run_browser_zoom = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_actions_run_browser_zoom",
                "actions.run",
                serde_json::json!({
                    "workspace_id": workspace_id,
                    "action_id": "custom.zoomBrowser"
                })
                .to_string(),
                "configured-token",
            ),
        );
        let run_browser_zoom_value = response_value(&run_browser_zoom);
        assert_eq!(run_browser_zoom_value["action_id"], "custom.zoomBrowser");
        assert_eq!(run_browser_zoom_value["result_type"], "browser");
        assert_eq!(
            run_browser_zoom_value["message"],
            serde_json::json!("browser zoom set")
        );
        assert!(run_browser_zoom_value["surface_id"].is_string());

        let run_browser_fill = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_actions_run_browser_fill",
                "actions.run",
                serde_json::json!({
                    "workspace_id": workspace_id,
                    "action_id": "custom.fillBrowser"
                })
                .to_string(),
                "configured-token",
            ),
        );
        let run_browser_fill_value = response_value(&run_browser_fill);
        assert_eq!(run_browser_fill_value["action_id"], "custom.fillBrowser");
        assert_eq!(run_browser_fill_value["result_type"], "browser");
        assert_eq!(
            run_browser_fill_value["message"],
            serde_json::json!("browser text filled")
        );
        assert!(run_browser_fill_value["surface_id"].is_string());

        let run_browser_highlight = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_actions_run_browser_highlight",
                "actions.run",
                serde_json::json!({
                    "workspace_id": workspace_id,
                    "action_id": "custom.highlightBrowser"
                })
                .to_string(),
                "configured-token",
            ),
        );
        let run_browser_highlight_value = response_value(&run_browser_highlight);
        assert_eq!(
            run_browser_highlight_value["action_id"],
            "custom.highlightBrowser"
        );
        assert_eq!(run_browser_highlight_value["result_type"], "browser");
        assert_eq!(
            run_browser_highlight_value["message"],
            serde_json::json!("browser element highlighted")
        );
        assert!(run_browser_highlight_value["surface_id"].is_string());

        let ui_only = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_actions_run_ui_only",
                "actions.run",
                serde_json::json!({
                    "workspace_id": workspace_id,
                    "action_id": "app.settings"
                })
                .to_string(),
                "configured-token",
            ),
        );
        assert_eq!(response_error_code(&ui_only), ErrorCode::InvalidRequest);

        let capabilities = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_capabilities",
                "system.capabilities",
                "{}",
                "configured-token",
            ),
        );
        let capabilities_value = response_value(&capabilities);
        let methods = capabilities_value["methods"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>();
        assert!(methods.contains(&"actions.run"));
    }

    #[test]
    fn desktop_config_get_reads_cmux_project_config_fallback() {
        let store_path = unique_temp_db_path("desktop-cmux-project-config-store");
        let config_path = unique_temp_db_path("desktop-cmux-project-config").with_extension("json");
        let project_root = unique_temp_db_path("desktop-cmux-project-config-root");
        let cmux_config_dir = project_root.join(CMUX_PROJECT_CONFIG_DIR_NAME);
        fs::create_dir_all(&cmux_config_dir).unwrap();
        let cmux_config_path = cmux_config_dir.join(CMUX_PROJECT_CONFIG_FILE_NAME);
        fs::write(
            &cmux_config_path,
            r#"{"shortcuts":{"bindings":{"workspace.new":"ctrl+shift+n"}},"actions":{"custom":[{"id":"custom.cmuxDocs","title":"Open cmux docs","target":"browser","command":["new-tab","https://cmux.com/ko/docs/getting-started"]}]},"ui":{"surface_tab_plus_action":"custom.cmuxDocs"}}"#,
        )
        .unwrap();

        let state = DesktopControlState::open_with_token_and_config(
            &store_path,
            "configured-token",
            &config_path,
        )
        .unwrap();
        let created = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace",
                "workspace.create",
                serde_json::json!({
                    "name": "cmux config fallback",
                    "project_root": project_root.to_string_lossy(),
                    "backend_profile": null
                })
                .to_string(),
                "configured-token",
            ),
        );
        let workspace_id = response_value(&created)["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();

        let effective = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_get_cmux_project",
                "config.get",
                serde_json::json!({ "workspace_id": workspace_id }).to_string(),
                "configured-token",
            ),
        );
        let value = response_value(&effective);
        assert_eq!(value["project_config_loaded"], true);
        assert_eq!(
            value["project_config_path"],
            serde_json::json!(project_root
                .join(PROJECT_CONFIG_DIR_NAME)
                .join(APP_CONFIG_FILE_NAME)
                .to_string_lossy())
        );
        assert_eq!(
            value["shortcuts"]["bindings"]["workspace.new"],
            serde_json::json!("ctrl+shift+n")
        );
        assert_eq!(
            value["actions"]["custom"][0]["command"],
            serde_json::json!([
                "open",
                "https://cmux.com/ko/docs/getting-started",
                "new_tab"
            ])
        );
        assert_eq!(
            value["ui"]["surface_tab_plus_action"],
            serde_json::json!("custom.cmuxDocs")
        );

        let agentmux_config_dir = project_root.join(PROJECT_CONFIG_DIR_NAME);
        fs::create_dir_all(&agentmux_config_dir).unwrap();
        fs::write(
            agentmux_config_dir.join(APP_CONFIG_FILE_NAME),
            r#"{"shortcuts":{"bindings":{"workspace.new":"ctrl+alt+n"}}}"#,
        )
        .unwrap();

        let overridden = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_get_agentmux_project",
                "config.reload",
                serde_json::json!({ "workspace_id": workspace_id }).to_string(),
                "configured-token",
            ),
        );
        let overridden_value = response_value(&overridden);
        assert_eq!(
            overridden_value["shortcuts"]["bindings"]["workspace.new"],
            serde_json::json!("ctrl+alt+n")
        );
        assert!(overridden_value["actions"]["custom"]
            .as_array()
            .unwrap()
            .is_empty());

        cleanup_temp_db(&store_path);
        let _ = fs::remove_file(config_path);
        let _ = fs::remove_dir_all(project_root);
    }

    #[test]
    fn desktop_dock_get_reads_project_and_global_configs() {
        let store_path = unique_temp_db_path("desktop-dock-store");
        let config_dir = unique_temp_db_path("desktop-dock-config-dir").with_extension("dir");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join(APP_CONFIG_FILE_NAME);
        let state = DesktopControlState::open_with_token_and_config(
            &store_path,
            "configured-token",
            &config_path,
        )
        .unwrap();
        fs::write(
            config_dir.join(DOCK_CONFIG_FILE_NAME),
            r#"{"controls":[{"id":"global","title":"Global","command":"echo global","height":120}]}"#,
        )
        .unwrap();

        let global = agentmux_control(
            &state,
            RequestEnvelope::new("req_dock_global", "dock.get", "{}", "configured-token"),
        );
        let global_value = response_value(&global);
        assert_eq!(global_value["source"], "global_agentmux");
        assert_eq!(global_value["requires_trust"], false);
        assert_eq!(global_value["trusted"], true);
        assert_eq!(global_value["controls"][0]["id"], "global");

        let project_root = unique_temp_db_path("desktop-dock-project-root").with_extension("dir");
        fs::create_dir_all(project_root.join(CMUX_PROJECT_CONFIG_DIR_NAME)).unwrap();
        fs::write(
            project_root
                .join(CMUX_PROJECT_CONFIG_DIR_NAME)
                .join(DOCK_CONFIG_FILE_NAME),
            r#"{"controls":[{"id":"logs","title":"Logs","command":"tail -f ./logs/dev.log","cwd":".","env":{"NO_COLOR":"1"}}]}"#,
        )
        .unwrap();
        let created = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_dock_workspace",
                "workspace.create",
                serde_json::json!({
                    "name": "Dock workspace",
                    "project_root": project_root.to_string_lossy(),
                    "backend_profile": null
                })
                .to_string(),
                "configured-token",
            ),
        );
        let workspace_id = response_value(&created)["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();

        let cmux_project = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_dock_cmux_project",
                "dock.get",
                serde_json::json!({ "workspace_id": workspace_id }).to_string(),
                "configured-token",
            ),
        );
        let cmux_project_value = response_value(&cmux_project);
        assert_eq!(cmux_project_value["source"], "project_cmux");
        assert_eq!(cmux_project_value["requires_trust"], true);
        assert_eq!(cmux_project_value["trusted"], false);
        assert_eq!(cmux_project_value["controls"][0]["id"], "logs");
        assert_eq!(cmux_project_value["controls"][0]["env"]["NO_COLOR"], "1");

        let trusted_cmux_project = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_dock_trust_cmux_project",
                "dock.trust",
                serde_json::json!({ "workspace_id": workspace_id }).to_string(),
                "configured-token",
            ),
        );
        assert_eq!(response_value(&trusted_cmux_project)["trusted"], true);

        fs::write(
            project_root
                .join(CMUX_PROJECT_CONFIG_DIR_NAME)
                .join(DOCK_CONFIG_FILE_NAME),
            r#"{"controls":[{"id":"logs","title":"Logs","command":"tail -f ./logs/changed.log","cwd":".","env":{"NO_COLOR":"1"}}]}"#,
        )
        .unwrap();
        let changed_cmux_project = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_dock_changed_cmux_project",
                "dock.get",
                serde_json::json!({ "workspace_id": workspace_id }).to_string(),
                "configured-token",
            ),
        );
        let changed_cmux_value = response_value(&changed_cmux_project);
        assert_eq!(changed_cmux_value["source"], "project_cmux");
        assert_eq!(changed_cmux_value["trusted"], false);
        assert_eq!(
            changed_cmux_value["controls"][0]["command"],
            "tail -f ./logs/changed.log"
        );

        fs::create_dir_all(project_root.join(PROJECT_CONFIG_DIR_NAME)).unwrap();
        fs::write(
            project_root
                .join(PROJECT_CONFIG_DIR_NAME)
                .join(DOCK_CONFIG_FILE_NAME),
            r#"{"controls":[{"id":"git","title":"Git","command":"lazygit","height":300}]}"#,
        )
        .unwrap();
        let agentmux_project = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_dock_agentmux_project",
                "dock.get",
                serde_json::json!({ "workspace_id": workspace_id }).to_string(),
                "configured-token",
            ),
        );
        let agentmux_project_value = response_value(&agentmux_project);
        assert_eq!(agentmux_project_value["source"], "project_agentmux");
        assert_eq!(agentmux_project_value["trusted"], false);
        assert_eq!(agentmux_project_value["controls"][0]["id"], "git");

        let trusted_agentmux_project = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_dock_trust_agentmux_project",
                "dock.trust",
                serde_json::json!({ "workspace_id": workspace_id }).to_string(),
                "configured-token",
            ),
        );
        let trusted_agentmux_value = response_value(&trusted_agentmux_project);
        assert_eq!(trusted_agentmux_value["source"], "project_agentmux");
        assert_eq!(trusted_agentmux_value["trusted"], true);

        cleanup_temp_db(&store_path);
        let _ = fs::remove_dir_all(config_dir);
        let _ = fs::remove_dir_all(project_root);
    }

    #[test]
    fn desktop_config_migrates_cmux_project_config_to_agentmux_path() {
        let store_path = unique_temp_db_path("desktop-cmux-migrate-store");
        let config_path = unique_temp_db_path("desktop-cmux-migrate").with_extension("json");
        let project_root = unique_temp_db_path("desktop-cmux-migrate-root");
        let cmux_config_dir = project_root.join(CMUX_PROJECT_CONFIG_DIR_NAME);
        fs::create_dir_all(&cmux_config_dir).unwrap();
        let cmux_config_path = cmux_config_dir.join(CMUX_PROJECT_CONFIG_FILE_NAME);
        fs::write(
            &cmux_config_path,
            r#"{"shortcuts":{"bindings":{"workspace.new":"ctrl+shift+m"}},"actions":{"custom":[{"id":"custom.cmuxMigration","title":"Open migrated docs","target":"browser","command":["new-tab","https://cmux.com/ko/docs/configuration"]}]}}"#,
        )
        .unwrap();

        let state = DesktopControlState::open_with_token_and_config(
            &store_path,
            "configured-token",
            &config_path,
        )
        .unwrap();
        let created = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace",
                "workspace.create",
                serde_json::json!({
                    "name": "cmux migration",
                    "project_root": project_root.to_string_lossy(),
                    "backend_profile": null
                })
                .to_string(),
                "configured-token",
            ),
        );
        let workspace_id = response_value(&created)["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();
        let agentmux_config_path = project_root
            .join(PROJECT_CONFIG_DIR_NAME)
            .join(APP_CONFIG_FILE_NAME);

        let migrated = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_migrate",
                "config.migrate_project",
                serde_json::json!({
                    "workspace_id": workspace_id.clone(),
                    "overwrite": false
                })
                .to_string(),
                "configured-token",
            ),
        );
        let migrated_value = response_value(&migrated);
        assert_eq!(
            migrated_value["source_path"],
            serde_json::json!(cmux_config_path.to_string_lossy())
        );
        assert_eq!(
            migrated_value["target_path"],
            serde_json::json!(agentmux_config_path.to_string_lossy())
        );
        assert_eq!(migrated_value["overwritten"], false);
        assert!(agentmux_config_path.is_file());
        assert_eq!(
            migrated_value["config"]["shortcuts"]["bindings"]["workspace.new"],
            serde_json::json!("ctrl+shift+m")
        );
        assert_eq!(
            migrated_value["config"]["actions"]["custom"][0]["command"],
            serde_json::json!(["open", "https://cmux.com/ko/docs/configuration", "new_tab"])
        );

        let refused = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_migrate_again",
                "config.migrate_project",
                serde_json::json!({ "workspace_id": workspace_id.clone() }).to_string(),
                "configured-token",
            ),
        );
        assert_eq!(response_error_code(&refused), ErrorCode::InvalidRequest);

        fs::write(
            &cmux_config_path,
            r#"{"shortcuts":{"bindings":{"workspace.new":"ctrl+alt+m"}}}"#,
        )
        .unwrap();
        let overwritten = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_migrate_overwrite",
                "config.migrate_project",
                serde_json::json!({
                    "workspace_id": workspace_id.clone(),
                    "overwrite": true
                })
                .to_string(),
                "configured-token",
            ),
        );
        let overwritten_value = response_value(&overwritten);
        assert_eq!(overwritten_value["overwritten"], true);
        assert_eq!(
            overwritten_value["config"]["shortcuts"]["bindings"]["workspace.new"],
            serde_json::json!("ctrl+alt+m")
        );

        cleanup_temp_db(&store_path);
        let _ = fs::remove_file(config_path);
        let _ = fs::remove_dir_all(project_root);
    }

    #[test]
    fn desktop_config_diagnostics_reports_invalid_sources_without_loading_them() {
        let store_path = unique_temp_db_path("desktop-config-diagnostics-store");
        let config_path = unique_temp_db_path("desktop-config-diagnostics").with_extension("json");
        fs::write(&config_path, "{not json").unwrap();
        let project_root = unique_temp_db_path("desktop-config-diagnostics-root");
        let cmux_config_dir = project_root.join(CMUX_PROJECT_CONFIG_DIR_NAME);
        fs::create_dir_all(&cmux_config_dir).unwrap();
        fs::write(
            cmux_config_dir.join(CMUX_PROJECT_CONFIG_FILE_NAME),
            "{not json",
        )
        .unwrap();

        let state = DesktopControlState::open_with_token_and_config(
            &store_path,
            "configured-token",
            &config_path,
        )
        .unwrap();
        let created = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace",
                "workspace.create",
                serde_json::json!({
                    "name": "config diagnostics",
                    "project_root": project_root.to_string_lossy(),
                    "backend_profile": null
                })
                .to_string(),
                "configured-token",
            ),
        );
        let workspace_id = response_value(&created)["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();

        let diagnostics = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_diagnostics",
                "config.diagnostics",
                serde_json::json!({ "workspace_id": workspace_id }).to_string(),
                "configured-token",
            ),
        );
        let value = response_value(&diagnostics);
        let entries = value["entries"].as_array().unwrap();
        let global = entries
            .iter()
            .find(|entry| entry["source"] == serde_json::json!("global"))
            .unwrap();
        assert_eq!(global["exists"], true);
        assert_eq!(global["valid"], false);
        assert_eq!(global["active"], true);
        assert!(global["message"]
            .as_str()
            .unwrap()
            .contains("Global config is invalid"));

        let project = entries
            .iter()
            .find(|entry| entry["source"] == serde_json::json!("project"))
            .unwrap();
        assert_eq!(project["exists"], false);
        assert_eq!(project["valid"], true);
        assert_eq!(project["active"], false);

        let cmux_project = entries
            .iter()
            .find(|entry| entry["source"] == serde_json::json!("cmux_project"))
            .unwrap();
        assert_eq!(cmux_project["exists"], true);
        assert_eq!(cmux_project["valid"], false);
        assert_eq!(cmux_project["active"], true);

        cleanup_temp_db(&store_path);
        let _ = fs::remove_file(config_path);
        let _ = fs::remove_dir_all(project_root);
    }

    #[test]
    fn desktop_config_diagnostics_hides_missing_cmux_project_source() {
        let state = DesktopControlState::new_in_memory_with_token("configured-token").unwrap();
        let created = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace",
                "workspace.create",
                r#"{"name":"Config diagnostics","project_root":null,"backend_profile":null}"#,
                "configured-token",
            ),
        );
        let workspace_id = response_value(&created)["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();

        let diagnostics = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_config_diagnostics",
                "config.diagnostics",
                serde_json::json!({ "workspace_id": workspace_id }).to_string(),
                "configured-token",
            ),
        );
        let value = response_value(&diagnostics);
        let entries = value["entries"].as_array().unwrap();
        assert!(entries
            .iter()
            .any(|entry| entry["source"] == serde_json::json!("global")));
        assert!(entries
            .iter()
            .any(|entry| entry["source"] == serde_json::json!("project")));
        assert!(!entries
            .iter()
            .any(|entry| entry["source"] == serde_json::json!("cmux_project")));
    }

    #[test]
    fn desktop_sidebar_metadata_round_trips_through_control_methods() {
        let state = DesktopControlState::new_in_memory_with_token("configured-token").unwrap();
        let created = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace",
                "workspace.create",
                r#"{"name":"Test","project_root":"D:\\work\\repo","backend_profile":null}"#,
                "configured-token",
            ),
        );
        let workspace_id = response_value(&created)["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();

        let status = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_status",
                "sidebar.set_status",
                format!(
                    r##"{{"workspace_id":"{workspace_id}","key":"build","label":"compiling","icon":"hammer","color":"#ff9500","priority":80}}"##
                ),
                "configured-token",
            ),
        );
        assert_eq!(response_value(&status)["label"], "compiling");

        let progress = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_progress",
                "sidebar.set_progress",
                format!(r#"{{"workspace_id":"{workspace_id}","value":0.5,"label":"Building"}}"#),
                "configured-token",
            ),
        );
        assert_eq!(response_value(&progress)["value"], 0.5);

        let log = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_log",
                "sidebar.log",
                format!(
                    r#"{{"workspace_id":"{workspace_id}","level":"success","source":"test","message":"ok"}}"#
                ),
                "configured-token",
            ),
        );
        assert_eq!(response_value(&log)["message"], "ok");

        let state_response = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_sidebar_state",
                "sidebar.state",
                format!(r#"{{"workspace_id":"{workspace_id}"}}"#),
                "configured-token",
            ),
        );
        let value = response_value(&state_response);
        assert_eq!(value["statuses"][0]["key"], "build");
        assert_eq!(value["progress"]["label"], "Building");
        assert_eq!(value["logs"][0]["message"], "ok");

        let identify = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_identify",
                "system.identify",
                format!(r#"{{"workspace_id":"{workspace_id}"}}"#),
                "configured-token",
            ),
        );
        assert_eq!(response_value(&identify)["workspace_id"], workspace_id);
    }

    #[test]
    #[cfg(windows)]
    fn load_or_create_control_token_uses_owner_only_acl_on_windows() {
        let path = unique_temp_db_path("control-token-acl").with_extension("token");
        let token = load_or_create_control_token(&path).unwrap();
        assert_eq!(token.len(), 64);

        let sddl = windows_token_file::file_dacl_sddl(&path).unwrap();
        assert!(
            sddl.starts_with("D:P"),
            "token file DACL should be protected, got {sddl}"
        );
        assert!(
            sddl.contains(";;;OW)") || sddl.contains(";;;S-1-3-4)"),
            "token file should grant access through Owner Rights SID, got {sddl}"
        );
        assert!(
            !sddl.contains(";;;WD)") && !sddl.contains(";;;S-1-1-0)"),
            "token file should not grant Everyone access, got {sddl}"
        );

        cleanup_temp_db(&path);
    }

    #[test]
    #[cfg(windows)]
    fn named_pipe_dispatches_to_desktop_control_state() {
        let state = Arc::new(DesktopControlState::new());
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pipe_name = format!(
            r"\\.\pipe\agentmux-desktop-host-test-{}-{nanos}",
            std::process::id()
        );
        let server_pipe_name = pipe_name.clone();
        let server_state = state.clone();
        let handle = std::thread::spawn(move || {
            agentmux_ipc::serve_one_named_pipe_request(&server_pipe_name, |request| {
                agentmux_control(server_state.as_ref(), request)
            })
            .unwrap();
        });

        let response = agentmux_ipc::send_named_pipe_request(
            &pipe_name,
            &RequestEnvelope::new(
                "req_pipe_workspace_list",
                "workspace.list",
                "{}",
                DESKTOP_CONTROL_TOKEN,
            ),
            std::time::Duration::from_secs(2),
        )
        .unwrap();

        handle.join().unwrap();
        assert_eq!(
            response_value(&response)["workspaces"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn workspace_methods_round_trip_through_desktop_store() {
        let state = DesktopControlState::new();
        let create = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_create",
                "workspace.create",
                r#"{"name":"Demo workspace","project_root":"D:\\Projects\\agentmux","backend_profile":"local"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let created = response_value(&create);
        let workspace_id = created
            .get("workspace_id")
            .and_then(serde_json::Value::as_str)
            .unwrap()
            .to_string();
        assert_eq!(created["name"], "Demo workspace");

        let list = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_list",
                "workspace.list",
                "{}",
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(
            response_value(&list)["workspaces"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let get = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_get",
                "workspace.get",
                format!(r#"{{"workspace_id":"{workspace_id}"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&get)["panes"].as_array().unwrap().len(), 1);

        let rename = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_rename",
                "workspace.rename",
                format!(r#"{{"workspace_id":"{workspace_id}","name":"Renamed"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&rename)["name"], "Renamed");

        let update = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_update",
                "workspace.update",
                format!(
                    r##"{{"workspace_id":"{workspace_id}","name":"Project Alpha","project_root":"D:\\work\\alpha","environment_profile_id":"Ubuntu","description":"demo project","icon":"PA","color":"#22C55E","default_wsl_distribution":"Ubuntu","default_terminal_profile":"powershell","default_agent_command":"codex"}}"##
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let updated = response_value(&update);
        assert_eq!(updated["name"], "Project Alpha");
        assert_eq!(updated["project_root"], "D:\\work\\alpha");
        assert_eq!(updated["description"], "demo project");
        assert_eq!(updated["icon"], "PA");
        assert_eq!(updated["color"], "#22C55E");
        assert_eq!(updated["default_wsl_distribution"], "Ubuntu");
        assert_eq!(updated["default_terminal_profile"], "powershell");
        assert_eq!(updated["default_agent_command"], "codex");

        let diagnostics = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_recovery_diagnostics",
                "diagnostics.recovery",
                "{}",
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let diagnostics = response_value(&diagnostics);
        assert_eq!(diagnostics["workspace_count"], 1);
        assert_eq!(diagnostics["pane_count"], 1);

        let close = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_close",
                "workspace.close",
                format!(r#"{{"workspace_id":"{workspace_id}","close_policy":"fail_if_running"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&close)["closed"], true);
    }

    #[test]
    fn workspace_group_methods_round_trip_through_desktop_store() {
        let state = DesktopControlState::new();
        let first = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_group_ws_a",
                "workspace.create",
                r#"{"name":"Alpha","project_root":null,"backend_profile":null}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let first_id = response_value(&first)["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();
        let second = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_group_ws_b",
                "workspace.create",
                r#"{"name":"Beta","project_root":null,"backend_profile":null}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let second_id = response_value(&second)["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();

        let create_group = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_group_create",
                "workspace_group.create",
                serde_json::json!({
                    "name": "Agents",
                    "anchor_workspace_id": first_id.clone(),
                    "workspace_ids": [first_id.clone()],
                    "collapsed": true,
                    "pinned": true,
                    "color": "#F97316",
                    "icon": "AG"
                })
                .to_string(),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let created_group = response_value(&create_group);
        let group_id = created_group["group_id"].as_str().unwrap().to_string();
        assert_eq!(created_group["name"], "Agents");
        assert_eq!(created_group["collapsed"], true);
        assert_eq!(created_group["pinned"], true);
        assert_eq!(created_group["members"].as_array().unwrap().len(), 1);

        let add = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_group_add",
                "workspace_group.add_workspace",
                serde_json::json!({
                    "group_id": group_id.clone(),
                    "workspace_id": second_id.clone(),
                    "position": 1
                })
                .to_string(),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&add)["members"].as_array().unwrap().len(), 2);

        let update = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_group_update",
                "workspace_group.update",
                serde_json::json!({
                    "group_id": group_id.clone(),
                    "name": "Core agents",
                    "collapsed": false,
                    "sort_order": 4
                })
                .to_string(),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let updated = response_value(&update);
        assert_eq!(updated["name"], "Core agents");
        assert_eq!(updated["collapsed"], false);
        assert_eq!(updated["sort_order"], 4);

        let list = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_group_list",
                "workspace_group.list",
                "{}",
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&list)["groups"].as_array().unwrap().len(), 1);

        let remove = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_group_remove",
                "workspace_group.remove_workspace",
                serde_json::json!({
                    "group_id": group_id.clone(),
                    "workspace_id": second_id
                })
                .to_string(),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(
            response_value(&remove)["members"].as_array().unwrap().len(),
            1
        );

        let close = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_group_close_workspace",
                "workspace.close",
                serde_json::json!({
                    "workspace_id": first_id,
                    "close_policy": "fail_if_running"
                })
                .to_string(),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&close)["closed"], true);

        let list_after_close = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_group_list_after_close",
                "workspace_group.list",
                "{}",
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let list_after_close_value = response_value(&list_after_close);
        let group_after_close = &list_after_close_value["groups"].as_array().unwrap()[0];
        assert!(group_after_close["anchor_workspace_id"].is_null());
        assert_eq!(group_after_close["members"].as_array().unwrap().len(), 0);

        let delete = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_group_delete",
                "workspace_group.delete",
                serde_json::json!({ "group_id": group_id }).to_string(),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&delete)["ok"], true);
    }

    #[test]
    fn pane_split_and_focus_round_trip_through_desktop_store() {
        let state = DesktopControlState::new();
        let create = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_create",
                "workspace.create",
                r#"{"name":"Pane workspace","project_root":null,"backend_profile":null}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let created = response_value(&create);
        let workspace_id = created["workspace_id"].as_str().unwrap();
        let root_pane_id = created["root_pane_id"].as_str().unwrap();

        let split = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_pane_split",
                "pane.split",
                format!(
                    r#"{{"workspace_id":"{workspace_id}","pane_id":"{root_pane_id}","axis":"vertical","ratio":0.42}}"#
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let split = response_value(&split);
        assert_eq!(split["panes"].as_array().unwrap().len(), 3);
        assert_ne!(split["workspace"]["active_pane_id"], root_pane_id);
        let split_parent = split["panes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|pane| pane["pane_id"].as_str() == Some(root_pane_id))
            .unwrap();
        assert_eq!(split_parent["split_ratio"], 0.42);

        let resized = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_pane_resize_layout",
                "pane.resize_layout",
                format!(
                    r#"{{"workspace_id":"{workspace_id}","pane_id":"{root_pane_id}","ratio":0.7}}"#
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let resized = response_value(&resized);
        let resized_parent = resized["panes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|pane| pane["pane_id"].as_str() == Some(root_pane_id))
            .unwrap();
        assert_eq!(resized_parent["split_ratio"], 0.7);

        let child_ids = split["panes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|pane| pane["parent_pane_id"].as_str() == Some(root_pane_id))
            .map(|pane| pane["pane_id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(child_ids.len(), 2);

        let focus = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_pane_focus",
                "pane.focus",
                format!(
                    r#"{{"workspace_id":"{workspace_id}","pane_id":"{}"}}"#,
                    child_ids[1]
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(
            response_value(&focus)["workspace"]["active_pane_id"],
            child_ids[1]
        );

        let close = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_pane_close",
                "pane.close",
                format!(
                    r#"{{"workspace_id":"{workspace_id}","pane_id":"{}","surface_policy":"fail_if_session_running"}}"#,
                    child_ids[1]
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let closed = response_value(&close);
        assert_eq!(closed["panes"].as_array().unwrap().len(), 1);
        assert_eq!(closed["workspace"]["active_pane_id"], root_pane_id);
    }

    #[test]
    fn pane_mount_and_unmount_surface_round_trip_through_desktop_store() {
        let state = DesktopControlState::new();
        {
            let mut store = state.store.lock().unwrap();
            store
                .save_workspace_bundle(&workspace_bundle_with_unmounted_surface())
                .unwrap();
        }

        let mount = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_pane_mount_surface",
                "pane.mount_surface",
                r#"{"workspace_id":"ws_surface","pane_id":"pane_right","surface_id":"surf_terminal"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let mounted = response_value(&mount);
        assert_eq!(mounted["workspace"]["active_pane_id"], "pane_right");
        let mounted_pane = mounted["panes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|pane| pane["pane_id"].as_str() == Some("pane_right"))
            .unwrap();
        assert_eq!(mounted_pane["mounted_surface_id"], "surf_terminal");

        let unmount = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_pane_unmount_surface",
                "pane.unmount_surface",
                r#"{"workspace_id":"ws_surface","pane_id":"pane_right"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let unmounted = response_value(&unmount);
        let unmounted_pane = unmounted["panes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|pane| pane["pane_id"].as_str() == Some("pane_right"))
            .unwrap();
        assert!(unmounted_pane["mounted_surface_id"].is_null());
        assert_eq!(unmounted["surfaces"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn surface_close_removes_surface_session_and_mounted_pane_reference() {
        let state = DesktopControlState::new();
        {
            let mut store = state.store.lock().unwrap();
            store
                .save_workspace_bundle(&workspace_bundle_with_unmounted_surface())
                .unwrap();
        }

        let mount = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_surface_close_mount",
                "pane.mount_surface",
                r#"{"workspace_id":"ws_surface","pane_id":"pane_right","surface_id":"surf_terminal"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert!(matches!(mount.outcome, ResponseOutcome::Ok { .. }));

        let close = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_surface_close",
                "surface.close",
                r#"{"workspace_id":"ws_surface","surface_id":"surf_terminal"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let closed = response_value(&close);
        assert_eq!(closed["surfaces"].as_array().unwrap().len(), 0);
        assert_eq!(closed["sessions"].as_array().unwrap().len(), 0);
        let pane = closed["panes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|pane| pane["pane_id"].as_str() == Some("pane_right"))
            .unwrap();
        assert!(pane["mounted_surface_id"].is_null());
    }

    #[test]
    fn browser_surface_and_commands_round_trip_through_desktop_control() {
        let state = DesktopControlState::new();
        let workspace = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_workspace",
                "workspace.create",
                r#"{"name":"Browser workspace","project_root":null,"backend_profile":null}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let workspace_id = response_string_field(&workspace, "workspace_id");
        let root_pane_id = response_string_field(&workspace, "root_pane_id");

        let surface = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_create_browser",
                "surface.create_browser",
                format!(
                    r#"{{"workspace_id":"{workspace_id}","pane_id":"{root_pane_id}","profile":"default"}}"#
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let surface_value = response_value(&surface);
        let surface_id = surface_value["surface_id"].as_str().unwrap().to_string();
        assert_eq!(surface_value["surface_type"], "browser");
        assert!(surface_value["browser_id"]
            .as_str()
            .unwrap()
            .starts_with("browser_"));

        let detail = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_workspace_get",
                "workspace.get",
                format!(r#"{{"workspace_id":"{workspace_id}"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let detail = response_value(&detail);
        let mounted_pane = detail["panes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|pane| pane["pane_id"].as_str() == Some(&root_pane_id))
            .unwrap();
        assert_eq!(mounted_pane["mounted_surface_id"], surface_id);

        let browser_tab = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_create_browser_tab",
                "surface.create_browser",
                format!(
                    r#"{{"workspace_id":"{workspace_id}","pane_id":null,"profile":"default","placement":"new_tab"}}"#
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let browser_tab_id = response_string_field(&browser_tab, "surface_id");
        let detail_with_tab = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_tab_workspace_get",
                "workspace.get",
                format!(r#"{{"workspace_id":"{workspace_id}"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let detail_with_tab = response_value(&detail_with_tab);
        let tab_host = detail_with_tab["panes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|pane| pane["mounted_surface_id"].as_str() == Some(&browser_tab_id))
            .unwrap();
        assert!(tab_host["parent_pane_id"].is_null());

        let navigate = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_navigate",
                "browser.navigate",
                format!(r#"{{"surface_id":"{surface_id}","url":"https://example.invalid"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&navigate)["url"], "https://example.invalid");

        let current_url = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_current_url",
                "browser.current_url",
                format!(r#"{{"surface_id":"{surface_id}"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(
            response_value(&current_url)["url"],
            "https://example.invalid"
        );

        let reload = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_reload",
                "browser.reload",
                format!(r#"{{"surface_id":"{surface_id}"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&reload)["url"], "https://example.invalid");

        let back = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_back",
                "browser.back",
                format!(r#"{{"surface_id":"{surface_id}"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&back)["url"], "https://example.invalid");

        let forward = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_forward",
                "browser.forward",
                format!(r#"{{"surface_id":"{surface_id}"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&forward)["url"], "https://example.invalid");

        let snapshot = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_snapshot",
                "browser.dom_snapshot",
                format!(r#"{{"surface_id":"{surface_id}","frame_id":"frame_{surface_id}"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let html = response_string_field(&snapshot, "html");
        assert!(html.contains(&surface_id));
        assert!(html.contains("https://example.invalid"));

        let frames = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_frames",
                "browser.frames",
                format!(r#"{{"surface_id":"{surface_id}"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let frames = response_value(&frames);
        assert_eq!(frames["surface_id"], surface_id);
        let frames_list = frames["frames"].as_array().unwrap();
        assert_eq!(frames_list.len(), 1);
        assert_eq!(frames_list[0]["url"], "https://example.invalid");

        let storage = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_storage",
                "browser.storage",
                format!(r#"{{"surface_id":"{surface_id}"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let storage = response_value(&storage);
        assert_eq!(storage["surface_id"], surface_id);
        assert!(storage["local_storage"].as_array().unwrap().is_empty());
        assert!(storage["session_storage"].as_array().unwrap().is_empty());

        let cookies = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_cookies",
                "browser.cookies",
                format!(r#"{{"surface_id":"{surface_id}"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let cookies = response_value(&cookies);
        assert_eq!(cookies["surface_id"], surface_id);
        assert!(cookies["cookies"].as_array().unwrap().is_empty());

        let downloads = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_downloads",
                "browser.downloads",
                format!(r#"{{"surface_id":"{surface_id}","limit":25}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let downloads = response_value(&downloads);
        assert_eq!(downloads["surface_id"], surface_id);
        assert_eq!(
            downloads["directory"],
            format!("memory://browser/{surface_id}/downloads")
        );
        assert!(downloads["downloads"].as_array().unwrap().is_empty());

        let history = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_history",
                "browser.history",
                format!(r#"{{"surface_id":"{surface_id}"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let history = response_value(&history);
        assert_eq!(history["surface_id"], surface_id);
        assert_eq!(history["current_index"], 0);
        let history_entries = history["entries"].as_array().unwrap();
        assert_eq!(history_entries.len(), 1);
        assert_eq!(history_entries[0]["url"], "https://example.invalid");

        let console = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_console",
                "browser.console",
                format!(r#"{{"surface_id":"{surface_id}","limit":25}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let console = response_value(&console);
        assert_eq!(console["surface_id"], surface_id);
        assert!(console["messages"].as_array().unwrap().is_empty());

        let dialogs = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_dialogs",
                "browser.dialogs",
                format!(r#"{{"surface_id":"{surface_id}","limit":25}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let dialogs = response_value(&dialogs);
        assert_eq!(dialogs["surface_id"], surface_id);
        assert!(dialogs["messages"].as_array().unwrap().is_empty());

        let errors = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_errors",
                "browser.errors",
                format!(r#"{{"surface_id":"{surface_id}","limit":25}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let errors = response_value(&errors);
        assert_eq!(errors["surface_id"], surface_id);
        assert!(errors["events"].as_array().unwrap().is_empty());

        let screenshot = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_screenshot",
                "browser.screenshot",
                format!(r#"{{"surface_id":"{surface_id}","format":"png"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let screenshot = response_value(&screenshot);
        assert_eq!(screenshot["format"], "png");
        assert!(screenshot["image_handle"]
            .as_str()
            .unwrap()
            .contains(&surface_id));
        assert!(screenshot["byte_count"].as_u64().unwrap() > 0);

        let click = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_click",
                "browser.click",
                format!(
                    r##"{{"surface_id":"{surface_id}","selector":"#login","frame_id":"frame_{surface_id}"}}"##
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&click)["ok"], true);

        let click_point = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_click_point",
                "browser.click",
                format!(r#"{{"surface_id":"{surface_id}","x":12.0,"y":24.0}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&click_point)["ok"], true);

        let typed = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_type",
                "browser.type",
                format!(
                    r##"{{"surface_id":"{surface_id}","selector":"#q","text":"agentmux","frame_id":"frame_{surface_id}"}}"##
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&typed)["ok"], true);

        let filled = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_fill",
                "browser.fill",
                format!(
                    r##"{{"surface_id":"{surface_id}","selector":"#q","text":"agentmux","frame_id":"frame_{surface_id}"}}"##
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&filled)["ok"], true);

        let pressed = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_press",
                "browser.press",
                format!(
                    r##"{{"surface_id":"{surface_id}","selector":"#q","key":"Enter","frame_id":"frame_{surface_id}"}}"##
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&pressed)["ok"], true);

        let selected = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_select",
                "browser.select",
                format!(
                    r##"{{"surface_id":"{surface_id}","selector":"#choice","values":["one"],"frame_id":"frame_{surface_id}"}}"##
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&selected)["ok"], true);

        let scrolled = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_scroll",
                "browser.scroll",
                format!(
                    r##"{{"surface_id":"{surface_id}","selector":"#list","y":400,"frame_id":"frame_{surface_id}"}}"##
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&scrolled)["ok"], true);

        let hovered = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_hover",
                "browser.hover",
                format!(
                    r##"{{"surface_id":"{surface_id}","selector":"#submit","frame_id":"frame_{surface_id}"}}"##
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&hovered)["ok"], true);

        let checked = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_check",
                "browser.check",
                format!(
                    r##"{{"surface_id":"{surface_id}","selector":"#agree","checked":true,"frame_id":"frame_{surface_id}"}}"##
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&checked)["ok"], true);

        let got = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_get",
                "browser.get",
                format!(
                    r##"{{"surface_id":"{surface_id}","selector":"#q","kind":"text","frame_id":"frame_{surface_id}"}}"##
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let got_value = response_value(&got);
        assert_eq!(got_value["surface_id"], surface_id);
        assert_eq!(got_value["selector"], "#q");
        assert_eq!(got_value["kind"], "text");

        let found = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_find",
                "browser.find",
                format!(
                    r##"{{"surface_id":"{surface_id}","query":"agentmux","selector":"main","limit":5,"frame_id":"frame_{surface_id}"}}"##
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let found_value = response_value(&found);
        assert_eq!(found_value["surface_id"], surface_id);
        assert_eq!(found_value["query"], "agentmux");
        assert_eq!(found_value["count"], 1);

        let highlighted = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_highlight",
                "browser.highlight",
                format!(
                    r##"{{"surface_id":"{surface_id}","selector":"#q","duration_ms":750,"frame_id":"frame_{surface_id}"}}"##
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&highlighted)["ok"], true);

        let focused = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_focus",
                "browser.focus",
                format!(
                    r##"{{"surface_id":"{surface_id}","selector":"#q","frame_id":"frame_{surface_id}"}}"##
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&focused)["ok"], true);

        let zoomed = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_zoom",
                "browser.zoom",
                format!(r#"{{"surface_id":"{surface_id}","percent":125}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&zoomed)["ok"], true);

        let waited = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_wait",
                "browser.wait_for_selector",
                format!(
                    r##"{{"surface_id":"{surface_id}","selector":"#q","timeout_ms":250,"frame_id":"frame_{surface_id}"}}"##
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let waited = response_value(&waited);
        assert_eq!(waited["surface_id"], surface_id);
        assert_eq!(waited["selector"], "#q");

        let evaluated = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_evaluate",
                "browser.evaluate",
                format!(
                    r#"{{"surface_id":"{surface_id}","script":"document.title","frame_id":"frame_{surface_id}"}}"#
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(
            response_string_field(&evaluated, "value_json"),
            r#"{"ok":true}"#
        );
    }

    #[test]
    fn browser_commands_reject_missing_or_non_browser_surfaces() {
        let state = DesktopControlState::new();
        let missing = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_missing",
                "browser.navigate",
                r#"{"surface_id":"surf_missing","url":"https://example.invalid"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_error_code(&missing), ErrorCode::SurfaceNotFound);

        {
            let mut store = state.store.lock().unwrap();
            store
                .save_workspace_bundle(&workspace_bundle_with_unmounted_surface())
                .unwrap();
        }
        let terminal_surface = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_terminal_surface",
                "browser.navigate",
                r#"{"surface_id":"surf_terminal","url":"https://example.invalid"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(
            response_error_code(&terminal_surface),
            ErrorCode::InvalidRequest
        );
    }

    #[test]
    fn browser_failures_are_exposed_as_diagnostics_and_notifications() {
        let state = DesktopControlState::new();
        let missing = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_missing_diagnostic",
                "browser.navigate",
                r#"{"surface_id":"surf_missing","url":"https://example.invalid"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_error_code(&missing), ErrorCode::SurfaceNotFound);

        let diagnostics = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_diagnostics_missing",
                "diagnostics.browser",
                r#"{"workspace_id":null,"surface_id":"surf_missing"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let failures = response_value(&diagnostics)["failures"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0]["surface_id"], "surf_missing");
        assert_eq!(failures[0]["operation"], "browser.navigate");
        assert_eq!(failures[0]["code"], "surface_not_found");

        {
            let mut store = state.store.lock().unwrap();
            store
                .save_workspace_bundle(&workspace_bundle_with_unmounted_surface())
                .unwrap();
        }
        let non_browser = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_non_browser_diagnostic",
                "browser.navigate",
                r#"{"surface_id":"surf_terminal","url":"https://example.invalid"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_error_code(&non_browser), ErrorCode::InvalidRequest);

        let workspace_diagnostics = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_diagnostics_workspace",
                "diagnostics.browser",
                r#"{"workspace_id":"ws_surface","surface_id":null}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let workspace_failures = response_value(&workspace_diagnostics)["failures"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(workspace_failures.len(), 1);
        assert_eq!(workspace_failures[0]["workspace_id"], "ws_surface");
        assert_eq!(workspace_failures[0]["surface_id"], "surf_terminal");
        assert_eq!(workspace_failures[0]["code"], "invalid_request");

        let notifications = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_failure_notifications",
                "notification.list",
                r#"{"workspace_id":"ws_surface","severity":"error","include_dismissed":false}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let notifications = response_value(&notifications)["notifications"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(notifications.len(), 1);
        assert_eq!(
            notifications[0]["notification_type"],
            "browser.action_failed"
        );
        assert_eq!(notifications[0]["severity"], "error");
        assert!(notifications[0]["message"]
            .as_str()
            .unwrap()
            .contains("browser.navigate"));
    }

    #[test]
    fn diagnostics_export_includes_health_queue_and_recent_failures() {
        let state = DesktopControlState::new();
        {
            let mut store = state.store.lock().unwrap();
            store
                .save_workspace_bundle(&workspace_bundle_with_unmounted_surface())
                .unwrap();
        }
        let _ = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_export_failure",
                "browser.navigate",
                r#"{"surface_id":"surf_terminal","url":"https://example.invalid"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );

        let export = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_diagnostics_export",
                "diagnostics.export",
                "{}",
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let export = response_value(&export);
        assert_eq!(export["format_version"], "agentmux.diagnostics.v1");
        assert_eq!(export["recovery"]["workspace_count"], 1);
        assert_eq!(export["browser"]["failures"].as_array().unwrap().len(), 1);
        assert!(export["notifications"]
            .as_array()
            .unwrap()
            .iter()
            .any(|notification| notification["notification_type"] == "browser.action_failed"));

        let backend = export["backend_health"]
            .as_array()
            .unwrap()
            .iter()
            .find(|backend| backend["backend_kind"] == "conpty")
            .unwrap();
        assert_eq!(backend["health"], "degraded");
        assert_eq!(backend["failed_sessions"], 1);

        let queues = export["queue_pressure"].as_array().unwrap();
        assert!(queues
            .iter()
            .any(|queue| queue["queue"] == "runtime.events.pending"));
        assert!(queues
            .iter()
            .any(|queue| queue["queue"] == "desktop.browser_failures"
                && queue["depth"] == 1
                && queue["capacity"] == MAX_BROWSER_FAILURES));
        assert_eq!(export["output_stream"]["active_sessions"], 0);
        assert_eq!(export["output_stream"]["active_subscriptions"], 0);
        assert_eq!(export["output_stream"]["frames_sent"], 0);
        assert_eq!(export["output_stream"]["renderer_queued_bytes"], 0);
        assert_eq!(export["output_stream"]["renderer_backpressure_events"], 0);
    }

    #[test]
    fn agent_notification_methods_read_persisted_desktop_history() {
        let state = DesktopControlState::new();
        {
            let mut store = state.store.lock().unwrap();
            store
                .save_workspace_bundle(&workspace_bundle_with_unmounted_surface())
                .unwrap();
            store
                .upsert_agent_state(&PersistedAgentState {
                    session_id: "ses_surface".to_string(),
                    workspace_id: "ws_surface".to_string(),
                    state: "waiting_for_input".to_string(),
                    attention: true,
                    reason: Some("review needed".to_string()),
                    updated_at: "2026-06-18T00:10:00Z".to_string(),
                    telemetry_json: None,
                })
                .unwrap();
            store
                .upsert_notification(&PersistedNotification {
                    notification_id: "not_desktop_test".to_string(),
                    notification_type: "agent.needs_input".to_string(),
                    severity: "warning".to_string(),
                    workspace_id: Some("ws_surface".to_string()),
                    session_id: Some("ses_surface".to_string()),
                    title: "Agent needs input".to_string(),
                    message: "review needed".to_string(),
                    created_at: "2026-06-18T00:10:00Z".to_string(),
                    dismissed: false,
                })
                .unwrap();
        }

        let state_response = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_agent_get_state",
                "agent.get_state",
                r#"{"session_id":"ses_surface"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let state_value = response_value(&state_response);
        assert_eq!(state_value["state"], "waiting_for_input");
        assert_eq!(state_value["attention"], true);

        let attention = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_agent_list_attention",
                "agent.list_attention",
                r#"{"workspace_id":"ws_surface"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(
            response_value(&attention)["sessions"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let notifications = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_notification_list",
                "notification.list",
                r#"{"workspace_id":"ws_surface","severity":"warning","include_dismissed":false}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(
            response_value(&notifications)["notifications"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let dismiss = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_notification_dismiss",
                "notification.dismiss",
                r#"{"notification_id":"not_desktop_test"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&dismiss)["ok"], true);

        let visible = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_notification_visible",
                "notification.list",
                r#"{"workspace_id":"ws_surface","severity":"warning","include_dismissed":false}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert!(response_value(&visible)["notifications"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn desktop_notification_adapter_receives_agent_notifications_once() {
        let state = DesktopControlState::new();
        let adapter = Arc::new(RecordingDesktopNotificationAdapter::default());
        state.set_desktop_notification_adapter(adapter.clone());

        let notification = NotificationSummaryResult {
            notification_id: "not_desktop_adapter".to_string(),
            notification_type: "agent.needs_input".to_string(),
            severity: "warning".to_string(),
            workspace_id: Some("ws_notify".to_string()),
            session_id: Some("ses_notify".to_string()),
            title: "Agent needs input".to_string(),
            message: "approval needed".to_string(),
            created_at: "2026-06-18T00:00:00Z".to_string(),
            dismissed: false,
        };
        state.dispatch_desktop_notification(&notification);
        state.dispatch_desktop_notification(&notification);
        state.dispatch_desktop_notification(&NotificationSummaryResult {
            notification_id: "not_backend".to_string(),
            notification_type: "backend.disconnected".to_string(),
            title: "Backend disconnected".to_string(),
            ..notification
        });

        let delivered = adapter.delivered();
        assert_eq!(
            delivered,
            vec![DesktopNotification {
                notification_id: "not_desktop_adapter".to_string(),
                notification_type: "agent.needs_input".to_string(),
                severity: "warning".to_string(),
                title: "Agent needs input".to_string(),
                body: "approval needed".to_string(),
            }]
        );
    }

    #[test]
    #[cfg(windows)]
    fn workspace_close_policies_coordinate_live_backend_sessions() {
        let state = DesktopControlState::new();
        let workspace_id = "ws_close_policy";
        let spawn = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_spawn_close_policy",
                "session.spawn",
                format!(
                    r#"{{"workspace_id":"{workspace_id}","command":["cmd.exe","/d","/q"],"cwd":null,"columns":120,"rows":30,"durability":"ephemeral"}}"#
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let session_id = response_string_field(&spawn, "session_id");

        let fail_close = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_close_fail_running",
                "workspace.close",
                format!(r#"{{"workspace_id":"{workspace_id}","close_policy":"fail_if_running"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_error_code(&fail_close), ErrorCode::Conflict);

        let terminate_close = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_close_terminate",
                "workspace.close",
                format!(
                    r#"{{"workspace_id":"{workspace_id}","close_policy":"terminate_sessions"}}"#
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&terminate_close)["closed"], true);

        let send_after_close = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_send_after_close",
                "session.send_text",
                format!(r#"{{"session_id":"{session_id}","text":"echo after close\r"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(
            response_error_code(&send_after_close),
            ErrorCode::SessionNotFound
        );
    }

    #[test]
    fn desktop_control_routes_wsl_direct_spawn_requests() {
        let state = DesktopControlState::new();
        let response = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_spawn_wsl_direct",
                "session.spawn",
                r#"{"workspace_id":"ws_wsl","backend":"wsl-direct","command":["bash"],"cwd":"relative/path","columns":120,"rows":30,"durability":"ephemeral"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );

        assert_eq!(response_error_code(&response), ErrorCode::InvalidRequest);
        assert!(response_error_message(&response).contains("WSL working directory"));
    }

    #[test]
    fn desktop_control_routes_tmux_control_spawn_requests() {
        let state = DesktopControlState::new();
        let response = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_spawn_wsl_tmux_control",
                "session.spawn",
                r#"{"workspace_id":"ws_tmux","backend":"wsl-tmux-control","command":["bash"],"cwd":"relative/path","columns":120,"rows":30,"durability":"durable"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );

        assert_eq!(response_error_code(&response), ErrorCode::InvalidRequest);
        assert!(response_error_message(&response).contains("WSL working directory"));
    }

    #[test]
    fn persisted_spawn_metadata_preserves_backend_native_id_for_recovery() {
        let params: SessionSpawnParams = serde_json::from_str(
            r#"{"workspace_id":"ws_tmux","backend":"wsl-tmux-control","backend_profile":"Ubuntu","command":["bash"],"cwd":"/tmp","columns":120,"rows":30,"durability":"durable"}"#,
        )
        .unwrap();
        let result = SessionSpawnResult {
            session_id: "ses_tmux".to_string(),
        };
        let summary = SessionSummaryResult {
            session_id: "ses_tmux".to_string(),
            workspace_id: "ws_tmux".to_string(),
            backend_kind: "wsl-tmux-control".to_string(),
            state: "running".to_string(),
            exit_code: None,
            backend_native_id: Some("agentmux_tmux".to_string()),
            cwd: Some("/tmp".to_string()),
        };

        let session =
            persisted_terminal_session(&params, &result, &summary, "durable".to_string(), "now");

        assert_eq!(session.backend_native_id.as_deref(), Some("agentmux_tmux"));
        assert!(should_attach_recovering_session(&PersistedSession {
            state: "recovering".to_string(),
            ..session
        }));
    }

    #[test]
    fn mounted_terminal_surfaces_are_indexed_by_session() {
        let mut bundle = workspace_bundle_with_unmounted_surface();
        bundle.panes[1].mounted_surface_id = Some("surf_terminal".to_string());
        let snapshot = RecoverySnapshot {
            workspaces: vec![bundle.workspace],
            panes: bundle.panes,
            surfaces: bundle.surfaces,
            sessions: bundle.sessions,
        };

        let mounted = mounted_terminal_surfaces_by_session(&snapshot);

        assert_eq!(
            mounted.get("ses_surface"),
            Some(&("pane_left".to_string(), "surf_terminal".to_string()))
        );
    }

    #[test]
    fn restore_agent_state_filter_requires_active_agent_metadata() {
        assert!(should_restore_agent_state(&PersistedAgentState {
            session_id: "ses_agent".to_string(),
            workspace_id: "ws_agent".to_string(),
            state: "running".to_string(),
            attention: false,
            reason: Some("Agent started: claude".to_string()),
            updated_at: "now".to_string(),
            telemetry_json: Some(r#"{"activity":"agent","session":"claude"}"#.to_string()),
        }));
        assert!(should_restore_agent_state(&PersistedAgentState {
            session_id: "ses_agent_team".to_string(),
            workspace_id: "ws_agent".to_string(),
            state: "waiting_for_input".to_string(),
            attention: true,
            reason: None,
            updated_at: "now".to_string(),
            telemetry_json: Some(
                r#"{"activity":"agent_team","session":"omo:split-window"}"#.to_string(),
            ),
        }));
        assert!(!should_restore_agent_state(&PersistedAgentState {
            session_id: "ses_done".to_string(),
            workspace_id: "ws_agent".to_string(),
            state: "completed".to_string(),
            attention: false,
            reason: Some("Agent started: claude".to_string()),
            updated_at: "now".to_string(),
            telemetry_json: Some(r#"{"activity":"agent","session":"claude"}"#.to_string()),
        }));
        assert!(!should_restore_agent_state(&PersistedAgentState {
            session_id: "ses_shell".to_string(),
            workspace_id: "ws_agent".to_string(),
            state: "running".to_string(),
            attention: false,
            reason: Some("plain shell".to_string()),
            updated_at: "now".to_string(),
            telemetry_json: None,
        }));
    }

    #[test]
    fn workspace_backend_profiles_prefer_default_wsl_distribution() {
        let mut bundle = workspace_bundle_with_unmounted_surface();
        bundle.workspace.environment_profile_id = Some("LegacyProfile".to_string());
        bundle.workspace.default_wsl_distribution = Some("Ubuntu-24.04".to_string());
        let snapshot = RecoverySnapshot {
            workspaces: vec![bundle.workspace],
            panes: Vec::new(),
            surfaces: Vec::new(),
            sessions: Vec::new(),
        };

        let profiles = workspace_backend_profiles(&snapshot);

        assert_eq!(
            profiles
                .get("ws_surface")
                .and_then(|value| value.as_deref()),
            Some("Ubuntu-24.04")
        );
    }

    #[test]
    fn restored_agent_launch_line_replays_safe_agent_command_for_shell_sessions() {
        let session = PersistedSession {
            session_id: "ses_shell".to_string(),
            workspace_id: "ws_agent".to_string(),
            backend_kind: "wsl-direct".to_string(),
            backend_attachment_id: None,
            backend_native_id: None,
            cwd: Some("/tmp".to_string()),
            command: vec!["bash".to_string(), "-l".to_string()],
            state: "disconnected".to_string(),
            exit_code: None,
            durability: "ephemeral".to_string(),
            created_at: "before".to_string(),
            last_seen_at: None,
            updated_at: "before".to_string(),
        };
        let state = PersistedAgentState {
            session_id: "ses_shell".to_string(),
            workspace_id: "ws_agent".to_string(),
            state: "running".to_string(),
            attention: false,
            reason: Some("Agent started: claude --resume".to_string()),
            updated_at: "before".to_string(),
            telemetry_json: Some(r#"{"activity":"agent","session":"claude --resume"}"#.to_string()),
        };

        assert_eq!(
            restored_agent_launch_line(&session, &state).as_deref(),
            Some("claude")
        );
    }

    #[test]
    fn restored_agent_launch_line_strips_resume_selector_and_keeps_safe_flags() {
        let session = PersistedSession {
            session_id: "ses_shell".to_string(),
            workspace_id: "ws_agent".to_string(),
            backend_kind: "conpty".to_string(),
            backend_attachment_id: None,
            backend_native_id: None,
            cwd: Some("D:\\work".to_string()),
            command: vec!["powershell.exe".to_string(), "-NoLogo".to_string()],
            state: "disconnected".to_string(),
            exit_code: None,
            durability: "ephemeral".to_string(),
            created_at: "before".to_string(),
            last_seen_at: None,
            updated_at: "before".to_string(),
        };
        let state = PersistedAgentState {
            session_id: "ses_shell".to_string(),
            workspace_id: "ws_agent".to_string(),
            state: "running".to_string(),
            attention: false,
            reason: Some(
                "Agent started: claude --dangerously-skip-permissions --resume".to_string(),
            ),
            updated_at: "before".to_string(),
            telemetry_json: Some(
                r#"{"activity":"agent","session":"claude --dangerously-skip-permissions --resume"}"#
                    .to_string(),
            ),
        };

        assert_eq!(
            restored_agent_launch_line(&session, &state).as_deref(),
            Some("claude --dangerously-skip-permissions")
        );
    }

    #[test]
    fn restored_agent_launch_line_preserves_codex_resume_session_selector() {
        let session = PersistedSession {
            session_id: "ses_shell".to_string(),
            workspace_id: "ws_agent".to_string(),
            backend_kind: "wsl-direct".to_string(),
            backend_attachment_id: None,
            backend_native_id: None,
            cwd: Some("/tmp".to_string()),
            command: vec!["bash".to_string(), "-l".to_string()],
            state: "disconnected".to_string(),
            exit_code: None,
            durability: "ephemeral".to_string(),
            created_at: "before".to_string(),
            last_seen_at: None,
            updated_at: "before".to_string(),
        };
        let state = PersistedAgentState {
            session_id: "ses_shell".to_string(),
            workspace_id: "ws_agent".to_string(),
            state: "running".to_string(),
            attention: false,
            reason: Some("Agent started: codex resume abc123".to_string()),
            updated_at: "before".to_string(),
            telemetry_json: Some(
                r#"{"activity":"agent","session":"codex resume abc123"}"#.to_string(),
            ),
        };

        assert_eq!(
            restored_agent_launch_line(&session, &state).as_deref(),
            Some("codex --no-alt-screen resume abc123")
        );
    }

    #[test]
    fn restored_agent_launch_line_resumes_last_codex_session_for_plain_codex() {
        let session = PersistedSession {
            session_id: "ses_shell".to_string(),
            workspace_id: "ws_agent".to_string(),
            backend_kind: "wsl-direct".to_string(),
            backend_attachment_id: None,
            backend_native_id: None,
            cwd: Some("/tmp".to_string()),
            command: vec!["bash".to_string(), "-l".to_string()],
            state: "disconnected".to_string(),
            exit_code: None,
            durability: "ephemeral".to_string(),
            created_at: "before".to_string(),
            last_seen_at: None,
            updated_at: "before".to_string(),
        };
        let state = PersistedAgentState {
            session_id: "ses_shell".to_string(),
            workspace_id: "ws_agent".to_string(),
            state: "running".to_string(),
            attention: false,
            reason: Some("Agent started: codex".to_string()),
            updated_at: "before".to_string(),
            telemetry_json: Some(r#"{"activity":"agent","session":"codex"}"#.to_string()),
        };

        assert_eq!(
            restored_agent_launch_line(&session, &state).as_deref(),
            Some("codex --no-alt-screen resume --last")
        );
    }

    #[test]
    fn restored_agent_launch_line_resumes_last_codex_session_without_replaying_prompt() {
        let state = PersistedAgentState {
            session_id: "ses_shell".to_string(),
            workspace_id: "ws_agent".to_string(),
            state: "running".to_string(),
            attention: false,
            reason: Some("Agent started: codex -m gpt-5 \"summarize recent commits\"".to_string()),
            updated_at: "before".to_string(),
            telemetry_json: Some(
                r#"{"activity":"agent","session":"codex -m gpt-5 \"summarize recent commits\""}"#
                    .to_string(),
            ),
        };

        assert_eq!(
            normalized_restored_agent_command_label(&state).as_deref(),
            Some("codex -m gpt-5 --no-alt-screen resume --last")
        );
    }

    #[test]
    fn restored_agent_launch_line_keeps_codex_assignment_options() {
        let state = PersistedAgentState {
            session_id: "ses_shell".to_string(),
            workspace_id: "ws_agent".to_string(),
            state: "running".to_string(),
            attention: false,
            reason: Some(r#"Agent started: codex --model=gpt-5 "new task""#.to_string()),
            updated_at: "before".to_string(),
            telemetry_json: Some(
                r#"{"activity":"agent","session":"codex --model=gpt-5 \"new task\""}"#.to_string(),
            ),
        };

        assert_eq!(
            normalized_restored_agent_command_label(&state).as_deref(),
            Some("codex --model=gpt-5 --no-alt-screen resume --last")
        );
    }

    #[test]
    fn restored_agent_launch_line_skips_sessions_that_already_spawn_agent() {
        let session = PersistedSession {
            session_id: "ses_agent".to_string(),
            workspace_id: "ws_agent".to_string(),
            backend_kind: "wsl-tmux-control".to_string(),
            backend_attachment_id: None,
            backend_native_id: Some("agentmux_ses_agent".to_string()),
            cwd: Some("/tmp".to_string()),
            command: vec!["claude".to_string()],
            state: "recovering".to_string(),
            exit_code: None,
            durability: "durable".to_string(),
            created_at: "before".to_string(),
            last_seen_at: None,
            updated_at: "before".to_string(),
        };
        let state = PersistedAgentState {
            session_id: "ses_agent".to_string(),
            workspace_id: "ws_agent".to_string(),
            state: "running".to_string(),
            attention: false,
            reason: Some("Agent started: claude".to_string()),
            updated_at: "before".to_string(),
            telemetry_json: Some(r#"{"activity":"agent","session":"claude"}"#.to_string()),
        };

        assert_eq!(restored_agent_launch_line(&session, &state), None);
    }

    #[test]
    fn restored_spawn_command_resumes_codex_when_persisted_command_launches_agent() {
        let session = PersistedSession {
            session_id: "ses_agent".to_string(),
            workspace_id: "ws_agent".to_string(),
            backend_kind: "wsl-direct".to_string(),
            backend_attachment_id: None,
            backend_native_id: None,
            cwd: Some("/tmp".to_string()),
            command: vec!["codex".to_string()],
            state: "disconnected".to_string(),
            exit_code: None,
            durability: "ephemeral".to_string(),
            created_at: "before".to_string(),
            last_seen_at: None,
            updated_at: "before".to_string(),
        };
        let state = PersistedAgentState {
            session_id: "ses_agent".to_string(),
            workspace_id: "ws_agent".to_string(),
            state: "running".to_string(),
            attention: false,
            reason: Some("Agent started: codex".to_string()),
            updated_at: "before".to_string(),
            telemetry_json: Some(r#"{"activity":"agent","session":"codex"}"#.to_string()),
        };

        assert_eq!(
            restored_spawn_command_for_session(&session, Some(&state)),
            vec!["codex", "--no-alt-screen", "resume", "--last"]
        );
    }

    #[test]
    fn restored_spawn_command_resumes_codex_without_agent_state() {
        let session = PersistedSession {
            session_id: "ses_agent".to_string(),
            workspace_id: "ws_agent".to_string(),
            backend_kind: "wsl-direct".to_string(),
            backend_attachment_id: None,
            backend_native_id: None,
            cwd: Some("/tmp".to_string()),
            command: vec!["codex".to_string()],
            state: "disconnected".to_string(),
            exit_code: None,
            durability: "ephemeral".to_string(),
            created_at: "before".to_string(),
            last_seen_at: None,
            updated_at: "before".to_string(),
        };

        assert_eq!(
            restored_spawn_command_for_session(&session, None),
            vec!["codex", "--no-alt-screen", "resume", "--last"]
        );
    }

    #[test]
    fn failed_durable_recovery_spawn_rolls_back_to_previous_surface() {
        let state = DesktopControlState::new();
        {
            let mut bundle = workspace_bundle_with_unmounted_surface();
            bundle.panes[1].mounted_surface_id = Some("surf_failed".to_string());
            bundle.sessions[0].backend_kind = "wsl-tmux-control".to_string();
            bundle.sessions[0].backend_native_id = Some("agentmux_ses_surface".to_string());
            bundle.sessions[0].state = "recovering".to_string();
            bundle.sessions[0].durability = "durable".to_string();
            bundle.surfaces.push(PersistedSurface {
                surface_id: "surf_failed".to_string(),
                workspace_id: "ws_surface".to_string(),
                surface_type: "terminal".to_string(),
                title: "Failed recovery".to_string(),
                session_id: Some("ses_failed".to_string()),
                browser_id: None,
                created_at: "before".to_string(),
                last_visible_at: None,
                updated_at: "before".to_string(),
            });
            bundle.sessions.push(PersistedSession {
                session_id: "ses_failed".to_string(),
                workspace_id: "ws_surface".to_string(),
                backend_kind: "wsl-tmux-control".to_string(),
                backend_attachment_id: None,
                backend_native_id: Some("agentmux_ses_failed".to_string()),
                cwd: Some("/tmp".to_string()),
                command: vec!["claude".to_string()],
                state: "starting".to_string(),
                exit_code: None,
                durability: "durable".to_string(),
                created_at: "before".to_string(),
                last_seen_at: None,
                updated_at: "before".to_string(),
            });
            state
                .store
                .lock()
                .unwrap()
                .save_workspace_bundle(&bundle)
                .unwrap();
        }

        state.discard_failed_recovery_spawn(
            "ws_surface",
            "pane_left",
            "surf_terminal",
            "ses_surface",
            "ses_failed",
        );

        let bundle = state
            .store
            .lock()
            .unwrap()
            .load_workspace_bundle("ws_surface")
            .unwrap()
            .unwrap();
        let pane = bundle
            .panes
            .iter()
            .find(|pane| pane.pane_id == "pane_left")
            .unwrap();
        assert_eq!(pane.mounted_surface_id.as_deref(), Some("surf_terminal"));
        assert!(!bundle
            .surfaces
            .iter()
            .any(|surface| surface.surface_id == "surf_failed"));
        assert!(!bundle
            .sessions
            .iter()
            .any(|session| session.session_id == "ses_failed"));
        let previous = bundle
            .sessions
            .iter()
            .find(|session| session.session_id == "ses_surface")
            .unwrap();
        assert_eq!(previous.state, "disconnected");
    }

    #[test]
    fn persisted_spawn_detects_agent_command_labels() {
        assert!(is_known_agent_launch(
            "claude --dangerously-skip-permissions"
        ));
        assert!(is_known_agent_launch("codex resume abc123"));
        assert!(!is_known_agent_launch("powershell.exe -NoLogo"));
    }

    #[test]
    fn terminal_input_buffer_detects_manual_agent_launches() {
        let state = DesktopControlState::new();

        assert!(state
            .completed_terminal_input_lines("ses_manual", "cla")
            .is_empty());
        assert_eq!(
            state.completed_terminal_input_lines(
                "ses_manual",
                "ude --dangerously-skip-permissions\r"
            ),
            vec!["claude --dangerously-skip-permissions".to_string()]
        );
        assert!(state
            .completed_terminal_input_lines("ses_manual", "ls\r")
            .is_empty());
        assert!(state
            .completed_terminal_input_lines("ses_manual", "co")
            .is_empty());
        assert_eq!(
            state.completed_terminal_input_lines("ses_manual", "dex resume abc123\n"),
            vec!["codex resume abc123".to_string()]
        );
    }

    #[test]
    fn terminal_input_buffer_handles_edits_and_cancellations() {
        let state = DesktopControlState::new();

        assert!(state
            .completed_terminal_input_lines("ses_edit", "clad")
            .is_empty());
        assert_eq!(
            state.completed_terminal_input_lines("ses_edit", "\u{7f}ude\r\n"),
            vec!["claude".to_string()]
        );

        assert!(state
            .completed_terminal_input_lines("ses_cancel", "claude")
            .is_empty());
        assert!(state
            .completed_terminal_input_lines("ses_cancel", "\u{3}")
            .is_empty());
        assert!(state
            .completed_terminal_input_lines("ses_cancel", "\r")
            .is_empty());
    }

    #[test]
    fn agent_action_cwd_falls_back_to_active_terminal_cwd_when_project_root_is_empty() {
        let state = DesktopControlState::new();
        {
            let mut bundle = workspace_bundle_with_unmounted_surface();
            bundle.panes[1].mounted_surface_id = Some("surf_terminal".to_string());
            bundle.sessions[0].cwd = Some("/mnt/d/projects/agentmux".to_string());
            state
                .store
                .lock()
                .unwrap()
                .save_workspace_bundle(&bundle)
                .unwrap();
        }
        let workspace = {
            let store = state.store.lock().unwrap();
            let bundle = store.load_workspace_bundle("ws_surface").unwrap().unwrap();
            workspace_summary(&bundle.workspace)
        };

        assert_eq!(
            state.agent_action_cwd(&workspace).unwrap().as_deref(),
            Some("/mnt/d/projects/agentmux")
        );
    }

    #[test]
    fn restore_cwd_for_agent_session_uses_workspace_context_when_saved_cwd_is_home() {
        let state = DesktopControlState::new();
        let agent_session = {
            let mut bundle = workspace_bundle_with_unmounted_surface();
            bundle.workspace.active_pane_id = "pane_right".to_string();
            bundle.panes[1].mounted_surface_id = Some("surf_terminal".to_string());
            bundle.panes[2].mounted_surface_id = Some("surf_context".to_string());
            bundle.sessions[0].cwd = Some("~".to_string());
            bundle.sessions[0].command = vec!["codex".to_string()];
            let agent_session = bundle.sessions[0].clone();
            bundle.surfaces.push(PersistedSurface {
                surface_id: "surf_context".to_string(),
                workspace_id: "ws_surface".to_string(),
                surface_type: "terminal".to_string(),
                title: "Context terminal".to_string(),
                session_id: Some("ses_context".to_string()),
                browser_id: None,
                created_at: "before".to_string(),
                last_visible_at: None,
                updated_at: "before".to_string(),
            });
            bundle.sessions.push(PersistedSession {
                session_id: "ses_context".to_string(),
                workspace_id: "ws_surface".to_string(),
                backend_kind: "wsl-direct".to_string(),
                backend_attachment_id: None,
                backend_native_id: None,
                cwd: Some("/mnt/d/projects/chore".to_string()),
                command: vec!["bash".to_string(), "-l".to_string()],
                state: "running".to_string(),
                exit_code: None,
                durability: "ephemeral".to_string(),
                created_at: "before".to_string(),
                last_seen_at: None,
                updated_at: "before".to_string(),
            });
            state
                .store
                .lock()
                .unwrap()
                .save_workspace_bundle(&bundle)
                .unwrap();
            agent_session
        };

        assert_eq!(
            state.restore_cwd_for_session(&agent_session).as_deref(),
            Some("/mnt/d/projects/chore")
        );
    }

    #[test]
    #[cfg(windows)]
    fn restore_ephemeral_terminals_respawns_and_replays_agent_state_idempotently() {
        let state = DesktopControlState::new();
        let project_root = unique_temp_db_path("restore-ephemeral-project-root");
        fs::create_dir_all(&project_root).unwrap();
        let project_root_text = project_root.to_string_lossy().to_string();
        {
            let mut bundle = workspace_bundle_with_unmounted_surface();
            bundle.workspace.project_root = Some(project_root_text.clone());
            bundle.panes[1].mounted_surface_id = Some("surf_terminal".to_string());
            bundle.sessions[0].cwd = env::current_dir()
                .ok()
                .map(|path| path.to_string_lossy().to_string());
            bundle.sessions[0].command = vec![
                "cmd.exe".to_string(),
                "/d".to_string(),
                "/q".to_string(),
                "/c".to_string(),
                "ping -n 4 127.0.0.1 >nul".to_string(),
            ];
            let mut store = state.store.lock().unwrap();
            store.save_workspace_bundle(&bundle).unwrap();
            store
                .upsert_agent_state(&PersistedAgentState {
                    session_id: "ses_surface".to_string(),
                    workspace_id: "ws_surface".to_string(),
                    state: "running".to_string(),
                    attention: false,
                    reason: Some("Agent started: claude".to_string()),
                    updated_at: "before".to_string(),
                    telemetry_json: Some(r#"{"activity":"agent","session":"claude"}"#.to_string()),
                })
                .unwrap();
        }

        state.seed_id_counter();
        state.restore_ephemeral_terminals();
        let snapshot = state.recovery_snapshot().unwrap();
        let terminal_surfaces = snapshot
            .surfaces
            .iter()
            .filter(|surface| surface.surface_type == "terminal")
            .collect::<Vec<_>>();
        assert_eq!(terminal_surfaces.len(), 1);
        assert_ne!(terminal_surfaces[0].surface_id, "surf_terminal");
        let first_session_id = terminal_surfaces[0].session_id.as_ref().unwrap().clone();
        assert_ne!(first_session_id, "ses_surface");
        assert_eq!(
            snapshot
                .panes
                .iter()
                .find(|pane| pane.pane_id == "pane_left")
                .unwrap()
                .mounted_surface_id
                .as_deref(),
            Some(terminal_surfaces[0].surface_id.as_str())
        );
        {
            let store = state.store.lock().unwrap();
            assert!(store.load_agent_state("ses_surface").unwrap().is_none());
            let restored_session = snapshot
                .sessions
                .iter()
                .find(|session| session.session_id == first_session_id)
                .unwrap();
            assert_eq!(
                restored_session.cwd.as_deref(),
                Some(project_root_text.as_str())
            );
            let restored = store.load_agent_state(&first_session_id).unwrap().unwrap();
            assert_eq!(restored.state, "running");
            assert_eq!(restored.reason.as_deref(), Some("Agent restored: claude"));
        }

        {
            let mut store = state.store.lock().unwrap();
            store
                .update_session_state(&first_session_id, "disconnected", None, "later")
                .unwrap();
        }
        state.restore_ephemeral_terminals();
        let snapshot = state.recovery_snapshot().unwrap();
        let terminal_surfaces = snapshot
            .surfaces
            .iter()
            .filter(|surface| surface.surface_type == "terminal")
            .collect::<Vec<_>>();
        assert_eq!(terminal_surfaces.len(), 1);
        assert_eq!(snapshot.sessions.len(), 1);
        assert!(!snapshot
            .sessions
            .iter()
            .any(|session| session.session_id == first_session_id));
        assert_eq!(snapshot.panes.len(), 3);
        let _ = fs::remove_dir_all(project_root);
    }

    #[test]
    fn persisted_terminal_surface_uses_env_title() {
        let params: SessionSpawnParams = serde_json::from_str(
            r#"{"workspace_id":"ws_dock","backend":"wsl-direct","command":["bash","-lc","lazygit"],"cwd":"/tmp","env":[{"key":"AGENTMUX_SURFACE_TITLE","value":"Git Dock"}],"columns":120,"rows":30,"durability":"ephemeral"}"#,
        )
        .unwrap();
        let result = SessionSpawnResult {
            session_id: "ses_dock".to_string(),
        };

        let surface = persisted_terminal_surface(&params, &result, "surf_dock", "now");

        assert_eq!(surface.title, "Git Dock");
    }

    #[test]
    fn persisted_terminal_surface_supports_dock_terminal_type() {
        let params: SessionSpawnParams = serde_json::from_str(
            r#"{"workspace_id":"ws_dock","backend":"wsl-direct","command":["bash","-lc","lazygit"],"cwd":"/tmp","env":[{"key":"AGENTMUX_SURFACE_TYPE","value":"dock-terminal"},{"key":"AGENTMUX_DOCK_CONTROL_ID","value":"git"}],"columns":120,"rows":30,"durability":"ephemeral","placement":"dock"}"#,
        )
        .unwrap();
        let result = SessionSpawnResult {
            session_id: "ses_dock".to_string(),
        };

        let surface = persisted_terminal_surface(&params, &result, "surf_dock", "now");

        assert_eq!(surface.surface_type, "dock-terminal");
        assert_eq!(surface.browser_id.as_deref(), Some("git"));
    }

    #[test]
    fn workspace_bundle_dock_spawn_does_not_create_a_top_tab_pane() {
        let params: SessionSpawnParams = serde_json::from_str(
            r#"{"workspace_id":"ws_dock","backend":"wsl-direct","command":["bash","-lc","lazygit"],"cwd":"/tmp","env":[{"key":"AGENTMUX_SURFACE_TYPE","value":"dock-terminal"},{"key":"AGENTMUX_DOCK_CONTROL_ID","value":"git"}],"columns":120,"rows":30,"durability":"ephemeral","placement":"dock"}"#,
        )
        .unwrap();
        let result = SessionSpawnResult {
            session_id: "ses_dock".to_string(),
        };
        let summary = SessionSummaryResult {
            session_id: "ses_dock".to_string(),
            workspace_id: "ws_dock".to_string(),
            backend_kind: "wsl-direct".to_string(),
            state: "running".to_string(),
            exit_code: None,
            backend_native_id: None,
            cwd: Some("/tmp".to_string()),
        };
        let existing = WorkspaceBundle {
            workspace: PersistedWorkspace {
                workspace_id: "ws_dock".to_string(),
                name: "Dock".to_string(),
                root_pane_id: "pane_root".to_string(),
                active_pane_id: "pane_root".to_string(),
                project_root: Some("/tmp".to_string()),
                environment_profile_id: None,
                description: None,
                icon: None,
                color: None,
                default_wsl_distribution: None,
                default_terminal_profile: None,
                default_agent_command: None,
                created_at: "before".to_string(),
                updated_at: "before".to_string(),
            },
            panes: vec![PersistedPane {
                pane_id: "pane_root".to_string(),
                workspace_id: "ws_dock".to_string(),
                parent_pane_id: None,
                kind: "leaf".to_string(),
                split_axis: None,
                split_ratio: None,
                mounted_surface_id: None,
                last_focused_at: None,
                created_at: "before".to_string(),
                updated_at: "before".to_string(),
            }],
            surfaces: Vec::new(),
            sessions: Vec::new(),
        };

        let bundle = workspace_bundle_from_spawn(&params, &result, &summary, Some(existing));

        assert_eq!(bundle.panes.len(), 1);
        assert_eq!(bundle.workspace.root_pane_id, "pane_root");
        assert_eq!(bundle.surfaces[0].surface_type, "dock-terminal");
        assert_eq!(bundle.surfaces[0].browser_id.as_deref(), Some("git"));
    }

    #[test]
    fn normalize_workspace_pane_tree_breaks_cycles_and_extra_split_children() {
        let mut bundle = workspace_bundle_with_unmounted_surface();
        bundle.panes[0].parent_pane_id = Some("pane_root".to_string());
        bundle.panes.push(PersistedPane {
            pane_id: "pane_extra".to_string(),
            workspace_id: "ws_surface".to_string(),
            parent_pane_id: Some("pane_root".to_string()),
            kind: "leaf".to_string(),
            split_axis: None,
            split_ratio: None,
            mounted_surface_id: None,
            last_focused_at: None,
            created_at: "2026-06-18T00:00:00Z".to_string(),
            updated_at: "2026-06-18T00:00:00Z".to_string(),
        });

        assert!(normalize_workspace_pane_tree(&mut bundle));

        let root = bundle
            .panes
            .iter()
            .find(|pane| pane.pane_id == "pane_root")
            .unwrap();
        let extra = bundle
            .panes
            .iter()
            .find(|pane| pane.pane_id == "pane_extra")
            .unwrap();
        assert_eq!(root.parent_pane_id, None);
        assert_eq!(extra.parent_pane_id, None);
        assert_eq!(bundle.workspace.root_pane_id, "pane_root");
        assert_eq!(bundle.workspace.active_pane_id, "pane_left");
    }

    #[test]
    fn normalize_workspace_pane_tree_dissolves_single_child_split() {
        // A split left with a single child — a half-closed split — must collapse:
        // the surviving child is promoted into the split's place so the pane never
        // renders as a split with one blank half.
        let mut bundle = workspace_bundle_with_unmounted_surface();
        bundle.panes.retain(|pane| pane.pane_id != "pane_right");
        bundle.workspace.active_pane_id = "pane_left".to_string();

        assert!(normalize_workspace_pane_tree(&mut bundle));

        assert!(
            bundle.panes.iter().all(|pane| pane.pane_id != "pane_root"),
            "the degenerate split must be removed"
        );
        let left = bundle
            .panes
            .iter()
            .find(|pane| pane.pane_id == "pane_left")
            .expect("the surviving child is kept");
        assert_eq!(left.parent_pane_id, None);
        assert_eq!(left.kind, "leaf");
        assert_eq!(bundle.workspace.root_pane_id, "pane_left");
        assert_eq!(bundle.workspace.active_pane_id, "pane_left");
        // No split may remain with fewer than two children.
        for pane in &bundle.panes {
            if pane.kind == "split" {
                let children = bundle
                    .panes
                    .iter()
                    .filter(|child| child.parent_pane_id.as_deref() == Some(pane.pane_id.as_str()))
                    .count();
                assert_eq!(children, 2, "split {} must keep two children", pane.pane_id);
            }
        }
    }

    #[test]
    fn normalize_workspace_pane_tree_dedups_and_clears_dangling_surface_mounts() {
        // A dangling mount (surface not in the bundle) is cleared; a valid mount
        // is kept. The fixture's only surface is "surf_terminal".
        let mut bundle = workspace_bundle_with_unmounted_surface();
        for pane in &mut bundle.panes {
            match pane.pane_id.as_str() {
                "pane_left" => pane.mounted_surface_id = Some("surf_terminal".to_string()),
                "pane_right" => pane.mounted_surface_id = Some("surf_ghost".to_string()),
                _ => {}
            }
        }
        assert!(normalize_workspace_pane_tree(&mut bundle));
        let left = bundle
            .panes
            .iter()
            .find(|p| p.pane_id == "pane_left")
            .unwrap();
        let right = bundle
            .panes
            .iter()
            .find(|p| p.pane_id == "pane_right")
            .unwrap();
        assert_eq!(left.mounted_surface_id.as_deref(), Some("surf_terminal"));
        assert_eq!(right.mounted_surface_id, None, "dangling mount cleared");

        // The same surface mounted by two panes is de-duplicated to exactly one.
        let mut dup = workspace_bundle_with_unmounted_surface();
        for pane in &mut dup.panes {
            if pane.kind == "leaf" {
                pane.mounted_surface_id = Some("surf_terminal".to_string());
            }
        }
        normalize_workspace_pane_tree(&mut dup);
        let mount_count = dup
            .panes
            .iter()
            .filter(|p| p.mounted_surface_id.as_deref() == Some("surf_terminal"))
            .count();
        assert_eq!(
            mount_count, 1,
            "surface mounted by exactly one pane after dedup"
        );
    }

    #[test]
    fn id_sequence_parses_domain_id_suffix() {
        assert_eq!(id_sequence("pane_00000042"), 42);
        assert_eq!(id_sequence("ws_00000001"), 1);
        assert_eq!(id_sequence("ses_00012345"), 12345);
        assert_eq!(id_sequence("surf_99999999"), 99_999_999);
        assert_eq!(id_sequence("garbage"), 0);
        assert_eq!(id_sequence(""), 0);
    }

    #[test]
    fn managed_wsl_env_value_includes_user_env_keys() {
        let value = managed_wsl_env_value(&[
            "NO_COLOR".to_string(),
            "AGENTMUX_SURFACE_TITLE".to_string(),
            "WSLENV".to_string(),
        ]);
        let parts = value.split(':').collect::<Vec<_>>();

        assert!(parts.contains(&"AGENTMUX_CONTROL_PIPE"));
        assert!(parts.contains(&"NO_COLOR"));
        assert!(parts.contains(&"AGENTMUX_SURFACE_TITLE"));
        assert!(!parts.contains(&"WSLENV"));
    }

    #[test]
    fn desktop_control_exposes_wsl_distribution_diagnostics() {
        let state = DesktopControlState::new();
        let response = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_wsl_distributions",
                "diagnostics.wsl_distributions",
                "{}",
                DESKTOP_CONTROL_TOKEN,
            ),
        );

        match response.outcome {
            ResponseOutcome::Ok { result_json } => {
                let value: serde_json::Value = serde_json::from_str(&result_json).unwrap();
                assert!(value["distributions"].is_array());
            }
            ResponseOutcome::Error(error) => assert!(matches!(
                error.code,
                ErrorCode::BackendUnavailable | ErrorCode::BackendDegraded
            )),
        }
    }

    #[test]
    #[cfg(windows)]
    fn agentmux_control_spawns_and_reads_conpty_output() {
        let state = DesktopControlState::new();
        let spawn = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_spawn",
                "session.spawn",
                r#"{"workspace_id":"ws_desktop","command":["cmd.exe","/d","/q","/c","echo agentmux-desktop-host %AGENTMUX_SURFACE_ID% %CMUX_SURFACE_ID% %AGENTMUX_PANE_ID% %CMUX_PANE_ID% %TMUX_PANE% %TMUX%"],"cwd":null,"columns":120,"rows":30,"durability":"ephemeral"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let session_id = response_string_field(&spawn, "session_id");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut text = String::new();

        while std::time::Instant::now() < deadline {
            let recent = agentmux_control(
                &state,
                RequestEnvelope::new(
                    "req_recent",
                    "session.read_recent",
                    format!(r#"{{"session_id":"{session_id}","max_bytes":8192}}"#),
                    DESKTOP_CONTROL_TOKEN,
                ),
            );
            text = response_string_field(&recent, "text");
            if text.contains("agentmux-desktop-host") {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        assert!(text.contains("agentmux-desktop-host"));
        let detail = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_workspace_get_after_spawn",
                "workspace.get",
                r#"{"workspace_id":"ws_desktop"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let detail = response_value(&detail);
        let surface_id = detail["surfaces"][0]["surface_id"].as_str().unwrap();
        let pane_id = detail["workspace"]["active_pane_id"].as_str().unwrap();
        assert!(text.contains(surface_id));
        assert!(text.contains(pane_id));
        assert!(text.contains(&format!("%{pane_id}")));
    }

    #[test]
    #[cfg(windows)]
    fn agentmux_control_snapshot_streams_conpty_output_via_offsets() {
        let state = DesktopControlState::new();
        let spawn = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_spawn",
                "session.spawn",
                r#"{"workspace_id":"ws_desktop","command":["cmd.exe","/d","/q","/c","echo agentmux-snapshot-stream"],"cwd":null,"columns":120,"rows":30,"durability":"ephemeral"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let session_id = response_string_field(&spawn, "session_id");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut accumulated: Vec<u8> = Vec::new();
        let mut expected: u64 = 0;

        while std::time::Instant::now() < deadline {
            let snap = agentmux_control(
                &state,
                RequestEnvelope::new(
                    "req_snap",
                    "session.snapshot",
                    format!(r#"{{"session_id":"{session_id}","since_offset":{expected}}}"#),
                    DESKTOP_CONTROL_TOKEN,
                ),
            );
            let snap = response_value(&snap);
            let base_offset = snap["base_offset"].as_u64().unwrap();
            let end_offset = snap["end_offset"].as_u64().unwrap();
            let bytes = BASE64_STANDARD
                .decode(snap["bytes_base64"].as_str().unwrap())
                .unwrap();
            if end_offset != expected {
                if base_offset > expected {
                    accumulated.clear();
                }
                accumulated.extend_from_slice(&bytes);
                expected = end_offset;
            }
            if String::from_utf8_lossy(&accumulated).contains("agentmux-snapshot-stream") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        let text = String::from_utf8_lossy(&accumulated);
        assert!(
            text.contains("agentmux-snapshot-stream"),
            "snapshot offset stream did not deliver output; got: {text:?}"
        );
    }

    // Diagnostic probe (not a CI gate): spawns the EXACT login-shell command the
    // desktop UI uses for a WSL terminal and logs, millisecond-by-millisecond,
    // when prompt bytes land in the snapshot ring. Run explicitly:
    //   cargo test -p agentmux-desktop-host wsl_snapshot_latency_probe -- --ignored --nocapture
    #[test]
    #[cfg(windows)]
    #[ignore = "requires a real WSL distribution; run explicitly with --ignored --nocapture"]
    fn wsl_snapshot_latency_probe() {
        let state = DesktopControlState::new();
        // Spawn three WSL login shells back-to-back. Terminal 1 pays any cold
        // WSL-VM boot; if 2 and 3 are also slow, the cost is per-spawn (not a
        // one-time boot) and pre-warming alone won't fix it.
        for round in 1..=3u32 {
            let params = serde_json::json!({
                "workspace_id": "ws_desktop",
                "backend": "wsl-direct",
                "command": ["sh","-c","login_shell=\"$(getent passwd \"$(id -un)\" 2>/dev/null | cut -d: -f7)\"; exec \"${login_shell:-${SHELL:-/bin/bash}}\" -l"],
                "cwd": serde_json::Value::Null,
                "columns": 120,
                "rows": 30,
                "durability": "ephemeral",
                "placement": "new_tab"
            })
            .to_string();
            let spawn_started = std::time::Instant::now();
            let spawn = agentmux_control(
                &state,
                RequestEnvelope::new("req_spawn", "session.spawn", params, DESKTOP_CONTROL_TOKEN),
            );
            let spawn_ms = spawn_started.elapsed().as_millis();
            let session_id = match &spawn.outcome {
                ResponseOutcome::Ok { result_json } => {
                    let v: serde_json::Value = serde_json::from_str(result_json).unwrap();
                    v["session_id"].as_str().unwrap().to_string()
                }
                ResponseOutcome::Error(error) => panic!("spawn failed: {error:?}"),
            };
            let start = std::time::Instant::now();
            let deadline = start + std::time::Duration::from_secs(15);
            let mut accumulated: Vec<u8> = Vec::new();
            let mut expected: u64 = 0;
            let mut first_byte_ms: Option<u128> = None;
            while std::time::Instant::now() < deadline {
                let snap = agentmux_control(
                    &state,
                    RequestEnvelope::new(
                        "req_snap",
                        "session.snapshot",
                        format!(r#"{{"session_id":"{session_id}","since_offset":{expected}}}"#),
                        DESKTOP_CONTROL_TOKEN,
                    ),
                );
                if let ResponseOutcome::Ok { result_json } = &snap.outcome {
                    let v: serde_json::Value = serde_json::from_str(result_json).unwrap();
                    let base_offset = v["base_offset"].as_u64().unwrap();
                    let end_offset = v["end_offset"].as_u64().unwrap();
                    let bytes = BASE64_STANDARD
                        .decode(v["bytes_base64"].as_str().unwrap())
                        .unwrap();
                    if end_offset != expected {
                        if base_offset > expected {
                            accumulated.clear();
                        }
                        accumulated.extend_from_slice(&bytes);
                        expected = end_offset;
                        if first_byte_ms.is_none() && !accumulated.is_empty() {
                            first_byte_ms = Some(start.elapsed().as_millis());
                            // First prompt arrived; stop early to start next round.
                            break;
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            eprintln!(
                "ROUND {round}: session={session_id} spawn_call={spawn_ms}ms first_byte={first_byte_ms:?}ms bytes={}",
                accumulated.len()
            );
        }
    }

    // Diagnostic probe (not a CI gate): verifies the WSL pre-warm keepalive both
    // (a) holds the VM while its stdin pipe is held and cleans up when the pipe
    // closes — crash-safe, no leaked VM — and (b) makes a real terminal spawn
    // warm-fast (~0.35s) instead of paying the ~5s cold boot.
    //   cargo test -p agentmux-desktop-host wsl_prewarm_keepalive_probe -- --ignored --nocapture
    #[test]
    #[cfg(windows)]
    #[ignore = "requires a real WSL distribution; run explicitly with --ignored --nocapture"]
    fn wsl_prewarm_keepalive_probe() {
        // --- Part A: anchor lifecycle ---
        let mut anchor = Command::new("wsl.exe");
        hide_console_window(&mut anchor);
        anchor
            .args(["--", "cat"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut child = anchor.spawn().expect("spawn anchor");
        let stdin = child.stdin.take();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(
            child.try_wait().unwrap().is_none(),
            "anchor must stay alive while its stdin pipe is held (keeps the VM warm)"
        );
        drop(stdin); // close the pipe -> cat sees EOF -> exits
        let mut exited = false;
        for _ in 0..50 {
            if child.try_wait().unwrap().is_some() {
                exited = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(
            exited,
            "anchor must exit shortly after its stdin pipe closes (no leaked VM)"
        );
        eprintln!("PART A ok: anchor held the VM, then exited on pipe close");

        // --- Part B: pre-warmed spawn is warm-fast ---
        let mut warm = Command::new("wsl.exe");
        hide_console_window(&mut warm);
        warm.args(["--", "cat"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut warm_child = warm.spawn().expect("spawn warm anchor");
        let warm_stdin = warm_child.stdin.take();
        std::thread::sleep(std::time::Duration::from_secs(7)); // let the VM boot

        let state = DesktopControlState::new();
        let params = serde_json::json!({
            "workspace_id": "ws_desktop",
            "backend": "wsl-direct",
            "command": ["sh","-c","login_shell=\"$(getent passwd \"$(id -un)\" 2>/dev/null | cut -d: -f7)\"; exec \"${login_shell:-${SHELL:-/bin/bash}}\" -l"],
            "cwd": serde_json::Value::Null,
            "columns": 120,
            "rows": 30,
            "durability": "ephemeral"
        })
        .to_string();
        let spawn = agentmux_control(
            &state,
            RequestEnvelope::new("rq", "session.spawn", params, DESKTOP_CONTROL_TOKEN),
        );
        let session_id = match &spawn.outcome {
            ResponseOutcome::Ok { result_json } => {
                let v: serde_json::Value = serde_json::from_str(result_json).unwrap();
                v["session_id"].as_str().unwrap().to_string()
            }
            ResponseOutcome::Error(error) => panic!("spawn failed: {error:?}"),
        };
        let start = std::time::Instant::now();
        let deadline = start + std::time::Duration::from_secs(10);
        let expected: u64 = 0;
        let mut first_byte_ms: Option<u128> = None;
        while std::time::Instant::now() < deadline {
            let snap = agentmux_control(
                &state,
                RequestEnvelope::new(
                    "rs",
                    "session.snapshot",
                    format!(r#"{{"session_id":"{session_id}","since_offset":{expected}}}"#),
                    DESKTOP_CONTROL_TOKEN,
                ),
            );
            if let ResponseOutcome::Ok { result_json } = &snap.outcome {
                let v: serde_json::Value = serde_json::from_str(result_json).unwrap();
                let end_offset = v["end_offset"].as_u64().unwrap();
                if end_offset != expected {
                    first_byte_ms = Some(start.elapsed().as_millis());
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        drop(warm_stdin);
        let _ = warm_child.wait();
        eprintln!("PART B: pre-warmed spawn first_byte={first_byte_ms:?}ms (cold was ~5149ms)");
        assert!(
            first_byte_ms.is_some_and(|ms| ms < 2500),
            "pre-warmed WSL spawn should be warm-fast (<2500ms); got {first_byte_ms:?}ms"
        );
    }

    #[test]
    #[cfg(windows)]
    fn new_tab_spawn_preserves_existing_split() {
        let state = DesktopControlState::new();
        let _ = agentmux_control(
            &state,
            RequestEnvelope::new(
                "r1",
                "session.spawn",
                r#"{"workspace_id":"ws_desktop","command":["cmd.exe","/d","/q","/c","echo a"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let detail = response_value(&agentmux_control(
            &state,
            RequestEnvelope::new(
                "r2",
                "workspace.get",
                r#"{"workspace_id":"ws_desktop"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        ));
        let p1 = detail["workspace"]["active_pane_id"]
            .as_str()
            .unwrap()
            .to_string();
        // Splitting p1 must succeed (response_value panics on a control error).
        let _ = response_value(&agentmux_control(
            &state,
            RequestEnvelope::new(
                "r3",
                "pane.split",
                format!(
                    r#"{{"workspace_id":"ws_desktop","pane_id":"{p1}","axis":"vertical","ratio":0.5}}"#
                ),
                DESKTOP_CONTROL_TOKEN,
            ),
        ));
        let _ = agentmux_control(
            &state,
            RequestEnvelope::new(
                "r4",
                "session.spawn",
                r#"{"workspace_id":"ws_desktop","command":["cmd.exe","/d","/q","/c","echo b"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral","placement":"new_tab"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let final_detail = response_value(&agentmux_control(
            &state,
            RequestEnvelope::new(
                "r5",
                "workspace.get",
                r#"{"workspace_id":"ws_desktop"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        ));
        // The split subtree (P1 + its 2 children) must survive the new-tab spawn.
        let panes = final_detail["panes"].as_array().unwrap();
        let children_of_p1 = panes
            .iter()
            .filter(|p| p["parent_pane_id"].as_str() == Some(p1.as_str()))
            .count();
        assert_eq!(
            children_of_p1, 2,
            "P1 must keep its 2 split children after a new-tab spawn"
        );
    }

    #[test]
    fn move_surface_tab_between_workspaces_preserves_split_subtree() {
        let mut source = workspace_bundle_with_unmounted_surface();
        source.panes[1].mounted_surface_id = Some("surf_terminal".to_string());
        let mut target = workspace_bundle_with_unmounted_surface();
        target.workspace.workspace_id = "ws_target".to_string();
        target.workspace.name = "Target workspace".to_string();
        for pane in &mut target.panes {
            pane.workspace_id = "ws_target".to_string();
            pane.pane_id = format!("target_{}", pane.pane_id);
            pane.parent_pane_id = pane
                .parent_pane_id
                .as_ref()
                .map(|parent_id| format!("target_{parent_id}"));
        }
        target.workspace.root_pane_id = "target_pane_root".to_string();
        target.workspace.active_pane_id = "target_pane_left".to_string();
        target.surfaces.clear();
        target.sessions.clear();

        let moved_sessions =
            move_surface_tab_between_workspaces(&mut source, &mut target, "surf_terminal")
                .expect("move surface tab");

        assert_eq!(moved_sessions, vec!["ses_surface".to_string()]);
        assert!(source
            .surfaces
            .iter()
            .all(|surface| surface.surface_id != "surf_terminal"));
        assert!(source
            .sessions
            .iter()
            .all(|session| session.session_id != "ses_surface"));
        assert_eq!(
            target
                .surfaces
                .iter()
                .find(|surface| surface.surface_id == "surf_terminal")
                .unwrap()
                .workspace_id,
            "ws_target"
        );
        assert_eq!(
            target
                .sessions
                .iter()
                .find(|session| session.session_id == "ses_surface")
                .unwrap()
                .workspace_id,
            "ws_target"
        );
        assert!(target.panes.iter().any(|pane| pane.pane_id == "pane_root"
            && pane.workspace_id == "ws_target"
            && pane.parent_pane_id.is_none()));
    }

    #[test]
    #[cfg(windows)]
    fn desktop_host_persists_spawn_metadata_for_recovery() {
        let path = unique_temp_db_path("desktop_host_persists_spawn_metadata_for_recovery");
        let state = DesktopControlState::open(&path).unwrap();
        let spawn = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_spawn",
                "session.spawn",
                r#"{"workspace_id":"ws_desktop","command":["cmd.exe","/d","/q","/c","echo agentmux-persisted"],"cwd":null,"columns":120,"rows":30,"durability":"ephemeral"}"#,
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let session_id = response_string_field(&spawn, "session_id");
        drop(state);

        let reopened = DesktopControlState::open(&path).unwrap();
        let snapshot = reopened.recovery_snapshot().unwrap();
        let persisted = snapshot
            .sessions
            .iter()
            .find(|session| session.session_id == session_id)
            .expect("persisted desktop session");

        assert_eq!(snapshot.workspaces[0].workspace_id, "ws_desktop");
        assert_eq!(persisted.state, "disconnected");
        assert_eq!(persisted.backend_native_id, None);
        assert_eq!(
            persisted.command,
            vec!["cmd.exe", "/d", "/q", "/c", "echo agentmux-persisted"]
        );

        cleanup_temp_db(&path);
    }

    fn response_string_field(response: &ResponseEnvelope, field: &str) -> String {
        response_value(response)
            .get(field)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("missing string field '{field}'"))
            .to_string()
    }

    fn response_error_code(response: &ResponseEnvelope) -> ErrorCode {
        match &response.outcome {
            ResponseOutcome::Ok { result_json } => {
                panic!("expected control error but got ok response: {result_json}")
            }
            ResponseOutcome::Error(error) => error.code,
        }
    }

    fn response_error_message(response: &ResponseEnvelope) -> String {
        match &response.outcome {
            ResponseOutcome::Ok { result_json } => {
                panic!("expected control error but got ok response: {result_json}")
            }
            ResponseOutcome::Error(error) => error.message.clone(),
        }
    }

    fn response_value(response: &ResponseEnvelope) -> serde_json::Value {
        let result_json = match &response.outcome {
            ResponseOutcome::Ok { result_json } => result_json,
            ResponseOutcome::Error(error) => panic!("unexpected control error: {error:?}"),
        };
        serde_json::from_str(result_json).unwrap()
    }

    fn unique_temp_db_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("agentmux-{name}-{nanos}.sqlite3"))
    }

    fn cleanup_temp_db(path: &Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(path.with_extension("sqlite3-wal"));
        let _ = fs::remove_file(path.with_extension("sqlite3-shm"));
    }

    #[derive(Default)]
    struct RecordingDesktopNotificationAdapter {
        delivered: Mutex<Vec<DesktopNotification>>,
    }

    impl RecordingDesktopNotificationAdapter {
        fn delivered(&self) -> Vec<DesktopNotification> {
            self.delivered.lock().unwrap().clone()
        }
    }

    impl DesktopNotificationAdapter for RecordingDesktopNotificationAdapter {
        fn notify(&self, notification: DesktopNotification) {
            self.delivered.lock().unwrap().push(notification);
        }
    }

    fn workspace_bundle_with_unmounted_surface() -> WorkspaceBundle {
        WorkspaceBundle {
            workspace: PersistedWorkspace {
                workspace_id: "ws_surface".to_string(),
                name: "Surface workspace".to_string(),
                root_pane_id: "pane_root".to_string(),
                active_pane_id: "pane_left".to_string(),
                project_root: None,
                environment_profile_id: None,
                description: None,
                icon: None,
                color: None,
                default_wsl_distribution: None,
                default_terminal_profile: None,
                default_agent_command: None,
                created_at: "2026-06-18T00:00:00Z".to_string(),
                updated_at: "2026-06-18T00:00:00Z".to_string(),
            },
            panes: vec![
                PersistedPane {
                    pane_id: "pane_root".to_string(),
                    workspace_id: "ws_surface".to_string(),
                    parent_pane_id: None,
                    kind: "split".to_string(),
                    split_axis: Some("vertical".to_string()),
                    split_ratio: Some(0.5),
                    mounted_surface_id: None,
                    last_focused_at: None,
                    created_at: "2026-06-18T00:00:00Z".to_string(),
                    updated_at: "2026-06-18T00:00:00Z".to_string(),
                },
                PersistedPane {
                    pane_id: "pane_left".to_string(),
                    workspace_id: "ws_surface".to_string(),
                    parent_pane_id: Some("pane_root".to_string()),
                    kind: "leaf".to_string(),
                    split_axis: None,
                    split_ratio: None,
                    mounted_surface_id: None,
                    last_focused_at: Some("2026-06-18T00:00:00Z".to_string()),
                    created_at: "2026-06-18T00:00:00Z".to_string(),
                    updated_at: "2026-06-18T00:00:00Z".to_string(),
                },
                PersistedPane {
                    pane_id: "pane_right".to_string(),
                    workspace_id: "ws_surface".to_string(),
                    parent_pane_id: Some("pane_root".to_string()),
                    kind: "leaf".to_string(),
                    split_axis: None,
                    split_ratio: None,
                    mounted_surface_id: None,
                    last_focused_at: None,
                    created_at: "2026-06-18T00:00:00Z".to_string(),
                    updated_at: "2026-06-18T00:00:00Z".to_string(),
                },
            ],
            surfaces: vec![PersistedSurface {
                surface_id: "surf_terminal".to_string(),
                workspace_id: "ws_surface".to_string(),
                surface_type: "terminal".to_string(),
                title: "Detached terminal".to_string(),
                session_id: Some("ses_surface".to_string()),
                browser_id: None,
                created_at: "2026-06-18T00:00:00Z".to_string(),
                last_visible_at: None,
                updated_at: "2026-06-18T00:00:00Z".to_string(),
            }],
            sessions: vec![PersistedSession {
                session_id: "ses_surface".to_string(),
                workspace_id: "ws_surface".to_string(),
                backend_kind: "conpty".to_string(),
                backend_attachment_id: None,
                backend_native_id: None,
                cwd: None,
                command: vec!["cmd.exe".to_string()],
                state: "disconnected".to_string(),
                exit_code: None,
                durability: "ephemeral".to_string(),
                created_at: "2026-06-18T00:00:00Z".to_string(),
                last_seen_at: None,
                updated_at: "2026-06-18T00:00:00Z".to_string(),
            }],
        }
    }
}
