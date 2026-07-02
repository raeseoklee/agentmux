//! A process transport that talks to the child over plain OS pipes (no
//! pseudo-console).
//!
//! `ConptyBackend` wraps every child in a Windows pseudo-console (ConPTY). That
//! is right for interactive shells, but it breaks `tmux -C` (control mode): in a
//! GUI process with no attached console the pseudo-console host is unreliable and
//! tmux reports `server exited unexpectedly` immediately. tmux control mode is a
//! line-oriented protocol on stdout/stdin and needs no terminal, so this backend
//! spawns the child with piped stdio instead and streams stdout straight through.
//!
//! It implements `SessionBackend` with `kind() == Conpty` so it can be dropped in
//! as the inner transport of `WslDirectBackend<PipeBackend>`.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use agentmux_backend::{
    AttachRequest, BackendError, BackendEvent, BackendKind, BackendResult, ControlCode, InputEvent,
    SessionBackend, SessionHandle, SpawnRequest, TerminalSize, TerminationMode,
};
use agentmux_backend_conpty::input_event_bytes;

struct PipeSession {
    session_id: String,
    child: Child,
    stdin: Option<ChildStdin>,
    exit_reported: bool,
}

#[derive(Default)]
pub struct PipeBackend {
    sessions: HashMap<String, PipeSession>,
    events: Arc<Mutex<Vec<BackendEvent>>>,
}

impl PipeBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn push_event(&self, event: BackendEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }

    fn poll_exits(&mut self) {
        let mut exited = Vec::new();
        for session in self.sessions.values_mut() {
            if session.exit_reported {
                continue;
            }
            if let Ok(Some(status)) = session.child.try_wait() {
                session.exit_reported = true;
                exited.push((session.session_id.clone(), status.code()));
            }
        }
        for (session_id, code) in exited {
            self.sessions.remove(&session_id);
            self.push_event(BackendEvent::Exited { session_id, code });
        }
    }
}

impl SessionBackend for PipeBackend {
    fn kind(&self) -> BackendKind {
        // Reports Conpty so it is interchangeable with ConptyBackend as the inner
        // transport of WslDirectBackend (which sets request.backend = Conpty).
        BackendKind::Conpty
    }

    fn spawn(&mut self, request: SpawnRequest) -> BackendResult<SessionHandle> {
        let mut command = Command::new(&request.command.executable);
        command.args(&request.command.args);
        if let Some(cwd) = request.cwd.as_deref().filter(|value| !value.is_empty()) {
            command.current_dir(cwd);
        }
        for (key, value) in &request.env {
            command.env(key, value);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // tmux control protocol is on stdout; discard stderr so a full pipe
            // can never block the child and stray text can't corrupt the stream.
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: no console window flash, and the child runs without
            // a console — which is exactly what tmux -C wants.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command
            .spawn()
            .map_err(|error| BackendError::spawn_failed(format!("pipe spawn failed: {error}")))?;

        let session_id = request.session_id.clone();
        let pid = child.id();
        let stdin = child.stdin.take();
        if let Some(stdout) = child.stdout.take() {
            spawn_output_reader(session_id.clone(), stdout, Arc::clone(&self.events));
        }

        self.sessions.insert(
            session_id.clone(),
            PipeSession {
                session_id: session_id.clone(),
                child,
                stdin,
                exit_reported: false,
            },
        );
        self.push_event(BackendEvent::Started {
            session_id: session_id.clone(),
        });

        Ok(SessionHandle {
            session_id,
            backend_kind: BackendKind::Conpty,
            backend_native_id: Some(pid.to_string()),
            transport_pid: Some(pid),
        })
    }

    fn attach(&mut self, _request: AttachRequest) -> BackendResult<SessionHandle> {
        Err(BackendError::unsupported(
            "pipe backend sessions are not attachable.",
        ))
    }

    fn send_input(&mut self, session_id: &str, input: InputEvent) -> BackendResult<()> {
        let bytes = input_event_bytes(&input)?;
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| BackendError::session_not_found(session_id))?;
        let stdin = session
            .stdin
            .as_mut()
            .ok_or_else(|| BackendError::input_failed("session stdin is closed"))?;
        stdin
            .write_all(&bytes)
            .map_err(|error| BackendError::input_failed(format!("write stdin failed: {error}")))?;
        stdin
            .flush()
            .map_err(|error| BackendError::input_failed(format!("flush stdin failed: {error}")))?;
        Ok(())
    }

    fn resize(&mut self, session_id: &str, size: TerminalSize) -> BackendResult<()> {
        // Pipes carry no terminal geometry; tmux-control resizes are driven by the
        // tmux backend's `resize-pane` control command, so this only acknowledges.
        if !self.sessions.contains_key(session_id) {
            return Err(BackendError::session_not_found(session_id));
        }
        self.push_event(BackendEvent::Resized {
            session_id: session_id.to_string(),
            columns: size.columns,
            rows: size.rows,
        });
        Ok(())
    }

    fn terminate(&mut self, session_id: &str, mode: TerminationMode) -> BackendResult<()> {
        match mode {
            TerminationMode::Interrupt => {
                self.send_input(session_id, InputEvent::Control(ControlCode::Interrupt))
            }
            TerminationMode::Soft => {
                let session = self
                    .sessions
                    .get_mut(session_id)
                    .ok_or_else(|| BackendError::session_not_found(session_id))?;
                // Close stdin so the child (tmux control client) receives EOF and
                // detaches cleanly. The session stays in the map so that
                // `poll_exits` can call `try_wait`, reap the child (avoiding a
                // zombie on Unix/WSL), and emit `BackendEvent::Exited` with the
                // real exit code — matching ConptyBackend lifecycle semantics.
                drop(session.stdin.take());
                Ok(())
            }
            TerminationMode::Kill => {
                let mut session = self
                    .sessions
                    .remove(session_id)
                    .ok_or_else(|| BackendError::session_not_found(session_id))?;
                let _ = session.child.kill();
                // Reap the child immediately to avoid a zombie PID on Unix/WSL.
                // On Windows this is a no-op in terms of zombie semantics but
                // ensures the OS handle is released promptly.
                let _ = session.child.wait();
                Ok(())
            }
        }
    }

    fn drain_events(&mut self) -> Vec<BackendEvent> {
        self.poll_exits();
        if let Ok(mut events) = self.events.lock() {
            std::mem::take(&mut *events)
        } else {
            Vec::new()
        }
    }
}

fn spawn_output_reader<R: Read + Send + 'static>(
    session_id: String,
    mut reader: R,
    events: Arc<Mutex<Vec<BackendEvent>>>,
) {
    thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    // EOF: child process has exited cleanly. Do not push an
                    // error event — `poll_exits` will call `child.try_wait()`
                    // in `drain_events` and emit `BackendEvent::Exited` with
                    // the real exit code, consistent with ConptyBackend
                    // behavior (which breaks silently on ERROR_BROKEN_PIPE /
                    // ERROR_HANDLE_EOF and relies on `poll_exits` for the exit
                    // signal).
                    break;
                }
                Ok(read) => {
                    if let Ok(mut events) = events.lock() {
                        events.push(BackendEvent::Output {
                            session_id: session_id.clone(),
                            bytes: buffer[..read].to_vec(),
                        });
                    }
                }
                Err(_) => break,
            }
        }
    });
}
