use std::collections::HashSet;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use agentmux_backend::{
    AttachRequest, BackendEvent, BackendKind, BackendResult, InputEvent, SessionBackend,
    SessionHandle, SpawnRequest, TerminalSize, TerminationMode,
};
use agentmux_bench_support::{
    duration_stats, elapsed_ms, invoke, main_exit, queue_snapshot, request, BenchError,
    BenchResult, DurationStats, QueueSnapshot, TOKEN,
};
use agentmux_core::{RuntimeControlPlane, TerminalRuntime};
use serde::Serialize;

fn main() -> ExitCode {
    main_exit(run_benchmark)
}

#[derive(Serialize)]
struct RestartRecoveryReport {
    benchmark: &'static str,
    backend_kind: &'static str,
    recovery_mode: &'static str,
    durable_session_count: usize,
    restart_to_attach_all_ms: f64,
    attach_latency_stats: DurationStats,
    recovered_output_latency_stats: DurationStats,
    backend_spawns_before_restart: usize,
    backend_spawns_after_recovery: usize,
    backend_attach_count: usize,
    duplicate_backend_ref_count: usize,
    queue_after_recovery: QueueSnapshot,
}

fn run_benchmark() -> BenchResult<RestartRecoveryReport> {
    let options = Options::parse()?;
    let state = Arc::new(Mutex::new(RecoveryState::default()));
    let mut initial = RuntimeControlPlane::new(
        TerminalRuntime::new(RecoveryBackend::new(Arc::clone(&state))),
        TOKEN,
    );
    let mut backend_refs = Vec::with_capacity(options.session_count);

    for index in 0..options.session_count {
        let params = serde_json::json!({
            "workspace_id": "ws_restart_recovery",
            "backend": "wsl-tmux-control",
            "backend_profile": "bench",
            "command": ["agentmux-recovery-bench", index.to_string()],
            "cwd": null,
            "columns": 120,
            "rows": 30,
            "durability": "durable"
        });
        let spawn: serde_json::Value = invoke(
            &mut initial,
            "bench_spawn",
            "session.spawn",
            &params.to_string(),
        )?;
        let session_id = spawn
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| BenchError::new("session.spawn did not return session_id"))?;
        initial.collect_events();
        let summary: serde_json::Value = invoke(
            &mut initial,
            "bench_get",
            "session.get",
            &serde_json::json!({"session_id": session_id}).to_string(),
        )?;
        let backend_ref = summary
            .get("backend_native_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| BenchError::new("session.get did not return backend_native_id"))?;
        backend_refs.push(backend_ref.to_string());
    }

    let backend_spawns_before_restart = state.lock().map(|state| state.spawn_count).unwrap_or(0);
    drop(initial);

    let mut recovered = RuntimeControlPlane::new(
        TerminalRuntime::new(RecoveryBackend::new(Arc::clone(&state))),
        TOKEN,
    );
    let restart_started = Instant::now();
    let mut attach_samples = Vec::with_capacity(backend_refs.len());
    let mut recovered_session_ids = Vec::with_capacity(backend_refs.len());

    for (index, backend_ref) in backend_refs.iter().enumerate() {
        let attach_started = Instant::now();
        let params = serde_json::json!({
            "session_id": format!("ses_recovered_{index:03}"),
            "workspace_id": "ws_restart_recovery",
            "backend": "wsl-tmux-control",
            "backend_profile": "bench",
            "backend_ref": backend_ref,
            "columns": 120,
            "rows": 30,
            "durability": "durable"
        });
        let attach: serde_json::Value = invoke(
            &mut recovered,
            "bench_attach",
            "session.attach",
            &params.to_string(),
        )?;
        attach_samples.push(attach_started.elapsed());
        let session_id = attach
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| BenchError::new("session.attach did not return session_id"))?;
        recovered_session_ids.push(session_id.to_string());
    }

    recovered.collect_events();
    let restart_to_attach_all = restart_started.elapsed();
    let mut output_samples = Vec::with_capacity(recovered_session_ids.len());

    for (index, session_id) in recovered_session_ids.iter().enumerate() {
        let marker = format!("agentmux-recovered-output-{index:03}");
        let started = Instant::now();
        let response = recovered.handle_request(request(
            "bench_send",
            "session.send_text",
            &serde_json::json!({"session_id": session_id, "text": marker}).to_string(),
        ));
        agentmux_bench_support::expect_ok(&response)?;
        wait_for_text(&mut recovered, session_id, &marker, Duration::from_secs(5))?;
        output_samples.push(started.elapsed());
    }

    let snapshot = state.lock().map(|state| state.clone()).unwrap_or_default();
    let duplicate_backend_ref_count = duplicate_count(&snapshot.attached_backend_refs);
    let queue_after_recovery = queue_snapshot(&recovered);

    Ok(RestartRecoveryReport {
        benchmark: "bench_restart_recovery",
        backend_kind: "wsl-tmux-control",
        recovery_mode: "simulated durable attach",
        durable_session_count: options.session_count,
        restart_to_attach_all_ms: elapsed_ms(restart_to_attach_all),
        attach_latency_stats: duration_stats(&attach_samples)?,
        recovered_output_latency_stats: duration_stats(&output_samples)?,
        backend_spawns_before_restart,
        backend_spawns_after_recovery: snapshot.spawn_count,
        backend_attach_count: snapshot.attach_count,
        duplicate_backend_ref_count,
        queue_after_recovery,
    })
}

fn wait_for_text(
    control: &mut RuntimeControlPlane<RecoveryBackend>,
    session_id: &str,
    marker: &str,
    timeout: Duration,
) -> BenchResult<()> {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        control.collect_events();
        let recent: serde_json::Value = invoke(
            control,
            "bench_recent",
            "session.read_recent",
            &serde_json::json!({"session_id": session_id, "max_bytes": 65_536}).to_string(),
        )?;
        if recent
            .get("text")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|text| text.contains(marker))
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }

    Err(BenchError::new(format!(
        "timed out waiting for recovered output marker {marker}"
    )))
}

fn duplicate_count(values: &[String]) -> usize {
    let mut seen = HashSet::new();
    values.iter().filter(|value| !seen.insert(*value)).count()
}

#[derive(Clone, Default)]
struct RecoveryState {
    spawn_count: usize,
    attach_count: usize,
    attached_backend_refs: Vec<String>,
}

struct RecoveryBackend {
    state: Arc<Mutex<RecoveryState>>,
    events: Vec<BackendEvent>,
}

impl RecoveryBackend {
    fn new(state: Arc<Mutex<RecoveryState>>) -> Self {
        Self {
            state,
            events: Vec::new(),
        }
    }
}

impl SessionBackend for RecoveryBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::WslTmuxControl
    }

    fn spawn(&mut self, request: SpawnRequest) -> BackendResult<SessionHandle> {
        let backend_ref = format!("tmux-ref-{}", request.session_id);
        if let Ok(mut state) = self.state.lock() {
            state.spawn_count += 1;
        }
        self.events.push(BackendEvent::Started {
            session_id: request.session_id.clone(),
        });
        Ok(SessionHandle {
            session_id: request.session_id,
            backend_kind: BackendKind::WslTmuxControl,
            backend_native_id: Some(backend_ref),
            transport_pid: None,
        })
    }

    fn attach(&mut self, request: AttachRequest) -> BackendResult<SessionHandle> {
        if let Ok(mut state) = self.state.lock() {
            state.attach_count += 1;
            state
                .attached_backend_refs
                .push(request.backend_ref.clone());
        }
        self.events.push(BackendEvent::Started {
            session_id: request.session_id.clone(),
        });
        Ok(SessionHandle {
            session_id: request.session_id,
            backend_kind: BackendKind::WslTmuxControl,
            backend_native_id: Some(request.backend_ref),
            transport_pid: None,
        })
    }

    fn send_input(&mut self, session_id: &str, input: InputEvent) -> BackendResult<()> {
        let text = match input {
            InputEvent::Text(text) => text,
            _ => "non-text-input".to_string(),
        };
        self.events.push(BackendEvent::Output {
            session_id: session_id.to_string(),
            bytes: text.into_bytes(),
        });
        Ok(())
    }

    fn resize(&mut self, session_id: &str, size: TerminalSize) -> BackendResult<()> {
        self.events.push(BackendEvent::Resized {
            session_id: session_id.to_string(),
            columns: size.columns,
            rows: size.rows,
        });
        Ok(())
    }

    fn terminate(&mut self, session_id: &str, _mode: TerminationMode) -> BackendResult<()> {
        self.events.push(BackendEvent::Exited {
            session_id: session_id.to_string(),
            code: Some(0),
        });
        Ok(())
    }

    fn drain_events(&mut self) -> Vec<BackendEvent> {
        std::mem::take(&mut self.events)
    }
}

struct Options {
    session_count: usize,
}

impl Options {
    fn parse() -> BenchResult<Self> {
        let mut session_count = 5;
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--sessions" => {
                    let value = args
                        .next()
                        .ok_or_else(|| BenchError::new("--sessions requires a value"))?;
                    session_count = value.parse::<usize>().map_err(|error| {
                        BenchError::new(format!("invalid --sessions value: {error}"))
                    })?;
                }
                "--help" | "-h" => {
                    return Err(BenchError::new(
                        "usage: agentmux-bench-restart-recovery [--sessions 5]",
                    ));
                }
                other => return Err(BenchError::new(format!("unknown argument: {other}"))),
            }
        }

        if session_count == 0 {
            return Err(BenchError::new("--sessions must be greater than zero"));
        }

        Ok(Self { session_count })
    }
}
