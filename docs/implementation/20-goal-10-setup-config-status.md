# Goal 10 Windows Setup and Config Foundation Status

Status: In progress
Date: 2026-06-19

This document records the first implementation slice for Goal 10 from the cmux Windows parity analysis: Windows setup and config foundation.

## Implemented

- `agentmux-ipc` now defines typed config payloads:
  - `AppConfigAppearance`
  - `AppConfigGetParams`
  - `AppConfigResult`
  - `AppConfigAppearanceUpdate`
  - `AppConfigUpdateParams`
- The desktop host now routes:
  - `config.get`
  - `config.update`
  - `config.export`
  - `config.import`
  - `config.reset`
- The desktop host resolves the app config path as:
  - `AGENTMUX_CONFIG_PATH` when set
  - otherwise `%APPDATA%\AgentMux\agentmux.json`
  - fallback to `%LOCALAPPDATA%\AgentMux\agentmux.json`
  - fallback to the current directory for constrained test/dev environments
- The app config currently persists appearance settings:
  - theme: `dark` or `light`
  - accent key
  - UI font size, clamped to the existing UI range
- Missing config files produce default config instead of blocking startup.
- Invalid config JSON or unsupported appearance values return explicit control errors.
- The React desktop UI now loads appearance settings through the control client and saves changes back through `config.update`.
- The browser preview control client mirrors the same config shape with `localStorage` so UI automation can verify reload persistence without Tauri.
- Added `config.reload` to the desktop control plane.
- Added `agentmux config get` and `agentmux config reload` CLI commands with
  `--json` support.
- `agentmux config get` and `agentmux config reload` accept `--workspace` to
  return the effective app plus project config for that workspace.
- Added a Settings > General reload control that rereads the app config and
  applies appearance and shortcut bindings without restarting the app.
- Added Settings > General config Export, Import, and Reset controls.
  - Export copies or displays a JSON config snapshot.
  - Import accepts pasted JSON and applies it through the control plane.
  - Reset restores the global config defaults.
- Settings > General also exposes project-scope Export project, Import project,
  and Reset project controls when the active workspace has a project config
  path.
- `config.export` accepts `scope: "project"` and returns the raw
  `.agentmux/agentmux.json` shape instead of the global/effective config shape.
- The desktop host now discovers `<projectRoot>/.agentmux/agentmux.json` for a
  requested workspace and overlays its `shortcuts.bindings` on top of global
  `%APPDATA%\AgentMux\agentmux.json` bindings.
- The effective app config now includes `actions.custom` definitions from both
  global and project config files, with project actions overriding global
  actions by `custom.*` id.
- Config validation now accepts browser custom action presets that open or
  navigate browser surfaces, while still rejecting arbitrary command arrays for
  non-agent targets.
- Settings > General shows both the global config path and the active
  workspace project config path.
- Project config loading now supports safe cmux compatibility fallback:
  `<projectRoot>/.agentmux/agentmux.json` remains the AgentMux write/export
  target, but if that file does not exist AgentMux can read compatible fields
  from `<projectRoot>/.cmux/cmux.json`. The fallback imports only fields already
  handled by the AgentMux validator: shortcuts, custom actions, UI action
  hooks, and notification action hooks.
- Project config now has an explicit cmux migration path:
  `config.migrate_project` reads compatible fields from
  `<projectRoot>/.cmux/cmux.json`, writes them to the AgentMux-owned
  `<projectRoot>/.agentmux/agentmux.json`, and refuses to overwrite an existing
  AgentMux project config unless the caller passes `overwrite=true`.
- The CLI exposes the migration as `agentmux config migrate-cmux`, with
  `--workspace`, `--force`/`--overwrite`, common control options, and managed
  terminal workspace environment fallback.
- Settings > General exposes a safe `Migrate .cmux` project button that applies
  the migrated effective config without restarting the app.
- Config diagnostics now report global, AgentMux project, and cmux project
  sources with existence, validity, active-source status, path, and actionable
  messages without forcing the normal config loader to succeed first.
- The CLI exposes diagnostics as `agentmux config diagnostics`, with
  `--workspace`, `--json`, common control options, and managed terminal
  workspace environment fallback.
- Settings > General shows the same config diagnostics rows, so invalid JSON or
  ignored `.cmux` fallback state is visible in the app.
- A published JSON Schema is available at
  `docs/schemas/agentmux.config.schema.json` for global and project config
  files.
- The CLI exposes the same schema as `agentmux config schema`, with optional
  `--output <path>` and `--json` support for editor/tool setup scripts.
- The schema and validators now document and accept browser custom action
  recipes for screenshot, DOM snapshot, evaluate, click, and type in addition
  to URL navigation presets.
- The desktop UI now includes a Windows setup panel opened from the WSL/tmux
  warning banner or the command palette `Windows setup` action.
- The setup panel shows WSL distribution readiness, WSL install guidance,
  selected distribution, tmux probe status, tmux install command, workspace
  project root, default WSL distribution save, and CLI smoke commands.
- The tmux probe can now run against the selected setup distribution instead
  of only the system/default distribution.

## Verification

The following checks passed on 2026-06-19:

```powershell
npm --prefix apps/desktop run build
npm --prefix apps/desktop run test:ui
npm run docs:check
$toolchain = Join-Path (Get-Location) '.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin'
$env:PATH = "$toolchain;$env:PATH"
.\.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\rustfmt.exe --edition 2021 --check apps\desktop\src-tauri\src\lib.rs crates\agentmux-ipc\src\lib.rs
.\.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe test -p agentmux-ipc -p agentmux-desktop-host desktop_config_update_persists_appearance_settings -- --nocapture
.\.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe test -p agentmux-ipc parses_app_config_migrate_project_params -- --nocapture
.\.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe test -p agentmux-ipc parses_app_config_diagnostics_params -- --nocapture
.\.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe test -p agentmux-cli config_migrate_cmux_parses_workspace_force_and_json_output -- --nocapture
.\.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe test -p agentmux-cli config_diagnostics_parses_workspace_and_json_output -- --nocapture
.\.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe test -p agentmux-cli config_schema_outputs_valid_json_schema -- --nocapture
.\.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe test -p agentmux-desktop-host desktop_config_migrates_cmux_project_config_to_agentmux_path -- --nocapture
.\.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe test -p agentmux-desktop-host desktop_config_diagnostics_reports_invalid_sources_without_loading_them -- --nocapture
.\.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe test -p agentmux-desktop-host desktop_actions_run_executes_control_safe_actions -- --nocapture
npm --prefix apps/desktop run build
cd apps/desktop; .\node_modules\.bin\playwright.cmd test -g "settings can migrate preview cmux project config"
cd apps/desktop; .\node_modules\.bin\playwright.cmd test tests/ui/agentmux-design.spec.ts -g "custom browser config actions"
npx playwright test tests/ui/agentmux-design.spec.ts -g "setup wizard|shows WSL install guidance"
```

Coverage added:

- Rust desktop-host test verifies `config.update` writes appearance settings and a reopened desktop state reads them back through `config.get`.
- Playwright UI test verifies the appearance theme persists through browser-preview reload.
- UI test verifies Settings reload applies externally changed preview config
  without an app restart.
- Rust desktop-host test now exercises `config.reload` after reopening state.
- Rust desktop-host test verifies workspace-scoped `config.get` merges project
  shortcut overrides and custom action definitions over global config.
- Rust desktop-host tests verify config import/export/reset, plus project config
  export/reset through a workspace-scoped request.
- Rust desktop-host tests verify browser custom action presets are normalized
  during config import.
- Rust desktop-host test verifies `.cmux/cmux.json` fallback loading and
  confirms `.agentmux/agentmux.json` takes precedence when both files exist.
- Rust desktop-host test verifies `.cmux/cmux.json` migration writes the
  normalized AgentMux project config path, refuses accidental overwrite, and
  supports explicit overwrite.
- Rust desktop-host test verifies config diagnostics can report invalid global
  and cmux project config sources without failing the diagnostics request.
- Playwright UI coverage verifies Settings global and project config JSON import
  and reset.
- Playwright UI coverage verifies Settings can migrate a preview `.cmux`
  project config, show updated diagnostics rows, and immediately apply the
  migrated workspace-plus action.
- CLI parser coverage verifies `agentmux config reload --json --pipe ...
  --workspace ...`.
- CLI parser coverage verifies `agentmux config migrate-cmux --workspace ...
  --force --json`.
- CLI parser coverage verifies `agentmux config diagnostics --workspace ...
  --json`.
- CLI coverage verifies `agentmux config schema` emits valid JSON Schema and
  parses `--output`/`--json`.
- Desktop-host action coverage verifies `actions.run` can execute a custom
  browser screenshot recipe through the browser control plane.
- Playwright UI coverage verifies the WSL missing banner opens the setup panel
  with `wsl --install` guidance.
- Playwright UI coverage verifies the command palette opens the setup panel,
  tmux probing succeeds in the preview WSL distribution, and saved project root
  plus default WSL distribution are reflected in workspace settings.

## Known Notes

- This slice does not yet implement a project-local config trust policy UI or a
  full editable config surface.

## Remaining Goal 10 Work

- Project-local config trust model beyond read-only shortcut overrides.
- Move more settings out of local React state as the setting surfaces mature.
