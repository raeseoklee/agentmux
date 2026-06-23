# Installer Contents Gate

Generated: 2026-06-19T11:21:33.5365735Z

This non-installing gate opens the NSIS setup executable with 7-Zip, verifies
that the installer script installs the CLI sidecars, extracts installer contents
to an ignored runtime directory, and compares extracted CLI sidecar hashes with
the prepared Tauri sidecar inputs.

- Result: passed
- Installer: docs/implementation/evidence/20260619-200830-IRAE-DESKTOP-installer-build-smoke/AgentMux_0.1.0_x64-setup.exe
- agentmux.exe in archive: True
- cmux.exe in archive: True
