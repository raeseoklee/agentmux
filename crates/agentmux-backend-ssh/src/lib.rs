//! SSH session backend built on `russh` (pure-Rust SSH client, `ring` crypto).
//!
//! The [`SessionBackend`] trait is synchronous, while `russh` is async. Each
//! session therefore owns a dedicated OS thread running a current-thread Tokio
//! runtime; the sync facade talks to it over Tokio mpsc channels and a shared
//! event queue that [`SshDirectBackend::drain_events`] flushes.
//!
//! Auth: public-key only (a private key file; `$AGENTMUX_SSH_KEY` or the usual
//! `~/.ssh/id_*` files). Host-key verification currently accepts any key — see
//! the SECURITY note on [`AcceptingClient`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use agentmux_backend::{
    AttachRequest, BackendError, BackendEvent, BackendKind, BackendResult, ControlCode, InputEvent,
    NamedKey, SessionBackend, SessionHandle, SpawnRequest, TerminalSize, TerminationMode,
};
use russh::client;
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg};
use russh::ChannelMsg;
use tokio::sync::mpsc;

const DEFAULT_PORT: u16 = 22;
const DEFAULT_TERM: &str = "xterm-256color";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshTarget {
    pub user: String,
    pub host: String,
    pub port: u16,
}

/// Parse a `user@host:port` connection string. `user` and `:port` are optional;
/// they fall back to the current OS user and port 22.
pub fn parse_ssh_target(value: &str) -> BackendResult<SshTarget> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(BackendError::invalid_request(
            "ssh target must be 'user@host[:port]'.",
        ));
    }

    let (user, rest) = match trimmed.split_once('@') {
        Some((user, rest)) => (user.to_string(), rest),
        None => (default_user(), trimmed),
    };

    let (host, port) = match rest.rsplit_once(':') {
        Some((host, port)) => {
            let port = port.parse::<u16>().map_err(|_| {
                BackendError::invalid_request(format!("invalid ssh port '{port}'."))
            })?;
            (host.to_string(), port)
        }
        None => (rest.to_string(), DEFAULT_PORT),
    };

    if host.is_empty() {
        return Err(BackendError::invalid_request("ssh target host is empty."));
    }
    if user.is_empty() {
        return Err(BackendError::invalid_request("ssh target user is empty."));
    }

    Ok(SshTarget { user, host, port })
}

fn default_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "root".to_string())
}

/// Resolve the private key file: `$AGENTMUX_SSH_KEY`, else the first existing of
/// `~/.ssh/id_ed25519`, `id_ecdsa`, `id_rsa`.
fn resolve_identity_file() -> BackendResult<PathBuf> {
    if let Some(explicit) = std::env::var_os("AGENTMUX_SSH_KEY") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Ok(path);
        }
        return Err(BackendError::new(
            "ssh_identity_missing",
            format!(
                "AGENTMUX_SSH_KEY points to a missing file: {}",
                path.display()
            ),
        ));
    }

    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| {
            BackendError::new(
                "ssh_identity_missing",
                "could not resolve the home directory.",
            )
        })?;
    for name in ["id_ed25519", "id_ecdsa", "id_rsa"] {
        let candidate = home.join(".ssh").join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(BackendError::new(
        "ssh_identity_missing",
        "no SSH private key found; set AGENTMUX_SSH_KEY or create ~/.ssh/id_ed25519.",
    ))
}

/// Encode a UI input event into the byte stream sent to the remote PTY.
pub fn input_to_bytes(input: InputEvent) -> Vec<u8> {
    match input {
        InputEvent::Text(text) => text.into_bytes(),
        InputEvent::Paste { text, bracketed } => {
            if bracketed {
                format!("\x1b[200~{text}\x1b[201~").into_bytes()
            } else {
                text.into_bytes()
            }
        }
        InputEvent::Key(key) => named_key_bytes(key),
        InputEvent::Control(code) => match code {
            ControlCode::Interrupt => vec![0x03],
            ControlCode::EndOfTransmission => vec![0x04],
        },
    }
}

fn named_key_bytes(key: NamedKey) -> Vec<u8> {
    match key {
        NamedKey::Enter => vec![b'\r'],
        NamedKey::Backspace => vec![0x7f],
        NamedKey::Tab => vec![b'\t'],
        NamedKey::Escape => vec![0x1b],
        NamedKey::ArrowUp => b"\x1b[A".to_vec(),
        NamedKey::ArrowDown => b"\x1b[B".to_vec(),
        NamedKey::ArrowRight => b"\x1b[C".to_vec(),
        NamedKey::ArrowLeft => b"\x1b[D".to_vec(),
        NamedKey::Function(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            _ => format!("\x1b[{}~", 10 + u16::from(n)).into_bytes(),
        },
    }
}

struct SshSession {
    input_tx: mpsc::UnboundedSender<Vec<u8>>,
    resize_tx: mpsc::UnboundedSender<(u16, u16)>,
    terminate_tx: mpsc::UnboundedSender<()>,
}

#[derive(Default)]
pub struct SshDirectBackend {
    events: Arc<Mutex<Vec<BackendEvent>>>,
    sessions: HashMap<String, SshSession>,
}

impl SshDirectBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn push_event(events: &Arc<Mutex<Vec<BackendEvent>>>, event: BackendEvent) {
        if let Ok(mut queue) = events.lock() {
            queue.push(event);
        }
    }
}

impl SessionBackend for SshDirectBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Ssh
    }

    fn spawn(&mut self, request: SpawnRequest) -> BackendResult<SessionHandle> {
        let target_spec = request.backend_profile.clone().ok_or_else(|| {
            BackendError::invalid_request("ssh backend requires a 'user@host:port' profile.")
        })?;
        let target = parse_ssh_target(&target_spec)?;
        let identity = resolve_identity_file()?;
        let session_id = request.session_id.clone();
        let size = request.initial_size;
        let native_id = format!("{}@{}:{}", target.user, target.host, target.port);

        let (input_tx, input_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (resize_tx, resize_rx) = mpsc::unbounded_channel::<(u16, u16)>();
        let (terminate_tx, terminate_rx) = mpsc::unbounded_channel::<()>();
        let events = Arc::clone(&self.events);
        let thread_session_id = session_id.clone();

        let spawn_result = thread::Builder::new()
            .name(format!("agentmux-ssh-{session_id}"))
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        SshDirectBackend::push_event(
                            &events,
                            BackendEvent::Error {
                                session_id: Some(thread_session_id.clone()),
                                error: BackendError::spawn_failed(format!(
                                    "failed to start ssh runtime: {error}"
                                )),
                            },
                        );
                        return;
                    }
                };

                let outcome = runtime.block_on(run_session(
                    thread_session_id.clone(),
                    target,
                    identity,
                    size,
                    Arc::clone(&events),
                    input_rx,
                    resize_rx,
                    terminate_rx,
                ));

                match outcome {
                    Ok(code) => SshDirectBackend::push_event(
                        &events,
                        BackendEvent::Exited {
                            session_id: thread_session_id,
                            code,
                        },
                    ),
                    Err(error) => SshDirectBackend::push_event(
                        &events,
                        BackendEvent::Error {
                            session_id: Some(thread_session_id),
                            error,
                        },
                    ),
                }
            });

        if let Err(error) = spawn_result {
            return Err(BackendError::spawn_failed(format!(
                "failed to spawn ssh session thread: {error}"
            )));
        }

        self.sessions.insert(
            session_id.clone(),
            SshSession {
                input_tx,
                resize_tx,
                terminate_tx,
            },
        );

        Ok(SessionHandle {
            session_id,
            backend_kind: BackendKind::Ssh,
            backend_native_id: Some(native_id),
            transport_pid: None,
        })
    }

    fn attach(&mut self, _request: AttachRequest) -> BackendResult<SessionHandle> {
        Err(BackendError::unsupported(
            "ssh backend does not support attaching to existing sessions.",
        ))
    }

    fn send_input(&mut self, session_id: &str, input: InputEvent) -> BackendResult<()> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| BackendError::session_not_found(session_id))?;
        session
            .input_tx
            .send(input_to_bytes(input))
            .map_err(|_| BackendError::input_failed("ssh session is no longer accepting input."))
    }

    fn resize(&mut self, session_id: &str, size: TerminalSize) -> BackendResult<()> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| BackendError::session_not_found(session_id))?;
        session
            .resize_tx
            .send((size.columns, size.rows))
            .map_err(|_| BackendError::resize_failed("ssh session is no longer accepting resizes."))
    }

    fn terminate(&mut self, session_id: &str, _mode: TerminationMode) -> BackendResult<()> {
        let Some(session) = self.sessions.remove(session_id) else {
            return Err(BackendError::session_not_found(session_id));
        };
        // Best-effort: the worker tears down the channel and emits Exited.
        let _ = session.terminate_tx.send(());
        Ok(())
    }

    fn drain_events(&mut self) -> Vec<BackendEvent> {
        match self.events.lock() {
            Ok(mut queue) => std::mem::take(&mut *queue),
            Err(_) => Vec::new(),
        }
    }
}

/// SECURITY: accepts any server host key (trust-on-first-use is not yet
/// implemented). Acceptable for a local dev tool; must be hardened before any
/// untrusted-network use.
struct AcceptingClient;

impl client::Handler for AcceptingClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_session(
    session_id: String,
    target: SshTarget,
    identity: PathBuf,
    size: TerminalSize,
    events: Arc<Mutex<Vec<BackendEvent>>>,
    mut input_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    mut resize_rx: mpsc::UnboundedReceiver<(u16, u16)>,
    mut terminate_rx: mpsc::UnboundedReceiver<()>,
) -> BackendResult<Option<i32>> {
    let key = load_secret_key(&identity, None).map_err(|error| {
        BackendError::new(
            "ssh_identity_invalid",
            format!("failed to load SSH key {}: {error}", identity.display()),
        )
    })?;

    let config = Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(30)),
        ..Default::default()
    });

    let mut session = client::connect(config, (target.host.as_str(), target.port), AcceptingClient)
        .await
        .map_err(|error| {
            BackendError::new(
                "ssh_connect_failed",
                format!(
                    "failed to connect to {}:{}: {error}",
                    target.host, target.port
                ),
            )
        })?;

    let hash_alg = session
        .best_supported_rsa_hash()
        .await
        .map_err(ssh_error)?
        .flatten();
    let auth = session
        .authenticate_publickey(
            &target.user,
            PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
        )
        .await
        .map_err(ssh_error)?;
    if !auth.success() {
        return Err(BackendError::new(
            "ssh_auth_failed",
            format!("public-key authentication failed for {}", target.user),
        ));
    }

    let mut channel = session.channel_open_session().await.map_err(ssh_error)?;
    let cols = u32::from(size.columns.max(1));
    let rows = u32::from(size.rows.max(1));
    channel
        .request_pty(false, DEFAULT_TERM, cols, rows, 0, 0, &[])
        .await
        .map_err(ssh_error)?;
    channel.request_shell(true).await.map_err(ssh_error)?;

    SshDirectBackend::push_event(
        &events,
        BackendEvent::Started {
            session_id: session_id.clone(),
        },
    );

    let mut exit_code: Option<i32> = None;
    loop {
        tokio::select! {
            input = input_rx.recv() => {
                match input {
                    Some(bytes) => channel.data(&bytes[..]).await.map_err(ssh_error)?,
                    None => break,
                }
            }
            resize = resize_rx.recv() => {
                if let Some((columns, rows)) = resize {
                    channel
                        .window_change(u32::from(columns.max(1)), u32::from(rows.max(1)), 0, 0)
                        .await
                        .map_err(ssh_error)?;
                }
            }
            _ = terminate_rx.recv() => {
                let _ = channel.eof().await;
                break;
            }
            message = channel.wait() => {
                match message {
                    Some(ChannelMsg::Data { ref data }) => {
                        SshDirectBackend::push_event(
                            &events,
                            BackendEvent::Output {
                                session_id: session_id.clone(),
                                bytes: data.to_vec(),
                            },
                        );
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        exit_code = Some(exit_status as i32);
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                    _ => {}
                }
            }
        }
    }

    Ok(exit_code)
}

fn ssh_error(error: russh::Error) -> BackendError {
    BackendError::new("ssh_protocol_error", error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_target() {
        let target = parse_ssh_target("deploy@10.0.4.12:2222").unwrap();
        assert_eq!(target.user, "deploy");
        assert_eq!(target.host, "10.0.4.12");
        assert_eq!(target.port, 2222);
    }

    #[test]
    fn defaults_port_when_absent() {
        let target = parse_ssh_target("ops@staging.lan").unwrap();
        assert_eq!(target.host, "staging.lan");
        assert_eq!(target.port, 22);
    }

    #[test]
    fn rejects_empty_target() {
        assert!(parse_ssh_target("   ").is_err());
    }

    #[test]
    fn rejects_bad_port() {
        assert!(parse_ssh_target("u@h:notaport").is_err());
    }

    #[test]
    fn encodes_named_keys_and_control() {
        assert_eq!(input_to_bytes(InputEvent::Key(NamedKey::Enter)), b"\r");
        assert_eq!(
            input_to_bytes(InputEvent::Key(NamedKey::ArrowUp)),
            b"\x1b[A"
        );
        assert_eq!(
            input_to_bytes(InputEvent::Control(ControlCode::Interrupt)),
            vec![0x03]
        );
        assert_eq!(input_to_bytes(InputEvent::Text("hi".to_string())), b"hi");
    }
}
