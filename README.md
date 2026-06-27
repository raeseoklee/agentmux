# AgentMux

AgentMux is a Windows desktop terminal multiplexer for running many AI agent
sessions, shells, and browser-assisted workflows side by side.

It is designed around three everyday jobs:

- Keep workspaces, tabs, and split panes organized while agents run.
- Reopen the same workspace after an app restart and reconnect useful sessions.
- Surface agent status, attention requests, notifications, and diagnostics in one
  place.

## Documentation

Start here:

- [User manual](./docs/en/user/manual.md)
- [Getting started](./docs/en/user/getting-started.md)
- [CLI guide](./docs/en/user/cli.md)
- [Troubleshooting](./docs/en/user/troubleshooting.md)
- [Operations runbook](./docs/en/operations/release-runbook.md)
- [Versioning and signed releases](./docs/en/release/versioning.md)

The full documentation index is in [docs/README.md](./docs/README.md).

## Release Builds

Release builds are published through GitHub Actions from SemVer tags such as
`v0.1.1`. The release workflow builds the Windows NSIS installer, writes a
SHA256 checksum, generates a GitHub Artifact Attestation when the repository
visibility and plan support it, and uploads the assets to the GitHub Release.

After downloading a release installer, verify its checksum and, when an
attestation is present, its provenance:

```powershell
Get-FileHash -Algorithm SHA256 .\AgentMux_0.1.1_x64-setup.exe
gh attestation verify .\AgentMux_0.1.1_x64-setup.exe --repo raeseoklee/agentmux
```

## Development Branch Notes

Detailed implementation notes, goal logs, architecture drafts, and evidence
captures live on `develop`. They are intentionally not part of the public
operational documentation set that is intended for `main`.
