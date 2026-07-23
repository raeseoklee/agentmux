use crate::{GitError, GitFileChange, Result, StatusSnapshot, StatusSummary, WorktreeInfo};

pub fn parse_porcelain_v2(
    output: &[u8],
    repository_root: impl Into<String>,
    generation: u64,
) -> Result<StatusSnapshot> {
    let mut records = output.split(|byte| *byte == 0);
    let mut branch = None;
    let mut head = None;
    let mut upstream = None;
    let mut ahead = 0;
    let mut behind = 0;
    let mut files = Vec::new();
    while let Some(record) = records.next() {
        let record = String::from_utf8_lossy(record);
        if record.is_empty() {
            continue;
        }
        if let Some(value) = record.strip_prefix("# branch.oid ") {
            if value != "(initial)" {
                head = Some(value.to_string());
            }
            continue;
        }
        if let Some(value) = record.strip_prefix("# branch.head ") {
            branch = Some(if value == "(detached)" {
                "detached".to_string()
            } else {
                value.to_string()
            });
            continue;
        }
        if let Some(value) = record.strip_prefix("# branch.upstream ") {
            upstream = Some(value.to_string());
            continue;
        }
        if let Some(value) = record.strip_prefix("# branch.ab ") {
            for token in value.split_whitespace() {
                if let Some(value) = token.strip_prefix('+') {
                    ahead = value.parse().unwrap_or(0);
                } else if let Some(value) = token.strip_prefix('-') {
                    behind = value.parse().unwrap_or(0);
                }
            }
            continue;
        }

        let (xy, path, original_path, conflict) = if record.starts_with("1 ") {
            let fields = record.splitn(9, ' ').collect::<Vec<_>>();
            if fields.len() != 9 {
                return Err(invalid_record("ordinary"));
            }
            (fields[1], fields[8].to_string(), None, false)
        } else if record.starts_with("2 ") {
            let fields = record.splitn(10, ' ').collect::<Vec<_>>();
            if fields.len() != 10 {
                return Err(invalid_record("rename"));
            }
            let original_path = records
                .next()
                .map(|value| String::from_utf8_lossy(value).to_string())
                .ok_or_else(|| invalid_record("rename source"))?;
            (fields[1], fields[9].to_string(), Some(original_path), false)
        } else if record.starts_with("u ") {
            let fields = record.splitn(11, ' ').collect::<Vec<_>>();
            if fields.len() != 11 {
                return Err(invalid_record("conflict"));
            }
            (fields[1], fields[10].to_string(), None, true)
        } else if let Some(path) = record.strip_prefix("? ") {
            ("??", path.to_string(), None, false)
        } else if record.starts_with("! ") {
            continue;
        } else {
            return Err(GitError::InvalidOutput(
                "Git returned an unknown porcelain-v2 status record.".to_string(),
            ));
        };

        let mut statuses = xy.chars();
        let index_status = statuses.next().unwrap_or('.');
        let worktree_status = statuses.next().unwrap_or('.');
        let untracked = index_status == '?' && worktree_status == '?';
        files.push(GitFileChange {
            path,
            original_path,
            index_status: index_status.to_string(),
            worktree_status: worktree_status.to_string(),
            staged: !matches!(index_status, '.' | '?'),
            unstaged: !matches!(worktree_status, '.' | '?') || untracked,
            untracked,
            conflict,
        });
    }

    let staged_count = files.iter().filter(|file| file.staged).count();
    let unstaged_count = files.iter().filter(|file| file.unstaged).count();
    let untracked_count = files.iter().filter(|file| file.untracked).count();
    let conflict_count = files.iter().filter(|file| file.conflict).count();
    let repository_root = repository_root.into();
    Ok(StatusSnapshot {
        summary: StatusSummary {
            repository_root,
            branch,
            head,
            upstream,
            ahead,
            behind,
            file_count: files.len(),
            staged_count,
            unstaged_count,
            untracked_count,
            conflict_count,
            generation,
        },
        files,
    })
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
