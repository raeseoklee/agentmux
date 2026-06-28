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

## Install

1. Download the latest `AgentMux_*_x64-setup.exe` from GitHub Releases.
2. Optionally verify the artifact attestation:

   ```powershell
   gh attestation verify .\AgentMux_0.1.1_x64-setup.exe --repo raeseoklee/agentmux
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

WSL support does not imply native Linux desktop support. AgentMux runs as a
Windows application and uses WSL as a Windows-hosted execution environment.

## Basic Workflow

1. Use one workspace per project.
2. Use top tabs for separate tasks.
3. Use split panes inside a tab for related shells or agents.
4. Keep long-running agents in durable WSL-tmux panes when possible.
5. Watch the sidebar and pane badges for agent attention or completion.
