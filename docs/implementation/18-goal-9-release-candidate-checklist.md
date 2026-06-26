# Goal 9 Release Candidate Checklist

Status: Evidence mostly current; manual install/uninstall smoke pending
Date: 2026-06-19

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
| Rust format | `cargo fmt --all -- --check` | passed via `npm run check` on 2026-06-19 |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` | passed via `npm run check` on 2026-06-19 |
| Rust tests | `cargo test --workspace` | passed via `npm run check` on 2026-06-19 |
| Desktop build | `npm --prefix apps/desktop run build` | `docs/implementation/evidence/20260619-141416-IRAE-DESKTOP-desktop-ui-gates` |
| UI smoke | `npm --prefix apps/desktop run test:ui` | `docs/implementation/evidence/20260619-141416-IRAE-DESKTOP-desktop-ui-gates` |
| Tauri debug build | `npm --prefix apps/desktop run tauri:build:debug` | `docs/implementation/evidence/20260619-201829-IRAE-DESKTOP-packaged-diagnostics-smoke` |
| Docs links | `npm run docs:check` | passed via `npm run check` on 2026-06-19 |
| Diagnostics export | `npm run diagnostics:packaged-smoke` | `docs/implementation/evidence/20260619-201829-IRAE-DESKTOP-packaged-diagnostics-smoke`; includes workspace-group restart ordering smoke and sidecar-prepared debug build |
| Installer artifact build | `npm run installer:build-smoke` | `docs/implementation/evidence/20260619-202610-IRAE-DESKTOP-installer-build-smoke`; includes `agentmux.exe` and `cmux.exe` sidecar preparation plus NSIS PATH hook compilation |
| Installer contents gate | `npm run installer:contents-gate` | `docs/implementation/evidence/20260619-202725-IRAE-DESKTOP-installer-contents-gate`; extracts installer sidecars, compares hashes, and verifies PATH hook wiring |
| Installer lifecycle installed gate | `npm run installer:lifecycle-gate -- installed -RequireCli -RequireUserPath` | pending after installing the sidecar/PATH-capable installer |
| Installed app smoke | `npm run installed:app-smoke` | `docs/implementation/evidence/20260619-195449-IRAE-DESKTOP-installed-app-smoke` |
| Browser CDP fixture smoke | `npm run browser:cdp-smoke` | `docs/implementation/evidence/20260619-131503-IRAE-DESKTOP-browser-cdp-smoke` |
| Real WSL/tmux reattach smoke | `npm run tmux:reattach-smoke` | `docs/implementation/evidence/20260619-131302-IRAE-DESKTOP-real-tmux-reattach-smoke` |
| Integration live smoke | `npm run integration:live-smoke` | `docs/implementation/evidence/20260619-195044-IRAE-DESKTOP-integration-live-smoke` |
| Release readiness audit | `npm run release:readiness-audit` | `docs/implementation/evidence/20260619-202822-IRAE-DESKTOP-release-readiness-audit` |
| WSL state gate | `npm run wsl:state-gate -- wsl_with_tmux` | `docs/implementation/evidence/20260619-195904-IRAE-DESKTOP-wsl-state-gate` |

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
  Evidence: `docs/implementation/evidence/20260619-131302-IRAE-DESKTOP-real-tmux-reattach-smoke`.

## Manual Gates

- Fresh install smoke:
- Windows-only WSL missing diagnostic:
  - Start from a machine or VM where `wsl.exe` is unavailable, or where
    `wsl.exe -l -q` returns no distributions.
  - Run `npm run wsl:state-gate -- wsl_exe_missing` or
    `npm run wsl:state-gate -- no_wsl_distribution` and archive the generated
    evidence directory.
  - Launch AgentMux.
  - Confirm the main window remains open.
  - Confirm the setup banner and settings diagnostics direct the user to
    `wsl --install`.
  - Confirm agent launch does not create a duplicate workspace or a partial
    tmux tab.
- Windows-only WSL present without tmux:
  - Use a WSL distribution where `command -v tmux` fails.
  - Run `npm run wsl:state-gate -- wsl_without_tmux` and archive the generated
    evidence directory.
  - Launch AgentMux and run the settings WSL tmux probe.
  - Confirm the diagnostic says tmux is unavailable and includes
    `sudo apt update && sudo apt install -y tmux`.
  - Confirm agent launch leaves the current tab/pane layout intact.
- Windows-only WSL present with tmux:
  - Current machine gate evidence:
    `docs/implementation/evidence/20260619-195904-IRAE-DESKTOP-wsl-state-gate`.
  - Confirm `wsl.exe -d <distribution> -- sh -lc 'command -v tmux && tmux -V'`
    succeeds.
  - Launch a WSL terminal and confirm it opens as a separate top tab.
  - Launch an agent and confirm it opens as another separate top tab.
  - Close the agent tab and confirm all panes belonging to that tab close.
  - Run the real tmux reattach smoke and confirm no duplicate shell process is
    created.
- Durable WSL recovery smoke:
  `docs/implementation/evidence/20260619-131302-IRAE-DESKTOP-real-tmux-reattach-smoke`
- Browser CDP smoke against local fixture: command available as
  `npm run browser:cdp-smoke`; evidence archived in
  `docs/implementation/evidence/20260619-131503-IRAE-DESKTOP-browser-cdp-smoke`.
- Installer artifact build:
  `docs/implementation/evidence/20260619-202610-IRAE-DESKTOP-installer-build-smoke`
  confirms the generated NSIS setup artifact, the Tauri sidecar inputs for
  installed `agentmux.exe` and `cmux.exe`, and the install/uninstall PATH hook
  compiles.
- Installer contents gate:
  `docs/implementation/evidence/20260619-202725-IRAE-DESKTOP-installer-contents-gate`
  opens the generated NSIS setup executable without installing it, verifies the
  generated `installer.nsi` copies `agentmux.exe` and `cmux.exe`, extracts the
  sidecars to an ignored runtime directory, confirms their hashes match the
  prepared Tauri sidecar inputs, and verifies the user PATH install/uninstall
  hook wiring.
- Installer lifecycle installed gate:
  `docs/implementation/evidence/20260619-202735-IRAE-DESKTOP-installer-lifecycle-gate`
  confirms the current machine has an AgentMux registry uninstall entry, the
  installed desktop executable, an uninstall command, and a Start Menu
  shortcut. It also records installed CLI sidecar and user PATH state without
  requiring those checks. The current installed app predates the
  sidecar/PATH-capable installer, so final
  signoff must rerun `npm run installer:lifecycle-gate -- installed
  -RequireCli -RequireUserPath` after installing the latest artifact.
- Installed app smoke:
  `docs/implementation/evidence/20260619-195449-IRAE-DESKTOP-installed-app-smoke`
  launches the installed AgentMux executable with isolated store, token, and
  control pipe paths, then verifies diagnostics export, workspace creation,
  native ConPTY session spawn, and terminal output capture.
- Release readiness audit:
  `docs/implementation/evidence/20260619-202822-IRAE-DESKTOP-release-readiness-audit`
  confirms the installer artifact is present, an installed AgentMux registry
  entry exists, the current machine has WSL distributions with tmux, and the
  current installed app still lacks installed `agentmux.exe`/`cmux.exe`
  sidecars plus install-directory user PATH registration until the latest
  artifact is installed. The integration doctor checks currently need
  attention for wrapper/shim/PATH or underlying-agent readiness.
- Integration live smoke:
  `docs/implementation/evidence/20260619-195044-IRAE-DESKTOP-integration-live-smoke`
  confirms isolated wrapper/shim setup, Windows doctor foundation checks, and
  Ubuntu WSL doctor foundation checks pass without mutating user PATH or
  profile files. The current machine still lacks `opencode` on Windows and WSL
  PATH, so rerun with `-RequireUnderlyingAgents` after installing OpenCode.
- Manual install/uninstall smoke:

Manual install/uninstall smoke steps:

1. On a clean release machine, run
   `npm run installer:lifecycle-gate -- preinstall` and archive the generated
   evidence.
2. Run `AgentMux_0.1.0_x64-setup.exe` from the installer artifact evidence.
3. Run `npm run installer:lifecycle-gate -- installed -RequireCli -RequireUserPath`
   and archive the generated evidence.
4. Launch AgentMux from the installed shortcut or Start menu entry.
5. Confirm the main window opens and the initial workspace UI renders.
6. Confirm the installed app can create a native shell pane, or run
   `npm run installed:app-smoke`.
7. Close the app and uninstall it through Windows Apps settings or the
   generated uninstaller.
8. Run `npm run installer:lifecycle-gate -- uninstalled` and archive the
   generated evidence.
9. Confirm the app no longer launches from the installed shortcut or Start menu
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
The browser CDP fixture smoke now has current dedicated evidence at
`docs/implementation/evidence/20260619-131503-IRAE-DESKTOP-browser-cdp-smoke`.
Packaged-app diagnostics export smoke evidence exists for
`docs/implementation/evidence/20260619-201829-IRAE-DESKTOP-packaged-diagnostics-smoke`;
this run also verifies sidecar-prepared debug Tauri build execution,
workspace-group sort order, and ordered membership after desktop-host restart.
Desktop build and UI smoke evidence exists for
`docs/implementation/evidence/20260619-141416-IRAE-DESKTOP-desktop-ui-gates`
with 30 passing UI tests.
Real WSL/tmux restart recovery evidence exists for
`docs/implementation/evidence/20260619-131302-IRAE-DESKTOP-real-tmux-reattach-smoke`.
Installer artifact build evidence exists for
`docs/implementation/evidence/20260619-202610-IRAE-DESKTOP-installer-build-smoke`.
It proves the current NSIS artifact was built after preparing Tauri sidecar
inputs for `agentmux.exe` and `cmux.exe`, and after compiling the NSIS
install/uninstall PATH hook.
Installer contents gate evidence exists for
`docs/implementation/evidence/20260619-202725-IRAE-DESKTOP-installer-contents-gate`.
It proves the current NSIS setup executable actually contains `agentmux.exe`
and `cmux.exe`, and that extracted sidecar hashes match the prepared Tauri
sidecar inputs. It also verifies the generated installer includes the PATH
hook source and install/uninstall hook calls.
Installed app smoke evidence exists for
`docs/implementation/evidence/20260619-195449-IRAE-DESKTOP-installed-app-smoke`.
It proves the currently installed app can run with an isolated control runtime,
export diagnostics, create a workspace, spawn a native ConPTY session, and read
the expected terminal output marker.
Installer lifecycle installed gate evidence exists for
`docs/implementation/evidence/20260619-202735-IRAE-DESKTOP-installer-lifecycle-gate`.
It proves the current machine is in the installed phase with a registry
uninstall entry, installed executable, uninstall command, and Start Menu
shortcut, and records installed `agentmux.exe`/`cmux.exe` plus user PATH state
without requiring them. Because the currently installed app predates the latest
sidecar/PATH-capable installer, clean-machine preinstall,
`installed -RequireCli -RequireUserPath`, and post-uninstall phase evidence is
still required before the manual install/uninstall gate is closed.
Release readiness audit evidence exists for
`docs/implementation/evidence/20260619-202822-IRAE-DESKTOP-release-readiness-audit`.
That audit observed the local WSL-with-tmux state, the new sidecar/PATH-capable
installer artifact, current installed CLI sidecar absence, missing installed
directory user PATH registration, and integration doctor output, but it does
not replace clean-machine WSL-missing or WSL-without-tmux passes.
WSL state gate evidence exists for
`docs/implementation/evidence/20260619-195904-IRAE-DESKTOP-wsl-state-gate`.
It proves the current machine's WSL matrix state is `wsl_with_tmux` and records
the selected Ubuntu distribution plus tmux version.
Integration live smoke evidence exists for
`docs/implementation/evidence/20260619-195044-IRAE-DESKTOP-integration-live-smoke`.
It proves the isolated integration wrapper/shim foundation and records the
remaining installed-agent gap: `opencode` is not on Windows or Ubuntu WSL PATH.
`npm run check` passed on 2026-06-19 after the current implementation slice.
Final gate completion still requires clean-machine preinstall,
CLI-inclusive installed, and uninstalled phase evidence for the generated
installer plus a clean-machine pass through the unobserved Windows-only WSL
state matrix: `wsl_exe_missing`, `no_wsl_distribution`, and
`wsl_without_tmux`.
