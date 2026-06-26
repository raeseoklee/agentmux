# Real WSL/tmux Reattach Smoke Evidence: IRAE-DESKTOP

Date: 2026-06-18
Machine: IRAE-DESKTOP

## Scope

This smoke verifies the live durable WSL/tmux backend path on a real WSL
distribution with tmux installed:

1. Launch a tmux-control session inside WSL.
2. Round-trip terminal output and typed input through the control backend.
3. Soft-detach the first control client.
4. Reattach a second control client to the same tmux session.
5. Send input after reattach and verify the same shell process handles it.

## Result

The smoke passed on the `Ubuntu` WSL distribution with `tmux 3.2a`.

- Launch/input/output round trip: passed
- Reattach without duplicate shell process: passed
- Command: `cargo test -p agentmux-backend-tmux --test tmux_control_smoke -- --nocapture`
- Exit code: 0
- Post-run smoke tmux sessions: none left behind

Artifacts:

- `summary.json`
- `tmux-control-smoke.stdout.txt`
- `tmux-control-smoke.stderr.txt`
- `tmux-version.txt`
- `wsl-version.txt`
- `wsl-distributions.txt`
