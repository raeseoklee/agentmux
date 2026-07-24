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

Do not treat Browser Preview or a mocked control client as equivalent to the
packaged desktop path. Changes that cross UI, transport, IPC validation, or
backend boundaries must identify every affected runtime surface and include
contract coverage for boundary values. User-visible desktop changes must also
be exercised in an isolated Tauri instance when their behavior depends on
native IPC, WebView2, PTY, filesystem, updater, or OS integration.

Before opening a pull request, run:

```powershell
npm run check
npm run desktop:gates
```

If the full gate is not available on your machine, note which subset you ran and
why the rest was skipped. A skipped applicable native-desktop verification
blocks release approval even when mock-based tests pass.

See [Release quality gates](docs/en/operations/release-quality-gates.md) for the
required test matrix, regression-fix rules, and release evidence format.

## Local Artifacts

Local agent harness files, Codexus session state, MCP configuration, Visual
Studio state, build output, test output, installer output, and verification
evidence are intentionally ignored. Keep those files local unless a maintainer
explicitly asks for a redacted artifact.
