use std::fmt;
use std::path::Path;

use rusqlite::{
    params, params_from_iter, Connection, OptionalExtension, Transaction, TransactionBehavior,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Migration {
    pub version: i32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const SCHEMA_MIGRATIONS_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
"#;

pub const INITIAL_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS workspaces (
  workspace_id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  root_pane_id TEXT NOT NULL,
  active_pane_id TEXT NOT NULL,
  project_root TEXT,
  environment_profile_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS panes (
  pane_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  parent_pane_id TEXT,
  kind TEXT NOT NULL,
  split_axis TEXT,
  split_ratio REAL,
  mounted_surface_id TEXT,
  last_focused_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS surfaces (
  surface_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  surface_type TEXT NOT NULL,
  title TEXT NOT NULL,
  session_id TEXT,
  browser_id TEXT,
  created_at TEXT NOT NULL,
  last_visible_at TEXT,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
  session_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  backend_kind TEXT NOT NULL,
  backend_attachment_id TEXT,
  backend_native_id TEXT,
  cwd TEXT,
  command_json TEXT NOT NULL,
  state TEXT NOT NULL,
  exit_code INTEGER,
  durability TEXT NOT NULL,
  created_at TEXT NOT NULL,
  last_seen_at TEXT,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS session_launch_specs (
  session_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  backend_profile TEXT,
  columns INTEGER NOT NULL,
  rows INTEGER NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS backend_attachments (
  attachment_id TEXT PRIMARY KEY,
  backend_kind TEXT NOT NULL,
  transport_pid INTEGER,
  health_state TEXT NOT NULL,
  last_heartbeat_at TEXT,
  diagnostics_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
"#;

pub const AGENT_NOTIFICATIONS_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS agent_states (
  session_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  state TEXT NOT NULL,
  attention INTEGER NOT NULL,
  reason TEXT,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS notifications (
  notification_id TEXT PRIMARY KEY,
  notification_type TEXT NOT NULL,
  severity TEXT NOT NULL,
  workspace_id TEXT,
  session_id TEXT,
  title TEXT NOT NULL,
  message TEXT NOT NULL,
  created_at TEXT NOT NULL,
  dismissed INTEGER NOT NULL DEFAULT 0
);
"#;

pub const AGENT_TELEMETRY_SCHEMA: &str = r#"
ALTER TABLE agent_states ADD COLUMN telemetry_json TEXT;
"#;

pub const SSH_PROFILES_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS ssh_profiles (
  profile_id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  host TEXT NOT NULL,
  user TEXT NOT NULL,
  port INTEGER,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
"#;

pub const SIDEBAR_METADATA_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sidebar_status (
  workspace_id TEXT NOT NULL,
  key TEXT NOT NULL,
  label TEXT NOT NULL,
  icon TEXT,
  color TEXT,
  priority INTEGER NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, key)
);

CREATE TABLE IF NOT EXISTS sidebar_progress (
  workspace_id TEXT PRIMARY KEY,
  value REAL NOT NULL,
  label TEXT,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sidebar_logs (
  log_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  level TEXT NOT NULL,
  source TEXT,
  message TEXT NOT NULL,
  created_at TEXT NOT NULL
);
"#;

pub const WORKSPACE_METADATA_SCHEMA: &str = r#"
ALTER TABLE workspaces ADD COLUMN description TEXT;
ALTER TABLE workspaces ADD COLUMN icon TEXT;
ALTER TABLE workspaces ADD COLUMN color TEXT;
ALTER TABLE workspaces ADD COLUMN default_wsl_distribution TEXT;
ALTER TABLE workspaces ADD COLUMN default_agent_command TEXT;
"#;

pub const WORKSPACE_TERMINAL_PROFILE_SCHEMA: &str = r#"
ALTER TABLE workspaces ADD COLUMN default_terminal_profile TEXT;
"#;

pub const WORKSPACE_GROUPS_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS workspace_groups (
  group_id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  anchor_workspace_id TEXT,
  collapsed INTEGER NOT NULL DEFAULT 0,
  pinned INTEGER NOT NULL DEFAULT 0,
  color TEXT,
  icon TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS workspace_group_members (
  group_id TEXT NOT NULL,
  workspace_id TEXT NOT NULL,
  position INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (group_id, workspace_id),
  UNIQUE (workspace_id)
);
"#;

pub const DOCK_TRUSTS_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS dock_trusts (
  workspace_id TEXT NOT NULL,
  source TEXT NOT NULL,
  config_path TEXT NOT NULL,
  config_hash TEXT NOT NULL,
  trusted_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, source, config_path)
);
"#;

pub const TEAM_COLLABORATION_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS team_tasks (
  task_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT,
  status TEXT NOT NULL,
  assigned_session_id TEXT,
  blocked_reason TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT
);

CREATE TABLE IF NOT EXISTS team_task_dependencies (
  task_id TEXT NOT NULL,
  depends_on_task_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (task_id, depends_on_task_id)
);

CREATE TABLE IF NOT EXISTS team_messages (
  message_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  thread_id TEXT,
  from_session_id TEXT,
  to_session_id TEXT,
  body TEXT NOT NULL,
  kind TEXT NOT NULL,
  created_at TEXT NOT NULL,
  read_at TEXT
);
"#;

pub const SESSION_LAUNCH_ENV_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS session_launch_specs (
  session_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  backend_profile TEXT,
  columns INTEGER NOT NULL,
  rows INTEGER NOT NULL,
  updated_at TEXT NOT NULL
);
ALTER TABLE session_launch_specs ADD COLUMN env_json TEXT NOT NULL DEFAULT '[]';
"#;

pub const WORKTREE_OPERATIONS_AND_GIT_REVIEW_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS worktree_operations (
  operation_id TEXT PRIMARY KEY,
  idempotency_key TEXT NOT NULL UNIQUE,
  repository_root TEXT NOT NULL,
  worktree_path TEXT NOT NULL,
  branch_name TEXT,
  revision TEXT,
  workspace_id TEXT,
  surface_id TEXT,
  session_id TEXT,
  owner_kind TEXT NOT NULL,
  owner_id TEXT,
  ownership_json TEXT NOT NULL DEFAULT '{}',
  request_json TEXT NOT NULL DEFAULT '{}',
  state TEXT NOT NULL CHECK(state IN (
    'prepared',
    'worktree_created',
    'workspace_created',
    'session_created',
    'completed',
    'failed',
    'rolling_back',
    'rolled_back'
  )),
  error_code TEXT,
  error_message TEXT,
  recovery_json TEXT NOT NULL DEFAULT '{}',
  recovery_attempts INTEGER NOT NULL DEFAULT 0,
  last_recovery_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  rolled_back_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_worktree_operations_recovery
  ON worktree_operations (state, updated_at, operation_id);
CREATE INDEX IF NOT EXISTS idx_worktree_operations_workspace
  ON worktree_operations (workspace_id, updated_at, operation_id);

CREATE TABLE IF NOT EXISTS git_review_threads (
  thread_id TEXT PRIMARY KEY,
  repository_root TEXT NOT NULL,
  workspace_id TEXT,
  diff_identity TEXT NOT NULL,
  path TEXT NOT NULL,
  hunk_id TEXT,
  side TEXT NOT NULL,
  line_number INTEGER,
  line_anchor TEXT NOT NULL,
  stale INTEGER NOT NULL DEFAULT 0,
  stale_reason TEXT,
  resolved_at TEXT,
  author_id TEXT NOT NULL,
  target_kind TEXT,
  target_id TEXT,
  delivery_state TEXT NOT NULL DEFAULT 'pending',
  delivery_error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_git_review_threads_repository_diff
  ON git_review_threads (repository_root, diff_identity, stale, updated_at, thread_id);
CREATE INDEX IF NOT EXISTS idx_git_review_threads_workspace
  ON git_review_threads (workspace_id, updated_at, thread_id);

CREATE TABLE IF NOT EXISTS git_review_comments (
  comment_id TEXT PRIMARY KEY,
  thread_id TEXT NOT NULL,
  author_id TEXT NOT NULL,
  body TEXT NOT NULL,
  target_kind TEXT,
  target_id TEXT,
  delivery_state TEXT NOT NULL DEFAULT 'pending',
  delivery_error TEXT,
  delivered_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_git_review_comments_thread
  ON git_review_comments (thread_id, created_at, comment_id);
"#;

pub const WORKTREE_REMOVED_STATE_SCHEMA: &str = r#"
DROP INDEX IF EXISTS idx_worktree_operations_recovery;
DROP INDEX IF EXISTS idx_worktree_operations_workspace;
ALTER TABLE worktree_operations RENAME TO worktree_operations_v12;

CREATE TABLE worktree_operations (
  operation_id TEXT PRIMARY KEY,
  idempotency_key TEXT NOT NULL UNIQUE,
  repository_root TEXT NOT NULL,
  worktree_path TEXT NOT NULL,
  branch_name TEXT,
  revision TEXT,
  workspace_id TEXT,
  surface_id TEXT,
  session_id TEXT,
  owner_kind TEXT NOT NULL,
  owner_id TEXT,
  ownership_json TEXT NOT NULL DEFAULT '{}',
  request_json TEXT NOT NULL DEFAULT '{}',
  state TEXT NOT NULL CHECK(state IN (
    'prepared',
    'worktree_created',
    'workspace_created',
    'session_created',
    'completed',
    'failed',
    'rolling_back',
    'rolled_back',
    'removed'
  )),
  error_code TEXT,
  error_message TEXT,
  recovery_json TEXT NOT NULL DEFAULT '{}',
  recovery_attempts INTEGER NOT NULL DEFAULT 0,
  last_recovery_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  rolled_back_at TEXT
);

INSERT INTO worktree_operations (
  operation_id, idempotency_key, repository_root, worktree_path,
  branch_name, revision, workspace_id, surface_id, session_id,
  owner_kind, owner_id, ownership_json, request_json, state,
  error_code, error_message, recovery_json, recovery_attempts,
  last_recovery_at, created_at, updated_at, completed_at, rolled_back_at
)
SELECT
  operation_id, idempotency_key, repository_root, worktree_path,
  branch_name, revision, workspace_id, surface_id, session_id,
  owner_kind, owner_id, ownership_json, request_json, state,
  error_code, error_message, recovery_json, recovery_attempts,
  last_recovery_at, created_at, updated_at, completed_at, rolled_back_at
FROM worktree_operations_v12;

DROP TABLE worktree_operations_v12;

CREATE INDEX idx_worktree_operations_recovery
  ON worktree_operations (state, updated_at, operation_id);
CREATE INDEX idx_worktree_operations_workspace
  ON worktree_operations (workspace_id, updated_at, operation_id);
"#;

pub const FIVE_TRACK_RECOVERY_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS git_review_delivery_attempts (
  attempt_id TEXT PRIMARY KEY,
  thread_id TEXT NOT NULL,
  payload_hash TEXT NOT NULL,
  target_kind TEXT NOT NULL,
  target_id TEXT NOT NULL,
  attempt_number INTEGER NOT NULL,
  state TEXT NOT NULL CHECK(state IN (
    'prepared',
    'sending',
    'confirmed',
    'uncertain',
    'failed'
  )),
  error_message TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  UNIQUE(thread_id, payload_hash, target_kind, target_id, attempt_number)
);

CREATE INDEX IF NOT EXISTS idx_git_review_delivery_attempt_lookup
  ON git_review_delivery_attempts (
    thread_id, payload_hash, target_kind, target_id, attempt_number DESC
  );

CREATE TABLE IF NOT EXISTS verified_agent_hooks (
  session_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  source TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  state TEXT NOT NULL,
  reason TEXT,
  telemetry_json TEXT,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_verified_agent_hooks_workspace
  ON verified_agent_hooks (workspace_id, updated_at, session_id);
"#;

pub const GIT_MUTATION_RECEIPTS_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS git_mutation_receipts (
  method TEXT NOT NULL,
  repository_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_fingerprint TEXT NOT NULL,
  response_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (method, repository_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_git_mutation_receipts_retention
  ON git_mutation_receipts (
    created_at DESC, method ASC, repository_id ASC, idempotency_key ASC
  );
"#;

pub const GIT_MUTATION_LIFECYCLE_SCHEMA: &str = r#"
ALTER TABLE git_mutation_receipts
  ADD COLUMN lifecycle_state TEXT NOT NULL DEFAULT 'completed'
  CHECK(lifecycle_state IN ('pending', 'completed', 'indeterminate'));
ALTER TABLE git_mutation_receipts
  ADD COLUMN precondition_fingerprint TEXT NOT NULL DEFAULT '';
ALTER TABLE git_mutation_receipts
  ADD COLUMN failure_json TEXT;
ALTER TABLE git_mutation_receipts
  ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';

UPDATE git_mutation_receipts
SET updated_at = created_at
WHERE updated_at = '';

CREATE INDEX IF NOT EXISTS idx_git_mutation_receipts_lifecycle
  ON git_mutation_receipts (
    lifecycle_state, updated_at, method, repository_id, idempotency_key
  );
"#;

pub const SURFACE_RESOURCE_URI_SCHEMA: &str = r#"
ALTER TABLE surfaces ADD COLUMN resource_uri TEXT;
"#;

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_workspace_session_schema",
        sql: INITIAL_SCHEMA,
    },
    Migration {
        version: 2,
        name: "agent_state_notification_schema",
        sql: AGENT_NOTIFICATIONS_SCHEMA,
    },
    Migration {
        version: 3,
        name: "agent_telemetry_column",
        sql: AGENT_TELEMETRY_SCHEMA,
    },
    Migration {
        version: 4,
        name: "ssh_profiles_schema",
        sql: SSH_PROFILES_SCHEMA,
    },
    Migration {
        version: 5,
        name: "sidebar_metadata_schema",
        sql: SIDEBAR_METADATA_SCHEMA,
    },
    Migration {
        version: 6,
        name: "workspace_metadata_columns",
        sql: WORKSPACE_METADATA_SCHEMA,
    },
    Migration {
        version: 7,
        name: "workspace_groups_schema",
        sql: WORKSPACE_GROUPS_SCHEMA,
    },
    Migration {
        version: 8,
        name: "dock_trusts_schema",
        sql: DOCK_TRUSTS_SCHEMA,
    },
    Migration {
        version: 9,
        name: "workspace_terminal_profile_column",
        sql: WORKSPACE_TERMINAL_PROFILE_SCHEMA,
    },
    Migration {
        version: 10,
        name: "team_collaboration_schema",
        sql: TEAM_COLLABORATION_SCHEMA,
    },
    Migration {
        version: 11,
        name: "session_launch_environment",
        sql: SESSION_LAUNCH_ENV_SCHEMA,
    },
    Migration {
        version: 12,
        name: "worktree_operations_and_git_review",
        sql: WORKTREE_OPERATIONS_AND_GIT_REVIEW_SCHEMA,
    },
    Migration {
        version: 13,
        name: "worktree_removed_state",
        sql: WORKTREE_REMOVED_STATE_SCHEMA,
    },
    Migration {
        version: 14,
        name: "five_track_recovery_state",
        sql: FIVE_TRACK_RECOVERY_SCHEMA,
    },
    Migration {
        version: 15,
        name: "git_mutation_receipts",
        sql: GIT_MUTATION_RECEIPTS_SCHEMA,
    },
    Migration {
        version: 16,
        name: "git_mutation_lifecycle",
        sql: GIT_MUTATION_LIFECYCLE_SCHEMA,
    },
    Migration {
        version: 17,
        name: "surface_resource_uri",
        sql: SURFACE_RESOURCE_URI_SCHEMA,
    },
];

pub const REDACTED_VALUE: &str = "redacted";

#[derive(Debug)]
pub enum StoreError {
    Sql(rusqlite::Error),
    Json(serde_json::Error),
    InvalidMigrationOrder,
    InvalidWorktreeOperationInitialState,
    InvalidWorktreeOperationState(String),
    InvalidWorktreeOperationTransition { from: String, to: String },
    WorktreeOperationNotFound(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Sql(error) => write!(f, "{error}"),
            StoreError::Json(error) => write!(f, "{error}"),
            StoreError::InvalidMigrationOrder => f.write_str("migrations are not strictly ordered"),
            StoreError::InvalidWorktreeOperationInitialState => {
                f.write_str("worktree operations must begin in the prepared state")
            }
            StoreError::InvalidWorktreeOperationState(state) => {
                write!(f, "invalid worktree operation state: {state}")
            }
            StoreError::InvalidWorktreeOperationTransition { from, to } => {
                write!(f, "invalid worktree operation transition: {from} -> {to}")
            }
            StoreError::WorktreeOperationNotFound(operation_id) => {
                write!(f, "worktree operation was not found: {operation_id}")
            }
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self {
        StoreError::Sql(value)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(value: serde_json::Error) -> Self {
        StoreError::Json(value)
    }
}

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedWorkspace {
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
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PersistedPane {
    pub pane_id: String,
    pub workspace_id: String,
    pub parent_pane_id: Option<String>,
    pub kind: String,
    pub split_axis: Option<String>,
    pub split_ratio: Option<f64>,
    pub mounted_surface_id: Option<String>,
    pub last_focused_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedSurface {
    pub surface_id: String,
    pub workspace_id: String,
    pub surface_type: String,
    pub title: String,
    pub session_id: Option<String>,
    pub browser_id: Option<String>,
    pub resource_uri: Option<String>,
    pub created_at: String,
    pub last_visible_at: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedSession {
    pub session_id: String,
    pub workspace_id: String,
    pub backend_kind: String,
    pub backend_attachment_id: Option<String>,
    pub backend_native_id: Option<String>,
    pub cwd: Option<String>,
    pub command: Vec<String>,
    pub state: String,
    pub exit_code: Option<i32>,
    pub durability: String,
    pub created_at: String,
    pub last_seen_at: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedSessionLaunchSpec {
    pub session_id: String,
    pub workspace_id: String,
    pub backend_profile: Option<String>,
    pub env: Vec<(String, String)>,
    pub columns: u16,
    pub rows: u16,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedAgentState {
    pub session_id: String,
    pub workspace_id: String,
    pub state: String,
    pub attention: bool,
    pub reason: Option<String>,
    pub updated_at: String,
    pub telemetry_json: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedProfile {
    pub profile_id: String,
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: Option<u16>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedNotification {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedSidebarStatus {
    pub workspace_id: String,
    pub key: String,
    pub label: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub priority: i64,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PersistedSidebarProgress {
    pub workspace_id: String,
    pub value: f64,
    pub label: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedSidebarLog {
    pub log_id: String,
    pub workspace_id: String,
    pub level: String,
    pub source: Option<String>,
    pub message: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedWorkspaceGroup {
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedWorkspaceGroupMember {
    pub group_id: String,
    pub workspace_id: String,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedDockTrust {
    pub workspace_id: String,
    pub source: String,
    pub config_path: String,
    pub config_hash: String,
    pub trusted_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedTeamTask {
    pub task_id: String,
    pub workspace_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub assigned_session_id: Option<String>,
    pub blocked_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedTeamMessage {
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

/// Durable state for the host-level worktree creation saga. The state order is
/// intentionally enforced in the store so callers can safely resume after a
/// process crash without re-running an already completed side effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorktreeOperationState {
    Prepared,
    WorktreeCreated,
    WorkspaceCreated,
    SessionCreated,
    Completed,
    Failed,
    RollingBack,
    RolledBack,
    Removed,
}

impl WorktreeOperationState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::WorktreeCreated => "worktree_created",
            Self::WorkspaceCreated => "workspace_created",
            Self::SessionCreated => "session_created",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::RollingBack => "rolling_back",
            Self::RolledBack => "rolled_back",
            Self::Removed => "removed",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "prepared" => Some(Self::Prepared),
            "worktree_created" => Some(Self::WorktreeCreated),
            "workspace_created" => Some(Self::WorkspaceCreated),
            "session_created" => Some(Self::SessionCreated),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "rolling_back" => Some(Self::RollingBack),
            "rolled_back" => Some(Self::RolledBack),
            "removed" => Some(Self::Removed),
            _ => None,
        }
    }

    pub const fn can_transition_to(self, next: Self) -> bool {
        if self as u8 == next as u8 {
            return true;
        }
        if next as u8 == Self::Removed as u8 {
            return self as u8 != Self::Removed as u8;
        }

        match self {
            Self::Prepared => matches!(
                next,
                Self::WorktreeCreated | Self::Failed | Self::RollingBack
            ),
            Self::WorktreeCreated => matches!(
                next,
                Self::WorkspaceCreated | Self::Failed | Self::RollingBack
            ),
            Self::WorkspaceCreated => matches!(
                next,
                Self::SessionCreated | Self::Failed | Self::RollingBack
            ),
            Self::SessionCreated => {
                matches!(next, Self::Completed | Self::Failed | Self::RollingBack)
            }
            Self::Failed => matches!(next, Self::RollingBack),
            Self::RollingBack => matches!(next, Self::Failed | Self::RolledBack),
            Self::Completed | Self::RolledBack | Self::Removed => false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedWorktreeOperation {
    pub operation_id: String,
    pub idempotency_key: String,
    pub repository_root: String,
    pub worktree_path: String,
    pub branch_name: Option<String>,
    pub revision: Option<String>,
    pub workspace_id: Option<String>,
    pub surface_id: Option<String>,
    pub session_id: Option<String>,
    pub owner_kind: String,
    pub owner_id: Option<String>,
    pub ownership_json: String,
    pub request_json: String,
    pub state: WorktreeOperationState,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub recovery_json: String,
    pub recovery_attempts: i64,
    pub last_recovery_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub rolled_back_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedGitReviewThread {
    pub thread_id: String,
    pub repository_root: String,
    pub workspace_id: Option<String>,
    pub diff_identity: String,
    pub path: String,
    pub hunk_id: Option<String>,
    pub side: String,
    pub line_number: Option<i64>,
    pub line_anchor: String,
    pub stale: bool,
    pub stale_reason: Option<String>,
    pub resolved_at: Option<String>,
    pub author_id: String,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub delivery_state: String,
    pub delivery_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedGitReviewComment {
    pub comment_id: String,
    pub thread_id: String,
    pub author_id: String,
    pub body: String,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub delivery_state: String,
    pub delivery_error: Option<String>,
    pub delivered_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitReviewDeliveryUpdate {
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub delivery_state: String,
    pub delivery_error: Option<String>,
    pub delivered_at: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedGitReviewDeliveryAttempt {
    pub attempt_id: String,
    pub thread_id: String,
    pub payload_hash: String,
    pub target_kind: String,
    pub target_id: String,
    pub attempt_number: i64,
    pub state: String,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedVerifiedAgentHook {
    pub session_id: String,
    pub workspace_id: String,
    pub source: String,
    pub sequence: u64,
    pub state: String,
    pub reason: Option<String>,
    pub telemetry_json: Option<String>,
    pub updated_at: String,
}

/// The durable intent written before an idempotent Git mutation is attempted.
///
/// `method`, `repository_id`, and `idempotency_key` form the durable identity.
/// The request fingerprint binds that identity to one payload, while the
/// precondition fingerprint lets callers reconcile interrupted operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedGitMutationIntent {
    pub method: String,
    pub repository_id: String,
    pub idempotency_key: String,
    pub request_fingerprint: String,
    pub precondition_fingerprint: String,
    pub created_at: String,
}

/// A completed Git mutation result retained for idempotent retries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedGitMutationReceipt {
    pub method: String,
    pub repository_id: String,
    pub idempotency_key: String,
    pub request_fingerprint: String,
    pub response_json: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitMutationReceiptLookup {
    Missing,
    Pending(PersistedGitMutationIntent),
    Match(PersistedGitMutationReceipt),
    Indeterminate {
        intent: PersistedGitMutationIntent,
        failure_json: Option<String>,
    },
    FingerprintMismatch {
        stored_request_fingerprint: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PersistedGitMutationRecord {
    method: String,
    repository_id: String,
    idempotency_key: String,
    request_fingerprint: String,
    response_json: String,
    created_at: String,
    lifecycle_state: String,
    precondition_fingerprint: String,
    failure_json: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceBundle {
    pub workspace: PersistedWorkspace,
    pub panes: Vec<PersistedPane>,
    pub surfaces: Vec<PersistedSurface>,
    pub sessions: Vec<PersistedSession>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecoverySnapshot {
    pub workspaces: Vec<PersistedWorkspace>,
    pub panes: Vec<PersistedPane>,
    pub surfaces: Vec<PersistedSurface>,
    pub sessions: Vec<PersistedSession>,
}

pub struct SqliteStore {
    connection: Connection,
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> StoreResult<Self> {
        let connection = Connection::open(path)?;
        initialize_connection(connection)
    }

    pub fn in_memory() -> StoreResult<Self> {
        let connection = Connection::open_in_memory()?;
        initialize_connection(connection)
    }

    pub fn schema_version(&self) -> StoreResult<i32> {
        self.connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(StoreError::from)
    }

    /// Durably reserves a Git mutation identity before its external effect.
    /// Existing rows are never replaced, so retries can only continue when
    /// both the request and repository precondition fingerprints still match.
    pub fn prepare_git_mutation(
        &mut self,
        intent: &PersistedGitMutationIntent,
    ) -> StoreResult<GitMutationReceiptLookup> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO git_mutation_receipts (
                method, repository_id, idempotency_key, request_fingerprint,
                response_json, created_at, lifecycle_state,
                precondition_fingerprint, failure_json, updated_at
             ) VALUES (?1, ?2, ?3, ?4, '', ?5, 'pending', ?6, NULL, ?5)
             ON CONFLICT(method, repository_id, idempotency_key) DO NOTHING",
            params![
                intent.method,
                intent.repository_id,
                intent.idempotency_key,
                intent.request_fingerprint,
                intent.created_at,
                intent.precondition_fingerprint,
            ],
        )?;
        let stored = load_git_mutation_record(
            &transaction,
            &intent.method,
            &intent.repository_id,
            &intent.idempotency_key,
        )?
        .expect("the inserted or conflicting Git mutation row must exist");
        transaction.commit()?;
        Ok(git_mutation_receipt_lookup(
            stored,
            &intent.request_fingerprint,
        ))
    }

    /// Marks a pending Git mutation completed after its external effect.
    pub fn complete_git_mutation(
        &mut self,
        intent: &PersistedGitMutationIntent,
        response_json: &str,
        updated_at: &str,
    ) -> StoreResult<GitMutationReceiptLookup> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        transaction.execute(
            "UPDATE git_mutation_receipts
             SET response_json = ?6,
                 lifecycle_state = 'completed',
                 failure_json = NULL,
                 updated_at = ?7
             WHERE method = ?1
               AND repository_id = ?2
               AND idempotency_key = ?3
               AND request_fingerprint = ?4
               AND precondition_fingerprint = ?5
               AND lifecycle_state = 'pending'",
            params![
                intent.method,
                intent.repository_id,
                intent.idempotency_key,
                intent.request_fingerprint,
                intent.precondition_fingerprint,
                response_json,
                updated_at,
            ],
        )?;
        let stored = load_git_mutation_record(
            &transaction,
            &intent.method,
            &intent.repository_id,
            &intent.idempotency_key,
        )?;
        transaction.commit()?;
        Ok(match stored {
            Some(stored) => git_mutation_receipt_lookup(stored, &intent.request_fingerprint),
            None => GitMutationReceiptLookup::Missing,
        })
    }

    /// Permanently blocks automatic replay when the external effect cannot be
    /// distinguished from a concurrent repository change.
    pub fn mark_git_mutation_indeterminate(
        &mut self,
        intent: &PersistedGitMutationIntent,
        failure_json: &str,
        updated_at: &str,
    ) -> StoreResult<GitMutationReceiptLookup> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        transaction.execute(
            "UPDATE git_mutation_receipts
             SET lifecycle_state = 'indeterminate',
                 failure_json = ?6,
                 updated_at = ?7
             WHERE method = ?1
               AND repository_id = ?2
               AND idempotency_key = ?3
               AND request_fingerprint = ?4
               AND precondition_fingerprint = ?5
               AND lifecycle_state = 'pending'",
            params![
                intent.method,
                intent.repository_id,
                intent.idempotency_key,
                intent.request_fingerprint,
                intent.precondition_fingerprint,
                failure_json,
                updated_at,
            ],
        )?;
        let stored = load_git_mutation_record(
            &transaction,
            &intent.method,
            &intent.repository_id,
            &intent.idempotency_key,
        )?;
        transaction.commit()?;
        Ok(match stored {
            Some(stored) => git_mutation_receipt_lookup(stored, &intent.request_fingerprint),
            None => GitMutationReceiptLookup::Missing,
        })
    }

    /// Atomically persists the first successful response for a Git mutation
    /// identity or returns the result of comparing a prior receipt.
    ///
    /// The operation uses an immediate transaction so concurrent callers do
    /// not observe a gap between the conflict-safe insert and the readback.
    pub fn create_or_load_git_mutation_receipt(
        &mut self,
        receipt: &PersistedGitMutationReceipt,
    ) -> StoreResult<GitMutationReceiptLookup> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO git_mutation_receipts (
                method, repository_id, idempotency_key, request_fingerprint,
                response_json, created_at, lifecycle_state,
                precondition_fingerprint, failure_json, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'completed', '', NULL, ?6)
             ON CONFLICT(method, repository_id, idempotency_key) DO NOTHING",
            params![
                receipt.method,
                receipt.repository_id,
                receipt.idempotency_key,
                receipt.request_fingerprint,
                receipt.response_json,
                receipt.created_at,
            ],
        )?;
        let stored = load_git_mutation_record(
            &transaction,
            &receipt.method,
            &receipt.repository_id,
            &receipt.idempotency_key,
        )?
        .expect("the inserted or conflicting Git mutation row must exist");
        transaction.commit()?;
        Ok(git_mutation_receipt_lookup(
            stored,
            &receipt.request_fingerprint,
        ))
    }

    /// Loads a Git mutation receipt and compares it with the supplied payload
    /// fingerprint. A mismatch is deliberately returned as data rather than a
    /// storage error so every transport can surface the same conflict code.
    pub fn load_git_mutation_receipt(
        &self,
        method: &str,
        repository_id: &str,
        idempotency_key: &str,
        request_fingerprint: &str,
    ) -> StoreResult<GitMutationReceiptLookup> {
        let stored =
            load_git_mutation_record(&self.connection, method, repository_id, idempotency_key)?;
        Ok(match stored {
            Some(stored) => git_mutation_receipt_lookup(stored, request_fingerprint),
            None => GitMutationReceiptLookup::Missing,
        })
    }

    /// Retains the newest `max_entries` completed receipts using a stable
    /// tie-breaker. Pending and indeterminate rows are never pruned.
    pub fn prune_git_mutation_receipts(&mut self, max_entries: usize) -> StoreResult<usize> {
        let max_entries = i64::try_from(max_entries).unwrap_or(i64::MAX);
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let deleted = transaction.execute(
            "DELETE FROM git_mutation_receipts
             WHERE lifecycle_state = 'completed'
               AND rowid IN (
               SELECT rowid
               FROM git_mutation_receipts
               WHERE lifecycle_state = 'completed'
               ORDER BY created_at DESC, method ASC, repository_id ASC, idempotency_key ASC
               LIMIT -1 OFFSET ?1
             )",
            [max_entries],
        )?;
        transaction.commit()?;
        Ok(deleted)
    }

    pub fn save_workspace_bundle(&mut self, bundle: &WorkspaceBundle) -> StoreResult<()> {
        let tx = self.connection.transaction()?;
        save_workspace_bundle_in_transaction(&tx, bundle)?;
        tx.commit().map_err(StoreError::from)
    }

    pub fn save_workspace_bundle_and_launch_spec(
        &mut self,
        bundle: &WorkspaceBundle,
        spec: &PersistedSessionLaunchSpec,
    ) -> StoreResult<()> {
        let tx = self.connection.transaction()?;
        save_workspace_bundle_in_transaction(&tx, bundle)?;
        upsert_session_launch_spec(&tx, spec)?;
        tx.commit().map_err(StoreError::from)
    }

    pub fn list_workspaces(&self) -> StoreResult<Vec<PersistedWorkspace>> {
        let mut statement = self.connection.prepare(
            "SELECT workspace_id, name, root_pane_id, active_pane_id, project_root,
                    environment_profile_id, description, icon, color,
                    default_wsl_distribution, default_terminal_profile, default_agent_command,
                    created_at, updated_at
             FROM workspaces
             ORDER BY updated_at DESC, workspace_id ASC",
        )?;
        let rows = statement.query_map([], workspace_from_row)?;
        collect_rows(rows)
    }

    pub fn load_session(&self, session_id: &str) -> StoreResult<Option<PersistedSession>> {
        self.connection
            .query_row(
                "SELECT session_id, workspace_id, backend_kind, backend_attachment_id,
                        backend_native_id, cwd, command_json, state, exit_code, durability,
                        created_at, last_seen_at, updated_at
                 FROM sessions
                 WHERE session_id = ?1",
                [session_id],
                session_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn upsert_session_launch_spec(
        &mut self,
        spec: &PersistedSessionLaunchSpec,
    ) -> StoreResult<()> {
        upsert_session_launch_spec(&self.connection, spec)
    }

    pub fn load_session_launch_spec(
        &self,
        session_id: &str,
    ) -> StoreResult<Option<PersistedSessionLaunchSpec>> {
        self.connection
            .query_row(
                "SELECT session_id, workspace_id, backend_profile, env_json, columns, rows, updated_at
                 FROM session_launch_specs
                 WHERE session_id = ?1",
                [session_id],
                |row| {
                    let env_json: String = row.get(3)?;
                    let env = serde_json::from_str(&env_json).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                    Ok(PersistedSessionLaunchSpec {
                        session_id: row.get(0)?,
                        workspace_id: row.get(1)?,
                        backend_profile: row.get(2)?,
                        env,
                        columns: row.get(4)?,
                        rows: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn list_sessions(&self) -> StoreResult<Vec<PersistedSession>> {
        let mut statement = self.connection.prepare(
            "SELECT session_id, workspace_id, backend_kind, backend_attachment_id,
                    backend_native_id, cwd, command_json, state, exit_code, durability,
                    created_at, last_seen_at, updated_at
             FROM sessions
             ORDER BY created_at ASC, session_id ASC",
        )?;
        let rows = statement.query_map([], session_from_row)?;
        collect_rows(rows)
    }

    pub fn load_workspace_bundle(
        &self,
        workspace_id: &str,
    ) -> StoreResult<Option<WorkspaceBundle>> {
        let workspace = self
            .connection
            .query_row(
                "SELECT workspace_id, name, root_pane_id, active_pane_id, project_root,
                        environment_profile_id, description, icon, color,
                        default_wsl_distribution, default_terminal_profile, default_agent_command,
                        created_at, updated_at
                 FROM workspaces
                 WHERE workspace_id = ?1",
                [workspace_id],
                workspace_from_row,
            )
            .optional()?;

        let Some(workspace) = workspace else {
            return Ok(None);
        };

        Ok(Some(WorkspaceBundle {
            workspace,
            panes: self.list_panes_for_workspace(workspace_id)?,
            surfaces: self.list_surfaces_for_workspace(workspace_id)?,
            sessions: self.list_sessions_for_workspace(workspace_id)?,
        }))
    }

    pub fn load_recovery_snapshot(&self) -> StoreResult<RecoverySnapshot> {
        let workspaces = self.list_workspaces()?;
        let panes = self.list_all_panes()?;
        let surfaces = self.list_all_surfaces()?;
        let sessions = self
            .list_sessions()?
            .into_iter()
            .map(normalize_session_for_recovery)
            .collect();

        Ok(RecoverySnapshot {
            workspaces,
            panes,
            surfaces,
            sessions,
        })
    }

    pub fn update_session_state(
        &mut self,
        session_id: &str,
        state: &str,
        exit_code: Option<i32>,
        updated_at: &str,
    ) -> StoreResult<bool> {
        let updated = self.connection.execute(
            "UPDATE sessions
             SET state = ?2,
                 exit_code = ?3,
                 last_seen_at = ?4,
                 updated_at = ?4
             WHERE session_id = ?1",
            params![session_id, state, exit_code, updated_at],
        )?;
        Ok(updated > 0)
    }

    /// Update a session's current working directory. Driven by live cwd
    /// tracking (OSC 7) so the footer git status follows the directory the
    /// terminal has actually `cd`'d into.
    pub fn update_session_cwd(
        &mut self,
        session_id: &str,
        cwd: Option<&str>,
        updated_at: &str,
    ) -> StoreResult<()> {
        self.connection.execute(
            "UPDATE sessions
             SET cwd = ?2,
                 updated_at = ?3
             WHERE session_id = ?1",
            params![session_id, cwd, updated_at],
        )?;
        Ok(())
    }

    /// Delete a session row. Used by startup recovery to drop a dead ephemeral
    /// session that has been superseded by a freshly respawned one.
    pub fn delete_session(&mut self, session_id: &str) -> StoreResult<()> {
        self.connection.execute(
            "DELETE FROM agent_states WHERE session_id = ?1",
            params![session_id],
        )?;
        self.connection.execute(
            "DELETE FROM session_launch_specs WHERE session_id = ?1",
            params![session_id],
        )?;
        self.connection.execute(
            "DELETE FROM sessions WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    /// Delete a surface row. Used by startup recovery to drop the now-orphaned
    /// surface left behind when an ephemeral terminal is respawned into its pane.
    pub fn delete_surface(&mut self, surface_id: &str) -> StoreResult<()> {
        self.connection.execute(
            "DELETE FROM surfaces WHERE surface_id = ?1",
            params![surface_id],
        )?;
        Ok(())
    }

    pub fn rename_workspace(
        &mut self,
        workspace_id: &str,
        name: &str,
        updated_at: &str,
    ) -> StoreResult<bool> {
        let updated = self.connection.execute(
            "UPDATE workspaces
             SET name = ?2,
                 updated_at = ?3
             WHERE workspace_id = ?1",
            params![workspace_id, name, updated_at],
        )?;
        Ok(updated > 0)
    }

    pub fn delete_workspace(&mut self, workspace_id: &str) -> StoreResult<bool> {
        let tx = self.connection.transaction()?;
        tx.execute(
            "DELETE FROM workspace_group_members WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        tx.execute(
            "UPDATE workspace_groups
             SET anchor_workspace_id = NULL
             WHERE anchor_workspace_id = ?1",
            params![workspace_id],
        )?;
        tx.execute(
            "DELETE FROM notifications WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        tx.execute(
            "DELETE FROM sidebar_logs WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        tx.execute(
            "DELETE FROM sidebar_progress WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        tx.execute(
            "DELETE FROM sidebar_status WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        tx.execute(
            "DELETE FROM dock_trusts WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        tx.execute(
            "DELETE FROM agent_states WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        tx.execute(
            "DELETE FROM session_launch_specs WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        tx.execute(
            "DELETE FROM sessions WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        tx.execute(
            "DELETE FROM surfaces WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        tx.execute(
            "DELETE FROM panes WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        let deleted = tx.execute(
            "DELETE FROM workspaces WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        tx.commit()?;
        Ok(deleted > 0)
    }

    pub fn upsert_workspace_group(&mut self, group: &PersistedWorkspaceGroup) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO workspace_groups (
                group_id, name, anchor_workspace_id, collapsed, pinned,
                color, icon, sort_order, created_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(group_id) DO UPDATE SET
                name = excluded.name,
                anchor_workspace_id = excluded.anchor_workspace_id,
                collapsed = excluded.collapsed,
                pinned = excluded.pinned,
                color = excluded.color,
                icon = excluded.icon,
                sort_order = excluded.sort_order,
                updated_at = excluded.updated_at",
            params![
                group.group_id,
                group.name,
                group.anchor_workspace_id,
                group.collapsed,
                group.pinned,
                group.color,
                group.icon,
                group.sort_order,
                group.created_at,
                group.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn list_workspace_groups(&self) -> StoreResult<Vec<PersistedWorkspaceGroup>> {
        let mut statement = self.connection.prepare(
            "SELECT group_id, name, anchor_workspace_id, collapsed, pinned,
                    color, icon, sort_order, created_at, updated_at
             FROM workspace_groups
             ORDER BY pinned DESC, sort_order ASC, updated_at DESC, group_id ASC",
        )?;
        let rows = statement.query_map([], workspace_group_from_row)?;
        collect_rows(rows)
    }

    pub fn load_workspace_group(
        &self,
        group_id: &str,
    ) -> StoreResult<Option<PersistedWorkspaceGroup>> {
        self.connection
            .query_row(
                "SELECT group_id, name, anchor_workspace_id, collapsed, pinned,
                        color, icon, sort_order, created_at, updated_at
                 FROM workspace_groups
                 WHERE group_id = ?1",
                [group_id],
                workspace_group_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn delete_workspace_group(&mut self, group_id: &str) -> StoreResult<bool> {
        let tx = self.connection.transaction()?;
        tx.execute(
            "DELETE FROM workspace_group_members WHERE group_id = ?1",
            params![group_id],
        )?;
        let deleted = tx.execute(
            "DELETE FROM workspace_groups WHERE group_id = ?1",
            params![group_id],
        )?;
        tx.commit()?;
        Ok(deleted > 0)
    }

    pub fn upsert_workspace_group_member(
        &mut self,
        member: &PersistedWorkspaceGroupMember,
    ) -> StoreResult<()> {
        let tx = self.connection.transaction()?;
        tx.execute(
            "DELETE FROM workspace_group_members WHERE workspace_id = ?1",
            params![member.workspace_id],
        )?;
        tx.execute(
            "INSERT INTO workspace_group_members (
                group_id, workspace_id, position, created_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(group_id, workspace_id) DO UPDATE SET
                position = excluded.position,
                updated_at = excluded.updated_at",
            params![
                member.group_id,
                member.workspace_id,
                member.position,
                member.created_at,
                member.updated_at
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn remove_workspace_group_member(
        &mut self,
        group_id: &str,
        workspace_id: &str,
    ) -> StoreResult<bool> {
        let deleted = self.connection.execute(
            "DELETE FROM workspace_group_members
             WHERE group_id = ?1 AND workspace_id = ?2",
            params![group_id, workspace_id],
        )?;
        Ok(deleted > 0)
    }

    pub fn list_workspace_group_members(
        &self,
        group_id: Option<&str>,
    ) -> StoreResult<Vec<PersistedWorkspaceGroupMember>> {
        if let Some(group_id) = group_id {
            let mut statement = self.connection.prepare(
                "SELECT group_id, workspace_id, position, created_at, updated_at
                 FROM workspace_group_members
                 WHERE group_id = ?1
                 ORDER BY position ASC, updated_at DESC, workspace_id ASC",
            )?;
            let rows = statement.query_map([group_id], workspace_group_member_from_row)?;
            return collect_rows(rows);
        }

        let mut statement = self.connection.prepare(
            "SELECT group_id, workspace_id, position, created_at, updated_at
             FROM workspace_group_members
             ORDER BY group_id ASC, position ASC, updated_at DESC, workspace_id ASC",
        )?;
        let rows = statement.query_map([], workspace_group_member_from_row)?;
        collect_rows(rows)
    }

    pub fn upsert_dock_trust(&mut self, trust: &PersistedDockTrust) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO dock_trusts (
                workspace_id, source, config_path, config_hash, trusted_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(workspace_id, source, config_path) DO UPDATE SET
                config_hash = excluded.config_hash,
                trusted_at = excluded.trusted_at,
                updated_at = excluded.updated_at",
            params![
                trust.workspace_id,
                trust.source,
                trust.config_path,
                trust.config_hash,
                trust.trusted_at,
                trust.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn load_dock_trust(
        &self,
        workspace_id: &str,
        source: &str,
        config_path: &str,
    ) -> StoreResult<Option<PersistedDockTrust>> {
        self.connection
            .query_row(
                "SELECT workspace_id, source, config_path, config_hash, trusted_at, updated_at
                 FROM dock_trusts
                 WHERE workspace_id = ?1 AND source = ?2 AND config_path = ?3",
                params![workspace_id, source, config_path],
                dock_trust_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn dock_trust_matches(
        &self,
        workspace_id: &str,
        source: &str,
        config_path: &str,
        config_hash: &str,
    ) -> StoreResult<bool> {
        Ok(self
            .load_dock_trust(workspace_id, source, config_path)?
            .is_some_and(|trust| trust.config_hash == config_hash))
    }

    pub fn upsert_agent_state(&mut self, state: &PersistedAgentState) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO agent_states (
                session_id, workspace_id, state, attention, reason, updated_at, telemetry_json
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(session_id) DO UPDATE SET
                workspace_id = excluded.workspace_id,
                state = excluded.state,
                attention = excluded.attention,
                reason = excluded.reason,
                updated_at = excluded.updated_at,
                telemetry_json = excluded.telemetry_json",
            params![
                state.session_id,
                state.workspace_id,
                state.state,
                state.attention,
                state.reason,
                state.updated_at,
                state.telemetry_json
            ],
        )?;
        Ok(())
    }

    pub fn load_agent_state(&self, session_id: &str) -> StoreResult<Option<PersistedAgentState>> {
        self.connection
            .query_row(
                "SELECT session_id, workspace_id, state, attention, reason, updated_at, telemetry_json
                 FROM agent_states
                 WHERE session_id = ?1",
                [session_id],
                agent_state_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn list_agent_attention(
        &self,
        workspace_id: Option<&str>,
    ) -> StoreResult<Vec<PersistedAgentState>> {
        if let Some(workspace_id) = workspace_id {
            let mut statement = self.connection.prepare(
                "SELECT session_id, workspace_id, state, attention, reason, updated_at, telemetry_json
                 FROM agent_states
                 WHERE attention = 1 AND workspace_id = ?1
                 ORDER BY updated_at DESC, session_id ASC",
            )?;
            let rows = statement.query_map([workspace_id], agent_state_from_row)?;
            return collect_rows(rows);
        }

        let mut statement = self.connection.prepare(
            "SELECT session_id, workspace_id, state, attention, reason, updated_at, telemetry_json
             FROM agent_states
             WHERE attention = 1
             ORDER BY updated_at DESC, session_id ASC",
        )?;
        let rows = statement.query_map([], agent_state_from_row)?;
        collect_rows(rows)
    }

    pub fn list_agent_states(
        &self,
        workspace_id: Option<&str>,
    ) -> StoreResult<Vec<PersistedAgentState>> {
        if let Some(workspace_id) = workspace_id {
            let mut statement = self.connection.prepare(
                "SELECT session_id, workspace_id, state, attention, reason, updated_at, telemetry_json
                 FROM agent_states
                 WHERE workspace_id = ?1
                 ORDER BY updated_at DESC, session_id ASC",
            )?;
            let rows = statement.query_map([workspace_id], agent_state_from_row)?;
            return collect_rows(rows);
        }

        let mut statement = self.connection.prepare(
            "SELECT session_id, workspace_id, state, attention, reason, updated_at, telemetry_json
             FROM agent_states
             ORDER BY updated_at DESC, session_id ASC",
        )?;
        let rows = statement.query_map([], agent_state_from_row)?;
        collect_rows(rows)
    }

    pub fn clear_agent_attention(
        &mut self,
        session_id: &str,
        updated_at: &str,
    ) -> StoreResult<bool> {
        let updated = self.connection.execute(
            "UPDATE agent_states
             SET attention = 0,
                 updated_at = ?2
             WHERE session_id = ?1",
            params![session_id, updated_at],
        )?;
        Ok(updated > 0)
    }

    pub fn upsert_notification(&mut self, notification: &PersistedNotification) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO notifications (
                notification_id, notification_type, severity, workspace_id, session_id,
                title, message, created_at, dismissed
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(notification_id) DO UPDATE SET
                notification_type = excluded.notification_type,
                severity = excluded.severity,
                workspace_id = excluded.workspace_id,
                session_id = excluded.session_id,
                title = excluded.title,
                message = excluded.message,
                created_at = excluded.created_at,
                dismissed = CASE
                  WHEN notifications.dismissed = 1 THEN 1
                  ELSE excluded.dismissed
                END",
            params![
                notification.notification_id,
                notification.notification_type,
                notification.severity,
                notification.workspace_id,
                notification.session_id,
                notification.title,
                notification.message,
                notification.created_at,
                notification.dismissed
            ],
        )?;
        Ok(())
    }

    pub fn list_notifications(
        &self,
        workspace_id: Option<&str>,
        severity: Option<&str>,
        include_dismissed: bool,
    ) -> StoreResult<Vec<PersistedNotification>> {
        let mut sql = String::from(
            "SELECT notification_id, notification_type, severity, workspace_id, session_id,
                    title, message, created_at, dismissed
             FROM notifications",
        );
        let mut predicates = Vec::new();
        let mut values = Vec::new();

        if let Some(workspace_id) = workspace_id {
            predicates.push("workspace_id = ?");
            values.push(workspace_id);
        }
        if let Some(severity) = severity {
            predicates.push("severity = ?");
            values.push(severity);
        }
        if !include_dismissed {
            predicates.push("dismissed = 0");
        }
        if !predicates.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&predicates.join(" AND "));
        }
        sql.push_str(" ORDER BY created_at DESC, notification_id DESC");

        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values), notification_from_row)?;
        collect_rows(rows)
    }

    pub fn dismiss_notification(&mut self, notification_id: &str) -> StoreResult<bool> {
        let updated = self.connection.execute(
            "UPDATE notifications
             SET dismissed = 1
             WHERE notification_id = ?1",
            params![notification_id],
        )?;
        Ok(updated > 0)
    }

    pub fn clear_notifications(
        &mut self,
        workspace_id: Option<&str>,
        severity: Option<&str>,
    ) -> StoreResult<usize> {
        let mut sql = String::from("UPDATE notifications SET dismissed = 1");
        let mut predicates = Vec::new();
        let mut values = Vec::new();
        if let Some(workspace_id) = workspace_id {
            predicates.push("workspace_id = ?");
            values.push(workspace_id);
        }
        if let Some(severity) = severity {
            predicates.push("severity = ?");
            values.push(severity);
        }
        if !predicates.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&predicates.join(" AND "));
        }
        self.connection
            .execute(&sql, params_from_iter(values))
            .map_err(StoreError::from)
    }

    pub fn upsert_team_task(&mut self, task: &PersistedTeamTask) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO team_tasks (
                task_id, workspace_id, title, description, status, assigned_session_id,
                blocked_reason, created_at, updated_at, completed_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(task_id) DO UPDATE SET
                workspace_id = excluded.workspace_id,
                title = excluded.title,
                description = excluded.description,
                status = excluded.status,
                assigned_session_id = excluded.assigned_session_id,
                blocked_reason = excluded.blocked_reason,
                updated_at = excluded.updated_at,
                completed_at = excluded.completed_at",
            params![
                task.task_id,
                task.workspace_id,
                task.title,
                task.description,
                task.status,
                task.assigned_session_id,
                task.blocked_reason,
                task.created_at,
                task.updated_at,
                task.completed_at
            ],
        )?;
        Ok(())
    }

    pub fn load_team_task(&self, task_id: &str) -> StoreResult<Option<PersistedTeamTask>> {
        self.connection
            .query_row(
                "SELECT task_id, workspace_id, title, description, status, assigned_session_id,
                        blocked_reason, created_at, updated_at, completed_at
                 FROM team_tasks
                 WHERE task_id = ?1",
                [task_id],
                team_task_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn list_team_tasks(
        &self,
        workspace_id: Option<&str>,
    ) -> StoreResult<Vec<PersistedTeamTask>> {
        if let Some(workspace_id) = workspace_id {
            let mut statement = self.connection.prepare(
                "SELECT task_id, workspace_id, title, description, status, assigned_session_id,
                        blocked_reason, created_at, updated_at, completed_at
                 FROM team_tasks
                 WHERE workspace_id = ?1
                 ORDER BY created_at ASC, task_id ASC",
            )?;
            let rows = statement.query_map([workspace_id], team_task_from_row)?;
            return collect_rows(rows);
        }

        let mut statement = self.connection.prepare(
            "SELECT task_id, workspace_id, title, description, status, assigned_session_id,
                    blocked_reason, created_at, updated_at, completed_at
             FROM team_tasks
             ORDER BY created_at ASC, task_id ASC",
        )?;
        let rows = statement.query_map([], team_task_from_row)?;
        collect_rows(rows)
    }

    pub fn set_team_task_status(
        &mut self,
        task_id: &str,
        status: &str,
        assigned_session_id: Option<&str>,
        blocked_reason: Option<&str>,
        completed_at: Option<&str>,
        updated_at: &str,
    ) -> StoreResult<bool> {
        let updated = self.connection.execute(
            "UPDATE team_tasks
             SET status = ?2,
                 assigned_session_id = COALESCE(?3, assigned_session_id),
                 blocked_reason = ?4,
                 completed_at = ?5,
                 updated_at = ?6
             WHERE task_id = ?1",
            params![
                task_id,
                status,
                assigned_session_id,
                blocked_reason,
                completed_at,
                updated_at
            ],
        )?;
        Ok(updated > 0)
    }

    pub fn replace_team_task_dependencies(
        &mut self,
        task_id: &str,
        depends_on: &[String],
        created_at: &str,
    ) -> StoreResult<()> {
        let tx = self.connection.transaction()?;
        tx.execute(
            "DELETE FROM team_task_dependencies WHERE task_id = ?1",
            [task_id],
        )?;
        for dependency in depends_on {
            tx.execute(
                "INSERT OR IGNORE INTO team_task_dependencies (
                    task_id, depends_on_task_id, created_at
                 )
                 VALUES (?1, ?2, ?3)",
                params![task_id, dependency, created_at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_team_task_dependencies(
        &self,
        workspace_id: Option<&str>,
    ) -> StoreResult<Vec<(String, String)>> {
        if let Some(workspace_id) = workspace_id {
            let mut statement = self.connection.prepare(
                "SELECT d.task_id, d.depends_on_task_id
                 FROM team_task_dependencies d
                 INNER JOIN team_tasks t ON t.task_id = d.task_id
                 WHERE t.workspace_id = ?1
                 ORDER BY d.task_id ASC, d.depends_on_task_id ASC",
            )?;
            let rows = statement.query_map([workspace_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
            return collect_rows(rows);
        }

        let mut statement = self.connection.prepare(
            "SELECT task_id, depends_on_task_id
             FROM team_task_dependencies
             ORDER BY task_id ASC, depends_on_task_id ASC",
        )?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        collect_rows(rows)
    }

    pub fn upsert_team_message(&mut self, message: &PersistedTeamMessage) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO team_messages (
                message_id, workspace_id, thread_id, from_session_id, to_session_id,
                body, kind, created_at, read_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(message_id) DO UPDATE SET
                workspace_id = excluded.workspace_id,
                thread_id = excluded.thread_id,
                from_session_id = excluded.from_session_id,
                to_session_id = excluded.to_session_id,
                body = excluded.body,
                kind = excluded.kind,
                read_at = excluded.read_at",
            params![
                message.message_id,
                message.workspace_id,
                message.thread_id,
                message.from_session_id,
                message.to_session_id,
                message.body,
                message.kind,
                message.created_at,
                message.read_at
            ],
        )?;
        Ok(())
    }

    pub fn load_team_message(&self, message_id: &str) -> StoreResult<Option<PersistedTeamMessage>> {
        self.connection
            .query_row(
                "SELECT message_id, workspace_id, thread_id, from_session_id, to_session_id,
                        body, kind, created_at, read_at
                 FROM team_messages
                 WHERE message_id = ?1",
                [message_id],
                team_message_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    /// Persists a deterministic mailbox delivery and its notification in one
    /// transaction. Replaying the same IDs is idempotent and cannot create a
    /// second mailbox entry after a host restart.
    pub fn upsert_team_message_and_notification(
        &mut self,
        message: &PersistedTeamMessage,
        notification: &PersistedNotification,
    ) -> StoreResult<()> {
        let tx = self.connection.transaction()?;
        tx.execute(
            "INSERT INTO team_messages (
                message_id, workspace_id, thread_id, from_session_id, to_session_id,
                body, kind, created_at, read_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(message_id) DO NOTHING",
            params![
                message.message_id,
                message.workspace_id,
                message.thread_id,
                message.from_session_id,
                message.to_session_id,
                message.body,
                message.kind,
                message.created_at,
                message.read_at
            ],
        )?;
        tx.execute(
            "INSERT INTO notifications (
                notification_id, notification_type, severity, workspace_id,
                session_id, title, message, created_at, dismissed
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(notification_id) DO NOTHING",
            params![
                notification.notification_id,
                notification.notification_type,
                notification.severity,
                notification.workspace_id,
                notification.session_id,
                notification.title,
                notification.message,
                notification.created_at,
                i64::from(notification.dismissed)
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_team_messages(
        &self,
        workspace_id: Option<&str>,
        include_read: bool,
    ) -> StoreResult<Vec<PersistedTeamMessage>> {
        let mut sql = String::from(
            "SELECT message_id, workspace_id, thread_id, from_session_id, to_session_id,
                    body, kind, created_at, read_at
             FROM team_messages",
        );
        let mut predicates = Vec::new();
        let mut values = Vec::new();
        if let Some(workspace_id) = workspace_id {
            predicates.push("workspace_id = ?");
            values.push(workspace_id);
        }
        if !include_read {
            predicates.push("read_at IS NULL");
        }
        if !predicates.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&predicates.join(" AND "));
        }
        sql.push_str(" ORDER BY created_at DESC, message_id DESC");
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values), team_message_from_row)?;
        collect_rows(rows)
    }

    pub fn mark_team_message_read(&mut self, message_id: &str, read_at: &str) -> StoreResult<bool> {
        let updated = self.connection.execute(
            "UPDATE team_messages
             SET read_at = ?2
             WHERE message_id = ?1",
            params![message_id, read_at],
        )?;
        Ok(updated > 0)
    }

    /// Inserts a saga exactly once for an idempotency key. A retry with the
    /// same key returns the original operation without replacing its state or
    /// ownership record.
    pub fn create_or_load_worktree_operation(
        &mut self,
        operation: &PersistedWorktreeOperation,
    ) -> StoreResult<PersistedWorktreeOperation> {
        if operation.state != WorktreeOperationState::Prepared {
            return Err(StoreError::InvalidWorktreeOperationInitialState);
        }

        let tx = self.connection.transaction()?;
        tx.execute(
            "INSERT INTO worktree_operations (
                operation_id, idempotency_key, repository_root, worktree_path,
                branch_name, revision, workspace_id, surface_id, session_id,
                owner_kind, owner_id, ownership_json, request_json, state,
                error_code, error_message, recovery_json, recovery_attempts,
                last_recovery_at, created_at, updated_at, completed_at, rolled_back_at
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23
             ) ON CONFLICT(idempotency_key) DO NOTHING",
            params![
                operation.operation_id,
                operation.idempotency_key,
                operation.repository_root,
                operation.worktree_path,
                operation.branch_name,
                operation.revision,
                operation.workspace_id,
                operation.surface_id,
                operation.session_id,
                operation.owner_kind,
                operation.owner_id,
                operation.ownership_json,
                operation.request_json,
                operation.state.as_str(),
                operation.error_code,
                operation.error_message,
                operation.recovery_json,
                operation.recovery_attempts,
                operation.last_recovery_at,
                operation.created_at,
                operation.updated_at,
                operation.completed_at,
                operation.rolled_back_at,
            ],
        )?;
        let stored = tx.query_row(
            "SELECT operation_id, idempotency_key, repository_root, worktree_path,
                    branch_name, revision, workspace_id, surface_id, session_id,
                    owner_kind, owner_id, ownership_json, request_json, state,
                    error_code, error_message, recovery_json, recovery_attempts,
                    last_recovery_at, created_at, updated_at, completed_at, rolled_back_at
             FROM worktree_operations
             WHERE idempotency_key = ?1",
            [operation.idempotency_key.as_str()],
            worktree_operation_from_row,
        )?;
        tx.commit()?;
        Ok(stored)
    }

    pub fn load_worktree_operation(
        &self,
        operation_id: &str,
    ) -> StoreResult<Option<PersistedWorktreeOperation>> {
        self.connection
            .query_row(
                "SELECT operation_id, idempotency_key, repository_root, worktree_path,
                        branch_name, revision, workspace_id, surface_id, session_id,
                        owner_kind, owner_id, ownership_json, request_json, state,
                        error_code, error_message, recovery_json, recovery_attempts,
                        last_recovery_at, created_at, updated_at, completed_at, rolled_back_at
                 FROM worktree_operations
                 WHERE operation_id = ?1",
                [operation_id],
                worktree_operation_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn load_worktree_operation_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> StoreResult<Option<PersistedWorktreeOperation>> {
        self.connection
            .query_row(
                "SELECT operation_id, idempotency_key, repository_root, worktree_path,
                        branch_name, revision, workspace_id, surface_id, session_id,
                        owner_kind, owner_id, ownership_json, request_json, state,
                        error_code, error_message, recovery_json, recovery_attempts,
                        last_recovery_at, created_at, updated_at, completed_at, rolled_back_at
                 FROM worktree_operations
                 WHERE idempotency_key = ?1",
                [idempotency_key],
                worktree_operation_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn transition_worktree_operation(
        &mut self,
        operation_id: &str,
        next_state: WorktreeOperationState,
        error_code: Option<&str>,
        error_message: Option<&str>,
        updated_at: &str,
    ) -> StoreResult<PersistedWorktreeOperation> {
        let current = self
            .load_worktree_operation(operation_id)?
            .ok_or_else(|| StoreError::WorktreeOperationNotFound(operation_id.to_string()))?;
        if !current.state.can_transition_to(next_state) {
            return Err(StoreError::InvalidWorktreeOperationTransition {
                from: current.state.as_str().to_string(),
                to: next_state.as_str().to_string(),
            });
        }

        self.connection.execute(
            "UPDATE worktree_operations
             SET state = ?2,
                 error_code = COALESCE(?3, error_code),
                 error_message = COALESCE(?4, error_message),
                 updated_at = ?5,
                 completed_at = CASE
                   WHEN ?2 = 'completed' THEN COALESCE(completed_at, ?5)
                   ELSE completed_at
                 END,
                 rolled_back_at = CASE
                   WHEN ?2 = 'rolled_back' THEN COALESCE(rolled_back_at, ?5)
                   ELSE rolled_back_at
                 END
             WHERE operation_id = ?1",
            params![
                operation_id,
                next_state.as_str(),
                error_code,
                error_message,
                updated_at
            ],
        )?;

        self.load_worktree_operation(operation_id)?
            .ok_or_else(|| StoreError::WorktreeOperationNotFound(operation_id.to_string()))
    }

    pub fn update_worktree_operation_resources(
        &mut self,
        operation_id: &str,
        workspace_id: Option<&str>,
        surface_id: Option<&str>,
        session_id: Option<&str>,
        ownership_json: Option<&str>,
        updated_at: &str,
    ) -> StoreResult<PersistedWorktreeOperation> {
        let updated = self.connection.execute(
            "UPDATE worktree_operations
             SET workspace_id = COALESCE(?2, workspace_id),
                 surface_id = COALESCE(?3, surface_id),
                 session_id = COALESCE(?4, session_id),
                 ownership_json = COALESCE(?5, ownership_json),
                 updated_at = ?6
             WHERE operation_id = ?1",
            params![
                operation_id,
                workspace_id,
                surface_id,
                session_id,
                ownership_json,
                updated_at
            ],
        )?;
        if updated == 0 {
            return Err(StoreError::WorktreeOperationNotFound(
                operation_id.to_string(),
            ));
        }
        self.load_worktree_operation(operation_id)?
            .ok_or_else(|| StoreError::WorktreeOperationNotFound(operation_id.to_string()))
    }

    pub fn mark_worktree_operation_removed(
        &mut self,
        operation_id: &str,
        ownership_json: &str,
        updated_at: &str,
    ) -> StoreResult<PersistedWorktreeOperation> {
        let current = self
            .load_worktree_operation(operation_id)?
            .ok_or_else(|| StoreError::WorktreeOperationNotFound(operation_id.to_string()))?;
        if !current
            .state
            .can_transition_to(WorktreeOperationState::Removed)
        {
            return Err(StoreError::InvalidWorktreeOperationTransition {
                from: current.state.as_str().to_string(),
                to: WorktreeOperationState::Removed.as_str().to_string(),
            });
        }
        self.connection.execute(
            "UPDATE worktree_operations
             SET state = 'removed',
                 workspace_id = NULL,
                 surface_id = NULL,
                 session_id = NULL,
                 ownership_json = ?2,
                 error_code = NULL,
                 error_message = NULL,
                 updated_at = ?3
             WHERE operation_id = ?1",
            params![operation_id, ownership_json, updated_at],
        )?;
        self.load_worktree_operation(operation_id)?
            .ok_or_else(|| StoreError::WorktreeOperationNotFound(operation_id.to_string()))
    }

    pub fn restart_removed_worktree_operation(
        &mut self,
        operation_id: &str,
        ownership_json: &str,
        restarted_at: &str,
    ) -> StoreResult<PersistedWorktreeOperation> {
        let current = self
            .load_worktree_operation(operation_id)?
            .ok_or_else(|| StoreError::WorktreeOperationNotFound(operation_id.to_string()))?;
        if current.state != WorktreeOperationState::Removed {
            return Err(StoreError::InvalidWorktreeOperationTransition {
                from: current.state.as_str().to_string(),
                to: WorktreeOperationState::Prepared.as_str().to_string(),
            });
        }
        self.connection.execute(
            "UPDATE worktree_operations
             SET state = 'prepared',
                 workspace_id = NULL,
                 surface_id = NULL,
                 session_id = NULL,
                 ownership_json = ?2,
                 error_code = NULL,
                 error_message = NULL,
                 recovery_json = '{}',
                 recovery_attempts = 0,
                 last_recovery_at = NULL,
                 created_at = ?3,
                 updated_at = ?3,
                 completed_at = NULL,
                 rolled_back_at = NULL
             WHERE operation_id = ?1",
            params![operation_id, ownership_json, restarted_at],
        )?;
        self.load_worktree_operation(operation_id)?
            .ok_or_else(|| StoreError::WorktreeOperationNotFound(operation_id.to_string()))
    }

    pub fn record_worktree_operation_recovery(
        &mut self,
        operation_id: &str,
        recovery_json: &str,
        error_code: Option<&str>,
        error_message: Option<&str>,
        recovered_at: &str,
    ) -> StoreResult<bool> {
        let updated = self.connection.execute(
            "UPDATE worktree_operations
             SET recovery_json = ?2,
                 recovery_attempts = recovery_attempts + 1,
                 last_recovery_at = ?3,
                 error_code = ?4,
                 error_message = ?5,
                 updated_at = ?3
             WHERE operation_id = ?1",
            params![
                operation_id,
                recovery_json,
                recovered_at,
                error_code,
                error_message
            ],
        )?;
        Ok(updated > 0)
    }

    pub fn list_recoverable_worktree_operations(
        &self,
    ) -> StoreResult<Vec<PersistedWorktreeOperation>> {
        let mut statement = self.connection.prepare(
            "SELECT operation_id, idempotency_key, repository_root, worktree_path,
                    branch_name, revision, workspace_id, surface_id, session_id,
                    owner_kind, owner_id, ownership_json, request_json, state,
                    error_code, error_message, recovery_json, recovery_attempts,
                    last_recovery_at, created_at, updated_at, completed_at, rolled_back_at
             FROM worktree_operations
             WHERE state NOT IN ('completed', 'rolled_back', 'removed')
             ORDER BY updated_at ASC, operation_id ASC",
        )?;
        let rows = statement.query_map([], worktree_operation_from_row)?;
        collect_rows(rows)
    }

    pub fn upsert_git_review_thread(
        &mut self,
        thread: &PersistedGitReviewThread,
    ) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO git_review_threads (
                thread_id, repository_root, workspace_id, diff_identity, path, hunk_id,
                side, line_number, line_anchor, stale, stale_reason, resolved_at,
                author_id, target_kind, target_id, delivery_state, delivery_error,
                created_at, updated_at
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19
             ) ON CONFLICT(thread_id) DO UPDATE SET
                repository_root = excluded.repository_root,
                workspace_id = excluded.workspace_id,
                diff_identity = excluded.diff_identity,
                path = excluded.path,
                hunk_id = excluded.hunk_id,
                side = excluded.side,
                line_number = excluded.line_number,
                line_anchor = excluded.line_anchor,
                stale = excluded.stale,
                stale_reason = excluded.stale_reason,
                resolved_at = excluded.resolved_at,
                author_id = excluded.author_id,
                target_kind = excluded.target_kind,
                target_id = excluded.target_id,
                delivery_state = excluded.delivery_state,
                delivery_error = excluded.delivery_error,
                updated_at = excluded.updated_at",
            params![
                thread.thread_id,
                thread.repository_root,
                thread.workspace_id,
                thread.diff_identity,
                thread.path,
                thread.hunk_id,
                thread.side,
                thread.line_number,
                thread.line_anchor,
                thread.stale,
                thread.stale_reason,
                thread.resolved_at,
                thread.author_id,
                thread.target_kind,
                thread.target_id,
                thread.delivery_state,
                thread.delivery_error,
                thread.created_at,
                thread.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn load_git_review_thread(
        &self,
        thread_id: &str,
    ) -> StoreResult<Option<PersistedGitReviewThread>> {
        self.connection
            .query_row(
                "SELECT thread_id, repository_root, workspace_id, diff_identity, path, hunk_id,
                        side, line_number, line_anchor, stale, stale_reason, resolved_at,
                        author_id, target_kind, target_id, delivery_state, delivery_error,
                        created_at, updated_at
                 FROM git_review_threads
                 WHERE thread_id = ?1",
                [thread_id],
                git_review_thread_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn delete_git_review_thread(&mut self, thread_id: &str) -> StoreResult<bool> {
        let tx = self.connection.transaction()?;
        tx.execute(
            "DELETE FROM git_review_comments WHERE thread_id = ?1",
            [thread_id],
        )?;
        let deleted = tx.execute(
            "DELETE FROM git_review_threads WHERE thread_id = ?1",
            [thread_id],
        )?;
        tx.commit()?;
        Ok(deleted > 0)
    }

    pub fn list_git_review_threads(
        &self,
        repository_root: &str,
        workspace_id: Option<&str>,
        include_stale: bool,
    ) -> StoreResult<Vec<PersistedGitReviewThread>> {
        let mut sql = String::from(
            "SELECT thread_id, repository_root, workspace_id, diff_identity, path, hunk_id,
                    side, line_number, line_anchor, stale, stale_reason, resolved_at,
                    author_id, target_kind, target_id, delivery_state, delivery_error,
                    created_at, updated_at
             FROM git_review_threads
             WHERE repository_root = ?",
        );
        let mut values = vec![repository_root];
        if let Some(workspace_id) = workspace_id {
            sql.push_str(" AND workspace_id = ?");
            values.push(workspace_id);
        }
        if !include_stale {
            sql.push_str(" AND stale = 0");
        }
        sql.push_str(" ORDER BY updated_at DESC, thread_id DESC");
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values), git_review_thread_from_row)?;
        collect_rows(rows)
    }

    /// Marks only anchors that no longer match the currently displayed diff as
    /// stale; callers can retain stale comments for audit and manual rebasing.
    pub fn mark_git_review_threads_stale_except_diff(
        &mut self,
        repository_root: &str,
        current_diff_identity: &str,
        stale_reason: &str,
        updated_at: &str,
    ) -> StoreResult<usize> {
        self.connection
            .execute(
                "UPDATE git_review_threads
                 SET stale = 1,
                     stale_reason = ?3,
                     updated_at = ?4
                 WHERE repository_root = ?1
                   AND diff_identity <> ?2
                   AND stale = 0",
                params![
                    repository_root,
                    current_diff_identity,
                    stale_reason,
                    updated_at
                ],
            )
            .map_err(StoreError::from)
    }

    pub fn update_git_review_thread_delivery(
        &mut self,
        thread_id: &str,
        delivery: &GitReviewDeliveryUpdate,
    ) -> StoreResult<bool> {
        let updated = self.connection.execute(
            "UPDATE git_review_threads
             SET target_kind = ?2,
                 target_id = ?3,
                 delivery_state = ?4,
                 delivery_error = ?5,
                 updated_at = ?6
             WHERE thread_id = ?1",
            params![
                thread_id,
                delivery.target_kind,
                delivery.target_id,
                delivery.delivery_state,
                delivery.delivery_error,
                delivery.updated_at
            ],
        )?;
        Ok(updated > 0)
    }

    pub fn update_git_review_delivery_batch(
        &mut self,
        thread_id: &str,
        delivery: &GitReviewDeliveryUpdate,
    ) -> StoreResult<bool> {
        let tx = self.connection.transaction()?;
        let updated = tx.execute(
            "UPDATE git_review_threads
             SET target_kind = ?2,
                 target_id = ?3,
                 delivery_state = ?4,
                 delivery_error = ?5,
                 updated_at = ?6
             WHERE thread_id = ?1",
            params![
                thread_id,
                delivery.target_kind,
                delivery.target_id,
                delivery.delivery_state,
                delivery.delivery_error,
                delivery.updated_at
            ],
        )?;
        if updated == 0 {
            return Ok(false);
        }
        tx.execute(
            "UPDATE git_review_comments
             SET target_kind = ?2,
                 target_id = ?3,
                 delivery_state = ?4,
                 delivery_error = ?5,
                 delivered_at = ?6,
                 updated_at = ?7
             WHERE thread_id = ?1",
            params![
                thread_id,
                delivery.target_kind,
                delivery.target_id,
                delivery.delivery_state,
                delivery.delivery_error,
                delivery.delivered_at,
                delivery.updated_at
            ],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn load_latest_git_review_delivery_attempt(
        &self,
        thread_id: &str,
        payload_hash: &str,
        target_kind: &str,
        target_id: &str,
    ) -> StoreResult<Option<PersistedGitReviewDeliveryAttempt>> {
        self.connection
            .query_row(
                "SELECT attempt_id, thread_id, payload_hash, target_kind, target_id,
                        attempt_number, state, error_message, created_at, updated_at, completed_at
                 FROM git_review_delivery_attempts
                 WHERE thread_id = ?1 AND payload_hash = ?2
                   AND target_kind = ?3 AND target_id = ?4
                 ORDER BY attempt_number DESC
                 LIMIT 1",
                params![thread_id, payload_hash, target_kind, target_id],
                git_review_delivery_attempt_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn load_latest_git_review_delivery_attempt_for_target(
        &self,
        thread_id: &str,
        target_kind: &str,
        target_id: &str,
    ) -> StoreResult<Option<PersistedGitReviewDeliveryAttempt>> {
        self.connection
            .query_row(
                "SELECT attempt_id, thread_id, payload_hash, target_kind, target_id,
                        attempt_number, state, error_message, created_at, updated_at, completed_at
                 FROM git_review_delivery_attempts
                 WHERE thread_id = ?1 AND target_kind = ?2 AND target_id = ?3
                 ORDER BY attempt_number DESC, created_at DESC
                 LIMIT 1",
                params![thread_id, target_kind, target_id],
                git_review_delivery_attempt_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn begin_git_review_delivery_attempt(
        &mut self,
        attempt: &PersistedGitReviewDeliveryAttempt,
        delivery: &GitReviewDeliveryUpdate,
    ) -> StoreResult<bool> {
        let tx = self.connection.transaction()?;
        tx.execute(
            "INSERT INTO git_review_delivery_attempts (
                attempt_id, thread_id, payload_hash, target_kind, target_id,
                attempt_number, state, error_message, created_at, updated_at, completed_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                attempt.attempt_id,
                attempt.thread_id,
                attempt.payload_hash,
                attempt.target_kind,
                attempt.target_id,
                attempt.attempt_number,
                attempt.state,
                attempt.error_message,
                attempt.created_at,
                attempt.updated_at,
                attempt.completed_at
            ],
        )?;
        if !update_git_review_delivery_batch_in_transaction(&tx, &attempt.thread_id, delivery)? {
            return Ok(false);
        }
        tx.commit()?;
        Ok(true)
    }

    pub fn update_git_review_delivery_attempt(
        &mut self,
        attempt_id: &str,
        expected_state: &str,
        next_state: &str,
        error_message: Option<&str>,
        completed_at: Option<&str>,
        delivery: &GitReviewDeliveryUpdate,
    ) -> StoreResult<bool> {
        let tx = self.connection.transaction()?;
        let updated = tx.execute(
            "UPDATE git_review_delivery_attempts
             SET state = ?3,
                 error_message = ?4,
                 updated_at = ?5,
                 completed_at = ?6
             WHERE attempt_id = ?1 AND state = ?2",
            params![
                attempt_id,
                expected_state,
                next_state,
                error_message,
                delivery.updated_at,
                completed_at
            ],
        )?;
        if updated == 0 {
            return Ok(false);
        }
        let thread_id: String = tx.query_row(
            "SELECT thread_id FROM git_review_delivery_attempts WHERE attempt_id = ?1",
            [attempt_id],
            |row| row.get(0),
        )?;
        if !update_git_review_delivery_batch_in_transaction(&tx, &thread_id, delivery)? {
            return Ok(false);
        }
        tx.commit()?;
        Ok(true)
    }

    pub fn upsert_verified_agent_hook(
        &mut self,
        hook: &PersistedVerifiedAgentHook,
    ) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO verified_agent_hooks (
                session_id, workspace_id, source, sequence, state, reason,
                telemetry_json, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(session_id) DO UPDATE SET
                workspace_id = excluded.workspace_id,
                source = excluded.source,
                sequence = excluded.sequence,
                state = excluded.state,
                reason = excluded.reason,
                telemetry_json = excluded.telemetry_json,
                updated_at = excluded.updated_at",
            params![
                hook.session_id,
                hook.workspace_id,
                hook.source,
                i64::try_from(hook.sequence).unwrap_or(i64::MAX),
                hook.state,
                hook.reason,
                hook.telemetry_json,
                hook.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn load_verified_agent_hook(
        &self,
        session_id: &str,
    ) -> StoreResult<Option<PersistedVerifiedAgentHook>> {
        self.connection
            .query_row(
                "SELECT session_id, workspace_id, source, sequence, state, reason,
                        telemetry_json, updated_at
                 FROM verified_agent_hooks
                 WHERE session_id = ?1",
                [session_id],
                verified_agent_hook_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn list_verified_agent_hooks(&self) -> StoreResult<Vec<PersistedVerifiedAgentHook>> {
        let mut statement = self.connection.prepare(
            "SELECT session_id, workspace_id, source, sequence, state, reason,
                    telemetry_json, updated_at
             FROM verified_agent_hooks
             ORDER BY updated_at ASC, session_id ASC",
        )?;
        let rows = statement.query_map([], verified_agent_hook_from_row)?;
        collect_rows(rows)
    }

    pub fn delete_verified_agent_hook(&mut self, session_id: &str) -> StoreResult<bool> {
        Ok(self.connection.execute(
            "DELETE FROM verified_agent_hooks WHERE session_id = ?1",
            [session_id],
        )? > 0)
    }

    pub fn upsert_git_review_comment(
        &mut self,
        comment: &PersistedGitReviewComment,
    ) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO git_review_comments (
                comment_id, thread_id, author_id, body, target_kind, target_id,
                delivery_state, delivery_error, delivered_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(comment_id) DO UPDATE SET
                thread_id = excluded.thread_id,
                author_id = excluded.author_id,
                body = excluded.body,
                target_kind = excluded.target_kind,
                target_id = excluded.target_id,
                delivery_state = excluded.delivery_state,
                delivery_error = excluded.delivery_error,
                delivered_at = excluded.delivered_at,
                updated_at = excluded.updated_at",
            params![
                comment.comment_id,
                comment.thread_id,
                comment.author_id,
                comment.body,
                comment.target_kind,
                comment.target_id,
                comment.delivery_state,
                comment.delivery_error,
                comment.delivered_at,
                comment.created_at,
                comment.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_git_review_comments(
        &self,
        thread_id: &str,
    ) -> StoreResult<Vec<PersistedGitReviewComment>> {
        let mut statement = self.connection.prepare(
            "SELECT comment_id, thread_id, author_id, body, target_kind, target_id,
                    delivery_state, delivery_error, delivered_at, created_at, updated_at
             FROM git_review_comments
             WHERE thread_id = ?1
             ORDER BY created_at ASC, comment_id ASC",
        )?;
        let rows = statement.query_map([thread_id], git_review_comment_from_row)?;
        collect_rows(rows)
    }

    pub fn load_git_review_comment(
        &self,
        comment_id: &str,
    ) -> StoreResult<Option<PersistedGitReviewComment>> {
        self.connection
            .query_row(
                "SELECT comment_id, thread_id, author_id, body, target_kind, target_id,
                        delivery_state, delivery_error, delivered_at, created_at, updated_at
                 FROM git_review_comments
                 WHERE comment_id = ?1",
                [comment_id],
                git_review_comment_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn delete_git_review_comment(&mut self, comment_id: &str) -> StoreResult<bool> {
        let deleted = self.connection.execute(
            "DELETE FROM git_review_comments WHERE comment_id = ?1",
            [comment_id],
        )?;
        Ok(deleted > 0)
    }

    pub fn update_git_review_comment_delivery(
        &mut self,
        comment_id: &str,
        delivery: &GitReviewDeliveryUpdate,
    ) -> StoreResult<bool> {
        let updated = self.connection.execute(
            "UPDATE git_review_comments
             SET target_kind = ?2,
                 target_id = ?3,
                 delivery_state = ?4,
                 delivery_error = ?5,
                 delivered_at = ?6,
                 updated_at = ?7
             WHERE comment_id = ?1",
            params![
                comment_id,
                delivery.target_kind,
                delivery.target_id,
                delivery.delivery_state,
                delivery.delivery_error,
                delivery.delivered_at,
                delivery.updated_at
            ],
        )?;
        Ok(updated > 0)
    }

    pub fn upsert_sidebar_status(&mut self, status: &PersistedSidebarStatus) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO sidebar_status (
                workspace_id, key, label, icon, color, priority, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(workspace_id, key) DO UPDATE SET
                label = excluded.label,
                icon = excluded.icon,
                color = excluded.color,
                priority = excluded.priority,
                updated_at = excluded.updated_at",
            params![
                status.workspace_id,
                status.key,
                status.label,
                status.icon,
                status.color,
                status.priority,
                status.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn delete_sidebar_status(&mut self, workspace_id: &str, key: &str) -> StoreResult<bool> {
        let deleted = self.connection.execute(
            "DELETE FROM sidebar_status WHERE workspace_id = ?1 AND key = ?2",
            params![workspace_id, key],
        )?;
        Ok(deleted > 0)
    }

    pub fn list_sidebar_status(
        &self,
        workspace_id: &str,
    ) -> StoreResult<Vec<PersistedSidebarStatus>> {
        let mut statement = self.connection.prepare(
            "SELECT workspace_id, key, label, icon, color, priority, updated_at
             FROM sidebar_status
             WHERE workspace_id = ?1
             ORDER BY priority DESC, updated_at DESC, key ASC",
        )?;
        let rows = statement.query_map([workspace_id], sidebar_status_from_row)?;
        collect_rows(rows)
    }

    pub fn upsert_sidebar_progress(
        &mut self,
        progress: &PersistedSidebarProgress,
    ) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO sidebar_progress (workspace_id, value, label, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(workspace_id) DO UPDATE SET
                value = excluded.value,
                label = excluded.label,
                updated_at = excluded.updated_at",
            params![
                progress.workspace_id,
                progress.value,
                progress.label,
                progress.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn load_sidebar_progress(
        &self,
        workspace_id: &str,
    ) -> StoreResult<Option<PersistedSidebarProgress>> {
        self.connection
            .query_row(
                "SELECT workspace_id, value, label, updated_at
                 FROM sidebar_progress
                 WHERE workspace_id = ?1",
                [workspace_id],
                sidebar_progress_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn delete_sidebar_progress(&mut self, workspace_id: &str) -> StoreResult<bool> {
        let deleted = self.connection.execute(
            "DELETE FROM sidebar_progress WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        Ok(deleted > 0)
    }

    pub fn append_sidebar_log(&mut self, log: &PersistedSidebarLog) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO sidebar_logs (log_id, workspace_id, level, source, message, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                log.log_id,
                log.workspace_id,
                log.level,
                log.source,
                log.message,
                log.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_sidebar_logs(
        &self,
        workspace_id: &str,
        limit: Option<usize>,
    ) -> StoreResult<Vec<PersistedSidebarLog>> {
        let limit = limit.unwrap_or(20).clamp(1, 200);
        let mut statement = self.connection.prepare(
            "SELECT log_id, workspace_id, level, source, message, created_at
             FROM sidebar_logs
             WHERE workspace_id = ?1
             ORDER BY created_at DESC, log_id DESC
             LIMIT ?2",
        )?;
        let rows =
            statement.query_map(params![workspace_id, limit as i64], sidebar_log_from_row)?;
        collect_rows(rows)
    }

    pub fn clear_sidebar_logs(&mut self, workspace_id: &str) -> StoreResult<usize> {
        self.connection
            .execute(
                "DELETE FROM sidebar_logs WHERE workspace_id = ?1",
                params![workspace_id],
            )
            .map_err(StoreError::from)
    }

    pub fn upsert_profile(&mut self, profile: &PersistedProfile) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO ssh_profiles (
                profile_id, name, host, user, port, created_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(profile_id) DO UPDATE SET
                name = excluded.name,
                host = excluded.host,
                user = excluded.user,
                port = excluded.port,
                updated_at = excluded.updated_at",
            params![
                profile.profile_id,
                profile.name,
                profile.host,
                profile.user,
                profile.port,
                profile.created_at,
                profile.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn list_profiles(&self) -> StoreResult<Vec<PersistedProfile>> {
        let mut statement = self.connection.prepare(
            "SELECT profile_id, name, host, user, port, created_at, updated_at
             FROM ssh_profiles
             ORDER BY created_at ASC, profile_id ASC",
        )?;
        let rows = statement.query_map([], profile_from_row)?;
        collect_rows(rows)
    }

    pub fn load_profile(&self, profile_id: &str) -> StoreResult<Option<PersistedProfile>> {
        self.connection
            .query_row(
                "SELECT profile_id, name, host, user, port, created_at, updated_at
                 FROM ssh_profiles
                 WHERE profile_id = ?1",
                [profile_id],
                profile_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn delete_profile(&mut self, profile_id: &str) -> StoreResult<bool> {
        let deleted = self.connection.execute(
            "DELETE FROM ssh_profiles WHERE profile_id = ?1",
            params![profile_id],
        )?;
        Ok(deleted > 0)
    }

    fn list_panes_for_workspace(&self, workspace_id: &str) -> StoreResult<Vec<PersistedPane>> {
        let mut statement = self.connection.prepare(
            "SELECT pane_id, workspace_id, parent_pane_id, kind, split_axis, split_ratio,
                    mounted_surface_id, last_focused_at, created_at, updated_at
             FROM panes
             WHERE workspace_id = ?1
             ORDER BY created_at ASC, pane_id ASC",
        )?;
        let rows = statement.query_map([workspace_id], pane_from_row)?;
        collect_rows(rows)
    }

    fn list_surfaces_for_workspace(
        &self,
        workspace_id: &str,
    ) -> StoreResult<Vec<PersistedSurface>> {
        let mut statement = self.connection.prepare(
            "SELECT surface_id, workspace_id, surface_type, title, session_id, browser_id,
                    resource_uri, created_at, last_visible_at, updated_at
             FROM surfaces
             WHERE workspace_id = ?1
             ORDER BY created_at ASC, surface_id ASC",
        )?;
        let rows = statement.query_map([workspace_id], surface_from_row)?;
        collect_rows(rows)
    }

    fn list_sessions_for_workspace(
        &self,
        workspace_id: &str,
    ) -> StoreResult<Vec<PersistedSession>> {
        let mut statement = self.connection.prepare(
            "SELECT session_id, workspace_id, backend_kind, backend_attachment_id,
                    backend_native_id, cwd, command_json, state, exit_code, durability,
                    created_at, last_seen_at, updated_at
             FROM sessions
             WHERE workspace_id = ?1
             ORDER BY created_at ASC, session_id ASC",
        )?;
        let rows = statement.query_map([workspace_id], session_from_row)?;
        collect_rows(rows)
    }

    fn list_all_panes(&self) -> StoreResult<Vec<PersistedPane>> {
        let mut statement = self.connection.prepare(
            "SELECT pane_id, workspace_id, parent_pane_id, kind, split_axis, split_ratio,
                    mounted_surface_id, last_focused_at, created_at, updated_at
             FROM panes
             ORDER BY created_at ASC, pane_id ASC",
        )?;
        let rows = statement.query_map([], pane_from_row)?;
        collect_rows(rows)
    }

    fn list_all_surfaces(&self) -> StoreResult<Vec<PersistedSurface>> {
        let mut statement = self.connection.prepare(
            "SELECT surface_id, workspace_id, surface_type, title, session_id, browser_id,
                    resource_uri, created_at, last_visible_at, updated_at
             FROM surfaces
             ORDER BY created_at ASC, surface_id ASC",
        )?;
        let rows = statement.query_map([], surface_from_row)?;
        collect_rows(rows)
    }
}

fn initialize_connection(connection: Connection) -> StoreResult<SqliteStore> {
    connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    apply_migrations(&connection, MIGRATIONS)?;
    Ok(SqliteStore { connection })
}

pub fn apply_migrations(connection: &Connection, migrations: &[Migration]) -> StoreResult<()> {
    if !migrations_are_ordered(migrations) {
        return Err(StoreError::InvalidMigrationOrder);
    }

    if migrations.is_empty() {
        let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
        transaction.execute_batch(SCHEMA_MIGRATIONS_SQL)?;
        transaction.commit()?;
        return Ok(());
    }

    for migration in migrations {
        // Keep the schema change and its ledger entry indivisible. SQLite DDL participates in an
        // explicit transaction, so a failed rename/copy/drop migration rolls back to the schema
        // that existed before this migration began. PRAGMAs are intentionally configured before
        // this function opens a transaction in `initialize_connection`: SQLite does not allow
        // changing `foreign_keys` from inside an active transaction.
        let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
        transaction.execute_batch(SCHEMA_MIGRATIONS_SQL)?;
        let already_applied = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
            [migration.version],
            |row| row.get::<_, bool>(0),
        )?;

        if !already_applied {
            transaction.execute_batch(migration.sql)?;
            transaction.execute(
                "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
                params![migration.version, migration.name],
            )?;
        }

        transaction.commit()?;
    }

    Ok(())
}

fn delete_missing_workspace_rows(
    transaction: &Connection,
    table: &str,
    id_column: &str,
    workspace_id: &str,
    retained_ids: &[&str],
) -> StoreResult<()> {
    if retained_ids.is_empty() {
        transaction.execute(
            &format!("DELETE FROM {table} WHERE workspace_id = ?1"),
            params![workspace_id],
        )?;
        return Ok(());
    }

    let placeholders = (0..retained_ids.len())
        .map(|index| format!("?{}", index + 2))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "DELETE FROM {table} WHERE workspace_id = ?1 AND {id_column} NOT IN ({placeholders})"
    );
    let values = std::iter::once(workspace_id).chain(retained_ids.iter().copied());
    transaction.execute(&sql, params_from_iter(values))?;
    Ok(())
}

pub fn migrations_are_ordered(migrations: &[Migration]) -> bool {
    migrations
        .windows(2)
        .all(|pair| pair[0].version < pair[1].version)
}

pub fn redact_env_key(key: &str) -> bool {
    let normalized = key.to_ascii_uppercase();
    normalized.contains("TOKEN")
        || normalized.contains("SECRET")
        || normalized.contains("PASSWORD")
        || normalized.ends_with("_KEY")
}

pub fn redact_env_pairs(pairs: &[(String, String)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(key, value)| {
            if redact_env_key(key) {
                (key.clone(), REDACTED_VALUE.to_string())
            } else {
                (key.clone(), value.clone())
            }
        })
        .collect()
}

pub fn redacted_env_json(pairs: &[(String, String)]) -> StoreResult<String> {
    serde_json::to_string(&redact_env_pairs(pairs)).map_err(StoreError::from)
}

pub fn recovery_state_for_session(state: &str, durability: &str) -> String {
    match state {
        "exited" | "failed" | "lost" => state.to_string(),
        _ if durability == "durable" => "recovering".to_string(),
        _ => "disconnected".to_string(),
    }
}

fn normalize_session_for_recovery(mut session: PersistedSession) -> PersistedSession {
    let state = recovery_state_for_session(&session.state, &session.durability);
    let active_non_durable = session.durability != "durable"
        && !matches!(session.state.as_str(), "exited" | "failed" | "lost");

    session.state = state;
    if active_non_durable {
        session.backend_attachment_id = None;
        session.backend_native_id = None;
    }
    session
}

fn save_workspace_bundle_in_transaction(
    connection: &Connection,
    bundle: &WorkspaceBundle,
) -> StoreResult<()> {
    upsert_workspace(connection, &bundle.workspace)?;
    delete_missing_workspace_rows(
        connection,
        "panes",
        "pane_id",
        &bundle.workspace.workspace_id,
        &bundle
            .panes
            .iter()
            .map(|pane| pane.pane_id.as_str())
            .collect::<Vec<_>>(),
    )?;
    delete_missing_workspace_rows(
        connection,
        "surfaces",
        "surface_id",
        &bundle.workspace.workspace_id,
        &bundle
            .surfaces
            .iter()
            .map(|surface| surface.surface_id.as_str())
            .collect::<Vec<_>>(),
    )?;
    delete_missing_workspace_rows(
        connection,
        "sessions",
        "session_id",
        &bundle.workspace.workspace_id,
        &bundle
            .sessions
            .iter()
            .map(|session| session.session_id.as_str())
            .collect::<Vec<_>>(),
    )?;
    delete_missing_workspace_rows(
        connection,
        "session_launch_specs",
        "session_id",
        &bundle.workspace.workspace_id,
        &bundle
            .sessions
            .iter()
            .map(|session| session.session_id.as_str())
            .collect::<Vec<_>>(),
    )?;
    delete_missing_workspace_rows(
        connection,
        "agent_states",
        "session_id",
        &bundle.workspace.workspace_id,
        &bundle
            .sessions
            .iter()
            .map(|session| session.session_id.as_str())
            .collect::<Vec<_>>(),
    )?;

    for pane in &bundle.panes {
        upsert_pane(connection, pane)?;
    }
    for surface in &bundle.surfaces {
        upsert_surface(connection, surface)?;
    }
    for session in &bundle.sessions {
        upsert_session(connection, session)?;
    }
    Ok(())
}

fn upsert_session_launch_spec(
    connection: &Connection,
    spec: &PersistedSessionLaunchSpec,
) -> StoreResult<()> {
    let env_json = serde_json::to_string(&spec.env)?;
    connection.execute(
        "INSERT INTO session_launch_specs (
            session_id, workspace_id, backend_profile, env_json, columns, rows, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(session_id) DO UPDATE SET
            workspace_id = excluded.workspace_id,
            backend_profile = excluded.backend_profile,
            env_json = excluded.env_json,
            columns = excluded.columns,
            rows = excluded.rows,
            updated_at = excluded.updated_at",
        params![
            spec.session_id,
            spec.workspace_id,
            spec.backend_profile,
            env_json,
            spec.columns,
            spec.rows,
            spec.updated_at
        ],
    )?;
    Ok(())
}

fn upsert_workspace(connection: &Connection, workspace: &PersistedWorkspace) -> StoreResult<()> {
    connection.execute(
        "INSERT INTO workspaces (
            workspace_id, name, root_pane_id, active_pane_id, project_root,
            environment_profile_id, description, icon, color,
            default_wsl_distribution, default_terminal_profile, default_agent_command, created_at, updated_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(workspace_id) DO UPDATE SET
            name = excluded.name,
            root_pane_id = excluded.root_pane_id,
            active_pane_id = excluded.active_pane_id,
            project_root = excluded.project_root,
            environment_profile_id = excluded.environment_profile_id,
            description = excluded.description,
            icon = excluded.icon,
            color = excluded.color,
            default_wsl_distribution = excluded.default_wsl_distribution,
            default_terminal_profile = excluded.default_terminal_profile,
            default_agent_command = excluded.default_agent_command,
            updated_at = excluded.updated_at",
        params![
            workspace.workspace_id,
            workspace.name,
            workspace.root_pane_id,
            workspace.active_pane_id,
            workspace.project_root,
            workspace.environment_profile_id,
            workspace.description,
            workspace.icon,
            workspace.color,
            workspace.default_wsl_distribution,
            workspace.default_terminal_profile,
            workspace.default_agent_command,
            workspace.created_at,
            workspace.updated_at
        ],
    )?;
    Ok(())
}

fn upsert_pane(connection: &Connection, pane: &PersistedPane) -> StoreResult<()> {
    connection.execute(
        "INSERT INTO panes (
            pane_id, workspace_id, parent_pane_id, kind, split_axis, split_ratio,
            mounted_surface_id, last_focused_at, created_at, updated_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(pane_id) DO UPDATE SET
            workspace_id = excluded.workspace_id,
            parent_pane_id = excluded.parent_pane_id,
            kind = excluded.kind,
            split_axis = excluded.split_axis,
            split_ratio = excluded.split_ratio,
            mounted_surface_id = excluded.mounted_surface_id,
            last_focused_at = excluded.last_focused_at,
            updated_at = excluded.updated_at",
        params![
            pane.pane_id,
            pane.workspace_id,
            pane.parent_pane_id,
            pane.kind,
            pane.split_axis,
            pane.split_ratio,
            pane.mounted_surface_id,
            pane.last_focused_at,
            pane.created_at,
            pane.updated_at
        ],
    )?;
    Ok(())
}

fn upsert_surface(connection: &Connection, surface: &PersistedSurface) -> StoreResult<()> {
    connection.execute(
        "INSERT INTO surfaces (
            surface_id, workspace_id, surface_type, title, session_id, browser_id,
            resource_uri, created_at, last_visible_at, updated_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(surface_id) DO UPDATE SET
            workspace_id = excluded.workspace_id,
            surface_type = excluded.surface_type,
            title = excluded.title,
            session_id = excluded.session_id,
            browser_id = excluded.browser_id,
            resource_uri = excluded.resource_uri,
            last_visible_at = excluded.last_visible_at,
            updated_at = excluded.updated_at",
        params![
            surface.surface_id,
            surface.workspace_id,
            surface.surface_type,
            surface.title,
            surface.session_id,
            surface.browser_id,
            surface.resource_uri,
            surface.created_at,
            surface.last_visible_at,
            surface.updated_at
        ],
    )?;
    Ok(())
}

fn upsert_session(connection: &Connection, session: &PersistedSession) -> StoreResult<()> {
    let command_json = serde_json::to_string(&session.command)?;
    connection.execute(
        "INSERT INTO sessions (
            session_id, workspace_id, backend_kind, backend_attachment_id,
            backend_native_id, cwd, command_json, state, exit_code, durability,
            created_at, last_seen_at, updated_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(session_id) DO UPDATE SET
            workspace_id = excluded.workspace_id,
            backend_kind = excluded.backend_kind,
            backend_attachment_id = excluded.backend_attachment_id,
            backend_native_id = excluded.backend_native_id,
            cwd = excluded.cwd,
            command_json = excluded.command_json,
            state = excluded.state,
            exit_code = excluded.exit_code,
            durability = excluded.durability,
            last_seen_at = excluded.last_seen_at,
            updated_at = excluded.updated_at",
        params![
            session.session_id,
            session.workspace_id,
            session.backend_kind,
            session.backend_attachment_id,
            session.backend_native_id,
            session.cwd,
            command_json,
            session.state,
            session.exit_code,
            session.durability,
            session.created_at,
            session.last_seen_at,
            session.updated_at
        ],
    )?;
    Ok(())
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> StoreResult<Vec<T>> {
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::from)
}

fn workspace_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedWorkspace> {
    Ok(PersistedWorkspace {
        workspace_id: row.get(0)?,
        name: row.get(1)?,
        root_pane_id: row.get(2)?,
        active_pane_id: row.get(3)?,
        project_root: row.get(4)?,
        environment_profile_id: row.get(5)?,
        description: row.get(6)?,
        icon: row.get(7)?,
        color: row.get(8)?,
        default_wsl_distribution: row.get(9)?,
        default_terminal_profile: row.get(10)?,
        default_agent_command: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn pane_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedPane> {
    Ok(PersistedPane {
        pane_id: row.get(0)?,
        workspace_id: row.get(1)?,
        parent_pane_id: row.get(2)?,
        kind: row.get(3)?,
        split_axis: row.get(4)?,
        split_ratio: row.get(5)?,
        mounted_surface_id: row.get(6)?,
        last_focused_at: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn surface_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedSurface> {
    Ok(PersistedSurface {
        surface_id: row.get(0)?,
        workspace_id: row.get(1)?,
        surface_type: row.get(2)?,
        title: row.get(3)?,
        session_id: row.get(4)?,
        browser_id: row.get(5)?,
        resource_uri: row.get(6)?,
        created_at: row.get(7)?,
        last_visible_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedSession> {
    let command_json: String = row.get(6)?;
    let command = serde_json::from_str(&command_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(error))
    })?;

    Ok(PersistedSession {
        session_id: row.get(0)?,
        workspace_id: row.get(1)?,
        backend_kind: row.get(2)?,
        backend_attachment_id: row.get(3)?,
        backend_native_id: row.get(4)?,
        cwd: row.get(5)?,
        command,
        state: row.get(7)?,
        exit_code: row.get(8)?,
        durability: row.get(9)?,
        created_at: row.get(10)?,
        last_seen_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn agent_state_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedAgentState> {
    Ok(PersistedAgentState {
        session_id: row.get(0)?,
        workspace_id: row.get(1)?,
        state: row.get(2)?,
        attention: row.get(3)?,
        reason: row.get(4)?,
        updated_at: row.get(5)?,
        telemetry_json: row.get(6)?,
    })
}

fn profile_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedProfile> {
    Ok(PersistedProfile {
        profile_id: row.get(0)?,
        name: row.get(1)?,
        host: row.get(2)?,
        user: row.get(3)?,
        port: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn notification_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedNotification> {
    Ok(PersistedNotification {
        notification_id: row.get(0)?,
        notification_type: row.get(1)?,
        severity: row.get(2)?,
        workspace_id: row.get(3)?,
        session_id: row.get(4)?,
        title: row.get(5)?,
        message: row.get(6)?,
        created_at: row.get(7)?,
        dismissed: row.get(8)?,
    })
}

fn sidebar_status_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedSidebarStatus> {
    Ok(PersistedSidebarStatus {
        workspace_id: row.get(0)?,
        key: row.get(1)?,
        label: row.get(2)?,
        icon: row.get(3)?,
        color: row.get(4)?,
        priority: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn sidebar_progress_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<PersistedSidebarProgress> {
    Ok(PersistedSidebarProgress {
        workspace_id: row.get(0)?,
        value: row.get(1)?,
        label: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

fn sidebar_log_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedSidebarLog> {
    Ok(PersistedSidebarLog {
        log_id: row.get(0)?,
        workspace_id: row.get(1)?,
        level: row.get(2)?,
        source: row.get(3)?,
        message: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn workspace_group_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedWorkspaceGroup> {
    Ok(PersistedWorkspaceGroup {
        group_id: row.get(0)?,
        name: row.get(1)?,
        anchor_workspace_id: row.get(2)?,
        collapsed: row.get(3)?,
        pinned: row.get(4)?,
        color: row.get(5)?,
        icon: row.get(6)?,
        sort_order: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn workspace_group_member_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<PersistedWorkspaceGroupMember> {
    Ok(PersistedWorkspaceGroupMember {
        group_id: row.get(0)?,
        workspace_id: row.get(1)?,
        position: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn dock_trust_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedDockTrust> {
    Ok(PersistedDockTrust {
        workspace_id: row.get(0)?,
        source: row.get(1)?,
        config_path: row.get(2)?,
        config_hash: row.get(3)?,
        trusted_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn team_task_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedTeamTask> {
    Ok(PersistedTeamTask {
        task_id: row.get(0)?,
        workspace_id: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        status: row.get(4)?,
        assigned_session_id: row.get(5)?,
        blocked_reason: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        completed_at: row.get(9)?,
    })
}

fn team_message_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedTeamMessage> {
    Ok(PersistedTeamMessage {
        message_id: row.get(0)?,
        workspace_id: row.get(1)?,
        thread_id: row.get(2)?,
        from_session_id: row.get(3)?,
        to_session_id: row.get(4)?,
        body: row.get(5)?,
        kind: row.get(6)?,
        created_at: row.get(7)?,
        read_at: row.get(8)?,
    })
}

fn update_git_review_delivery_batch_in_transaction(
    tx: &Transaction<'_>,
    thread_id: &str,
    delivery: &GitReviewDeliveryUpdate,
) -> rusqlite::Result<bool> {
    let updated = tx.execute(
        "UPDATE git_review_threads
         SET target_kind = ?2,
             target_id = ?3,
             delivery_state = ?4,
             delivery_error = ?5,
             updated_at = ?6
         WHERE thread_id = ?1",
        params![
            thread_id,
            delivery.target_kind,
            delivery.target_id,
            delivery.delivery_state,
            delivery.delivery_error,
            delivery.updated_at
        ],
    )?;
    if updated == 0 {
        return Ok(false);
    }
    tx.execute(
        "UPDATE git_review_comments
         SET target_kind = ?2,
             target_id = ?3,
             delivery_state = ?4,
             delivery_error = ?5,
             delivered_at = ?6,
             updated_at = ?7
         WHERE thread_id = ?1",
        params![
            thread_id,
            delivery.target_kind,
            delivery.target_id,
            delivery.delivery_state,
            delivery.delivery_error,
            delivery.delivered_at,
            delivery.updated_at
        ],
    )?;
    Ok(true)
}

fn worktree_operation_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<PersistedWorktreeOperation> {
    let state: String = row.get(13)?;
    let state = WorktreeOperationState::parse(&state).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            13,
            rusqlite::types::Type::Text,
            Box::new(StoreError::InvalidWorktreeOperationState(state)),
        )
    })?;

    Ok(PersistedWorktreeOperation {
        operation_id: row.get(0)?,
        idempotency_key: row.get(1)?,
        repository_root: row.get(2)?,
        worktree_path: row.get(3)?,
        branch_name: row.get(4)?,
        revision: row.get(5)?,
        workspace_id: row.get(6)?,
        surface_id: row.get(7)?,
        session_id: row.get(8)?,
        owner_kind: row.get(9)?,
        owner_id: row.get(10)?,
        ownership_json: row.get(11)?,
        request_json: row.get(12)?,
        state,
        error_code: row.get(14)?,
        error_message: row.get(15)?,
        recovery_json: row.get(16)?,
        recovery_attempts: row.get(17)?,
        last_recovery_at: row.get(18)?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
        completed_at: row.get(21)?,
        rolled_back_at: row.get(22)?,
    })
}

fn git_review_thread_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<PersistedGitReviewThread> {
    Ok(PersistedGitReviewThread {
        thread_id: row.get(0)?,
        repository_root: row.get(1)?,
        workspace_id: row.get(2)?,
        diff_identity: row.get(3)?,
        path: row.get(4)?,
        hunk_id: row.get(5)?,
        side: row.get(6)?,
        line_number: row.get(7)?,
        line_anchor: row.get(8)?,
        stale: row.get(9)?,
        stale_reason: row.get(10)?,
        resolved_at: row.get(11)?,
        author_id: row.get(12)?,
        target_kind: row.get(13)?,
        target_id: row.get(14)?,
        delivery_state: row.get(15)?,
        delivery_error: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

fn git_review_comment_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<PersistedGitReviewComment> {
    Ok(PersistedGitReviewComment {
        comment_id: row.get(0)?,
        thread_id: row.get(1)?,
        author_id: row.get(2)?,
        body: row.get(3)?,
        target_kind: row.get(4)?,
        target_id: row.get(5)?,
        delivery_state: row.get(6)?,
        delivery_error: row.get(7)?,
        delivered_at: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn git_review_delivery_attempt_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<PersistedGitReviewDeliveryAttempt> {
    Ok(PersistedGitReviewDeliveryAttempt {
        attempt_id: row.get(0)?,
        thread_id: row.get(1)?,
        payload_hash: row.get(2)?,
        target_kind: row.get(3)?,
        target_id: row.get(4)?,
        attempt_number: row.get(5)?,
        state: row.get(6)?,
        error_message: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        completed_at: row.get(10)?,
    })
}

fn verified_agent_hook_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<PersistedVerifiedAgentHook> {
    let sequence: i64 = row.get(3)?;
    Ok(PersistedVerifiedAgentHook {
        session_id: row.get(0)?,
        workspace_id: row.get(1)?,
        source: row.get(2)?,
        sequence: u64::try_from(sequence).unwrap_or_default(),
        state: row.get(4)?,
        reason: row.get(5)?,
        telemetry_json: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn load_git_mutation_record(
    connection: &Connection,
    method: &str,
    repository_id: &str,
    idempotency_key: &str,
) -> rusqlite::Result<Option<PersistedGitMutationRecord>> {
    connection
        .query_row(
            "SELECT method, repository_id, idempotency_key, request_fingerprint,
                    response_json, created_at, lifecycle_state,
                    precondition_fingerprint, failure_json
             FROM git_mutation_receipts
             WHERE method = ?1
               AND repository_id = ?2
               AND idempotency_key = ?3",
            params![method, repository_id, idempotency_key],
            git_mutation_record_from_row,
        )
        .optional()
}

fn git_mutation_record_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<PersistedGitMutationRecord> {
    Ok(PersistedGitMutationRecord {
        method: row.get(0)?,
        repository_id: row.get(1)?,
        idempotency_key: row.get(2)?,
        request_fingerprint: row.get(3)?,
        response_json: row.get(4)?,
        created_at: row.get(5)?,
        lifecycle_state: row.get(6)?,
        precondition_fingerprint: row.get(7)?,
        failure_json: row.get(8)?,
    })
}

fn git_mutation_receipt_lookup(
    stored: PersistedGitMutationRecord,
    request_fingerprint: &str,
) -> GitMutationReceiptLookup {
    if stored.request_fingerprint != request_fingerprint {
        return GitMutationReceiptLookup::FingerprintMismatch {
            stored_request_fingerprint: stored.request_fingerprint,
        };
    }
    let intent = PersistedGitMutationIntent {
        method: stored.method.clone(),
        repository_id: stored.repository_id.clone(),
        idempotency_key: stored.idempotency_key.clone(),
        request_fingerprint: stored.request_fingerprint.clone(),
        precondition_fingerprint: stored.precondition_fingerprint,
        created_at: stored.created_at.clone(),
    };
    match stored.lifecycle_state.as_str() {
        "pending" => GitMutationReceiptLookup::Pending(intent),
        "completed" => GitMutationReceiptLookup::Match(PersistedGitMutationReceipt {
            method: stored.method,
            repository_id: stored.repository_id,
            idempotency_key: stored.idempotency_key,
            request_fingerprint: stored.request_fingerprint,
            response_json: stored.response_json,
            created_at: stored.created_at,
        }),
        "indeterminate" => GitMutationReceiptLookup::Indeterminate {
            intent,
            failure_json: stored.failure_json,
        },
        _ => GitMutationReceiptLookup::Indeterminate {
            intent,
            failure_json: Some(r#"{"reason":"invalid_lifecycle_state"}"#.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn initial_migration_is_versioned() {
        assert_eq!(MIGRATIONS[0].version, 1);
        assert!(MIGRATIONS[0]
            .sql
            .contains("CREATE TABLE IF NOT EXISTS sessions"));
        assert_eq!(MIGRATIONS[1].version, 2);
        assert!(MIGRATIONS[1]
            .sql
            .contains("CREATE TABLE IF NOT EXISTS notifications"));
    }

    #[test]
    fn applies_migrations_and_records_schema_version() {
        let store = SqliteStore::in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), 17);
    }

    #[test]
    fn worktree_operations_and_git_review_schema_are_versioned() {
        let migration = &MIGRATIONS[11];
        assert_eq!(migration.version, 12);
        assert!(migration
            .sql
            .contains("CREATE TABLE IF NOT EXISTS worktree_operations"));
        assert!(migration
            .sql
            .contains("CREATE TABLE IF NOT EXISTS git_review_threads"));
        assert!(migration
            .sql
            .contains("CREATE TABLE IF NOT EXISTS git_review_comments"));
        let removed = &MIGRATIONS[12];
        assert_eq!(removed.version, 13);
        assert!(removed.sql.contains("'removed'"));
    }

    #[test]
    fn worktree_and_review_migration_upgrades_existing_database() {
        let connection = Connection::open_in_memory().unwrap();
        apply_migrations(&connection, &MIGRATIONS[..11]).unwrap();
        let before: i32 = connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(before, 11);

        apply_migrations(&connection, MIGRATIONS).unwrap();
        let after: i32 = connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(after, 17);
        let tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table'
                   AND name IN (
                     'worktree_operations',
                     'git_review_threads',
                     'git_review_comments',
                     'git_mutation_receipts'
                   )",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 4);
    }

    #[test]
    fn failed_migration_rolls_back_schema_and_ledger_together() {
        let connection = Connection::open_in_memory().unwrap();
        let invalid_migration = Migration {
            version: 1,
            name: "invalid_atomicity_probe",
            sql: r#"
CREATE TABLE migration_atomicity_probe (value TEXT NOT NULL);
INSERT INTO migration_atomicity_probe (value) VALUES ('before failure');
SELECT * FROM migration_atomicity_failure;
"#,
        };

        assert!(apply_migrations(&connection, &[invalid_migration]).is_err());

        let schema_ledger_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let probe_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'migration_atomicity_probe')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!schema_ledger_exists);
        assert!(!probe_exists);

        let retry = Migration {
            version: 1,
            name: "atomicity_probe",
            sql: "CREATE TABLE migration_atomicity_probe (value TEXT NOT NULL);",
        };
        apply_migrations(&connection, &[retry]).unwrap();
        let recorded_name: String = connection
            .query_row(
                "SELECT name FROM schema_migrations WHERE version = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(recorded_name, "atomicity_probe");
    }

    #[test]
    fn destructive_worktree_migration_rolls_back_and_is_retryable() {
        let connection = Connection::open_in_memory().unwrap();
        apply_migrations(&connection, &MIGRATIONS[..12]).unwrap();
        let mut store = SqliteStore { connection };
        store
            .create_or_load_worktree_operation(&sample_worktree_operation(
                "op_atomic_v12",
                "key_atomic_v12",
            ))
            .unwrap();

        let failing_sql = format!(
            "{WORKTREE_REMOVED_STATE_SCHEMA}\nSELECT * FROM worktree_migration_fault_injection;"
        );
        let failed_v13 = Migration {
            version: 13,
            name: "worktree_removed_state_fault_injection",
            sql: Box::leak(failing_sql.into_boxed_str()),
        };
        assert!(apply_migrations(&store.connection, &[failed_v13]).is_err());

        let schema_version: i32 = store
            .connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let backup_table_exists: bool = store
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'worktree_operations_v12')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let worktree_schema: String = store
            .connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'worktree_operations'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(schema_version, 12);
        assert!(!backup_table_exists);
        assert!(!worktree_schema.contains("'removed'"));
        assert_eq!(
            store
                .load_worktree_operation("op_atomic_v12")
                .unwrap()
                .unwrap()
                .state,
            WorktreeOperationState::Prepared
        );

        apply_migrations(&store.connection, MIGRATIONS).unwrap();
        assert_eq!(store.schema_version().unwrap(), 17);
        assert_eq!(
            store
                .load_worktree_operation("op_atomic_v12")
                .unwrap()
                .unwrap()
                .state,
            WorktreeOperationState::Prepared
        );
    }

    #[test]
    fn removed_state_migration_preserves_v12_operations() {
        let connection = Connection::open_in_memory().unwrap();
        apply_migrations(&connection, &MIGRATIONS[..12]).unwrap();
        let mut store = SqliteStore { connection };
        store
            .create_or_load_worktree_operation(&sample_worktree_operation("op_v12", "key_v12"))
            .unwrap();
        apply_migrations(&store.connection, MIGRATIONS).unwrap();
        let removed = store
            .mark_worktree_operation_removed("op_v12", r#"{"owned":false}"#, "removed")
            .unwrap();
        assert_eq!(removed.state, WorktreeOperationState::Removed);
        assert_eq!(store.schema_version().unwrap(), 17);
    }

    #[test]
    fn worktree_operations_are_idempotent_and_enforce_ordered_transitions() {
        let mut store = SqliteStore::in_memory().unwrap();
        let operation = sample_worktree_operation("op_1", "request_1");
        let inserted = store.create_or_load_worktree_operation(&operation).unwrap();
        assert_eq!(inserted.operation_id, "op_1");
        assert_eq!(inserted.state, WorktreeOperationState::Prepared);

        let mut retried = sample_worktree_operation("op_retry", "request_1");
        retried.repository_root = "D:\\other-repository".to_string();
        let reused = store.create_or_load_worktree_operation(&retried).unwrap();
        assert_eq!(reused.operation_id, "op_1");
        assert_eq!(reused.repository_root, "D:\\repo");

        let created = store
            .transition_worktree_operation(
                "op_1",
                WorktreeOperationState::WorktreeCreated,
                None,
                None,
                "2026-07-23T00:01:00Z",
            )
            .unwrap();
        assert_eq!(created.state, WorktreeOperationState::WorktreeCreated);
        let workspace = store
            .transition_worktree_operation(
                "op_1",
                WorktreeOperationState::WorkspaceCreated,
                None,
                None,
                "2026-07-23T00:02:00Z",
            )
            .unwrap();
        assert_eq!(workspace.state, WorktreeOperationState::WorkspaceCreated);
        let resources = store
            .update_worktree_operation_resources(
                "op_1",
                Some("ws_agent"),
                Some("surface_agent"),
                Some("session_agent"),
                Some(r#"{"owned":true}"#),
                "2026-07-23T00:02:30Z",
            )
            .unwrap();
        assert_eq!(resources.workspace_id.as_deref(), Some("ws_agent"));
        assert_eq!(resources.surface_id.as_deref(), Some("surface_agent"));
        assert_eq!(resources.session_id.as_deref(), Some("session_agent"));
        assert_eq!(resources.ownership_json, r#"{"owned":true}"#);
        store
            .transition_worktree_operation(
                "op_1",
                WorktreeOperationState::SessionCreated,
                None,
                None,
                "2026-07-23T00:03:00Z",
            )
            .unwrap();
        let completed = store
            .transition_worktree_operation(
                "op_1",
                WorktreeOperationState::Completed,
                None,
                None,
                "2026-07-23T00:04:00Z",
            )
            .unwrap();
        assert_eq!(
            completed.completed_at.as_deref(),
            Some("2026-07-23T00:04:00Z")
        );
    }

    #[test]
    fn worktree_operations_reject_illegal_transitions_and_list_recovery_work() {
        let mut store = SqliteStore::in_memory().unwrap();
        store
            .create_or_load_worktree_operation(&sample_worktree_operation("op_1", "request_1"))
            .unwrap();
        let error = store
            .transition_worktree_operation(
                "op_1",
                WorktreeOperationState::Completed,
                None,
                None,
                "2026-07-23T00:01:00Z",
            )
            .unwrap_err();
        assert!(matches!(
            error,
            StoreError::InvalidWorktreeOperationTransition { .. }
        ));

        store
            .transition_worktree_operation(
                "op_1",
                WorktreeOperationState::WorktreeCreated,
                None,
                None,
                "2026-07-23T00:02:00Z",
            )
            .unwrap();
        assert!(store
            .record_worktree_operation_recovery(
                "op_1",
                r#"{"retry":"attach"}"#,
                Some("spawn_timeout"),
                Some("session start timed out"),
                "2026-07-23T00:03:00Z",
            )
            .unwrap());

        store
            .create_or_load_worktree_operation(&sample_worktree_operation("op_2", "request_2"))
            .unwrap();
        for state in [
            WorktreeOperationState::WorktreeCreated,
            WorktreeOperationState::WorkspaceCreated,
            WorktreeOperationState::SessionCreated,
            WorktreeOperationState::Completed,
        ] {
            store
                .transition_worktree_operation("op_2", state, None, None, "2026-07-23T00:04:00Z")
                .unwrap();
        }

        let recoverable = store.list_recoverable_worktree_operations().unwrap();
        assert_eq!(recoverable.len(), 1);
        assert_eq!(recoverable[0].operation_id, "op_1");
        assert_eq!(recoverable[0].recovery_attempts, 1);
        assert_eq!(recoverable[0].error_code.as_deref(), Some("spawn_timeout"));

        store
            .create_or_load_worktree_operation(&sample_worktree_operation("op_3", "request_3"))
            .unwrap();
        store
            .transition_worktree_operation(
                "op_3",
                WorktreeOperationState::Failed,
                Some("git_failed"),
                Some("worktree creation failed"),
                "2026-07-23T00:05:00Z",
            )
            .unwrap();
        store
            .transition_worktree_operation(
                "op_3",
                WorktreeOperationState::RollingBack,
                None,
                None,
                "2026-07-23T00:06:00Z",
            )
            .unwrap();
        let rolled_back = store
            .transition_worktree_operation(
                "op_3",
                WorktreeOperationState::RolledBack,
                None,
                None,
                "2026-07-23T00:07:00Z",
            )
            .unwrap();
        assert_eq!(
            rolled_back.rolled_back_at.as_deref(),
            Some("2026-07-23T00:07:00Z")
        );
    }

    #[test]
    fn removed_worktree_operations_clear_resources_and_can_restart() {
        let mut store = SqliteStore::in_memory().unwrap();
        store
            .create_or_load_worktree_operation(&sample_worktree_operation("op_removed", "key"))
            .unwrap();
        for state in [
            WorktreeOperationState::WorktreeCreated,
            WorktreeOperationState::WorkspaceCreated,
            WorktreeOperationState::SessionCreated,
            WorktreeOperationState::Completed,
        ] {
            store
                .transition_worktree_operation("op_removed", state, None, None, "completed")
                .unwrap();
        }
        store
            .update_worktree_operation_resources(
                "op_removed",
                Some("ws_old"),
                Some("surface_old"),
                Some("session_old"),
                None,
                "owned",
            )
            .unwrap();
        let removed = store
            .mark_worktree_operation_removed("op_removed", r#"{"owned":false}"#, "removed")
            .unwrap();
        assert_eq!(removed.state, WorktreeOperationState::Removed);
        assert_eq!(removed.workspace_id, None);
        assert_eq!(removed.surface_id, None);
        assert_eq!(removed.session_id, None);

        let restarted = store
            .restart_removed_worktree_operation("op_removed", r#"{"owned":false}"#, "restarted")
            .unwrap();
        assert_eq!(restarted.state, WorktreeOperationState::Prepared);
        assert_eq!(restarted.recovery_attempts, 0);
        assert_eq!(restarted.completed_at, None);
    }

    #[test]
    fn git_review_threads_and_comments_support_crud_delivery_and_stale_marking() {
        let mut store = SqliteStore::in_memory().unwrap();
        store
            .upsert_git_review_thread(&sample_git_review_thread("thread_old", "diff_old"))
            .unwrap();
        store
            .upsert_git_review_thread(&sample_git_review_thread("thread_current", "diff_current"))
            .unwrap();
        store
            .upsert_git_review_comment(&PersistedGitReviewComment {
                comment_id: "comment_1".to_string(),
                thread_id: "thread_old".to_string(),
                author_id: "reviewer".to_string(),
                body: "Please cover the rollback path.".to_string(),
                target_kind: Some("mailbox".to_string()),
                target_id: Some("session_worker".to_string()),
                delivery_state: "pending".to_string(),
                delivery_error: None,
                delivered_at: None,
                created_at: "2026-07-23T00:00:01Z".to_string(),
                updated_at: "2026-07-23T00:00:01Z".to_string(),
            })
            .unwrap();
        let delivery = GitReviewDeliveryUpdate {
            target_kind: Some("mailbox".to_string()),
            target_id: Some("session_worker".to_string()),
            delivery_state: "delivered".to_string(),
            delivery_error: None,
            delivered_at: Some("2026-07-23T00:00:02Z".to_string()),
            updated_at: "2026-07-23T00:00:02Z".to_string(),
        };
        assert!(store
            .update_git_review_delivery_batch("thread_old", &delivery)
            .unwrap());
        assert!(store
            .update_git_review_comment_delivery("comment_1", &delivery)
            .unwrap());
        assert!(store
            .update_git_review_thread_delivery("thread_old", &delivery)
            .unwrap());

        assert_eq!(
            store
                .mark_git_review_threads_stale_except_diff(
                    "D:\\repo",
                    "diff_current",
                    "diff changed",
                    "2026-07-23T00:00:03Z",
                )
                .unwrap(),
            1
        );
        let old = store.load_git_review_thread("thread_old").unwrap().unwrap();
        assert!(old.stale);
        assert_eq!(old.stale_reason.as_deref(), Some("diff changed"));
        assert_eq!(old.delivery_state, "delivered");
        assert!(store
            .list_git_review_threads("D:\\repo", None, false)
            .unwrap()
            .iter()
            .all(|thread| thread.thread_id != "thread_old"));
        let comments = store.list_git_review_comments("thread_old").unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].delivery_state, "delivered");
        assert_eq!(
            comments[0].delivered_at.as_deref(),
            Some("2026-07-23T00:00:02Z")
        );
        assert_eq!(
            store
                .load_git_review_comment("comment_1")
                .unwrap()
                .unwrap()
                .body,
            "Please cover the rollback path."
        );
        assert!(store.delete_git_review_comment("comment_1").unwrap());
        assert!(store
            .list_git_review_comments("thread_old")
            .unwrap()
            .is_empty());
        assert!(store.delete_git_review_thread("thread_old").unwrap());
        assert!(store
            .load_git_review_thread("thread_old")
            .unwrap()
            .is_none());
    }

    #[test]
    fn git_review_delivery_batch_rolls_back_on_comment_update_failure() {
        let mut store = SqliteStore::in_memory().unwrap();
        store
            .upsert_git_review_thread(&sample_git_review_thread("thread_atomic", "diff"))
            .unwrap();
        store
            .upsert_git_review_comment(&PersistedGitReviewComment {
                comment_id: "comment_atomic".to_string(),
                thread_id: "thread_atomic".to_string(),
                author_id: "reviewer".to_string(),
                body: "Atomic delivery".to_string(),
                target_kind: None,
                target_id: None,
                delivery_state: "pending".to_string(),
                delivery_error: None,
                delivered_at: None,
                created_at: "created".to_string(),
                updated_at: "created".to_string(),
            })
            .unwrap();
        store
            .connection
            .execute_batch(
                "CREATE TRIGGER fail_review_comment_delivery
                 BEFORE UPDATE ON git_review_comments
                 BEGIN SELECT RAISE(FAIL, 'injected comment update failure'); END;",
            )
            .unwrap();
        let update = GitReviewDeliveryUpdate {
            target_kind: Some("mailbox".to_string()),
            target_id: Some("session_worker".to_string()),
            delivery_state: "delivering".to_string(),
            delivery_error: None,
            delivered_at: None,
            updated_at: "updated".to_string(),
        };
        assert!(store
            .update_git_review_delivery_batch("thread_atomic", &update)
            .is_err());
        let thread = store
            .load_git_review_thread("thread_atomic")
            .unwrap()
            .unwrap();
        let comment = store
            .load_git_review_comment("comment_atomic")
            .unwrap()
            .unwrap();
        assert_eq!(thread.delivery_state, "pending");
        assert_eq!(comment.delivery_state, "pending");
    }

    #[test]
    fn review_attempt_begin_rolls_back_at_the_comment_side_effect_boundary() {
        let mut store = SqliteStore::in_memory().unwrap();
        store
            .upsert_git_review_thread(&sample_git_review_thread("thread_attempt", "diff"))
            .unwrap();
        store
            .upsert_git_review_comment(&PersistedGitReviewComment {
                comment_id: "comment_attempt".to_string(),
                thread_id: "thread_attempt".to_string(),
                author_id: "reviewer".to_string(),
                body: "Atomic attempt".to_string(),
                target_kind: None,
                target_id: None,
                delivery_state: "pending".to_string(),
                delivery_error: None,
                delivered_at: None,
                created_at: "created".to_string(),
                updated_at: "created".to_string(),
            })
            .unwrap();
        store
            .connection
            .execute_batch(
                "CREATE TRIGGER fail_attempt_comment_delivery
                 BEFORE UPDATE ON git_review_comments
                 BEGIN SELECT RAISE(FAIL, 'injected attempt boundary failure'); END;",
            )
            .unwrap();
        let attempt = sample_git_review_delivery_attempt("attempt_atomic", "thread_attempt");
        let update = GitReviewDeliveryUpdate {
            target_kind: Some("mailbox".to_string()),
            target_id: Some("session_worker".to_string()),
            delivery_state: "delivering".to_string(),
            delivery_error: None,
            delivered_at: None,
            updated_at: "updated".to_string(),
        };
        assert!(store
            .begin_git_review_delivery_attempt(&attempt, &update)
            .is_err());
        assert!(store
            .load_latest_git_review_delivery_attempt(
                "thread_attempt",
                "payload",
                "mailbox",
                "session_worker"
            )
            .unwrap()
            .is_none());
        assert_eq!(
            store
                .load_git_review_thread("thread_attempt")
                .unwrap()
                .unwrap()
                .delivery_state,
            "pending"
        );
    }

    #[test]
    fn review_attempt_confirmation_rolls_back_at_the_comment_side_effect_boundary() {
        let mut store = SqliteStore::in_memory().unwrap();
        store
            .upsert_git_review_thread(&sample_git_review_thread("thread_confirm", "diff"))
            .unwrap();
        store
            .upsert_git_review_comment(&PersistedGitReviewComment {
                comment_id: "comment_confirm".to_string(),
                thread_id: "thread_confirm".to_string(),
                author_id: "reviewer".to_string(),
                body: "Atomic confirmation".to_string(),
                target_kind: None,
                target_id: None,
                delivery_state: "pending".to_string(),
                delivery_error: None,
                delivered_at: None,
                created_at: "created".to_string(),
                updated_at: "created".to_string(),
            })
            .unwrap();
        let attempt = sample_git_review_delivery_attempt("attempt_confirm", "thread_confirm");
        let delivering = GitReviewDeliveryUpdate {
            target_kind: Some("mailbox".to_string()),
            target_id: Some("session_worker".to_string()),
            delivery_state: "delivering".to_string(),
            delivery_error: None,
            delivered_at: None,
            updated_at: "sending".to_string(),
        };
        assert!(store
            .begin_git_review_delivery_attempt(&attempt, &delivering)
            .unwrap());
        assert!(store
            .update_git_review_delivery_attempt(
                &attempt.attempt_id,
                "prepared",
                "sending",
                None,
                None,
                &delivering,
            )
            .unwrap());
        store
            .connection
            .execute_batch(
                "CREATE TRIGGER fail_confirmation_comment_delivery
                 BEFORE UPDATE ON git_review_comments
                 BEGIN SELECT RAISE(FAIL, 'injected confirmation boundary failure'); END;",
            )
            .unwrap();
        let confirmed = GitReviewDeliveryUpdate {
            target_kind: Some("mailbox".to_string()),
            target_id: Some("session_worker".to_string()),
            delivery_state: "delivered".to_string(),
            delivery_error: None,
            delivered_at: Some("confirmed".to_string()),
            updated_at: "confirmed".to_string(),
        };
        assert!(store
            .update_git_review_delivery_attempt(
                &attempt.attempt_id,
                "sending",
                "confirmed",
                None,
                Some("confirmed"),
                &confirmed,
            )
            .is_err());
        let persisted = store
            .load_latest_git_review_delivery_attempt(
                "thread_confirm",
                "payload",
                "mailbox",
                "session_worker",
            )
            .unwrap()
            .unwrap();
        assert_eq!(persisted.state, "sending");
        assert_eq!(
            store
                .load_git_review_thread("thread_confirm")
                .unwrap()
                .unwrap()
                .delivery_state,
            "delivering"
        );
        assert_eq!(
            store
                .load_git_review_comment("comment_confirm")
                .unwrap()
                .unwrap()
                .delivery_state,
            "delivering"
        );
    }

    #[test]
    fn deterministic_mailbox_delivery_rolls_back_when_notification_insert_fails() {
        let mut store = SqliteStore::in_memory().unwrap();
        store
            .connection
            .execute_batch(
                "CREATE TRIGGER fail_review_notification
                 BEFORE INSERT ON notifications
                 BEGIN SELECT RAISE(FAIL, 'injected notification failure'); END;",
            )
            .unwrap();
        let message = PersistedTeamMessage {
            message_id: "msg_attempt".to_string(),
            workspace_id: "ws".to_string(),
            thread_id: Some("thread".to_string()),
            from_session_id: None,
            to_session_id: Some("session".to_string()),
            body: "review".to_string(),
            kind: "git_review".to_string(),
            created_at: "created".to_string(),
            read_at: None,
        };
        let notification = PersistedNotification {
            notification_id: "not_attempt".to_string(),
            notification_type: "team.message".to_string(),
            severity: "info".to_string(),
            workspace_id: Some("ws".to_string()),
            session_id: Some("session".to_string()),
            title: "Agent message".to_string(),
            message: "review".to_string(),
            created_at: "created".to_string(),
            dismissed: false,
        };
        assert!(store
            .upsert_team_message_and_notification(&message, &notification)
            .is_err());
        assert!(store.load_team_message("msg_attempt").unwrap().is_none());
    }

    #[test]
    fn verified_hook_sequence_survives_store_reopen() {
        let path = unique_temp_db_path("verified_hook_sequence_survives_store_reopen");
        {
            let mut store = SqliteStore::open(&path).unwrap();
            store
                .upsert_verified_agent_hook(&PersistedVerifiedAgentHook {
                    session_id: "session_hook".to_string(),
                    workspace_id: "workspace_hook".to_string(),
                    source: "claude_hook".to_string(),
                    sequence: 42,
                    state: "running".to_string(),
                    reason: Some("PreToolUse".to_string()),
                    telemetry_json: Some(r#"{"model":"claude"}"#.to_string()),
                    updated_at: "updated".to_string(),
                })
                .unwrap();
        }
        let mut reopened = SqliteStore::open(&path).unwrap();
        let hook = reopened
            .load_verified_agent_hook("session_hook")
            .unwrap()
            .unwrap();
        assert_eq!(hook.sequence, 42);
        assert_eq!(hook.state, "running");
        assert!(reopened.delete_verified_agent_hook("session_hook").unwrap());
        drop(reopened);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn five_track_recovery_schema_is_versioned() {
        assert_eq!(MIGRATIONS[13].version, 14);
        assert!(MIGRATIONS[13]
            .sql
            .contains("CREATE TABLE IF NOT EXISTS git_review_delivery_attempts"));
        assert!(MIGRATIONS[13]
            .sql
            .contains("CREATE TABLE IF NOT EXISTS verified_agent_hooks"));
    }

    #[test]
    fn git_mutation_receipt_schema_upgrades_existing_v14_store() {
        let connection = Connection::open_in_memory().unwrap();
        apply_migrations(&connection, &MIGRATIONS[..14]).unwrap();
        let before: i32 = connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(before, 14);

        apply_migrations(&connection, MIGRATIONS).unwrap();
        let table_exists: bool = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_master
                   WHERE type = 'table' AND name = 'git_mutation_receipts'
                 )",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let after: i32 = connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(table_exists);
        assert_eq!(after, 17);
    }

    #[test]
    fn git_mutation_lifecycle_upgrades_v15_receipts_as_completed() {
        let connection = Connection::open_in_memory().unwrap();
        apply_migrations(&connection, &MIGRATIONS[..15]).unwrap();
        connection
            .execute(
                "INSERT INTO git_mutation_receipts (
                    method, repository_id, idempotency_key, request_fingerprint,
                    response_json, created_at
                 ) VALUES ('git.commit', 'repo-a', 'key-a', 'request-a', '{}', 'created')",
                [],
            )
            .unwrap();

        apply_migrations(&connection, MIGRATIONS).unwrap();
        let upgraded: (String, String, String) = connection
            .query_row(
                "SELECT lifecycle_state, precondition_fingerprint, updated_at
                 FROM git_mutation_receipts
                 WHERE idempotency_key = 'key-a'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(upgraded, ("completed".into(), "".into(), "created".into()));
    }

    #[test]
    fn git_mutation_lifecycle_persists_pending_then_completes_after_reopen() {
        let path = unique_temp_db_path("git_mutation_lifecycle_reopen");
        let intent = sample_git_mutation_intent("git.commit", "repo-a", "key-a");
        {
            let mut store = SqliteStore::open(&path).unwrap();
            assert_eq!(
                store.prepare_git_mutation(&intent).unwrap(),
                GitMutationReceiptLookup::Pending(intent.clone())
            );
        }

        let mut reopened = SqliteStore::open(&path).unwrap();
        assert_eq!(
            reopened.prepare_git_mutation(&intent).unwrap(),
            GitMutationReceiptLookup::Pending(intent.clone())
        );
        assert_eq!(
            reopened
                .complete_git_mutation(&intent, r#"{"commit":"abc"}"#, "completed")
                .unwrap(),
            GitMutationReceiptLookup::Match(PersistedGitMutationReceipt {
                method: intent.method.clone(),
                repository_id: intent.repository_id.clone(),
                idempotency_key: intent.idempotency_key.clone(),
                request_fingerprint: intent.request_fingerprint.clone(),
                response_json: r#"{"commit":"abc"}"#.to_string(),
                created_at: intent.created_at.clone(),
            })
        );
        drop(reopened);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn git_mutation_pending_rejects_mismatch_and_can_be_marked_indeterminate() {
        let mut store = SqliteStore::in_memory().unwrap();
        let intent = sample_git_mutation_intent("git.stage", "repo-a", "key-a");
        assert!(matches!(
            store.prepare_git_mutation(&intent).unwrap(),
            GitMutationReceiptLookup::Pending(_)
        ));

        let mut changed_request = intent.clone();
        changed_request.request_fingerprint = "different-request".to_string();
        assert_eq!(
            store.prepare_git_mutation(&changed_request).unwrap(),
            GitMutationReceiptLookup::FingerprintMismatch {
                stored_request_fingerprint: intent.request_fingerprint.clone(),
            }
        );

        let mut changed_precondition = intent.clone();
        changed_precondition.precondition_fingerprint = "different-state".to_string();
        assert_eq!(
            store.prepare_git_mutation(&changed_precondition).unwrap(),
            GitMutationReceiptLookup::Pending(intent.clone())
        );

        let failure = r#"{"reason":"repository_changed"}"#;
        assert_eq!(
            store
                .mark_git_mutation_indeterminate(&intent, failure, "failed")
                .unwrap(),
            GitMutationReceiptLookup::Indeterminate {
                intent: intent.clone(),
                failure_json: Some(failure.to_string()),
            }
        );
        assert_eq!(store.prune_git_mutation_receipts(0).unwrap(), 0);
    }

    #[test]
    fn failed_completion_persistence_leaves_a_durable_pending_intent() {
        let mut store = SqliteStore::in_memory().unwrap();
        let intent = sample_git_mutation_intent("git.unstage", "repo-a", "key-a");
        store.prepare_git_mutation(&intent).unwrap();
        store
            .connection
            .execute_batch(
                "CREATE TRIGGER fail_git_mutation_completion
                 BEFORE UPDATE OF lifecycle_state ON git_mutation_receipts
                 WHEN NEW.lifecycle_state = 'completed'
                 BEGIN
                   SELECT RAISE(FAIL, 'injected completion persistence failure');
                 END;",
            )
            .unwrap();

        let error = store
            .complete_git_mutation(&intent, r#"{"generation":2}"#, "completed")
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("injected completion persistence failure"));
        assert_eq!(
            store
                .load_git_mutation_receipt(
                    &intent.method,
                    &intent.repository_id,
                    &intent.idempotency_key,
                    &intent.request_fingerprint,
                )
                .unwrap(),
            GitMutationReceiptLookup::Pending(intent)
        );
    }

    #[test]
    fn git_mutation_receipts_survive_reopen_and_replay_same_payload() {
        let path = unique_temp_db_path("git_mutation_receipts_survive_reopen");
        let receipt = sample_git_mutation_receipt("commit", "repo-a", "request-a", "hash-a", "1");
        {
            let mut store = SqliteStore::open(&path).unwrap();
            assert_eq!(
                store.create_or_load_git_mutation_receipt(&receipt).unwrap(),
                GitMutationReceiptLookup::Match(receipt.clone())
            );
        }

        let mut reopened = SqliteStore::open(&path).unwrap();
        assert_eq!(
            reopened
                .create_or_load_git_mutation_receipt(&receipt)
                .unwrap(),
            GitMutationReceiptLookup::Match(receipt.clone())
        );
        assert_eq!(
            reopened
                .load_git_mutation_receipt("commit", "repo-a", "request-a", "hash-a")
                .unwrap(),
            GitMutationReceiptLookup::Match(receipt)
        );
        drop(reopened);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn git_mutation_receipts_expose_fingerprint_mismatch_without_overwriting_result() {
        let mut store = SqliteStore::in_memory().unwrap();
        let stored = sample_git_mutation_receipt("commit", "repo-a", "request-a", "hash-a", "1");
        assert_eq!(
            store.create_or_load_git_mutation_receipt(&stored).unwrap(),
            GitMutationReceiptLookup::Match(stored.clone())
        );

        let conflicting =
            sample_git_mutation_receipt("commit", "repo-a", "request-a", "hash-b", "2");
        assert_eq!(
            store
                .create_or_load_git_mutation_receipt(&conflicting)
                .unwrap(),
            GitMutationReceiptLookup::FingerprintMismatch {
                stored_request_fingerprint: "hash-a".to_string(),
            }
        );
        assert_eq!(
            store
                .load_git_mutation_receipt("commit", "repo-a", "request-a", "hash-a")
                .unwrap(),
            GitMutationReceiptLookup::Match(stored)
        );
    }

    #[test]
    fn git_mutation_receipt_pruning_retains_newest_entries_with_stable_ordering() {
        let mut store = SqliteStore::in_memory().unwrap();
        for (key, created_at) in [("old", "1"), ("middle", "2"), ("new", "3")] {
            store
                .create_or_load_git_mutation_receipt(&sample_git_mutation_receipt(
                    "stage", "repo-a", key, key, created_at,
                ))
                .unwrap();
        }

        assert_eq!(store.prune_git_mutation_receipts(2).unwrap(), 1);
        assert_eq!(
            store
                .load_git_mutation_receipt("stage", "repo-a", "old", "old")
                .unwrap(),
            GitMutationReceiptLookup::Missing
        );
        assert!(matches!(
            store
                .load_git_mutation_receipt("stage", "repo-a", "middle", "middle")
                .unwrap(),
            GitMutationReceiptLookup::Match(_)
        ));
        assert!(matches!(
            store
                .load_git_mutation_receipt("stage", "repo-a", "new", "new")
                .unwrap(),
            GitMutationReceiptLookup::Match(_)
        ));
    }

    #[test]
    fn git_mutation_receipt_schema_is_versioned() {
        assert_eq!(MIGRATIONS[14].version, 15);
        assert!(MIGRATIONS[14]
            .sql
            .contains("CREATE TABLE IF NOT EXISTS git_mutation_receipts"));
    }

    #[test]
    fn git_mutation_lifecycle_schema_is_versioned() {
        assert_eq!(MIGRATIONS[15].version, 16);
        assert!(MIGRATIONS[15].sql.contains("ADD COLUMN lifecycle_state"));
    }

    #[test]
    fn surface_resource_uri_schema_is_versioned() {
        assert_eq!(MIGRATIONS[16].version, 17);
        assert!(MIGRATIONS[16].sql.contains("ADD COLUMN resource_uri"));
    }

    #[test]
    fn dock_trust_schema_is_versioned() {
        assert_eq!(MIGRATIONS[7].version, 8);
        assert!(MIGRATIONS[7]
            .sql
            .contains("CREATE TABLE IF NOT EXISTS dock_trusts"));
    }

    #[test]
    fn workspace_terminal_profile_schema_is_versioned() {
        assert_eq!(MIGRATIONS[8].version, 9);
        assert!(MIGRATIONS[8]
            .sql
            .contains("ADD COLUMN default_terminal_profile"));
    }

    #[test]
    fn team_collaboration_schema_is_versioned() {
        assert_eq!(MIGRATIONS[9].version, 10);
        assert!(MIGRATIONS[9]
            .sql
            .contains("CREATE TABLE IF NOT EXISTS team_tasks"));
        assert!(MIGRATIONS[9]
            .sql
            .contains("CREATE TABLE IF NOT EXISTS team_messages"));
    }

    #[test]
    fn migration_versions_are_ordered() {
        assert!(migrations_are_ordered(MIGRATIONS));
    }

    #[test]
    fn workspace_metadata_survives_reopen() {
        let path = unique_temp_db_path("workspace_metadata_survives_reopen");
        {
            let mut store = SqliteStore::open(&path).unwrap();
            store.save_workspace_bundle(&sample_bundle()).unwrap();
        }
        {
            let store = SqliteStore::open(&path).unwrap();
            let bundle = store.load_workspace_bundle("ws_test").unwrap().unwrap();
            assert_eq!(bundle.workspace.name, "Test workspace");
            assert_eq!(
                bundle.workspace.description.as_deref(),
                Some("Primary AgentMux workspace")
            );
            assert_eq!(bundle.workspace.icon.as_deref(), Some("A"));
            assert_eq!(bundle.workspace.color.as_deref(), Some("#F97316"));
            assert_eq!(
                bundle.workspace.default_wsl_distribution.as_deref(),
                Some("Ubuntu")
            );
            assert_eq!(
                bundle.workspace.default_terminal_profile.as_deref(),
                Some("powershell")
            );
            assert_eq!(
                bundle.workspace.default_agent_command.as_deref(),
                Some("claude")
            );
            assert_eq!(bundle.panes.len(), 1);
            assert_eq!(bundle.surfaces.len(), 1);
            assert_eq!(bundle.sessions.len(), 2);
        }

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn recovery_marks_active_non_durable_sessions_disconnected_without_backend_ids() {
        let mut store = SqliteStore::in_memory().unwrap();
        store.save_workspace_bundle(&sample_bundle()).unwrap();

        let snapshot = store.load_recovery_snapshot().unwrap();
        let native = snapshot
            .sessions
            .iter()
            .find(|session| session.session_id == "ses_native")
            .unwrap();
        let durable = snapshot
            .sessions
            .iter()
            .find(|session| session.session_id == "ses_durable")
            .unwrap();

        assert_eq!(native.state, "disconnected");
        assert_eq!(native.backend_native_id, None);
        assert_eq!(native.backend_attachment_id, None);
        assert_eq!(durable.state, "recovering");
        assert_eq!(durable.backend_native_id.as_deref(), Some("tmux-pane-1"));
    }

    #[test]
    fn exited_session_state_survives_recovery_without_normalization() {
        let mut store = SqliteStore::in_memory().unwrap();
        store.save_workspace_bundle(&sample_bundle()).unwrap();
        store
            .update_session_state("ses_native", "exited", Some(0), "2026-06-18T00:02:00Z")
            .unwrap();

        let snapshot = store.load_recovery_snapshot().unwrap();
        let native = snapshot
            .sessions
            .iter()
            .find(|session| session.session_id == "ses_native")
            .unwrap();

        assert_eq!(native.state, "exited");
        assert_eq!(native.exit_code, Some(0));
    }

    #[test]
    fn session_launch_environment_schema_is_versioned() {
        assert_eq!(MIGRATIONS[10].version, 11);
        assert!(MIGRATIONS[10]
            .sql
            .contains("ALTER TABLE session_launch_specs ADD COLUMN env_json"));
    }

    #[test]
    fn session_launch_spec_round_trips_and_is_removed_with_session() {
        let mut store = SqliteStore::in_memory().unwrap();
        store.save_workspace_bundle(&sample_bundle()).unwrap();
        let spec = PersistedSessionLaunchSpec {
            session_id: "ses_native".to_string(),
            workspace_id: "ws_test".to_string(),
            backend_profile: Some("PowerShell".to_string()),
            env: vec![("AGENTMUX_TEST".to_string(), "preserved".to_string())],
            columns: 132,
            rows: 41,
            updated_at: "2026-06-18T00:03:00Z".to_string(),
        };

        store.upsert_session_launch_spec(&spec).unwrap();
        assert_eq!(
            store.load_session_launch_spec("ses_native").unwrap(),
            Some(spec)
        );

        store.delete_session("ses_native").unwrap();
        assert!(store
            .load_session_launch_spec("ses_native")
            .unwrap()
            .is_none());
    }

    #[test]
    fn workspace_bundle_and_launch_spec_commit_atomically() {
        let mut store = SqliteStore::in_memory().unwrap();
        store.save_workspace_bundle(&sample_bundle()).unwrap();
        store
            .connection
            .execute_batch(
                "CREATE TRIGGER fail_launch_spec_insert
                 BEFORE INSERT ON session_launch_specs
                 BEGIN
                   SELECT RAISE(ABORT, 'injected launch spec failure');
                 END;",
            )
            .unwrap();

        let mut changed = sample_bundle();
        changed.workspace.name = "Must roll back".to_string();
        let spec = PersistedSessionLaunchSpec {
            session_id: "ses_native".to_string(),
            workspace_id: "ws_test".to_string(),
            backend_profile: Some("PowerShell".to_string()),
            env: vec![("TERM".to_string(), "xterm-256color".to_string())],
            columns: 132,
            rows: 41,
            updated_at: "2026-06-18T00:03:00Z".to_string(),
        };

        let error = store
            .save_workspace_bundle_and_launch_spec(&changed, &spec)
            .unwrap_err();
        assert!(error.to_string().contains("injected launch spec failure"));
        assert_eq!(
            store
                .load_workspace_bundle("ws_test")
                .unwrap()
                .unwrap()
                .workspace
                .name,
            "Test workspace"
        );
        assert!(store
            .load_session_launch_spec("ses_native")
            .unwrap()
            .is_none());
    }

    #[test]
    fn workspace_rename_and_delete_update_metadata() {
        let mut store = SqliteStore::in_memory().unwrap();
        store.save_workspace_bundle(&sample_bundle()).unwrap();
        store
            .upsert_agent_state(&PersistedAgentState {
                session_id: "ses_native".to_string(),
                workspace_id: "ws_test".to_string(),
                state: "waiting_for_input".to_string(),
                attention: true,
                reason: Some("needs prompt".to_string()),
                updated_at: "2026-06-18T00:03:00Z".to_string(),
                telemetry_json: None,
            })
            .unwrap();
        store
            .upsert_notification(&PersistedNotification {
                notification_id: "not_test".to_string(),
                notification_type: "agent.needs_input".to_string(),
                severity: "warning".to_string(),
                workspace_id: Some("ws_test".to_string()),
                session_id: Some("ses_native".to_string()),
                title: "Agent needs input".to_string(),
                message: "needs prompt".to_string(),
                created_at: "2026-06-18T00:03:00Z".to_string(),
                dismissed: false,
            })
            .unwrap();

        assert!(store
            .rename_workspace("ws_test", "Renamed", "2026-06-18T00:03:00Z")
            .unwrap());
        let renamed = store.load_workspace_bundle("ws_test").unwrap().unwrap();
        assert_eq!(renamed.workspace.name, "Renamed");

        assert!(store.delete_workspace("ws_test").unwrap());
        assert!(store.load_workspace_bundle("ws_test").unwrap().is_none());
        assert!(store.list_sessions().unwrap().is_empty());
        assert!(store
            .list_agent_attention(Some("ws_test"))
            .unwrap()
            .is_empty());
        assert!(store
            .list_notifications(Some("ws_test"), None, true)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn save_workspace_bundle_removes_rows_missing_from_replacement() {
        let mut store = SqliteStore::in_memory().unwrap();
        store.save_workspace_bundle(&sample_bundle()).unwrap();
        store
            .upsert_agent_state(&PersistedAgentState {
                session_id: "ses_native".to_string(),
                workspace_id: "ws_test".to_string(),
                state: "failed".to_string(),
                attention: true,
                reason: None,
                updated_at: "2026-06-18T00:01:30Z".to_string(),
                telemetry_json: None,
            })
            .unwrap();

        let mut replacement = sample_bundle();
        replacement.surfaces.clear();
        replacement
            .sessions
            .retain(|session| session.session_id == "ses_durable");
        replacement.panes[0].mounted_surface_id = None;
        store.save_workspace_bundle(&replacement).unwrap();

        let reloaded = store.load_workspace_bundle("ws_test").unwrap().unwrap();
        assert_eq!(reloaded.panes.len(), 1);
        assert!(reloaded.surfaces.is_empty());
        assert_eq!(reloaded.sessions.len(), 1);
        assert_eq!(reloaded.sessions[0].session_id, "ses_durable");
        assert!(store.load_agent_state("ses_native").unwrap().is_none());
    }

    #[test]
    fn delete_session_removes_agent_state() {
        let mut store = SqliteStore::in_memory().unwrap();
        store.save_workspace_bundle(&sample_bundle()).unwrap();
        store
            .upsert_agent_state(&PersistedAgentState {
                session_id: "ses_native".to_string(),
                workspace_id: "ws_test".to_string(),
                state: "running".to_string(),
                attention: false,
                reason: Some("Agent started: claude".to_string()),
                updated_at: "2026-06-18T00:01:30Z".to_string(),
                telemetry_json: Some(r#"{"activity":"agent","session":"claude"}"#.to_string()),
            })
            .unwrap();

        store.delete_session("ses_native").unwrap();

        assert!(store.load_session("ses_native").unwrap().is_none());
        assert!(store.load_agent_state("ses_native").unwrap().is_none());
    }

    #[test]
    fn agent_state_and_notifications_survive_reopen_and_filters() {
        let path = unique_temp_db_path("agent_state_and_notifications_survive_reopen_and_filters");
        {
            let mut store = SqliteStore::open(&path).unwrap();
            store.save_workspace_bundle(&sample_bundle()).unwrap();
            store
                .upsert_agent_state(&PersistedAgentState {
                    session_id: "ses_native".to_string(),
                    workspace_id: "ws_test".to_string(),
                    state: "waiting_for_input".to_string(),
                    attention: true,
                    reason: Some("confirm change".to_string()),
                    updated_at: "2026-06-18T00:04:00Z".to_string(),
                    telemetry_json: None,
                })
                .unwrap();
            store
                .upsert_notification(&PersistedNotification {
                    notification_id: "not_20260618_000001".to_string(),
                    notification_type: "agent.needs_input".to_string(),
                    severity: "warning".to_string(),
                    workspace_id: Some("ws_test".to_string()),
                    session_id: Some("ses_native".to_string()),
                    title: "Agent needs input".to_string(),
                    message: "confirm change".to_string(),
                    created_at: "2026-06-18T00:04:00Z".to_string(),
                    dismissed: false,
                })
                .unwrap();
            store
                .upsert_notification(&PersistedNotification {
                    notification_id: "not_20260618_000002".to_string(),
                    notification_type: "agent.completed".to_string(),
                    severity: "info".to_string(),
                    workspace_id: Some("ws_test".to_string()),
                    session_id: Some("ses_durable".to_string()),
                    title: "Agent completed".to_string(),
                    message: "done".to_string(),
                    created_at: "2026-06-18T00:05:00Z".to_string(),
                    dismissed: false,
                })
                .unwrap();
        }
        {
            let mut store = SqliteStore::open(&path).unwrap();
            let state = store.load_agent_state("ses_native").unwrap().unwrap();
            assert_eq!(state.state, "waiting_for_input");
            assert!(state.attention);

            let attention = store.list_agent_attention(Some("ws_test")).unwrap();
            assert_eq!(attention.len(), 1);
            assert_eq!(attention[0].session_id, "ses_native");

            let warning = store
                .list_notifications(Some("ws_test"), Some("warning"), false)
                .unwrap();
            assert_eq!(warning.len(), 1);
            assert_eq!(warning[0].notification_type, "agent.needs_input");

            assert!(store.dismiss_notification("not_20260618_000001").unwrap());
            assert!(store
                .list_notifications(Some("ws_test"), Some("warning"), false)
                .unwrap()
                .is_empty());
            assert_eq!(
                store
                    .list_notifications(Some("ws_test"), Some("warning"), true)
                    .unwrap()
                    .len(),
                1
            );
        }

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn sidebar_metadata_survives_reopen() {
        let path = unique_temp_db_path("sidebar_metadata_survives_reopen");
        {
            let mut store = SqliteStore::open(&path).unwrap();
            store.save_workspace_bundle(&sample_bundle()).unwrap();
            store
                .upsert_sidebar_status(&PersistedSidebarStatus {
                    workspace_id: "ws_test".to_string(),
                    key: "build".to_string(),
                    label: "compiling".to_string(),
                    icon: Some("hammer".to_string()),
                    color: Some("#ff9500".to_string()),
                    priority: 80,
                    updated_at: "2026-06-19T00:00:00Z".to_string(),
                })
                .unwrap();
            store
                .upsert_sidebar_progress(&PersistedSidebarProgress {
                    workspace_id: "ws_test".to_string(),
                    value: 0.5,
                    label: Some("Building".to_string()),
                    updated_at: "2026-06-19T00:00:01Z".to_string(),
                })
                .unwrap();
            store
                .append_sidebar_log(&PersistedSidebarLog {
                    log_id: "log_1".to_string(),
                    workspace_id: "ws_test".to_string(),
                    level: "success".to_string(),
                    source: Some("test".to_string()),
                    message: "ok".to_string(),
                    created_at: "2026-06-19T00:00:02Z".to_string(),
                })
                .unwrap();
        }
        {
            let store = SqliteStore::open(&path).unwrap();
            assert_eq!(
                store.list_sidebar_status("ws_test").unwrap()[0].key,
                "build"
            );
            assert_eq!(
                store
                    .load_sidebar_progress("ws_test")
                    .unwrap()
                    .unwrap()
                    .value,
                0.5
            );
            assert_eq!(
                store.list_sidebar_logs("ws_test", Some(5)).unwrap()[0].message,
                "ok"
            );
        }

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn dock_trust_survives_reopen_and_matches_current_hash() {
        let path = unique_temp_db_path("dock_trust_survives_reopen");
        {
            let mut store = SqliteStore::open(&path).unwrap();
            store
                .upsert_dock_trust(&PersistedDockTrust {
                    workspace_id: "ws_test".to_string(),
                    source: "project_agentmux".to_string(),
                    config_path: "D:\\repo\\.agentmux\\dock.json".to_string(),
                    config_hash: "hash_a".to_string(),
                    trusted_at: "2026-06-19T00:00:00Z".to_string(),
                    updated_at: "2026-06-19T00:00:00Z".to_string(),
                })
                .unwrap();
            assert!(store
                .dock_trust_matches(
                    "ws_test",
                    "project_agentmux",
                    "D:\\repo\\.agentmux\\dock.json",
                    "hash_a"
                )
                .unwrap());
            assert!(!store
                .dock_trust_matches(
                    "ws_test",
                    "project_agentmux",
                    "D:\\repo\\.agentmux\\dock.json",
                    "hash_b"
                )
                .unwrap());
        }
        {
            let store = SqliteStore::open(&path).unwrap();
            let trust = store
                .load_dock_trust(
                    "ws_test",
                    "project_agentmux",
                    "D:\\repo\\.agentmux\\dock.json",
                )
                .unwrap()
                .unwrap();
            assert_eq!(trust.config_hash, "hash_a");
        }

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn workspace_groups_survive_reopen_and_cleanup_workspace_membership() {
        let path = unique_temp_db_path("workspace_groups_survive_reopen");
        {
            let mut store = SqliteStore::open(&path).unwrap();
            store.save_workspace_bundle(&sample_bundle()).unwrap();
            store
                .upsert_workspace_group(&PersistedWorkspaceGroup {
                    group_id: "grp_agents".to_string(),
                    name: "Agents".to_string(),
                    anchor_workspace_id: Some("ws_test".to_string()),
                    collapsed: true,
                    pinned: true,
                    color: Some("#22C55E".to_string()),
                    icon: Some("A".to_string()),
                    sort_order: 10,
                    created_at: "2026-06-19T00:10:00Z".to_string(),
                    updated_at: "2026-06-19T00:10:00Z".to_string(),
                })
                .unwrap();
            store
                .upsert_workspace_group(&PersistedWorkspaceGroup {
                    group_id: "grp_ops".to_string(),
                    name: "Ops".to_string(),
                    anchor_workspace_id: None,
                    collapsed: false,
                    pinned: true,
                    color: Some("#38BDF8".to_string()),
                    icon: Some("O".to_string()),
                    sort_order: 2,
                    created_at: "2026-06-19T00:09:00Z".to_string(),
                    updated_at: "2026-06-19T00:09:00Z".to_string(),
                })
                .unwrap();
            store
                .upsert_workspace_group_member(&PersistedWorkspaceGroupMember {
                    group_id: "grp_agents".to_string(),
                    workspace_id: "ws_test".to_string(),
                    position: 1,
                    created_at: "2026-06-19T00:10:01Z".to_string(),
                    updated_at: "2026-06-19T00:10:01Z".to_string(),
                })
                .unwrap();
        }
        {
            let mut store = SqliteStore::open(&path).unwrap();
            let groups = store.list_workspace_groups().unwrap();
            assert_eq!(groups.len(), 2);
            assert_eq!(groups[0].group_id, "grp_ops");
            assert_eq!(groups[0].sort_order, 2);
            assert_eq!(groups[1].group_id, "grp_agents");
            assert_eq!(groups[1].sort_order, 10);
            assert_eq!(groups[1].name, "Agents");
            assert!(groups[1].collapsed);
            assert!(groups[1].pinned);
            let members = store
                .list_workspace_group_members(Some("grp_agents"))
                .unwrap();
            assert_eq!(members.len(), 1);
            assert_eq!(members[0].workspace_id, "ws_test");
            assert_eq!(members[0].position, 1);

            assert!(store.delete_workspace("ws_test").unwrap());
            assert!(store
                .list_workspace_group_members(Some("grp_agents"))
                .unwrap()
                .is_empty());
            assert_eq!(
                store
                    .load_workspace_group("grp_agents")
                    .unwrap()
                    .unwrap()
                    .anchor_workspace_id,
                None
            );
        }

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn sensitive_env_keys_are_detected_and_redacted() {
        let env = vec![
            ("OPENAI_API_KEY".to_string(), "secret".to_string()),
            ("AGENTMUX_TOKEN".to_string(), "token".to_string()),
            ("PATH".to_string(), "C:\\Windows".to_string()),
        ];

        assert!(redact_env_key("OPENAI_API_KEY"));
        assert!(redact_env_key("AGENTMUX_TOKEN"));
        assert!(!redact_env_key("PATH"));
        assert_eq!(
            redact_env_pairs(&env),
            vec![
                ("OPENAI_API_KEY".to_string(), REDACTED_VALUE.to_string()),
                ("AGENTMUX_TOKEN".to_string(), REDACTED_VALUE.to_string()),
                ("PATH".to_string(), "C:\\Windows".to_string()),
            ]
        );
    }

    fn sample_worktree_operation(
        operation_id: &str,
        idempotency_key: &str,
    ) -> PersistedWorktreeOperation {
        PersistedWorktreeOperation {
            operation_id: operation_id.to_string(),
            idempotency_key: idempotency_key.to_string(),
            repository_root: "D:\\repo".to_string(),
            worktree_path: format!("D:\\worktrees\\{operation_id}"),
            branch_name: Some(format!("agent/{operation_id}")),
            revision: Some("HEAD".to_string()),
            workspace_id: None,
            surface_id: None,
            session_id: None,
            owner_kind: "agentmux".to_string(),
            owner_id: Some("desktop-host".to_string()),
            ownership_json: r#"{"worktree":true,"workspace":true,"session":true}"#.to_string(),
            request_json: r#"{"agent":"claude","terminal_profile":"wsl"}"#.to_string(),
            state: WorktreeOperationState::Prepared,
            error_code: None,
            error_message: None,
            recovery_json: "{}".to_string(),
            recovery_attempts: 0,
            last_recovery_at: None,
            created_at: "2026-07-23T00:00:00Z".to_string(),
            updated_at: "2026-07-23T00:00:00Z".to_string(),
            completed_at: None,
            rolled_back_at: None,
        }
    }

    fn sample_git_mutation_receipt(
        method: &str,
        repository_id: &str,
        idempotency_key: &str,
        request_fingerprint: &str,
        created_at: &str,
    ) -> PersistedGitMutationReceipt {
        PersistedGitMutationReceipt {
            method: method.to_string(),
            repository_id: repository_id.to_string(),
            idempotency_key: idempotency_key.to_string(),
            request_fingerprint: request_fingerprint.to_string(),
            response_json: format!(r#"{{"result":"{idempotency_key}"}}"#),
            created_at: created_at.to_string(),
        }
    }

    fn sample_git_mutation_intent(
        method: &str,
        repository_id: &str,
        idempotency_key: &str,
    ) -> PersistedGitMutationIntent {
        PersistedGitMutationIntent {
            method: method.to_string(),
            repository_id: repository_id.to_string(),
            idempotency_key: idempotency_key.to_string(),
            request_fingerprint: format!("request-{idempotency_key}"),
            precondition_fingerprint: format!("state-{idempotency_key}"),
            created_at: "created".to_string(),
        }
    }

    fn sample_git_review_thread(thread_id: &str, diff_identity: &str) -> PersistedGitReviewThread {
        PersistedGitReviewThread {
            thread_id: thread_id.to_string(),
            repository_root: "D:\\repo".to_string(),
            workspace_id: Some("ws_test".to_string()),
            diff_identity: diff_identity.to_string(),
            path: "crates/agentmux-store/src/lib.rs".to_string(),
            hunk_id: Some("@@ -10,4 +10,6 @@".to_string()),
            side: "new".to_string(),
            line_number: Some(42),
            line_anchor: "new:42:hash".to_string(),
            stale: false,
            stale_reason: None,
            resolved_at: None,
            author_id: "reviewer".to_string(),
            target_kind: Some("mailbox".to_string()),
            target_id: Some("session_worker".to_string()),
            delivery_state: "pending".to_string(),
            delivery_error: None,
            created_at: "2026-07-23T00:00:00Z".to_string(),
            updated_at: "2026-07-23T00:00:00Z".to_string(),
        }
    }

    fn sample_git_review_delivery_attempt(
        attempt_id: &str,
        thread_id: &str,
    ) -> PersistedGitReviewDeliveryAttempt {
        PersistedGitReviewDeliveryAttempt {
            attempt_id: attempt_id.to_string(),
            thread_id: thread_id.to_string(),
            payload_hash: "payload".to_string(),
            target_kind: "mailbox".to_string(),
            target_id: "session_worker".to_string(),
            attempt_number: 1,
            state: "prepared".to_string(),
            error_message: None,
            created_at: "created".to_string(),
            updated_at: "created".to_string(),
            completed_at: None,
        }
    }

    fn sample_bundle() -> WorkspaceBundle {
        WorkspaceBundle {
            workspace: PersistedWorkspace {
                workspace_id: "ws_test".to_string(),
                name: "Test workspace".to_string(),
                root_pane_id: "pane_root".to_string(),
                active_pane_id: "pane_root".to_string(),
                project_root: Some("D:\\Projects\\agentmux".to_string()),
                environment_profile_id: None,
                description: Some("Primary AgentMux workspace".to_string()),
                icon: Some("A".to_string()),
                color: Some("#F97316".to_string()),
                default_wsl_distribution: Some("Ubuntu".to_string()),
                default_terminal_profile: Some("powershell".to_string()),
                default_agent_command: Some("claude".to_string()),
                created_at: "2026-06-18T00:00:00Z".to_string(),
                updated_at: "2026-06-18T00:01:00Z".to_string(),
            },
            panes: vec![PersistedPane {
                pane_id: "pane_root".to_string(),
                workspace_id: "ws_test".to_string(),
                parent_pane_id: None,
                kind: "leaf".to_string(),
                split_axis: None,
                split_ratio: None,
                mounted_surface_id: Some("surf_terminal".to_string()),
                last_focused_at: Some("2026-06-18T00:01:00Z".to_string()),
                created_at: "2026-06-18T00:00:00Z".to_string(),
                updated_at: "2026-06-18T00:01:00Z".to_string(),
            }],
            surfaces: vec![PersistedSurface {
                surface_id: "surf_terminal".to_string(),
                workspace_id: "ws_test".to_string(),
                surface_type: "terminal".to_string(),
                title: "Native shell".to_string(),
                session_id: Some("ses_native".to_string()),
                browser_id: None,
                resource_uri: None,
                created_at: "2026-06-18T00:00:00Z".to_string(),
                last_visible_at: Some("2026-06-18T00:01:00Z".to_string()),
                updated_at: "2026-06-18T00:01:00Z".to_string(),
            }],
            sessions: vec![
                PersistedSession {
                    session_id: "ses_native".to_string(),
                    workspace_id: "ws_test".to_string(),
                    backend_kind: "conpty".to_string(),
                    backend_attachment_id: Some("att_native".to_string()),
                    backend_native_id: Some("1234".to_string()),
                    cwd: None,
                    command: vec!["cmd.exe".to_string()],
                    state: "running".to_string(),
                    exit_code: None,
                    durability: "ephemeral".to_string(),
                    created_at: "2026-06-18T00:00:00Z".to_string(),
                    last_seen_at: Some("2026-06-18T00:01:00Z".to_string()),
                    updated_at: "2026-06-18T00:01:00Z".to_string(),
                },
                PersistedSession {
                    session_id: "ses_durable".to_string(),
                    workspace_id: "ws_test".to_string(),
                    backend_kind: "wsl-tmux-control".to_string(),
                    backend_attachment_id: Some("att_durable".to_string()),
                    backend_native_id: Some("tmux-pane-1".to_string()),
                    cwd: Some("/home/dev/project".to_string()),
                    command: vec!["bash".to_string()],
                    state: "running".to_string(),
                    exit_code: None,
                    durability: "durable".to_string(),
                    created_at: "2026-06-18T00:00:00Z".to_string(),
                    last_seen_at: Some("2026-06-18T00:01:00Z".to_string()),
                    updated_at: "2026-06-18T00:01:00Z".to_string(),
                },
            ],
        }
    }

    fn unique_temp_db_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("agentmux-{name}-{nanos}.sqlite3"))
    }
}
