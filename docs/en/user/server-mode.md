# AgentMux Server Mode

AgentMux server mode exposes the desktop workspace experience through an
authenticated HTTP endpoint. It uses the same React workspace shell as the
Windows desktop application; it is not a separate simplified web UI.

## Start a Local Server

```powershell
agentmux server --port 8765
```

The default bind is loopback-only. The command prints the local URL and an
authentication token. Treat the token as a local secret.

The initial project directory is the directory where the command is started,
unless `--cwd` is supplied:

```powershell
agentmux server --port 8765 --cwd D:\work\project
```

## Runtime Modes

### Local

Local mode owns its sessions inside the server process. It supports WSL direct,
PowerShell, and Command Prompt profiles. WSL launches through the same
login-shell bootstrap used by the desktop application, including working
directory tracking, user shell selection, and compatible zsh/Powerlevel10k
environment handling.

Source Control requests include the active Pane cwd and backend profile. Moving
between Panes therefore updates the branch, commit, repository path, and file
list for the selected Pane instead of keeping the directory where the server
was launched.

### Desktop Bridge

```powershell
agentmux server --desktop-control --port 8765
```

Desktop-bridge mode forwards supported operations to a running AgentMux desktop
control plane. Use it when the web client must address desktop-owned
Workspaces, durable WSL-tmux sessions, or persisted layout state.

## Current Parity Boundary

The web and desktop clients share the same Workspace, Tab, Pane, Terminal,
Source Control, settings, and localization components. Backend capabilities
still differ where Windows desktop ownership is required:

- durable WSL-tmux attach and persisted Workspace layout require desktop bridge;
- embedded Browser surfaces are owned by the desktop browser host;
- tray notifications, updater integration, and native window controls are
  desktop-only;
- local mode starts a fresh server-owned runtime when restarted.

The UI must hide or disable commands that the selected server mode does not
advertise. It must not silently substitute a different backend.

## Security

- Keep the default loopback bind for normal use.
- Do not paste the bearer token into logs, screenshots, or issue reports.
- Remote binding requires an explicit allow-remote policy and should be placed
  behind a trusted TLS reverse proxy.
- MCP over HTTP has additional allowed-host and allowed-origin checks. See the
  [MCP guide](./mcp.md).

## Verification

Maintainers can run the server contract smoke with:

```powershell
npm run server:smoke
```

The smoke covers UI assets, authentication rejection, WSL discovery, WSL
output, and ConPTY output. Release review also checks the live UI with an active
WSL Pane and verifies that Source Control follows the selected Pane.

See the [user manual](./manual.md) and
[known limitations](./known-limitations.md) for the broader product behavior.
