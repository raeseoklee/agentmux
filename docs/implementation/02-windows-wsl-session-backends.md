# Windows and WSL Session Backends

Status: Draft
Date: 2026-06-18

이 문서는 AgentMux의 session backend 구현 기준을 정의한다. 첫 타겟은 Windows이며, WSL 기반 Linux 개발 환경과 durable terminal session을 핵심 제품 경로로 다룬다.

## Backend Requirements

Backend는 다음 공통 기능을 제공해야 한다.

- spawn: 새 session 실행
- attach: 기존 durable session 재연결
- input: byte/text/key 입력
- resize: terminal size 변경
- output: terminal byte stream 전달
- state: running, exited, detached, failed 전달
- diagnostics: backend-specific 상태 보고
- terminate: soft/hard 종료

Backend는 다음을 UI에 직접 노출하지 않는다.

- OS handle
- raw process object
- tmux pane id 같은 backend-native id
- shell escaping detail
- platform-specific recovery command

## Backend Selection

| User intent | Default backend | Notes |
|---|---|---|
| Windows native shell | `conpty` | PowerShell, cmd, native Windows CLI |
| WSL one-off shell | `wsl-direct` | 빠른 실행, durability 낮음 |
| WSL agent session | `wsl-tmux-control` | durable session, restart recovery 우선 |
| Automation benchmark | 명시 선택 | benchmark 재현성을 위해 backend 고정 |

Backend selection은 config에서 override할 수 있어야 한다.

```toml
[backend]
default_windows = "conpty"
default_wsl = "wsl-tmux-control"

[wsl]
default_distribution = "Ubuntu"
prefer_tmux_control = true
```

## Native Windows Backend

Backend kind: `conpty`

Responsibilities:

- ConPTY pseudoconsole 생성
- child process spawn
- stdin/stdout/stderr stream 연결
- terminal resize 전달
- exit code 수집
- process tree termination 정책 적용

Spawn fields:

- `command`: executable and args
- `cwd`: Windows path
- `env`: sanitized environment overlay
- `initial_size`: columns and rows
- `shell_kind`: `powershell`, `cmd`, `custom`

Error cases:

| Error | Meaning | User action |
|---|---|---|
| `conpty_unavailable` | OS가 required API를 제공하지 않음 | Windows version 확인 |
| `spawn_denied` | executable 실행 불가 | command/cwd/env 확인 |
| `cwd_not_found` | working directory 없음 | project path 수정 |
| `process_exited_early` | spawn 직후 종료 | output과 exit code 표시 |

Implementation notes:

- Process launch는 shell escaping을 최소화하기 위해 executable/args 배열로 처리한다.
- PowerShell profile loading 여부는 config로 제어한다.
- Ctrl+C, Ctrl+D, Enter, function key 등 named key는 renderer key event에서 backend input event로 변환한다.
- `resize`는 최신 size만 유지하고 짧은 debounce window로 coalesce한다.

## WSL Distribution Discovery

Backend kinds: `wsl-direct`, `wsl-tmux-control`

Discovery command:

- `wsl.exe --list --quiet`

Required outputs:

- distribution name
- default distribution flag if available
- WSL availability status
- actionable diagnostics

Discovery must distinguish:

| Case | Diagnostic |
|---|---|
| `wsl.exe` missing | WSL feature unavailable |
| no distributions | User must install a distribution |
| distribution stopped | Not an error; launch may start it |
| command timeout | WSL subsystem unhealthy or blocked |

## Windows Path to WSL Path

Path resolution policy:

1. If user supplies explicit WSL path, use it as-is.
2. If workspace root is a Windows drive path, convert through `wslpath`.
3. If `wslpath` fails, fall back to `/mnt/<drive>/...` only when deterministic.
4. Persist both original Windows path and resolved WSL path.

Examples:

| Windows path | WSL path |
|---|---|
| `C:\Users\irae\project` | `/mnt/c/Users/irae/project` |
| `D:\work\repo` | `/mnt/d/work/repo` |

Rules:

- Do not silently rewrite paths with spaces through string concatenation.
- Prefer argument arrays over shell strings.
- Record path conversion failures with command, exit code, stderr excerpt.

## WSL Direct Backend

Backend kind: `wsl-direct`

Launch shape:

```text
wsl.exe --distribution <name> --cd <wsl-cwd> --exec <shell-or-command> <args...>
```

Use cases:

- quick shell
- diagnostics
- environments where durable backend is unavailable
- test baseline against direct WSL behavior

Limitations:

- App restart generally loses process attachment.
- Output recovery depends on AgentMux snapshots, not backend durability.
- Long-running agent sessions should default to durable backend when available.

Exit criteria:

- Selected distribution opens in expected directory.
- Resize and input work.
- Distribution/cwd/command failures produce typed errors.

## Durable WSL Backend

Backend kind: `wsl-tmux-control`

Purpose:

- Keep shell and agent sessions alive independently of visible UI panes.
- Reattach after UI restart.
- Mirror output and lifecycle through tmux control mode.
- Use WSL Linux environment as the durable execution substrate.

### Session Mapping

AgentMux concepts map to backend concepts as follows:

| AgentMux | Backend |
|---|---|
| Workspace | tmux session group or named session |
| Session | tmux pane or window-backed command |
| Surface | AgentMux view of a backend pane |
| Pane | AgentMux UI layout node, independent from backend layout |

AgentMux UI layout should not be forced to equal backend layout. Backend pane/window IDs are execution references; UI panes are presentation references.

### Naming

Recommended durable session name:

```text
agentmux_<workspace_id_short>
```

Recommended metadata:

- workspace id
- session id
- backend pane id
- distribution
- WSL cwd
- command
- created time
- last attach time

Do not derive security decisions from names. Names are diagnostics and lookup hints only.

### Control Client Lifecycle

Attach sequence:

1. Resolve distribution.
2. Resolve WSL cwd.
3. Ensure durable backend binary is available or use system command directly.
4. Start control mode client.
5. Attach to existing named session or create it.
6. Query windows and panes.
7. Rebuild mapping with persisted metadata.
8. Subscribe output events.
9. Publish recovered sessions to core.

Detach sequence:

1. Stop accepting new input for detached sessions.
2. Flush pending output batches.
3. Persist last known backend ids and snapshot cursor.
4. Close control client transport without killing durable backend session.

### Parser

The tmux-control parser must be fixture-driven.

Parser outputs:

- `%begin`
- `%end`
- `%error`
- `%exit`
- `%output`
- pane/window/session add/remove/change events
- layout changes
- command response lines

Parser requirements:

- Preserve byte payload ordering per backend pane.
- Decode escaped output only once.
- Tolerate partial lines from transport.
- Tolerate unknown control messages by emitting diagnostics, not crashing.
- Associate command responses with request ids when possible.

Fixture categories:

- simple command response
- output with escape sequences
- high-volume output split across reads
- pane death
- server exit
- unknown line
- malformed output payload
- attach to existing session

### Input

Input event types:

- `Text`
- `Paste`
- `Key`
- `Control`
- `Resize`

Rules:

- Text input goes to the focused AgentMux session, then backend pane.
- Paste can be bracketed paste when terminal mode indicates support.
- Named keys are translated at the backend boundary.
- Input to detached or missing backend pane returns typed error.

### Resize

AgentMux supports two resize concepts:

- visual pane size in desktop UI
- backend terminal dimensions

For durable backend:

- Resize only sessions mounted in visible leaf panes by default.
- Hidden sessions keep last known size unless config requests background resize.
- Resize events are coalesced per session.
- Backend resize failure marks session as degraded but does not kill it.

### Output Backpressure

Durable backend output can exceed UI consumption rate. Required behavior:

- Visible mounted sessions receive priority batches.
- Hidden sessions append to bounded ring and compact snapshots.
- If a backend supports pausing pane output, core may use it for extreme overload.
- If pausing is unavailable, core drops oldest hidden rendered deltas after marking truncation.
- Output truncation must create a diagnostics event.

### Recovery

Recovery cases:

| Case | Expected behavior |
|---|---|
| UI restart, backend alive | Reattach and restore surfaces |
| Core restart, backend alive | Reattach using persisted metadata |
| Backend server dead | Mark sessions lost, show last snapshot |
| Pane missing but session metadata exists | Mark session lost individually |
| Parser transport lost | Attempt bounded reconnect, then mark degraded |

Recovery must not duplicate long-running agent processes. A create operation during recovery must be blocked until attach/list result is known.

## Backend Diagnostics

Each backend must expose:

- backend kind
- attachment id
- health state
- pid or transport id where safe
- command line redacted as needed
- last output timestamp
- last input timestamp
- buffered bytes
- dropped output count
- active sessions
- recent typed errors

Diagnostics API is part of supportability, not just development logging.

## Security Rules

- Never persist unsanitized environment secrets.
- Redact tokens in command diagnostics.
- Local IPC token must be required for backend control operations.
- Browser automation and terminal input APIs must be scoped to explicit surface/session ids.
- Do not expose raw WSL command strings in logs when args may contain secrets.

