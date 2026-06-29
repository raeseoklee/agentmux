use std::fmt;
use std::path::Path;

use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Transaction};

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
];

pub const REDACTED_VALUE: &str = "redacted";

#[derive(Debug)]
pub enum StoreError {
    Sql(rusqlite::Error),
    Json(serde_json::Error),
    InvalidMigrationOrder,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Sql(error) => write!(f, "{error}"),
            StoreError::Json(error) => write!(f, "{error}"),
            StoreError::InvalidMigrationOrder => f.write_str("migrations are not strictly ordered"),
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

    pub fn save_workspace_bundle(&mut self, bundle: &WorkspaceBundle) -> StoreResult<()> {
        let tx = self.connection.transaction()?;
        upsert_workspace(&tx, &bundle.workspace)?;
        delete_missing_workspace_rows(
            &tx,
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
            &tx,
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
            &tx,
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
            &tx,
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
            upsert_pane(&tx, pane)?;
        }
        for surface in &bundle.surfaces {
            upsert_surface(&tx, surface)?;
        }
        for session in &bundle.sessions {
            upsert_session(&tx, session)?;
        }

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
    ) -> StoreResult<()> {
        self.connection.execute(
            "UPDATE sessions
             SET state = ?2,
                 exit_code = ?3,
                 last_seen_at = ?4,
                 updated_at = ?4
             WHERE session_id = ?1",
            params![session_id, state, exit_code, updated_at],
        )?;
        Ok(())
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
                    created_at, last_visible_at, updated_at
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
                    created_at, last_visible_at, updated_at
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

    connection.execute_batch(SCHEMA_MIGRATIONS_SQL)?;
    for migration in migrations {
        let already_applied = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
            [migration.version],
            |row| row.get::<_, bool>(0),
        )?;

        if already_applied {
            continue;
        }

        connection.execute_batch(migration.sql)?;
        connection.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            params![migration.version, migration.name],
        )?;
    }

    Ok(())
}

fn delete_missing_workspace_rows(
    transaction: &Transaction<'_>,
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
            created_at, last_visible_at, updated_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(surface_id) DO UPDATE SET
            workspace_id = excluded.workspace_id,
            surface_type = excluded.surface_type,
            title = excluded.title,
            session_id = excluded.session_id,
            browser_id = excluded.browser_id,
            last_visible_at = excluded.last_visible_at,
            updated_at = excluded.updated_at",
        params![
            surface.surface_id,
            surface.workspace_id,
            surface.surface_type,
            surface.title,
            surface.session_id,
            surface.browser_id,
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
        created_at: row.get(6)?,
        last_visible_at: row.get(7)?,
        updated_at: row.get(8)?,
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
        assert_eq!(store.schema_version().unwrap(), 10);
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
