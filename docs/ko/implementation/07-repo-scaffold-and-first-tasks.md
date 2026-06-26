# Repository Scaffold and First Tasks

Status: Draft
Date: 2026-06-18

이 문서는 실제 코드 저장소를 어떻게 열고, 어떤 순서로 첫 구현 작업을 진행할지 정의한다.

## Proposed Repository Layout

```text
.
├── apps/
│   └── desktop/
│       ├── src/
│       ├── src-tauri/
│       └── package.json
├── crates/
│   ├── agentmux-core/
│   ├── agentmux-ipc/
│   ├── agentmux-store/
│   ├── agentmux-backend/
│   ├── agentmux-backend-conpty/
│   ├── agentmux-backend-wsl/
│   ├── agentmux-backend-tmux/
│   ├── agentmux-browser/
│   ├── agentmux-cli/
│   └── agentmux-telemetry/
├── benches/
│   ├── single-terminal-latency/
│   ├── many-idle-sessions/
│   ├── high-output/
│   ├── resize-storm/
│   └── restart-recovery/
├── tests/
│   ├── fixtures/
│   │   ├── tmux-control/
│   │   └── ipc/
│   └── integration/
├── docs/
└── tools/
```

## Workspace Bootstrap

Initial setup tasks:

- Create Rust workspace root.
- Create minimal crates with compile-only tests.
- Create desktop app shell.
- Add formatter and linter commands.
- Add test command wrappers.
- Add CI workflow.
- Add docs link check.

Suggested root commands:

```text
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets
```

Desktop commands depend on the selected app scaffold, but should be wrapped in root-level scripts or documented clearly.

## First Vertical Slice

The first user-visible feature should be:

"Open the app, create one terminal session, type a command, see output, resize the pane, close the session."

Implementation order:

1. `agentmux-core`: ids, session state, event types.
2. `agentmux-backend`: backend trait and input/output event types.
3. `agentmux-backend-conpty`: minimal Windows shell spawn.
4. `agentmux-ipc`: local request/response envelope.
5. `apps/desktop`: one terminal surface wired to IPC.
6. `agentmux-store`: persist minimal workspace/session metadata.
7. `benches/single-terminal-latency`: first latency probe.

Do not start with a complete layout engine or browser automation. The first slice must prove the process, input, output, and render path.

## Initial Data Types

Core IDs:

```rust
pub struct WorkspaceId(String);
pub struct PaneId(String);
pub struct SurfaceId(String);
pub struct SessionId(String);
pub struct BackendAttachmentId(String);
```

Session state:

```rust
pub enum SessionState {
    Starting,
    Running,
    Detached,
    Recovering,
    Exited { code: Option<i32> },
    Failed { code: String, message: String },
    Lost,
}
```

Backend event:

```rust
pub enum BackendEvent {
    Started { session_id: SessionId },
    Output { session_id: SessionId, bytes: Vec<u8> },
    Resized { session_id: SessionId, columns: u16, rows: u16 },
    Exited { session_id: SessionId, code: Option<i32> },
    HealthChanged { attachment_id: BackendAttachmentId, state: BackendHealth },
    Error { session_id: Option<SessionId>, error: BackendError },
}
```

These are starting shapes, not final APIs. Keep them small until the first vertical slice works.

## Initial SQLite Tables

Minimum tables:

```sql
CREATE TABLE workspaces (
  workspace_id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  project_root TEXT,
  active_pane_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE panes (
  pane_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  parent_pane_id TEXT,
  kind TEXT NOT NULL,
  split_axis TEXT,
  split_ratio REAL,
  mounted_surface_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE surfaces (
  surface_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  surface_type TEXT NOT NULL,
  title TEXT NOT NULL,
  session_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE sessions (
  session_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  backend_kind TEXT NOT NULL,
  backend_native_id TEXT,
  cwd TEXT,
  command_json TEXT NOT NULL,
  state TEXT NOT NULL,
  exit_code INTEGER,
  durability TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

Migration rules:

- Every migration is versioned.
- Migrations are tested.
- Unknown enum values do not crash startup.
- Sensitive env values are not stored in `command_json`.

## First Task Breakdown

### Task 1: Core Types

Deliverables:

- id types
- workspace/session/surface/pane structs
- session state machine
- event enum
- unit tests

Done when:

- tests pass
- state transitions reject invalid terminal states

### Task 2: IPC Envelope

Deliverables:

- request/response envelope
- typed error codes
- schema field
- auth placeholder
- fixture tests

Done when:

- valid fixture parses
- invalid fixture returns typed error

### Task 3: ConPTY Prototype

Deliverables:

- spawn native shell
- read output
- write input
- resize
- exit event

Done when:

- integration test can run `echo agentmux`
- exit code is captured

### Task 4: Desktop Terminal Prototype

Deliverables:

- app shell
- terminal renderer adapter
- one mounted terminal surface
- keyboard input path
- resize path

Done when:

- user can type into native shell
- renderer unmount does not kill session in core prototype

### Task 5: Store MVP

Deliverables:

- SQLite migrations
- workspace/session persistence
- startup load
- simple recovery state marking

Done when:

- app restart restores workspace metadata
- non-durable session is marked disconnected rather than duplicated

### Task 6: WSL Direct

Deliverables:

- distribution discovery
- WSL launch
- path resolution
- typed diagnostics

Done when:

- selected distribution shell opens
- missing WSL and missing distribution are distinct errors

### Task 7: Durable WSL Prototype

Deliverables:

- tmux-control launch
- parser fixtures
- create/attach durable session
- output mapping
- detach/reattach smoke test

Done when:

- UI restart reconnects to an existing durable WSL shell
- no duplicate process is created

## Definition of Done

For implementation tasks:

- Code compiles.
- Unit tests for changed core logic pass.
- Integration or manual verification exists for platform behavior.
- Relevant docs are updated.
- Error paths have typed errors.
- User-visible failures have diagnostics.
- No unbounded queue or output buffer is introduced.

For backend tasks:

- Spawn failure is tested.
- Exit state is tested.
- Resize behavior is tested or manually documented.
- Logs redact command/env secrets where applicable.
- Recovery behavior is documented.

For UI tasks:

- Keyboard-only flow is considered.
- Focus state is visible.
- Text does not overlap in normal desktop window sizes.
- Hidden surfaces do not continue active rendering.

