# Advanced Agent Workflows

AgentMux provides one control contract for the desktop UI, CLI, server mode,
and MCP. The workflows below require a running AgentMux desktop or server
control plane unless a command is explicitly described as local-only.

## Large Git Repositories

The Source Control panel loads a bounded page of changed files and virtualizes
the visible rows. Scrolling requests additional pages without rendering the
entire change set. Desktop mode refreshes after repository change events;
server mode uses the authenticated control bridge and a bounded fallback poll.

Use the CLI when you need scriptable output:

```powershell
agentmux git status --workspace <workspace-id> --json
agentmux git page --workspace <workspace-id> --limit 200 --json
agentmux git diff --workspace <workspace-id> --path src/app.ts --json
```

Git mutations use argument arrays rather than command-string interpolation.
Discarding local changes requires explicit confirmation:

```powershell
agentmux git discard --workspace <workspace-id> --path src/app.ts --yes
```

## Isolated Agent Worktrees

Create a worktree, workspace, terminal, and agent command as one recoverable
operation. Always provide a stable idempotency key so retries reuse the same
operation instead of creating duplicate resources.

```powershell
agentmux agent worktree create `
  --workspace <source-workspace-id> `
  --branch agent/fix-login `
  --destination D:\worktrees\fix-login `
  --base main `
  --create-branch `
  --idempotency-key issue-142-fix-login `
  -- claude
```

Inspect or recover an interrupted operation:

```powershell
agentmux agent worktree list --include-completed --json
agentmux agent worktree recover --operation-id <operation-id> --json
```

AgentMux removal only accepts a worktree recorded as owned by AgentMux. It
does not delete the Git branch. Removal requires `--yes` and rejects the main
worktree or an arbitrary path.

```powershell
agentmux agent worktree remove <worktree-id> --yes
```

The equivalent MCP tools are available in the `standard` profile for creation,
listing, and recovery. Destructive removal requires the `full` profile.

## Diff Review Feedback

The Source Control diff viewer can attach a review thread to a line or hunk.
Threads retain the path, side, line, revisions, hunk header, and diff hash so a
later refresh can identify stale anchors.

Create and deliver a review from the CLI:

```powershell
agentmux git review thread create `
  --workspace <workspace-id> `
  --path src/app.ts --side right --line 42 `
  --body "Handle the cancellation path before updating state."

agentmux git review thread deliver <thread-id> `
  --target mailbox --session <agent-session-id> --include-context
```

Use `--target terminal` to paste the feedback into the selected terminal. A
mailbox delivery creates a durable team message. Delivery is explicit; creating
a review thread never sends input to an agent by itself.

## Claude and Codex Hooks

AgentMux can install lifecycle adapters into the user-level Claude Code and
Codex hook configuration. Preview first:

```powershell
agentmux agent hooks preview --provider all
agentmux agent hooks install --provider all --yes
```

The installer preserves unrelated configuration, writes a timestamped backup,
and replaces files atomically. It stops without writing when an existing Codex
`notify` command needs manual chaining. Hook events are accepted only from an
AgentMux-managed terminal context; provider conversation IDs are not treated as
AgentMux terminal session IDs.

Review configured hooks in each agent before trusting them. Hook commands run
with the Windows user's permissions.

## Development Server Links

AgentMux inspects live terminal output for local HTTP and HTTPS development
server URLs. ANSI control sequences and chunk boundaries are removed before
matching, wildcard WSL hosts are normalized to a browser-reachable loopback
address, and duplicate candidates expire after a bounded interval.

Detected links appear as an approval action. Opening a candidate creates an
embedded browser split; detection alone never navigates a browser.

```powershell
agentmux dev-server candidate list --workspace <workspace-id> --json
agentmux dev-server candidate open <candidate-id> --axis vertical --ratio 0.4
agentmux dev-server candidate dismiss <candidate-id> --reason ignored
```

## Terminal Warm Retention

Recently visited terminal tabs keep their xterm instance for a short grace
period. This preserves the viewport and avoids a blank intermediate frame when
switching tabs. Hidden tabs are eventually serialized and released, and the
active tab is never evicted. GPU rendering remains limited to visible panes so
large workspaces do not exhaust WebView2 graphics contexts.

## Troubleshooting

- Run `agentmux diagnostics export --json` when a control operation fails.
- Run `agentmux agent hooks status --provider all --json` before reinstalling
  hooks.
- Use a new idempotency key only for a genuinely new worktree operation.
- Refresh the Source Control panel after changing the repository outside the
  AgentMux process if the filesystem watcher reports an error.

See also [MCP control plane](./mcp.md) and
[Troubleshooting](./troubleshooting.md).
