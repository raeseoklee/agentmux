use std::{collections::HashMap, time::Duration};

use agentmux_backend::{
    AttachRequest, BackendError, BackendEvent, BackendHealth, BackendKind, BackendResult,
    CommandSpec, InputEvent, NamedKey, SessionBackend, SessionHandle, SpawnRequest, TerminalSize,
    TerminationMode,
};
use agentmux_backend_wsl::{PipeBackend, WslDirectBackend, WslDirectConfig};

pub const TMUX_EXE: &str = "tmux";
pub const TMUX_CONTROL_ARGS: &[&str] = &["-C", "new-session", "-A", "-s"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TmuxControlMessage {
    Begin {
        command_id: Option<String>,
        raw: String,
    },
    End {
        command_id: Option<String>,
        raw: String,
    },
    Error {
        command_id: Option<String>,
        raw: String,
    },
    Exit,
    Output {
        pane_id: String,
        payload: Vec<u8>,
    },
    WindowAdd {
        window_id: String,
    },
    WindowClose {
        window_id: String,
    },
    PaneAdd {
        pane_id: String,
    },
    PaneClose {
        pane_id: String,
    },
    PaneDied {
        pane_id: String,
    },
    LayoutChange {
        window_id: String,
        layout: String,
    },
    SessionChanged {
        session_id: String,
        name: String,
    },
    SessionsChanged,
    WindowRenamed {
        window_id: String,
        name: String,
    },
    CommandResponse {
        command_id: Option<String>,
        line: String,
    },
    Malformed {
        line: String,
        reason: String,
    },
    Unknown(String),
}

pub fn parse_control_line(line: &str) -> TmuxControlMessage {
    parse_control_line_bytes(line.as_bytes(), None)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmuxPaneMapping {
    pub agent_session_id: String,
    pub tmux_session_id: Option<String>,
    pub window_id: Option<String>,
    pub pane_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmuxSessionTarget {
    pub session_name: String,
    pub distribution: Option<String>,
}

pub fn durable_session_name(workspace_id: &str) -> String {
    let short = workspace_id
        .trim()
        .trim_start_matches("ws_")
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .take(24)
        .collect::<String>();
    if short.is_empty() {
        "agentmux_workspace".to_string()
    } else {
        format!("agentmux_{short}")
    }
}

pub fn tmux_control_launch_command(session_name: &str) -> CommandSpec {
    let mut args = TMUX_CONTROL_ARGS
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    args.push(session_name.to_string());
    CommandSpec::with_args(TMUX_EXE, args)
}

pub fn tmux_control_spawn_command(session_name: &str, command: CommandSpec) -> CommandSpec {
    CommandSpec::with_args(
        "sh",
        vec![
            "-lc".to_string(),
            tmux_detached_spawn_then_attach_script(session_name, command),
        ],
    )
}

pub fn tmux_control_attach_command(session_name: &str) -> CommandSpec {
    CommandSpec::with_args(
        TMUX_EXE,
        vec![
            "-C".to_string(),
            "attach-session".to_string(),
            "-t".to_string(),
            session_name.to_string(),
        ],
    )
}

pub fn tmux_list_panes_command(session_name: &str) -> CommandSpec {
    CommandSpec::with_args(
        TMUX_EXE,
        vec![
            "list-panes".to_string(),
            "-t".to_string(),
            session_name.to_string(),
            "-a".to_string(),
            "-F".to_string(),
            "#{session_id}\t#{window_id}\t#{pane_id}\t#{pane_current_command}\t#{pane_current_path}"
                .to_string(),
        ],
    )
}

pub fn tmux_shell_command(command: CommandSpec) -> String {
    std::iter::once(command.executable)
        .chain(command.args)
        .map(posix_shell_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn posix_shell_quote(value: String) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/'))
    {
        value
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

pub fn tmux_control_quote(value: &str) -> String {
    if !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':' | b'%' | b'@' | b'$')
        })
    {
        value.to_string()
    } else {
        format!(
            "\"{}\"",
            value
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\r', "\\r")
                .replace('\n', "\\n")
        )
    }
}

pub fn tmux_control_lines_for_input(target: &str, input: InputEvent) -> Vec<String> {
    match input {
        InputEvent::Text(text) => literal_text_lines(target, &text),
        InputEvent::Paste { text, .. } => literal_text_lines(target, &text),
        InputEvent::Key(key) => key_line(target, key).into_iter().collect(),
        InputEvent::Control(control) => {
            let key = match control {
                agentmux_backend::ControlCode::Interrupt => "C-c",
                agentmux_backend::ControlCode::EndOfTransmission => "C-d",
            };
            vec![format!(
                "send-keys -t {} {}\n",
                tmux_control_quote(target),
                key
            )]
        }
    }
}

fn literal_text_lines(target: &str, text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut chunk = String::new();

    for ch in text.chars() {
        match ch {
            '\r' => {
                if !chunk.is_empty() {
                    lines.push(send_literal_line(target, &chunk));
                    chunk.clear();
                }
                lines.push(send_key_line(target, "Enter"));
            }
            '\n' => {
                if !chunk.is_empty() {
                    lines.push(send_literal_line(target, &chunk));
                    chunk.clear();
                }
                lines.push(send_key_line(target, "Enter"));
            }
            other => chunk.push(other),
        }
    }

    if !chunk.is_empty() {
        lines.push(send_literal_line(target, &chunk));
    }

    lines
}

fn key_line(target: &str, key: NamedKey) -> Option<String> {
    let key = match key {
        NamedKey::Enter => "Enter".to_string(),
        NamedKey::Backspace => "BSpace".to_string(),
        NamedKey::Tab => "Tab".to_string(),
        NamedKey::Escape => "Escape".to_string(),
        NamedKey::ArrowUp => "Up".to_string(),
        NamedKey::ArrowDown => "Down".to_string(),
        NamedKey::ArrowLeft => "Left".to_string(),
        NamedKey::ArrowRight => "Right".to_string(),
        NamedKey::Function(n) => format!("F{n}"),
    };
    Some(send_key_line(target, &key))
}

fn send_literal_line(target: &str, text: &str) -> String {
    format!(
        "send-keys -t {} -l {}\n",
        tmux_control_quote(target),
        tmux_control_quote(text)
    )
}

fn send_key_line(target: &str, key: &str) -> String {
    format!("send-keys -t {} {}\n", tmux_control_quote(target), key)
}

pub fn tmux_resize_control_line(target: &str, size: TerminalSize) -> String {
    format!(
        "refresh-client -C {}x{}\nresize-window -t {} -x {} -y {}\nresize-pane -t {} -x {} -y {}\n",
        size.columns,
        size.rows,
        tmux_control_quote(target),
        size.columns,
        size.rows,
        tmux_control_quote(target),
        size.columns,
        size.rows
    )
}

pub fn tmux_detach_control_line() -> String {
    "detach-client\n".to_string()
}

pub fn tmux_kill_session_control_line(target: &str) -> String {
    format!("kill-session -t {}\n", tmux_control_quote(target))
}

pub fn tmux_active_pane_control_line(target: &str) -> String {
    format!(
        "display-message -p -t {} \"#{{pane_id}}\"\n",
        tmux_control_quote(target)
    )
}

pub fn tmux_detached_spawn_then_attach_script(session_name: &str, command: CommandSpec) -> String {
    let target = posix_shell_quote(session_name.to_string());
    let command = posix_shell_quote(tmux_shell_command(command));
    format!(
        "if ! tmux has-session -t {target} 2>/dev/null; then tmux new-session -d -s {target} {command} || exit $?; fi; exec tmux -C attach-session -t {target}"
    )
}

pub fn tmux_capture_pane_control_line(target: &str) -> String {
    format!("capture-pane -p -e -t {}\n", tmux_control_quote(target))
}

pub fn tmux_send_literal_command(pane_id: &str, text: &str) -> CommandSpec {
    CommandSpec::with_args(
        TMUX_EXE,
        vec![
            "send-keys".to_string(),
            "-t".to_string(),
            pane_id.to_string(),
            "-l".to_string(),
            text.to_string(),
        ],
    )
}

pub fn tmux_resize_pane_command(pane_id: &str, size: TerminalSize) -> CommandSpec {
    CommandSpec::with_args(
        TMUX_EXE,
        vec![
            "resize-pane".to_string(),
            "-t".to_string(),
            pane_id.to_string(),
            "-x".to_string(),
            size.columns.to_string(),
            "-y".to_string(),
            size.rows.to_string(),
        ],
    )
}

#[derive(Debug)]
pub struct TmuxControlParser {
    buffer: Vec<u8>,
    active_command_id: Option<String>,
    max_line_bytes: usize,
}

impl Default for TmuxControlParser {
    fn default() -> Self {
        Self {
            buffer: Vec::new(),
            active_command_id: None,
            max_line_bytes: 1024 * 1024,
        }
    }
}

impl TmuxControlParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<TmuxControlMessage> {
        self.buffer.extend_from_slice(bytes);
        let mut messages = Vec::new();

        while let Some(position) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line = self.buffer.drain(..=position).collect::<Vec<_>>();
            if line.ends_with(b"\n") {
                line.pop();
            }
            if line.ends_with(b"\r") {
                line.pop();
            }
            messages.push(self.parse_line(&line));
        }

        if self.buffer.len() > self.max_line_bytes {
            let line = String::from_utf8_lossy(&self.buffer).into_owned();
            self.buffer.clear();
            messages.push(TmuxControlMessage::Malformed {
                line,
                reason: "tmux-control line exceeded parser buffer limit".to_string(),
            });
        }

        messages
    }

    pub fn flush(&mut self) -> Option<TmuxControlMessage> {
        if self.buffer.is_empty() {
            None
        } else {
            let line = std::mem::take(&mut self.buffer);
            Some(self.parse_line(&line))
        }
    }

    fn parse_line(&mut self, line: &[u8]) -> TmuxControlMessage {
        let line = strip_terminal_prefix_before_control_line(line);
        let message = parse_control_line_bytes(line, self.active_command_id.as_deref());
        match &message {
            TmuxControlMessage::Begin { command_id, .. } => {
                self.active_command_id = command_id.clone();
            }
            TmuxControlMessage::End { .. } | TmuxControlMessage::Error { .. } => {
                self.active_command_id = None;
            }
            _ => {}
        }
        message
    }
}

fn strip_terminal_prefix_before_control_line(line: &[u8]) -> &[u8] {
    if line.first() == Some(&b'%') {
        return line;
    }

    line.iter()
        .position(|byte| *byte == b'%')
        .map(|position| &line[position..])
        .unwrap_or(line)
}

fn parse_control_line_bytes(line: &[u8], active_command_id: Option<&str>) -> TmuxControlMessage {
    if let Some(rest) = strip_prefix(line, b"%begin ") {
        let raw = text(rest);
        TmuxControlMessage::Begin {
            command_id: command_id_from_control_fields(&raw),
            raw,
        }
    } else if let Some(rest) = strip_prefix(line, b"%end ") {
        let raw = text(rest);
        TmuxControlMessage::End {
            command_id: command_id_from_control_fields(&raw),
            raw,
        }
    } else if let Some(rest) = strip_prefix(line, b"%error ") {
        let raw = text(rest);
        TmuxControlMessage::Error {
            command_id: command_id_from_control_fields(&raw),
            raw,
        }
    } else if line == b"%exit" {
        TmuxControlMessage::Exit
    } else if let Some(rest) = strip_prefix(line, b"%output ") {
        parse_output(rest)
    } else if let Some(rest) = strip_prefix(line, b"%window-add ") {
        TmuxControlMessage::WindowAdd {
            window_id: text(rest),
        }
    } else if let Some(rest) = strip_prefix(line, b"%window-close ") {
        TmuxControlMessage::WindowClose {
            window_id: text(rest),
        }
    } else if let Some(rest) = strip_prefix(line, b"%pane-add ") {
        TmuxControlMessage::PaneAdd {
            pane_id: text(rest),
        }
    } else if let Some(rest) = strip_prefix(line, b"%pane-close ") {
        TmuxControlMessage::PaneClose {
            pane_id: text(rest),
        }
    } else if let Some(rest) =
        strip_prefix(line, b"%pane-died ").or_else(|| strip_prefix(line, b"%pane-exited "))
    {
        TmuxControlMessage::PaneDied {
            pane_id: text(rest),
        }
    } else if let Some(rest) = strip_prefix(line, b"%layout-change ") {
        let mut parts = split_once_byte(rest, b' ');
        let window_id = text(parts.next().unwrap_or_default());
        let layout = text(parts.next().unwrap_or_default());
        TmuxControlMessage::LayoutChange { window_id, layout }
    } else if let Some(rest) = strip_prefix(line, b"%session-changed ") {
        let mut parts = split_once_byte(rest, b' ');
        let session_id = text(parts.next().unwrap_or_default());
        let name = text(parts.next().unwrap_or_default());
        TmuxControlMessage::SessionChanged { session_id, name }
    } else if line == b"%sessions-changed" {
        TmuxControlMessage::SessionsChanged
    } else if let Some(rest) = strip_prefix(line, b"%window-renamed ") {
        let mut parts = split_once_byte(rest, b' ');
        let window_id = text(parts.next().unwrap_or_default());
        let name = text(parts.next().unwrap_or_default());
        TmuxControlMessage::WindowRenamed { window_id, name }
    } else if active_command_id.is_some() {
        TmuxControlMessage::CommandResponse {
            command_id: active_command_id.map(ToString::to_string),
            line: text(line),
        }
    } else {
        TmuxControlMessage::Unknown(text(line))
    }
}

fn parse_output(rest: &[u8]) -> TmuxControlMessage {
    let mut parts = split_once_byte(rest, b' ');
    let pane_id = text(parts.next().unwrap_or_default());
    if pane_id.is_empty() {
        return TmuxControlMessage::Malformed {
            line: text(rest),
            reason: "tmux output line did not include a pane id".to_string(),
        };
    }
    let payload = decode_tmux_payload(parts.next().unwrap_or_default());
    TmuxControlMessage::Output { pane_id, payload }
}

pub fn decode_tmux_payload(payload: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(payload.len());
    let mut index = 0;

    while index < payload.len() {
        if payload[index] != b'\\' {
            output.push(payload[index]);
            index += 1;
            continue;
        }

        let Some(next) = payload.get(index + 1).copied() else {
            output.push(b'\\');
            break;
        };

        if index + 3 < payload.len()
            && payload[index + 1].is_ascii_digit()
            && payload[index + 2].is_ascii_digit()
            && payload[index + 3].is_ascii_digit()
        {
            let octal = &payload[index + 1..index + 4];
            if let Ok(text) = std::str::from_utf8(octal) {
                if let Ok(value) = u8::from_str_radix(text, 8) {
                    output.push(value);
                    index += 4;
                    continue;
                }
            }
        }

        match next {
            b'\\' => output.push(b'\\'),
            b'n' => output.push(b'\n'),
            b'r' => output.push(b'\r'),
            b't' => output.push(b'\t'),
            other => output.push(other),
        }
        index += 2;
    }

    output
}

fn command_id_from_control_fields(raw: &str) -> Option<String> {
    raw.split_whitespace().nth(1).map(ToString::to_string)
}

fn strip_prefix<'a>(line: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    line.strip_prefix(prefix)
}

fn split_once_byte(line: &[u8], separator: u8) -> impl Iterator<Item = &[u8]> {
    let position = line.iter().position(|byte| *byte == separator);
    let first = position.map_or(line, |position| &line[..position]);
    let second = position.map(|position| &line[position + 1..]);
    [Some(first), second].into_iter().flatten()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

pub fn parse_fixture(fixture: &str) -> Vec<TmuxControlMessage> {
    let mut parser = TmuxControlParser::new();
    let mut messages = parser.push(fixture.as_bytes());
    if let Some(message) = parser.flush() {
        messages.push(message);
    }
    messages
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TmuxControlConfig {
    pub distribution: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TmuxControlSession {
    session_name: String,
    pane_id: Option<String>,
    started: bool,
    replay_state: TmuxReplayState,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum TmuxReplayState {
    #[default]
    None,
    AwaitingActivePane,
    AwaitingCaptureBegin,
    Capturing,
}

pub struct TmuxControlBackend<B = WslDirectBackend> {
    transport: B,
    events: Vec<BackendEvent>,
    sessions: HashMap<String, TmuxControlSession>,
    parsers: HashMap<String, TmuxControlParser>,
}

impl TmuxControlBackend<WslDirectBackend<PipeBackend>> {
    pub fn new() -> Self {
        Self::with_config(TmuxControlConfig::default())
    }

    pub fn with_config(config: TmuxControlConfig) -> Self {
        let wsl_config = match config.distribution {
            Some(distribution) => WslDirectConfig::for_distribution(distribution),
            None => WslDirectConfig::default(),
        };
        // Pipe transport, NOT ConPTY: tmux control mode dies under a pseudo-console
        // in a GUI process. Pipes carry the line-based control protocol cleanly.
        Self::with_transport(WslDirectBackend::with_backend(
            wsl_config,
            PipeBackend::new(),
        ))
    }
}

impl Default for TmuxControlBackend<WslDirectBackend<PipeBackend>> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B> TmuxControlBackend<B> {
    pub fn with_transport(transport: B) -> Self {
        Self {
            transport,
            events: Vec::new(),
            sessions: HashMap::new(),
            parsers: HashMap::new(),
        }
    }

    pub fn transport(&self) -> &B {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut B {
        &mut self.transport
    }

    pub fn session_name_for_spawn(request: &SpawnRequest) -> String {
        // Key the tmux session on the unique agentmux session id, NOT the
        // workspace id. A workspace-shared name meant every durable pane ran
        // `new-session -A -s agentmux_<ws>` against the SAME tmux session: the
        // second pane onward only *attached* (no pane content is replayed in
        // control mode, so it rendered blank), and the contention killed the
        // server ("server exited unexpectedly"). One tmux session per pane keeps
        // them independent and lets each pane's prompt stream immediately.
        durable_session_name(&request.session_id)
    }

    fn target_for_session(&self, session_id: &str) -> BackendResult<String> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| BackendError::session_not_found(session_id))?;
        Ok(session
            .pane_id
            .clone()
            .unwrap_or_else(|| session.session_name.clone()))
    }

    fn session_name_for_session(&self, session_id: &str) -> BackendResult<String> {
        self.sessions
            .get(session_id)
            .map(|session| session.session_name.clone())
            .ok_or_else(|| BackendError::session_not_found(session_id))
    }

    fn send_control_line(&mut self, session_id: &str, line: String) -> BackendResult<()>
    where
        B: SessionBackend,
    {
        self.transport
            .send_input(session_id, InputEvent::Text(line))
            .map_err(tmux_control_transport_error)
    }
}

impl<B> SessionBackend for TmuxControlBackend<B>
where
    B: SessionBackend,
{
    fn kind(&self) -> BackendKind {
        BackendKind::WslTmuxControl
    }

    fn spawn(&mut self, mut request: SpawnRequest) -> BackendResult<SessionHandle> {
        if request
            .backend
            .is_some_and(|backend| backend != BackendKind::WslTmuxControl)
        {
            return Err(BackendError::unsupported(
                "tmux-control backend cannot spawn the requested backend kind.",
            ));
        }

        let session_name = Self::session_name_for_spawn(&request);
        request.command = tmux_control_spawn_command(&session_name, request.command);
        request.backend = Some(BackendKind::WslDirect);

        let mut handle = self
            .transport
            .spawn(request)
            .map_err(tmux_control_transport_error)?;
        let session_id = handle.session_id.clone();
        self.sessions.insert(
            session_id.clone(),
            TmuxControlSession {
                session_name: session_name.clone(),
                pane_id: None,
                started: false,
                replay_state: TmuxReplayState::AwaitingActivePane,
            },
        );
        self.parsers
            .insert(session_id.clone(), TmuxControlParser::new());
        handle.backend_kind = BackendKind::WslTmuxControl;
        handle.backend_native_id = Some(session_name.clone());
        self.send_control_line(&session_id, tmux_active_pane_control_line(&session_name))?;
        self.send_control_line(&session_id, tmux_capture_pane_control_line(&session_name))?;
        Ok(handle)
    }

    fn attach(&mut self, request: AttachRequest) -> BackendResult<SessionHandle> {
        let session_name = request.backend_ref;
        let mut handle = self
            .transport
            .spawn(SpawnRequest {
                session_id: request.session_id,
                workspace_id: None,
                backend: Some(BackendKind::WslDirect),
                backend_profile: request.backend_profile,
                command: tmux_control_attach_command(&session_name),
                cwd: None,
                env: Vec::new(),
                initial_size: request.initial_size,
            })
            .map_err(tmux_control_transport_error)?;
        let session_id = handle.session_id.clone();
        self.sessions.insert(
            session_id.clone(),
            TmuxControlSession {
                session_name: session_name.clone(),
                pane_id: None,
                started: false,
                replay_state: TmuxReplayState::AwaitingActivePane,
            },
        );
        self.parsers
            .insert(session_id.clone(), TmuxControlParser::new());
        handle.backend_kind = BackendKind::WslTmuxControl;
        handle.backend_native_id = Some(session_name.clone());
        self.send_control_line(&session_id, tmux_active_pane_control_line(&session_name))?;
        self.send_control_line(&session_id, tmux_capture_pane_control_line(&session_name))?;
        Ok(handle)
    }

    fn send_input(&mut self, session_id: &str, input: InputEvent) -> BackendResult<()> {
        let target = self.target_for_session(session_id)?;
        for line in tmux_control_lines_for_input(&target, input) {
            self.send_control_line(session_id, line)?;
        }
        Ok(())
    }

    fn resize(&mut self, session_id: &str, size: TerminalSize) -> BackendResult<()> {
        let target = self.target_for_session(session_id)?;
        self.send_control_line(session_id, tmux_resize_control_line(&target, size))?;
        self.transport
            .resize(session_id, size)
            .map_err(tmux_control_transport_error)
    }

    fn terminate(&mut self, session_id: &str, mode: TerminationMode) -> BackendResult<()> {
        match mode {
            TerminationMode::Interrupt => self.send_input(
                session_id,
                InputEvent::Control(agentmux_backend::ControlCode::Interrupt),
            ),
            TerminationMode::Soft => {
                self.send_control_line(session_id, tmux_detach_control_line())?;
                self.transport
                    .terminate(session_id, mode)
                    .map_err(tmux_control_transport_error)?;
                self.sessions.remove(session_id);
                self.parsers.remove(session_id);
                Ok(())
            }
            TerminationMode::Kill => {
                let target = self.session_name_for_session(session_id)?;
                self.send_control_line(session_id, tmux_kill_session_control_line(&target))?;
                std::thread::sleep(Duration::from_millis(50));
                self.transport
                    .terminate(session_id, mode)
                    .map_err(tmux_control_transport_error)?;
                self.sessions.remove(session_id);
                self.parsers.remove(session_id);
                Ok(())
            }
        }
    }

    fn drain_events(&mut self) -> Vec<BackendEvent> {
        let mut events = std::mem::take(&mut self.events);
        for event in self.transport.drain_events() {
            match event {
                BackendEvent::Started { session_id } if self.sessions.contains_key(&session_id) => {
                    events.push(BackendEvent::HealthChanged {
                        attachment_id: session_id,
                        state: BackendHealth::Starting,
                    });
                }
                BackendEvent::Output { session_id, bytes } => {
                    events.extend(self.apply_control_output(&session_id, &bytes));
                }
                BackendEvent::Exited { session_id, code } => {
                    self.sessions.remove(&session_id);
                    self.parsers.remove(&session_id);
                    events.push(BackendEvent::Exited { session_id, code });
                }
                other => events.push(other),
            }
        }
        events
    }
}

impl<B> TmuxControlBackend<B> {
    fn mark_session_started(
        sessions: &mut HashMap<String, TmuxControlSession>,
        session_id: &str,
        events: &mut Vec<BackendEvent>,
    ) {
        let Some(session) = sessions.get_mut(session_id) else {
            return;
        };
        if session.started {
            return;
        }
        session.started = true;
        events.push(BackendEvent::Started {
            session_id: session_id.to_string(),
        });
        events.push(BackendEvent::HealthChanged {
            attachment_id: session_id.to_string(),
            state: BackendHealth::Healthy,
        });
    }

    fn apply_control_output(&mut self, session_id: &str, bytes: &[u8]) -> Vec<BackendEvent> {
        let Some(parser) = self.parsers.get_mut(session_id) else {
            return vec![BackendEvent::Output {
                session_id: session_id.to_string(),
                bytes: bytes.to_vec(),
            }];
        };

        let mut events = Vec::new();
        for message in parser.push(bytes) {
            match message {
                TmuxControlMessage::Begin { .. } => {
                    if let Some(session) = self.sessions.get_mut(session_id) {
                        if session.replay_state == TmuxReplayState::AwaitingCaptureBegin {
                            session.replay_state = TmuxReplayState::Capturing;
                        }
                    }
                }
                TmuxControlMessage::End { .. } => {
                    if let Some(session) = self.sessions.get_mut(session_id) {
                        if session.replay_state == TmuxReplayState::Capturing {
                            session.replay_state = TmuxReplayState::None;
                        }
                    }
                }
                TmuxControlMessage::Output { pane_id, payload } => {
                    let mut resolved_pane = false;
                    if let Some(session) = self.sessions.get_mut(session_id) {
                        if session.pane_id.is_none() {
                            session.pane_id = Some(pane_id);
                            resolved_pane = true;
                        }
                    }
                    if resolved_pane {
                        Self::mark_session_started(&mut self.sessions, session_id, &mut events);
                    }
                    events.push(BackendEvent::Output {
                        session_id: session_id.to_string(),
                        bytes: payload,
                    });
                }
                TmuxControlMessage::PaneAdd { pane_id } => {
                    let mut resolved_pane = false;
                    if let Some(session) = self.sessions.get_mut(session_id) {
                        resolved_pane = session.pane_id.is_none();
                        session.pane_id = Some(pane_id);
                    }
                    if resolved_pane {
                        Self::mark_session_started(&mut self.sessions, session_id, &mut events);
                    }
                }
                TmuxControlMessage::CommandResponse { line, .. }
                    if self.sessions.get(session_id).is_some_and(|session| {
                        session.replay_state == TmuxReplayState::Capturing
                    }) =>
                {
                    let mut bytes = line.into_bytes();
                    bytes.extend_from_slice(b"\r\n");
                    events.push(BackendEvent::Output {
                        session_id: session_id.to_string(),
                        bytes,
                    });
                }
                TmuxControlMessage::CommandResponse { line, .. } if line.starts_with('%') => {
                    let mut resolved_pane = false;
                    if let Some(session) = self.sessions.get_mut(session_id) {
                        resolved_pane = session.pane_id.is_none();
                        session.pane_id = Some(line);
                        if session.replay_state == TmuxReplayState::AwaitingActivePane {
                            session.replay_state = TmuxReplayState::AwaitingCaptureBegin;
                        }
                    }
                    if resolved_pane {
                        Self::mark_session_started(&mut self.sessions, session_id, &mut events);
                    }
                }
                TmuxControlMessage::PaneClose { pane_id }
                | TmuxControlMessage::PaneDied { pane_id } => {
                    if self
                        .sessions
                        .get(session_id)
                        .and_then(|session| session.pane_id.as_deref())
                        == Some(pane_id.as_str())
                    {
                        events.push(BackendEvent::Exited {
                            session_id: session_id.to_string(),
                            code: None,
                        });
                    }
                }
                TmuxControlMessage::Exit => {
                    events.push(BackendEvent::Exited {
                        session_id: session_id.to_string(),
                        code: None,
                    });
                }
                TmuxControlMessage::Error { raw, .. } => events.push(BackendEvent::Error {
                    session_id: Some(session_id.to_string()),
                    error: BackendError::new("tmux_control_error", raw),
                }),
                TmuxControlMessage::Malformed { reason, .. } => events.push(BackendEvent::Error {
                    session_id: Some(session_id.to_string()),
                    error: BackendError::new("tmux_control_parse_error", reason),
                }),
                _ => {}
            }
        }
        events
    }
}

fn tmux_control_transport_error(error: BackendError) -> BackendError {
    match error.code.as_str() {
        "wsl_unavailable"
        | "no_wsl_distributions"
        | "wsl_distribution_not_found"
        | "invalid_wsl_cwd"
        | "wsl_launch_timeout" => error,
        "session_not_found" => error,
        "timeout" => BackendError::new("tmux_control_timeout", error.message),
        _ => BackendError::new("tmux_control_failed", error.message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_recognizes_output_lines() {
        assert_eq!(
            parse_control_line("%output %1 hello"),
            TmuxControlMessage::Output {
                pane_id: "%1".to_string(),
                payload: b"hello".to_vec()
            }
        );
    }

    #[test]
    fn parser_decodes_tmux_output_escapes_once() {
        assert_eq!(
            decode_tmux_payload(br"hello\040agentmux\012path\\tail"),
            b"hello agentmux\npath\\tail".to_vec()
        );
    }

    #[test]
    fn parser_tolerates_partial_lines_from_transport() {
        let mut parser = TmuxControlParser::new();

        assert!(parser.push(b"%output %1 hel").is_empty());
        assert_eq!(
            parser.push(b"lo\\012\r\n%exit\n"),
            vec![
                TmuxControlMessage::Output {
                    pane_id: "%1".to_string(),
                    payload: b"hello\n".to_vec()
                },
                TmuxControlMessage::Exit
            ]
        );
    }

    #[test]
    fn parser_correlates_command_response_lines_with_begin_id() {
        let messages = parse_fixture(include_str!(
            "../../../tests/fixtures/tmux-control/simple-command.txt"
        ));

        assert_eq!(
            messages,
            vec![
                TmuxControlMessage::Begin {
                    command_id: Some("7".to_string()),
                    raw: "1700000000 7 0".to_string()
                },
                TmuxControlMessage::CommandResponse {
                    command_id: Some("7".to_string()),
                    line: "pane-count 1".to_string()
                },
                TmuxControlMessage::End {
                    command_id: Some("7".to_string()),
                    raw: "1700000000 7 0".to_string()
                }
            ]
        );
    }

    #[test]
    fn parser_fixture_covers_output_escapes_and_topology_events() {
        let output = parse_fixture(include_str!(
            "../../../tests/fixtures/tmux-control/output-escapes.txt"
        ));
        assert_eq!(
            output,
            vec![
                TmuxControlMessage::Output {
                    pane_id: "%1".to_string(),
                    payload: b"hello agentmux\n".to_vec()
                },
                TmuxControlMessage::Output {
                    pane_id: "%1".to_string(),
                    payload: b"path\\tail".to_vec()
                }
            ]
        );

        let topology = parse_fixture(include_str!(
            "../../../tests/fixtures/tmux-control/topology-events.txt"
        ));
        assert!(topology.contains(&TmuxControlMessage::WindowAdd {
            window_id: "@1".to_string()
        }));
        assert!(topology.contains(&TmuxControlMessage::PaneAdd {
            pane_id: "%1".to_string()
        }));
        assert!(topology.contains(&TmuxControlMessage::PaneDied {
            pane_id: "%1".to_string()
        }));
        assert!(topology.contains(&TmuxControlMessage::Exit));
        assert!(topology.contains(&TmuxControlMessage::Unknown("%weird value".to_string())));
    }

    #[test]
    fn tmux_command_builders_use_argument_arrays() {
        assert_eq!(
            durable_session_name("ws_abcdef1234567890_extra"),
            "agentmux_abcdef1234567890_extra"
        );

        assert_eq!(
            tmux_control_launch_command("agentmux_demo").args,
            vec!["-C", "new-session", "-A", "-s", "agentmux_demo"]
        );
        assert_eq!(
            tmux_send_literal_command("%1", "hello world").args,
            vec!["send-keys", "-t", "%1", "-l", "hello world"]
        );
        assert_eq!(
            tmux_resize_pane_command("%1", TerminalSize::new(120, 30)).args,
            vec!["resize-pane", "-t", "%1", "-x", "120", "-y", "30"]
        );
        assert_eq!(
            tmux_control_spawn_command(
                "agentmux_demo",
                CommandSpec::with_args("bash", vec!["-lc".to_string(), "echo hello".to_string()])
            )
            .executable,
            "sh"
        );
        assert_eq!(
            tmux_control_spawn_command(
                "agentmux_demo",
                CommandSpec::with_args("bash", vec!["-lc".to_string(), "echo hello".to_string()])
            )
            .args,
            vec![
                "-lc",
                concat!(
                    "if ! tmux has-session -t agentmux_demo 2>/dev/null; then ",
                    r#"tmux new-session -d -s agentmux_demo 'bash -lc '\''echo hello'\''' || exit $?; "#,
                    "fi; ",
                    "exec tmux -C attach-session -t agentmux_demo"
                )
            ]
        );
    }

    #[test]
    fn parser_tolerates_unknown_lines() {
        assert_eq!(
            parse_control_line("%weird value"),
            TmuxControlMessage::Unknown("%weird value".to_string())
        );
    }

    #[test]
    fn parser_skips_conpty_escape_prefix_before_control_lines() {
        let mut parser = TmuxControlParser::new();

        assert_eq!(
            parser.push(
                b"\x1b]0;C:\\Windows\\SYSTEM32\\wsl.exe\x07\x1b[?25h%output %1 hello\\015\\012\r\n"
            ),
            vec![TmuxControlMessage::Output {
                pane_id: "%1".to_string(),
                payload: b"hello\r\n".to_vec()
            }]
        );
    }

    #[test]
    fn backend_kind_is_tmux_control() {
        let backend = TmuxControlBackend::new();
        assert_eq!(backend.kind(), BackendKind::WslTmuxControl);
    }

    #[test]
    fn spawn_launches_tmux_control_session_through_wsl_transport() {
        let transport = RecordingTransport::default();
        let mut backend = TmuxControlBackend::with_transport(transport);
        let handle = backend
            .spawn(SpawnRequest {
                session_id: "ses_tmux".to_string(),
                workspace_id: Some("ws_demo123".to_string()),
                backend: Some(BackendKind::WslTmuxControl),
                backend_profile: Some("Ubuntu".to_string()),
                command: CommandSpec::with_args(
                    "bash",
                    vec!["-lc".to_string(), "echo hello".to_string()],
                ),
                cwd: Some("/home/dev/repo".to_string()),
                env: Vec::new(),
                initial_size: TerminalSize::new(120, 30),
            })
            .unwrap();

        assert_eq!(handle.backend_kind, BackendKind::WslTmuxControl);
        assert_eq!(
            handle.backend_native_id.as_deref(),
            Some("agentmux_ses_tmux")
        );
        let spawn = backend.transport().last_spawn.as_ref().unwrap();
        assert_eq!(spawn.backend, Some(BackendKind::WslDirect));
        assert_eq!(spawn.backend_profile.as_deref(), Some("Ubuntu"));
        assert_eq!(spawn.command.executable, "sh");
        assert_eq!(
            spawn.command.args,
            vec![
                "-lc",
                concat!(
                    "if ! tmux has-session -t agentmux_ses_tmux 2>/dev/null; then ",
                    r#"tmux new-session -d -s agentmux_ses_tmux 'bash -lc '\''echo hello'\''' || exit $?; "#,
                    "fi; ",
                    "exec tmux -C attach-session -t agentmux_ses_tmux"
                )
            ]
        );
        assert_eq!(
            backend.transport().sent_text,
            vec![
                "display-message -p -t agentmux_ses_tmux \"#{pane_id}\"\n",
                "capture-pane -p -e -t agentmux_ses_tmux\n"
            ]
        );
    }

    #[test]
    fn attach_launches_tmux_control_attach_through_wsl_transport() {
        let transport = RecordingTransport::default();
        let mut backend = TmuxControlBackend::with_transport(transport);
        let handle = backend
            .attach(AttachRequest {
                session_id: "ses_recovered".to_string(),
                backend: BackendKind::WslTmuxControl,
                backend_profile: Some("Ubuntu".to_string()),
                backend_ref: "agentmux_demo123".to_string(),
                initial_size: TerminalSize::new(100, 24),
            })
            .unwrap();

        assert_eq!(handle.backend_kind, BackendKind::WslTmuxControl);
        assert_eq!(
            handle.backend_native_id.as_deref(),
            Some("agentmux_demo123")
        );
        assert_eq!(
            backend
                .transport()
                .last_spawn
                .as_ref()
                .unwrap()
                .command
                .args,
            vec!["-C", "attach-session", "-t", "agentmux_demo123"]
        );
        assert_eq!(
            backend
                .transport()
                .last_spawn
                .as_ref()
                .unwrap()
                .backend_profile
                .as_deref(),
            Some("Ubuntu")
        );
        assert_eq!(
            backend.transport().sent_text,
            vec![
                "display-message -p -t agentmux_demo123 \"#{pane_id}\"\n",
                "capture-pane -p -e -t agentmux_demo123\n"
            ]
        );
    }

    #[test]
    fn transport_start_does_not_mark_tmux_ready_until_control_pane_resolves() {
        let transport = RecordingTransport::default();
        let mut backend = TmuxControlBackend::with_transport(transport);
        backend
            .spawn(SpawnRequest {
                session_id: "ses_tmux".to_string(),
                workspace_id: Some("ws_demo123".to_string()),
                backend: Some(BackendKind::WslTmuxControl),
                backend_profile: None,
                command: CommandSpec::new("bash"),
                cwd: None,
                env: Vec::new(),
                initial_size: TerminalSize::new(80, 24),
            })
            .unwrap();

        let startup_events = backend.drain_events();
        assert!(!startup_events.contains(&BackendEvent::Started {
            session_id: "ses_tmux".to_string(),
        }));
        assert!(startup_events.contains(&BackendEvent::HealthChanged {
            attachment_id: "ses_tmux".to_string(),
            state: BackendHealth::Starting,
        }));

        backend.transport_mut().events.push(BackendEvent::Output {
            session_id: "ses_tmux".to_string(),
            bytes: b"%pane-add %4\n".to_vec(),
        });
        let ready_events = backend.drain_events();
        assert!(ready_events.contains(&BackendEvent::Started {
            session_id: "ses_tmux".to_string(),
        }));
        assert!(ready_events.contains(&BackendEvent::HealthChanged {
            attachment_id: "ses_tmux".to_string(),
            state: BackendHealth::Healthy,
        }));
    }

    #[test]
    fn attach_replays_captured_pane_contents_after_active_pane_resolution() {
        let transport = RecordingTransport::default();
        let mut backend = TmuxControlBackend::with_transport(transport);
        backend
            .attach(AttachRequest {
                session_id: "ses_recovered".to_string(),
                backend: BackendKind::WslTmuxControl,
                backend_profile: Some("Ubuntu".to_string()),
                backend_ref: "agentmux_demo123".to_string(),
                initial_size: TerminalSize::new(100, 24),
            })
            .unwrap();
        backend.transport_mut().events.push(BackendEvent::Output {
            session_id: "ses_recovered".to_string(),
            bytes: concat!(
                "%begin 1 1 0\n",
                "%1\n",
                "%end 1 1 0\n",
                "%begin 1 2 0\n",
                "hello\n",
                "%literal output\n",
                "world\n",
                "%end 1 2 0\n"
            )
            .as_bytes()
            .to_vec(),
        });

        let events = backend.drain_events();

        assert!(events.contains(&BackendEvent::Output {
            session_id: "ses_recovered".to_string(),
            bytes: b"hello\r\n".to_vec()
        }));
        assert!(events.contains(&BackendEvent::Output {
            session_id: "ses_recovered".to_string(),
            bytes: b"%literal output\r\n".to_vec()
        }));
        assert!(events.contains(&BackendEvent::Output {
            session_id: "ses_recovered".to_string(),
            bytes: b"world\r\n".to_vec()
        }));
        assert_eq!(
            backend.transport().sent_text.last().map(String::as_str),
            Some("capture-pane -p -e -t agentmux_demo123\n")
        );
    }

    #[test]
    fn input_resize_and_soft_terminate_send_tmux_control_commands() {
        let transport = RecordingTransport::default();
        let mut backend = TmuxControlBackend::with_transport(transport);
        backend
            .spawn(SpawnRequest {
                session_id: "ses_tmux".to_string(),
                workspace_id: Some("ws_demo123".to_string()),
                backend: Some(BackendKind::WslTmuxControl),
                backend_profile: None,
                command: CommandSpec::new("bash"),
                cwd: None,
                env: Vec::new(),
                initial_size: TerminalSize::new(80, 24),
            })
            .unwrap();

        backend
            .send_input("ses_tmux", InputEvent::Text("hi\n".to_string()))
            .unwrap();
        backend
            .resize("ses_tmux", TerminalSize::new(120, 30))
            .unwrap();
        backend
            .terminate("ses_tmux", TerminationMode::Soft)
            .unwrap();

        assert_eq!(
            backend.transport().sent_text,
            vec![
                "display-message -p -t agentmux_ses_tmux \"#{pane_id}\"\n",
                "capture-pane -p -e -t agentmux_ses_tmux\n",
                "send-keys -t agentmux_ses_tmux -l hi\n",
                "send-keys -t agentmux_ses_tmux Enter\n",
                concat!(
                    "refresh-client -C 120x30\n",
                    "resize-window -t agentmux_ses_tmux -x 120 -y 30\n",
                    "resize-pane -t agentmux_ses_tmux -x 120 -y 30\n"
                ),
                "detach-client\n"
            ]
        );
        assert_eq!(
            backend.transport().last_resize,
            Some(TerminalSize::new(120, 30))
        );
        assert_eq!(
            backend.transport().last_termination,
            Some(TerminationMode::Soft)
        );
    }

    #[test]
    fn drain_events_parses_tmux_output_and_tracks_pane_target() {
        let transport = RecordingTransport::default();
        let mut backend = TmuxControlBackend::with_transport(transport);
        backend
            .spawn(SpawnRequest {
                session_id: "ses_tmux".to_string(),
                workspace_id: Some("ws_demo123".to_string()),
                backend: Some(BackendKind::WslTmuxControl),
                backend_profile: None,
                command: CommandSpec::new("bash"),
                cwd: None,
                env: Vec::new(),
                initial_size: TerminalSize::new(80, 24),
            })
            .unwrap();
        backend.transport_mut().events.push(BackendEvent::Output {
            session_id: "ses_tmux".to_string(),
            bytes: b"%pane-add %4\n%output %4 hello\\012\n".to_vec(),
        });

        let events = backend.drain_events();
        assert!(events.contains(&BackendEvent::Output {
            session_id: "ses_tmux".to_string(),
            bytes: b"hello\n".to_vec()
        }));

        backend
            .send_input("ses_tmux", InputEvent::Text("x".to_string()))
            .unwrap();
        assert_eq!(
            backend.transport().sent_text.last().map(String::as_str),
            Some("send-keys -t %4 -l x\n")
        );
    }

    #[derive(Default)]
    struct RecordingTransport {
        last_spawn: Option<SpawnRequest>,
        events: Vec<BackendEvent>,
        sent_text: Vec<String>,
        last_resize: Option<TerminalSize>,
        last_termination: Option<TerminationMode>,
    }

    impl SessionBackend for RecordingTransport {
        fn kind(&self) -> BackendKind {
            BackendKind::WslDirect
        }

        fn spawn(&mut self, request: SpawnRequest) -> BackendResult<SessionHandle> {
            self.events.push(BackendEvent::Started {
                session_id: request.session_id.clone(),
            });
            self.last_spawn = Some(request.clone());
            Ok(SessionHandle {
                session_id: request.session_id,
                backend_kind: BackendKind::WslDirect,
                backend_native_id: Some("transport".to_string()),
                transport_pid: Some(7),
            })
        }

        fn attach(&mut self, _request: AttachRequest) -> BackendResult<SessionHandle> {
            Err(BackendError::unsupported(
                "recording transport does not attach",
            ))
        }

        fn send_input(&mut self, _session_id: &str, input: InputEvent) -> BackendResult<()> {
            if let InputEvent::Text(text) = input {
                self.sent_text.push(text);
            }
            Ok(())
        }

        fn resize(&mut self, _session_id: &str, size: TerminalSize) -> BackendResult<()> {
            self.last_resize = Some(size);
            Ok(())
        }

        fn terminate(&mut self, _session_id: &str, mode: TerminationMode) -> BackendResult<()> {
            self.last_termination = Some(mode);
            Ok(())
        }

        fn drain_events(&mut self) -> Vec<BackendEvent> {
            std::mem::take(&mut self.events)
        }
    }
}
