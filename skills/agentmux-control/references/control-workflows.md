# AgentMux Control Workflows

Use this reference for AgentMux control-plane and cmux-compatible workflows.

## CLI Orientation

Prefer JSON output when another tool or agent will consume the result.

Common discovery commands:

```powershell
agentmux diagnostics
agentmux workspace list --json
agentmux actions list --workspace <workspace-id> --json
agentmux config diagnostics --workspace <workspace-id> --json
```

Workspace and session commands:

```powershell
agentmux workspace create <name> --project <path>
agentmux workspace get <workspace-id> --json
agentmux workspace rename <workspace-id> <name>
agentmux session spawn --workspace <workspace-id> -- <command>
agentmux session list --workspace <workspace-id> --json
agentmux events poll --workspace <workspace-id>
```

Avoid destructive commands such as `workspace close` unless the user asked for
that exact effect and the close policy is clear.

## Browser Automation

Open browser surfaces as their own tab unless an active-pane operation is
requested:

```powershell
agentmux browser open --workspace <workspace-id> --placement new-tab
agentmux browser navigate <surface-id> https://example.com
agentmux browser dom-snapshot <surface-id> --frame <frame-id>
agentmux browser wait-for-selector <surface-id> "#ready" --timeout-ms 5000
agentmux browser click <surface-id> --selector "#submit" --frame <frame-id>
agentmux browser evaluate <surface-id> -- "document.title"
agentmux browser diagnostics --workspace <workspace-id>
```

Use `--frame` for frame-specific DOM commands only when the frame identifier is
known from `browser frames`, a prior snapshot, or the user's explicit target.

## Dock Config

Dock config lookup is AgentMux-first with cmux-compatible fallbacks:

1. Project `.agentmux/dock.json`
2. Project `.cmux/dock.json`
3. Global AgentMux `dock.json`
4. Global cmux `dock.json` from `CMUX_DOCK_PATH` or
   `%USERPROFILE%\.config\cmux\dock.json`

Minimal Dock config:

```json
{
  "controls": [
    {
      "id": "git",
      "title": "Git",
      "command": "lazygit",
      "cwd": ".",
      "height": 30,
      "env": {
        "NO_COLOR": "1"
      }
    }
  ]
}
```

Project Dock files require trust before execution. Do not execute a project
Dock command by bypassing the UI or backend trust path.

## Agent Integrations

Prefer the AgentMux-native integration commands. The `cmux` executable is a
compatibility alias for third-party tools, not the primary user interface:

```powershell
agentmux integrations doctor claude-teams --distribution Ubuntu --json
agentmux integrations install-shims --user-path
agentmux integrations setup omo
agentmux integrations setup omc
agentmux integrations launch claude-teams
agentmux integrations launch omx
```

When MCP is available, use `agent_integration_status`, `agent_worker_start`,
`agent_worker_list`, and `agent_worker_send`. Use `agent_integration_setup` or
`agent_worker_stop` only with the `full` profile. `codex-pane` is an independent
Codex CLI worker; it is not a Codex built-in `/agent` thread.

When a wrapper launches inside WSL, `AGENTMUX_EXE` points to the Windows-side
`agentmux.exe` path translated for WSL. `CMUX_EXE` remains available only for
compatibility. Descendant tmux panes must inherit the integration marker and
captured WSL path; the desktop host restores that Linux path without copying the
Windows `PATH` and regenerates each pane-specific `TMUX` identity.

## Verification

Choose the smallest gate that covers the change:

```powershell
npm --prefix apps/desktop run build
apps/desktop/node_modules/.bin/playwright.cmd test -g "Dock|TextBox|browser"
.toolchains/rustup/toolchains/stable-x86_64-pc-windows-msvc/bin/cargo.exe test -p agentmux-cli
.toolchains/rustup/toolchains/stable-x86_64-pc-windows-msvc/bin/cargo.exe test -p agentmux-desktop-host
npm run docs:check
```

Run broader gates when touching installer, WSL/tmux wrappers, persistence, or
cross-process control-plane behavior.
