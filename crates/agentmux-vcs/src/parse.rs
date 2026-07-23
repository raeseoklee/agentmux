use crate::{GitError, GitFileChange, Result, StatusSnapshot, StatusSummary, WorktreeInfo};

pub const MAX_STATUS_ENTRIES: usize = 100_000;
const MAX_PORCELAIN_RECORD_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
struct PendingRename {
    xy: String,
    path: String,
}

/// Incrementally parses the NUL-delimited output of `git status --porcelain=v2 -z`.
///
/// The status reader feeds this parser as child-process bytes arrive, avoiding a second
/// full-size stdout allocation while retaining the complete bounded snapshot required by
/// paging and repository-change reconciliation.
#[derive(Debug)]
pub(crate) struct PorcelainV2StreamParser {
    pending: Vec<u8>,
    branch: Option<String>,
    head: Option<String>,
    upstream: Option<String>,
    ahead: u64,
    behind: u64,
    files: Vec<GitFileChange>,
    pending_rename: Option<PendingRename>,
    max_entries: usize,
}

impl PorcelainV2StreamParser {
    pub(crate) fn new(max_entries: usize) -> Self {
        Self {
            pending: Vec::new(),
            branch: None,
            head: None,
            upstream: None,
            ahead: 0,
            behind: 0,
            files: Vec::new(),
            pending_rename: None,
            max_entries,
        }
    }

    pub(crate) fn file_count(&self) -> usize {
        self.files.len()
    }

    pub(crate) fn first_changes(&self, limit: usize) -> Vec<GitFileChange> {
        self.files.iter().take(limit).cloned().collect()
    }

    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<()> {
        if chunk.is_empty() {
            return Ok(());
        }

        // Move the buffered tail out of `self` so records can be borrowed directly while
        // `consume_record` updates parser state. This avoids copying each status record.
        let mut input = std::mem::take(&mut self.pending);
        input.extend_from_slice(chunk);
        let mut start = 0;
        while let Some(relative_end) = input[start..].iter().position(|byte| *byte == 0) {
            let end = start + relative_end;
            self.consume_record(&input[start..end])?;
            start = end + 1;
        }
        self.pending.extend_from_slice(&input[start..]);
        if self.pending.len() > MAX_PORCELAIN_RECORD_BYTES {
            return Err(GitError::InvalidOutput(format!(
                "Git returned a porcelain-v2 record larger than {MAX_PORCELAIN_RECORD_BYTES} bytes."
            )));
        }
        Ok(())
    }

    pub(crate) fn finish(
        mut self,
        repository_root: impl Into<String>,
        generation: u64,
    ) -> Result<StatusSnapshot> {
        if !self.pending.is_empty() {
            return Err(GitError::InvalidOutput(
                "Git returned an unterminated porcelain-v2 status record.".to_string(),
            ));
        }
        if self.pending_rename.take().is_some() {
            return Err(invalid_record("rename source"));
        }

        let staged_count = self.files.iter().filter(|file| file.staged).count();
        let unstaged_count = self.files.iter().filter(|file| file.unstaged).count();
        let untracked_count = self.files.iter().filter(|file| file.untracked).count();
        let conflict_count = self.files.iter().filter(|file| file.conflict).count();
        Ok(StatusSnapshot {
            summary: StatusSummary {
                repository_root: repository_root.into(),
                branch: self.branch,
                head: self.head,
                upstream: self.upstream,
                ahead: self.ahead,
                behind: self.behind,
                file_count: self.files.len(),
                staged_count,
                unstaged_count,
                untracked_count,
                conflict_count,
                generation,
            },
            files: self.files,
        })
    }

    fn consume_record(&mut self, raw_record: &[u8]) -> Result<()> {
        if let Some(rename) = self.pending_rename.take() {
            if raw_record.is_empty() {
                return Err(invalid_record("rename source"));
            }
            return self.push_change(rename.xy, rename.path, Some(lossy(raw_record)), false);
        }

        if raw_record.is_empty() {
            return Ok(());
        }
        let record = String::from_utf8_lossy(raw_record);
        if let Some(value) = record.strip_prefix("# branch.oid ") {
            if value != "(initial)" {
                self.head = Some(value.to_string());
            }
            return Ok(());
        }
        if let Some(value) = record.strip_prefix("# branch.head ") {
            self.branch = Some(if value == "(detached)" {
                "detached".to_string()
            } else {
                value.to_string()
            });
            return Ok(());
        }
        if let Some(value) = record.strip_prefix("# branch.upstream ") {
            self.upstream = Some(value.to_string());
            return Ok(());
        }
        if let Some(value) = record.strip_prefix("# branch.ab ") {
            for token in value.split_whitespace() {
                if let Some(value) = token.strip_prefix('+') {
                    self.ahead = value.parse().unwrap_or(0);
                } else if let Some(value) = token.strip_prefix('-') {
                    self.behind = value.parse().unwrap_or(0);
                }
            }
            return Ok(());
        }

        if record.starts_with("1 ") {
            let fields = record.splitn(9, ' ').collect::<Vec<_>>();
            if fields.len() != 9 {
                return Err(invalid_record("ordinary"));
            }
            return self.push_change(fields[1].to_string(), fields[8].to_string(), None, false);
        }
        if record.starts_with("2 ") {
            let fields = record.splitn(10, ' ').collect::<Vec<_>>();
            if fields.len() != 10 {
                return Err(invalid_record("rename"));
            }
            self.pending_rename = Some(PendingRename {
                xy: fields[1].to_string(),
                path: fields[9].to_string(),
            });
            return Ok(());
        }
        if record.starts_with("u ") {
            let fields = record.splitn(11, ' ').collect::<Vec<_>>();
            if fields.len() != 11 {
                return Err(invalid_record("conflict"));
            }
            return self.push_change(fields[1].to_string(), fields[10].to_string(), None, true);
        }
        if let Some(path) = record.strip_prefix("? ") {
            return self.push_change("??".to_string(), path.to_string(), None, false);
        }
        if record.starts_with("! ") {
            return Ok(());
        }
        Err(GitError::InvalidOutput(
            "Git returned an unknown porcelain-v2 status record.".to_string(),
        ))
    }

    fn push_change(
        &mut self,
        xy: String,
        path: String,
        original_path: Option<String>,
        conflict: bool,
    ) -> Result<()> {
        if self.files.len() >= self.max_entries {
            return Err(GitError::StatusEntryLimit {
                limit: self.max_entries,
            });
        }
        let mut statuses = xy.chars();
        let index_status = statuses.next().unwrap_or('.');
        let worktree_status = statuses.next().unwrap_or('.');
        let untracked = index_status == '?' && worktree_status == '?';
        self.files.push(GitFileChange {
            path,
            original_path,
            index_status: index_status.to_string(),
            worktree_status: worktree_status.to_string(),
            staged: !matches!(index_status, '.' | '?'),
            unstaged: !matches!(worktree_status, '.' | '?') || untracked,
            untracked,
            conflict,
        });
        Ok(())
    }
}

fn lossy(value: &[u8]) -> String {
    String::from_utf8_lossy(value).to_string()
}

pub fn parse_porcelain_v2(
    output: &[u8],
    repository_root: impl Into<String>,
    generation: u64,
) -> Result<StatusSnapshot> {
    let mut parser = PorcelainV2StreamParser::new(MAX_STATUS_ENTRIES);
    parser.push(output)?;
    parser.finish(repository_root, generation)
}

fn invalid_record(kind: &str) -> GitError {
    GitError::InvalidOutput(format!(
        "Git returned an invalid porcelain-v2 {kind} record."
    ))
}

pub(crate) fn parse_worktree_porcelain(output: &[u8]) -> Result<Vec<WorktreeInfo>> {
    #[derive(Default)]
    struct PendingWorktree {
        path: Option<String>,
        head: Option<String>,
        branch: Option<String>,
        detached: bool,
        bare: bool,
        locked_reason: Option<String>,
        prunable_reason: Option<String>,
    }

    fn finish(pending: &mut PendingWorktree, worktrees: &mut Vec<WorktreeInfo>) {
        let Some(path) = pending.path.take() else {
            return;
        };
        worktrees.push(WorktreeInfo {
            path,
            head: pending.head.take(),
            branch: pending.branch.take(),
            detached: pending.detached,
            bare: pending.bare,
            locked_reason: pending.locked_reason.take(),
            prunable_reason: pending.prunable_reason.take(),
            main: worktrees.is_empty(),
        });
        *pending = PendingWorktree::default();
    }

    let mut worktrees = Vec::new();
    let mut pending = PendingWorktree::default();
    for raw_record in output.split(|byte| *byte == 0) {
        let record = String::from_utf8_lossy(raw_record);
        if record.is_empty() {
            finish(&mut pending, &mut worktrees);
        } else if let Some(path) = record.strip_prefix("worktree ") {
            if pending.path.is_some() {
                finish(&mut pending, &mut worktrees);
            }
            if path.is_empty() {
                return Err(GitError::InvalidOutput(
                    "Git returned an empty worktree path.".to_string(),
                ));
            }
            pending.path = Some(path.to_string());
        } else if let Some(head) = record.strip_prefix("HEAD ") {
            pending.head = Some(head.to_string());
        } else if let Some(branch) = record.strip_prefix("branch ") {
            pending.branch = Some(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_string(),
            );
        } else if record == "detached" {
            pending.detached = true;
        } else if record == "bare" {
            pending.bare = true;
        } else if record == "locked" {
            pending.locked_reason = Some(String::new());
        } else if let Some(reason) = record.strip_prefix("locked ") {
            pending.locked_reason = Some(reason.to_string());
        } else if record == "prunable" {
            pending.prunable_reason = Some(String::new());
        } else if let Some(reason) = record.strip_prefix("prunable ") {
            pending.prunable_reason = Some(reason.to_string());
        }
    }
    finish(&mut pending, &mut worktrees);
    Ok(worktrees)
}
