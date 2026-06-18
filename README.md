# AgentMux

AgentMux is a Windows-first, cross-platform terminal multiplexer designed for running many AI agent sessions, shells, and browser-assisted development workflows in parallel.

The current repository contains the product requirements and implementation design documents that define the first build plan.

## Documentation

- [System requirements and detailed design](./docs/ieee-29148-system-design.md)
- [Implementation documents](./docs/implementation/README.md)
- [Implementation roadmap](./docs/implementation/00-implementation-roadmap.md)

## Initial Implementation Direction

- Rust core runtime.
- Tauri-style desktop shell.
- TypeScript and React UI.
- Windows ConPTY backend.
- WSL direct shell backend.
- WSL durable session backend through tmux-control semantics.
- Local IPC and CLI control plane.
- Performance benchmarks from the first vertical slice.

