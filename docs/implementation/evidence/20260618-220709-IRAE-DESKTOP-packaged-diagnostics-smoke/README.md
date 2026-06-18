# Packaged Diagnostics Smoke Evidence: IRAE-DESKTOP

Date: 2026-06-18
Machine: IRAE-DESKTOP

## Scope

This smoke verifies the packaged desktop host control pipe path:

1. Build the Tauri desktop executable with `npm --prefix apps/desktop run
   tauri:build -- --debug --no-bundle --ci`.
2. Build the CLI with `cargo build -p agentmux-cli`.
3. Launch `target/release/agentmux-desktop-host.exe` with isolated
   `AGENTMUX_STORE_PATH`, `AGENTMUX_CONTROL_TOKEN_PATH`, and
   `AGENTMUX_CONTROL_PIPE`.
4. Call `agentmux diagnostics export --json` against that pipe and token.
5. Validate the response envelope and diagnostics bundle shape.

## Result

The smoke passed.

- Desktop executable: `target/release/agentmux-desktop-host.exe`
- CLI executable: `target/debug/agentmux.exe`
- Diagnostics envelope schema: `agentmux.control.v1`
- Diagnostics format version: `agentmux.diagnostics.v1`
- Queue pressure entries: 4
- Backend health entries: 0 for the empty smoke runtime
- Browser failure entries: 0

Artifacts:

- `summary.json`
- `diagnostics-export.json`
- `diagnostics-export.stderr.txt`
- `tauri-build.stdout.txt`
- `tauri-build.stderr.txt`
- `cli-build.stdout.txt`
- `cli-build.stderr.txt`

The isolated smoke store and control token were generated under a local
`runtime` directory during the run. Those runtime files are intentionally not
committed.
