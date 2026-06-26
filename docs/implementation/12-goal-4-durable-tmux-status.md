# Goal 4 Durable WSL tmux Backend Status

Status: Draft
Date: 2026-06-18

This document records the current implementation evidence for Goal 4: Durable WSL tmux Backend.

## Implemented

- `agentmux-backend-tmux` now has a byte-stream `TmuxControlParser` that buffers partial transport reads and emits complete control messages only after line boundaries.
- The parser preserves `%output` payloads as bytes and decodes tmux escaped payloads once, including octal escapes such as `\040` and `\012`.
- Command response lines between `%begin` and `%end` are correlated with the active tmux command id when the id is present in the control fields.
- The parser recognizes `%begin`, `%end`, `%error`, `%exit`, `%output`, `%window-add`, `%window-close`, `%pane-add`, `%pane-close`, `%pane-died`, `%pane-exited`, `%layout-change`, and `%session-changed`.
- Unknown and malformed control lines produce typed parser messages instead of panicking.
- The parser now skips ConPTY-injected terminal escape prefixes before tmux
  control lines, so `%output` lines remain visible even when WSL emits title or
  cursor control sequences around tmux output.
- Fixture files now cover a simple command response, escaped output payloads, and topology/lifecycle events.
- Initial durable tmux helper types and command builders exist for stable session names, tmux control launch, pane listing, literal input, and pane resizing.
- `TmuxControlBackend` now wraps a transport backend. The default transport is WSL Direct, so tmux control mode is launched inside the selected WSL distribution through the same ConPTY-backed WSL path as direct shells.
- `spawn` now builds a stable `agentmux_<workspace>` tmux session name, launches `tmux -C new-session -A -s <name> <command>`, returns `backend_kind = "wsl-tmux-control"`, and stores the tmux session name as the backend-native id.
- `attach` now launches `tmux -C attach-session -t <name>` through the WSL Direct transport and returns a tmux-control session handle.
- Basic input, resize, soft detach, interrupt, and kill operations are translated into tmux control commands and sent to the control transport.
- Kill termination targets the durable tmux session name rather than the
  currently active pane id, which prevents failed smoke runs from leaving stale
  tmux sessions behind.
- tmux `%output` lines are parsed into AgentMux backend output events, and pane add/death events update the active target or mark the session exited.
- The tmux-control backend now waits for tmux `%pane-add` or `%output` events to learn the active pane target instead of sending an eager startup `display-message` command.
- The desktop backend router now owns a `TmuxControlBackend`, routes `wsl-tmux-control` spawn requests to it, and drains tmux-control events with the other desktop backends.
- `SpawnRequest` now carries optional `workspace_id`, allowing durable backends to derive stable workspace-scoped backend names instead of inventing per-process names.
- The control plane now implements `session.attach`. It can preserve a provided `session_id`, attach a backend reference through the selected backend, and report the attached session through the normal `session.get` summary path.
- `SessionSummaryResult` now includes `backend_native_id`, so desktop persistence can store the tmux session name needed for restart recovery.
- Desktop startup now performs best-effort durable recovery: persisted `recovering` sessions with `backend_kind = "wsl-tmux-control"` and a saved backend-native id are attached through `session.attach`. Successful attaches are persisted back as live session state; failed attaches leave the row in `recovering` for a later retry.

## Not Yet Implemented

- Durable close policies still cannot kill or detach external tmux sessions through the desktop host.
- Output history and snapshot cursors are not yet proven end to end.
- The real WSL/tmux launch/input/output and reattach/no-duplicate-process
  smoke tests remain gated behind explicit environment variables because they
  require a Windows machine with WSL and tmux installed.

## Verification Evidence

The following targeted command passed on 2026-06-18 using a repository-local Rust toolchain under `.toolchains`:

```powershell
cargo test -p agentmux-backend-tmux -- --nocapture
```

The tmux backend tests covered:

- output line parsing into byte payloads
- escaped output decoding
- partial line buffering across transport reads
- command response correlation with `%begin` ids
- fixture-driven simple command response parsing
- fixture-driven escaped output parsing
- fixture-driven topology event parsing
- unknown line tolerance
- tmux command builders using argument arrays
- tmux-control spawn translation into WSL Direct transport requests
- tmux-control attach translation into WSL Direct transport requests
- input, resize, soft detach, interrupt, and kill command generation
- tmux output parsing into backend output events and pane target tracking
- default tmux smoke tests remain registered but skip live WSL/tmux execution unless explicitly enabled
- desktop router dispatch of `wsl-tmux-control` spawn requests
- control-plane `session.attach` preserving an existing session id and backend-native id
- persistence of backend-native ids for future recovery
- startup recovery candidate selection for persisted durable tmux sessions
- backend kind reporting `wsl-tmux-control`
- ConPTY escape-prefix tolerance before tmux control lines

The live WSL/tmux smoke runner passed on `IRAE-DESKTOP` with the `Ubuntu`
distribution and `tmux 3.2a`:

```powershell
npm run tmux:reattach-smoke
```

Evidence:

- [summary.json](./evidence/20260618-222707-IRAE-DESKTOP-real-tmux-reattach-smoke/summary.json)
- [summary](./evidence/20260618-222707-IRAE-DESKTOP-real-tmux-reattach-smoke/README.md)

## Remaining Gap

Goal 4 now has parser, command-builder, transport-backed spawn/attach, desktop routing, basic control command translation, persisted backend-native ids, best-effort startup attach for persisted durable sessions, and real WSL/tmux launch plus reattach evidence. The remaining implementation gap is coordinating durable close policies with external tmux sessions and expanding output history/snapshot behavior beyond the live control smoke.
