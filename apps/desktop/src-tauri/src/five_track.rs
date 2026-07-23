use super::*;

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use agentmux_core::{
    AgentHookNormalizer, AgentHookNormalizerConfig, AgentHookProvider, AgentHookState, OutputDelta,
    PtyStreamMetadata, PtyUrlDetector, PtyUrlDetectorConfig,
};
use agentmux_ipc::{
    AgentHookStateParams, AgentHookStateResult, AgentWorktreeCreateParams, AgentWorktreeListParams,
    AgentWorktreeListResult, AgentWorktreeRecoverParams, AgentWorktreeRemoveParams,
    AgentWorktreeResult, DevelopmentServerCandidateDismissParams,
    DevelopmentServerCandidateDismissResult, DevelopmentServerCandidateListParams,
    DevelopmentServerCandidateListResult, DevelopmentServerCandidateOpenInSplitParams,
    DevelopmentServerCandidateOpenInSplitResult, DevelopmentServerCandidateParams,
    DevelopmentServerCandidateResult, GitAllMutationParams, GitChangeSummaryResult,
    GitCommitParams as IpcGitCommitParams, GitDiffParams as IpcGitDiffParams,
    GitDiffResult as IpcGitDiffResult, GitMutationResult as IpcGitMutationResult,
    GitPathMutationParams, GitRepositoryChangedEvent, GitRepositoryParams,
    GitReviewCommentCreateParams, GitReviewCommentIdParams, GitReviewCommentListParams,
    GitReviewCommentListResult, GitReviewCommentResult, GitReviewCommentUpdateParams,
    GitReviewDeliveryResult, GitReviewLineAnchor, GitReviewThreadCreateParams,
    GitReviewThreadDeliverParams, GitReviewThreadIdParams, GitReviewThreadListParams,
    GitReviewThreadListResult, GitReviewThreadMarkStaleParams, GitReviewThreadResult,
    GitReviewThreadUpdateParams, GitStatusPageParams, GitStatusPageResult, GitStatusSummaryResult,
    TeamMessageSendParams, EVENT_AGENT_HOOK_STATE_CHANGED, EVENT_DEV_SERVER_CANDIDATE_DETECTED,
    EVENT_GIT_REPOSITORY_CHANGED, METHOD_AGENT_HOOK_STATE, METHOD_AGENT_WORKTREE_CREATE,
    METHOD_AGENT_WORKTREE_LIST, METHOD_AGENT_WORKTREE_RECOVER, METHOD_AGENT_WORKTREE_REMOVE,
    METHOD_DEV_SERVER_CANDIDATE_DETECTED, METHOD_DEV_SERVER_CANDIDATE_DISMISS,
    METHOD_DEV_SERVER_CANDIDATE_LIST, METHOD_DEV_SERVER_CANDIDATE_OPEN_IN_SPLIT, METHOD_GIT_COMMIT,
    METHOD_GIT_DIFF, METHOD_GIT_DISCARD, METHOD_GIT_REVIEW_COMMENT_CREATE,
    METHOD_GIT_REVIEW_COMMENT_DELETE, METHOD_GIT_REVIEW_COMMENT_LIST,
    METHOD_GIT_REVIEW_COMMENT_UPDATE, METHOD_GIT_REVIEW_THREAD_CREATE,
    METHOD_GIT_REVIEW_THREAD_DELETE, METHOD_GIT_REVIEW_THREAD_DELIVER,
    METHOD_GIT_REVIEW_THREAD_LIST, METHOD_GIT_REVIEW_THREAD_MARK_STALE,
    METHOD_GIT_REVIEW_THREAD_UPDATE, METHOD_GIT_STAGE, METHOD_GIT_STAGE_ALL,
    METHOD_GIT_STATUS_PAGE, METHOD_GIT_STATUS_SUMMARY, METHOD_GIT_UNSTAGE, METHOD_GIT_UNSTAGE_ALL,
};
use agentmux_store::{
    GitReviewDeliveryUpdate, PersistedGitReviewComment, PersistedGitReviewThread,
    PersistedNotification, PersistedWorktreeOperation, WorktreeOperationState,
};
use agentmux_vcs::{
    DiffRequest, GitClient, GitContext, GitError, GitFileChange, GitHost as VcsGitHost, Repository,
    StatusSnapshot,
};
const LEGACY_GIT_STATUS: &str = "git.status";
const STATUS_CACHE_MAX_AGE: Duration = Duration::from_secs(5 * 60);
const REPOSITORY_MONITOR_RECONCILE_INTERVAL: Duration = Duration::from_secs(2);
const REPOSITORY_FALLBACK_STATUS_INTERVAL: Duration = Duration::from_secs(30);
const REPOSITORY_EVENT_DEBOUNCE: Duration = Duration::from_millis(250);
const MAX_WATCHER_STARTS_PER_TICK: usize = 2;
const MAX_FALLBACK_STATUS_READS_PER_TICK: usize = 1;
const MAX_OBSERVED_REPOSITORIES: usize = 64;
const MAX_MUTATION_IDEMPOTENCY_RESULTS: usize = 512;
const MAX_DEV_SERVER_CANDIDATES: usize = 500;
const MAX_DEV_SERVER_CANDIDATES_PER_SESSION: usize = 32;

pub(super) struct FiveTrackState {
    shared: Arc<FiveTrackShared>,
}

struct FiveTrackShared {
    git: GitClient,
    git_status_cache: Mutex<HashMap<String, CachedStatusSnapshot>>,
    observed_repositories: Mutex<HashMap<String, ObservedRepository>>,
    mutation_results: Mutex<MutationResultCache>,
    monitor_started: AtomicBool,
    worktree_saga_guard: Mutex<()>,
    hook_normalizer: Mutex<AgentHookNormalizer>,
    verified_hooks: Mutex<HashMap<String, VerifiedHookRecord>>,
    url_detectors: Mutex<HashMap<String, PtyUrlDetector>>,
    dev_server_candidates: Mutex<VecDeque<DevelopmentServerCandidateResult>>,
}

#[derive(Clone)]
struct CachedStatusSnapshot {
    repository: Repository,
    snapshot: StatusSnapshot,
    captured_at: Instant,
}

#[derive(Clone)]
struct ObservedRepository {
    workspace_id: String,
    repository_id: String,
    repository: Repository,
    scan_root: Option<PathBuf>,
    monitor_mode: RepositoryMonitorMode,
    watch_cancel: Option<Arc<RepositoryWatchCancellation>>,
    fallback_status_hash: Option<u64>,
    next_fallback_status_at: Instant,
    last_event_at: Option<Instant>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RepositoryMonitorMode {
    Pending,
    NativeWatcher,
    FallbackStatus,
}

struct RepositoryWatchCancellation {
    requested: AtomicBool,
    handles: Mutex<HashSet<usize>>,
}

impl RepositoryWatchCancellation {
    fn new() -> Self {
        Self {
            requested: AtomicBool::new(false),
            handles: Mutex::new(HashSet::new()),
        }
    }

    #[cfg(windows)]
    fn register(&self, handle: usize) -> bool {
        let Ok(mut handles) = self.handles.lock() else {
            return false;
        };
        if self.requested.load(Ordering::Acquire) {
            return false;
        }
        handles.insert(handle);
        true
    }

    #[cfg(windows)]
    fn unregister(&self, handle: usize) {
        if let Ok(mut handles) = self.handles.lock() {
            handles.remove(&handle);
        }
    }

    fn request(&self) {
        if self.requested.swap(true, Ordering::AcqRel) {
            return;
        }
        #[cfg(windows)]
        if let Ok(handles) = self.handles.lock() {
            for handle in handles.iter().copied() {
                cancel_directory_io(handle);
            }
        }
    }
}

#[derive(Default)]
struct MutationResultCache {
    values: HashMap<String, IpcGitMutationResult>,
    order: VecDeque<String>,
}

#[derive(Clone)]
struct VerifiedHookRecord {
    session_id: String,
    sequence: u64,
    state: String,
    reason: Option<String>,
    telemetry: Option<AgentTelemetry>,
}

impl FiveTrackState {
    pub(super) fn new() -> Self {
        Self {
            shared: Arc::new(FiveTrackShared {
                git: GitClient::default(),
                git_status_cache: Mutex::new(HashMap::new()),
                observed_repositories: Mutex::new(HashMap::new()),
                mutation_results: Mutex::new(MutationResultCache::default()),
                monitor_started: AtomicBool::new(false),
                worktree_saga_guard: Mutex::new(()),
                hook_normalizer: Mutex::new(AgentHookNormalizer::new(
                    AgentHookNormalizerConfig::default(),
                )),
                verified_hooks: Mutex::new(HashMap::new()),
                url_detectors: Mutex::new(HashMap::new()),
                dev_server_candidates: Mutex::new(VecDeque::new()),
            }),
        }
    }

    pub(super) fn start_repository_monitor(&self, app: tauri::AppHandle) {
        if self.shared.monitor_started.swap(true, Ordering::AcqRel) {
            return;
        }
        let shared = Arc::clone(&self.shared);
        let _ = thread::Builder::new()
            .name("agentmux-git-monitor".to_string())
            .spawn(move || loop {
                reconcile_observed_repositories(&shared, Some(&app));
                thread::sleep(REPOSITORY_MONITOR_RECONCILE_INTERVAL);
            });
    }
}

pub(super) fn is_five_track_method(method: &str) -> bool {
    matches!(
        method,
        LEGACY_GIT_STATUS
            | METHOD_GIT_STATUS_SUMMARY
            | METHOD_GIT_STATUS_PAGE
            | METHOD_GIT_DIFF
            | METHOD_GIT_STAGE
            | METHOD_GIT_UNSTAGE
            | METHOD_GIT_STAGE_ALL
            | METHOD_GIT_UNSTAGE_ALL
            | METHOD_GIT_DISCARD
            | METHOD_GIT_COMMIT
            | METHOD_AGENT_WORKTREE_CREATE
            | METHOD_AGENT_WORKTREE_LIST
            | METHOD_AGENT_WORKTREE_RECOVER
            | METHOD_AGENT_WORKTREE_REMOVE
            | METHOD_GIT_REVIEW_THREAD_LIST
            | METHOD_GIT_REVIEW_THREAD_CREATE
            | METHOD_GIT_REVIEW_THREAD_UPDATE
            | METHOD_GIT_REVIEW_THREAD_DELETE
            | METHOD_GIT_REVIEW_THREAD_MARK_STALE
            | METHOD_GIT_REVIEW_THREAD_DELIVER
            | METHOD_GIT_REVIEW_COMMENT_LIST
            | METHOD_GIT_REVIEW_COMMENT_CREATE
            | METHOD_GIT_REVIEW_COMMENT_UPDATE
            | METHOD_GIT_REVIEW_COMMENT_DELETE
            | METHOD_AGENT_HOOK_STATE
            | METHOD_DEV_SERVER_CANDIDATE_DETECTED
            | METHOD_DEV_SERVER_CANDIDATE_LIST
            | METHOD_DEV_SERVER_CANDIDATE_DISMISS
            | METHOD_DEV_SERVER_CANDIDATE_OPEN_IN_SPLIT
    )
}

pub(super) fn is_mutating_five_track_method(method: &str) -> bool {
    matches!(
        method,
        METHOD_GIT_STAGE
            | METHOD_GIT_UNSTAGE
            | METHOD_GIT_STAGE_ALL
            | METHOD_GIT_UNSTAGE_ALL
            | METHOD_GIT_DISCARD
            | METHOD_GIT_COMMIT
            | METHOD_AGENT_WORKTREE_CREATE
            | METHOD_AGENT_WORKTREE_RECOVER
            | METHOD_AGENT_WORKTREE_REMOVE
            | METHOD_GIT_REVIEW_THREAD_CREATE
            | METHOD_GIT_REVIEW_THREAD_UPDATE
            | METHOD_GIT_REVIEW_THREAD_DELETE
            | METHOD_GIT_REVIEW_THREAD_MARK_STALE
            | METHOD_GIT_REVIEW_THREAD_DELIVER
            | METHOD_GIT_REVIEW_COMMENT_CREATE
            | METHOD_GIT_REVIEW_COMMENT_UPDATE
            | METHOD_GIT_REVIEW_COMMENT_DELETE
            | METHOD_AGENT_HOOK_STATE
            | METHOD_DEV_SERVER_CANDIDATE_DETECTED
            | METHOD_DEV_SERVER_CANDIDATE_DISMISS
            | METHOD_DEV_SERVER_CANDIDATE_OPEN_IN_SPLIT
    )
}

impl DesktopControlState {
    pub(super) fn handle_five_track_request(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        match request.method.as_str() {
            LEGACY_GIT_STATUS => self.handle_legacy_git_status(request),
            METHOD_GIT_STATUS_SUMMARY => self.handle_git_status_summary_v2(request),
            METHOD_GIT_STATUS_PAGE => self.handle_git_status_page_v2(request),
            METHOD_GIT_DIFF => self.handle_git_diff_v2_or_legacy(request),
            METHOD_GIT_STAGE => self.handle_git_paths_mutation(request, GitMutation::Stage),
            METHOD_GIT_UNSTAGE => self.handle_git_paths_mutation(request, GitMutation::Unstage),
            METHOD_GIT_STAGE_ALL => self.handle_git_all_mutation(request, GitMutation::Stage),
            METHOD_GIT_UNSTAGE_ALL => self.handle_git_all_mutation(request, GitMutation::Unstage),
            METHOD_GIT_DISCARD => self.handle_git_paths_mutation(request, GitMutation::Discard),
            METHOD_GIT_COMMIT => self.handle_git_commit_v2_or_legacy(request),
            METHOD_AGENT_WORKTREE_CREATE => self.handle_agent_worktree_create(request),
            METHOD_AGENT_WORKTREE_LIST => self.handle_agent_worktree_list(request),
            METHOD_AGENT_WORKTREE_RECOVER => self.handle_agent_worktree_recover(request),
            METHOD_AGENT_WORKTREE_REMOVE => self.handle_agent_worktree_remove(request),
            METHOD_GIT_REVIEW_THREAD_LIST => self.handle_git_review_thread_list(request),
            METHOD_GIT_REVIEW_THREAD_CREATE => self.handle_git_review_thread_create(request),
            METHOD_GIT_REVIEW_THREAD_UPDATE => self.handle_git_review_thread_update(request),
            METHOD_GIT_REVIEW_THREAD_DELETE => self.handle_git_review_thread_delete(request),
            METHOD_GIT_REVIEW_THREAD_MARK_STALE => {
                self.handle_git_review_thread_mark_stale(request)
            }
            METHOD_GIT_REVIEW_THREAD_DELIVER => self.handle_git_review_thread_deliver(request),
            METHOD_GIT_REVIEW_COMMENT_LIST => self.handle_git_review_comment_list(request),
            METHOD_GIT_REVIEW_COMMENT_CREATE => self.handle_git_review_comment_create(request),
            METHOD_GIT_REVIEW_COMMENT_UPDATE => self.handle_git_review_comment_update(request),
            METHOD_GIT_REVIEW_COMMENT_DELETE => self.handle_git_review_comment_delete(request),
            METHOD_AGENT_HOOK_STATE => self.handle_agent_hook_state(request),
            METHOD_DEV_SERVER_CANDIDATE_DETECTED => {
                self.handle_dev_server_candidate_detected(request)
            }
            METHOD_DEV_SERVER_CANDIDATE_LIST => self.handle_dev_server_candidate_list(request),
            METHOD_DEV_SERVER_CANDIDATE_DISMISS => {
                self.handle_dev_server_candidate_dismiss(request)
            }
            METHOD_DEV_SERVER_CANDIDATE_OPEN_IN_SPLIT => {
                self.handle_dev_server_candidate_open_in_split(request)
            }
            _ => Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::UnsupportedMethod,
                format!("Unsupported method '{}'.", request.method),
            ))),
        }
    }
}

#[derive(Clone, Copy)]
enum GitMutation {
    Stage,
    Unstage,
    Discard,
}

fn vcs_error(error: GitError) -> DesktopHostError {
    let code = match error {
        GitError::InvalidBranch(_)
        | GitError::InvalidRevision(_)
        | GitError::InvalidPath(_)
        | GitError::InvalidCommitMessage(_)
        | GitError::InvalidWorktreeDestination(_)
        | GitError::InvalidWorktreeRequest(_)
        | GitError::InvalidStatusPage(_)
        | GitError::NotRepository => ErrorCode::InvalidRequest,
        GitError::Timeout { .. }
        | GitError::OutputLimit { .. }
        | GitError::StateUnavailable(_)
        | GitError::Io { .. }
        | GitError::CommandFailed { .. }
        | GitError::InvalidOutput(_) => ErrorCode::Conflict,
    };
    DesktopHostError::Control(ControlError::new(code, error.to_string()))
}

impl DesktopControlState {
    fn workspace_vcs_context(
        &self,
        workspace_id: &str,
    ) -> Result<(GitContext, GitCommandContext), DesktopHostError> {
        let legacy = self.workspace_git_context(workspace_id)?;
        let context = match &legacy.host {
            GitHost::Native => GitContext::native(legacy.cwd.clone()),
            GitHost::Wsl { distribution } => {
                GitContext::wsl(legacy.cwd.clone(), distribution.clone())
            }
        };
        Ok((context, legacy))
    }

    fn resolve_vcs_repository(
        &self,
        workspace_id: &str,
    ) -> Result<(GitCommandContext, Repository), DesktopHostError> {
        let (context, legacy) = self.workspace_vcs_context(workspace_id)?;
        let repository = self
            .five_track
            .shared
            .git
            .require_repository(&context)
            .map_err(vcs_error)?;
        Ok((legacy, repository))
    }

    fn status_snapshot(
        &self,
        workspace_id: &str,
        repository: &Repository,
    ) -> Result<(String, StatusSnapshot), DesktopHostError> {
        let repository_id = repository_id(repository);
        let generation = self.five_track.shared.git.generation(repository);
        if let Ok(cache) = self.five_track.shared.git_status_cache.lock() {
            if let Some(entry) = cache.get(&repository_id) {
                if entry.captured_at.elapsed() <= STATUS_CACHE_MAX_AGE
                    && entry.snapshot.summary.generation == generation
                {
                    self.observe_repository(workspace_id, &repository_id, &entry.repository);
                    return Ok((repository_id, entry.snapshot.clone()));
                }
            }
        }

        let snapshot = self
            .five_track
            .shared
            .git
            .read_status(repository)
            .map_err(vcs_error)?;
        if let Ok(mut cache) = self.five_track.shared.git_status_cache.lock() {
            cache.insert(
                repository_id.clone(),
                CachedStatusSnapshot {
                    repository: repository.clone(),
                    snapshot: snapshot.clone(),
                    captured_at: Instant::now(),
                },
            );
        }
        self.observe_repository(workspace_id, &repository_id, repository);
        Ok((repository_id, snapshot))
    }

    fn observe_repository(&self, workspace_id: &str, repository_id: &str, repository: &Repository) {
        let key = format!("{workspace_id}|{repository_id}");
        let Ok(mut observed) = self.five_track.shared.observed_repositories.lock() else {
            return;
        };
        if observed.len() >= MAX_OBSERVED_REPOSITORIES && !observed.contains_key(&key) {
            return;
        }
        observed
            .entry(key)
            .and_modify(|entry| {
                entry.workspace_id = workspace_id.to_string();
                entry.repository = repository.clone();
                let scan_root = repository_scan_root(repository);
                if entry.scan_root != scan_root {
                    if let Some(cancel) = entry.watch_cancel.take() {
                        cancel.request();
                    }
                    entry.scan_root = scan_root;
                    entry.monitor_mode = RepositoryMonitorMode::Pending;
                    entry.fallback_status_hash = None;
                    entry.next_fallback_status_at = Instant::now();
                }
            })
            .or_insert_with(|| ObservedRepository {
                workspace_id: workspace_id.to_string(),
                repository_id: repository_id.to_string(),
                repository: repository.clone(),
                scan_root: repository_scan_root(repository),
                monitor_mode: RepositoryMonitorMode::Pending,
                watch_cancel: None,
                fallback_status_hash: None,
                next_fallback_status_at: Instant::now(),
                last_event_at: None,
            });
    }

    fn handle_legacy_git_status(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitWorkspaceParams = request.parse_params()?;
        let (context, _) = self.workspace_vcs_context(&params.workspace_id)?;
        let Some(repository) = self
            .five_track
            .shared
            .git
            .resolve_repository(&context)
            .map_err(vcs_error)?
        else {
            return Ok(ResponseEnvelope::ok_typed(
                request.id.clone(),
                &GitStatusResult {
                    is_repository: false,
                    repository_root: None,
                    branch: None,
                    head: None,
                    upstream: None,
                    ahead: 0,
                    behind: 0,
                    files: Vec::new(),
                },
            ));
        };
        let (_, snapshot) = self.status_snapshot(&params.workspace_id, &repository)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &legacy_git_status_result(&snapshot),
        ))
    }

    fn handle_git_status_summary_v2(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitRepositoryParams = request.parse_params()?;
        validate_workspace_id(&params.workspace_id)?;
        let (_, repository) = self.resolve_vcs_repository(&params.workspace_id)?;
        let (repository_id, snapshot) = self.status_snapshot(&params.workspace_id, &repository)?;
        validate_repository_selector(params.repository_id.as_deref(), &repository_id)?;
        let summary = &snapshot.summary;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &GitStatusSummaryResult {
                workspace_id: params.workspace_id,
                repository_id,
                repository_root: summary.repository_root.clone(),
                branch: summary.branch.clone(),
                head_oid: summary.head.clone(),
                upstream: summary.upstream.clone(),
                ahead: summary.ahead,
                behind: summary.behind,
                staged_count: summary.staged_count,
                unstaged_count: summary.unstaged_count,
                untracked_count: summary.untracked_count,
                conflicted_count: summary.conflict_count,
                generation: summary.generation,
                refreshed_at: timestamp(),
            },
        ))
    }

    fn handle_git_status_page_v2(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitStatusPageParams = request.parse_params()?;
        params.validate()?;
        let (_, repository) = self.resolve_vcs_repository(&params.workspace_id)?;
        let (repository_id, snapshot) = self.status_snapshot(&params.workspace_id, &repository)?;
        validate_repository_selector(params.repository_id.as_deref(), &repository_id)?;
        validate_generation(params.generation, snapshot.summary.generation)?;

        let filtered = snapshot
            .files
            .iter()
            .filter(|change| git_change_matches_state(change, params.state.as_deref()))
            .collect::<Vec<_>>();
        let offset = parse_git_cursor(params.cursor.as_deref())?;
        if offset > filtered.len() {
            return Err(control_invalid_request(format!(
                "Git status cursor {offset} exceeds {} changes.",
                filtered.len()
            )));
        }
        let limit = params.limit.unwrap_or(250).clamp(1, 500);
        let end = offset.saturating_add(limit).min(filtered.len());
        let changes = filtered[offset..end]
            .iter()
            .map(|change| git_change_result(change))
            .collect();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &GitStatusPageResult {
                workspace_id: params.workspace_id,
                repository_id,
                generation: snapshot.summary.generation,
                changes,
                next_cursor: (end < filtered.len()).then(|| end.to_string()),
                total_count: Some(filtered.len()),
            },
        ))
    }

    fn handle_git_diff_v2_or_legacy(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let raw: serde_json::Value = serde_json::from_str(&request.params_json)?;
        let is_legacy = raw.get("staged").is_some() || raw.get("untracked").is_some();
        if is_legacy {
            let params: GitDiffParams = request.parse_params()?;
            validate_git_relative_path(&params.path)?;
            let (_, repository) = self.resolve_vcs_repository(&params.workspace_id)?;
            let result = self
                .five_track
                .shared
                .git
                .diff(
                    &repository,
                    &DiffRequest {
                        path: params.path.clone(),
                        staged: params.staged,
                        untracked: params.untracked,
                    },
                )
                .map_err(vcs_error)?;
            return Ok(ResponseEnvelope::ok_typed(
                request.id.clone(),
                &GitDiffResult {
                    path: result.path,
                    staged: result.staged,
                    patch: result.patch,
                    truncated: result.truncated,
                },
            ));
        }

        let params: IpcGitDiffParams = request.parse_params()?;
        params.validate()?;
        let (_, repository) = self.resolve_vcs_repository(&params.workspace_id)?;
        let repository_id = repository_id(&repository);
        validate_repository_selector(params.repository_id.as_deref(), &repository_id)?;
        validate_generation(
            params.generation,
            self.five_track.shared.git.generation(&repository),
        )?;
        let (staged, untracked) = match params.stage.as_deref() {
            None | Some("unstaged" | "worktree") => (false, false),
            Some("staged" | "index") => (true, false),
            Some("untracked") => (false, true),
            Some(value) => {
                return Err(control_invalid_request(format!(
                    "Unsupported Git diff stage '{value}'."
                )))
            }
        };
        let result = self
            .five_track
            .shared
            .git
            .diff(
                &repository,
                &DiffRequest {
                    path: params.path.clone(),
                    staged,
                    untracked,
                },
            )
            .map_err(vcs_error)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &IpcGitDiffResult {
                workspace_id: params.workspace_id,
                repository_id,
                generation: result.generation,
                path: result.path,
                original_path: None,
                is_binary: diff_is_binary(&result.patch),
                diff: result.patch,
                truncated: result.truncated,
            },
        ))
    }

    fn handle_git_paths_mutation(
        &self,
        request: &RequestEnvelope,
        mutation: GitMutation,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitPathMutationParams = request.parse_params()?;
        let legacy_all = params.paths.is_empty() && params.idempotency_key.is_none();
        if !legacy_all {
            params.validate()?;
        }
        let (legacy_context, repository) = self.resolve_vcs_repository(&params.workspace_id)?;
        let repository_id = repository_id(&repository);
        validate_repository_selector(params.repository_id.as_deref(), &repository_id)?;
        if let Some(result) = self.reused_git_mutation(
            request.method.as_str(),
            &repository_id,
            params.idempotency_key.as_deref(),
        ) {
            return Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result));
        }

        let generation = match mutation {
            GitMutation::Stage => self
                .five_track
                .shared
                .git
                .stage(&repository, &params.paths)
                .map_err(vcs_error)?,
            GitMutation::Unstage => self
                .five_track
                .shared
                .git
                .unstage(&repository, &params.paths)
                .map_err(vcs_error)?,
            GitMutation::Discard => {
                validate_git_paths(&params.paths)?;
                let mut args = vec![
                    "restore".to_string(),
                    "--worktree".to_string(),
                    "--source=HEAD".to_string(),
                    "--".to_string(),
                ];
                args.extend(params.paths.iter().cloned());
                let output = run_git_command(&legacy_context, &args)?;
                ensure_git_success(&output, "discard changes")?;
                self.five_track
                    .shared
                    .git
                    .mark_repository_changed(&repository)
            }
        };
        let result = IpcGitMutationResult {
            workspace_id: params.workspace_id.clone(),
            repository_id: repository_id.clone(),
            generation,
            affected_paths: params.paths.clone(),
            commit_oid: None,
            reused: false,
        };
        self.remember_git_mutation(
            request.method.as_str(),
            &repository_id,
            params.idempotency_key.as_deref(),
            &result,
        );
        self.git_repository_mutated(
            &params.workspace_id,
            &repository_id,
            &repository,
            generation,
            request.method.as_str(),
        );
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn handle_git_all_mutation(
        &self,
        request: &RequestEnvelope,
        mutation: GitMutation,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitAllMutationParams = request.parse_params()?;
        params.validate()?;
        let (_, repository) = self.resolve_vcs_repository(&params.workspace_id)?;
        let repository_id = repository_id(&repository);
        validate_repository_selector(params.repository_id.as_deref(), &repository_id)?;
        if let Some(result) = self.reused_git_mutation(
            request.method.as_str(),
            &repository_id,
            params.idempotency_key.as_deref(),
        ) {
            return Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result));
        }
        let generation = match mutation {
            GitMutation::Stage => self
                .five_track
                .shared
                .git
                .stage(&repository, &[])
                .map_err(vcs_error)?,
            GitMutation::Unstage => self
                .five_track
                .shared
                .git
                .unstage(&repository, &[])
                .map_err(vcs_error)?,
            GitMutation::Discard => unreachable!("discard-all is intentionally unsupported"),
        };
        let result = IpcGitMutationResult {
            workspace_id: params.workspace_id.clone(),
            repository_id: repository_id.clone(),
            generation,
            affected_paths: Vec::new(),
            commit_oid: None,
            reused: false,
        };
        self.remember_git_mutation(
            request.method.as_str(),
            &repository_id,
            params.idempotency_key.as_deref(),
            &result,
        );
        self.git_repository_mutated(
            &params.workspace_id,
            &repository_id,
            &repository,
            generation,
            request.method.as_str(),
        );
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn handle_git_commit_v2_or_legacy(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let raw: serde_json::Value = serde_json::from_str(&request.params_json)?;
        let is_legacy = raw.get("repository_id").is_none()
            && raw.get("idempotency_key").is_none()
            && raw.get("amend").is_none();
        let params: IpcGitCommitParams = request.parse_params()?;
        params.validate()?;
        let (legacy_context, repository) = self.resolve_vcs_repository(&params.workspace_id)?;
        let repository_id = repository_id(&repository);
        validate_repository_selector(params.repository_id.as_deref(), &repository_id)?;
        if let Some(result) = self.reused_git_mutation(
            request.method.as_str(),
            &repository_id,
            params.idempotency_key.as_deref(),
        ) {
            return Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result));
        }

        let (commit, summary, generation) = if params.amend {
            let output = run_git_command(
                &legacy_context,
                &[
                    "commit".to_string(),
                    "--amend".to_string(),
                    "-m".to_string(),
                    params.message.clone(),
                ],
            )?;
            ensure_git_success(&output, "amend commit")?;
            let oid = run_git_read_command(
                &legacy_context,
                &[
                    "rev-parse".to_string(),
                    "--short".to_string(),
                    "--verify".to_string(),
                    "HEAD".to_string(),
                ],
            )?;
            ensure_git_success(&oid, "read amended commit")?;
            (
                String::from_utf8_lossy(&oid.stdout).trim().to_string(),
                String::from_utf8_lossy(&output.stdout).trim().to_string(),
                self.five_track
                    .shared
                    .git
                    .mark_repository_changed(&repository),
            )
        } else {
            let result = self
                .five_track
                .shared
                .git
                .commit(&repository, &params.message)
                .map_err(vcs_error)?;
            (result.commit, result.summary, result.generation)
        };
        self.git_repository_mutated(
            &params.workspace_id,
            &repository_id,
            &repository,
            generation,
            request.method.as_str(),
        );
        if is_legacy {
            return Ok(ResponseEnvelope::ok_typed(
                request.id.clone(),
                &GitCommitResult { commit, summary },
            ));
        }
        let result = IpcGitMutationResult {
            workspace_id: params.workspace_id,
            repository_id: repository_id.clone(),
            generation,
            affected_paths: Vec::new(),
            commit_oid: Some(commit),
            reused: false,
        };
        self.remember_git_mutation(
            request.method.as_str(),
            &repository_id,
            params.idempotency_key.as_deref(),
            &result,
        );
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn reused_git_mutation(
        &self,
        method: &str,
        repository_id: &str,
        idempotency_key: Option<&str>,
    ) -> Option<IpcGitMutationResult> {
        let key = mutation_cache_key(method, repository_id, idempotency_key?);
        let cache = self.five_track.shared.mutation_results.lock().ok()?;
        let mut result = cache.values.get(&key)?.clone();
        result.reused = true;
        Some(result)
    }

    fn remember_git_mutation(
        &self,
        method: &str,
        repository_id: &str,
        idempotency_key: Option<&str>,
        result: &IpcGitMutationResult,
    ) {
        let Some(idempotency_key) = idempotency_key else {
            return;
        };
        let key = mutation_cache_key(method, repository_id, idempotency_key);
        let Ok(mut cache) = self.five_track.shared.mutation_results.lock() else {
            return;
        };
        if !cache.values.contains_key(&key) {
            cache.order.push_back(key.clone());
        }
        cache.values.insert(key, result.clone());
        while cache.order.len() > MAX_MUTATION_IDEMPOTENCY_RESULTS {
            if let Some(oldest) = cache.order.pop_front() {
                cache.values.remove(&oldest);
            }
        }
    }

    fn git_repository_mutated(
        &self,
        workspace_id: &str,
        repository_id: &str,
        repository: &Repository,
        generation: u64,
        reason: &str,
    ) {
        if let Ok(mut cache) = self.five_track.shared.git_status_cache.lock() {
            cache.remove(repository_id);
        }
        self.observe_repository(workspace_id, repository_id, repository);
        let event = GitRepositoryChangedEvent {
            workspace_id: workspace_id.to_string(),
            repository_id: repository_id.to_string(),
            generation,
            reason: reason.to_string(),
        };
        let should_emit = self
            .five_track
            .shared
            .observed_repositories
            .lock()
            .ok()
            .and_then(|mut observed| {
                let key = format!("{workspace_id}|{repository_id}");
                let entry = observed.get_mut(&key)?;
                let should_emit = entry
                    .last_event_at
                    .is_none_or(|last| last.elapsed() >= REPOSITORY_EVENT_DEBOUNCE);
                if should_emit {
                    entry.last_event_at = Some(Instant::now());
                }
                Some(should_emit)
            })
            .unwrap_or(true);
        if should_emit {
            if let Some(handle) = self.app_handle.get() {
                let _ = handle.emit(EVENT_GIT_REPOSITORY_CHANGED, event);
            }
        }
    }
}

fn mutation_cache_key(method: &str, repository_id: &str, idempotency_key: &str) -> String {
    format!("{method}|{repository_id}|{idempotency_key}")
}

fn validate_workspace_id(workspace_id: &str) -> Result<(), DesktopHostError> {
    if workspace_id.trim().is_empty() {
        Err(control_invalid_request("workspace_id is required"))
    } else {
        Ok(())
    }
}

fn validate_repository_selector(
    requested: Option<&str>,
    actual: &str,
) -> Result<(), DesktopHostError> {
    if requested.is_some_and(|requested| requested != actual) {
        return Err(control_invalid_request(
            "repository_id does not identify the workspace repository",
        ));
    }
    Ok(())
}

fn validate_generation(requested: Option<u64>, actual: u64) -> Result<(), DesktopHostError> {
    if requested.is_some_and(|requested| requested != actual) {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::Conflict,
            format!("Git repository generation changed to {actual}; refresh and retry."),
        )));
    }
    Ok(())
}

fn parse_git_cursor(cursor: Option<&str>) -> Result<usize, DesktopHostError> {
    cursor
        .map(str::trim)
        .filter(|cursor| !cursor.is_empty())
        .map(|cursor| {
            cursor
                .parse::<usize>()
                .map_err(|_| control_invalid_request("Git status cursor is invalid."))
        })
        .transpose()
        .map(|cursor| cursor.unwrap_or(0))
}

fn git_change_matches_state(change: &GitFileChange, state: Option<&str>) -> bool {
    match state.map(str::trim).filter(|state| !state.is_empty()) {
        None | Some("all") => true,
        Some("staged") => change.staged,
        Some("unstaged") => change.unstaged,
        Some("untracked") => change.untracked,
        Some("conflicted" | "conflict") => change.conflict,
        Some(_) => false,
    }
}

fn git_change_result(change: &GitFileChange) -> GitChangeSummaryResult {
    GitChangeSummaryResult {
        path: change.path.clone(),
        original_path: change.original_path.clone(),
        status: git_change_status(change),
        staged: change.staged,
        unstaged: change.unstaged,
        untracked: change.untracked,
        conflicted: change.conflict,
        is_binary: false,
        additions: None,
        deletions: None,
    }
}

fn git_change_status(change: &GitFileChange) -> String {
    if change.conflict {
        "U".to_string()
    } else if change.untracked {
        "?".to_string()
    } else if change.staged && change.index_status != "." {
        change.index_status.clone()
    } else if change.worktree_status != "." {
        change.worktree_status.clone()
    } else {
        "M".to_string()
    }
}

fn legacy_git_status_result(snapshot: &StatusSnapshot) -> GitStatusResult {
    GitStatusResult {
        is_repository: true,
        repository_root: Some(snapshot.summary.repository_root.clone()),
        branch: snapshot.summary.branch.clone(),
        head: snapshot.summary.head.clone(),
        upstream: snapshot.summary.upstream.clone(),
        ahead: snapshot.summary.ahead,
        behind: snapshot.summary.behind,
        files: snapshot
            .files
            .iter()
            .map(|change| GitFileChangeResult {
                path: change.path.clone(),
                original_path: change.original_path.clone(),
                index_status: change.index_status.clone(),
                worktree_status: change.worktree_status.clone(),
                staged: change.staged,
                unstaged: change.unstaged,
                untracked: change.untracked,
                conflict: change.conflict,
            })
            .collect(),
    }
}

fn diff_is_binary(diff: &str) -> bool {
    diff.contains("GIT binary patch") || diff.lines().any(|line| line.starts_with("Binary files "))
}

fn repository_id(repository: &Repository) -> String {
    let host = match repository.host() {
        VcsGitHost::Native => "native".to_string(),
        VcsGitHost::Wsl { distribution } => {
            format!("wsl:{}", distribution.as_deref().unwrap_or_default())
        }
    };
    format!("repo_{:016x}", stable_hash(&(host, repository.root())))
}

fn stable_hash(value: &impl Hash) -> u64 {
    let mut hasher = StableHasher::default();
    value.hash(&mut hasher);
    hasher.finish()
}

fn stable_hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = StableHasher::default();
    hasher.write(bytes);
    hasher.finish()
}

struct StableHasher(u64);

impl Default for StableHasher {
    fn default() -> Self {
        Self(0xcbf29ce484222325)
    }
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
}

fn repository_scan_root(repository: &Repository) -> Option<PathBuf> {
    match repository.host() {
        VcsGitHost::Native => Some(PathBuf::from(repository.root())),
        VcsGitHost::Wsl { distribution } => {
            wsl_path_for_windows_scan(repository.root(), distribution.as_deref())
        }
    }
}

fn wsl_path_for_windows_scan(path: &str, distribution: Option<&str>) -> Option<PathBuf> {
    let normalized = path.replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix("/mnt/") {
        let mut parts = rest.splitn(2, '/');
        let drive = parts.next()?;
        let tail = parts.next().unwrap_or_default().replace('/', "\\");
        if drive.len() == 1 && drive.as_bytes()[0].is_ascii_alphabetic() {
            return Some(PathBuf::from(format!(
                "{}:\\{tail}",
                drive.to_ascii_uppercase()
            )));
        }
    }
    distribution
        .filter(|distribution| !distribution.trim().is_empty())
        .map(|distribution| {
            PathBuf::from(format!(
                r"\\wsl.localhost\{}\{}",
                distribution,
                normalized.trim_start_matches('/').replace('/', "\\")
            ))
        })
}

fn reconcile_observed_repositories(shared: &Arc<FiveTrackShared>, app: Option<&tauri::AppHandle>) {
    let pending = shared
        .observed_repositories
        .lock()
        .map(|observed| {
            observed
                .iter()
                .filter(|(_, entry)| entry.monitor_mode == RepositoryMonitorMode::Pending)
                .take(MAX_WATCHER_STARTS_PER_TICK)
                .map(|(key, entry)| (key.clone(), entry.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for (key, snapshot) in pending {
        let watcher = snapshot
            .scan_root
            .as_deref()
            .ok_or_else(|| "repository has no Windows-visible watch root".to_string())
            .and_then(|root| {
                start_repository_native_watchers(
                    Arc::clone(shared),
                    app.cloned(),
                    key.clone(),
                    root,
                )
            })
            .ok();
        let mode = if watcher.is_some() {
            RepositoryMonitorMode::NativeWatcher
        } else {
            RepositoryMonitorMode::FallbackStatus
        };
        if let Ok(mut observed) = shared.observed_repositories.lock() {
            if let Some(entry) = observed.get_mut(&key) {
                if entry.monitor_mode == RepositoryMonitorMode::Pending {
                    entry.monitor_mode = mode;
                    entry.watch_cancel = watcher;
                    entry.next_fallback_status_at =
                        Instant::now() + REPOSITORY_FALLBACK_STATUS_INTERVAL;
                }
            }
        }
    }

    let now = Instant::now();
    let fallback = shared
        .observed_repositories
        .lock()
        .map(|mut observed| {
            observed
                .iter_mut()
                .filter(|(_, entry)| {
                    entry.monitor_mode == RepositoryMonitorMode::FallbackStatus
                        && entry.next_fallback_status_at <= now
                })
                .take(MAX_FALLBACK_STATUS_READS_PER_TICK)
                .map(|(key, entry)| {
                    entry.next_fallback_status_at = now + REPOSITORY_FALLBACK_STATUS_INTERVAL;
                    (key.clone(), entry.clone())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for (key, snapshot) in fallback {
        let Ok(status) = shared.git.read_status(&snapshot.repository) else {
            continue;
        };
        let digest = status_content_hash(&status);
        let changed = shared
            .observed_repositories
            .lock()
            .ok()
            .and_then(|mut observed| {
                let entry = observed.get_mut(&key)?;
                let changed = entry
                    .fallback_status_hash
                    .is_some_and(|previous| previous != digest);
                entry.fallback_status_hash = Some(digest);
                Some(changed)
            })
            .unwrap_or(false);
        if changed {
            signal_repository_changed(shared, &key, "fallback_status", app);
        }
    }
}

fn signal_repository_changed(
    shared: &FiveTrackShared,
    key: &str,
    reason: &str,
    app: Option<&tauri::AppHandle>,
) {
    let snapshot = shared
        .observed_repositories
        .lock()
        .ok()
        .and_then(|mut observed| {
            let entry = observed.get_mut(key)?;
            if entry
                .last_event_at
                .is_some_and(|last| last.elapsed() < REPOSITORY_EVENT_DEBOUNCE)
            {
                return None;
            }
            entry.last_event_at = Some(Instant::now());
            Some(entry.clone())
        });
    let Some(snapshot) = snapshot else {
        return;
    };
    let generation = shared.git.mark_repository_changed(&snapshot.repository);
    if let Ok(mut cache) = shared.git_status_cache.lock() {
        cache.remove(&snapshot.repository_id);
    }
    if let Some(app) = app {
        let _ = app.emit(
            EVENT_GIT_REPOSITORY_CHANGED,
            GitRepositoryChangedEvent {
                workspace_id: snapshot.workspace_id,
                repository_id: snapshot.repository_id,
                generation,
                reason: reason.to_string(),
            },
        );
    }
}

fn status_content_hash(snapshot: &StatusSnapshot) -> u64 {
    let mut value = snapshot.clone();
    value.summary.generation = 0;
    serde_json::to_vec(&value)
        .map(|bytes| stable_hash_bytes(&bytes))
        .unwrap_or_default()
}

fn repository_watch_roots(root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![root.to_path_buf()];
    let dot_git = root.join(".git");
    if dot_git.is_file() {
        if let Ok(contents) = fs::read_to_string(&dot_git) {
            if let Some(path) = contents
                .lines()
                .next()
                .and_then(|line| line.trim().strip_prefix("gitdir:"))
                .map(str::trim)
                .filter(|path| !path.is_empty())
            {
                let path = PathBuf::from(path);
                let git_dir = if path.is_absolute() {
                    path
                } else {
                    root.join(path)
                };
                if git_dir.is_dir() && !roots.iter().any(|entry| entry == &git_dir) {
                    roots.push(git_dir);
                }
            }
        }
    }
    roots
}

#[cfg(windows)]
fn start_repository_native_watchers(
    shared: Arc<FiveTrackShared>,
    app: Option<tauri::AppHandle>,
    key: String,
    root: &Path,
) -> Result<Arc<RepositoryWatchCancellation>, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let mut handles = Vec::new();
    for path in repository_watch_roots(root) {
        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            for handle in handles {
                unsafe {
                    CloseHandle(handle as *mut std::ffi::c_void);
                }
            }
            return Err(format!("failed to watch {}", path.display()));
        }
        handles.push(handle as usize);
    }

    let cancellation = Arc::new(RepositoryWatchCancellation::new());
    let mut started = 0usize;
    for handle in handles {
        if !cancellation.register(handle) {
            unsafe {
                CloseHandle(handle as *mut std::ffi::c_void);
            }
            continue;
        }
        let shared = Arc::clone(&shared);
        let app = app.clone();
        let key = key.clone();
        let watch_cancel = Arc::clone(&cancellation);
        match thread::Builder::new()
            .name("agentmux-git-watch".to_string())
            .spawn(move || watch_repository_directory(handle, shared, app, key, watch_cancel))
        {
            Ok(_) => started += 1,
            Err(_) => {
                cancellation.unregister(handle);
                unsafe {
                    CloseHandle(handle as *mut std::ffi::c_void);
                }
            }
        }
    }
    if started == 0 {
        Err("failed to start repository watcher thread".to_string())
    } else {
        Ok(cancellation)
    }
}

#[cfg(windows)]
fn cancel_directory_io(handle: usize) {
    #[link(name = "Kernel32")]
    extern "system" {
        fn CancelIoEx(handle: *mut std::ffi::c_void, overlapped: *const std::ffi::c_void) -> i32;
    }
    unsafe {
        CancelIoEx(handle as *mut std::ffi::c_void, std::ptr::null());
    }
}

#[cfg(windows)]
fn watch_repository_directory(
    handle: usize,
    shared: Arc<FiveTrackShared>,
    app: Option<tauri::AppHandle>,
    key: String,
    cancellation: Arc<RepositoryWatchCancellation>,
) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        ReadDirectoryChangesW, FILE_NOTIFY_CHANGE_ATTRIBUTES, FILE_NOTIFY_CHANGE_CREATION,
        FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
        FILE_NOTIFY_CHANGE_SIZE,
    };

    let handle = handle as *mut std::ffi::c_void;
    let handle_id = handle as usize;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let mut bytes_returned = 0u32;
        let ok = unsafe {
            ReadDirectoryChangesW(
                handle,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                1,
                FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_DIR_NAME
                    | FILE_NOTIFY_CHANGE_ATTRIBUTES
                    | FILE_NOTIFY_CHANGE_SIZE
                    | FILE_NOTIFY_CHANGE_LAST_WRITE
                    | FILE_NOTIFY_CHANGE_CREATION,
                &mut bytes_returned,
                std::ptr::null_mut(),
                None,
            )
        };
        if ok == 0 || cancellation.requested.load(Ordering::Acquire) {
            break;
        }
        if bytes_returned > 0 {
            signal_repository_changed(&shared, &key, "filesystem_watcher", app.as_ref());
        }
    }
    cancellation.unregister(handle_id);
    unsafe {
        CloseHandle(handle);
    }
}

#[cfg(not(windows))]
fn start_repository_native_watchers(
    _shared: Arc<FiveTrackShared>,
    _app: Option<tauri::AppHandle>,
    _key: String,
    _root: &Path,
) -> Result<Arc<RepositoryWatchCancellation>, String> {
    Err("native repository watching is only available on Windows".to_string())
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
struct WorktreeOwnership {
    source_workspace_id: String,
    repository_host: String,
    #[serde(default)]
    distribution: Option<String>,
    #[serde(default)]
    worktree_owned: bool,
    #[serde(default)]
    workspace_owned: bool,
    #[serde(default)]
    session_owned: bool,
    #[serde(default)]
    pane_id: Option<String>,
}

impl DesktopControlState {
    fn handle_agent_worktree_create(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: AgentWorktreeCreateParams = request.parse_params()?;
        params.validate()?;
        let _guard = self
            .five_track
            .shared
            .worktree_saga_guard
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("worktree saga lock is unavailable".to_string())
            })?;
        let request_json = serde_json::to_string(&params)?;

        if let Some(existing) = self
            .store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .load_worktree_operation_by_idempotency_key(&params.idempotency_key)?
        {
            ensure_same_worktree_request(&existing, &request_json)?;
            let mut result = self.resume_worktree_operation(existing)?;
            result.reused = true;
            return Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result));
        }

        let (context, _) = self.workspace_vcs_context(&params.workspace_id)?;
        let repository = self
            .five_track
            .shared
            .git
            .require_repository(&context)
            .map_err(vcs_error)?;
        self.five_track
            .shared
            .git
            .validate_branch_name(&context, &params.branch)
            .map_err(vcs_error)?;
        if let Some(revision) = params.base_revision.as_deref() {
            self.five_track
                .shared
                .git
                .validate_revision(&repository, revision)
                .map_err(vcs_error)?;
        }
        let allowed_root = worktree_allowed_root(repository.root())?;
        let destination = self
            .five_track
            .shared
            .git
            .verify_worktree_destination(&repository, &allowed_root, &params.destination)
            .map_err(vcs_error)?;
        let ownership = WorktreeOwnership {
            source_workspace_id: params.workspace_id.clone(),
            repository_host: match repository.host() {
                VcsGitHost::Native => "native".to_string(),
                VcsGitHost::Wsl { .. } => "wsl".to_string(),
            },
            distribution: match repository.host() {
                VcsGitHost::Wsl { distribution } => distribution.clone(),
                VcsGitHost::Native => None,
            },
            ..WorktreeOwnership::default()
        };
        let now = timestamp();
        let operation = PersistedWorktreeOperation {
            operation_id: format!("wtop_{}", unique_time_id()),
            idempotency_key: params.idempotency_key.clone(),
            repository_root: repository.root().to_string(),
            worktree_path: destination.path().to_string(),
            branch_name: Some(params.branch.clone()),
            revision: params.base_revision.clone(),
            workspace_id: None,
            surface_id: None,
            session_id: None,
            owner_kind: "agentmux.desktop".to_string(),
            owner_id: Some(params.workspace_id.clone()),
            ownership_json: serde_json::to_string(&ownership)?,
            request_json,
            state: WorktreeOperationState::Prepared,
            error_code: None,
            error_message: None,
            recovery_json: "{}".to_string(),
            recovery_attempts: 0,
            last_recovery_at: None,
            created_at: now.clone(),
            updated_at: now,
            completed_at: None,
            rolled_back_at: None,
        };
        let operation = self
            .store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .create_or_load_worktree_operation(&operation)?;
        ensure_same_worktree_request(&operation, &serde_json::to_string(&params)?)?;
        let result = self.resume_worktree_operation(operation)?;
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn handle_agent_worktree_list(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: AgentWorktreeListParams = request.parse_params()?;
        let store = self.store.lock().map_err(|_| {
            DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
        })?;
        let mut operations = store.list_recoverable_worktree_operations()?;
        if params.include_completed {
            for workspace in store.list_workspaces()? {
                let Some(operation_id) = managed_worktree_operation_id(&workspace.description)
                else {
                    continue;
                };
                if operations
                    .iter()
                    .any(|operation| operation.operation_id == operation_id)
                {
                    continue;
                }
                if let Some(operation) = store.load_worktree_operation(&operation_id)? {
                    operations.push(operation);
                }
            }
        }
        drop(store);
        let worktrees = operations
            .iter()
            .filter(|operation| {
                params.workspace_id.as_deref().is_none_or(|workspace_id| {
                    operation.owner_id.as_deref() == Some(workspace_id)
                        || operation.workspace_id.as_deref() == Some(workspace_id)
                })
            })
            .map(|operation| worktree_result(operation, false, false))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AgentWorktreeListResult { worktrees },
        ))
    }

    fn handle_agent_worktree_recover(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: AgentWorktreeRecoverParams = request.parse_params()?;
        params.validate()?;
        let _guard = self
            .five_track
            .shared
            .worktree_saga_guard
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("worktree saga lock is unavailable".to_string())
            })?;
        let operation = {
            let store = self.store.lock().map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?;
            match params.operation_id.as_deref() {
                Some(operation_id) => store.load_worktree_operation(operation_id)?,
                None => store.load_worktree_operation_by_idempotency_key(
                    params
                        .idempotency_key
                        .as_deref()
                        .expect("validated idempotency key"),
                )?,
            }
        }
        .ok_or_else(|| control_invalid_request("Worktree operation not found."))?;
        let mut result = self.resume_worktree_operation(operation)?;
        result.recovered = true;
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn handle_agent_worktree_remove(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: AgentWorktreeRemoveParams = request.parse_params()?;
        params.validate()?;
        let _guard = self
            .five_track
            .shared
            .worktree_saga_guard
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("worktree saga lock is unavailable".to_string())
            })?;
        let operation = self
            .store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .load_worktree_operation(&params.worktree_id)?
            .ok_or_else(|| control_invalid_request("Managed worktree not found."))?;
        ensure_agentmux_owned_operation(&operation)?;
        self.rollback_worktree_resources(&operation, params.force, false)?;
        let mut result = worktree_result(&operation, true, true)?;
        result.state = "removed".to_string();
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn resume_worktree_operation(
        &self,
        mut operation: PersistedWorktreeOperation,
    ) -> Result<AgentWorktreeResult, DesktopHostError> {
        ensure_agentmux_owned_operation(&operation)?;
        let params: AgentWorktreeCreateParams = serde_json::from_str(&operation.request_json)?;
        params.validate()?;
        let context = worktree_operation_git_context(&operation)?;
        let repository = self
            .five_track
            .shared
            .git
            .require_repository(&context)
            .map_err(vcs_error)?;
        if repository.root() != operation.repository_root {
            return Err(control_invalid_request(
                "Worktree recovery repository no longer matches the journal.",
            ));
        }

        let outcome = (|| {
            if operation.state == WorktreeOperationState::Completed {
                return Ok(operation.clone());
            }
            if matches!(
                operation.state,
                WorktreeOperationState::Failed | WorktreeOperationState::RollingBack
            ) {
                self.rollback_worktree_resources(&operation, true, true)?;
                operation = self.load_worktree_operation_required(&operation.operation_id)?;
                return Ok(operation.clone());
            }
            if operation.state == WorktreeOperationState::RolledBack {
                return Ok(operation.clone());
            }

            if operation.state == WorktreeOperationState::Prepared {
                let existing = self
                    .five_track
                    .shared
                    .git
                    .list_worktrees(&repository)
                    .map_err(vcs_error)?
                    .into_iter()
                    .find(|worktree| {
                        worktree_paths_equal(
                            repository.host(),
                            worktree.path(),
                            &operation.worktree_path,
                        )
                    });
                if existing.is_none() {
                    let allowed_root = worktree_allowed_root(repository.root())?;
                    let destination = self
                        .five_track
                        .shared
                        .git
                        .verify_worktree_destination(
                            &repository,
                            &allowed_root,
                            &operation.worktree_path,
                        )
                        .map_err(vcs_error)?;
                    self.five_track
                        .shared
                        .git
                        .create_worktree(
                            &repository,
                            &destination,
                            &params.branch,
                            params.base_revision.as_deref(),
                            params.create_branch,
                        )
                        .map_err(vcs_error)?;
                } else if existing
                    .as_ref()
                    .and_then(|worktree| worktree.branch())
                    .is_some_and(|branch| {
                        branch != params.branch && branch != format!("refs/heads/{}", params.branch)
                    })
                {
                    return Err(control_invalid_request(
                        "The journal path is registered to a different Git branch.",
                    ));
                }
                operation = self.update_worktree_ownership(&operation, |ownership| {
                    ownership.worktree_owned = true;
                })?;
                operation = self.transition_worktree(
                    &operation,
                    WorktreeOperationState::WorktreeCreated,
                    None,
                )?;
            }

            if operation.state == WorktreeOperationState::WorktreeCreated {
                let workspace_id = self
                    .find_managed_worktree_workspace(&operation)?
                    .unwrap_or(self.create_managed_worktree_workspace(&operation, &params)?);
                operation = self.update_worktree_resources(
                    &operation,
                    Some(&workspace_id),
                    None,
                    None,
                    |ownership| ownership.workspace_owned = true,
                )?;
                operation = self.transition_worktree(
                    &operation,
                    WorktreeOperationState::WorkspaceCreated,
                    None,
                )?;
            }

            if operation.state == WorktreeOperationState::WorkspaceCreated {
                let placement =
                    if let Some(existing) = self.find_managed_worktree_session(&operation)? {
                        existing
                    } else {
                        self.spawn_managed_worktree_terminal(&operation, &params)?
                    };
                operation = self.update_worktree_resources(
                    &operation,
                    operation.workspace_id.as_deref(),
                    placement.surface_id.as_deref(),
                    placement.session_id.as_deref(),
                    |ownership| {
                        ownership.session_owned = placement.session_id.is_some();
                        ownership.pane_id = Some(placement.pane_id.clone());
                    },
                )?;
                operation = self.transition_worktree(
                    &operation,
                    WorktreeOperationState::SessionCreated,
                    None,
                )?;
            }

            if operation.state == WorktreeOperationState::SessionCreated {
                operation =
                    self.transition_worktree(&operation, WorktreeOperationState::Completed, None)?;
            }
            Ok(operation.clone())
        })();

        match outcome {
            Ok(operation) => worktree_result(&operation, false, false),
            Err(error) => {
                let message = error.to_string();
                let failed = self
                    .mark_worktree_failed(&operation, &message)
                    .unwrap_or(operation);
                let _ = self.rollback_worktree_resources(&failed, true, true);
                Err(error)
            }
        }
    }

    fn load_worktree_operation_required(
        &self,
        operation_id: &str,
    ) -> Result<PersistedWorktreeOperation, DesktopHostError> {
        self.store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .load_worktree_operation(operation_id)?
            .ok_or_else(|| control_invalid_request("Worktree operation not found."))
    }

    fn transition_worktree(
        &self,
        operation: &PersistedWorktreeOperation,
        state: WorktreeOperationState,
        error: Option<&str>,
    ) -> Result<PersistedWorktreeOperation, DesktopHostError> {
        self.store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .transition_worktree_operation(
                &operation.operation_id,
                state,
                error.map(|_| "operation_failed"),
                error,
                &timestamp(),
            )
            .map_err(DesktopHostError::from)
    }

    fn mark_worktree_failed(
        &self,
        operation: &PersistedWorktreeOperation,
        message: &str,
    ) -> Result<PersistedWorktreeOperation, DesktopHostError> {
        if matches!(
            operation.state,
            WorktreeOperationState::Completed | WorktreeOperationState::RolledBack
        ) {
            return Ok(operation.clone());
        }
        self.transition_worktree(operation, WorktreeOperationState::Failed, Some(message))
    }

    fn update_worktree_ownership(
        &self,
        operation: &PersistedWorktreeOperation,
        update: impl FnOnce(&mut WorktreeOwnership),
    ) -> Result<PersistedWorktreeOperation, DesktopHostError> {
        self.update_worktree_resources(operation, None, None, None, update)
    }

    fn update_worktree_resources(
        &self,
        operation: &PersistedWorktreeOperation,
        workspace_id: Option<&str>,
        surface_id: Option<&str>,
        session_id: Option<&str>,
        update: impl FnOnce(&mut WorktreeOwnership),
    ) -> Result<PersistedWorktreeOperation, DesktopHostError> {
        let mut ownership = worktree_ownership(operation)?;
        update(&mut ownership);
        let ownership_json = serde_json::to_string(&ownership)?;
        self.store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .update_worktree_operation_resources(
                &operation.operation_id,
                workspace_id,
                surface_id,
                session_id,
                Some(&ownership_json),
                &timestamp(),
            )
            .map_err(DesktopHostError::from)
    }

    fn find_managed_worktree_workspace(
        &self,
        operation: &PersistedWorktreeOperation,
    ) -> Result<Option<String>, DesktopHostError> {
        let marker = managed_worktree_marker(&operation.operation_id);
        let store = self.store.lock().map_err(|_| {
            DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
        })?;
        Ok(store
            .list_workspaces()?
            .into_iter()
            .find(|workspace| {
                workspace.description.as_deref() == Some(marker.as_str())
                    && paths_equal(&workspace.project_root, &operation.worktree_path)
            })
            .map(|workspace| workspace.workspace_id))
    }

    fn create_managed_worktree_workspace(
        &self,
        operation: &PersistedWorktreeOperation,
        params: &AgentWorktreeCreateParams,
    ) -> Result<String, DesktopHostError> {
        let now = timestamp();
        let workspace_id = WorkspaceId::new().to_string();
        let pane_id = PaneId::new().to_string();
        let source = self.load_workspace_or_not_found(&params.workspace_id)?;
        let bundle = WorkspaceBundle {
            workspace: PersistedWorkspace {
                workspace_id: workspace_id.clone(),
                name: format!("{} [{}]", source.workspace.name, params.branch),
                root_pane_id: pane_id.clone(),
                active_pane_id: pane_id.clone(),
                project_root: Some(operation.worktree_path.clone()),
                environment_profile_id: params
                    .backend_profile
                    .clone()
                    .or(source.workspace.environment_profile_id),
                description: Some(managed_worktree_marker(&operation.operation_id)),
                icon: source.workspace.icon,
                color: source.workspace.color,
                default_wsl_distribution: source.workspace.default_wsl_distribution,
                default_terminal_profile: source.workspace.default_terminal_profile,
                default_agent_command: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            },
            panes: vec![PersistedPane {
                pane_id,
                workspace_id: workspace_id.clone(),
                parent_pane_id: None,
                kind: "leaf".to_string(),
                split_axis: None,
                split_ratio: None,
                mounted_surface_id: None,
                last_focused_at: Some(now.clone()),
                created_at: now.clone(),
                updated_at: now,
            }],
            surfaces: Vec::new(),
            sessions: Vec::new(),
        };
        self.store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .save_workspace_bundle(&bundle)?;
        Ok(workspace_id)
    }

    fn find_managed_worktree_session(
        &self,
        operation: &PersistedWorktreeOperation,
    ) -> Result<Option<TerminalPlacementResult>, DesktopHostError> {
        let Some(workspace_id) = operation.workspace_id.as_deref() else {
            return Ok(None);
        };
        let bundle = self.load_workspace_or_not_found(workspace_id)?;
        let Some(session) = bundle.sessions.first() else {
            return Ok(None);
        };
        let surface = bundle
            .surfaces
            .iter()
            .find(|surface| surface.session_id.as_deref() == Some(session.session_id.as_str()));
        let pane = surface.and_then(|surface| {
            bundle.panes.iter().find(|pane| {
                pane.mounted_surface_id.as_deref() == Some(surface.surface_id.as_str())
            })
        });
        Ok(Some(TerminalPlacementResult {
            workspace_id: workspace_id.to_string(),
            source_pane_id: None,
            pane_id: pane
                .map(|pane| pane.pane_id.clone())
                .unwrap_or(bundle.workspace.active_pane_id),
            surface_id: surface.map(|surface| surface.surface_id.clone()),
            session_id: Some(session.session_id.clone()),
            backend: Some(session.backend_kind.clone()),
            backend_profile: None,
            cwd: session.cwd.clone(),
            columns: 120,
            rows: 30,
            rolled_back: false,
        }))
    }

    fn spawn_managed_worktree_terminal(
        &self,
        operation: &PersistedWorktreeOperation,
        params: &AgentWorktreeCreateParams,
    ) -> Result<TerminalPlacementResult, DesktopHostError> {
        let workspace_id = operation
            .workspace_id
            .clone()
            .ok_or_else(|| control_invalid_request("Managed workspace is missing."))?;
        let bundle = self.load_workspace_or_not_found(&workspace_id)?;
        let cwd = validated_worktree_cwd(&operation.worktree_path, params.cwd.as_deref())?;
        self.spawn_terminal_placement(
            TerminalOpenParams {
                workspace_id,
                pane_id: Some(bundle.workspace.active_pane_id),
                backend: params.backend.clone(),
                backend_profile: params.backend_profile.clone(),
                command: params.command.clone(),
                cwd: Some(cwd),
                env: Vec::new(),
                columns: Some(120),
                rows: Some(30),
                durability: None,
                placement: Some("active_pane".to_string()),
            },
            None,
        )
        .map_err(|failure| failure.error)
    }

    fn rollback_worktree_resources(
        &self,
        operation: &PersistedWorktreeOperation,
        force: bool,
        transition_state: bool,
    ) -> Result<(), DesktopHostError> {
        ensure_agentmux_owned_operation(operation)?;
        let ownership = worktree_ownership(operation)?;
        if transition_state
            && !matches!(
                operation.state,
                WorktreeOperationState::RollingBack | WorktreeOperationState::RolledBack
            )
        {
            let _ =
                self.transition_worktree(operation, WorktreeOperationState::RollingBack, None)?;
        }

        let mut errors = Vec::new();
        if ownership.session_owned {
            if let Some(session_id) = operation.session_id.as_deref() {
                self.terminate_runtime_session(session_id, TerminationMode::Kill);
            }
        }
        if ownership.workspace_owned {
            if let Some(workspace_id) = operation.workspace_id.as_deref() {
                let verified = self
                    .store
                    .lock()
                    .ok()
                    .and_then(|store| store.load_workspace_bundle(workspace_id).ok().flatten())
                    .is_none_or(|bundle| {
                        bundle.workspace.description.as_deref()
                            == Some(managed_worktree_marker(&operation.operation_id).as_str())
                            && paths_equal(&bundle.workspace.project_root, &operation.worktree_path)
                    });
                if verified {
                    if let Ok(mut store) = self.store.lock() {
                        if let Err(error) = store.delete_workspace(workspace_id) {
                            errors.push(error.to_string());
                        }
                    }
                } else {
                    errors.push("managed workspace ownership marker no longer matches".to_string());
                }
            }
        }
        if ownership.worktree_owned {
            match self.remove_owned_worktree(operation, force) {
                Ok(()) => {}
                Err(error) => errors.push(error.to_string()),
            }
        }

        if !errors.is_empty() {
            let detail = errors.join("; ");
            if let Ok(mut store) = self.store.lock() {
                let _ = store.record_worktree_operation_recovery(
                    &operation.operation_id,
                    &serde_json::json!({ "rollback_errors": errors }).to_string(),
                    Some("rollback_failed"),
                    Some(&detail),
                    &timestamp(),
                );
                if transition_state {
                    let _ = store.transition_worktree_operation(
                        &operation.operation_id,
                        WorktreeOperationState::Failed,
                        Some("rollback_failed"),
                        Some(&detail),
                        &timestamp(),
                    );
                }
            }
            return Err(DesktopHostError::StateUnavailable(detail));
        }
        if transition_state {
            let current = self.load_worktree_operation_required(&operation.operation_id)?;
            if current.state != WorktreeOperationState::RolledBack {
                self.transition_worktree(&current, WorktreeOperationState::RolledBack, None)?;
            }
        }
        Ok(())
    }

    fn remove_owned_worktree(
        &self,
        operation: &PersistedWorktreeOperation,
        force: bool,
    ) -> Result<(), DesktopHostError> {
        let context = worktree_operation_git_context(operation)?;
        let repository = self
            .five_track
            .shared
            .git
            .require_repository(&context)
            .map_err(vcs_error)?;
        let registered = self
            .five_track
            .shared
            .git
            .list_worktrees(&repository)
            .map_err(vcs_error)?
            .into_iter()
            .any(|worktree| {
                worktree_paths_equal(repository.host(), worktree.path(), &operation.worktree_path)
            });
        if registered {
            self.five_track
                .shared
                .git
                .remove_worktree(&repository, &operation.worktree_path, force)
                .map_err(vcs_error)?;
        }
        Ok(())
    }
}

fn ensure_same_worktree_request(
    operation: &PersistedWorktreeOperation,
    request_json: &str,
) -> Result<(), DesktopHostError> {
    if operation.request_json != request_json {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::Conflict,
            "The idempotency key is already bound to a different worktree request.",
        )));
    }
    Ok(())
}

fn ensure_agentmux_owned_operation(
    operation: &PersistedWorktreeOperation,
) -> Result<(), DesktopHostError> {
    if operation.owner_kind != "agentmux.desktop" {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::Unauthorized,
            "Worktree operation is not owned by this AgentMux desktop host.",
        )));
    }
    let ownership = worktree_ownership(operation)?;
    if ownership.source_workspace_id.is_empty() {
        return Err(control_invalid_request(
            "Worktree ownership metadata is invalid.",
        ));
    }
    Ok(())
}

fn worktree_ownership(
    operation: &PersistedWorktreeOperation,
) -> Result<WorktreeOwnership, DesktopHostError> {
    serde_json::from_str(&operation.ownership_json).map_err(DesktopHostError::from)
}

fn worktree_result(
    operation: &PersistedWorktreeOperation,
    reused: bool,
    recovered: bool,
) -> Result<AgentWorktreeResult, DesktopHostError> {
    let ownership = worktree_ownership(operation)?;
    Ok(AgentWorktreeResult {
        operation_id: operation.operation_id.clone(),
        worktree_id: operation.operation_id.clone(),
        workspace_id: operation
            .workspace_id
            .clone()
            .unwrap_or(ownership.source_workspace_id),
        branch: operation.branch_name.clone().unwrap_or_default(),
        path: operation.worktree_path.clone(),
        state: operation.state.as_str().to_string(),
        surface_id: operation.surface_id.clone(),
        pane_id: ownership.pane_id,
        session_id: operation.session_id.clone(),
        reused,
        recovered,
    })
}

fn worktree_operation_git_context(
    operation: &PersistedWorktreeOperation,
) -> Result<GitContext, DesktopHostError> {
    let ownership = worktree_ownership(operation)?;
    match ownership.repository_host.as_str() {
        "native" => Ok(GitContext::native(operation.repository_root.clone())),
        "wsl" => Ok(GitContext::wsl(
            operation.repository_root.clone(),
            ownership.distribution,
        )),
        _ => Err(control_invalid_request("Unknown worktree repository host.")),
    }
}

fn worktree_allowed_root(repository_root: &str) -> Result<String, DesktopHostError> {
    if repository_root.starts_with('/') {
        let trimmed = repository_root.trim_end_matches('/');
        let parent = trimmed
            .rsplit_once('/')
            .map(|(parent, _)| if parent.is_empty() { "/" } else { parent })
            .ok_or_else(|| control_invalid_request("Repository has no safe worktree parent."))?;
        return Ok(parent.to_string());
    }
    Path::new(repository_root)
        .parent()
        .filter(|parent| parent.parent().is_some())
        .map(|parent| parent.to_string_lossy().to_string())
        .ok_or_else(|| control_invalid_request("Repository has no safe worktree parent."))
}

fn validated_worktree_cwd(
    worktree_path: &str,
    requested: Option<&str>,
) -> Result<String, DesktopHostError> {
    let Some(requested) = requested.map(str::trim).filter(|cwd| !cwd.is_empty()) else {
        return Ok(worktree_path.to_string());
    };
    if worktree_path.starts_with('/') {
        let root = worktree_path.trim_end_matches('/');
        if requested == root || requested.starts_with(&format!("{root}/")) {
            return Ok(requested.to_string());
        }
    } else {
        let root = PathBuf::from(worktree_path);
        let requested_path = PathBuf::from(requested);
        if requested_path.starts_with(&root) {
            return Ok(requested.to_string());
        }
    }
    Err(control_invalid_request(
        "Worktree terminal cwd must stay inside the managed worktree.",
    ))
}

fn worktree_paths_equal(host: &VcsGitHost, left: &str, right: &str) -> bool {
    match host {
        VcsGitHost::Native => left
            .replace('/', "\\")
            .eq_ignore_ascii_case(&right.replace('/', "\\")),
        VcsGitHost::Wsl { .. } => left.trim_end_matches('/') == right.trim_end_matches('/'),
    }
}

fn paths_equal(value: &Option<String>, expected: &str) -> bool {
    value.as_deref().is_some_and(|value| {
        value
            .replace('/', "\\")
            .trim_end_matches('\\')
            .eq_ignore_ascii_case(expected.replace('/', "\\").trim_end_matches('\\'))
    })
}

fn managed_worktree_marker(operation_id: &str) -> String {
    format!("agentmux-worktree:{operation_id}")
}

fn managed_worktree_operation_id(description: &Option<String>) -> Option<String> {
    description
        .as_deref()?
        .strip_prefix("agentmux-worktree:")
        .map(str::to_string)
}

impl DesktopControlState {
    fn handle_git_review_thread_list(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitReviewThreadListParams = request.parse_params()?;
        params.validate()?;
        let (_, repository) = self.resolve_vcs_repository(&params.workspace_id)?;
        let repository_id = repository_id(&repository);
        validate_repository_selector(params.repository_id.as_deref(), &repository_id)?;
        let store = self.store.lock().map_err(|_| {
            DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
        })?;
        let mut threads = store
            .list_git_review_threads(
                repository.root(),
                Some(&params.workspace_id),
                params.include_stale,
            )?
            .into_iter()
            .filter(|thread| params.include_resolved || thread.resolved_at.is_none())
            .filter(|thread| {
                params
                    .path
                    .as_deref()
                    .is_none_or(|path| thread.path == path)
            })
            .collect::<Vec<_>>();
        if let Some(limit) = params.limit {
            threads.truncate(limit);
        }
        let results = threads
            .iter()
            .map(|thread| review_thread_result(&store, thread, &repository_id))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &GitReviewThreadListResult { threads: results },
        ))
    }

    fn handle_git_review_thread_create(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitReviewThreadCreateParams = request.parse_params()?;
        params.validate()?;
        validate_git_relative_path(&params.anchor.path)?;
        let (_, repository) = self.resolve_vcs_repository(&params.workspace_id)?;
        let repository_id = repository_id(&repository);
        validate_repository_selector(params.repository_id.as_deref(), &repository_id)?;
        let now = timestamp();
        let thread_id = format!("review_{}", unique_time_id());
        let comment_id = format!("comment_{}", unique_time_id());
        let author_id = params
            .author_session_id
            .clone()
            .unwrap_or_else(|| "user".to_string());
        let thread = PersistedGitReviewThread {
            thread_id: thread_id.clone(),
            repository_root: repository.root().to_string(),
            workspace_id: Some(params.workspace_id),
            diff_identity: review_diff_identity(&repository_id, &params.anchor),
            path: params.anchor.path.clone(),
            hunk_id: params.anchor.hunk_header.clone(),
            side: params.anchor.side.clone(),
            line_number: Some(i64::from(params.anchor.line)),
            line_anchor: serde_json::to_string(&params.anchor)?,
            stale: false,
            stale_reason: None,
            resolved_at: None,
            author_id: author_id.clone(),
            target_kind: None,
            target_id: None,
            delivery_state: "draft".to_string(),
            delivery_error: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let comment = PersistedGitReviewComment {
            comment_id,
            thread_id: thread_id.clone(),
            author_id,
            body: params.body,
            target_kind: None,
            target_id: None,
            delivery_state: "draft".to_string(),
            delivery_error: None,
            delivered_at: None,
            created_at: now.clone(),
            updated_at: now,
        };
        let mut store = self.store.lock().map_err(|_| {
            DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
        })?;
        store.upsert_git_review_thread(&thread)?;
        if let Err(error) = store.upsert_git_review_comment(&comment) {
            let _ = store.delete_git_review_thread(&thread_id);
            return Err(error.into());
        }
        let result = review_thread_result(&store, &thread, &repository_id)?;
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn handle_git_review_thread_update(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitReviewThreadUpdateParams = request.parse_params()?;
        params.validate()?;
        let mut store = self.store.lock().map_err(|_| {
            DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
        })?;
        let mut thread = store
            .load_git_review_thread(&params.thread_id)?
            .ok_or_else(|| control_invalid_request("Git review thread not found."))?;
        let repository_id = review_repository_id(&thread);
        if let Some(anchor) = params.anchor {
            validate_git_relative_path(&anchor.path)?;
            thread.diff_identity = review_diff_identity(&repository_id, &anchor);
            thread.path = anchor.path.clone();
            thread.hunk_id = anchor.hunk_header.clone();
            thread.side = anchor.side.clone();
            thread.line_number = Some(i64::from(anchor.line));
            thread.line_anchor = serde_json::to_string(&anchor)?;
            thread.stale = false;
            thread.stale_reason = None;
        }
        if let Some(resolved) = params.resolved {
            thread.resolved_at = resolved.then(timestamp);
        }
        thread.updated_at = timestamp();
        store.upsert_git_review_thread(&thread)?;
        let result = review_thread_result(&store, &thread, &repository_id)?;
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn handle_git_review_thread_delete(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitReviewThreadIdParams = request.parse_params()?;
        params.validate()?;
        let deleted = self
            .store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .delete_git_review_thread(&params.thread_id)?;
        if !deleted {
            return Err(control_invalid_request("Git review thread not found."));
        }
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_git_review_thread_mark_stale(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitReviewThreadMarkStaleParams = request.parse_params()?;
        params.validate()?;
        let mut store = self.store.lock().map_err(|_| {
            DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
        })?;
        let mut thread = store
            .load_git_review_thread(&params.thread_id)?
            .ok_or_else(|| control_invalid_request("Git review thread not found."))?;
        thread.stale = params.stale;
        thread.stale_reason = if params.stale { params.reason } else { None };
        thread.updated_at = timestamp();
        store.upsert_git_review_thread(&thread)?;
        let repository_id = review_repository_id(&thread);
        let result = review_thread_result(&store, &thread, &repository_id)?;
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn handle_git_review_comment_list(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitReviewCommentListParams = request.parse_params()?;
        params.validate()?;
        let store = self.store.lock().map_err(|_| {
            DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
        })?;
        if store.load_git_review_thread(&params.thread_id)?.is_none() {
            return Err(control_invalid_request("Git review thread not found."));
        }
        let mut comments = store.list_git_review_comments(&params.thread_id)?;
        if let Some(limit) = params.limit {
            comments.truncate(limit);
        }
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &GitReviewCommentListResult {
                comments: comments.iter().map(review_comment_result).collect(),
            },
        ))
    }

    fn handle_git_review_comment_create(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitReviewCommentCreateParams = request.parse_params()?;
        params.validate()?;
        let now = timestamp();
        let mut store = self.store.lock().map_err(|_| {
            DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
        })?;
        if store.load_git_review_thread(&params.thread_id)?.is_none() {
            return Err(control_invalid_request("Git review thread not found."));
        }
        let comment = PersistedGitReviewComment {
            comment_id: format!("comment_{}", unique_time_id()),
            thread_id: params.thread_id,
            author_id: params
                .author_session_id
                .unwrap_or_else(|| "user".to_string()),
            body: params.body,
            target_kind: None,
            target_id: None,
            delivery_state: "draft".to_string(),
            delivery_error: None,
            delivered_at: None,
            created_at: now.clone(),
            updated_at: now,
        };
        store.upsert_git_review_comment(&comment)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &review_comment_result(&comment),
        ))
    }

    fn handle_git_review_comment_update(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitReviewCommentUpdateParams = request.parse_params()?;
        params.validate()?;
        let mut store = self.store.lock().map_err(|_| {
            DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
        })?;
        let mut comment = store
            .load_git_review_comment(&params.comment_id)?
            .ok_or_else(|| control_invalid_request("Git review comment not found."))?;
        comment.body = params.body;
        comment.updated_at = timestamp();
        comment.delivery_state = "draft".to_string();
        comment.delivery_error = None;
        comment.delivered_at = None;
        store.upsert_git_review_comment(&comment)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &review_comment_result(&comment),
        ))
    }

    fn handle_git_review_comment_delete(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitReviewCommentIdParams = request.parse_params()?;
        params.validate()?;
        let deleted = self
            .store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .delete_git_review_comment(&params.comment_id)?;
        if !deleted {
            return Err(control_invalid_request("Git review comment not found."));
        }
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AckResult { ok: true },
        ))
    }

    fn handle_git_review_thread_deliver(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitReviewThreadDeliverParams = request.parse_params()?;
        params.validate()?;
        if !matches!(params.target.as_str(), "terminal" | "mailbox") {
            return Err(control_invalid_request(
                "Review target must be 'terminal' or 'mailbox'.",
            ));
        }
        let target_session_id = params
            .target_session_id
            .as_deref()
            .ok_or_else(|| control_invalid_request("target_session_id is required."))?;
        let (thread, comments) = {
            let store = self.store.lock().map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?;
            let thread = store
                .load_git_review_thread(&params.thread_id)?
                .ok_or_else(|| control_invalid_request("Git review thread not found."))?;
            let comments = store.list_git_review_comments(&params.thread_id)?;
            (thread, comments)
        };
        let body = format_review_delivery(&thread, &comments, params.include_context)?;
        match params.target.as_str() {
            "terminal" => {
                self.send_paste_direct(target_session_id, format!("{body}\r"), true)?;
            }
            "mailbox" => {
                let workspace_id = thread
                    .workspace_id
                    .clone()
                    .ok_or_else(|| control_invalid_request("Review thread has no workspace."))?;
                let envelope = RequestEnvelope::new(
                    format!("{}_mailbox", request.id),
                    "team.message.send",
                    serde_json::to_string(&TeamMessageSendParams {
                        workspace_id,
                        thread_id: Some(thread.thread_id.clone()),
                        from_session_id: None,
                        to_session_id: Some(target_session_id.to_string()),
                        body: body.clone(),
                        kind: Some("git_review".to_string()),
                    })?,
                    self.control_token.clone(),
                );
                let response = self.handle_team_message_send(&envelope)?;
                let _: TeamMessageResult = response_result_json(&response)?;
            }
            _ => unreachable!(),
        }

        let delivered_at = timestamp();
        let update = GitReviewDeliveryUpdate {
            target_kind: Some(params.target.clone()),
            target_id: Some(target_session_id.to_string()),
            delivery_state: "delivered".to_string(),
            delivery_error: None,
            delivered_at: Some(delivered_at.clone()),
            updated_at: delivered_at.clone(),
        };
        let mut store = self.store.lock().map_err(|_| {
            DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
        })?;
        store.update_git_review_thread_delivery(&thread.thread_id, &update)?;
        for comment in &comments {
            store.update_git_review_comment_delivery(&comment.comment_id, &update)?;
        }
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &GitReviewDeliveryResult {
                thread_id: thread.thread_id,
                target: params.target,
                target_session_id: Some(target_session_id.to_string()),
                delivered_at,
            },
        ))
    }
}

fn review_thread_result(
    store: &SqliteStore,
    thread: &PersistedGitReviewThread,
    repository_id: &str,
) -> Result<GitReviewThreadResult, DesktopHostError> {
    let anchor: GitReviewLineAnchor = serde_json::from_str(&thread.line_anchor)?;
    Ok(GitReviewThreadResult {
        thread_id: thread.thread_id.clone(),
        workspace_id: thread.workspace_id.clone().unwrap_or_default(),
        repository_id: repository_id.to_string(),
        anchor,
        resolved: thread.resolved_at.is_some(),
        stale: thread.stale,
        stale_reason: thread.stale_reason.clone(),
        created_at: thread.created_at.clone(),
        updated_at: thread.updated_at.clone(),
        comments: store
            .list_git_review_comments(&thread.thread_id)?
            .iter()
            .map(review_comment_result)
            .collect(),
    })
}

fn review_comment_result(comment: &PersistedGitReviewComment) -> GitReviewCommentResult {
    GitReviewCommentResult {
        comment_id: comment.comment_id.clone(),
        thread_id: comment.thread_id.clone(),
        body: comment.body.clone(),
        author_session_id: (comment.author_id != "user").then(|| comment.author_id.clone()),
        created_at: comment.created_at.clone(),
        updated_at: comment.updated_at.clone(),
    }
}

fn review_diff_identity(repository_id: &str, anchor: &GitReviewLineAnchor) -> String {
    format!(
        "{repository_id}:{:016x}",
        stable_hash(&(
            anchor.path.as_str(),
            anchor.side.as_str(),
            anchor.line,
            anchor.start_line,
            anchor.base_revision.as_deref(),
            anchor.head_revision.as_deref(),
            anchor.hunk_header.as_deref(),
            anchor.diff_hash.as_deref(),
        ))
    )
}

fn review_repository_id(thread: &PersistedGitReviewThread) -> String {
    thread
        .diff_identity
        .split_once(':')
        .map(|(repository_id, _)| repository_id.to_string())
        .unwrap_or_else(|| format!("repo_{:016x}", stable_hash(&thread.repository_root)))
}

fn format_review_delivery(
    thread: &PersistedGitReviewThread,
    comments: &[PersistedGitReviewComment],
    include_context: bool,
) -> Result<String, DesktopHostError> {
    let anchor: GitReviewLineAnchor = serde_json::from_str(&thread.line_anchor)?;
    let mut body = format!(
        "Git review feedback for {}:{} ({})",
        anchor.path, anchor.line, anchor.side
    );
    if include_context {
        if let Some(hunk) = anchor.hunk_header.as_deref() {
            body.push_str(&format!("\nHunk: {hunk}"));
        }
        if let Some(head) = anchor.head_revision.as_deref() {
            body.push_str(&format!("\nRevision: {head}"));
        }
    }
    for comment in comments {
        body.push_str("\n\n- ");
        body.push_str(comment.body.trim());
    }
    Ok(body)
}

impl DesktopControlState {
    fn resolve_hook_target_session(
        &self,
        workspace_id: &str,
        target: &str,
    ) -> Result<PersistedSession, DesktopHostError> {
        let store = self.store.lock().map_err(|_| {
            DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
        })?;
        let session = if let Some(surface_id) = target.strip_prefix("surface:") {
            let bundle = store
                .load_workspace_bundle(workspace_id)?
                .ok_or_else(|| workspace_not_found(workspace_id))?;
            let session_id = bundle
                .surfaces
                .iter()
                .find(|surface| surface.surface_id == surface_id)
                .and_then(|surface| surface.session_id.as_deref())
                .ok_or_else(|| {
                    control_invalid_request("Hook target surface has no terminal session.")
                })?;
            bundle
                .sessions
                .iter()
                .find(|session| session.session_id == session_id)
                .cloned()
        } else if let Some(pane_id) = target.strip_prefix("pane:") {
            let bundle = store
                .load_workspace_bundle(workspace_id)?
                .ok_or_else(|| workspace_not_found(workspace_id))?;
            terminal_session_for_pane(&bundle, pane_id).cloned()
        } else {
            store.load_session(target)?
        }
        .ok_or_else(|| control_invalid_request("Hook target session not found."))?;
        if session.workspace_id != workspace_id {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::Unauthorized,
                "Hook workspace does not own the target session.",
            )));
        }
        Ok(session)
    }

    fn handle_agent_hook_state(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let mut params: AgentHookStateParams = request.parse_params()?;
        params.validate()?;
        let session = self.resolve_hook_target_session(&params.workspace_id, &params.session_id)?;
        params.session_id = session.session_id;
        let provider = hook_provider(&params.source)?;
        let provider_event_key = match provider {
            AgentHookProvider::Claude => "hook_event_name",
            AgentHookProvider::Codex => "event",
        };
        let mut payload = serde_json::Map::new();
        payload.insert(
            provider_event_key.to_string(),
            serde_json::Value::String("agentmux.hook_state".to_string()),
        );
        payload.insert(
            "session_id".to_string(),
            serde_json::Value::String(params.session_id.clone()),
        );
        payload.insert(
            "sequence".to_string(),
            serde_json::Value::Number(params.sequence.into()),
        );
        payload.insert(
            "state".to_string(),
            serde_json::Value::String(params.state.clone()),
        );
        if let Some(reason) = params.reason.clone() {
            payload.insert("reason".to_string(), serde_json::Value::String(reason));
        }
        let payload = serde_json::to_vec(&payload)?;
        let now_ms = unix_time_millis();
        let normalized = self
            .five_track
            .shared
            .hook_normalizer
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable(
                    "agent hook normalizer is unavailable".to_string(),
                )
            })?
            .normalize(provider, &payload, Some(&params.session_id), None, now_ms)
            .map_err(|error| control_invalid_request(error.to_string()))?;
        let Some(normalized) = normalized else {
            return Ok(ResponseEnvelope::ok_typed(
                request.id.clone(),
                &AgentHookStateResult {
                    workspace_id: params.workspace_id,
                    session_id: params.session_id,
                    sequence: params.sequence,
                    state: params.state,
                    accepted: false,
                    deduplicated: true,
                },
            ));
        };
        let state = normalized_hook_state(normalized.state);
        {
            let mut hooks = self.five_track.shared.verified_hooks.lock().map_err(|_| {
                DesktopHostError::StateUnavailable("verified hook state is unavailable".to_string())
            })?;
            if let Some(previous) = hooks.get(&params.session_id) {
                if params.sequence < previous.sequence {
                    return Err(DesktopHostError::Control(ControlError::new(
                        ErrorCode::Conflict,
                        "Agent hook sequence is stale.",
                    )));
                }
                if params.sequence == previous.sequence {
                    if previous.state == state {
                        return Ok(ResponseEnvelope::ok_typed(
                            request.id.clone(),
                            &AgentHookStateResult {
                                workspace_id: params.workspace_id,
                                session_id: params.session_id,
                                sequence: params.sequence,
                                state,
                                accepted: false,
                                deduplicated: true,
                            },
                        ));
                    }
                    return Err(DesktopHostError::Control(ControlError::new(
                        ErrorCode::Conflict,
                        "Agent hook sequence is already bound to another state.",
                    )));
                }
            }
            hooks.insert(
                params.session_id.clone(),
                VerifiedHookRecord {
                    session_id: params.session_id.clone(),
                    sequence: params.sequence,
                    state: state.clone(),
                    reason: params.reason.clone(),
                    telemetry: params.telemetry.clone(),
                },
            );
        }
        self.apply_verified_hook_record(
            &VerifiedHookRecord {
                session_id: params.session_id.clone(),
                sequence: params.sequence,
                state: state.clone(),
                reason: params.reason,
                telemetry: params.telemetry,
            },
            &request.id,
        )?;
        if let Some(handle) = self.app_handle.get() {
            let _ = handle.emit(
                EVENT_AGENT_HOOK_STATE_CHANGED,
                AgentHookStateResult {
                    workspace_id: params.workspace_id.clone(),
                    session_id: params.session_id.clone(),
                    sequence: params.sequence,
                    state: state.clone(),
                    accepted: true,
                    deduplicated: false,
                },
            );
        }
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &AgentHookStateResult {
                workspace_id: params.workspace_id,
                session_id: params.session_id,
                sequence: params.sequence,
                state,
                accepted: true,
                deduplicated: false,
            },
        ))
    }

    fn apply_verified_hook_record(
        &self,
        record: &VerifiedHookRecord,
        request_id: &str,
    ) -> Result<(), DesktopHostError> {
        let mut control = self.control.lock().map_err(|_| {
            DesktopHostError::StateUnavailable("desktop control state is unavailable".to_string())
        })?;
        let response = control.handle_request(RequestEnvelope::new(
            request_id,
            "agent.set_state",
            serde_json::json!({
                "session_id": record.session_id,
                "state": record.state,
                "reason": record.reason,
                "telemetry": record.telemetry,
            })
            .to_string(),
            self.control_token.clone(),
        ));
        self.persist_agent_set_state(&mut control, &response)
    }

    pub(super) fn reassert_verified_hook_states(&self) {
        let hooks = self
            .five_track
            .shared
            .verified_hooks
            .lock()
            .map(|hooks| hooks.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        if hooks.is_empty() {
            return;
        }
        let current = self
            .control
            .lock()
            .map(|control| {
                control
                    .agent_state_snapshot()
                    .into_iter()
                    .map(|state| (state.session_id, state.state))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        for hook in hooks {
            if current.get(&hook.session_id) == Some(&hook.state) {
                continue;
            }
            let _ = self.apply_verified_hook_record(&hook, "desktop_reassert_verified_hook");
        }
    }

    pub(super) fn process_five_track_output(
        &self,
        deltas: &[OutputDelta],
        session_state_updates: &[(String, String, Option<i32>)],
    ) {
        let terminal_sessions = session_state_updates
            .iter()
            .filter(|(_, state, _)| is_terminal_state(state))
            .map(|(session_id, _, _)| session_id.clone())
            .collect::<HashSet<_>>();
        if !terminal_sessions.is_empty() {
            if let Ok(mut hooks) = self.five_track.shared.verified_hooks.lock() {
                hooks.retain(|session_id, _| !terminal_sessions.contains(session_id));
            }
        }

        let now_ms = unix_time_millis();
        let mut detected = Vec::new();
        for delta in deltas {
            let session_id = delta.session_id.to_string();
            let metadata = PtyStreamMetadata {
                session_id: session_id.clone(),
                process_id: None,
            };
            let found = self
                .five_track
                .shared
                .url_detectors
                .lock()
                .ok()
                .map(|mut detectors| {
                    detectors
                        .entry(session_id)
                        .or_insert_with(|| PtyUrlDetector::new(PtyUrlDetectorConfig::default()))
                        .push(&delta.bytes, &metadata, now_ms)
                })
                .unwrap_or_default();
            detected.extend(found);
        }
        for session_id in &terminal_sessions {
            let metadata = PtyStreamMetadata {
                session_id: session_id.clone(),
                process_id: None,
            };
            let found = self
                .five_track
                .shared
                .url_detectors
                .lock()
                .ok()
                .and_then(|mut detectors| {
                    detectors
                        .remove(session_id)
                        .map(|mut detector| detector.finish(&metadata, now_ms))
                })
                .unwrap_or_default();
            detected.extend(found);
        }
        for candidate in detected {
            let workspace_id = self
                .store
                .lock()
                .ok()
                .and_then(|store| store.load_session(&candidate.session_id).ok().flatten())
                .map(|session| session.workspace_id);
            let Some(workspace_id) = workspace_id else {
                continue;
            };
            let params = DevelopmentServerCandidateParams {
                workspace_id,
                session_id: candidate.session_id,
                url: candidate.origin,
                source: "pty".to_string(),
                detected_at: timestamp(),
                process_id: candidate.process_id,
            };
            let _ = self.record_dev_server_candidate(params);
        }
    }

    fn handle_dev_server_candidate_detected(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: DevelopmentServerCandidateParams = request.parse_params()?;
        params.validate()?;
        let result = self.record_dev_server_candidate(params)?;
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn record_dev_server_candidate(
        &self,
        params: DevelopmentServerCandidateParams,
    ) -> Result<DevelopmentServerCandidateResult, DesktopHostError> {
        params.validate()?;
        if !is_safe_development_server_url(&params.url) {
            return Err(control_invalid_request(
                "Development server URL must use http or https without credentials.",
            ));
        }
        let session = self
            .store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .load_session(&params.session_id)?
            .ok_or_else(|| control_invalid_request("Development server session not found."))?;
        if session.workspace_id != params.workspace_id {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::Unauthorized,
                "Development server workspace does not own the session.",
            )));
        }
        let mut candidates = self
            .five_track
            .shared
            .dev_server_candidates
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable(
                    "development server candidates are unavailable".to_string(),
                )
            })?;
        if let Some(existing) = candidates.iter().find(|candidate| {
            candidate.session_id == params.session_id
                && candidate.url == params.url
                && !candidate.dismissed
        }) {
            return Ok(existing.clone());
        }
        while candidates
            .iter()
            .filter(|candidate| candidate.session_id == params.session_id)
            .count()
            >= MAX_DEV_SERVER_CANDIDATES_PER_SESSION
        {
            if let Some(index) = candidates
                .iter()
                .position(|candidate| candidate.session_id == params.session_id)
            {
                candidates.remove(index);
            } else {
                break;
            }
        }
        while candidates.len() >= MAX_DEV_SERVER_CANDIDATES {
            candidates.pop_front();
        }
        let result = DevelopmentServerCandidateResult {
            candidate_id: format!("devsrv_{}", unique_time_id()),
            workspace_id: params.workspace_id.clone(),
            session_id: params.session_id.clone(),
            url: params.url,
            source: params.source,
            detected_at: params.detected_at,
            process_id: params.process_id,
            dismissed: false,
            opened_surface_id: None,
        };
        candidates.push_back(result.clone());
        drop(candidates);

        let notification = PersistedNotification {
            notification_id: format!("not_{}", result.candidate_id),
            notification_type: "dev_server.detected".to_string(),
            severity: "info".to_string(),
            workspace_id: Some(result.workspace_id.clone()),
            session_id: Some(result.session_id.clone()),
            title: "Development server detected".to_string(),
            message: result.url.clone(),
            created_at: result.detected_at.clone(),
            dismissed: false,
        };
        if let Ok(mut store) = self.store.lock() {
            let _ = store.upsert_notification(&notification);
        }
        self.dispatch_desktop_notification(&notification_result_from_persisted(&notification));
        if let Some(handle) = self.app_handle.get() {
            let _ = handle.emit(EVENT_DEV_SERVER_CANDIDATE_DETECTED, result.clone());
        }
        Ok(result)
    }

    fn handle_dev_server_candidate_list(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: DevelopmentServerCandidateListParams = request.parse_params()?;
        params.validate()?;
        let candidates = self
            .five_track
            .shared
            .dev_server_candidates
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable(
                    "development server candidates are unavailable".to_string(),
                )
            })?;
        let mut result = candidates
            .iter()
            .rev()
            .filter(|candidate| {
                params
                    .workspace_id
                    .as_deref()
                    .is_none_or(|workspace_id| candidate.workspace_id == workspace_id)
                    && params
                        .session_id
                        .as_deref()
                        .is_none_or(|session_id| candidate.session_id == session_id)
                    && (params.include_dismissed || !candidate.dismissed)
            })
            .cloned()
            .collect::<Vec<_>>();
        result.truncate(params.limit.unwrap_or(100));
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &DevelopmentServerCandidateListResult { candidates: result },
        ))
    }

    fn handle_dev_server_candidate_dismiss(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: DevelopmentServerCandidateDismissParams = request.parse_params()?;
        params.validate()?;
        let mut candidates = self
            .five_track
            .shared
            .dev_server_candidates
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable(
                    "development server candidates are unavailable".to_string(),
                )
            })?;
        let candidate = candidates
            .iter_mut()
            .find(|candidate| candidate.candidate_id == params.candidate_id)
            .ok_or_else(|| control_invalid_request("Development server candidate not found."))?;
        candidate.dismissed = true;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &DevelopmentServerCandidateDismissResult {
                candidate_id: params.candidate_id,
                dismissed: true,
            },
        ))
    }

    fn handle_dev_server_candidate_open_in_split(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: DevelopmentServerCandidateOpenInSplitParams = request.parse_params()?;
        params.validate()?;
        let candidate = self
            .five_track
            .shared
            .dev_server_candidates
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable(
                    "development server candidates are unavailable".to_string(),
                )
            })?
            .iter()
            .find(|candidate| candidate.candidate_id == params.candidate_id)
            .cloned()
            .ok_or_else(|| control_invalid_request("Development server candidate not found."))?;
        if candidate.dismissed {
            return Err(control_invalid_request(
                "Dismissed development server candidates cannot be opened.",
            ));
        }
        let axis = params.axis.as_deref().unwrap_or("vertical");
        if !matches!(axis, "horizontal" | "vertical") {
            return Err(control_invalid_request(
                "Development server split axis must be horizontal or vertical.",
            ));
        }
        let ratio = params.ratio.unwrap_or(0.5);
        if !(0.1..=0.9).contains(&ratio) {
            return Err(control_invalid_request(
                "Development server split ratio must be between 0.1 and 0.9.",
            ));
        }
        let (original, target_pane_id) = {
            let mut store = self.store.lock().map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?;
            let mut bundle = store
                .load_workspace_bundle(&candidate.workspace_id)?
                .ok_or_else(|| workspace_not_found(&candidate.workspace_id))?;
            let original = bundle.clone();
            let source_pane_id = params
                .pane_id
                .clone()
                .unwrap_or_else(|| bundle.workspace.active_pane_id.clone());
            let target = split_pane_in_bundle(&mut bundle, &source_pane_id, axis, ratio)?;
            store.save_workspace_bundle(&bundle)?;
            (original, target)
        };

        let mut created_surface_id = None;
        let opened = (|| {
            let create = RequestEnvelope::new(
                format!("{}_create_browser", request.id),
                "surface.create_browser",
                serde_json::to_string(&SurfaceCreateBrowserParams {
                    workspace_id: candidate.workspace_id.clone(),
                    pane_id: Some(target_pane_id.clone()),
                    profile: None,
                    placement: Some("active_pane".to_string()),
                })?,
                self.control_token.clone(),
            );
            let response = self.handle_surface_create_browser(&create)?;
            let surface: SurfaceSummaryResult = response_result_json(&response)?;
            created_surface_id = Some(surface.surface_id.clone());
            let navigate = RequestEnvelope::new(
                format!("{}_navigate", request.id),
                "browser.navigate",
                serde_json::to_string(&BrowserNavigateParams {
                    surface_id: surface.surface_id.clone(),
                    url: candidate.url.clone(),
                })?,
                self.control_token.clone(),
            );
            let response = self.handle_browser_navigate(&navigate)?;
            let _: BrowserNavigationResult = response_result_json(&response)?;
            Ok::<_, DesktopHostError>(surface)
        })();
        let surface = match opened {
            Ok(surface) => surface,
            Err(error) => {
                if let Some(surface_id) = created_surface_id.as_deref() {
                    let _ = self.close_browser_surface_if_present(surface_id);
                }
                if let Ok(mut store) = self.store.lock() {
                    let _ = store.save_workspace_bundle(&original);
                }
                return Err(error);
            }
        };
        let mut candidates = self
            .five_track
            .shared
            .dev_server_candidates
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable(
                    "development server candidates are unavailable".to_string(),
                )
            })?;
        let current = candidates
            .iter_mut()
            .find(|entry| entry.candidate_id == candidate.candidate_id)
            .ok_or_else(|| control_invalid_request("Development server candidate disappeared."))?;
        current.opened_surface_id = Some(surface.surface_id.clone());
        let result = current.clone();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &DevelopmentServerCandidateOpenInSplitResult {
                candidate: result,
                pane_id: target_pane_id,
                surface_id: surface.surface_id,
            },
        ))
    }
}

fn hook_provider(source: &str) -> Result<AgentHookProvider, DesktopHostError> {
    let source = source.to_ascii_lowercase();
    if source.contains("claude") {
        Ok(AgentHookProvider::Claude)
    } else if source.contains("codex") {
        Ok(AgentHookProvider::Codex)
    } else {
        Err(control_invalid_request(
            "Hook source must identify Claude or Codex.",
        ))
    }
}

fn normalized_hook_state(state: AgentHookState) -> String {
    match state {
        AgentHookState::Started | AgentHookState::Running => "running",
        AgentHookState::WaitingForInput => "waiting_for_input",
        AgentHookState::Completed => "completed",
        AgentHookState::Failed => "failed",
        AgentHookState::Exited => "detached",
    }
    .to_string()
}

fn unix_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

fn is_safe_development_server_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    let Some(authority) = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"))
    else {
        return false;
    };
    !authority.is_empty() && !authority.contains('@') && !authority.chars().any(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    use agentmux_browser::BrowserAutomationResult;
    use std::process::Command;
    use std::sync::{Arc, Mutex};

    fn decode_ok<T: serde::de::DeserializeOwned>(response: ResponseEnvelope) -> T {
        match response.outcome {
            ResponseOutcome::Ok { result_json } => {
                serde_json::from_str(&result_json).expect("valid typed response")
            }
            ResponseOutcome::Error(error) => panic!("unexpected control error: {error:?}"),
        }
    }

    fn error_code(response: ResponseEnvelope) -> ErrorCode {
        match response.outcome {
            ResponseOutcome::Error(error) => error.code,
            ResponseOutcome::Ok { result_json } => {
                panic!("expected control error, got {result_json}")
            }
        }
    }

    fn test_request<T: serde::Serialize>(id: &str, method: &str, params: &T) -> RequestEnvelope {
        RequestEnvelope::new(
            id,
            method,
            serde_json::to_string(params).expect("serializable request"),
            DESKTOP_CONTROL_TOKEN,
        )
    }

    fn temporary_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("agentmux-five-track-{label}-{}", unique_time_id()))
    }

    fn run_git(repository: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(args)
            .output()
            .expect("git should start");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn create_git_repository(label: &str) -> PathBuf {
        let repository = temporary_path(label);
        fs::create_dir_all(&repository).expect("temporary repository");
        run_git(&repository, &["init", "-q"]);
        run_git(&repository, &["config", "user.name", "AgentMux Test"]);
        run_git(
            &repository,
            &["config", "user.email", "agentmux@example.invalid"],
        );
        fs::write(repository.join("tracked.txt"), "initial\n").expect("tracked fixture");
        run_git(&repository, &["add", "tracked.txt"]);
        run_git(&repository, &["commit", "-q", "-m", "initial"]);
        repository
    }

    fn workspace_bundle(
        workspace_id: &str,
        project_root: Option<String>,
        sessions: &[(&str, &str, &str)],
    ) -> WorkspaceBundle {
        let now = "2026-07-23T00:00:00Z".to_string();
        let mut panes = Vec::new();
        let mut surfaces = Vec::new();
        let mut persisted_sessions = Vec::new();
        for (index, (pane_id, surface_id, session_id)) in sessions.iter().enumerate() {
            panes.push(PersistedPane {
                pane_id: (*pane_id).to_string(),
                workspace_id: workspace_id.to_string(),
                parent_pane_id: None,
                kind: "leaf".to_string(),
                split_axis: None,
                split_ratio: None,
                mounted_surface_id: Some((*surface_id).to_string()),
                last_focused_at: Some(now.clone()),
                created_at: now.clone(),
                updated_at: now.clone(),
            });
            surfaces.push(PersistedSurface {
                surface_id: (*surface_id).to_string(),
                workspace_id: workspace_id.to_string(),
                surface_type: "terminal".to_string(),
                title: format!("terminal-{index}"),
                session_id: Some((*session_id).to_string()),
                browser_id: None,
                created_at: now.clone(),
                last_visible_at: Some(now.clone()),
                updated_at: now.clone(),
            });
            persisted_sessions.push(PersistedSession {
                session_id: (*session_id).to_string(),
                workspace_id: workspace_id.to_string(),
                backend_kind: "conpty".to_string(),
                backend_attachment_id: None,
                backend_native_id: None,
                cwd: project_root.clone(),
                command: vec!["cmd.exe".to_string()],
                state: "running".to_string(),
                exit_code: None,
                durability: "ephemeral".to_string(),
                created_at: now.clone(),
                last_seen_at: Some(now.clone()),
                updated_at: now.clone(),
            });
        }
        if panes.is_empty() {
            panes.push(PersistedPane {
                pane_id: "pane_root".to_string(),
                workspace_id: workspace_id.to_string(),
                parent_pane_id: None,
                kind: "leaf".to_string(),
                split_axis: None,
                split_ratio: None,
                mounted_surface_id: None,
                last_focused_at: Some(now.clone()),
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }
        let root_pane_id = panes[0].pane_id.clone();
        let active_pane_id = panes
            .last()
            .map(|pane| pane.pane_id.clone())
            .unwrap_or_else(|| root_pane_id.clone());
        WorkspaceBundle {
            workspace: PersistedWorkspace {
                workspace_id: workspace_id.to_string(),
                name: workspace_id.to_string(),
                root_pane_id,
                active_pane_id,
                project_root,
                environment_profile_id: None,
                description: None,
                icon: None,
                color: None,
                default_wsl_distribution: None,
                default_terminal_profile: None,
                default_agent_command: None,
                created_at: now.clone(),
                updated_at: now,
            },
            panes,
            surfaces,
            sessions: persisted_sessions,
        }
    }

    fn save_bundle(host: &DesktopControlState, bundle: &WorkspaceBundle) {
        host.store
            .lock()
            .expect("store lock")
            .save_workspace_bundle(bundle)
            .expect("workspace bundle should persist");
    }

    #[test]
    fn repository_monitor_policy_is_change_driven_and_bounded() {
        assert!(STATUS_CACHE_MAX_AGE >= Duration::from_secs(5 * 60));
        assert!(
            REPOSITORY_FALLBACK_STATUS_INTERVAL >= Duration::from_secs(30),
            "fallback Git status must remain low-frequency"
        );
        assert!(MAX_WATCHER_STARTS_PER_TICK <= 2);
        assert_eq!(MAX_FALLBACK_STATUS_READS_PER_TICK, 1);
        assert!(REPOSITORY_EVENT_DEBOUNCE >= Duration::from_millis(200));
    }

    #[test]
    fn git_status_pages_share_one_snapshot_until_change_signal() {
        let repository = create_git_repository("paging");
        fs::write(repository.join("tracked.txt"), "changed\n").expect("modified fixture");
        for index in 0..11 {
            fs::write(repository.join(format!("new-{index:02}.txt")), "new\n")
                .expect("untracked fixture");
        }
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        save_bundle(
            &host,
            &workspace_bundle(
                "ws_git",
                Some(repository.to_string_lossy().to_string()),
                &[],
            ),
        );

        let summary: GitStatusSummaryResult = decode_ok(host.handle_request(test_request(
            "git_summary",
            METHOD_GIT_STATUS_SUMMARY,
            &GitRepositoryParams {
                workspace_id: "ws_git".to_string(),
                repository_id: None,
            },
        )));
        assert_eq!(summary.staged_count, 0);
        assert_eq!(summary.unstaged_count, 12);
        assert_eq!(summary.untracked_count, 11);
        let first: GitStatusPageResult = decode_ok(host.handle_request(test_request(
            "git_page_1",
            METHOD_GIT_STATUS_PAGE,
            &GitStatusPageParams {
                workspace_id: "ws_git".to_string(),
                repository_id: Some(summary.repository_id.clone()),
                generation: Some(summary.generation),
                state: None,
                cursor: None,
                limit: Some(5),
            },
        )));
        assert_eq!(first.changes.len(), 5);
        assert_eq!(first.total_count, Some(12));

        fs::write(repository.join("late.txt"), "late\n").expect("late fixture");
        let second: GitStatusPageResult = decode_ok(host.handle_request(test_request(
            "git_page_2",
            METHOD_GIT_STATUS_PAGE,
            &GitStatusPageParams {
                workspace_id: "ws_git".to_string(),
                repository_id: Some(summary.repository_id.clone()),
                generation: Some(summary.generation),
                state: None,
                cursor: first.next_cursor.clone(),
                limit: Some(5),
            },
        )));
        assert_eq!(second.generation, summary.generation);
        assert_eq!(second.total_count, Some(12));

        let key = format!("ws_git|{}", summary.repository_id);
        signal_repository_changed(&host.five_track.shared, &key, "test", None);
        let refreshed: GitStatusSummaryResult = decode_ok(host.handle_request(test_request(
            "git_summary_refreshed",
            METHOD_GIT_STATUS_SUMMARY,
            &GitRepositoryParams {
                workspace_id: "ws_git".to_string(),
                repository_id: Some(summary.repository_id),
            },
        )));
        assert_eq!(refreshed.staged_count, 0);
        assert_eq!(refreshed.unstaged_count, 13);
        assert_eq!(refreshed.untracked_count, 12);
        assert!(refreshed.generation > summary.generation);

        fs::remove_dir_all(repository).expect("temporary repository cleanup");
    }

    #[test]
    fn five_track_methods_require_the_desktop_control_token() {
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let response = host.handle_request(RequestEnvelope::new(
            "git_unauthorized",
            METHOD_GIT_STATUS_SUMMARY,
            r#"{"workspace_id":"ws_missing","repository_id":null}"#,
            "wrong-token",
        ));
        assert_eq!(error_code(response), ErrorCode::Unauthorized);
    }

    #[test]
    fn worktree_idempotency_and_rollback_require_agentmux_ownership() {
        let ownership = WorktreeOwnership {
            source_workspace_id: "ws_source".to_string(),
            repository_host: "native".to_string(),
            ..WorktreeOwnership::default()
        };
        let operation = PersistedWorktreeOperation {
            operation_id: "worktree_1".to_string(),
            idempotency_key: "key_1".to_string(),
            repository_root: r"D:\repo".to_string(),
            worktree_path: r"D:\repo-worker".to_string(),
            branch_name: Some("agent/worker".to_string()),
            revision: Some("HEAD".to_string()),
            workspace_id: None,
            surface_id: None,
            session_id: None,
            owner_kind: "agentmux.desktop".to_string(),
            owner_id: None,
            ownership_json: serde_json::to_string(&ownership).unwrap(),
            request_json: "{\"request\":1}".to_string(),
            state: WorktreeOperationState::Prepared,
            error_code: None,
            error_message: None,
            recovery_json: "{}".to_string(),
            recovery_attempts: 0,
            last_recovery_at: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            completed_at: None,
            rolled_back_at: None,
        };
        ensure_same_worktree_request(&operation, "{\"request\":1}")
            .expect("same idempotent request");
        assert!(ensure_same_worktree_request(&operation, "{\"request\":2}").is_err());
        ensure_agentmux_owned_operation(&operation).expect("owned operation");
        let mut external = operation;
        external.owner_kind = "external".to_string();
        assert!(matches!(
            ensure_agentmux_owned_operation(&external),
            Err(DesktopHostError::Control(ControlError {
                code: ErrorCode::Unauthorized,
                ..
            }))
        ));
    }

    #[test]
    fn review_threads_deliver_to_the_existing_team_mailbox() {
        let repository = create_git_repository("review");
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        save_bundle(
            &host,
            &workspace_bundle(
                "ws_review",
                Some(repository.to_string_lossy().to_string()),
                &[("pane_worker", "surface_worker", "ses_worker")],
            ),
        );
        let created: GitReviewThreadResult = decode_ok(host.handle_request(test_request(
            "review_create",
            METHOD_GIT_REVIEW_THREAD_CREATE,
            &GitReviewThreadCreateParams {
                workspace_id: "ws_review".to_string(),
                repository_id: None,
                anchor: GitReviewLineAnchor {
                    path: "tracked.txt".to_string(),
                    side: "right".to_string(),
                    line: 1,
                    start_line: None,
                    base_revision: None,
                    head_revision: Some("HEAD".to_string()),
                    hunk_header: Some("@@ -1 +1 @@".to_string()),
                    diff_hash: None,
                },
                body: "Please keep the stable API.".to_string(),
                author_session_id: None,
            },
        )));
        let delivered: GitReviewDeliveryResult = decode_ok(host.handle_request(test_request(
            "review_deliver",
            METHOD_GIT_REVIEW_THREAD_DELIVER,
            &GitReviewThreadDeliverParams {
                thread_id: created.thread_id,
                target: "mailbox".to_string(),
                target_session_id: Some("ses_worker".to_string()),
                include_context: true,
            },
        )));
        assert_eq!(delivered.target, "mailbox");
        let messages = host
            .store
            .lock()
            .unwrap()
            .list_team_messages(Some("ws_review"), true)
            .expect("mailbox messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].kind, "git_review");
        assert_eq!(messages[0].to_session_id.as_deref(), Some("ses_worker"));
        assert!(messages[0].body.contains("Please keep the stable API."));

        fs::remove_dir_all(repository).expect("temporary repository cleanup");
    }

    #[test]
    #[cfg(windows)]
    fn hook_surface_locator_targets_exact_non_active_session_and_rejects_stale_state() {
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let spawn = host.handle_request(RequestEnvelope::new(
            "hook_spawn",
            "session.spawn",
            r#"{"workspace_id":"ws_hooks","command":["cmd.exe","/d","/q"],"cwd":null,"columns":80,"rows":24,"durability":"ephemeral"}"#,
            DESKTOP_CONTROL_TOKEN,
        ));
        let spawned: SessionSpawnResult = decode_ok(spawn);
        let mut bundle = host
            .store
            .lock()
            .unwrap()
            .load_workspace_bundle("ws_hooks")
            .unwrap()
            .expect("spawned workspace");
        let target_surface = bundle
            .surfaces
            .iter()
            .find(|surface| surface.session_id.as_deref() == Some(&spawned.session_id))
            .expect("spawned surface")
            .surface_id
            .clone();
        let active = workspace_bundle(
            "ws_hooks",
            None,
            &[(
                "pane_active_other",
                "surface_active_other",
                "ses_active_other",
            )],
        );
        bundle.panes.extend(active.panes);
        bundle.surfaces.extend(active.surfaces);
        bundle.sessions.extend(active.sessions);
        bundle.workspace.active_pane_id = "pane_active_other".to_string();
        save_bundle(&host, &bundle);

        let accepted: AgentHookStateResult = decode_ok(host.handle_request(test_request(
            "hook_state",
            METHOD_AGENT_HOOK_STATE,
            &AgentHookStateParams {
                workspace_id: "ws_hooks".to_string(),
                session_id: format!("surface:{target_surface}"),
                sequence: 20,
                state: "running".to_string(),
                reason: Some("PreToolUse".to_string()),
                source: "claude_hook".to_string(),
                observed_at: "2026-07-23T00:00:00Z".to_string(),
                telemetry: None,
            },
        )));
        assert_eq!(accepted.session_id, spawned.session_id);
        assert!(accepted.accepted);
        let persisted = host
            .store
            .lock()
            .unwrap()
            .load_agent_state(&spawned.session_id)
            .unwrap()
            .expect("hook state persisted");
        assert_eq!(persisted.state, "running");
        assert!(host
            .store
            .lock()
            .unwrap()
            .load_agent_state("ses_active_other")
            .unwrap()
            .is_none());

        let stale = host.handle_request(test_request(
            "hook_stale",
            METHOD_AGENT_HOOK_STATE,
            &AgentHookStateParams {
                workspace_id: "ws_hooks".to_string(),
                session_id: format!("surface:{target_surface}"),
                sequence: 19,
                state: "completed".to_string(),
                reason: Some("Stop".to_string()),
                source: "claude_hook".to_string(),
                observed_at: "2026-07-23T00:00:01Z".to_string(),
                telemetry: None,
            },
        ));
        assert!(matches!(
            error_code(stale),
            ErrorCode::InvalidRequest | ErrorCode::Conflict
        ));
        let _ = host.handle_request(RequestEnvelope::new(
            "hook_terminate",
            "session.terminate",
            serde_json::json!({"session_id": spawned.session_id, "mode": "kill"}).to_string(),
            DESKTOP_CONTROL_TOKEN,
        ));
    }

    #[test]
    fn raw_output_detects_url_and_opens_browser_in_the_same_workspace() {
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        save_bundle(
            &host,
            &workspace_bundle(
                "ws_url",
                None,
                &[("pane_terminal", "surface_terminal", "ses_url")],
            ),
        );
        host.process_five_track_output(
            &[OutputDelta {
                session_id: SessionId::from_string("ses_url"),
                from_offset: 0,
                bytes: b"ready at http://127.0.0.1:4173\r\n".to_vec(),
            }],
            &[],
        );
        let listed: DevelopmentServerCandidateListResult =
            decode_ok(host.handle_request(test_request(
                "url_list",
                METHOD_DEV_SERVER_CANDIDATE_LIST,
                &DevelopmentServerCandidateListParams {
                    workspace_id: Some("ws_url".to_string()),
                    session_id: Some("ses_url".to_string()),
                    include_dismissed: false,
                    limit: Some(10),
                },
            )));
        assert_eq!(listed.candidates.len(), 1);
        let opened: DevelopmentServerCandidateOpenInSplitResult =
            decode_ok(host.handle_request(test_request(
                "url_open",
                METHOD_DEV_SERVER_CANDIDATE_OPEN_IN_SPLIT,
                &DevelopmentServerCandidateOpenInSplitParams {
                    candidate_id: listed.candidates[0].candidate_id.clone(),
                    pane_id: Some("pane_terminal".to_string()),
                    axis: Some("vertical".to_string()),
                    ratio: Some(0.5),
                },
            )));
        let bundle = host
            .store
            .lock()
            .unwrap()
            .load_workspace_bundle("ws_url")
            .unwrap()
            .expect("workspace after browser split");
        assert!(bundle
            .panes
            .iter()
            .any(|pane| pane.pane_id == opened.pane_id));
        assert!(bundle.surfaces.iter().any(|surface| {
            surface.surface_id == opened.surface_id && surface.workspace_id == "ws_url"
        }));
    }

    struct NavigateFailingBrowser {
        inner: InMemoryBrowserAutomation,
        closed: Arc<Mutex<Vec<String>>>,
    }

    impl BrowserAutomation for NavigateFailingBrowser {
        fn create_surface(
            &mut self,
            surface_id: String,
            workspace_id: String,
            profile: Option<String>,
        ) -> BrowserAutomationResult<BrowserSurface> {
            self.inner.create_surface(surface_id, workspace_id, profile)
        }

        fn surface(&self, surface_id: &str) -> BrowserAutomationResult<BrowserSurface> {
            self.inner.surface(surface_id)
        }

        fn close_surface(&mut self, surface_id: &str) -> BrowserAutomationResult<BrowserSurface> {
            self.closed.lock().unwrap().push(surface_id.to_string());
            self.inner.close_surface(surface_id)
        }

        fn execute(
            &mut self,
            command: BrowserCommand,
        ) -> BrowserAutomationResult<BrowserCommandResult> {
            if matches!(&command, BrowserCommand::Navigate { .. }) {
                return Err(BrowserAutomationError::automation_failed(
                    "injected navigation failure",
                ));
            }
            self.inner.execute(command)
        }
    }

    #[test]
    fn browser_split_navigation_failure_closes_created_surface_and_restores_topology() {
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let original = workspace_bundle(
            "ws_rollback",
            None,
            &[("pane_terminal", "surface_terminal", "ses_rollback")],
        );
        save_bundle(&host, &original);
        let candidate = host
            .record_dev_server_candidate(DevelopmentServerCandidateParams {
                workspace_id: "ws_rollback".to_string(),
                session_id: "ses_rollback".to_string(),
                url: "http://127.0.0.1:3000".to_string(),
                source: "test".to_string(),
                detected_at: "2026-07-23T00:00:00Z".to_string(),
                process_id: None,
            })
            .expect("candidate");
        let closed = Arc::new(Mutex::new(Vec::new()));
        *host.browser.lock().unwrap() = Box::new(NavigateFailingBrowser {
            inner: InMemoryBrowserAutomation::new(),
            closed: Arc::clone(&closed),
        });

        let response = host.handle_request(test_request(
            "rollback_open",
            METHOD_DEV_SERVER_CANDIDATE_OPEN_IN_SPLIT,
            &DevelopmentServerCandidateOpenInSplitParams {
                candidate_id: candidate.candidate_id,
                pane_id: Some("pane_terminal".to_string()),
                axis: Some("vertical".to_string()),
                ratio: Some(0.5),
            },
        ));
        assert_eq!(error_code(response), ErrorCode::BackendDegraded);
        let restored = host
            .store
            .lock()
            .unwrap()
            .load_workspace_bundle("ws_rollback")
            .unwrap()
            .expect("restored workspace");
        assert_eq!(restored, original);
        let closed = closed.lock().unwrap();
        assert_eq!(closed.len(), 1, "new browser surface must be closed");
        assert!(!closed[0].is_empty());
    }

    #[test]
    #[cfg(windows)]
    fn native_repository_watcher_invalidates_generation_on_file_change() {
        let repository_path = create_git_repository("watcher");
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let repository = host
            .five_track
            .shared
            .git
            .require_repository(&GitContext::native(
                repository_path.to_string_lossy().to_string(),
            ))
            .expect("resolved repository");
        let repository_id = repository_id(&repository);
        host.observe_repository("ws_watch", &repository_id, &repository);
        reconcile_observed_repositories(&host.five_track.shared, None);
        let mode = host
            .five_track
            .shared
            .observed_repositories
            .lock()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .monitor_mode;
        assert_eq!(mode, RepositoryMonitorMode::NativeWatcher);
        let generation = host.five_track.shared.git.generation(&repository);
        fs::write(repository_path.join("watch-change.txt"), "changed\n").expect("watch fixture");
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline
            && host.five_track.shared.git.generation(&repository) == generation
        {
            thread::sleep(Duration::from_millis(25));
        }
        assert!(host.five_track.shared.git.generation(&repository) > generation);
        fs::remove_dir_all(repository_path).expect("temporary repository cleanup");
    }
}
