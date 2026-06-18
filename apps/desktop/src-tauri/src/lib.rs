use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::fmt;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agentmux_backend::{
    AttachRequest, BackendError, BackendEvent, BackendKind as BackendTraitKind, BackendResult,
    InputEvent, SessionBackend, SessionHandle, SpawnRequest, TerminalSize, TerminationMode,
};
use agentmux_backend_conpty::ConptyBackend;
use agentmux_backend_ssh::SshDirectBackend;
use agentmux_backend_tmux::TmuxControlBackend;
use agentmux_backend_wsl::{
    discover_wsl_distributions as discover_wsl_distributions_from_backend, WslDiagnosticCode,
    WslDirectBackend, WslDirectConfig, WslDistribution,
};
use agentmux_browser::{
    BrowserAutomation, BrowserAutomationError, BrowserAutomationErrorCode, BrowserCommand,
    BrowserCommandResult, BrowserSurface, CdpBrowserAutomation, InMemoryBrowserAutomation,
};
use agentmux_core::{PaneId, RuntimeControlPlane, SurfaceId, TerminalRuntime, WorkspaceId};
use agentmux_ipc::{
    AckResult, AgentAttentionListResult, AgentListAttentionParams, AgentStateResult,
    AgentTelemetry, BrowserActionResult, BrowserClickParams, BrowserDiagnosticResult,
    BrowserDiagnosticsParams, BrowserDiagnosticsResult, BrowserDomSnapshotParams,
    BrowserDomSnapshotResult, BrowserEvaluateParams, BrowserEvaluateResult, BrowserNavigateParams,
    BrowserNavigationResult, BrowserScreenshotParams, BrowserScreenshotResult, BrowserTypeParams,
    ControlError, ControlPipeConnection, DiagnosticsBackendHealthResult, DiagnosticsExportResult,
    DiagnosticsQueuePressureResult, ErrorCode, EventSubscribeParams, EventSubscribeResult,
    NotificationDismissParams, NotificationListParams, NotificationListResult,
    NotificationSummaryResult, PaneCloseParams, PaneFocusParams, PaneMountSurfaceParams,
    PaneResizeLayoutParams, PaneSplitParams, PaneSummaryResult, PaneUnmountSurfaceParams,
    ProfileCreateParams, ProfileIdParams, ProfileListResult, ProfileSummaryResult,
    ProfileUpdateParams, RecoveryDiagnosticsResult, RecoverySessionResult, RequestEnvelope,
    ResponseEnvelope, ResponseOutcome, SessionIdParams, SessionSpawnParams, SessionSpawnResult,
    SessionSummaryResult, SurfaceCreateBrowserParams, SurfaceSummaryResult, WorkspaceCloseParams,
    WorkspaceCloseResult, WorkspaceCreateParams, WorkspaceDetailResult, WorkspaceIdParams,
    WorkspaceListResult, WorkspaceRenameParams, WorkspaceSummaryResult, WslDistributionListResult,
    WslDistributionResult, DEFAULT_CONTROL_PIPE_NAME, DEFAULT_LOCAL_CONTROL_TOKEN,
};
use agentmux_store::{
    PersistedAgentState, PersistedNotification, PersistedPane, PersistedProfile, PersistedSession,
    PersistedSurface, PersistedWorkspace, RecoverySnapshot, SqliteStore, StoreError,
    WorkspaceBundle,
};

pub const DESKTOP_CONTROL_TOKEN: &str = DEFAULT_LOCAL_CONTROL_TOKEN;
const MAX_BROWSER_FAILURES: usize = 100;

type DesktopRuntimeControl = RuntimeControlPlane<DesktopBackendRouter>;

pub struct DesktopBackendRouter {
    conpty: ConptyBackend,
    wsl_direct: WslDirectBackend,
    tmux_control: TmuxControlBackend,
    ssh: SshDirectBackend,
    routes: HashMap<String, BackendTraitKind>,
}

impl DesktopBackendRouter {
    pub fn new() -> Self {
        Self {
            conpty: ConptyBackend::new(),
            wsl_direct: WslDirectBackend::with_config(WslDirectConfig::default()),
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

pub struct DesktopControlState {
    control: Mutex<DesktopRuntimeControl>,
    store: Mutex<SqliteStore>,
    browser: Mutex<Box<dyn BrowserAutomation>>,
    browser_failures: Mutex<VecDeque<BrowserFailureRecord>>,
    browser_failure_counter: Mutex<u64>,
    control_token: String,
    desktop_notifications: Mutex<DesktopNotificationState>,
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

#[derive(Default)]
struct DesktopNotificationState {
    adapter: Option<Arc<dyn DesktopNotificationAdapter>>,
    delivered_notification_ids: HashSet<String>,
}

impl DesktopControlState {
    pub fn new() -> Self {
        Self::new_in_memory().expect("failed to initialize in-memory desktop state")
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, DesktopHostError> {
        Self::open_with_token(path, DESKTOP_CONTROL_TOKEN)
    }

    pub fn open_with_token(
        path: impl AsRef<Path>,
        token: impl Into<String>,
    ) -> Result<Self, DesktopHostError> {
        let token = token.into();
        let runtime = TerminalRuntime::new(DesktopBackendRouter::new());
        let state = Self {
            control: Mutex::new(RuntimeControlPlane::new(runtime, token.clone())),
            store: Mutex::new(SqliteStore::open(path)?),
            browser: Mutex::new(browser_automation_from_environment()?),
            browser_failures: Mutex::new(VecDeque::new()),
            browser_failure_counter: Mutex::new(0),
            control_token: token,
            desktop_notifications: Mutex::new(DesktopNotificationState::default()),
        };
        state.recover_durable_sessions();
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
            control_token: token,
            desktop_notifications: Mutex::new(DesktopNotificationState::default()),
        };
        state.recover_durable_sessions();
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
        if is_desktop_store_method(&request.method) {
            return self.handle_desktop_store_request(request);
        }

        let id = request.id.clone();
        let request_for_persistence = request.clone();
        let Ok(mut control) = self.control.lock() else {
            return ResponseEnvelope::error(
                id,
                ControlError::new(ErrorCode::Conflict, "Desktop control state is unavailable."),
            );
        };

        control.collect_events();
        if let Some(error) = self.persist_agent_snapshots(&control, &id) {
            return error;
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

    fn recover_durable_sessions(&self) {
        let Ok(snapshot) = self.recovery_snapshot() else {
            return;
        };
        let workspace_profiles = snapshot
            .workspaces
            .iter()
            .map(|workspace| {
                (
                    workspace.workspace_id.clone(),
                    workspace.environment_profile_id.clone(),
                )
            })
            .collect::<HashMap<_, _>>();

        for session in snapshot.sessions {
            if !should_attach_recovering_session(&session) {
                continue;
            }
            let Some(backend_ref) = session.backend_native_id.clone() else {
                continue;
            };
            let backend_profile = workspace_profiles
                .get(&session.workspace_id)
                .cloned()
                .flatten();
            let params_json = serde_json::json!({
                "session_id": session.session_id,
                "workspace_id": session.workspace_id,
                "backend": session.backend_kind,
                "backend_profile": backend_profile,
                "backend_ref": backend_ref,
                "columns": 120,
                "rows": 30,
                "durability": "durable",
            })
            .to_string();

            let response = self.handle_request(RequestEnvelope::new(
                "desktop_startup_recover_durable_session",
                "session.attach",
                params_json,
                self.control_token.clone(),
            ));
            if matches!(response.outcome, ResponseOutcome::Ok { .. }) {
                let _ = self.persist_session_summary_from_id(
                    match response_result_json::<SessionSpawnResult>(&response) {
                        Ok(result) => result.session_id,
                        Err(_) => continue,
                    },
                );
            }
        }
    }

    fn handle_desktop_store_request(&self, request: RequestEnvelope) -> ResponseEnvelope {
        let id = request.id.clone();
        if let Err(error) = validate_desktop_request(&request, &self.control_token) {
            return ResponseEnvelope::error(id, error);
        }

        let response = match request.method.as_str() {
            "workspace.create" => self.handle_workspace_create(&request),
            "workspace.list" => self.handle_workspace_list(&request),
            "workspace.get" => self.handle_workspace_get(&request),
            "workspace.rename" => self.handle_workspace_rename(&request),
            "workspace.close" => self.handle_workspace_close(&request),
            "pane.split" => self.handle_pane_split(&request),
            "pane.focus" => self.handle_pane_focus(&request),
            "pane.close" => self.handle_pane_close(&request),
            "pane.resize_layout" => self.handle_pane_resize_layout(&request),
            "pane.mount_surface" => self.handle_pane_mount_surface(&request),
            "pane.unmount_surface" => self.handle_pane_unmount_surface(&request),
            "surface.create_browser" => self.handle_surface_create_browser(&request),
            "browser.navigate" => self.handle_browser_navigate(&request),
            "browser.screenshot" => self.handle_browser_screenshot(&request),
            "browser.dom_snapshot" => self.handle_browser_dom_snapshot(&request),
            "browser.click" => self.handle_browser_click(&request),
            "browser.type" => self.handle_browser_type(&request),
            "browser.evaluate" => self.handle_browser_evaluate(&request),
            "agent.get_state" => self.handle_agent_get_state(&request),
            "agent.list_attention" => self.handle_agent_list_attention(&request),
            "agent.list" => self.handle_agent_list(&request),
            "notification.list" => self.handle_notification_list(&request),
            "notification.dismiss" => self.handle_notification_dismiss(&request),
            "profile.list" => self.handle_profile_list(&request),
            "profile.create" => self.handle_profile_create(&request),
            "profile.update" => self.handle_profile_update(&request),
            "profile.delete" => self.handle_profile_delete(&request),
            "diagnostics.browser" => self.handle_browser_diagnostics(&request),
            "diagnostics.export" => self.handle_diagnostics_export(&request),
            "diagnostics.recovery" => self.handle_recovery_diagnostics(&request),
            "diagnostics.wsl_distributions" => self.handle_wsl_distributions(&request),
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
            "session.get" => self.persist_session_summary(response),
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
        Ok(())
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
        let pane_id = params
            .pane_id
            .clone()
            .unwrap_or_else(|| bundle.workspace.active_pane_id.clone());
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
        let now = timestamp();
        let surface = persisted_browser_surface(&browser_surface, &now);
        bundle.surfaces.push(surface.clone());
        mount_surface_in_bundle(&mut bundle, &pane_id, &surface_id)?;
        store.save_workspace_bundle(&bundle)?;

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &surface_summary(&surface),
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

    fn handle_browser_click(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: BrowserClickParams = request.parse_params()?;
        let command = if let Some(selector) = params.selector {
            BrowserCommand::ClickSelector {
                surface_id: params.surface_id,
                selector,
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

        Ok(DiagnosticsExportResult {
            generated_at: timestamp(),
            format_version: "agentmux.diagnostics.v1".to_string(),
            recovery,
            browser,
            notifications,
            backend_health,
            queue_pressure,
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
        let Ok(store) = self.store.lock() else {
            return Err(DesktopHostError::StateUnavailable(
                "desktop store state is unavailable".to_string(),
            ));
        };
        store
            .load_workspace_bundle(workspace_id)?
            .ok_or_else(|| workspace_not_found(workspace_id).into())
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

fn workspace_bundle_from_spawn(
    params: &SessionSpawnParams,
    result: &SessionSpawnResult,
    summary: &SessionSummaryResult,
    existing: Option<WorkspaceBundle>,
) -> WorkspaceBundle {
    let now = timestamp();
    let surface_id = format!("surf_{}", result.session_id);
    let durability = params
        .durability
        .clone()
        .unwrap_or_else(|| "ephemeral".to_string());

    let surface = persisted_terminal_surface(params, result, &surface_id, &now);
    let session = persisted_terminal_session(params, result, summary, durability, &now);

    if let Some(mut bundle) = existing {
        bundle.workspace.updated_at = now.clone();
        if let Some(active_pane) = bundle
            .panes
            .iter_mut()
            .find(|pane| pane.pane_id == bundle.workspace.active_pane_id)
        {
            active_pane.mounted_surface_id = Some(surface_id.clone());
            active_pane.last_focused_at = Some(now.clone());
            active_pane.updated_at = now;
        }
        bundle.surfaces.push(surface);
        bundle.sessions.push(session);
        return bundle;
    }

    let pane_id = PaneId::new().to_string();
    WorkspaceBundle {
        workspace: PersistedWorkspace {
            workspace_id: params.workspace_id.clone(),
            name: params.workspace_id.clone(),
            root_pane_id: pane_id.clone(),
            active_pane_id: pane_id.clone(),
            project_root: params.cwd.clone(),
            environment_profile_id: None,
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

fn persisted_terminal_surface(
    params: &SessionSpawnParams,
    result: &SessionSpawnResult,
    surface_id: &str,
    now: &str,
) -> PersistedSurface {
    PersistedSurface {
        surface_id: surface_id.to_string(),
        workspace_id: params.workspace_id.clone(),
        surface_type: "terminal".to_string(),
        title: params
            .command
            .first()
            .cloned()
            .unwrap_or_else(|| "terminal".to_string()),
        session_id: Some(result.session_id.clone()),
        browser_id: None,
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
        "workspace.create"
            | "workspace.list"
            | "workspace.get"
            | "workspace.rename"
            | "workspace.close"
            | "pane.split"
            | "pane.focus"
            | "pane.close"
            | "pane.resize_layout"
            | "pane.mount_surface"
            | "pane.unmount_surface"
            | "surface.create_browser"
            | "browser.navigate"
            | "browser.screenshot"
            | "browser.dom_snapshot"
            | "browser.click"
            | "browser.type"
            | "browser.evaluate"
            | "agent.get_state"
            | "agent.list_attention"
            | "agent.list"
            | "notification.list"
            | "notification.dismiss"
            | "profile.list"
            | "profile.create"
            | "profile.update"
            | "profile.delete"
            | "diagnostics.browser"
            | "diagnostics.export"
            | "diagnostics.recovery"
            | "diagnostics.wsl_distributions"
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

    if request.auth.token != expected_token {
        return Err(ControlError::new(
            ErrorCode::Unauthorized,
            "Invalid local control token.",
        ));
    }

    Ok(())
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

fn browser_error_from_automation(error: BrowserAutomationError) -> DesktopHostError {
    let code = match error.code {
        BrowserAutomationErrorCode::SurfaceNotFound => ErrorCode::SurfaceNotFound,
        BrowserAutomationErrorCode::InvalidRequest => ErrorCode::InvalidRequest,
        BrowserAutomationErrorCode::AutomationFailed => ErrorCode::BackendDegraded,
    };
    DesktopHostError::Control(ControlError::new(code, error.message))
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

fn workspace_summary(workspace: &PersistedWorkspace) -> WorkspaceSummaryResult {
    WorkspaceSummaryResult {
        workspace_id: workspace.workspace_id.clone(),
        name: workspace.name.clone(),
        root_pane_id: workspace.root_pane_id.clone(),
        active_pane_id: workspace.active_pane_id.clone(),
        project_root: workspace.project_root.clone(),
        environment_profile_id: workspace.environment_profile_id.clone(),
    }
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

fn persisted_session_summary(session: &PersistedSession) -> SessionSummaryResult {
    SessionSummaryResult {
        session_id: session.session_id.clone(),
        workspace_id: session.workspace_id.clone(),
        backend_kind: session.backend_kind.clone(),
        state: session.state.clone(),
        exit_code: session.exit_code,
        backend_native_id: session.backend_native_id.clone(),
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
        "agent.needs_input" | "agent.completed" | "agent.failed" | "browser.action_failed"
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
                r#"{"name":"Demo workspace","project_root":"D:\\Workspace\\irae\\agentmux","backend_profile":"local"}"#,
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

        let snapshot = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_snapshot",
                "browser.dom_snapshot",
                format!(r#"{{"surface_id":"{surface_id}"}}"#),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        let html = response_string_field(&snapshot, "html");
        assert!(html.contains(&surface_id));
        assert!(html.contains("https://example.invalid"));

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
                format!(r##"{{"surface_id":"{surface_id}","selector":"#login"}}"##),
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
                format!(r##"{{"surface_id":"{surface_id}","selector":"#q","text":"agentmux"}}"##),
                DESKTOP_CONTROL_TOKEN,
            ),
        );
        assert_eq!(response_value(&typed)["ok"], true);

        let evaluated = agentmux_control(
            &state,
            RequestEnvelope::new(
                "req_browser_evaluate",
                "browser.evaluate",
                format!(r#"{{"surface_id":"{surface_id}","script":"document.title"}}"#),
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
                r#"{"workspace_id":"ws_desktop","command":["cmd.exe","/d","/q","/c","echo agentmux-desktop-host"],"cwd":null,"columns":120,"rows":30,"durability":"ephemeral"}"#,
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
