#![cfg(windows)]

use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use agentmux_backend::{
    AttachRequest, BackendEvent, BackendKind, CommandSpec, InputEvent, SessionBackend,
    SpawnRequest, TerminalSize, TerminationMode,
};
use agentmux_backend_tmux::TmuxControlBackend;
use agentmux_backend_wsl::discover_wsl_distributions;

static TMUX_SMOKE_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn tmux_control_launches_in_wsl_and_round_trips_output() {
    if std::env::var_os("AGENTMUX_RUN_TMUX_SMOKE").is_none() {
        eprintln!("skipping tmux-control smoke test; set AGENTMUX_RUN_TMUX_SMOKE=1 to run it");
        return;
    }

    let _guard = TMUX_SMOKE_LOCK.lock().unwrap();
    let Some(distribution) = distribution_with_tmux() else {
        eprintln!("skipping tmux-control smoke test because no WSL distribution has tmux");
        return;
    };

    let unique = unique_suffix();
    let session_id = format!("ses_tmux_smoke_{unique}");
    let workspace_id = format!("ws_tmux_smoke_{unique}");
    let mut backend = TmuxControlBackend::new();
    let handle = backend
        .spawn(SpawnRequest {
            session_id: session_id.clone(),
            workspace_id: Some(workspace_id),
            backend: Some(BackendKind::WslTmuxControl),
            backend_profile: Some(distribution.clone()),
            command: CommandSpec::with_args(
                "sh",
                vec![
                    "-lc".to_string(),
                    "sleep 0.2; printf 'agentmux-tmux-ready\\n'; sleep 1; read value; printf 'agentmux-tmux-input:%s\\n' \"$value\"; sleep 1"
                        .to_string(),
                ],
            ),
            cwd: Some("/tmp".to_string()),
            env: Vec::new(),
            initial_size: TerminalSize::new(100, 24),
        })
        .unwrap_or_else(|error| panic!("spawn tmux-control session in {distribution}: {error}"));

    assert_eq!(handle.backend_kind, BackendKind::WslTmuxControl);
    assert!(handle
        .backend_native_id
        .as_deref()
        .is_some_and(|id| id.starts_with("agentmux_ses_tmux_smoke_")));

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();
    let mut sent_input = false;
    let mut saw_exit = false;

    while Instant::now() < deadline {
        for event in backend.drain_events() {
            match event {
                BackendEvent::Output { bytes, .. } => output.extend(bytes),
                BackendEvent::Exited { .. } => saw_exit = true,
                BackendEvent::Error { error, .. } => {
                    diagnostics.push(format!("{}: {}", error.code, error.message));
                }
                _ => {}
            }
        }

        let text = String::from_utf8_lossy(&output);
        if text.contains("agentmux-tmux-ready") && !sent_input {
            backend
                .send_input(
                    &session_id,
                    InputEvent::Text("agentmux-control-input\n".to_string()),
                )
                .unwrap();
            backend
                .resize(&session_id, TerminalSize::new(120, 30))
                .unwrap();
            sent_input = true;
        }

        if text.contains("agentmux-tmux-input:agentmux-control-input") && saw_exit {
            return;
        }

        std::thread::sleep(Duration::from_millis(25));
    }

    let _ = backend.terminate(&session_id, TerminationMode::Kill);
    panic!(
        "tmux-control smoke test timed out. distribution={distribution:?}, sent_input={sent_input}, saw_exit={saw_exit}, output={:?}, diagnostics={diagnostics:?}",
        String::from_utf8_lossy(&output)
    );
}

#[test]
fn tmux_control_reattaches_without_duplicating_shell_process() {
    if std::env::var_os("AGENTMUX_RUN_TMUX_SMOKE").is_none() {
        eprintln!(
            "skipping tmux-control reattach smoke test; set AGENTMUX_RUN_TMUX_SMOKE=1 to run it"
        );
        return;
    }

    if std::env::var_os("AGENTMUX_RUN_TMUX_REATTACH_SMOKE").is_none() {
        eprintln!(
            "skipping tmux-control reattach smoke test; set AGENTMUX_RUN_TMUX_REATTACH_SMOKE=1 to run it"
        );
        return;
    }

    let _guard = TMUX_SMOKE_LOCK.lock().unwrap();
    let Some(distribution) = distribution_with_tmux() else {
        eprintln!("skipping tmux-control reattach smoke test because no WSL distribution has tmux");
        return;
    };

    let unique = unique_suffix();
    let first_session_id = format!("ses_tmux_first_{unique}");
    let recovered_session_id = format!("ses_tmux_recovered_{unique}");
    let workspace_id = format!("ws_tmux_recovery_{unique}");
    let mut first = TmuxControlBackend::new();
    let handle = first
        .spawn(SpawnRequest {
            session_id: first_session_id.clone(),
            workspace_id: Some(workspace_id),
            backend: Some(BackendKind::WslTmuxControl),
            backend_profile: Some(distribution.clone()),
            command: CommandSpec::with_args(
                "sh",
                vec![
                    "-lc".to_string(),
                    "sleep 0.2; printf 'agentmux-tmux-pid:%s\\n' \"$$\"; sleep 1; while IFS= read -r value; do printf 'agentmux-tmux-echo:%s:%s\\n' \"$$\" \"$value\"; [ \"$value\" = agentmux-quit ] && break; done"
                        .to_string(),
                ],
            ),
            cwd: Some("/tmp".to_string()),
            env: Vec::new(),
            initial_size: TerminalSize::new(100, 24),
        })
        .unwrap_or_else(|error| {
            panic!("spawn reattach tmux-control session in {distribution}: {error}")
        });
    let backend_ref = handle
        .backend_native_id
        .clone()
        .expect("tmux session name as backend native id");

    let first_output = wait_for_output(&mut first, "agentmux-tmux-pid:", &first_session_id)
        .unwrap_or_else(|| {
            let _ = first.terminate(&first_session_id, TerminationMode::Kill);
            panic!("timed out waiting for tmux shell pid from first control client")
        });
    let pid = extract_after(&first_output, "agentmux-tmux-pid:")
        .unwrap_or_else(|| panic!("missing pid in output: {first_output:?}"));

    first
        .terminate(&first_session_id, TerminationMode::Soft)
        .unwrap();

    let mut recovered = TmuxControlBackend::new();
    recovered
        .attach(AttachRequest {
            session_id: recovered_session_id.clone(),
            backend: BackendKind::WslTmuxControl,
            backend_profile: Some(distribution.clone()),
            backend_ref: backend_ref.clone(),
            initial_size: TerminalSize::new(100, 24),
        })
        .unwrap_or_else(|error| {
            panic!("attach reattach tmux-control session in {distribution}: {error}")
        });
    recovered
        .send_input(
            &recovered_session_id,
            InputEvent::Text("agentmux-after-attach\n".to_string()),
        )
        .unwrap();

    let expected = format!("agentmux-tmux-echo:{pid}:agentmux-after-attach");
    let recovered_output = wait_for_output(&mut recovered, &expected, &recovered_session_id)
        .unwrap_or_else(|| {
            let _ = recovered.terminate(&recovered_session_id, TerminationMode::Kill);
            panic!("timed out waiting for reattached tmux shell output; expected {expected}")
        });
    assert!(
        recovered_output.contains(&expected),
        "reattached output did not prove same shell process. output={recovered_output:?}, expected={expected:?}"
    );

    recovered
        .send_input(
            &recovered_session_id,
            InputEvent::Text("agentmux-quit\n".to_string()),
        )
        .unwrap();
    let quit_expected = format!("agentmux-tmux-echo:{pid}:agentmux-quit");
    let _ = wait_for_output(&mut recovered, &quit_expected, &recovered_session_id);
    let _ = recovered.terminate(&recovered_session_id, TerminationMode::Kill);
}

fn distribution_with_tmux() -> Option<String> {
    let distributions = discover_wsl_distributions().ok()?;
    distributions.into_iter().find_map(|distribution| {
        let status = Command::new("wsl.exe")
            .args([
                "--distribution",
                &distribution.name,
                "--exec",
                "sh",
                "-lc",
                "command -v tmux >/dev/null 2>&1",
            ])
            .status()
            .ok()?;
        status.success().then_some(distribution.name)
    })
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}_{:x}", std::process::id(), nanos & 0xffffffffff)
}

fn wait_for_output<B>(
    backend: &mut TmuxControlBackend<B>,
    needle: &str,
    session_id: &str,
) -> Option<String>
where
    B: SessionBackend,
{
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut output = Vec::new();

    while Instant::now() < deadline {
        for event in backend.drain_events() {
            if let BackendEvent::Output {
                session_id: event_session_id,
                bytes,
            } = event
            {
                if event_session_id == session_id {
                    output.extend(bytes);
                }
            }
        }

        let text = String::from_utf8_lossy(&output);
        if text.contains(needle) {
            return Some(text.into_owned());
        }

        std::thread::sleep(Duration::from_millis(25));
    }

    None
}

fn extract_after(text: &str, prefix: &str) -> Option<String> {
    let rest = text.split(prefix).nth(1)?;
    rest.lines().next().map(str::trim).map(ToString::to_string)
}
