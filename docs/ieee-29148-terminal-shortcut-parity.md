# Terminal Shortcut & Interaction Parity — Requirements Specification

Conforms to ISO/IEC/IEEE 29148:2018.

| Field | Value |
|-------|-------|
| Document ID | SRS-UX-003 |
| Status | Draft |
| Scope | Desktop terminal UX (apps/desktop): keyboard shortcuts, clipboard, scrollback interactions |
| Related | [ieee-29148-system-design.md](./en/ieee-29148-system-design.md), [ieee-29148-dnd-and-realtime-sidebar.md](./ieee-29148-dnd-and-realtime-sidebar.md) |

## 1. Purpose

Close the gap between AgentMux and mainstream terminal emulators (Windows Terminal, iTerm2, PuTTY, tmux-style multiplexers) for everyday shortcuts and interactions. Users reported intermittent copy/paste misbehavior and missing Ctrl+Tab tab switching; a systematic review found further gaps listed here with a phased plan.

## 2. Current state (verified in code)

Already present: rebindable shortcut system with two-step chords (`actions.ts` defaults + config overrides); Ctrl+Shift+P / Ctrl+K palette; Ctrl+F search overlay; Ctrl+, settings; Ctrl+I notifications; Ctrl+N workspace; Ctrl+T new WSL terminal; Ctrl+D / Ctrl+Shift+D splits; Ctrl+Shift+U attention jump; clipboard via Ctrl+C(-with-selection)/Ctrl+Shift+C/Ctrl+V/Shift+Insert/Ctrl+Insert/right-click; URL Ctrl+click; wheel scrollback with alt-screen and ConPTY-TUI handling; per-workspace tab strip with drag reorder.

Landing alongside this spec (in progress): copy-on-select; clipboard stale-fallback discipline; Ctrl+Tab / Ctrl+Shift+Tab tab cycling (`surface.nextTab`/`surface.prevTab`).

## 3. Requirements & phased plan

Priorities: P1 = high-value, low-risk, ship next; P2 = valuable, needs design care; P3 = nice-to-have / advanced.

### P1 — core parity

| ID | Requirement | Default binding | Notes |
|----|-------------|-----------------|-------|
| TS-1 | Scrollback paging from the keyboard | Shift+PgUp / Shift+PgDn | Renderer key handler; scroll one page in the normal buffer, pass through in alt buffer/TUI. Ctrl+Home / Ctrl+End scroll to top/bottom. |
| TS-2 | Jump to tab N in the active workspace | Ctrl+Alt+1..9 | Matches Windows Terminal; avoids shell-conflicting bare Ctrl+digits. Uses the tab order (incl. drag reorder). |
| TS-3 | Close current tab | Ctrl+Shift+W | Reuses the existing close-surface path incl. running-session confirmation. |
| TS-4 | Clear terminal buffer action | Ctrl+Shift+K + palette entry | xterm `clear()`; sends nothing to the shell. |
| TS-5 | Per-terminal font zoom | Ctrl+= / Ctrl+- / Ctrl+0 | Adjusts the existing fontSize setting (persisted), reset to default. |
| TS-6 | Select-all in terminal | Ctrl+Shift+A | xterm `selectAll()`; combined with copy-on-select this yields copy-visible-buffer. |

### P2 — multiplexer ergonomics

| ID | Requirement | Default binding | Notes |
|----|-------------|-----------------|-------|
| TS-7 | Directional pane focus | Alt+←↑↓→ | Geometric neighbor search over the split tree; skip when a TUI needs Alt-keys? No — WT precedent: terminal apps rarely need Alt+arrows; rebindable anyway. |
| TS-17 | Workspace cycling | Ctrl+Alt+↑ / Ctrl+Alt+↓ | Previous/next workspace in sidebar order (wrap-around), matching the vertical sidebar visually (Discord/Slack server-switch convention). Uses the same selection path as clicking a workspace card. |
| TS-8 | Keyboard pane resize | Ctrl+Alt+Shift+←↑↓→ | Adjusts splitRatio of the parent split (existing resizePane). |
| TS-9 | In-buffer terminal search | Ctrl+Shift+F, F3 / Shift+F3 next/prev | xterm search addon; the existing Ctrl+F overlay is app-level search — verify and keep both, clearly labeled. |
| TS-10 | Middle-click paste | mouse middle button | X11 habit for WSL users; paste selection-clipboard (use the last terminal selection, falling back to system clipboard). Setting-gated, default on. |
| TS-11 | Multi-line paste guard | — | Optional confirm dialog when pasting ≥2 lines into a non-bracketed-paste session (Windows Terminal behavior). Setting, default on. |
| TS-12 | Bracketed-paste audit | — | Verify `sendPaste` wraps ESC[200~/201~ when the app enabled mode 2004 and normalizes \r\n→\r; fix if raw (agent report pending). |

### P3 — advanced

| ID | Requirement | Default binding | Notes |
|----|-------------|-----------------|-------|
| TS-13 | Zoom (maximize) a pane temporarily | Ctrl+Shift+Z | tmux `z` equivalent; hide siblings, keep sessions running. High value for agent-grid workflows but needs layout-state design. |
| TS-14 | Broadcast input to all panes in a tab | palette toggle | tmux synchronize-panes; danger-gated UI badge while active. |
| TS-15 | Tab rename | F2 / double-click title | Persist per-surface title override. |
| TS-16 | Fullscreen toggle | F11 | Tauri window fullscreen. |

## 4. Constraints

- C-1: All new bindings go through the existing rebindable-shortcut system (config overrides, conflict reporting in settings) — no hardcoded key handling outside the renderer's terminal-scoped keys.
- C-2: Terminal-focused behavior must not break TUI input: any key consumed app-side must be one mainstream terminals also reserve; everything stays rebindable.
- C-3: Gates stay green (tsc/vite build, cargo fmt/clippy/test, Playwright); each phase lands with tests for its bindings.
- C-4: No wire-schema breaking changes.

## 5. Acceptance / order

Implement P1 as one change set (TS-1..TS-6, est. small), then P2 items individually (TS-12 first if the audit finds raw pastes), P3 on demand. Each item: action descriptor + default binding + settings visibility + a Playwright binding test.
