# Installed Lifecycle E2E Release Closure

Status: Completed
Date: 2026-06-25

This document closes the remaining installed-app lifecycle and release cleanup
items left after Goal 18 P2. It records a real NSIS install, installed desktop
smoke, installed CLI server smoke, and uninstall lifecycle gate on the Windows
lab machine.

## Scope

- Latest installer:
  `target/release/bundle/nsis/AgentMux_0.1.0_x64-setup.exe`
- Installer SHA-256:
  `72774DD4C4ECCB75EC1BA92CE94B15191514F079B0D497B8932858A8162E2B55`
- Install target:
  `C:\Users\irae\AppData\Local\AgentMux`

## Completed Gates

Pre-install audit:

```text
docs/implementation/evidence/20260625-204641-IRAE-DESKTOP-installer-lifecycle-gate
result: passed
```

Installed lifecycle gate:

```text
docs/implementation/evidence/20260625-204723-IRAE-DESKTOP-installer-lifecycle-gate
result: passed
registry_entry_present: true
installed_executable_present: true
installed_agentmux_cli_present: true
installed_cmux_cli_present: true
install_directory_on_user_path: true
uninstall_command_present: true
shortcuts_present: true
```

Installed desktop app smoke:

```text
cx session verify --verify "powershell -NoProfile -ExecutionPolicy Bypass -File tools/run-installed-app-smoke.ps1 -SkipBuild -CliExe C:\Users\irae\AppData\Local\AgentMux\agentmux.exe" --json
verification_20260625_114809_1071cb
docs/implementation/evidence/20260625-204809-IRAE-DESKTOP-installed-app-smoke
result: passed
installed_executable: C:\Users\irae\AppData\Local\AgentMux\agentmux-desktop-host.exe
cli_executable: C:\Users\irae\AppData\Local\AgentMux\agentmux.exe
```

Installed CLI server smoke:

```text
cx session verify --verify "powershell -NoProfile -ExecutionPolicy Bypass -File tools/run-server-mode-smoke.ps1 -SkipBuild -Port 18779 -AgentMuxExe C:\Users\irae\AppData\Local\AgentMux\agentmux.exe" --json
verification_20260625_114809_0e6853
rootStatus: 200
assetStatus: 200
stateStatus: 200
unauthenticatedApiStatus: 401
wslDistributionCount: 3
wslRecentContainsEcho: true
recentContainsEcho: true
serverWorkingDirectory: C:\Users\irae\AppData\Local\AgentMux
```

Uninstalled lifecycle gate:

```text
docs/implementation/evidence/20260625-205038-IRAE-DESKTOP-installer-lifecycle-gate
result: passed
registry_entry_present: false
installed_executable_present: false
installed_agentmux_cli_present: false
installed_cmux_cli_present: false
install_directory_on_user_path: false
uninstall_command_present: false
shortcuts_present: false
```

## Script Hardening

`tools/run-installed-app-smoke.ps1` now accepts `-CliExe` and prefers the
installed `agentmux.exe` next to the discovered installed desktop host. This
prevents installed-app smoke from launching the installed desktop executable but
accidentally using `target\debug\agentmux.exe` for control-plane commands.

## Observations

- The installed phase proved user PATH registration through the registry-backed
  lifecycle gate. The already-running Codex PowerShell process did not refresh
  its inherited PATH, so `where agentmux` from that process did not find the new
  command until a future shell starts.
- Silent uninstall removed executable payloads, registry uninstall metadata,
  Start Menu shortcuts, and the user PATH entry.
- Silent uninstall intentionally left user data in
  `C:\Users\irae\AppData\Local\AgentMux`, including SQLite store and control
  token files. The lifecycle gate treats executable and registration cleanup as
  the release blocker, not user data removal.

## Release Status

The P2 packaged server-mode gate plus the installed lifecycle closure are now
covered by committed evidence. Remaining release work is ordinary product QA:
manual exploratory UI pass, performance budget review, and any final installer
signing/versioning decision.
