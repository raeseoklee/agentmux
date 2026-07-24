use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CONTROL_SCHEMA: &str = "agentmux.control.v1";
pub const EVENT_SCHEMA: &str = "agentmux.event.v1";
pub const DEFAULT_CONTROL_PIPE_NAME: &str = r"\\.\pipe\agentmux-control";
pub const DEFAULT_LOCAL_CONTROL_TOKEN: &str = "desktop-bootstrap-token";
pub const DEFAULT_CONTROL_TOKEN_FILE_NAME: &str = "control.token";

/// Stable control-plane method names for the Git operations surface. Keeping
/// these in the IPC crate prevents the desktop, server, CLI, and MCP adapters
/// from silently drifting into separate APIs.
pub const METHOD_GIT_STATUS_SUMMARY: &str = "git.status_summary";
pub const METHOD_GIT_STATUS_PAGE: &str = "git.status_page";
pub const METHOD_GIT_DIFF: &str = "git.diff";
pub const METHOD_GIT_STAGE: &str = "git.stage";
pub const METHOD_GIT_UNSTAGE: &str = "git.unstage";
pub const METHOD_GIT_STAGE_ALL: &str = "git.stage_all";
pub const METHOD_GIT_UNSTAGE_ALL: &str = "git.unstage_all";
pub const METHOD_GIT_DISCARD: &str = "git.discard";
pub const METHOD_GIT_COMMIT: &str = "git.commit";
pub const EVENT_GIT_REPOSITORY_CHANGED: &str = "git.repository_changed";

pub const METHOD_AGENT_WORKTREE_CREATE: &str = "agent.worktree.create";
pub const METHOD_AGENT_WORKTREE_LIST: &str = "agent.worktree.list";
pub const METHOD_AGENT_WORKTREE_RECOVER: &str = "agent.worktree.recover";
pub const METHOD_AGENT_WORKTREE_REMOVE: &str = "agent.worktree.remove";
pub const EVENT_AGENT_WORKTREE_PROGRESS: &str = "agent.worktree.progress";

pub const METHOD_GIT_REVIEW_THREAD_LIST: &str = "git.review_thread.list";
pub const METHOD_GIT_REVIEW_THREAD_CREATE: &str = "git.review_thread.create";
pub const METHOD_GIT_REVIEW_THREAD_UPDATE: &str = "git.review_thread.update";
pub const METHOD_GIT_REVIEW_THREAD_DELETE: &str = "git.review_thread.delete";
pub const METHOD_GIT_REVIEW_THREAD_MARK_STALE: &str = "git.review_thread.mark_stale";
pub const METHOD_GIT_REVIEW_THREAD_DELIVER: &str = "git.review_thread.deliver";
pub const METHOD_GIT_REVIEW_COMMENT_LIST: &str = "git.review_comment.list";
pub const METHOD_GIT_REVIEW_COMMENT_CREATE: &str = "git.review_comment.create";
pub const METHOD_GIT_REVIEW_COMMENT_UPDATE: &str = "git.review_comment.update";
pub const METHOD_GIT_REVIEW_COMMENT_DELETE: &str = "git.review_comment.delete";

pub const METHOD_AGENT_HOOK_STATE: &str = "agent.hook_state";
pub const EVENT_AGENT_HOOK_STATE_CHANGED: &str = "agent.hook_state_changed";
pub const METHOD_DEV_SERVER_CANDIDATE_DETECTED: &str = "dev_server.candidate_detected";
pub const METHOD_DEV_SERVER_CANDIDATE_LIST: &str = "dev_server.candidate.list";
pub const METHOD_DEV_SERVER_CANDIDATE_DISMISS: &str = "dev_server.candidate.dismiss";
pub const METHOD_DEV_SERVER_CANDIDATE_OPEN_IN_SPLIT: &str = "dev_server.candidate.open_in_split";
pub const EVENT_DEV_SERVER_CANDIDATE_DETECTED: &str = "dev_server.candidate_detected";

pub const MAX_GIT_STATUS_PAGE_SIZE: usize = 500;
pub const MAX_GIT_PATHS_PER_MUTATION: usize = 500;
pub const MAX_GIT_DIFF_CONTEXT_LINES: u16 = 200;
pub const MAX_GIT_COMMIT_MESSAGE_BYTES: usize = 16 * 1024;
pub const MAX_REVIEW_BODY_BYTES: usize = 32 * 1024;
pub const MAX_REVIEW_COMMENTS_PER_THREAD: usize = 500;
pub const MAX_DEV_SERVER_CANDIDATES: usize = 500;
pub const MAX_IDEMPOTENCY_KEY_BYTES: usize = 256;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Auth {
    pub token: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestEnvelope {
    pub schema: String,
    pub id: String,
    pub method: String,
    pub params_json: String,
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<ControlCaller>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlCaller {
    pub source: String,
    pub profile: Option<String>,
    pub client_session_id: Option<String>,
}

impl RequestEnvelope {
    pub fn new(
        id: impl Into<String>,
        method: impl Into<String>,
        params_json: impl Into<String>,
        token: impl Into<String>,
    ) -> Self {
        Self {
            schema: CONTROL_SCHEMA.to_string(),
            id: id.into(),
            method: method.into(),
            params_json: params_json.into(),
            auth: Auth {
                token: token.into(),
            },
            caller: None,
        }
    }

    pub fn with_caller(mut self, caller: ControlCaller) -> Self {
        self.caller = Some(caller);
        self
    }

    pub fn parse_params<T>(&self) -> Result<T, ControlError>
    where
        T: DeserializeOwned,
    {
        serde_json::from_str(&self.params_json).map_err(|error| {
            ControlError::new(
                ErrorCode::InvalidRequest,
                format!("Invalid params for '{}': {error}", self.method),
            )
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResponseEnvelope {
    pub schema: String,
    pub id: String,
    pub outcome: ResponseOutcome,
}

impl ResponseEnvelope {
    pub fn ok(id: impl Into<String>, result_json: impl Into<String>) -> Self {
        Self {
            schema: CONTROL_SCHEMA.to_string(),
            id: id.into(),
            outcome: ResponseOutcome::Ok {
                result_json: result_json.into(),
            },
        }
    }

    pub fn error(id: impl Into<String>, error: ControlError) -> Self {
        Self {
            schema: CONTROL_SCHEMA.to_string(),
            id: id.into(),
            outcome: ResponseOutcome::Error(error),
        }
    }

    pub fn ok_typed<T>(id: impl Into<String>, result: &T) -> Self
    where
        T: Serialize,
    {
        match to_json(result) {
            Ok(result_json) => Self::ok(id, result_json),
            Err(error) => Self::error(id, error),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ResponseOutcome {
    Ok { result_json: String },
    Error(ControlError),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    InvalidRequest,
    UnsupportedMethod,
    WorkspaceNotFound,
    PaneNotFound,
    SurfaceNotFound,
    SessionNotFound,
    BackendUnavailable,
    BackendDegraded,
    SpawnFailed,
    AttachFailed,
    Timeout,
    Conflict,
    PermissionDenied,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Unauthorized => "unauthorized",
            ErrorCode::InvalidRequest => "invalid_request",
            ErrorCode::UnsupportedMethod => "unsupported_method",
            ErrorCode::WorkspaceNotFound => "workspace_not_found",
            ErrorCode::PaneNotFound => "pane_not_found",
            ErrorCode::SurfaceNotFound => "surface_not_found",
            ErrorCode::SessionNotFound => "session_not_found",
            ErrorCode::BackendUnavailable => "backend_unavailable",
            ErrorCode::BackendDegraded => "backend_degraded",
            ErrorCode::SpawnFailed => "spawn_failed",
            ErrorCode::AttachFailed => "attach_failed",
            ErrorCode::Timeout => "timeout",
            ErrorCode::Conflict => "conflict",
            ErrorCode::PermissionDenied => "permission_denied",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlError {
    pub code: ErrorCode,
    pub message: String,
    pub details_json: Option<String>,
}

impl ControlError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details_json: None,
        }
    }

    pub fn with_details(mut self, details_json: impl Into<String>) -> Self {
        self.details_json = Some(details_json.into());
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventFrame {
    pub schema: String,
    pub event_id: String,
    pub event_type: String,
    pub occurred_at: String,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub data_json: String,
}

impl EventFrame {
    pub fn new(event_id: impl Into<String>, event_type: impl Into<String>) -> Self {
        Self {
            schema: EVENT_SCHEMA.to_string(),
            event_id: event_id.into(),
            event_type: event_type.into(),
            occurred_at: String::new(),
            workspace_id: None,
            session_id: None,
            data_json: "{}".to_string(),
        }
    }
}

pub struct ControlPipeConnection {
    file: File,
}

impl ControlPipeConnection {
    #[cfg(windows)]
    fn new(file: File) -> Self {
        Self { file }
    }

    pub fn write_response(&mut self, response: &ResponseEnvelope) -> io::Result<()> {
        self.write_json_line(response)
    }

    pub fn write_event(&mut self, event: &EventFrame) -> io::Result<()> {
        self.write_json_line(event)
    }

    fn write_json_line<T>(&mut self, value: &T) -> io::Result<()>
    where
        T: Serialize,
    {
        let json = serde_json::to_string(value)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        writeln!(self.file, "{json}")?;
        self.file.flush()
    }
}

pub struct NamedPipeEventStream {
    reader: BufReader<File>,
}

impl NamedPipeEventStream {
    pub fn read_event(&mut self) -> io::Result<Option<EventFrame>> {
        let mut event_json = String::new();
        let bytes = self.reader.read_line(&mut event_json)?;
        if bytes == 0 || event_json.trim().is_empty() {
            return Ok(None);
        }

        serde_json::from_str(event_json.trim_end())
            .map(Some)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionSpawnParams {
    pub workspace_id: String,
    pub backend: Option<String>,
    pub backend_profile: Option<String>,
    pub command: Vec<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: Vec<EnvVarParam>,
    pub columns: u16,
    pub rows: u16,
    pub durability: Option<String>,
    pub placement: Option<String>,
    pub pane_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerminalOpenParams {
    pub workspace_id: String,
    pub pane_id: Option<String>,
    pub backend: Option<String>,
    pub backend_profile: Option<String>,
    #[serde(default)]
    pub command: Vec<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: Vec<EnvVarParam>,
    pub columns: Option<u16>,
    pub rows: Option<u16>,
    pub durability: Option<String>,
    pub placement: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerminalSplitParams {
    pub workspace_id: String,
    pub pane_id: String,
    pub axis: String,
    pub ratio: Option<f64>,
    pub behavior: Option<String>,
    pub backend: Option<String>,
    pub backend_profile: Option<String>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub env: Vec<EnvVarParam>,
    pub cwd: Option<String>,
    pub columns: Option<u16>,
    pub rows: Option<u16>,
    pub durability: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerminalPlacementResult {
    pub workspace_id: String,
    pub source_pane_id: Option<String>,
    pub pane_id: String,
    pub surface_id: Option<String>,
    pub session_id: Option<String>,
    pub backend: Option<String>,
    pub backend_profile: Option<String>,
    pub cwd: Option<String>,
    pub columns: u16,
    pub rows: u16,
    pub rolled_back: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnvVarParam {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionAttachParams {
    pub session_id: Option<String>,
    pub workspace_id: String,
    pub backend: String,
    pub backend_profile: Option<String>,
    pub backend_ref: String,
    pub columns: u16,
    pub rows: u16,
    pub durability: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceCreateParams {
    pub name: String,
    pub project_root: Option<String>,
    pub backend_profile: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceIdParams {
    pub workspace_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceRenameParams {
    pub workspace_id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceUpdateParams {
    pub workspace_id: String,
    pub name: String,
    pub project_root: Option<String>,
    pub environment_profile_id: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub default_wsl_distribution: Option<String>,
    pub default_terminal_profile: Option<String>,
    pub default_agent_command: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceCloseParams {
    pub workspace_id: String,
    pub close_policy: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceGroupListParams {}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceGroupCreateParams {
    pub name: String,
    pub anchor_workspace_id: Option<String>,
    pub workspace_ids: Option<Vec<String>>,
    pub collapsed: Option<bool>,
    pub pinned: Option<bool>,
    pub color: Option<String>,
    pub icon: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceGroupUpdateParams {
    pub group_id: String,
    pub name: Option<String>,
    pub anchor_workspace_id: Option<String>,
    pub collapsed: Option<bool>,
    pub pinned: Option<bool>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub sort_order: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceGroupIdParams {
    pub group_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceGroupMemberParams {
    pub group_id: String,
    pub workspace_id: String,
    pub position: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PaneSplitParams {
    pub workspace_id: String,
    pub pane_id: String,
    pub axis: String,
    pub ratio: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PaneFocusParams {
    pub workspace_id: String,
    pub pane_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PaneCloseParams {
    pub workspace_id: String,
    pub pane_id: String,
    pub surface_policy: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PaneResizeLayoutParams {
    pub workspace_id: String,
    pub pane_id: String,
    pub ratio: f64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PaneMountSurfaceParams {
    pub workspace_id: String,
    pub pane_id: String,
    pub surface_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PaneUnmountSurfaceParams {
    pub workspace_id: String,
    pub pane_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SurfaceCreateBrowserParams {
    pub workspace_id: String,
    pub pane_id: Option<String>,
    pub profile: Option<String>,
    pub placement: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SurfaceCloseParams {
    pub workspace_id: String,
    pub surface_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SurfaceMoveWorkspaceParams {
    pub source_workspace_id: String,
    pub target_workspace_id: String,
    pub surface_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SurfaceMoveWorkspaceResult {
    pub source: WorkspaceDetailResult,
    pub target: WorkspaceDetailResult,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionIdParams {
    pub session_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionSendTextParams {
    pub session_id: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionSendPasteParams {
    pub session_id: String,
    pub text: String,
    #[serde(default = "default_bracketed_paste")]
    pub bracketed: bool,
}

fn default_bracketed_paste() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionSendKeyParams {
    pub session_id: String,
    pub key: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionResizeParams {
    pub session_id: String,
    pub columns: u16,
    pub rows: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionTerminateParams {
    pub session_id: String,
    pub mode: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionReadRecentParams {
    pub session_id: String,
    pub max_bytes: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionListParams {
    pub workspace_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventPollParams {
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub types: Option<Vec<String>>,
    pub max_events: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventSubscribeParams {
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub types: Option<Vec<String>>,
    pub after_event_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentTelemetry {
    pub activity: Option<String>,
    pub session: Option<String>,
    pub cost: Option<String>,
    pub tokens: Option<String>,
    pub cache: Option<String>,
    pub rate: Option<String>,
    pub ctx: Option<String>,
    pub team_id: Option<String>,
    pub team_role: Option<String>,
    pub worker_name: Option<String>,
    pub parent_session_id: Option<String>,
    pub layout_root_pane_id: Option<String>,
    pub main_ratio: Option<String>,
    pub max_workers: Option<u16>,
    pub worker_index: Option<u16>,
    pub default_worker_kind: Option<String>,
    pub distribution: Option<String>,
    pub team_cwd: Option<String>,
    pub durability: Option<String>,
    pub team_mode: Option<String>,
    pub team_status: Option<String>,
    pub team_layout: Option<String>,
    pub team_generation: Option<u64>,
    pub team_mutation_id: Option<String>,
    pub team_mutation_owner_id: Option<String>,
    pub team_auto_adopt: Option<bool>,
    pub team_idempotency_key: Option<String>,
    pub team_member_idempotency_key: Option<String>,
}

impl AgentTelemetry {
    pub fn is_empty(&self) -> bool {
        self.activity.is_none()
            && self.session.is_none()
            && self.cost.is_none()
            && self.tokens.is_none()
            && self.cache.is_none()
            && self.rate.is_none()
            && self.ctx.is_none()
            && self.team_id.is_none()
            && self.team_role.is_none()
            && self.worker_name.is_none()
            && self.parent_session_id.is_none()
            && self.layout_root_pane_id.is_none()
            && self.main_ratio.is_none()
            && self.max_workers.is_none()
            && self.worker_index.is_none()
            && self.default_worker_kind.is_none()
            && self.distribution.is_none()
            && self.team_cwd.is_none()
            && self.durability.is_none()
            && self.team_mode.is_none()
            && self.team_status.is_none()
            && self.team_layout.is_none()
            && self.team_generation.is_none()
            && self.team_mutation_id.is_none()
            && self.team_mutation_owner_id.is_none()
            && self.team_auto_adopt.is_none()
            && self.team_idempotency_key.is_none()
            && self.team_member_idempotency_key.is_none()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentSetStateParams {
    pub session_id: String,
    pub state: String,
    pub reason: Option<String>,
    #[serde(default)]
    pub telemetry: Option<AgentTelemetry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentTeamReserveParams {
    pub team_id: String,
    pub main_session_id: String,
    pub expected_generation: u64,
    pub next_generation: u64,
    pub mutation_id: Option<String>,
    #[serde(default)]
    pub claim: bool,
    #[serde(default)]
    pub claim_telemetry: Option<AgentTelemetry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentTeamSettleParams {
    pub team_id: String,
    pub main_session_id: String,
    pub generation: u64,
    pub mutation_id: String,
    pub telemetry: AgentTelemetry,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentTeamRecoverParams {
    pub team_id: String,
    pub main_session_id: String,
    pub generation: u64,
    pub mutation_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentListAttentionParams {
    pub workspace_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NotificationListParams {
    pub workspace_id: Option<String>,
    pub severity: Option<String>,
    pub include_dismissed: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NotificationCreateParams {
    pub title: String,
    pub body: Option<String>,
    pub subtitle: Option<String>,
    pub severity: Option<String>,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NotificationDismissParams {
    pub notification_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NotificationClearParams {
    pub workspace_id: Option<String>,
    pub severity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SidebarWorkspaceParams {
    pub workspace_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SidebarStatusSetParams {
    pub workspace_id: Option<String>,
    pub key: String,
    pub label: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub priority: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SidebarStatusKeyParams {
    pub workspace_id: Option<String>,
    pub key: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SidebarProgressSetParams {
    pub workspace_id: Option<String>,
    pub value: f64,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SidebarLogAddParams {
    pub workspace_id: Option<String>,
    pub level: Option<String>,
    pub source: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SidebarLogListParams {
    pub workspace_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TeamTaskListParams {
    pub workspace_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TeamTaskCreateParams {
    pub workspace_id: String,
    pub title: String,
    pub description: Option<String>,
    pub assigned_session_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TeamTaskIdParams {
    pub task_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TeamTaskClaimParams {
    pub task_id: String,
    pub session_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TeamTaskBlockParams {
    pub task_id: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TeamTaskDependencyParams {
    pub task_id: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TeamMessageListParams {
    pub workspace_id: Option<String>,
    pub include_read: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TeamMessageSendParams {
    pub workspace_id: String,
    pub thread_id: Option<String>,
    pub from_session_id: Option<String>,
    pub to_session_id: Option<String>,
    pub body: String,
    pub kind: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TeamMessageMarkReadParams {
    pub message_id: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SystemIdentifyParams {
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDiagnosticsParams {
    pub workspace_id: Option<String>,
    pub surface_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserNavigateParams {
    pub surface_id: String,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserSurfaceParams {
    pub surface_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserScreenshotParams {
    pub surface_id: String,
    pub format: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDomSnapshotParams {
    pub surface_id: String,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BrowserClickParams {
    pub surface_id: String,
    pub selector: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserTypeParams {
    pub surface_id: String,
    pub selector: String,
    pub text: String,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserFillParams {
    pub surface_id: String,
    pub selector: String,
    pub text: String,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserPressParams {
    pub surface_id: String,
    pub selector: String,
    pub key: String,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserSelectParams {
    pub surface_id: String,
    pub selector: String,
    pub values: Vec<String>,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserScrollParams {
    pub surface_id: String,
    pub selector: Option<String>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserHoverParams {
    pub surface_id: String,
    pub selector: String,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserCheckParams {
    pub surface_id: String,
    pub selector: String,
    pub checked: Option<bool>,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserGetParams {
    pub surface_id: String,
    pub selector: String,
    pub kind: Option<String>,
    pub attribute: Option<String>,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserFindParams {
    pub surface_id: String,
    pub query: String,
    pub selector: Option<String>,
    pub limit: Option<u16>,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserHighlightParams {
    pub surface_id: String,
    pub selector: String,
    pub duration_ms: Option<u64>,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserFocusParams {
    pub surface_id: String,
    pub selector: String,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserZoomParams {
    pub surface_id: String,
    pub percent: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserWaitForSelectorParams {
    pub surface_id: String,
    pub selector: String,
    pub timeout_ms: Option<u64>,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserEvaluateParams {
    pub surface_id: String,
    pub script: String,
    pub frame_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserConsoleParams {
    pub surface_id: String,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDialogsParams {
    pub surface_id: String,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDialogRespondParams {
    pub surface_id: String,
    pub dialog_id: String,
    pub accept: bool,
    pub prompt_text: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDialogCancelParams {
    pub surface_id: String,
    pub dialog_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserErrorsParams {
    pub surface_id: String,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionSpawnResult {
    pub session_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceSummaryResult {
    pub workspace_id: String,
    pub name: String,
    pub root_pane_id: String,
    pub active_pane_id: String,
    pub project_root: Option<String>,
    pub environment_profile_id: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub default_wsl_distribution: Option<String>,
    pub default_terminal_profile: Option<String>,
    pub default_agent_command: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceListResult {
    pub workspaces: Vec<WorkspaceSummaryResult>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PaneSummaryResult {
    pub pane_id: String,
    pub workspace_id: String,
    pub parent_pane_id: Option<String>,
    pub kind: String,
    pub split_axis: Option<String>,
    pub split_ratio: Option<f64>,
    pub mounted_surface_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SurfaceSummaryResult {
    pub surface_id: String,
    pub workspace_id: String,
    pub surface_type: String,
    pub title: String,
    pub session_id: Option<String>,
    pub browser_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkspaceDetailResult {
    pub workspace: WorkspaceSummaryResult,
    pub panes: Vec<PaneSummaryResult>,
    pub surfaces: Vec<SurfaceSummaryResult>,
    pub sessions: Vec<SessionSummaryResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceCloseResult {
    pub workspace_id: String,
    pub closed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceGroupMemberResult {
    pub workspace_id: String,
    pub position: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceGroupResult {
    pub group_id: String,
    pub name: String,
    pub anchor_workspace_id: Option<String>,
    pub collapsed: bool,
    pub pinned: bool,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
    pub members: Vec<WorkspaceGroupMemberResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceGroupListResult {
    pub groups: Vec<WorkspaceGroupResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionSummaryResult {
    pub session_id: String,
    pub workspace_id: String,
    pub backend_kind: String,
    pub state: String,
    pub exit_code: Option<i32>,
    pub backend_native_id: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionListResult {
    pub sessions: Vec<SessionSummaryResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionReadRecentResult {
    pub session_id: String,
    pub text: String,
    pub byte_count: usize,
}

/// Params for `session.snapshot`: an atomic capture of a session's recent output
/// ring plus the absolute byte offsets it covers, used to cold-start a
/// stream-first renderer that then attaches the live stream at `end_offset`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionSnapshotParams {
    pub session_id: String,
    /// Optional absolute offset; the result then returns only bytes at/after it,
    /// for efficient delta polling. Omit for a full cold-start snapshot.
    #[serde(default)]
    pub since_offset: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionSnapshotResult {
    pub session_id: String,
    /// Absolute offset of the first byte in `bytes_base64`.
    pub base_offset: u64,
    /// Absolute total bytes ever emitted by the session; where the live stream
    /// attaches. `bytes_base64` covers `[base_offset, end_offset)`.
    pub end_offset: u64,
    /// Base64-encoded raw recent-output bytes for `[base_offset, end_offset)`.
    pub bytes_base64: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionOutputPressureParams {
    pub session_id: String,
    pub queued_bytes: u64,
    pub max_queued_bytes: u64,
    pub backpressure_events: u64,
    pub write_in_flight: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventPollResult {
    pub events: Vec<EventFrame>,
    pub dropped_count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventSubscribeResult {
    pub subscribed: bool,
    pub cursor: String,
    pub dropped_count: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlAuditListParams {
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlAuditRecord {
    pub request_id: String,
    pub method: String,
    pub source: String,
    pub profile: Option<String>,
    pub client_session_id: Option<String>,
    pub occurred_at: String,
    pub succeeded: bool,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlAuditListResult {
    pub records: Vec<ControlAuditRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentStateResult {
    pub session_id: String,
    pub workspace_id: String,
    pub state: String,
    pub attention: bool,
    pub reason: Option<String>,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub telemetry: Option<AgentTelemetry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentTeamReserveResult {
    pub team_id: String,
    pub main_session_id: String,
    pub generation: u64,
    pub mutation_id: Option<String>,
    #[serde(default)]
    pub reused: bool,
    #[serde(default)]
    pub acquired: bool,
    #[serde(default)]
    pub recovered: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentTeamSettleResult {
    pub team_id: String,
    pub main_session_id: String,
    pub generation: u64,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentTeamRecoverResult {
    pub team_id: String,
    pub main_session_id: String,
    pub generation: u64,
    pub recovered: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentAttentionListResult {
    pub sessions: Vec<AgentStateResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NotificationSummaryResult {
    pub notification_id: String,
    pub notification_type: String,
    pub severity: String,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub title: String,
    pub message: String,
    pub created_at: String,
    pub dismissed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NotificationListResult {
    pub notifications: Vec<NotificationSummaryResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NotificationClearResult {
    pub cleared: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SidebarStatusResult {
    pub workspace_id: String,
    pub key: String,
    pub label: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub priority: i64,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SidebarProgressResult {
    pub workspace_id: String,
    pub value: f64,
    pub label: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SidebarLogResult {
    pub log_id: String,
    pub workspace_id: String,
    pub level: String,
    pub source: Option<String>,
    pub message: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SidebarStatusListResult {
    pub statuses: Vec<SidebarStatusResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SidebarLogListResult {
    pub logs: Vec<SidebarLogResult>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SidebarStateResult {
    pub workspace_id: String,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub git_hash: Option<String>,
    pub ports: Vec<String>,
    pub statuses: Vec<SidebarStatusResult>,
    pub progress: Option<SidebarProgressResult>,
    pub logs: Vec<SidebarLogResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TeamTaskResult {
    pub task_id: String,
    pub workspace_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub assigned_session_id: Option<String>,
    pub blocked_reason: Option<String>,
    pub depends_on: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TeamTaskListResult {
    pub tasks: Vec<TeamTaskResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TeamMessageResult {
    pub message_id: String,
    pub workspace_id: String,
    pub thread_id: Option<String>,
    pub from_session_id: Option<String>,
    pub to_session_id: Option<String>,
    pub body: String,
    pub kind: String,
    pub created_at: String,
    pub read_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TeamMessageListResult {
    pub messages: Vec<TeamMessageResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SystemCapabilitiesResult {
    pub product: String,
    pub control_schema: String,
    pub access_mode: String,
    pub pipe_name: String,
    pub cmux_compat: bool,
    pub methods: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SystemIdentifyResult {
    pub in_agentmux: bool,
    pub workspace_id: Option<String>,
    pub pane_id: Option<String>,
    pub surface_id: Option<String>,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub backend_kind: Option<String>,
    pub control_pipe: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionListParams {
    pub workspace_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionRunParams {
    pub action_id: String,
    pub workspace_id: Option<String>,
    pub pane_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionSummaryResult {
    pub id: String,
    pub title: String,
    pub group: String,
    pub source: String,
    pub target: Option<String>,
    pub command: Vec<String>,
    pub keywords: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionListResult {
    pub workspace_id: Option<String>,
    pub actions: Vec<ActionSummaryResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionRunResult {
    pub action_id: String,
    pub workspace_id: Option<String>,
    pub result_type: String,
    pub session_id: Option<String>,
    pub surface_id: Option<String>,
    pub pane_id: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserNavigationResult {
    pub surface_id: String,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserScreenshotResult {
    pub surface_id: String,
    pub format: String,
    pub image_handle: String,
    pub byte_count: usize,
    pub data_base64: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDomSnapshotResult {
    pub surface_id: String,
    pub html: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserFrameResult {
    pub frame_id: String,
    pub parent_frame_id: Option<String>,
    pub url: String,
    pub name: Option<String>,
    pub security_origin: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserFramesResult {
    pub surface_id: String,
    pub frames: Vec<BrowserFrameResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserStorageEntryResult {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserStorageResult {
    pub surface_id: String,
    pub local_storage: Vec<BrowserStorageEntryResult>,
    pub session_storage: Vec<BrowserStorageEntryResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserCookieResult {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: Option<String>,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserCookiesResult {
    pub surface_id: String,
    pub cookies: Vec<BrowserCookieResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDownloadsParams {
    pub surface_id: String,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDownloadResult {
    pub file_name: String,
    pub path: String,
    pub byte_count: u64,
    pub modified_at: Option<String>,
    pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDownloadsResult {
    pub surface_id: String,
    pub directory: String,
    pub downloads: Vec<BrowserDownloadResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserHistoryEntryResult {
    pub id: i64,
    pub url: String,
    pub title: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserHistoryResult {
    pub surface_id: String,
    pub current_index: i64,
    pub entries: Vec<BrowserHistoryEntryResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserConsoleMessageResult {
    pub level: String,
    pub text: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserConsoleResult {
    pub surface_id: String,
    pub messages: Vec<BrowserConsoleMessageResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDialogMessageResult {
    pub dialog_id: String,
    pub surface_id: String,
    pub kind: String,
    pub dialog_type: String,
    pub message: String,
    pub default_value: Option<String>,
    pub status: String,
    pub response: Option<String>,
    pub timestamp: String,
    pub resolved_at: Option<String>,
    pub resolution: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDialogsResult {
    pub surface_id: String,
    pub messages: Vec<BrowserDialogMessageResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDialogHandledResult {
    pub surface_id: String,
    pub dialog_id: String,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserErrorEventResult {
    pub kind: String,
    pub message: String,
    pub source: String,
    pub line: u32,
    pub column: u32,
    pub stack: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserErrorsResult {
    pub surface_id: String,
    pub events: Vec<BrowserErrorEventResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserActionResult {
    pub surface_id: String,
    pub ok: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserGetResult {
    pub surface_id: String,
    pub selector: String,
    pub kind: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserFindResult {
    pub surface_id: String,
    pub query: String,
    pub count: usize,
    pub matches: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserWaitForSelectorResult {
    pub surface_id: String,
    pub selector: String,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserEvaluateResult {
    pub surface_id: String,
    pub value_json: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDiagnosticResult {
    pub surface_id: Option<String>,
    pub workspace_id: Option<String>,
    pub operation: String,
    pub code: String,
    pub message: String,
    pub occurred_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDiagnosticsResult {
    pub failures: Vec<BrowserDiagnosticResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiagnosticsBackendHealthResult {
    pub backend_kind: String,
    pub health: String,
    pub active_sessions: usize,
    pub recovering_sessions: usize,
    pub failed_sessions: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiagnosticsQueuePressureResult {
    pub queue: String,
    pub depth: usize,
    pub capacity: usize,
    pub dropped_count: usize,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiagnosticsOutputStreamResult {
    pub active_sessions: usize,
    pub active_subscriptions: usize,
    pub frames_sent: u64,
    pub bytes_sent: u64,
    pub send_failures: u64,
    pub closed_channels: u64,
    pub pump_runs: u64,
    pub pump_active_runs: u64,
    pub pump_idle_runs: u64,
    pub last_frame_at: Option<String>,
    pub renderer_queued_bytes: u64,
    pub renderer_max_queued_bytes: u64,
    pub renderer_backpressure_events: u64,
    pub renderer_write_in_flight_sessions: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiagnosticsExportResult {
    pub generated_at: String,
    pub format_version: String,
    pub recovery: RecoveryDiagnosticsResult,
    pub browser: BrowserDiagnosticsResult,
    pub notifications: Vec<NotificationSummaryResult>,
    pub backend_health: Vec<DiagnosticsBackendHealthResult>,
    pub queue_pressure: Vec<DiagnosticsQueuePressureResult>,
    pub output_stream: DiagnosticsOutputStreamResult,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecoverySessionResult {
    pub session_id: String,
    pub workspace_id: String,
    pub backend_kind: String,
    pub state: String,
    pub durability: String,
    pub backend_native_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecoveryDiagnosticsResult {
    pub workspace_count: usize,
    pub pane_count: usize,
    pub surface_count: usize,
    pub session_count: usize,
    pub sessions: Vec<RecoverySessionResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WslDistributionResult {
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WslDistributionListResult {
    pub distributions: Vec<WslDistributionResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TmuxDiagnosticsParams {
    pub distribution: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TmuxDiagnosticsResult {
    pub available: bool,
    pub distribution: Option<String>,
    pub version: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AppConfigAppearance {
    pub theme: String,
    pub accent_key: String,
    pub font_size: f64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppConfigLocale {
    pub language: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppConfigUpdates {
    pub auto_check: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AppConfigShortcuts {
    #[serde(default)]
    pub bindings: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AppConfigActions {
    #[serde(default)]
    pub custom: Vec<AppConfigCustomAction>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AppConfigUi {
    #[serde(default)]
    pub workspace_plus_action: Option<String>,
    #[serde(default)]
    pub surface_tab_plus_action: Option<String>,
    #[serde(default)]
    pub surface_tab_actions: Option<Vec<String>>,
    #[serde(default)]
    pub text_box_max_lines: Option<u8>,
    #[serde(default)]
    pub terminal_inner_margin: Option<u8>,
    #[serde(default)]
    pub terminal_gpu_acceleration: Option<String>,
    #[serde(default)]
    pub terminal_start_directory: Option<String>,
    #[serde(default)]
    pub terminal_start_custom_cwd: Option<String>,
    #[serde(default)]
    pub terminal_split_behavior: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AppConfigNotifications {
    #[serde(default)]
    pub actions: Vec<AppConfigNotificationAction>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AppConfigNotificationAction {
    pub action: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub notification_type: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub dismiss_on_run: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AppConfigCustomAction {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub group: Option<String>,
    pub target: String,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AppConfigResult {
    pub format_version: String,
    pub config_path: String,
    pub project_config_path: Option<String>,
    pub project_config_loaded: bool,
    pub appearance: AppConfigAppearance,
    pub locale: AppConfigLocale,
    pub updates: AppConfigUpdates,
    pub shortcuts: AppConfigShortcuts,
    pub actions: AppConfigActions,
    pub ui: AppConfigUi,
    pub notifications: AppConfigNotifications,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppConfigGetParams {
    pub workspace_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppConfigExportParams {
    pub workspace_id: Option<String>,
    pub scope: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AppConfigExportResult {
    pub json: String,
    pub config: AppConfigResult,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppConfigImportParams {
    pub workspace_id: Option<String>,
    pub scope: Option<String>,
    pub json: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppConfigResetParams {
    pub workspace_id: Option<String>,
    pub scope: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppConfigMigrateProjectParams {
    pub workspace_id: Option<String>,
    pub overwrite: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AppConfigMigrateProjectResult {
    pub source_path: String,
    pub target_path: String,
    pub overwritten: bool,
    pub config: AppConfigResult,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppConfigDiagnosticsParams {
    pub workspace_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppConfigDiagnosticsEntry {
    pub source: String,
    pub path: Option<String>,
    pub exists: bool,
    pub valid: bool,
    pub active: bool,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppConfigDiagnosticsResult {
    pub entries: Vec<AppConfigDiagnosticsEntry>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DockGetParams {
    pub workspace_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DockTrustParams {
    pub workspace_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DockControlResult {
    pub id: String,
    pub title: String,
    pub command: String,
    pub cwd: Option<String>,
    pub height: Option<u16>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DockConfigResult {
    pub source: String,
    pub config_path: Option<String>,
    pub requires_trust: bool,
    pub trusted: bool,
    pub controls: Vec<DockControlResult>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AppConfigAppearanceUpdate {
    pub theme: Option<String>,
    pub accent_key: Option<String>,
    pub font_size: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppConfigLocaleUpdate {
    pub language: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppConfigUpdatesUpdate {
    pub auto_check: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AppConfigShortcutsUpdate {
    pub bindings: Option<BTreeMap<String, serde_json::Value>>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AppConfigUiUpdate {
    pub workspace_plus_action: Option<String>,
    pub surface_tab_plus_action: Option<String>,
    pub surface_tab_actions: Option<Vec<String>>,
    pub text_box_max_lines: Option<u8>,
    pub terminal_inner_margin: Option<u8>,
    pub terminal_gpu_acceleration: Option<String>,
    pub terminal_start_directory: Option<String>,
    pub terminal_start_custom_cwd: Option<String>,
    pub terminal_split_behavior: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AppConfigUpdateParams {
    pub workspace_id: Option<String>,
    pub appearance: Option<AppConfigAppearanceUpdate>,
    pub locale: Option<AppConfigLocaleUpdate>,
    pub updates: Option<AppConfigUpdatesUpdate>,
    pub shortcuts: Option<AppConfigShortcutsUpdate>,
    pub ui: Option<AppConfigUiUpdate>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProfileCreateParams {
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProfileUpdateParams {
    pub profile_id: String,
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProfileIdParams {
    pub profile_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProfileSummaryResult {
    pub profile_id: String,
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProfileListResult {
    pub profiles: Vec<ProfileSummaryResult>,
}

// ---------------------------------------------------------------------------
// Git operations
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitRepositoryParams {
    pub workspace_id: String,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
}

/// Parameters for [`METHOD_GIT_STATUS_SUMMARY`]. The alias intentionally keeps
/// the common repository selector identical across the summary and diff APIs.
pub type GitStatusSummaryParams = GitRepositoryParams;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitStatusPageParams {
    pub workspace_id: String,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    /// Case-insensitive repository-relative path query. The host binds cursors to this value.
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub generation: Option<u64>,
}

impl GitStatusPageParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required(
            "workspace_id",
            &self.workspace_id,
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "repository_id",
            self.repository_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "pane_id",
            self.pane_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional("state", self.state.as_deref(), 64)?;
        validate_optional_filter("query", self.query.as_deref(), 512)?;
        validate_optional("cursor", self.cursor.as_deref(), 1024)?;
        if let Some(limit) = self.limit {
            validate_range("limit", limit, 1, MAX_GIT_STATUS_PAGE_SIZE)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitChangeSummaryResult {
    pub path: String,
    #[serde(default)]
    pub original_path: Option<String>,
    pub status: String,
    #[serde(default)]
    pub staged: bool,
    #[serde(default)]
    pub unstaged: bool,
    #[serde(default)]
    pub untracked: bool,
    #[serde(default)]
    pub conflicted: bool,
    #[serde(default)]
    pub is_binary: bool,
    #[serde(default)]
    pub additions: Option<u64>,
    #[serde(default)]
    pub deletions: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitStatusSummaryResult {
    pub workspace_id: String,
    pub repository_id: String,
    pub repository_root: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub head_oid: Option<String>,
    #[serde(default)]
    pub upstream: Option<String>,
    #[serde(default)]
    pub ahead: u64,
    #[serde(default)]
    pub behind: u64,
    #[serde(default)]
    pub staged_count: usize,
    #[serde(default)]
    pub unstaged_count: usize,
    #[serde(default)]
    pub untracked_count: usize,
    #[serde(default)]
    pub conflicted_count: usize,
    pub generation: u64,
    pub refreshed_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitStatusPageResult {
    pub workspace_id: String,
    pub repository_id: String,
    pub generation: u64,
    #[serde(default)]
    pub summary: Option<GitStatusSummaryResult>,
    pub changes: Vec<GitChangeSummaryResult>,
    #[serde(default)]
    pub next_cursor: Option<String>,
    #[serde(default)]
    pub total_count: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitRepositoryChangedEvent {
    pub workspace_id: String,
    pub repository_id: String,
    pub generation: u64,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitDiffParams {
    pub workspace_id: String,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
    pub path: String,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub context_lines: Option<u16>,
    #[serde(default)]
    pub generation: Option<u64>,
}

impl GitDiffParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required(
            "workspace_id",
            &self.workspace_id,
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_required("path", &self.path, 4096)?;
        validate_optional(
            "repository_id",
            self.repository_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "pane_id",
            self.pane_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional("stage", self.stage.as_deref(), 32)?;
        if let Some(context_lines) = self.context_lines {
            validate_range(
                "context_lines",
                context_lines,
                0,
                MAX_GIT_DIFF_CONTEXT_LINES,
            )?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitDiffResult {
    pub workspace_id: String,
    pub repository_id: String,
    pub generation: u64,
    pub path: String,
    #[serde(default)]
    pub original_path: Option<String>,
    #[serde(default)]
    pub diff: String,
    #[serde(default)]
    pub is_binary: bool,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub diff_hash: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitPathMutationParams {
    pub workspace_id: String,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

impl GitPathMutationParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required(
            "workspace_id",
            &self.workspace_id,
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "repository_id",
            self.repository_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "pane_id",
            self.pane_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "idempotency_key",
            self.idempotency_key.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        if self.paths.is_empty() || self.paths.len() > MAX_GIT_PATHS_PER_MUTATION {
            return Err(invalid_request(format!(
                "paths must contain between 1 and {MAX_GIT_PATHS_PER_MUTATION} entries"
            )));
        }
        for path in &self.paths {
            validate_required("paths[]", path, 4096)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitAllMutationParams {
    pub workspace_id: String,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

impl GitAllMutationParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required(
            "workspace_id",
            &self.workspace_id,
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "repository_id",
            self.repository_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "pane_id",
            self.pane_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "idempotency_key",
            self.idempotency_key.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitCommitParams {
    pub workspace_id: String,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
    pub message: String,
    #[serde(default)]
    pub amend: bool,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

impl GitCommitParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required(
            "workspace_id",
            &self.workspace_id,
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_required("message", &self.message, MAX_GIT_COMMIT_MESSAGE_BYTES)?;
        validate_optional(
            "repository_id",
            self.repository_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "pane_id",
            self.pane_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "idempotency_key",
            self.idempotency_key.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitMutationResult {
    pub workspace_id: String,
    pub repository_id: String,
    pub generation: u64,
    #[serde(default)]
    pub affected_paths: Vec<String>,
    #[serde(default)]
    pub commit_oid: Option<String>,
    #[serde(default)]
    pub reused: bool,
}

// ---------------------------------------------------------------------------
// Atomic agent worktrees
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentWorktreeCreateParams {
    pub workspace_id: String,
    pub branch: String,
    pub destination: String,
    #[serde(default)]
    pub base_revision: Option<String>,
    #[serde(default)]
    pub create_branch: bool,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub backend_profile: Option<String>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub idempotency_key: String,
}

impl AgentWorktreeCreateParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required(
            "workspace_id",
            &self.workspace_id,
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_required("branch", &self.branch, 512)?;
        validate_required("destination", &self.destination, 4096)?;
        validate_required(
            "idempotency_key",
            &self.idempotency_key,
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional("base_revision", self.base_revision.as_deref(), 512)?;
        validate_optional("backend", self.backend.as_deref(), 128)?;
        validate_optional("backend_profile", self.backend_profile.as_deref(), 512)?;
        validate_optional("cwd", self.cwd.as_deref(), 4096)?;
        if self.command.len() > 128 {
            return Err(invalid_request("command may contain at most 128 arguments"));
        }
        for argument in &self.command {
            validate_required("command[]", argument, 16 * 1024)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentWorktreeListParams {
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub include_completed: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentWorktreeRecoverParams {
    #[serde(default)]
    pub operation_id: Option<String>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

impl AgentWorktreeRecoverParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_optional(
            "operation_id",
            self.operation_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "idempotency_key",
            self.idempotency_key.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        if self.operation_id.is_none() && self.idempotency_key.is_none() {
            return Err(invalid_request(
                "operation_id or idempotency_key is required",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentWorktreeRemoveParams {
    pub worktree_id: String,
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

impl AgentWorktreeRemoveParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required("worktree_id", &self.worktree_id, MAX_IDEMPOTENCY_KEY_BYTES)?;
        validate_optional(
            "idempotency_key",
            self.idempotency_key.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentWorktreeProgressResult {
    pub operation_id: String,
    pub worktree_id: Option<String>,
    pub workspace_id: String,
    pub state: String,
    pub step: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub rolled_back: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentWorktreeResult {
    pub operation_id: String,
    pub worktree_id: String,
    pub workspace_id: String,
    pub branch: String,
    pub path: String,
    pub state: String,
    #[serde(default)]
    pub surface_id: Option<String>,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub reused: bool,
    #[serde(default)]
    pub recovered: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentWorktreeListResult {
    pub worktrees: Vec<AgentWorktreeResult>,
}

// ---------------------------------------------------------------------------
// Diff review comments
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewLineAnchor {
    pub path: String,
    pub side: String,
    pub line: u32,
    #[serde(default)]
    pub start_line: Option<u32>,
    #[serde(default)]
    pub base_revision: Option<String>,
    #[serde(default)]
    pub head_revision: Option<String>,
    #[serde(default)]
    pub hunk_header: Option<String>,
    #[serde(default)]
    pub diff_hash: Option<String>,
}

impl GitReviewLineAnchor {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required("anchor.path", &self.path, 4096)?;
        validate_required("anchor.side", &self.side, 32)?;
        if self.line == 0 {
            return Err(invalid_request("anchor.line must be greater than 0"));
        }
        if let Some(start_line) = self.start_line {
            if start_line == 0 || start_line > self.line {
                return Err(invalid_request(
                    "anchor.start_line must be between 1 and anchor.line",
                ));
            }
        }
        validate_optional("anchor.base_revision", self.base_revision.as_deref(), 512)?;
        validate_optional("anchor.head_revision", self.head_revision.as_deref(), 512)?;
        validate_optional("anchor.hunk_header", self.hunk_header.as_deref(), 4096)?;
        validate_optional("anchor.diff_hash", self.diff_hash.as_deref(), 512)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewThreadListParams {
    pub workspace_id: String,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub include_resolved: bool,
    #[serde(default)]
    pub include_stale: bool,
    #[serde(default)]
    pub limit: Option<usize>,
}

impl GitReviewThreadListParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required(
            "workspace_id",
            &self.workspace_id,
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "pane_id",
            self.pane_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "repository_id",
            self.repository_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional("path", self.path.as_deref(), 4096)?;
        if let Some(limit) = self.limit {
            validate_range("limit", limit, 1, MAX_REVIEW_COMMENTS_PER_THREAD)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewThreadCreateParams {
    pub workspace_id: String,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
    pub anchor: GitReviewLineAnchor,
    pub body: String,
    #[serde(default)]
    pub author_session_id: Option<String>,
}

impl GitReviewThreadCreateParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required(
            "workspace_id",
            &self.workspace_id,
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "pane_id",
            self.pane_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "repository_id",
            self.repository_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_required("body", &self.body, MAX_REVIEW_BODY_BYTES)?;
        validate_optional(
            "author_session_id",
            self.author_session_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        self.anchor.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewThreadUpdateParams {
    pub thread_id: String,
    #[serde(default)]
    pub resolved: Option<bool>,
    #[serde(default)]
    pub anchor: Option<GitReviewLineAnchor>,
}

impl GitReviewThreadUpdateParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required("thread_id", &self.thread_id, MAX_IDEMPOTENCY_KEY_BYTES)?;
        if self.resolved.is_none() && self.anchor.is_none() {
            return Err(invalid_request("resolved or anchor is required"));
        }
        if let Some(anchor) = &self.anchor {
            anchor.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewThreadIdParams {
    pub thread_id: String,
}

impl GitReviewThreadIdParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required("thread_id", &self.thread_id, MAX_IDEMPOTENCY_KEY_BYTES)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewThreadMarkStaleParams {
    pub thread_id: String,
    pub stale: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

impl GitReviewThreadMarkStaleParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required("thread_id", &self.thread_id, MAX_IDEMPOTENCY_KEY_BYTES)?;
        validate_optional("reason", self.reason.as_deref(), 4096)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewThreadDeliverParams {
    pub thread_id: String,
    pub target: String,
    #[serde(default)]
    pub target_session_id: Option<String>,
    #[serde(default)]
    pub include_context: bool,
}

impl GitReviewThreadDeliverParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required("thread_id", &self.thread_id, MAX_IDEMPOTENCY_KEY_BYTES)?;
        validate_required("target", &self.target, 64)?;
        validate_optional(
            "target_session_id",
            self.target_session_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewCommentListParams {
    pub thread_id: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

impl GitReviewCommentListParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required("thread_id", &self.thread_id, MAX_IDEMPOTENCY_KEY_BYTES)?;
        if let Some(limit) = self.limit {
            validate_range("limit", limit, 1, MAX_REVIEW_COMMENTS_PER_THREAD)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewCommentCreateParams {
    pub thread_id: String,
    pub body: String,
    #[serde(default)]
    pub author_session_id: Option<String>,
}

impl GitReviewCommentCreateParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required("thread_id", &self.thread_id, MAX_IDEMPOTENCY_KEY_BYTES)?;
        validate_required("body", &self.body, MAX_REVIEW_BODY_BYTES)?;
        validate_optional(
            "author_session_id",
            self.author_session_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewCommentUpdateParams {
    pub comment_id: String,
    pub body: String,
}

impl GitReviewCommentUpdateParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required("comment_id", &self.comment_id, MAX_IDEMPOTENCY_KEY_BYTES)?;
        validate_required("body", &self.body, MAX_REVIEW_BODY_BYTES)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewCommentIdParams {
    pub comment_id: String,
}

impl GitReviewCommentIdParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required("comment_id", &self.comment_id, MAX_IDEMPOTENCY_KEY_BYTES)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewCommentResult {
    pub comment_id: String,
    pub thread_id: String,
    pub body: String,
    #[serde(default)]
    pub author_session_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewThreadResult {
    pub thread_id: String,
    pub workspace_id: String,
    pub repository_id: String,
    #[serde(default)]
    pub author_session_id: Option<String>,
    pub anchor: GitReviewLineAnchor,
    #[serde(default)]
    pub resolved: bool,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub stale_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub comments: Vec<GitReviewCommentResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewThreadListResult {
    pub threads: Vec<GitReviewThreadResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewCommentListResult {
    pub comments: Vec<GitReviewCommentResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitReviewDeliveryResult {
    pub thread_id: String,
    pub target: String,
    #[serde(default)]
    pub target_session_id: Option<String>,
    pub delivered_at: String,
}

// ---------------------------------------------------------------------------
// Normalized agent hooks and detected development servers
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentHookStateParams {
    pub workspace_id: String,
    pub session_id: String,
    pub sequence: u64,
    pub state: String,
    #[serde(default)]
    pub reason: Option<String>,
    pub source: String,
    pub observed_at: String,
    #[serde(default)]
    pub telemetry: Option<AgentTelemetry>,
}

impl AgentHookStateParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required(
            "workspace_id",
            &self.workspace_id,
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_required("session_id", &self.session_id, MAX_IDEMPOTENCY_KEY_BYTES)?;
        validate_required("state", &self.state, 128)?;
        validate_required("source", &self.source, 128)?;
        validate_required("observed_at", &self.observed_at, 128)?;
        validate_optional("reason", self.reason.as_deref(), 4096)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentHookStateResult {
    pub workspace_id: String,
    pub session_id: String,
    pub sequence: u64,
    pub state: String,
    #[serde(default)]
    pub accepted: bool,
    #[serde(default)]
    pub deduplicated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DevelopmentServerCandidateParams {
    pub workspace_id: String,
    pub session_id: String,
    pub url: String,
    pub source: String,
    pub detected_at: String,
    #[serde(default)]
    pub process_id: Option<u32>,
}

impl DevelopmentServerCandidateParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required(
            "workspace_id",
            &self.workspace_id,
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_required("session_id", &self.session_id, MAX_IDEMPOTENCY_KEY_BYTES)?;
        validate_required("url", &self.url, 4096)?;
        validate_required("source", &self.source, 128)?;
        validate_required("detected_at", &self.detected_at, 128)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DevelopmentServerCandidateListParams {
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub include_dismissed: bool,
    #[serde(default)]
    pub limit: Option<usize>,
}

impl DevelopmentServerCandidateListParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_optional(
            "workspace_id",
            self.workspace_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "session_id",
            self.session_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        if let Some(limit) = self.limit {
            validate_range("limit", limit, 1, MAX_DEV_SERVER_CANDIDATES)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DevelopmentServerCandidateIdParams {
    pub candidate_id: String,
}

impl DevelopmentServerCandidateIdParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required(
            "candidate_id",
            &self.candidate_id,
            MAX_IDEMPOTENCY_KEY_BYTES,
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DevelopmentServerCandidateDismissParams {
    pub candidate_id: String,
    #[serde(default)]
    pub reason: Option<String>,
}

impl DevelopmentServerCandidateDismissParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required(
            "candidate_id",
            &self.candidate_id,
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional("reason", self.reason.as_deref(), 4096)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DevelopmentServerCandidateOpenInSplitParams {
    pub candidate_id: String,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub axis: Option<String>,
    #[serde(default)]
    pub ratio: Option<f64>,
}

impl DevelopmentServerCandidateOpenInSplitParams {
    pub fn validate(&self) -> Result<(), ControlError> {
        validate_required(
            "candidate_id",
            &self.candidate_id,
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional(
            "pane_id",
            self.pane_id.as_deref(),
            MAX_IDEMPOTENCY_KEY_BYTES,
        )?;
        validate_optional("axis", self.axis.as_deref(), 32)?;
        if let Some(ratio) = self.ratio {
            if !(0.05..=0.95).contains(&ratio) {
                return Err(invalid_request("ratio must be between 0.05 and 0.95"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DevelopmentServerCandidateResult {
    pub candidate_id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub url: String,
    pub source: String,
    pub detected_at: String,
    #[serde(default)]
    pub process_id: Option<u32>,
    #[serde(default)]
    pub dismissed: bool,
    #[serde(default)]
    pub opened_surface_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DevelopmentServerCandidateListResult {
    pub candidates: Vec<DevelopmentServerCandidateResult>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DevelopmentServerCandidateOpenInSplitResult {
    pub candidate: DevelopmentServerCandidateResult,
    pub pane_id: String,
    pub surface_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DevelopmentServerCandidateDismissResult {
    pub candidate_id: String,
    pub dismissed: bool,
}

fn invalid_request(message: impl Into<String>) -> ControlError {
    ControlError::new(ErrorCode::InvalidRequest, message)
}

fn validate_required(name: &str, value: &str, max_bytes: usize) -> Result<(), ControlError> {
    if value.trim().is_empty() {
        return Err(invalid_request(format!("{name} is required")));
    }
    if value.len() > max_bytes {
        return Err(invalid_request(format!("{name} exceeds {max_bytes} bytes")));
    }
    Ok(())
}

fn validate_optional(
    name: &str,
    value: Option<&str>,
    max_bytes: usize,
) -> Result<(), ControlError> {
    if let Some(value) = value {
        validate_required(name, value, max_bytes)?;
    }
    Ok(())
}

fn validate_optional_filter(
    name: &str,
    value: Option<&str>,
    max_bytes: usize,
) -> Result<(), ControlError> {
    if value.is_some_and(|value| value.len() > max_bytes) {
        return Err(invalid_request(format!("{name} exceeds {max_bytes} bytes")));
    }
    Ok(())
}

fn validate_range<T>(name: &str, value: T, min: T, max: T) -> Result<(), ControlError>
where
    T: std::fmt::Display + PartialOrd,
{
    if value < min || value > max {
        return Err(invalid_request(format!(
            "{name} must be between {min} and {max}"
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AckResult {
    pub ok: bool,
}

pub fn to_json<T>(value: &T) -> Result<String, ControlError>
where
    T: Serialize,
{
    serde_json::to_string(value)
        .map_err(|error| ControlError::new(ErrorCode::InvalidRequest, error.to_string()))
}

pub fn default_control_token_path() -> std::io::Result<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("AGENTMUX_CONTROL_TOKEN_PATH") {
        return Ok(std::path::PathBuf::from(path));
    }

    let base = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "unable to resolve AgentMux config directory",
            )
        })?;
    Ok(base.join("AgentMux").join(DEFAULT_CONTROL_TOKEN_FILE_NAME))
}

pub fn read_control_token(path: impl AsRef<std::path::Path>) -> std::io::Result<String> {
    let token = std::fs::read_to_string(path)?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "AgentMux control token file is empty",
        ));
    }
    Ok(token)
}

pub fn send_named_pipe_request(
    pipe_name: &str,
    request: &RequestEnvelope,
    timeout: std::time::Duration,
) -> std::io::Result<ResponseEnvelope> {
    transport::send_named_pipe_request(pipe_name, request, timeout)
}

pub fn subscribe_named_pipe_events(
    pipe_name: &str,
    request: &RequestEnvelope,
    timeout: std::time::Duration,
) -> std::io::Result<(ResponseEnvelope, NamedPipeEventStream)> {
    transport::subscribe_named_pipe_events(pipe_name, request, timeout)
}

pub fn serve_named_pipe_requests<F>(pipe_name: &str, handler: F) -> std::io::Result<()>
where
    F: Fn(RequestEnvelope) -> ResponseEnvelope,
{
    transport::serve_named_pipe_requests(pipe_name, handler)
}

pub fn serve_named_pipe_streaming_requests<F>(pipe_name: &str, handler: F) -> std::io::Result<()>
where
    F: Fn(RequestEnvelope, ControlPipeConnection) -> std::io::Result<()> + Send + Sync + 'static,
{
    transport::serve_named_pipe_streaming_requests(pipe_name, handler)
}

pub fn serve_one_named_pipe_request<F>(pipe_name: &str, handler: F) -> std::io::Result<()>
where
    F: Fn(RequestEnvelope) -> ResponseEnvelope,
{
    transport::serve_one_named_pipe_request(pipe_name, handler)
}

pub fn serve_one_named_pipe_streaming_request<F>(pipe_name: &str, handler: F) -> std::io::Result<()>
where
    F: FnOnce(RequestEnvelope, ControlPipeConnection) -> std::io::Result<()>,
{
    transport::serve_one_named_pipe_streaming_request(pipe_name, handler)
}

#[cfg(windows)]
mod transport {
    use std::ffi::OsStr;
    use std::fs::{File, OpenOptions};
    use std::io::{self, BufRead, BufReader, Write};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use std::ptr::null_mut;
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{
        GetLastError, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };

    use super::{
        ControlError, ControlPipeConnection, ErrorCode, NamedPipeEventStream, RequestEnvelope,
        ResponseEnvelope,
    };

    pub fn send_named_pipe_request(
        pipe_name: &str,
        request: &RequestEnvelope,
        timeout: Duration,
    ) -> io::Result<ResponseEnvelope> {
        let mut file = open_pipe(pipe_name, timeout)?;

        let request_json = serde_json::to_string(request)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
        writeln!(file, "{request_json}")?;
        file.flush()?;

        let response_json = read_line_with_timeout(file, timeout)?;
        if response_json.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "AgentMux control pipe closed without a response",
            ));
        }

        serde_json::from_str(response_json.trim_end())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))
    }

    pub fn subscribe_named_pipe_events(
        pipe_name: &str,
        request: &RequestEnvelope,
        timeout: Duration,
    ) -> io::Result<(ResponseEnvelope, NamedPipeEventStream)> {
        let mut file = open_pipe(pipe_name, timeout)?;

        let request_json = serde_json::to_string(request)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
        writeln!(file, "{request_json}")?;
        file.flush()?;

        // Apply a read timeout only to the initial handshake response. The
        // returned NamedPipeEventStream performs long-lived blocking reads for
        // subsequent events and must NOT inherit a deadline.
        let (response_json, reader) = read_line_with_timeout_returning_reader(file, timeout)?;
        if response_json.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "AgentMux control pipe closed without a subscription response",
            ));
        }
        let response = serde_json::from_str(response_json.trim_end())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        Ok((response, NamedPipeEventStream { reader }))
    }

    /// Read one line from `file` with a deadline equal to `timeout`.
    ///
    /// Spawns a thread that owns the blocking `read_line` call and sends the
    /// result back over a channel.  The caller keeps a duplicate handle
    /// (`File::try_clone`) and waits on the channel with `recv_timeout`.  On
    /// success the duplicate is dropped immediately.  On timeout the duplicate
    /// is dropped, which closes that handle; Windows sees the last client
    /// handle close and returns `ERROR_BROKEN_PIPE` to the thread's blocked
    /// `ReadFile`, letting the thread exit promptly without leaking a handle
    /// or blocking indefinitely.
    fn read_line_with_timeout(file: File, timeout: Duration) -> io::Result<String> {
        // Duplicate the handle: caller keeps `_cancel`, thread owns `file`.
        // Dropping `_cancel` on timeout unblocks the thread's ReadFile.
        let _cancel = file.try_clone()?;
        let (tx, rx) = std::sync::mpsc::channel::<io::Result<String>>();
        thread::spawn(move || {
            let mut reader = BufReader::new(file);
            let mut line = String::new();
            let result = reader.read_line(&mut line).map(|_| line);
            let _ = tx.send(result);
        });
        // `_cancel` is dropped here on timeout, unblocking the thread.
        rx.recv_timeout(timeout).unwrap_or_else(|_| {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "AgentMux control pipe response timed out",
            ))
        })
    }

    /// Like `read_line_with_timeout` but also returns the `BufReader` so the
    /// caller can continue reading from the same pipe after the handshake.
    ///
    /// Uses the same duplicate-handle cancellation strategy as
    /// `read_line_with_timeout`: a clone of `file` is kept in the caller; on
    /// timeout it is dropped, causing the thread's blocked `ReadFile` to
    /// return `ERROR_BROKEN_PIPE` and the thread to exit without leaking
    /// handles or blocking indefinitely.
    fn read_line_with_timeout_returning_reader(
        file: File,
        timeout: Duration,
    ) -> io::Result<(String, BufReader<File>)> {
        // Duplicate the handle: caller keeps `_cancel`, thread owns `file`.
        let _cancel = file.try_clone()?;
        let (tx, rx) = std::sync::mpsc::channel::<io::Result<(String, BufReader<File>)>>();
        thread::spawn(move || {
            let mut reader = BufReader::new(file);
            let mut line = String::new();
            let result = reader.read_line(&mut line).map(|_| (line, reader));
            let _ = tx.send(result);
        });
        // `_cancel` is dropped here on timeout, unblocking the thread.
        rx.recv_timeout(timeout).unwrap_or_else(|_| {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "AgentMux control pipe subscription response timed out",
            ))
        })
    }

    pub fn serve_named_pipe_requests<F>(pipe_name: &str, handler: F) -> io::Result<()>
    where
        F: Fn(RequestEnvelope) -> ResponseEnvelope,
    {
        loop {
            serve_one_named_pipe_request(pipe_name, &handler)?;
        }
    }

    pub fn serve_named_pipe_streaming_requests<F>(pipe_name: &str, handler: F) -> io::Result<()>
    where
        F: Fn(RequestEnvelope, ControlPipeConnection) -> io::Result<()> + Send + Sync + 'static,
    {
        let handler = Arc::new(handler);
        loop {
            let handle = create_pipe(pipe_name)?;
            connect_pipe(handle)?;
            let file = unsafe { File::from_raw_handle(handle as RawHandle) };
            let handler = Arc::clone(&handler);
            thread::spawn(move || {
                let _ = handle_streaming_connection(file, |request, connection| {
                    handler(request, connection)
                });
            });
        }
    }

    pub fn serve_one_named_pipe_request<F>(pipe_name: &str, handler: F) -> io::Result<()>
    where
        F: Fn(RequestEnvelope) -> ResponseEnvelope,
    {
        let handle = create_pipe(pipe_name)?;
        connect_pipe(handle)?;
        let mut file = unsafe { File::from_raw_handle(handle as RawHandle) };
        handle_connection(&mut file, handler)
    }

    pub fn serve_one_named_pipe_streaming_request<F>(pipe_name: &str, handler: F) -> io::Result<()>
    where
        F: FnOnce(RequestEnvelope, ControlPipeConnection) -> io::Result<()>,
    {
        let handle = create_pipe(pipe_name)?;
        connect_pipe(handle)?;
        let file = unsafe { File::from_raw_handle(handle as RawHandle) };
        handle_streaming_connection(file, handler)
    }

    fn open_pipe(pipe_name: &str, timeout: Duration) -> io::Result<File> {
        let deadline = Instant::now() + timeout;
        loop {
            match OpenOptions::new().read(true).write(true).open(pipe_name) {
                Ok(file) => return Ok(file),
                Err(error) => {
                    if Instant::now() >= deadline {
                        return Err(error);
                    }
                    thread::sleep(Duration::from_millis(25));
                }
            }
        }
    }

    fn create_pipe(pipe_name: &str) -> io::Result<HANDLE> {
        let wide_name = wide_null(pipe_name);
        let handle = unsafe {
            CreateNamedPipeW(
                wide_name.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                65_536,
                65_536,
                0,
                null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(handle)
        }
    }

    fn connect_pipe(handle: HANDLE) -> io::Result<()> {
        let connected = unsafe { ConnectNamedPipe(handle, null_mut()) };
        if connected != 0 || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn handle_connection<F>(file: &mut File, handler: F) -> io::Result<()>
    where
        F: Fn(RequestEnvelope) -> ResponseEnvelope,
    {
        let mut request_json = String::new();
        {
            let mut reader = BufReader::new(&mut *file);
            reader.read_line(&mut request_json)?;
        }

        let response = match serde_json::from_str::<RequestEnvelope>(request_json.trim_end()) {
            Ok(request) => handler(request),
            Err(error) => ResponseEnvelope::error(
                "invalid_request",
                ControlError::new(
                    ErrorCode::InvalidRequest,
                    format!("Invalid control request JSON: {error}"),
                ),
            ),
        };
        let response_json = serde_json::to_string(&response)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        writeln!(file, "{response_json}")?;
        file.flush()
    }

    fn handle_streaming_connection<F>(mut file: File, handler: F) -> io::Result<()>
    where
        F: FnOnce(RequestEnvelope, ControlPipeConnection) -> io::Result<()>,
    {
        let mut request_json = String::new();
        {
            let mut reader = BufReader::new(&mut file);
            reader.read_line(&mut request_json)?;
        }

        let mut connection = ControlPipeConnection::new(file);
        let request = match serde_json::from_str::<RequestEnvelope>(request_json.trim_end()) {
            Ok(request) => request,
            Err(error) => {
                let response = ResponseEnvelope::error(
                    "invalid_request",
                    ControlError::new(
                        ErrorCode::InvalidRequest,
                        format!("Invalid control request JSON: {error}"),
                    ),
                );
                connection.write_response(&response)?;
                return Ok(());
            }
        };

        handler(request, connection)
    }

    fn wide_null(value: &str) -> Vec<u16> {
        OsStr::new(value).encode_wide().chain(Some(0)).collect()
    }
}

#[cfg(not(windows))]
mod transport {
    use std::io;
    use std::time::Duration;

    use super::{ControlPipeConnection, NamedPipeEventStream, RequestEnvelope, ResponseEnvelope};

    pub fn send_named_pipe_request(
        _pipe_name: &str,
        _request: &RequestEnvelope,
        _timeout: Duration,
    ) -> io::Result<ResponseEnvelope> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "AgentMux named pipe transport is only available on Windows",
        ))
    }

    pub fn subscribe_named_pipe_events(
        _pipe_name: &str,
        _request: &RequestEnvelope,
        _timeout: Duration,
    ) -> io::Result<(ResponseEnvelope, NamedPipeEventStream)> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "AgentMux named pipe transport is only available on Windows",
        ))
    }

    pub fn serve_named_pipe_requests<F>(_pipe_name: &str, _handler: F) -> io::Result<()>
    where
        F: Fn(RequestEnvelope) -> ResponseEnvelope,
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "AgentMux named pipe transport is only available on Windows",
        ))
    }

    pub fn serve_named_pipe_streaming_requests<F>(_pipe_name: &str, _handler: F) -> io::Result<()>
    where
        F: Fn(RequestEnvelope, ControlPipeConnection) -> io::Result<()> + Send + Sync + 'static,
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "AgentMux named pipe transport is only available on Windows",
        ))
    }

    pub fn serve_one_named_pipe_request<F>(_pipe_name: &str, _handler: F) -> io::Result<()>
    where
        F: Fn(RequestEnvelope) -> ResponseEnvelope,
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "AgentMux named pipe transport is only available on Windows",
        ))
    }

    pub fn serve_one_named_pipe_streaming_request<F>(
        _pipe_name: &str,
        _handler: F,
    ) -> io::Result<()>
    where
        F: FnOnce(RequestEnvelope, ControlPipeConnection) -> io::Result<()>,
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "AgentMux named pipe transport is only available on Windows",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_uses_control_schema() {
        let request = RequestEnvelope::new("req_1", "workspace.list", "{}", "token");
        assert_eq!(request.schema, CONTROL_SCHEMA);
        assert_eq!(request.method, "workspace.list");
    }

    #[test]
    fn error_codes_are_protocol_strings() {
        assert_eq!(ErrorCode::Unauthorized.as_str(), "unauthorized");
        assert_eq!(ErrorCode::SessionNotFound.as_str(), "session_not_found");
    }

    #[test]
    fn parses_session_spawn_params() {
        let request = RequestEnvelope::new(
            "req_spawn",
            "session.spawn",
            r#"{"workspace_id":"ws_1","command":["cmd.exe","/c","echo ok"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral"}"#,
            "token",
        );

        let params: SessionSpawnParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id, "ws_1");
        assert_eq!(params.backend, None);
        assert_eq!(params.backend_profile, None);
        assert_eq!(params.command[0], "cmd.exe");
        assert_eq!(params.columns, 80);
    }

    #[test]
    fn parses_terminal_split_params_with_optional_environment() {
        let omitted = RequestEnvelope::new(
            "req_terminal_split_default_env",
            "terminal.split",
            r#"{"workspace_id":"ws_1","pane_id":"pane_1","axis":"vertical","ratio":null,"behavior":"clone_current","backend":null,"backend_profile":null,"command":[],"cwd":null,"columns":null,"rows":null,"durability":null}"#,
            "token",
        );
        let params: TerminalSplitParams = omitted.parse_params().unwrap();
        assert!(params.env.is_empty());

        let supplied = RequestEnvelope::new(
            "req_terminal_split_env",
            "terminal.split",
            r#"{"workspace_id":"ws_1","pane_id":"pane_1","axis":"vertical","ratio":null,"behavior":"clone_current","backend":null,"backend_profile":null,"command":[],"env":[{"key":"TEAM","value":"worker"}],"cwd":null,"columns":null,"rows":null,"durability":null}"#,
            "token",
        );
        let params: TerminalSplitParams = supplied.parse_params().unwrap();
        assert_eq!(
            params.env,
            vec![EnvVarParam {
                key: "TEAM".to_string(),
                value: "worker".to_string(),
            }]
        );
    }

    #[test]
    fn agent_telemetry_team_fields_are_not_empty() {
        let mut telemetry = AgentTelemetry::default();
        assert!(telemetry.is_empty());

        telemetry.team_generation = Some(1);
        assert!(!telemetry.is_empty());

        telemetry.team_generation = None;
        telemetry.team_mutation_id = Some("mutation-1".to_string());
        assert!(!telemetry.is_empty());

        telemetry.team_mutation_id = None;
        telemetry.team_auto_adopt = Some(false);
        assert!(!telemetry.is_empty());
    }

    #[test]
    fn parses_session_spawn_backend() {
        let request = RequestEnvelope::new(
            "req_spawn",
            "session.spawn",
            r#"{"workspace_id":"ws_1","backend":"wsl-direct","backend_profile":"Ubuntu","command":["bash"],"cwd":"/home/dev","columns":80,"rows":24,"durability":"ephemeral"}"#,
            "token",
        );

        let params: SessionSpawnParams = request.parse_params().unwrap();
        assert_eq!(params.backend.as_deref(), Some("wsl-direct"));
        assert_eq!(params.backend_profile.as_deref(), Some("Ubuntu"));
    }

    #[test]
    fn parses_action_run_params() {
        let request = RequestEnvelope::new(
            "req_action_run",
            "actions.run",
            r#"{"action_id":"custom.verify","workspace_id":"ws_1","pane_id":"pane_1"}"#,
            "token",
        );

        let params: ActionRunParams = request.parse_params().unwrap();
        assert_eq!(params.action_id, "custom.verify");
        assert_eq!(params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(params.pane_id.as_deref(), Some("pane_1"));
    }

    #[test]
    fn parses_app_config_migrate_project_params() {
        let request = RequestEnvelope::new(
            "req_config_migrate",
            "config.migrate_project",
            r#"{"workspace_id":"ws_1","overwrite":true}"#,
            "token",
        );

        let params: AppConfigMigrateProjectParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(params.overwrite, Some(true));
    }

    #[test]
    fn parses_app_config_terminal_gpu_acceleration_update() {
        let request = RequestEnvelope::new(
            "req_config_update",
            "config.update",
            r#"{"ui":{"terminal_gpu_acceleration":"on"}}"#,
            "token",
        );

        let params: AppConfigUpdateParams = request.parse_params().unwrap();
        assert_eq!(
            params
                .ui
                .and_then(|ui| ui.terminal_gpu_acceleration)
                .as_deref(),
            Some("on")
        );
    }

    #[test]
    fn parses_app_config_diagnostics_params() {
        let request = RequestEnvelope::new(
            "req_config_diagnostics",
            "config.diagnostics",
            r#"{"workspace_id":"ws_1"}"#,
            "token",
        );

        let params: AppConfigDiagnosticsParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id.as_deref(), Some("ws_1"));
    }

    #[test]
    fn parses_dock_get_params() {
        let request = RequestEnvelope::new(
            "req_dock_get",
            "dock.get",
            r#"{"workspace_id":"ws_1"}"#,
            "token",
        );

        let params: DockGetParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id.as_deref(), Some("ws_1"));
    }

    #[test]
    fn parses_dock_trust_params() {
        let request = RequestEnvelope::new(
            "req_dock_trust",
            "dock.trust",
            r#"{"workspace_id":"ws_1"}"#,
            "token",
        );

        let params: DockTrustParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id, "ws_1");
    }

    #[test]
    fn parses_session_attach_params() {
        let request = RequestEnvelope::new(
            "req_attach",
            "session.attach",
            r#"{"session_id":"ses_existing","workspace_id":"ws_1","backend":"wsl-tmux-control","backend_profile":"Ubuntu","backend_ref":"agentmux_ws_1","columns":80,"rows":24,"durability":"durable"}"#,
            "token",
        );

        let params: SessionAttachParams = request.parse_params().unwrap();
        assert_eq!(params.session_id.as_deref(), Some("ses_existing"));
        assert_eq!(params.workspace_id, "ws_1");
        assert_eq!(params.backend, "wsl-tmux-control");
        assert_eq!(params.backend_profile.as_deref(), Some("Ubuntu"));
        assert_eq!(params.backend_ref, "agentmux_ws_1");
    }

    #[test]
    fn parses_session_list_params() {
        let request = RequestEnvelope::new(
            "req_session_list",
            "session.list",
            r#"{"workspace_id":"ws_1"}"#,
            "token",
        );

        let params: SessionListParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id.as_deref(), Some("ws_1"));
    }

    #[test]
    fn parses_event_poll_params() {
        let request = RequestEnvelope::new(
            "req_events_poll",
            "events.poll",
            r#"{"workspace_id":"ws_1","session_id":"ses_1","types":["session.state_changed"],"max_events":10}"#,
            "token",
        );

        let params: EventPollParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(params.session_id.as_deref(), Some("ses_1"));
        assert_eq!(
            params.types.as_deref(),
            Some(vec!["session.state_changed".to_string()].as_slice())
        );
        assert_eq!(params.max_events, Some(10));
    }

    #[test]
    fn parses_event_subscribe_params() {
        let request = RequestEnvelope::new(
            "req_events_subscribe",
            "events.subscribe",
            r#"{"workspace_id":"ws_1","session_id":"ses_1","types":["session.output"],"after_event_id":"evt_00000012"}"#,
            "token",
        );

        let params: EventSubscribeParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(params.session_id.as_deref(), Some("ses_1"));
        assert_eq!(params.types, Some(vec!["session.output".to_string()]));
        assert_eq!(params.after_event_id.as_deref(), Some("evt_00000012"));
    }

    #[test]
    fn parses_agent_and_notification_params() {
        let request = RequestEnvelope::new(
            "req_agent_state",
            "agent.set_state",
            r#"{"session_id":"ses_1","state":"waiting_for_input","reason":"approval needed"}"#,
            "token",
        );
        let params: AgentSetStateParams = request.parse_params().unwrap();
        assert_eq!(params.session_id, "ses_1");
        assert_eq!(params.state, "waiting_for_input");
        assert_eq!(params.reason.as_deref(), Some("approval needed"));

        let request = RequestEnvelope::new(
            "req_notifications",
            "notification.list",
            r#"{"workspace_id":"ws_1","severity":"warning","include_dismissed":true}"#,
            "token",
        );
        let params: NotificationListParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(params.severity.as_deref(), Some("warning"));
        assert_eq!(params.include_dismissed, Some(true));

        let request = RequestEnvelope::new(
            "req_browser_diagnostics",
            "diagnostics.browser",
            r#"{"workspace_id":"ws_1","surface_id":"surf_browser"}"#,
            "token",
        );
        let params: BrowserDiagnosticsParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(params.surface_id.as_deref(), Some("surf_browser"));
    }

    #[test]
    fn control_api_fixtures_match_current_schema() {
        let request: RequestEnvelope = serde_json::from_str(include_str!(
            "../../../tests/fixtures/control-plane/session-spawn-request.json"
        ))
        .unwrap();
        assert_eq!(request.schema, CONTROL_SCHEMA);
        assert_eq!(request.method, "session.spawn");
        let params: SessionSpawnParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id, "ws_fixture");
        assert_eq!(params.backend.as_deref(), Some("conpty"));
        assert_eq!(params.command, vec!["cmd.exe", "/d", "/q"]);

        let response: ResponseEnvelope = serde_json::from_str(include_str!(
            "../../../tests/fixtures/control-plane/session-spawn-response.json"
        ))
        .unwrap();
        let ResponseOutcome::Ok { result_json } = response.outcome else {
            panic!("expected ok response fixture");
        };
        let result: SessionSpawnResult = serde_json::from_str(&result_json).unwrap();
        assert_eq!(result.session_id, "ses_fixture");

        let response: ResponseEnvelope = serde_json::from_str(include_str!(
            "../../../tests/fixtures/control-plane/unauthorized-response.json"
        ))
        .unwrap();
        let ResponseOutcome::Error(error) = response.outcome else {
            panic!("expected error response fixture");
        };
        assert_eq!(error.code, ErrorCode::Unauthorized);
        assert_eq!(error.message, "Invalid local control token.");
    }

    #[test]
    #[cfg(windows)]
    fn named_pipe_transport_round_trips_response_envelope() {
        let pipe_name = format!(
            r"\\.\pipe\agentmux-ipc-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        );
        let server_pipe_name = pipe_name.clone();
        let handle = std::thread::spawn(move || {
            serve_one_named_pipe_request(&server_pipe_name, |request| {
                ResponseEnvelope::ok_typed(request.id, &AckResult { ok: true })
            })
            .unwrap();
        });

        let response = send_named_pipe_request(
            &pipe_name,
            &RequestEnvelope::new("req_pipe", "diagnostics.recovery", "{}", "token"),
            std::time::Duration::from_secs(2),
        )
        .unwrap();

        handle.join().unwrap();
        assert!(matches!(response.outcome, ResponseOutcome::Ok { .. }));
    }

    #[test]
    #[cfg(windows)]
    fn named_pipe_streaming_transport_reads_subscription_events() {
        let pipe_name = format!(
            r"\\.\pipe\agentmux-ipc-stream-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        );
        let server_pipe_name = pipe_name.clone();
        let handle = std::thread::spawn(move || {
            serve_one_named_pipe_streaming_request(&server_pipe_name, |request, mut stream| {
                stream.write_response(&ResponseEnvelope::ok_typed(
                    request.id,
                    &EventSubscribeResult {
                        subscribed: true,
                        cursor: "evt_00000000".to_string(),
                        dropped_count: 0,
                    },
                ))?;

                let mut event = EventFrame::new("evt_00000001", "session.output");
                event.session_id = Some("ses_1".to_string());
                event.data_json = r#"{"byte_count":2}"#.to_string();
                stream.write_event(&event)
            })
            .unwrap();
        });

        let (response, mut events) = subscribe_named_pipe_events(
            &pipe_name,
            &RequestEnvelope::new("req_stream", "events.subscribe", "{}", "token"),
            std::time::Duration::from_secs(2),
        )
        .unwrap();
        assert!(matches!(response.outcome, ResponseOutcome::Ok { .. }));

        let event = events.read_event().unwrap().unwrap();
        handle.join().unwrap();
        assert_eq!(event.event_id, "evt_00000001");
        assert_eq!(event.event_type, "session.output");
        assert_eq!(event.session_id.as_deref(), Some("ses_1"));
    }

    #[test]
    fn parses_workspace_create_params() {
        let request = RequestEnvelope::new(
            "req_workspace",
            "workspace.create",
            r#"{"name":"AgentMux","project_root":"D:\\Projects\\agentmux","backend_profile":null}"#,
            "token",
        );

        let params: WorkspaceCreateParams = request.parse_params().unwrap();
        assert_eq!(params.name, "AgentMux");
        assert!(params.project_root.unwrap().contains("agentmux"));
    }

    #[test]
    fn parses_workspace_update_params() {
        let request = RequestEnvelope::new(
            "req_workspace_update",
            "workspace.update",
            r##"{"workspace_id":"ws_1","name":"AgentMux","project_root":"D:\\Projects\\agentmux","environment_profile_id":"Ubuntu","description":"demo","icon":"AM","color":"#22C55E","default_wsl_distribution":"Ubuntu","default_terminal_profile":"powershell","default_agent_command":"codex --resume"}"##,
            "token",
        );
        let params: WorkspaceUpdateParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id, "ws_1");
        assert_eq!(params.name, "AgentMux");
        assert_eq!(params.environment_profile_id.as_deref(), Some("Ubuntu"));
        assert_eq!(params.description.as_deref(), Some("demo"));
        assert_eq!(
            params.default_agent_command.as_deref(),
            Some("codex --resume")
        );
        assert_eq!(
            params.default_terminal_profile.as_deref(),
            Some("powershell")
        );
    }

    #[test]
    fn parses_workspace_group_params() {
        let request = RequestEnvelope::new(
            "req_group_create",
            "workspace_group.create",
            r##"{"name":"Agents","anchor_workspace_id":"ws_1","workspace_ids":["ws_1","ws_2"],"collapsed":true,"pinned":true,"color":"#22C55E","icon":"A"}"##,
            "token",
        );

        let params: WorkspaceGroupCreateParams = request.parse_params().unwrap();
        assert_eq!(params.name, "Agents");
        assert_eq!(params.anchor_workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(params.workspace_ids.as_ref().unwrap().len(), 2);
        assert_eq!(params.collapsed, Some(true));
        assert_eq!(params.pinned, Some(true));

        let request = RequestEnvelope::new(
            "req_group_add",
            "workspace_group.add_workspace",
            r#"{"group_id":"grp_1","workspace_id":"ws_2","position":5}"#,
            "token",
        );

        let params: WorkspaceGroupMemberParams = request.parse_params().unwrap();
        assert_eq!(params.group_id, "grp_1");
        assert_eq!(params.workspace_id, "ws_2");
        assert_eq!(params.position, Some(5));

        let request = RequestEnvelope::new(
            "req_group_update",
            "workspace_group.update",
            r##"{"group_id":"grp_1","name":"Core","sort_order":7}"##,
            "token",
        );

        let params: WorkspaceGroupUpdateParams = request.parse_params().unwrap();
        assert_eq!(params.group_id, "grp_1");
        assert_eq!(params.name.as_deref(), Some("Core"));
        assert_eq!(params.sort_order, Some(7));
    }

    #[test]
    fn parses_pane_split_params() {
        let request = RequestEnvelope::new(
            "req_pane_split",
            "pane.split",
            r#"{"workspace_id":"ws_1","pane_id":"pane_1","axis":"vertical","ratio":0.4}"#,
            "token",
        );

        let params: PaneSplitParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id, "ws_1");
        assert_eq!(params.pane_id, "pane_1");
        assert_eq!(params.axis, "vertical");
        assert_eq!(params.ratio, Some(0.4));
    }

    #[test]
    fn parses_pane_focus_params() {
        let request = RequestEnvelope::new(
            "req_pane_focus",
            "pane.focus",
            r#"{"workspace_id":"ws_1","pane_id":"pane_2"}"#,
            "token",
        );

        let params: PaneFocusParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id, "ws_1");
        assert_eq!(params.pane_id, "pane_2");
    }

    #[test]
    fn parses_pane_close_params() {
        let request = RequestEnvelope::new(
            "req_pane_close",
            "pane.close",
            r#"{"workspace_id":"ws_1","pane_id":"pane_2","surface_policy":"fail_if_session_running"}"#,
            "token",
        );

        let params: PaneCloseParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id, "ws_1");
        assert_eq!(params.pane_id, "pane_2");
        assert_eq!(params.surface_policy, "fail_if_session_running");
    }

    #[test]
    fn parses_pane_resize_layout_params() {
        let request = RequestEnvelope::new(
            "req_pane_resize_layout",
            "pane.resize_layout",
            r#"{"workspace_id":"ws_1","pane_id":"pane_split","ratio":0.65}"#,
            "token",
        );

        let params: PaneResizeLayoutParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id, "ws_1");
        assert_eq!(params.pane_id, "pane_split");
        assert_eq!(params.ratio, 0.65);
    }

    #[test]
    fn parses_pane_mount_surface_params() {
        let request = RequestEnvelope::new(
            "req_pane_mount_surface",
            "pane.mount_surface",
            r#"{"workspace_id":"ws_1","pane_id":"pane_2","surface_id":"surf_1"}"#,
            "token",
        );

        let params: PaneMountSurfaceParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id, "ws_1");
        assert_eq!(params.pane_id, "pane_2");
        assert_eq!(params.surface_id, "surf_1");
    }

    #[test]
    fn parses_pane_unmount_surface_params() {
        let request = RequestEnvelope::new(
            "req_pane_unmount_surface",
            "pane.unmount_surface",
            r#"{"workspace_id":"ws_1","pane_id":"pane_2"}"#,
            "token",
        );

        let params: PaneUnmountSurfaceParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id, "ws_1");
        assert_eq!(params.pane_id, "pane_2");
    }

    #[test]
    fn parses_surface_close_params() {
        let request = RequestEnvelope::new(
            "req_surface_close",
            "surface.close",
            r#"{"workspace_id":"ws_1","surface_id":"surf_1"}"#,
            "token",
        );

        let params: SurfaceCloseParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id, "ws_1");
        assert_eq!(params.surface_id, "surf_1");
    }

    #[test]
    fn parses_browser_surface_and_command_params() {
        let request = RequestEnvelope::new(
            "req_create_browser",
            "surface.create_browser",
            r#"{"workspace_id":"ws_browser","pane_id":"pane_browser","profile":"default","placement":"active_pane"}"#,
            "token",
        );
        let params: SurfaceCreateBrowserParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id, "ws_browser");
        assert_eq!(params.pane_id.as_deref(), Some("pane_browser"));
        assert_eq!(params.placement.as_deref(), Some("active_pane"));
        assert_eq!(params.profile.as_deref(), Some("default"));

        let request = RequestEnvelope::new(
            "req_browser_navigate",
            "browser.navigate",
            r#"{"surface_id":"surf_browser","url":"https://example.invalid"}"#,
            "token",
        );
        let params: BrowserNavigateParams = request.parse_params().unwrap();
        assert_eq!(params.surface_id, "surf_browser");
        assert_eq!(params.url, "https://example.invalid");

        let request = RequestEnvelope::new(
            "req_browser_screenshot",
            "browser.screenshot",
            r#"{"surface_id":"surf_browser","format":"png"}"#,
            "token",
        );
        let params: BrowserScreenshotParams = request.parse_params().unwrap();
        assert_eq!(params.format.as_deref(), Some("png"));

        let request = RequestEnvelope::new(
            "req_browser_dom",
            "browser.dom_snapshot",
            r#"{"surface_id":"surf_browser","frame_id":"frame_1"}"#,
            "token",
        );
        let params: BrowserDomSnapshotParams = request.parse_params().unwrap();
        assert_eq!(params.surface_id, "surf_browser");
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));

        let request = RequestEnvelope::new(
            "req_browser_click",
            "browser.click",
            r#"{"surface_id":"surf_browser","x":12.0,"y":24.0}"#,
            "token",
        );
        let params: BrowserClickParams = request.parse_params().unwrap();
        assert_eq!(params.x, Some(12.0));
        assert_eq!(params.y, Some(24.0));
        assert_eq!(params.frame_id, None);

        let request = RequestEnvelope::new(
            "req_browser_click_frame",
            "browser.click",
            r##"{"surface_id":"surf_browser","selector":"#q","frame_id":"frame_1"}"##,
            "token",
        );
        let params: BrowserClickParams = request.parse_params().unwrap();
        assert_eq!(params.selector.as_deref(), Some("#q"));
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));

        let request = RequestEnvelope::new(
            "req_browser_type",
            "browser.type",
            r##"{"surface_id":"surf_browser","selector":"#q","text":"agentmux","frame_id":"frame_1"}"##,
            "token",
        );
        let params: BrowserTypeParams = request.parse_params().unwrap();
        assert_eq!(params.selector, "#q");
        assert_eq!(params.text, "agentmux");
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));

        let request = RequestEnvelope::new(
            "req_browser_fill",
            "browser.fill",
            r##"{"surface_id":"surf_browser","selector":"#q","text":"agentmux","frame_id":"frame_1"}"##,
            "token",
        );
        let params: BrowserFillParams = request.parse_params().unwrap();
        assert_eq!(params.selector, "#q");
        assert_eq!(params.text, "agentmux");
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));

        let request = RequestEnvelope::new(
            "req_browser_press",
            "browser.press",
            r##"{"surface_id":"surf_browser","selector":"#q","key":"Enter","frame_id":"frame_1"}"##,
            "token",
        );
        let params: BrowserPressParams = request.parse_params().unwrap();
        assert_eq!(params.key, "Enter");
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));

        let request = RequestEnvelope::new(
            "req_browser_select",
            "browser.select",
            r##"{"surface_id":"surf_browser","selector":"#choice","values":["one","two"],"frame_id":"frame_1"}"##,
            "token",
        );
        let params: BrowserSelectParams = request.parse_params().unwrap();
        assert_eq!(params.values, vec!["one".to_string(), "two".to_string()]);
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));

        let request = RequestEnvelope::new(
            "req_browser_scroll",
            "browser.scroll",
            r##"{"surface_id":"surf_browser","selector":"#list","x":0,"y":400,"frame_id":"frame_1"}"##,
            "token",
        );
        let params: BrowserScrollParams = request.parse_params().unwrap();
        assert_eq!(params.selector.as_deref(), Some("#list"));
        assert_eq!(params.y, Some(400));
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));

        let request = RequestEnvelope::new(
            "req_browser_hover",
            "browser.hover",
            r##"{"surface_id":"surf_browser","selector":"#submit","frame_id":"frame_1"}"##,
            "token",
        );
        let params: BrowserHoverParams = request.parse_params().unwrap();
        assert_eq!(params.selector, "#submit");
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));

        let request = RequestEnvelope::new(
            "req_browser_check",
            "browser.check",
            r##"{"surface_id":"surf_browser","selector":"#agree","checked":true,"frame_id":"frame_1"}"##,
            "token",
        );
        let params: BrowserCheckParams = request.parse_params().unwrap();
        assert_eq!(params.checked, Some(true));
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));

        let request = RequestEnvelope::new(
            "req_browser_get",
            "browser.get",
            r##"{"surface_id":"surf_browser","selector":"#title","kind":"attribute","attribute":"href","frame_id":"frame_1"}"##,
            "token",
        );
        let params: BrowserGetParams = request.parse_params().unwrap();
        assert_eq!(params.kind.as_deref(), Some("attribute"));
        assert_eq!(params.attribute.as_deref(), Some("href"));
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));

        let request = RequestEnvelope::new(
            "req_browser_find",
            "browser.find",
            r##"{"surface_id":"surf_browser","query":"agentmux","selector":"main","limit":5,"frame_id":"frame_1"}"##,
            "token",
        );
        let params: BrowserFindParams = request.parse_params().unwrap();
        assert_eq!(params.query, "agentmux");
        assert_eq!(params.selector.as_deref(), Some("main"));
        assert_eq!(params.limit, Some(5));
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));

        let request = RequestEnvelope::new(
            "req_browser_highlight",
            "browser.highlight",
            r##"{"surface_id":"surf_browser","selector":"#q","duration_ms":750,"frame_id":"frame_1"}"##,
            "token",
        );
        let params: BrowserHighlightParams = request.parse_params().unwrap();
        assert_eq!(params.selector, "#q");
        assert_eq!(params.duration_ms, Some(750));
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));

        let request = RequestEnvelope::new(
            "req_browser_focus",
            "browser.focus",
            r##"{"surface_id":"surf_browser","selector":"#q","frame_id":"frame_1"}"##,
            "token",
        );
        let params: BrowserFocusParams = request.parse_params().unwrap();
        assert_eq!(params.selector, "#q");
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));

        let request = RequestEnvelope::new(
            "req_browser_zoom",
            "browser.zoom",
            r#"{"surface_id":"surf_browser","percent":125}"#,
            "token",
        );
        let params: BrowserZoomParams = request.parse_params().unwrap();
        assert_eq!(params.percent, 125);

        let request = RequestEnvelope::new(
            "req_browser_wait",
            "browser.wait_for_selector",
            r##"{"surface_id":"surf_browser","selector":"#ready","timeout_ms":1500,"frame_id":"frame_1"}"##,
            "token",
        );
        let params: BrowserWaitForSelectorParams = request.parse_params().unwrap();
        assert_eq!(params.selector, "#ready");
        assert_eq!(params.timeout_ms, Some(1500));
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));

        let request = RequestEnvelope::new(
            "req_browser_current_url",
            "browser.current_url",
            r#"{"surface_id":"surf_browser"}"#,
            "token",
        );
        let params: BrowserSurfaceParams = request.parse_params().unwrap();
        assert_eq!(params.surface_id, "surf_browser");

        let request = RequestEnvelope::new(
            "req_browser_frames",
            "browser.frames",
            r#"{"surface_id":"surf_browser"}"#,
            "token",
        );
        let params: BrowserSurfaceParams = request.parse_params().unwrap();
        assert_eq!(params.surface_id, "surf_browser");

        let request = RequestEnvelope::new(
            "req_browser_storage",
            "browser.storage",
            r#"{"surface_id":"surf_browser"}"#,
            "token",
        );
        let params: BrowserSurfaceParams = request.parse_params().unwrap();
        assert_eq!(params.surface_id, "surf_browser");

        let request = RequestEnvelope::new(
            "req_browser_cookies",
            "browser.cookies",
            r#"{"surface_id":"surf_browser"}"#,
            "token",
        );
        let params: BrowserSurfaceParams = request.parse_params().unwrap();
        assert_eq!(params.surface_id, "surf_browser");

        let request = RequestEnvelope::new(
            "req_browser_downloads",
            "browser.downloads",
            r#"{"surface_id":"surf_browser","limit":25}"#,
            "token",
        );
        let params: BrowserDownloadsParams = request.parse_params().unwrap();
        assert_eq!(params.surface_id, "surf_browser");
        assert_eq!(params.limit, Some(25));

        let request = RequestEnvelope::new(
            "req_browser_history",
            "browser.history",
            r#"{"surface_id":"surf_browser"}"#,
            "token",
        );
        let params: BrowserSurfaceParams = request.parse_params().unwrap();
        assert_eq!(params.surface_id, "surf_browser");

        let request = RequestEnvelope::new(
            "req_browser_console",
            "browser.console",
            r#"{"surface_id":"surf_browser","limit":25}"#,
            "token",
        );
        let params: BrowserConsoleParams = request.parse_params().unwrap();
        assert_eq!(params.surface_id, "surf_browser");
        assert_eq!(params.limit, Some(25));

        let request = RequestEnvelope::new(
            "req_browser_dialogs",
            "browser.dialogs",
            r#"{"surface_id":"surf_browser","limit":25}"#,
            "token",
        );
        let params: BrowserDialogsParams = request.parse_params().unwrap();
        assert_eq!(params.surface_id, "surf_browser");
        assert_eq!(params.limit, Some(25));

        let request = RequestEnvelope::new(
            "req_browser_dialog_respond",
            "browser.dialog.respond",
            r#"{"surface_id":"surf_browser","dialog_id":"surf_browser:dialog:0001","accept":true,"prompt_text":"approved"}"#,
            "token",
        );
        let params: BrowserDialogRespondParams = request.parse_params().unwrap();
        assert_eq!(params.dialog_id, "surf_browser:dialog:0001");
        assert!(params.accept);
        assert_eq!(params.prompt_text.as_deref(), Some("approved"));

        let request = RequestEnvelope::new(
            "req_browser_dialog_cancel",
            "browser.dialog.cancel",
            r#"{"surface_id":"surf_browser","dialog_id":"surf_browser:dialog:0001"}"#,
            "token",
        );
        let params: BrowserDialogCancelParams = request.parse_params().unwrap();
        assert_eq!(params.surface_id, "surf_browser");
        assert_eq!(params.dialog_id, "surf_browser:dialog:0001");

        let request = RequestEnvelope::new(
            "req_browser_errors",
            "browser.errors",
            r#"{"surface_id":"surf_browser","limit":25}"#,
            "token",
        );
        let params: BrowserErrorsParams = request.parse_params().unwrap();
        assert_eq!(params.surface_id, "surf_browser");
        assert_eq!(params.limit, Some(25));

        let request = RequestEnvelope::new(
            "req_browser_eval",
            "browser.evaluate",
            r#"{"surface_id":"surf_browser","script":"document.title"}"#,
            "token",
        );
        let params: BrowserEvaluateParams = request.parse_params().unwrap();
        assert_eq!(params.script, "document.title");
        assert_eq!(params.frame_id, None);

        let request = RequestEnvelope::new(
            "req_browser_eval_frame",
            "browser.evaluate",
            r#"{"surface_id":"surf_browser","script":"document.body.dataset.ready","frame_id":"frame_1"}"#,
            "token",
        );
        let params: BrowserEvaluateParams = request.parse_params().unwrap();
        assert_eq!(params.script, "document.body.dataset.ready");
        assert_eq!(params.frame_id.as_deref(), Some("frame_1"));
    }

    #[test]
    fn serializes_typed_result_into_response() {
        let response = ResponseEnvelope::ok_typed(
            "req_spawn",
            &SessionSpawnResult {
                session_id: "ses_1".to_string(),
            },
        );

        match response.outcome {
            ResponseOutcome::Ok { result_json } => {
                assert!(result_json.contains("ses_1"));
            }
            ResponseOutcome::Error(error) => panic!("unexpected error: {error:?}"),
        }
    }

    #[test]
    fn request_caller_metadata_round_trips_and_remains_optional() {
        let request = RequestEnvelope::new("req_caller", "workspace.list", "{}", "token")
            .with_caller(ControlCaller {
                source: "mcp-http".to_string(),
                profile: Some("standard".to_string()),
                client_session_id: Some("mcp-session-1".to_string()),
            });
        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: RequestEnvelope = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.caller, request.caller);

        let legacy: RequestEnvelope = serde_json::from_str(
            r#"{"schema":"agentmux.control.v1","id":"req_legacy","method":"workspace.list","params_json":"{}","auth":{"token":"token"}}"#,
        )
        .unwrap();
        assert_eq!(legacy.caller, None);
    }

    #[test]
    fn git_operation_contracts_round_trip_and_validate() {
        let page: GitStatusPageParams = serde_json::from_str(
            r#"{"workspace_id":"ws_1","repository_id":"repo_1","state":"unstaged","cursor":"cursor_1","limit":100,"generation":8}"#,
        )
        .unwrap();
        page.validate().unwrap();
        assert_eq!(serde_json::to_value(&page).unwrap()["limit"], 100);

        let unfiltered_page: GitStatusPageParams =
            serde_json::from_str(r#"{"workspace_id":"ws_1","query":""}"#).unwrap();
        unfiltered_page.validate().unwrap();

        let max_filter: GitStatusPageParams = serde_json::from_value(serde_json::json!({
            "workspace_id": "ws_1",
            "query": "a".repeat(512),
        }))
        .unwrap();
        max_filter.validate().unwrap();

        let oversized_filter: GitStatusPageParams = serde_json::from_value(serde_json::json!({
            "workspace_id": "ws_1",
            "query": "a".repeat(513),
        }))
        .unwrap();
        assert!(oversized_filter.validate().is_err());

        let diff: GitDiffParams = serde_json::from_str(
            r#"{"workspace_id":"ws_1","path":"src/main.rs","stage":"working_tree","context_lines":12}"#,
        )
        .unwrap();
        diff.validate().unwrap();

        let mutation: GitPathMutationParams = serde_json::from_str(
            r#"{"workspace_id":"ws_1","paths":["src/main.rs"],"idempotency_key":"git-stage-1"}"#,
        )
        .unwrap();
        mutation.validate().unwrap();

        let summary = GitStatusSummaryResult {
            workspace_id: "ws_1".to_string(),
            repository_id: "repo_1".to_string(),
            repository_root: "D:/work/repo".to_string(),
            branch: Some("main".to_string()),
            head_oid: Some("abc123".to_string()),
            upstream: Some("origin/main".to_string()),
            ahead: 1,
            behind: 0,
            staged_count: 2,
            unstaged_count: 3,
            untracked_count: 4,
            conflicted_count: 0,
            generation: 9,
            refreshed_at: "2026-07-23T00:00:00Z".to_string(),
        };
        let encoded = serde_json::to_string(&summary).unwrap();
        assert_eq!(
            serde_json::from_str::<GitStatusSummaryResult>(&encoded).unwrap(),
            summary
        );

        let invalid: GitStatusPageParams =
            serde_json::from_str(r#"{"workspace_id":"ws_1","limit":501}"#).unwrap();
        assert_eq!(
            invalid.validate().unwrap_err().code,
            ErrorCode::InvalidRequest
        );
    }

    #[test]
    fn worktree_contracts_require_safe_operation_identity() {
        let create: AgentWorktreeCreateParams = serde_json::from_str(
            r#"{"workspace_id":"ws_1","branch":"agent/fix-scroll","destination":"D:/worktrees/fix-scroll","base_revision":"main","create_branch":true,"backend":"wsl-direct","backend_profile":"Ubuntu","command":["claude","-c"],"idempotency_key":"worktree-create-1"}"#,
        )
        .unwrap();
        create.validate().unwrap();
        assert_eq!(
            serde_json::from_str::<AgentWorktreeCreateParams>(
                &serde_json::to_string(&create).unwrap()
            )
            .unwrap(),
            create
        );

        let missing_identity: AgentWorktreeRecoverParams = serde_json::from_str("{}").unwrap();
        assert_eq!(
            missing_identity.validate().unwrap_err().code,
            ErrorCode::InvalidRequest
        );

        let result = AgentWorktreeResult {
            operation_id: "op_1".to_string(),
            worktree_id: "wt_1".to_string(),
            workspace_id: "ws_2".to_string(),
            branch: "agent/fix-scroll".to_string(),
            path: "D:/worktrees/fix-scroll".to_string(),
            state: "completed".to_string(),
            surface_id: Some("surface_1".to_string()),
            pane_id: Some("pane_1".to_string()),
            session_id: Some("session_1".to_string()),
            reused: false,
            recovered: false,
        };
        assert_eq!(
            serde_json::from_str::<AgentWorktreeResult>(&serde_json::to_string(&result).unwrap())
                .unwrap(),
            result
        );
    }

    #[test]
    fn review_contracts_preserve_anchor_and_comment_history() {
        let create: GitReviewThreadCreateParams = serde_json::from_str(
            r#"{"workspace_id":"ws_1","pane_id":"pane_1","repository_id":"repo_1","anchor":{"path":"src/lib.rs","side":"right","line":42,"start_line":40,"base_revision":"base","head_revision":"head","hunk_header":"@@ -40,3 +40,5 @@","diff_hash":"hash"},"body":"Please handle the error path.","author_session_id":"ses_1"}"#,
        )
        .unwrap();
        create.validate().unwrap();
        assert_eq!(create.pane_id.as_deref(), Some("pane_1"));

        let invalid_anchor: GitReviewThreadCreateParams = serde_json::from_str(
            r#"{"workspace_id":"ws_1","anchor":{"path":"src/lib.rs","side":"right","line":7,"start_line":8},"body":"note"}"#,
        )
        .unwrap();
        assert_eq!(
            invalid_anchor.validate().unwrap_err().code,
            ErrorCode::InvalidRequest
        );

        let comment = GitReviewCommentResult {
            comment_id: "comment_1".to_string(),
            thread_id: "thread_1".to_string(),
            body: "Please handle the error path.".to_string(),
            author_session_id: Some("ses_1".to_string()),
            created_at: "2026-07-23T00:00:00Z".to_string(),
            updated_at: "2026-07-23T00:00:00Z".to_string(),
        };
        let thread = GitReviewThreadResult {
            thread_id: "thread_1".to_string(),
            workspace_id: "ws_1".to_string(),
            repository_id: "repo_1".to_string(),
            author_session_id: Some("ses_1".to_string()),
            anchor: create.anchor,
            resolved: false,
            stale: false,
            stale_reason: None,
            created_at: "2026-07-23T00:00:00Z".to_string(),
            updated_at: "2026-07-23T00:00:00Z".to_string(),
            comments: vec![comment],
        };
        assert_eq!(
            serde_json::from_str::<GitReviewThreadResult>(&serde_json::to_string(&thread).unwrap())
                .unwrap(),
            thread
        );

        let mut legacy_thread = serde_json::to_value(&thread).unwrap();
        legacy_thread
            .as_object_mut()
            .expect("serialized review thread object")
            .remove("author_session_id");
        let legacy_thread: GitReviewThreadResult = serde_json::from_value(legacy_thread).unwrap();
        assert_eq!(legacy_thread.author_session_id, None);
    }

    #[test]
    fn agent_hook_and_development_server_contracts_round_trip() {
        let hook: AgentHookStateParams = serde_json::from_str(
            r#"{"workspace_id":"ws_1","session_id":"ses_1","sequence":4,"state":"waiting_for_input","reason":"approval needed","source":"claude_hook","observed_at":"2026-07-23T00:00:00Z"}"#,
        )
        .unwrap();
        hook.validate().unwrap();

        let candidate: DevelopmentServerCandidateParams = serde_json::from_str(
            r#"{"workspace_id":"ws_1","session_id":"ses_1","url":"http://127.0.0.1:5173","source":"vite","detected_at":"2026-07-23T00:00:00Z","process_id":1234}"#,
        )
        .unwrap();
        candidate.validate().unwrap();

        let open: DevelopmentServerCandidateOpenInSplitParams = serde_json::from_str(
            r#"{"candidate_id":"dev_1","pane_id":"pane_1","axis":"vertical","ratio":0.4}"#,
        )
        .unwrap();
        open.validate().unwrap();

        let invalid: DevelopmentServerCandidateOpenInSplitParams =
            serde_json::from_str(r#"{"candidate_id":"dev_1","ratio":1.0}"#).unwrap();
        assert_eq!(
            invalid.validate().unwrap_err().code,
            ErrorCode::InvalidRequest
        );
    }

    #[test]
    fn five_track_method_names_are_stable_control_fixtures() {
        let methods = [
            METHOD_GIT_STATUS_SUMMARY,
            METHOD_GIT_STATUS_PAGE,
            METHOD_GIT_DIFF,
            METHOD_GIT_STAGE,
            METHOD_GIT_UNSTAGE,
            METHOD_GIT_STAGE_ALL,
            METHOD_GIT_UNSTAGE_ALL,
            METHOD_GIT_DISCARD,
            METHOD_GIT_COMMIT,
            METHOD_AGENT_WORKTREE_CREATE,
            METHOD_AGENT_WORKTREE_LIST,
            METHOD_AGENT_WORKTREE_RECOVER,
            METHOD_AGENT_WORKTREE_REMOVE,
            METHOD_GIT_REVIEW_THREAD_LIST,
            METHOD_GIT_REVIEW_THREAD_CREATE,
            METHOD_GIT_REVIEW_THREAD_UPDATE,
            METHOD_GIT_REVIEW_THREAD_DELETE,
            METHOD_GIT_REVIEW_THREAD_MARK_STALE,
            METHOD_GIT_REVIEW_THREAD_DELIVER,
            METHOD_GIT_REVIEW_COMMENT_LIST,
            METHOD_GIT_REVIEW_COMMENT_CREATE,
            METHOD_GIT_REVIEW_COMMENT_UPDATE,
            METHOD_GIT_REVIEW_COMMENT_DELETE,
            METHOD_AGENT_HOOK_STATE,
            METHOD_DEV_SERVER_CANDIDATE_DETECTED,
            METHOD_DEV_SERVER_CANDIDATE_LIST,
            METHOD_DEV_SERVER_CANDIDATE_DISMISS,
            METHOD_DEV_SERVER_CANDIDATE_OPEN_IN_SPLIT,
        ];
        assert!(methods.iter().all(|method| method.contains('.')));
        assert_eq!(EVENT_GIT_REPOSITORY_CHANGED, "git.repository_changed");
        assert_eq!(EVENT_AGENT_WORKTREE_PROGRESS, "agent.worktree.progress");
        assert_eq!(EVENT_AGENT_HOOK_STATE_CHANGED, "agent.hook_state_changed");
        assert_eq!(
            EVENT_DEV_SERVER_CANDIDATE_DETECTED,
            "dev_server.candidate_detected"
        );
    }
}
