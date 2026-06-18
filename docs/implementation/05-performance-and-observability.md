# Performance and Observability

Status: Draft
Date: 2026-06-18

AgentMux는 여러 AI agent session을 동시에 다루는 제품이다. 성능은 부가 최적화가 아니라 architecture requirement다. 이 문서는 성능 예산, 측정 방법, 관측성, backpressure 정책을 정의한다.

## Reference Performance Targets

| ID | Target | Measurement |
|---|---:|---|
| PERF-001 | 20 idle terminal sessions without sustained UI jank | local Windows benchmark |
| PERF-002 | 50 idle terminal sessions with bounded memory growth | local stress benchmark |
| PERF-003 | p95 visible keystroke-to-echo below 50 ms in one-pane workspace | latency probe |
| PERF-004 | p95 visible keystroke-to-frame below 80 ms with eight mounted panes | latency probe |
| PERF-005 | workspace switch below 100 ms p95 excluding backend reconnect | UI benchmark |
| PERF-006 | startup to first usable terminal below 2 seconds target | startup trace |
| PERF-007 | hidden panes produce no continuous renderer work | renderer lifecycle probe |

These targets should be measured on a named reference Windows laptop profile. The profile must record CPU, RAM, Windows version, WSL version, display scale, and power mode.

## Performance Principles

- Input path has priority over output path.
- Visible output has priority over hidden output.
- Hidden terminal renderers are unmounted.
- Backend output is batched before IPC and before renderer writes.
- Scrollback memory is bounded per session.
- Resize events are coalesced.
- Metrics are collected from Phase 1 onward.

## Data Path Budget

Input path:

```text
keyboard event -> UI adapter -> IPC request -> core dispatch -> backend write -> backend output -> core batch -> UI render
```

Budget focus:

- UI event handler must not perform heavy synchronous work.
- IPC send should be non-blocking from the renderer's perspective.
- Core dispatch must avoid waiting behind output flush.
- Backend write queue must be small and prioritized.

Output path:

```text
backend read -> parser/decoder -> session buffer -> event batch -> IPC stream -> renderer write
```

Budget focus:

- Backend read loops never call persistence synchronously.
- Output batches are size- and time-bounded.
- Visible sessions get shorter batch intervals.
- Hidden sessions use ring buffer and snapshots.

## Queue Policy

Recommended queues:

| Queue | Capacity | Overflow behavior |
|---|---:|---|
| `input_commands` | small | return error if session detached; otherwise wait briefly |
| `resize_commands` | one latest per session | replace old size |
| `visible_output` | medium | batch and throttle frame writes |
| `hidden_output` | bounded per session | drop oldest rendered delta, mark truncation |
| `diagnostics` | bounded global | sample and increment dropped count |
| `persistence` | bounded | compact writes; never block input path indefinitely |

Every overflow path must be observable.

## Benchmarks

### `bench_single_terminal_latency`

Measures:

- startup to shell prompt
- keypress to echoed byte
- keypress to rendered frame
- resize latency

Backends:

- `conpty`
- `wsl-direct`
- `wsl-tmux-control`

### `bench_many_idle_sessions`

Measures:

- CPU idle percentage
- memory per session
- handle count
- task count
- workspace switch latency

Scenarios:

- 5 sessions
- 20 sessions
- 50 sessions

### `bench_high_output`

Measures:

- output throughput
- UI frame stability
- dropped hidden output count
- visible session latency while another session prints heavily

Scenario:

- one visible shell waiting for input
- one hidden shell generating continuous output
- several idle sessions

### `bench_resize_storm`

Measures:

- resize event coalescing
- backend resize calls
- UI frame drops
- final terminal size correctness

### `bench_restart_recovery`

Measures:

- shutdown/detach time
- restart to layout restored
- restart to durable session output visible
- duplicate process detection

## Instrumentation

Use structured spans and counters.

Required spans:

- `app.startup`
- `core.startup`
- `ipc.request`
- `session.spawn`
- `session.attach`
- `session.input`
- `session.resize`
- `backend.read`
- `output.batch`
- `renderer.write`
- `workspace.switch`
- `recovery.attach`

Required counters:

- active workspaces
- active panes
- active surfaces
- active sessions
- active backend attachments
- bytes read per session
- bytes rendered per visible surface
- hidden bytes buffered
- output batches sent
- dropped output batches
- IPC request count by method
- IPC error count by code
- backend reconnect count

Required gauges:

- process RSS
- CPU usage
- queue depth by queue
- renderer mounted count
- hidden surface count
- per-session scrollback bytes

## Diagnostics UI

Diagnostics panel should show:

- active backend attachments
- backend health
- queue pressure
- dropped output count
- recent typed errors
- last recovery attempt
- per-session output rate
- renderer mounted/unmounted state

Diagnostics must be useful to developers without attaching a debugger.

## Logging Policy

Rules:

- Logs must be structured.
- Secrets in env, command args, URLs, and output-derived tokens must be redacted where detected.
- High-volume terminal output is not logged by default.
- User can export diagnostics bundle with metadata, not raw unlimited terminal content.
- Benchmark logs include machine profile and app version.

## Optimization Boundaries

Do early:

- bounded queues
- renderer lifecycle management
- output batching
- resize coalescing
- per-session scrollback limit
- benchmark harness

Do later only if measurement requires:

- custom GPU terminal renderer
- native parser micro-optimizations
- file-backed scrollback segments
- shared memory data plane
- backend process pooling

## Release Performance Gate

A release candidate cannot pass if:

- 20 idle session benchmark shows sustained jank.
- p95 visible input latency exceeds budget on reference hardware.
- hidden output can freeze visible input.
- memory grows without bound during 30-minute idle session run.
- restart recovery creates duplicate durable sessions.

