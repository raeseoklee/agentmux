# Known Limitations

AgentMux is early Windows-only software. These limits are intentional to state
plainly before wider public use.

## Platform Scope

- Windows 10 and Windows 11 are the active desktop targets.
- WSL is supported as a Windows-hosted Linux development environment.
- Native macOS and Linux desktop applications are backlog work, not current
  release commitments.

## Session Restore

- Workspace, tab, pane, and session metadata are persisted.
- Durable WSL-tmux sessions can reconnect to existing tmux state when WSL and
  tmux are available.
- PowerShell and Command Prompt sessions can restore layout and restart known
  commands, but they cannot preserve a process across app restarts the way tmux
  can.
- Agent command restore depends on saved session metadata and the agent CLI's
  own resume behavior.

## Terminal Rendering

- Full-screen terminal UIs depend on font metrics, line height, and pane size.
- If a TUI appears misaligned, use the bundled terminal font, keep line height
  near the default, and resize the pane once.
- WebGL rendering is used only where safe; AgentMux falls back when a graphics
  context is unavailable.

## Server Mode

- Server mode is local-first and binds to loopback by default.
- Do not expose server mode to an untrusted network unless you understand the
  token and host settings.

## Signing

- Release assets include SHA256 files, Tauri updater signatures, and GitHub
  Artifact Attestations.
- GitHub Artifact Attestation proves release provenance, but it is not Windows
  Authenticode publisher signing.
- Windows may still show an unknown-publisher prompt until Authenticode signing
  is added.
