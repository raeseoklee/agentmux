---
name: agentmux-control
description: Use when Codex needs to operate, inspect, automate, or modify AgentMux/cmux-on-Windows workflows, including control-plane CLI commands, workspace/session/pane/browser actions, WSL/tmux diagnostics, config/action registry changes, Dock setup, Dock execution, or AgentMux integration troubleshooting.
---

# AgentMux Control

## Operating Model

Treat AgentMux as a Windows-only desktop multiplexer whose durable agent work
runs through WSL and tmux-compatible shims. Prefer AgentMux's existing
control-plane, CLI, config, and test helpers over ad hoc UI or process hacks.

Use the AgentMux MCP server as the primary control surface when it is available.
Prefer the least-privileged profile that can complete the task: `read` for
inspection, `standard` only for a trusted client that may execute arbitrary
terminal commands and mutate terminal, browser, or shared coordination state,
and `full` only for explicitly approved lifecycle, configuration, JavaScript,
or other administrative work. `standard` is a trusted write/command-execution
profile, not a safe or non-destructive profile. Fall back to the `agentmux` CLI
when MCP is unavailable, still connecting, or does not expose the required
capability.

Profiles are enforced MCP tool-surface policy, not a sandbox against processes
running as the same Windows user. A command-capable profile can launch the CLI
or other local programs with that user's authority. Never present profile
selection as operating-system isolation.

## Workflow

1. Establish the target workspace, pane, surface, or config scope before
   taking action. Use AgentMux MCP tools when available, otherwise use the
   equivalent `agentmux` CLI command. Use the `cmux` alias only for
   compatibility checks.
2. Check WSL/tmux state before launching durable agent sessions. Surface WSL
   missing and tmux missing guidance instead of silently falling back to
   PowerShell behavior.
3. For team orchestration, inspect `agent_integration_status`, then use
   `agent_worker_start`, `agent_worker_list`, and `agent_worker_send`. Treat
   `codex-pane` as an independent Codex CLI worker, never as a Codex-owned
   built-in `/agent` thread.
4. Keep top tabs and splits distinct: new terminal/browser/agent/Dock launches
   should create top-level tabs unless the user explicitly requests an active
   pane operation.
5. For browser automation, prefer the built-in `agentmux browser ...` commands
   and include `--frame` only when the target frame is known.
6. For Dock work, read `dock.json` from AgentMux-first paths and preserve the
   project trust boundary before executing commands.
7. Verify with the narrowest reliable gate first, then broaden to UI, Rust, or
   release gates when the change crosses boundaries.

## References

Read [control-workflows.md](references/control-workflows.md) when you need CLI
command shapes, Dock config paths, browser automation patterns, or verification
checklists.

## Guardrails

- Do not close workspaces, kill sessions, or terminate panes unless the user
  clearly requested that operation.
- Do not bypass an MCP profile boundary by dropping to the CLI. If a requested
  action needs a stronger profile, ask for the appropriate explicit approval.
- Treat `agent_integration_setup` as a direct local mutation. It can write to an
  environment-selected integration directory and can modify the Windows user
  `PATH`; it is not recorded by desktop control-plane audit diagnostics.
- Do not bypass project Dock trust prompts for project-sourced commands.
- Do not assume Unix socket cmux compatibility. AgentMux uses a Windows named
  pipe and exposes `cmux.exe`, `--socket`, and `CMUX_SOCKET_PATH` as aliases.
- When editing repo code, keep behavior aligned with the implementation docs in
  `docs/implementation/19-cmux-windows-parity-gap-analysis.md` and
  `docs/implementation/23-overall-completion-goal-groups.md`.
