# Goal 16 Server Mode Desktop UI Status

Status: Local server mode serves the shared desktop UI; desktop bridge retained as an opt-in mode
Date: 2026-06-23

This document records the Goal 16 slice for starting AgentMux as a CLI-hosted
web-accessible server while keeping the product Windows desktop-first.

## Implemented

- Added the `agentmux server` command family.
- `agentmux server` starts a local HTTP server on `127.0.0.1:8765` by default.
- The default server mode is `local`, so the CLI process owns an in-process
  `RuntimeControlPlane` and `TerminalRuntime`.
- Local server mode supports:
  - `conpty` backend by default, using `powershell.exe -NoLogo`.
  - `wsl-direct` backend when requested, such as `--backend wsl-direct
    --distribution Ubuntu -- bash -l`.
  - a synthetic server workspace, `ws_server`, when no workspace ID is passed.
  - session spawn, text send, named-key send, recent-output read, session list,
    and soft termination through local HTTP APIs.
- Added `--mode desktop-bridge` and `--desktop-control` for cases where the web
  server should expose the running Windows desktop app's workspace/session
  state through the existing named-pipe control plane.
- Added local-only bind safety:
  - default host is loopback.
  - non-loopback hosts require `--allow-remote`.
- `agentmux server` now serves the shared desktop React UI from
  `apps/desktop/dist` instead of a separate standalone web terminal page.
- The server injects `window.__AGENTMUX_SERVER__` into `index.html`; the React
  app selects a server-backed `ControlClient` and keeps the same workspace,
  tab, pane, and terminal components used by the desktop app.
- The server-backed control client maps terminal operations to the local HTTP
  API while maintaining browser-side workspace/pane/surface layout state for
  the shared UI.
- Packaged Windows server mode now includes the shared desktop UI bundle in the
  NSIS artifact and validates that the extracted `agentmux.exe server` can serve
  the same UI without relying on the source-tree `apps/desktop/dist` directory.
- Added `tools/run-server-mode-smoke.ps1` and `npm run server:smoke`.
  The smoke now builds the desktop UI bundle and asserts that `/` contains the
  desktop server bootstrap rather than the retired standalone web-terminal UI.
- Added a per-process local server auth token:
  - the token is emitted in `--json` output and injected into
    `window.__AGENTMUX_SERVER__` for the shared desktop UI.
  - `/api/*` requests require `X-AgentMux-Server-Token`.
  - `/api/session/<session-id>/stream` WebSocket connections require the token
    query parameter.
  - `tools/run-server-mode-smoke.ps1` verifies that unauthenticated `/api/state`
    returns 401 before using the token for the rest of the smoke.

## CLI Examples

```text
agentmux server
agentmux server -- cmd.exe /d /q
agentmux server --port 8787 --backend conpty -- powershell.exe -NoLogo
agentmux server --backend wsl-direct --distribution Ubuntu -- bash -l
agentmux server --mode desktop-bridge --workspace <workspace-id>
agentmux server --host 0.0.0.0 --allow-remote --backend conpty -- cmd.exe /d /q
```

## HTTP Surface

The server currently exposes a small JSON surface:

```text
GET  /
GET  /assets/<desktop-bundle-asset>
GET  /api/state
GET  /api/sessions?workspace=<workspace-id>
POST /api/spawn
GET  /api/session/<session-id>/recent?max_bytes=<n>
POST /api/session/<session-id>/send
POST /api/session/<session-id>/key
POST /api/session/<session-id>/resize
POST /api/session/<session-id>/terminate
```

`POST /api/spawn` accepts:

```json
{
  "workspace_id": "ws_server",
  "backend": "conpty",
  "backend_profile": null,
  "command_line": "cmd.exe /d /q",
  "cwd": null,
  "columns": 120,
  "rows": 36
}
```

## Security Boundary

- Loopback is the default and recommended server binding.
- Remote binding is intentionally explicit through `--allow-remote`.
- Local browser auth tokens are now required for JSON APIs and terminal-stream
  WebSockets.
- Non-loopback remote binding should still be treated as an advanced/developer
  path until packaged UX and operator documentation make token handling clear.

## WSL Behavior

The default backend is now Windows-native `conpty`, so server mode can start on
Windows even when WSL is not installed. WSL terminals remain available as an
explicit opt-in path through `--backend wsl-direct --distribution <name>`.
If WSL is unavailable for an explicit WSL spawn, spawn errors include install
guidance.

## Validation

The following checks passed on 2026-06-19:

```text
C:\Users\irae\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe check -p agentmux-cli
C:\Users\irae\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe test -p agentmux-cli server_
C:\Users\irae\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe build -p agentmux-cli
powershell -NoProfile -ExecutionPolicy Bypass -File tools/run-server-mode-smoke.ps1 -SkipBuild -Port 18766
```

Codexus evidence:

```text
verification_20260619_153908_2a2763
verification_20260619_154010_9f181a
verification_20260619_154025_4d5a81
verification_20260619_154024_ccb403
```

Additional shared-UI server-mode checks passed on 2026-06-23:

```text
npm --prefix apps/desktop run build
C:\Users\irae\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe check -p agentmux-cli
C:\Users\irae\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe build -p agentmux-cli
powershell -NoProfile -ExecutionPolicy Bypass -File tools/run-server-mode-smoke.ps1 -Port 18771
powershell -NoProfile -ExecutionPolicy Bypass -File tools/run-server-mode-smoke.ps1 -SkipBuild -Port 18772
C:\Users\irae\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe test -p agentmux-cli server_
powershell -NoProfile -ExecutionPolicy Bypass -File tools/run-server-mode-smoke.ps1 -SkipBuild -Port 18773
npm run docs:check
```

Codexus evidence:

```text
verification_20260622_150819_32c653
verification_20260622_151055_fa5925
verification_20260622_151239_01b36c
verification_20260622_151247_e11395
verification_20260622_151256_b0a3f3
```

Packaged server-mode release gate checks passed on 2026-06-25:

```text
npm run installer:build-smoke
npm run installer:contents-gate
cx session verify --verify "powershell -NoProfile -ExecutionPolicy Bypass -File tools/run-server-mode-smoke.ps1 -SkipBuild -Port 18777 -AgentMuxExe <extracted-agentmux.exe>" --json
cx session verify --verify ".\.toolchains\cargo\bin\rustup.exe run stable-x86_64-pc-windows-msvc cargo test -p agentmux-cli server_" --json
```

Evidence:

```text
docs/implementation/evidence/20260625-202721-IRAE-DESKTOP-installer-build-smoke
docs/implementation/evidence/20260625-203112-IRAE-DESKTOP-installer-contents-gate
verification_20260625_113139_efbaef
verification_20260625_113214_5df752
```

## Remaining Polish

- Keep stream-first output and backpressure gates current as the terminal path
  evolves.
- Keep manual installed/uninstalled lifecycle evidence current for release
  candidates, especially `-RequireCli` and `-RequireUserPath` checks.
