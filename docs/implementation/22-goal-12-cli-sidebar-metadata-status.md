# Goal 12 - CLI and Sidebar Metadata Compatibility Status

Status: Implemented first and cmux-alias slices
Date: 2026-06-19

## Source Baseline

This slice follows the cmux public API and notification docs reviewed for Windows parity:

- <https://cmux.com/ko/docs/api>
- <https://cmux.com/ko/docs/notifications>

The cmux API surface exposes script-friendly commands such as
`list-workspaces`, `new-workspace`, `current-workspace`, `new-split`, `send`,
`send-key`, `notify`, `set-status`, `clear-status`, `list-status`,
`set-progress`, `clear-progress`, `log`, `clear-log`, `list-log`,
`sidebar-state`, `ping`, `capabilities`, and `identify`. It also makes
workspace context available to child processes through environment variables.

## Implemented

- Added persistent sidebar metadata tables for workspace status entries, progress, and logs.
- Added control-plane methods for:
  - `system.ping`
  - `system.capabilities`
  - `system.identify`
  - `actions.list`
  - `notification.create`
  - `notification.clear`
  - `sidebar.set_status`
  - `sidebar.clear_status`
  - `sidebar.list_status`
  - `sidebar.set_progress`
  - `sidebar.clear_progress`
  - `sidebar.log`
  - `sidebar.clear_log`
  - `sidebar.list_log`
  - `sidebar.state`
- Added top-level CLI aliases matching the cmux-style workflow:
  - `ping`
  - `list-workspaces`
  - `new-workspace`
  - `current-workspace`
  - `close-workspace`
  - `list-surfaces`
  - `new-split`
  - `send`
  - `send-key`
  - `list-notifications`
  - `actions list`
  - `notify`
  - `clear-notifications`
  - `set-status`
  - `clear-status`
  - `list-status`
  - `set-progress`
  - `clear-progress`
  - `log`
  - `clear-log`
  - `list-log`
  - `sidebar-state`
  - `capabilities`
  - `identify`
- Added a `cmux` binary target that calls the same Windows named-pipe control
  client and presents `cmux`-named help/error output for script discovery.
- Added `--socket` as a cmux-compatible alias for the Windows control pipe
  option, and made the CLI fall back from `AGENTMUX_CONTROL_PIPE` to
  `CMUX_SOCKET_PATH` when choosing the pipe name.
- Injected managed terminal environment variables:
  - `AGENTMUX_CONTROL_PIPE`
  - `AGENTMUX_CONTROL_TOKEN`
  - `AGENTMUX_WORKSPACE_ID`
  - `AGENTMUX_SURFACE_ID`
  - `CMUX_SOCKET_PATH`
  - `CMUX_WORKSPACE_ID`
  - `CMUX_SURFACE_ID`
- Extended `WSLENV` so AgentMux/cmux workspace and surface identity variables
  cross into WSL-launched sessions.
- Added workspace-scoped action registry discovery for scripts and agents.
  `agentmux actions list` returns built-in action metadata plus effective
  project custom actions, and defaults to `AGENTMUX_WORKSPACE_ID` or
  `CMUX_WORKSPACE_ID` when no `--workspace` flag is supplied.
- Added config-driven notification action hooks at `notifications.actions`.
  Matching notifications can expose Settings-panel buttons that execute existing
  built-in or `custom.*` action-registry actions.
- Added a compact desktop sidebar metadata panel that renders status pills, progress, and recent logs for the active workspace.
- Extended the browser preview client so UI tests can seed synthetic sidebar state.

## Verification Added

- CLI parser coverage for the new cmux-style aliases.
- CLI parser coverage for `cmux` binary-facing help, `--socket`, default
  `new-workspace`, confirmation-free `close-workspace`, active-pane
  `new-split`, and active-session `send`/`send-key` compatibility commands.
- CLI parser coverage for `agentmux actions list --workspace <id> --json`.
- Store persistence coverage for sidebar metadata across reopen.
- Desktop host control-plane round-trip coverage for status/progress/log/sidebar state and workspace-scoped action registry discovery.
- Playwright coverage that seeds sidebar metadata through the preview API and verifies the rendered sidebar panel.
- Playwright coverage verifies a WSL setup notification hook executing
  `browser.openNewTab` through the action registry.

## Verification Notes

- `npm --prefix apps/desktop run build`, `npm --prefix apps/desktop run test:ui`, `npm run docs:check`, and `git diff --check` pass for this slice.
- Rust unit coverage for IPC, store, core library behavior, CLI parsers, and desktop-host metadata methods passes.
- Live ConPTY smoke checks pass after the hosted-terminal handle inheritance
  fix and environment-block propagation work.

## Remaining Gaps

- Full Unix-domain cmux socket protocol compatibility is not implemented;
  `cmux.exe` and `--socket` are compatibility aliases over AgentMux's JSON
  control envelope on the Windows named pipe.
- The sidebar panel is read-only. Workspace color, icon, description, and richer metadata editing remain future work.
- Host-side arbitrary notification custom command hooks are not implemented;
  current notification hooks intentionally execute only known action-registry
  actions.
- Browser/task/runtime commands beyond this metadata slice still need separate parity goals.

## Next Slice Candidates

1. Decide the trust model for host-side notification custom command hooks, or
   keep the action-registry-only hook model.
2. Add workspace metadata editing for color, icon, and description.
3. Add a real Unix-socket-compatible shim if non-CLI cmux integrations need to
   connect with raw socket clients instead of the Windows named pipe.
4. Expand browser/task/runtime commands in the dedicated browser parity goal.
