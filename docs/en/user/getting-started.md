# Getting Started

## Requirements

- Windows 10 or Windows 11.
- AgentMux installer from the GitHub Release page.
- AgentMux is Windows-only. Native macOS and Linux desktop builds are not
  supported in the current release line.
- WSL is optional for basic PowerShell/cmd terminals, but recommended for
  Linux development shells, durable tmux sessions, and most AI-agent workflows
  on Windows.
- `tmux` is required inside WSL when using durable WSL-tmux sessions.
- Pane workers and tmux integrations require their underlying agent CLI inside
  the selected WSL distribution. Install and authenticate `codex`, `claude`,
  or the relevant integration executable in that Linux environment; a command
  that resolves only through `/mnt/c/...` is a Windows installation and may not
  run as a Linux worker.
- WSL-to-Windows interoperability must be active for tmux-compatible
  integrations. Their WSL-side shims call back into the Windows
  `agentmux.exe` control plane.
- MCP tools that inspect or modify workspaces require the AgentMux desktop and
  its local control plane to be running. Protocol discovery and setup preview
  are the only operations that work without the desktop.

## Install

1. Download the latest `AgentMux_*_x64-setup.exe` from GitHub Releases.
2. Verify the artifact attestation:

   ```powershell
   $installer = Get-ChildItem .\AgentMux_*_x64-setup.exe | Sort-Object LastWriteTime -Descending | Select-Object -First 1
   gh attestation verify $installer.FullName --repo raeseoklee/agentmux --signer-workflow raeseoklee/agentmux/.github/workflows/release.yml
   ```

3. Run the installer.
4. Launch AgentMux from the Start menu.

Packaged AgentMux builds check GitHub Releases for updates at startup. Open
Settings > General > Updates to check manually, install an available update, or
disable automatic checks.

## First Run

1. Create or open a workspace.
2. Set the workspace project root if you want new terminals to start in that
   folder.
3. Open a terminal from the tab bar or a pane's empty-state action.
4. Choose a shell profile:
   - WSL direct shell
   - Durable WSL terminal through tmux
   - PowerShell
   - Command Prompt
5. Split panes or add tabs as needed.

## WSL Setup

If WSL is not installed, AgentMux shows setup guidance instead of silently
failing. Install WSL from an elevated PowerShell prompt:

```powershell
wsl --install
```

After WSL is available, install tmux inside your distribution when durable
sessions are needed:

```bash
sudo apt update
sudo apt install tmux
```

Install each agent CLI inside the same distribution that AgentMux will use. For
example, a WSL-native Codex installation can be placed in the user-local prefix:

```bash
npm install -g --prefix "$HOME/.local" @openai/codex@latest
hash -r
command -v codex
codex --version
```

`command -v codex` should report a Linux path such as
`/home/<user>/.local/bin/codex`, not `/mnt/c/Users/.../codex`. Run the agent once
and complete its provider login before asking AgentMux to start pane workers.
Apply the same rule to Claude Code and other integrations.

Before starting an MCP-managed visible team, set the workspace project root,
select the intended default WSL distribution, and open a terminal in that
workspace. These values provide the inherited working directory and runtime for
new workers.

Verify WSL interoperability before launching a tmux-compatible integration:

```bash
test -e /proc/sys/fs/binfmt_misc/WSLInterop
/mnt/c/Windows/System32/cmd.exe /d /q /c echo interop-ok
```

If the registration is missing or the Windows command reports `Exec format
error`, follow the live-session and restart guidance in
[Troubleshooting](./troubleshooting.md#a-tmux-integration-reports-exec-format-error).

WSL support does not imply native Linux desktop support. AgentMux runs as a
Windows application and uses WSL as a Windows-hosted execution environment.

## Before Public Use

Read [Known limitations](./known-limitations.md) if you depend on session
restore, server mode, or Windows publisher signing.

## Basic Workflow

1. Use one workspace per project.
2. Use top tabs for separate tasks.
3. Use split panes inside a tab for related shells or agents.
4. Keep long-running agents in durable WSL-tmux panes when possible.
5. Watch the sidebar and pane badges for agent attention or completion.
