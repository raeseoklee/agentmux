use std::fmt;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use agentmux_backend::SessionBackend;
use agentmux_backend_conpty::ConptyBackend;
use agentmux_core::{RuntimeControlPlane, TerminalRuntime};
use agentmux_ipc::{RequestEnvelope, ResponseEnvelope, ResponseOutcome};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub const TOKEN: &str = "bench-local-token";

pub type BenchControl = RuntimeControlPlane<ConptyBackend>;
pub type BenchResult<T> = Result<T, BenchError>;

#[derive(Debug)]
pub struct BenchError {
    message: String,
}

impl BenchError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for BenchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BenchError {}

impl From<serde_json::Error> for BenchError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error.to_string())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SpawnedSession {
    pub session_id: String,
    pub startup_to_prompt_ms: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct DurationStats {
    pub count: usize,
    pub min_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ProcessSample {
    pub working_set_bytes: Option<u64>,
    pub commit_bytes: Option<u64>,
    pub handle_count: Option<u32>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct QueueSnapshot {
    pub event_queue_depth: usize,
    pub event_history_depth: usize,
    pub event_backlog_limit: usize,
    pub dropped_event_count: usize,
    pub notification_depth: usize,
    pub notification_limit: usize,
}

pub fn main_exit<T, F>(run: F) -> ExitCode
where
    T: Serialize,
    F: FnOnce() -> BenchResult<T>,
{
    match run() {
        Ok(report) => match serde_json::to_string_pretty(&report) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

pub fn new_conpty_control() -> BenchControl {
    RuntimeControlPlane::new(TerminalRuntime::new(ConptyBackend::new()), TOKEN)
}

pub fn request(id: &str, method: &str, params_json: &str) -> RequestEnvelope {
    RequestEnvelope::new(id, method, params_json, TOKEN)
}

pub fn invoke<B, T>(
    control: &mut RuntimeControlPlane<B>,
    id: &str,
    method: &str,
    params_json: &str,
) -> BenchResult<T>
where
    B: SessionBackend,
    T: DeserializeOwned,
{
    let response = control.handle_request(request(id, method, params_json));
    response_result(&response)
}

pub fn expect_ok(response: &ResponseEnvelope) -> BenchResult<()> {
    match &response.outcome {
        ResponseOutcome::Ok { .. } => Ok(()),
        ResponseOutcome::Error(error) => Err(BenchError::new(format!(
            "control request failed: {}: {}",
            error.code.as_str(),
            error.message
        ))),
    }
}

pub fn response_result<T>(response: &ResponseEnvelope) -> BenchResult<T>
where
    T: DeserializeOwned,
{
    let result_json = match &response.outcome {
        ResponseOutcome::Ok { result_json } => result_json,
        ResponseOutcome::Error(error) => {
            return Err(BenchError::new(format!(
                "control request failed: {}: {}",
                error.code.as_str(),
                error.message
            )))
        }
    };

    serde_json::from_str(result_json).map_err(BenchError::from)
}

pub fn json_string_field(response: &ResponseEnvelope, field: &str) -> BenchResult<String> {
    let value: serde_json::Value = response_result(response)?;
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| BenchError::new(format!("missing string field '{field}'")))
}

pub fn spawn_cmd_session(
    control: &mut BenchControl,
    workspace_id: &str,
    prompt_marker: &str,
    columns: u16,
    rows: u16,
) -> BenchResult<SpawnedSession> {
    let started = Instant::now();
    let prompt_base = prompt_marker.strip_suffix('>').unwrap_or(prompt_marker);
    let escaped_prompt = escape_cmd_prompt(prompt_base);
    let response = control.handle_request(request(
        "bench_spawn",
        "session.spawn",
        &format!(
            r#"{{"workspace_id":"{workspace_id}","command":["cmd.exe","/d","/q","/k","prompt {escaped_prompt}$G"],"cwd":null,"columns":{columns},"rows":{rows},"durability":"ephemeral"}}"#
        ),
    ));
    let session_id = json_string_field(&response, "session_id")?;
    wait_for_text(control, &session_id, prompt_marker, Duration::from_secs(10))?;

    Ok(SpawnedSession {
        session_id,
        startup_to_prompt_ms: elapsed_ms(started.elapsed()),
    })
}

pub fn send_text(
    control: &mut BenchControl,
    session_id: &str,
    text: &str,
) -> BenchResult<Duration> {
    let started = Instant::now();
    let response = control.handle_request(request(
        "bench_send",
        "session.send_text",
        &serde_json::json!({"session_id": session_id, "text": text}).to_string(),
    ));
    expect_ok(&response)?;
    Ok(started.elapsed())
}

pub fn resize_session(
    control: &mut BenchControl,
    session_id: &str,
    columns: u16,
    rows: u16,
) -> BenchResult<Duration> {
    let started = Instant::now();
    let response = control.handle_request(request(
        "bench_resize",
        "session.resize",
        &format!(r#"{{"session_id":"{session_id}","columns":{columns},"rows":{rows}}}"#),
    ));
    expect_ok(&response)?;
    Ok(started.elapsed())
}

pub fn terminate_session(control: &mut BenchControl, session_id: &str) -> BenchResult<Duration> {
    let started = Instant::now();
    let response = control.handle_request(request(
        "bench_terminate",
        "session.terminate",
        &format!(r#"{{"session_id":"{session_id}","mode":"kill"}}"#),
    ));
    expect_ok(&response)?;
    Ok(started.elapsed())
}

pub fn read_recent(
    control: &mut BenchControl,
    session_id: &str,
    max_bytes: usize,
) -> BenchResult<String> {
    let value: serde_json::Value = invoke(
        control,
        "bench_recent",
        "session.read_recent",
        &format!(r#"{{"session_id":"{session_id}","max_bytes":{max_bytes}}}"#),
    )?;
    value
        .get("text")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| BenchError::new("missing recent output text"))
}

pub fn wait_for_text(
    control: &mut BenchControl,
    session_id: &str,
    marker: &str,
    timeout: Duration,
) -> BenchResult<String> {
    let deadline = Instant::now() + timeout;
    let mut text = String::new();

    while Instant::now() < deadline {
        control.collect_events();
        text = read_recent(control, session_id, 65_536)?;
        if text.contains(marker) {
            return Ok(text);
        }

        thread::sleep(Duration::from_millis(10));
    }

    Err(BenchError::new(format!(
        "timed out waiting for '{marker}' in output: {text:?}"
    )))
}

pub fn observe_control_loop(control: &mut BenchControl, duration: Duration) -> Vec<Duration> {
    let deadline = Instant::now() + duration;
    let mut samples = Vec::new();

    while Instant::now() < deadline {
        let started = Instant::now();
        control.collect_events();
        samples.push(started.elapsed());
        thread::sleep(Duration::from_millis(50));
    }

    samples
}

pub fn queue_snapshot<B>(control: &RuntimeControlPlane<B>) -> QueueSnapshot
where
    B: SessionBackend,
{
    QueueSnapshot {
        event_queue_depth: control.event_queue_depth(),
        event_history_depth: control.event_history_depth(),
        event_backlog_limit: control.event_backlog_limit(),
        dropped_event_count: control.dropped_event_count(),
        notification_depth: control.notification_depth(),
        notification_limit: control.notification_limit(),
    }
}

pub fn duration_stats(samples: &[Duration]) -> BenchResult<DurationStats> {
    if samples.is_empty() {
        return Err(BenchError::new("cannot summarize empty duration samples"));
    }

    let mut values = samples.iter().copied().map(elapsed_ms).collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);

    Ok(DurationStats {
        count: values.len(),
        min_ms: values[0],
        p50_ms: percentile(&values, 0.50),
        p95_ms: percentile(&values, 0.95),
        p99_ms: percentile(&values, 0.99),
        max_ms: values[values.len() - 1],
    })
}

pub fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

pub fn process_sample() -> ProcessSample {
    platform_process_sample()
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    let last = values.len() - 1;
    let index = (last as f64 * percentile).ceil() as usize;
    values[index.min(last)]
}

fn escape_cmd_prompt(prompt: &str) -> String {
    prompt.replace('$', "$$")
}

#[cfg(windows)]
fn platform_process_sample() -> ProcessSample {
    use std::mem::{size_of, zeroed};

    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

    let process = unsafe { GetCurrentProcess() };
    let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { zeroed() };
    counters.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let memory_ok = unsafe { GetProcessMemoryInfo(process, &mut counters, counters.cb) } != 0;

    let mut handle_count = 0;
    let handles_ok = unsafe { GetProcessHandleCount(process, &mut handle_count) } != 0;

    ProcessSample {
        working_set_bytes: memory_ok.then_some(counters.WorkingSetSize as u64),
        commit_bytes: memory_ok.then_some(counters.PagefileUsage as u64),
        handle_count: handles_ok.then_some(handle_count),
    }
}

#[cfg(not(windows))]
fn platform_process_sample() -> ProcessSample {
    ProcessSample {
        working_set_bytes: None,
        commit_bytes: None,
        handle_count: None,
    }
}
