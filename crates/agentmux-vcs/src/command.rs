use std::collections::BTreeMap;
use std::io::{self, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::{GitContext, GitError, GitHost, OutputStream, Result};

const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(4);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OverflowPolicy {
    Fail,
    Truncate,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CaptureLimits {
    pub timeout: Duration,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub stdout_overflow: OverflowPolicy,
}

#[derive(Debug)]
pub(crate) struct ExecutionOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
}

pub fn build_git_command_spec(
    context: &GitContext,
    arguments: &[String],
    read_only: bool,
) -> CommandSpec {
    let mut env = BTreeMap::from([
        ("GIT_TERMINAL_PROMPT".to_string(), "0".to_string()),
        ("GCM_INTERACTIVE".to_string(), "Never".to_string()),
        ("LC_ALL".to_string(), "C".to_string()),
    ]);
    if read_only {
        env.insert("GIT_OPTIONAL_LOCKS".to_string(), "0".to_string());
    }

    match &context.host {
        GitHost::Native => {
            let mut args = vec![
                "-C".to_string(),
                context.cwd.clone(),
                "--literal-pathspecs".to_string(),
            ];
            args.extend(arguments.iter().cloned());
            CommandSpec {
                program: "git".to_string(),
                args,
                env,
            }
        }
        GitHost::Wsl { distribution } => {
            let mut args = Vec::new();
            if let Some(distribution) = distribution
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                args.extend(["--distribution".to_string(), distribution.to_string()]);
            }
            args.extend([
                "--exec".to_string(),
                "git".to_string(),
                "-C".to_string(),
                context.cwd.clone(),
                "--literal-pathspecs".to_string(),
            ]);
            args.extend(arguments.iter().cloned());
            CommandSpec {
                program: "wsl.exe".to_string(),
                args,
                env,
            }
        }
    }
}

pub(crate) fn build_wsl_utility_spec(
    host: &GitHost,
    program: &str,
    arguments: &[String],
) -> Result<CommandSpec> {
    let GitHost::Wsl { distribution } = host else {
        return Err(GitError::InvalidWorktreeRequest(
            "A WSL filesystem check requires a WSL repository.".to_string(),
        ));
    };
    let mut args = Vec::new();
    if let Some(distribution) = distribution
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        args.extend(["--distribution".to_string(), distribution.to_string()]);
    }
    args.extend(["--exec".to_string(), program.to_string()]);
    args.extend(arguments.iter().cloned());
    Ok(CommandSpec {
        program: "wsl.exe".to_string(),
        args,
        env: BTreeMap::from([("LC_ALL".to_string(), "C".to_string())]),
    })
}

pub(crate) fn execute(
    spec: &CommandSpec,
    operation: &str,
    limits: CaptureLimits,
) -> Result<ExecutionOutput> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .envs(&spec.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console_window(&mut command);

    let mut child = command.spawn().map_err(|error| GitError::Io {
        operation: operation.to_string(),
        message: error.to_string(),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        GitError::StateUnavailable("Git stdout pipe was unavailable.".to_string())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        GitError::StateUnavailable("Git stderr pipe was unavailable.".to_string())
    })?;
    let (overflow_tx, overflow_rx) = mpsc::channel();
    let stdout_reader = spawn_reader(
        stdout,
        limits.stdout_bytes,
        limits.stdout_overflow,
        OutputStream::Stdout,
        overflow_tx.clone(),
    );
    let stderr_reader = spawn_reader(
        stderr,
        limits.stderr_bytes,
        OverflowPolicy::Fail,
        OutputStream::Stderr,
        overflow_tx,
    );
    let started_at = Instant::now();

    let status = loop {
        if let Ok((stream, limit)) = overflow_rx.try_recv() {
            terminate(&mut child);
            let _ = join_reader(stdout_reader);
            let _ = join_reader(stderr_reader);
            return Err(GitError::OutputLimit {
                operation: operation.to_string(),
                stream,
                limit,
            });
        }
        let wait_result = match child.try_wait() {
            Ok(result) => result,
            Err(error) => {
                terminate(&mut child);
                let _ = join_reader(stdout_reader);
                let _ = join_reader(stderr_reader);
                return Err(GitError::Io {
                    operation: operation.to_string(),
                    message: error.to_string(),
                });
            }
        };
        match wait_result {
            Some(status) => break status,
            None if started_at.elapsed() >= limits.timeout => {
                terminate(&mut child);
                let _ = join_reader(stdout_reader);
                let _ = join_reader(stderr_reader);
                return Err(GitError::Timeout {
                    operation: operation.to_string(),
                    timeout_ms: limits.timeout.as_millis(),
                });
            }
            None => thread::sleep(PROCESS_POLL_INTERVAL),
        }
    };

    let stdout = join_reader(stdout_reader);
    let stderr = join_reader(stderr_reader);
    let stdout = stdout?;
    let stderr = stderr?;
    if stdout.truncated && limits.stdout_overflow == OverflowPolicy::Fail {
        return Err(GitError::OutputLimit {
            operation: operation.to_string(),
            stream: OutputStream::Stdout,
            limit: limits.stdout_bytes,
        });
    }
    if stderr.truncated {
        return Err(GitError::OutputLimit {
            operation: operation.to_string(),
            stream: OutputStream::Stderr,
            limit: limits.stderr_bytes,
        });
    }
    Ok(ExecutionOutput {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
        stdout_truncated: stdout.truncated,
    })
}

fn terminate(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[derive(Debug)]
struct ReaderOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

fn spawn_reader<R>(
    reader: R,
    limit: usize,
    policy: OverflowPolicy,
    stream: OutputStream,
    overflow_tx: Sender<(OutputStream, usize)>,
) -> thread::JoinHandle<io::Result<ReaderOutput>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || read_bounded(reader, limit, policy, stream, overflow_tx))
}

fn read_bounded<R: Read>(
    mut reader: R,
    limit: usize,
    policy: OverflowPolicy,
    stream: OutputStream,
    overflow_tx: Sender<(OutputStream, usize)>,
) -> io::Result<ReaderOutput> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0_u8; 16 * 1024];
    let mut truncated = false;
    let mut overflow_reported = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        let retained = count.min(remaining);
        bytes.extend_from_slice(&buffer[..retained]);
        if retained < count {
            truncated = true;
            if policy == OverflowPolicy::Fail && !overflow_reported {
                let _ = overflow_tx.send((stream, limit));
                overflow_reported = true;
            }
        }
    }
    Ok(ReaderOutput { bytes, truncated })
}

fn join_reader(reader: thread::JoinHandle<io::Result<ReaderOutput>>) -> Result<ReaderOutput> {
    reader
        .join()
        .map_err(|_| GitError::StateUnavailable("Git output reader panicked.".to_string()))?
        .map_err(|error| GitError::Io {
            operation: "read Git output".to_string(),
            message: error.to_string(),
        })
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn bounded_reader_truncates_without_allocating_past_limit() {
        let input = vec![b'x'; 128 * 1024];
        let (sender, _receiver) = mpsc::channel();
        let output = read_bounded(
            Cursor::new(input),
            1024,
            OverflowPolicy::Truncate,
            OutputStream::Stdout,
            sender,
        )
        .expect("reader should succeed");
        assert_eq!(output.bytes.len(), 1024);
        assert!(output.truncated);
    }

    #[test]
    fn bounded_reader_reports_fail_policy_overflow() {
        let (sender, receiver) = mpsc::channel();
        let output = read_bounded(
            Cursor::new(vec![b'x'; 32]),
            8,
            OverflowPolicy::Fail,
            OutputStream::Stderr,
            sender,
        )
        .expect("reader should succeed");
        assert!(output.truncated);
        assert_eq!(receiver.try_recv(), Ok((OutputStream::Stderr, 8)));
    }

    #[cfg(windows)]
    #[test]
    fn process_runner_enforces_timeout() {
        let result = execute(
            &CommandSpec {
                program: "ping.exe".to_string(),
                args: vec!["-n".to_string(), "10".to_string(), "127.0.0.1".to_string()],
                env: BTreeMap::new(),
            },
            "exercise timeout",
            CaptureLimits {
                timeout: Duration::from_millis(25),
                stdout_bytes: 64 * 1024,
                stderr_bytes: 64 * 1024,
                stdout_overflow: OverflowPolicy::Fail,
            },
        );
        assert!(matches!(result, Err(GitError::Timeout { .. })));
    }

    #[cfg(windows)]
    #[test]
    fn process_runner_enforces_output_limit() {
        let result = execute(
            &CommandSpec {
                program: "ping.exe".to_string(),
                args: vec!["-n".to_string(), "1".to_string(), "127.0.0.1".to_string()],
                env: BTreeMap::new(),
            },
            "exercise output limit",
            CaptureLimits {
                timeout: Duration::from_secs(2),
                stdout_bytes: 8,
                stderr_bytes: 64 * 1024,
                stdout_overflow: OverflowPolicy::Fail,
            },
        );
        assert!(matches!(
            result,
            Err(GitError::OutputLimit {
                stream: OutputStream::Stdout,
                limit: 8,
                ..
            })
        ));
    }
}
