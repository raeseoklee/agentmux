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
