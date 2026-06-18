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
                    environment_profile_id, created_at, updated_at
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
                        environment_profile_id, created_at, updated_at
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
            "DELETE FROM notifications WHERE workspace_id = ?1",
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

    pub fn upsert_agent_state(&mut self, state: &PersistedAgentState) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO agent_states (
                session_id, workspace_id, state, attention, reason, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(session_id) DO UPDATE SET
                workspace_id = excluded.workspace_id,
                state = excluded.state,
                attention = excluded.attention,
                reason = excluded.reason,
                updated_at = excluded.updated_at",
            params![
                state.session_id,
                state.workspace_id,
                state.state,
                state.attention,
                state.reason,
                state.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn load_agent_state(&self, session_id: &str) -> StoreResult<Option<PersistedAgentState>> {
        self.connection
            .query_row(
                "SELECT session_id, workspace_id, state, attention, reason, updated_at
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
                "SELECT session_id, workspace_id, state, attention, reason, updated_at
                 FROM agent_states
                 WHERE attention = 1 AND workspace_id = ?1
                 ORDER BY updated_at DESC, session_id ASC",
            )?;
            let rows = statement.query_map([workspace_id], agent_state_from_row)?;
            return collect_rows(rows);
        }

        let mut statement = self.connection.prepare(
            "SELECT session_id, workspace_id, state, attention, reason, updated_at
             FROM agent_states
             WHERE attention = 1
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
            environment_profile_id, created_at, updated_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(workspace_id) DO UPDATE SET
            name = excluded.name,
            root_pane_id = excluded.root_pane_id,
            active_pane_id = excluded.active_pane_id,
            project_root = excluded.project_root,
            environment_profile_id = excluded.environment_profile_id,
            updated_at = excluded.updated_at",
        params![
            workspace.workspace_id,
            workspace.name,
            workspace.root_pane_id,
            workspace.active_pane_id,
            workspace.project_root,
            workspace.environment_profile_id,
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
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
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
        assert_eq!(store.schema_version().unwrap(), 2);
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
                project_root: Some("D:\\Workspace\\irae\\agentmux".to_string()),
                environment_profile_id: None,
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
                    cwd: Some("/home/irae/project".to_string()),
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
