# Installer Contents Gate

Generated: 2026-06-25T11:31:15.5340386Z

This non-installing gate opens the NSIS setup executable with 7-Zip, verifies
that the installer script installs the CLI sidecars, extracts installer contents
to an ignored runtime directory, and compares extracted CLI sidecar hashes with
the prepared Tauri sidecar inputs.

- Result: passed
- Installer: docs/implementation/evidence/20260625-202721-IRAE-DESKTOP-installer-build-smoke/AgentMux_0.1.0_x64-setup.exe
- agentmux.exe in archive: True
- cmux.exe in archive: True
- Desktop UI index in archive: True
- Desktop UI extracted root: docs/implementation/evidence/20260625-203112-IRAE-DESKTOP-installer-contents-gate/runtime/installer-extract/dist
- Desktop UI extracted asset count: 7
- PATH hook included: True
- PATH hook writes user PATH: True
