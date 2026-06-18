# Performance Gate Evidence: IRAE-DESKTOP

Date: 2026-06-18
Machine: IRAE-DESKTOP
Mode: full performance gate run

## Reference Profile

- Windows: Microsoft Windows 11 Pro for Workstations 10.0.22631
- CPU: 13th Gen Intel(R) Core(TM) i9-13900K, 24 cores, 32 logical processors
- RAM: 68493266944 bytes
- WSL version values: 2.7.8.0, 6.18.33.1-1, 1.0.73.2, 1.2.6676,
  1.611.1-81528511, 10.0.26100.1-240331, 10.0.22631.6199
- WSL distributions: Ubuntu, Nvidia_SDKM_Ubuntu_22.04_JetPack_6.2.2,
  Nvidia_SDKM_Ubuntu_22.04_JetPack_6.2.1

See `reference-profile.json` for the full captured profile.

## Results

All benchmark commands exited with code 0. See `manifest.json` for exact
commands, elapsed wall time, and artifact names.

The browser CDP fixture smoke also passed on the same machine. See
`browser-cdp-smoke.txt`.

Key values:

- `bench_single_terminal_latency`: startup to prompt 50.9104 ms, command
  round trip 20.6578 ms, resize request 0.0300 ms.
- `bench_many_idle_sessions`: 20-session idle control-loop p95 0.0176 ms;
  50-session idle control-loop p95 0.0290 ms; dropped events 0 in both
  scenarios.
- `bench_high_output`: 5000 hidden-output lines, visible probe p95 21.0679 ms,
  dropped events 0.
- `bench_resize_storm`: 200 resize requests, resize p95 0.0100 ms,
  post-storm echo 10.6617 ms.
- `bench_restart_recovery`: simulated durable attach for 5 sessions,
  duplicate backend refs 0.

## Remaining Release Evidence

This run records the automated benchmark evidence and browser CDP fixture smoke.
Separate release evidence now exists for packaged diagnostics export and the
real WSL/tmux reattach smoke. Final release signoff still needs manual
installer smoke and the remaining known-blocker audit entries.
