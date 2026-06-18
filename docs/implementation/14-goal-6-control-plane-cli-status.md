# Goal 6 Control Plane and CLI Status

Status: In progress
Date: 2026-06-18

This document records the current Goal 6 implementation slice for local control plane IPC and CLI automation.

## Implemented

- `agentmux-ipc` now exposes a Windows named pipe transport for newline-delimited JSON control envelopes and event stream frames.
- The transport has client helpers, a concurrent streaming server loop helper, and one-request helpers used by IPC smoke tests.
- The default local pipe name is `\\.\pipe\agentmux-control`.
- The desktop host now loads or creates a per-user 32-byte random hex control token at startup.
- `AGENTMUX_CONTROL_TOKEN_PATH` can override the token file path for the desktop host and CLI.
- Newly-created token files use `0600` on Unix and a protected Owner Rights DACL on Windows.
- The desktop host starts a background control pipe server at app startup and dispatches incoming requests through the same `DesktopControlState` used by the Tauri command.
- `AGENTMUX_CONTROL_PIPE` can override the pipe name for the desktop host and CLI.
- `AGENTMUX_CONTROL_TOKEN` can override the CLI token.
- The Tauri UI now obtains the runtime control token through a local bootstrap command instead of hard-coding the development token string.
- The CLI now maps the following commands onto the local control pipe:
  - `agentmux workspace create <name> --project <path>`
  - `agentmux workspace list`
  - `agentmux workspace get <workspace-id>`
  - `agentmux workspace rename <workspace-id> <name>`
  - `agentmux workspace close <workspace-id> --policy <policy> --yes`
  - `agentmux session spawn --workspace <id> -- <command>`
  - `agentmux session list --workspace <id>`
  - `agentmux session get <session-id>`
  - `agentmux session send-text <session-id> <text>`
  - `agentmux session send-key <session-id> <key>`
  - `agentmux session read-recent <session-id> --max-bytes <bytes>`
  - `agentmux session terminate <session-id> --mode <mode> --yes`
  - `agentmux events poll --workspace <id>`
  - `agentmux events watch --workspace <id>`
  - `agentmux diagnostics`
- CLI commands default to compact human-readable text and support `--json` for response-envelope JSON.
- Destructive CLI commands require `--yes` or `--confirm` before invoking the local control pipe.
- The runtime control plane now implements `session.list` with an optional workspace filter.
- The runtime control plane now records a bounded in-memory event backlog and exposes `events.poll` with optional workspace, session, type, and max-event filters.
- `events.poll` reports cumulative dropped-event count; unmatched filtered events remain queued for later polling.
- The runtime control plane also records a bounded non-consuming event history for `events.subscribe` replay by `after_event_id`.
- `agentmux events watch` now uses `events.subscribe` over the named pipe and reconnects with the last observed event cursor.
- Control API fixture tests now lock representative request, success response, and error response envelope samples.

## Validation

The following targeted checks passed on 2026-06-18 using the repository-local Rust toolchain:

```text
cargo fmt --all -- --check
cargo test -p agentmux-ipc
cargo test -p agentmux-core
cargo test -p agentmux-cli
cargo test -p agentmux-desktop-host
cargo clippy -p agentmux-ipc -p agentmux-core -p agentmux-cli -p agentmux-desktop-host --all-targets -- -D warnings
npm run check
npm --prefix apps/desktop run tauri:build -- --debug --no-bundle --ci
```

Covered behavior includes:

- named pipe request/response round trip
- named pipe subscription response/event-frame stream round trip
- control API fixture request/response schema compatibility
- generated control token persistence and configured-token validation
- Windows token-file ACL verification through the file's DACL SDDL
- control-plane `session.list`, `events.poll`, and `events.subscribe` cursor replay behavior
- CLI token-file discovery plus parsing for workspace create/close, session spawn/list/read-recent/terminate, and event watch filters
- desktop-host pipe dispatch into `DesktopControlState`
- existing `agentmux terminal run` ConPTY smoke behavior
- desktop host workspace/session/pane control behavior after moving Tauri state behind `Arc`
- `session.list` remains scoped to live runtime sessions; persisted/disconnected records stay in `workspace.get` and recovery diagnostics

## Remaining Work

- No remaining Goal 6 implementation items are currently tracked in this status note.

## Summary

Goal 6 now has a real local named pipe transport, per-user token discovery with owner-scoped token-file creation, API fixture coverage, live runtime session listing, polling and streaming event commands, and a CLI wrapper for workspace/session/event/diagnostics control methods including confirmed destructive operations.
