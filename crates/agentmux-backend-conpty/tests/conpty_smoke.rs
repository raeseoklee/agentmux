#![cfg(windows)]

use std::time::{Duration, Instant};

use agentmux_backend::{BackendEvent, CommandSpec, SessionBackend, SpawnRequest, TerminalSize};
use agentmux_backend_conpty::ConptyBackend;

#[test]
fn conpty_spawn_echo_reports_output_and_exit() {
    let mut backend = ConptyBackend::new();
    let session_id = "ses_conpty_smoke".to_string();
    let handle = backend
        .spawn(SpawnRequest {
            session_id: session_id.clone(),
            workspace_id: None,
            backend: None,
            backend_profile: None,
            command: CommandSpec::with_args(
                "cmd.exe",
                vec![
                    "/d".to_string(),
                    "/q".to_string(),
                    "/c".to_string(),
                    "echo agentmux".to_string(),
                ],
            ),
            cwd: None,
            env: Vec::new(),
            initial_size: TerminalSize::new(80, 24),
        })
        .expect("spawn cmd.exe in ConPTY");

    assert_eq!(handle.session_id, session_id);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();
    let mut saw_exit = false;

    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));

        for event in backend.drain_events() {
            match event {
                BackendEvent::Output {
                    session_id: event_session_id,
                    bytes,
                } if event_session_id == session_id => output.extend(bytes),
                BackendEvent::Exited {
                    session_id: event_session_id,
                    code,
                } if event_session_id == session_id => {
                    assert_eq!(code, Some(0));
                    saw_exit = true;
                }
                BackendEvent::Error {
                    session_id: Some(event_session_id),
                    error,
                } if event_session_id == session_id => {
                    diagnostics.push(format!("{}: {}", error.code, error.message));
                }
                _ => {}
            }
        }

        if saw_exit && String::from_utf8_lossy(&output).contains("agentmux") {
            return;
        }
    }

    panic!(
        "ConPTY smoke test timed out. saw_exit={saw_exit}, output={:?}, diagnostics={diagnostics:?}",
        String::from_utf8_lossy(&output)
    );
}
