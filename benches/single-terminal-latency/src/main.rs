use std::process::ExitCode;
use std::time::{Duration, Instant};

use agentmux_bench_support::{
    duration_stats, elapsed_ms, main_exit, new_conpty_control, process_sample, queue_snapshot,
    resize_session, send_text, spawn_cmd_session, terminate_session, wait_for_text, BenchResult,
    DurationStats, ProcessSample, QueueSnapshot,
};
use serde::Serialize;

const PROMPT_MARKER: &str = "AgentMux>";
const OUTPUT_MARKER: &str = "agentmux-latency-probe";

fn main() -> ExitCode {
    main_exit(run_probe)
}

#[derive(Serialize)]
struct ProbeReport {
    benchmark: &'static str,
    backend_kind: &'static str,
    startup_to_prompt_ms: f64,
    command_round_trip_ms: f64,
    resize_request_ms: f64,
    command_round_trip_stats: DurationStats,
    queue: QueueSnapshot,
    process: ProcessSample,
}

fn run_probe() -> BenchResult<ProbeReport> {
    let mut control = new_conpty_control();
    let spawned = spawn_cmd_session(&mut control, "ws_bench", PROMPT_MARKER, 120, 30)?;
    let command_start = Instant::now();
    send_text(
        &mut control,
        &spawned.session_id,
        &format!("echo {OUTPUT_MARKER}\r"),
    )?;
    wait_for_text(
        &mut control,
        &spawned.session_id,
        OUTPUT_MARKER,
        Duration::from_secs(5),
    )?;
    let command_round_trip = command_start.elapsed();
    let command_round_trip_stats = duration_stats(&[command_round_trip])?;

    let resize_request = resize_session(&mut control, &spawned.session_id, 100, 28)?;

    terminate_session(&mut control, &spawned.session_id)?;
    control.collect_events();

    Ok(ProbeReport {
        benchmark: "bench_single_terminal_latency",
        backend_kind: "conpty",
        startup_to_prompt_ms: spawned.startup_to_prompt_ms,
        command_round_trip_ms: elapsed_ms(command_round_trip),
        resize_request_ms: elapsed_ms(resize_request),
        command_round_trip_stats,
        queue: queue_snapshot(&control),
        process: process_sample(),
    })
}
