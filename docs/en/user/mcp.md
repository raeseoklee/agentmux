# AgentMux MCP Control Plane

AgentMux exposes a local Model Context Protocol (MCP) server so Codex, Claude
Code, and other MCP clients can inspect and operate the running AgentMux desktop
through its authenticated Windows named-pipe control plane.

## Availability

The installed `agentmux.exe` provides these local stdio commands:

```powershell
agentmux mcp serve --help
agentmux mcp doctor --help
agentmux mcp setup --help
```

`mcp serve` can initialize and answer protocol discovery such as `initialize`
and `tools/list` without the AgentMux desktop. Tools that inspect or change
desktop state require the desktop control plane to be running. A successful
`mcp doctor` also requires that connection. `mcp help`, command help, and
`mcp setup` preview mode do not start or require the desktop application.

## Choose the Least-Privilege Profile

Always start with the narrowest profile that can complete the workflow.

| Profile | Capabilities | Recommended use |
| --- | --- | --- |
| `read` | Lists workspaces, sessions, agent attention, pane workers, integration readiness, team messages and tasks; reads terminal output, browser snapshots, events, context, and diagnostics. | Monitoring, status collection, and first-time client setup. |
| `standard` | Includes `read`, then adds pane focus, terminal open/split/input, pane-worker start/send, browser open/navigation/click/fill, team messaging/task updates, and agent-state updates. This is a trusted write and command-execution profile: terminal tools can run arbitrary commands and browser tools can mutate external systems. | Trusted interactive agent workflows that require command execution or writes. |
| `full` | Includes `standard`, then adds pane-worker stop, integration shim setup, workspace/pane/surface close, session termination, config updates, browser JavaScript evaluation, action execution, and notification clearing. | Trusted operator automation that genuinely needs destructive or high-impact actions. |

`standard` is not a safe or non-destructive profile. Grant it only to a client
that may execute commands and change terminal, browser, and shared coordination
state. Profiles are enforced on every MCP tool call, but they are tool-surface
policy, not a security sandbox against another process running as the same
Windows user. A client that can execute arbitrary terminal commands may invoke
other programs with that user's authority. Tool annotations describe risk and
do not provide containment. Do not grant `full` merely to avoid choosing tools.

Desktop control-plane calls use the local token, and mutations sent through
that control plane are attributed to their MCP profile in audit diagnostics.
`agent_integration_setup` is an exception: the MCP server performs it directly,
so it does not appear in desktop control-plane audit records. It writes shims to
the integration directory selected by `AGENTMUX_CMUXTERM_HOME`, the compatible
`CMUXTERM_HOME`, or the default user directory. With `add_to_user_path: true`,
it also modifies the Windows user `PATH`.

## Agent Pane Workers and tmux Integrations

AgentMux exposes typed MCP tools for agent-oriented panes:

| Tool | Profile | Purpose |
| --- | --- | --- |
| `agent_worker_list` | `read` | List AgentMux-managed tmux integration workers and independent Codex pane workers. |
| `agent_integration_status` | `read` | Diagnose Claude Teams, OMO, OMX, or OMC wrapper and WSL readiness, using the selected workspace's default WSL distribution when available. |
| `agent_worker_start` | `standard` | Split the active pane or create a tab, then start `claude-teams`, `omo`, `omx`, `omc`, or `codex-pane`. |
| `agent_worker_send` | `standard` | Send literal instructions to a worker and optionally submit Enter. |
| `agent_worker_stop` | `full` | Terminate a worker session. |
| `agent_integration_setup` | `full` | Install shared tmux-compatible wrappers and optionally add their directory to the Windows user PATH. |

Use `kind: "claude-teams"` for Claude Code Agent Teams. The lead process gets
the AgentMux tmux shim, and descendants created by `tmux split-window` inherit
the integration environment while receiving pane-specific `TMUX` and
`TMUX_PANE` identities from the desktop host.

Use `kind: "codex-pane"` for an independent Codex CLI process in another
AgentMux pane. This is not a Codex built-in subagent: Codex owns its internal
`/agent` threads, and AgentMux cannot move or terminate those threads as panes.
The explicit name prevents an MCP client from presenting an independent CLI
process as a native Codex subagent.

`agent_worker_send` and `agent_worker_stop` reject ordinary terminal sessions.
If worker metadata registration fails after a launch, AgentMux stops the new
session and closes its pane instead of leaving an untracked worker behind.

Start with `agent_integration_status`, use `agent_integration_setup` only from
an approved `full` client, and then use `agent_worker_start`. `team_message_send`
operates AgentMux's shared mailbox; steering an arbitrary terminal TUI is done
with `agent_worker_send` or the lower-level terminal input tools.

The shim captures the WSL-side `PATH` separately when it crosses into
`agentmux.exe`; it never copies the Windows process `PATH` into a Linux child.
The desktop restores the captured WSL path inside each integration child so
recursive `tmux split-window` calls continue to resolve the AgentMux shim.
On restart, persisted integration launch settings are reused and disconnected
tmux-owned child workers are respawned into their original panes. Their saved
command and working directory are restored independently so a lead crash or a
desktop restart does not leave a permanently empty worker pane.

## Run a Local stdio Server

For a temporary read-only server:

```powershell
agentmux mcp serve --profile read
```

The default control pipe and token file are discovered automatically. Advanced
launches can override them:

```powershell
agentmux mcp serve --profile standard `
  --pipe agentmux-control `
  --token-file "$env:LOCALAPPDATA\AgentMux\control.token"
```

Use either `--token` or `--token-file`, never both. Environment-based overrides
are also available through `AGENTMUX_CONTROL_PIPE`, `AGENTMUX_CONTROL_TOKEN`,
and `AGENTMUX_CONTROL_TOKEN_PATH`. Avoid putting a token directly in a checked-in
client configuration.

## Diagnose the Connection

Run the doctor after the desktop is open:

```powershell
agentmux mcp doctor --profile read --json
```

The result reports the stdio transport, selected profile, control-pipe name,
token source, control-plane reachability, schema, and errors. A nonzero exit code
means the MCP server should not be registered yet. Start AgentMux and confirm
that the selected Windows account can read the control token before retrying.

## Configure Codex

AgentMux uses Codex's TOML configuration, normally
`%USERPROFILE%\.codex\config.toml` or `%CODEX_HOME%\config.toml`.

Preview a read-only registration first:

```powershell
agentmux mcp setup --client codex --profile read --json
```

Apply the reviewed change explicitly:

```powershell
agentmux mcp setup --client codex --profile standard --install --json
```

The managed entry runs the installed absolute path as:

```text
agentmux.exe mcp serve --profile standard
```

## Configure Claude Code

Claude Code uses JSON configuration, normally `%USERPROFILE%\.claude.json`.

```powershell
agentmux mcp setup --client claude --profile read --json
agentmux mcp setup --client claude --profile standard --install --json
```

`setup` is preview-only unless `--install` is present. Installation preserves
unrelated settings, rejects an unrelated server already named `agentmux`, and
re-reads and re-merges the latest client file before an atomic replacement so a
concurrent Codex or Claude change is not replaced by the preview snapshot. A
timestamped backup captures the exact latest snapshot that is replaced. If the
file keeps changing during compare-and-replace attempts, setup stops without
writing instead of overwriting the other client.

Use `--config <path>` to target a non-default client file or
`--executable <absolute-path-to-agentmux.exe>` when configuring a different
installed copy.

## Remote Streamable HTTP

Start the desktop first, then opt into an authenticated Streamable HTTP endpoint
from desktop-bridge server mode:

```powershell
agentmux server --desktop-control --port 8765 `
  --mcp-http --mcp-port 8766 --mcp-profile standard --json
```

The JSON startup result reports the `/mcp` URL and generated `auth_token`. MCP
clients send that value as `Authorization: Bearer <auth_token>`. New MCP
sessions default to `read`; request `standard` or `full` with the
`X-AgentMux-Mcp-Profile` header, up to the `--mcp-profile` ceiling.

Loopback is the default. A non-loopback bind also requires `--allow-remote` and
at least one exact `--mcp-allowed-host`; browser-origin clients additionally use
repeatable exact `--mcp-allowed-origin` values. MCP session IDs are bound to the
authenticated token identity and selected profile, so they cannot be replayed
under another principal or privilege level.

## Installed-Binary Verification

Release CI silently installs the generated NSIS package into an isolated
directory and verifies the packaged `agentmux.exe` before publishing. The smoke
checks MCP root/serve/doctor/setup help and generates non-mutating Codex and
Claude setup previews without launching the desktop.

Operators can perform the non-mutating checks manually:

```powershell
agentmux mcp help
agentmux mcp doctor --help
agentmux mcp setup --client codex --profile read --json
```
