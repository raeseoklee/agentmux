# AgentMux CLI Guide

The `agentmux` CLI talks to a running AgentMux desktop or server instance through
the local control plane.

## Identify the Running Instance

```powershell
agentmux identify --json
agentmux ping
```

## Workspaces

List workspaces:

```powershell
agentmux workspace list --json
```

Create a workspace:

```powershell
agentmux workspace create AgentMux --project D:\work\repo
```

Close a workspace:

```powershell
agentmux workspace close <workspace-id> --policy fail_if_running --yes
```

Use `detach_sessions` or `terminate_sessions` only when that behavior is
intentional.

## Sessions

Spawn a Windows shell:

```powershell
agentmux session spawn --workspace <workspace-id> -- cmd.exe /d /q
```

Spawn a WSL shell:

```powershell
agentmux terminal run --backend wsl-direct --distribution Ubuntu --cwd D:\work\repo -- bash -lc pwd
```

List sessions:

```powershell
agentmux session list --workspace <workspace-id> --json
```

Terminate a session:

```powershell
agentmux session terminate <session-id> --mode soft --yes
```

## Agent State

Set a session as waiting for input:

```powershell
agentmux agent set-state <session-id> waiting_for_input --reason "needs input"
```

List agent attention:

```powershell
agentmux agent list-attention --json
```

List all agent sessions or AgentMux-managed workers:

```powershell
agentmux agent list --workspace <workspace-id> --json
```

## Agent Integrations

Install the AgentMux-first tmux compatibility wrappers and add them to the
Windows user PATH:

```powershell
agentmux integrations install-shims --user-path
```

Check Claude Teams readiness inside the selected WSL distribution:

```powershell
agentmux integrations doctor claude-teams --distribution Ubuntu --json
```

From an AgentMux terminal context, launch a supported integration:

```powershell
agentmux integrations launch claude-teams
agentmux integrations launch omx
```

The `cmux` executable remains a compatibility alias for tools that invoke cmux
or tmux command shapes. User-facing automation should prefer `agentmux`.

## Notifications and Sidebar

Send an OS/app notification:

```powershell
agentmux notify --title "Build" --body "Done"
```

Set workspace status:

```powershell
agentmux set-status build compiling --priority 80
agentmux set-progress 0.5 --label "Building"
agentmux log --level success -- "All tests passed"
```

Clear status:

```powershell
agentmux clear-status build
agentmux clear-progress
```

## Browser Surfaces

Open a browser surface:

```powershell
agentmux browser open --workspace <workspace-id> --placement new-tab
```

Navigate:

```powershell
agentmux browser navigate <surface-id> https://example.com
```

## Diagnostics

Export diagnostics:

```powershell
agentmux diagnostics export --json
```

Reload config:

```powershell
agentmux config reload --json
```

Export config schema:

```powershell
agentmux config schema --output agentmux.config.schema.json
```

## Server Mode

Run the local web UI:

```powershell
agentmux server --workspace <workspace-id> --port 8765
```

Open `http://127.0.0.1:8765/` in a browser.

Expose the running desktop control plane through authenticated MCP Streamable
HTTP on a separate port:

```powershell
agentmux server --desktop-control --port 8765 `
  --mcp-http --mcp-port 8766 --mcp-profile standard --json
```

## MCP

AgentMux can expose its authenticated local control plane to MCP clients over
stdio:

```powershell
agentmux mcp doctor --profile read --json
agentmux mcp serve --profile read
agentmux mcp setup --client codex --profile read --json
```

See the [MCP control-plane guide](./mcp.md) for profile permissions, Codex and
Claude configuration, installation safety, and remote transport status.
