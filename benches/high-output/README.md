# High Output Benchmark

Measures hidden-output pressure while a visible session continues to receive latency probes.

Default scenario:

```powershell
cargo run -p agentmux-bench-high-output
```

Quick smoke scenario:

```powershell
cargo run -p agentmux-bench-high-output -- --lines 100 --visible-probes 2
```

The report includes hidden-output duration, visible probe p50/p95/p99, process samples, and queue pressure.
