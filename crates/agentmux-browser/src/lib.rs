use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde_json::{json, Value};
use tempfile::TempDir;
use tungstenite::{connect, Message};

const AGENTMUX_CONSOLE_RECORDER_SOURCE: &str = r#"(function () {
  if (!window.__agentmuxConsoleMessages) {
    Object.defineProperty(window, "__agentmuxConsoleMessages", {
      value: [],
      writable: false,
      configurable: false
    });
  }
  if (window.__agentmuxConsoleRecorderInstalled) {
    return;
  }
  Object.defineProperty(window, "__agentmuxConsoleRecorderInstalled", {
    value: true,
    writable: false,
    configurable: false
  });
  const levels = ["debug", "log", "info", "warn", "error"];
  const stringify = function (value) {
    try {
      if (value instanceof Error) {
        return value.stack || value.message || String(value);
      }
      if (typeof value === "string") {
        return value;
      }
      const json = JSON.stringify(value);
      return json === undefined ? String(value) : json;
    } catch (_) {
      return String(value);
    }
  };
  levels.forEach(function (level) {
    const original = console[level] && console[level].bind(console);
    if (!original) {
      return;
    }
    console[level] = function () {
      const args = Array.prototype.slice.call(arguments);
      window.__agentmuxConsoleMessages.push({
        level: level,
        text: args.map(stringify).join(" "),
        timestamp: new Date().toISOString()
      });
      if (window.__agentmuxConsoleMessages.length > 500) {
        window.__agentmuxConsoleMessages.splice(0, window.__agentmuxConsoleMessages.length - 500);
      }
      return original.apply(console, args);
    };
  });
})();"#;

const AGENTMUX_DIALOG_RECORDER_SOURCE: &str = r#"(function () {
  if (!window.__agentmuxDialogMessages) {
    Object.defineProperty(window, "__agentmuxDialogMessages", {
      value: [],
      writable: false,
      configurable: false
    });
  }
  if (window.__agentmuxDialogRecorderInstalled) {
    return;
  }
  Object.defineProperty(window, "__agentmuxDialogRecorderInstalled", {
    value: true,
    writable: false,
    configurable: false
  });
  const record = function (type, message, defaultValue, response) {
    window.__agentmuxDialogMessages.push({
      type: type,
      message: String(message || ""),
      defaultValue: defaultValue == null ? null : String(defaultValue),
      response: response == null ? null : String(response),
      timestamp: new Date().toISOString()
    });
    if (window.__agentmuxDialogMessages.length > 500) {
      window.__agentmuxDialogMessages.splice(0, window.__agentmuxDialogMessages.length - 500);
    }
  };
  window.alert = function (message) {
    record("alert", message, null, null);
  };
  window.confirm = function (message) {
    record("confirm", message, null, true);
    return true;
  };
  window.prompt = function (message, defaultValue) {
    const response = defaultValue == null ? "" : String(defaultValue);
    record("prompt", message, defaultValue, response);
    return response;
  };
})();"#;

const AGENTMUX_ERROR_RECORDER_SOURCE: &str = r#"(function () {
  if (!window.__agentmuxErrorEvents) {
    Object.defineProperty(window, "__agentmuxErrorEvents", {
      value: [],
      writable: false,
      configurable: false
    });
  }
  if (window.__agentmuxErrorRecorderInstalled) {
    return;
  }
  Object.defineProperty(window, "__agentmuxErrorRecorderInstalled", {
    value: true,
    writable: false,
    configurable: false
  });
  const stringify = function (value) {
    try {
      if (value instanceof Error) {
        return value.stack || value.message || String(value);
      }
      if (typeof value === "string") {
        return value;
      }
      const json = JSON.stringify(value);
      return json === undefined ? String(value) : json;
    } catch (_) {
      return String(value);
    }
  };
  const push = function (event) {
    window.__agentmuxErrorEvents.push(Object.assign({
      timestamp: new Date().toISOString()
    }, event));
    if (window.__agentmuxErrorEvents.length > 500) {
      window.__agentmuxErrorEvents.splice(0, window.__agentmuxErrorEvents.length - 500);
    }
  };
  window.addEventListener("error", function (event) {
    push({
      kind: "error",
      message: String(event.message || ""),
      source: event.filename || "",
      line: event.lineno || 0,
      column: event.colno || 0,
      stack: event.error && event.error.stack ? String(event.error.stack) : ""
    });
  });
  window.addEventListener("unhandledrejection", function (event) {
    push({
      kind: "unhandledrejection",
      message: stringify(event.reason),
      source: "",
      line: 0,
      column: 0,
      stack: event.reason && event.reason.stack ? String(event.reason.stack) : ""
    });
  });
})();"#;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSurface {
    pub surface_id: String,
    pub browser_id: String,
    pub workspace_id: String,
    pub current_url: Option<String>,
    pub profile: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserCommand {
    Navigate {
        surface_id: String,
        url: String,
    },
    Reload {
        surface_id: String,
    },
    GoBack {
        surface_id: String,
    },
    GoForward {
        surface_id: String,
    },
    CurrentUrl {
        surface_id: String,
    },
    Screenshot {
        surface_id: String,
        format: String,
    },
    DomSnapshot {
        surface_id: String,
        frame_id: Option<String>,
    },
    Frames {
        surface_id: String,
    },
    StorageSnapshot {
        surface_id: String,
    },
    Cookies {
        surface_id: String,
    },
    Downloads {
        surface_id: String,
        limit: usize,
    },
    History {
        surface_id: String,
    },
    ConsoleMessages {
        surface_id: String,
        limit: usize,
    },
    DialogMessages {
        surface_id: String,
        limit: usize,
    },
    ErrorEvents {
        surface_id: String,
        limit: usize,
    },
    ClickSelector {
        surface_id: String,
        selector: String,
        frame_id: Option<String>,
    },
    ClickPoint {
        surface_id: String,
        x: i32,
        y: i32,
    },
    TypeText {
        surface_id: String,
        selector: String,
        text: String,
        frame_id: Option<String>,
    },
    FillText {
        surface_id: String,
        selector: String,
        text: String,
        frame_id: Option<String>,
    },
    PressKey {
        surface_id: String,
        selector: String,
        key: String,
        frame_id: Option<String>,
    },
    SelectValues {
        surface_id: String,
        selector: String,
        values: Vec<String>,
        frame_id: Option<String>,
    },
    ScrollBy {
        surface_id: String,
        selector: Option<String>,
        x: i32,
        y: i32,
        frame_id: Option<String>,
    },
    HoverSelector {
        surface_id: String,
        selector: String,
        frame_id: Option<String>,
    },
    CheckSelector {
        surface_id: String,
        selector: String,
        checked: bool,
        frame_id: Option<String>,
    },
    GetElement {
        surface_id: String,
        selector: String,
        kind: String,
        attribute: Option<String>,
        frame_id: Option<String>,
    },
    FindText {
        surface_id: String,
        query: String,
        selector: Option<String>,
        limit: u16,
        frame_id: Option<String>,
    },
    HighlightSelector {
        surface_id: String,
        selector: String,
        duration_ms: u64,
        frame_id: Option<String>,
    },
    FocusSelector {
        surface_id: String,
        selector: String,
        frame_id: Option<String>,
    },
    SetZoom {
        surface_id: String,
        percent: u16,
    },
    WaitForSelector {
        surface_id: String,
        selector: String,
        timeout_ms: u64,
        frame_id: Option<String>,
    },
    Evaluate {
        surface_id: String,
        script: String,
        frame_id: Option<String>,
    },
}

impl BrowserCommand {
    pub fn surface_id(&self) -> &str {
        match self {
            BrowserCommand::Navigate { surface_id, .. }
            | BrowserCommand::Reload { surface_id }
            | BrowserCommand::GoBack { surface_id }
            | BrowserCommand::GoForward { surface_id }
            | BrowserCommand::CurrentUrl { surface_id }
            | BrowserCommand::Screenshot { surface_id, .. }
            | BrowserCommand::DomSnapshot { surface_id, .. }
            | BrowserCommand::Frames { surface_id }
            | BrowserCommand::StorageSnapshot { surface_id }
            | BrowserCommand::Cookies { surface_id }
            | BrowserCommand::Downloads { surface_id, .. }
            | BrowserCommand::History { surface_id }
            | BrowserCommand::ConsoleMessages { surface_id, .. }
            | BrowserCommand::DialogMessages { surface_id, .. }
            | BrowserCommand::ErrorEvents { surface_id, .. }
            | BrowserCommand::ClickSelector { surface_id, .. }
            | BrowserCommand::ClickPoint { surface_id, .. }
            | BrowserCommand::TypeText { surface_id, .. }
            | BrowserCommand::FillText { surface_id, .. }
            | BrowserCommand::PressKey { surface_id, .. }
            | BrowserCommand::SelectValues { surface_id, .. }
            | BrowserCommand::ScrollBy { surface_id, .. }
            | BrowserCommand::HoverSelector { surface_id, .. }
            | BrowserCommand::CheckSelector { surface_id, .. }
            | BrowserCommand::GetElement { surface_id, .. }
            | BrowserCommand::FindText { surface_id, .. }
            | BrowserCommand::HighlightSelector { surface_id, .. }
            | BrowserCommand::FocusSelector { surface_id, .. }
            | BrowserCommand::SetZoom { surface_id, .. }
            | BrowserCommand::WaitForSelector { surface_id, .. }
            | BrowserCommand::Evaluate { surface_id, .. } => surface_id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserCommandResult {
    Navigated {
        surface_id: String,
        url: String,
    },
    Screenshot {
        surface_id: String,
        format: String,
        bytes: Vec<u8>,
    },
    DomSnapshot {
        surface_id: String,
        html: String,
    },
    Frames {
        surface_id: String,
        frames: Vec<BrowserFrameInfo>,
    },
    StorageSnapshot {
        surface_id: String,
        local_storage: Vec<BrowserStorageEntry>,
        session_storage: Vec<BrowserStorageEntry>,
    },
    Cookies {
        surface_id: String,
        cookies: Vec<BrowserCookieInfo>,
    },
    Downloads {
        surface_id: String,
        directory: String,
        downloads: Vec<BrowserDownloadInfo>,
    },
    History {
        surface_id: String,
        current_index: i64,
        entries: Vec<BrowserHistoryEntry>,
    },
    ConsoleMessages {
        surface_id: String,
        messages: Vec<BrowserConsoleMessage>,
    },
    DialogMessages {
        surface_id: String,
        messages: Vec<BrowserDialogMessage>,
    },
    ErrorEvents {
        surface_id: String,
        events: Vec<BrowserErrorEvent>,
    },
    Clicked {
        surface_id: String,
        target: String,
    },
    Typed {
        surface_id: String,
        selector: String,
        text: String,
    },
    Filled {
        surface_id: String,
        selector: String,
        text: String,
    },
    Pressed {
        surface_id: String,
        selector: String,
        key: String,
    },
    Selected {
        surface_id: String,
        selector: String,
        values: Vec<String>,
    },
    Scrolled {
        surface_id: String,
        target: String,
        x: i32,
        y: i32,
    },
    Hovered {
        surface_id: String,
        selector: String,
    },
    Checked {
        surface_id: String,
        selector: String,
        checked: bool,
    },
    Got {
        surface_id: String,
        selector: String,
        kind: String,
        value: String,
    },
    Found {
        surface_id: String,
        query: String,
        count: usize,
        matches: Vec<String>,
    },
    Highlighted {
        surface_id: String,
        selector: String,
        duration_ms: u64,
    },
    Focused {
        surface_id: String,
        selector: String,
    },
    Zoomed {
        surface_id: String,
        percent: u16,
    },
    WaitedForSelector {
        surface_id: String,
        selector: String,
        elapsed_ms: u64,
    },
    Evaluated {
        surface_id: String,
        value_json: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserFrameInfo {
    pub frame_id: String,
    pub parent_frame_id: Option<String>,
    pub url: String,
    pub name: Option<String>,
    pub security_origin: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserStorageEntry {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserCookieInfo {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: Option<String>,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserDownloadInfo {
    pub file_name: String,
    pub path: String,
    pub byte_count: u64,
    pub modified_at: Option<String>,
    pub complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserConsoleMessage {
    pub level: String,
    pub text: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserHistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserDialogMessage {
    pub dialog_type: String,
    pub message: String,
    pub default_value: Option<String>,
    pub response: Option<String>,
    pub timestamp: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserErrorEvent {
    pub kind: String,
    pub message: String,
    pub source: String,
    pub line: u32,
    pub column: u32,
    pub stack: String,
    pub timestamp: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserAutomationErrorCode {
    SurfaceNotFound,
    InvalidRequest,
    AutomationFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserAutomationError {
    pub code: BrowserAutomationErrorCode,
    pub message: String,
}

impl BrowserAutomationError {
    pub fn surface_not_found(surface_id: &str) -> Self {
        Self {
            code: BrowserAutomationErrorCode::SurfaceNotFound,
            message: format!("Browser surface '{surface_id}' was not found."),
        }
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: BrowserAutomationErrorCode::InvalidRequest,
            message: message.into(),
        }
    }

    pub fn automation_failed(message: impl Into<String>) -> Self {
        Self {
            code: BrowserAutomationErrorCode::AutomationFailed,
            message: message.into(),
        }
    }
}

impl fmt::Display for BrowserAutomationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl Error for BrowserAutomationError {}

pub type BrowserAutomationResult<T> = Result<T, BrowserAutomationError>;

pub trait BrowserAutomation: Send {
    fn create_surface(
        &mut self,
        surface_id: String,
        workspace_id: String,
        profile: Option<String>,
    ) -> BrowserAutomationResult<BrowserSurface>;

    fn surface(&self, surface_id: &str) -> BrowserAutomationResult<BrowserSurface>;

    fn close_surface(&mut self, surface_id: &str) -> BrowserAutomationResult<BrowserSurface>;

    fn execute(&mut self, command: BrowserCommand)
        -> BrowserAutomationResult<BrowserCommandResult>;
}

#[derive(Default)]
pub struct InMemoryBrowserAutomation {
    surfaces: HashMap<String, BrowserSurface>,
    next_browser_id: u64,
}

impl InMemoryBrowserAutomation {
    pub fn new() -> Self {
        Self::default()
    }

    fn require_surface_mut(
        &mut self,
        surface_id: &str,
    ) -> BrowserAutomationResult<&mut BrowserSurface> {
        self.surfaces
            .get_mut(surface_id)
            .ok_or_else(|| BrowserAutomationError::surface_not_found(surface_id))
    }

    fn require_surface(&self, surface_id: &str) -> BrowserAutomationResult<&BrowserSurface> {
        self.surfaces
            .get(surface_id)
            .ok_or_else(|| BrowserAutomationError::surface_not_found(surface_id))
    }
}

impl BrowserAutomation for InMemoryBrowserAutomation {
    fn create_surface(
        &mut self,
        surface_id: String,
        workspace_id: String,
        profile: Option<String>,
    ) -> BrowserAutomationResult<BrowserSurface> {
        if surface_id.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Browser surface id must not be empty.",
            ));
        }
        if workspace_id.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Browser workspace id must not be empty.",
            ));
        }
        if self.surfaces.contains_key(&surface_id) {
            return Err(BrowserAutomationError::invalid_request(format!(
                "Browser surface '{surface_id}' already exists."
            )));
        }

        self.next_browser_id += 1;
        let surface = BrowserSurface {
            surface_id: surface_id.clone(),
            browser_id: format!("browser_{:08}", self.next_browser_id),
            workspace_id,
            current_url: None,
            profile,
        };
        self.surfaces.insert(surface_id, surface.clone());
        Ok(surface)
    }

    fn surface(&self, surface_id: &str) -> BrowserAutomationResult<BrowserSurface> {
        Ok(self.require_surface(surface_id)?.clone())
    }

    fn close_surface(&mut self, surface_id: &str) -> BrowserAutomationResult<BrowserSurface> {
        self.surfaces
            .remove(surface_id)
            .ok_or_else(|| BrowserAutomationError::surface_not_found(surface_id))
    }

    fn execute(
        &mut self,
        command: BrowserCommand,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        match command {
            BrowserCommand::Navigate { surface_id, url } => {
                if url.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Navigation URL must not be empty.",
                    ));
                }
                let surface = self.require_surface_mut(&surface_id)?;
                surface.current_url = Some(url.clone());
                Ok(BrowserCommandResult::Navigated { surface_id, url })
            }
            BrowserCommand::Reload { surface_id }
            | BrowserCommand::GoBack { surface_id }
            | BrowserCommand::GoForward { surface_id }
            | BrowserCommand::CurrentUrl { surface_id } => {
                let surface = self.require_surface(&surface_id)?;
                let url = surface
                    .current_url
                    .clone()
                    .unwrap_or_else(|| "about:blank".to_string());
                Ok(BrowserCommandResult::Navigated { surface_id, url })
            }
            BrowserCommand::Screenshot { surface_id, format } => {
                self.require_surface(&surface_id)?;
                let format = if format.trim().is_empty() {
                    "png".to_string()
                } else {
                    format
                };
                Ok(BrowserCommandResult::Screenshot {
                    bytes: format!("agentmux-browser:{surface_id}:{format}").into_bytes(),
                    surface_id,
                    format,
                })
            }
            BrowserCommand::DomSnapshot { surface_id, .. } => {
                let surface = self.require_surface(&surface_id)?;
                let url = surface.current_url.as_deref().unwrap_or("about:blank");
                Ok(BrowserCommandResult::DomSnapshot {
                    html: format!(
                        r#"<html data-agentmux-surface="{surface_id}"><body>{url}</body></html>"#
                    ),
                    surface_id,
                })
            }
            BrowserCommand::Frames { surface_id } => {
                let surface = self.require_surface(&surface_id)?;
                Ok(BrowserCommandResult::Frames {
                    surface_id: surface_id.clone(),
                    frames: vec![BrowserFrameInfo {
                        frame_id: format!("frame_{surface_id}"),
                        parent_frame_id: None,
                        url: surface
                            .current_url
                            .clone()
                            .unwrap_or_else(|| "about:blank".to_string()),
                        name: None,
                        security_origin: None,
                    }],
                })
            }
            BrowserCommand::StorageSnapshot { surface_id } => {
                self.require_surface(&surface_id)?;
                Ok(BrowserCommandResult::StorageSnapshot {
                    surface_id,
                    local_storage: Vec::new(),
                    session_storage: Vec::new(),
                })
            }
            BrowserCommand::Cookies { surface_id } => {
                self.require_surface(&surface_id)?;
                Ok(BrowserCommandResult::Cookies {
                    surface_id,
                    cookies: Vec::new(),
                })
            }
            BrowserCommand::Downloads { surface_id, .. } => {
                self.require_surface(&surface_id)?;
                Ok(BrowserCommandResult::Downloads {
                    directory: format!("memory://browser/{surface_id}/downloads"),
                    surface_id,
                    downloads: Vec::new(),
                })
            }
            BrowserCommand::History { surface_id } => {
                let surface = self.require_surface(&surface_id)?;
                let url = surface
                    .current_url
                    .clone()
                    .unwrap_or_else(|| "about:blank".to_string());
                Ok(BrowserCommandResult::History {
                    surface_id,
                    current_index: 0,
                    entries: vec![BrowserHistoryEntry {
                        id: 0,
                        title: url.clone(),
                        url,
                    }],
                })
            }
            BrowserCommand::ConsoleMessages { surface_id, .. } => {
                self.require_surface(&surface_id)?;
                Ok(BrowserCommandResult::ConsoleMessages {
                    surface_id,
                    messages: Vec::new(),
                })
            }
            BrowserCommand::DialogMessages { surface_id, .. } => {
                self.require_surface(&surface_id)?;
                Ok(BrowserCommandResult::DialogMessages {
                    surface_id,
                    messages: Vec::new(),
                })
            }
            BrowserCommand::ErrorEvents { surface_id, .. } => {
                self.require_surface(&surface_id)?;
                Ok(BrowserCommandResult::ErrorEvents {
                    surface_id,
                    events: Vec::new(),
                })
            }
            BrowserCommand::ClickSelector {
                surface_id,
                selector,
                ..
            } => {
                self.require_surface(&surface_id)?;
                if selector.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Click selector must not be empty.",
                    ));
                }
                Ok(BrowserCommandResult::Clicked {
                    surface_id,
                    target: selector,
                })
            }
            BrowserCommand::ClickPoint { surface_id, x, y } => {
                self.require_surface(&surface_id)?;
                if x < 0 || y < 0 {
                    return Err(BrowserAutomationError::invalid_request(
                        "Click coordinates must be non-negative.",
                    ));
                }
                Ok(BrowserCommandResult::Clicked {
                    surface_id,
                    target: format!("{x},{y}"),
                })
            }
            BrowserCommand::TypeText {
                surface_id,
                selector,
                text,
                ..
            } => {
                self.require_surface(&surface_id)?;
                if selector.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Type selector must not be empty.",
                    ));
                }
                Ok(BrowserCommandResult::Typed {
                    surface_id,
                    selector,
                    text,
                })
            }
            BrowserCommand::FillText {
                surface_id,
                selector,
                text,
                ..
            } => {
                self.require_surface(&surface_id)?;
                if selector.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Fill selector must not be empty.",
                    ));
                }
                Ok(BrowserCommandResult::Filled {
                    surface_id,
                    selector,
                    text,
                })
            }
            BrowserCommand::PressKey {
                surface_id,
                selector,
                key,
                ..
            } => {
                self.require_surface(&surface_id)?;
                if selector.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Press selector must not be empty.",
                    ));
                }
                if key.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Press key must not be empty.",
                    ));
                }
                Ok(BrowserCommandResult::Pressed {
                    surface_id,
                    selector,
                    key,
                })
            }
            BrowserCommand::SelectValues {
                surface_id,
                selector,
                values,
                ..
            } => {
                self.require_surface(&surface_id)?;
                if selector.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Select selector must not be empty.",
                    ));
                }
                if values.is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Select values must not be empty.",
                    ));
                }
                Ok(BrowserCommandResult::Selected {
                    surface_id,
                    selector,
                    values,
                })
            }
            BrowserCommand::ScrollBy {
                surface_id,
                selector,
                x,
                y,
                ..
            } => {
                self.require_surface(&surface_id)?;
                if selector
                    .as_deref()
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(false)
                {
                    return Err(BrowserAutomationError::invalid_request(
                        "Scroll selector must not be empty.",
                    ));
                }
                Ok(BrowserCommandResult::Scrolled {
                    surface_id,
                    target: selector.unwrap_or_else(|| "window".to_string()),
                    x,
                    y,
                })
            }
            BrowserCommand::HoverSelector {
                surface_id,
                selector,
                ..
            } => {
                self.require_surface(&surface_id)?;
                if selector.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Hover selector must not be empty.",
                    ));
                }
                Ok(BrowserCommandResult::Hovered {
                    surface_id,
                    selector,
                })
            }
            BrowserCommand::CheckSelector {
                surface_id,
                selector,
                checked,
                ..
            } => {
                self.require_surface(&surface_id)?;
                if selector.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Check selector must not be empty.",
                    ));
                }
                Ok(BrowserCommandResult::Checked {
                    surface_id,
                    selector,
                    checked,
                })
            }
            BrowserCommand::GetElement {
                surface_id,
                selector,
                kind,
                attribute,
                ..
            } => {
                self.require_surface(&surface_id)?;
                if selector.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Get selector must not be empty.",
                    ));
                }
                let kind = normalize_browser_get_kind(&kind, attribute.as_deref())?;
                Ok(BrowserCommandResult::Got {
                    value: format!("{kind}:{selector}"),
                    surface_id,
                    selector,
                    kind,
                })
            }
            BrowserCommand::FindText {
                surface_id,
                query,
                selector,
                limit,
                ..
            } => {
                self.require_surface(&surface_id)?;
                if query.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Find query must not be empty.",
                    ));
                }
                if selector
                    .as_deref()
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(false)
                {
                    return Err(BrowserAutomationError::invalid_request(
                        "Find selector must not be empty.",
                    ));
                }
                let limit = limit.max(1);
                Ok(BrowserCommandResult::Found {
                    surface_id,
                    query: query.clone(),
                    count: 1,
                    matches: vec![format!(
                        "{}:{}",
                        selector.unwrap_or_else(|| "body".to_string()),
                        query
                    )]
                    .into_iter()
                    .take(usize::from(limit))
                    .collect(),
                })
            }
            BrowserCommand::HighlightSelector {
                surface_id,
                selector,
                duration_ms,
                ..
            } => {
                self.require_surface(&surface_id)?;
                if selector.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Highlight selector must not be empty.",
                    ));
                }
                Ok(BrowserCommandResult::Highlighted {
                    surface_id,
                    selector,
                    duration_ms: duration_ms.max(1),
                })
            }
            BrowserCommand::FocusSelector {
                surface_id,
                selector,
                ..
            } => {
                self.require_surface(&surface_id)?;
                if selector.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Focus selector must not be empty.",
                    ));
                }
                Ok(BrowserCommandResult::Focused {
                    surface_id,
                    selector,
                })
            }
            BrowserCommand::SetZoom {
                surface_id,
                percent,
            } => {
                self.require_surface(&surface_id)?;
                validate_zoom_percent(percent)?;
                Ok(BrowserCommandResult::Zoomed {
                    surface_id,
                    percent,
                })
            }
            BrowserCommand::WaitForSelector {
                surface_id,
                selector,
                ..
            } => {
                self.require_surface(&surface_id)?;
                if selector.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Wait selector must not be empty.",
                    ));
                }
                Ok(BrowserCommandResult::WaitedForSelector {
                    surface_id,
                    selector,
                    elapsed_ms: 1,
                })
            }
            BrowserCommand::Evaluate {
                surface_id, script, ..
            } => {
                self.require_surface(&surface_id)?;
                if script.trim().is_empty() {
                    return Err(BrowserAutomationError::invalid_request(
                        "Evaluate script must not be empty.",
                    ));
                }
                Ok(BrowserCommandResult::Evaluated {
                    surface_id,
                    value_json: r#"{"ok":true}"#.to_string(),
                })
            }
        }
    }
}

pub struct CdpBrowserAutomation {
    executable: PathBuf,
    surfaces: HashMap<String, CdpBrowserSurface>,
    next_browser_id: u64,
    startup_timeout: Duration,
    headless: bool,
}

struct CdpBrowserSurface {
    surface: BrowserSurface,
    websocket_url: String,
    downloads_dir: PathBuf,
    child: Child,
    _profile_dir: TempDir,
}

impl Drop for CdpBrowserSurface {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl CdpBrowserAutomation {
    pub fn new() -> BrowserAutomationResult<Self> {
        let executable = discover_browser_executable().ok_or_else(|| {
            BrowserAutomationError::automation_failed(
                "No Chrome, Edge, or Chromium executable was found for browser automation.",
            )
        })?;
        Ok(Self::with_executable(executable))
    }

    pub fn with_executable(executable: impl Into<PathBuf>) -> Self {
        let headless = env::var("AGENTMUX_BROWSER_HEADLESS")
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        Self::with_executable_and_headless(executable, headless)
    }

    pub fn with_executable_and_headless(executable: impl Into<PathBuf>, headless: bool) -> Self {
        Self {
            executable: executable.into(),
            surfaces: HashMap::new(),
            next_browser_id: 0,
            startup_timeout: Duration::from_secs(10),
            headless,
        }
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    fn require_surface(&self, surface_id: &str) -> BrowserAutomationResult<&CdpBrowserSurface> {
        self.surfaces
            .get(surface_id)
            .ok_or_else(|| BrowserAutomationError::surface_not_found(surface_id))
    }

    fn launch_surface(
        &mut self,
        surface_id: String,
        workspace_id: String,
        profile: Option<String>,
    ) -> BrowserAutomationResult<BrowserSurface> {
        validate_surface_create(
            &surface_id,
            &workspace_id,
            self.surfaces.contains_key(&surface_id),
        )?;
        if !self.executable.is_file() {
            return Err(BrowserAutomationError::automation_failed(format!(
                "Configured browser executable '{}' does not exist.",
                self.executable.display()
            )));
        }

        let port = allocate_loopback_port()?;
        let profile_dir = tempfile::Builder::new()
            .prefix("agentmux-browser-")
            .tempdir()
            .map_err(|error| {
                BrowserAutomationError::automation_failed(format!(
                    "Failed to create browser profile directory: {error}"
                ))
            })?;
        let downloads_dir = profile_dir.path().join("downloads");
        fs::create_dir_all(&downloads_dir).map_err(|error| {
            BrowserAutomationError::automation_failed(format!(
                "Failed to create browser download directory: {error}"
            ))
        })?;
        let mut command = Command::new(&self.executable);
        command
            .arg(format!("--remote-debugging-port={port}"))
            .arg("--remote-debugging-address=127.0.0.1")
            .arg(format!("--user-data-dir={}", profile_dir.path().display()))
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-background-networking")
            .arg("--disable-sync");
        if self.headless {
            command.arg("--headless=new").arg("--disable-gpu");
        }
        let mut child = command
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                BrowserAutomationError::automation_failed(format!(
                    "Failed to launch browser '{}': {error}",
                    self.executable.display()
                ))
            })?;

        let websocket_url = match wait_for_cdp_target(port, self.startup_timeout) {
            Ok(websocket_url) => websocket_url,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        if let Err(error) = install_browser_recorders(&websocket_url) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        let _ = configure_download_behavior(&websocket_url, &downloads_dir);

        self.next_browser_id += 1;
        let surface = BrowserSurface {
            surface_id: surface_id.clone(),
            browser_id: format!("cdp_browser_{:08}", self.next_browser_id),
            workspace_id,
            current_url: Some("about:blank".to_string()),
            profile,
        };
        self.surfaces.insert(
            surface_id,
            CdpBrowserSurface {
                surface: surface.clone(),
                websocket_url,
                downloads_dir,
                child,
                _profile_dir: profile_dir,
            },
        );
        Ok(surface)
    }

    fn execute_navigate(
        &mut self,
        surface_id: String,
        url: String,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if url.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Navigation URL must not be empty.",
            ));
        }
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let result = cdp_call(&websocket_url, "Page.navigate", json!({ "url": url }))?;
        if let Some(error_text) = result.get("errorText").and_then(Value::as_str) {
            return Err(BrowserAutomationError::automation_failed(format!(
                "Browser navigation failed: {error_text}"
            )));
        }
        thread::sleep(Duration::from_millis(100));
        if let Some(surface) = self.surfaces.get_mut(&surface_id) {
            surface.surface.current_url = Some(url.clone());
        }
        Ok(BrowserCommandResult::Navigated { surface_id, url })
    }

    fn execute_current_url(
        &mut self,
        surface_id: String,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let url = self.read_current_url(&surface_id, &websocket_url)?;
        Ok(BrowserCommandResult::Navigated { surface_id, url })
    }

    fn execute_reload(
        &mut self,
        surface_id: String,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        cdp_call(&websocket_url, "Page.reload", json!({}))?;
        thread::sleep(Duration::from_millis(100));
        let url = self.read_current_url(&surface_id, &websocket_url)?;
        Ok(BrowserCommandResult::Navigated { surface_id, url })
    }

    fn execute_history_delta(
        &mut self,
        surface_id: String,
        delta: i64,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let history = cdp_call(&websocket_url, "Page.getNavigationHistory", json!({}))?;
        let current_index = history
            .get("currentIndex")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let target_index = current_index + delta;
        if target_index >= 0 {
            if let Some(entry) = history
                .get("entries")
                .and_then(Value::as_array)
                .and_then(|entries| entries.get(target_index as usize))
            {
                if let Some(entry_id) = entry.get("id").and_then(Value::as_i64) {
                    cdp_call(
                        &websocket_url,
                        "Page.navigateToHistoryEntry",
                        json!({ "entryId": entry_id }),
                    )?;
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
        let url = self.read_current_url(&surface_id, &websocket_url)?;
        Ok(BrowserCommandResult::Navigated { surface_id, url })
    }

    fn read_current_url(
        &mut self,
        surface_id: &str,
        websocket_url: &str,
    ) -> BrowserAutomationResult<String> {
        let result = cdp_call(
            websocket_url,
            "Runtime.evaluate",
            json!({
                "expression": "window.location.href",
                "returnByValue": true,
                "awaitPromise": true,
            }),
        )?;
        let url = runtime_result_value(&result)?
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| "about:blank".to_string());
        if let Some(surface) = self.surfaces.get_mut(surface_id) {
            surface.surface.current_url = Some(url.clone());
        }
        Ok(url)
    }

    fn execute_screenshot(
        &self,
        surface_id: String,
        format: String,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let format = normalize_screenshot_format(&format)?;
        let result = cdp_call(
            &websocket_url,
            "Page.captureScreenshot",
            json!({ "format": format }),
        )?;
        let data = result
            .get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| BrowserAutomationError::automation_failed("Screenshot data missing."))?;
        let bytes = BASE64.decode(data).map_err(|error| {
            BrowserAutomationError::automation_failed(format!(
                "Failed to decode screenshot data: {error}"
            ))
        })?;
        Ok(BrowserCommandResult::Screenshot {
            surface_id,
            format,
            bytes,
        })
    }

    fn execute_dom_snapshot(
        &self,
        surface_id: String,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let result = cdp_runtime_evaluate(
            &websocket_url,
            "document.documentElement ? document.documentElement.outerHTML : ''".to_string(),
            frame_id.as_deref(),
        )?;
        let html = runtime_result_value(&result)?
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(String::new);
        Ok(BrowserCommandResult::DomSnapshot { surface_id, html })
    }

    fn execute_frames(&self, surface_id: String) -> BrowserAutomationResult<BrowserCommandResult> {
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let result = cdp_call(&websocket_url, "Page.getFrameTree", json!({}))?;
        let mut frames = Vec::new();
        collect_frame_tree(result.get("frameTree"), None, &mut frames);
        Ok(BrowserCommandResult::Frames { surface_id, frames })
    }

    fn execute_storage_snapshot(
        &self,
        surface_id: String,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let result = cdp_call(
            &websocket_url,
            "Runtime.evaluate",
            json!({
                "expression": r#"(() => {
  const toEntries = (storage) => {
    const entries = [];
    for (let index = 0; index < storage.length; index += 1) {
      const key = storage.key(index);
      entries.push({ key, value: storage.getItem(key) ?? "" });
    }
    entries.sort((left, right) => String(left.key).localeCompare(String(right.key)));
    return entries;
  };
  return {
    localStorage: toEntries(window.localStorage),
    sessionStorage: toEntries(window.sessionStorage)
  };
})()"#,
                "returnByValue": true,
                "awaitPromise": true,
            }),
        )?;
        let value = runtime_result_value(&result)?;
        Ok(BrowserCommandResult::StorageSnapshot {
            surface_id,
            local_storage: storage_entries_from_value(value.get("localStorage")),
            session_storage: storage_entries_from_value(value.get("sessionStorage")),
        })
    }

    fn execute_cookies(&self, surface_id: String) -> BrowserAutomationResult<BrowserCommandResult> {
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let result = cdp_call(&websocket_url, "Network.getCookies", json!({}))?;
        let cookies = result
            .get("cookies")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(cookie_info_from_value).collect())
            .unwrap_or_default();
        Ok(BrowserCommandResult::Cookies {
            surface_id,
            cookies,
        })
    }

    fn execute_downloads(
        &self,
        surface_id: String,
        limit: usize,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        let surface = self.require_surface(&surface_id)?;
        let _ = configure_download_behavior(&surface.websocket_url, &surface.downloads_dir);
        let downloads = downloads_from_directory(&surface.downloads_dir, limit.clamp(1, 500))?;
        Ok(BrowserCommandResult::Downloads {
            directory: surface.downloads_dir.display().to_string(),
            surface_id,
            downloads,
        })
    }

    fn execute_history(&self, surface_id: String) -> BrowserAutomationResult<BrowserCommandResult> {
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let result = cdp_call(&websocket_url, "Page.getNavigationHistory", json!({}))?;
        Ok(BrowserCommandResult::History {
            surface_id,
            current_index: result
                .get("currentIndex")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            entries: history_entries_from_value(result.get("entries")),
        })
    }

    fn execute_console_messages(
        &self,
        surface_id: String,
        limit: usize,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let limit = limit.clamp(1, 500);
        let expression = format!(
            r#"(function () {{
  {};
  return (window.__agentmuxConsoleMessages || []).slice(-{});
}})()"#,
            AGENTMUX_CONSOLE_RECORDER_SOURCE, limit
        );
        let result = cdp_call(
            &websocket_url,
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true,
            }),
        )?;
        let value = runtime_result_value(&result)?;
        Ok(BrowserCommandResult::ConsoleMessages {
            surface_id,
            messages: console_messages_from_value(Some(&value)),
        })
    }

    fn execute_dialog_messages(
        &self,
        surface_id: String,
        limit: usize,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let limit = limit.clamp(1, 500);
        let expression = format!(
            r#"(function () {{
  {};
  return (window.__agentmuxDialogMessages || []).slice(-{});
}})()"#,
            AGENTMUX_DIALOG_RECORDER_SOURCE, limit
        );
        let result = cdp_call(
            &websocket_url,
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true,
            }),
        )?;
        let value = runtime_result_value(&result)?;
        Ok(BrowserCommandResult::DialogMessages {
            surface_id,
            messages: dialog_messages_from_value(Some(&value)),
        })
    }

    fn execute_error_events(
        &self,
        surface_id: String,
        limit: usize,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let limit = limit.clamp(1, 500);
        let expression = format!(
            r#"(function () {{
  {};
  return (window.__agentmuxErrorEvents || []).slice(-{});
}})()"#,
            AGENTMUX_ERROR_RECORDER_SOURCE, limit
        );
        let result = cdp_call(
            &websocket_url,
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true,
            }),
        )?;
        let value = runtime_result_value(&result)?;
        Ok(BrowserCommandResult::ErrorEvents {
            surface_id,
            events: error_events_from_value(Some(&value)),
        })
    }

    fn execute_click_selector(
        &self,
        surface_id: String,
        selector: String,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if selector.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Click selector must not be empty.",
            ));
        }
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let expression = format!(
            r##"(() => {{
  const selector = {};
  const element = document.querySelector(selector);
  if (!element) {{
    throw new Error(`No element matches selector: ${{selector}}`);
  }}
  element.scrollIntoView({{ block: "center", inline: "center" }});
  element.click();
  return true;
}})()"##,
            json_string_literal(&selector)?
        );
        let result = cdp_runtime_evaluate(&websocket_url, expression, frame_id.as_deref())?;
        runtime_result_value(&result)?;
        Ok(BrowserCommandResult::Clicked {
            surface_id,
            target: selector,
        })
    }

    fn execute_click_point(
        &self,
        surface_id: String,
        x: i32,
        y: i32,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if x < 0 || y < 0 {
            return Err(BrowserAutomationError::invalid_request(
                "Click coordinates must be non-negative.",
            ));
        }
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let point = json!({
            "x": x,
            "y": y,
            "button": "left",
            "clickCount": 1,
        });
        cdp_call(
            &websocket_url,
            "Input.dispatchMouseEvent",
            with_type(point.clone(), "mouseMoved"),
        )?;
        cdp_call(
            &websocket_url,
            "Input.dispatchMouseEvent",
            with_type(point.clone(), "mousePressed"),
        )?;
        cdp_call(
            &websocket_url,
            "Input.dispatchMouseEvent",
            with_type(point, "mouseReleased"),
        )?;
        Ok(BrowserCommandResult::Clicked {
            surface_id,
            target: format!("{x},{y}"),
        })
    }

    fn execute_type_text(
        &self,
        surface_id: String,
        selector: String,
        text: String,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if selector.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Type selector must not be empty.",
            ));
        }
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let expression = format!(
            r##"(() => {{
  const selector = {};
  const element = document.querySelector(selector);
  if (!element) {{
    throw new Error(`No element matches selector: ${{selector}}`);
  }}
  element.focus();
  return true;
}})()"##,
            json_string_literal(&selector)?
        );
        let result = cdp_runtime_evaluate(&websocket_url, expression, frame_id.as_deref())?;
        runtime_result_value(&result)?;
        cdp_call(&websocket_url, "Input.insertText", json!({ "text": text }))?;
        Ok(BrowserCommandResult::Typed {
            surface_id,
            selector,
            text,
        })
    }

    fn execute_fill_text(
        &self,
        surface_id: String,
        selector: String,
        text: String,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if selector.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Fill selector must not be empty.",
            ));
        }
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let expression = format!(
            r#"(() => {{
  const selector = {};
  const text = {};
  const element = document.querySelector(selector);
  if (!element) {{
    throw new Error(`No element matches selector: ${{selector}}`);
  }}
  element.scrollIntoView({{ block: "center", inline: "center" }});
  element.focus();
  if ("value" in element) {{
    element.value = text;
  }} else {{
    element.textContent = text;
  }}
  element.dispatchEvent(new Event("input", {{ bubbles: true }}));
  element.dispatchEvent(new Event("change", {{ bubbles: true }}));
  return true;
}})()"#,
            json_string_literal(&selector)?,
            json_string_literal(&text)?
        );
        let result = cdp_runtime_evaluate(&websocket_url, expression, frame_id.as_deref())?;
        runtime_result_value(&result)?;
        Ok(BrowserCommandResult::Filled {
            surface_id,
            selector,
            text,
        })
    }

    fn execute_press_key(
        &self,
        surface_id: String,
        selector: String,
        key: String,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if selector.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Press selector must not be empty.",
            ));
        }
        let key = normalize_browser_key(&key)?;
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let expression = format!(
            r#"(() => {{
  const selector = {};
  const element = document.querySelector(selector);
  if (!element) {{
    throw new Error(`No element matches selector: ${{selector}}`);
  }}
  element.scrollIntoView({{ block: "center", inline: "center" }});
  element.focus();
  return true;
}})()"#,
            json_string_literal(&selector)?
        );
        let result = cdp_runtime_evaluate(&websocket_url, expression, frame_id.as_deref())?;
        runtime_result_value(&result)?;
        cdp_call(
            &websocket_url,
            "Input.dispatchKeyEvent",
            browser_key_event_params("keyDown", &key),
        )?;
        cdp_call(
            &websocket_url,
            "Input.dispatchKeyEvent",
            browser_key_event_params("keyUp", &key),
        )?;
        Ok(BrowserCommandResult::Pressed {
            surface_id,
            selector,
            key,
        })
    }

    fn execute_select_values(
        &self,
        surface_id: String,
        selector: String,
        values: Vec<String>,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if selector.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Select selector must not be empty.",
            ));
        }
        if values.is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Select values must not be empty.",
            ));
        }
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let expression = format!(
            r#"(() => {{
  const selector = {};
  const values = {};
  const element = document.querySelector(selector);
  if (!element) {{
    throw new Error(`No element matches selector: ${{selector}}`);
  }}
  if (!(element instanceof HTMLSelectElement)) {{
    throw new Error(`Element is not a select: ${{selector}}`);
  }}
  if (!element.multiple && values.length > 1) {{
    throw new Error("Cannot select multiple values on a single-select element.");
  }}
  const wanted = new Set(values.map(String));
  let matched = 0;
  for (const option of element.options) {{
    const selected = wanted.has(option.value) || wanted.has(option.label) || wanted.has(option.text);
    option.selected = selected;
    if (selected) {{
      matched += 1;
    }}
  }}
  if (matched === 0) {{
    throw new Error("No select options matched the requested value.");
  }}
  element.dispatchEvent(new Event("input", {{ bubbles: true }}));
  element.dispatchEvent(new Event("change", {{ bubbles: true }}));
  return Array.from(element.selectedOptions).map((option) => option.value);
}})()"#,
            json_string_literal(&selector)?,
            serde_json::to_string(&values).map_err(|error| {
                BrowserAutomationError::automation_failed(format!(
                    "Failed to encode select values: {error}"
                ))
            })?
        );
        let result = cdp_runtime_evaluate(&websocket_url, expression, frame_id.as_deref())?;
        let selected_values = runtime_result_value(&result)?
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| values.clone());
        Ok(BrowserCommandResult::Selected {
            surface_id,
            selector,
            values: selected_values,
        })
    }

    fn execute_scroll_by(
        &self,
        surface_id: String,
        selector: Option<String>,
        x: i32,
        y: i32,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if selector
            .as_deref()
            .map(|value| value.trim().is_empty())
            .unwrap_or(false)
        {
            return Err(BrowserAutomationError::invalid_request(
                "Scroll selector must not be empty.",
            ));
        }
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let expression = if let Some(selector) = selector.as_deref() {
            format!(
                r#"(() => {{
  const selector = {};
  const element = document.querySelector(selector);
  if (!element) {{
    throw new Error(`No element matches selector: ${{selector}}`);
  }}
  element.scrollBy({{ left: {}, top: {}, behavior: "instant" }});
  return true;
}})()"#,
                json_string_literal(selector)?,
                x,
                y
            )
        } else {
            format!(
                r#"(() => {{
  window.scrollBy({{ left: {}, top: {}, behavior: "instant" }});
  return true;
}})()"#,
                x, y
            )
        };
        let result = cdp_runtime_evaluate(&websocket_url, expression, frame_id.as_deref())?;
        runtime_result_value(&result)?;
        Ok(BrowserCommandResult::Scrolled {
            surface_id,
            target: selector.unwrap_or_else(|| "window".to_string()),
            x,
            y,
        })
    }

    fn execute_hover_selector(
        &self,
        surface_id: String,
        selector: String,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if selector.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Hover selector must not be empty.",
            ));
        }
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let expression = format!(
            r#"(() => {{
  const selector = {};
  const element = document.querySelector(selector);
  if (!element) {{
    throw new Error(`No element matches selector: ${{selector}}`);
  }}
  element.scrollIntoView({{ block: "center", inline: "center" }});
  const rect = element.getBoundingClientRect();
  return {{
    x: Math.round(rect.left + rect.width / 2),
    y: Math.round(rect.top + rect.height / 2)
  }};
}})()"#,
            json_string_literal(&selector)?
        );
        let result = cdp_runtime_evaluate(&websocket_url, expression, frame_id.as_deref())?;
        let point = runtime_result_value(&result)?;
        let x = point.get("x").and_then(Value::as_i64).unwrap_or(0);
        let y = point.get("y").and_then(Value::as_i64).unwrap_or(0);
        cdp_call(
            &websocket_url,
            "Input.dispatchMouseEvent",
            json!({
                "type": "mouseMoved",
                "x": x,
                "y": y,
            }),
        )?;
        Ok(BrowserCommandResult::Hovered {
            surface_id,
            selector,
        })
    }

    fn execute_check_selector(
        &self,
        surface_id: String,
        selector: String,
        checked: bool,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if selector.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Check selector must not be empty.",
            ));
        }
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let expression = format!(
            r#"(() => {{
  const selector = {};
  const checked = {};
  const element = document.querySelector(selector);
  if (!element) {{
    throw new Error(`No element matches selector: ${{selector}}`);
  }}
  if (!("checked" in element)) {{
    throw new Error(`Element is not checkable: ${{selector}}`);
  }}
  element.scrollIntoView({{ block: "center", inline: "center" }});
  element.focus();
  element.checked = checked;
  element.dispatchEvent(new Event("input", {{ bubbles: true }}));
  element.dispatchEvent(new Event("change", {{ bubbles: true }}));
  return element.checked;
}})()"#,
            json_string_literal(&selector)?,
            checked
        );
        let result = cdp_runtime_evaluate(&websocket_url, expression, frame_id.as_deref())?;
        let checked = runtime_result_value(&result)?.as_bool().unwrap_or(checked);
        Ok(BrowserCommandResult::Checked {
            surface_id,
            selector,
            checked,
        })
    }

    fn execute_get_element(
        &self,
        surface_id: String,
        selector: String,
        kind: String,
        attribute: Option<String>,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if selector.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Get selector must not be empty.",
            ));
        }
        let kind = normalize_browser_get_kind(&kind, attribute.as_deref())?;
        let attribute_name = kind.strip_prefix("attribute:").unwrap_or("");
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let expression = format!(
            r#"(() => {{
  const selector = {};
  const kind = {};
  const attribute = {};
  const element = document.querySelector(selector);
  if (!element) {{
    throw new Error(`No element matches selector: ${{selector}}`);
  }}
  if (kind === "text") {{
    return element.innerText ?? element.textContent ?? "";
  }}
  if (kind === "html") {{
    return element.outerHTML ?? "";
  }}
  if (kind === "value") {{
    return "value" in element ? String(element.value ?? "") : "";
  }}
  if (kind.startsWith("attribute:")) {{
    return element.getAttribute(attribute) ?? "";
  }}
  throw new Error(`Unsupported get kind: ${{kind}}`);
}})()"#,
            json_string_literal(&selector)?,
            json_string_literal(&kind)?,
            json_string_literal(attribute_name)?
        );
        let result = cdp_runtime_evaluate(&websocket_url, expression, frame_id.as_deref())?;
        let value = runtime_result_value(&result)?
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(String::new);
        Ok(BrowserCommandResult::Got {
            surface_id,
            selector,
            kind,
            value,
        })
    }

    fn execute_find_text(
        &self,
        surface_id: String,
        query: String,
        selector: Option<String>,
        limit: u16,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if query.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Find query must not be empty.",
            ));
        }
        if selector
            .as_deref()
            .map(|value| value.trim().is_empty())
            .unwrap_or(false)
        {
            return Err(BrowserAutomationError::invalid_request(
                "Find selector must not be empty.",
            ));
        }
        let limit = limit.max(1);
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let expression = format!(
            r#"(() => {{
  const query = {};
  const scopeSelector = {};
  const limit = {};
  const root = scopeSelector ? document.querySelector(scopeSelector) : document.body;
  if (!root) {{
    throw new Error(`No element matches selector: ${{scopeSelector}}`);
  }}
  const needle = query.toLowerCase();
  const matches = [];
  let count = 0;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  while (walker.nextNode()) {{
    const text = walker.currentNode.nodeValue || "";
    if (!text.toLowerCase().includes(needle)) {{
      continue;
    }}
    count += 1;
    if (matches.length < limit) {{
      const parent = walker.currentNode.parentElement;
      const label = parent
        ? `${{parent.tagName.toLowerCase()}}${{parent.id ? String.fromCharCode(35) + parent.id : ""}}`
        : "text";
      const compact = text.replace(/\s+/g, " ").trim();
      matches.push(`${{label}}: ${{compact.slice(0, 160)}}`);
    }}
  }}
  return {{ count, matches }};
}})()"#,
            json_string_literal(&query)?,
            json_string_literal(selector.as_deref().unwrap_or(""))?,
            limit
        );
        let result = cdp_runtime_evaluate(&websocket_url, expression, frame_id.as_deref())?;
        let value = runtime_result_value(&result)?;
        let count = value
            .get("count")
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok())
            .unwrap_or(0);
        let matches = value
            .get("matches")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(BrowserCommandResult::Found {
            surface_id,
            query,
            count,
            matches,
        })
    }

    fn execute_highlight_selector(
        &self,
        surface_id: String,
        selector: String,
        duration_ms: u64,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if selector.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Highlight selector must not be empty.",
            ));
        }
        let duration_ms = duration_ms.max(1);
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let expression = format!(
            r#"(() => {{
  const selector = {};
  const durationMs = {};
  const element = document.querySelector(selector);
  if (!element) {{
    throw new Error(`No element matches selector: ${{selector}}`);
  }}
  element.scrollIntoView({{ block: "center", inline: "center" }});
  const previousOutline = element.style.outline;
  const previousBoxShadow = element.style.boxShadow;
  element.style.outline = "3px solid #f59e0b";
  element.style.boxShadow = "0 0 0 4px rgba(245,158,11,0.35)";
  setTimeout(() => {{
    element.style.outline = previousOutline;
    element.style.boxShadow = previousBoxShadow;
  }}, durationMs);
  return true;
}})()"#,
            json_string_literal(&selector)?,
            duration_ms
        );
        let result = cdp_runtime_evaluate(&websocket_url, expression, frame_id.as_deref())?;
        runtime_result_value(&result)?;
        Ok(BrowserCommandResult::Highlighted {
            surface_id,
            selector,
            duration_ms,
        })
    }

    fn execute_focus_selector(
        &self,
        surface_id: String,
        selector: String,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if selector.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Focus selector must not be empty.",
            ));
        }
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let expression = format!(
            r#"(() => {{
  const selector = {};
  const element = document.querySelector(selector);
  if (!element) {{
    throw new Error(`No element matches selector: ${{selector}}`);
  }}
  element.scrollIntoView({{ block: "center", inline: "center" }});
  element.focus();
  return true;
}})()"#,
            json_string_literal(&selector)?
        );
        let result = cdp_runtime_evaluate(&websocket_url, expression, frame_id.as_deref())?;
        runtime_result_value(&result)?;
        Ok(BrowserCommandResult::Focused {
            surface_id,
            selector,
        })
    }

    fn execute_set_zoom(
        &self,
        surface_id: String,
        percent: u16,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        validate_zoom_percent(percent)?;
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        cdp_call(
            &websocket_url,
            "Emulation.setPageScaleFactor",
            json!({ "pageScaleFactor": f64::from(percent) / 100.0 }),
        )?;
        Ok(BrowserCommandResult::Zoomed {
            surface_id,
            percent,
        })
    }

    fn execute_evaluate(
        &self,
        surface_id: String,
        script: String,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if script.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Evaluate script must not be empty.",
            ));
        }
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let result = cdp_runtime_evaluate(&websocket_url, script, frame_id.as_deref())?;
        let value_json =
            serde_json::to_string(&runtime_result_value(&result)?).map_err(|error| {
                BrowserAutomationError::automation_failed(format!(
                    "Failed to serialize browser evaluation result: {error}"
                ))
            })?;
        Ok(BrowserCommandResult::Evaluated {
            surface_id,
            value_json,
        })
    }

    fn execute_wait_for_selector(
        &self,
        surface_id: String,
        selector: String,
        timeout_ms: u64,
        frame_id: Option<String>,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if selector.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Wait selector must not be empty.",
            ));
        }
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let timeout_ms = timeout_ms.max(1);
        let started = Instant::now();
        let expression = format!(
            r#"new Promise((resolve, reject) => {{
  const selector = {};
  const deadline = Date.now() + {};
  const tick = () => {{
    const element = document.querySelector(selector);
    if (element) {{
      resolve(true);
      return;
    }}
    if (Date.now() >= deadline) {{
      reject(new Error(`Timed out waiting for selector: ${{selector}}`));
      return;
    }}
    setTimeout(tick, 50);
  }};
  tick();
}})"#,
            json_string_literal(&selector)?,
            timeout_ms
        );
        let result = cdp_runtime_evaluate(&websocket_url, expression, frame_id.as_deref())?;
        runtime_result_value(&result)?;
        Ok(BrowserCommandResult::WaitedForSelector {
            surface_id,
            selector,
            elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        })
    }
}

fn cdp_runtime_evaluate(
    websocket_url: &str,
    expression: String,
    frame_id: Option<&str>,
) -> BrowserAutomationResult<Value> {
    let mut params = json!({
        "expression": expression,
        "returnByValue": true,
        "awaitPromise": true,
    });
    if let Some(frame_id) = frame_id.map(str::trim).filter(|value| !value.is_empty()) {
        let context = cdp_call(
            websocket_url,
            "Page.createIsolatedWorld",
            json!({
                "frameId": frame_id,
                "worldName": "agentmux",
                "grantUniveralAccess": true,
            }),
        )?;
        let Some(context_id) = context.get("executionContextId").and_then(Value::as_i64) else {
            return Err(BrowserAutomationError::automation_failed(
                "CDP did not return an execution context for the requested frame.",
            ));
        };
        if let Value::Object(map) = &mut params {
            map.insert("contextId".to_string(), Value::Number(context_id.into()));
        }
    }
    cdp_call(websocket_url, "Runtime.evaluate", params)
}

impl BrowserAutomation for CdpBrowserAutomation {
    fn create_surface(
        &mut self,
        surface_id: String,
        workspace_id: String,
        profile: Option<String>,
    ) -> BrowserAutomationResult<BrowserSurface> {
        self.launch_surface(surface_id, workspace_id, profile)
    }

    fn surface(&self, surface_id: &str) -> BrowserAutomationResult<BrowserSurface> {
        Ok(self.require_surface(surface_id)?.surface.clone())
    }

    fn close_surface(&mut self, surface_id: &str) -> BrowserAutomationResult<BrowserSurface> {
        self.surfaces
            .remove(surface_id)
            .map(|surface| surface.surface.clone())
            .ok_or_else(|| BrowserAutomationError::surface_not_found(surface_id))
    }

    fn execute(
        &mut self,
        command: BrowserCommand,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        match command {
            BrowserCommand::Navigate { surface_id, url } => self.execute_navigate(surface_id, url),
            BrowserCommand::Reload { surface_id } => self.execute_reload(surface_id),
            BrowserCommand::GoBack { surface_id } => self.execute_history_delta(surface_id, -1),
            BrowserCommand::GoForward { surface_id } => self.execute_history_delta(surface_id, 1),
            BrowserCommand::CurrentUrl { surface_id } => self.execute_current_url(surface_id),
            BrowserCommand::Screenshot { surface_id, format } => {
                self.execute_screenshot(surface_id, format)
            }
            BrowserCommand::DomSnapshot {
                surface_id,
                frame_id,
            } => self.execute_dom_snapshot(surface_id, frame_id),
            BrowserCommand::Frames { surface_id } => self.execute_frames(surface_id),
            BrowserCommand::StorageSnapshot { surface_id } => {
                self.execute_storage_snapshot(surface_id)
            }
            BrowserCommand::Cookies { surface_id } => self.execute_cookies(surface_id),
            BrowserCommand::Downloads { surface_id, limit } => {
                self.execute_downloads(surface_id, limit)
            }
            BrowserCommand::History { surface_id } => self.execute_history(surface_id),
            BrowserCommand::ConsoleMessages { surface_id, limit } => {
                self.execute_console_messages(surface_id, limit)
            }
            BrowserCommand::DialogMessages { surface_id, limit } => {
                self.execute_dialog_messages(surface_id, limit)
            }
            BrowserCommand::ErrorEvents { surface_id, limit } => {
                self.execute_error_events(surface_id, limit)
            }
            BrowserCommand::ClickSelector {
                surface_id,
                selector,
                frame_id,
            } => self.execute_click_selector(surface_id, selector, frame_id),
            BrowserCommand::ClickPoint { surface_id, x, y } => {
                self.execute_click_point(surface_id, x, y)
            }
            BrowserCommand::TypeText {
                surface_id,
                selector,
                text,
                frame_id,
            } => self.execute_type_text(surface_id, selector, text, frame_id),
            BrowserCommand::FillText {
                surface_id,
                selector,
                text,
                frame_id,
            } => self.execute_fill_text(surface_id, selector, text, frame_id),
            BrowserCommand::PressKey {
                surface_id,
                selector,
                key,
                frame_id,
            } => self.execute_press_key(surface_id, selector, key, frame_id),
            BrowserCommand::SelectValues {
                surface_id,
                selector,
                values,
                frame_id,
            } => self.execute_select_values(surface_id, selector, values, frame_id),
            BrowserCommand::ScrollBy {
                surface_id,
                selector,
                x,
                y,
                frame_id,
            } => self.execute_scroll_by(surface_id, selector, x, y, frame_id),
            BrowserCommand::HoverSelector {
                surface_id,
                selector,
                frame_id,
            } => self.execute_hover_selector(surface_id, selector, frame_id),
            BrowserCommand::CheckSelector {
                surface_id,
                selector,
                checked,
                frame_id,
            } => self.execute_check_selector(surface_id, selector, checked, frame_id),
            BrowserCommand::GetElement {
                surface_id,
                selector,
                kind,
                attribute,
                frame_id,
            } => self.execute_get_element(surface_id, selector, kind, attribute, frame_id),
            BrowserCommand::FindText {
                surface_id,
                query,
                selector,
                limit,
                frame_id,
            } => self.execute_find_text(surface_id, query, selector, limit, frame_id),
            BrowserCommand::HighlightSelector {
                surface_id,
                selector,
                duration_ms,
                frame_id,
            } => self.execute_highlight_selector(surface_id, selector, duration_ms, frame_id),
            BrowserCommand::FocusSelector {
                surface_id,
                selector,
                frame_id,
            } => self.execute_focus_selector(surface_id, selector, frame_id),
            BrowserCommand::SetZoom {
                surface_id,
                percent,
            } => self.execute_set_zoom(surface_id, percent),
            BrowserCommand::WaitForSelector {
                surface_id,
                selector,
                timeout_ms,
                frame_id,
            } => self.execute_wait_for_selector(surface_id, selector, timeout_ms, frame_id),
            BrowserCommand::Evaluate {
                surface_id,
                script,
                frame_id,
            } => self.execute_evaluate(surface_id, script, frame_id),
        }
    }
}

pub fn discover_browser_executable() -> Option<PathBuf> {
    if let Some(path) = env::var_os("AGENTMUX_BROWSER_EXECUTABLE").map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }

    browser_executable_candidates()
        .into_iter()
        .find(|candidate| candidate.is_file())
}

fn browser_executable_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if cfg!(windows) {
        for base in ["LOCALAPPDATA", "PROGRAMFILES", "PROGRAMFILES(X86)"] {
            if let Some(base) = env::var_os(base).map(PathBuf::from) {
                candidates.push(base.join("Microsoft/Edge/Application/msedge.exe"));
                candidates.push(base.join("Google/Chrome/Application/chrome.exe"));
                candidates.push(base.join("Chromium/Application/chrome.exe"));
            }
        }
        candidates.extend(path_executable_candidates(&[
            "msedge.exe",
            "chrome.exe",
            "chromium.exe",
        ]));
    } else {
        candidates.extend(path_executable_candidates(&[
            "google-chrome",
            "chromium",
            "chromium-browser",
            "microsoft-edge",
            "msedge",
            "chrome",
        ]));
    }
    candidates
}

fn path_executable_candidates(names: &[&str]) -> Vec<PathBuf> {
    env::var_os("PATH")
        .map(|path| {
            env::split_paths(&path)
                .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
                .collect()
        })
        .unwrap_or_default()
}

fn validate_surface_create(
    surface_id: &str,
    workspace_id: &str,
    exists: bool,
) -> BrowserAutomationResult<()> {
    if surface_id.trim().is_empty() {
        return Err(BrowserAutomationError::invalid_request(
            "Browser surface id must not be empty.",
        ));
    }
    if workspace_id.trim().is_empty() {
        return Err(BrowserAutomationError::invalid_request(
            "Browser workspace id must not be empty.",
        ));
    }
    if exists {
        return Err(BrowserAutomationError::invalid_request(format!(
            "Browser surface '{surface_id}' already exists."
        )));
    }
    Ok(())
}

fn allocate_loopback_port() -> BrowserAutomationResult<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|error| {
        BrowserAutomationError::automation_failed(format!(
            "Failed to allocate browser debugging port: {error}"
        ))
    })?;
    let port = listener
        .local_addr()
        .map_err(|error| {
            BrowserAutomationError::automation_failed(format!(
                "Failed to read browser debugging port: {error}"
            ))
        })?
        .port();
    drop(listener);
    Ok(port)
}

fn wait_for_cdp_target(port: u16, timeout: Duration) -> BrowserAutomationResult<String> {
    let deadline = Instant::now() + timeout;
    loop {
        if cdp_http_json("GET", port, "/json/version").is_ok() {
            break;
        }
        if Instant::now() >= deadline {
            return Err(BrowserAutomationError::automation_failed(
                "Timed out waiting for browser debugging endpoint.",
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }

    let target = cdp_http_json("PUT", port, "/json/new?about:blank")
        .or_else(|_| cdp_http_json("GET", port, "/json/new?about:blank"))?;
    target
        .get("webSocketDebuggerUrl")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            BrowserAutomationError::automation_failed(
                "Browser debugging target did not expose a WebSocket URL.",
            )
        })
}

fn cdp_http_json(method: &str, port: u16, path: &str) -> BrowserAutomationResult<Value> {
    let body = cdp_http_request(method, port, path)?;
    serde_json::from_str(&body).map_err(|error| {
        BrowserAutomationError::automation_failed(format!(
            "Failed to parse browser debugging response: {error}"
        ))
    })
}

fn cdp_http_request(method: &str, port: u16, path: &str) -> BrowserAutomationResult<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|error| {
        BrowserAutomationError::automation_failed(format!(
            "Failed to connect to browser debugging endpoint: {error}"
        ))
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| {
            BrowserAutomationError::automation_failed(format!(
                "Failed to configure browser debugging read timeout: {error}"
            ))
        })?;
    let request =
        format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).map_err(|error| {
        BrowserAutomationError::automation_failed(format!(
            "Failed to write browser debugging request: {error}"
        ))
    })?;

    let response = read_http_response(&mut stream)?;
    let (headers, body) = response.split_once("\r\n\r\n").ok_or_else(|| {
        BrowserAutomationError::automation_failed("Malformed browser debugging response.")
    })?;
    let status_line = headers.lines().next().unwrap_or_default();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(0);
    if !(200..300).contains(&status) {
        return Err(BrowserAutomationError::automation_failed(format!(
            "Browser debugging endpoint returned HTTP {status}."
        )));
    }
    Ok(body.to_string())
}

fn read_http_response(stream: &mut TcpStream) -> BrowserAutomationResult<String> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                bytes.extend_from_slice(&buffer[..count]);
                if http_response_complete(&bytes) {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) && !bytes.is_empty() =>
            {
                break;
            }
            Err(error) => {
                return Err(BrowserAutomationError::automation_failed(format!(
                    "Failed to read browser debugging response: {error}"
                )));
            }
        }
    }
    String::from_utf8(bytes).map_err(|error| {
        BrowserAutomationError::automation_failed(format!(
            "Browser debugging response was not valid UTF-8: {error}"
        ))
    })
}

fn http_response_complete(bytes: &[u8]) -> bool {
    let Some(header_end) = find_header_end(bytes) else {
        return false;
    };
    let headers = String::from_utf8_lossy(&bytes[..header_end]);
    let Some(content_length) = headers.lines().find_map(http_content_length) else {
        return false;
    };
    bytes.len() >= header_end + 4 + content_length
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn http_content_length(line: &str) -> Option<usize> {
    let (name, value) = line.split_once(':')?;
    if !name.eq_ignore_ascii_case("content-length") {
        return None;
    }
    value.trim().parse().ok()
}

fn cdp_call(websocket_url: &str, method: &str, params: Value) -> BrowserAutomationResult<Value> {
    let (mut socket, _) = connect(websocket_url).map_err(|error| {
        BrowserAutomationError::automation_failed(format!(
            "Failed to connect to browser target: {error}"
        ))
    })?;
    let request = json!({
        "id": 1_u64,
        "method": method,
        "params": params,
    });
    socket
        .send(Message::Text(request.to_string()))
        .map_err(|error| {
            BrowserAutomationError::automation_failed(format!(
                "Failed to send browser command '{method}': {error}"
            ))
        })?;

    loop {
        let message = socket.read().map_err(|error| {
            BrowserAutomationError::automation_failed(format!(
                "Failed to read browser command '{method}' response: {error}"
            ))
        })?;
        let Message::Text(text) = message else {
            continue;
        };
        let response: Value = serde_json::from_str(&text).map_err(|error| {
            BrowserAutomationError::automation_failed(format!(
                "Failed to parse browser command '{method}' response: {error}"
            ))
        })?;
        if response.get("id").and_then(Value::as_u64) != Some(1) {
            continue;
        }
        if let Some(error) = response.get("error") {
            return Err(BrowserAutomationError::automation_failed(format!(
                "Browser command '{method}' failed: {}",
                cdp_protocol_error_message(error)
            )));
        }
        return response.get("result").cloned().ok_or_else(|| {
            BrowserAutomationError::automation_failed(format!(
                "Browser command '{method}' response missing result."
            ))
        });
    }
}

fn install_browser_recorders(websocket_url: &str) -> BrowserAutomationResult<()> {
    install_page_recorder(websocket_url, AGENTMUX_CONSOLE_RECORDER_SOURCE)?;
    install_page_recorder(websocket_url, AGENTMUX_DIALOG_RECORDER_SOURCE)?;
    install_page_recorder(websocket_url, AGENTMUX_ERROR_RECORDER_SOURCE)?;
    Ok(())
}

fn configure_download_behavior(
    websocket_url: &str,
    downloads_dir: &Path,
) -> BrowserAutomationResult<()> {
    cdp_call(
        websocket_url,
        "Browser.setDownloadBehavior",
        json!({
            "behavior": "allow",
            "downloadPath": downloads_dir.display().to_string(),
            "eventsEnabled": true,
        }),
    )?;
    Ok(())
}

fn install_page_recorder(websocket_url: &str, source: &str) -> BrowserAutomationResult<()> {
    cdp_call(
        websocket_url,
        "Page.addScriptToEvaluateOnNewDocument",
        json!({ "source": source }),
    )?;
    cdp_call(
        websocket_url,
        "Runtime.evaluate",
        json!({
            "expression": source,
            "returnByValue": true,
            "awaitPromise": true,
        }),
    )?;
    Ok(())
}

fn cdp_protocol_error_message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| error.to_string())
}

fn runtime_result_value(result: &Value) -> BrowserAutomationResult<Value> {
    if let Some(exception) = result.get("exceptionDetails") {
        return Err(BrowserAutomationError::automation_failed(format!(
            "Browser script failed: {}",
            exception
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("runtime exception")
        )));
    }
    Ok(result
        .get("result")
        .and_then(|result| result.get("value"))
        .cloned()
        .unwrap_or(Value::Null))
}

fn collect_frame_tree(
    node: Option<&Value>,
    inherited_parent_id: Option<String>,
    frames: &mut Vec<BrowserFrameInfo>,
) {
    let Some(node) = node else {
        return;
    };
    let Some(frame) = node.get("frame") else {
        return;
    };
    let frame_id = frame
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if !frame_id.is_empty() {
        let parent_frame_id = frame
            .get("parentId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or(inherited_parent_id.clone());
        frames.push(BrowserFrameInfo {
            frame_id: frame_id.clone(),
            parent_frame_id,
            url: frame
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("about:blank")
                .to_string(),
            name: frame
                .get("name")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            security_origin: frame
                .get("securityOrigin")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        });
        if let Some(children) = node.get("childFrames").and_then(Value::as_array) {
            for child in children {
                collect_frame_tree(Some(child), Some(frame_id.clone()), frames);
            }
        }
    }
}

fn storage_entries_from_value(value: Option<&Value>) -> Vec<BrowserStorageEntry> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(BrowserStorageEntry {
                        key: item.get("key")?.as_str()?.to_string(),
                        value: item
                            .get("value")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn cookie_info_from_value(value: &Value) -> Option<BrowserCookieInfo> {
    let expires = value
        .get("expires")
        .and_then(Value::as_f64)
        .and_then(|seconds| {
            if seconds <= 0.0 || !seconds.is_finite() {
                None
            } else {
                Some(format!("{seconds:.0}"))
            }
        });
    Some(BrowserCookieInfo {
        name: value.get("name")?.as_str()?.to_string(),
        value: value
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        domain: value
            .get("domain")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        path: value
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("/")
            .to_string(),
        expires,
        http_only: value
            .get("httpOnly")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        secure: value
            .get("secure")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        same_site: value
            .get("sameSite")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn downloads_from_directory(
    directory: &Path,
    limit: usize,
) -> BrowserAutomationResult<Vec<BrowserDownloadInfo>> {
    let entries = fs::read_dir(directory).map_err(|error| {
        BrowserAutomationError::automation_failed(format!(
            "Failed to read browser download directory '{}': {error}",
            directory.display()
        ))
    })?;
    let mut downloads = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            BrowserAutomationError::automation_failed(format!(
                "Failed to read browser download entry: {error}"
            ))
        })?;
        let file_type = entry.file_type().map_err(|error| {
            BrowserAutomationError::automation_failed(format!(
                "Failed to read browser download file type: {error}"
            ))
        })?;
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        let metadata = entry.metadata().map_err(|error| {
            BrowserAutomationError::automation_failed(format!(
                "Failed to read browser download metadata: {error}"
            ))
        })?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        downloads.push(BrowserDownloadInfo {
            complete: !file_name.ends_with(".crdownload") && !file_name.ends_with(".tmp"),
            file_name,
            path: path.display().to_string(),
            byte_count: metadata.len(),
            modified_at: metadata.modified().ok().and_then(epoch_millis_string),
        });
    }
    downloads.sort_by(|left, right| {
        right
            .modified_at
            .cmp(&left.modified_at)
            .then_with(|| left.file_name.cmp(&right.file_name))
    });
    downloads.truncate(limit);
    Ok(downloads)
}

fn epoch_millis_string(value: std::time::SystemTime) -> Option<String> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().to_string())
}

fn console_messages_from_value(value: Option<&Value>) -> Vec<BrowserConsoleMessage> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(BrowserConsoleMessage {
                        level: item.get("level")?.as_str()?.to_string(),
                        text: item
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        timestamp: item
                            .get("timestamp")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn dialog_messages_from_value(value: Option<&Value>) -> Vec<BrowserDialogMessage> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(BrowserDialogMessage {
                        dialog_type: item.get("type")?.as_str()?.to_string(),
                        message: item
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        default_value: item
                            .get("defaultValue")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        response: item
                            .get("response")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        timestamp: item
                            .get("timestamp")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn error_events_from_value(value: Option<&Value>) -> Vec<BrowserErrorEvent> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(BrowserErrorEvent {
                        kind: item.get("kind")?.as_str()?.to_string(),
                        message: item
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        source: item
                            .get("source")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        line: item.get("line").and_then(Value::as_u64).unwrap_or(0) as u32,
                        column: item.get("column").and_then(Value::as_u64).unwrap_or(0) as u32,
                        stack: item
                            .get("stack")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        timestamp: item
                            .get("timestamp")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn history_entries_from_value(value: Option<&Value>) -> Vec<BrowserHistoryEntry> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(BrowserHistoryEntry {
                        id: item.get("id")?.as_i64()?,
                        url: item
                            .get("url")
                            .and_then(Value::as_str)
                            .unwrap_or("about:blank")
                            .to_string(),
                        title: item
                            .get("title")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_screenshot_format(format: &str) -> BrowserAutomationResult<String> {
    let normalized = match format.trim().to_ascii_lowercase().as_str() {
        "" => "png",
        "jpg" => "jpeg",
        "jpeg" => "jpeg",
        "png" => "png",
        "webp" => "webp",
        other => {
            return Err(BrowserAutomationError::invalid_request(format!(
                "Unsupported screenshot format '{other}'."
            )));
        }
    };
    Ok(normalized.to_string())
}

fn validate_zoom_percent(percent: u16) -> BrowserAutomationResult<()> {
    if !(25..=500).contains(&percent) {
        return Err(BrowserAutomationError::invalid_request(
            "Browser zoom percent must be between 25 and 500.",
        ));
    }
    Ok(())
}

fn normalize_browser_get_kind(
    kind: &str,
    attribute: Option<&str>,
) -> BrowserAutomationResult<String> {
    let normalized = kind.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "text" => Ok("text".to_string()),
        "html" | "outer-html" | "outer_html" => Ok("html".to_string()),
        "value" => Ok("value".to_string()),
        "attr" | "attribute" => {
            let attribute = attribute
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    BrowserAutomationError::invalid_request(
                        "Browser get attribute requires an attribute name.",
                    )
                })?;
            Ok(format!("attribute:{attribute}"))
        }
        other if other.starts_with("attribute:") => {
            let attribute = other.trim_start_matches("attribute:").trim();
            if attribute.is_empty() {
                return Err(BrowserAutomationError::invalid_request(
                    "Browser get attribute requires an attribute name.",
                ));
            }
            Ok(format!("attribute:{attribute}"))
        }
        other => Err(BrowserAutomationError::invalid_request(format!(
            "Unsupported browser get kind '{other}'."
        ))),
    }
}

fn normalize_browser_key(key: &str) -> BrowserAutomationResult<String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(BrowserAutomationError::invalid_request(
            "Press key must not be empty.",
        ));
    }
    let normalized = match trimmed.to_ascii_lowercase().as_str() {
        "enter" | "return" => "Enter",
        "tab" => "Tab",
        "escape" | "esc" => "Escape",
        "backspace" => "Backspace",
        "delete" | "del" => "Delete",
        "space" => " ",
        "arrowup" | "up" => "ArrowUp",
        "arrowdown" | "down" => "ArrowDown",
        "arrowleft" | "left" => "ArrowLeft",
        "arrowright" | "right" => "ArrowRight",
        _ => trimmed,
    };
    Ok(normalized.to_string())
}

fn browser_key_event_params(event_type: &str, key: &str) -> Value {
    let mut params = json!({
        "type": event_type,
        "key": key,
    });
    if let Value::Object(map) = &mut params {
        if let Some(code) = browser_key_code(key) {
            map.insert("code".to_string(), Value::String(code.to_string()));
        }
        if let Some(virtual_key_code) = browser_windows_virtual_key_code(key) {
            map.insert(
                "windowsVirtualKeyCode".to_string(),
                Value::Number(virtual_key_code.into()),
            );
        }
        if event_type == "keyDown" && browser_key_is_printable(key) {
            map.insert("text".to_string(), Value::String(key.to_string()));
            map.insert("unmodifiedText".to_string(), Value::String(key.to_string()));
        }
    }
    params
}

fn browser_key_is_printable(key: &str) -> bool {
    key.chars().count() == 1
}

fn browser_key_code(key: &str) -> Option<&'static str> {
    match key {
        "Enter" => Some("Enter"),
        "Tab" => Some("Tab"),
        "Escape" => Some("Escape"),
        "Backspace" => Some("Backspace"),
        "Delete" => Some("Delete"),
        "ArrowUp" => Some("ArrowUp"),
        "ArrowDown" => Some("ArrowDown"),
        "ArrowLeft" => Some("ArrowLeft"),
        "ArrowRight" => Some("ArrowRight"),
        " " => Some("Space"),
        _ => None,
    }
}

fn browser_windows_virtual_key_code(key: &str) -> Option<i64> {
    match key {
        "Backspace" => Some(8),
        "Tab" => Some(9),
        "Enter" => Some(13),
        "Escape" => Some(27),
        " " => Some(32),
        "ArrowLeft" => Some(37),
        "ArrowUp" => Some(38),
        "ArrowRight" => Some(39),
        "ArrowDown" => Some(40),
        "Delete" => Some(46),
        _ => None,
    }
}

fn json_string_literal(value: &str) -> BrowserAutomationResult<String> {
    serde_json::to_string(value).map_err(|error| {
        BrowserAutomationError::automation_failed(format!(
            "Failed to encode browser script literal: {error}"
        ))
    })
}

fn with_type(mut value: Value, event_type: &str) -> Value {
    if let Value::Object(map) = &mut value {
        map.insert("type".to_string(), Value::String(event_type.to_string()));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_commands_are_scoped_to_surface_id() {
        let command = BrowserCommand::Navigate {
            surface_id: "surf_browser".to_string(),
            url: "http://localhost:5173".to_string(),
        };

        assert_eq!(command.surface_id(), "surf_browser");

        let wait = BrowserCommand::WaitForSelector {
            surface_id: "surf_browser".to_string(),
            selector: "#ready".to_string(),
            timeout_ms: 1000,
            frame_id: None,
        };
        assert_eq!(wait.surface_id(), "surf_browser");

        let current = BrowserCommand::CurrentUrl {
            surface_id: "surf_browser".to_string(),
        };
        assert_eq!(current.surface_id(), "surf_browser");

        let zoom = BrowserCommand::SetZoom {
            surface_id: "surf_browser".to_string(),
            percent: 125,
        };
        assert_eq!(zoom.surface_id(), "surf_browser");

        let fill = BrowserCommand::FillText {
            surface_id: "surf_browser".to_string(),
            selector: "#q".to_string(),
            text: "agentmux".to_string(),
            frame_id: None,
        };
        assert_eq!(fill.surface_id(), "surf_browser");

        let get = BrowserCommand::GetElement {
            surface_id: "surf_browser".to_string(),
            selector: "#q".to_string(),
            kind: "text".to_string(),
            attribute: None,
            frame_id: None,
        };
        assert_eq!(get.surface_id(), "surf_browser");

        let frames = BrowserCommand::Frames {
            surface_id: "surf_frames".to_string(),
        };
        assert_eq!(frames.surface_id(), "surf_frames");

        let storage = BrowserCommand::StorageSnapshot {
            surface_id: "surf_storage".to_string(),
        };
        assert_eq!(storage.surface_id(), "surf_storage");

        let cookies = BrowserCommand::Cookies {
            surface_id: "surf_cookies".to_string(),
        };
        assert_eq!(cookies.surface_id(), "surf_cookies");

        let downloads = BrowserCommand::Downloads {
            surface_id: "surf_downloads".to_string(),
            limit: 25,
        };
        assert_eq!(downloads.surface_id(), "surf_downloads");

        let history = BrowserCommand::History {
            surface_id: "surf_history".to_string(),
        };
        assert_eq!(history.surface_id(), "surf_history");

        let console = BrowserCommand::ConsoleMessages {
            surface_id: "surf_console".to_string(),
            limit: 25,
        };
        assert_eq!(console.surface_id(), "surf_console");

        let dialogs = BrowserCommand::DialogMessages {
            surface_id: "surf_dialogs".to_string(),
            limit: 25,
        };
        assert_eq!(dialogs.surface_id(), "surf_dialogs");

        let errors = BrowserCommand::ErrorEvents {
            surface_id: "surf_errors".to_string(),
            limit: 25,
        };
        assert_eq!(errors.surface_id(), "surf_errors");
    }

    #[test]
    fn in_memory_browser_requires_explicit_surface_scope() {
        let mut browser = InMemoryBrowserAutomation::new();
        browser
            .create_surface(
                "surf_one".to_string(),
                "ws_browser".to_string(),
                Some("default".to_string()),
            )
            .unwrap();
        browser
            .create_surface("surf_two".to_string(), "ws_browser".to_string(), None)
            .unwrap();

        let result = browser
            .execute(BrowserCommand::Navigate {
                surface_id: "surf_two".to_string(),
                url: "http://127.0.0.1:5173".to_string(),
            })
            .unwrap();
        assert_eq!(
            result,
            BrowserCommandResult::Navigated {
                surface_id: "surf_two".to_string(),
                url: "http://127.0.0.1:5173".to_string(),
            }
        );
        assert_eq!(browser.surface("surf_one").unwrap().current_url, None);
        assert_eq!(
            browser.surface("surf_two").unwrap().current_url.as_deref(),
            Some("http://127.0.0.1:5173")
        );

        let waited = browser
            .execute(BrowserCommand::WaitForSelector {
                surface_id: "surf_two".to_string(),
                selector: "#ready".to_string(),
                timeout_ms: 250,
                frame_id: None,
            })
            .unwrap();
        assert_eq!(
            waited,
            BrowserCommandResult::WaitedForSelector {
                surface_id: "surf_two".to_string(),
                selector: "#ready".to_string(),
                elapsed_ms: 1,
            }
        );

        let current = browser
            .execute(BrowserCommand::CurrentUrl {
                surface_id: "surf_two".to_string(),
            })
            .unwrap();
        assert_eq!(
            current,
            BrowserCommandResult::Navigated {
                surface_id: "surf_two".to_string(),
                url: "http://127.0.0.1:5173".to_string(),
            }
        );

        let focused = browser
            .execute(BrowserCommand::FocusSelector {
                surface_id: "surf_two".to_string(),
                selector: "#q".to_string(),
                frame_id: None,
            })
            .unwrap();
        assert_eq!(
            focused,
            BrowserCommandResult::Focused {
                surface_id: "surf_two".to_string(),
                selector: "#q".to_string(),
            }
        );

        let zoomed = browser
            .execute(BrowserCommand::SetZoom {
                surface_id: "surf_two".to_string(),
                percent: 125,
            })
            .unwrap();
        assert_eq!(
            zoomed,
            BrowserCommandResult::Zoomed {
                surface_id: "surf_two".to_string(),
                percent: 125,
            }
        );

        let filled = browser
            .execute(BrowserCommand::FillText {
                surface_id: "surf_two".to_string(),
                selector: "#q".to_string(),
                text: "agentmux".to_string(),
                frame_id: None,
            })
            .unwrap();
        assert_eq!(
            filled,
            BrowserCommandResult::Filled {
                surface_id: "surf_two".to_string(),
                selector: "#q".to_string(),
                text: "agentmux".to_string(),
            }
        );

        let pressed = browser
            .execute(BrowserCommand::PressKey {
                surface_id: "surf_two".to_string(),
                selector: "#q".to_string(),
                key: "Enter".to_string(),
                frame_id: None,
            })
            .unwrap();
        assert_eq!(
            pressed,
            BrowserCommandResult::Pressed {
                surface_id: "surf_two".to_string(),
                selector: "#q".to_string(),
                key: "Enter".to_string(),
            }
        );

        let selected = browser
            .execute(BrowserCommand::SelectValues {
                surface_id: "surf_two".to_string(),
                selector: "#choice".to_string(),
                values: vec!["one".to_string()],
                frame_id: None,
            })
            .unwrap();
        assert_eq!(
            selected,
            BrowserCommandResult::Selected {
                surface_id: "surf_two".to_string(),
                selector: "#choice".to_string(),
                values: vec!["one".to_string()],
            }
        );

        let scrolled = browser
            .execute(BrowserCommand::ScrollBy {
                surface_id: "surf_two".to_string(),
                selector: None,
                x: 0,
                y: 400,
                frame_id: None,
            })
            .unwrap();
        assert_eq!(
            scrolled,
            BrowserCommandResult::Scrolled {
                surface_id: "surf_two".to_string(),
                target: "window".to_string(),
                x: 0,
                y: 400,
            }
        );

        let hovered = browser
            .execute(BrowserCommand::HoverSelector {
                surface_id: "surf_two".to_string(),
                selector: "#submit".to_string(),
                frame_id: None,
            })
            .unwrap();
        assert_eq!(
            hovered,
            BrowserCommandResult::Hovered {
                surface_id: "surf_two".to_string(),
                selector: "#submit".to_string(),
            }
        );

        let checked = browser
            .execute(BrowserCommand::CheckSelector {
                surface_id: "surf_two".to_string(),
                selector: "#agree".to_string(),
                checked: true,
                frame_id: None,
            })
            .unwrap();
        assert_eq!(
            checked,
            BrowserCommandResult::Checked {
                surface_id: "surf_two".to_string(),
                selector: "#agree".to_string(),
                checked: true,
            }
        );

        let got = browser
            .execute(BrowserCommand::GetElement {
                surface_id: "surf_two".to_string(),
                selector: "#q".to_string(),
                kind: "text".to_string(),
                attribute: None,
                frame_id: None,
            })
            .unwrap();
        assert_eq!(
            got,
            BrowserCommandResult::Got {
                surface_id: "surf_two".to_string(),
                selector: "#q".to_string(),
                kind: "text".to_string(),
                value: "text:#q".to_string(),
            }
        );

        let found = browser
            .execute(BrowserCommand::FindText {
                surface_id: "surf_two".to_string(),
                query: "agentmux".to_string(),
                selector: Some("main".to_string()),
                limit: 5,
                frame_id: None,
            })
            .unwrap();
        assert_eq!(
            found,
            BrowserCommandResult::Found {
                surface_id: "surf_two".to_string(),
                query: "agentmux".to_string(),
                count: 1,
                matches: vec!["main:agentmux".to_string()],
            }
        );

        let highlighted = browser
            .execute(BrowserCommand::HighlightSelector {
                surface_id: "surf_two".to_string(),
                selector: "#q".to_string(),
                duration_ms: 250,
                frame_id: None,
            })
            .unwrap();
        assert_eq!(
            highlighted,
            BrowserCommandResult::Highlighted {
                surface_id: "surf_two".to_string(),
                selector: "#q".to_string(),
                duration_ms: 250,
            }
        );

        let frames = browser
            .execute(BrowserCommand::Frames {
                surface_id: "surf_two".to_string(),
            })
            .unwrap();
        let BrowserCommandResult::Frames { surface_id, frames } = frames else {
            panic!("frames command should return frames result");
        };
        assert_eq!(surface_id, "surf_two");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].frame_id, "frame_surf_two");
        assert_eq!(frames[0].url, "http://127.0.0.1:5173");

        let storage = browser
            .execute(BrowserCommand::StorageSnapshot {
                surface_id: "surf_two".to_string(),
            })
            .unwrap();
        let BrowserCommandResult::StorageSnapshot {
            surface_id,
            local_storage,
            session_storage,
        } = storage
        else {
            panic!("storage command should return storage snapshot result");
        };
        assert_eq!(surface_id, "surf_two");
        assert!(local_storage.is_empty());
        assert!(session_storage.is_empty());

        let cookies = browser
            .execute(BrowserCommand::Cookies {
                surface_id: "surf_two".to_string(),
            })
            .unwrap();
        let BrowserCommandResult::Cookies {
            surface_id,
            cookies,
        } = cookies
        else {
            panic!("cookies command should return cookies result");
        };
        assert_eq!(surface_id, "surf_two");
        assert!(cookies.is_empty());

        let downloads = browser
            .execute(BrowserCommand::Downloads {
                surface_id: "surf_two".to_string(),
                limit: 10,
            })
            .unwrap();
        let BrowserCommandResult::Downloads {
            surface_id,
            directory,
            downloads,
        } = downloads
        else {
            panic!("downloads command should return downloads result");
        };
        assert_eq!(surface_id, "surf_two");
        assert_eq!(directory, "memory://browser/surf_two/downloads");
        assert!(downloads.is_empty());

        let history = browser
            .execute(BrowserCommand::History {
                surface_id: "surf_two".to_string(),
            })
            .unwrap();
        let BrowserCommandResult::History {
            surface_id,
            current_index,
            entries,
        } = history
        else {
            panic!("history command should return history result");
        };
        assert_eq!(surface_id, "surf_two");
        assert_eq!(current_index, 0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].url, "http://127.0.0.1:5173");

        let console = browser
            .execute(BrowserCommand::ConsoleMessages {
                surface_id: "surf_two".to_string(),
                limit: 10,
            })
            .unwrap();
        let BrowserCommandResult::ConsoleMessages {
            surface_id,
            messages,
        } = console
        else {
            panic!("console command should return console messages result");
        };
        assert_eq!(surface_id, "surf_two");
        assert!(messages.is_empty());

        let dialogs = browser
            .execute(BrowserCommand::DialogMessages {
                surface_id: "surf_two".to_string(),
                limit: 10,
            })
            .unwrap();
        let BrowserCommandResult::DialogMessages {
            surface_id,
            messages,
        } = dialogs
        else {
            panic!("dialogs command should return dialog messages result");
        };
        assert_eq!(surface_id, "surf_two");
        assert!(messages.is_empty());

        let errors = browser
            .execute(BrowserCommand::ErrorEvents {
                surface_id: "surf_two".to_string(),
                limit: 10,
            })
            .unwrap();
        let BrowserCommandResult::ErrorEvents { surface_id, events } = errors else {
            panic!("errors command should return error events result");
        };
        assert_eq!(surface_id, "surf_two");
        assert!(events.is_empty());
    }

    #[test]
    fn in_memory_browser_rejects_unknown_surface_commands() {
        let mut browser = InMemoryBrowserAutomation::new();
        let error = browser
            .execute(BrowserCommand::DomSnapshot {
                surface_id: "surf_missing".to_string(),
                frame_id: None,
            })
            .unwrap_err();

        assert_eq!(error.code, BrowserAutomationErrorCode::SurfaceNotFound);
    }

    #[test]
    fn in_memory_browser_returns_scoped_artifacts() {
        let mut browser = InMemoryBrowserAutomation::new();
        browser
            .create_surface("surf_artifact".to_string(), "ws_browser".to_string(), None)
            .unwrap();
        browser
            .execute(BrowserCommand::Navigate {
                surface_id: "surf_artifact".to_string(),
                url: "https://example.invalid".to_string(),
            })
            .unwrap();

        let snapshot = browser
            .execute(BrowserCommand::DomSnapshot {
                surface_id: "surf_artifact".to_string(),
                frame_id: None,
            })
            .unwrap();
        assert_eq!(
            snapshot,
            BrowserCommandResult::DomSnapshot {
                surface_id: "surf_artifact".to_string(),
                html: r#"<html data-agentmux-surface="surf_artifact"><body>https://example.invalid</body></html>"#
                    .to_string(),
            }
        );

        let screenshot = browser
            .execute(BrowserCommand::Screenshot {
                surface_id: "surf_artifact".to_string(),
                format: "png".to_string(),
            })
            .unwrap();
        assert_eq!(
            screenshot,
            BrowserCommandResult::Screenshot {
                surface_id: "surf_artifact".to_string(),
                format: "png".to_string(),
                bytes: b"agentmux-browser:surf_artifact:png".to_vec(),
            }
        );
    }

    #[test]
    fn cdp_browser_config_does_not_launch_until_surface_creation() {
        let browser = CdpBrowserAutomation::with_executable("C:/missing/browser.exe");

        assert_eq!(browser.executable(), Path::new("C:/missing/browser.exe"));
        assert_eq!(
            browser.surface("surf_missing").unwrap_err().code,
            BrowserAutomationErrorCode::SurfaceNotFound
        );
    }

    #[test]
    fn cdp_helpers_normalize_screenshot_formats() {
        assert_eq!(normalize_screenshot_format("").unwrap(), "png");
        assert_eq!(normalize_screenshot_format("PNG").unwrap(), "png");
        assert_eq!(normalize_screenshot_format("jpg").unwrap(), "jpeg");
        assert_eq!(normalize_screenshot_format("webp").unwrap(), "webp");
        assert_eq!(
            normalize_screenshot_format("bmp").unwrap_err().code,
            BrowserAutomationErrorCode::InvalidRequest
        );
    }

    #[test]
    fn runtime_result_value_reports_script_exceptions() {
        let result = json!({
            "exceptionDetails": {
                "text": "Uncaught"
            }
        });

        let error = runtime_result_value(&result).unwrap_err();
        assert_eq!(error.code, BrowserAutomationErrorCode::AutomationFailed);
        assert!(error.message.contains("Uncaught"));
    }

    #[test]
    #[ignore = "launches an installed Edge/Chrome/Chromium process for CDP smoke coverage"]
    fn cdp_browser_launches_real_browser_smoke() {
        let Some(executable) = discover_browser_executable() else {
            eprintln!("skipping CDP smoke because no supported browser executable was found");
            return;
        };
        let fixture_url = start_browser_fixture_server();
        let mut browser = CdpBrowserAutomation::with_executable_and_headless(executable, true);
        let surface = browser
            .create_surface(
                "surf_cdp_smoke".to_string(),
                "ws_cdp_smoke".to_string(),
                Some("smoke".to_string()),
            )
            .unwrap();
        assert!(surface.browser_id.starts_with("cdp_browser_"));

        browser
            .execute(BrowserCommand::Navigate {
                surface_id: surface.surface_id.clone(),
                url: fixture_url,
            })
            .unwrap();
        browser
            .execute(BrowserCommand::TypeText {
                surface_id: surface.surface_id.clone(),
                selector: "#q".to_string(),
                text: "agentmux".to_string(),
                frame_id: None,
            })
            .unwrap();
        browser
            .execute(BrowserCommand::ClickSelector {
                surface_id: surface.surface_id.clone(),
                selector: "#b".to_string(),
                frame_id: None,
            })
            .unwrap();
        let evaluated = browser
            .execute(BrowserCommand::Evaluate {
                surface_id: surface.surface_id.clone(),
                script: r##"({ value: document.querySelector("#q").value, clicked: document.body.dataset.clicked })"##.to_string(),
                frame_id: None,
            })
            .unwrap();
        let BrowserCommandResult::Evaluated {
            surface_id,
            value_json,
        } = evaluated
        else {
            panic!("expected evaluation");
        };
        assert_eq!(surface_id, surface.surface_id);
        let value: Value = serde_json::from_str(&value_json).unwrap();
        assert_eq!(value["value"], "agentmux");
        assert_eq!(value["clicked"], "yes");

        let snapshot = browser
            .execute(BrowserCommand::DomSnapshot {
                surface_id: surface.surface_id.clone(),
                frame_id: None,
            })
            .unwrap();
        let BrowserCommandResult::DomSnapshot { html, .. } = snapshot else {
            panic!("expected DOM snapshot");
        };
        assert!(html.contains("button"));

        let screenshot = browser
            .execute(BrowserCommand::Screenshot {
                surface_id: surface.surface_id,
                format: "png".to_string(),
            })
            .unwrap();
        let BrowserCommandResult::Screenshot { bytes, .. } = screenshot else {
            panic!("expected screenshot");
        };
        assert!(!bytes.is_empty());
    }

    fn start_browser_fixture_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            for _ in 0..4 {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut request_buffer = [0; 1024];
                let _ = stream.read(&mut request_buffer);
                let body = r#"<!doctype html>
<html>
  <head><title>AgentMux CDP fixture</title></head>
  <body>
    <input id="q" />
    <button id="b" onclick="document.body.dataset.clicked='yes'">Go</button>
  </body>
</html>"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        url
    }
}
