use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitError {
    Io {
        operation: String,
        message: String,
    },
    Timeout {
        operation: String,
        timeout_ms: u128,
    },
    OutputLimit {
        operation: String,
        stream: OutputStream,
        limit: usize,
    },
    CommandFailed {
        operation: String,
        exit_code: Option<i32>,
        detail: String,
    },
    NotRepository,
    InvalidBranch(String),
    InvalidRevision(String),
    InvalidPath(String),
    InvalidCommitMessage(String),
    InvalidWorktreeDestination(String),
    InvalidWorktreeRequest(String),
    InvalidStatusPage(String),
    StatusEntryLimit {
        limit: usize,
    },
    InvalidOutput(String),
    StateUnavailable(String),
}

impl fmt::Display for GitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, message } => write!(formatter, "{operation}: {message}"),
            Self::Timeout {
                operation,
                timeout_ms,
            } => write!(formatter, "{operation} timed out after {timeout_ms} ms"),
            Self::OutputLimit {
                operation,
                stream,
                limit,
            } => write!(
                formatter,
                "{operation} exceeded the {stream:?} capture limit of {limit} bytes"
            ),
            Self::CommandFailed {
                operation,
                exit_code,
                detail,
            } => {
                if detail.is_empty() {
                    write!(formatter, "Git could not {operation} (exit {exit_code:?})")
                } else {
                    write!(
                        formatter,
                        "Git could not {operation} (exit {exit_code:?}): {detail}"
                    )
                }
            }
            Self::NotRepository => write!(formatter, "the directory is not a Git repository"),
            Self::InvalidBranch(value) => write!(formatter, "invalid Git branch: {value}"),
            Self::InvalidRevision(value) => write!(formatter, "invalid Git revision: {value}"),
            Self::InvalidPath(value) => write!(
                formatter,
                "Git path must stay inside the repository: {value}"
            ),
            Self::InvalidCommitMessage(message)
            | Self::InvalidWorktreeDestination(message)
            | Self::InvalidWorktreeRequest(message)
            | Self::InvalidStatusPage(message)
            | Self::InvalidOutput(message)
            | Self::StateUnavailable(message) => formatter.write_str(message),
            Self::StatusEntryLimit { limit } => write!(
                formatter,
                "Git status contains more than the configured {limit} change entries"
            ),
        }
    }
}

impl std::error::Error for GitError {}

pub type Result<T> = std::result::Result<T, GitError>;
