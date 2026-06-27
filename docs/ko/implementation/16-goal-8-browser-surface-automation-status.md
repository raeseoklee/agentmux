# Goal 8 Browser Surface Automation Status

Status: In progress
Date: 2026-06-18

This document records Goal 8 implementation slices for browser surfaces, scoped browser automation commands, and desktop browser pane integration.

## Implemented

- `agentmux-browser` now defines the browser automation boundary:
  - `BrowserSurface`
  - `BrowserCommand`
  - `BrowserCommandResult`
  - `BrowserAutomation`
  - `BrowserAutomationError`
- Browser commands expose an explicit `surface_id`.
- `BrowserSurface` carries both the UI `surface_id` and backend `browser_id`.
- `InMemoryBrowserAutomation` provides a deterministic test adapter for:
  - surface creation
  - surface lookup and close
  - navigation
  - screenshot artifact bytes
  - DOM snapshot
  - selector click
  - selector typing
  - evaluate
- `CdpBrowserAutomation` provides a real browser runtime adapter using Chrome
  DevTools Protocol against Microsoft Edge, Chrome, or Chromium.
- The CDP adapter launches an isolated browser profile per browser surface,
  creates a page target, and maps the existing command API to CDP:
  - `Page.navigate`
  - `Page.captureScreenshot`
  - `Runtime.evaluate` for DOM snapshot and evaluation
  - `Input.dispatchMouseEvent`
  - `Input.insertText`
- The desktop host selects browser automation with
  `AGENTMUX_BROWSER_AUTOMATION=auto|cdp|memory`; production `auto` uses CDP
  when a supported browser executable is discovered and otherwise falls back to
  the deterministic in-memory adapter.
- `AGENTMUX_BROWSER_EXECUTABLE` can point at a specific browser executable for
  lab verification.
- Browser automation rejects unknown surface ids and validates required command inputs.
- The in-memory adapter keeps navigation scoped to the requested surface so one browser surface cannot silently mutate another.
- `agentmux-ipc` now defines typed params/results for:
  - `surface.create_browser`
  - `browser.navigate`
  - `browser.screenshot`
  - `browser.dom_snapshot`
  - `browser.click`
  - `browser.type`
  - `browser.evaluate`
- The desktop host routes `surface.create_browser` through the workspace store, creates a persisted browser surface, and mounts it into the requested pane or the active pane.
- The desktop host routes browser commands to the configured automation adapter after validating that the target `surface_id` exists and is a browser surface.
- Browser command responses include scoped navigation, screenshot handle metadata, DOM snapshots, action acknowledgements, and evaluate JSON.
- Browser commands reject missing surfaces and terminal surfaces without silently retargeting another surface.
- Browser command failures are recorded in a bounded desktop diagnostics history exposed by `diagnostics.browser`.
- Browser command failures also create persisted `browser.action_failed` notifications with `error` severity.
- The desktop UI refreshes notification state after browser action failures so the notification panel can show the failure without waiting for the next poll.
- The desktop React UI now exposes a `Browser` workspace action that creates and mounts a browser surface in the active pane.
- Browser panes render a dedicated browser surface panel with URL navigation, DOM snapshot, screenshot, selector click, coordinate click, selector typing, and evaluate controls.
- Browser panes embed the navigated URL in a constrained browser viewport for
  `http`, `https`, and `data` URLs, while blocking unsafe schemes from the
  embedded preview path.
- Browser pane output shows the current URL, last action state, screenshot handle metadata, DOM snapshot HTML, evaluate result, and command errors.
- Browser preview automation covers browser surface creation and command flow in Playwright.

## Validation

The following targeted checks passed on 2026-06-18 using the repository-local Rust toolchain:

```text
cargo test -p agentmux-browser
npm run browser:cdp-smoke
cargo test -p agentmux-ipc -p agentmux-browser -p agentmux-desktop-host
npm --prefix apps/desktop run build
npm --prefix apps/desktop run test:ui
```

Covered behavior includes:

- command-to-surface id extraction
- explicit surface scoping across multiple browser surfaces
- unknown surface rejection
- scoped DOM snapshot and screenshot artifact output
- browser IPC param parsing
- desktop `surface.create_browser` persistence and pane mounting
- desktop browser navigate, screenshot, DOM snapshot, click, type, and evaluate routing
- missing or non-browser surface rejection
- browser failure diagnostics and `browser.action_failed` notification creation
- CDP adapter command helpers and deterministic runtime selection
- CDP real browser smoke against installed Chrome on Windows in headless mode
  with a local HTTP fixture
- TypeScript control client browser method mapping
- desktop browser pane UI creation and command flow
- embedded browser viewport URL rendering in Playwright

## Remaining Work

- Unify the embedded pane viewport with the CDP-controlled browser target so the
  visible pane and automation target are the same browser surface.
- Run and archive the local HTTP fixture CDP smoke as part of the Windows
  release lab evidence.

## Summary

Goal 8 now has a stable automation boundary, scoped in-memory adapter, CDP real browser adapter, typed IPC surface, desktop control-plane routing, desktop UI controls for browser surface creation plus first browser commands, embedded URL viewport rendering, and diagnostics/notification surfacing for browser automation failures. The next slice should unify the visible pane viewport with the CDP-controlled target and promote CDP smoke coverage into the Windows lab gate.
