# Windows Development Bootstrap

Status: Draft
Date: 2026-06-18

This checklist prepares a local Windows machine for AgentMux development.

## Required Tools

- Git for Windows
- Rust toolchain through rustup
- Node.js 22 LTS or newer
- npm
- WSL 2 with at least one Linux distribution for WSL backend work
- tmux inside the selected WSL distribution for durable WSL backend work

## Verify Environment

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/bootstrap-windows.ps1
```

Then run the checks that are available on the machine:

```powershell
npm run docs:check
Push-Location apps/desktop; npm install; npm run build; Pop-Location
cargo test --workspace
```

The repository-level check wrapper runs Rust checks and documentation link checks:

```powershell
npm run check
```

If Rust is installed under the repository-local `.toolchains` directory, `npm run check` will use that local Cargo binary without requiring a global PATH change.

## Initial Development Focus

The first implementation milestone is the native terminal vertical slice:

1. Start the desktop shell.
2. Create one native Windows terminal session.
3. Type a command.
4. See terminal output.
5. Resize the pane.
6. Close the session and surface the exit state.

WSL direct and durable tmux-control work should start after this path exists.
