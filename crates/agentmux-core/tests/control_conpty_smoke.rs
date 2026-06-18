#![cfg(windows)]

use std::time::{Duration, Instant};

use agentmux_backend_conpty::ConptyBackend;
use agentmux_core::{RuntimeControlPlane, TerminalRuntime};
use agentmux_ipc::{RequestEnvelope, ResponseEnvelope, ResponseOutcome};

#[test]
fn control_plane_spawn_read_recent_through_conpty() {
    let runtime = TerminalRuntime::new(ConptyBackend::new());
    let mut control = RuntimeControlPlane::new(runtime, "test-token");

    let spawn = control.handle_request(request(
        "req_spawn",
        "session.spawn",
        r#"{"workspace_id":"ws_control_smoke","command":["cmd.exe","/d","/q","/c","echo agentmux-control"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral"}"#,
    ));
    let session_id = session_id_from_spawn(&spawn);

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        control.runtime_mut().drain_events();

        let recent = control.handle_request(request(
            "req_recent",
            "session.read_recent",
            &format!(r#"{{"session_id":"{session_id}","max_bytes":4096}}"#),
        ));

        if ok_json(&recent).contains("agentmux-control") {
            return;
        }

        std::thread::sleep(Duration::from_millis(50));
    }

    panic!("timed out waiting for ConPTY output through control plane");
}

fn request(id: &str, method: &str, params_json: &str) -> RequestEnvelope {
    RequestEnvelope::new(id, method, params_json, "test-token")
}

fn session_id_from_spawn(response: &ResponseEnvelope) -> String {
    ok_json(response)
        .split("\"session_id\":\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("session id in spawn result")
        .to_string()
}

fn ok_json(response: &ResponseEnvelope) -> &str {
    match &response.outcome {
        ResponseOutcome::Ok { result_json } => result_json,
        ResponseOutcome::Error(error) => panic!("unexpected error: {error:?}"),
    }
}
