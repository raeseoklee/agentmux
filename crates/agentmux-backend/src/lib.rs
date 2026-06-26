use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendKind {
    Conpty,
    WslDirect,
    WslTmuxControl,
    Ssh,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalSize {
    pub columns: u16,
    pub rows: u16,
}

impl TerminalSize {
    pub fn new(columns: u16, rows: u16) -> Self {
        Self { columns, rows }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub executable: String,
    pub args: Vec<String>,
}

impl CommandSpec {
    pub fn new(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
        }
    }

    pub fn with_args(executable: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            executable: executable.into(),
            args,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputEvent {
    Text(String),
    Paste { text: String, bracketed: bool },
    Key(NamedKey),
    Control(ControlCode),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NamedKey {
    Enter,
    Backspace,
    Tab,
    Escape,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Function(u8),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControlCode {
    Interrupt,
    EndOfTransmission,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpawnRequest {
    pub session_id: String,
    pub workspace_id: Option<String>,
    pub backend: Option<BackendKind>,
    pub backend_profile: Option<String>,
    pub command: CommandSpec,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
    pub initial_size: TerminalSize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachRequest {
    pub session_id: String,
    pub backend: BackendKind,
    pub backend_profile: Option<String>,
    pub backend_ref: String,
    pub initial_size: TerminalSize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionHandle {
    pub session_id: String,
    pub backend_kind: BackendKind,
    pub backend_native_id: Option<String>,
    pub transport_pid: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminationMode {
    Soft,
    Interrupt,
    Kill,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendHealth {
    Starting,
    Healthy,
    Degraded,
    Disconnected,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendEvent {
    Started {
        session_id: String,
    },
    Output {
        session_id: String,
        bytes: Vec<u8>,
    },
    Resized {
        session_id: String,
        columns: u16,
        rows: u16,
    },
    Exited {
        session_id: String,
        code: Option<i32>,
    },
    HealthChanged {
        attachment_id: String,
        state: BackendHealth,
    },
    Error {
        session_id: Option<String>,
        error: BackendError,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendError {
    pub code: String,
    pub message: String,
}

impl BackendError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new("backend_unavailable", message)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new("unsupported_backend_operation", message)
    }

    pub fn session_not_found(session_id: impl AsRef<str>) -> Self {
        Self::new(
            "session_not_found",
            format!("Session '{}' does not exist.", session_id.as_ref()),
        )
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new("invalid_request", message)
    }

    pub fn spawn_failed(message: impl Into<String>) -> Self {
        Self::new("spawn_failed", message)
    }

    pub fn input_failed(message: impl Into<String>) -> Self {
        Self::new("input_failed", message)
    }

    pub fn resize_failed(message: impl Into<String>) -> Self {
        Self::new("resize_failed", message)
    }

    pub fn terminate_failed(message: impl Into<String>) -> Self {
        Self::new("terminate_failed", message)
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for BackendError {}

pub type BackendResult<T> = Result<T, BackendError>;

pub trait SessionBackend {
    fn kind(&self) -> BackendKind;

    fn spawn(&mut self, request: SpawnRequest) -> BackendResult<SessionHandle>;

    fn attach(&mut self, request: AttachRequest) -> BackendResult<SessionHandle>;

    fn send_input(&mut self, session_id: &str, input: InputEvent) -> BackendResult<()>;

    fn resize(&mut self, session_id: &str, size: TerminalSize) -> BackendResult<()>;

    fn terminate(&mut self, session_id: &str, mode: TerminationMode) -> BackendResult<()>;

    fn set_output_paused(&mut self, _session_id: &str, _paused: bool) -> BackendResult<()> {
        Ok(())
    }

    fn drain_events(&mut self) -> Vec<BackendEvent>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_size_keeps_columns_and_rows() {
        let size = TerminalSize::new(120, 40);
        assert_eq!(size.columns, 120);
        assert_eq!(size.rows, 40);
    }

    #[test]
    fn backend_error_has_stable_code() {
        let error = BackendError::unavailable("WSL is unavailable");
        assert_eq!(error.code, "backend_unavailable");
        assert!(error.to_string().contains("WSL"));
    }
}
