# Single Terminal Latency Benchmark

Measures startup to first prompt, keypress to echoed byte, keypress to rendered frame, and resize latency for a single visible terminal.

Current probe:

```powershell
cargo run -p agentmux-bench-single-terminal-latency
```

The probe launches a single ConPTY-backed `cmd.exe`, waits for a deterministic
prompt, sends an `echo` command through the control envelope, measures the time
until the output marker is readable, measures a resize request, terminates the
session, and prints JSON metrics with queue pressure and process samples.
