# Goal 11 Action Registry and Shortcuts Status

Status: Implemented planned slices plus control-plane execution
Date: 2026-06-19

This document records the implementation slices for Goal 11 from the cmux Windows parity analysis: action registry, shortcut resolver, command palette wiring, custom actions, and configurable surface chrome.

## Implemented Slice

- Added a React-side action registry model with stable action IDs for:
  - command palette, search, settings, theme toggle
  - new workspace
  - new WSL terminal
  - split right and split down
  - browser surface open
  - Claude, Codex, and custom durable agent launches
  - dynamic workspace select and WSL distribution shell actions
- Replaced the hard-coded command palette rows with registry-backed rows.
- Added Windows shortcut defaults derived from the cmux shortcut model:
  - `app.commandPalette`: `ctrl+shift+p`
  - `app.commandPalette.legacy`: `ctrl+k`
  - `workspace.new`: `ctrl+n`
  - `terminal.newWsl`: `ctrl+t`
  - `terminal.textBox`: `ctrl+alt+i`
  - `pane.splitRight`: `ctrl+d`
  - `pane.splitDown`: `ctrl+shift+d`
  - `browser.openNewTab`: `ctrl+shift+l`
  - `app.search`: `ctrl+f`
  - `app.settings`: `ctrl+,`
  - `view.toggleTheme`: `ctrl+alt+l`
- Added a shortcut resolver with one-step and two-step chord support.
- Added config-backed shortcut overrides at `shortcuts.bindings`.
  - Supported values match the cmux shape used for shortcuts: a string, a two-item string array, or `null`.
  - Empty strings and `none`/`clear`/`unbound`/`disabled` are treated as unbound in the UI resolver.
- Extended `config.get` and `config.update` to persist shortcut bindings in `%APPDATA%\AgentMux\agentmux.json`.
- Updated the Settings shortcut tab to render the live action registry and resolved shortcut labels.
- Added command palette keyboard navigation:
  - `ArrowDown` and `ArrowUp` move the selected result.
  - `Enter` executes the selected result.
  - The selection resets when the palette opens or the query changes.
- Added user-editable shortcut rebinding in Settings > Shortcuts.
  - A shortcut can be changed with a single stroke such as `ctrl+t`.
  - A two-step chord can be entered as `ctrl+b, c`.
  - A shortcut can be cleared from the same row.
- Added duplicate shortcut conflict diagnostics in the shortcut settings tab.
- Added workspace-scoped shortcut overrides from
  `<projectRoot>/.agentmux/agentmux.json`.
  - `config.get`, `config.reload`, and `config.update` can receive a
    `workspace_id` and return the effective global plus project shortcut map.
  - Project bindings override `%APPDATA%\AgentMux\agentmux.json` bindings.
  - `agentmux config get/reload --workspace <id>` exposes the same effective
    config through the CLI.
  - Switching the active workspace reloads shortcut bindings for that project.
- Added config-defined custom actions at `actions.custom`.
  - Custom action IDs must use the `custom.*` namespace so project config cannot
    override built-in actions.
  - Supported execution targets are `agent`, `wsl-terminal`, and `browser`.
  - `agent` actions require an explicit command array and run through the
    existing durable WSL/tmux agent launch path.
  - `wsl-terminal` actions reuse the existing terminal creation path and do
    not accept arbitrary command arrays.
  - `browser` actions reuse the existing browser creation path. Empty command
    arrays open a browser tab, while normalized presets such as
    `["open", "<url>", "new_tab"]`, `["new-tab", "<url>"]`, and
    `["active-pane", "<url>"]` open or navigate browser surfaces without
    granting arbitrary host command execution.
  - Browser custom actions also support safe automation recipes for
    screenshot capture, DOM snapshot, JavaScript evaluation, selector/coordinate
    click, selector typing, form fill/press/select/scroll/hover/check,
    highlight/focus, wait-for-selector, zoom, and navigation controls through
    the existing browser control-plane methods.
  - Config-defined browser automation recipes can target frames with
    `frame:<frame-id>`, `frame=<frame-id>`, `frame-id:<frame-id>`,
    `frame-id=<frame-id>`, `frame_id:<frame-id>`, or
    `frame_id=<frame-id>` command tokens. Normalized persisted commands remain
    idempotent when they already contain the trailing frame slot.
  - Custom actions appear in the command palette, shortcut settings, and
    shortcut resolver.
- Added config-driven UI action hooks at `ui`.
  - `ui.workspace_plus_action` controls the sidebar workspace plus button.
  - `ui.surface_tab_plus_action` controls the surface tab strip plus button.
  - `ui.surface_tab_actions` controls the action buttons rendered on the
    right side of the surface tab bar.
  - Missing or unresolved configured action IDs fall back to the built-in
    defaults for the two plus buttons.
  - An explicit empty `surface_tab_actions` array hides the tab-bar action
    buttons.
  - Project `.agentmux/agentmux.json` UI settings override global UI settings
    for the active workspace.
- Added config-driven notification action hooks at `notifications.actions`.
  - Hooks can match notification type and severity.
  - Matching Settings-panel notification rows render buttons backed by existing
    built-in or `custom.*` action IDs.
  - `dismissOnRun` can dismiss the notification after the action succeeds.
- Added control-plane action registry discovery through `actions.list`.
  - The response includes built-in action metadata plus effective
    workspace-scoped `actions.custom` entries.
  - The CLI exposes this as `agentmux actions list`.
  - `agentmux actions list` uses `AGENTMUX_WORKSPACE_ID`/`CMUX_WORKSPACE_ID`
    when `--workspace` is not specified, so managed terminal agents can
    discover project-specific actions.
- Added control-plane action execution through `actions.run`.
  - The request accepts an `action_id`, optional `workspace_id`, and optional
    `pane_id`.
  - The CLI exposes this as `agentmux actions run <action-id>`.
  - `agentmux actions run` uses `AGENTMUX_WORKSPACE_ID`/`CMUX_WORKSPACE_ID`
    and `AGENTMUX_PANE_ID`/`CMUX_PANE_ID` when explicit options are not
    supplied, so managed terminal agents can execute discovered actions in
    their current workspace/pane context.
  - The desktop host executes control-safe actions by reusing existing control
    methods: workspace creation, WSL terminal launch, durable WSL/tmux agent
    launch, pane splitting, browser surface creation, and custom browser
    navigation/automation presets.
  - UI-only actions such as Settings, search, theme toggle, and command palette
    return an explicit unsupported-action error rather than pretending to run
    without a UI owner.
- Added `terminal.textBox` as a built-in UI-only action so the TextBox composer
  is discoverable through the command palette, shortcut settings, and
  `actions.list` metadata while still returning an explicit UI-only error from
  `actions.run`.

## Verification

Commands run:

```powershell
npm --prefix apps/desktop run build
```

```powershell
npm --prefix apps/desktop run test:ui
```

```powershell
npm run desktop:gates
```

```powershell
$toolchain = Join-Path (Get-Location) '.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin'
$env:PATH = "$toolchain;$env:PATH"
.\.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe test -p agentmux-ipc -p agentmux-desktop-host desktop_config_update_persists_appearance_settings -- --nocapture
```

```powershell
$toolchain = Join-Path (Get-Location) '.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin'
$env:PATH = "$toolchain;$env:PATH"
cargo test -p agentmux-desktop-host desktop_config_get_merges_project_shortcut_overrides -- --nocapture
cargo test -p agentmux-cli config_reload_parses_control_options -- --nocapture
cargo test -p agentmux-cli actions_list_parses_workspace_and_json_output -- --nocapture
```

```powershell
$toolchain = Join-Path (Get-Location) '.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin'
$env:PATH = "$toolchain;$env:PATH"
cargo test -p agentmux-desktop-host desktop_config_import_export_and_reset_round_trip -- --nocapture
```

```powershell
$toolchain = Join-Path (Get-Location) '.toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin'
$env:PATH = "$toolchain;$env:PATH"
cargo test -p agentmux-ipc parses_action_run_params -- --nocapture
cargo test -p agentmux-cli actions_run_parses_workspace_pane_and_json_output -- --nocapture
cargo test -p agentmux-desktop-host desktop_actions_run_executes_control_safe_actions -- --nocapture
```

```powershell
cd apps/desktop
.\node_modules\.bin\playwright.cmd test tests/ui/agentmux-design.spec.ts -g "custom browser config actions"
```

```powershell
C:\Users\irae\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe test -p agentmux-desktop-host desktop_actions_run_executes_control_safe_actions
C:\Users\irae\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe test -p agentmux-desktop-host browser_surface_and_commands_round_trip_through_desktop_control
```

Results:

- Desktop build passed.
- Playwright UI suite passed with 44 tests in the latest direct run.
- Latest desktop gate evidence:
  `docs/implementation/evidence/20260619-141416-IRAE-DESKTOP-desktop-ui-gates`.
- Rust config persistence test passed.
- UI test verifies `ctrl+shift+p` opens the command palette.
- UI test verifies palette arrow navigation changes the selected command and
  Enter executes the selected browser action.
- UI test verifies a config override can rebind `workspace.new` to the two-step chord `ctrl+b`, `c`.
- UI test verifies Settings can edit a shortcut, show duplicate binding
  conflicts, resolve the conflict, and execute the edited chord.
- UI test verifies a config-defined custom action appears in the command
  palette and executes through a two-step shortcut.
- UI test verifies config can rebind the workspace plus button to a WSL
  terminal action, rebind the surface-tab plus button to browser creation, and
  render a configured custom tab-bar action button.
- UI test verifies a notification hook can execute `browser.openNewTab` through
  the existing action registry.
- UI test verifies a config-defined custom browser action can navigate to a
  configured URL through a two-step shortcut.
- UI test verifies a config-defined custom browser action can run screenshot
  and frame-targeted fill automation recipes through two-step shortcuts.
- UI test verifies the built-in TextBox composer shortcut sends a draft through
  the active terminal `session.send-text` path.
- Rust desktop-host test verifies browser custom actions preserve frame targets
  through config import, `actions.list`, and control-safe `actions.run`.
- Rust test verifies shortcut bindings persist across desktop state reopen.
- Rust desktop-host test verifies project shortcut bindings override global
  shortcut bindings and that project custom actions, UI action settings, and
  notification action hooks are exposed for workspace-scoped config reads.
- Rust desktop-host test verifies `actions.list` returns built-in actions and
  project custom actions for a workspace-scoped request.
- Rust desktop-host test verifies config import/export/reset normalizes custom
  browser action presets.
- CLI parser test verifies `agentmux config reload --workspace <id> --json`
  preserves the workspace-scoped config request.
- CLI parser test verifies `agentmux actions list --workspace <id> --json`
  preserves the workspace-scoped action registry request.
- IPC parser test verifies `actions.run` request parameters.
- CLI parser test verifies `agentmux actions run <id> --workspace <id>
  --pane <id> --json`.
- Rust desktop-host test verifies `actions.run` can create a workspace, execute
  custom browser actions through the internal browser surface navigation and
  automation control methods, advertises `actions.run` through system
  capabilities, and
  rejects UI-only actions with `invalid_request`.

## Remaining Goal 11 Work

- No current Goal 11 implementation items remain. Future work may broaden
  `actions.run` to additional UI-mediated workflows if the desktop UI gains a
  safe request/ack bridge for visual actions.
