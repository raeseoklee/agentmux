# Release Readiness Audit

Generated: 2026-06-19T11:28:34.1943922Z

This audit is non-mutating. It records installer artifact presence, Windows
install detection, the current WSL/tmux state, and cmux integration doctor
results. It does not install or uninstall AgentMux.

- Readiness: needs_attention
- Installer: found
- Installed app: detected
- Installed CLI sidecars: missing
- Installed directory on user PATH: False
- WSL state: wsl_with_tmux
- Integration doctor: needs_attention

Manual install/uninstall remains a human-controlled release gate.
