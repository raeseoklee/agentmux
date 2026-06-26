---
name: agentmux-control
description: Use when Codex needs to operate, inspect, automate, or modify AgentMux/cmux-on-Windows workflows, including control-plane CLI commands, workspace/session/pane/browser actions, WSL/tmux diagnostics, config/action registry changes, Dock setup, Dock execution, or AgentMux integration troubleshooting.
---

# AgentMux Control

## Operating Model

Treat AgentMux as a Windows-only desktop multiplexer whose durable agent work
runs through WSL and tmux-compatible shims. Prefer AgentMux's existing
control-plane, CLI, config, and test helpers over ad hoc UI or process hacks.

## Workflow

1. Establish the target workspace, pane, surface, or config scope before
   running commands. Use `agentmux` when available; use the `cmux` alias only
   for compatibility checks.
2. Check WSL/tmux state before launching durable agent sessions. Surface WSL
   missing and tmux missing guidance instead of silently falling back to
   PowerShell behavior.
3. Keep top tabs and splits distinct: new terminal/browser/agent/Dock launches
   should create top-level tabs unless the user explicitly requests an active
   pane operation.
4. For browser automation, prefer the built-in `agentmux browser ...` commands
   and include `--frame` only when the target frame is known.
5. For Dock work, read `dock.json` from AgentMux-first paths and preserve the
   project trust boundary before executing commands.
6. Verify with the narrowest reliable gate first, then broaden to UI, Rust, or
   release gates when the change crosses boundaries.

## References

Read [control-workflows.md](references/control-workflows.md) when you need CLI
command shapes, Dock config paths, browser automation patterns, or verification
checklists.

## Guardrails

- Do not close workspaces, kill sessions, or terminate panes unless the user
  clearly requested that operation.
- Do not bypass project Dock trust prompts for project-sourced commands.
- Do not assume Unix socket cmux compatibility. AgentMux uses a Windows named
  pipe and exposes `cmux.exe`, `--socket`, and `CMUX_SOCKET_PATH` as aliases.
- When editing repo code, keep behavior aligned with the implementation docs in
  `docs/implementation/19-cmux-windows-parity-gap-analysis.md` and
  `docs/implementation/23-overall-completion-goal-groups.md`.
