# Goal 18 P2 Installed Server Mode Release Gate

Status: Completed
Date: 2026-06-25

This document records the P2 release gate for packaged Windows server mode.
The gate proves that `agentmux.exe server` from the NSIS artifact can serve the
same desktop UI without relying on the source-tree `apps/desktop/dist`
directory.

## Implemented

- Tauri now bundles the desktop UI build output as a `dist` resource in the
  NSIS artifact.
- `agentmux.exe server` now searches packaged UI locations in addition to
  source-tree development locations:
  - `<exe-dir>/dist`
  - `<exe-dir>/resources/dist`
  - parent-directory variants used by installed resource layouts
  - existing source-tree `apps/desktop/dist` fallbacks
- `tools/run-installer-contents-gate.ps1` now extracts the full NSIS archive and
  verifies the desktop UI bundle:
  - `dist/index.html` is listed in the installer archive.
  - `dist/assets/*` files are listed and extracted.
  - `agentmux.exe` and `cmux.exe` still match their prepared Tauri sidecar
    inputs.
- `tools/run-server-mode-smoke.ps1` now defaults the server working directory to
  the explicit `-AgentMuxExe` directory. This prevents packaged server smoke
  tests from accidentally finding the source-tree UI bundle through the repo
  root working directory.

## Evidence

NSIS build smoke:

```text
npm run installer:build-smoke
docs/implementation/evidence/20260625-202721-IRAE-DESKTOP-installer-build-smoke
AgentMux_0.1.0_x64-setup.exe
sha256 72774DD4C4ECCB75EC1BA92CE94B15191514F079B0D497B8932858A8162E2B55
```

Installer contents gate:

```text
npm run installer:contents-gate
docs/implementation/evidence/20260625-203112-IRAE-DESKTOP-installer-contents-gate
archive_index_present: true
archive_asset_count: 7
extracted_dist_root: docs/implementation/evidence/20260625-203112-IRAE-DESKTOP-installer-contents-gate/runtime/installer-extract/dist
extracted_asset_count: 7
```

Packaged server smoke:

```text
cx session verify --verify "powershell -NoProfile -ExecutionPolicy Bypass -File tools/run-server-mode-smoke.ps1 -SkipBuild -Port 18777 -AgentMuxExe <extracted-agentmux.exe>" --json
verification_20260625_113139_efbaef
rootStatus: 200
assetStatus: 200
stateStatus: 200
unauthenticatedApiStatus: 401
wslDistributionCount: 3
wslRecentContainsEcho: true
recentContainsEcho: true
serverWorkingDirectory: docs/implementation/evidence/20260625-203112-IRAE-DESKTOP-installer-contents-gate/runtime/installer-extract
```

Server regression tests:

```text
cx session verify --verify ".\.toolchains\cargo\bin\rustup.exe run stable-x86_64-pc-windows-msvc cargo test -p agentmux-cli server_" --json
verification_20260625_113214_5df752
```

Additional local checks:

```text
.\.toolchains\cargo\bin\rustup.exe run stable-x86_64-pc-windows-msvc cargo test -p agentmux-cli desktop_ui_dist_candidates_include_packaged_resource_locations
.\.toolchains\cargo\bin\rustup.exe run stable-x86_64-pc-windows-msvc cargo fmt --check
PowerShell parser checks for tools/run-installer-contents-gate.ps1 and tools/run-server-mode-smoke.ps1
```

## Remaining Separate Gates

- Manual installed/uninstalled lifecycle evidence with
  `npm run installer:lifecycle-gate -- installed -RequireCli -RequireUserPath`
  remains a separate release-candidate signoff.
- End-to-end interactive UI review from a freshly installed app remains a manual
  usability check, not a blocker for this packaged server-mode gate.
