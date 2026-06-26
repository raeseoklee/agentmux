# Goal 14 - tmux-Compat and Multi-Agent Integration Status

Status: Implemented first through twenty-first compatibility slices
Date: 2026-06-19

This document records the first implementation slices for Goal 14 from the cmux
Windows parity analysis: exposing enough tmux-shaped context for tools that
expect `TMUX`/`TMUX_PANE`, and translating a practical subset of tmux commands
into native AgentMux pane/session control.

## Implemented

- Managed terminal sessions now receive pane-scoped compatibility variables:
  - `AGENTMUX_PANE_ID`
  - `CMUX_PANE_ID`
  - `TMUX`
  - `TMUX_PANE`
- Spawn preparation resolves the AgentMux pane before launching the process.
  New-tab spawns receive the same pane ID that is later persisted for the new
  top-level pane.
- `WSLENV` now forwards pane and fake tmux identity variables into WSL
  processes.
- Added the `agentmux __tmux-compat` shim target for a growing command subset:
  - `display-message`
  - `capture-pane`
  - `kill-pane`
  - `kill-window`
  - `list-panes`
  - `list-sessions`
  - `list-windows`
  - `new-session`
  - `new-window`
  - `rename-session`
  - `rename-window`
  - `select-pane`
  - `select-window`
  - `send-keys`
  - `split-window`
  - `switch-client`
  - `has-session`
- The shim accepts tmux-style pane targets such as `%pane_123` and maps them
  back to AgentMux pane IDs.
- `split-window` translates to `pane.split` and can attach a provided command
  to the newly-created empty pane through `session.spawn`.
- `new-window` translates to `session.spawn` with `new_tab` placement when an
  explicit command is supplied.
- `new-session` creates a new AgentMux workspace and can attach an explicit
  first command to that workspace's root pane.
- `split-window`, `new-window`, and `new-session` without an explicit command
  now launch a default shell for the current backend context: WSL backends use
  `bash`, while ConPTY uses the Windows command shell.
- When an integration wrapper calls `split-window`, `new-window`, or
  `new-session` with a command, AgentMux records best-effort team metadata for
  the spawned worker session through `agent.set_state`, `sidebar.set_status`,
  and `sidebar.log`.
- Agent team worker sessions marked with `AgentTelemetry.activity=agent_team`
  now receive automatic lifecycle updates when their terminal session reaches a
  terminal state: clean exits become `completed`, non-zero exits and backend
  failures become `failed`, and the original telemetry is preserved for UI and
  sidebar persistence.
- `select-window` maps a tmux window target to an AgentMux top tab/root pane and
  focuses the first leaf pane in that root.
- `switch-client` accepts tmux session/window targets, resolves AgentMux
  workspaces by ID or name, and focuses the target window's first leaf pane.
- `rename-window` and `rename-session` map tmux naming operations to
  `workspace.rename`, since the current AgentMux tmux session/window boundary is
  represented by the workspace/top-tab model.
- `kill-window` maps a tmux window target to an AgentMux top tab/root pane and
  closes the mounted surface, reusing the existing tab-close subtree cleanup.
- `split-window` and `new-window` accept `-P -F <format>` and render the same
  pane-format helper used by `display-message`.
- `send-keys` resolves the pane's mounted terminal session and translates
  common tmux key names such as `Enter`, `Tab`, `Escape`, arrows, and
  `Backspace`.
- `capture-pane` reads recent terminal output through `session.read_recent`.
- `kill-pane` closes the native AgentMux pane and its mounted surface through
  `pane.close`.
- `list-windows` exposes top-level AgentMux root panes as tmux-shaped windows.
- `list-sessions` exposes AgentMux workspaces as tmux-shaped sessions, and
  `has-session` resolves workspace/session and optional window targets by ID or
  name.
- `list-panes -a` and `list-windows -a` now enumerate every AgentMux workspace
  as tmux-shaped sessions, with pane/window format keys including
  `session_id`, `session_name`, `window_index`, `window_id`, `window_name`, and
  `pane_index`.
- Pane target resolution now understands more tmux-shaped target grammar:
  direct fake pane IDs, `.` and `!` for the current pane/window, pane indexes
  within the active window, `window.pane`, `:window.pane`, `session:window`,
  and `session:window.pane`. Session targets resolve to AgentMux workspaces by
  ID or name, and window/pane indexes resolve through the native top-tab/root
  pane tree.
- Added cmux-style agent integration wrapper entries:
  - `cmux claude-teams ...`
  - `cmux omo ...`
  - `cmux omx ...`
  - `cmux omc ...`
- Added `cmux integrations setup <kind>` and `cmux integrations env <kind>` for
  inspecting or preparing integration environments without launching the agent.
- Integration setup creates both POSIX and Windows tmux shims:
  - `<base>/<kind>-bin/tmux`
  - `<base>/<kind>-bin/tmux.cmd`
- Generated tmux shims prefer `CMUX_EXE` when present so WSL-launched agents
  can call back into the exact Windows `cmux.exe` that prepared the wrapper.
- Wrapper runtime prepends the shim directory to `PATH` and exports
  `AGENTMUX_*`, `CMUX_*`, `TMUX`, and `TMUX_PANE` variables for the child
  process.
- Wrapper runtime also exports `AGENTMUX_AGENT_INTEGRATION` and
  `CMUX_AGENT_INTEGRATION` so tmux-compat commands can attribute native worker
  panes to the integration that requested them.
- `claude-teams` enables `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` before
  launching `claude`.
- `omo` creates a non-mutating shadow OpenCode config under
  `<base>/omo-config`, adds `oh-my-opencode` to the shadow `plugin` array, and
  writes `oh-my-opencode.json` with `tmux.enabled` set to `true`.
- OMO shadow config merging now preserves common JSONC source formatting and
  comments for `opencode.json` and `oh-my-opencode.json` while adding
  `oh-my-opencode` and enabling `tmux.enabled`.
- `omo` launch now ensures `oh-my-opencode` is installed in the shadow config
  directory before launching OpenCode. If a WSL distribution is selected through
  explicit setup options or AgentMux/cmux WSL environment variables, the
  installer runs inside that WSL distribution; otherwise it uses the current
  Windows shell. In both cases it prefers `bun add oh-my-opencode` and falls
  back to `npm install oh-my-opencode --save`.
- `cmux integrations setup omo --install-packages` exposes the same package
  installation path without launching OpenCode, and accepts
  `--distribution <name>` for WSL-side package installation.
- Package installation is scoped to `<base>/omo-config`: if the shadow
  `node_modules` path is a symlink to the user's original OpenCode
  `node_modules`, AgentMux removes the shadow symlink and creates a local
  shadow `node_modules` directory before installing the package.
- OMO package setup now reports whether shadow `node_modules` was already
  isolated or whether a symlink was replaced, and `cmux integrations doctor omo`
  surfaces a dedicated `omo-node-modules-isolated` check with a targeted fix.
- `omc` setup now writes `<base>/omc-restore-node-options.cjs` and wrapper
  runtime injects it with `NODE_OPTIONS`, while preserving the user's original
  `NODE_OPTIONS` in `AGENTMUX_ORIGINAL_NODE_OPTIONS`/`CMUX_ORIGINAL_NODE_OPTIONS`
  so the module can restore child-process inheritance after startup.
- Added `cmux integrations install-shims` to create persistent integration
  entrypoints without launching an agent.
- The installer writes both POSIX and Windows command launchers under
  `<base>/bin`:
  - `claude-teams` and `claude-teams.cmd`
  - `omo` and `omo.cmd`
  - `omx` and `omx.cmd`
  - `omc` and `omc.cmd`
- The persistent launchers call the cmux-compatible wrapper commands while the
  per-integration setup still owns each isolated tmux shim directory.
- The installer writes reusable PATH snippets:
  - `<base>/agentmux-integrations.ps1`
  - `<base>/agentmux-integrations.sh`
- Optional `--powershell-profile` and `--shell-profile` arguments update user
  profiles through an idempotent managed block.
- `cmux integrations install-shims --user-path` can explicitly add the
  AgentMux-managed integration bin directory to the Windows user PATH through
  `HKCU\Environment\Path`. The default remains non-mutating beyond wrapper and
  snippet files.
- Added `cmux integrations doctor [kind]` to inspect integration readiness
  without mutating user files. The doctor checks wrapper files, tmux shim files,
  generated shadow config or restore-module files, PATH visibility, and the
  current shell's underlying agent executable availability.
- Doctor output supports `--json` for automation and text output for quick
  troubleshooting.
- Doctor now reports the `omo-package` state so users can see whether
  `oh-my-opencode` is present or whether `bun`/`npm` is available to install it.
- Doctor now validates OMO shadow config contents, not just file presence:
  `opencode.json` must include `oh-my-opencode` in the `plugin` array, and
  `oh-my-opencode.json` must have `tmux.enabled=true`.
- `cmux integrations doctor [kind] --distribution <name>` now validates the
  selected WSL execution context: WSL distribution reachability, WSL-side
  underlying agent executable resolution, WSL-visible tmux shim files, and
  WSL-visible integration config or restore-module files.
- When wrapper commands are launched from WSL or with an explicit
  `AGENTMUX_WSL_DISTRIBUTION`/`CMUX_WSL_DISTRIBUTION`, AgentMux now relaunches
  the underlying agent through `wsl.exe --distribution <name> --exec` instead
  of resolving `claude`, `opencode`, `omx`, or `omc` as Windows binaries.
- The WSL launcher converts AgentMux-managed Windows paths to `/mnt/<drive>/...`
  values, prepends the WSL-visible tmux shim directory to Linux `PATH`, and
  exports `CMUX_EXE`/`AGENTMUX_EXE` for the generated tmux shim callback.
- `cmux integrations env <kind> --distribution <name>` now reports the WSL
  command shape that would be used for the wrapper instead of silently
  returning Windows-only environment values.

## Verification

- CLI parser coverage verifies `__tmux-compat split-window`, `send-keys`,
  `capture-pane`, `kill-pane`, `kill-window`, `new-window`, `new-session`,
  `rename-window`, `rename-session`, `select-window`, `switch-client`,
  `list-sessions`, `has-session`, and all-workspace `list-panes`/`list-windows`
  command shapes.
- CLI helper coverage verifies fake tmux pane ID rendering, session/window
  format rendering, pane index/window index format rendering, and key mapping.
- CLI helper coverage verifies tmux-shaped pane target splitting and resolution
  for current pane/window markers, window indexes, pane indexes, and
  `session:window.pane` grammar.
- CLI helper coverage verifies default-shell command selection for WSL and
  ConPTY tmux-compat contexts.
- CLI helper coverage verifies generated agent-team metadata for native worker
  sessions created through tmux-compat flows.
- Core control-plane coverage verifies agent-team worker session lifecycle
  transitions from terminal session state to `completed` and `failed` agent
  states, including event emission, notification creation, attention state, and
  telemetry preservation.
- CLI integration coverage verifies `integrations setup` parsing, shim file
  creation, and `omo` shadow config generation without modifying the original
  OpenCode config directory.
- CLI integration coverage verifies JSONC-style OMO source configs keep
  comments while the generated shadow config adds `oh-my-opencode` and flips
  `tmux.enabled` to `true`.
- CLI integration coverage verifies `omc` restore module generation and
  `NODE_OPTIONS` composition, including paths with spaces.
- CLI integration coverage verifies `integrations install-shims` parsing,
  wrapper generation, PATH snippet generation, and idempotent managed profile
  block updates.
- CLI helper coverage verifies Windows user PATH append behavior, duplicate
  detection, and registry query parsing without mutating the real registry.
- CLI integration coverage verifies doctor parsing, JSON output, PATH
  detection, ready-state reporting for an installed `omo` shape, and
  needs-attention reporting when OMO shadow config content is stale or broken.
- CLI integration coverage verifies the `omo` package installer using a fake
  package manager and confirms installation stays inside the shadow config
  directory.
- CLI integration coverage verifies a symlinked OMO shadow `node_modules`
  directory is reported by doctor, replaced before install, and reflected in
  package setup output metadata.
- CLI helper coverage verifies the WSL OMO package-install command shape,
  Windows-to-WSL shadow config conversion, and `bun`/`npm` fallback script.
- CLI integration coverage verifies generated tmux shims honor `CMUX_EXE`.
- CLI helper coverage verifies the WSL wrapper command shape, Windows-to-WSL
  path conversion, Linux `PATH` prepend behavior, and underlying agent argument
  forwarding.
- CLI integration coverage verifies `integrations doctor --distribution`
  parsing and the WSL doctor command argument shape without requiring a local
  WSL distribution in unit tests.
- Local WSL doctor smoke verified that
  `cmux integrations doctor omo --distribution Ubuntu --json` reaches the
  Ubuntu distribution and reports missing WSL-side `opencode`, tmux shim, and
  shadow config with actionable fix text.
- Desktop-host ConPTY smoke coverage verifies spawned terminal processes see
  surface, pane, `TMUX`, and `TMUX_PANE` environment variables.
- Release readiness audit evidence at
  `docs/implementation/evidence/20260619-202822-IRAE-DESKTOP-release-readiness-audit`
  ran non-mutating integration doctor checks for `claude-teams`, `omo`, `omx`,
  and `omc` on Windows and against the local Ubuntu WSL distribution. The audit
  confirms WSL/tmux is reachable on the current machine and records current
  needs-attention states for wrapper/shim/PATH, installed CLI sidecars,
  installed-directory user PATH registration, or underlying-agent readiness.
- WSL state gate evidence at
  `docs/implementation/evidence/20260619-195904-IRAE-DESKTOP-wsl-state-gate`
  explicitly classifies the current machine as `wsl_with_tmux`, records the
  reachable distributions, and captures the selected Ubuntu tmux version.
- Integration live smoke evidence at
  `docs/implementation/evidence/20260619-195044-IRAE-DESKTOP-integration-live-smoke`
  verifies isolated wrapper/shim installation plus Windows and Ubuntu WSL
  doctor foundation checks. It temporarily prepends only the evidence runtime
  bin directory for doctor subprocesses and does not mutate the user PATH or
  profile files. Current installed-agent readiness is blocked only by missing
  `opencode` on Windows and Ubuntu WSL PATH; `claude`, `omx`, and `omc`
  underlying checks passed in this smoke.

## Remaining Goal 14 Work

- This is not a full tmux command replacement. Remaining gaps include the rest
  of tmux target grammar, richer layout commands, hooks/buffers, and broader
  format expansion.
- Wrapper launch and doctor are WSL-aware, but the full end-to-end flow still
  needs live launch validation with `-RequireUnderlyingAgents` after
  OpenCode/oh-my-opencode is available as `opencode` on Windows and Ubuntu WSL
  PATH.
- Agent team metadata and terminal lifecycle now cover basic running,
  completed, and failed states. Fine-grained attention/progress still depends on
  underlying agent tools emitting explicit AgentMux markers or tmux-compatible
  status updates.
- `omo` has shadow config generation, common JSONC-preserving merges, content
  diagnostics, Windows/WSL package installation, symlink fallback reporting,
  and isolated doctor foundation validation. Live validation against current
  OpenCode/oh-my-opencode releases remains because `opencode` is not currently
  available on Windows or Ubuntu WSL PATH.
- `omc` has first-slice `NODE_OPTIONS` restore module support, but it still
  needs live validation against installed Claude Code and OMC versions.
- Persistent shim installation now exists as a CLI flow, including explicit
  user PATH registration. Remaining work is packaging documentation,
  clean-machine `wsl_exe_missing`, `no_wsl_distribution`, and
  `wsl_without_tmux` validation, and deciding whether the Windows installer
  should invoke the same PATH registration flow automatically.
