# AgentMux Desktop Performance Optimization — Requirements Specification

Conforms to ISO/IEC/IEEE 29148:2018 (Requirements engineering — Software requirements specification).

| Field | Value |
|-------|-------|
| Document ID | SRS-PERF-001 |
| Status | Implemented baseline; PR-7 through PR-12 complete |
| Scope | AgentMux desktop application (Tauri host + React/xterm UI + Rust control plane hot paths) |
| Related | [ieee-29148-system-design.md](./ieee-29148-system-design.md), [implementation/17-goal-9-performance-diagnostics-status.md](../ko/implementation/17-goal-9-performance-diagnostics-status.md), [implementation/04-ui-terminal-rendering.md](../ko/implementation/04-ui-terminal-rendering.md) |

## 1. Introduction

Implementation note, 2026-06-25:

- PR-1 through PR-6 were already present in the desktop and core code paths.
- This follow-up implements PR-7 through PR-10: split action descriptor
  construction, byte-level agent-signal scan prefilters with per-session
  heuristic throttling, read-only runtime pre-dispatch event-collection
  reduction, and an amortized `VecDeque` recent-output ring.
- The next follow-up implements PR-11 and PR-12: DFS backtracking for
  `renderPane` cycle detection, cached WebGL addon module loading, and debounced
  WebGL teardown on pane deactivation.
- Verification note: `npm run desktop:build` and `git diff --check` pass for
  PR-11/PR-12. A targeted Playwright split-pane smoke currently times out before
  it reaches pane rendering because the preview test suite still assumes a
  default workspace exists; that fixture needs a separate no-default-workspace
  update.

### 1.1 Purpose

This specification defines the requirements for an overall responsiveness improvement of the AgentMux desktop application. Users report the application feels generally laggy ("전반적으로 느려"). The dominant cause is a 1.2 s polling loop that unconditionally triggers a full re-render of the ~8000-line root React component on every tick, compounded by un-memoized render paths, redundant IPC fan-out, and unnecessary serialization on the hot terminal-output path.

### 1.2 Scope

In scope: the desktop UI render pipeline (`apps/desktop/src/agentmux/*`, `apps/desktop/src/terminal/*`, `apps/desktop/src/control/ControlClient.ts`), the Tauri host request dispatch (`apps/desktop/src-tauri/src/lib.rs`), and Rust control-plane output hot paths (`crates/agentmux-core/src/lib.rs`).

Out of scope: the named-pipe transport, CLI, browser-surface automation, persistence schema, and any change to the `agentmux.control.v1` / `agentmux.event.v1` wire schemas (optimizations must be backward compatible).

### 1.3 Definitions

| Term | Definition |
|------|------------|
| Poll tick | One iteration of the periodic UI refresh loop (`POLL_INTERVAL_MS = 1200`). |
| Snapshot poll | The per-active-terminal output delta poll (`SNAPSHOT_POLL_MS = 40`). |
| Hot path | Code executed per output chunk or per poll tick. |
| Render churn | Re-renders caused by new object/array/function references rather than changed data. |

## 2. References

- ISO/IEC/IEEE 29148:2018.
- Performance findings analysis (internal), 12 ranked findings, summarized in §4.
- Existing perf gates: `npm run perf:gates`, `cargo run -p agentmux-bench-single-terminal-latency`.

## 3. Overall Description

### 3.1 Product perspective

The desktop UI polls the control plane every 1.2 s for workspace detail, agent attention, agent states, notifications, profiles, sidebar state, and workspace groups, and each active terminal polls output deltas every 40 ms. Both paths currently do more work than required, producing render churn and IPC contention on a single `Mutex<DesktopRuntimeControl>`.

### 3.2 Constraints

- C-1: No wire-schema breaking changes; control/event envelopes stay compatible.
- C-2: `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test --workspace` must stay green (clippy warnings are hard errors).
- C-3: `npm --prefix apps/desktop run build` (tsc + vite) must pass.
- C-4: No behavioral regression in terminal rendering, agent-state detection, notifications, or workspace/pane UX. Existing Playwright UI tests must pass.
- C-5: Changes must be observable as reduced re-render count / reduced per-tick IPC calls, not merely asserted.

### 3.3 Assumptions

- Pane trees are shallow in practice (depth ≤ 5).
- Profiles and workspace groups change only on explicit user mutation, never autonomously.

## 4. Specific Requirements

Priority: P1 (dominant), P2 (significant), P3 (polish). Each requirement is verifiable.

### 4.1 Functional / Performance requirements

| ID | Priority | Requirement | Evidence / source | Verification |
|----|----------|-------------|-------------------|--------------|
| PR-1 | P1 | The poll loop SHALL call each state setter (`setDetail`, `setAttention`, `setAgentStates`, `setNotifications`, `setProfiles`, `setSidebarState`) only when the newly fetched value differs from the current value by a cheap structural-equality check. | `useAgentmuxControl.ts:291-316`, `mergeNotifications` 235-248 | A poll tick that returns identical data produces zero React re-renders (verified via render counter / React Profiler in dev). |
| PR-2 | P1 | The leaf pane renderer SHALL be extracted into a `React.memo` component (`PaneView`) receiving stable `useCallback` handlers and hoisted style objects, so panes whose inputs are unchanged do not reconcile. | `AgentmuxTerminalApp.tsx:2919`, `:3001-3037` | Switching focus between two panes re-renders only the two affected `PaneView` instances, not all panes. |
| PR-3 | P2 | Component-body inline style objects that reference only CSS variables (`iconBtn`, `winCtlBtn`, `groupActionBtn`, etc.) SHALL be module-level constants; `rootStyle`/`buildRootVars` SHALL be wrapped in `useMemo` keyed on `[theme, accent, fontSize]`. | `AgentmuxTerminalApp.tsx:2298-2385` | Style prop references are referentially stable across renders with unchanged theme/accent/fontSize. |
| PR-4 | P2 | `refreshSidebar` triggered by `activePaneId` change SHALL be debounced (~150 ms) and SHALL skip if a regular poll tick is imminent, preventing concurrent `getSidebarState` calls. | `AgentmuxTerminalApp.tsx:1218-1223` | Rapid pane switching issues at most one `getSidebarState` per debounce window. |
| PR-5 | P2 | The periodic poll SHALL NOT fetch `listProfiles` or `listWorkspaceGroups`; these SHALL be fetched on initial hydration and reloaded only after a mutation that can affect them. | `useAgentmuxControl.ts:291-316` (line 306) | Per-tick IPC call count drops from 7 to 4; profiles/groups still update after their mutations. |
| PR-6 | P2 | The `session.snapshot` handler SHALL avoid Base64 encoding when there is no new output (`endOffset == expected`); the JS decode path SHALL use a native decode (`atob`/`Uint8Array.from` or `Buffer`) rather than a per-character loop. | `agentmux-core/src/lib.rs:1225-1233`, `ControlClient.ts:689-696`, `LiveTerminal.tsx:131,167-170` | A steady (no-output) terminal performs zero Base64 encode/decode per snapshot poll. |
| PR-7 | P3 | The command `actions` list SHALL separate static descriptors from workspace/WSL-derived descriptors so the list rebuilds only when those source lists change identity. | `AgentmuxTerminalApp.tsx:2405-2636` | After PR-1, `actions` useMemo does not recompute on no-op poll ticks. |
| PR-8 | P3 | Agent-signal detection SHALL early-reject output batches lacking a marker byte (`':'` or `0x1b`) before UTF-8 decoding and line scanning, and heuristic scanning SHALL be rate-limited per session. | `agentmux-core/src/lib.rs:1882-1891`, called at `:829-839` | Batches without markers skip `from_utf8_lossy` and the line scan (unit test on a non-marker batch). |
| PR-9 | P3 | The Tauri host SHALL NOT invoke `collect_events()` redundantly before request dispatch for read-only methods that do not depend on event state. | `lib.rs:467,473` | `workspace.get` / `session.snapshot` do not drain all four backends pre-dispatch. |
| PR-10 | P3 | The recent-output ring trim SHALL avoid an O(limit) memmove on every chunk (e.g., amortized shift or `VecDeque`). | `agentmux-core/src/lib.rs:2068-2085` | Bench shows reduced per-chunk cost under sustained 1 MB/s output. |
| PR-11 | P3 | `renderPane` recursion SHALL avoid cloning the `visited` set per level (DFS backtracking or precomputed map). | `AgentmuxTerminalApp.tsx:2927` | No `new Set(visited)` per recursive level. |
| PR-12 | P3 | The WebGL addon module SHALL be cached after first dynamic import; WebGL disable on deactivation SHALL be debounced to survive rapid pane switching. | `LiveTerminal.tsx:280-293`, `XtermTerminalRenderer.ts:155-166` | Second activation of a session does not re-import the addon chunk. |

### 4.2 Non-functional requirements

- NFR-1: No measurable increase in idle CPU; target reduction of UI re-renders on a no-op tick from "full tree" to zero.
- NFR-2: All changes individually revertable; PR-1..PR-6 landable as one cohesive change set, PR-7..PR-12 as follow-ups.
- NFR-3: Maintain code style consistent with surrounding files; keep the workspace clippy-clean.

## 5. Verification & Acceptance

| Method | Applies to |
|--------|-----------|
| React Profiler / a dev render counter on `AgentmuxTerminalApp` and `PaneView` | PR-1, PR-2, PR-3, PR-7 |
| Counting IPC calls per tick (instrument `ControlClient` in dev) | PR-4, PR-5 |
| Unit tests in `agentmux-core` | PR-6 (snapshot no-op), PR-8 (marker reject), PR-10 (ring), |
| `npm --prefix apps/desktop run build`, `npm run check`, Playwright `test:ui` | C-2, C-3, C-4 |
| `npm run perf:gates`, single-terminal latency bench | NFR-1, PR-10 |

Acceptance: a no-op poll tick causes zero re-renders (PR-1/PR-2), per-tick IPC drops 7→4 (PR-5), and all gates stay green (C-2..C-4).

## 6. Implementation Order

1. PR-1 (gate setState) — removes the root cause of render churn.
2. PR-2 (`PaneView` memo) — stops O(N-panes) reconciliation per tick.
3. PR-3 (hoist styles) — cheap prop-churn reduction.
4. PR-4 (debounce sidebar) — IPC pile-up on focus change.
5. PR-5 (trim hot poll fan-out) — IPC volume + Mutex contention.
6. PR-6 (snapshot Base64 no-op skip + native decode) — hot output path.
7. PR-7, PR-8, PR-9, PR-10, PR-11, PR-12 — follow-up polish.

PR-1..PR-6 address the reported general lag and are intended to land as a single low-risk change set.
