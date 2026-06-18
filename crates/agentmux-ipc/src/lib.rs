use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub const CONTROL_SCHEMA: &str = "agentmux.control.v1";
pub const EVENT_SCHEMA: &str = "agentmux.event.v1";
pub const DEFAULT_CONTROL_PIPE_NAME: &str = r"\\.\pipe\agentmux-control";
pub const DEFAULT_LOCAL_CONTROL_TOKEN: &str = "desktop-bootstrap-token";
pub const DEFAULT_CONTROL_TOKEN_FILE_NAME: &str = "control.token";

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
        }
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
    pub columns: u16,
    pub rows: u16,
    pub durability: Option<String>,
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
pub struct WorkspaceCloseParams {
    pub workspace_id: String,
    pub close_policy: String,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentSetStateParams {
    pub session_id: String,
    pub state: String,
    pub reason: Option<String>,
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
pub struct NotificationDismissParams {
    pub notification_id: String,
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
pub struct BrowserScreenshotParams {
    pub surface_id: String,
    pub format: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDomSnapshotParams {
    pub surface_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BrowserClickParams {
    pub surface_id: String,
    pub selector: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserTypeParams {
    pub surface_id: String,
    pub selector: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserEvaluateParams {
    pub surface_id: String,
    pub script: String,
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
pub struct SessionSummaryResult {
    pub session_id: String,
    pub workspace_id: String,
    pub backend_kind: String,
    pub state: String,
    pub exit_code: Option<i32>,
    pub backend_native_id: Option<String>,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentStateResult {
    pub session_id: String,
    pub workspace_id: String,
    pub state: String,
    pub attention: bool,
    pub reason: Option<String>,
    pub updated_at: Option<String>,
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
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserDomSnapshotResult {
    pub surface_id: String,
    pub html: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserActionResult {
    pub surface_id: String,
    pub ok: bool,
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
pub struct DiagnosticsExportResult {
    pub generated_at: String,
    pub format_version: String,
    pub recovery: RecoveryDiagnosticsResult,
    pub browser: BrowserDiagnosticsResult,
    pub notifications: Vec<NotificationSummaryResult>,
    pub backend_health: Vec<DiagnosticsBackendHealthResult>,
    pub queue_pressure: Vec<DiagnosticsQueuePressureResult>,
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

        let mut reader = BufReader::new(file);
        let mut response_json = String::new();
        reader.read_line(&mut response_json)?;
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

        let mut reader = BufReader::new(file);
        let mut response_json = String::new();
        reader.read_line(&mut response_json)?;
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
    fn parses_session_spawn_backend() {
        let request = RequestEnvelope::new(
            "req_spawn",
            "session.spawn",
            r#"{"workspace_id":"ws_1","backend":"wsl-direct","backend_profile":"Ubuntu","command":["bash"],"cwd":"/home/irae","columns":80,"rows":24,"durability":"ephemeral"}"#,
            "token",
        );

        let params: SessionSpawnParams = request.parse_params().unwrap();
        assert_eq!(params.backend.as_deref(), Some("wsl-direct"));
        assert_eq!(params.backend_profile.as_deref(), Some("Ubuntu"));
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
            r#"{"name":"AgentMux","project_root":"D:\\Workspace\\irae\\agentmux","backend_profile":null}"#,
            "token",
        );

        let params: WorkspaceCreateParams = request.parse_params().unwrap();
        assert_eq!(params.name, "AgentMux");
        assert!(params.project_root.unwrap().contains("agentmux"));
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
    fn parses_browser_surface_and_command_params() {
        let request = RequestEnvelope::new(
            "req_create_browser",
            "surface.create_browser",
            r#"{"workspace_id":"ws_browser","pane_id":"pane_browser","profile":"default"}"#,
            "token",
        );
        let params: SurfaceCreateBrowserParams = request.parse_params().unwrap();
        assert_eq!(params.workspace_id, "ws_browser");
        assert_eq!(params.pane_id.as_deref(), Some("pane_browser"));
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
            "req_browser_click",
            "browser.click",
            r#"{"surface_id":"surf_browser","x":12.0,"y":24.0}"#,
            "token",
        );
        let params: BrowserClickParams = request.parse_params().unwrap();
        assert_eq!(params.x, Some(12.0));
        assert_eq!(params.y, Some(24.0));

        let request = RequestEnvelope::new(
            "req_browser_type",
            "browser.type",
            r##"{"surface_id":"surf_browser","selector":"#q","text":"agentmux"}"##,
            "token",
        );
        let params: BrowserTypeParams = request.parse_params().unwrap();
        assert_eq!(params.selector, "#q");
        assert_eq!(params.text, "agentmux");

        let request = RequestEnvelope::new(
            "req_browser_eval",
            "browser.evaluate",
            r#"{"surface_id":"surf_browser","script":"document.title"}"#,
            "token",
        );
        let params: BrowserEvaluateParams = request.parse_params().unwrap();
        assert_eq!(params.script, "document.title");
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
}
