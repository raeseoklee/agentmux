# Goal 9 Performance Diagnostics Status

Status: In progress
Date: 2026-06-18

This document records Goal 9 implementation slices for performance benchmarks,
diagnostics export, queue pressure visibility, and release-candidate gates.

## Implemented

- `agentmux-core` exposes bounded runtime queue depths and capacities for:
  - pending event queue
  - replay event history
  - runtime notification history
- `agentmux-ipc` defines `DiagnosticsExportResult` with:
  - generated timestamp and format version
  - recovery diagnostics
  - recent browser automation failures
  - persisted notification summaries
  - backend health summaries
  - queue pressure summaries
- The desktop host implements `diagnostics.export`.
- Backend health is derived from persisted recovery/session state and grouped by
  backend kind with active, recovering, and failed session counts.
- Queue pressure reports depth, capacity, dropped count, and state for runtime
  event queues, runtime notifications, and desktop browser failure diagnostics.
- The CLI exposes the diagnostics bundle with `agentmux diagnostics export`.
- `agentmux diagnostics export --json` returns the control envelope for scripts,
  while the default output prints a compact human-readable summary.
- Benchmark harnesses now cover the required Goal 9 scenarios:
  - `agentmux-bench-single-terminal-latency`
  - `agentmux-bench-many-idle-sessions`
  - `agentmux-bench-high-output`
  - `agentmux-bench-resize-storm`
  - `agentmux-bench-restart-recovery`
- Benchmark reports are structured JSON and include scenario parameters,
  latency distributions, process samples where available, and queue pressure.
- `docs/implementation/18-goal-9-release-candidate-checklist.md` captures the
  reference-machine profile, automated gates, performance gates, manual gates,
  and known-blocker audit needed before release-candidate signoff.
- `tools/run-performance-gates.ps1` records a reference machine profile,
  benchmark JSON outputs, stderr logs, and a manifest in one evidence directory.
- `tools/run-packaged-diagnostics-smoke.ps1` builds the desktop executable,
  launches it with isolated store/token/pipe paths, calls
  `agentmux diagnostics export --json` through the packaged host, validates the
  diagnostics bundle shape, and archives the evidence.
- `tools/run-desktop-ui-gates.ps1` runs the desktop production build, verifies
  the `dist` output, runs the Playwright UI smoke suite, and archives the
  evidence.
- `tools/run-installer-build-smoke.ps1` builds an unsigned NSIS setup
  executable, verifies the artifact, records its size and SHA-256 hash, and
  archives the evidence.
- `tools/run-tmux-reattach-smoke.ps1` finds a WSL distribution with tmux,
  runs the live tmux-control launch and reattach integration smokes, rejects
  skipped runs, and archives the evidence.

## Validation

The following checks passed on 2026-06-18 using the repository-local Rust
toolchain:

```text
cargo test -p agentmux-ipc -p agentmux-core -p agentmux-cli -p agentmux-desktop-host diagnostics
```

Covered behavior includes:

- desktop control routing for `diagnostics.export`
- recovery counts in the exported bundle
- backend health grouping
- runtime and desktop queue pressure entries
- browser failure history inclusion
- persisted `browser.action_failed` notification inclusion

Benchmark compile and smoke validation should run after each benchmark change:

```text
cargo test -p agentmux-bench-support -p agentmux-bench-single-terminal-latency -p agentmux-bench-many-idle-sessions -p agentmux-bench-high-output -p agentmux-bench-resize-storm -p agentmux-bench-restart-recovery
cargo run -p agentmux-bench-many-idle-sessions -- --sessions 1 --observe-ms 250
cargo run -p agentmux-bench-high-output -- --lines 100 --visible-probes 1
cargo run -p agentmux-bench-resize-storm -- --iterations 5
cargo run -p agentmux-bench-restart-recovery -- --sessions 2
tools/run-performance-gates.ps1 -Smoke -OutputDir <temp-dir>
```

The benchmark slice was smoke-validated on 2026-06-18 with:

- `cargo run -p agentmux-bench-single-terminal-latency`
- `cargo run -p agentmux-bench-many-idle-sessions -- --sessions 1 --observe-ms 250`
- `cargo run -p agentmux-bench-high-output -- --lines 100 --visible-probes 1`
- `cargo run -p agentmux-bench-resize-storm -- --iterations 5`
- `cargo run -p agentmux-bench-restart-recovery -- --sessions 2`
- `tools/run-performance-gates.ps1 -Smoke -OutputDir <temp-dir>`, which wrote
  `reference-profile.json`, `manifest.json`, benchmark JSON outputs, and stderr
  logs.

The full repository check also passed after the benchmark additions:

```text
npm run check
```

A full performance-gate run was recorded on `IRAE-DESKTOP`:

- [manifest.json](./evidence/20260618-220217-IRAE-DESKTOP-performance-gates/manifest.json)
- [summary](./evidence/20260618-220217-IRAE-DESKTOP-performance-gates/README.md)
- [reference-profile.json](./evidence/20260618-220217-IRAE-DESKTOP-performance-gates/reference-profile.json)
- [browser-cdp-smoke.txt](./evidence/20260618-220217-IRAE-DESKTOP-performance-gates/browser-cdp-smoke.txt)

A packaged diagnostics export smoke was recorded on `IRAE-DESKTOP`:

- [summary.json](./evidence/20260618-220709-IRAE-DESKTOP-packaged-diagnostics-smoke/summary.json)
- [diagnostics-export.json](./evidence/20260618-220709-IRAE-DESKTOP-packaged-diagnostics-smoke/diagnostics-export.json)
- [summary](./evidence/20260618-220709-IRAE-DESKTOP-packaged-diagnostics-smoke/README.md)

A desktop build and UI smoke gate was recorded on `IRAE-DESKTOP`:

- [summary.json](./evidence/20260618-223819-IRAE-DESKTOP-desktop-ui-gates/summary.json)
- [summary](./evidence/20260618-223819-IRAE-DESKTOP-desktop-ui-gates/README.md)
- [archived dist](./evidence/20260618-223819-IRAE-DESKTOP-desktop-ui-gates/dist/index.html)
- [ui-smoke.stdout.txt](./evidence/20260618-223819-IRAE-DESKTOP-desktop-ui-gates/ui-smoke.stdout.txt)

An installer artifact build smoke was recorded on `IRAE-DESKTOP`:

- [summary.json](./evidence/20260618-223659-IRAE-DESKTOP-installer-build-smoke/summary.json)
- [summary](./evidence/20260618-223659-IRAE-DESKTOP-installer-build-smoke/README.md)
- [installer](./evidence/20260618-223659-IRAE-DESKTOP-installer-build-smoke/AgentMux_0.1.0_x64-setup.exe)
- [installer-build.stderr.txt](./evidence/20260618-223659-IRAE-DESKTOP-installer-build-smoke/installer-build.stderr.txt)

A real WSL/tmux launch and reattach smoke was recorded on `IRAE-DESKTOP`:

- [summary.json](./evidence/20260618-222707-IRAE-DESKTOP-real-tmux-reattach-smoke/summary.json)
- [summary](./evidence/20260618-222707-IRAE-DESKTOP-real-tmux-reattach-smoke/README.md)
- [tmux-control-smoke.stdout.txt](./evidence/20260618-222707-IRAE-DESKTOP-real-tmux-reattach-smoke/tmux-control-smoke.stdout.txt)

## Remaining Work

- Run the same release performance gates on any additional named Windows
  reference machines selected for release signoff.
- Fill in manual install/uninstall smoke in the release-candidate checklist.

## Summary

Goal 9 now has diagnostics export with backend health and queue pressure
evidence, benchmark binaries for the required performance scenarios, packaged
diagnostics smoke evidence, desktop build/UI smoke evidence, browser CDP fixture
evidence, installer artifact build evidence, and real WSL/tmux reattach
evidence on the named Windows reference machine.
