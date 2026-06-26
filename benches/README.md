# Benchmarks

Benchmark harnesses live here. Each binary prints JSON so release-gate runs can
be archived and compared.

## Required Benchmarks

```powershell
cargo run -p agentmux-bench-single-terminal-latency
cargo run -p agentmux-bench-many-idle-sessions
cargo run -p agentmux-bench-high-output
cargo run -p agentmux-bench-resize-storm
cargo run -p agentmux-bench-restart-recovery
```

## Smoke Runs

Use these smaller scenarios while developing:

```powershell
cargo run -p agentmux-bench-many-idle-sessions -- --sessions 1,2 --observe-ms 250
cargo run -p agentmux-bench-high-output -- --lines 100 --visible-probes 2
cargo run -p agentmux-bench-resize-storm -- --iterations 5
cargo run -p agentmux-bench-restart-recovery -- --sessions 2
```

The restart-recovery benchmark uses a simulated durable backend. It proves the
core durable attach path and duplicate-spawn accounting quickly; the release lab
must still run the real WSL/tmux recovery scenario before a release candidate can
pass.
