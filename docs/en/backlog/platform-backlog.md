# Platform Backlog

Status: Backlog

AgentMux is Windows-only for the current product line. The active product scope
is Windows 10/11, ConPTY, PowerShell, Command Prompt, WSL, and WSL tmux.

Native macOS and Linux desktop support is deferred. Do not treat these items as
release commitments until they are explicitly promoted into an active roadmap.

## Active Platform Scope

- Windows desktop application.
- Windows NSIS installer.
- GitHub Releases updater artifacts for Windows.
- Native Windows terminal hosting through ConPTY.
- WSL direct shells as a Windows-hosted Linux development environment.
- Durable WSL tmux sessions for agent workflows.
- Windows named-pipe control plane.

## Deferred Platform Work

| Item | Status | Promotion Criteria |
|---|---|---|
| Native macOS desktop app | Backlog | Dedicated owner, packaging plan, terminal backend decision, release and updater plan. |
| Native Linux desktop app | Backlog | Dedicated owner, packaging plan, PTY/session backend decision, desktop integration plan. |
| macOS updater channel | Backlog | Native macOS app promoted and signed distribution selected. |
| Linux updater channel | Backlog | Native Linux app promoted and package format selected. |
| Cross-platform CI release matrix | Backlog | At least one non-Windows platform promoted into active scope. |
| Non-Windows terminal backend parity | Backlog | Native PTY, session restore, clipboard, font rendering, and agent lifecycle parity plan. |

## Explicit Non-Goals For Current Releases

- Do not publish macOS or Linux desktop binaries.
- Do not advertise cross-platform desktop support.
- Do not add release blockers for native macOS or Linux behavior.
- Do not convert WSL support into a generic Linux desktop support claim.

## Documentation Rule

Operational documentation should describe AgentMux as Windows-only. If a future
document discusses macOS or Linux, it should link here and mark the work as
backlog unless the platform has been promoted by an explicit product decision.
