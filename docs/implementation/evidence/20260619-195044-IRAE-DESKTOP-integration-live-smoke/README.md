# Integration Live Smoke

Generated: 2026-06-19T10:50:47.4107008Z

This smoke installs AgentMux/cmux integration wrappers and tmux shims into an
isolated evidence runtime directory, temporarily prepends that bin directory to
PATH only for doctor subprocesses, and records Windows plus WSL doctor results.

- Result: passed
- Foundation ready: True
- Underlying agents ready: False
- Base dir: D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\runtime\cmuxterm
- Bin dir: D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\runtime\cmuxterm\bin
- WSL distribution: Ubuntu

Use -RequireUnderlyingAgents to make missing claude, opencode, omx, or
omc executables fail the smoke.
