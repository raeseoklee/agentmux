// Hide the Windows console window in release builds (this is a GUI app).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use agentmux_desktop_host::{
    agentmux_control as handle_agentmux_control, default_control_pipe_name,
    default_control_token_path, default_store_path, load_or_create_control_token,
    start_control_pipe_server, DesktopControlState, DesktopNotification,
    DesktopNotificationAdapter,
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
    start_control_pipe_server(state.clone(), default_control_pipe_name());
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
            agentmux_control_token
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AgentMux desktop app");
}
