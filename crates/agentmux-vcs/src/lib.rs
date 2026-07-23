//! Windows-first Git operations shared by AgentMux desktop, server, CLI, and MCP hosts.
//!
//! All Git invocations use argument arrays, literal pathspecs, bounded output, and a fixed
//! deadline. Repository handles can only be obtained through resolution, keeping mutation APIs
//! anchored to a verified Git worktree.
#![forbid(unsafe_code)]

mod command;
mod error;
mod model;
mod parse;

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard, Weak};
use std::thread;
use std::time::{Duration, Instant};

pub use command::{build_git_command_spec, CommandSpec};
use command::{
    build_wsl_utility_spec, execute, execute_streaming_stdout, CaptureLimits, ExecutionOutput,
    OverflowPolicy,
};
pub use error::{GitError, OutputStream, Result};
pub use model::{
    CommitResult, CreateWorktreeResult, DiffRequest, DiffResult, GitContext, GitFileChange,
    GitHost, Repository, StatusPage, StatusSnapshot, StatusSummary, VerifiedWorktreeDestination,
    WorktreeInfo, MAX_STATUS_PAGE_SIZE,
};
pub use parse::{parse_porcelain_v2, MAX_STATUS_ENTRIES};
use parse::{parse_worktree_porcelain, PorcelainV2StreamParser};

const MAX_COMMIT_MESSAGE_BYTES: usize = 64 * 1024;
const MAX_BRANCH_BYTES: usize = 1024;
const MAX_REVISION_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug)]
pub struct GitConfig {
    pub command_timeout: Duration,
    pub resolve_output_bytes: usize,
    pub status_output_bytes: usize,
    pub status_entry_limit: usize,
    pub diff_output_bytes: usize,
    pub worktree_output_bytes: usize,
    pub mutation_output_bytes: usize,
    pub stderr_output_bytes: usize,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            command_timeout: Duration::from_secs(15),
            resolve_output_bytes: 64 * 1024,
            status_output_bytes: 16 * 1024 * 1024,
            status_entry_limit: MAX_STATUS_ENTRIES,
            diff_output_bytes: 2 * 1024 * 1024,
            worktree_output_bytes: 2 * 1024 * 1024,
            mutation_output_bytes: 2 * 1024 * 1024,
            stderr_output_bytes: 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StatusReadMetrics {
    /// Total porcelain bytes consumed from the child stdout pipe.
    pub stdout_bytes: usize,
    /// Time from process launch until the first file record is available to paging.
    pub first_change_after: Option<Duration>,
    /// Time from process launch until the complete snapshot is ready for the host cache.
    pub completed_after: Duration,
}

#[derive(Clone, Debug)]
pub struct StatusReadResult {
    pub snapshot: StatusSnapshot,
    pub metrics: StatusReadMetrics,
}

#[derive(Clone, Debug)]
pub struct StatusFirstPage {
    pub generation: u64,
    pub changes: Vec<GitFileChange>,
    pub first_change_after: Option<Duration>,
}

#[derive(Clone, Debug)]
pub enum StatusScanFirstPage {
    Prefix(StatusFirstPage),
    Complete(Arc<StatusReadResult>),
}

#[derive(Clone)]
pub struct StatusScan {
    generation: u64,
    shared: Arc<StatusScanShared>,
}

struct StatusScanShared {
    state: Mutex<StatusScanState>,
    changed: Condvar,
    cancelled: AtomicBool,
    prefix_published: AtomicBool,
}

enum StatusScanState {
    Running { prefix: Option<StatusFirstPage> },
    Complete(Arc<StatusReadResult>),
    Failed(GitError),
    Cancelled,
}

impl StatusScan {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn wait_for_first_page(&self) -> Result<StatusScanFirstPage> {
        let mut state = recover_lock(&self.shared.state);
        loop {
            match &*state {
                StatusScanState::Running {
                    prefix: Some(prefix),
                } => return Ok(StatusScanFirstPage::Prefix(prefix.clone())),
                StatusScanState::Running { prefix: None } => {
                    state = self
                        .shared
                        .changed
                        .wait(state)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
                StatusScanState::Complete(result) => {
                    return Ok(StatusScanFirstPage::Complete(result.clone()))
                }
                StatusScanState::Failed(error) => return Err(error.clone()),
                StatusScanState::Cancelled => {
                    return Err(GitError::StateUnavailable(
                        "Git status scan was cancelled.".to_string(),
                    ))
                }
            }
        }
    }

    pub fn wait_for_completion(&self) -> Result<Arc<StatusReadResult>> {
        let mut state = recover_lock(&self.shared.state);
        loop {
            match &*state {
                StatusScanState::Running { .. } => {
                    state = self
                        .shared
                        .changed
                        .wait(state)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
                StatusScanState::Complete(result) => return Ok(result.clone()),
                StatusScanState::Failed(error) => return Err(error.clone()),
                StatusScanState::Cancelled => {
                    return Err(GitError::StateUnavailable(
                        "Git status scan was cancelled.".to_string(),
                    ))
                }
            }
        }
    }

    pub fn try_completion(&self) -> Option<Result<Arc<StatusReadResult>>> {
        let state = recover_lock(&self.shared.state);
        match &*state {
            StatusScanState::Running { .. } => None,
            StatusScanState::Complete(result) => Some(Ok(result.clone())),
            StatusScanState::Failed(error) => Some(Err(error.clone())),
            StatusScanState::Cancelled => Some(Err(GitError::StateUnavailable(
                "Git status scan was cancelled.".to_string(),
            ))),
        }
    }

    pub fn cancel(&self) {
        self.shared.cancelled.store(true, Ordering::Release);
        let mut state = recover_lock(&self.shared.state);
        if matches!(*state, StatusScanState::Running { .. }) {
            *state = StatusScanState::Cancelled;
        }
        self.shared.changed.notify_all();
    }

    fn publish_prefix(&self, prefix: StatusFirstPage) {
        let mut state = recover_lock(&self.shared.state);
        if let StatusScanState::Running {
            prefix: current @ None,
        } = &mut *state
        {
            *current = Some(prefix);
            self.shared.changed.notify_all();
        }
    }

    fn finish(&self, result: Result<StatusReadResult>) {
        let mut state = recover_lock(&self.shared.state);
        if self.shared.cancelled.load(Ordering::Acquire) {
            *state = StatusScanState::Cancelled;
        } else {
            *state = match result {
                Ok(result) => StatusScanState::Complete(Arc::new(result)),
                Err(error) => StatusScanState::Failed(error),
            };
        }
        self.shared.changed.notify_all();
    }
}

#[derive(Default)]
struct RepositoryState {
    repository_locks: Mutex<HashMap<String, Weak<RwLock<()>>>>,
    generations: Mutex<HashMap<String, u64>>,
}

#[derive(Clone)]
pub struct GitClient {
    config: GitConfig,
    state: Arc<RepositoryState>,
}

impl Default for GitClient {
    fn default() -> Self {
        Self::new(GitConfig::default())
    }
}

impl GitClient {
    pub fn new(config: GitConfig) -> Self {
        Self {
            config,
            state: Arc::new(RepositoryState::default()),
        }
    }

    pub fn resolve_repository(&self, context: &GitContext) -> Result<Option<Repository>> {
        let context = normalize_context(context)?;
        let output = self.run(
            &context,
            &["rev-parse".to_string(), "--show-toplevel".to_string()],
            true,
            "resolve the repository",
            self.config.resolve_output_bytes,
            OverflowPolicy::Fail,
        )?;
        if !output.status.success() {
            if is_not_repository(&output) {
                return Ok(None);
            }
            return Err(command_failed(output, "resolve the repository"));
        }

        let root = output_text(&output.stdout).trim().to_string();
        if root.is_empty() {
            return Err(GitError::InvalidOutput(
                "Git returned an empty repository root.".to_string(),
            ));
        }
        Ok(Some(Repository {
            host: context.host,
            root,
        }))
    }

    pub fn require_repository(&self, context: &GitContext) -> Result<Repository> {
        self.resolve_repository(context)?
            .ok_or(GitError::NotRepository)
    }

    pub fn read_status(&self, repository: &Repository) -> Result<StatusSnapshot> {
        Ok(self.read_status_with_metrics(repository)?.snapshot)
    }

    pub fn read_status_with_metrics(&self, repository: &Repository) -> Result<StatusReadResult> {
        let generation = self.generation(repository);
        self.read_status_with_progress(repository, generation, |_, _| Ok(()))
    }

    pub fn start_status_scan(
        &self,
        repository: &Repository,
        first_page_limit: usize,
    ) -> Result<StatusScan> {
        if first_page_limit == 0 || first_page_limit > MAX_STATUS_PAGE_SIZE {
            return Err(GitError::InvalidStatusPage(format!(
                "Git status first page size must be between 1 and {MAX_STATUS_PAGE_SIZE}."
            )));
        }
        let generation = self.generation(repository);
        let scan = StatusScan {
            generation,
            shared: Arc::new(StatusScanShared {
                state: Mutex::new(StatusScanState::Running { prefix: None }),
                changed: Condvar::new(),
                cancelled: AtomicBool::new(false),
                prefix_published: AtomicBool::new(false),
            }),
        };
        let worker_scan = scan.clone();
        let client = self.clone();
        let repository = repository.clone();
        thread::Builder::new()
            .name("agentmux-git-status".to_string())
            .spawn(move || {
                let result =
                    client.read_status_with_progress(&repository, generation, |parser, elapsed| {
                        if worker_scan.shared.cancelled.load(Ordering::Acquire) {
                            return Err(GitError::StateUnavailable(
                                "Git status scan was cancelled.".to_string(),
                            ));
                        }
                        if parser.file_count() > 0
                            && !worker_scan
                                .shared
                                .prefix_published
                                .swap(true, Ordering::AcqRel)
                        {
                            worker_scan.publish_prefix(StatusFirstPage {
                                generation,
                                changes: parser.first_changes(first_page_limit),
                                first_change_after: Some(elapsed),
                            });
                        }
                        Ok(())
                    });
                worker_scan.finish(result);
            })
            .map_err(|error| GitError::Io {
                operation: "start repository status scan".to_string(),
                message: error.to_string(),
            })?;
        Ok(scan)
    }

    fn read_status_with_progress<F>(
        &self,
        repository: &Repository,
        generation: u64,
        mut on_progress: F,
    ) -> Result<StatusReadResult>
    where
        F: FnMut(&PorcelainV2StreamParser, Duration) -> Result<()>,
    {
        let repository_lock = self.repository_lock(&repository.key());
        let _guard = recover_read(&repository_lock);
        let arguments = [
            "-c".to_string(),
            "core.quotePath=false".to_string(),
            "status".to_string(),
            "--porcelain=v2".to_string(),
            "--branch".to_string(),
            "-z".to_string(),
            "--untracked-files=all".to_string(),
        ];
        let spec = build_git_command_spec(&repository.context(), &arguments, true);
        let mut parser = PorcelainV2StreamParser::new(self.config.status_entry_limit);
        let started_at = Instant::now();
        let mut first_change_after = None;
        let output = execute_streaming_stdout(
            &spec,
            "read repository status",
            CaptureLimits {
                timeout: self.config.command_timeout,
                stdout_bytes: self.config.status_output_bytes,
                stderr_bytes: self.config.stderr_output_bytes,
                stdout_overflow: OverflowPolicy::Fail,
            },
            |chunk| {
                parser.push(chunk)?;
                if first_change_after.is_none() && parser.file_count() > 0 {
                    first_change_after = Some(started_at.elapsed());
                }
                on_progress(&parser, started_at.elapsed())?;
                Ok(())
            },
        )?;
        let completed_after = started_at.elapsed();
        let command_output = ExecutionOutput {
            status: output.status,
            stdout: Vec::new(),
            stderr: output.stderr,
            stdout_truncated: false,
        };
        ensure_success(
            command_output.status.success(),
            &command_output,
            "read repository status",
        )?;
        let snapshot = parser.finish(repository.root.clone(), generation)?;
        Ok(StatusReadResult {
            snapshot,
            metrics: StatusReadMetrics {
                stdout_bytes: output.stdout_bytes,
                first_change_after: first_change_after.or(output.first_stdout_after),
                completed_after,
            },
        })
    }

    pub fn status_summary(&self, repository: &Repository) -> Result<StatusSummary> {
        Ok(self.read_status(repository)?.summary)
    }

    pub fn status_page(
        &self,
        repository: &Repository,
        offset: usize,
        limit: usize,
    ) -> Result<StatusPage> {
        self.read_status(repository)?.page(offset, limit)
    }

    pub fn diff(&self, repository: &Repository, request: &DiffRequest) -> Result<DiffResult> {
        validate_relative_path(&request.path)?;
        if request.staged && request.untracked {
            return Err(GitError::InvalidPath(
                "an untracked diff cannot also be staged".to_string(),
            ));
        }
        let repository_lock = self.repository_lock(&repository.key());
        let _guard = recover_read(&repository_lock);
        let arguments = if request.untracked {
            vec![
                "-c".to_string(),
                "core.quotePath=false".to_string(),
                "diff".to_string(),
                "--no-index".to_string(),
                "--no-ext-diff".to_string(),
                "--no-color".to_string(),
                "--".to_string(),
                "/dev/null".to_string(),
                request.path.clone(),
            ]
        } else {
            let mut arguments = vec![
                "-c".to_string(),
                "core.quotePath=false".to_string(),
                "diff".to_string(),
                "--no-ext-diff".to_string(),
                "--no-color".to_string(),
            ];
            if request.staged {
                arguments.push("--cached".to_string());
            }
            arguments.extend(["--".to_string(), request.path.clone()]);
            arguments
        };
        let output = self.run(
            &repository.context(),
            &arguments,
            true,
            "read the file diff",
            self.config.diff_output_bytes,
            OverflowPolicy::Truncate,
        )?;
        let accepted_difference = request.untracked && output.status.code() == Some(1);
        if !(output.status.success() || accepted_difference) {
            return Err(command_failed(output, "read the file diff"));
        }
        Ok(DiffResult {
            path: request.path.clone(),
            staged: request.staged,
            patch: output_text(&output.stdout),
            truncated: output.stdout_truncated,
            generation: self.generation(repository),
        })
    }

    pub fn stage(&self, repository: &Repository, paths: &[String]) -> Result<u64> {
        validate_relative_paths(paths)?;
        self.with_mutation(repository, || {
            let mut arguments = vec!["add".to_string(), "-A".to_string(), "--".to_string()];
            if paths.is_empty() {
                arguments.push(".".to_string());
            } else {
                arguments.extend(paths.iter().cloned());
            }
            let output = self.run(
                &repository.context(),
                &arguments,
                false,
                "stage changes",
                self.config.mutation_output_bytes,
                OverflowPolicy::Fail,
            )?;
            ensure_success(output.status.success(), &output, "stage changes")
        })
        .map(|(_, generation)| generation)
    }

    pub fn unstage(&self, repository: &Repository, paths: &[String]) -> Result<u64> {
        validate_relative_paths(paths)?;
        self.with_mutation(repository, || {
            let head = self.run(
                &repository.context(),
                &[
                    "rev-parse".to_string(),
                    "--verify".to_string(),
                    "--end-of-options".to_string(),
                    "HEAD".to_string(),
                ],
                true,
                "inspect repository HEAD",
                self.config.resolve_output_bytes,
                OverflowPolicy::Fail,
            )?;
            let mut arguments = if head.status.success() {
                vec![
                    "reset".to_string(),
                    "-q".to_string(),
                    "HEAD".to_string(),
                    "--".to_string(),
                ]
            } else {
                vec![
                    "rm".to_string(),
                    "-r".to_string(),
                    "--cached".to_string(),
                    "--ignore-unmatch".to_string(),
                    "--".to_string(),
                ]
            };
            if paths.is_empty() {
                arguments.push(".".to_string());
            } else {
                arguments.extend(paths.iter().cloned());
            }
            let output = self.run(
                &repository.context(),
                &arguments,
                false,
                "unstage changes",
                self.config.mutation_output_bytes,
                OverflowPolicy::Fail,
            )?;
            ensure_success(output.status.success(), &output, "unstage changes")
        })
        .map(|(_, generation)| generation)
    }

    pub fn commit(&self, repository: &Repository, message: &str) -> Result<CommitResult> {
        validate_commit_message(message)?;
        let ((commit, summary), generation) = self.with_mutation(repository, || {
            let output = self.run(
                &repository.context(),
                &["commit".to_string(), "-m".to_string(), message.to_string()],
                false,
                "create the commit",
                self.config.mutation_output_bytes,
                OverflowPolicy::Fail,
            )?;
            ensure_success(output.status.success(), &output, "create the commit")?;
            let commit = self.run(
                &repository.context(),
                &[
                    "rev-parse".to_string(),
                    "--short".to_string(),
                    "--verify".to_string(),
                    "--end-of-options".to_string(),
                    "HEAD".to_string(),
                ],
                true,
                "read the new commit",
                self.config.resolve_output_bytes,
                OverflowPolicy::Fail,
            )?;
            ensure_success(commit.status.success(), &commit, "read the new commit")?;
            Ok((
                output_text(&commit.stdout).trim().to_string(),
                output_text(&output.stdout).trim().to_string(),
            ))
        })?;
        Ok(CommitResult {
            commit,
            summary,
            generation,
        })
    }

    pub fn validate_branch_name(&self, context: &GitContext, branch: &str) -> Result<()> {
        let branch = branch.trim();
        if branch.is_empty() || branch.len() > MAX_BRANCH_BYTES || branch.contains('\0') {
            return Err(GitError::InvalidBranch(branch.to_string()));
        }
        let output = self.run(
            &normalize_context(context)?,
            &[
                "check-ref-format".to_string(),
                "--branch".to_string(),
                branch.to_string(),
            ],
            true,
            "validate the branch name",
            self.config.resolve_output_bytes,
            OverflowPolicy::Fail,
        )?;
        if output.status.success() {
            Ok(())
        } else {
            Err(GitError::InvalidBranch(branch.to_string()))
        }
    }

    pub fn local_branch_head(
        &self,
        repository: &Repository,
        branch: &str,
    ) -> Result<Option<String>> {
        self.validate_branch_name(&repository.context(), branch)?;
        let repository_lock = self.repository_lock(&repository.key());
        let _guard = recover_read(&repository_lock);
        let reference = format!("refs/heads/{}", branch.trim());
        let exists = self.run(
            &repository.context(),
            &[
                "show-ref".to_string(),
                "--verify".to_string(),
                "--quiet".to_string(),
                reference.clone(),
            ],
            true,
            "verify the local branch",
            self.config.resolve_output_bytes,
            OverflowPolicy::Fail,
        )?;
        if exists.status.code() == Some(1) {
            return Ok(None);
        }
        ensure_success(exists.status.success(), &exists, "verify the local branch")?;
        let output = self.run(
            &repository.context(),
            &[
                "rev-parse".to_string(),
                "--verify".to_string(),
                "--quiet".to_string(),
                "--end-of-options".to_string(),
                format!("{reference}^{{commit}}"),
            ],
            true,
            "read the local branch",
            self.config.resolve_output_bytes,
            OverflowPolicy::Fail,
        )?;
        ensure_success(output.status.success(), &output, "read the local branch")?;
        let head = output_text(&output.stdout).trim().to_string();
        Ok((!head.is_empty()).then_some(head))
    }

    pub fn validate_revision(&self, repository: &Repository, revision: &str) -> Result<String> {
        let revision = revision.trim();
        if revision.is_empty() || revision.len() > MAX_REVISION_BYTES || revision.contains('\0') {
            return Err(GitError::InvalidRevision(revision.to_string()));
        }
        let repository_lock = self.repository_lock(&repository.key());
        let _guard = recover_read(&repository_lock);
        let qualified = format!("{revision}^{{commit}}");
        let output = self.run(
            &repository.context(),
            &[
                "rev-parse".to_string(),
                "--verify".to_string(),
                "--end-of-options".to_string(),
                qualified,
            ],
            true,
            "validate the revision",
            self.config.resolve_output_bytes,
            OverflowPolicy::Fail,
        )?;
        if !output.status.success() {
            return Err(GitError::InvalidRevision(revision.to_string()));
        }
        let resolved = output_text(&output.stdout).trim().to_string();
        if resolved.is_empty() {
            Err(GitError::InvalidRevision(revision.to_string()))
        } else {
            Ok(resolved)
        }
    }

    pub fn verify_worktree_destination(
        &self,
        repository: &Repository,
        allowed_root: &str,
        destination: &str,
    ) -> Result<VerifiedWorktreeDestination> {
        match repository.host() {
            GitHost::Native => verify_native_worktree_destination(allowed_root, destination),
            GitHost::Wsl { .. } => {
                self.verify_wsl_worktree_destination(repository, allowed_root, destination)
            }
        }
    }

    pub fn create_worktree(
        &self,
        repository: &Repository,
        destination: &VerifiedWorktreeDestination,
        branch: &str,
        base_revision: Option<&str>,
        create_branch: bool,
    ) -> Result<CreateWorktreeResult> {
        if repository.host() != destination.host() {
            return Err(GitError::InvalidWorktreeDestination(
                "The worktree destination belongs to a different Git host.".to_string(),
            ));
        }
        self.validate_branch_name(&repository.context(), branch)?;
        let branch = branch.trim().to_string();
        let (head, target) = if create_branch {
            let base_revision = base_revision.unwrap_or("HEAD");
            let head = self.validate_revision(repository, base_revision)?;
            (head.clone(), head)
        } else {
            if base_revision.is_some() {
                return Err(GitError::InvalidWorktreeRequest(
                    "base_revision is only valid when create_branch is true.".to_string(),
                ));
            }
            let head = self.validate_revision(repository, &format!("refs/heads/{branch}"))?;
            (head, branch.clone())
        };
        let arguments =
            create_worktree_arguments(destination.path(), &branch, &target, create_branch);
        let ((), generation) = self.with_mutation(repository, || {
            let current = self.verify_worktree_destination(
                repository,
                destination.allowed_root(),
                destination.path(),
            )?;
            if current.path() != destination.path() {
                return Err(GitError::InvalidWorktreeDestination(
                    "The worktree destination changed after verification.".to_string(),
                ));
            }
            let output = self.run(
                &repository.context(),
                &arguments,
                false,
                "create the worktree",
                self.config.mutation_output_bytes,
                OverflowPolicy::Fail,
            )?;
            ensure_success(output.status.success(), &output, "create the worktree")
        })?;
        Ok(CreateWorktreeResult {
            worktree: WorktreeInfo {
                path: destination.path().to_string(),
                head: Some(head),
                branch: Some(branch),
                detached: false,
                bare: false,
                locked_reason: None,
                prunable_reason: None,
                main: false,
            },
            generation,
        })
    }

    pub fn list_worktrees(&self, repository: &Repository) -> Result<Vec<WorktreeInfo>> {
        let repository_lock = self.repository_lock(&repository.key());
        let _guard = recover_read(&repository_lock);
        self.list_worktrees_unlocked(repository)
    }

    pub fn remove_worktree(
        &self,
        repository: &Repository,
        destination: &str,
        force: bool,
    ) -> Result<u64> {
        validate_worktree_lookup_path(repository.host(), destination)?;
        self.with_mutation(repository, || {
            let worktrees = self.list_worktrees_unlocked(repository)?;
            let worktree = worktrees
                .iter()
                .find(|worktree| {
                    worktree_paths_equal(repository.host(), worktree.path(), destination)
                })
                .ok_or_else(|| {
                    GitError::InvalidWorktreeDestination(
                        "The destination is not a registered worktree for this repository."
                            .to_string(),
                    )
                })?;
            if worktree.is_main() {
                return Err(GitError::InvalidWorktreeRequest(
                    "The primary worktree cannot be removed.".to_string(),
                ));
            }
            let arguments = remove_worktree_arguments(worktree.path(), force);
            let output = self.run(
                &repository.context(),
                &arguments,
                false,
                "remove the worktree",
                self.config.mutation_output_bytes,
                OverflowPolicy::Fail,
            )?;
            ensure_success(output.status.success(), &output, "remove the worktree")
        })
        .map(|(_, generation)| generation)
    }

    pub fn generation(&self, repository: &Repository) -> u64 {
        let generations = recover_lock(&self.state.generations);
        generations.get(&repository.key()).copied().unwrap_or(0)
    }

    pub fn mark_repository_changed(&self, repository: &Repository) -> u64 {
        let repository_lock = self.repository_lock(&repository.key());
        let _guard = recover_write(&repository_lock);
        self.bump_generation(repository)
    }

    fn with_mutation<T>(
        &self,
        repository: &Repository,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<(T, u64)> {
        let repository_lock = self.repository_lock(&repository.key());
        let _guard = recover_write(&repository_lock);
        let result = operation()?;
        Ok((result, self.bump_generation(repository)))
    }

    fn bump_generation(&self, repository: &Repository) -> u64 {
        let mut generations = recover_lock(&self.state.generations);
        let generation = generations.entry(repository.key()).or_default();
        *generation = generation.saturating_add(1);
        *generation
    }

    fn repository_lock(&self, key: &str) -> Arc<RwLock<()>> {
        let mut locks = recover_lock(&self.state.repository_locks);
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(key).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(RwLock::new(()));
        locks.insert(key.to_string(), Arc::downgrade(&lock));
        lock
    }

    fn run(
        &self,
        context: &GitContext,
        arguments: &[String],
        read_only: bool,
        operation: &str,
        stdout_bytes: usize,
        stdout_overflow: OverflowPolicy,
    ) -> Result<ExecutionOutput> {
        let spec = build_git_command_spec(context, arguments, read_only);
        execute(
            &spec,
            operation,
            CaptureLimits {
                timeout: self.config.command_timeout,
                stdout_bytes,
                stderr_bytes: self.config.stderr_output_bytes,
                stdout_overflow,
            },
        )
    }

    fn list_worktrees_unlocked(&self, repository: &Repository) -> Result<Vec<WorktreeInfo>> {
        let output = self.run(
            &repository.context(),
            &[
                "worktree".to_string(),
                "list".to_string(),
                "--porcelain".to_string(),
                "-z".to_string(),
            ],
            true,
            "list worktrees",
            self.config.worktree_output_bytes,
            OverflowPolicy::Fail,
        )?;
        ensure_success(output.status.success(), &output, "list worktrees")?;
        parse_worktree_porcelain(&output.stdout)
    }

    fn verify_wsl_worktree_destination(
        &self,
        repository: &Repository,
        allowed_root: &str,
        destination: &str,
    ) -> Result<VerifiedWorktreeDestination> {
        let allowed_root = normalize_posix_absolute(allowed_root, "allowed root")?;
        if allowed_root == "/" {
            return Err(GitError::InvalidWorktreeDestination(
                "The WSL filesystem root cannot be used as a worktree root.".to_string(),
            ));
        }
        let destination = if destination.starts_with('/') {
            normalize_posix_absolute(destination, "destination")?
        } else {
            normalize_posix_absolute(
                &format!("{}/{}", allowed_root.trim_end_matches('/'), destination),
                "destination",
            )?
        };
        let root_check = self.run_wsl_utility(
            repository.host(),
            "test",
            &["-d".to_string(), allowed_root.clone()],
            "verify the WSL worktree root",
        )?;
        if !root_check.status.success() {
            return Err(GitError::InvalidWorktreeDestination(
                "The WSL worktree root must be an existing directory.".to_string(),
            ));
        }
        let canonical_root = self.wsl_canonicalize(repository.host(), &allowed_root)?;
        let destination_parent = posix_parent(&destination)?;
        let parent_check = self.run_wsl_utility(
            repository.host(),
            "test",
            &["-d".to_string(), destination_parent.to_string()],
            "verify the WSL worktree parent",
        )?;
        if !parent_check.status.success() {
            return Err(GitError::InvalidWorktreeDestination(
                "The WSL worktree destination parent must already exist.".to_string(),
            ));
        }
        let canonical_parent = self.wsl_canonicalize(repository.host(), destination_parent)?;
        if canonical_parent != canonical_root
            && !is_posix_descendant(&canonical_root, &canonical_parent)
        {
            return Err(GitError::InvalidWorktreeDestination(
                "The worktree destination parent escapes the allowed WSL root.".to_string(),
            ));
        }
        for predicate in ["-e", "-L"] {
            let destination_check = self.run_wsl_utility(
                repository.host(),
                "test",
                &[predicate.to_string(), destination.clone()],
                "verify the WSL worktree destination",
            )?;
            if destination_check.status.success() {
                return Err(GitError::InvalidWorktreeDestination(
                    "The worktree destination must not already exist.".to_string(),
                ));
            }
            if destination_check.status.code() != Some(1) {
                return Err(command_failed(
                    destination_check,
                    "verify the WSL worktree destination",
                ));
            }
        }
        let canonical_destination = self.wsl_canonicalize(repository.host(), &destination)?;
        if !is_posix_descendant(&canonical_root, &canonical_destination) {
            return Err(GitError::InvalidWorktreeDestination(
                "The worktree destination must stay under the allowed WSL root.".to_string(),
            ));
        }
        Ok(VerifiedWorktreeDestination {
            host: repository.host().clone(),
            allowed_root: canonical_root,
            path: canonical_destination,
        })
    }

    pub fn delete_local_branch(
        &self,
        repository: &Repository,
        branch: &str,
        force: bool,
    ) -> Result<u64> {
        self.validate_branch_name(&repository.context(), branch)?;
        let branch = branch.trim().to_string();
        self.with_mutation(repository, || {
            let arguments = vec![
                "branch".to_string(),
                if force { "-D" } else { "-d" }.to_string(),
                "--".to_string(),
                branch,
            ];
            let output = self.run(
                &repository.context(),
                &arguments,
                false,
                "delete the local branch",
                self.config.mutation_output_bytes,
                OverflowPolicy::Fail,
            )?;
            ensure_success(output.status.success(), &output, "delete the local branch")
        })
        .map(|(_, generation)| generation)
    }

    fn wsl_canonicalize(&self, host: &GitHost, path: &str) -> Result<String> {
        let output = self.run_wsl_utility(
            host,
            "readlink",
            &[
                "--canonicalize-missing".to_string(),
                "--".to_string(),
                path.to_string(),
            ],
            "canonicalize the WSL worktree path",
        )?;
        ensure_success(
            output.status.success(),
            &output,
            "canonicalize the WSL worktree path",
        )?;
        let path = output_text(&output.stdout).trim().to_string();
        normalize_posix_absolute(&path, "canonical WSL path")
    }

    fn run_wsl_utility(
        &self,
        host: &GitHost,
        program: &str,
        arguments: &[String],
        operation: &str,
    ) -> Result<ExecutionOutput> {
        let spec = build_wsl_utility_spec(host, program, arguments)?;
        execute(
            &spec,
            operation,
            CaptureLimits {
                timeout: self.config.command_timeout,
                stdout_bytes: self.config.resolve_output_bytes,
                stderr_bytes: self.config.stderr_output_bytes,
                stdout_overflow: OverflowPolicy::Fail,
            },
        )
    }
}

fn create_worktree_arguments(
    destination: &str,
    branch: &str,
    target: &str,
    create_branch: bool,
) -> Vec<String> {
    let mut arguments = vec!["worktree".to_string(), "add".to_string()];
    if create_branch {
        arguments.extend(["-b".to_string(), branch.to_string()]);
    }
    arguments.extend([
        "--".to_string(),
        destination.to_string(),
        target.to_string(),
    ]);
    arguments
}

fn remove_worktree_arguments(destination: &str, force: bool) -> Vec<String> {
    let mut arguments = vec!["worktree".to_string(), "remove".to_string()];
    if force {
        arguments.push("--force".to_string());
    }
    arguments.extend(["--".to_string(), destination.to_string()]);
    arguments
}

fn verify_native_worktree_destination(
    allowed_root: &str,
    destination: &str,
) -> Result<VerifiedWorktreeDestination> {
    if allowed_root.trim().is_empty() || destination.trim().is_empty() {
        return Err(GitError::InvalidWorktreeDestination(
            "Worktree root and destination cannot be empty.".to_string(),
        ));
    }
    let allowed_root = Path::new(allowed_root);
    reject_unsafe_native_path(allowed_root)?;
    let canonical_root = canonicalize_native_path(allowed_root).map_err(|error| {
        GitError::InvalidWorktreeDestination(format!(
            "The worktree root could not be resolved: {error}"
        ))
    })?;
    reject_unsafe_native_path(&canonical_root)?;
    if !canonical_root.is_dir() || canonical_root.parent().is_none() {
        return Err(GitError::InvalidWorktreeDestination(
            "The worktree root must be an existing non-root directory.".to_string(),
        ));
    }

    let requested = Path::new(destination);
    reject_unsafe_native_path(requested)?;
    reject_parent_components(requested)?;
    let candidate = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        canonical_root.join(requested)
    };
    match fs::symlink_metadata(&candidate) {
        Ok(_) => {
            return Err(GitError::InvalidWorktreeDestination(
                "The worktree destination must be a new path.".to_string(),
            ));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(GitError::InvalidWorktreeDestination(format!(
                "The worktree destination could not be inspected: {error}"
            )));
        }
    }

    let parent = candidate.parent().ok_or_else(|| {
        GitError::InvalidWorktreeDestination(
            "The worktree destination must have a parent directory.".to_string(),
        )
    })?;
    if !parent.is_dir() {
        return Err(GitError::InvalidWorktreeDestination(
            "The worktree destination parent must already exist.".to_string(),
        ));
    }
    let canonical_parent = canonicalize_native_path(parent).map_err(|error| {
        GitError::InvalidWorktreeDestination(format!(
            "The worktree destination parent could not be resolved: {error}"
        ))
    })?;
    reject_unsafe_native_path(&canonical_parent)?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(GitError::InvalidWorktreeDestination(
            "The worktree destination escapes the allowed root.".to_string(),
        ));
    }
    let name = candidate.file_name().ok_or_else(|| {
        GitError::InvalidWorktreeDestination(
            "The worktree destination could not be normalized.".to_string(),
        )
    })?;
    let normalized_destination = canonical_parent.join(name);
    if normalized_destination == canonical_root
        || !normalized_destination.starts_with(&canonical_root)
    {
        return Err(GitError::InvalidWorktreeDestination(
            "The worktree destination must be below the allowed root.".to_string(),
        ));
    }
    Ok(VerifiedWorktreeDestination {
        host: GitHost::Native,
        allowed_root: canonical_root.to_string_lossy().to_string(),
        path: normalized_destination.to_string_lossy().to_string(),
    })
}

fn reject_parent_components(path: &Path) -> Result<()> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(GitError::InvalidWorktreeDestination(
            "Worktree paths cannot contain parent traversal.".to_string(),
        ));
    }
    Ok(())
}

fn canonicalize_native_path(path: &Path) -> std::io::Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        let value = canonical.to_string_lossy();
        if let Some(value) = value.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{value}")));
        }
        if let Some(value) = value.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(value));
        }
    }
    Ok(canonical)
}

#[cfg(windows)]
fn reject_unsafe_native_path(path: &Path) -> Result<()> {
    use std::path::Prefix;

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::Disk(_) => {}
                Prefix::Verbatim(_)
                | Prefix::VerbatimUNC(_, _)
                | Prefix::VerbatimDisk(_)
                | Prefix::DeviceNS(_)
                | Prefix::UNC(_, _) => {
                    return Err(GitError::InvalidWorktreeDestination(
                        "UNC, device, and verbatim worktree paths are not allowed.".to_string(),
                    ));
                }
            },
            Component::Normal(segment) if segment.to_string_lossy().contains(':') => {
                return Err(GitError::InvalidWorktreeDestination(
                    "Alternate data stream worktree paths are not allowed.".to_string(),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn reject_unsafe_native_path(_path: &Path) -> Result<()> {
    Ok(())
}

fn normalize_posix_absolute(path: &str, label: &str) -> Result<String> {
    if path.is_empty() || path.contains('\0') || !path.starts_with('/') || path.starts_with("//") {
        return Err(GitError::InvalidWorktreeDestination(format!(
            "The WSL {label} must be an absolute local path."
        )));
    }
    let mut components = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                return Err(GitError::InvalidWorktreeDestination(format!(
                    "The WSL {label} cannot contain parent traversal."
                )));
            }
            value => components.push(value),
        }
    }
    Ok(format!("/{}", components.join("/")))
}

fn is_posix_descendant(root: &str, destination: &str) -> bool {
    destination != root
        && destination
            .strip_prefix(root.trim_end_matches('/'))
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn posix_parent(path: &str) -> Result<&str> {
    let (parent, name) = path.rsplit_once('/').ok_or_else(|| {
        GitError::InvalidWorktreeDestination(
            "The WSL worktree destination must have a parent.".to_string(),
        )
    })?;
    if name.is_empty() {
        return Err(GitError::InvalidWorktreeDestination(
            "The WSL worktree destination must have a final path component.".to_string(),
        ));
    }
    Ok(if parent.is_empty() { "/" } else { parent })
}

fn validate_worktree_lookup_path(host: &GitHost, destination: &str) -> Result<()> {
    if destination.trim().is_empty() || destination.contains('\0') {
        return Err(GitError::InvalidWorktreeDestination(
            "The worktree destination cannot be empty.".to_string(),
        ));
    }
    match host {
        GitHost::Native => {
            let path = Path::new(destination);
            reject_unsafe_native_path(path)?;
            reject_parent_components(path)?;
            if !path.is_absolute() {
                return Err(GitError::InvalidWorktreeDestination(
                    "The worktree destination must be absolute.".to_string(),
                ));
            }
        }
        GitHost::Wsl { .. } => {
            normalize_posix_absolute(destination, "worktree destination")?;
        }
    }
    Ok(())
}

fn worktree_paths_equal(host: &GitHost, left: &str, right: &str) -> bool {
    match host {
        GitHost::Native => {
            let left_path = Path::new(left);
            let right_path = Path::new(right);
            match same_file::is_same_file(left_path, right_path) {
                Ok(equivalent) => return equivalent,
                Err(_) if left_path.exists() || right_path.exists() => return false,
                Err(_) => {}
            }
            let normalize = |value: &str| {
                canonicalize_native_path(Path::new(value))
                    .unwrap_or_else(|_| PathBuf::from(value))
                    .to_string_lossy()
                    .replace('/', "\\")
                    .trim_end_matches('\\')
                    .to_ascii_lowercase()
            };
            normalize(left) == normalize(right)
        }
        GitHost::Wsl { .. } => left.trim_end_matches('/') == right.trim_end_matches('/'),
    }
}

pub fn validate_relative_path(path: &str) -> Result<()> {
    let path = path.trim();
    let normalized = path.replace('\\', "/");
    let has_drive_prefix = normalized
        .as_bytes()
        .get(1)
        .is_some_and(|character| *character == b':');
    if path.is_empty()
        || path.contains('\0')
        || normalized.starts_with('/')
        || normalized.starts_with("//")
        || has_drive_prefix
        || normalized.split('/').any(|segment| segment == "..")
    {
        return Err(GitError::InvalidPath(path.to_string()));
    }
    Ok(())
}

pub fn validate_relative_paths(paths: &[String]) -> Result<()> {
    paths
        .iter()
        .try_for_each(|path| validate_relative_path(path))
}

fn validate_commit_message(message: &str) -> Result<()> {
    if message.trim().is_empty() {
        return Err(GitError::InvalidCommitMessage(
            "Commit message cannot be empty.".to_string(),
        ));
    }
    if message.len() > MAX_COMMIT_MESSAGE_BYTES {
        return Err(GitError::InvalidCommitMessage(
            "Commit message is too long.".to_string(),
        ));
    }
    if message.contains('\0') {
        return Err(GitError::InvalidCommitMessage(
            "Commit message contains a null byte.".to_string(),
        ));
    }
    Ok(())
}

fn normalize_context(context: &GitContext) -> Result<GitContext> {
    if context.cwd.trim().is_empty() || context.cwd.contains('\0') {
        return Err(GitError::InvalidPath(context.cwd.clone()));
    }
    let mut context = context.clone();
    if matches!(context.host, GitHost::Native) {
        let path = Path::new(&context.cwd);
        if path.is_file() {
            context.cwd = path.parent().unwrap_or(path).to_string_lossy().to_string();
        }
    }
    Ok(context)
}

fn ensure_success(success: bool, output: &ExecutionOutput, operation: &str) -> Result<()> {
    if success {
        Ok(())
    } else {
        Err(command_failed_ref(output, operation))
    }
}

fn command_failed(output: ExecutionOutput, operation: &str) -> GitError {
    command_failed_ref(&output, operation)
}

fn command_failed_ref(output: &ExecutionOutput, operation: &str) -> GitError {
    let stderr = output_text(&output.stderr).trim().to_string();
    let stdout = output_text(&output.stdout).trim().to_string();
    GitError::CommandFailed {
        operation: operation.to_string(),
        exit_code: output.status.code(),
        detail: if stderr.is_empty() { stdout } else { stderr },
    }
}

fn is_not_repository(output: &ExecutionOutput) -> bool {
    let detail = format!(
        "{}\n{}",
        output_text(&output.stderr),
        output_text(&output.stdout)
    )
    .to_ascii_lowercase();
    detail.contains("not a git repository")
}

fn output_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

fn recover_lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn recover_read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn recover_write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn command_specs_use_argument_arrays_and_literal_pathspecs() {
        let native = build_git_command_spec(
            &GitContext::native(r"D:\repo with spaces"),
            &["status".to_string()],
            true,
        );
        assert_eq!(native.program, "git");
        assert_eq!(
            native.args,
            [
                "-C",
                r"D:\repo with spaces",
                "--literal-pathspecs",
                "status"
            ]
        );
        assert_eq!(native.env.get("GIT_OPTIONAL_LOCKS"), Some(&"0".to_string()));

        let wsl = build_git_command_spec(
            &GitContext::wsl("/mnt/d/repo with spaces", Some("Ubuntu-24.04".to_string())),
            &["diff".to_string()],
            false,
        );
        assert_eq!(wsl.program, "wsl.exe");
        assert_eq!(
            wsl.args,
            [
                "--distribution",
                "Ubuntu-24.04",
                "--exec",
                "git",
                "-C",
                "/mnt/d/repo with spaces",
                "--literal-pathspecs",
                "diff"
            ]
        );
        assert!(!wsl.env.contains_key("GIT_OPTIONAL_LOCKS"));

        let utility = build_wsl_utility_spec(
            &GitHost::Wsl {
                distribution: Some("Ubuntu-24.04".to_string()),
            },
            "readlink",
            &[
                "--canonicalize-missing".to_string(),
                "/mnt/d/worktree".to_string(),
            ],
        )
        .expect("WSL utility spec should build");
        assert_eq!(utility.program, "wsl.exe");
        assert_eq!(
            utility.args,
            [
                "--distribution",
                "Ubuntu-24.04",
                "--exec",
                "readlink",
                "--canonicalize-missing",
                "/mnt/d/worktree"
            ]
        );
    }

    #[test]
    fn worktree_arguments_keep_paths_and_revisions_after_option_terminator() {
        assert_eq!(
            create_worktree_arguments(
                r"D:\worktrees\feature one",
                "feature/one",
                "0123456789abcdef",
                true,
            ),
            [
                "worktree",
                "add",
                "-b",
                "feature/one",
                "--",
                r"D:\worktrees\feature one",
                "0123456789abcdef"
            ]
        );
        assert_eq!(
            create_worktree_arguments(
                "/mnt/d/worktrees/existing",
                "feature/existing",
                "feature/existing",
                false,
            ),
            [
                "worktree",
                "add",
                "--",
                "/mnt/d/worktrees/existing",
                "feature/existing"
            ]
        );
        assert_eq!(
            remove_worktree_arguments(r"D:\worktrees\feature one", true),
            [
                "worktree",
                "remove",
                "--force",
                "--",
                r"D:\worktrees\feature one"
            ]
        );
    }

    #[test]
    fn worktree_porcelain_parser_preserves_registration_metadata() {
        let output = concat!(
            "worktree D:/repo\0",
            "HEAD 1111111111111111111111111111111111111111\0",
            "branch refs/heads/main\0",
            "\0",
            "worktree D:/worktrees/review\0",
            "HEAD 2222222222222222222222222222222222222222\0",
            "detached\0",
            "locked review in progress\0",
            "prunable administrative file missing\0",
            "\0",
        );
        let worktrees =
            parse_worktree_porcelain(output.as_bytes()).expect("worktrees should parse");
        assert_eq!(worktrees.len(), 2);
        assert!(worktrees[0].is_main());
        assert_eq!(worktrees[0].branch(), Some("main"));
        assert!(!worktrees[1].is_main());
        assert!(worktrees[1].is_detached());
        assert_eq!(worktrees[1].locked_reason(), Some("review in progress"));
        assert_eq!(
            worktrees[1].prunable_reason(),
            Some("administrative file missing")
        );
    }

    #[test]
    fn native_worktree_destination_rejects_escape_existing_and_special_paths() {
        let directory = TempDir::new().expect("temporary root should be created");
        let allowed_root = directory.path().join("worktrees");
        fs::create_dir(&allowed_root).expect("allowed root should be created");
        fs::create_dir(allowed_root.join("feature")).expect("nested parent should be created");

        let verified = verify_native_worktree_destination(
            &allowed_root.to_string_lossy(),
            "feature/new-worktree",
        )
        .expect("new child path should verify");
        assert!(Path::new(verified.path()).starts_with(
            canonicalize_native_path(&allowed_root).expect("allowed root should canonicalize")
        ));

        assert!(
            verify_native_worktree_destination(&allowed_root.to_string_lossy(), "../outside")
                .is_err()
        );
        assert!(verify_native_worktree_destination(
            &allowed_root.to_string_lossy(),
            &directory.path().join("outside").to_string_lossy()
        )
        .is_err());

        let existing = allowed_root.join("existing");
        fs::create_dir(&existing).expect("existing destination should be created");
        assert!(verify_native_worktree_destination(
            &allowed_root.to_string_lossy(),
            &existing.to_string_lossy()
        )
        .is_err());

        #[cfg(windows)]
        {
            assert!(verify_native_worktree_destination(
                &allowed_root.to_string_lossy(),
                r"\\server\share\worktree"
            )
            .is_err());
            assert!(verify_native_worktree_destination(
                &allowed_root.to_string_lossy(),
                r"\\?\C:\worktree"
            )
            .is_err());
        }
    }

    #[test]
    fn posix_worktree_paths_reject_network_and_traversal_forms() {
        assert_eq!(
            normalize_posix_absolute("/mnt/d/worktrees/feature", "destination")
                .expect("valid path should normalize"),
            "/mnt/d/worktrees/feature"
        );
        assert!(normalize_posix_absolute("../outside", "destination").is_err());
        assert!(normalize_posix_absolute("/mnt/d/../outside", "destination").is_err());
        assert!(normalize_posix_absolute("//server/share", "destination").is_err());
        assert!(is_posix_descendant(
            "/mnt/d/worktrees",
            "/mnt/d/worktrees/feature"
        ));
        assert!(!is_posix_descendant(
            "/mnt/d/worktrees",
            "/mnt/d/worktrees-other/feature"
        ));
    }

    #[test]
    fn porcelain_parser_preserves_metadata_and_change_kinds() {
        let output = concat!(
            "# branch.oid 0123456789abcdef\0",
            "# branch.head feature/vcs\0",
            "# branch.upstream origin/feature/vcs\0",
            "# branch.ab +2 -1\0",
            "1 .M N... 100644 100644 100644 1111111 2222222 src/main.rs\0",
            "2 R. N... 100644 100644 100644 3333333 4444444 R100 docs/new name.md\0",
            "docs/old name.md\0",
            "u UU N... 100644 100644 100644 100644 aaaaaaa bbbbbbb ccccccc conflict.txt\0",
            "? notes/new file.txt\0",
        );
        let snapshot =
            parse_porcelain_v2(output.as_bytes(), r"D:\repo", 7).expect("porcelain should parse");
        assert_eq!(snapshot.summary.branch.as_deref(), Some("feature/vcs"));
        assert_eq!((snapshot.summary.ahead, snapshot.summary.behind), (2, 1));
        assert_eq!(snapshot.summary.generation, 7);
        assert_eq!(snapshot.summary.file_count, 4);
        assert_eq!(snapshot.summary.conflict_count, 1);
        assert_eq!(
            snapshot.files[1].original_path.as_deref(),
            Some("docs/old name.md")
        );
        assert!(snapshot.files[3].untracked);
    }

    #[test]
    fn streaming_porcelain_parser_handles_record_boundaries_and_renames() {
        let output = concat!(
            "# branch.head feature/streaming\0",
            "2 R. N... 100644 100644 100644 3333333 4444444 R100 docs/new name.md\0",
            "docs/old name.md\0",
            "? generated/untracked.rs\0",
        );
        let mut parser = PorcelainV2StreamParser::new(MAX_STATUS_ENTRIES);
        for chunk in output.as_bytes().chunks(7) {
            parser.push(chunk).expect("chunk should parse");
        }
        let snapshot = parser
            .finish(r"D:\repo", 4)
            .expect("streaming porcelain should finish");
        assert_eq!(
            snapshot.summary.branch.as_deref(),
            Some("feature/streaming")
        );
        assert_eq!(snapshot.summary.file_count, 2);
        assert_eq!(
            snapshot.files[0].original_path.as_deref(),
            Some("docs/old name.md")
        );
        assert!(snapshot.files[1].untracked);
    }

    #[test]
    fn streaming_porcelain_parser_enforces_entry_limit() {
        let mut parser = PorcelainV2StreamParser::new(2);
        let error = parser
            .push(&synthetic_mixed_status(3))
            .expect_err("third change must exceed the configured entry cap");
        assert!(matches!(error, GitError::StatusEntryLimit { limit: 2 }));
    }

    #[test]
    fn status_pages_are_bounded_and_stable() {
        let output = synthetic_mixed_status(25);
        let snapshot = parse_porcelain_v2(&output, r"D:\repo", 3).expect("status should parse");
        let page = snapshot.page(10, 10).expect("page should be valid");
        assert_eq!(page.files.len(), 10);
        assert_eq!(page.files[0].path, "src/working/file-00010.rs");
        assert_eq!(page.next_offset, Some(20));
        assert!(snapshot.page(0, MAX_STATUS_PAGE_SIZE + 1).is_err());
        assert!(snapshot.page(26, 10).is_err());
    }

    #[test]
    fn paths_reject_escape_absolute_and_null_inputs() {
        assert!(validate_relative_path("src/main.rs").is_ok());
        assert!(validate_relative_path("docs/file with spaces.md").is_ok());
        assert!(validate_relative_path("../outside.txt").is_err());
        assert!(validate_relative_path(r"C:\outside.txt").is_err());
        assert!(validate_relative_path("/home/user/outside.txt").is_err());
        assert!(validate_relative_path("src/\0bad").is_err());
    }

    #[test]
    fn repository_mutation_lock_serializes_callers() {
        let client = GitClient::default();
        let held = client.repository_lock("native:test");
        let guard = recover_write(&held);
        let other = client.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            let lock = other.repository_lock("native:test");
            let _guard = recover_write(&lock);
            sender.send(()).expect("signal should send");
        });
        assert!(receiver.recv_timeout(Duration::from_millis(75)).is_err());
        drop(guard);
        assert!(receiver.recv_timeout(Duration::from_secs(2)).is_ok());
        worker.join().expect("worker should finish");
    }

    #[test]
    fn parses_5k_mixed_porcelain_records_within_budget() {
        assert_large_status_parses(5_000, Duration::from_secs(1));
    }

    #[test]
    fn parses_10k_mixed_porcelain_records_within_budget() {
        assert_large_status_parses(10_000, Duration::from_millis(1500));
    }

    #[test]
    fn parses_15k_mixed_porcelain_records_within_budget() {
        assert_large_status_parses(15_000, Duration::from_secs(2));
    }

    #[test]
    #[ignore = "performance gate creates 5,000 files; run through tools/run-performance-gates.ps1"]
    fn streams_5k_native_status_first_page_within_budget() {
        assert_native_status_scan(5_000);
    }

    #[test]
    #[ignore = "performance gate creates 10,000 files; run through tools/run-performance-gates.ps1"]
    fn streams_10k_native_status_first_page_within_budget() {
        assert_native_status_scan(10_000);
    }

    #[test]
    #[ignore = "performance gate creates 15,000 files; run through tools/run-performance-gates.ps1"]
    fn reads_15k_native_status_records_within_budget() {
        assert_native_status_scan(15_000);
    }

    fn assert_native_status_scan(file_count: usize) {
        let directory = TempDir::new().expect("temporary repository should be created");
        let context = GitContext::native(directory.path().to_string_lossy());
        let client = GitClient::default();
        run_setup(&client, &context, &["init", "-q"]);
        let generated = directory.path().join("generated");
        fs::create_dir(&generated).expect("generated fixture directory should be created");
        for index in 0..file_count {
            fs::write(generated.join(format!("file-{index:05}.txt")), b"x")
                .expect("performance fixture should be written");
        }

        let started_at = Instant::now();
        let repository = client
            .require_repository(&context)
            .expect("repository should resolve");
        let scan = client
            .start_status_scan(&repository, 250)
            .expect("status scan should start");
        let first_page = scan
            .wait_for_first_page()
            .expect("first status page should return");
        let first_page_after = started_at.elapsed();
        let first_page_count = match &first_page {
            StatusScanFirstPage::Prefix(prefix) => prefix.changes.len(),
            StatusScanFirstPage::Complete(result) => result.snapshot.files.len().min(250),
        };
        assert!(first_page_count > 0 && first_page_count <= 250);
        assert!(
            first_page_after < Duration::from_millis(900),
            "native Git {file_count} request-to-first-page was {first_page_after:?}, budget was 900ms"
        );
        let result = scan
            .wait_for_completion()
            .expect("status snapshot should complete");
        let completed_after = started_at.elapsed();
        assert_eq!(result.snapshot.summary.file_count, file_count);
        assert_eq!(result.snapshot.summary.untracked_count, file_count);
        assert!(result.metrics.stdout_bytes > 0);
        assert!(result.metrics.first_change_after.is_some());
        assert!(
            result
                .metrics
                .first_change_after
                .expect("first status record should be observable")
                <= result.metrics.completed_after
        );
        assert!(
            completed_after < Duration::from_secs(10),
            "read {file_count} native status records in {completed_after:?}, budget was 10s"
        );
        eprintln!(
            "native git status {file_count}: returned_first_page={first_page_after:?}, parser_first_change={:?}, completed={completed_after:?}, bytes={}",
            result.metrics.first_change_after,
            result.metrics.stdout_bytes,
        );
    }

    #[test]
    fn native_status_reports_first_record_latency() {
        let directory = TempDir::new().expect("temporary repository should be created");
        let context = GitContext::native(directory.path().to_string_lossy());
        let client = GitClient::default();
        run_setup(&client, &context, &["init", "-q"]);
        fs::write(directory.path().join("first-visible.txt"), "x")
            .expect("fixture should be written");
        let repository = client
            .require_repository(&context)
            .expect("repository should resolve");
        let result = client
            .read_status_with_metrics(&repository)
            .expect("status should stream");

        assert_eq!(result.snapshot.summary.file_count, 1);
        assert!(result.metrics.stdout_bytes > 0);
        let first = result
            .metrics
            .first_change_after
            .expect("first visible status record should be measured");
        assert!(first <= result.metrics.completed_after);
        assert!(
            first < Duration::from_secs(1),
            "first visible status record arrived too late: {first:?}"
        );
    }

    #[test]
    fn native_status_stream_enforces_output_and_entry_bounds() {
        let directory = TempDir::new().expect("temporary repository should be created");
        let context = GitContext::native(directory.path().to_string_lossy());
        let setup_client = GitClient::default();
        run_setup(&setup_client, &context, &["init", "-q"]);
        for index in 0..3 {
            fs::write(directory.path().join(format!("bounded-{index}.txt")), "x")
                .expect("fixture should be written");
        }
        let repository = setup_client
            .require_repository(&context)
            .expect("repository should resolve");

        let entry_limited = GitClient::new(GitConfig {
            status_entry_limit: 2,
            ..GitConfig::default()
        });
        assert!(matches!(
            entry_limited.read_status(&repository),
            Err(GitError::StatusEntryLimit { limit: 2 })
        ));

        let output_limited = GitClient::new(GitConfig {
            status_output_bytes: 16,
            ..GitConfig::default()
        });
        assert!(matches!(
            output_limited.read_status(&repository),
            Err(GitError::OutputLimit {
                stream: OutputStream::Stdout,
                limit: 16,
                ..
            })
        ));
    }

    #[test]
    fn native_repository_flow_covers_status_diff_mutations_and_validation() {
        let directory = TempDir::new().expect("temporary repository should be created");
        let context = GitContext::native(directory.path().to_string_lossy());
        let client = GitClient::default();
        run_setup(&client, &context, &["init", "-q"]);
        run_setup(&client, &context, &["config", "user.name", "AgentMux Test"]);
        run_setup(
            &client,
            &context,
            &["config", "user.email", "agentmux@example.invalid"],
        );
        fs::write(directory.path().join("tracked.txt"), "before\n")
            .expect("fixture should be written");
        run_setup(&client, &context, &["add", "tracked.txt"]);
        run_setup(&client, &context, &["commit", "-q", "-m", "initial"]);

        fs::write(directory.path().join("tracked.txt"), "before\nafter\n")
            .expect("tracked fixture should be updated");
        fs::write(directory.path().join("untracked.txt"), "new note\n")
            .expect("untracked fixture should be written");

        let repository = client
            .require_repository(&context)
            .expect("repository should resolve");
        let snapshot = client.read_status(&repository).expect("status should load");
        assert_eq!(snapshot.summary.file_count, 2);
        assert_eq!(snapshot.summary.generation, 0);
        assert!(snapshot.files.iter().any(|file| file.path == "tracked.txt"));
        assert!(snapshot
            .files
            .iter()
            .any(|file| file.path == "untracked.txt" && file.untracked));

        let diff = client
            .diff(
                &repository,
                &DiffRequest {
                    path: "tracked.txt".to_string(),
                    staged: false,
                    untracked: false,
                },
            )
            .expect("tracked diff should load");
        assert!(diff.patch.contains("+after"));
        let untracked_diff = client
            .diff(
                &repository,
                &DiffRequest {
                    path: "untracked.txt".to_string(),
                    staged: false,
                    untracked: true,
                },
            )
            .expect("untracked diff should load");
        assert!(untracked_diff.patch.contains("+new note"));

        assert_eq!(
            client.stage(&repository, &[]).expect("stage should work"),
            1
        );
        assert_eq!(
            client
                .unstage(&repository, &["untracked.txt".to_string()])
                .expect("unstage should work"),
            2
        );
        assert_eq!(
            client.stage(&repository, &[]).expect("restage should work"),
            3
        );
        let commit = client
            .commit(&repository, "source control foundation")
            .expect("commit should work");
        assert_eq!(commit.generation, 4);
        assert!(!commit.commit.is_empty());
        assert!(client
            .read_status(&repository)
            .expect("clean status should load")
            .files
            .is_empty());

        client
            .validate_branch_name(&context, "feature/vcs-core")
            .expect("valid branch should pass");
        assert!(client
            .validate_branch_name(&context, "../invalid branch")
            .is_err());
        assert!(!client
            .validate_revision(&repository, "HEAD")
            .expect("HEAD should resolve")
            .is_empty());
        assert!(client
            .validate_revision(&repository, "--definitely-invalid")
            .is_err());
    }

    #[test]
    fn native_worktree_flow_creates_lists_and_removes_without_deleting_branch() {
        let directory = TempDir::new().expect("temporary root should be created");
        let repository_path = directory.path().join("repository");
        let worktree_root = directory.path().join("worktrees");
        fs::create_dir(&repository_path).expect("repository directory should be created");
        fs::create_dir(&worktree_root).expect("worktree root should be created");
        let context = GitContext::native(repository_path.to_string_lossy());
        let client = GitClient::default();
        run_setup(&client, &context, &["init", "-q"]);
        run_setup(&client, &context, &["config", "user.name", "AgentMux Test"]);
        run_setup(
            &client,
            &context,
            &["config", "user.email", "agentmux@example.invalid"],
        );
        fs::write(repository_path.join("README.md"), "worktree fixture\n")
            .expect("fixture should be written");
        run_setup(&client, &context, &["add", "README.md"]);
        run_setup(&client, &context, &["commit", "-q", "-m", "initial"]);
        let repository = client
            .require_repository(&context)
            .expect("repository should resolve");
        let destination_path = worktree_root.join("feature-one");
        let destination = client
            .verify_worktree_destination(
                &repository,
                &worktree_root.to_string_lossy(),
                &destination_path.to_string_lossy(),
            )
            .expect("destination should verify");
        let created = client
            .create_worktree(
                &repository,
                &destination,
                "feature/worktree-one",
                Some("HEAD"),
                true,
            )
            .expect("worktree should be created");
        assert_eq!(created.generation, 1);
        assert!(destination_path.is_dir());

        let worktrees = client
            .list_worktrees(&repository)
            .expect("worktrees should list");
        assert_eq!(worktrees.len(), 2);
        assert!(worktrees[0].is_main());
        assert!(worktrees
            .iter()
            .any(|worktree| worktree.branch() == Some("feature/worktree-one")));

        assert_eq!(
            client
                .remove_worktree(&repository, destination.path(), false)
                .expect("worktree should be removed"),
            2
        );
        assert!(!destination_path.exists());
        assert!(client
            .validate_revision(&repository, "refs/heads/feature/worktree-one")
            .is_ok());
        assert!(client
            .local_branch_head(&repository, "feature/worktree-one")
            .expect("owned branch should resolve")
            .is_some());
        assert_eq!(
            client
                .delete_local_branch(&repository, "feature/worktree-one", true)
                .expect("owned branch should be deleted"),
            3
        );
        assert!(client
            .local_branch_head(&repository, "feature/worktree-one")
            .expect("deleted branch lookup should succeed")
            .is_none());
        assert!(client
            .remove_worktree(&repository, repository.root(), true)
            .is_err());
    }

    fn run_setup(client: &GitClient, context: &GitContext, arguments: &[&str]) {
        let arguments = arguments
            .iter()
            .map(|argument| argument.to_string())
            .collect::<Vec<_>>();
        let output = client
            .run(
                context,
                &arguments,
                false,
                "prepare the test repository",
                1024 * 1024,
                OverflowPolicy::Fail,
            )
            .expect("Git should start");
        ensure_success(
            output.status.success(),
            &output,
            "prepare the test repository",
        )
        .expect("Git setup should succeed");
    }

    fn synthetic_mixed_status(count: usize) -> Vec<u8> {
        let mut output = Vec::with_capacity(count * 150);
        output.extend_from_slice(b"# branch.oid 0123456789abcdef\0");
        output.extend_from_slice(b"# branch.head perf/status\0");
        for index in 0..count {
            let record = match index % 5 {
                0 => format!("1 .M N... 100644 100644 100644 1111111 2222222 src/working/file-{index:05}.rs\0"),
                1 => format!("1 M. N... 100644 100644 100644 1111111 2222222 src/staged/file-{index:05}.rs\0"),
                2 => format!("? src/untracked/file-{index:05}.rs\0"),
                3 => format!("2 R. N... 100644 100644 100644 3333333 4444444 R100 src/renamed/file-{index:05}.rs\0src/previous/file-{index:05}.rs\0"),
                _ => format!("u UU N... 100644 100644 100644 100644 aaaaaaa bbbbbbb ccccccc src/conflicted/file-{index:05}.rs\0"),
            };
            output.extend_from_slice(record.as_bytes());
        }
        output
    }

    fn assert_large_status_parses(count: usize, budget: Duration) {
        let output = synthetic_mixed_status(count);
        let started_at = Instant::now();
        let mut parser = PorcelainV2StreamParser::new(MAX_STATUS_ENTRIES);
        let mut first_change_after = None;
        for chunk in output.chunks(1537) {
            parser.push(chunk).expect("synthetic chunk should parse");
            if first_change_after.is_none() && parser.file_count() > 0 {
                first_change_after = Some(started_at.elapsed());
            }
        }
        let snapshot = parser
            .finish(r"D:\repo", 0)
            .expect("synthetic porcelain should parse");
        let elapsed = started_at.elapsed();
        assert_eq!(snapshot.summary.file_count, count);
        assert_eq!(snapshot.files.len(), count);
        assert!(first_change_after.is_some());
        assert!(first_change_after.expect("first record should arrive") <= elapsed);
        assert!(
            elapsed < budget,
            "parsed {count} records in {elapsed:?}, budget was {budget:?}"
        );
        eprintln!(
            "streaming porcelain {count}: first_change={:?}, completed={elapsed:?}",
            first_change_after,
        );
    }
}
