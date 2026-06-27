# AgentMux Feature Overview

Status: Draft

AgentMux is a Windows desktop terminal multiplexer for AI-agent workflows. Its
core value is keeping many terminals, agents, browser surfaces, and workspace
states visible and recoverable from one application.

## Terminal and Session Execution

- Native Windows shells through ConPTY.
- WSL direct shells with distribution discovery and Windows-to-WSL path
  conversion.
- Durable WSL sessions through tmux for long-running agent work.
- Session metadata, layout, and workspace state persisted in SQLite.

## Agent Workflows

- Agent launch through actions, command palette entries, and CLI commands.
- Agent lifecycle markers for running, waiting for input, completed, and failed
  states.
- Workspace, pane, notification, and OS-level attention signals for agents that
  need intervention.

## Workspace Layout

- Workspaces, top-level tabs, and split panes.
- Tabs own their own pane layout; adding a tab does not mutate another tab's
  split tree.
- Pane split, resize, focus, close, and surface mount/unmount operations.
- Move and reorder controls for workspaces, tabs, and panes.

## Browser and Automation

- Browser surfaces can be opened beside terminal sessions.
- CDP-backed browser automation supports navigation, screenshot, DOM snapshot,
  click, type, and evaluate operations.
- Browser actions are scoped to the selected surface.

## Control Plane and CLI

- `agentmux` CLI for workspace, session, pane, notification, browser, action,
  diagnostic, and configuration workflows.
- Local named-pipe control plane for desktop automation.
- Event polling and subscription APIs for agents and external tools.

## Packaging and Operations

- Windows NSIS installer builds through GitHub Actions.
- SHA256 checksums and GitHub Release uploads.
- Tauri updater artifacts and `latest.json` are published to GitHub Releases so
  packaged desktop apps can check for updates without a separate update server.
- GitHub Artifact Attestations are generated when the repository visibility and
  plan support them.

The Korean feature notes are kept at [../ko/features.md](../ko/features.md).
