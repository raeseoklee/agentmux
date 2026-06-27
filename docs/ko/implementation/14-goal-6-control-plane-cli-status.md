# Goal 6 Control Plane and CLI Status

Status: In progress
Date: 2026-06-19

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
- `CMUX_SOCKET_PATH` and `--socket` are accepted by the CLI as cmux-compatible
  aliases for the Windows control pipe path.
- The Tauri UI now obtains the runtime control token through a local bootstrap command instead of hard-coding the development token string.
- The CLI now maps the following commands onto the local control pipe:
  - `agentmux workspace create <name> --project <path>`
  - `agentmux workspace list`
  - `agentmux workspace get <workspace-id>`
  - `agentmux workspace rename <workspace-id> <name>`
  - `agentmux workspace close <workspace-id> --policy <policy> --yes`
  - `agentmux workspace group list`
  - `agentmux workspace group create <name> --workspace <workspace-id>`
  - `agentmux workspace group update <group-id> [name]`
  - `agentmux workspace group delete <group-id> --yes`
  - `agentmux workspace group add <group-id> <workspace-id>`
  - `agentmux workspace group remove <group-id> <workspace-id>`
  - `agentmux session spawn --workspace <id> -- <command>`
  - `agentmux session list --workspace <id>`
  - `agentmux session get <session-id>`
  - `agentmux session send-text <session-id> <text>`
  - `agentmux session send-key <session-id> <key>`
  - `agentmux session read-recent <session-id> --max-bytes <bytes>`
  - `agentmux session terminate <session-id> --mode <mode> --yes`
  - `agentmux events poll --workspace <id>`
  - `agentmux events watch --workspace <id>`
  - `agentmux browser open --workspace <id> --placement new-tab`
  - `agentmux browser navigate <surface-id> <url>`
  - `agentmux browser reload <surface-id>`
  - `agentmux browser back <surface-id>`
  - `agentmux browser forward <surface-id>`
  - `agentmux browser current-url <surface-id>`
  - `agentmux browser screenshot <surface-id> --format <format>`
  - `agentmux browser dom-snapshot <surface-id> [--frame <frame-id>]`
  - `agentmux browser frames <surface-id>`
  - `agentmux browser storage <surface-id>`
  - `agentmux browser cookies <surface-id>`
  - `agentmux browser downloads <surface-id> --limit <count>`
  - `agentmux browser history <surface-id>`
  - `agentmux browser console <surface-id> --limit <count>`
  - `agentmux browser dialogs <surface-id> --limit <count>`
  - `agentmux browser errors <surface-id> --limit <count>`
  - `agentmux browser click <surface-id> --selector <css> [--frame <frame-id>]`
  - `agentmux browser type <surface-id> <selector> [--frame <frame-id>] <text>`
  - `agentmux browser fill <surface-id> <selector> [--frame <frame-id>] <text>`
  - `agentmux browser press <surface-id> <selector> [--frame <frame-id>] <key>`
  - `agentmux browser select <surface-id> <selector> [--frame <frame-id>] <value...>`
  - `agentmux browser scroll <surface-id> [--frame <frame-id>] --y <pixels>`
  - `agentmux browser hover <surface-id> <selector> [--frame <frame-id>]`
  - `agentmux browser check <surface-id> <selector> [--frame <frame-id>]`
  - `agentmux browser get <surface-id> <selector> [--frame <frame-id>] --kind <text|html|value|attribute>`
  - `agentmux browser find <surface-id> <query> [--frame <frame-id>] --selector <css>`
  - `agentmux browser highlight <surface-id> <selector> [--frame <frame-id>] --duration-ms <ms>`
  - `agentmux browser focus <surface-id> <selector> [--frame <frame-id>]`
  - `agentmux browser zoom <surface-id> <percent>`
  - `agentmux browser wait-for-selector <surface-id> <selector> [--frame <frame-id>] --timeout-ms <ms>`
  - `agentmux browser evaluate <surface-id> [--frame <frame-id>] -- <script>`
  - `agentmux browser diagnostics --workspace <id>`
  - `agentmux ssh <user@host[:port]> --workspace <id>`
  - `agentmux ssh --profile <profile-name-or-id> --workspace <id>`
  - `agentmux diagnostics`
- Later parity work added a `cmux` binary target and cmux-style top-level
  aliases over the same control pipe for core script commands such as
  `list-workspaces`, `current-workspace`, `new-split`, `send`, `notify`,
  `sidebar-state`, `ping`, `capabilities`, and `identify`.
- The `cmux` binary can also use the native command families, including
  `cmux browser open|navigate|reload|back|forward|current-url|screenshot|dom-snapshot|frames|storage|cookies|downloads|history|console|dialogs|errors|click|type|fill|press|select|scroll|hover|check|get|find|highlight|focus|zoom|wait-for-selector|evaluate`
  and `cmux ssh <target-or-profile>`.
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
- CLI browser command parsing for browser surface creation, navigation,
  reload/back/forward/current-url, screenshot, frame-targeted DOM snapshot,
  frames, storage, cookies, downloads, history, console, dialogs, errors,
  frame-targeted click/type/fill/press/select/scroll/hover/check/get/find,
  frame-targeted highlight/focus, zoom, frame-targeted
  wait-for-selector/evaluate, and diagnostics commands
- CLI SSH command parsing for direct `user@host[:port]` targets and saved
  profile name/id targets
- desktop-host pipe dispatch into `DesktopControlState`
- existing `agentmux terminal run` ConPTY smoke behavior
- desktop host workspace/session/pane control behavior after moving Tauri state behind `Arc`
- `session.list` remains scoped to live runtime sessions; persisted/disconnected records stay in `workspace.get` and recovery diagnostics

## Remaining Work

- No remaining Goal 6 implementation items are currently tracked in this status note.

## Summary

Goal 6 now has a real local named pipe transport, per-user token discovery with owner-scoped token-file creation, API fixture coverage, live runtime session listing, polling and streaming event commands, and a CLI wrapper for workspace/session/event/browser/SSH/diagnostics control methods including confirmed destructive operations.
