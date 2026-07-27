# Troubleshooting

## WSL Is Not Detected

Check from PowerShell:

```powershell
wsl --status
wsl -l -v
```

If WSL is missing:

```powershell
wsl --install
```

Restart Windows if WSL installation asks for it.

## Durable tmux Sessions Do Not Open

Inside the selected WSL distribution:

```bash
tmux -V
```

If tmux is missing:

```bash
sudo apt update
sudo apt install tmux
```

Then restart AgentMux or reopen the affected terminal.

## A WSL Agent Worker Finds the Windows CLI

AgentMux pane workers run inside the selected WSL distribution. Check the
executable there:

```bash
command -v codex
command -v claude
```

A path under `/mnt/c/Users/...` is a Windows installation imported through the
WSL `PATH`. It may be discoverable but still fail as a Linux worker. Install the
agent CLI inside the selected distribution, ensure its Linux bin directory
precedes Windows paths, complete provider authentication, and retry.

For Codex, an error such as `Missing optional dependency
@openai/codex-linux-x64` means WSL invoked the Windows npm installation. One
user-local repair is:

```bash
npm install -g --prefix "$HOME/.local" @openai/codex@latest
hash -r
command -v codex
codex --version
codex login
```

The resolved command should be a Linux path such as
`/home/<user>/.local/bin/codex`.

## A tmux Integration Reports Exec Format Error

AgentMux tmux-compatible integrations call the Windows `agentmux.exe` control
plane from WSL. The MCP stdio connection may still be healthy through its
`/init` bridge while these later callbacks fail if the WSL `binfmt_misc`
registration is missing.

Check the current distribution:

```bash
test -e /proc/sys/fs/binfmt_misc/WSLInterop
/mnt/c/Windows/System32/cmd.exe /d /q /c echo interop-ok
```

If `WSLInterop` is absent and active agents must remain running, register the
standard WSL interpreter for the current live instance:

```bash
sudo sh -c 'echo ":WSLInterop:M::MZ::/init:P" > /proc/sys/fs/binfmt_misc/register'
```

Verify the two checks again, then retry only the failed worker. This registration
is an in-memory repair and may disappear when the WSL instance stops.

After all WSL jobs and tmux sessions are safely finished, terminate only the
affected distribution from PowerShell and reopen it:

```powershell
wsl --terminate Ubuntu
```

This command terminates every process in that distribution, so do not run it
while agents or unsaved shells are active. If a fresh instance still lacks the
registration, confirm WSL is current and that `/etc/wsl.conf` does not disable
`[interop]`. Do not unmask or rewrite `systemd-binfmt.service` blindly; WSL may
generate its own protection and registration overrides for that service.

## MCP Works on Windows but Is Missing in WSL

Windows and WSL clients use different home directories and configuration
files. `agentmux mcp setup --client codex` and `--client claude` target the
Windows user configuration by default. Register the MCP server again from the
WSL-native client when the lead agent runs inside an AgentMux WSL pane. See
[Configure Codex](./mcp.md#configure-codex) and
[Configure Claude Code](./mcp.md#configure-claude-code) for the commands.

If Claude reports `ENOEXEC` or `posix_spawn` for the Windows
`agentmux.exe`, it attempted to execute the PE file directly. Remove the broken
entry and register the `/init` plus `cmd.exe` bridge shown in the MCP guide.
The bridge preserves stdio while avoiding a direct PE spawn from the Linux
client.

If registration exists but health checking fails, keep the AgentMux desktop
open and run the installed Windows executable through the same bridge:

```bash
/init /mnt/c/Windows/System32/cmd.exe /d /q /c \
  'C:\Users\<Windows-user>\AppData\Local\AgentMux\agentmux.exe' \
  mcp doctor --profile standard --json
```

## A Restored Pane Is Empty

An empty restored pane usually means the layout was restored but the terminal
backend did not reconnect.

Try:

1. Wait a few seconds for backend recovery.
2. Check whether the pane header says disconnected or recovering.
3. Reopen the terminal with the same profile.
4. Export diagnostics with `agentmux diagnostics export --json`.

For durable WSL-tmux panes, also verify that WSL and tmux are available.

## Copy and Paste Behaves Like `^V`

This means the key sequence reached the shell instead of the app clipboard path.

Try:

1. Click the terminal pane to focus it.
2. Use the app menu or context action for paste.
3. Confirm clipboard permission is available in the desktop app.
4. Restart AgentMux if the clipboard plugin was updated during installation.

## Claude, Codex, or Another TUI Looks Misaligned

Terminal UI alignment depends on font metrics and pane size.

Try:

1. Use the bundled terminal font from settings.
2. Keep line height near the default value.
3. Resize the pane once after opening a full-screen TUI.
4. Disable ligatures only if the TUI renders ambiguous glyphs.

## PowerShell or cmd Does Not Restore Like WSL

Windows ConPTY terminals can restore layout and restart known commands, but they
do not have tmux-style process persistence. Use durable WSL-tmux for long-running
agent sessions that must survive app restarts.

For a compact list of product limits, see
[Known limitations](./known-limitations.md).

## Git Status Shows `no git`

The status bar reads Git state from the current workspace project root. Set the
workspace project root to a Git repository and reopen or refresh the workspace.

## Collect Diagnostics

Use:

```powershell
agentmux diagnostics export --json
```

Attach the output when reporting a bug. Avoid sharing secrets; AgentMux redacts
common token and key patterns, but review diagnostics before posting publicly.
