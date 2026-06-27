# AgentMux User Manual

## Workspace Model

AgentMux uses a simple hierarchy:

- Workspace: a project or working context.
- Tab: an independent surface inside a workspace.
- Split pane: a layout area inside the active tab.
- Terminal session: the shell or agent process shown in a pane.

Tabs own their panes. Adding a tab creates a separate surface; it should not add
a pane to the previous tab. Closing a tab closes the panes attached to that tab.

## Workspaces

Use workspaces to separate projects.

Common actions:

- Create a workspace.
- Rename a workspace after creation.
- Set a project root.
- Change workspace color, icon, or description.
- Close a workspace.

If a workspace has running terminals, AgentMux should ask for confirmation before
closing it.

## Tabs and Panes

Use tabs for independent tasks and split panes for related processes.

Common actions:

- Add a tab.
- Close a tab.
- Split the current pane horizontally or vertically.
- Resize split panes.
- Move or reorder workspaces, tabs, and panes using the available move controls.
- Close a pane.

## Terminal Profiles

AgentMux supports multiple terminal profiles. The available set depends on the
machine and settings.

Typical profiles:

- WSL direct shell.
- Durable WSL terminal through tmux.
- Windows PowerShell.
- Command Prompt.

The default profile can be changed from settings. WSL-only actions remain
disabled or show setup guidance when WSL is unavailable.

## Durable Sessions

Durable WSL-tmux sessions are intended for long-running agent work.

Expected behavior:

- Reopening AgentMux should restore the workspace and pane layout.
- Durable sessions should reconnect to the existing tmux session when possible.
- Agent sessions should preserve or restart the agent command according to the
  saved session metadata.

For best results, keep `tmux` installed in the selected WSL distribution.

## Agents

AgentMux can track agent state through explicit agent markers and integration
metadata.

Visible states include:

- Running.
- Waiting for input.
- Completed.
- Failed.

When an agent waits for input or completes, AgentMux can show pane badges,
workspace sidebar attention, and OS notifications.

## Clipboard

Use standard terminal clipboard behavior:

- `Ctrl+C` sends interrupt when terminal input is focused and no text is selected.
- Copy selected terminal text with the app's copy command or context action.
- Paste with the app's paste command or terminal paste shortcut.

If `Ctrl+V` appears as `^V`, check the troubleshooting guide.

## Sidebar and Status Bar

The sidebar summarizes workspaces and attention state. The bottom status bar
shows contextual information such as:

- Git branch and short commit hash when available.
- Workspace project path.
- Active backend or shell profile.
- Running session count.

## Settings

Use settings to manage:

- Theme and terminal appearance.
- Terminal font, line height, and padding.
- Default shell profile.
- WSL diagnostics and distribution selection.
- Workspace metadata.
- Notification behavior.

## Server Mode

AgentMux can also run in a local server mode for browser access to the same UI.

Example:

```powershell
agentmux server --workspace <workspace-id> --port 8765
```

By default, server mode is intended for local access. Do not expose it to an
untrusted network unless an explicit remote-access policy has been configured.
