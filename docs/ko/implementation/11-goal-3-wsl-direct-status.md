# Goal 3 WSL Direct Shell Status

Status: Draft
Date: 2026-06-18

This document records the current implementation evidence for Goal 3: WSL Direct Shell.

## Implemented

- `agentmux-backend-wsl` now exposes `WslDirectConfig`, `WslDirectBackend`, `WslDistribution`, and typed `WslDiagnostic` values for WSL-specific setup failures.
- Distribution discovery has a stable command builder for `wsl.exe --list --quiet`.
- Distribution output parsing strips null bytes and BOM markers, accepts quiet output, and preserves a best-effort default marker when present.
- Empty distribution output maps to a typed `no_wsl_distributions` diagnostic.
- WSL cwd resolution accepts explicit Linux paths and `~`, converts Windows drive paths with `wslpath -a` inside the selected distribution when available, falls back to deterministic `/mnt/<drive>/...` conversion when `wslpath` cannot answer, and rejects relative paths with a typed `invalid_wsl_cwd` diagnostic.
- Direct WSL launch command construction uses an argument array shaped as `wsl.exe --distribution <name> --cd <wsl-cwd> --exec <command> <args...>` so paths and commands with spaces are not string-concatenated.
- `WslDirectBackend` now wraps an inner `SessionBackend`. The default inner backend is `ConptyBackend`, which means direct WSL sessions run through the same Windows pseudo-terminal transport as native shells.
- WSL direct spawn translates `SpawnRequest` into the WSL launch shape, clears the Windows cwd before delegation, reports backend kind `wsl-direct`, and delegates input, resize, termination, and event draining to the inner backend.
- `agentmux terminal run` now accepts `--backend wsl-direct`, optional `--distribution <name>`, and optional `--cwd <path>` before the command separator.
- `SessionSpawnParams` now accepts an optional `backend` field, and the core control plane parses `conpty`, `wsl-direct`, and `wsl-tmux-control` backend names.
- `SessionSpawnParams` now also accepts optional `backend_profile`; for `wsl-direct`, this profile is interpreted as the selected WSL distribution name.
- `SessionHandle` now reports the actual backend kind used for the session, so runtimes backed by a router can persist and report `backend_kind = "wsl-direct"` for WSL sessions.
- The desktop host now owns `DesktopBackendRouter`, which routes `conpty` sessions to `ConptyBackend` and `wsl-direct` sessions to `WslDirectBackend`, then routes input, resize, termination, and event draining by session id.
- The React control client now sends `backend: "conpty"` for native shell spawns, preserving existing native behavior while using the same explicit spawn contract.
- The desktop host now exposes `diagnostics.wsl_distributions`, which runs WSL distribution discovery and returns distribution names plus default flags or a typed backend availability/degraded error.
- The React desktop UI now loads WSL distributions through the control client, selects the default distribution when available, and exposes a WSL shell action that spawns `backend: "wsl-direct"` with the selected distribution and workspace root.
- The browser preview control client mirrors WSL distribution discovery and WSL shell spawn so the Vite-only UI path remains usable.
- A Windows-only WSL direct smoke test now discovers an installed distribution, launches `bash` through `WslDirectBackend`, verifies the session opens in `/tmp`, sends terminal input, resizes the backend terminal, and observes clean process exit. The test skips only when no WSL distribution is available.
- WSL direct spawn now validates the selected distribution before launching. Missing selected distributions return typed backend code `wsl_distribution_not_found`, and the control plane maps that to `backend_unavailable` while preserving the backend code in error details.
- A Windows-only cwd conversion smoke test now launches WSL direct with the repository's Windows path as cwd and verifies that the WSL process starts in the matching `/mnt/...` directory, proving the `wslpath`-first/fallback conversion path.
- WSL direct spawn now runs a short WSL launch probe before opening the ConPTY-backed session. Probe timeouts and inner backend spawn timeouts are promoted to typed backend code `wsl_launch_timeout`, which the control plane maps to `timeout` while preserving the backend code in error details.
- Invalid WSL cwd resolution now preserves backend code `invalid_wsl_cwd` through the control plane instead of collapsing to a generic invalid request.

## Diagnostics Status

- Goal 3 has typed diagnostics for missing `wsl.exe`, no-distribution discovery, missing selected distribution, invalid WSL cwd, and WSL launch timeout.

## Verification Evidence

The following targeted commands passed on 2026-06-18 using a repository-local Rust toolchain under `.toolchains`:

```powershell
cargo test -p agentmux-backend-wsl
cargo test -p agentmux-cli
cargo test -p agentmux-core -p agentmux-backend-wsl -p agentmux-desktop-host
```

The WSL backend tests covered:

- discovery command shape
- distribution output parsing and no-distribution diagnostics
- selected distribution validation and `wsl_distribution_not_found`
- WSL direct launch argument shape
- WSL launch probe argument shape and `wsl_launch_timeout` diagnostic code
- Windows drive path conversion with `wslpath` command construction and deterministic fallback
- WSL cwd resolution and invalid relative cwd diagnostics
- spawn translation from AgentMux command/cwd into `wsl.exe --distribution ... --cd ... --exec ...`
- spawn timeout promotion from backend `timeout` to `wsl_launch_timeout`
- delegation of input, resize, termination, and event draining to the inner backend
- Windows lab smoke coverage for selected distribution launch, `/tmp` cwd, terminal input, resize, output, and exit
- Windows lab smoke coverage for missing selected distribution failing before backend launch
- Windows lab smoke coverage for launching with a Windows workspace path and landing in the matching WSL `/mnt/...` cwd

The CLI tests covered:

- existing ConPTY terminal run behavior
- `--backend wsl-direct --distribution <name> --cwd <path>` parsing
- validation that `--distribution` requires `--backend wsl-direct`

The core and desktop host tests covered:

- parsing `session.spawn.backend`
- parsing and forwarding `session.spawn.backend_profile`
- recording backend kind from the returned session handle
- desktop routing of `backend = "wsl-direct"` spawn requests into WSL cwd validation without requiring an installed WSL distribution
- desktop WSL distribution diagnostics returning either distribution JSON or a typed backend availability/degraded error
- preservation of existing ConPTY desktop spawn, read, persistence, and close-policy behavior
- core mapping of `wsl_launch_timeout` to control-plane `timeout` and `invalid_wsl_cwd` to `invalid_request`, preserving backend detail codes

The React desktop build covered:

- native shell spawns continuing to send explicit `backend: "conpty"`
- WSL distribution loading through `diagnostics.wsl_distributions`
- WSL shell spawns sending `backend: "wsl-direct"` plus selected distribution profile

## Status

Goal 3 now has a real WSL launch proof on machines with an installed distribution, typed WSL setup diagnostics, WSL cwd conversion through `wslpath` with deterministic fallback, and a distinct launch timeout diagnostic.
