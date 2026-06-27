# Goal 1 Native Terminal Slice Status

Status: Draft
Date: 2026-06-18

This document records the current implementation evidence for Goal 1: Native Terminal Vertical Slice.

## Implemented

- `agentmux-backend` now includes typed backend errors for session lookup, invalid requests, spawn, input, resize, and termination failures.
- `agentmux-backend-conpty` includes a Windows-gated ConPTY backend.
- The ConPTY backend creates pseudoconsole pipes, starts a child process with `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE`, reads output on a background thread, writes translated input bytes, resizes with `ResizePseudoConsole`, and closes resources through session drop.
- The ConPTY process startup path clears inherited standard handles with `STARTF_USESTDHANDLES` so tests and redirected parent processes do not steal child output from the pseudoconsole pipe.
- Non-Windows builds expose a typed `backend_unavailable` result for ConPTY spawn.
- Unit-level helper coverage exists for terminal input byte translation, bracketed paste, command-line quoting, and backend kind.
- A Windows-only smoke test exists at `crates/agentmux-backend-conpty/tests/conpty_smoke.rs` to run `cmd.exe /d /q /c echo agentmux` through ConPTY and assert output plus exit code.
- `agentmux-core` includes a backend-agnostic `TerminalRuntime<B: SessionBackend>` that can spawn sessions, route text/key/resize/terminate commands to a backend, maintain session state, and translate backend events into core events.
- `agentmux-ipc` includes serde-backed control-plane params and result types for session spawn, get, send text, send key, resize, terminate, and read recent output.
- `agentmux-core` includes `RuntimeControlPlane<B: SessionBackend>`, which validates schema/auth token, dispatches `RequestEnvelope` methods to `TerminalRuntime`, maps backend errors into control-plane errors, and returns typed `ResponseEnvelope` results.
- A Windows-only core integration test verifies `RequestEnvelope -> RuntimeControlPlane -> TerminalRuntime -> ConptyBackend -> session.read_recent` with real ConPTY output.
- `apps/desktop` now mounts an xterm.js-based terminal renderer through `XtermTerminalRenderer`.
- The desktop UI includes a `ControlClient` boundary that calls the Tauri `agentmux_control` command with typed control envelopes and falls back to a browser preview client during Vite-only builds.
- The terminal UI forwards xterm input through `ControlClient.sendText`, forwards xterm resize events through `ControlClient.resize`, and polls `session.get` plus `session.read_recent` after spawning a shell so output and exit state are reflected in the same surface.
- `apps/desktop/src-tauri` now contains the `agentmux-desktop-host` Rust crate, which owns a mutex-protected `RuntimeControlPlane<ConptyBackend>` and exposes an `agentmux_control` host function with the same request/response envelope shape expected by the UI.
- `apps/desktop/src-tauri/src/main.rs` registers `agentmux_control` as a Tauri command, manages `DesktopControlState`, enables the global Tauri invoke bridge, and builds as a no-bundle debug Tauri app.
- `benches/single-terminal-latency` now contains a runnable probe that launches a single ConPTY-backed `cmd.exe`, waits for a deterministic prompt, measures command round-trip output through the control envelope, measures resize request latency, and emits JSON metrics.

## Not Yet Implemented

- The desktop terminal renderer uses short-interval polling, not a live backend event stream.
- There is no automated visual Tauri window test that clicks the UI and asserts rendered native shell output.
- IPC transport is still an in-process envelope dispatcher, not a running named pipe server.
- Environment overlays are accepted in the backend spawn request shape but are not yet applied to `CreateProcessW`.
- Output batching and backpressure are not yet implemented around ConPTY output events.

## Verification Evidence

The following commands passed on 2026-06-18 using a repository-local Rust toolchain under `.toolchains`:

```powershell
npm run check
npm --prefix apps/desktop run build
npm --prefix apps/desktop run tauri:build -- --debug --no-bundle --ci
cargo run -p agentmux-bench-single-terminal-latency
```

The check wrapper ran `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `node tools/check-doc-links.mjs`.

The Windows-only ConPTY smoke test passed and verified output plus exit code for `cmd.exe /d /q /c echo agentmux`. The core control-plane smoke test passed and verified recent output for `cmd.exe /d /q /c echo agentmux-control` through typed IPC envelopes. The desktop host smoke test verifies the `agentmux_control` host function can spawn `cmd.exe /d /q /c echo agentmux-desktop-host` and read recent output through the same control envelope path. The desktop xterm/control-client boundary passed `npm run build` from `apps/desktop`, and `npm run tauri:build -- --debug --no-bundle --ci` built `target/debug/agentmux-desktop-host.exe`.

The first local single-terminal latency probe produced this sample output:

```json
{"startup_to_prompt_ms":52.647,"command_round_trip_ms":20.603,"resize_request_ms":0.025}
```

## Remaining Gap

Goal 1 is close enough for the next implementation group to begin, but the proof should still be strengthened with an automated visual/native-app test that launches the Tauri window, clicks the terminal action, types into xterm, and asserts rendered output plus exited state.
