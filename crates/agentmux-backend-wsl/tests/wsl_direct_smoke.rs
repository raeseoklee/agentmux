#![cfg(windows)]

use std::process::Command;
use std::time::{Duration, Instant};

use agentmux_backend::{
    BackendEvent, BackendKind, CommandSpec, InputEvent, SessionBackend, SpawnRequest, TerminalSize,
};
use agentmux_backend_wsl::{
    distribution_discovery_command, distributions_or_diagnostic, fallback_windows_path_to_wsl,
    WslDirectBackend, DEFAULT_WSL_CWD,
};

#[test]
fn wsl_direct_launches_selected_distribution_with_input_resize_and_exit() {
    let Some(distribution) = discover_smoke_distribution() else {
        eprintln!("Skipping WSL direct smoke test: no WSL distribution is available.");
        return;
    };

    let mut backend = WslDirectBackend::new();
    let session_id = "ses_wsl_direct_smoke".to_string();
    let script = [
        r#"printf 'agentmux-wsl-cwd:%s\n' "$PWD""#,
        r#"read value"#,
        r#"printf 'agentmux-wsl-input:%s\n' "$value""#,
    ]
    .join("; ");

    let handle = backend
        .spawn(SpawnRequest {
            session_id: session_id.clone(),
            workspace_id: None,
            backend: Some(BackendKind::WslDirect),
            backend_profile: Some(distribution.clone()),
            command: CommandSpec::with_args("bash", vec!["-lc".to_string(), script]),
            cwd: Some("/tmp".to_string()),
            env: Vec::new(),
            initial_size: TerminalSize::new(100, 30),
        })
        .unwrap_or_else(|error| panic!("spawn WSL direct session in {distribution}: {error}"));

    assert_eq!(handle.session_id, session_id);
    assert_eq!(handle.backend_kind, BackendKind::WslDirect);

    backend
        .resize(&session_id, TerminalSize::new(120, 32))
        .expect("resize WSL direct session");
    backend
        .send_input(
            &session_id,
            InputEvent::Text("agentmux-input\n".to_string()),
        )
        .expect("send input to WSL direct session");

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut output = Vec::new();
    let mut saw_resize = false;
    let mut saw_exit = false;
    let mut diagnostics = Vec::new();

    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        for event in backend.drain_events() {
            match event {
                BackendEvent::Output { bytes, .. } => output.extend(bytes),
                BackendEvent::Resized { .. } => saw_resize = true,
                BackendEvent::Exited { code, .. } => {
                    saw_exit = true;
                    assert_eq!(code, Some(0));
                }
                BackendEvent::Error { error, .. } => {
                    diagnostics.push(format!("{}: {}", error.code, error.message));
                }
                BackendEvent::Started { .. } | BackendEvent::HealthChanged { .. } => {}
            }
        }

        let text = String::from_utf8_lossy(&output);
        if saw_resize
            && saw_exit
            && text.contains("agentmux-wsl-cwd:/tmp")
            && text.contains("agentmux-wsl-input:agentmux-input")
        {
            return;
        }
    }

    panic!(
        "WSL direct smoke test timed out. distribution={distribution:?}, default_cwd={DEFAULT_WSL_CWD:?}, saw_resize={saw_resize}, saw_exit={saw_exit}, output={:?}, diagnostics={diagnostics:?}",
        String::from_utf8_lossy(&output),
    );
}

#[test]
fn wsl_direct_reports_missing_selected_distribution_before_launch() {
    if discover_smoke_distribution().is_none() {
        eprintln!(
            "Skipping WSL missing-distribution smoke test: no WSL distribution is available."
        );
        return;
    }

    let mut backend = WslDirectBackend::new();
    let error = backend
        .spawn(SpawnRequest {
            session_id: "ses_wsl_missing_distribution".to_string(),
            workspace_id: None,
            backend: Some(BackendKind::WslDirect),
            backend_profile: Some("AgentMuxDefinitelyMissingDistribution".to_string()),
            command: CommandSpec::with_args("bash", vec!["-lc".to_string(), "pwd".to_string()]),
            cwd: Some("/tmp".to_string()),
            env: Vec::new(),
            initial_size: TerminalSize::new(80, 24),
        })
        .expect_err("missing distribution should fail before launch");

    assert_eq!(error.code, "wsl_distribution_not_found");
    assert!(error
        .message
        .contains("AgentMuxDefinitelyMissingDistribution"));
}

#[test]
fn wsl_direct_resolves_windows_cwd_with_wslpath_or_deterministic_fallback() {
    let Some(distribution) = discover_smoke_distribution() else {
        eprintln!("Skipping WSL cwd conversion smoke test: no WSL distribution is available.");
        return;
    };

    let current_dir = std::env::current_dir().expect("current directory");
    let current_dir = current_dir.to_string_lossy().to_string();
    let expected = fallback_windows_path_to_wsl(&current_dir)
        .expect("workspace current directory should be a Windows drive path");

    let mut backend = WslDirectBackend::new();
    let session_id = "ses_wsl_cwd_conversion_smoke".to_string();
    backend
        .spawn(SpawnRequest {
            session_id: session_id.clone(),
            workspace_id: None,
            backend: Some(BackendKind::WslDirect),
            backend_profile: Some(distribution.clone()),
            command: CommandSpec::with_args("bash", vec!["-lc".to_string(), "pwd".to_string()]),
            cwd: Some(current_dir.clone()),
            env: Vec::new(),
            initial_size: TerminalSize::new(80, 24),
        })
        .unwrap_or_else(|error| {
            panic!("spawn WSL direct cwd conversion session in {distribution}: {error}")
        });

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut output = Vec::new();
    let mut saw_exit = false;

    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        for event in backend.drain_events() {
            match event {
                BackendEvent::Output { bytes, .. } => output.extend(bytes),
                BackendEvent::Exited { code, .. } => {
                    saw_exit = true;
                    assert_eq!(code, Some(0));
                }
                BackendEvent::Started { .. }
                | BackendEvent::Resized { .. }
                | BackendEvent::HealthChanged { .. }
                | BackendEvent::Error { .. } => {}
            }
        }

        let text = String::from_utf8_lossy(&output).replace('\\', "/");
        if saw_exit && text.contains(&expected) {
            return;
        }
    }

    panic!(
        "WSL cwd conversion smoke test timed out. distribution={distribution:?}, current_dir={current_dir:?}, expected={expected:?}, output={:?}",
        String::from_utf8_lossy(&output),
    );
}

fn discover_smoke_distribution() -> Option<String> {
    if let Ok(distribution) = std::env::var("AGENTMUX_WSL_TEST_DISTRIBUTION") {
        if !distribution.trim().is_empty() {
            return Some(distribution);
        }
    }

    let command = distribution_discovery_command();
    let output = Command::new(command.executable)
        .args(command.args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let distributions = distributions_or_diagnostic(&stdout).ok()?;
    distributions
        .iter()
        .find(|distribution| distribution.is_default)
        .or_else(|| distributions.first())
        .map(|distribution| distribution.name.clone())
}
