# Implementation Goal Groups

Status: Draft
Date: 2026-06-18

This document turns the implementation roadmap into goal-sized work groups that can be executed and verified independently.

## Goal 0: Repository Foundation

Deliverables:

- Rust workspace and crate layout.
- Desktop app workspace placeholder.
- Benchmark, fixture, integration test, and tool directories.
- Formatter, linter, unit test, docs check, and CI entrypoints.
- Windows bootstrap documentation.

Done when:

- `cargo test --workspace` can run on a Rust-enabled machine.
- Desktop placeholder can build with the documented Node command.
- CI has Rust, desktop build, and docs check jobs.
- Documentation links are checked by script.

## Goal 1: Native Terminal Vertical Slice

Deliverables:

- Core IDs, state machine, workspace/session/pane/surface types.
- Shared backend trait and event types.
- Minimal ConPTY backend.
- IPC envelope and typed error model.
- One terminal surface in the desktop UI.
- First single-terminal latency probe.

Done when:

- A native Windows shell opens from the app.
- Input, output, resize, and exit status work end to end.
- Basic latency benchmark runs locally.

## Goal 2: Persistence and Recovery Base

Deliverables:

- SQLite migrations.
- Workspace, pane, surface, and session metadata persistence.
- Startup load path.
- Non-durable recovery state handling.
- Sensitive metadata redaction policy.

Done when:

- Workspace metadata survives app restart.
- Non-durable sessions are not duplicated on restart.
- Migration and recovery unit tests pass.

## Goal 3: WSL Direct Shell

Deliverables:

- WSL distribution discovery.
- Windows-to-WSL path resolution.
- Direct WSL shell launch.
- Typed WSL diagnostics.

Done when:

- A selected distribution shell opens in the expected directory.
- Input, output, resize, and exit work.
- Missing WSL, missing distribution, and invalid cwd are distinct errors.

## Goal 4: Durable WSL tmux Backend

Deliverables:

- tmux-control launcher.
- Fixture-driven parser.
- Create, attach, detach, list, and recover commands.
- AgentMux session id to backend pane id mapping.

Done when:

- UI restart reconnects to a durable WSL session.
- Long-running processes are not duplicated.
- Parser fixture tests pass.

## Goal 5: Workspace and Pane UX

Deliverables:

- Workspace CRUD.
- Split pane layout tree.
- Surface mount and unmount lifecycle.
- Safe close flow.
- Hidden renderer lifecycle policy.

Done when:

- Users can create, rename, focus, split, and close workspaces and panes.
- Hidden terminal surfaces stop active rendering work.
- Running sessions require explicit close policy.

## Goal 6: Control Plane and CLI

Deliverables:

- Local named pipe IPC server.
- Auth token.
- Workspace, pane, surface, and session methods.
- Event subscription or polling.
- CLI wrapper.
- API fixture tests.

Done when:

- CLI can create and inspect workspaces and sessions.
- CLI can send input and read recent output.
- Event stream reports session state changes.

## Goal 7: Agent Lifecycle and Notifications

Deliverables:

- Agent lifecycle state machine.
- Hook, marker, and heuristic detector boundary.
- Attention, completed, and failed signals.
- Notification history.
- Workspace and pane badges.

Done when:

- Agent state is visible without opening every pane.
- Attention can be dismissed.
- Notification history filters by workspace and severity.

## Goal 8: Browser Surface Automation

Deliverables:

- Browser surface model.
- Browser pane UI.
- Navigation, screenshot, DOM snapshot, click, type, and evaluate API.
- Explicit surface scoping.

Done when:

- Users can create browser surfaces in panes.
- Automation targets only the requested browser surface.
- Browser failures surface through diagnostics.

## Goal 9: Performance, Diagnostics, and Release Candidate

Deliverables:

- Idle session benchmarks.
- High-output stress benchmark.
- Resize storm benchmark.
- Restart recovery benchmark.
- Diagnostics export.
- Packaging and release checklist.

Done when:

- Release performance gates are recorded on a reference Windows machine.
- Diagnostics expose backend health and queue pressure.
- Release checklist has no known blocker from the requirements document.
