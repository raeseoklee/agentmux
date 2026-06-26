# AgentMux Documentation

This directory separates operational documents from development-only material.
For `main`, publish the user and operations documents unless a release manager
explicitly decides otherwise.

## User Documentation

- [Getting started](./user/getting-started.md)
- [User manual](./user/manual.md)
- [CLI guide](./user/cli.md)
- [Troubleshooting](./user/troubleshooting.md)

## Operations Documentation

- [Operations overview](./operations/README.md)
- [Release runbook](./operations/release-runbook.md)
- [Main merge policy](./operations/main-merge-policy.md)
- [Versioning and signed releases](./release/versioning.md)
- [AgentMux config schema](./schemas/agentmux.config.schema.json)

## Development-Only Documentation

The following paths are useful while building AgentMux, but they should stay out
of `main` unless they are intentionally promoted:

- `docs/implementation/**`
- `docs/implementation/evidence/**`
- `docs/development/**`
- `docs/ieee-*.md`
- `docs/features.md`

Use [main-merge-policy.md](./operations/main-merge-policy.md) when promoting
only operational documentation from `develop` to `main`.
