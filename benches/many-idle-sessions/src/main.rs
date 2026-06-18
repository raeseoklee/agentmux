use std::process::ExitCode;
use std::time::Duration;

use agentmux_bench_support::{
    duration_stats, main_exit, new_conpty_control, observe_control_loop, process_sample,
    queue_snapshot, spawn_cmd_session, terminate_session, BenchError, BenchResult, DurationStats,
    ProcessSample, QueueSnapshot,
};
use serde::Serialize;

fn main() -> ExitCode {
    main_exit(run_benchmark)
}

#[derive(Serialize)]
struct ManyIdleReport {
    benchmark: &'static str,
    backend_kind: &'static str,
    observation_ms: u64,
    scenarios: Vec<IdleScenarioReport>,
}

#[derive(Serialize)]
struct IdleScenarioReport {
    session_count: usize,
    startup_to_prompt_stats: DurationStats,
    idle_control_loop_stats: DurationStats,
    process_before: ProcessSample,
    process_after_spawn: ProcessSample,
    process_after_observation: ProcessSample,
    queue_after_observation: QueueSnapshot,
}

fn run_benchmark() -> BenchResult<ManyIdleReport> {
    let options = Options::parse()?;
    let mut scenarios = Vec::new();

    for session_count in options.session_counts {
        scenarios.push(run_scenario(session_count, options.observation)?);
    }

    Ok(ManyIdleReport {
        benchmark: "bench_many_idle_sessions",
        backend_kind: "conpty",
        observation_ms: options.observation.as_millis() as u64,
        scenarios,
    })
}

fn run_scenario(session_count: usize, observation: Duration) -> BenchResult<IdleScenarioReport> {
    let mut control = new_conpty_control();
    let process_before = process_sample();
    let mut session_ids = Vec::with_capacity(session_count);
    let mut startup_samples = Vec::with_capacity(session_count);

    for index in 0..session_count {
        let prompt = format!("AgentMuxIdle{index:02}>");
        let session = spawn_cmd_session(&mut control, "ws_many_idle", &prompt, 120, 30)?;
        startup_samples.push(Duration::from_secs_f64(
            session.startup_to_prompt_ms / 1000.0,
        ));
        session_ids.push(session.session_id);
    }

    let process_after_spawn = process_sample();
    let idle_samples = observe_control_loop(&mut control, observation);
    let process_after_observation = process_sample();
    let queue_after_observation = queue_snapshot(&control);

    for session_id in session_ids {
        terminate_session(&mut control, &session_id)?;
    }
    control.collect_events();

    Ok(IdleScenarioReport {
        session_count,
        startup_to_prompt_stats: duration_stats(&startup_samples)?,
        idle_control_loop_stats: duration_stats(&idle_samples)?,
        process_before,
        process_after_spawn,
        process_after_observation,
        queue_after_observation,
    })
}

#[derive(Clone)]
struct Options {
    session_counts: Vec<usize>,
    observation: Duration,
}

impl Options {
    fn parse() -> BenchResult<Self> {
        let mut session_counts = vec![20, 50];
        let mut observation = Duration::from_secs(5);
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--sessions" => {
                    let value = args.next().ok_or_else(|| {
                        BenchError::new("--sessions requires a comma-separated value")
                    })?;
                    session_counts = parse_counts(&value)?;
                }
                "--observe-ms" => {
                    let value = args
                        .next()
                        .ok_or_else(|| BenchError::new("--observe-ms requires a value"))?;
                    let millis = value.parse::<u64>().map_err(|error| {
                        BenchError::new(format!("invalid --observe-ms value: {error}"))
                    })?;
                    observation = Duration::from_millis(millis);
                }
                "--help" | "-h" => {
                    return Err(BenchError::new(
                        "usage: agentmux-bench-many-idle-sessions [--sessions 20,50] [--observe-ms 5000]",
                    ));
                }
                other => return Err(BenchError::new(format!("unknown argument: {other}"))),
            }
        }

        if session_counts.is_empty() {
            return Err(BenchError::new("at least one session count is required"));
        }

        Ok(Self {
            session_counts,
            observation,
        })
    }
}

fn parse_counts(value: &str) -> BenchResult<Vec<usize>> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<usize>()
                .map_err(|error| BenchError::new(format!("invalid session count: {error}")))
                .and_then(|count| {
                    if count == 0 {
                        Err(BenchError::new("session count must be greater than zero"))
                    } else {
                        Ok(count)
                    }
                })
        })
        .collect()
}
