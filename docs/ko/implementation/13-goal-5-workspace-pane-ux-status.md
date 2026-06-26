# Goal 5 Workspace and Pane UX Status

Status: Draft
Date: 2026-06-18

This document records the current implementation evidence for Goal 5: Workspace and Pane UX.

## Implemented

- The IPC contract now has typed `PaneSplitParams`, `PaneFocusParams`, `PaneCloseParams`, `PaneResizeLayoutParams`, `PaneMountSurfaceParams`, and `PaneUnmountSurfaceParams`.
- The desktop host implements `pane.split` for leaf panes. It converts the target leaf into a split parent, creates two child leaf panes, moves any mounted surface onto the first child, and returns a full workspace detail snapshot.
- The desktop host implements `pane.focus` for leaf panes and persists `active_pane_id` plus focus timestamps through the existing workspace bundle save path.
- The desktop host implements `pane.close` for leaf panes. It supports `detach_surface`, `close_surface`, and `fail_if_session_running`, collapses the remaining sibling into the split parent, and persists the resulting workspace detail.
- The desktop host implements `pane.resize_layout` for split panes, validating ratios from `0.1` through `0.9` and persisting the updated split ratio.
- The desktop host implements `pane.mount_surface` and `pane.unmount_surface` for leaf panes. Mounting preserves the surface record, ensures a surface is mounted in only one pane, and focuses the target pane.
- The TypeScript control client now maps typed pane and surface summaries from `workspace.get`, `pane.split`, `pane.focus`, `pane.close`, `pane.resize_layout`, `pane.mount_surface`, and `pane.unmount_surface`.
- The desktop UI renders the persisted pane tree, supports vertical and horizontal split commands, focuses panes from the canvas, exposes active-pane close with selectable surface policies, provides split ratio sliders, and can mount/unmount detached surfaces.
- The desktop UI now exposes workspace rename and close commands; workspace close has selectable close policies.
- The xterm renderer is now mounted only for the active pane when that pane has a terminal surface, preventing input from being sent to a stale session after focusing an empty pane.
- The browser-preview control client mirrors the same pane/surface shape for local UI development outside Tauri.

## Not Yet Implemented

- Empty pane affordances are minimal; command routing works, but there is no full pane command palette yet.
- Hidden terminal output is still polled through the current recent-output path instead of a cursor/snapshot model.
- Workspace rename is exposed through a simple native dialog; polished in-app rename remains future UX work.
- Split ratio adjustment uses range sliders rather than pointer-drag split handles.

## Verification Evidence

The following targeted commands passed on 2026-06-18 using the repository-local Rust toolchain and desktop Node workspace:

```powershell
cargo test -p agentmux-store -p agentmux-ipc -p agentmux-desktop-host -- --nocapture
npm --prefix apps/desktop run build
```

The tests covered:

- parsing `pane.split`, `pane.focus`, `pane.close`, `pane.resize_layout`, `pane.mount_surface`, and `pane.unmount_surface` request params
- replacement bundle pruning for removed pane, surface, and session rows
- desktop-host split/focus/close/resize-layout/mount/unmount round trips through the persisted workspace bundle
- preservation of existing workspace/session behavior while adding pane APIs
- TypeScript checking and Vite production build for the split-pane UI

## Remaining Gap

Goal 5 now has a persisted split/focus/close/resize-layout/mount/unmount backbone, active-pane-aware terminal mount, basic workspace rename/close UI, selectable close policies, and interactive split ratio sliders. The next implementation slice should polish in-app rename, add richer empty-pane affordances, and replace range sliders with pointer-drag split handles.
