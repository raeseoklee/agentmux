# AgentMux

AgentMux is a Windows-first, cross-platform terminal multiplexer designed for running many AI agent sessions, shells, and browser-assisted development workflows in parallel.

The current repository contains the product requirements and implementation design documents that define the first build plan.

## Documentation

- [User-facing feature list](./docs/features.md)
- [System requirements and detailed design](./docs/ieee-29148-system-design.md)
- [Implementation documents](./docs/implementation/README.md)
- [Implementation roadmap](./docs/implementation/00-implementation-roadmap.md)
- [Implementation goal groups](./docs/implementation/08-goal-groups.md)
- [Windows development bootstrap](./docs/development/bootstrap-windows.md)

## Initial Implementation Direction

- Rust core runtime.
- Tauri-style desktop shell.
- TypeScript and React UI.
- Windows ConPTY backend.
- WSL direct shell backend.
- WSL durable session backend through tmux-control semantics.
- Local IPC and CLI control plane.
- Performance benchmarks from the first vertical slice.

## Development Commands

```powershell
npm run docs:check
npm run check
cargo test --workspace
npm --prefix apps/desktop run build
npm --prefix apps/desktop run tauri:build -- --debug --no-bundle --ci
cargo run -p agentmux-bench-single-terminal-latency
npm run skills:install
```

Use `tools/bootstrap-windows.ps1` to check whether Git, Rust, Node.js, and npm are available on a Windows development machine.
