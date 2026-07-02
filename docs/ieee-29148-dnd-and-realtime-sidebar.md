# Drag-and-Drop Placement & Real-Time Sidebar — Requirements Specification

Conforms to ISO/IEC/IEEE 29148:2018.

| Field | Value |
|-------|-------|
| Document ID | SRS-UX-002 |
| Status | Draft |
| Scope | Desktop UI (apps/desktop) + Tauri host (src-tauri) |
| Related | [ieee-29148-system-design.md](./ieee-29148-system-design.md), [ieee-29148-desktop-performance-optimization.md](./ieee-29148-desktop-performance-optimization.md) |

## 1. Purpose

Two user-reported UX gaps:
1. Tabs, workspaces, and panes should be repositionable by drag-and-drop. The capability largely exists on `main` (surface-tab reorder + cross-workspace move, workspace card/group/member reorder, pane-header drag to swap mounted surfaces) but is unreleased and only the group/member path has regression coverage — the rest is unverified.
2. The footer git status and current path lag by one beat ("한템포 느려"): the sidebar is on a 5 s poll tier and git status has a 3 s cache, so a `cd` in the terminal takes up to ~5 s to reflect. It should feel real-time.

## 2. Requirements

### 2.1 Drag-and-drop placement (verify + cover)

| ID | Requirement | Verification |
|----|-------------|--------------|
| DD-1 | Dragging a surface tab onto another tab SHALL reorder the tab strip, honoring before/after placement from the drop position; order persists per workspace across reload. | Playwright test: create 2+ tabs, drag last onto first, assert order; reload, assert persisted. |
| DD-2 | Dragging a surface tab onto a workspace card SHALL move the surface to that workspace and select it. | Playwright test. |
| DD-3 | Dragging an ungrouped workspace card onto another card SHALL reorder the sidebar list with before/after placement; order persists. | Playwright test. |
| DD-4 | Dragging a pane header onto another leaf pane SHALL swap the two panes' mounted surfaces (or move into an empty pane). Sessions must keep running through the swap. | Playwright test: 2 panes, drag one header onto the other, assert surfaces swapped. |
| DD-5 | Existing group/member drag behavior SHALL remain unchanged. | Existing tests stay green. |
| DD-6 | Any defect found while adding DD-1..DD-4 coverage SHALL be fixed in the same change set. | Tests green. |

### 2.2 Real-time footer (cwd + git)

| ID | Requirement | Verification |
|----|-------------|--------------|
| RT-1 | When a session emits an OSC 7 cwd change, the host SHALL push a change signal to the UI (Tauri event, e.g. `agentmux://sidebar-changed`) from the output pump when it drains cwd updates — no new wire-schema methods. | Rust unit test where feasible; manual/e2e observation. |
| RT-2 | The UI SHALL subscribe to that signal and trigger the existing debounced `refreshSidebar` (~150 ms), so a `cd` reflects in the footer within ~0.5 s instead of up to 5 s. | Manual verification + review. |
| RT-3 | The git-status cache MUST NOT serve stale data across a cwd change (cache is keyed by cwd, so a new cwd is a miss — preserve this property). | Code review. |
| RT-4 | The 5 s sidebar poll remains as fallback; no increase in idle IPC volume beyond the pushed events. | Code review. |
| RT-5 | The signal SHALL be lightweight (no payload beyond workspace/session id needed by the UI) and debounced/coalesced on the UI side to avoid refresh storms during rapid `cd` sequences. | Code review. |

## 3. Constraints

- C-1: No breaking changes to `agentmux.control.v1` / `agentmux.event.v1`.
- C-2: All gates stay green: desktop build (tsc+vite), `npm run check` (fmt, clippy `-D warnings`, tests, docs), Playwright suite not worse than baseline.
- C-3: Minimal diffs; follow existing DnD patterns (payload MIME types, `reorderIds`, `dropPlacementFromEvent`).
- C-4: No commits with AI signatures.

## 4. Acceptance

DD-1..DD-4 each have a passing Playwright test; footer cwd updates within ~0.5 s of a `cd` in a live terminal; gates green.
