# Goal 7 Agent Lifecycle and Notifications Status

Status: Complete
Date: 2026-06-18

This document records the current Goal 7 implementation slices for agent lifecycle signals, persisted notification history, and desktop attention visibility.

## Implemented

- `agentmux-ipc` now defines typed params/results for:
  - `agent.set_state`
  - `agent.get_state`
  - `agent.list_attention`
  - `agent.clear_attention`
  - `notification.list`
  - `notification.dismiss`
- `agentmux-core` tracks in-memory agent state per live session.
- `waiting_for_input` and `failed` set an attention flag; `agent.clear_attention` dismisses that flag without changing process state.
- `waiting_for_input`, `completed`, and `failed` create bounded in-memory notification history entries.
- Agent state changes emit `agent.state_changed` events.
- Notification creation emits `notification.created` events.
- `agentmux-core` detects explicit terminal output markers:
  - shell marker line: `::agentmux-agent {"state":"waiting_for_input","reason":"approval needed"}`
  - OSC marker: `ESC]777;agentmux;{"state":"completed","message":"tests passed"}BEL` or the same payload terminated with ST
- Marker-detected state changes use the same attention, notification, event, and deduplication path as `agent.set_state`.
- Optional heuristic output detection is a separate opt-in input path. It is disabled by default, runs only when explicitly enabled, skips heuristic parsing when an explicit marker is present in the same output batch, and emits source `heuristic_output`.
- `agent.state_changed` event payloads include `state`, `reason`, and `source` (`control_api`, `shell_marker`, `osc_777`, or `heuristic_output`).
- `agentmux-store` migration v2 persists recent agent state and notification history in SQLite.
- The desktop host synchronizes successful `agent.set_state` and `agent.clear_attention` results into SQLite.
- The desktop host also synchronizes runtime agent-state and notification snapshots after event collection, so marker-detected signals become visible through the persisted desktop APIs.
- Desktop `agent.get_state`, `agent.list_attention`, `notification.list`, and `notification.dismiss` read persisted state, so recent signals survive host restart.
- Newly generated notification ids include a timestamp component before the sequence number to avoid overwriting previous persisted notifications after restart.
- The Tauri desktop host registers `tauri-plugin-notification` and dispatches OS desktop notifications for `agent.needs_input`, `agent.completed`, and `agent.failed`.
- Desktop notification delivery is deduplicated per host run by notification id and ignores dismissed or non-agent notification types.
- The browser-preview desktop client exposes a test-only synthetic agent-state hook for UI automation.
- `agentmux-cli` maps agent and notification commands onto the local control pipe:
  - `agentmux agent set-state <session-id> <state> --reason <text>`
  - `agentmux agent get-state <session-id>`
  - `agentmux agent list-attention --workspace <id>`
  - `agentmux agent clear-attention <session-id>`
  - `agentmux notification list --workspace <id> --severity <level>`
  - `agentmux notification dismiss <notification-id>`
- The desktop UI now shows workspace attention counts in the sidebar, pane title attention badges, dismissible attention rows, and a notification history panel filtered by active workspace and severity.
- Playwright UI automation now drives a synthetic attention event and verifies the sidebar badge, pane badge, attention clear flow, notification panel, and notification dismissal flow.

## Validation

The following targeted checks passed on 2026-06-18 using the repository-local Rust toolchain:

```text
cargo fmt --all -- --check
cargo test -p agentmux-ipc -p agentmux-store -p agentmux-core -p agentmux-cli -p agentmux-desktop-host
cargo clippy -p agentmux-ipc -p agentmux-store -p agentmux-core -p agentmux-cli -p agentmux-desktop-host --all-targets -- -D warnings
npm --prefix apps/desktop run build
npm --prefix apps/desktop run test:ui
npm run check
npm --prefix apps/desktop run tauri:build -- --debug --no-bundle --ci
```

Covered behavior includes:

- agent and notification IPC params parsing
- agent state set/get/list-attention/clear-attention behavior
- shell marker and OSC 777 parser behavior
- opt-in heuristic detector input behavior and explicit-marker precedence
- terminal output marker integration into `agent.state_changed`, attention state, and notification history
- notification creation, listing, and dismissal
- `agent.state_changed` and `notification.created` event emission
- SQLite migration, persistence, filtering, and dismissal for agent states and notifications
- desktop host persisted agent/notification API routing
- desktop OS notification adapter filtering and deduplication
- CLI parsing for agent state and notification filters
- synthetic attention UI automation for sidebar, pane, panel, clear, and dismiss behavior
- TypeScript control client and desktop UI build

## Remaining Work

- No Goal 7-specific implementation work remains. The next roadmap group is Goal 8 Browser Surface Automation.

## Summary

Goal 7 now has the control-plane, CLI, explicit marker detector boundary, opt-in heuristic detector boundary, persisted metadata, OS desktop notifications, desktop UI projection, and automated UI coverage for advisory agent lifecycle state, attention dismissal, notification history, and event stream integration.
