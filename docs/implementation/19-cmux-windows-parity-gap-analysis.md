# cmux Windows Parity Gap Analysis

Status: Draft
Date: 2026-06-19

이 문서는 cmux 공식 문서 전체를 기준선으로 삼아 AgentMux를 "Windows용 cmux"에 가깝게 만들기 위해 필요한 미구현 기능을 정리한다. 목표는 macOS cmux를 그대로 복제하는 것이 아니라, Windows-only 제품으로서 같은 사용자 여정을 제공하는 것이다.

## Sources

공식 문서 기준:

- [Getting Started](https://cmux.com/ko/docs/getting-started)
- [Concepts](https://cmux.com/ko/docs/concepts)
- [Workspace Groups](https://cmux.com/ko/docs/workspace-groups)
- [Configuration](https://cmux.com/ko/docs/configuration)
- [TextBox](https://cmux.com/ko/docs/textbox)
- [Session Restore](https://cmux.com/ko/docs/session-restore)
- [Custom Commands](https://cmux.com/ko/docs/custom-commands)
- [Dock](https://cmux.com/ko/docs/dock)
- [Keyboard Shortcuts](https://cmux.com/ko/docs/keyboard-shortcuts)
- [CLI Reference](https://cmux.com/ko/docs/api)
- [Browser Automation](https://cmux.com/ko/docs/browser-automation)
- [Skills](https://cmux.com/ko/docs/skills)
- [Notifications](https://cmux.com/ko/docs/notifications)
- [SSH](https://cmux.com/ko/docs/ssh)
- [Claude Code Teams](https://cmux.com/ko/docs/agent-integrations/claude-code-teams)
- [oh-my-opencode](https://cmux.com/ko/docs/agent-integrations/oh-my-opencode)
- [oh-my-codex](https://cmux.com/ko/docs/agent-integrations/oh-my-codex)
- [oh-my-claudecode](https://cmux.com/ko/docs/agent-integrations/oh-my-claudecode)

## Product Interpretation

cmux는 macOS, Ghostty, Unix socket, Sparkle update, DMG/Homebrew 배포를 전제로 한다. AgentMux의 제품 기준은 다음처럼 변환한다.

| cmux 기준 | AgentMux Windows 해석 |
|---|---|
| Ghostty 기반 terminal | xterm.js UI + Rust backend + ConPTY/WSL/tmux transport |
| macOS app lifecycle | Tauri Windows app lifecycle |
| Unix socket control API | Windows named pipe JSON control API |
| `/tmp/cmux.sock` | `\\.\pipe\agentmux-control`, with `cmux.exe`, `--socket`, and `CMUX_SOCKET_PATH` aliases over the Windows named pipe |
| `~/.config/cmux/cmux.json` | `%APPDATA%\AgentMux\agentmux.json` and project `.agentmux/agentmux.json`, with compatible `.cmux/cmux.json` project read fallback plus explicit project migration |
| macOS DMG/Homebrew/Sparkle | Windows installer, optional winget/MSIX, Windows update channel |
| local shell default | WSL-first terminal, with Windows-native shell as explicit fallback only if product scope later changes |
| tmux in a local Unix shell | tmux inside selected WSL distribution |

## Current AgentMux Baseline

Implemented or partially implemented today:

- Desktop shell and React UI with workspace sidebar, pane tree, surface tab bar, command palette, settings overlay, notification panel, and browser panel.
- Workspace CRUD, pane split/focus/close/resize, surface mount/unmount, terminal and browser surface persistence.
- WSL direct terminal backend and WSL tmux-control durable backend.
- WSL missing and tmux missing diagnostics in the UI.
- Local control plane over Windows named pipe with per-user control token.
- CLI coverage for workspace/session/agent/notification/events/diagnostics,
  direct terminal run, sidebar metadata, and a `cmux` alias binary for core
  cmux-style script commands.
- Agent lifecycle state, attention markers, persisted notifications, OS desktop notification adapter, and UI badges.
- Browser automation boundary with in-memory adapter, CDP adapter, and commands for create/navigate/reload/back/forward/current-url/screenshot/dom snapshot/frames/storage/cookies/downloads/history/console/dialogs/errors/click/type/fill/press/select/scroll/hover/check/get/find/highlight/focus/zoom/wait-for-selector/evaluate.
- SSH backend crate and desktop SSH profile UI.
- Release evidence for desktop build, UI smoke, browser CDP smoke, WSL/tmux reattach smoke, diagnostics export, installer artifact build, and performance gates.

Important current limitation:

- The product now behaves more like a working AgentMux prototype than a cmux-compatible Windows distribution. The core multiplexer model exists, but most cmux customization, CLI breadth, right-sidebar tooling, multi-agent shim integration, and advanced resume behavior are still absent.

## Gap Matrix

| Area | cmux capability | AgentMux status | Gap |
|---|---|---|---|
| Install and first run | Install, initial workspace, CLI setup, auto update | Installer evidence exists; initial workspace exists; CLI binaries exist; the current NSIS build prepares `agentmux.exe` and `cmux.exe` as Tauri sidecars, registers the installed app directory in the Windows user PATH during install, removes it during uninstall, and the installer contents gate extracts both sidecars from the setup executable while verifying their hashes and PATH hook wiring; integration wrapper PATH snippets can be generated through `cmux integrations install-shims`, and `--user-path` can explicitly register the integration bin directory in the Windows user PATH; the desktop UI has a Windows setup panel for WSL detection, WSL install guidance, selected-distribution tmux probing, tmux install guidance, default WSL selection, project root save, and CLI smoke commands | No auto-update channel, no winget/MSIX plan, and installed sidecar CLI plus user PATH registration still need clean-machine lifecycle evidence with `npm run installer:lifecycle-gate -- installed -RequireCli -RequireUserPath` |
| Core hierarchy | Window -> workspace -> pane -> surface -> panel | Workspace/pane/surface model exists | No multi-window model, no workspace switcher/focus history parity, incomplete keyboard-driven navigation |
| Workspace groups | Collapsible groups, anchors, pinning, colors, icons, group CLI | Foundation implemented: store schema, IPC, CLI, preview client, and basic sidebar collapse/rendering exist | Needs multi-select grouping, context menus, group plus button, drag/reorder, anchor close policy, and Playwright coverage |
| Workspace metadata | Colors, icons, descriptions, badges, progress/log/custom metadata | Attention count exists | No general status/progress/log/sidebar-state API, no color/icon picker, no description editing |
| Configuration | Ghostty config plus cmux.json, app-owned settings, schema fallback | App config file, project `.agentmux/agentmux.json` overlays, compatible `.cmux/cmux.json` read fallback and explicit migration to AgentMux-owned project config, appearance/shortcut persistence, reload, JSON export/import, global reset, project-scope config buttons, config diagnostics UI/CLI, and a published JSON Schema artifact exist | No trust model beyond validated AgentMux JSON and no full editable config surface |
| Action registry | Builtin/custom actions, Command Palette rows, tab bar buttons, plus-button override | Registry-backed palette, shortcut settings, `actions.custom` config actions, browser URL action presets, configurable plus/tab-bar action hooks, `actions.list` discovery, and control-safe `actions.run` execution exist | UI-only actions still require the desktop UI owner; no request/ack bridge for visual-only workflows |
| Custom workspace commands | JSON-defined layouts, commands, worktree templates | Not implemented | Needs workspace layout DSL and execution engine |
| Keyboard shortcuts | Configurable one-step and two-step chords | App and project config support one-step/two-step bindings, Settings rebinding, custom action shortcuts, and conflict diagnostics | No shortcut profiles or per-context keymaps yet |
| TextBox | Rich prompt composer before terminal send | Active-terminal composer exists in the React shell. `terminal.textBox` opens a bottom composer through the action registry/shortcut layer, persists unsent drafts per terminal session, sends through `session.send-text`, and honors global/project `ui.text_box_max_lines` config with schema and UI coverage. | Needs restored TextBox state in future session snapshots, richer focus policy, and shell/agent-specific paste semantics |
| Session restore | Layout, cwd, scrollback, browser history, agent native resume IDs, manual previous-session restore | Metadata and best-effort tmux attach exist | No versioned app snapshot, no scrollback replay, no browser history restore, no resume binding trust policy, no manual previous launch restore |
| tmux compatibility | tmux shim translating agent tmux commands to native cmux splits | Basic WSL tmux-control backend exists; managed sessions now receive fake `TMUX`/`TMUX_PANE`; `agentmux __tmux-compat` translates display/capture/has/kill/list/new/rename/select/send/split/switch pane, window, and session commands into native AgentMux control calls; wrapper setup creates tmux shim files that prefer `CMUX_EXE` for WSL callbacks | Missing live wrapper validation, full tmux target grammar, richer formats, hooks/buffers, and multi-agent team mapping |
| Agent integrations | `claude-teams`, `omo`, `omx`, `omc` wrappers and shadow configs | `cmux claude-teams`, `cmux omo`, `cmux omx`, and `cmux omc` launch wrappers exist; WSL-launched wrappers execute the underlying agent inside the selected distribution; `cmux integrations setup/env` can prepare and inspect shim environments, including WSL command shape; `cmux integrations install-shims` writes persistent wrapper entrypoints and shell PATH snippets, and `--user-path` explicitly registers the bin directory in the Windows user PATH; `cmux integrations doctor` reports wrapper, shim, PATH, shadow config, restore-module, package, executable readiness, OMO plugin registration, OMO tmux enablement, and shadow `node_modules` isolation, and `--distribution` validates WSL-side executable and file visibility; `omo` shadow config generation preserves common JSONC comments/formatting while adding `oh-my-opencode`, and shadow-scoped package installation can run through WSL `bun`/`npm` when a distribution is selected; `omc` writes a Node restore module and injects it through `NODE_OPTIONS`; worker panes spawned through tmux-compat wrappers are attributed through agent state/sidebar metadata and automatically transition to completed or failed when their terminal session exits | Needs end-to-end live WSL validation against installed agent tools |
| CLI compatibility | Broad cmux CLI, socket methods, env vars, identify/capabilities/status/progress/log/browser/workspace-group | AgentMux CLI has native command families plus a `cmux` binary/alias layer for list/create/current/close workspace, list surfaces, split, send, notify, sidebar metadata, ping, capabilities, identify, workspace-group management, and browser automation commands | Raw Unix-socket compatibility and many advanced browser subcommands are still missing |
| Notifications | Panel lifecycle, unread jump, custom command, hooks, OSC notify, integrations | Basic agent/browser notification list/dismiss, OS notification, CLI notify/clear, and config-driven action-registry notification hooks exist | No suppression policy, read/unread lifecycle, sidebar jump, OSC notify compatibility, or trusted host-side arbitrary command hook pipeline |
| Browser automation | Broad command set: navigation, wait, DOM, JS injection, frames, dialog, download, cookies/storage/history, console/errors | Basic create/navigate/reload/back/forward/current-url/screenshot/dom/frames/storage/cookies/downloads/history/console/dialogs/errors/click/type/fill/press/select/scroll/hover/check/get/find/highlight/focus/zoom/wait-for-selector/evaluate exists; selector-based DOM inspection, mutation, wait, and evaluate commands accept optional frame targeting via `frame_id`/`--frame`; download baseline assigns a per-surface download directory and exposes completed/in-progress files; dialog baseline records alert/confirm/prompt and auto-returns safe defaults; error baseline records window error and unhandled rejection events; config custom actions can open or navigate browser surfaces by URL preset and run safe automation recipes, including element highlight and frame-targeted selector recipes through `frame:<frame-id>` tokens; `agentmux browser` and `cmux browser` expose open, navigate, reload, back, forward, current-url, screenshot, dom-snapshot, frames, storage, cookies, downloads, history, console, dialogs, errors, click, type, fill, press, select, scroll, hover, check, get, find, highlight, focus, zoom, wait-for-selector, evaluate, and diagnostics through the control pipe | Needs visible pane/CDP target unification plus broader browser UI parity |
| Browser UI | Browser focus mode, address bar shortcuts, DevTools, React Grab | Minimal browser panel exists | No real webview/CDP unified surface, no focus mode, no DevTools/console UI, no React Grab |
| Dock | Right-sidebar TUI controls from project/global dock.json | Embedded execution foundation implemented: `dock.get` reads `.agentmux/dock.json`, compatible `.cmux/dock.json`, global AgentMux dock, and global cmux dock candidates; the React shell renders configured controls in a right-side Dock panel with source/trust metadata; project Dock controls require backend-auditable trust approval persisted in the desktop store by workspace, source, config path, and config content hash; trusted controls launch as Dock-owned WSL terminal slots with cwd/env/title/control-id metadata plus restart/close controls and persisted per-control height overrides. | Deeper cmux TUI affordances can still be expanded after real-world Dock workflow testing |
| SSH | `cmux ssh`, deep links, remote browser proxy, scp drag/drop, relay daemon, reconnect | SSH profiles, backend skeleton, settings UI profile editing, and `agentmux ssh`/`cmux ssh` CLI launch exist; the CLI accepts direct `user@host[:port]` targets or saved profile name/id targets and opens an SSH session through the desktop control pipe | No deeplink handler, no remote browser network routing, no remote relay, no upload/reconnect story, and no remote command execution mode yet |
| Skills | Installable Codex skills for cmux workflows | Foundation implemented: repo-packaged `skills/agentmux-control` includes Codex operating instructions, UI metadata, and a control workflow reference for CLI, browser automation, diagnostics, integrations, and Dock tasks; `npm run skills:install` copies it into Codex's skill directory or a caller-provided destination. | Optional packaged-installer integration remains; split specialized skills only if the combined workflow becomes too broad |
| Release/product polish | Download/update/changelog/community path | Evidence exists but product channel incomplete | Needs signed release flow, update UX, support diagnostics package, user docs |

## Implementation Goal Groups

These are post-Goal-9 parity tracks. They should be handled as separate goals because each has its own data model, IPC contract, UI workflow, and release gate.

### Goal 10: Windows Setup and Config Foundation

Deliverables:

- First-run setup wizard for WSL distribution detection, WSL install guidance, tmux install guidance, default distribution selection, and project root selection.
- App config file at `%APPDATA%\AgentMux\agentmux.json`.
- Project config file at `.agentmux/agentmux.json`.
- Compatible `.cmux/cmux.json` project read fallback where safe, with an
  explicit migration workflow into `.agentmux/agentmux.json`.
- Schema validation, config diagnostics, and reload-config command.
- Config-backed app preferences currently stored only in React state.

Done when:

- A fresh Windows machine without WSL shows a clear install path and does not create unusable terminal sessions.
- A machine with WSL but no tmux shows a distro-specific tmux setup path.
- Config reload updates visible settings without app restart.
- Invalid project config produces an actionable UI and CLI diagnostic.

### Goal 11: Action Registry, Shortcuts, and Command Palette

Deliverables:

- Internal action registry with stable IDs for new terminal, browser, split, agent launches, workspace commands, and custom commands.
- Command Palette backed by registry instead of hard-coded rows.
- Configurable surface tab bar buttons and workspace plus-button action.
- Control-plane action registry listing and safe execution for CLI and agent
  integrations.
- Shortcut resolver with one-step and two-step chord support.
- Windows keymap policy that maps cmux `cmd` defaults to `ctrl` or explicit Windows alternatives.

Done when:

- A project config can add a Codex/Claude action to the palette and tab bar.
- A project config can add a browser URL preset to the palette or shortcuts.
- A shortcut can be rebound through config and exercised by UI tests.
- The workspace plus button can be overridden by a configured worktree workflow.
- `agentmux actions list --json` exposes built-in and project custom action
  metadata from a managed terminal workspace context.
- `agentmux actions run <id> --json` can execute control-safe built-in and
  project custom actions from a managed terminal workspace context.

### Goal 12: cmux-Compatible CLI and Sidebar Metadata

Deliverables:

- CLI aliases or compatibility subcommands for cmux-style workflows: workspace, surface, pane/split, actions, notify, status, progress, log, sidebar-state, capabilities, identify.
- `AGENTMUX_*` variables plus optional `CMUX_*` compatibility variables inside managed terminals.
- `cmux.exe`, `--socket`, and `CMUX_SOCKET_PATH` compatibility over the Windows
  named-pipe transport.
- Generic notification create/clear commands.
- Config-driven notification action hooks backed by the action registry.
- Sidebar metadata store and UI for status pills, progress bars, logs, ports, git branch, and custom metadata.

Done when:

- A script can call `agentmux notify`, `agentmux set-status`, `agentmux set-progress`, and `agentmux log` from a WSL terminal and see the sidebar update.
- `identify --json` returns active workspace, pane, surface, cwd, backend, and pipe context.
- Existing agent state notifications continue to use the same metadata channel.
- A notification can expose a configured Settings-panel action button without
  granting arbitrary host command execution.

### Goal 13: Session Restore and Resume Bindings

Deliverables:

- Versioned app snapshot for windows, workspaces, panes, surfaces, active focus, cwd, browser URLs, browser history, TextBox state, and scrollback cursor metadata.
- Manual restore previous launch flow.
- Resume binding API for terminal surfaces, including trusted tmux attach commands.
- Agent native session ID capture model and sanitized resume command storage.
- Auto-resume setting per agent class.

Done when:

- Restart restores layout without duplicating WSL/tmux sessions.
- Browser surfaces reopen to previous URLs.
- A trusted tmux binding can be resumed automatically.
- An untrusted arbitrary resume command requires user approval.

### Goal 14: tmux-Compat and Multi-Agent Integrations

Deliverables:

- `agentmux __tmux-compat` shim target.
- Fake `TMUX` and `TMUX_PANE` environment mapping to AgentMux workspace/pane IDs.
- tmux command translation for split-window, send-keys, capture-pane, select-pane/window, kill-pane/window, list-panes/windows, new-session/window.
- Wrapper commands for Claude Code Teams, oh-my-opencode, oh-my-codex, and oh-my-claudecode.
- Shadow config setup for integrations that must not mutate the user's normal agent config.
- WSL-specific installation and PATH behavior.

Current slices:

- Managed AgentMux terminal sessions export fake `TMUX`/`TMUX_PANE` plus
  AgentMux/cmux pane identity variables.
- `agentmux __tmux-compat` translates `display-message`, `capture-pane`,
  `has-session`, `kill-pane`, `kill-window`, `list-panes`, `list-sessions`,
  `list-windows`, `new-session`, `new-window`, `rename-session`,
  `rename-window`, `select-pane`, `select-window`, `send-keys`,
  `split-window`, and `switch-client` through the Windows control pipe.
- tmux-compatible `split-window`, `new-window`, and `new-session` launch a
  backend-appropriate default shell when called without an explicit command.
- `list-panes -a` and `list-windows -a` enumerate all AgentMux workspaces, with
  tmux-shaped session/window/pane index format keys for common inspection
  scripts.
- tmux-compatible pane targets resolve direct fake pane IDs, current
  pane/window markers, active-window pane indexes, `window.pane`,
  `:window.pane`, `session:window`, and `session:window.pane` forms.
- `cmux claude-teams`, `cmux omo`, `cmux omx`, and `cmux omc` wrappers now
  prepare per-integration tmux shim directories and launch the underlying
  agent command with shim-first `PATH`.
- WSL-launched integration wrappers resolve the underlying agent command inside
  the selected Linux distribution, convert AgentMux-managed Windows paths to
  WSL paths, and export `CMUX_EXE` so generated tmux shims call back through the
  preparing Windows binary.
- `cmux integrations setup/env <kind>` exposes the wrapper preparation path
  without launching the agent, and `omo` writes a shadow OpenCode config with
  `oh-my-opencode` registered and tmux enabled.
- `omc` setup writes a Node restore module and injects it through
  `NODE_OPTIONS` while preserving the user's original `NODE_OPTIONS` for child
  process inheritance.
- `cmux integrations install-shims` creates persistent `claude-teams`, `omo`,
  `omx`, and `omc` entrypoints under an AgentMux-managed bin directory and can
  write idempotent PowerShell or POSIX shell PATH blocks.
- `cmux integrations install-shims --user-path` can explicitly register that
  AgentMux-managed bin directory in the Windows user PATH.
- `cmux integrations doctor [kind]` reports wrapper, tmux shim, PATH,
  integration-specific config, restore-module, and underlying executable
  readiness without mutating user files.
- `cmux integrations doctor [kind] --distribution <name>` checks the selected
  WSL distribution, WSL-side agent executable resolution, and WSL-visible
  shim/config paths.
- `cmux integrations doctor omo` checks the shadow OpenCode plugin array and
  `oh-my-opencode` tmux enablement, so stale or manually edited shadow configs
  report `needs-attention`.
- OMO shadow config generation preserves common JSONC comments/formatting when
  adding `oh-my-opencode` and enabling `tmux.enabled`.
- `cmux omo` and `cmux integrations setup omo --install-packages` install
  `oh-my-opencode` inside the shadow OpenCode config using `bun` or `npm`
  without mutating the user's original config directory. When a WSL distribution
  is selected, installation runs through WSL `bun`/`npm` against the WSL-visible
  shadow config path.
- OMO install/setup reports whether shadow `node_modules` was already isolated
  or replaced from a symlink, and doctor flags a symlinked shadow
  `node_modules` path with a direct repair command.
- Integration wrappers export their identity, and tmux-created worker sessions
  are marked through `agent.set_state`, `sidebar.set_status`, and `sidebar.log`
  so native panes carry team/source metadata.
- Agent-team worker sessions preserve their integration telemetry and
  automatically become `completed` on clean terminal exit or `failed` on
  non-zero/backend failure exits.

Done when:

- A tmux-aware agent team creates native AgentMux panes, not hidden real tmux panes.
- Worker panes appear beside the main session with independent surface state and notification metadata.
- Wrapper commands pass through original agent arguments and can run inside the selected WSL distribution.

### Goal 15: Workspace Groups and Advanced Sidebar UX

Deliverables:

- Workspace group store schema: group ID, anchor workspace, membership, collapsed state, pinning, color, icon, placement.
- Sidebar multi-select, context menus, group header, group plus button, drag/reorder.
- Workspace color/icon/description editing.
- Group CLI commands and persistence.

Current slices:

- Workspace group persistence now records group metadata and ordered
  membership.
- The control plane exposes workspace-group list/create/update/delete plus
  add/remove workspace membership.
- `agentmux workspace group ...` and `agentmux workspace-group ...` provide
  CLI access to the same lifecycle.
- The sidebar renders persisted group headers and supports collapse/expand
  without changing the tab/pane ownership model.
- Closing a workspace cleans up its group membership and clears it as a group
  anchor.

Done when:

- Multiple workspaces can be grouped, collapsed, pinned, renamed, and restored after restart.
- New workspaces created while inside a group use the configured placement.
- Closing a group anchor follows an explicit confirmation policy.

### Goal 16: Browser Parity Expansion

Deliverables:

- Unify visible browser pane with CDP-controlled target.
- Add advanced command families for richer script targeting beyond evaluate, state mutation, and frame-targeted interactions.
- Browser focus mode and DevTools/console affordances.
- Browser diagnostics attached to the surface and workspace.

Done when:

- Automation and the visible pane operate on the same page instance.
- A browser script can complete a realistic login/form/navigation flow using only AgentMux commands.
- Browser failures produce actionable diagnostics without corrupting other surfaces.

### Goal 17: SSH Remote Workspace Parity

Deliverables:

- `agentmux ssh` CLI command and desktop open-SSH workflow.
- Windows URL protocol/deeplink handler for SSH and prompt/rules equivalents.
- Remote relay daemon or transport layer for browser proxying, remote CLI relay, and persistent PTY reconnection.
- Remote localhost browser routing.
- Drag/drop file upload through SSH.
- Host-level reconnect/backoff and notification cooldown.

Implemented so far:

- `agentmux ssh user@host[:port]` and `cmux ssh user@host[:port]` create an
  SSH terminal session in the active or specified workspace.
- `agentmux ssh --profile <name-or-id>` and `cmux ssh --profile <name-or-id>`
  resolve saved desktop SSH profile metadata before spawning the session.

Done when:

- `agentmux ssh user@host` creates or selects a remote workspace according to
  the final SSH product workflow.
- A browser surface in that workspace resolves `localhost` against the remote host.
- Disconnect and reconnect preserve the remote session.
- Remote processes can send local AgentMux notifications.

### Goal 18: Dock, TextBox, and Agent-Facing Skills

Deliverables:

- Right-sidebar Dock terminal controls from project/global `dock.json`.
- Trust prompt for project Dock commands.
- TextBox composer for new terminals, splits, and restored state.
- AgentMux Codex skills for CLI control, browser automation, settings/config editing, diagnostics, and Dock setup.
- Installer or docs for skill installation.

Current slices:

- The React shell has a `terminal.textBox` UI action and default `Ctrl+Alt+I`
  shortcut that opens a bottom composer for the active terminal pane.
- TextBox submissions use the existing `session.send-text` path and are covered
  by targeted Playwright preview testing.
- TextBox drafts persist per terminal session in local storage until sent.
- `dock.get` loads Dock controls using AgentMux-first and cmux-compatible
  fallback paths: project `.agentmux/dock.json`, project `.cmux/dock.json`,
  global AgentMux `dock.json`, then global cmux `dock.json`.
- The desktop UI renders loaded Dock controls in a right-side panel and marks
  project-sourced Dock files as requiring review before future execution.
- Project Dock execution now persists backend-auditable trust approval and launches each
  control as a Dock-owned WSL terminal slot with command, cwd, env, title, and
  control-id metadata.
- Dock terminal slots expose persisted per-control height controls layered over
  `dock.json` defaults.
- `skills/agentmux-control` packages AgentMux operating guidance for Codex with
  a reference covering CLI, browser automation, diagnostics, integrations, and
  Dock workflows.
- `tools/install-agentmux-skill.ps1` and `npm run skills:install` provide a
  repeatable local install path for the repo-packaged skill.

Done when:

- A project can commit `.agentmux/dock.json` and show reproducible right-sidebar controls.
- A user can compose prompt text before sending it to a terminal.
- Codex can learn and use AgentMux control workflows through installed skills.

## Recommended Next Slice

The fastest path toward the user's desired product is not to start with every cmux feature. The next slice should be:

1. Goal 10 setup/config foundation.
2. Goal 11 action registry plus shortcuts.
3. Goal 12 CLI/sidebar metadata compatibility.
4. Goal 14 tmux-compat wrapper for Codex and Claude first.

Reasoning:

- The setup/config layer prevents Windows-specific friction from leaking into every feature.
- The action registry unlocks custom commands, plus-button workflows, palette customization, and later Dock actions.
- CLI/sidebar metadata is the integration spine for agents and scripts.
- tmux-compat is the product-defining feature that makes multi-agent tools appear as native AgentMux panes instead of raw tmux panes.

## Open Decisions

- Product naming: keep `cmux.exe` as a compatibility alias only, or document it
  as a first-class Windows script interface beside `agentmux`.
- Config naming: `.agentmux/agentmux.json` remains the owned write path; keep
  `.cmux/cmux.json` as a compatible read/migration source rather than a write
  target unless broader cmux config ownership is explicitly chosen later.
- Shell scope: remain WSL-only for the default product path, or expose Windows-native terminal as a secondary explicit mode.
- Browser runtime: use Edge WebView2 for visible panes, CDP-launched Chrome/Edge, or a unified WebView2 automation path.
- Update channel: Tauri updater, winget, MSIX, or a custom installer update flow.
- Remote SSH architecture: minimal direct SSH first, or invest early in a relay daemon compatible with browser proxy and notification relay.

## Tracking Notes

- Existing Goals 0-9 should remain the MVP/release-candidate baseline.
- Goals 10-18 are cmux parity goals and should be tracked separately so MVP stabilization does not mix with broader product parity.
- Each goal should add or update status notes in this directory when implementation starts.
