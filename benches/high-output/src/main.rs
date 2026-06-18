use std::process::ExitCode;
use std::time::{Duration, Instant};

use agentmux_bench_support::{
    duration_stats, elapsed_ms, main_exit, new_conpty_control, process_sample, queue_snapshot,
    read_recent, send_text, spawn_cmd_session, terminate_session, wait_for_text, BenchError,
    BenchResult, DurationStats, ProcessSample, QueueSnapshot,
};
use serde::Serialize;

const HIDDEN_DONE_MARKER: &str = "agentmux-high-output-done";

fn main() -> ExitCode {
    main_exit(run_benchmark)
}

#[derive(Serialize)]
struct HighOutputReport {
    benchmark: &'static str,
    backend_kind: &'static str,
    hidden_output_line_count: usize,
    visible_probe_count: usize,
    hidden_output_duration_ms: f64,
    visible_probe_round_trip_stats: DurationStats,
    hidden_recent_buffer_bytes: usize,
    process_before: ProcessSample,
    process_after: ProcessSample,
    queue_after: QueueSnapshot,
}

fn run_benchmark() -> BenchResult<HighOutputReport> {
    let options = Options::parse()?;
    let mut control = new_conpty_control();
    let process_before = process_sample();
    let visible = spawn_cmd_session(&mut control, "ws_high_output", "AgentMuxVisible>", 120, 30)?;
    let hidden = spawn_cmd_session(&mut control, "ws_high_output", "AgentMuxHidden>", 120, 30)?;

    let hidden_command = format!(
        "powershell -NoProfile -Command \"1..{} | ForEach-Object {{ Write-Output ('agentmux-high-output-' + $_) }}; Write-Output '{}'\"\r",
        options.line_count, HIDDEN_DONE_MARKER
    );
    let hidden_started = Instant::now();
    send_text(&mut control, &hidden.session_id, &hidden_command)?;

    let mut visible_samples = Vec::with_capacity(options.visible_probe_count);
    for index in 0..options.visible_probe_count {
        let marker = format!("agentmux-visible-probe-{index:03}");
        let started = Instant::now();
        send_text(
            &mut control,
            &visible.session_id,
            &format!("echo {marker}\r"),
        )?;
        wait_for_text(
            &mut control,
            &visible.session_id,
            &marker,
            Duration::from_secs(10),
        )?;
        visible_samples.push(started.elapsed());
    }

    wait_for_text(
        &mut control,
        &hidden.session_id,
        HIDDEN_DONE_MARKER,
        Duration::from_secs(30),
    )?;
    let hidden_output_duration = hidden_started.elapsed();
    let hidden_recent = read_recent(&mut control, &hidden.session_id, 65_536)?;
    control.collect_events();

    let queue_after = queue_snapshot(&control);
    let process_after = process_sample();

    terminate_session(&mut control, &visible.session_id)?;
    terminate_session(&mut control, &hidden.session_id)?;
    control.collect_events();

    Ok(HighOutputReport {
        benchmark: "bench_high_output",
        backend_kind: "conpty",
        hidden_output_line_count: options.line_count,
        visible_probe_count: options.visible_probe_count,
        hidden_output_duration_ms: elapsed_ms(hidden_output_duration),
        visible_probe_round_trip_stats: duration_stats(&visible_samples)?,
        hidden_recent_buffer_bytes: hidden_recent.len(),
        process_before,
        process_after,
        queue_after,
    })
}

struct Options {
    line_count: usize,
    visible_probe_count: usize,
}

impl Options {
    fn parse() -> BenchResult<Self> {
        let mut line_count = 5_000;
        let mut visible_probe_count = 10;
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--lines" => {
                    let value = args
                        .next()
                        .ok_or_else(|| BenchError::new("--lines requires a value"))?;
                    line_count = parse_positive_usize("--lines", &value)?;
                }
                "--visible-probes" => {
                    let value = args
                        .next()
                        .ok_or_else(|| BenchError::new("--visible-probes requires a value"))?;
                    visible_probe_count = parse_positive_usize("--visible-probes", &value)?;
                }
                "--help" | "-h" => {
                    return Err(BenchError::new(
                        "usage: agentmux-bench-high-output [--lines 5000] [--visible-probes 10]",
                    ));
                }
                other => return Err(BenchError::new(format!("unknown argument: {other}"))),
            }
        }

        Ok(Self {
            line_count,
            visible_probe_count,
        })
    }
}

fn parse_positive_usize(name: &str, value: &str) -> BenchResult<usize> {
    let parsed = value
        .parse::<usize>()
        .map_err(|error| BenchError::new(format!("invalid {name} value: {error}")))?;
    if parsed == 0 {
        Err(BenchError::new(format!("{name} must be greater than zero")))
    } else {
        Ok(parsed)
    }
}
