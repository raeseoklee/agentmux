# Many Idle Sessions Benchmark

Measures ConPTY session startup, prompt readiness, idle control-loop drain cost, queue pressure, and process resource samples for many mostly idle terminals.

Default release-candidate scenarios:

```powershell
cargo run -p agentmux-bench-many-idle-sessions
```

Quick smoke scenario:

```powershell
cargo run -p agentmux-bench-many-idle-sessions -- --sessions 1,2 --observe-ms 250
```

The default session counts are 20 and 50 to match `PERF-001` and `PERF-002`.
