use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::io;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde_json::{json, Value};
use tempfile::TempDir;
use tungstenite::{connect, Message};

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
    Screenshot {
        surface_id: String,
        format: String,
    },
    DomSnapshot {
        surface_id: String,
    },
    ClickSelector {
        surface_id: String,
        selector: String,
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
    },
    Evaluate {
        surface_id: String,
        script: String,
    },
}

impl BrowserCommand {
    pub fn surface_id(&self) -> &str {
        match self {
            BrowserCommand::Navigate { surface_id, .. }
            | BrowserCommand::Screenshot { surface_id, .. }
            | BrowserCommand::DomSnapshot { surface_id }
            | BrowserCommand::ClickSelector { surface_id, .. }
            | BrowserCommand::ClickPoint { surface_id, .. }
            | BrowserCommand::TypeText { surface_id, .. }
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
    Clicked {
        surface_id: String,
        target: String,
    },
    Typed {
        surface_id: String,
        selector: String,
        text: String,
    },
    Evaluated {
        surface_id: String,
        value_json: String,
    },
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
            BrowserCommand::DomSnapshot { surface_id } => {
                let surface = self.require_surface(&surface_id)?;
                let url = surface.current_url.as_deref().unwrap_or("about:blank");
                Ok(BrowserCommandResult::DomSnapshot {
                    html: format!(
                        r#"<html data-agentmux-surface="{surface_id}"><body>{url}</body></html>"#
                    ),
                    surface_id,
                })
            }
            BrowserCommand::ClickSelector {
                surface_id,
                selector,
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
            BrowserCommand::Evaluate { surface_id, script } => {
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
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let result = cdp_call(
            &websocket_url,
            "Runtime.evaluate",
            json!({
                "expression": "document.documentElement ? document.documentElement.outerHTML : ''",
                "returnByValue": true,
                "awaitPromise": true,
            }),
        )?;
        let html = runtime_result_value(&result)?
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(String::new);
        Ok(BrowserCommandResult::DomSnapshot { surface_id, html })
    }

    fn execute_click_selector(
        &self,
        surface_id: String,
        selector: String,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if selector.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Click selector must not be empty.",
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
  element.click();
  return true;
}})()"#,
            json_string_literal(&selector)?
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
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if selector.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Type selector must not be empty.",
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
  element.focus();
  return true;
}})()"#,
            json_string_literal(&selector)?
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
        runtime_result_value(&result)?;
        cdp_call(&websocket_url, "Input.insertText", json!({ "text": text }))?;
        Ok(BrowserCommandResult::Typed {
            surface_id,
            selector,
            text,
        })
    }

    fn execute_evaluate(
        &self,
        surface_id: String,
        script: String,
    ) -> BrowserAutomationResult<BrowserCommandResult> {
        if script.trim().is_empty() {
            return Err(BrowserAutomationError::invalid_request(
                "Evaluate script must not be empty.",
            ));
        }
        let websocket_url = self.require_surface(&surface_id)?.websocket_url.clone();
        let result = cdp_call(
            &websocket_url,
            "Runtime.evaluate",
            json!({
                "expression": script,
                "returnByValue": true,
                "awaitPromise": true,
            }),
        )?;
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
            BrowserCommand::Screenshot { surface_id, format } => {
                self.execute_screenshot(surface_id, format)
            }
            BrowserCommand::DomSnapshot { surface_id } => self.execute_dom_snapshot(surface_id),
            BrowserCommand::ClickSelector {
                surface_id,
                selector,
            } => self.execute_click_selector(surface_id, selector),
            BrowserCommand::ClickPoint { surface_id, x, y } => {
                self.execute_click_point(surface_id, x, y)
            }
            BrowserCommand::TypeText {
                surface_id,
                selector,
                text,
            } => self.execute_type_text(surface_id, selector, text),
            BrowserCommand::Evaluate { surface_id, script } => {
                self.execute_evaluate(surface_id, script)
            }
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
    }

    #[test]
    fn in_memory_browser_rejects_unknown_surface_commands() {
        let mut browser = InMemoryBrowserAutomation::new();
        let error = browser
            .execute(BrowserCommand::DomSnapshot {
                surface_id: "surf_missing".to_string(),
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
            })
            .unwrap();
        browser
            .execute(BrowserCommand::ClickSelector {
                surface_id: surface.surface_id.clone(),
                selector: "#b".to_string(),
            })
            .unwrap();
        let evaluated = browser
            .execute(BrowserCommand::Evaluate {
                surface_id: surface.surface_id.clone(),
                script: r##"({ value: document.querySelector("#q").value, clicked: document.body.dataset.clicked })"##.to_string(),
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
