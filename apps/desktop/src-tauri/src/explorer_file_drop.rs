use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use webview2_com::{
    take_pwstr,
    Microsoft::Web::WebView2::Win32::{
        ICoreWebView2File, ICoreWebView2WebMessageReceivedEventArgs2,
    },
    WebMessageReceivedEventHandler,
};
use windows_core::{Interface, PWSTR};

const FILE_DROP_MESSAGE_TYPE: &str = "agentmux.explorer-file-drop";
const FILE_DROP_EVENT: &str = "agentmux://explorer-file-drop";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExplorerFileDropMetadata {
    #[serde(rename = "type")]
    message_type: String,
    target_pane_id: Option<String>,
    target_session_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExplorerFileDropPayload {
    paths: Vec<String>,
    target_pane_id: Option<String>,
    target_session_id: Option<String>,
}

pub fn install(app: &tauri::AppHandle) -> Result<(), String> {
    let webview = app
        .get_webview_window("main")
        .ok_or_else(|| "main webview is unavailable".to_string())?;
    let app_handle = app.clone();
    webview
        .with_webview(move |platform_webview| unsafe {
            if let Err(error) = install_web_message_handler(platform_webview, app_handle) {
                eprintln!("[agentmux] Explorer file-drop bridge unavailable: {error}");
            }
        })
        .map_err(|error| error.to_string())
}

unsafe fn install_web_message_handler(
    platform_webview: tauri::webview::PlatformWebview,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let webview = unsafe { platform_webview.controller().CoreWebView2() }
        .map_err(|error| error.to_string())?;
    let mut token = 0;
    let handler = WebMessageReceivedEventHandler::create(Box::new(move |_sender, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let mut raw_message = PWSTR::null();
        if unsafe { args.TryGetWebMessageAsString(&mut raw_message) }.is_err() {
            return Ok(());
        }
        let message = take_pwstr(raw_message);
        let Ok(metadata) = serde_json::from_str::<ExplorerFileDropMetadata>(&message) else {
            return Ok(());
        };
        if metadata.message_type != FILE_DROP_MESSAGE_TYPE {
            return Ok(());
        }

        let Ok(args2) = args.cast::<ICoreWebView2WebMessageReceivedEventArgs2>() else {
            return Ok(());
        };
        let objects = unsafe { args2.AdditionalObjects()? };
        let mut count = 0;
        unsafe { objects.Count(&mut count)? };
        let mut paths = Vec::with_capacity(count as usize);
        for index in 0..count {
            let object = unsafe { objects.GetValueAtIndex(index)? };
            let Ok(file) = object.cast::<ICoreWebView2File>() else {
                continue;
            };
            let mut raw_path = PWSTR::null();
            unsafe { file.Path(&mut raw_path)? };
            let path = take_pwstr(raw_path);
            if !path.trim().is_empty() {
                paths.push(path);
            }
        }
        if !paths.is_empty() {
            let _ = app.emit(
                FILE_DROP_EVENT,
                ExplorerFileDropPayload {
                    paths,
                    target_pane_id: metadata.target_pane_id,
                    target_session_id: metadata.target_session_id,
                },
            );
        }
        Ok(())
    }));
    unsafe {
        webview
            .add_WebMessageReceived(&handler, &mut token)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
