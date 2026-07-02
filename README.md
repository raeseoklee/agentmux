# AgentMux

[![CI](https://github.com/raeseoklee/agentmux/actions/workflows/ci.yml/badge.svg)](https://github.com/raeseoklee/agentmux/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/raeseoklee/agentmux?label=release)](https://github.com/raeseoklee/agentmux/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)

AgentMux is a Windows desktop terminal multiplexer for running AI-agent
sessions, shells, and browser-assisted workflows side by side.

It is built for developers who need to watch several agents at once, jump to the
one that needs input, and keep terminal/browser context together in one
workspace.

## What It Does

- Organize work by workspace, tab, and split pane.
- Run WSL, durable WSL-tmux, PowerShell, and Command Prompt sessions.
- Reopen saved workspaces after restart and reconnect or restart known sessions.
- Track agent running, waiting, completed, and failed states.
- Show workspace and pane attention badges when an agent needs intervention.
- Open agent-generated links in embedded browser panes.
- Expose a local CLI/control plane for automation, diagnostics, and integration.

AgentMux is Windows-only for the current product line. WSL is a first-class
Windows integration for Linux development environments; native macOS and Linux
desktop apps are tracked in the platform backlog.

## Download

Download the latest Windows installer from
[GitHub Releases](https://github.com/raeseoklee/agentmux/releases/latest).

Requirements:

- Windows 10 or Windows 11.
- WSL is optional for PowerShell/cmd workflows.
- WSL with `tmux` is recommended for durable Linux agent sessions.

## Quick Start

1. Install `AgentMux_*_x64-setup.exe`.
2. Open AgentMux from the Start menu.
3. Create a workspace and set its project root.
4. Open a terminal profile: WSL, durable WSL-tmux, PowerShell, or Command Prompt.
5. Add tabs or split panes for related agent sessions.

For a full walkthrough, see [Getting started](./docs/en/user/getting-started.md).

## Verify a Release

Release builds are published by GitHub Actions. The release workflow builds the
Windows NSIS installer, writes a SHA256 checksum, creates Tauri updater
artifacts, generates GitHub Artifact Attestations, verifies those attestations,
and uploads the assets to GitHub Releases.

After downloading an installer:

```powershell
Get-FileHash -Algorithm SHA256 .\AgentMux_0.1.6_x64-setup.exe
gh attestation verify .\AgentMux_0.1.6_x64-setup.exe --repo raeseoklee/agentmux --signer-workflow raeseoklee/agentmux/.github/workflows/release.yml
```

GitHub Artifact Attestation is release provenance, not Windows Authenticode
publisher signing. Windows may still show an unknown-publisher prompt until an
Authenticode certificate is added.

## Current Limits

AgentMux is early software. The core Windows workflow is usable, but these areas
are still being refined:

- Durable WSL-tmux sessions are the best path for long-running agent work.
- PowerShell and Command Prompt panes can restore layout and restart known
  commands, but they do not have tmux-style process persistence.
- Agent command restore depends on saved session metadata and the agent CLI's
  own resume behavior.
- Native macOS and Linux desktop builds are not in the active release scope.

See [Known limitations](./docs/en/user/known-limitations.md) and
[Troubleshooting](./docs/en/user/troubleshooting.md).

## Documentation

- [Getting started](./docs/en/user/getting-started.md)
- [User manual](./docs/en/user/manual.md)
- [Feature overview](./docs/en/features.md)
- [CLI guide](./docs/en/user/cli.md)
- [Troubleshooting](./docs/en/user/troubleshooting.md)
- [Versioning and release verification](./docs/en/release/versioning.md)
- [Platform backlog](./docs/en/backlog/platform-backlog.md)

The full documentation index is in [docs/README.md](./docs/README.md).

## Security

AgentMux controls local shells, WSL sessions, tmux sessions, SSH sessions, and
browser surfaces. Review diagnostics before sharing them publicly.

Report vulnerabilities through GitHub private vulnerability reporting when
available, or follow [SECURITY.md](./SECURITY.md).

## Contributing

Issues and focused pull requests are welcome. Please read
[CONTRIBUTING.md](./CONTRIBUTING.md) before opening a PR.

## License

AgentMux is licensed under the MIT License. See [LICENSE](./LICENSE) and
[THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md).
