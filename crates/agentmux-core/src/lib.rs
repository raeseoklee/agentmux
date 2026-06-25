use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::prelude::{Engine as _, BASE64_STANDARD};
use std::time::{SystemTime, UNIX_EPOCH};

use agentmux_backend::{
    AttachRequest, BackendError, BackendEvent, BackendKind as BackendTraitKind, BackendResult,
    CommandSpec, InputEvent, NamedKey, SessionBackend, SpawnRequest, TerminalSize, TerminationMode,
};
use agentmux_ipc::{
    AckResult, AgentAttentionListResult, AgentListAttentionParams, AgentSetStateParams,
    AgentStateResult, AgentTelemetry, ControlError, ErrorCode, EventFrame, EventPollParams,
    EventPollResult, EventSubscribeParams, EventSubscribeResult, NotificationDismissParams,
    NotificationListParams, NotificationListResult, NotificationSummaryResult, RequestEnvelope,
    ResponseEnvelope, SessionAttachParams, SessionIdParams, SessionListParams, SessionListResult,
    SessionOutputPressureParams, SessionReadRecentParams, SessionReadRecentResult,
    SessionResizeParams, SessionSendKeyParams, SessionSendTextParams, SessionSnapshotParams,
    SessionSnapshotResult, SessionSpawnParams, SessionSpawnResult, SessionSummaryResult,
    SessionTerminateParams,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_id(prefix: &str) -> String {
    let value = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{value:08}")
}

/// Seed the global id counter so ids minted this run never collide with ids
/// persisted by a previous run.
///
/// `next_id` draws from a single process-wide counter shared by every domain id
/// (workspace, pane, surface, session, …). It restarts at 1 on each launch, so
/// without seeding the first new pane/surface/session after a restart reuses an
/// id already on disk and the store's upsert silently overwrites the existing
/// row — mixing entities across workspaces and panes. Call this once at startup
/// with `max_persisted_sequence + 1`. Idempotent and monotonic: it only ever
/// raises the counter (`fetch_max`), so it is safe to call more than once.
pub fn seed_next_id(at_least: u64) {
    NEXT_ID.fetch_max(at_least, Ordering::Relaxed);
}

fn next_timestamped_id(prefix: &str, sequence: u64) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("{prefix}_{millis}_{sequence:06}")
}

macro_rules! id_type {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                Self(next_id($prefix))
            }

            pub fn from_string(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id_type!(WorkspaceId, "ws");
id_type!(PaneId, "pane");
id_type!(SurfaceId, "surf");
id_type!(SessionId, "ses");
id_type!(BackendAttachmentId, "att");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaneKind {
    Split,
    Leaf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceType {
    Terminal,
    Browser,
    Log,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendKind {
    Conpty,
    WslDirect,
    WslTmuxControl,
    Ssh,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Durability {
    Ephemeral,
    Durable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionState {
    Starting,
    Running,
    Detached,
    Recovering,
    Disconnected,
    Exited { code: Option<i32> },
    Failed { code: String, message: String },
    Lost,
}

impl SessionState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            SessionState::Exited { .. } | SessionState::Failed { .. } | SessionState::Lost
        )
    }

    pub fn can_transition_to(&self, next: &SessionState) -> bool {
        use SessionState::*;

        match self {
            Starting => matches!(
                next,
                Running | Disconnected | Exited { .. } | Failed { .. } | Lost
            ),
            Running => matches!(
                next,
                Detached | Recovering | Disconnected | Exited { .. } | Failed { .. } | Lost
            ),
            Detached => matches!(
                next,
                Running | Recovering | Disconnected | Exited { .. } | Failed { .. } | Lost
            ),
            Recovering => matches!(
                next,
                Running | Detached | Disconnected | Failed { .. } | Lost
            ),
            Disconnected => matches!(next, Recovering | Failed { .. } | Lost),
            Exited { .. } | Failed { .. } | Lost => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentState {
    Unknown,
    Running,
    Idle,
    WaitingForInput,
    Completed,
    Failed,
    Detached,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentStateRecord {
    session_id: SessionId,
    workspace_id: WorkspaceId,
    state: AgentState,
    attention: bool,
    reason: Option<String>,
    updated_at: String,
    telemetry: Option<AgentTelemetry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationRecord {
    pub notification_id: String,
    pub notification_type: String,
    pub severity: String,
    pub workspace_id: Option<WorkspaceId>,
    pub session_id: Option<SessionId>,
    pub title: String,
    pub message: String,
    pub created_at: String,
    pub dismissed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DetectedAgentSignal {
    state: AgentState,
    reason: Option<String>,
    source: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AgentSignalDetectorInput<'a> {
    bytes: &'a [u8],
    heuristic: Option<AgentHeuristicDetectorInput>,
}

impl<'a> AgentSignalDetectorInput<'a> {
    fn explicit_only(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            heuristic: None,
        }
    }

    #[cfg(test)]
    fn with_heuristics(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            heuristic: Some(AgentHeuristicDetectorInput { enabled: true }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AgentHeuristicDetectorInput {
    enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Workspace {
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub root_pane_id: PaneId,
    pub active_pane_id: PaneId,
    pub project_root: Option<String>,
    pub environment_profile_id: Option<String>,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pane {
    pub pane_id: PaneId,
    pub workspace_id: WorkspaceId,
    pub parent_pane_id: Option<PaneId>,
    pub kind: PaneKind,
    pub split_axis: Option<SplitAxis>,
    pub split_ratio: Option<f32>,
    pub mounted_surface_id: Option<SurfaceId>,
    pub last_focused_at: Option<SystemTime>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Surface {
    pub surface_id: SurfaceId,
    pub workspace_id: WorkspaceId,
    pub surface_type: SurfaceType,
    pub title: String,
    pub session_id: Option<SessionId>,
    pub browser_id: Option<String>,
    pub created_at: SystemTime,
    pub last_visible_at: Option<SystemTime>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Session {
    pub session_id: SessionId,
    pub backend_kind: BackendKind,
    pub backend_attachment_id: Option<BackendAttachmentId>,
    pub backend_native_id: Option<String>,
    pub workspace_id: WorkspaceId,
    pub cwd: Option<String>,
    pub command: Vec<String>,
    pub state: SessionState,
    pub exit_code: Option<i32>,
    pub created_at: SystemTime,
    pub last_seen_at: Option<SystemTime>,
    pub durability: Durability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoreEvent {
    WorkspaceChanged {
        workspace_id: WorkspaceId,
    },
    PaneChanged {
        workspace_id: WorkspaceId,
        pane_id: PaneId,
    },
    SurfaceMounted {
        pane_id: PaneId,
        surface_id: SurfaceId,
    },
    SessionStateChanged {
        session_id: SessionId,
        from: SessionState,
        to: SessionState,
    },
    SessionOutputBatch {
        session_id: SessionId,
        from_offset: u64,
        bytes: Vec<u8>,
    },
    SessionCwdChanged {
        session_id: SessionId,
        cwd: String,
    },
    AgentStateChanged {
        session_id: SessionId,
        state: AgentState,
        reason: Option<String>,
        source: String,
        telemetry: Option<AgentTelemetry>,
    },
    NotificationCreated {
        notification: NotificationRecord,
    },
    BackendHealthChanged {
        attachment_id: BackendAttachmentId,
        state: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSpawnSpec {
    pub workspace_id: WorkspaceId,
    pub backend: Option<BackendKind>,
    pub backend_profile: Option<String>,
    pub command: CommandSpec,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
    pub initial_size: TerminalSize,
    pub durability: Durability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionAttachSpec {
    pub session_id: Option<SessionId>,
    pub workspace_id: WorkspaceId,
    pub backend: BackendKind,
    pub backend_profile: Option<String>,
    pub backend_ref: String,
    pub initial_size: TerminalSize,
    pub durability: Durability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeSessionSummary {
    pub session_id: SessionId,
    pub workspace_id: WorkspaceId,
    pub backend_kind: BackendKind,
    pub state: SessionState,
    pub exit_code: Option<i32>,
    pub backend_native_id: Option<String>,
}

/// An atomic snapshot of a session's recent output ring plus the absolute byte
/// offsets it covers. `bytes` are the raw bytes for the half-open absolute range
/// `[base_offset, end_offset)`; `end_offset` equals the total bytes ever emitted
/// by the session, which is where a live output stream attaches.
#[derive(Debug, Clone)]
pub struct OutputSnapshot {
    pub base_offset: u64,
    pub end_offset: u64,
    pub bytes: Vec<u8>,
}

/// A contiguous run of session output for the live stream: `bytes` begin at the
/// absolute offset `from_offset`.
#[derive(Debug, Clone)]
pub struct OutputDelta {
    pub session_id: SessionId,
    pub from_offset: u64,
    pub bytes: Vec<u8>,
}

const OUTPUT_FLOW_CONTROL_PAUSE_BYTES: u64 = 1024 * 1024;
const OUTPUT_FLOW_CONTROL_RESUME_BYTES: u64 = 256 * 1024;
const AGENT_HEURISTIC_SCAN_INTERVAL_BYTES: u64 = 8 * 1024;

#[derive(Debug, Clone, Default)]
struct RecentOutputBuffer {
    bytes: VecDeque<u8>,
}

impl RecentOutputBuffer {
    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn append_limited(&mut self, bytes: &[u8], limit: usize) {
        if limit == 0 {
            self.bytes.clear();
            return;
        }

        if bytes.len() >= limit {
            self.bytes.clear();
            self.bytes
                .extend(bytes[bytes.len() - limit..].iter().copied());
            return;
        }

        self.bytes.extend(bytes.iter().copied());
        let overflow = self.bytes.len().saturating_sub(limit);
        if overflow > 0 {
            self.bytes.drain(..overflow);
        }
    }

    fn tail(&self, max_bytes: usize) -> Vec<u8> {
        let count = max_bytes.min(self.bytes.len());
        self.bytes
            .iter()
            .skip(self.bytes.len().saturating_sub(count))
            .copied()
            .collect()
    }

    fn from_index(&self, index: usize) -> Vec<u8> {
        self.bytes.iter().skip(index).copied().collect()
    }
}

pub struct TerminalRuntime<B>
where
    B: SessionBackend,
{
    backend: B,
    sessions: HashMap<SessionId, Session>,
    recent_output: HashMap<SessionId, RecentOutputBuffer>,
    recent_output_limit: usize,
    // Absolute total bytes ever emitted per session (monotonic; never rebased on
    // ring rotation). Drives the snapshot/stream offset contract.
    output_offsets: HashMap<SessionId, u64>,
}

impl<B> TerminalRuntime<B>
where
    B: SessionBackend,
{
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            sessions: HashMap::new(),
            recent_output: HashMap::new(),
            recent_output_limit: 64 * 1024,
            output_offsets: HashMap::new(),
        }
    }

    pub fn spawn_session(&mut self, spec: SessionSpawnSpec) -> BackendResult<SessionId> {
        let session_id = SessionId::new();
        let now = SystemTime::now();
        let command = spec.command.clone();
        let handle = self.backend.spawn(SpawnRequest {
            session_id: session_id.to_string(),
            workspace_id: Some(spec.workspace_id.to_string()),
            backend: spec.backend.map(backend_kind_to_backend),
            backend_profile: spec.backend_profile,
            command: spec.command,
            cwd: spec.cwd.clone(),
            env: spec.env,
            initial_size: spec.initial_size,
        })?;

        let session = Session {
            session_id: session_id.clone(),
            backend_kind: backend_kind_from_backend(handle.backend_kind),
            backend_attachment_id: None,
            backend_native_id: handle.backend_native_id,
            workspace_id: spec.workspace_id,
            cwd: spec.cwd,
            command: command_spec_to_vec(command),
            state: SessionState::Starting,
            exit_code: None,
            created_at: now,
            last_seen_at: Some(now),
            durability: spec.durability,
        };

        self.sessions.insert(session_id.clone(), session);
        Ok(session_id)
    }

    pub fn attach_session(&mut self, spec: SessionAttachSpec) -> BackendResult<SessionId> {
        let session_id = spec.session_id.unwrap_or_default();
        let now = SystemTime::now();
        let backend = spec.backend;
        let backend_ref = spec.backend_ref.clone();
        let handle = self.backend.attach(AttachRequest {
            session_id: session_id.to_string(),
            backend: backend_kind_to_backend(backend),
            backend_profile: spec.backend_profile,
            backend_ref: spec.backend_ref,
            initial_size: spec.initial_size,
        })?;

        let session = Session {
            session_id: session_id.clone(),
            backend_kind: backend_kind_from_backend(handle.backend_kind),
            backend_attachment_id: None,
            backend_native_id: handle.backend_native_id.or(Some(backend_ref.clone())),
            workspace_id: spec.workspace_id,
            cwd: None,
            command: vec!["attach".to_string(), backend_ref],
            state: SessionState::Starting,
            exit_code: None,
            created_at: now,
            last_seen_at: Some(now),
            durability: spec.durability,
        };

        self.sessions.insert(session_id.clone(), session);
        Ok(session_id)
    }

    pub fn send_text(
        &mut self,
        session_id: &SessionId,
        text: impl Into<String>,
    ) -> BackendResult<()> {
        self.backend
            .send_input(session_id.as_str(), InputEvent::Text(text.into()))
    }

    pub fn send_key(&mut self, session_id: &SessionId, key: NamedKey) -> BackendResult<()> {
        self.backend
            .send_input(session_id.as_str(), InputEvent::Key(key))
    }

    pub fn resize_session(
        &mut self,
        session_id: &SessionId,
        size: TerminalSize,
    ) -> BackendResult<()> {
        self.backend.resize(session_id.as_str(), size)
    }

    pub fn set_output_paused(&mut self, session_id: &SessionId, paused: bool) -> BackendResult<()> {
        self.backend.set_output_paused(session_id.as_str(), paused)
    }

    pub fn terminate_session(
        &mut self,
        session_id: &SessionId,
        mode: TerminationMode,
    ) -> BackendResult<()> {
        self.backend.terminate(session_id.as_str(), mode)
    }

    /// Update the in-memory working directory for a session (driven by OSC 7).
    /// Returns true when the session exists and the cwd actually changed.
    pub fn set_session_cwd(&mut self, session_id: &SessionId, cwd: &str) -> bool {
        match self.sessions.get_mut(session_id) {
            Some(session) if session.cwd.as_deref() != Some(cwd) => {
                session.cwd = Some(cwd.to_string());
                true
            }
            _ => false,
        }
    }

    pub fn session_summary(&self, session_id: &SessionId) -> Option<RuntimeSessionSummary> {
        self.sessions
            .get(session_id)
            .map(|session| RuntimeSessionSummary {
                session_id: session.session_id.clone(),
                workspace_id: session.workspace_id.clone(),
                backend_kind: session.backend_kind,
                state: session.state.clone(),
                exit_code: session.exit_code,
                backend_native_id: session.backend_native_id.clone(),
            })
    }

    pub fn session_summaries(
        &self,
        workspace_id: Option<&WorkspaceId>,
    ) -> Vec<RuntimeSessionSummary> {
        self.sessions
            .values()
            .filter(|session| {
                workspace_id
                    .map(|workspace_id| session.workspace_id == *workspace_id)
                    .unwrap_or(true)
            })
            .map(|session| RuntimeSessionSummary {
                session_id: session.session_id.clone(),
                workspace_id: session.workspace_id.clone(),
                backend_kind: session.backend_kind,
                state: session.state.clone(),
                exit_code: session.exit_code,
                backend_native_id: session.backend_native_id.clone(),
            })
            .collect()
    }

    pub fn read_recent(&self, session_id: &SessionId, max_bytes: usize) -> Option<Vec<u8>> {
        if !self.sessions.contains_key(session_id) {
            return None;
        }

        let Some(output) = self.recent_output.get(session_id) else {
            return Some(Vec::new());
        };
        Some(output.tail(max_bytes))
    }

    /// Atomic snapshot of a session's recent output ring plus the absolute byte
    /// range it covers. Returns `None` if the session is unknown.
    pub fn snapshot_output(
        &self,
        session_id: &SessionId,
        since_offset: Option<u64>,
    ) -> Option<OutputSnapshot> {
        if !self.sessions.contains_key(session_id) {
            return None;
        }
        let end_offset = self.output_offsets.get(session_id).copied().unwrap_or(0);
        let ring = self.recent_output.get(session_id);
        let ring_len = ring.map(RecentOutputBuffer::len).unwrap_or(0);
        let ring_base = end_offset.saturating_sub(ring_len as u64);
        // Start at `since` when it lies within the retained ring; otherwise
        // return the whole ring (cold start, or the caller fell behind the
        // bounded window — signalled to the renderer by base_offset > since).
        let start = match since_offset {
            Some(since) if since >= ring_base && since <= end_offset => since,
            _ => ring_base,
        };
        let bytes = ring
            .map(|ring| ring.from_index((start - ring_base) as usize))
            .unwrap_or_default();
        Some(OutputSnapshot {
            base_offset: start,
            end_offset,
            bytes,
        })
    }

    pub fn drain_events(&mut self) -> Vec<CoreEvent> {
        self.backend
            .drain_events()
            .into_iter()
            .filter_map(|event| self.apply_backend_event(event))
            .collect()
    }

    fn apply_backend_event(&mut self, event: BackendEvent) -> Option<CoreEvent> {
        match event {
            BackendEvent::Started { session_id } => {
                let session_id = SessionId::from_string(session_id);
                let session = self.sessions.get_mut(&session_id)?;
                let from = session.state.clone();
                if from.can_transition_to(&SessionState::Running) {
                    session.state = SessionState::Running;
                    session.last_seen_at = Some(SystemTime::now());
                    Some(CoreEvent::SessionStateChanged {
                        session_id,
                        from,
                        to: SessionState::Running,
                    })
                } else {
                    None
                }
            }
            BackendEvent::Output { session_id, bytes } => {
                let session_id = SessionId::from_string(session_id);
                if let Some(session) = self.sessions.get_mut(&session_id) {
                    session.last_seen_at = Some(SystemTime::now());
                    let from_offset = {
                        let counter = self.output_offsets.entry(session_id.clone()).or_default();
                        let from_offset = *counter;
                        *counter += bytes.len() as u64;
                        from_offset
                    };
                    append_recent_output(
                        &mut self.recent_output,
                        &session_id,
                        &bytes,
                        self.recent_output_limit,
                    );
                    Some(CoreEvent::SessionOutputBatch {
                        session_id,
                        from_offset,
                        bytes,
                    })
                } else {
                    None
                }
            }
            BackendEvent::Resized {
                session_id,
                columns,
                rows,
            } => Some(CoreEvent::SessionStateChanged {
                session_id: SessionId::from_string(session_id),
                from: SessionState::Running,
                to: SessionState::Running,
            })
            .filter(|_| columns > 0 && rows > 0),
            BackendEvent::Exited { session_id, code } => {
                let session_id = SessionId::from_string(session_id);
                let session = self.sessions.get_mut(&session_id)?;
                let from = session.state.clone();
                let to = SessionState::Exited { code };
                if from.can_transition_to(&to) {
                    session.state = to.clone();
                    session.exit_code = code;
                    session.last_seen_at = Some(SystemTime::now());
                    Some(CoreEvent::SessionStateChanged {
                        session_id,
                        from,
                        to,
                    })
                } else {
                    None
                }
            }
            BackendEvent::HealthChanged {
                attachment_id,
                state,
            } => Some(CoreEvent::BackendHealthChanged {
                attachment_id: BackendAttachmentId::from_string(attachment_id),
                state: format!("{state:?}"),
            }),
            BackendEvent::Error { session_id, error } => session_id.map(|session_id| {
                let session_id = SessionId::from_string(session_id);
                if let Some(session) = self.sessions.get_mut(&session_id) {
                    let failed = SessionState::Failed {
                        code: error.code,
                        message: error.message,
                    };
                    let from = session.state.clone();
                    session.state = failed.clone();
                    CoreEvent::SessionStateChanged {
                        session_id,
                        from,
                        to: failed,
                    }
                } else {
                    CoreEvent::BackendHealthChanged {
                        attachment_id: BackendAttachmentId::from_string("unknown"),
                        state: error.message,
                    }
                }
            }),
        }
    }
}

pub struct RuntimeControlPlane<B>
where
    B: SessionBackend,
{
    runtime: TerminalRuntime<B>,
    auth_token: String,
    agent_states: HashMap<String, AgentStateRecord>,
    agent_heuristics_enabled: bool,
    notifications: Vec<NotificationRecord>,
    next_notification_id: u64,
    notification_limit: usize,
    agent_heuristic_next_scan_offset: HashMap<SessionId, u64>,
    events: Vec<EventFrame>,
    event_history: Vec<EventFrame>,
    next_event_id: u64,
    dropped_event_count: usize,
    event_backlog_limit: usize,
    // Per-session coalesced output deltas tapped for the live byte stream,
    // drained by the host and pushed to the per-session Channel. Separate from
    // the byte-less `events` history.
    pending_output: HashMap<SessionId, OutputDelta>,
    // Per-session working-directory updates detected from OSC 7 in terminal
    // output, drained by the host and persisted so the footer git status tracks
    // the directory the shell has cd'd into. Last write per session wins.
    pending_cwd: HashMap<SessionId, String>,
}

impl<B> RuntimeControlPlane<B>
where
    B: SessionBackend,
{
    pub fn new(runtime: TerminalRuntime<B>, auth_token: impl Into<String>) -> Self {
        Self {
            runtime,
            auth_token: auth_token.into(),
            agent_states: HashMap::new(),
            agent_heuristics_enabled: false,
            notifications: Vec::new(),
            next_notification_id: 1,
            notification_limit: 256,
            agent_heuristic_next_scan_offset: HashMap::new(),
            events: Vec::new(),
            event_history: Vec::new(),
            next_event_id: 1,
            dropped_event_count: 0,
            event_backlog_limit: 1024,
            pending_output: HashMap::new(),
            pending_cwd: HashMap::new(),
        }
    }

    pub fn runtime(&self) -> &TerminalRuntime<B> {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut TerminalRuntime<B> {
        &mut self.runtime
    }

    pub fn set_agent_heuristics_enabled(&mut self, enabled: bool) {
        self.agent_heuristics_enabled = enabled;
    }

    fn agent_heuristic_scan_allowed(
        &mut self,
        session_id: &SessionId,
        from_offset: u64,
        byte_len: usize,
    ) -> bool {
        let next_allowed = self
            .agent_heuristic_next_scan_offset
            .get(session_id)
            .copied()
            .unwrap_or(0);
        if from_offset < next_allowed {
            return false;
        }

        self.agent_heuristic_next_scan_offset.insert(
            session_id.clone(),
            from_offset
                .saturating_add(byte_len as u64)
                .saturating_add(AGENT_HEURISTIC_SCAN_INTERVAL_BYTES),
        );
        true
    }

    /// Atomic snapshot of a session's recent output ring + absolute offsets, for
    /// a renderer cold-start that then attaches the stream at `end_offset`.
    pub fn snapshot_output(
        &self,
        session_id: &SessionId,
        since_offset: Option<u64>,
    ) -> Option<OutputSnapshot> {
        self.runtime.snapshot_output(session_id, since_offset)
    }

    /// Drains the coalesced per-session output deltas accumulated since the last
    /// call, for the live output stream. Call after `collect_events`.
    pub fn drain_output_stream(&mut self) -> Vec<OutputDelta> {
        if self.pending_output.is_empty() {
            return Vec::new();
        }
        self.pending_output
            .drain()
            .map(|(_, delta)| delta)
            .collect()
    }

    /// Drains the per-session working-directory updates detected from OSC 7
    /// since the last call. Call after `collect_events`; the host persists each
    /// `(session_id, cwd)` so the footer git status follows the live directory.
    pub fn drain_cwd_updates(&mut self) -> Vec<(String, String)> {
        if self.pending_cwd.is_empty() {
            return Vec::new();
        }
        self.pending_cwd
            .drain()
            .map(|(session_id, cwd)| (session_id.to_string(), cwd))
            .collect()
    }

    pub fn collect_events(&mut self) {
        for event in self.runtime.drain_events() {
            let agent_signals = match &event {
                CoreEvent::SessionOutputBatch {
                    session_id,
                    bytes,
                    from_offset,
                } => {
                    // Tap raw output bytes for the live stream, coalescing
                    // contiguous batches per session (separate from the
                    // byte-less EventFrame pushed below).
                    self.pending_output
                        .entry(session_id.clone())
                        .or_insert_with(|| OutputDelta {
                            session_id: session_id.clone(),
                            from_offset: *from_offset,
                            bytes: Vec::new(),
                        })
                        .bytes
                        .extend_from_slice(bytes);
                    let mut signals = detect_agent_signals(bytes);
                    if signals.is_empty()
                        && self.agent_heuristics_enabled
                        && contains_heuristic_agent_signal_marker(bytes)
                        && self.agent_heuristic_scan_allowed(session_id, *from_offset, bytes.len())
                    {
                        signals.extend(detect_heuristic_agent_signals(bytes));
                    }
                    signals
                        .into_iter()
                        .map(|signal| (session_id.clone(), signal))
                        .collect()
                }
                _ => Vec::new(),
            };
            let agent_lifecycle = match &event {
                CoreEvent::SessionStateChanged { session_id, to, .. } => {
                    self.agent_lifecycle_transition(session_id, to)
                }
                _ => None,
            };
            // Live cwd tracking: tap output for OSC 7 so the footer git follows
            // the directory the shell cd'd into.
            let cwd_update = match &event {
                CoreEvent::SessionOutputBatch {
                    session_id, bytes, ..
                } => parse_osc7_cwd(bytes).map(|cwd| (session_id.clone(), cwd)),
                _ => None,
            };
            if let Some(frame) = self.frame_from_core_event(event) {
                self.push_event(frame);
            }
            if let Some((session_id, cwd)) = cwd_update {
                if self.runtime.set_session_cwd(&session_id, &cwd) {
                    self.pending_cwd.insert(session_id.clone(), cwd.clone());
                    if let Some(frame) =
                        self.frame_from_core_event(CoreEvent::SessionCwdChanged { session_id, cwd })
                    {
                        self.push_event(frame);
                    }
                }
            }
            if let Some((session_id, state, reason, source)) = agent_lifecycle {
                let _ = self.apply_agent_state_transition(
                    session_id,
                    state,
                    Some(reason),
                    source,
                    None,
                );
            }
            for (session_id, signal) in agent_signals {
                if let Ok(result) = self.apply_agent_state_transition(
                    session_id,
                    signal.state,
                    signal.reason,
                    signal.source,
                    None,
                ) {
                    let _ = result;
                }
            }
        }
    }

    pub fn agent_state_snapshot(&self) -> Vec<AgentStateResult> {
        self.agent_states.values().map(agent_state_result).collect()
    }

    pub fn notification_snapshot(&self) -> Vec<NotificationSummaryResult> {
        self.notifications
            .iter()
            .rev()
            .map(notification_summary_result)
            .collect()
    }

    pub fn current_event_cursor(&self) -> String {
        format!("evt_{:08}", self.next_event_id.saturating_sub(1))
    }

    pub fn dropped_event_count(&self) -> usize {
        self.dropped_event_count
    }

    pub fn event_queue_depth(&self) -> usize {
        self.events.len()
    }

    pub fn event_history_depth(&self) -> usize {
        self.event_history.len()
    }

    pub fn event_backlog_limit(&self) -> usize {
        self.event_backlog_limit
    }

    pub fn notification_depth(&self) -> usize {
        self.notifications.len()
    }

    pub fn notification_limit(&self) -> usize {
        self.notification_limit
    }

    pub fn validate_event_cursor(&self, event_id: &str) -> Result<(), ControlError> {
        parse_event_id(event_id).map(|_| ())
    }

    pub fn event_subscription_batch(
        &mut self,
        params: &EventSubscribeParams,
        after_event_id: &str,
        max_events: usize,
    ) -> Result<Vec<EventFrame>, ControlError> {
        let cursor = parse_event_id(after_event_id)?;
        self.collect_events();
        Ok(self
            .event_history
            .iter()
            .filter(|event| event_id_value(&event.event_id).is_some_and(|id| id > cursor))
            .filter(|event| event_matches_subscription(event, params))
            .take(max_events)
            .cloned()
            .collect())
    }

    pub fn handle_request(&mut self, request: RequestEnvelope) -> ResponseEnvelope {
        let id = request.id.clone();

        if request.schema != agentmux_ipc::CONTROL_SCHEMA {
            return ResponseEnvelope::error(
                id,
                ControlError::new(ErrorCode::InvalidRequest, "Unsupported control schema."),
            );
        }

        if request.auth.token != self.auth_token {
            return ResponseEnvelope::error(
                id,
                ControlError::new(ErrorCode::Unauthorized, "Invalid local control token."),
            );
        }

        let result = match request.method.as_str() {
            "session.spawn" => self.handle_session_spawn(&request),
            "session.attach" => self.handle_session_attach(&request),
            "session.list" => self.handle_session_list(&request),
            "session.get" => self.handle_session_get(&request),
            "session.send_text" => self.handle_session_send_text(&request),
            "session.send_key" => self.handle_session_send_key(&request),
            "session.resize" => self.handle_session_resize(&request),
            "session.terminate" => self.handle_session_terminate(&request),
            "session.read_recent" => self.handle_session_read_recent(&request),
            "session.snapshot" => self.handle_session_snapshot(&request),
            "session.report_output_pressure" => {
                self.handle_session_report_output_pressure(&request)
            }
            "events.poll" => self.handle_events_poll(&request),
            "events.subscribe" => self.handle_events_subscribe(&request),
            "agent.set_state" => self.handle_agent_set_state(&request),
            "agent.get_state" => self.handle_agent_get_state(&request),
            "agent.list_attention" => self.handle_agent_list_attention(&request),
            "agent.clear_attention" => self.handle_agent_clear_attention(&request),
            "notification.list" => self.handle_notification_list(&request),
            "notification.dismiss" => self.handle_notification_dismiss(&request),
            _ => Err(ControlError::new(
                ErrorCode::UnsupportedMethod,
                format!("Unsupported method '{}'.", request.method),
            )),
        };

        match result {
            Ok(response) => response,
            Err(error) => ResponseEnvelope::error(id, error),
        }
    }

    fn handle_session_spawn(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        let params: SessionSpawnParams = request.parse_params()?;
        if params.command.is_empty() {
            return Err(ControlError::new(
                ErrorCode::InvalidRequest,
                "session.spawn requires a non-empty command array.",
            ));
        }

        let durability = parse_durability(params.durability.as_deref())?;
        let backend = parse_backend_kind(params.backend.as_deref())?;
        let command = CommandSpec::with_args(
            params.command[0].clone(),
            params.command.iter().skip(1).cloned().collect(),
        );
        let session_id = self
            .runtime
            .spawn_session(SessionSpawnSpec {
                workspace_id: WorkspaceId::from_string(params.workspace_id),
                backend,
                backend_profile: params.backend_profile,
                command,
                cwd: params.cwd,
                env: params
                    .env
                    .into_iter()
                    .map(|entry| (entry.key, entry.value))
                    .collect(),
                initial_size: TerminalSize::new(params.columns, params.rows),
                durability,
            })
            .map_err(control_error_from_backend)?;

        self.collect_events();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &SessionSpawnResult {
                session_id: session_id.to_string(),
            },
        ))
    }

    fn handle_session_attach(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        let params: SessionAttachParams = request.parse_params()?;
        if params.backend_ref.trim().is_empty() {
            return Err(ControlError::new(
                ErrorCode::InvalidRequest,
                "session.attach requires a non-empty backend_ref.",
            ));
        }

        let backend = parse_backend_kind(Some(&params.backend))?.ok_or_else(|| {
            ControlError::new(
                ErrorCode::InvalidRequest,
                "session.attach requires backend.",
            )
        })?;
        let durability = parse_durability(params.durability.as_deref())?;
        let session_id = self
            .runtime
            .attach_session(SessionAttachSpec {
                session_id: params.session_id.map(SessionId::from_string),
                workspace_id: WorkspaceId::from_string(params.workspace_id),
                backend,
                backend_profile: params.backend_profile,
                backend_ref: params.backend_ref,
                initial_size: TerminalSize::new(params.columns, params.rows),
                durability,
            })
            .map_err(control_error_from_backend)?;

        self.collect_events();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &SessionSpawnResult {
                session_id: session_id.to_string(),
            },
        ))
    }

    fn handle_session_list(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        self.collect_events();
        let params: SessionListParams = request.parse_params()?;
        let workspace_id = params.workspace_id.map(WorkspaceId::from_string);
        let sessions = self
            .runtime
            .session_summaries(workspace_id.as_ref())
            .into_iter()
            .map(session_summary_result)
            .collect();

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &SessionListResult { sessions },
        ))
    }

    fn handle_session_get(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        self.collect_events();
        let params: SessionIdParams = request.parse_params()?;
        let session_id = SessionId::from_string(params.session_id);
        let summary = self
            .runtime
            .session_summary(&session_id)
            .ok_or_else(|| ControlError::new(ErrorCode::SessionNotFound, "Session not found."))?;

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &session_summary_result(summary),
        ))
    }

    fn handle_session_send_text(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        let params: SessionSendTextParams = request.parse_params()?;
        let session_id = SessionId::from_string(params.session_id);
        self.runtime
            .send_text(&session_id, params.text)
            .map_err(control_error_from_backend)?;
        self.collect_events();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_session_send_key(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        let params: SessionSendKeyParams = request.parse_params()?;
        let key = parse_named_key(&params.key)?;
        let session_id = SessionId::from_string(params.session_id);
        self.runtime
            .send_key(&session_id, key)
            .map_err(control_error_from_backend)?;
        self.collect_events();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_session_resize(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        let params: SessionResizeParams = request.parse_params()?;
        let session_id = SessionId::from_string(params.session_id);
        self.runtime
            .resize_session(&session_id, TerminalSize::new(params.columns, params.rows))
            .map_err(control_error_from_backend)?;
        self.collect_events();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_session_terminate(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        let params: SessionTerminateParams = request.parse_params()?;
        let session_id = SessionId::from_string(params.session_id);
        self.runtime
            .terminate_session(&session_id, parse_termination_mode(&params.mode)?)
            .map_err(control_error_from_backend)?;
        self.collect_events();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_session_read_recent(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        self.collect_events();
        let params: SessionReadRecentParams = request.parse_params()?;
        let session_id = SessionId::from_string(params.session_id);
        let output = self
            .runtime
            .read_recent(&session_id, params.max_bytes)
            .ok_or_else(|| ControlError::new(ErrorCode::SessionNotFound, "Session not found."))?;
        let text = String::from_utf8_lossy(&output).to_string();

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &SessionReadRecentResult {
                session_id: session_id.to_string(),
                text,
                byte_count: output.len(),
            },
        ))
    }

    fn handle_session_snapshot(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        self.collect_events();
        let params: SessionSnapshotParams = request.parse_params()?;
        let session_id = SessionId::from_string(params.session_id);
        let snapshot = self
            .snapshot_output(&session_id, params.since_offset)
            .ok_or_else(|| ControlError::new(ErrorCode::SessionNotFound, "Session not found."))?;
        // PR-6: when the delta poll asked for everything at or after `since` and
        // there is no new output, `bytes` is empty. Skip the Base64 encode (it
        // would only produce ""), keeping the steady, no-output snapshot poll
        // free of any encoding on the hot path. The empty string is wire-
        // compatible: clients already treat it as "no bytes".
        let bytes_base64 = if snapshot.bytes.is_empty() {
            String::new()
        } else {
            BASE64_STANDARD.encode(&snapshot.bytes)
        };
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &SessionSnapshotResult {
                session_id: session_id.to_string(),
                base_offset: snapshot.base_offset,
                end_offset: snapshot.end_offset,
                bytes_base64,
            },
        ))
    }

    fn handle_session_report_output_pressure(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        let params: SessionOutputPressureParams = request.parse_params()?;
        let session_id = SessionId::from_string(params.session_id);
        let pause =
            params.write_in_flight && params.queued_bytes >= OUTPUT_FLOW_CONTROL_PAUSE_BYTES;
        let resume =
            !params.write_in_flight || params.queued_bytes <= OUTPUT_FLOW_CONTROL_RESUME_BYTES;
        if pause || resume {
            self.runtime
                .set_output_paused(&session_id, pause && !resume)
                .map_err(control_error_from_backend)?;
        }
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_events_poll(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        self.collect_events();
        let params: EventPollParams = request.parse_params()?;
        let max_events = params
            .max_events
            .unwrap_or(256)
            .min(self.event_backlog_limit);
        let mut events = Vec::new();
        let mut retained = Vec::new();

        for event in self.events.drain(..) {
            if event_matches_poll(&event, &params) && events.len() < max_events {
                events.push(event);
            } else {
                retained.push(event);
            }
        }
        self.events = retained;

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &EventPollResult {
                events,
                dropped_count: self.dropped_event_count,
            },
        ))
    }

    fn handle_events_subscribe(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        self.collect_events();
        let params: EventSubscribeParams = request.parse_params()?;
        let cursor = if let Some(after_event_id) = params.after_event_id.as_deref() {
            self.validate_event_cursor(after_event_id)?;
            after_event_id.to_string()
        } else {
            self.current_event_cursor()
        };

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &EventSubscribeResult {
                subscribed: true,
                cursor,
                dropped_count: self.dropped_event_count,
            },
        ))
    }

    fn handle_agent_set_state(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        self.collect_events();
        let params: AgentSetStateParams = request.parse_params()?;
        let state = parse_agent_state(&params.state)?;
        let session_id = SessionId::from_string(params.session_id);
        let result = self.apply_agent_state_transition(
            session_id,
            state,
            params.reason,
            "control_api",
            params.telemetry,
        )?;

        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn handle_agent_get_state(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        self.collect_events();
        let params: SessionIdParams = request.parse_params()?;
        let session_id = SessionId::from_string(params.session_id);
        let summary = self
            .runtime
            .session_summary(&session_id)
            .ok_or_else(|| ControlError::new(ErrorCode::SessionNotFound, "Session not found."))?;
        let result = self
            .agent_states
            .get(&session_id.to_string())
            .map(agent_state_result)
            .unwrap_or_else(|| AgentStateResult {
                session_id: session_id.to_string(),
                workspace_id: summary.workspace_id.to_string(),
                state: agent_state_label(AgentState::Unknown).to_string(),
                attention: false,
                reason: None,
                updated_at: None,
                telemetry: None,
            });

        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn handle_agent_list_attention(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        self.collect_events();
        let params: AgentListAttentionParams = request.parse_params()?;
        let workspace_id = params.workspace_id.as_deref();
        let sessions = self
            .agent_states
            .values()
            .filter(|record| record.attention)
            .filter(|record| match workspace_id {
                Some(workspace_id) => record.workspace_id.as_str() == workspace_id,
                None => true,
            })
            .map(agent_state_result)
            .collect();

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AgentAttentionListResult { sessions },
        ))
    }

    fn handle_agent_clear_attention(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        self.collect_events();
        let params: SessionIdParams = request.parse_params()?;
        let session_id = SessionId::from_string(params.session_id);
        self.runtime
            .session_summary(&session_id)
            .ok_or_else(|| ControlError::new(ErrorCode::SessionNotFound, "Session not found."))?;
        if let Some(record) = self.agent_states.get_mut(&session_id.to_string()) {
            record.attention = false;
            record.updated_at = event_timestamp();
        }

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_notification_list(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        self.collect_events();
        let params: NotificationListParams = request.parse_params()?;
        let include_dismissed = params.include_dismissed.unwrap_or(false);
        let notifications = self
            .notifications
            .iter()
            .rev()
            .filter(|notification| include_dismissed || !notification.dismissed)
            .filter(|notification| match params.workspace_id.as_deref() {
                Some(workspace_id) => notification
                    .workspace_id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == workspace_id),
                None => true,
            })
            .filter(|notification| match params.severity.as_deref() {
                Some(severity) => notification.severity == severity,
                None => true,
            })
            .map(notification_summary_result)
            .collect();

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &NotificationListResult { notifications },
        ))
    }

    fn handle_notification_dismiss(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        self.collect_events();
        let params: NotificationDismissParams = request.parse_params()?;
        let Some(notification) = self
            .notifications
            .iter_mut()
            .find(|notification| notification.notification_id == params.notification_id)
        else {
            return Err(ControlError::new(
                ErrorCode::InvalidRequest,
                "Notification not found.",
            ));
        };
        notification.dismissed = true;

        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn apply_agent_state_transition(
        &mut self,
        session_id: SessionId,
        state: AgentState,
        reason: Option<String>,
        source: &str,
        telemetry: Option<AgentTelemetry>,
    ) -> Result<AgentStateResult, ControlError> {
        let summary = self
            .runtime
            .session_summary(&session_id)
            .ok_or_else(|| ControlError::new(ErrorCode::SessionNotFound, "Session not found."))?;
        let attention = agent_state_requires_attention(state);
        // Carry forward the last known telemetry when a transition omits it, so a
        // bare state change (e.g. waiting-for-input) does not erase the metrics.
        let telemetry = telemetry.or_else(|| {
            self.agent_states
                .get(&session_id.to_string())
                .and_then(|previous| previous.telemetry.clone())
        });
        let record = AgentStateRecord {
            session_id: session_id.clone(),
            workspace_id: summary.workspace_id.clone(),
            state,
            attention,
            reason,
            updated_at: event_timestamp(),
            telemetry,
        };
        let duplicate = self
            .agent_states
            .get(&session_id.to_string())
            .is_some_and(|previous| {
                previous.state == record.state
                    && previous.reason == record.reason
                    && previous.attention == record.attention
                    && previous.telemetry == record.telemetry
            });
        self.agent_states
            .insert(session_id.to_string(), record.clone());

        if !duplicate {
            self.push_core_event(CoreEvent::AgentStateChanged {
                session_id: session_id.clone(),
                state,
                reason: record.reason.clone(),
                source: source.to_string(),
                telemetry: record.telemetry.clone(),
            });
            if let Some(notification) = self.notification_for_agent_state(&record) {
                self.push_notification(notification);
            }
        }

        Ok(agent_state_result(&record))
    }

    fn agent_lifecycle_transition(
        &self,
        session_id: &SessionId,
        session_state: &SessionState,
    ) -> Option<(SessionId, AgentState, String, &'static str)> {
        let record = self.agent_states.get(&session_id.to_string())?;
        let activity = agent_state_record_activity(record)?;
        if !matches!(activity, "agent" | "agent_team") {
            return None;
        }
        let subject = if activity == "agent_team" {
            "Agent team worker"
        } else {
            "Agent"
        };
        let source = if activity == "agent_team" {
            "agent_team_lifecycle"
        } else {
            "agent_lifecycle"
        };

        let (state, reason) = match session_state {
            SessionState::Exited { code } => match code {
                Some(code) if *code != 0 => (
                    AgentState::Failed,
                    format!("{subject} exited with code {code}."),
                ),
                Some(_) => (
                    AgentState::Completed,
                    format!("{subject} exited successfully."),
                ),
                None => (AgentState::Completed, format!("{subject} exited.")),
            },
            SessionState::Failed { code, message } => (
                AgentState::Failed,
                format!("{subject} failed ({code}): {message}"),
            ),
            SessionState::Lost => (AgentState::Failed, format!("{subject} session was lost.")),
            _ => return None,
        };
        if record.state == state {
            return None;
        }
        if record.state == AgentState::Failed && state == AgentState::Completed {
            return None;
        }

        Some((session_id.clone(), state, reason, source))
    }

    fn push_event(&mut self, event: EventFrame) {
        if self.event_history.len() >= self.event_backlog_limit {
            self.event_history.remove(0);
            self.dropped_event_count += 1;
        }
        if self.events.len() >= self.event_backlog_limit {
            self.events.remove(0);
        }
        self.events.push(event.clone());
        self.event_history.push(event);
    }

    fn push_core_event(&mut self, event: CoreEvent) {
        if let Some(frame) = self.frame_from_core_event(event) {
            self.push_event(frame);
        }
    }

    fn push_notification(&mut self, notification: NotificationRecord) {
        if self.notifications.len() >= self.notification_limit {
            self.notifications.remove(0);
        }
        self.notifications.push(notification.clone());
        self.push_core_event(CoreEvent::NotificationCreated { notification });
    }

    fn notification_for_agent_state(
        &mut self,
        record: &AgentStateRecord,
    ) -> Option<NotificationRecord> {
        let (notification_type, severity, title, message) = match record.state {
            AgentState::WaitingForInput => (
                "agent.needs_input",
                "warning",
                "Agent needs input",
                record
                    .reason
                    .clone()
                    .unwrap_or_else(|| "Agent is waiting for user input.".to_string()),
            ),
            AgentState::Completed => (
                "agent.completed",
                "info",
                "Agent completed",
                record
                    .reason
                    .clone()
                    .unwrap_or_else(|| "Agent finished successfully.".to_string()),
            ),
            AgentState::Failed => (
                "agent.failed",
                "error",
                "Agent failed",
                record
                    .reason
                    .clone()
                    .unwrap_or_else(|| "Agent reported a failure.".to_string()),
            ),
            _ => return None,
        };
        let notification_id = next_timestamped_id("not", self.next_notification_id);
        self.next_notification_id += 1;
        Some(NotificationRecord {
            notification_id,
            notification_type: notification_type.to_string(),
            severity: severity.to_string(),
            workspace_id: Some(record.workspace_id.clone()),
            session_id: Some(record.session_id.clone()),
            title: title.to_string(),
            message,
            created_at: event_timestamp(),
            dismissed: false,
        })
    }

    fn frame_from_core_event(&mut self, event: CoreEvent) -> Option<EventFrame> {
        let event_id = format!("evt_{:08}", self.next_event_id);
        self.next_event_id += 1;

        let mut frame = match event {
            CoreEvent::WorkspaceChanged { workspace_id } => {
                let mut frame = EventFrame::new(event_id, "workspace.changed");
                frame.workspace_id = Some(workspace_id.to_string());
                frame.data_json = "{}".to_string();
                frame
            }
            CoreEvent::PaneChanged {
                workspace_id,
                pane_id,
            } => {
                let mut frame = EventFrame::new(event_id, "pane.changed");
                frame.workspace_id = Some(workspace_id.to_string());
                frame.data_json = serde_json::json!({
                    "pane_id": pane_id.to_string(),
                })
                .to_string();
                frame
            }
            CoreEvent::SurfaceMounted {
                pane_id,
                surface_id,
            } => {
                let mut frame = EventFrame::new(event_id, "surface.mounted");
                frame.data_json = serde_json::json!({
                    "pane_id": pane_id.to_string(),
                    "surface_id": surface_id.to_string(),
                })
                .to_string();
                frame
            }
            CoreEvent::SessionStateChanged {
                session_id,
                from,
                to,
            } => {
                let mut frame = EventFrame::new(event_id, "session.state_changed");
                frame.workspace_id = self
                    .runtime
                    .session_summary(&session_id)
                    .map(|summary| summary.workspace_id.to_string());
                frame.session_id = Some(session_id.to_string());
                frame.data_json = serde_json::json!({
                    "from": session_state_label(&from),
                    "to": session_state_label(&to),
                    "exit_code": session_exit_code(&to),
                })
                .to_string();
                frame
            }
            CoreEvent::SessionOutputBatch {
                session_id, bytes, ..
            } => {
                let mut frame = EventFrame::new(event_id, "session.output");
                frame.workspace_id = self
                    .runtime
                    .session_summary(&session_id)
                    .map(|summary| summary.workspace_id.to_string());
                frame.session_id = Some(session_id.to_string());
                frame.data_json = serde_json::json!({
                    "byte_count": bytes.len(),
                })
                .to_string();
                frame
            }
            CoreEvent::AgentStateChanged {
                session_id,
                state,
                reason,
                source,
                telemetry,
            } => {
                let mut frame = EventFrame::new(event_id, "agent.state_changed");
                frame.workspace_id = self
                    .runtime
                    .session_summary(&session_id)
                    .map(|summary| summary.workspace_id.to_string());
                frame.session_id = Some(session_id.to_string());
                frame.data_json = serde_json::json!({
                    "state": agent_state_label(state),
                    "reason": reason,
                    "source": source,
                    "telemetry": telemetry,
                })
                .to_string();
                frame
            }
            CoreEvent::SessionCwdChanged { session_id, cwd } => {
                let mut frame = EventFrame::new(event_id, "session.cwd_changed");
                frame.workspace_id = self
                    .runtime
                    .session_summary(&session_id)
                    .map(|summary| summary.workspace_id.to_string());
                frame.session_id = Some(session_id.to_string());
                frame.data_json = serde_json::json!({ "cwd": cwd }).to_string();
                frame
            }
            CoreEvent::NotificationCreated { notification } => {
                let mut frame = EventFrame::new(event_id, "notification.created");
                frame.workspace_id = notification.workspace_id.as_ref().map(ToString::to_string);
                frame.session_id = notification.session_id.as_ref().map(ToString::to_string);
                frame.data_json = serde_json::json!({
                    "notification_id": notification.notification_id,
                    "notification_type": notification.notification_type,
                    "severity": notification.severity,
                    "title": notification.title,
                    "message": notification.message,
                })
                .to_string();
                frame
            }
            CoreEvent::BackendHealthChanged {
                attachment_id,
                state,
            } => {
                let mut frame = EventFrame::new(event_id, "backend.health_changed");
                frame.data_json = serde_json::json!({
                    "attachment_id": attachment_id.to_string(),
                    "state": state,
                })
                .to_string();
                frame
            }
        };
        frame.occurred_at = event_timestamp();
        Some(frame)
    }
}

fn event_matches_poll(event: &EventFrame, params: &EventPollParams) -> bool {
    event_matches_filters(
        event,
        params.workspace_id.as_deref(),
        params.session_id.as_deref(),
        params.types.as_deref(),
    )
}

fn event_matches_subscription(event: &EventFrame, params: &EventSubscribeParams) -> bool {
    event_matches_filters(
        event,
        params.workspace_id.as_deref(),
        params.session_id.as_deref(),
        params.types.as_deref(),
    )
}

fn event_matches_filters(
    event: &EventFrame,
    workspace_id: Option<&str>,
    session_id: Option<&str>,
    types: Option<&[String]>,
) -> bool {
    if workspace_id.is_some_and(|workspace_id| event.workspace_id.as_deref() != Some(workspace_id))
    {
        return false;
    }

    if session_id.is_some_and(|session_id| event.session_id.as_deref() != Some(session_id)) {
        return false;
    }

    if types.is_some_and(|types| {
        !types
            .iter()
            .any(|event_type| event_type == &event.event_type)
    }) {
        return false;
    }

    true
}

fn event_id_value(event_id: &str) -> Option<u64> {
    event_id.strip_prefix("evt_")?.parse().ok()
}

fn parse_event_id(event_id: &str) -> Result<u64, ControlError> {
    event_id_value(event_id).ok_or_else(|| {
        ControlError::new(
            ErrorCode::InvalidRequest,
            format!("Invalid event cursor '{event_id}'."),
        )
    })
}

fn agent_state_result(record: &AgentStateRecord) -> AgentStateResult {
    AgentStateResult {
        session_id: record.session_id.to_string(),
        workspace_id: record.workspace_id.to_string(),
        state: agent_state_label(record.state).to_string(),
        attention: record.attention,
        reason: record.reason.clone(),
        updated_at: Some(record.updated_at.clone()),
        telemetry: record.telemetry.clone(),
    }
}

fn notification_summary_result(notification: &NotificationRecord) -> NotificationSummaryResult {
    NotificationSummaryResult {
        notification_id: notification.notification_id.clone(),
        notification_type: notification.notification_type.clone(),
        severity: notification.severity.clone(),
        workspace_id: notification.workspace_id.as_ref().map(ToString::to_string),
        session_id: notification.session_id.as_ref().map(ToString::to_string),
        title: notification.title.clone(),
        message: notification.message.clone(),
        created_at: notification.created_at.clone(),
        dismissed: notification.dismissed,
    }
}

fn parse_agent_state(value: &str) -> Result<AgentState, ControlError> {
    match value {
        "unknown" => Ok(AgentState::Unknown),
        "agent.started" | "started" => Ok(AgentState::Running),
        "agent.running" => Ok(AgentState::Running),
        "running" => Ok(AgentState::Running),
        "idle" => Ok(AgentState::Idle),
        "agent.awaiting_input" | "awaiting_input" | "waiting_for_input" | "needs_input" => {
            Ok(AgentState::WaitingForInput)
        }
        "agent.completed" => Ok(AgentState::Completed),
        "completed" => Ok(AgentState::Completed),
        "agent.failed" => Ok(AgentState::Failed),
        "failed" => Ok(AgentState::Failed),
        "agent.exited" => Ok(AgentState::Detached),
        "detached" => Ok(AgentState::Detached),
        _ => Err(ControlError::new(
            ErrorCode::InvalidRequest,
            format!("Unsupported agent state '{value}'."),
        )),
    }
}

fn agent_state_label(state: AgentState) -> &'static str {
    match state {
        AgentState::Unknown => "unknown",
        AgentState::Running => "running",
        AgentState::Idle => "idle",
        AgentState::WaitingForInput => "waiting_for_input",
        AgentState::Completed => "completed",
        AgentState::Failed => "failed",
        AgentState::Detached => "detached",
    }
}

fn agent_state_requires_attention(state: AgentState) -> bool {
    matches!(state, AgentState::WaitingForInput | AgentState::Failed)
}

fn agent_state_record_activity(record: &AgentStateRecord) -> Option<&str> {
    record
        .telemetry
        .as_ref()
        .and_then(|telemetry| telemetry.activity.as_deref())
}

fn detect_agent_signals(bytes: &[u8]) -> Vec<DetectedAgentSignal> {
    detect_agent_signals_from_input(AgentSignalDetectorInput::explicit_only(bytes))
}

fn detect_agent_signals_from_input(
    input: AgentSignalDetectorInput<'_>,
) -> Vec<DetectedAgentSignal> {
    let mut decoded: Option<Cow<'_, str>> = None;
    let mut signals = Vec::new();

    if contains_explicit_agent_signal_marker(input.bytes) {
        let text = decoded.get_or_insert_with(|| String::from_utf8_lossy(input.bytes));
        signals.extend(detect_explicit_agent_signals(text.as_ref()));
    }

    if signals.is_empty()
        && input.heuristic.is_some_and(|heuristic| heuristic.enabled)
        && contains_heuristic_agent_signal_marker(input.bytes)
    {
        let text = decoded.get_or_insert_with(|| String::from_utf8_lossy(input.bytes));
        signals.extend(detect_heuristic_output_signals(text.as_ref()));
    }
    signals
}

fn detect_heuristic_agent_signals(bytes: &[u8]) -> Vec<DetectedAgentSignal> {
    if !contains_heuristic_agent_signal_marker(bytes) {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(bytes);
    detect_heuristic_output_signals(&text)
}

fn contains_explicit_agent_signal_marker(bytes: &[u8]) -> bool {
    bytes.contains(&b':') || bytes.contains(&0x1b)
}

fn contains_heuristic_agent_signal_marker(bytes: &[u8]) -> bool {
    contains_ascii_case_insensitive(bytes, b"input")
        || contains_ascii_case_insensitive(bytes, b"approval")
}

fn contains_ascii_case_insensitive(bytes: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && bytes.windows(needle.len()).any(|window| {
            window.iter().zip(needle.iter()).all(|(actual, expected)| {
                actual.to_ascii_lowercase() == expected.to_ascii_lowercase()
            })
        })
}

fn detect_explicit_agent_signals(text: &str) -> Vec<DetectedAgentSignal> {
    let mut signals = Vec::new();
    signals.extend(detect_shell_marker_signals(text));
    signals.extend(detect_osc_agentmux_signals(text));
    signals
}

fn detect_shell_marker_signals(text: &str) -> Vec<DetectedAgentSignal> {
    const PREFIX: &str = "::agentmux-agent";

    text.lines()
        .filter_map(|line| {
            let payload = line
                .find(PREFIX)
                .map(|index| line[index + PREFIX.len()..].trim())?;
            parse_agent_signal_payload(payload, "shell_marker")
        })
        .collect()
}

fn detect_osc_agentmux_signals(text: &str) -> Vec<DetectedAgentSignal> {
    const PREFIX: &str = "\u{1b}]777;agentmux;";

    let mut rest = text;
    let mut signals = Vec::new();
    while let Some(start) = rest.find(PREFIX) {
        let payload_start = start + PREFIX.len();
        let payload_and_after = &rest[payload_start..];
        let terminator = first_osc_terminator(payload_and_after);
        let Some((payload_end, terminator_len)) = terminator else {
            break;
        };
        if let Some(signal) =
            parse_agent_signal_payload(&payload_and_after[..payload_end], "osc_777")
        {
            signals.push(signal);
        }
        rest = &payload_and_after[payload_end + terminator_len..];
    }
    signals
}

fn detect_heuristic_output_signals(text: &str) -> Vec<DetectedAgentSignal> {
    text.lines()
        .filter_map(|line| {
            let reason = line.trim();
            if reason.is_empty() {
                return None;
            }
            let normalized = reason.to_ascii_lowercase();
            if !looks_like_waiting_for_input(&normalized) {
                return None;
            }
            Some(DetectedAgentSignal {
                state: AgentState::WaitingForInput,
                reason: Some(reason.to_string()),
                source: "heuristic_output",
            })
        })
        .take(1)
        .collect()
}

fn looks_like_waiting_for_input(normalized: &str) -> bool {
    const WAITING_INPUT_PATTERNS: &[&str] = &[
        "waiting for input",
        "waiting for approval",
        "approval required",
        "requires approval",
        "needs input",
        "requires input",
        "user input required",
    ];
    const NEGATED_PATTERNS: &[&str] = &[
        "no input required",
        "not waiting for input",
        "does not require input",
        "doesn't require input",
    ];

    WAITING_INPUT_PATTERNS
        .iter()
        .any(|pattern| normalized.contains(pattern))
        && !NEGATED_PATTERNS
            .iter()
            .any(|pattern| normalized.contains(pattern))
}

fn first_osc_terminator(value: &str) -> Option<(usize, usize)> {
    let bel = value.find('\u{0007}').map(|index| (index, 1));
    let st = value.find("\u{1b}\\").map(|index| (index, 2));
    match (bel, st) {
        (Some(bel), Some(st)) => Some(if bel.0 <= st.0 { bel } else { st }),
        (Some(bel), None) => Some(bel),
        (None, Some(st)) => Some(st),
        (None, None) => None,
    }
}

fn parse_agent_signal_payload(payload: &str, source: &'static str) -> Option<DetectedAgentSignal> {
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    let state = value
        .get("state")
        .or_else(|| value.get("event"))
        .and_then(serde_json::Value::as_str)?;
    let state = parse_agent_state(state).ok()?;
    let reason = value
        .get("reason")
        .or_else(|| value.get("message"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);

    Some(DetectedAgentSignal {
        state,
        reason,
        source,
    })
}

fn session_exit_code(state: &SessionState) -> Option<i32> {
    match state {
        SessionState::Exited { code } => *code,
        _ => None,
    }
}

fn event_timestamp() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}Z", duration.as_secs(), duration.subsec_millis())
}

/// Parse the working directory from the LAST OSC 7 sequence in `bytes`, if any.
/// OSC 7 is `ESC ] 7 ; file://<host><abs-path> ST`, where ST is BEL (0x07) or
/// `ESC \`. Returns the percent-decoded absolute path (host component stripped).
/// Taking the last occurrence means the most recent `cd` in a batch wins.
fn parse_osc7_cwd(bytes: &[u8]) -> Option<String> {
    const PREFIX: &[u8] = b"\x1b]7;";
    let start = bytes.windows(PREFIX.len()).rposition(|w| w == PREFIX)? + PREFIX.len();
    let rest = &bytes[start..];
    let end = rest
        .iter()
        .position(|&b| b == 0x07)
        .or_else(|| rest.windows(2).position(|w| w == [0x1b, 0x5c]))?;
    let text = std::str::from_utf8(&rest[..end]).ok()?;
    let after_scheme = text.strip_prefix("file://").unwrap_or(text);
    // Drop the host component (everything up to the first '/' of the path).
    let path_start = after_scheme.find('/')?;
    let decoded = percent_decode(&after_scheme[path_start..]);
    (!decoded.is_empty()).then_some(decoded)
}

/// Minimal percent-decoder for OSC 7 file-URI paths (e.g. `%20` → space). Leaves
/// malformed escapes untouched.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn append_recent_output(
    recent_output: &mut HashMap<SessionId, RecentOutputBuffer>,
    session_id: &SessionId,
    bytes: &[u8],
    limit: usize,
) {
    if limit == 0 {
        return;
    }

    let buffer = recent_output.entry(session_id.clone()).or_default();
    buffer.append_limited(bytes, limit);
}

fn parse_durability(value: Option<&str>) -> Result<Durability, ControlError> {
    match value.unwrap_or("ephemeral") {
        "ephemeral" => Ok(Durability::Ephemeral),
        "durable" => Ok(Durability::Durable),
        other => Err(ControlError::new(
            ErrorCode::InvalidRequest,
            format!("Unknown durability '{other}'."),
        )),
    }
}

fn parse_backend_kind(value: Option<&str>) -> Result<Option<BackendKind>, ControlError> {
    match value {
        None | Some("conpty") => Ok(value.map(|_| BackendKind::Conpty)),
        Some("wsl-direct") => Ok(Some(BackendKind::WslDirect)),
        Some("wsl-tmux-control") => Ok(Some(BackendKind::WslTmuxControl)),
        Some("ssh") => Ok(Some(BackendKind::Ssh)),
        Some(other) => Err(ControlError::new(
            ErrorCode::InvalidRequest,
            format!("Unknown backend '{other}'."),
        )),
    }
}

fn parse_named_key(value: &str) -> Result<NamedKey, ControlError> {
    match value {
        "enter" => Ok(NamedKey::Enter),
        "backspace" => Ok(NamedKey::Backspace),
        "tab" => Ok(NamedKey::Tab),
        "escape" => Ok(NamedKey::Escape),
        "arrow_up" => Ok(NamedKey::ArrowUp),
        "arrow_down" => Ok(NamedKey::ArrowDown),
        "arrow_left" => Ok(NamedKey::ArrowLeft),
        "arrow_right" => Ok(NamedKey::ArrowRight),
        value if value.starts_with('f') => value[1..]
            .parse::<u8>()
            .map(NamedKey::Function)
            .map_err(|_| invalid_key(value)),
        other => Err(invalid_key(other)),
    }
}

fn invalid_key(value: &str) -> ControlError {
    ControlError::new(
        ErrorCode::InvalidRequest,
        format!("Unsupported key '{value}'."),
    )
}

fn parse_termination_mode(value: &str) -> Result<TerminationMode, ControlError> {
    match value {
        "soft" => Ok(TerminationMode::Soft),
        "interrupt" => Ok(TerminationMode::Interrupt),
        "kill" => Ok(TerminationMode::Kill),
        other => Err(ControlError::new(
            ErrorCode::InvalidRequest,
            format!("Unsupported termination mode '{other}'."),
        )),
    }
}

fn control_error_from_backend(error: BackendError) -> ControlError {
    let code = match error.code.as_str() {
        "backend_unavailable" => ErrorCode::BackendUnavailable,
        "backend_degraded" => ErrorCode::BackendDegraded,
        "wsl_unavailable" | "no_wsl_distributions" | "wsl_distribution_not_found" => {
            ErrorCode::BackendUnavailable
        }
        "invalid_wsl_cwd" => ErrorCode::InvalidRequest,
        "wsl_launch_timeout" => ErrorCode::Timeout,
        "session_not_found" => ErrorCode::SessionNotFound,
        "spawn_failed" => ErrorCode::SpawnFailed,
        "attach_failed" => ErrorCode::AttachFailed,
        "tmux_control_failed" | "tmux_control_error" | "tmux_control_parse_error" => {
            ErrorCode::AttachFailed
        }
        "tmux_control_timeout" => ErrorCode::Timeout,
        "ssh_connect_failed" | "ssh_identity_missing" => ErrorCode::BackendUnavailable,
        "ssh_auth_failed" => ErrorCode::PermissionDenied,
        "ssh_identity_invalid" => ErrorCode::InvalidRequest,
        "ssh_protocol_error" => ErrorCode::BackendDegraded,
        "unsupported_backend_operation" => ErrorCode::InvalidRequest,
        "timeout" => ErrorCode::Timeout,
        "permission_denied" => ErrorCode::PermissionDenied,
        "invalid_request" => ErrorCode::InvalidRequest,
        _ => ErrorCode::InvalidRequest,
    };

    ControlError::new(code, error.message)
        .with_details(format!(r#"{{"backend_code":"{}"}}"#, error.code))
}

fn session_summary_result(summary: RuntimeSessionSummary) -> SessionSummaryResult {
    SessionSummaryResult {
        session_id: summary.session_id.to_string(),
        workspace_id: summary.workspace_id.to_string(),
        backend_kind: backend_kind_label(summary.backend_kind).to_string(),
        state: session_state_label(&summary.state),
        exit_code: summary.exit_code,
        backend_native_id: summary.backend_native_id,
    }
}

fn backend_kind_label(kind: BackendKind) -> &'static str {
    match kind {
        BackendKind::Conpty => "conpty",
        BackendKind::WslDirect => "wsl-direct",
        BackendKind::WslTmuxControl => "wsl-tmux-control",
        BackendKind::Ssh => "ssh",
    }
}

fn session_state_label(state: &SessionState) -> String {
    match state {
        SessionState::Starting => "starting".to_string(),
        SessionState::Running => "running".to_string(),
        SessionState::Detached => "detached".to_string(),
        SessionState::Recovering => "recovering".to_string(),
        SessionState::Disconnected => "disconnected".to_string(),
        SessionState::Exited { .. } => "exited".to_string(),
        SessionState::Failed { .. } => "failed".to_string(),
        SessionState::Lost => "lost".to_string(),
    }
}

fn backend_kind_from_backend(kind: BackendTraitKind) -> BackendKind {
    match kind {
        BackendTraitKind::Conpty => BackendKind::Conpty,
        BackendTraitKind::WslDirect => BackendKind::WslDirect,
        BackendTraitKind::WslTmuxControl => BackendKind::WslTmuxControl,
        BackendTraitKind::Ssh => BackendKind::Ssh,
    }
}

fn backend_kind_to_backend(kind: BackendKind) -> BackendTraitKind {
    match kind {
        BackendKind::Conpty => BackendTraitKind::Conpty,
        BackendKind::WslDirect => BackendTraitKind::WslDirect,
        BackendKind::WslTmuxControl => BackendTraitKind::WslTmuxControl,
        BackendKind::Ssh => BackendTraitKind::Ssh,
    }
}

fn command_spec_to_vec(command: CommandSpec) -> Vec<String> {
    std::iter::once(command.executable)
        .chain(command.args)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentmux_backend::{
        AttachRequest, BackendError, BackendKind as BackendTraitKind, SessionHandle,
    };
    use agentmux_ipc::ResponseOutcome;

    #[test]
    fn parse_osc7_cwd_extracts_bel_terminated_path() {
        let bytes = b"prompt\x1b]7;file://wsl-host/mnt/d/workspace/agentmux\x07$ ";
        assert_eq!(
            parse_osc7_cwd(bytes),
            Some("/mnt/d/workspace/agentmux".to_string())
        );
    }

    #[test]
    fn parse_osc7_cwd_handles_st_terminator_and_percent_decoding() {
        let bytes = b"\x1b]7;file://host/home/irae/my%20project\x1b\\";
        assert_eq!(
            parse_osc7_cwd(bytes),
            Some("/home/irae/my project".to_string())
        );
    }

    #[test]
    fn parse_osc7_cwd_takes_last_occurrence_and_ignores_non_osc7() {
        assert_eq!(parse_osc7_cwd(b"just plain output, no escape"), None);
        let bytes = b"\x1b]7;file://h/a\x07middle\x1b]7;file://h/b\x07";
        assert_eq!(parse_osc7_cwd(bytes), Some("/b".to_string()));
    }

    #[test]
    fn generated_ids_have_stable_prefixes() {
        assert!(WorkspaceId::new().as_str().starts_with("ws_"));
        assert!(PaneId::new().as_str().starts_with("pane_"));
        assert!(SessionId::new().as_str().starts_with("ses_"));
    }

    #[test]
    fn running_session_can_detach_and_exit() {
        let running = SessionState::Running;
        assert!(running.can_transition_to(&SessionState::Detached));
        assert!(running.can_transition_to(&SessionState::Exited { code: Some(0) }));
    }

    #[test]
    fn terminal_states_reject_further_transitions() {
        let exited = SessionState::Exited { code: Some(0) };
        let failed = SessionState::Failed {
            code: "spawn_failed".to_string(),
            message: "spawn failed".to_string(),
        };

        assert!(exited.is_terminal());
        assert!(failed.is_terminal());
        assert!(!exited.can_transition_to(&SessionState::Running));
        assert!(!failed.can_transition_to(&SessionState::Recovering));
    }

    #[test]
    fn control_error_maps_wsl_diagnostics_without_losing_backend_code() {
        let timeout =
            control_error_from_backend(BackendError::new("wsl_launch_timeout", "WSL timed out"));
        assert_eq!(timeout.code, ErrorCode::Timeout);
        assert_eq!(
            timeout.details_json.as_deref(),
            Some(r#"{"backend_code":"wsl_launch_timeout"}"#)
        );

        let invalid_cwd =
            control_error_from_backend(BackendError::new("invalid_wsl_cwd", "bad cwd"));
        assert_eq!(invalid_cwd.code, ErrorCode::InvalidRequest);
        assert_eq!(
            invalid_cwd.details_json.as_deref(),
            Some(r#"{"backend_code":"invalid_wsl_cwd"}"#)
        );
    }

    #[test]
    fn runtime_spawns_and_maps_backend_events() {
        let workspace_id = WorkspaceId::new();
        let backend = FakeBackend::default();
        let mut runtime = TerminalRuntime::new(backend);

        let session_id = runtime
            .spawn_session(SessionSpawnSpec {
                workspace_id: workspace_id.clone(),
                backend: None,
                backend_profile: None,
                command: CommandSpec::with_args(
                    "cmd.exe",
                    vec!["/c".to_string(), "echo agentmux".to_string()],
                ),
                cwd: None,
                env: Vec::new(),
                initial_size: TerminalSize::new(80, 24),
                durability: Durability::Ephemeral,
            })
            .unwrap();

        let events = runtime.drain_events();
        assert!(matches!(
            events.first(),
            Some(CoreEvent::SessionStateChanged {
                to: SessionState::Running,
                ..
            })
        ));

        let summary = runtime.session_summary(&session_id).unwrap();
        assert_eq!(summary.workspace_id, workspace_id);
        assert_eq!(summary.state, SessionState::Running);
    }

    #[test]
    fn control_plane_dispatches_spawn_send_and_read_recent() {
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");

        let spawn = control.handle_request(request(
            "req_spawn",
            "session.spawn",
            r#"{"workspace_id":"ws_test","command":["cmd.exe","/c","echo agentmux"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral"}"#,
        ));
        let session_id = ok_json(&spawn)
            .split("\"session_id\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("session id in spawn result")
            .to_string();

        let send = control.handle_request(request(
            "req_send",
            "session.send_text",
            &format!(r#"{{"session_id":"{session_id}","text":"hello"}}"#),
        ));
        assert!(ok_json(&send).contains("\"ok\":true"));

        let recent = control.handle_request(request(
            "req_recent",
            "session.read_recent",
            &format!(r#"{{"session_id":"{session_id}","max_bytes":128}}"#),
        ));
        let recent_json = ok_json(&recent);
        assert!(recent_json.contains("\"text\":\"ok\""));
    }

    #[test]
    fn session_snapshot_skips_base64_when_no_new_output() {
        // PR-6: a steady (no new output) delta poll must not perform any Base64
        // encoding. After consuming all output, a snapshot at `end_offset`
        // returns an empty `bytes_base64` and `base_offset == end_offset`.
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");

        let spawn = control.handle_request(request(
            "req_spawn_snap",
            "session.spawn",
            r#"{"workspace_id":"ws_snap","command":["cmd.exe"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral"}"#,
        ));
        let session_id = ok_json(&spawn)
            .split("\"session_id\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("session id in spawn result")
            .to_string();

        // Produce two bytes ("ok") of output via the fake backend.
        let send = control.handle_request(request(
            "req_send_snap",
            "session.send_text",
            &format!(r#"{{"session_id":"{session_id}","text":"hello"}}"#),
        ));
        assert!(ok_json(&send).contains("\"ok\":true"));

        // Cold-start snapshot: consumes all output, non-empty payload.
        let first = control.handle_request(request(
            "req_snap_cold",
            "session.snapshot",
            &format!(r#"{{"session_id":"{session_id}","since_offset":null}}"#),
        ));
        let first: SessionSnapshotResult = serde_json::from_str(ok_json(&first)).unwrap();
        assert_eq!(first.base_offset, 0);
        assert_eq!(first.end_offset, 2);
        assert!(!first.bytes_base64.is_empty());

        // Delta snapshot at the consumed offset: no new output, so the encode is
        // skipped and the payload is empty (wire-compatible with "no bytes").
        let steady = control.handle_request(request(
            "req_snap_steady",
            "session.snapshot",
            &format!(
                r#"{{"session_id":"{session_id}","since_offset":{}}}"#,
                first.end_offset
            ),
        ));
        let steady: SessionSnapshotResult = serde_json::from_str(ok_json(&steady)).unwrap();
        assert_eq!(steady.base_offset, first.end_offset);
        assert_eq!(steady.end_offset, first.end_offset);
        assert!(steady.bytes_base64.is_empty());
    }

    #[test]
    fn control_plane_lists_sessions_and_polls_events() {
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");

        let spawn = control.handle_request(request(
            "req_spawn_events",
            "session.spawn",
            r#"{"workspace_id":"ws_events","command":["cmd.exe"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral"}"#,
        ));
        let session_id = ok_json(&spawn)
            .split("\"session_id\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("session id in spawn result")
            .to_string();

        let list = control.handle_request(request(
            "req_session_list",
            "session.list",
            r#"{"workspace_id":"ws_events"}"#,
        ));
        let list: SessionListResult = serde_json::from_str(ok_json(&list)).unwrap();
        assert_eq!(list.sessions.len(), 1);
        assert_eq!(list.sessions[0].session_id, session_id);
        assert_eq!(list.sessions[0].state, "running");

        let send = control.handle_request(request(
            "req_send_for_event",
            "session.send_text",
            &format!(r#"{{"session_id":"{session_id}","text":"hello"}}"#),
        ));
        assert!(ok_json(&send).contains("\"ok\":true"));

        let events = control.handle_request(request(
            "req_events_poll",
            "events.poll",
            r#"{"workspace_id":"ws_events","types":["session.state_changed","session.output"],"max_events":10}"#,
        ));
        let events: EventPollResult = serde_json::from_str(ok_json(&events)).unwrap();
        assert_eq!(events.dropped_count, 0);
        assert!(events
            .events
            .iter()
            .any(|event| event.event_type == "session.state_changed"
                && event.session_id.as_deref() == Some(&session_id)));
        assert!(events
            .events
            .iter()
            .any(|event| event.event_type == "session.output"
                && event.session_id.as_deref() == Some(&session_id)
                && event.data_json.contains("\"byte_count\":2")));

        let empty = control.handle_request(request(
            "req_events_poll_empty",
            "events.poll",
            r#"{"workspace_id":"ws_events","max_events":10}"#,
        ));
        let empty: EventPollResult = serde_json::from_str(ok_json(&empty)).unwrap();
        assert!(empty.events.is_empty());

        let subscribe = control.handle_request(request(
            "req_events_subscribe",
            "events.subscribe",
            r#"{"workspace_id":"ws_events","types":["session.state_changed","session.output"],"after_event_id":"evt_00000000"}"#,
        ));
        let subscribe: EventSubscribeResult = serde_json::from_str(ok_json(&subscribe)).unwrap();
        assert_eq!(subscribe.cursor, "evt_00000000");

        let replay_params = EventSubscribeParams {
            workspace_id: Some("ws_events".to_string()),
            session_id: Some(session_id.clone()),
            types: Some(vec!["session.output".to_string()]),
            after_event_id: Some("evt_00000000".to_string()),
        };
        let replay = control
            .event_subscription_batch(&replay_params, "evt_00000000", 10)
            .unwrap();
        assert!(replay
            .iter()
            .any(|event| event.event_type == "session.output"
                && event.session_id.as_deref() == Some(&session_id)));
    }

    #[test]
    fn control_plane_tracks_agent_attention_and_notifications() {
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");

        let spawn = control.handle_request(request(
            "req_spawn_agent",
            "session.spawn",
            r#"{"workspace_id":"ws_agent","command":["cmd.exe"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral"}"#,
        ));
        let session_id = ok_json(&spawn)
            .split("\"session_id\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("session id in spawn result")
            .to_string();

        let state = control.handle_request(request(
            "req_agent_waiting",
            "agent.set_state",
            &format!(
                r#"{{"session_id":"{session_id}","state":"waiting_for_input","reason":"approval needed"}}"#
            ),
        ));
        let state: AgentStateResult = serde_json::from_str(ok_json(&state)).unwrap();
        assert_eq!(state.state, "waiting_for_input");
        assert!(state.attention);

        let attention = control.handle_request(request(
            "req_attention",
            "agent.list_attention",
            r#"{"workspace_id":"ws_agent"}"#,
        ));
        let attention: AgentAttentionListResult =
            serde_json::from_str(ok_json(&attention)).unwrap();
        assert_eq!(attention.sessions.len(), 1);

        let notifications = control.handle_request(request(
            "req_notifications",
            "notification.list",
            r#"{"workspace_id":"ws_agent","severity":"warning","include_dismissed":false}"#,
        ));
        let notifications: NotificationListResult =
            serde_json::from_str(ok_json(&notifications)).unwrap();
        assert_eq!(notifications.notifications.len(), 1);
        assert_eq!(
            notifications.notifications[0].notification_type,
            "agent.needs_input"
        );
        let notification_id = notifications.notifications[0].notification_id.clone();

        let events = control.handle_request(request(
            "req_agent_events",
            "events.poll",
            r#"{"workspace_id":"ws_agent","types":["agent.state_changed","notification.created"],"max_events":10}"#,
        ));
        let events: EventPollResult = serde_json::from_str(ok_json(&events)).unwrap();
        assert!(events
            .events
            .iter()
            .any(|event| event.event_type == "agent.state_changed"
                && event.session_id.as_deref() == Some(&session_id)
                && event.data_json.contains("\"source\":\"control_api\"")));
        assert!(events
            .events
            .iter()
            .any(|event| event.event_type == "notification.created"
                && event.session_id.as_deref() == Some(&session_id)));

        let clear = control.handle_request(request(
            "req_clear_attention",
            "agent.clear_attention",
            &format!(r#"{{"session_id":"{session_id}"}}"#),
        ));
        assert!(ok_json(&clear).contains("\"ok\":true"));

        let attention = control.handle_request(request(
            "req_attention_empty",
            "agent.list_attention",
            r#"{"workspace_id":"ws_agent"}"#,
        ));
        let attention: AgentAttentionListResult =
            serde_json::from_str(ok_json(&attention)).unwrap();
        assert!(attention.sessions.is_empty());

        let dismiss = control.handle_request(request(
            "req_dismiss_notification",
            "notification.dismiss",
            &format!(r#"{{"notification_id":"{notification_id}"}}"#),
        ));
        assert!(ok_json(&dismiss).contains("\"ok\":true"));

        let visible = control.handle_request(request(
            "req_notifications_visible",
            "notification.list",
            r#"{"workspace_id":"ws_agent"}"#,
        ));
        let visible: NotificationListResult = serde_json::from_str(ok_json(&visible)).unwrap();
        assert!(visible.notifications.is_empty());
    }

    #[test]
    fn control_plane_marks_agent_team_session_completed_on_clean_exit() {
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");

        let spawn = control.handle_request(request(
            "req_spawn_agent_team_complete",
            "session.spawn",
            r#"{"workspace_id":"ws_agent_team","command":["cmd.exe"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral"}"#,
        ));
        let session_id = ok_json(&spawn)
            .split("\"session_id\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("session id in spawn result")
            .to_string();

        let state = control.handle_request(request(
            "req_agent_team_running",
            "agent.set_state",
            &format!(
                r#"{{"session_id":"{session_id}","state":"running","reason":"omo split worker","telemetry":{{"activity":"agent_team","session":"omo:split-window","ctx":"pane_worker"}}}}"#
            ),
        ));
        let state: AgentStateResult = serde_json::from_str(ok_json(&state)).unwrap();
        assert_eq!(state.state, "running");

        control.runtime.backend.events.push(BackendEvent::Exited {
            session_id: session_id.clone(),
            code: Some(0),
        });

        let events = control.handle_request(request(
            "req_agent_team_complete_events",
            "events.poll",
            r#"{"workspace_id":"ws_agent_team","types":["session.state_changed","agent.state_changed","notification.created"],"max_events":10}"#,
        ));
        let events: EventPollResult = serde_json::from_str(ok_json(&events)).unwrap();
        assert!(events.events.iter().any(|event| {
            event.event_type == "session.state_changed"
                && event.session_id.as_deref() == Some(&session_id)
                && event.data_json.contains("\"to\":\"exited\"")
        }));
        assert!(events.events.iter().any(|event| {
            event.event_type == "agent.state_changed"
                && event.session_id.as_deref() == Some(&session_id)
                && event.data_json.contains("\"state\":\"completed\"")
                && event
                    .data_json
                    .contains("\"source\":\"agent_team_lifecycle\"")
                && event.data_json.contains("\"activity\":\"agent_team\"")
        }));
        assert!(events.events.iter().any(|event| {
            event.event_type == "notification.created"
                && event.session_id.as_deref() == Some(&session_id)
                && event.data_json.contains("agent.completed")
        }));

        let state = control.handle_request(request(
            "req_agent_team_complete_state",
            "agent.get_state",
            &format!(r#"{{"session_id":"{session_id}"}}"#),
        ));
        let state: AgentStateResult = serde_json::from_str(ok_json(&state)).unwrap();
        assert_eq!(state.state, "completed");
        assert!(!state.attention);
        assert_eq!(
            state
                .telemetry
                .and_then(|telemetry| telemetry.activity)
                .as_deref(),
            Some("agent_team")
        );
    }

    #[test]
    fn control_plane_marks_agent_team_session_failed_on_nonzero_exit() {
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");

        let spawn = control.handle_request(request(
            "req_spawn_agent_team_failed",
            "session.spawn",
            r#"{"workspace_id":"ws_agent_team_failed","command":["cmd.exe"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral"}"#,
        ));
        let session_id = ok_json(&spawn)
            .split("\"session_id\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("session id in spawn result")
            .to_string();

        let _ = control.handle_request(request(
            "req_agent_team_running_failed",
            "agent.set_state",
            &format!(
                r#"{{"session_id":"{session_id}","state":"running","reason":"claude-teams split worker","telemetry":{{"activity":"agent_team","session":"claude-teams:split-window"}}}}"#
            ),
        ));

        control.runtime.backend.events.push(BackendEvent::Exited {
            session_id: session_id.clone(),
            code: Some(2),
        });

        let events = control.handle_request(request(
            "req_agent_team_failed_events",
            "events.poll",
            r#"{"workspace_id":"ws_agent_team_failed","types":["agent.state_changed","notification.created"],"max_events":10}"#,
        ));
        let events: EventPollResult = serde_json::from_str(ok_json(&events)).unwrap();
        assert!(events.events.iter().any(|event| {
            event.event_type == "agent.state_changed"
                && event.session_id.as_deref() == Some(&session_id)
                && event.data_json.contains("\"state\":\"failed\"")
                && event.data_json.contains("exited with code 2")
        }));
        assert!(events.events.iter().any(|event| {
            event.event_type == "notification.created"
                && event.session_id.as_deref() == Some(&session_id)
                && event.data_json.contains("agent.failed")
        }));

        let state = control.handle_request(request(
            "req_agent_team_failed_state",
            "agent.get_state",
            &format!(r#"{{"session_id":"{session_id}"}}"#),
        ));
        let state: AgentStateResult = serde_json::from_str(ok_json(&state)).unwrap();
        assert_eq!(state.state, "failed");
        assert!(state.attention);
    }

    #[test]
    fn control_plane_marks_agent_session_completed_on_clean_exit() {
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");

        let spawn = control.handle_request(request(
            "req_spawn_agent_complete",
            "session.spawn",
            r#"{"workspace_id":"ws_agent_complete","command":["cmd.exe"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral"}"#,
        ));
        let session_id = ok_json(&spawn)
            .split("\"session_id\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("session id in spawn result")
            .to_string();

        let state = control.handle_request(request(
            "req_agent_running",
            "agent.set_state",
            &format!(
                r#"{{"session_id":"{session_id}","state":"running","reason":"Agent started: claude","telemetry":{{"activity":"agent","session":"claude"}}}}"#
            ),
        ));
        let state: AgentStateResult = serde_json::from_str(ok_json(&state)).unwrap();
        assert_eq!(state.state, "running");

        control.runtime.backend.events.push(BackendEvent::Exited {
            session_id: session_id.clone(),
            code: Some(0),
        });

        let events = control.handle_request(request(
            "req_agent_complete_events",
            "events.poll",
            r#"{"workspace_id":"ws_agent_complete","types":["session.state_changed","agent.state_changed","notification.created"],"max_events":10}"#,
        ));
        let events: EventPollResult = serde_json::from_str(ok_json(&events)).unwrap();
        assert!(events.events.iter().any(|event| {
            event.event_type == "agent.state_changed"
                && event.session_id.as_deref() == Some(&session_id)
                && event.data_json.contains("\"state\":\"completed\"")
                && event.data_json.contains("\"source\":\"agent_lifecycle\"")
                && event.data_json.contains("\"activity\":\"agent\"")
                && event.data_json.contains("exited successfully")
        }));
        assert!(events.events.iter().any(|event| {
            event.event_type == "notification.created"
                && event.session_id.as_deref() == Some(&session_id)
                && event.data_json.contains("agent.completed")
        }));

        let state = control.handle_request(request(
            "req_agent_complete_state",
            "agent.get_state",
            &format!(r#"{{"session_id":"{session_id}"}}"#),
        ));
        let state: AgentStateResult = serde_json::from_str(ok_json(&state)).unwrap();
        assert_eq!(state.state, "completed");
        assert!(!state.attention);
        assert_eq!(
            state
                .telemetry
                .and_then(|telemetry| telemetry.activity)
                .as_deref(),
            Some("agent")
        );
    }

    #[test]
    fn control_plane_marks_agent_session_failed_on_nonzero_exit() {
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");

        let spawn = control.handle_request(request(
            "req_spawn_agent_failed",
            "session.spawn",
            r#"{"workspace_id":"ws_agent_failed","command":["cmd.exe"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral"}"#,
        ));
        let session_id = ok_json(&spawn)
            .split("\"session_id\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("session id in spawn result")
            .to_string();

        let _ = control.handle_request(request(
            "req_agent_running_failed",
            "agent.set_state",
            &format!(
                r#"{{"session_id":"{session_id}","state":"running","reason":"Agent started: claude","telemetry":{{"activity":"agent","session":"claude"}}}}"#
            ),
        ));

        control.runtime.backend.events.push(BackendEvent::Exited {
            session_id: session_id.clone(),
            code: Some(3),
        });

        let events = control.handle_request(request(
            "req_agent_failed_events",
            "events.poll",
            r#"{"workspace_id":"ws_agent_failed","types":["agent.state_changed","notification.created"],"max_events":10}"#,
        ));
        let events: EventPollResult = serde_json::from_str(ok_json(&events)).unwrap();
        assert!(events.events.iter().any(|event| {
            event.event_type == "agent.state_changed"
                && event.session_id.as_deref() == Some(&session_id)
                && event.data_json.contains("\"state\":\"failed\"")
                && event.data_json.contains("\"source\":\"agent_lifecycle\"")
                && event.data_json.contains("exited with code 3")
        }));
        assert!(events.events.iter().any(|event| {
            event.event_type == "notification.created"
                && event.session_id.as_deref() == Some(&session_id)
                && event.data_json.contains("agent.failed")
        }));

        let state = control.handle_request(request(
            "req_agent_failed_state",
            "agent.get_state",
            &format!(r#"{{"session_id":"{session_id}"}}"#),
        ));
        let state: AgentStateResult = serde_json::from_str(ok_json(&state)).unwrap();
        assert_eq!(state.state, "failed");
        assert!(state.attention);
    }

    #[test]
    fn detector_parses_shell_marker_agent_signal() {
        let signals = detect_agent_signals(
            br#"build output
::agentmux-agent {"state":"waiting_for_input","reason":"approve patch"}
"#,
        );

        assert_eq!(
            signals,
            vec![DetectedAgentSignal {
                state: AgentState::WaitingForInput,
                reason: Some("approve patch".to_string()),
                source: "shell_marker",
            }]
        );
    }

    #[test]
    fn detector_parses_osc_agent_signal() {
        let signals = detect_agent_signals(
            b"\x1b]777;agentmux;{\"state\":\"agent.completed\",\"message\":\"tests passed\"}\x07",
        );

        assert_eq!(
            signals,
            vec![DetectedAgentSignal {
                state: AgentState::Completed,
                reason: Some("tests passed".to_string()),
                source: "osc_777",
            }]
        );
    }

    #[test]
    fn detector_rejects_unmarked_output_before_signal_scan() {
        assert!(detect_agent_signals(b"plain terminal output without control markers").is_empty());
        assert!(
            detect_agent_signals_from_input(AgentSignalDetectorInput::with_heuristics(
                b"plain terminal output without control markers"
            ))
            .is_empty()
        );
    }

    #[test]
    fn detector_keeps_heuristic_input_opt_in_and_separate() {
        assert!(detect_agent_signals(b"approval required before continuing").is_empty());

        let heuristic = detect_agent_signals_from_input(AgentSignalDetectorInput::with_heuristics(
            b"approval required before continuing\n",
        ));
        assert_eq!(
            heuristic,
            vec![DetectedAgentSignal {
                state: AgentState::WaitingForInput,
                reason: Some("approval required before continuing".to_string()),
                source: "heuristic_output",
            }]
        );

        let explicit = detect_agent_signals_from_input(AgentSignalDetectorInput::with_heuristics(
            br#"approval required before continuing
::agentmux-agent {"state":"completed","reason":"done"}
"#,
        ));
        assert_eq!(
            explicit,
            vec![DetectedAgentSignal {
                state: AgentState::Completed,
                reason: Some("done".to_string()),
                source: "shell_marker",
            }]
        );
    }

    #[test]
    fn heuristic_agent_scan_is_rate_limited_per_session() {
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");
        let session_id = SessionId::from_string("ses_rate_limited");

        assert!(control.agent_heuristic_scan_allowed(&session_id, 0, 32));
        assert!(!control.agent_heuristic_scan_allowed(&session_id, 64, 32));
        assert!(control.agent_heuristic_scan_allowed(
            &session_id,
            AGENT_HEURISTIC_SCAN_INTERVAL_BYTES + 64,
            32
        ));
    }

    #[test]
    fn control_plane_detects_agent_marker_from_terminal_output() {
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");

        let spawn = control.handle_request(request(
            "req_spawn_marker_agent",
            "session.spawn",
            r#"{"workspace_id":"ws_marker","command":["cmd.exe"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral"}"#,
        ));
        let session_id = ok_json(&spawn)
            .split("\"session_id\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("session id in spawn result")
            .to_string();

        control.runtime.backend.events.push(BackendEvent::Output {
            session_id: session_id.clone(),
            bytes: br#"working
::agentmux-agent {"state":"waiting_for_input","reason":"choose next step"}
"#
            .to_vec(),
        });
        control.runtime.backend.events.push(BackendEvent::Output {
            session_id: session_id.clone(),
            bytes: br#"duplicate
::agentmux-agent {"state":"waiting_for_input","reason":"choose next step"}
"#
            .to_vec(),
        });

        let events = control.handle_request(request(
            "req_marker_events",
            "events.poll",
            r#"{"workspace_id":"ws_marker","types":["agent.state_changed","notification.created"],"max_events":10}"#,
        ));
        let events: EventPollResult = serde_json::from_str(ok_json(&events)).unwrap();
        assert_eq!(
            events
                .events
                .iter()
                .filter(|event| event.event_type == "agent.state_changed")
                .count(),
            1
        );
        assert!(events.events.iter().any(|event| {
            event.event_type == "agent.state_changed"
                && event.session_id.as_deref() == Some(&session_id)
                && event.data_json.contains("\"source\":\"shell_marker\"")
                && event.data_json.contains("choose next step")
        }));

        let attention = control.handle_request(request(
            "req_marker_attention",
            "agent.list_attention",
            r#"{"workspace_id":"ws_marker"}"#,
        ));
        let attention: AgentAttentionListResult =
            serde_json::from_str(ok_json(&attention)).unwrap();
        assert_eq!(attention.sessions.len(), 1);
        assert_eq!(
            attention.sessions[0].reason.as_deref(),
            Some("choose next step")
        );

        let notifications = control.handle_request(request(
            "req_marker_notifications",
            "notification.list",
            r#"{"workspace_id":"ws_marker","severity":"warning","include_dismissed":false}"#,
        ));
        let notifications: NotificationListResult =
            serde_json::from_str(ok_json(&notifications)).unwrap();
        assert_eq!(notifications.notifications.len(), 1);
    }

    #[test]
    fn control_plane_detects_opt_in_heuristic_agent_attention() {
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");
        control.set_agent_heuristics_enabled(true);

        let spawn = control.handle_request(request(
            "req_spawn_heuristic_agent",
            "session.spawn",
            r#"{"workspace_id":"ws_heuristic","command":["cmd.exe"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral"}"#,
        ));
        let session_id = ok_json(&spawn)
            .split("\"session_id\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("session id in spawn result")
            .to_string();

        control.runtime.backend.events.push(BackendEvent::Output {
            session_id: session_id.clone(),
            bytes: b"assistant is waiting for input from the user\n".to_vec(),
        });

        let events = control.handle_request(request(
            "req_heuristic_events",
            "events.poll",
            r#"{"workspace_id":"ws_heuristic","types":["agent.state_changed","notification.created"],"max_events":10}"#,
        ));
        let events: EventPollResult = serde_json::from_str(ok_json(&events)).unwrap();
        assert!(events.events.iter().any(|event| {
            event.event_type == "agent.state_changed"
                && event.session_id.as_deref() == Some(&session_id)
                && event.data_json.contains("\"source\":\"heuristic_output\"")
        }));

        let attention = control.handle_request(request(
            "req_heuristic_attention",
            "agent.list_attention",
            r#"{"workspace_id":"ws_heuristic"}"#,
        ));
        let attention: AgentAttentionListResult =
            serde_json::from_str(ok_json(&attention)).unwrap();
        assert_eq!(attention.sessions.len(), 1);
        assert_eq!(
            attention.sessions[0].reason.as_deref(),
            Some("assistant is waiting for input from the user")
        );

        let notifications = control.handle_request(request(
            "req_heuristic_notifications",
            "notification.list",
            r#"{"workspace_id":"ws_heuristic","severity":"warning","include_dismissed":false}"#,
        ));
        let notifications: NotificationListResult =
            serde_json::from_str(ok_json(&notifications)).unwrap();
        assert_eq!(notifications.notifications.len(), 1);
        assert_eq!(
            notifications.notifications[0].notification_type,
            "agent.needs_input"
        );
    }

    #[test]
    fn control_plane_records_backend_kind_from_session_handle() {
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");

        let spawn = control.handle_request(request(
            "req_spawn",
            "session.spawn",
            r#"{"workspace_id":"ws_test","backend":"wsl-direct","command":["bash"],"cwd":"/home/irae","columns":80,"rows":24,"durability":"ephemeral"}"#,
        ));
        let session_id = ok_json(&spawn)
            .split("\"session_id\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("session id in spawn result")
            .to_string();

        let summary = control.handle_request(request(
            "req_get",
            "session.get",
            &format!(r#"{{"session_id":"{session_id}"}}"#),
        ));
        assert!(ok_json(&summary).contains("\"backend_kind\":\"wsl-direct\""));
    }

    #[test]
    fn control_plane_attaches_existing_backend_ref_with_requested_session_id() {
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");

        let attach = control.handle_request(request(
            "req_attach",
            "session.attach",
            r#"{"session_id":"ses_recovered","workspace_id":"ws_test","backend":"wsl-tmux-control","backend_profile":"Ubuntu","backend_ref":"agentmux_ws_test","columns":120,"rows":30,"durability":"durable"}"#,
        ));
        assert!(ok_json(&attach).contains("\"session_id\":\"ses_recovered\""));

        let summary = control.handle_request(request(
            "req_get_recovered",
            "session.get",
            r#"{"session_id":"ses_recovered"}"#,
        ));
        let summary = ok_json(&summary);
        assert!(summary.contains("\"backend_kind\":\"wsl-tmux-control\""));
        assert!(summary.contains("\"backend_native_id\":\"agentmux_ws_test\""));
        assert!(summary.contains("\"state\":\"running\""));
    }

    #[test]
    fn control_plane_rejects_invalid_token() {
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");

        let response = control.handle_request(RequestEnvelope::new(
            "req_bad_auth",
            "session.get",
            r#"{"session_id":"ses_missing"}"#,
            "wrong-token",
        ));

        assert!(matches!(
            response.outcome,
            ResponseOutcome::Error(ControlError {
                code: ErrorCode::Unauthorized,
                ..
            })
        ));
    }

    #[test]
    fn output_pressure_report_pauses_and_resumes_backend_output() {
        let backend = FakeBackend::default();
        let runtime = TerminalRuntime::new(backend);
        let mut control = RuntimeControlPlane::new(runtime, "test-token");

        let spawn = control.handle_request(request(
            "req_spawn",
            "session.spawn",
            r#"{"workspace_id":"ws_test","backend":"conpty","command":["cmd.exe"],"columns":120,"rows":30,"durability":"ephemeral"}"#,
        ));
        let spawn: SessionSpawnResult = serde_json::from_str(ok_json(&spawn)).unwrap();
        let session_id = spawn.session_id;

        let pause = control.handle_request(request(
            "req_pressure_pause",
            "session.report_output_pressure",
            &format!(
                r#"{{"session_id":"{session_id}","queued_bytes":1048576,"max_queued_bytes":1048576,"backpressure_events":1,"write_in_flight":true}}"#
            ),
        ));
        assert!(ok_json(&pause).contains("\"ok\":true"));

        let resume = control.handle_request(request(
            "req_pressure_resume",
            "session.report_output_pressure",
            &format!(
                r#"{{"session_id":"{session_id}","queued_bytes":0,"max_queued_bytes":1048576,"backpressure_events":1,"write_in_flight":false}}"#
            ),
        ));
        assert!(ok_json(&resume).contains("\"ok\":true"));

        assert_eq!(
            control.runtime.backend.output_pauses,
            vec![(session_id.clone(), true), (session_id, false)]
        );
    }

    fn request(id: &str, method: &str, params_json: &str) -> RequestEnvelope {
        RequestEnvelope::new(id, method, params_json, "test-token")
    }

    fn ok_json(response: &ResponseEnvelope) -> &str {
        match &response.outcome {
            ResponseOutcome::Ok { result_json } => result_json,
            ResponseOutcome::Error(error) => panic!("unexpected error: {error:?}"),
        }
    }

    #[derive(Default)]
    struct FakeBackend {
        events: Vec<BackendEvent>,
        output_pauses: Vec<(String, bool)>,
    }

    impl SessionBackend for FakeBackend {
        fn kind(&self) -> BackendTraitKind {
            BackendTraitKind::Conpty
        }

        fn spawn(&mut self, request: SpawnRequest) -> BackendResult<SessionHandle> {
            self.events.push(BackendEvent::Started {
                session_id: request.session_id.clone(),
            });
            let backend_kind = request.backend.unwrap_or(BackendTraitKind::Conpty);
            Ok(SessionHandle {
                session_id: request.session_id,
                backend_kind,
                backend_native_id: Some("fake-process".to_string()),
                transport_pid: Some(42),
            })
        }

        fn attach(&mut self, request: AttachRequest) -> BackendResult<SessionHandle> {
            self.events.push(BackendEvent::Started {
                session_id: request.session_id.clone(),
            });
            Ok(SessionHandle {
                session_id: request.session_id,
                backend_kind: request.backend,
                backend_native_id: Some(request.backend_ref),
                transport_pid: Some(84),
            })
        }

        fn send_input(&mut self, session_id: &str, _input: InputEvent) -> BackendResult<()> {
            self.events.push(BackendEvent::Output {
                session_id: session_id.to_string(),
                bytes: b"ok".to_vec(),
            });
            Ok(())
        }

        fn resize(&mut self, session_id: &str, size: TerminalSize) -> BackendResult<()> {
            self.events.push(BackendEvent::Resized {
                session_id: session_id.to_string(),
                columns: size.columns,
                rows: size.rows,
            });
            Ok(())
        }

        fn terminate(&mut self, session_id: &str, _mode: TerminationMode) -> BackendResult<()> {
            self.events.push(BackendEvent::Exited {
                session_id: session_id.to_string(),
                code: Some(0),
            });
            Ok(())
        }

        fn set_output_paused(&mut self, session_id: &str, paused: bool) -> BackendResult<()> {
            self.output_pauses.push((session_id.to_string(), paused));
            Ok(())
        }

        fn drain_events(&mut self) -> Vec<BackendEvent> {
            std::mem::take(&mut self.events)
        }
    }
}
