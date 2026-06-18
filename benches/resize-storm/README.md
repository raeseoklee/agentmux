# Resize Storm Benchmark

Measures repeated terminal resize request latency and verifies that the session remains usable afterward.

Default scenario:

```powershell
cargo run -p agentmux-bench-resize-storm
```

Quick smoke scenario:

```powershell
cargo run -p agentmux-bench-resize-storm -- --iterations 5
```

The report includes resize p50/p95/p99, final requested size, post-storm echo latency, and queue pressure.
