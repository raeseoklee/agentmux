# Goal 9 Release Candidate Checklist

Status: Evidence in progress
Date: 2026-06-18

This checklist records the evidence required before Goal 9 can be marked
complete for a release candidate.

## Reference Machine Profile

Record the following before running release gates:

- Machine name:
- Windows edition and version:
- WSL version:
- Installed WSL distributions:
- CPU:
- RAM:
- GPU/display:
- Display scale:
- Power mode:
- AgentMux commit:
- AgentMux version:

## Automated Gates

| Gate | Command | Evidence |
|---|---|---|
| Rust format | `cargo fmt --all -- --check` | passed via `npm run check` on 2026-06-18 |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` | passed via `npm run check` on 2026-06-18 |
| Rust tests | `cargo test --workspace` | passed via `npm run check` on 2026-06-18 |
| Desktop build | `npm --prefix apps/desktop run build` | `docs/implementation/evidence/20260618-223819-IRAE-DESKTOP-desktop-ui-gates` |
| UI smoke | `npm --prefix apps/desktop run test:ui` | `docs/implementation/evidence/20260618-223819-IRAE-DESKTOP-desktop-ui-gates` |
| Tauri debug build | `npm --prefix apps/desktop run tauri:build -- --debug --no-bundle --ci` | `docs/implementation/evidence/20260618-220709-IRAE-DESKTOP-packaged-diagnostics-smoke` |
| Docs links | `npm run docs:check` | passed via `npm run check` on 2026-06-18 |
| Diagnostics export | `npm run diagnostics:packaged-smoke` | `docs/implementation/evidence/20260618-220709-IRAE-DESKTOP-packaged-diagnostics-smoke` |
| Installer artifact build | `npm run installer:build-smoke` | `docs/implementation/evidence/20260618-223659-IRAE-DESKTOP-installer-build-smoke` |
| Browser CDP fixture smoke | `npm run browser:cdp-smoke` | `docs/implementation/evidence/20260618-220217-IRAE-DESKTOP-performance-gates/browser-cdp-smoke.txt` |
| Real WSL/tmux reattach smoke | `npm run tmux:reattach-smoke` | `docs/implementation/evidence/20260618-222707-IRAE-DESKTOP-real-tmux-reattach-smoke` |

## Performance Gates

Archive JSON output for each command:

```powershell
cargo run -p agentmux-bench-single-terminal-latency
cargo run -p agentmux-bench-many-idle-sessions
cargo run -p agentmux-bench-high-output
cargo run -p agentmux-bench-resize-storm
cargo run -p agentmux-bench-restart-recovery
```

The commands can be captured into one evidence directory with:

```powershell
npm run perf:gates
```

Use `tools/run-performance-gates.ps1 -Smoke -OutputDir <temp-dir>` to verify the
runner without producing release evidence.

Required release evidence:

- 20 idle sessions show no sustained jank.
- 50 idle sessions show bounded memory growth.
- Single visible input p95 stays within the latency budget.
- Hidden high output does not freeze visible input.
- Resize storm leaves the terminal session usable.
- Real WSL/tmux restart recovery does not create duplicate durable sessions.
  Evidence: `docs/implementation/evidence/20260618-222707-IRAE-DESKTOP-real-tmux-reattach-smoke`.

## Manual Gates

- Fresh install smoke:
- Existing WSL distribution smoke:
- No WSL distribution diagnostic:
- Durable WSL recovery smoke:
  `docs/implementation/evidence/20260618-222707-IRAE-DESKTOP-real-tmux-reattach-smoke`
- Browser CDP smoke against local fixture: command available as
  `npm run browser:cdp-smoke`; evidence archived in
  `docs/implementation/evidence/20260618-220217-IRAE-DESKTOP-performance-gates`.
- Installer artifact build:
  `docs/implementation/evidence/20260618-223659-IRAE-DESKTOP-installer-build-smoke`
- Manual install/uninstall smoke:

Manual install/uninstall smoke steps:

1. Run `AgentMux_0.1.0_x64-setup.exe` from the installer artifact evidence.
2. Launch AgentMux from the installed shortcut or Start menu entry.
3. Confirm the main window opens and the initial workspace UI renders.
4. Confirm the installed app can create a native shell pane.
5. Close the app and uninstall it through Windows Apps settings or the
   generated uninstaller.
6. Confirm the app no longer launches from the installed shortcut or Start menu
   entry.

## Known Blocker Audit

A release candidate cannot pass while any item is true:

- duplicate durable session bug: not reproduced by the real WSL/tmux reattach
  smoke; same shell process handled post-reattach input.
- unbounded output memory growth: not observed in the full performance-gate run;
  high-output and idle-session scenarios completed with dropped events at 0.
- unauthenticated local IPC control path: `npm run check` covers token rejection
  and owner-only token ACL tests.
- crash on backend disconnect: `npm run check` covers detach/exit and backend
  lifecycle tests; broader manual disconnect exploration remains useful.
- destructive close without explicit user action: `npm run check` covers
  terminate and workspace-close confirmation requirements.

## Current Status

The benchmark binaries and diagnostics export path are implemented. A full
benchmark evidence run exists for
`docs/implementation/evidence/20260618-220217-IRAE-DESKTOP-performance-gates`.
That evidence directory also contains archived browser CDP fixture smoke output.
Packaged-app diagnostics export smoke evidence exists for
`docs/implementation/evidence/20260618-220709-IRAE-DESKTOP-packaged-diagnostics-smoke`.
Real WSL/tmux restart recovery evidence exists for
`docs/implementation/evidence/20260618-222707-IRAE-DESKTOP-real-tmux-reattach-smoke`.
Installer artifact build evidence exists for
`docs/implementation/evidence/20260618-223659-IRAE-DESKTOP-installer-build-smoke`.
Final gate completion still requires manual install/uninstall smoke.
