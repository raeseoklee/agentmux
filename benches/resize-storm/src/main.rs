use std::process::ExitCode;
use std::time::{Duration, Instant};

use agentmux_bench_support::{
    duration_stats, elapsed_ms, main_exit, new_conpty_control, process_sample, queue_snapshot,
    resize_session, send_text, spawn_cmd_session, terminate_session, wait_for_text, BenchError,
    BenchResult, DurationStats, ProcessSample, QueueSnapshot,
};
use serde::Serialize;

fn main() -> ExitCode {
    main_exit(run_benchmark)
}

#[derive(Serialize)]
struct ResizeStormReport {
    benchmark: &'static str,
    backend_kind: &'static str,
    iterations: usize,
    resize_request_stats: DurationStats,
    storm_elapsed_ms: f64,
    final_requested_columns: u16,
    final_requested_rows: u16,
    post_storm_echo_ms: f64,
    process_after: ProcessSample,
    queue_after: QueueSnapshot,
}

fn run_benchmark() -> BenchResult<ResizeStormReport> {
    let options = Options::parse()?;
    let mut control = new_conpty_control();
    let session = spawn_cmd_session(&mut control, "ws_resize_storm", "AgentMuxResize>", 120, 30)?;
    let mut samples = Vec::with_capacity(options.iterations);
    let mut final_size = (120, 30);
    let storm_started = Instant::now();

    for index in 0..options.iterations {
        let columns = 80 + (index % 81) as u16;
        let rows = 20 + (index % 25) as u16;
        final_size = (columns, rows);
        samples.push(resize_session(
            &mut control,
            &session.session_id,
            columns,
            rows,
        )?);
    }

    control.collect_events();
    let storm_elapsed = storm_started.elapsed();
    let marker = "agentmux-resize-storm-ok";
    let echo_started = Instant::now();
    send_text(
        &mut control,
        &session.session_id,
        &format!("echo {marker}\r"),
    )?;
    wait_for_text(
        &mut control,
        &session.session_id,
        marker,
        Duration::from_secs(5),
    )?;
    let post_storm_echo = echo_started.elapsed();
    let queue_after = queue_snapshot(&control);
    let process_after = process_sample();

    terminate_session(&mut control, &session.session_id)?;
    control.collect_events();

    Ok(ResizeStormReport {
        benchmark: "bench_resize_storm",
        backend_kind: "conpty",
        iterations: options.iterations,
        resize_request_stats: duration_stats(&samples)?,
        storm_elapsed_ms: elapsed_ms(storm_elapsed),
        final_requested_columns: final_size.0,
        final_requested_rows: final_size.1,
        post_storm_echo_ms: elapsed_ms(post_storm_echo),
        process_after,
        queue_after,
    })
}

struct Options {
    iterations: usize,
}

impl Options {
    fn parse() -> BenchResult<Self> {
        let mut iterations = 200;
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--iterations" => {
                    let value = args
                        .next()
                        .ok_or_else(|| BenchError::new("--iterations requires a value"))?;
                    iterations = value.parse::<usize>().map_err(|error| {
                        BenchError::new(format!("invalid --iterations value: {error}"))
                    })?;
                }
                "--help" | "-h" => {
                    return Err(BenchError::new(
                        "usage: agentmux-bench-resize-storm [--iterations 200]",
                    ));
                }
                other => return Err(BenchError::new(format!("unknown argument: {other}"))),
            }
        }

        if iterations == 0 {
            return Err(BenchError::new("--iterations must be greater than zero"));
        }

        Ok(Self { iterations })
    }
}
