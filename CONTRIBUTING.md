# Contributing

Thanks for taking the time to improve AgentMux.

## Development Setup

Requirements:

- Windows 10 or Windows 11 for the full desktop and ConPTY path.
- Rust stable.
- Node.js 22 or newer.
- npm.
- WSL with tmux for durable WSL/tmux workflows.

Common commands:

```powershell
npm run check
npm run docs:check
cargo test --workspace
npm run desktop:build
npm --prefix apps/desktop run tauri:dev
```

## Pull Requests

Keep changes focused. For user-visible behavior, include the test or manual
verification command you ran. For backend or control-plane changes, mention the
affected request/response methods or event frames.

Before opening a pull request, run:

```powershell
npm run check
```

If the full gate is not available on your machine, note which subset you ran and
why the rest was skipped.

## Local Artifacts

Local agent harness files, Codexus session state, MCP configuration, Visual
Studio state, build output, test output, installer output, and verification
evidence are intentionally ignored. Keep those files local unless a maintainer
explicitly asks for a redacted artifact.
