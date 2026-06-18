# Restart Recovery Benchmark

Measures the durable attach path used during restart recovery without requiring a live WSL/tmux lab.

Default scenario:

```powershell
cargo run -p agentmux-bench-restart-recovery
```

Quick smoke scenario:

```powershell
cargo run -p agentmux-bench-restart-recovery -- --sessions 2
```

This benchmark uses a simulated durable backend to measure attach latency, recovered-output visibility, and duplicate-spawn detection. The release lab still needs a real WSL/tmux run before this can become the final release gate.
