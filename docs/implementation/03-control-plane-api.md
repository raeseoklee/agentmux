# Control Plane API

Status: Draft
Date: 2026-06-18

이 문서는 AgentMux의 local automation API, CLI mapping, event stream contract를 정의한다. Control plane은 AI agent orchestration, CLI scripting, MCP integration, diagnostics의 기반이다.

## Goals

- UI 없이도 workspace, pane, session을 제어할 수 있다.
- API는 versioned이고 typed error를 반환한다.
- High-volume terminal output과 low-latency control commands를 분리한다.
- 모든 destructive operation은 명시적인 target id를 요구한다.
- API는 기본적으로 local user scope 안에서만 접근 가능하다.

## Transport

Initial transport:

- Windows named pipe
- JSON request/response envelope
- local auth token
- newline-delimited frames or length-prefixed frames

Current implementation note: the desktop host starts a Windows named pipe server on
`\\.\pipe\agentmux-control` by default. `AGENTMUX_CONTROL_PIPE` can override the pipe
name for local testing. Frames are newline-delimited JSON `RequestEnvelope` and
`ResponseEnvelope` values. The CLI uses the same pipe transport and falls back only
to an explicit error when the desktop runtime is not running.

Future transports may include:

- Unix domain socket for non-Windows platforms
- stdio bridge for MCP
- WebSocket only if explicitly local-bound and authenticated

## Authentication

MVP auth model:

- Core runtime creates a per-user random token.
- Token is stored in user config directory with owner-only permissions where available.
- CLI reads token from config.
- UI receives token through desktop runtime bootstrap.
- Requests without valid token return `unauthorized`.

Current implementation note: desktop startup loads or creates a 32-byte random
hex token at the shared AgentMux token path. `AGENTMUX_CONTROL_TOKEN_PATH` can
override that path. CLI commands use `AGENTMUX_CONTROL_TOKEN` when set, otherwise
read the token file. The Tauri UI obtains the token through an app-local bootstrap
command before sending control envelopes. On Unix, newly-created token files use
`0600`; on Windows, newly-created token files use a protected Owner Rights DACL
(`D:P(A;;FA;;;OW)`) through `CreateFileW`. Existing token files are read as-is
and are not rewritten automatically.

Security limits:

- This is same-user local control, not a multi-user sandbox.
- Do not expose network listener by default.
- Do not treat token as a substitute for OS process isolation.

## Envelope

Request:

```json
{
  "schema": "agentmux.control.v1",
  "id": "req_01",
  "method": "session.spawn",
  "params": {},
  "auth": {
    "token": "redacted"
  }
}
```

Response:

```json
{
  "schema": "agentmux.control.v1",
  "id": "req_01",
  "ok": true,
  "result": {}
}
```

Error response:

```json
{
  "schema": "agentmux.control.v1",
  "id": "req_01",
  "ok": false,
  "error": {
    "code": "session_not_found",
    "message": "Session does not exist or is not attached.",
    "details": {
      "session_id": "ses_123"
    }
  }
}
```

## Error Codes

| Code | Meaning |
|---|---|
| `unauthorized` | Missing or invalid local token |
| `invalid_request` | Envelope or params invalid |
| `unsupported_method` | Method unavailable in current version |
| `workspace_not_found` | Workspace id unknown |
| `pane_not_found` | Pane id unknown |
| `surface_not_found` | Surface id unknown |
| `session_not_found` | Session id unknown |
| `backend_unavailable` | Backend cannot be used on this machine |
| `backend_degraded` | Backend exists but cannot complete request |
| `spawn_failed` | Process launch failed |
| `attach_failed` | Durable attach failed |
| `timeout` | Operation exceeded deadline |
| `conflict` | Operation conflicts with current state |
| `permission_denied` | OS or policy denied operation |

## Workspace Methods

| Method | Params | Result |
|---|---|---|
| `workspace.list` | none | workspaces |
| `workspace.create` | name, project_root, backend_profile | workspace |
| `workspace.rename` | workspace_id, name | workspace |
| `workspace.focus` | workspace_id | workspace |
| `workspace.close` | workspace_id, close_policy | close result |
| `workspace.get` | workspace_id | workspace detail |

Close policies:

- `detach_sessions`
- `terminate_sessions`
- `fail_if_running`

## Pane Methods

| Method | Params | Result |
|---|---|---|
| `pane.split` | workspace_id, pane_id, axis, ratio optional | workspace detail |
| `pane.focus` | workspace_id, pane_id | workspace detail |
| `pane.close` | workspace_id, pane_id, surface_policy | workspace detail |
| `pane.mount_surface` | workspace_id, pane_id, surface_id | workspace detail |
| `pane.unmount_surface` | workspace_id, pane_id | workspace detail |
| `pane.resize_layout` | workspace_id, pane_id, ratio | workspace detail |

Current implementation note: `pane.split`, `pane.focus`, `pane.close`, `pane.resize_layout`, `pane.mount_surface`, and `pane.unmount_surface` are implemented in the desktop host. `pane.split` accepts `horizontal` or `vertical` axes and ratios from `0.1` through `0.9`; it splits only leaf panes and returns a full workspace detail snapshot. `pane.close` closes only leaf panes, collapses the remaining sibling into the split parent, supports `detach_surface`, `close_surface`, and `fail_if_session_running`, and returns a full workspace detail snapshot. `pane.resize_layout` updates a split pane ratio from `0.1` through `0.9`. `pane.mount_surface` moves a workspace-local surface onto one leaf pane, unmounting the same surface from any previous pane, and focuses the target pane. `pane.unmount_surface` clears a leaf pane's mounted surface while preserving the surface record.

Surface policies:

- `detach_surface`
- `close_surface`
- `fail_if_session_running`

## Session Methods

| Method | Params | Result |
|---|---|---|
| `session.spawn` | workspace_id, backend optional, backend_profile optional, command, cwd, env, size | session |
| `session.attach` | session_id optional, workspace_id, backend, backend_profile optional, backend_ref, size | session |
| `session.list` | workspace_id optional | sessions |
| `session.get` | session_id | session detail |
| `session.send_text` | session_id, text | ack |
| `session.send_key` | session_id, key | ack |
| `session.paste` | session_id, text, bracketed optional | ack |
| `session.resize` | session_id, columns, rows | ack |
| `session.read_recent` | session_id, max_bytes | output chunk |
| `session.terminate` | session_id, mode | termination result |
| `session.detach` | session_id | detach result |

Current implementation note: `session.spawn`, `session.attach`, `session.list`,
`session.get`, `session.send_text`, `session.send_key`, `session.resize`,
`session.terminate`, and `session.read_recent` are implemented in the runtime
control plane. `session.list` accepts an optional `workspace_id` filter and reports
live runtime sessions only. Persisted or disconnected session records remain
discoverable through `workspace.get` and `diagnostics.recovery`, where callers can
see recovery-oriented state without mixing it into the live control surface.

Termination modes:

- `soft`
- `interrupt`
- `kill`

Spawn backend names:

- `conpty` (default when omitted)
- `wsl-direct`
- `wsl-tmux-control`

For `wsl-direct`, `backend_profile` is the selected WSL distribution name. If it is omitted, the WSL default distribution is used.
For `wsl-tmux-control`, `backend_profile` is also the selected WSL distribution name; the durable tmux session name is derived from `workspace_id`.
For `session.attach`, callers may provide `session_id` when recovering a persisted session; otherwise the runtime may allocate a new session id.
If the selected distribution is missing, the control response uses `backend_unavailable` with backend detail code `wsl_distribution_not_found`.
If WSL cwd resolution fails, the control response uses `invalid_request` with backend detail code `invalid_wsl_cwd`.
If WSL launch exceeds its deadline, the control response uses `timeout` with backend detail code `wsl_launch_timeout`.

## Surface Methods

| Method | Params | Result |
|---|---|---|
| `surface.list` | workspace_id optional | surfaces |
| `surface.create_terminal` | session_id | surface |
| `surface.create_browser` | workspace_id, pane_id optional, profile optional | surface |
| `surface.close` | surface_id, close_policy | close result |
| `surface.rename` | surface_id, title | surface |

Current implementation note: the desktop host implements
`surface.create_browser` for persisted browser surfaces. It creates a browser
surface record, assigns a browser id from the configured browser automation
adapter, and mounts the surface into the requested pane or the workspace active
pane. Browser surface metadata is included in `workspace.get` through the
existing surface summary fields. `AGENTMUX_BROWSER_AUTOMATION=auto|cdp|memory`
selects the adapter, with `auto` preferring CDP when Edge, Chrome, or Chromium
is discoverable and falling back to the deterministic in-memory adapter.
`AGENTMUX_BROWSER_EXECUTABLE` can pin the CDP browser executable.

## Agent Methods

| Method | Params | Result |
|---|---|---|
| `agent.set_state` | session_id, state, reason optional | agent state |
| `agent.get_state` | session_id | agent state |
| `agent.list_attention` | workspace_id optional | sessions needing attention |
| `agent.clear_attention` | session_id | ack |

Rules:

- Agent state is advisory product state.
- Process lifecycle remains owned by session backend.
- Heuristic detectors may propose state changes but must not terminate processes.

Current implementation note: `agent.set_state`, `agent.get_state`,
`agent.list_attention`, and `agent.clear_attention` are implemented in the
runtime control plane. Agent states are advisory product signals keyed by
session id. `waiting_for_input` and `failed` set the attention flag; `completed`,
`failed`, and `waiting_for_input` create notification history entries and emit
`agent.state_changed` plus `notification.created` events. The desktop host
persists successful state changes in SQLite and serves persisted
`agent.get_state` / `agent.list_attention` responses so recent attention state
survives host restart.

Explicit marker inputs:

- Shell marker line: `::agentmux-agent {"state":"waiting_for_input","reason":"approval needed"}`
- Terminal notification OSC: `ESC]777;agentmux;{"state":"completed","message":"tests passed"}BEL`
- The OSC form may also use ST (`ESC\`) as the terminator.

Marker payload rules:

- Payload is JSON.
- `state` or `event` is required.
- `reason` or `message` is optional.
- Supported states include the public state names plus aliases such as
  `agent.awaiting_input`, `awaiting_input`, `needs_input`, `agent.completed`,
  and `agent.failed`.
- Marker-detected states are advisory only. They share the same deduplicated
  attention, notification, and event path as `agent.set_state`.

Optional heuristic inputs:

- Heuristic output detection is disabled by default and must be enabled by an
  explicit runtime setting.
- Heuristic parsing consumes ordinary terminal output separately from explicit
  shell/OSC marker parsing.
- If an explicit marker appears in the same output batch, explicit marker parsing
  wins and heuristic parsing is skipped for that batch.
- The current heuristic input recognizes conservative waiting-for-input /
  approval-needed phrases and emits `waiting_for_input` with source
  `heuristic_output`.
- Heuristic-detected states are advisory only, must be dismissible, and must not
  terminate or interrupt session backends.

## Notification Methods

| Method | Params | Result |
|---|---|---|
| `notification.list` | workspace_id optional, severity optional, include_dismissed optional | notifications |
| `notification.dismiss` | notification_id | ack |

Current implementation note: runtime notification history remains bounded
in-memory for event emission, and the desktop host synchronizes notification
summaries into SQLite for durable `notification.list` and `notification.dismiss`
behavior. `notification.list` returns newest first and hides dismissed
notifications unless `include_dismissed` is true. Browser automation failures
are persisted as `browser.action_failed` notifications with `error` severity.

## Browser Methods

| Method | Params | Result |
|---|---|---|
| `browser.navigate` | surface_id, url | navigation result |
| `browser.screenshot` | surface_id, format | image handle |
| `browser.dom_snapshot` | surface_id | snapshot |
| `browser.click` | surface_id, selector or coordinates | action result |
| `browser.type` | surface_id, selector, text | action result |
| `browser.evaluate` | surface_id, script | evaluation result |

Rules:

- Browser operation must target explicit `surface_id`.
- The operation must not silently switch to another browser surface.
- Screenshot payloads should use artifact handles instead of huge inline JSON where possible.

Current implementation note: the desktop host implements the browser command
methods against the configured browser automation adapter. The CDP adapter maps
the API to Chrome DevTools Protocol for navigation, screenshot capture, DOM
snapshot, selector click, coordinate click, text insertion, and script
evaluation; the in-memory adapter remains available for deterministic tests and
fallback. Commands first validate that the target `surface_id` exists in
persisted workspace metadata and has `surface_type = "browser"`. Missing
surfaces return `surface_not_found`; terminal or other non-browser surfaces
return `invalid_request`. `browser.screenshot` returns an image handle and byte
count rather than inline bytes. Failed browser commands are recorded in
`diagnostics.browser` and persisted as `browser.action_failed` notifications.

## Diagnostics Methods

| Method | Params | Result |
|---|---|---|
| `diagnostics.browser` | workspace_id optional, surface_id optional | recent browser automation failures |
| `diagnostics.export` | none | diagnostics bundle with recovery, browser, notification, backend health, and queue pressure summaries |
| `diagnostics.recovery` | none | recovery metadata counts and session recovery states |
| `diagnostics.wsl_distributions` | none | WSL distribution names and default flags |

The recovery diagnostics result should expose enough information to prove startup recovery did not duplicate non-durable sessions and can identify durable sessions waiting for backend attach.
Browser diagnostics are bounded to recent failures and include surface id,
workspace id when known, operation, error code, message, and occurrence time.
Diagnostics export intentionally returns metadata and bounded histories only. It
includes backend health derived from persisted session state and queue pressure
for runtime event queues, runtime notifications, and desktop browser failure
history; it does not include unlimited terminal output.

## Event Stream

Polling request:

```json
{
  "schema": "agentmux.control.v1",
  "id": "req_events_poll",
  "method": "events.poll",
  "params": {
    "workspace_id": "ws_123",
    "session_id": "ses_123",
    "types": ["session.state_changed"],
    "max_events": 128
  },
  "auth": {
    "token": "redacted"
  }
}
```

Subscribe request:

```json
{
  "schema": "agentmux.control.v1",
  "id": "req_events",
  "method": "events.subscribe",
  "params": {
    "workspace_id": "ws_123",
    "types": ["session.state_changed", "agent.state_changed", "notification.created"],
    "after_event_id": "evt_00000042"
  },
  "auth": {
    "token": "redacted"
  }
}
```

Subscribe response:

```json
{
  "schema": "agentmux.control.v1",
  "id": "req_events",
  "ok": true,
  "result": {
    "subscribed": true,
    "cursor": "evt_00000042",
    "dropped_count": 0
  }
}
```

Event frame:

```json
{
  "schema": "agentmux.event.v1",
  "event_id": "evt_00000043",
  "event_type": "session.state_changed",
  "occurred_at": "2026-06-18T00:00:00Z",
  "workspace_id": "ws_123",
  "session_id": "ses_123",
  "data_json": "{\"from\":\"running\",\"to\":\"exited\",\"exit_code\":0}"
}
```

`agent.state_changed` frames use the same envelope and include this `data_json`
shape:

```json
{
  "state": "waiting_for_input",
  "reason": "approval needed",
  "source": "shell_marker"
}
```

Event stream rules:

- Events are ordered per connection as delivered by core.
- Terminal output may be batched and is not guaranteed to preserve one OS read per event.
- Event consumers must handle reconnect and resume cursor.
- Core may drop low-priority diagnostic events under pressure, but must report dropped count.

Current implementation note: `events.poll` and `events.subscribe` are implemented
in the runtime control plane and local named pipe transport. `events.poll` drains
matching events from a bounded in-memory poll queue and preserves unmatched
events for later polling. `events.subscribe` sends an initial response envelope
and then newline-delimited `EventFrame` JSON values on the same pipe connection.
The runtime keeps a bounded non-consuming event history for subscribe replay via
`after_event_id`; the CLI `events watch` command uses this stream and reconnects
with the last observed cursor.

## CLI Mapping

Expected command shape:

```text
agentmux workspace create <name> --project <path>
agentmux workspace list
agentmux workspace close <workspace-id> --policy fail_if_running --yes
agentmux session spawn --workspace <id> --backend wsl-tmux-control -- <command>
agentmux session list --workspace <id>
agentmux session send-text <session-id> <text>
agentmux session send-key <session-id> enter
agentmux session read-recent <session-id> --max-bytes 8192
agentmux session terminate <session-id> --mode soft --yes
agentmux agent set-state <session-id> waiting_for_input --reason "approval needed"
agentmux agent list-attention --workspace <id>
agentmux notification list --severity warning
agentmux events poll --workspace <id>
agentmux events watch --workspace <id>
agentmux diagnostics
agentmux diagnostics export --json
```

CLI rules:

- CLI talks to the same IPC API as UI.
- CLI output defaults to human-readable text.
- `--json` returns API-shaped JSON for scripts.
- Commands that remove workspaces or stop sessions require `--yes` or `--confirm`.

Current implementation note: the CLI maps `workspace create`, `workspace list`,
`workspace get`, `workspace rename`, `session spawn`, `session list`,
`workspace close`, `session get`, `session send-text`, `session send-key`,
`session read-recent`, `session terminate`, `agent set-state`, `agent get-state`,
`agent list-attention`, `agent clear-attention`, `notification list`,
`notification dismiss`, `events poll`, `events watch`, `diagnostics`, and
`diagnostics export` onto the local named pipe control API. `workspace close` and
`session terminate` require
`--yes` or `--confirm`. `--json` prints the response envelope, while the default
output is compact human-readable text. `events watch` uses `events.subscribe` and
accepts `--after-event <event-id>` for cursor resume.

## Compatibility Policy

- `schema` field changes only on breaking protocol revision.
- Unknown response fields must be ignored by clients.
- Unknown enum values must be displayed as `unknown:<value>` by UI.
- Removing a method requires deprecation period.
- API fixture tests must lock request and response samples.

