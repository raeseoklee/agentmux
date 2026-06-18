# Goal 2 Persistence and Recovery Status

Status: Draft
Date: 2026-06-18

This document records the current implementation evidence for Goal 2: Persistence and Recovery Base.

## Implemented

- `agentmux-store` now uses SQLite through `rusqlite` with the bundled SQLite library for Windows-friendly builds.
- Store initialization enables foreign keys, requests WAL mode, applies ordered migrations, and records applied versions in `schema_migrations`.
- The initial schema persists workspaces, panes, surfaces, sessions, and backend attachment metadata.
- Workspace records include `root_pane_id`, `active_pane_id`, `project_root`, and `environment_profile_id`.
- Session records include backend kind, backend attachment id, backend-native id, cwd, command JSON, state, exit code, durability, timestamps, and last-seen time.
- `SqliteStore::save_workspace_bundle` saves a workspace, panes, surfaces, and sessions in a single transaction and prunes same-workspace rows that are no longer present in the replacement bundle.
- `SqliteStore::load_workspace_bundle` reloads one workspace and its pane, surface, and session metadata.
- `SqliteStore::load_recovery_snapshot` loads all persisted metadata and normalizes active sessions for startup recovery.
- Active non-durable sessions are returned as `disconnected` during recovery and have backend attachment/native ids cleared so restart logic cannot accidentally treat them as attachable live processes.
- Active durable sessions are returned as `recovering`, preserving backend-native ids for future attach logic.
- Sensitive environment metadata can be redacted through `redact_env_pairs` and `redacted_env_json`; keys containing token, secret, password, or ending in `_KEY` are replaced with `redacted`.
- `DesktopControlState` now opens a `SqliteStore` at startup. The default path is under `%LOCALAPPDATA%\AgentMux\agentmux.sqlite3`, and `AGENTMUX_STORE_PATH` can override it for tests or local diagnostics.
- The Tauri host persists `session.spawn` metadata into SQLite, including a synthetic workspace, pane, surface, and session bundle for the spawned terminal.
- The Tauri host updates persisted session state from successful `session.get` responses so exited states can survive recovery without being normalized back to disconnected.
- A restart-style desktop host test opens a temp SQLite store, spawns a ConPTY command, drops the host, reopens the store through a fresh host, and verifies the non-durable session is recovered as `disconnected` without backend ids.
- `agentmux-ipc` now includes typed params/results for workspace create, list, get, rename, close, workspace details, pane/surface summaries, and recovery diagnostics.
- The desktop `agentmux_control` command now handles `workspace.create`, `workspace.list`, `workspace.get`, `workspace.rename`, `workspace.close`, and `diagnostics.recovery` directly against `SqliteStore`.
- Workspace close supports `fail_if_running`, `detach_sessions`, and `terminate_sessions` policy names. `fail_if_running` now checks live runtime state, `detach_sessions` closes live ConPTY transports with a soft backend termination mode, and `terminate_sessions` kills live ConPTY sessions before deleting persisted workspace metadata.
- The React desktop UI now calls `workspace.list`, creates the first persisted workspace when needed, renders stored workspaces in the sidebar, creates new workspaces through `workspace.create`, loads selected workspace details through `workspace.get`, and spawns native terminal sessions into the selected workspace instead of using a hard-coded workspace id.
- The browser preview control client mirrors the workspace API so the Vite UI can be exercised without a Tauri host.

## Not Yet Implemented

- Durable WSL close policies are not attached to a live tmux backend yet, so unattached durable termination still returns a conflict instead of killing external tmux sessions.
- Durable WSL attach logic now exists in the Goal 4 tmux backend, and desktop startup attempts best-effort attach for persisted `recovering` tmux sessions. End-to-end durable restart proof is still tracked under Goal 4.
- Store rows currently keep flexible string enum values but no unknown-enum warning diagnostics are emitted.

## Verification Evidence

The following command passed on 2026-06-18 using a repository-local Rust toolchain under `.toolchains`:

```powershell
npm run check
```

The check wrapper ran formatter, clippy, all workspace tests, and docs link validation. Store-specific tests covered:

- migration version recording
- migration ordering
- workspace metadata surviving close and reopen of a SQLite file
- replacement bundle pruning for removed pane, surface, and session rows
- recovery normalization for active non-durable and durable sessions
- exited session state preservation during recovery
- desktop host spawn persistence across a fresh host/store open
- workspace CRUD through the desktop `agentmux_control` envelope
- workspace close policy coordination with a live ConPTY backend session
- recovery diagnostics through the desktop `agentmux_control` envelope
- React desktop build against workspace CRUD client methods
- sensitive environment key redaction

## Remaining Gap

Goal 2 has its Windows native persistence and recovery base in place. The remaining recovery gap belongs to the durable WSL/tmux work: startup attach now has a best-effort path, but durable close policies and end-to-end no-duplicate restart proof still need to be completed under Goal 4.
