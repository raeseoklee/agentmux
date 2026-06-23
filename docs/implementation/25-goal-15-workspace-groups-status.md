# Goal 15 Workspace Groups Status

Status: Foundation, sidebar editing, multi-select, reorder controls, search, and restart smoke implemented
Date: 2026-06-19

This document records the current Goal 15 slice for workspace groups and
advanced sidebar UX.

## Implemented

- Added SQLite persistence for `workspace_groups` and
  `workspace_group_members`.
- Added migration version 7 for group schema creation.
- Workspace groups persist:
  - group ID
  - name
  - anchor workspace ID
  - collapsed state
  - pinned state
  - color
  - icon
  - sort order
  - membership order
- Deleting a workspace now removes its group membership and clears it as a
  group anchor without deleting the group itself.
- Added IPC request/result types for:
  - `workspace_group.list`
  - `workspace_group.create`
  - `workspace_group.update`
  - `workspace_group.delete`
  - `workspace_group.add_workspace`
  - `workspace_group.remove_workspace`
- Added desktop-host handlers, capability registration, and store-method
  authorization for the workspace group methods.
- Added CLI commands:
  - `agentmux workspace group list`
  - `agentmux workspace group create <name> --workspace <id>`
  - `agentmux workspace group update <group-id> [name]`
  - `agentmux workspace group update <group-id> --sort-order <n>`
  - `agentmux workspace group delete <group-id> --yes`
  - `agentmux workspace group add <group-id> <workspace-id>`
  - `agentmux workspace group remove <group-id> <workspace-id>`
  - `agentmux workspace-group ...` alias for the same command family.
- Added TypeScript control-client methods and preview-client behavior for the
  same group lifecycle.
- The desktop sidebar now renders persisted group headers, sorts pinned groups
  first, shows member counts, and supports collapsing/expanding group members.
- The desktop control hook now exposes group create, update, delete,
  add-workspace, remove-workspace, and create-workspace-in-group operations to
  the React shell.
- The sidebar now supports creating a group from the active workspace, editing
  group name/icon/color, pinning/unpinning, deleting a group without deleting
  its workspaces, adding the active workspace to an existing group, and creating
  a new workspace directly inside a group.
- Workspaces created from a group enter the same inline rename flow used by the
  normal workspace create action.
- Workspace cards now have sidebar selection checkboxes for multi-select
  grouping.
- The sidebar selection bar can create a new group from the selected
  workspaces, and existing group headers can add the selected workspaces to that
  group.
- Group headers now provide move-up/move-down controls that persist group
  `sort_order` through `workspace_group.update`.
- Workspace cards inside a group now provide move-up/move-down controls that
  persist membership `position` through `workspace_group.add_workspace`.
- Group headers also act as pointer drag handles for direct drag-and-drop group
  reordering.
- Workspace cards inside a group can be drag-and-dropped to reorder members
  directly.
- Group headers now expose a right-click context menu for the primary group
  actions: create workspace in group, add selected/current workspace, move,
  pin/unpin, edit, and delete.
- Workspace cards now expose a right-click context menu for rename and close.
- Closing a workspace from the UI now asks for confirmation, and the
  confirmation explicitly warns when that workspace is a group anchor and will
  clear the affected group anchors/membership.
- Store, IPC, and desktop-host tests now explicitly cover persisted group
  `sort_order` and member `position` behavior across reopen/control-plane
  round trips.
- The packaged diagnostics smoke now creates workspace groups, reorders group
  and member positions, restarts the packaged desktop host, and verifies the
  ordering survives restart.
- Workspace group placement is defined as sidebar organization only. It does
  not change top-tab ownership, split-pane membership, workspace creation
  semantics outside the explicit create-in-group command, or tab focus rules.
- The sidebar now has a workspace/group filter for large workspace sets. Group
  name matches keep the group visible, workspace matches narrow the visible
  members, no-match state is explicit, and clearing the filter restores the
  complete sidebar.

## Validation

The following checks passed on 2026-06-19:

```text
npm run check
npm --prefix apps/desktop run build
apps/desktop/node_modules/.bin/playwright.cmd test tests/ui/agentmux-design.spec.ts -g "workspace groups|agent launch|durable WSL-tmux|command palette opens"
apps/desktop/node_modules/.bin/playwright.cmd test tests/ui/agentmux-design.spec.ts -g "workspace groups|selected workspaces"
apps/desktop/node_modules/.bin/playwright.cmd test tests/ui/agentmux-design.spec.ts -g "reordered|selected workspaces|workspace groups"
apps/desktop/node_modules/.bin/playwright.cmd test tests/ui/agentmux-design.spec.ts -g "drag reordered|reordered from the sidebar"
apps/desktop/node_modules/.bin/playwright.cmd test tests/ui/agentmux-design.spec.ts -g "context menu|workspace groups|selected workspaces|reordered"
apps/desktop/node_modules/.bin/playwright.cmd test tests/ui/agentmux-design.spec.ts -g "context menu|group anchor"
apps/desktop/node_modules/.bin/playwright.cmd test tests/ui/agentmux-design.spec.ts -g "workspace sidebar filter"
apps/desktop/node_modules/.bin/playwright.cmd test tests/ui/agentmux-design.spec.ts -g "workspace groups|selected workspaces|workspace sidebar filter|reordered|drag reordered|context menu|group anchor"
powershell -NoProfile -ExecutionPolicy Bypass -File tools/run-packaged-diagnostics-smoke.ps1 -SkipBuild
```

The full check includes Rust formatting, clippy/check coverage, unit tests, doc
link validation, and the newly-added Goal 15 tests for store, IPC, CLI, and
desktop-host group behavior, including group ordering persistence after store
reopen. The targeted UI coverage verifies group creation,
rename/icon/color editing, collapse/expand behavior, group-specific workspace
creation, the temporary absence of the unstable titlebar agent launcher, and
the command-palette durable WSL-tmux launch path. The multi-select UI coverage
verifies grouping selected workspaces into a new group and adding selected
workspaces to an existing group. The reorder coverage verifies moving groups in
the sidebar and moving workspace members inside a group with both explicit move
controls and pointer drag-and-drop. The context-menu coverage verifies
right-click access to group creation and movement actions, and verifies the
anchor-aware workspace close confirmation. The packaged restart smoke evidence
is archived at
`docs/implementation/evidence/20260619-201829-IRAE-DESKTOP-packaged-diagnostics-smoke`
and records `workspace_group_restart_smoke = "passed"` with the expected group
and member ordering after desktop-host restart.

## Summary

Goal 15 now has the durable data model, IPC surface, CLI surface, preview
client behavior, sidebar rendering, direct sidebar editing controls, group and
member reordering, search/filter UX, and packaged restart evidence for
persisted ordering.
