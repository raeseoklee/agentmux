// Hide the Windows console window for the GUI app, including local dev/debug launches.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::sync::Arc;

use agentmux_desktop_host::{
    agentmux_control as handle_agentmux_control, default_control_pipe_name,
    default_control_token_path, default_store_path, load_or_create_control_token,
    run_wsl_prewarm_keepalive, start_control_pipe_server, DesktopControlState, DesktopNotification,
    DesktopNotificationAdapter, OutputStreamFrame,
};
use agentmux_ipc::{RequestEnvelope, ResponseEnvelope};
use tauri_plugin_notification::NotificationExt;

struct TauriDesktopNotificationAdapter {
    app: tauri::AppHandle,
}

impl DesktopNotificationAdapter for TauriDesktopNotificationAdapter {
    fn notify(&self, notification: DesktopNotification) {
        if let Err(error) = self
            .app
            .notification()
            .builder()
            .title(notification.title)
            .body(notification.body)
            .show()
        {
            eprintln!(
                "agentmux: failed to show desktop notification {}: {error}",
                notification.notification_id
            );
        }
    }
}

#[tauri::command]
fn agentmux_control(
    state: tauri::State<'_, Arc<DesktopControlState>>,
    request: RequestEnvelope,
) -> ResponseEnvelope {
    handle_agentmux_control(state.inner().as_ref(), request)
}

#[tauri::command]
fn agentmux_control_token(state: tauri::State<'_, Arc<DesktopControlState>>) -> String {
    state.inner().control_token().to_string()
}

#[tauri::command(rename_all = "snake_case")]
fn session_subscribe_output(
    state: tauri::State<'_, Arc<DesktopControlState>>,
    session_id: String,
    subscription_id: String,
    on_event: tauri::ipc::Channel<OutputStreamFrame>,
) {
    #[cfg(debug_assertions)]
    eprintln!("agentmux: terminal output stream subscribed session={session_id} subscription={subscription_id}");
    state
        .inner()
        .register_output_channel(session_id, subscription_id, on_event);
}

#[tauri::command(rename_all = "snake_case")]
fn session_unsubscribe_output(
    state: tauri::State<'_, Arc<DesktopControlState>>,
    session_id: String,
    subscription_id: String,
) {
    #[cfg(debug_assertions)]
    eprintln!("agentmux: terminal output stream unsubscribed session={session_id} subscription={subscription_id}");
    state
        .inner()
        .unregister_output_channel(&session_id, &subscription_id);
}

#[tauri::command(rename_all = "snake_case")]
fn session_send_text_direct(
    state: tauri::State<'_, Arc<DesktopControlState>>,
    session_id: String,
    text: String,
) -> Result<(), String> {
    state
        .inner()
        .send_text_direct(&session_id, text)
        .map_err(|error| error.to_string())
}

fn main() {
    let store_path = default_store_path().expect("failed to resolve AgentMux store path");
    let token_path =
        default_control_token_path().expect("failed to resolve AgentMux control token path");
    let control_token =
        load_or_create_control_token(token_path).expect("failed to load AgentMux control token");
    let state = Arc::new(
        DesktopControlState::open_with_token(store_path, control_token)
            .expect("failed to open AgentMux store"),
    );
    // Seed the id counter from the persisted high-water mark FIRST, before any
    // spawn/split/restore mints an id — otherwise the per-process counter restarts
    // at 1 and new entities overwrite persisted rows with colliding ids.
    state.seed_id_counter();
    // Ephemeral sessions from a previous run are dead; mark them disconnected so
    // the UI never renders a ghost "running" terminal stuck "starting…". Store-only
    // and fast, so it runs synchronously before we begin serving requests.
    state.reconcile_orphaned_ephemeral_sessions();
    start_control_pipe_server(state.clone(), default_control_pipe_name());
    // Recover durable WSL/tmux sessions on a background thread: it probes
    // wsl.exe and can block for seconds, so it must not delay the control pipe
    // server or the window from coming up.
    let recovery_state = state.clone();
    std::thread::spawn(move || {
        recovery_state.recover_durable_sessions();
        // Optionally re-spawn ephemeral terminals into their panes. OFF by default
        // (opt in with AGENTMUX_ENABLE_TERMINAL_RESTORE) — it isn't robust yet, so
        // dead ephemeral terminals normally show as reopenable panes instead. No-op
        // unless the env var is set.
        recovery_state.restore_ephemeral_terminals();
    });
    // Pre-warm and keep the WSL2 VM resident so terminals open in ~0.35s instead
    // of paying the ~5s cold boot on the first launch (and again after every WSL
    // idle shutdown). Best-effort; opt out with AGENTMUX_DISABLE_WSL_PREWARM.
    std::thread::spawn(run_wsl_prewarm_keepalive);
    // Background pump: drains coalesced terminal output and pushes it to each
    // session's Tauri channel. It stays light while idle, then briefly tightens
    // after input/output so interactive echo does not wait on the idle tick.
    let pump_state = state.clone();
    std::thread::spawn(move || loop {
        let had_output = pump_state.pump_output_stream();
        std::thread::sleep(pump_state.output_stream_pump_delay(had_output));
    });
    let notification_state = state.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            notification_state.set_desktop_notification_adapter(Arc::new(
                TauriDesktopNotificationAdapter {
                    app: app.handle().clone(),
                },
            ));
            Ok(())
        })
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            agentmux_control,
            agentmux_control_token,
            session_subscribe_output,
            session_unsubscribe_output,
            session_send_text_direct
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AgentMux desktop app");
}
