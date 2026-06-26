# Overall Completion Goal Groups

Status: In progress; core implementation groups are green, release/manual gates remain
Date: 2026-06-19

This document groups the remaining work for the active goal:

`AgentMux Windows cmux parity overall completion`

The target product shape is a Windows-only AgentMux experience inspired by the
cmux getting started flow, with WSL and tmux used as the durable Linux execution
base when agent sessions need panes, restore, and process continuity.

Current verification:

- `npm run check` passed on 2026-06-19 after the config import/export/reset,
  custom browser action preset, cmux CLI alias, tmux-compat foundation, and
  persistent integration shim installer, tmux command expansion, and OMO package
  automation plus agent-team metadata, WSL-aware wrapper launch, WSL doctor
  validation, WSL-side OMO package installation, and explicit user PATH
  registration plus OMO shadow config diagnostics, JSONC-preserving merge, and
  agent-team lifecycle transition, OMO symlink fallback reporting, and
  tmux session command/default-shell plus all-workspace list coverage and
  cmux project-config fallback, tmux target-grammar, and control-plane action
  execution plus explicit cmux project-config migration and config diagnostics
  plus config schema export slices.
- `cargo test -p agentmux-cli` passed on 2026-06-19 after adding the CLI
  browser automation command family and SSH launch command.
- Targeted browser wait-for-selector, navigation-control, DOM action,
  frame-targeted DOM snapshot/get/find/wait plus selector mutation commands,
  DOM get/find/highlight, focus, zoom, frames, storage, cookies, downloads,
  history, console, dialogs, and errors checks passed on 2026-06-19:
  `cargo test -p agentmux-browser`,
  `cargo test -p agentmux-ipc parses_browser_surface_and_command_params`,
  `cargo test -p agentmux-cli browser_cli_options_parse_control_shapes`,
  `cargo test -p agentmux-desktop-host browser_surface_and_commands_round_trip_through_desktop_control`,
  `cargo test -p agentmux-desktop-host desktop_actions_run_executes_control_safe_actions`,
  `npm --prefix apps/desktop run build`, and
  `apps/desktop/node_modules/.bin/playwright.cmd test -g "custom browser config actions can run automation recipes"`.
- TextBox composer foundation checks passed on 2026-06-19:
  `npm --prefix apps/desktop run build`,
  `cargo test -p agentmux-desktop-host desktop_actions_run_executes_control_safe_actions`,
  `apps/desktop/node_modules/.bin/playwright.cmd test -g "TextBox composer sends a draft"`,
  and `apps/desktop/node_modules/.bin/playwright.cmd test -g "TextBox"`.
- Dock foundation and execution checks passed on 2026-06-19:
  `npm --prefix apps/desktop run build`,
  `cargo test -p agentmux-ipc parses_dock_get_params`,
  `cargo test -p agentmux-desktop-host desktop_dock_get_reads_project_and_global_configs`,
  `cargo test -p agentmux-desktop-host persisted_terminal_surface_uses_env_title`,
  `cargo test -p agentmux-desktop-host managed_wsl_env_value_includes_user_env_keys`,
  `cargo test -p agentmux-desktop-host persisted_terminal_surface_supports_dock_terminal_type`,
  `cargo test -p agentmux-desktop-host workspace_bundle_dock_spawn_does_not_create_a_top_tab_pane`,
  and `apps/desktop/node_modules/.bin/playwright.cmd test -g "Dock"`.
- Agent-facing skill packaging check passed on 2026-06-19:
  `python C:\Users\irae\.codex\skills\.system\skill-creator\scripts\quick_validate.py skills\agentmux-control`.
- AgentMux skill install script check passed on 2026-06-19 by installing
  `skills/agentmux-control` into a temp destination with
  `tools/install-agentmux-skill.ps1 -DestinationRoot <temp>`.
- `npm --prefix apps/desktop run build` passed on 2026-06-19 after the Goal 15
  workspace group foundation slice.
- `npm --prefix apps/desktop run build` and targeted Playwright setup coverage
  passed on 2026-06-19 after adding the Windows setup panel:
  `npx playwright test tests/ui/agentmux-design.spec.ts -g "setup wizard|shows WSL install guidance"`.
- `npm --prefix apps/desktop run build` and targeted Playwright UI coverage
  passed on 2026-06-19 after the Goal 15 sidebar group editing slice.
- `npm run check` passed on 2026-06-19 after the Goal 15 workspace group
  foundation slice, including the new store, IPC, CLI, and desktop-host tests.
- Desktop build and Playwright UI suite passed after the same slices.
- Latest direct Playwright UI suite passed with 47 tests after the Dock
  execution, skill packaging, and TextBox draft persistence slices.
- Desktop build and UI smoke evidence:
  `docs/implementation/evidence/20260619-154446-IRAE-DESKTOP-desktop-ui-gates`.
- Packaged diagnostics and workspace-group restart smoke evidence:
  `docs/implementation/evidence/20260619-201829-IRAE-DESKTOP-packaged-diagnostics-smoke`.
- Browser CDP fixture evidence:
  `docs/implementation/evidence/20260619-131503-IRAE-DESKTOP-browser-cdp-smoke`.
- Real WSL/tmux round-trip and reattach evidence:
  `docs/implementation/evidence/20260619-131302-IRAE-DESKTOP-real-tmux-reattach-smoke`.
- Installer artifact build evidence:
  `docs/implementation/evidence/20260619-202610-IRAE-DESKTOP-installer-build-smoke`.
- Installer contents gate evidence:
  `docs/implementation/evidence/20260619-202725-IRAE-DESKTOP-installer-contents-gate`.
- Installer lifecycle installed gate evidence:
  `docs/implementation/evidence/20260619-202735-IRAE-DESKTOP-installer-lifecycle-gate`.
- Installed app smoke evidence:
  `docs/implementation/evidence/20260619-195449-IRAE-DESKTOP-installed-app-smoke`.
- Integration live smoke evidence:
  `docs/implementation/evidence/20260619-195044-IRAE-DESKTOP-integration-live-smoke`.
- Release readiness audit evidence:
  `docs/implementation/evidence/20260619-202822-IRAE-DESKTOP-release-readiness-audit`.
- WSL state gate evidence:
  `docs/implementation/evidence/20260619-195904-IRAE-DESKTOP-wsl-state-gate`.
- Server mode web terminal checks passed on 2026-06-19:
  `cargo check -p agentmux-cli`,
  `cargo test -p agentmux-cli server_`,
  `cargo build -p agentmux-cli`, and
  `powershell -NoProfile -ExecutionPolicy Bypass -File tools/run-server-mode-smoke.ps1 -SkipBuild -Port 18766`.
  Codexus evidence:
  `verification_20260619_153908_2a2763`,
  `verification_20260619_154010_9f181a`,
  `verification_20260619_154025_4d5a81`, and
  `verification_20260619_154024_ccb403`.

## G1 - Tab, Pane, and Surface Lifecycle

Goal: make top tabs own their visible pane layout, and make tab close tear down
the panes and runtime surfaces that belong to that tab.

Group status: implemented; only optional tab metadata polish remains.

Current slice:

- New terminal tabs no longer auto-create split panes.
- `session.spawn` accepts optional placement metadata so UI calls can choose
  `new_tab` or `active_pane`.
- New top tabs create independent root panes, while empty-pane terminal launch
  stays inside the current tab.
- The top tab strip renders one representative surface per root pane tree.
- A new `surface.close` control-plane method removes a surface and its session
  metadata.
- Closing a top tab removes that tab's root pane subtree and every mounted
  surface/session in that subtree.
- Browser preview and desktop host both support closing surface tabs.
- The previously unstable titlebar agent launch button is removed pending a
  cleaner reimplementation; agent execution remains available through the
  action registry and command palette.
- UI coverage verifies that adding a WSL terminal creates a separate top tab
  without changing the split layout, and that closing that tab returns to one
  mounted pane.
- UI coverage also verifies that split panes stay scoped to their top tab.

Remaining:

- Add explicit persisted tab metadata if surface/root inference becomes too
  limiting for labels, icons, browser tabs, or reordered tabs.
- Add persisted tab ordering if users need drag/reorder semantics.

## G2 - WSL and tmux Agent Execution

Goal: launch agents through WSL tmux panes with deterministic diagnostics.

Group status: implemented for the Windows-only WSL/tmux baseline.

Current slice:

- Removed the unstable titlebar agent launch button pending a cleaner
  reimplementation.
- Agent launch now creates a separate top tab without mutating the current
  split-pane layout when triggered through the action registry or command
  palette.
- WSL is treated as required for durable agent execution on Windows.
- The UI shows WSL installation guidance when no distribution is available.
- The UI shows tmux installation guidance when the selected WSL distribution
  lacks tmux.
- Existing agent launch paths remain covered through the control/action layers;
  titlebar-specific UI coverage should return when the button is rebuilt.
- Added a diagnostics settings tab that runs a WSL tmux probe from the UI.
- Fixed the live ConPTY output-capture issue by preventing redirected parent
  stdio handles from being inherited by hosted terminal processes.
- Verified WSL direct and tmux-control smoke tests against the local Ubuntu WSL
  distribution with tmux installed.

Remaining:

- Consider adding a guarded full round-trip tmux smoke action to diagnostics;
  for now the round-trip check is covered by automated smoke tests.

## G3 - Project, Config, and Dock Parity

Goal: make setup and project configuration feel complete for repeated use.

Group status: baseline implemented; broader config parity remains product polish.

Current slice:

- Added persistent workspace metadata for description, icon, color, default WSL
  distribution, and default agent command.
- Added `workspace.update` to the desktop control plane while keeping
  `workspace.rename` intact for narrow rename flows.
- Added a project settings tab that edits workspace name, project root,
  description, icon, color, default WSL distribution, and default agent command.
- Workspace cards render configured icon, color, and description.
- New terminal and agent launch paths now prefer the active workspace project
  root and default WSL distribution.
- Workspace default agent commands remain configurable for the rebuilt launcher
  and action-registry paths.
- The command palette now distinguishes creating a new WSL terminal tab from
  opening a WSL terminal in the active pane.
- App config now supports explicit reload through the control plane, CLI, and
  Settings UI without restarting the app.
- App config now supports control-plane export/import/reset and Settings >
  General buttons for JSON export, pasted JSON import, and global reset.
- Project config now supports Settings > General Export project, Import
  project, and Reset project controls when the active workspace has a project
  config path.
- Shortcut settings now support user rebinding, two-step chord entry, clearing,
  and duplicate binding diagnostics.
- Workspace-scoped `config.get`, `config.reload`, and `config.update` now merge
  `<projectRoot>/.agentmux/agentmux.json` shortcut bindings over global app
  config, and `agentmux config get/reload --workspace <id>` exposes the same
  effective config.
- `actions.custom` now lets global or project config add command-palette
  actions in the `custom.*` namespace. The execution policy supports durable
  WSL/tmux agent commands, WSL terminal actions, and browser actions that open
  a browser tab, navigate to a configured URL, or run safe browser automation
  recipes through the control-plane browser API. Browser automation recipes can
  include `frame:<frame-id>`-style tokens and keep those frame targets through
  config normalization, shortcut execution, and `actions.run`.
- App and project config now support `ui.workspace_plus_action`,
  `ui.surface_tab_plus_action`, and `ui.surface_tab_actions`.
- The sidebar workspace plus button, the surface-tab plus button, and the
  right-side surface-tab action buttons now execute through the same action
  registry used by shortcuts and the command palette.
- App and project config now support `notifications.actions` hooks that match
  notification type/severity and render action-registry buttons in Settings.
- Notification action hooks execute existing built-in or `custom.*` UI actions;
  `dismissOnRun` can close the notification after a successful action.
- `actions.list` now exposes built-in action metadata plus effective
  workspace-scoped custom actions through the control plane and
  `agentmux actions list`.
- Managed terminal agents can call `agentmux actions list` without specifying a
  workspace because the CLI falls back to `AGENTMUX_WORKSPACE_ID` or
  `CMUX_WORKSPACE_ID`.
- `actions.run` now lets CLI users and managed terminal agents execute
  control-safe built-in or `custom.*` actions through the desktop host. The CLI
  exposes this as `agentmux actions run <action-id>`, falling back to
  AgentMux/cmux workspace and pane environment variables when explicit
  `--workspace` or `--pane` options are absent.
- A `cmux` CLI binary target now exposes the same Windows named-pipe control
  client with cmux-style top-level aliases for core script workflows including
  `list-workspaces`, `new-workspace`, `current-workspace`, `new-split`, `send`,
  `send-key`, `notify`, sidebar metadata, `ping`, `capabilities`, and
  `identify`.
- The CLI accepts `--socket` and `CMUX_SOCKET_PATH` as compatibility aliases
  for the Windows control pipe path.
- Managed terminal sessions now receive `AGENTMUX_PANE_ID`, `CMUX_PANE_ID`,
  `TMUX`, and `TMUX_PANE`; these variables also cross into WSL through
  `WSLENV`.
- `agentmux __tmux-compat` translates a growing tmux-shaped command subset:
  `display-message`, `capture-pane`, `kill-pane`, `kill-window`,
  `list-panes`, `list-sessions`, `list-windows`, `has-session`,
  `new-session`, `new-window`, `rename-session`, `rename-window`,
  `select-pane`, `select-window`, `send-keys`, `split-window`, and
  `switch-client`.
- `list-panes -a` and `list-windows -a` can enumerate all AgentMux workspaces,
  and pane/window format rendering now includes session/window/pane index
  fields used by common tmux scripts.
- tmux-compatible pane targets now cover direct fake pane IDs, current
  pane/window markers, active-window pane indexes, `window.pane`,
  `:window.pane`, `session:window`, and `session:window.pane` forms.
- tmux-compatible `split-window`, `new-window`, and `new-session` no longer
  require an explicit command; they launch a backend-appropriate default shell
  when the caller omits one.
- `cmux claude-teams`, `cmux omo`, `cmux omx`, and `cmux omc` now prepare
  wrapper-specific tmux shim directories and launch their underlying agent
  commands with shim-first `PATH`.
- `cmux integrations setup/env <kind>` can prepare and inspect those wrapper
  environments without launching the agent, and `omo` creates a non-mutating
  shadow OpenCode config with `oh-my-opencode` registered and tmux enabled.
- `omc` setup writes a Node restore module and injects it through
  `NODE_OPTIONS` while preserving the user's original `NODE_OPTIONS` value.
- `cmux integrations install-shims` now writes persistent `claude-teams`,
  `omo`, `omx`, and `omc` entrypoints plus PowerShell and POSIX shell PATH
  snippets, with optional idempotent profile updates.
- `cmux integrations install-shims --user-path` can explicitly add the
  AgentMux-managed integration bin directory to the Windows user PATH through
  HKCU environment registration.
- `cmux integrations doctor [kind]` now reports wrapper, tmux shim, PATH,
  shadow config, restore-module, and underlying executable readiness without
  changing user files.
- `cmux integrations doctor [kind] --distribution <name>` checks the selected
  WSL execution context, including distribution reachability, WSL-side agent
  executable resolution, and WSL-visible shim/config files.
- `npm run integration:live-smoke` now prepares integration wrappers and tmux
  shims inside an isolated evidence runtime directory, temporarily prepends
  only that bin directory for doctor subprocesses, and verifies both Windows
  and Ubuntu WSL doctor foundation checks without mutating user PATH/profile
  files.
- `cmux integrations doctor omo` now checks OMO shadow config content, including
  `oh-my-opencode` plugin registration and `tmux.enabled=true`.
- OMO shadow config generation now preserves common JSONC comments/formatting
  while adding `oh-my-opencode` and enabling `tmux.enabled`.
- `cmux omo` and `cmux integrations setup omo --install-packages` now install
  `oh-my-opencode` inside the shadow OpenCode config with `bun` or `npm`, while
  keeping the user's original OpenCode config untouched; with
  `--distribution <name>` or AgentMux/cmux WSL distribution environment,
  installation runs inside the selected WSL distribution.
- OMO package setup reports whether shadow `node_modules` was isolated or a
  symlink was replaced, and doctor exposes `omo-node-modules-isolated` so users
  can repair accidental links back to their original OpenCode packages.
- Integration wrappers now export their integration identity, and tmux-created
  worker sessions are marked through agent state plus sidebar status/log
  metadata.
- Agent-team worker sessions now follow their terminal lifecycle: clean exits
  become `completed`, non-zero exits and backend failures become `failed`, and
  telemetry is preserved for UI/sidebar persistence.
- Integration wrappers launched from WSL or an explicit AgentMux/cmux WSL
  distribution override now execute the underlying agent inside that WSL
  distribution, while routing tmux shim callbacks back through the preparing
  Windows `cmux.exe`.
- Project config loading can now read compatible `.cmux/cmux.json` fields when
  the AgentMux-owned `.agentmux/agentmux.json` file is absent. The write/export
  target remains `.agentmux/agentmux.json`, and that file takes precedence when
  both exist.
- `config.migrate_project`, `agentmux config migrate-cmux`, and Settings >
  General `Migrate .cmux` can copy compatible `.cmux/cmux.json` project fields
  into `.agentmux/agentmux.json`, refusing accidental overwrites unless an
  explicit overwrite request is made.
- `config.diagnostics`, `agentmux config diagnostics`, and Settings > General
  diagnostics rows now report global, AgentMux project, and cmux project config
  existence, validity, active-source status, paths, and messages even when a
  broken config file would make normal `config.get` fail.
- `docs/schemas/agentmux.config.schema.json` publishes a JSON Schema for
  AgentMux global and project config files, and `agentmux config schema`
  exports the same schema to stdout or `--output <path>` without requiring a
  running desktop host.
- The Windows setup panel can be opened from the WSL/tmux warning banner or
  command palette. It checks WSL distribution availability, displays
  `wsl --install` guidance when needed, probes tmux for the selected
  distribution, shows the tmux install command, and saves workspace project
  root plus default WSL distribution.
- Host-side notification hooks remain intentionally limited to action-registry
  action IDs for the Windows v1 scope; arbitrary notification shell commands
  stay out of scope for security and predictability.

Remaining:

- Expand editable config beyond appearance/shortcuts/workspace defaults if a
  full `config.json` parity view is needed.

## G4 - Browser and Remote Workflows

Goal: make browser surfaces and remote profiles usable as first-class workflow
tabs.

Group status: browser CLI automation and SSH CLI launch baselines implemented;
remote SSH workflow depth remains product polish.

Current slice:

- Browser surfaces support both explicit new top-tab placement and active-pane
  placement through `surface.create_browser`.
- The command palette exposes separate browser commands for new top tabs and
  active-pane mounting.
- Config-defined custom browser actions can open a browser tab or navigate a
  configured URL using normalized command presets such as
  `["new-tab", "https://example.com"]` and
  `["active-pane", "https://example.com"]`.
- Config-defined custom browser actions can also run browser automation
  recipes through the command palette, shortcuts, notification hooks, and
  `actions.run`: screenshot, DOM snapshot, evaluate, click, type, fill, press,
  select, scroll, hover, check, highlight, focus, zoom, wait-for-selector
  commands plus reload/back/forward/current-url navigation controls execute
  through the existing browser control-plane API.
- Config-defined browser automation recipes accept explicit frame target tokens
  such as `frame:<frame-id>` and preserve normalized trailing frame slots, so
  iframe-aware recipes can be saved, reloaded, listed, and executed without
  losing their target frame.
- `agentmux browser` and `cmux browser` now expose browser surface automation
  through the control pipe: `open`, `navigate`, `reload`, `back`, `forward`,
  `current-url`, `screenshot`, `dom-snapshot`, `frames`, `storage`,
  `cookies`, `downloads`, `history`, `console`, `dialogs`, `errors`, `click`,
  `type`, `fill`, `press`, `select`, `scroll`, `hover`, `check`, `get`,
  `find`, `highlight`, `focus`, `zoom`, `wait-for-selector`, `evaluate`, and
  `diagnostics`.
- Browser frame-tree, local/session storage snapshot, cookie listing, and
  navigation history commands are available through the core automation layer,
  desktop control host, IPC result contracts, and CLI aliases.
- Browser downloads use a per-surface download directory when supported by the
  CDP browser and expose completed or in-progress files through
  `browser.downloads` and `agentmux browser downloads`.
- `browser.dom_snapshot`, `browser.click`, `browser.type`, `browser.fill`,
  `browser.press`, `browser.select`, `browser.scroll`, `browser.hover`,
  `browser.check`, `browser.get`, `browser.find`, `browser.highlight`,
  `browser.focus`, `browser.wait_for_selector`, and `browser.evaluate` accept
  an optional `frame_id`/`--frame` target. CDP execution creates an isolated
  world for the requested frame before running selector-based DOM inspection,
  mutation, waits, or scripts, so `browser.frames` output can be used directly
  for frame-scoped automation.
- `browser.console` and `agentmux browser console` expose recent injected
  console messages, including `error` level entries, with a bounded `--limit`
  for diagnostic inspection.
- `browser.dialogs` and `agentmux browser dialogs` expose recorded
  alert/confirm/prompt calls. The injected dialog recorder returns safe
  automation defaults so scripted flows do not block on modal browser dialogs.
- `browser.errors` and `agentmux browser errors` expose recorded `error` and
  `unhandledrejection` events with source, location, stack, and message fields.
- Browser CLI commands use compact text by default and support `--json` for
  response-envelope output, matching the rest of the control-plane CLI.
- Browser new-tab placement is covered in desktop-host and Playwright tests.
- Managed terminal sessions receive `AGENTMUX_SURFACE_ID` and
  `CMUX_SURFACE_ID` in addition to workspace/control environment variables.
- Managed terminal sessions also extend `WSLENV` so AgentMux/cmux identity
  variables cross into WSL launches.
- SSH profile metadata can be edited from the settings UI, not only created or
  deleted.
- `agentmux ssh` and `cmux ssh` can open direct `user@host[:port]` targets or
  saved SSH profile name/id targets through the desktop control pipe. The
  command defaults to a new top tab and can target the active pane with
  `--active-pane`.
- Raw cmux socket compatibility is deferred for Windows v1. Managed sessions
  and WSL launches receive AgentMux/cmux identity variables plus a `cmux.exe`
  named-pipe client, which is the supported compatibility layer unless a real
  raw-socket client requirement appears.

Remaining:

- Add richer SSH launch feedback once the SSH transport backend leaves the
  current profile/command plumbing stage.
- Expand SSH parity beyond direct interactive shell launch: deeplinks, remote
  command execution, remote browser localhost routing, upload/reconnect policy,
  and relay/notification plumbing.
- Add visible browser/CDP target unification and browser UI parity once the
  automation command baseline is considered stable.

## G5 - Workspace Groups and Sidebar Organization

Goal: group related workspaces without changing the tab/pane ownership model.

Group status: persistence, IPC, CLI, preview behavior, sidebar rendering,
direct sidebar editing controls, multi-select grouping, button-based plus
pointer drag reordering, and search/filter organization are implemented.

Current slice:

- Workspace groups now have SQLite persistence for group metadata and ordered
  membership.
- Closing a workspace removes its group membership and clears it as an anchor
  while leaving the group itself intact.
- The control plane exposes `workspace_group.list/create/update/delete`,
  `workspace_group.add_workspace`, and `workspace_group.remove_workspace`.
- The CLI exposes `agentmux workspace group ...` and `agentmux workspace-group
  ...` command families.
- The React control client and browser preview client support the same group
  lifecycle.
- The sidebar renders group headers, member counts, pinned-first ordering, and
  collapsed/expanded group members.
- The React control hook exposes group create/update/delete, membership changes,
  and create-workspace-in-group helpers.
- The sidebar can create a group from the active workspace, edit group
  name/icon/color, pin or unpin a group, delete a group without deleting member
  workspaces, add the active workspace to an existing group, and create a new
  workspace directly inside a group.
- Group-created workspaces enter the existing inline rename flow.
- Workspace cards now expose selection checkboxes, and the sidebar selection
  bar can create a new group from selected workspaces.
- Existing group headers can add selected workspaces to that group.
- Group headers expose move-up/move-down controls that persist group
  `sort_order` through the control plane.
- Group member workspace cards expose move-up/move-down controls that persist
  membership `position`.
- Group headers and group member workspace cards also support direct pointer
  drag-and-drop reordering.
- Group headers expose a right-click context menu for primary group actions:
  create workspace in group, add selected/current workspace, move, pin/unpin,
  edit, and delete.
- Workspace cards expose a right-click context menu for rename and close, with
  anchor-aware confirmation when closing a workspace that anchors groups.
- Store, IPC, CLI, and desktop-host tests cover persisted group ordering and
  membership order across reopen/control-plane round trips.
- Targeted Playwright coverage verifies group creation, editing,
  collapse/expand, group-specific workspace creation, the temporary absence of
  the unstable titlebar agent launcher, and command-palette durable WSL-tmux
  launching.
- Additional Playwright coverage verifies grouping selected workspaces into a
  new group and adding selected workspaces to an existing group.
- Reorder coverage verifies moving groups in the sidebar and moving workspace
  members inside a group with both explicit move controls and pointer
  drag-and-drop.
- Context-menu coverage verifies right-click access to group creation and
  movement actions.
- Workspace context-menu coverage verifies the anchor-aware close warning.
- Packaged diagnostics smoke verifies workspace group `sort_order` and ordered
  membership survive desktop-host restart.
- Workspace group placement is sidebar-only for the current product shape; it
  does not change top-tab ownership, split-pane membership, or tab focus rules.
- The sidebar now has a workspace/group filter that narrows visible groups,
  ungrouped workspaces, and grouped members while preserving the underlying
  group/collapse state.

## G6 - Release Stability

Goal: reach a build that can be installed, smoke-tested, and debugged on a clean
Windows machine.

Group status: automated gates are current; final release still needs manual
clean-machine validation.

Current slice:

- The release-candidate checklist now has explicit Windows-only WSL states:
  missing WSL, WSL present without tmux, and WSL present with tmux.
- The current verification set includes desktop build, Playwright UI smoke,
  docs link checks, desktop-host/core/CLI/store/IPC unit tests, ConPTY smoke,
  WSL direct smoke, and real WSL tmux round-trip/reattach smoke.
- `git diff --check` is part of the working verification set.
- Packaged diagnostics and workspace-group restart smoke passed with evidence
  at
  `docs/implementation/evidence/20260619-201829-IRAE-DESKTOP-packaged-diagnostics-smoke`.
- Browser CDP fixture smoke passed with evidence at
  `docs/implementation/evidence/20260619-131503-IRAE-DESKTOP-browser-cdp-smoke`.
- Desktop build and UI smoke passed with evidence at
  `docs/implementation/evidence/20260619-154446-IRAE-DESKTOP-desktop-ui-gates`.
- Real WSL/tmux reattach smoke passed with evidence at
  `docs/implementation/evidence/20260619-131302-IRAE-DESKTOP-real-tmux-reattach-smoke`.
- Installer artifact build smoke passed with evidence at
  `docs/implementation/evidence/20260619-202610-IRAE-DESKTOP-installer-build-smoke`.
  It now prepares and records Tauri sidecar inputs for installed
  `agentmux.exe` and `cmux.exe` CLI binaries before building the NSIS setup
  artifact, and compiles the install/uninstall PATH hook.
- `npm run installer:contents-gate` passed with evidence at
  `docs/implementation/evidence/20260619-202725-IRAE-DESKTOP-installer-contents-gate`.
  It opens the generated NSIS setup without installing it, confirms the
  generated installer script installs `agentmux.exe` and `cmux.exe`, extracts
  both sidecars, and verifies their hashes match the prepared Tauri sidecar
  binaries. It also verifies the generated installer includes the AgentMux NSIS
  hook file and calls the install/uninstall PATH hook.
- `npm run installer:lifecycle-gate -- installed` passed with evidence at
  `docs/implementation/evidence/20260619-202735-IRAE-DESKTOP-installer-lifecycle-gate`.
  It non-mutatingly verifies the current machine has the generated installer
  artifact, an AgentMux registry uninstall entry, installed executable,
  uninstall command, and Start Menu shortcut, and records installed CLI sidecar
  plus user PATH state without requiring those checks. The current installed
  app predates the sidecar/PATH-capable installer, so final signoff must
  reinstall the latest artifact and rerun with `-RequireCli -RequireUserPath`.
- `npm run installed:app-smoke` passed with evidence at
  `docs/implementation/evidence/20260619-195449-IRAE-DESKTOP-installed-app-smoke`.
  It launches the installed AgentMux executable using isolated runtime paths
  and verifies diagnostics export, workspace creation, native ConPTY spawn, and
  terminal output capture.
- `npm run release:readiness-audit` now captures a non-mutating release audit
  covering installer artifact presence, Windows install detection, WSL/tmux
  matrix observation for the current machine, installed CLI sidecar detection,
  and cmux integration doctor results. Latest evidence:
  `docs/implementation/evidence/20260619-202822-IRAE-DESKTOP-release-readiness-audit`.
  The latest audit intentionally reports needs-attention because the currently
  installed app predates the sidecar-capable installer and lacks installed
  `agentmux.exe`, `cmux.exe`, and installed-directory user PATH registration.
- `npm run wsl:state-gate -- wsl_with_tmux` now captures the current machine's
  explicit WSL matrix state, including reachable distributions, selected
  distribution, and tmux version. Latest evidence:
  `docs/implementation/evidence/20260619-195904-IRAE-DESKTOP-wsl-state-gate`.
- `npm run integration:live-smoke` passed with evidence at
  `docs/implementation/evidence/20260619-195044-IRAE-DESKTOP-integration-live-smoke`.
  The isolated wrapper/shim foundation is ready on Windows and Ubuntu WSL; the
  remaining installed-agent gap is `opencode` missing from Windows and WSL PATH.

Remaining:

- Keep `npm --prefix apps/desktop run build` green.
- Keep Playwright coverage green, with temporary skips removed as their features
  return.
- Keep IPC/store/core/desktop-host unit tests green.
- Keep diagnostics export useful for runtime, browser automation, notifications,
  and queue pressure.
- Run clean-machine `preinstall`, CLI-inclusive `installed`, and
  post-uninstall lifecycle gates for the final release candidate artifact; the
  current machine's desktop-only `installed` phase, installed-app launch, and
  terminal smoke now have automated evidence.
- Run clean-machine WSL-missing and WSL-without-tmux passes with
  `npm run wsl:state-gate -- wsl_exe_missing`,
  `npm run wsl:state-gate -- no_wsl_distribution`, and
  `npm run wsl:state-gate -- wsl_without_tmux`; the current machine has passed
  only the local WSL-with-tmux state.

## G7 - Dock, TextBox, and Agent-Facing Skills

Goal: add the cmux-style prompt/control surfaces that sit beside normal
terminal panes without weakening the Windows-only WSL/tmux safety model.

Group status: TextBox, embedded Dock execution/lifecycle foundation, first
agent-facing skill packaging, and skill install automation are implemented;
optional installer integration and deeper Dock polish remain.

Current slice:

- `terminal.textBox` is a built-in UI action with action-registry metadata and
  a default `Ctrl+Alt+I` shortcut.
- The React shell renders a bottom TextBox composer for the active terminal
  pane.
- Browser panes and empty panes keep the TextBox action disabled.
- Submitted drafts use the existing `session.send-text` path, so the feature
  works with the same terminal backends as direct xterm input.
- TextBox drafts persist per active terminal session in local storage until the
  draft is sent.
- Global and project config can set `ui.text_box_max_lines` from 2 to 12; the
  React composer applies the setting to its visible/max resize height, and the
  JSON Schema documents the field.
- Targeted Playwright coverage verifies creating a WSL terminal tab, opening
  TextBox through the shortcut, restoring an unsent draft, applying configured
  max-line height, sending a draft, and observing terminal output.
- `dock.get` reads AgentMux-first and cmux-compatible Dock config candidates:
  project `.agentmux/dock.json`, project `.cmux/dock.json`, global AgentMux
  `dock.json`, then global cmux `dock.json`.
- The React shell renders loaded Dock controls in a right-side panel with source
  and review-required metadata.
- Rust and Playwright coverage verify project/global Dock precedence and
  right-panel rendering.
- Project-sourced Dock controls require a backend-auditable trust approval
  persisted in the desktop store by workspace, source, config path, and config
  content hash before execution.
- Trusted Dock controls launch as Dock-owned WSL terminal slots with command,
  cwd, env, surface-title, and control-id metadata. Dock terminals render inside
  the right panel instead of creating top-level tabs, and expose restart/close
  controls.
- Dock terminal slots expose a per-control height slider and persist user
  height overrides by workspace, Dock source/path, and control id.
- Added the repo-packaged `agentmux-control` Codex skill with concise operating
  instructions, UI metadata, and a control workflow reference for CLI, browser,
  diagnostics, integrations, and Dock tasks.
- Added `tools/install-agentmux-skill.ps1` and `npm run skills:install` to copy
  repo-packaged skills into Codex's skill directory or a caller-provided
  destination.

Remaining:

- Include TextBox draft state in a future session restore snapshot.
- Decide paste semantics for agent prompts versus shell commands before adding
  richer multiline handling.
- Consider wiring skill installation into the packaged installer only if
  AgentMux should automatically modify Codex's local skill directory; split out
  specialized skills only if usage shows the combined control workflow is too
  broad.

## Current Priority

1. Run the clean-machine installer lifecycle phases not covered by the current
   machine: `npm run installer:lifecycle-gate -- preinstall` before install,
   `npm run installer:lifecycle-gate -- installed -RequireCli -RequireUserPath`
   after installing the latest sidecar/PATH-capable artifact, and
   `npm run installer:lifecycle-gate -- uninstalled` after uninstall.
2. Run the clean-machine Windows-only WSL state matrix gates not covered by the
   current machine: `wsl_exe_missing`, `no_wsl_distribution`, and
   `wsl_without_tmux`.
3. Continue Goal 14 beyond the WSL-aware wrapper and doctor foundation: run
   `npm run integration:live-smoke -- -RequireUnderlyingAgents` after
   installing OpenCode/oh-my-opencode, and continue broadening tmux
   target/format coverage for agent integrations.
4. Triage optional polish after release gates: persisted tab metadata/reorder,
   advanced browser automation subcommands, and deeper remote SSH workflows.
