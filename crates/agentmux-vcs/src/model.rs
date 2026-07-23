use serde::{Deserialize, Serialize};

use crate::{GitError, Result};

pub const MAX_STATUS_PAGE_SIZE: usize = 1_000;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum GitHost {
    Native,
    Wsl { distribution: Option<String> },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitContext {
    pub host: GitHost,
    pub cwd: String,
}

impl GitContext {
    pub fn native(cwd: impl Into<String>) -> Self {
        Self {
            host: GitHost::Native,
            cwd: cwd.into(),
        }
    }

    pub fn wsl(cwd: impl Into<String>, distribution: Option<String>) -> Self {
        Self {
            host: GitHost::Wsl { distribution },
            cwd: cwd.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Repository {
    pub(crate) host: GitHost,
    pub(crate) root: String,
}

impl Repository {
    pub fn host(&self) -> &GitHost {
        &self.host
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn context(&self) -> GitContext {
        GitContext {
            host: self.host.clone(),
            cwd: self.root.clone(),
        }
    }

    pub(crate) fn key(&self) -> String {
        match &self.host {
            GitHost::Native => format!("native:{}", normalize_native_key(&self.root)),
            GitHost::Wsl { distribution } => format!(
                "wsl:{}:{}",
                distribution
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                self.root.trim_end_matches('/')
            ),
        }
    }
}

fn normalize_native_key(value: &str) -> String {
    value
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitFileChange {
    pub path: String,
    pub original_path: Option<String>,
    pub index_status: String,
    pub worktree_status: String,
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
    pub conflict: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StatusSummary {
    pub repository_root: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u64,
    pub behind: u64,
    pub file_count: usize,
    pub staged_count: usize,
    pub unstaged_count: usize,
    pub untracked_count: usize,
    pub conflict_count: usize,
    pub generation: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StatusSnapshot {
    pub summary: StatusSummary,
    pub files: Vec<GitFileChange>,
}

impl StatusSnapshot {
    pub fn page(&self, offset: usize, limit: usize) -> Result<StatusPage> {
        if limit == 0 || limit > MAX_STATUS_PAGE_SIZE {
            return Err(GitError::InvalidStatusPage(format!(
                "Git status page size must be between 1 and {MAX_STATUS_PAGE_SIZE}."
            )));
        }
        if offset > self.files.len() {
            return Err(GitError::InvalidStatusPage(format!(
                "Git status page offset {offset} exceeds {} files.",
                self.files.len()
            )));
        }

        let end = offset.saturating_add(limit).min(self.files.len());
        Ok(StatusPage {
            summary: self.summary.clone(),
            offset,
            next_offset: (end < self.files.len()).then_some(end),
            files: self.files[offset..end].to_vec(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StatusPage {
    pub summary: StatusSummary,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub files: Vec<GitFileChange>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiffRequest {
    pub path: String,
    #[serde(default)]
    pub staged: bool,
    #[serde(default)]
    pub untracked: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiffResult {
    pub path: String,
    pub staged: bool,
    pub patch: String,
    pub truncated: bool,
    pub generation: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommitResult {
    pub commit: String,
    pub summary: String,
    pub generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedWorktreeDestination {
    pub(crate) host: GitHost,
    pub(crate) allowed_root: String,
    pub(crate) path: String,
}

impl VerifiedWorktreeDestination {
    pub fn host(&self) -> &GitHost {
        &self.host
    }

    pub fn allowed_root(&self) -> &str {
        &self.allowed_root
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorktreeInfo {
    pub(crate) path: String,
    pub(crate) head: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) detached: bool,
    pub(crate) bare: bool,
    pub(crate) locked_reason: Option<String>,
    pub(crate) prunable_reason: Option<String>,
    pub(crate) main: bool,
}

impl WorktreeInfo {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn head(&self) -> Option<&str> {
        self.head.as_deref()
    }

    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    pub fn is_detached(&self) -> bool {
        self.detached
    }

    pub fn is_bare(&self) -> bool {
        self.bare
    }

    pub fn locked_reason(&self) -> Option<&str> {
        self.locked_reason.as_deref()
    }

    pub fn prunable_reason(&self) -> Option<&str> {
        self.prunable_reason.as_deref()
    }

    pub fn is_main(&self) -> bool {
        self.main
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CreateWorktreeResult {
    pub worktree: WorktreeInfo,
    pub generation: u64,
}
