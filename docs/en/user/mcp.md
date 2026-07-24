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
| `read` | Lists workspaces, sessions, agent attention, pane workers, integration readiness, worktree operations, Git status/diffs/reviews, development-server candidates, team messages, and tasks; reads terminal output, Markdown documents, browser snapshots, events, context, and diagnostics. | Monitoring, status collection, review, and first-time client setup. |
| `standard` | Includes `read`, then adds pane focus, terminal open/split/input, Markdown opening, pane-worker start/send, worktree create/recovery, caller-scoped Git stage/unstage/non-amend commit, caller-owned review authoring/delivery/comment deletion, development-server split opening, browser operations, team messaging/task updates, and agent-state updates. This is a trusted write and command-execution profile. | Trusted interactive agent workflows that require command execution or writes constrained to the caller's immutable pane and repository context. |
| `full` | Includes `standard`, then adds worker release, worktree removal, repository-wide Git stage/unstage, discard, administrative commit authority, review-thread deletion, integration setup, workspace/pane/surface close, session termination, config updates, browser JavaScript evaluation, action execution, and notification clearing. | Trusted operator automation that genuinely needs destructive, cross-context, or high-impact actions. |

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

## Git, Worktree, Review, and Development-Server Tools

The five-track workflow adds the following MCP surfaces. Read tools are safe for
monitoring, but every write remains subject to the selected profile and the
desktop control-plane audit log.

| Workflow | `read` | `standard` | `full` |
| --- | --- | --- | --- |
| Git | `git_status_summary`, `git_status_page`, `git_diff` | `git_stage`, `git_unstage`, and non-amend `git_commit`, all constrained to the caller's immutable pane and repository context | `git_stage_all`, `git_unstage_all`, `git_discard`, amend and cross-context commit authority |
| Agent worktrees | `agent_worktree_list` | `agent_worktree_create`, `agent_worktree_recover` | `agent_worktree_remove` |
| Diff review | `git_review_thread_list`, `git_review_comment_list` | Create/update caller-owned threads and comments; mark owned threads stale; deliver owned threads to allowed targets; delete owned comments | Delete threads and administer reviews outside the caller-owned context |
| Development servers | `development_server_candidate_list` | Dismiss a candidate or open it in a browser split | - |
| Markdown artifacts | `markdown_read` | `markdown_open`, constrained to a file inside the caller's workspace project root | - |

Worktree creation is a recoverable saga: Git worktree creation, AgentMux
workspace creation, terminal startup, and agent launch are compensated in
reverse order when a later step fails. Review delivery is explicit because it
writes to an agent mailbox or terminal. The `standard` profile may deliver only
caller-owned reviews to targets allowed by its immutable pane context; `full`
retains administrative cross-context authority. See
[Advanced agent workflows](./advanced-agent-workflows.md) for examples and
recovery guidance.

## Agent Pane Workers and tmux Integrations

AgentMux exposes typed MCP tools for agent-oriented panes:

| Tool | Profile | Purpose |
| --- | --- | --- |
| `agent_worker_list` | `read` | List AgentMux-managed tmux integration workers and independent Codex pane workers. |
| `agent_integration_status` | `read` | Diagnose Claude Teams, OMO, OMX, or OMC wrapper and WSL readiness, using the selected workspace's default WSL distribution when available. |
| `agent_team_list` | `read` | Reconstruct adaptive teams and their live membership from persisted agent telemetry. |
| `agent_worker_start` | `standard` | Split the active pane or create a tab, then start `claude-teams`, `omo`, `omx`, `omc`, or `codex-pane`. |
| `agent_team_start` | `standard` | Create an adaptive team manifest around the current terminal. Seed workers are optional. |
| `agent_team_spawn` | `standard` | Add one top-level worker with generation and idempotency protection, then reflow the managed layout. |
| `agent_team_release` | `full` | Terminate and release one worker owned by the selected team, then reflow the remaining workers. |
| `agent_team_reflow` | `standard` | Recompute managed pane ratios, or preview them with `dry_run`, without moving foreign panes. |
| `agent_worker_send` | `standard` | Send literal instructions to a worker and optionally submit Enter. |
| `agent_worker_stop` | `full` | Terminate a worker session. |
| `agent_integration_setup` | `full` | Install shared tmux-compatible wrappers and optionally add their directory to the Windows user PATH. |

### Run an adaptive visible team

The normal workflow does not pre-register a fixed number of agents. Start an
empty adaptive team around the lead terminal, then let the lead agent add and
release workers as the task graph changes. AgentMux provides lifecycle,
visibility, capacity, and conflict controls; the lead model decides whether a
new worker is useful for the current project.

```json
{
  "workspace_id": "workspace-id",
  "pane_id": "main-pane-id",
  "mode": "adaptive",
  "layout": "main-left-workers-right",
  "main_ratio": 0.55,
  "max_workers": 6,
  "default_worker_kind": "codex-pane",
  "distribution": "Ubuntu",
  "idempotency_key": "release-0.2.0-analysis",
  "workers": []
}
```

`workers` may be omitted. `max_workers` is a safety ceiling, not a requested
worker count, and includes every managed non-main member, including descendants
adopted from tmux. The response contains a durable `team_id` and `generation`.
Persist the `team_id` in the agent's working context and read the latest team
with `agent_team_list` before making a lifecycle change.

The desktop host claims the main session and generation under one control-plane
lock. Concurrent starts cannot replace an existing team owner. A concurrent
retry with the same non-empty `idempotency_key` reuses the in-progress claim;
it never starts the same seed workers twice.

Each MCP server serializes its own team mutations, while the desktop host stores
the reservation owner and applies generation plus mutation-ID compare-and-set
checks. A repeated live reservation reports `provisioning` without repeating a
split, resize, or termination. If the owning MCP process exits, a later client
may recover the abandoned reservation as `layout_dirty`, inspect the visible
panes, and continue. A live owner cannot be taken over.

When another independent workstream appears, add exactly one worker:

```json
{
  "team_id": "team-id",
  "expected_generation": 1,
  "idempotency_key": "release-0.2.0-docs",
  "name": "docs",
  "args": ["Review the release documentation and report gaps."]
}
```

AgentMux atomically reserves the next generation before it changes topology.
A stale `expected_generation` returns `generation_conflict` without opening a
pane. Repeating the same `idempotency_key` returns the existing worker instead
of creating a duplicate. A successful spawn keeps the main terminal on the
left, places workers in an equal-height stack on the right, and returns the new
generation plus the worker's `pane_id`, `surface_id`, and `session_id`.

Release a completed worker from a client running the `full` profile:

```json
{
  "team_id": "team-id",
  "expected_generation": 2,
  "name": "docs",
  "mode": "soft"
}
```

`agent_team_release` only accepts a member owned by the selected team, but it
still terminates a process and closes a pane, so it requires the `full` profile.
Membership is cleared only after session termination succeeds. If termination
fails, the pane remains visible and the worker remains a team member so it can
be retried safely. Settlement is compare-and-set protected; a stale completion
cannot overwrite a newer team generation.

### Layout and tmux auto-adoption

Each worker pane keeps its own live terminal output subscription. Non-focused
workers therefore continue to render and report attention while the lead is
active. Use `agent_team_list` for membership, `agent_worker_list` for process
state, `agent_attention_list` for workers waiting for input, and
`team_task_list` for shared progress.

With `auto_adopt_tmux` enabled, managed `split-window` descendants created
through the AgentMux tmux shim inherit the team identity and main-session
anchor. Team environment is sufficient for adoption, so a separate integration
marker is not required. The first descendant is placed to the right of the main
pane; later descendants are added to the worker stack and reflowed
automatically. New tabs and new sessions are not silently added to the managed
layout. This supports Claude Code Agent Teams and the OMO/OMX/OMC integrations
without knowing their eventual worker count.

If a tmux shim process exits after reserving a generation but before registering
its pane, the next managed split verifies that the recorded owner process is no
longer alive, recovers the abandoned reservation as `layout_dirty`, reloads the
canonical topology, and continues the split in the same invocation. A live
owner still produces a conflict, so recovery cannot duplicate an in-flight
worker.

AgentMux only resizes the managed subtree. If a user-created pane appears under
that subtree, or the split axes no longer match the managed layout, reflow
returns `layout_conflict`, performs no resize, and marks the layout dirty for an
explicit operator decision. Preview repairs with:

```json
{
  "team_id": "team-id",
  "expected_generation": 3,
  "dry_run": true
}
```

A non-dry-run reflow reserves the next generation before applying ratios. This
serializes it with MCP spawn/release and tmux auto-adoption. A dry run performs
no reservation and never changes topology.

### Fixed seed teams and compensation

For a fully known batch, set `mode` to `fixed` and pass one to eight named seed
workers. Adaptive teams may also include seed workers and then grow later.
Worker names are unique and contain 1-64 characters.

Multi-step starts and spawns use compensation rather than pretending several
control-plane calls are one database transaction. A failed MCP-created worker
is terminated and its pane is closed; initial seed workers are rolled back in
reverse order. A tmux-created descendant is retained if only automatic reflow
fails, because terminating a child owned by the agent framework would be more
surprising than reporting a dirty layout.

`codex-pane` starts an independent Codex CLI process and is not a Codex built-in
`/agent` thread. `claude-teams`, `omo`, `omx`, and `omc` launch tmux-compatible
lead processes whose descendants can be auto-adopted. `agent_worker_send` and
generic `agent_worker_stop` reject ordinary terminal sessions.

The shim captures the WSL-side `PATH` separately when it crosses into
`agentmux.exe`; it never copies the Windows process `PATH` into a Linux child.
The desktop restores the captured WSL path and persisted team telemetry after a
restart so recursive splits remain attributable to the same team.

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
