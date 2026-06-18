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
| `pane.split` | pane_id, axis, ratio, surface_id optional | pane tree |
| `pane.focus` | pane_id | focused pane |
| `pane.close` | pane_id, surface_policy | pane tree |
| `pane.mount_surface` | pane_id, surface_id | pane |
| `pane.unmount_surface` | pane_id | pane |
| `pane.resize_layout` | workspace_id, layout_patch | pane tree |

Surface policies:

- `detach_surface`
- `close_surface`
- `fail_if_session_running`

## Session Methods

| Method | Params | Result |
|---|---|---|
| `session.spawn` | workspace_id, backend, command, cwd, env, size | session |
| `session.attach` | workspace_id, backend, backend_ref | session |
| `session.list` | workspace_id optional | sessions |
| `session.get` | session_id | session detail |
| `session.send_text` | session_id, text | ack |
| `session.send_key` | session_id, key | ack |
| `session.paste` | session_id, text, bracketed optional | ack |
| `session.resize` | session_id, columns, rows | ack |
| `session.read_recent` | session_id, max_bytes | output chunk |
| `session.terminate` | session_id, mode | termination result |
| `session.detach` | session_id | detach result |

Termination modes:

- `soft`
- `interrupt`
- `kill`

## Surface Methods

| Method | Params | Result |
|---|---|---|
| `surface.list` | workspace_id optional | surfaces |
| `surface.create_terminal` | session_id | surface |
| `surface.create_browser` | workspace_id, profile optional | surface |
| `surface.close` | surface_id, close_policy | close result |
| `surface.rename` | surface_id, title | surface |

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

## Event Stream

Subscribe request:

```json
{
  "schema": "agentmux.control.v1",
  "id": "req_events",
  "method": "events.subscribe",
  "params": {
    "workspace_id": "ws_123",
    "types": ["session.state_changed", "agent.state_changed", "notification.created"]
  },
  "auth": {
    "token": "redacted"
  }
}
```

Event frame:

```json
{
  "schema": "agentmux.event.v1",
  "event_id": "evt_001",
  "type": "session.state_changed",
  "occurred_at": "2026-06-18T00:00:00Z",
  "workspace_id": "ws_123",
  "session_id": "ses_123",
  "data": {
    "from": "running",
    "to": "exited",
    "exit_code": 0
  }
}
```

Event stream rules:

- Events are ordered per connection as delivered by core.
- Terminal output may be batched and is not guaranteed to preserve one OS read per event.
- Event consumers must handle reconnect and resume cursor.
- Core may drop low-priority diagnostic events under pressure, but must report dropped count.

## CLI Mapping

Expected command shape:

```text
agentmux workspace create <name> --project <path>
agentmux workspace list
agentmux session spawn --workspace <id> --backend wsl-tmux-control -- <command>
agentmux session send-text <session-id> <text>
agentmux session send-key <session-id> enter
agentmux session read-recent <session-id> --max-bytes 8192
agentmux events watch --workspace <id>
agentmux diagnostics
```

CLI rules:

- CLI talks to the same IPC API as UI.
- CLI output defaults to human-readable text.
- `--json` returns API-shaped JSON for scripts.
- Commands that may kill processes require explicit flags.

## Compatibility Policy

- `schema` field changes only on breaking protocol revision.
- Unknown response fields must be ignored by clients.
- Unknown enum values must be displayed as `unknown:<value>` by UI.
- Removing a method requires deprecation period.
- API fixture tests must lock request and response samples.

