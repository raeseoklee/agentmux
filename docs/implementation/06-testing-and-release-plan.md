# Testing and Release Plan

Status: Draft
Date: 2026-06-18

이 문서는 AgentMux의 테스트 계층, Windows/WSL 검증 환경, release gate를 정의한다.

## Test Pyramid

| Layer | Scope | Examples |
|---|---|---|
| Unit | Pure logic | id generation, state machine, parser, config validation |
| Contract | API schema | request/response fixtures, error codes, event schema |
| Integration | Runtime + backend | ConPTY spawn, WSL launch, durable attach |
| UI automation | Desktop behavior | split pane, focus, type, notification |
| Performance | Budgets | latency, idle sessions, high output |
| Recovery | Crash/restart | layout restore, durable reattach, lost backend |
| Manual exploratory | Platform edge cases | Windows policy, WSL install variants, shell profiles |

## Required Unit Tests

Core:

- workspace create/rename/close state transitions
- pane split/close layout invariants
- surface mount/unmount rules
- session state machine
- safe close policy
- agent state transitions

Parser:

- tmux-control output line parsing
- partial line buffering
- escaped payload decoding
- unknown message handling
- malformed message diagnostics

Store:

- migrations
- atomic layout update
- recovery query
- unknown enum tolerance
- redaction before persistence

IPC:

- envelope validation
- auth failure
- typed error mapping
- schema fixture stability

## Backend Integration Tests

### Windows Native

Scenarios:

- spawn PowerShell
- spawn cmd
- send text
- send Ctrl+C
- resize
- process exits with code
- invalid cwd
- invalid command

### WSL Direct

Scenarios:

- discover distributions
- launch default distribution
- launch selected distribution
- resolve Windows path
- run command in WSL cwd
- handle WSL unavailable
- handle missing distribution

### Durable WSL

Scenarios:

- create durable workspace session
- spawn shell-backed session
- detach control client
- reattach without duplicate process
- recover after UI restart
- recover after core restart where supported
- backend server dead
- pane missing
- high-output session

## UI Automation Tests

Minimum desktop test flows:

1. Launch app and create workspace.
2. Open native terminal.
3. Type command and verify visible output.
4. Split pane horizontally.
5. Launch second terminal.
6. Focus pane via click and keyboard shortcut.
7. Resize split and verify terminal dimensions update.
8. Close pane with running session and verify safe close prompt.
9. Restart app and verify workspace layout.
10. Trigger synthetic attention event and verify badge/panel.

UI tests should avoid fragile pixel-only assertions except for rendering smoke tests.

Current coverage note: `apps/desktop/tests/ui/synthetic-attention.spec.ts`
uses the browser-preview control client to trigger synthetic agent attention and
verifies the workspace badge, pane badge, attention clear flow, notification
panel entry, and notification dismissal.

The same Playwright suite also creates a browser surface in the active pane and
drives navigation, DOM snapshot, screenshot, click, type, and evaluate controls
through the browser-preview client.

Current unit coverage also verifies that browser command failures are exposed
through `diagnostics.browser` and persisted as `browser.action_failed`
notifications.

Goal 8 CDP coverage is split deliberately: unit tests cover CDP command helper
behavior and runtime selection without launching a browser, while
`npm run browser:cdp-smoke` launches installed Edge/Chrome in headless mode and
verifies navigation, screenshot, DOM snapshot, click, type, and evaluate against
a local HTTP fixture.

The desktop build and UI smoke release gate is captured with:

```powershell
npm run desktop:gates
```

That runner records the production frontend build, generated `dist` files, and
the Playwright smoke suite output.

## Performance Tests

Performance tests are not optional end-game tests. They begin with Phase 1 and become release gates later.

Required benchmarks:

- `bench_single_terminal_latency`
- `bench_many_idle_sessions`
- `bench_high_output`
- `bench_resize_storm`
- `bench_restart_recovery`

Current benchmark harnesses:

- `cargo run -p agentmux-bench-single-terminal-latency`
- `cargo run -p agentmux-bench-many-idle-sessions`
- `cargo run -p agentmux-bench-high-output`
- `cargo run -p agentmux-bench-resize-storm`
- `cargo run -p agentmux-bench-restart-recovery`

The restart-recovery binary uses a simulated durable backend so it can run
without a WSL/tmux lab. The real WSL/tmux recovery gate is captured separately
with:

```powershell
npm run tmux:reattach-smoke
```

That runner verifies launch/input/output and reattach without duplicating the
shell process on a reference Windows machine with WSL and tmux installed.

Each run records:

- app version
- git commit if available
- Windows version
- WSL version
- CPU model
- RAM
- display scale
- power mode
- backend kind
- scenario parameters
- p50/p95/p99 where relevant

## CI Matrix

Recommended initial CI:

| Job | OS | Purpose |
|---|---|---|
| `rust-test` | Windows | core, IPC, store, parser tests |
| `desktop-build` | Windows | desktop app compiles |
| `schema-fixtures` | Windows | API fixture stability |
| `lint-format` | Windows | formatter and linter |
| `docs-check` | Windows or Linux | links and forbidden stale text checks |

WSL integration in hosted CI may be limited. Durable WSL tests should have a local lab profile until CI can provision WSL reliably.

## Local Windows Lab

Maintain at least one reference machine profile:

- Windows edition and version
- WSL version
- installed distributions
- CPU/RAM
- GPU/display
- power mode
- terminal font/rendering settings

Local lab checklist:

- fresh install
- existing WSL distribution
- no WSL distribution
- long path workspace
- path with spaces
- non-ASCII path
- high-DPI display
- laptop battery mode

## Release Gates

A release candidate must pass:

- all unit tests
- all API contract tests
- Windows native backend integration tests
- WSL direct integration tests on local lab
- durable WSL recovery tests on local lab
- UI smoke suite
- performance release gate
- installer smoke test
- diagnostics export smoke test

Current diagnostics export coverage: unit tests exercise `diagnostics.export`
through the desktop control envelope and verify recovery counts, backend health,
queue pressure, browser failure history, and persisted notification inclusion.
The CLI exposes the same bundle through `agentmux diagnostics export`.
`npm run diagnostics:packaged-smoke` verifies the packaged desktop host named
pipe path with isolated store, token, and pipe settings.
`npm run tmux:reattach-smoke` verifies the real WSL/tmux launch and reattach
path and archives release evidence.
`npm run installer:build-smoke` verifies that an unsigned NSIS setup executable
can be produced, prepares the `agentmux.exe` and `cmux.exe` Tauri sidecar
inputs, compiles the install/uninstall PATH hook, and records the artifact
hash. Manual release signoff still requires installing and uninstalling that
setup package on a release machine.
`npm run installer:contents-gate` opens the generated setup executable without
installing it, extracts the CLI sidecars with 7-Zip, and compares their hashes
against the prepared Tauri sidecar inputs. It also verifies that the generated
installer script includes the AgentMux NSIS hook file and calls the
install/uninstall PATH hook.
`npm run installer:lifecycle-gate -- preinstall`, `-- installed`, and
`-- uninstalled` record non-mutating evidence for each manual installer
lifecycle phase. The installed phase also verifies registry uninstall metadata,
the installed executable, an uninstall command, and Start Menu shortcut
presence. Use `npm run installer:lifecycle-gate -- installed -RequireCli -RequireUserPath`
for final signoff so installed `agentmux.exe` and `cmux.exe` sidecars plus
install-directory user PATH registration are also required.
The 2026-06-25 release closure captured a real NSIS install, installed desktop
smoke, installed CLI server smoke, and silent uninstall gate in
`29-installed-lifecycle-e2e-release-closure.md`.

Release candidate must not have:

- known duplicate durable session bug
- known unbounded output memory growth
- known local IPC unauthenticated control path
- known crash on backend disconnect
- known destructive close without explicit user action

## Manual Verification Script

Manual release smoke:

1. Install app.
2. Create workspace for a real repository.
3. Open native shell and run a simple command.
4. Open WSL durable shell and run a long command.
5. Split panes and switch focus.
6. Start a long-running agent-like command.
7. Close UI.
8. Reopen UI and verify durable session remains.
9. Send input to recovered session.
10. Open diagnostics and export bundle.
11. Create CLI-driven session.
12. Verify notification history.

## Documentation Checks

Docs should be checked for:

- broken relative links
- stale requirement IDs
- outdated command names
- missing verification evidence for release claims
- accidental inclusion of external comparison notes in implementation docs

