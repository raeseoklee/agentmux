# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

AgentMux is a Windows-first, cross-platform terminal multiplexer for running many AI-agent sessions, shells, and browser-assisted workflows in parallel. The repository is in early implementation: a Rust core runtime, a Tauri desktop shell (React + TypeScript + xterm.js), and a CLI, built incrementally against the design and goal documents in `docs/`.

Implementation is **goal-driven**. The roadmap is split into Goals 0–9 (`docs/implementation/08-goal-groups.md`), each with explicit "Done when" criteria and a status doc (`docs/implementation/09-*`…`18-*`). Read the relevant goal/status doc before working on a feature area. Several design docs are written in Korean; the goal-group and status docs are in English.

## Commands

All commands run from the repo root. Primary shell is PowerShell; a Bash tool is also available.

```powershell
npm run check                  # full gate: cargo fmt --check, clippy -D warnings, cargo test --workspace, then doc-link check
npm run docs:check             # validate internal doc links (node tools/check-doc-links.mjs)
cargo test --workspace         # all Rust tests
npm run desktop:build          # tsc --noEmit + vite build for the desktop UI
npm --prefix apps/desktop run tauri:dev    # run the desktop app in dev
```

Run a single Rust test: `cargo test -p <crate> <test_name>` (e.g. `cargo test -p agentmux-ipc parses_session_spawn_params`).

Performance / smoke gates (PowerShell scripts under `tools/`, exposed as npm scripts):

```powershell
npm run perf:gates                      # run benchmark performance gates
npm run diagnostics:packaged-smoke
npm run tmux:reattach-smoke
npm run browser:cdp-smoke
cargo run -p agentmux-bench-single-terminal-latency   # individual bench
```

`npm run check` (and `tools/check.ps1`) will use a Rust toolchain vendored under `.toolchains/` if `cargo` is not on PATH. `tools/bootstrap-windows.ps1` checks for Git, Rust, Node.js, and npm.

The CI gate is `cargo fmt --check` + `cargo clippy -D warnings` + tests; **clippy warnings are hard errors**, so keep the workspace warning-clean.

## Architecture

### Crate layering (Rust workspace, see `Cargo.toml`)

The dependency direction is one-way: backends and UI never couple directly — the UI sees only the core control API and its event stream.

- `agentmux-backend` — the `SessionBackend` trait and shared types (`SpawnRequest`, `AttachRequest`, `InputEvent`, `BackendEvent`, `TerminalSize`, `TerminationMode`, `BackendHealth`). All backends implement this trait.
- `agentmux-backend-conpty` / `-wsl` / `-tmux` — the three concrete backends: Windows ConPTY, direct WSL shell, and durable WSL via tmux-control.
- `agentmux-core` — the runtime brain. `TerminalRuntime<B: SessionBackend>` owns sessions and recent-output buffers and translates `BackendEvent` → `CoreEvent`. `RuntimeControlPlane<B>` wraps it, owns auth/events/agent-state/notifications, and dispatches control requests in `handle_request` (method string → handler). The session lifecycle is a state machine — `SessionState::can_transition_to` is the single source of truth for legal transitions; respect it when adding states or events.
- `agentmux-ipc` — the wire protocol: `RequestEnvelope`/`ResponseEnvelope` (schema `agentmux.control.v1`), `EventFrame` (schema `agentmux.event.v1`), `ErrorCode`, all typed param/result structs, and the Windows named-pipe transport. Params/results cross the boundary as JSON strings inside the envelope (`params_json`, `result_json`); use `parse_params::<T>()` / `ok_typed(&result)`.
- `agentmux-store` — SQLite (WAL) persistence: versioned migrations and workspace/pane/surface/session metadata.
- `agentmux-browser`, `agentmux-telemetry` — browser-surface automation and telemetry.
- `agentmux-cli` — `agentmux` binary; talks to a running desktop instance over the named pipe.
- `apps/desktop/src-tauri` — crate `agentmux-desktop-host`; the Tauri host.
- `benches/*` — the performance benchmarks named in the perf gates.

### Two entry points into the same control plane

The desktop **host** (`apps/desktop/src-tauri`) constructs the `DesktopControlState`, then exposes it **two ways**: as a Tauri command (`agentmux_control`) that the React UI invokes in-process, *and* as a named-pipe server (`start_control_pipe_server`) that the CLI and external tools connect to. Both paths funnel into the same `RuntimeControlPlane::handle_request`. When adding a control method, wire it once in `handle_request` and it is reachable from both.

- **Auth**: every request carries an `Auth` token; the host generates/loads it via `load_or_create_control_token` and writes it to `%LOCALAPPDATA%\AgentMux\control.token` (overridable with `AGENTMUX_CONTROL_TOKEN_PATH`). Default pipe: `\\.\pipe\agentmux-control`.
- **Platform note**: the named-pipe transport in `agentmux-ipc` is Windows-only; the `#[cfg(not(windows))]` module returns `Unsupported`. Logic that must be testable cross-platform lives in `agentmux-core`, not in the transport.

### Events and agent detection

`CoreEvent`s are converted to `EventFrame`s with a monotonic cursor (`evt_00000001`, …); clients poll (`events.poll`) or subscribe with an `after_event_id` cursor (`events.subscribe`). Agent lifecycle state is detected from session output in `agentmux-core`: explicit shell markers (lines containing `::agentmux-agent`), OSC 777 sequences (`ESC]777;agentmux;…`), and optional opt-in heuristics. Attention-worthy states (`WaitingForInput`, `Failed`) generate notifications.

### Domain IDs

Typed newtypes with prefixes: `WorkspaceId` (`ws`), `PaneId` (`pane`), `SurfaceId` (`surf`), `SessionId` (`ses`), `BackendAttachmentId` (`att`). A **session** (execution object) outlives a **pane** (display object); hidden surfaces must stop active rendering but keep bounded scrollback.

### Tests & fixtures

Unit tests live inline (`#[cfg(test)]`) in each crate. Wire-protocol fixtures are in `tests/fixtures/control-plane/` and are asserted against the live structs (see `control_api_fixtures_match_current_schema` in `agentmux-ipc`) — if you change a param/result struct, update the matching fixture. Platform integration tests go under `tests/integration/`, separated by backend so CI can run the available subset.

## Conventions

Each feature PR is expected to record (per `docs/implementation/README.md`): the requirement ID or roadmap phase touched, the user-visible behavior, the backend/IPC/UI boundary involved, the test name or manual-verification steps, and any benchmark impact (or why it couldn't be measured).
