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

## Browser Surfaces

AgentMux can open browser surfaces inside the workspace for documentation,
issues, pull requests, local preview servers, and agent-generated links.
The browser pane shows the rendered frame from the same managed CDP session
used by AgentMux automation. Address navigation, back/forward, reload, clicks,
mouse-wheel scrolling, and focused key input are sent back to that session.

When a terminal prints an OSC 8 hyperlink or a plain `http://` / `https://`
URL, Ctrl-click the link to open it in an embedded browser split beside that
terminal. In browser-hosted sessions that use macOS-style shortcuts, Cmd-click
works the same way.

## Markdown Surfaces

Open a local Markdown file from the terminal profile menu to review agent
artifacts without leaving the workspace. Markdown surfaces can open in a new
tab or replace the active empty pane. They provide rendered GitHub-flavored
Markdown, document search, reload, relative document links, and local image
previews.

Markdown surfaces are read-only. AgentMux accepts Markdown files and supported
local images only when their canonical paths stay inside the workspace project
root. Remote images are not fetched by the viewer. Use the embedded browser for
remote web content.

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

- Theme, preset accent colors, and custom HEX accent colors.
- Terminal font size, inner margin, and GPU acceleration.
- Default shell profile.
- WSL diagnostics and distribution selection.
- Workspace metadata.
- Notification behavior.

### Terminal GPU acceleration

AgentMux exposes three terminal renderer policies under **Settings >
Appearance**:

- `Auto` (default): use WebGL only on Windows when WebGL2 is available and the
  reported renderer is hardware accelerated. Software and unknown renderers
  stay on the DOM renderer.
- `On`: request WebGL even when the renderer is software based. AgentMux still
  falls back to DOM after an attach failure or context loss.
- `Off`: always use the DOM renderer.

Only the focused terminal pane owns a WebGL context. Hidden and background
panes release their contexts, which avoids WebView2 context exhaustion in
large workspaces.

The same setting can be edited in the global configuration file at
`%APPDATA%\AgentMux\agentmux.json` or in a project override at
`.agentmux\agentmux.json`:

```json
{
  "ui": {
    "terminal_gpu_acceleration": "auto"
  }
}
```

Supported values are `auto`, `on`, and `off`. Invalid values are rejected by
configuration import, update, and diagnostics commands.

## Server Mode

AgentMux can also run in a local server mode for browser access to the same
React workspace UI used by the desktop app. The selected Pane drives Terminal
cwd, backend context, Source Control, and the status bar.

Example:

```powershell
agentmux server --workspace <workspace-id> --port 8765
```

By default, server mode is intended for local access. Do not expose it to an
untrusted network unless an explicit remote-access policy has been configured.

Local mode owns its Terminal sessions directly. Desktop-bridge mode connects
the web UI to the running desktop control plane and is required for durable
WSL-tmux and desktop-persisted Workspace state. See the
[server-mode guide](./server-mode.md) for supported profiles, security, and
current parity limits.
