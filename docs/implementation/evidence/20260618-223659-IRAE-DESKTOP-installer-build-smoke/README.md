# Installer Build Smoke Evidence: IRAE-DESKTOP

Date: 2026-06-18
Machine: IRAE-DESKTOP

## Scope

This smoke verifies that the Windows installer artifact can be produced from
the current Tauri desktop app:

1. Run the frontend build through Tauri's `beforeBuildCommand`.
2. Build the desktop host in release mode.
3. Produce an unsigned NSIS setup executable.
4. Archive the setup executable, size, and SHA-256 hash.

## Result

The smoke passed.

- Command: `tauri build --ci --no-sign -b nsis`
- Exit code: 0
- Installer: `AgentMux_0.1.0_x64-setup.exe`
- Installer size: 3102053 bytes
- Installer SHA-256:
  `5A773E697AFCF77537591BED28CDF62A55BA046113B7CD8FB7D3BA04B72A7D5F`

This is an installer artifact build smoke. It does not install or uninstall the
application on the machine.

Artifacts:

- `AgentMux_0.1.0_x64-setup.exe`
- `summary.json`
- `installer-build.stdout.txt`
- `installer-build.stderr.txt`
