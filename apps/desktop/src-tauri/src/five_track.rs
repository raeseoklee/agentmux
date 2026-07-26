use super::*;

use std::collections::{hash_map::DefaultHasher, HashMap, HashSet, VecDeque};
use std::fs;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
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
    EVENT_AGENT_HOOK_STATE_CHANGED, EVENT_DEV_SERVER_CANDIDATE_DETECTED,
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
    GitMutationReceiptLookup, GitReviewDeliveryUpdate, PersistedGitMutationIntent,
    PersistedGitReviewComment, PersistedGitReviewDeliveryAttempt, PersistedGitReviewThread,
    PersistedNotification, PersistedTeamMessage, PersistedVerifiedAgentHook,
    PersistedWorktreeOperation, WorktreeOperationState,
};
use agentmux_vcs::{
    DiffRequest, GitClient, GitContext, GitError, GitFileChange, GitHost as VcsGitHost, Repository,
    StatusReadResult, StatusScan, StatusScanFirstPage, StatusSnapshot,
};
const LEGACY_GIT_STATUS: &str = "git.status";
const REPOSITORY_MONITOR_RECONCILE_INTERVAL: Duration = Duration::from_secs(2);
const REPOSITORY_FALLBACK_STATUS_INTERVAL: Duration = Duration::from_secs(30);
const REPOSITORY_EVENT_DEBOUNCE: Duration = Duration::from_millis(250);
const MAX_WATCHER_STARTS_PER_TICK: usize = 2;
const MAX_FALLBACK_STATUS_READS_PER_TICK: usize = 1;
const MAX_OBSERVED_REPOSITORIES: usize = 64;
const MAX_MUTATION_IDEMPOTENCY_RECEIPTS: usize = 512;
const MAX_DEV_SERVER_CANDIDATES: usize = 500;
const MAX_DEV_SERVER_CANDIDATES_PER_SESSION: usize = 32;
const MAX_STATUS_QUERY_INDEXES: usize = 16;

pub(super) struct FiveTrackState {
    shared: Arc<FiveTrackShared>,
}

struct FiveTrackShared {
    git: GitClient,
    git_status_cache: Mutex<HashMap<String, CachedStatusSnapshot>>,
    git_status_scans: Mutex<HashMap<String, CachedStatusScan>>,
    observed_repositories: Mutex<HashMap<String, ObservedRepository>>,
    git_mutation_guard: Mutex<()>,
    monitor_started: AtomicBool,
    monitor_app: OnceLock<tauri::AppHandle>,
    worktree_saga_guard: Mutex<()>,
    review_delivery_guard: Mutex<()>,
    hook_normalizer: Mutex<AgentHookNormalizer>,
    verified_hooks: Mutex<HashMap<String, VerifiedHookRecord>>,
    url_detectors: Mutex<HashMap<String, PtyUrlDetector>>,
    dev_server_candidates: Mutex<VecDeque<DevelopmentServerCandidateResult>>,
}

#[derive(Clone)]
struct CachedStatusSnapshot {
    repository: Repository,
    snapshot: Arc<StatusSnapshot>,
    page_indexes: Arc<GitStatusPageIndexes>,
    scan_started_at: Instant,
    captured_at: Instant,
}

#[derive(Clone)]
struct CachedStatusScan {
    generation: u64,
    scan: StatusScan,
    started_at: Instant,
}

type GitStatusQueryCache = Arc<Mutex<VecDeque<(GitStatusPageFilter, Arc<Vec<usize>>)>>>;

#[derive(Clone, Debug)]
struct GitStatusPageIndexes {
    all: Arc<Vec<usize>>,
    staged: Arc<Vec<usize>>,
    unstaged: Arc<Vec<usize>>,
    untracked: Arc<Vec<usize>>,
    conflicted: Arc<Vec<usize>>,
    query_cache: GitStatusQueryCache,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct GitStatusPageFilter {
    state: String,
    query: String,
}

impl GitStatusPageFilter {
    fn from_request(state: Option<&str>, query: Option<&str>) -> Self {
        Self {
            state: state
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("all")
                .to_ascii_lowercase(),
            query: query
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_lowercase)
                .unwrap_or_default(),
        }
    }
}

impl GitStatusPageIndexes {
    fn from_snapshot(snapshot: &StatusSnapshot) -> Self {
        let mut all = Vec::with_capacity(snapshot.files.len());
        let mut staged = Vec::with_capacity(snapshot.summary.staged_count);
        let mut unstaged = Vec::with_capacity(snapshot.summary.unstaged_count);
        let mut untracked = Vec::with_capacity(snapshot.summary.untracked_count);
        let mut conflicted = Vec::with_capacity(snapshot.summary.conflict_count);
        for (index, change) in snapshot.files.iter().enumerate() {
            all.push(index);
            if change.staged {
                staged.push(index);
            }
            if change.unstaged {
                unstaged.push(index);
            }
            if change.untracked {
                untracked.push(index);
            }
            if change.conflict {
                conflicted.push(index);
            }
        }
        Self {
            all: Arc::new(all),
            staged: Arc::new(staged),
            unstaged: Arc::new(unstaged),
            untracked: Arc::new(untracked),
            conflicted: Arc::new(conflicted),
            query_cache: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    fn for_filter(
        &self,
        snapshot: &StatusSnapshot,
        filter: &GitStatusPageFilter,
    ) -> Arc<Vec<usize>> {
        let base = match filter.state.as_str() {
            "all" => self.all.clone(),
            "staged" => self.staged.clone(),
            "unstaged" => self.unstaged.clone(),
            "untracked" => self.untracked.clone(),
            "conflicted" | "conflict" => self.conflicted.clone(),
            _ => Arc::new(Vec::new()),
        };
        if filter.query.is_empty() {
            return base;
        }
        if let Ok(cache) = self.query_cache.lock() {
            if let Some((_, indexes)) = cache.iter().find(|(key, _)| key == filter) {
                return indexes.clone();
            }
        }
        let indexes: Arc<Vec<usize>> = Arc::new(
            base.iter()
                .copied()
                .filter(|index| status_change_matches_query(&snapshot.files[*index], &filter.query))
                .collect(),
        );
        if let Ok(mut cache) = self.query_cache.lock() {
            if let Some((_, cached)) = cache.iter().find(|(key, _)| key == filter) {
                return cached.clone();
            }
            if cache.len() >= MAX_STATUS_QUERY_INDEXES {
                cache.pop_front();
            }
            cache.push_back((filter.clone(), indexes.clone()));
        }
        indexes
    }
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
    pending_change_reason: Option<String>,
    debounce_cancel: Option<Arc<AtomicBool>>,
    last_observed_at: Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RepositoryMonitorMode {
    Pending,
    NativeWatcher,
    FallbackStatus,
}

struct RepositoryWatchCancellation {
    requested: AtomicBool,
    failed: AtomicBool,
    handles: Mutex<HashSet<usize>>,
    #[cfg(windows)]
    cancel_event: usize,
}

impl RepositoryWatchCancellation {
    #[cfg(windows)]
    fn new() -> Result<Self, String> {
        use windows_sys::Win32::System::Threading::CreateEventW;

        let cancel_event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if cancel_event.is_null() {
            return Err("failed to create repository watcher cancellation event".to_string());
        }
        Ok(Self {
            requested: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            handles: Mutex::new(HashSet::new()),
            cancel_event: cancel_event as usize,
        })
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
        self.requested.store(true, Ordering::Release);
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::Threading::SetEvent(
                self.cancel_event as *mut std::ffi::c_void,
            );
        }
    }

    fn fail(&self) {
        self.failed.store(true, Ordering::Release);
        self.request();
    }

    fn failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    #[cfg(windows)]
    fn cancel_event(&self) -> *mut std::ffi::c_void {
        self.cancel_event as *mut std::ffi::c_void
    }
}

#[cfg(windows)]
impl Drop for RepositoryWatchCancellation {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.cancel_event as *mut std::ffi::c_void);
        }
    }
}

#[derive(Clone)]
struct VerifiedHookRecord {
    workspace_id: String,
    session_id: String,
    source: String,
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
                git_status_scans: Mutex::new(HashMap::new()),
                observed_repositories: Mutex::new(HashMap::new()),
                git_mutation_guard: Mutex::new(()),
                monitor_started: AtomicBool::new(false),
                monitor_app: OnceLock::new(),
                worktree_saga_guard: Mutex::new(()),
                review_delivery_guard: Mutex::new(()),
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
        let _ = self.shared.monitor_app.set(app.clone());
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

enum PreparedGitMutation {
    Untracked,
    Execute(PersistedGitMutationIntent),
    Reused(IpcGitMutationResult),
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
        | GitError::StatusEntryLimit { .. }
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
        pane_id: Option<&str>,
    ) -> Result<(GitContext, GitCommandContext), DesktopHostError> {
        let legacy = self.workspace_git_context_for_pane(workspace_id, pane_id)?;
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
        pane_id: Option<&str>,
        repository_selector: Option<&str>,
    ) -> Result<(GitCommandContext, Repository), DesktopHostError> {
        let (context, legacy) = self.workspace_vcs_context(workspace_id, pane_id)?;
        if let Some(repository_id) = repository_selector {
            let observed_key = format!("{workspace_id}|{repository_id}");
            let observed_here = self
                .five_track
                .shared
                .observed_repositories
                .lock()
                .ok()
                .is_some_and(|observed| observed.contains_key(&observed_key));
            let cached_repository = self
                .five_track
                .shared
                .git_status_cache
                .lock()
                .ok()
                .and_then(|cache| cache.get(repository_id).cloned())
                .map(|cached| cached.repository);
            if let Some(repository) = cached_repository.filter(|repository| {
                observed_here && git_context_matches_repository_root(&context, repository)
            }) {
                return Ok((legacy, repository));
            }
        }
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
    ) -> Result<(String, CachedStatusSnapshot), DesktopHostError> {
        self.completed_status_snapshot(workspace_id, repository, None)
    }

    fn completed_status_snapshot(
        &self,
        workspace_id: &str,
        repository: &Repository,
        requested_generation: Option<u64>,
    ) -> Result<(String, CachedStatusSnapshot), DesktopHostError> {
        let repository_id = repository_id(repository);
        self.observe_and_arm_repository(workspace_id, &repository_id, repository);
        let generation = self.five_track.shared.git.generation(repository);
        if let Ok(cache) = self.five_track.shared.git_status_cache.lock() {
            if let Some(entry) = cache.get(&repository_id) {
                if entry.snapshot.summary.generation == generation {
                    validate_generation(requested_generation, generation)?;
                    return Ok((repository_id, entry.clone()));
                }
            }
        }
        let active = if let Ok(scans) = self.five_track.shared.git_status_scans.lock() {
            scans.get(&repository_id).cloned()
        } else {
            None
        };
        let active = match active {
            Some(active) => {
                validate_generation(requested_generation, active.generation)?;
                active
            }
            None if requested_generation.is_some() => {
                return Err(DesktopHostError::Control(ControlError::new(
                    ErrorCode::Conflict,
                    "Git status snapshot is no longer available; refresh page zero.",
                )))
            }
            None => self.start_status_scan(repository, &repository_id, 250)?,
        };
        let result = active.scan.wait_for_completion().map_err(vcs_error)?;
        let cached = self.install_completed_status_scan(
            workspace_id,
            repository,
            &repository_id,
            active.generation,
            active.started_at,
            result,
        )?;
        Ok((repository_id, cached))
    }

    fn start_status_scan(
        &self,
        repository: &Repository,
        repository_id: &str,
        first_page_limit: usize,
    ) -> Result<CachedStatusScan, DesktopHostError> {
        let mut scans = self
            .five_track
            .shared
            .git_status_scans
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable(
                    "Git status scan state is unavailable".to_string(),
                )
            })?;
        if let Some(active) = scans.get(repository_id) {
            return Ok(active.clone());
        }
        if scans.len() >= MAX_OBSERVED_REPOSITORIES {
            if let Some(oldest) = scans
                .iter()
                .min_by_key(|(_, active)| active.started_at)
                .map(|(key, _)| key.clone())
            {
                if let Some(evicted) = scans.remove(&oldest) {
                    evicted.scan.cancel();
                }
            }
        }
        let generation = self
            .five_track
            .shared
            .git
            .mark_repository_changed(repository);
        let started_at = Instant::now();
        let scan = self
            .five_track
            .shared
            .git
            .start_status_scan(repository, first_page_limit)
            .map_err(vcs_error)?;
        let active = CachedStatusScan {
            generation,
            scan,
            started_at,
        };
        scans.insert(repository_id.to_string(), active.clone());
        Ok(active)
    }

    fn first_status_page_with_retry(
        &self,
        repository: &Repository,
        repository_id: &str,
        limit: usize,
    ) -> Result<(CachedStatusScan, StatusScanFirstPage), DesktopHostError> {
        for attempt in 0..3 {
            let active = self.start_status_scan(repository, repository_id, limit)?;
            match active.scan.wait_for_first_page() {
                Ok(page) => return Ok((active, page)),
                Err(GitError::StateUnavailable(message))
                    if message.contains("status scan was cancelled") && attempt < 2 =>
                {
                    if let Ok(mut scans) = self.five_track.shared.git_status_scans.lock() {
                        if scans
                            .get(repository_id)
                            .is_some_and(|current| current.generation == active.generation)
                        {
                            scans.remove(repository_id);
                        }
                    }
                    thread::sleep(Duration::from_millis(25 * (attempt + 1) as u64));
                }
                Err(error) => return Err(vcs_error(error)),
            }
        }
        Err(DesktopHostError::StateUnavailable(
            "Git status scan could not stabilize after retries.".to_string(),
        ))
    }

    fn install_completed_status_scan(
        &self,
        workspace_id: &str,
        repository: &Repository,
        repository_id: &str,
        generation: u64,
        scan_started_at: Instant,
        result: Arc<StatusReadResult>,
    ) -> Result<CachedStatusSnapshot, DesktopHostError> {
        if result.snapshot.summary.generation != generation {
            if let Ok(mut scans) = self.five_track.shared.git_status_scans.lock() {
                scans.remove(repository_id);
            }
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::Conflict,
                "Git status scan generation no longer matches its active request.",
            )));
        }
        let current_generation = self.five_track.shared.git.generation(repository);
        if current_generation != generation {
            if let Ok(mut scans) = self.five_track.shared.git_status_scans.lock() {
                if scans
                    .get(repository_id)
                    .is_some_and(|active| active.generation == generation)
                {
                    scans.remove(repository_id);
                }
            }
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::Conflict,
                "Git repository changed while its status snapshot was loading; refresh and retry.",
            )));
        }
        let snapshot = result.snapshot.clone();
        let cached = CachedStatusSnapshot {
            repository: repository.clone(),
            page_indexes: Arc::new(GitStatusPageIndexes::from_snapshot(&snapshot)),
            snapshot: Arc::new(snapshot),
            scan_started_at,
            captured_at: Instant::now(),
        };
        if let Ok(mut cache) = self.five_track.shared.git_status_cache.lock() {
            if cache.len() >= MAX_OBSERVED_REPOSITORIES && !cache.contains_key(repository_id) {
                if let Some(oldest) = cache
                    .iter()
                    .min_by_key(|(_, entry)| entry.captured_at)
                    .map(|(key, _)| key.clone())
                {
                    cache.remove(&oldest);
                }
            }
            cache.insert(repository_id.to_string(), cached.clone());
        }
        if let Ok(mut scans) = self.five_track.shared.git_status_scans.lock() {
            if scans
                .get(repository_id)
                .is_some_and(|active| active.generation == generation)
            {
                scans.remove(repository_id);
            }
        }
        self.observe_repository(workspace_id, repository_id, repository);
        Ok(cached)
    }

    fn invalidate_status_snapshot(&self, repository_id: &str) {
        if let Ok(mut scans) = self.five_track.shared.git_status_scans.lock() {
            if let Some(active) = scans.remove(repository_id) {
                active.scan.cancel();
            }
        }
        if let Ok(mut cache) = self.five_track.shared.git_status_cache.lock() {
            cache.remove(repository_id);
        }
    }

    fn observe_repository(
        &self,
        workspace_id: &str,
        repository_id: &str,
        repository: &Repository,
    ) -> bool {
        let key = format!("{workspace_id}|{repository_id}");
        let Ok(mut observed) = self.five_track.shared.observed_repositories.lock() else {
            return false;
        };
        let observation_created = !observed.contains_key(&key);
        if observed.len() >= MAX_OBSERVED_REPOSITORIES && !observed.contains_key(&key) {
            if let Some(oldest_key) = observed
                .iter()
                .min_by_key(|(_, entry)| entry.last_observed_at)
                .map(|(key, _)| key.clone())
            {
                if let Some(mut evicted) = observed.remove(&oldest_key) {
                    if let Some(cancel) = evicted.watch_cancel.take() {
                        cancel.request();
                    }
                    if let Some(cancel) = evicted.debounce_cancel.take() {
                        cancel.store(true, Ordering::Release);
                    }
                }
            }
        }
        observed
            .entry(key)
            .and_modify(|entry| {
                entry.workspace_id = workspace_id.to_string();
                entry.repository = repository.clone();
                entry.last_observed_at = Instant::now();
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
                pending_change_reason: None,
                debounce_cancel: None,
                last_observed_at: Instant::now(),
            });
        observation_created
    }

    fn observe_and_arm_repository(
        &self,
        workspace_id: &str,
        repository_id: &str,
        repository: &Repository,
    ) {
        let had_cached_snapshot = self
            .five_track
            .shared
            .git_status_cache
            .lock()
            .ok()
            .is_some_and(|cache| cache.contains_key(repository_id));
        let observation_created = self.observe_repository(workspace_id, repository_id, repository);
        if observation_created && had_cached_snapshot {
            self.five_track
                .shared
                .git
                .mark_repository_changed(repository);
            self.invalidate_status_snapshot(repository_id);
        }
        let key = format!("{workspace_id}|{repository_id}");
        arm_observed_repository(
            &self.five_track.shared,
            self.five_track.shared.monitor_app.get(),
            &key,
        );
    }

    fn handle_legacy_git_status(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitWorkspaceParams = request.parse_params()?;
        let (context, _) = self.workspace_vcs_context(&params.workspace_id, None)?;
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
        let (_, cached) = self.status_snapshot(&params.workspace_id, &repository)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &legacy_git_status_result(&cached.snapshot),
        ))
    }

    fn handle_git_status_summary_v2(
        &self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        let params: GitRepositoryParams = request.parse_params()?;
        validate_workspace_id(&params.workspace_id)?;
        let (_, repository) = self.resolve_vcs_repository(
            &params.workspace_id,
            params.pane_id.as_deref(),
            params.repository_id.as_deref(),
        )?;
        let (repository_id, cached) = self.status_snapshot(&params.workspace_id, &repository)?;
        validate_repository_selector(params.repository_id.as_deref(), &repository_id)?;
        let summary = &cached.snapshot.summary;
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
        let (_, repository) = self.resolve_vcs_repository(
            &params.workspace_id,
            params.pane_id.as_deref(),
            params.repository_id.as_deref(),
        )?;
        if params.cursor.is_some() && params.generation.is_none() {
            return Err(control_invalid_request(
                "Git status cursors require their snapshot generation.",
            ));
        }
        let repository_id = repository_id(&repository);
        self.observe_and_arm_repository(&params.workspace_id, &repository_id, &repository);
        validate_repository_selector(params.repository_id.as_deref(), &repository_id)?;
        let limit = params.limit.unwrap_or(250).clamp(1, 500);

        if params.cursor.is_none() && params.generation.is_none() {
            let generation = self.five_track.shared.git.generation(&repository);
            let cached = self
                .five_track
                .shared
                .git_status_cache
                .lock()
                .ok()
                .and_then(|cache| cache.get(&repository_id).cloned())
                .filter(|entry| entry.snapshot.summary.generation == generation);
            if let Some(cached) = cached {
                return self.complete_status_page_response(
                    request,
                    &params,
                    &repository_id,
                    &cached,
                );
            }

            let (active, first_page) =
                self.first_status_page_with_retry(&repository, &repository_id, limit)?;
            return match first_page {
                StatusScanFirstPage::Prefix(prefix) => {
                    let filter = GitStatusPageFilter::from_request(
                        params.state.as_deref(),
                        params.query.as_deref(),
                    );
                    let changes = prefix
                        .changes
                        .iter()
                        .filter(|change| status_change_matches_filter(change, &filter))
                        .map(git_change_result)
                        .collect();
                    Ok(ResponseEnvelope::ok_typed(
                        request.id.clone(),
                        &GitStatusPageResult {
                            workspace_id: params.workspace_id,
                            repository_id,
                            generation: prefix.generation,
                            summary: None,
                            changes,
                            next_cursor: None,
                            total_count: None,
                        },
                    ))
                }
                StatusScanFirstPage::Complete(result) => {
                    let cached = self.install_completed_status_scan(
                        &params.workspace_id,
                        &repository,
                        &repository_id,
                        active.generation,
                        active.started_at,
                        result,
                    )?;
                    self.complete_status_page_response(request, &params, &repository_id, &cached)
                }
            };
        }

        let (_, cached) =
            self.completed_status_snapshot(&params.workspace_id, &repository, params.generation)?;
        self.complete_status_page_response(request, &params, &repository_id, &cached)
    }

    fn complete_status_page_response(
        &self,
        request: &RequestEnvelope,
        params: &GitStatusPageParams,
        repository_id: &str,
        cached: &CachedStatusSnapshot,
    ) -> Result<ResponseEnvelope, DesktopHostError> {
        validate_generation(params.generation, cached.snapshot.summary.generation)?;

        let filter =
            GitStatusPageFilter::from_request(params.state.as_deref(), params.query.as_deref());
        let filtered = cached.page_indexes.for_filter(&cached.snapshot, &filter);
        let offset = parse_git_cursor(
            params.cursor.as_deref(),
            cached.snapshot.summary.generation,
            &filter,
        )?;
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
            .map(|index| git_change_result(&cached.snapshot.files[*index]))
            .collect();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &GitStatusPageResult {
                workspace_id: params.workspace_id.clone(),
                repository_id: repository_id.to_string(),
                generation: cached.snapshot.summary.generation,
                summary: Some(GitStatusSummaryResult {
                    workspace_id: params.workspace_id.clone(),
                    repository_id: repository_id.to_string(),
                    repository_root: cached.snapshot.summary.repository_root.clone(),
                    branch: cached.snapshot.summary.branch.clone(),
                    head_oid: cached.snapshot.summary.head.clone(),
                    upstream: cached.snapshot.summary.upstream.clone(),
                    ahead: cached.snapshot.summary.ahead,
                    behind: cached.snapshot.summary.behind,
                    staged_count: cached.snapshot.summary.staged_count,
                    unstaged_count: cached.snapshot.summary.unstaged_count,
                    untracked_count: cached.snapshot.summary.untracked_count,
                    conflicted_count: cached.snapshot.summary.conflict_count,
                    generation: cached.snapshot.summary.generation,
                    refreshed_at: timestamp(),
                }),
                changes,
                next_cursor: (end < filtered.len())
                    .then(|| format_git_cursor(cached.snapshot.summary.generation, &filter, end)),
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
            let (_, repository) = self.resolve_vcs_repository(&params.workspace_id, None, None)?;
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
        let (_, repository) = self.resolve_vcs_repository(
            &params.workspace_id,
            params.pane_id.as_deref(),
            params.repository_id.as_deref(),
        )?;
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
        let diff_hash = format!("{:016x}", stable_hash_bytes(result.patch.as_bytes()));
        if let Ok(mut store) = self.store.lock() {
            if let Ok(threads) =
                store.list_git_review_threads(repository.root(), Some(&params.workspace_id), true)
            {
                for mut thread in threads
                    .into_iter()
                    .filter(|thread| thread.path == result.path && !thread.stale)
                {
                    let anchor = serde_json::from_str::<GitReviewLineAnchor>(&thread.line_anchor);
                    let still_current = anchor
                        .ok()
                        .and_then(|anchor| anchor.diff_hash)
                        .is_some_and(|identity| identity == diff_hash);
                    if !still_current {
                        thread.stale = true;
                        thread.stale_reason = Some("diff content changed".to_string());
                        thread.updated_at = timestamp();
                        let _ = store.upsert_git_review_thread(&thread);
                    }
                }
            }
        }
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
                diff_hash,
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
        let (legacy_context, repository) = self.resolve_vcs_repository(
            &params.workspace_id,
            params.pane_id.as_deref(),
            params.repository_id.as_deref(),
        )?;
        let repository_id = repository_id(&repository);
        validate_repository_selector(params.repository_id.as_deref(), &repository_id)?;
        let request_fingerprint = git_mutation_request_fingerprint(&params)?;
        let _mutation_guard = self.lock_git_mutation(params.idempotency_key.as_deref())?;
        let intent = match self.prepare_git_mutation(
            request.method.as_str(),
            &repository_id,
            params.idempotency_key.as_deref(),
            &request_fingerprint,
            &repository,
        )? {
            PreparedGitMutation::Untracked => None,
            PreparedGitMutation::Execute(intent) => Some(intent),
            PreparedGitMutation::Reused(result) => {
                return Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result));
            }
        };

        self.invalidate_status_snapshot(&repository_id);
        let mutation_result = match mutation {
            GitMutation::Stage => self
                .five_track
                .shared
                .git
                .stage(&repository, &params.paths)
                .map_err(vcs_error),
            GitMutation::Unstage => self
                .five_track
                .shared
                .git
                .unstage(&repository, &params.paths)
                .map_err(vcs_error),
            GitMutation::Discard => (|| {
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
                Ok(self
                    .five_track
                    .shared
                    .git
                    .mark_repository_changed(&repository))
            })(),
        };
        let generation = match mutation_result {
            Ok(generation) => generation,
            Err(error) => {
                return Err(self.reconcile_failed_git_mutation(
                    intent.as_ref(),
                    &repository,
                    error,
                ));
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
        self.complete_git_mutation(intent.as_ref(), &result)?;
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
        let (_, repository) = self.resolve_vcs_repository(
            &params.workspace_id,
            params.pane_id.as_deref(),
            params.repository_id.as_deref(),
        )?;
        let repository_id = repository_id(&repository);
        validate_repository_selector(params.repository_id.as_deref(), &repository_id)?;
        let request_fingerprint = git_mutation_request_fingerprint(&params)?;
        let _mutation_guard = self.lock_git_mutation(params.idempotency_key.as_deref())?;
        let intent = match self.prepare_git_mutation(
            request.method.as_str(),
            &repository_id,
            params.idempotency_key.as_deref(),
            &request_fingerprint,
            &repository,
        )? {
            PreparedGitMutation::Untracked => None,
            PreparedGitMutation::Execute(intent) => Some(intent),
            PreparedGitMutation::Reused(result) => {
                return Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result));
            }
        };
        self.invalidate_status_snapshot(&repository_id);
        let mutation_result = match mutation {
            GitMutation::Stage => self
                .five_track
                .shared
                .git
                .stage(&repository, &[])
                .map_err(vcs_error),
            GitMutation::Unstage => self
                .five_track
                .shared
                .git
                .unstage(&repository, &[])
                .map_err(vcs_error),
            GitMutation::Discard => unreachable!("discard-all is intentionally unsupported"),
        };
        let generation = match mutation_result {
            Ok(generation) => generation,
            Err(error) => {
                return Err(self.reconcile_failed_git_mutation(
                    intent.as_ref(),
                    &repository,
                    error,
                ));
            }
        };
        let result = IpcGitMutationResult {
            workspace_id: params.workspace_id.clone(),
            repository_id: repository_id.clone(),
            generation,
            affected_paths: Vec::new(),
            commit_oid: None,
            reused: false,
        };
        self.complete_git_mutation(intent.as_ref(), &result)?;
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
        let (legacy_context, repository) = self.resolve_vcs_repository(
            &params.workspace_id,
            params.pane_id.as_deref(),
            params.repository_id.as_deref(),
        )?;
        let repository_id = repository_id(&repository);
        validate_repository_selector(params.repository_id.as_deref(), &repository_id)?;
        let request_fingerprint = git_mutation_request_fingerprint(&params)?;
        let _mutation_guard = self.lock_git_mutation(params.idempotency_key.as_deref())?;
        let intent = match self.prepare_git_mutation(
            request.method.as_str(),
            &repository_id,
            params.idempotency_key.as_deref(),
            &request_fingerprint,
            &repository,
        )? {
            PreparedGitMutation::Untracked => None,
            PreparedGitMutation::Execute(intent) => Some(intent),
            PreparedGitMutation::Reused(result) => {
                return Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result));
            }
        };

        self.invalidate_status_snapshot(&repository_id);
        let commit_result = (|| {
            if params.amend {
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
                return Ok((
                    String::from_utf8_lossy(&oid.stdout).trim().to_string(),
                    String::from_utf8_lossy(&output.stdout).trim().to_string(),
                    self.five_track
                        .shared
                        .git
                        .mark_repository_changed(&repository),
                ));
            }
            let result = self
                .five_track
                .shared
                .git
                .commit(&repository, &params.message)
                .map_err(vcs_error)?;
            Ok((result.commit, result.summary, result.generation))
        })();
        let (commit, summary, generation) = match commit_result {
            Ok(result) => result,
            Err(error) => {
                return Err(self.reconcile_failed_git_mutation(
                    intent.as_ref(),
                    &repository,
                    error,
                ));
            }
        };
        if is_legacy {
            self.git_repository_mutated(
                &params.workspace_id,
                &repository_id,
                &repository,
                generation,
                request.method.as_str(),
            );
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
        self.complete_git_mutation(intent.as_ref(), &result)?;
        self.git_repository_mutated(
            &result.workspace_id,
            &repository_id,
            &repository,
            generation,
            request.method.as_str(),
        );
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn lock_git_mutation(
        &self,
        idempotency_key: Option<&str>,
    ) -> Result<Option<std::sync::MutexGuard<'_, ()>>, DesktopHostError> {
        if idempotency_key.is_none() {
            return Ok(None);
        }
        self.five_track
            .shared
            .git_mutation_guard
            .lock()
            .map(Some)
            .map_err(|_| {
                DesktopHostError::StateUnavailable(
                    "Git mutation idempotency state is unavailable".to_string(),
                )
            })
    }

    fn prepare_git_mutation(
        &self,
        method: &str,
        repository_id: &str,
        idempotency_key: Option<&str>,
        request_fingerprint: &str,
        repository: &Repository,
    ) -> Result<PreparedGitMutation, DesktopHostError> {
        let Some(idempotency_key) = idempotency_key else {
            return Ok(PreparedGitMutation::Untracked);
        };
        let current_precondition =
            git_mutation_precondition_fingerprint(&self.five_track.shared.git, repository)
                .map_err(vcs_error)?;
        let proposed = PersistedGitMutationIntent {
            method: method.to_string(),
            repository_id: repository_id.to_string(),
            idempotency_key: idempotency_key.to_string(),
            request_fingerprint: request_fingerprint.to_string(),
            precondition_fingerprint: current_precondition.clone(),
            created_at: timestamp(),
        };
        let mut store = self.store.lock().map_err(|_| {
            DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
        })?;
        match store.prepare_git_mutation(&proposed)? {
            GitMutationReceiptLookup::Pending(intent)
                if intent.precondition_fingerprint == current_precondition =>
            {
                Ok(PreparedGitMutation::Execute(intent))
            }
            GitMutationReceiptLookup::Pending(intent) => {
                drop(store);
                Err(self.mark_git_mutation_indeterminate(
                    &intent,
                    "repository_precondition_changed",
                    Some(format!(
                        "stored={}, current={current_precondition}",
                        intent.precondition_fingerprint
                    )),
                ))
            }
            GitMutationReceiptLookup::Match(receipt) => {
                let mut result =
                    serde_json::from_str::<IpcGitMutationResult>(&receipt.response_json)?;
                result.reused = true;
                Ok(PreparedGitMutation::Reused(result))
            }
            GitMutationReceiptLookup::Indeterminate {
                intent,
                failure_json,
            } => Err(git_mutation_indeterminate_conflict(
                &intent,
                "stored_indeterminate_outcome",
                failure_json,
                None,
            )),
            GitMutationReceiptLookup::FingerprintMismatch {
                stored_request_fingerprint,
            } => Err(git_mutation_idempotency_conflict(
                &stored_request_fingerprint,
            )),
            GitMutationReceiptLookup::Missing => Err(DesktopHostError::StateUnavailable(
                "Git mutation intent was not persisted".to_string(),
            )),
        }
    }

    fn complete_git_mutation(
        &self,
        intent: Option<&PersistedGitMutationIntent>,
        result: &IpcGitMutationResult,
    ) -> Result<(), DesktopHostError> {
        let Some(intent) = intent else {
            return Ok(());
        };
        let response_json = serde_json::to_string(result).map_err(|error| {
            self.mark_git_mutation_indeterminate(
                intent,
                "response_encoding_failed_after_effect",
                Some(error.to_string()),
            )
        })?;
        let completion = {
            let mut store = self.store.lock().map_err(|_| {
                self.mark_git_mutation_indeterminate(
                    intent,
                    "completion_store_lock_failed_after_effect",
                    None,
                )
            })?;
            let completion = store.complete_git_mutation(intent, &response_json, &timestamp());
            if matches!(completion, Ok(GitMutationReceiptLookup::Match(_))) {
                let _ = store.prune_git_mutation_receipts(MAX_MUTATION_IDEMPOTENCY_RECEIPTS);
            }
            completion
        };
        match completion {
            Ok(GitMutationReceiptLookup::Match(_)) => Ok(()),
            Ok(GitMutationReceiptLookup::FingerprintMismatch {
                stored_request_fingerprint,
            }) => Err(git_mutation_idempotency_conflict(
                &stored_request_fingerprint,
            )),
            Ok(GitMutationReceiptLookup::Indeterminate { failure_json, .. }) => {
                Err(git_mutation_indeterminate_conflict(
                    intent,
                    "stored_indeterminate_outcome",
                    failure_json,
                    None,
                ))
            }
            Ok(GitMutationReceiptLookup::Pending(_)) | Ok(GitMutationReceiptLookup::Missing) => {
                Err(self.mark_git_mutation_indeterminate(
                    intent,
                    "completion_transition_not_persisted",
                    None,
                ))
            }
            Err(error) => Err(self.mark_git_mutation_indeterminate(
                intent,
                "completion_persistence_failed_after_effect",
                Some(error.to_string()),
            )),
        }
    }

    fn reconcile_failed_git_mutation(
        &self,
        intent: Option<&PersistedGitMutationIntent>,
        repository: &Repository,
        error: DesktopHostError,
    ) -> DesktopHostError {
        let Some(intent) = intent else {
            return error;
        };
        match git_mutation_precondition_fingerprint(&self.five_track.shared.git, repository) {
            Ok(current) if current == intent.precondition_fingerprint => error,
            Ok(current) => self.mark_git_mutation_indeterminate(
                intent,
                "git_error_changed_repository_state",
                Some(format!("error={error}; current={current}")),
            ),
            Err(fingerprint_error) => self.mark_git_mutation_indeterminate(
                intent,
                "git_error_precondition_unreadable",
                Some(format!(
                    "error={error}; fingerprint_error={fingerprint_error}"
                )),
            ),
        }
    }

    fn mark_git_mutation_indeterminate(
        &self,
        intent: &PersistedGitMutationIntent,
        reason: &str,
        source: Option<String>,
    ) -> DesktopHostError {
        let failure_json = serde_json::json!({
            "reason": reason,
            "source": source,
        })
        .to_string();
        let persistence_error = match self.store.lock() {
            Ok(mut store) => store
                .mark_git_mutation_indeterminate(intent, &failure_json, &timestamp())
                .err()
                .map(|error| error.to_string()),
            Err(_) => Some("desktop store lock is unavailable".to_string()),
        };
        git_mutation_indeterminate_conflict(intent, reason, Some(failure_json), persistence_error)
    }

    fn git_repository_mutated(
        &self,
        workspace_id: &str,
        repository_id: &str,
        repository: &Repository,
        generation: u64,
        reason: &str,
    ) {
        self.invalidate_status_snapshot(repository_id);
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

fn git_mutation_request_fingerprint(
    params: &impl serde::Serialize,
) -> Result<String, DesktopHostError> {
    serde_json::to_string(params).map_err(DesktopHostError::from)
}

fn git_mutation_precondition_fingerprint(
    git: &GitClient,
    repository: &Repository,
) -> Result<String, GitError> {
    let snapshot = git.read_status(repository)?;
    let mut hasher = StableHasher::default();
    snapshot.summary.head.hash(&mut hasher);
    snapshot.files.len().hash(&mut hasher);
    for change in &snapshot.files {
        (
            &change.path,
            &change.original_path,
            &change.index_status,
            &change.worktree_status,
            change.staged,
            change.unstaged,
            change.untracked,
            change.conflict,
        )
            .hash(&mut hasher);
        if change.staged {
            let diff = git.diff(
                repository,
                &DiffRequest {
                    path: change.path.clone(),
                    staged: true,
                    untracked: false,
                },
            )?;
            if diff.truncated {
                return Err(GitError::StateUnavailable(format!(
                    "The staged diff for '{}' is too large to fingerprint safely.",
                    change.path
                )));
            }
            ("staged", &diff.patch).hash(&mut hasher);
        }
        if change.untracked {
            let diff = git.diff(
                repository,
                &DiffRequest {
                    path: change.path.clone(),
                    staged: false,
                    untracked: true,
                },
            )?;
            if diff.truncated {
                return Err(GitError::StateUnavailable(format!(
                    "The untracked diff for '{}' is too large to fingerprint safely.",
                    change.path
                )));
            }
            ("untracked", &diff.patch).hash(&mut hasher);
        } else if change.unstaged {
            let diff = git.diff(
                repository,
                &DiffRequest {
                    path: change.path.clone(),
                    staged: false,
                    untracked: false,
                },
            )?;
            if diff.truncated {
                return Err(GitError::StateUnavailable(format!(
                    "The worktree diff for '{}' is too large to fingerprint safely.",
                    change.path
                )));
            }
            ("unstaged", &diff.patch).hash(&mut hasher);
        }
    }
    Ok(format!("v1:{:016x}", hasher.finish()))
}

fn git_mutation_idempotency_conflict(stored_request_fingerprint: &str) -> DesktopHostError {
    DesktopHostError::Control(
        ControlError::new(
            ErrorCode::Conflict,
            "The Git idempotency key is already bound to a different request.",
        )
        .with_details(
            serde_json::json!({
                "kind": "git_mutation_fingerprint_mismatch",
                "stored_request_fingerprint": stored_request_fingerprint,
                "safe_to_retry": false,
            })
            .to_string(),
        ),
    )
}

fn git_mutation_indeterminate_conflict(
    intent: &PersistedGitMutationIntent,
    reason: &str,
    failure_json: Option<String>,
    persistence_error: Option<String>,
) -> DesktopHostError {
    DesktopHostError::Control(
        ControlError::new(
            ErrorCode::Conflict,
            "The Git mutation outcome is indeterminate; automatic replay was blocked.",
        )
        .with_details(
            serde_json::json!({
                "kind": "git_mutation_indeterminate",
                "lifecycle_state": "indeterminate",
                "method": intent.method,
                "repository_id": intent.repository_id,
                "idempotency_key": intent.idempotency_key,
                "reason": reason,
                "failure_json": failure_json,
                "persistence_error": persistence_error,
                "safe_to_retry": false,
            })
            .to_string(),
        ),
    )
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

fn status_change_matches_query(change: &GitFileChange, query: &str) -> bool {
    change.path.to_lowercase().contains(query)
        || change
            .original_path
            .as_deref()
            .is_some_and(|path| path.to_lowercase().contains(query))
}

fn status_change_matches_filter(change: &GitFileChange, filter: &GitStatusPageFilter) -> bool {
    let state_matches = match filter.state.as_str() {
        "all" => true,
        "staged" => change.staged,
        "unstaged" => change.unstaged,
        "untracked" => change.untracked,
        "conflicted" | "conflict" => change.conflict,
        _ => false,
    };
    state_matches && (filter.query.is_empty() || status_change_matches_query(change, &filter.query))
}

fn git_cursor_fingerprint(generation: u64, filter: &GitStatusPageFilter) -> u64 {
    let mut hasher = DefaultHasher::new();
    generation.hash(&mut hasher);
    filter.hash(&mut hasher);
    hasher.finish()
}

fn format_git_cursor(generation: u64, filter: &GitStatusPageFilter, offset: usize) -> String {
    format!(
        "v1:{generation}:{:016x}:{offset}",
        git_cursor_fingerprint(generation, filter)
    )
}

fn parse_git_cursor(
    cursor: Option<&str>,
    generation: u64,
    filter: &GitStatusPageFilter,
) -> Result<usize, DesktopHostError> {
    let Some(cursor) = cursor.map(str::trim).filter(|cursor| !cursor.is_empty()) else {
        return Ok(0);
    };
    let mut fields = cursor.split(':');
    let version = fields.next();
    let cursor_generation = fields.next().and_then(|value| value.parse::<u64>().ok());
    let fingerprint = fields
        .next()
        .and_then(|value| u64::from_str_radix(value, 16).ok());
    let offset = fields.next().and_then(|value| value.parse::<usize>().ok());
    if version != Some("v1")
        || fields.next().is_some()
        || cursor_generation != Some(generation)
        || fingerprint != Some(git_cursor_fingerprint(generation, filter))
    {
        return Err(DesktopHostError::Control(ControlError::new(
            ErrorCode::Conflict,
            "Git status cursor belongs to a different generation, state, or query; refresh and retry.",
        )));
    }
    offset.ok_or_else(|| control_invalid_request("Git status cursor is invalid."))
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

fn git_context_matches_repository_root(context: &GitContext, repository: &Repository) -> bool {
    if &context.host != repository.host() {
        return false;
    }
    let normalize = |value: &str| {
        let normalized = value.replace('\\', "/").trim_end_matches('/').to_string();
        if matches!(&context.host, VcsGitHost::Native) {
            normalized.to_ascii_lowercase()
        } else {
            normalized
        }
    };
    let cwd = normalize(&context.cwd);
    let root = normalize(repository.root());
    cwd == root
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
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for key in pending {
        arm_observed_repository(shared, app, &key);
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
        let status = shared.git.read_status(&snapshot.repository).ok();
        let digest = status.as_ref().map(status_content_hash);
        let changed = shared
            .observed_repositories
            .lock()
            .ok()
            .and_then(|mut observed| {
                let entry = observed.get_mut(&key)?;
                let changed = digest.is_some_and(|digest| {
                    entry
                        .fallback_status_hash
                        .is_some_and(|previous| previous != digest)
                });
                if let Some(digest) = digest {
                    entry.fallback_status_hash = Some(digest);
                }
                entry.monitor_mode = RepositoryMonitorMode::Pending;
                Some(changed)
            })
            .unwrap_or(false);
        if changed {
            signal_repository_changed(shared, &key, "fallback_status", app);
        }
    }
}

fn arm_observed_repository(
    shared: &Arc<FiveTrackShared>,
    app: Option<&tauri::AppHandle>,
    key: &str,
) {
    let snapshot = shared
        .observed_repositories
        .lock()
        .ok()
        .and_then(|observed| observed.get(key).cloned())
        .filter(|entry| entry.monitor_mode == RepositoryMonitorMode::Pending);
    let Some(snapshot) = snapshot else {
        return;
    };
    let watcher = snapshot
        .scan_root
        .as_deref()
        .ok_or_else(|| "repository has no Windows-visible watch root".to_string())
        .and_then(|root| {
            start_repository_native_watchers(
                Arc::clone(shared),
                app.cloned(),
                key.to_string(),
                root,
            )
        })
        .ok();
    let adopted = shared
        .observed_repositories
        .lock()
        .map(|mut observed| {
            adopt_repository_watcher(&mut observed, key, &snapshot, watcher.as_ref())
        })
        .unwrap_or(false);
    if !adopted {
        if let Some(watcher) = watcher {
            watcher.request();
        }
    }
}

fn adopt_repository_watcher(
    observed: &mut HashMap<String, ObservedRepository>,
    key: &str,
    snapshot: &ObservedRepository,
    watcher: Option<&Arc<RepositoryWatchCancellation>>,
) -> bool {
    let Some(entry) = observed.get_mut(key) else {
        return false;
    };
    if entry.monitor_mode != RepositoryMonitorMode::Pending || entry.scan_root != snapshot.scan_root
    {
        return false;
    }

    let watcher_started = watcher
        .map(|cancellation| !cancellation.failed())
        .unwrap_or(false);
    entry.monitor_mode = if watcher_started {
        RepositoryMonitorMode::NativeWatcher
    } else {
        RepositoryMonitorMode::FallbackStatus
    };
    entry.watch_cancel = watcher.cloned().filter(|_| watcher_started);
    entry.next_fallback_status_at = Instant::now() + REPOSITORY_FALLBACK_STATUS_INTERVAL;
    watcher_started
}

fn signal_repository_changed(
    shared: &Arc<FiveTrackShared>,
    key: &str,
    reason: &str,
    app: Option<&tauri::AppHandle>,
) {
    let cancellation = shared
        .observed_repositories
        .lock()
        .ok()
        .and_then(|mut observed| {
            let entry = observed.get_mut(key)?;
            entry.last_event_at = Some(Instant::now());
            entry.pending_change_reason = Some(reason.to_string());
            if entry.debounce_cancel.is_some() {
                return Some(None);
            }
            let cancellation = Arc::new(AtomicBool::new(false));
            entry.debounce_cancel = Some(Arc::clone(&cancellation));
            Some(Some(cancellation))
        });
    let Some(cancellation) = cancellation else {
        return;
    };
    let Some(cancellation) = cancellation else {
        return;
    };
    let shared = Arc::clone(shared);
    let key = key.to_string();
    let app = app.cloned();
    let worker_shared = Arc::clone(&shared);
    let worker_key = key.clone();
    let worker_app = app.clone();
    let worker_cancellation = Arc::clone(&cancellation);
    let spawn = thread::Builder::new()
        .name("agentmux-git-debounce".to_string())
        .spawn(move || {
            flush_repository_change_after_debounce(
                worker_shared,
                worker_key,
                worker_app,
                worker_cancellation,
            )
        });
    if spawn.is_err() {
        flush_repository_change_after_debounce(shared, key, app, cancellation);
    }
}

fn flush_repository_change_after_debounce(
    shared: Arc<FiveTrackShared>,
    key: String,
    app: Option<tauri::AppHandle>,
    cancellation: Arc<AtomicBool>,
) {
    let snapshot = loop {
        if cancellation.load(Ordering::Acquire) {
            return;
        }
        let next = shared
            .observed_repositories
            .lock()
            .ok()
            .and_then(|mut observed| {
                let entry = observed.get_mut(&key)?;
                if !entry
                    .debounce_cancel
                    .as_ref()
                    .is_some_and(|active| Arc::ptr_eq(active, &cancellation))
                {
                    return None;
                }
                let Some(last_event_at) = entry.last_event_at else {
                    entry.debounce_cancel = None;
                    return None;
                };
                let elapsed = last_event_at.elapsed();
                if elapsed < REPOSITORY_EVENT_DEBOUNCE {
                    return Some(Err(REPOSITORY_EVENT_DEBOUNCE - elapsed));
                }
                let reason = entry
                    .pending_change_reason
                    .take()
                    .unwrap_or_else(|| "filesystem_watcher".to_string());
                entry.debounce_cancel = None;
                Some(Ok((entry.clone(), reason)))
            });
        match next {
            Some(Err(wait)) => thread::sleep(wait),
            Some(Ok(snapshot)) => break snapshot,
            None => return,
        }
    };
    let (snapshot, reason) = snapshot;
    let covered_by_completed_scan = snapshot.last_event_at.is_some_and(|event_at| {
        completed_status_scan_covers_event(&shared, &snapshot.repository_id, event_at)
    });
    if covered_by_completed_scan {
        return;
    }
    let generation = shared.git.mark_repository_changed(&snapshot.repository);
    // Let an exact scan already in progress complete. Cancelling on every filesystem event can
    // starve repositories with generated files or active agents; completion reconciles the
    // snapshot to the latest generation before publishing it.
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
                reason,
            },
        );
    }
}

fn completed_status_scan_covers_event(
    shared: &FiveTrackShared,
    repository_id: &str,
    event_at: Instant,
) -> bool {
    shared
        .git_status_cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(repository_id).cloned())
        .is_some_and(|cached| cached.scan_started_at >= event_at)
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
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        FindCloseChangeNotification, FindFirstChangeNotificationW, FILE_NOTIFY_CHANGE_ATTRIBUTES,
        FILE_NOTIFY_CHANGE_CREATION, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
        FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
    };

    let cancellation = Arc::new(RepositoryWatchCancellation::new()?);
    let mut handles = Vec::new();
    for path in repository_watch_roots(root) {
        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let handle = unsafe {
            FindFirstChangeNotificationW(
                wide.as_ptr(),
                1,
                FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_DIR_NAME
                    | FILE_NOTIFY_CHANGE_ATTRIBUTES
                    | FILE_NOTIFY_CHANGE_SIZE
                    | FILE_NOTIFY_CHANGE_LAST_WRITE
                    | FILE_NOTIFY_CHANGE_CREATION,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            for handle in handles {
                unsafe {
                    FindCloseChangeNotification(handle as *mut std::ffi::c_void);
                }
            }
            return Err(format!("failed to watch {}", path.display()));
        }
        handles.push(handle as usize);
    }

    let expected = handles.len();
    let mut started = 0usize;
    for handle in handles {
        if !cancellation.register(handle) {
            unsafe {
                FindCloseChangeNotification(handle as *mut std::ffi::c_void);
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
                    FindCloseChangeNotification(handle as *mut std::ffi::c_void);
                }
            }
        }
    }
    if started != expected {
        cancellation.request();
        Err("failed to start every repository watcher thread".to_string())
    } else {
        Ok(cancellation)
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
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
    use windows_sys::Win32::Storage::FileSystem::{
        FindCloseChangeNotification, FindNextChangeNotification,
    };
    use windows_sys::Win32::System::Threading::{WaitForMultipleObjects, INFINITE};

    let handle = handle as *mut std::ffi::c_void;
    let handle_id = handle as usize;
    let wait_handles = [handle, cancellation.cancel_event()];
    loop {
        match unsafe { WaitForMultipleObjects(2, wait_handles.as_ptr(), 0, INFINITE) } {
            WAIT_OBJECT_0 => {
                if cancellation.requested.load(Ordering::Acquire) {
                    break;
                }
                signal_repository_changed(&shared, &key, "filesystem_watcher", app.as_ref());
                if unsafe { FindNextChangeNotification(handle) } == 0 {
                    mark_repository_watcher_failed(&shared, &key, &cancellation);
                    break;
                }
            }
            result if result == WAIT_OBJECT_0 + 1 => break,
            _ => {
                mark_repository_watcher_failed(&shared, &key, &cancellation);
                break;
            }
        }
    }
    cancellation.unregister(handle_id);
    unsafe {
        FindCloseChangeNotification(handle);
    }
}

fn mark_repository_watcher_failed(
    shared: &Arc<FiveTrackShared>,
    key: &str,
    cancellation: &Arc<RepositoryWatchCancellation>,
) {
    cancellation.fail();
    let Ok(mut observed) = shared.observed_repositories.lock() else {
        return;
    };
    let Some(entry) = observed.get_mut(key) else {
        return;
    };
    let is_current = entry
        .watch_cancel
        .as_ref()
        .map(|current| Arc::ptr_eq(current, cancellation))
        .unwrap_or(false);
    if is_current {
        entry.watch_cancel = None;
        entry.monitor_mode = RepositoryMonitorMode::Pending;
        entry.next_fallback_status_at = Instant::now();
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
    #[serde(default)]
    journal_version: u8,
    source_workspace_id: String,
    repository_host: String,
    #[serde(default)]
    distribution: Option<String>,
    #[serde(default)]
    worktree_owned: bool,
    #[serde(default)]
    worktree_preexisting: Option<bool>,
    #[serde(default)]
    worktree_create_started: bool,
    #[serde(default)]
    worktree_create_succeeded: bool,
    #[serde(default)]
    ownership_uncertain: bool,
    #[serde(default)]
    branch_preexisting: Option<bool>,
    #[serde(default)]
    branch_owned: bool,
    #[serde(default)]
    created_branch_head: Option<String>,
    #[serde(default)]
    workspace_owned: bool,
    #[serde(default)]
    session_owned: bool,
    #[serde(default)]
    pane_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreparedWorktreeRecovery {
    Create,
    ContinueOwned,
    RejectPreexisting,
    RejectUncertain,
}

fn prepared_worktree_recovery(
    ownership: &WorktreeOwnership,
    destination_registered: bool,
) -> PreparedWorktreeRecovery {
    if !destination_registered {
        return if ownership.worktree_create_succeeded || ownership.worktree_owned {
            PreparedWorktreeRecovery::RejectUncertain
        } else {
            PreparedWorktreeRecovery::Create
        };
    }
    if ownership.journal_version >= 2
        && ownership.worktree_create_succeeded
        && ownership.worktree_owned
        && !ownership.ownership_uncertain
    {
        return PreparedWorktreeRecovery::ContinueOwned;
    }
    if ownership.journal_version >= 2
        && !ownership.worktree_create_started
        && ownership.worktree_preexisting != Some(false)
    {
        return PreparedWorktreeRecovery::RejectPreexisting;
    }
    PreparedWorktreeRecovery::RejectUncertain
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
            let existing = if existing.state == WorktreeOperationState::Removed {
                let mut ownership = worktree_ownership(&existing)?;
                ownership.worktree_owned = false;
                ownership.worktree_preexisting = None;
                ownership.worktree_create_started = false;
                ownership.worktree_create_succeeded = false;
                ownership.ownership_uncertain = false;
                ownership.branch_preexisting = None;
                ownership.branch_owned = false;
                ownership.created_branch_head = None;
                ownership.workspace_owned = false;
                ownership.session_owned = false;
                ownership.pane_id = None;
                self.store
                    .lock()
                    .map_err(|_| {
                        DesktopHostError::StateUnavailable(
                            "desktop store state is unavailable".to_string(),
                        )
                    })?
                    .restart_removed_worktree_operation(
                        &existing.operation_id,
                        &serde_json::to_string(&ownership)?,
                        &timestamp(),
                    )?
            } else {
                existing
            };
            let mut result = self.resume_worktree_operation(existing)?;
            result.reused = true;
            return Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result));
        }

        let (context, _) = self.workspace_vcs_context(&params.workspace_id, None)?;
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
            journal_version: 2,
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
        if operation.state == WorktreeOperationState::Removed {
            return Ok(ResponseEnvelope::ok_typed(
                request.id.clone(),
                &worktree_result(&operation, true, false)?,
            ));
        }
        self.rollback_worktree_resources(&operation, params.force, false)?;
        let mut ownership = worktree_ownership(&operation)?;
        ownership.worktree_owned = false;
        ownership.worktree_preexisting = None;
        ownership.worktree_create_started = false;
        ownership.worktree_create_succeeded = false;
        ownership.ownership_uncertain = false;
        ownership.branch_preexisting = None;
        ownership.branch_owned = false;
        ownership.created_branch_head = None;
        ownership.workspace_owned = false;
        ownership.session_owned = false;
        ownership.pane_id = None;
        let removed = self
            .store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .mark_worktree_operation_removed(
                &operation.operation_id,
                &serde_json::to_string(&ownership)?,
                &timestamp(),
            )?;
        let result = worktree_result(&removed, false, false)?;
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
            if operation.state == WorktreeOperationState::Removed {
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
                let ownership = worktree_ownership(&operation)?;
                match prepared_worktree_recovery(&ownership, existing.is_some()) {
                    PreparedWorktreeRecovery::ContinueOwned => {}
                    PreparedWorktreeRecovery::Create => {
                        let branch_head = self
                            .five_track
                            .shared
                            .git
                            .local_branch_head(&repository, &params.branch)
                            .map_err(vcs_error)?;
                        operation = self.update_worktree_ownership(&operation, |ownership| {
                            ownership.journal_version = 2;
                            ownership.worktree_preexisting = Some(false);
                            ownership.worktree_create_started = true;
                            ownership.worktree_create_succeeded = false;
                            ownership.worktree_owned = false;
                            ownership.ownership_uncertain = false;
                            ownership.branch_preexisting = Some(branch_head.is_some());
                            ownership.branch_owned = false;
                            ownership.created_branch_head = None;
                        })?;
                        if params.create_branch && branch_head.is_some() {
                            return Err(DesktopHostError::Control(ControlError::new(
                                ErrorCode::Conflict,
                                "The requested branch existed before AgentMux began the worktree operation.",
                            )));
                        }
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
                        let created = self
                            .five_track
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
                        let created_head = created.worktree.head().map(str::to_string);
                        operation = self.update_worktree_ownership(&operation, |ownership| {
                            ownership.worktree_create_succeeded = true;
                            ownership.worktree_owned = true;
                            ownership.branch_owned =
                                params.create_branch && ownership.branch_preexisting == Some(false);
                            ownership.created_branch_head = created_head;
                        })?;
                    }
                    PreparedWorktreeRecovery::RejectPreexisting => {
                        self.update_worktree_ownership(&operation, |ownership| {
                            ownership.worktree_preexisting = Some(true);
                            ownership.worktree_owned = false;
                            ownership.branch_owned = false;
                        })?;
                        return Err(DesktopHostError::Control(ControlError::new(
                            ErrorCode::Conflict,
                            "The worktree destination existed before AgentMux began this operation.",
                        )));
                    }
                    PreparedWorktreeRecovery::RejectUncertain => {
                        self.update_worktree_ownership(&operation, |ownership| {
                            ownership.ownership_uncertain = true;
                            ownership.worktree_owned = false;
                            ownership.branch_owned = false;
                        })?;
                        return Err(DesktopHostError::Control(ControlError::new(
                            ErrorCode::Conflict,
                            "Worktree creation outcome is uncertain after recovery; AgentMux will not adopt or delete the destination.",
                        )));
                    }
                }
                if existing
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
                operation = self.transition_worktree(
                    &operation,
                    WorktreeOperationState::WorktreeCreated,
                    None,
                )?;
            }

            if operation.state == WorktreeOperationState::WorktreeCreated {
                let workspace_id =
                    if let Some(existing) = self.find_managed_worktree_workspace(&operation)? {
                        operation = self.update_worktree_resources(
                            &operation,
                            Some(&existing),
                            None,
                            None,
                            |ownership| ownership.workspace_owned = true,
                        )?;
                        existing
                    } else {
                        let reserved = operation
                            .workspace_id
                            .clone()
                            .unwrap_or_else(|| WorkspaceId::new().to_string());
                        operation = self.update_worktree_resources(
                            &operation,
                            Some(&reserved),
                            None,
                            None,
                            |ownership| ownership.workspace_owned = true,
                        )?;
                        self.create_managed_worktree_workspace(&operation, &params, &reserved)?;
                        reserved
                    };
                debug_assert_eq!(
                    operation.workspace_id.as_deref(),
                    Some(workspace_id.as_str())
                );
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
                operation = match self.update_worktree_resources(
                    &operation,
                    operation.workspace_id.as_deref(),
                    placement.surface_id.as_deref(),
                    placement.session_id.as_deref(),
                    |ownership| {
                        ownership.session_owned = placement.session_id.is_some();
                        ownership.pane_id = Some(placement.pane_id.clone());
                    },
                ) {
                    Ok(updated) => updated,
                    Err(error) => {
                        if let Some(session_id) = placement.session_id.as_deref() {
                            self.terminate_runtime_session(session_id, TerminationMode::Kill);
                        }
                        return Err(error);
                    }
                };
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
                let latest = self
                    .load_worktree_operation_required(&operation.operation_id)
                    .unwrap_or(operation);
                let failed = self
                    .mark_worktree_failed(&latest, &message)
                    .unwrap_or(latest);
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
        workspace_id: &str,
    ) -> Result<String, DesktopHostError> {
        let now = timestamp();
        let pane_id = PaneId::new().to_string();
        let source = self.load_workspace_or_not_found(&params.workspace_id)?;
        let bundle = WorkspaceBundle {
            workspace: PersistedWorkspace {
                workspace_id: workspace_id.to_string(),
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
                workspace_id: workspace_id.to_string(),
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
        Ok(workspace_id.to_string())
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
        let mut worktree_removed = true;
        if ownership.worktree_owned {
            match self.remove_owned_worktree(operation, force) {
                Ok(()) => {}
                Err(error) => {
                    worktree_removed = false;
                    errors.push(error.to_string());
                }
            }
        }
        if ownership.branch_owned && worktree_removed {
            match self.remove_owned_branch(operation, force) {
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
            .find(|worktree| {
                worktree_paths_equal(repository.host(), worktree.path(), &operation.worktree_path)
            });
        if let Some(registered) = registered {
            let expected_branch = operation
                .branch_name
                .as_deref()
                .ok_or_else(|| control_invalid_request("Owned worktree branch is missing."))?;
            if registered.branch().is_none_or(|branch| {
                branch != expected_branch && branch != format!("refs/heads/{expected_branch}")
            }) {
                return Err(DesktopHostError::Control(ControlError::new(
                    ErrorCode::Conflict,
                    "Managed worktree ownership drifted to another branch; automatic deletion was refused.",
                )));
            }
            self.five_track
                .shared
                .git
                .remove_worktree(&repository, &operation.worktree_path, force)
                .map_err(vcs_error)?;
        }
        Ok(())
    }

    fn remove_owned_branch(
        &self,
        operation: &PersistedWorktreeOperation,
        force: bool,
    ) -> Result<(), DesktopHostError> {
        let ownership = worktree_ownership(operation)?;
        if !ownership.branch_owned {
            return Ok(());
        }
        let branch = operation
            .branch_name
            .as_deref()
            .ok_or_else(|| control_invalid_request("Owned worktree branch is missing."))?;
        let context = worktree_operation_git_context(operation)?;
        let repository = self
            .five_track
            .shared
            .git
            .require_repository(&context)
            .map_err(vcs_error)?;
        let Some(current_head) = self
            .five_track
            .shared
            .git
            .local_branch_head(&repository, branch)
            .map_err(vcs_error)?
        else {
            return Ok(());
        };
        if !owned_branch_head_is_unchanged(ownership.created_branch_head.as_deref(), &current_head)
        {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::Conflict,
                "Managed branch advanced after AgentMux created it; automatic deletion was refused.",
            )));
        }
        self.five_track
            .shared
            .git
            .delete_local_branch(&repository, branch, force)
            .map_err(vcs_error)?;
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

fn owned_branch_head_is_unchanged(created_head: Option<&str>, current_head: &str) -> bool {
    created_head.is_some_and(|created_head| created_head == current_head)
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
        let root = normalized_posix_path_segments(worktree_path);
        let requested_segments = normalized_posix_path_segments(requested);
        if let (Some(root), Some(requested_segments)) = (root, requested_segments) {
            if requested_segments.starts_with(&root) {
                return Ok(requested.to_string());
            }
        }
    } else {
        let root = PathBuf::from(worktree_path);
        let requested_path = PathBuf::from(requested);
        let contains_parent = requested_path
            .components()
            .any(|component| component == Component::ParentDir);
        let canonical_root = fs::canonicalize(&root);
        let canonical_requested = fs::canonicalize(&requested_path);
        if !contains_parent
            && canonical_root
                .as_ref()
                .ok()
                .zip(canonical_requested.as_ref().ok())
                .is_some_and(|(root, requested)| requested.starts_with(root))
        {
            return Ok(requested.to_string());
        }
    }
    Err(control_invalid_request(
        "Worktree terminal cwd must stay inside the managed worktree.",
    ))
}

fn normalized_posix_path_segments(path: &str) -> Option<Vec<&str>> {
    if !path.starts_with('/') || path.contains('\\') {
        return None;
    }
    let mut segments = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => return None,
            value => segments.push(value),
        }
    }
    Some(segments)
}

fn worktree_paths_equal(host: &VcsGitHost, left: &str, right: &str) -> bool {
    match host {
        VcsGitHost::Native => paths_equivalent(Path::new(left), Path::new(right)),
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
        let (_, repository) = self.resolve_vcs_repository(
            &params.workspace_id,
            params.pane_id.as_deref(),
            params.repository_id.as_deref(),
        )?;
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
        let (_, repository) = self.resolve_vcs_repository(
            &params.workspace_id,
            params.pane_id.as_deref(),
            params.repository_id.as_deref(),
        )?;
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
        let _delivery_guard = self
            .five_track
            .shared
            .review_delivery_guard
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable(
                    "review delivery state is unavailable".to_string(),
                )
            })?;
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
            let workspace_id = thread
                .workspace_id
                .as_deref()
                .ok_or_else(|| control_invalid_request("Review thread has no workspace."))?;
            let owns_session = store
                .load_workspace_bundle(workspace_id)?
                .is_some_and(|bundle| {
                    bundle
                        .sessions
                        .iter()
                        .any(|session| session.session_id == target_session_id)
                });
            if !owns_session {
                return Err(control_invalid_request(
                    "Review workspace does not own the target session.",
                ));
            }
            (thread, comments)
        };
        let same_target = thread.target_kind.as_deref() == Some(params.target.as_str())
            && thread.target_id.as_deref() == Some(target_session_id);
        let all_comments_delivered = !comments.is_empty()
            && comments.iter().all(|comment| {
                comment.delivery_state == "delivered"
                    && comment.target_kind.as_deref() == Some(params.target.as_str())
                    && comment.target_id.as_deref() == Some(target_session_id)
            });
        if same_target && thread.delivery_state == "delivered" && all_comments_delivered {
            let delivered_at = comments
                .iter()
                .filter_map(|comment| comment.delivered_at.as_deref())
                .max()
                .unwrap_or(thread.updated_at.as_str())
                .to_string();
            return Ok(ResponseEnvelope::ok_typed(
                request.id.clone(),
                &GitReviewDeliveryResult {
                    thread_id: thread.thread_id,
                    target: params.target,
                    target_session_id: Some(target_session_id.to_string()),
                    delivered_at,
                },
            ));
        }
        let body = format_review_delivery(&thread, &comments, params.include_context)?;
        let payload_hash = review_delivery_payload_hash(
            &thread.thread_id,
            &params.target,
            target_session_id,
            &body,
        );
        let (attempt, already_sending) = self.prepare_review_delivery_attempt(
            &thread,
            &params.target,
            target_session_id,
            &payload_hash,
        )?;

        if attempt.state == "confirmed" {
            let delivered_at = attempt
                .completed_at
                .clone()
                .unwrap_or_else(|| attempt.updated_at.clone());
            return Ok(ResponseEnvelope::ok_typed(
                request.id.clone(),
                &GitReviewDeliveryResult {
                    thread_id: thread.thread_id,
                    target: params.target,
                    target_session_id: Some(target_session_id.to_string()),
                    delivered_at,
                },
            ));
        }

        if already_sending {
            let mailbox_recorded = if params.target == "mailbox" {
                let message_id = review_mailbox_message_id(&attempt.attempt_id);
                self.store
                    .lock()
                    .map_err(|_| {
                        DesktopHostError::StateUnavailable(
                            "desktop store state is unavailable".to_string(),
                        )
                    })?
                    .load_team_message(&message_id)?
                    .is_some()
            } else {
                false
            };
            match interrupted_review_delivery_action(&params.target, mailbox_recorded) {
                InterruptedReviewDeliveryAction::Confirm => {
                    let delivered_at = timestamp();
                    self.confirm_review_delivery_attempt(&attempt, &delivered_at)?;
                    return Ok(ResponseEnvelope::ok_typed(
                        request.id.clone(),
                        &GitReviewDeliveryResult {
                            thread_id: thread.thread_id,
                            target: params.target,
                            target_session_id: Some(target_session_id.to_string()),
                            delivered_at,
                        },
                    ));
                }
                InterruptedReviewDeliveryAction::MarkUncertain => {
                    self.mark_review_delivery_uncertain(
                        &attempt,
                        "AgentMux restarted while terminal delivery was in flight; automatic resend is disabled to avoid duplicate input.",
                    )?;
                    return Err(review_delivery_uncertain_error());
                }
                InterruptedReviewDeliveryAction::Dispatch => {}
            }
        }

        if !already_sending {
            self.mark_review_delivery_sending(&attempt)?;
        }
        let delivery_result = match params.target.as_str() {
            "terminal" => self.send_paste_direct(target_session_id, format!("{body}\r"), true),
            "mailbox" => self.deliver_review_mailbox(&attempt, &thread, target_session_id, &body),
            _ => unreachable!(),
        };
        if let Err(error) = delivery_result {
            if review_delivery_failure_disposition(&params.target)
                == ReviewDeliveryFailureDisposition::MarkUncertain
            {
                let reason = format!(
                    "Terminal delivery returned an error after dispatch began; partial delivery cannot be excluded: {error}"
                );
                self.mark_review_delivery_uncertain(&attempt, &reason)?;
                return Err(review_delivery_uncertain_error());
            }
            let failed_at = timestamp();
            let failed = GitReviewDeliveryUpdate {
                target_kind: Some(params.target.clone()),
                target_id: Some(target_session_id.to_string()),
                delivery_state: "failed".to_string(),
                delivery_error: Some(error.to_string()),
                delivered_at: None,
                updated_at: failed_at.clone(),
            };
            if let Ok(mut store) = self.store.lock() {
                let _ = store.update_git_review_delivery_attempt(
                    &attempt.attempt_id,
                    "sending",
                    "failed",
                    Some(&error.to_string()),
                    Some(&failed_at),
                    &failed,
                );
            }
            return Err(error);
        }

        let delivered_at = timestamp();
        self.confirm_review_delivery_attempt(&attempt, &delivered_at)?;
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

    fn prepare_review_delivery_attempt(
        &self,
        thread: &PersistedGitReviewThread,
        target: &str,
        target_session_id: &str,
        payload_hash: &str,
    ) -> Result<(PersistedGitReviewDeliveryAttempt, bool), DesktopHostError> {
        let mut store = self.store.lock().map_err(|_| {
            DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
        })?;
        let latest = store.load_latest_git_review_delivery_attempt(
            &thread.thread_id,
            payload_hash,
            target,
            target_session_id,
        )?;
        let latest_for_target = store.load_latest_git_review_delivery_attempt_for_target(
            &thread.thread_id,
            target,
            target_session_id,
        )?;
        if let Some(attempt) = latest.as_ref() {
            match attempt.state.as_str() {
                "confirmed" => return Ok((attempt.clone(), true)),
                "prepared" => return Ok((attempt.clone(), false)),
                "sending" => return Ok((attempt.clone(), true)),
                "uncertain" => return Err(review_delivery_uncertain_error()),
                "failed" => {}
                _ => {
                    return Err(DesktopHostError::StateUnavailable(
                        "review delivery journal contains an invalid state".to_string(),
                    ));
                }
            }
        } else if matches!(thread.delivery_state.as_str(), "delivering" | "uncertain")
            && thread.target_kind.as_deref() == Some(target)
            && thread.target_id.as_deref() == Some(target_session_id)
            && latest_for_target.is_none()
        {
            let now = timestamp();
            let reason =
                "Legacy delivery was interrupted before a payload attempt journal was recorded.";
            let attempt = PersistedGitReviewDeliveryAttempt {
                attempt_id: format!("review_delivery_{}", unique_time_id()),
                thread_id: thread.thread_id.clone(),
                payload_hash: payload_hash.to_string(),
                target_kind: target.to_string(),
                target_id: target_session_id.to_string(),
                attempt_number: 1,
                state: "uncertain".to_string(),
                error_message: Some(reason.to_string()),
                created_at: now.clone(),
                updated_at: now.clone(),
                completed_at: Some(now.clone()),
            };
            let delivery = GitReviewDeliveryUpdate {
                target_kind: Some(target.to_string()),
                target_id: Some(target_session_id.to_string()),
                delivery_state: "uncertain".to_string(),
                delivery_error: Some(reason.to_string()),
                delivered_at: None,
                updated_at: now,
            };
            if !store.begin_git_review_delivery_attempt(&attempt, &delivery)? {
                return Err(control_invalid_request("Git review thread not found."));
            }
            return Err(review_delivery_uncertain_error());
        }

        let now = timestamp();
        let attempt_number = latest
            .as_ref()
            .map(|attempt| attempt.attempt_number + 1)
            .unwrap_or(1);
        let attempt = PersistedGitReviewDeliveryAttempt {
            attempt_id: format!("review_delivery_{}", unique_time_id()),
            thread_id: thread.thread_id.clone(),
            payload_hash: payload_hash.to_string(),
            target_kind: target.to_string(),
            target_id: target_session_id.to_string(),
            attempt_number,
            state: "prepared".to_string(),
            error_message: None,
            created_at: now.clone(),
            updated_at: now.clone(),
            completed_at: None,
        };
        let delivery = GitReviewDeliveryUpdate {
            target_kind: Some(target.to_string()),
            target_id: Some(target_session_id.to_string()),
            delivery_state: "delivering".to_string(),
            delivery_error: None,
            delivered_at: None,
            updated_at: now,
        };
        if !store.begin_git_review_delivery_attempt(&attempt, &delivery)? {
            return Err(control_invalid_request("Git review thread not found."));
        }
        Ok((attempt, false))
    }

    fn mark_review_delivery_sending(
        &self,
        attempt: &PersistedGitReviewDeliveryAttempt,
    ) -> Result<(), DesktopHostError> {
        let now = timestamp();
        let delivery = GitReviewDeliveryUpdate {
            target_kind: Some(attempt.target_kind.clone()),
            target_id: Some(attempt.target_id.clone()),
            delivery_state: "delivering".to_string(),
            delivery_error: None,
            delivered_at: None,
            updated_at: now,
        };
        let updated = self
            .store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .update_git_review_delivery_attempt(
                &attempt.attempt_id,
                "prepared",
                "sending",
                None,
                None,
                &delivery,
            )?;
        if !updated {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::Conflict,
                "Review delivery state changed before dispatch.",
            )));
        }
        Ok(())
    }

    fn mark_review_delivery_uncertain(
        &self,
        attempt: &PersistedGitReviewDeliveryAttempt,
        reason: &str,
    ) -> Result<(), DesktopHostError> {
        let now = timestamp();
        let delivery = GitReviewDeliveryUpdate {
            target_kind: Some(attempt.target_kind.clone()),
            target_id: Some(attempt.target_id.clone()),
            delivery_state: "uncertain".to_string(),
            delivery_error: Some(reason.to_string()),
            delivered_at: None,
            updated_at: now.clone(),
        };
        let updated = self
            .store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .update_git_review_delivery_attempt(
                &attempt.attempt_id,
                "sending",
                "uncertain",
                Some(reason),
                Some(&now),
                &delivery,
            )?;
        if !updated {
            return Err(DesktopHostError::Control(ControlError::new(
                ErrorCode::Conflict,
                "Review delivery state changed before interrupted delivery reconciliation.",
            )));
        }
        Ok(())
    }

    fn confirm_review_delivery_attempt(
        &self,
        attempt: &PersistedGitReviewDeliveryAttempt,
        delivered_at: &str,
    ) -> Result<(), DesktopHostError> {
        let update = GitReviewDeliveryUpdate {
            target_kind: Some(attempt.target_kind.clone()),
            target_id: Some(attempt.target_id.clone()),
            delivery_state: "delivered".to_string(),
            delivery_error: None,
            delivered_at: Some(delivered_at.to_string()),
            updated_at: delivered_at.to_string(),
        };
        let updated = self
            .store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .update_git_review_delivery_attempt(
                &attempt.attempt_id,
                "sending",
                "confirmed",
                None,
                Some(delivered_at),
                &update,
            )?;
        if !updated {
            let current = self
                .store
                .lock()
                .map_err(|_| {
                    DesktopHostError::StateUnavailable(
                        "desktop store state is unavailable".to_string(),
                    )
                })?
                .load_latest_git_review_delivery_attempt(
                    &attempt.thread_id,
                    &attempt.payload_hash,
                    &attempt.target_kind,
                    &attempt.target_id,
                )?;
            if current
                .as_ref()
                .is_none_or(|current| current.state != "confirmed")
            {
                return Err(DesktopHostError::StateUnavailable(
                    "review delivery confirmation could not be persisted".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn deliver_review_mailbox(
        &self,
        attempt: &PersistedGitReviewDeliveryAttempt,
        thread: &PersistedGitReviewThread,
        target_session_id: &str,
        body: &str,
    ) -> Result<(), DesktopHostError> {
        let workspace_id = thread
            .workspace_id
            .clone()
            .ok_or_else(|| control_invalid_request("Review thread has no workspace."))?;
        let now = timestamp();
        let message = PersistedTeamMessage {
            message_id: review_mailbox_message_id(&attempt.attempt_id),
            workspace_id: workspace_id.clone(),
            thread_id: Some(thread.thread_id.clone()),
            from_session_id: None,
            to_session_id: Some(target_session_id.to_string()),
            body: body.to_string(),
            kind: "git_review".to_string(),
            created_at: now.clone(),
            read_at: None,
        };
        let notification = PersistedNotification {
            notification_id: review_mailbox_notification_id(&attempt.attempt_id),
            notification_type: "team.message".to_string(),
            severity: "info".to_string(),
            workspace_id: Some(workspace_id),
            session_id: Some(target_session_id.to_string()),
            title: "Agent message".to_string(),
            message: body.to_string(),
            created_at: now,
            dismissed: false,
        };
        self.store
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?
            .upsert_team_message_and_notification(&message, &notification)?;
        self.dispatch_desktop_notification(&notification_result_from_persisted(&notification));
        Ok(())
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
        author_session_id: (thread.author_id != "user").then(|| thread.author_id.clone()),
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

fn review_delivery_payload_hash(
    thread_id: &str,
    target: &str,
    target_session_id: &str,
    body: &str,
) -> String {
    let body_hash = stable_hash_bytes(body.as_bytes());
    let component = |domain: u8| {
        stable_hash(&(
            domain,
            thread_id,
            target,
            target_session_id,
            body_hash,
            body.len(),
        ))
    };
    format!(
        "{:016x}{:016x}{:016x}{:016x}",
        component(0),
        component(1),
        component(2),
        component(3)
    )
}

fn review_mailbox_message_id(attempt_id: &str) -> String {
    format!("msg_{attempt_id}")
}

fn review_mailbox_notification_id(attempt_id: &str) -> String {
    format!("not_{attempt_id}")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InterruptedReviewDeliveryAction {
    Dispatch,
    Confirm,
    MarkUncertain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReviewDeliveryFailureDisposition {
    RetryableFailure,
    MarkUncertain,
}

fn interrupted_review_delivery_action(
    target: &str,
    mailbox_recorded: bool,
) -> InterruptedReviewDeliveryAction {
    match (target, mailbox_recorded) {
        ("mailbox", true) => InterruptedReviewDeliveryAction::Confirm,
        ("mailbox", false) => InterruptedReviewDeliveryAction::Dispatch,
        _ => InterruptedReviewDeliveryAction::MarkUncertain,
    }
}

fn review_delivery_failure_disposition(target: &str) -> ReviewDeliveryFailureDisposition {
    if target == "mailbox" {
        ReviewDeliveryFailureDisposition::RetryableFailure
    } else {
        ReviewDeliveryFailureDisposition::MarkUncertain
    }
}

fn review_delivery_uncertain_error() -> DesktopHostError {
    DesktopHostError::Control(ControlError::new(
        ErrorCode::Conflict,
        "A previous terminal delivery may already have been sent. Automatic resend is disabled; edit the review payload before retrying.",
    ))
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
        let record = VerifiedHookRecord {
            workspace_id: params.workspace_id.clone(),
            session_id: params.session_id.clone(),
            source: params.source.clone(),
            sequence: params.sequence,
            state: state.clone(),
            reason: params.reason.clone(),
            telemetry: params.telemetry.clone(),
        };
        {
            let mut store = self.store.lock().map_err(|_| {
                DesktopHostError::StateUnavailable("desktop store state is unavailable".to_string())
            })?;
            if let Some(previous) = store.load_verified_agent_hook(&params.session_id)? {
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
            store.upsert_verified_agent_hook(&persisted_verified_hook(&record)?)?;
        }
        self.five_track
            .shared
            .verified_hooks
            .lock()
            .map_err(|_| {
                DesktopHostError::StateUnavailable("verified hook state is unavailable".to_string())
            })?
            .insert(params.session_id.clone(), record.clone());
        self.apply_verified_hook_record(&record, &request.id)?;
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
        let hooks = if let Ok(mut store) = self.store.lock() {
            let persisted = store.list_verified_agent_hooks().unwrap_or_default();
            let mut active = Vec::new();
            for hook in persisted {
                let session_active = store
                    .load_session(&hook.session_id)
                    .ok()
                    .flatten()
                    .is_some_and(|session| !is_terminal_state(&session.state));
                if session_active {
                    if let Ok(record) = verified_hook_from_persisted(&hook) {
                        active.push(record);
                    }
                } else {
                    let _ = store.delete_verified_agent_hook(&hook.session_id);
                }
            }
            active
        } else {
            Vec::new()
        };
        if hooks.is_empty() {
            return;
        }
        if let Ok(mut memory) = self.five_track.shared.verified_hooks.lock() {
            memory.extend(
                hooks
                    .iter()
                    .cloned()
                    .map(|hook| (hook.session_id.clone(), hook)),
            );
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
            if let Ok(mut store) = self.store.lock() {
                for session_id in &terminal_sessions {
                    let _ = store.delete_verified_agent_hook(session_id);
                }
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
                source: pty_dev_server_source(candidate.process_id).to_string(),
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
                "Development server URL must use http or https on localhost or a loopback address.",
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
        let bookkeeping = (|| {
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
                .ok_or_else(|| {
                    control_invalid_request("Development server candidate disappeared.")
                })?;
            current.opened_surface_id = Some(surface.surface_id.clone());
            Ok::<_, DesktopHostError>(current.clone())
        })();
        let result = match bookkeeping {
            Ok(result) => result,
            Err(error) => {
                let _ = self.close_browser_surface_if_present(&surface.surface_id);
                if let Ok(mut store) = self.store.lock() {
                    let _ = store.save_workspace_bundle(&original);
                }
                return Err(error);
            }
        };
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

fn persisted_verified_hook(
    record: &VerifiedHookRecord,
) -> Result<PersistedVerifiedAgentHook, DesktopHostError> {
    Ok(PersistedVerifiedAgentHook {
        session_id: record.session_id.clone(),
        workspace_id: record.workspace_id.clone(),
        source: record.source.clone(),
        sequence: record.sequence,
        state: record.state.clone(),
        reason: record.reason.clone(),
        telemetry_json: record
            .telemetry
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?,
        updated_at: timestamp(),
    })
}

fn verified_hook_from_persisted(
    hook: &PersistedVerifiedAgentHook,
) -> Result<VerifiedHookRecord, DesktopHostError> {
    Ok(VerifiedHookRecord {
        workspace_id: hook.workspace_id.clone(),
        session_id: hook.session_id.clone(),
        source: hook.source.clone(),
        sequence: hook.sequence,
        state: hook.state.clone(),
        reason: hook.reason.clone(),
        telemetry: hook
            .telemetry_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?,
    })
}

fn unix_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

fn pty_dev_server_source(process_id: Option<u32>) -> &'static str {
    if process_id.is_some() {
        "pty_process_attributed"
    } else {
        // ConPTY and WSL output deltas currently identify the owning session,
        // not the child process that printed a URL. Keep the PID empty rather
        // than incorrectly attributing it to the shell/transport process.
        "pty_output_unattributed"
    }
}

fn is_safe_development_server_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    let Some(authority) = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"))
    else {
        return false;
    };
    let authority = authority.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') || authority.chars().any(char::is_whitespace)
    {
        return false;
    }
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        let Some(close) = bracketed.find(']') else {
            return false;
        };
        let host = &bracketed[..close];
        let suffix = &bracketed[close + 1..];
        if !suffix.is_empty() && (!suffix.starts_with(':') || suffix[1..].parse::<u16>().is_err()) {
            return false;
        }
        host
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        if host.is_empty() || port.parse::<u16>().is_err() {
            return false;
        }
        host
    } else {
        authority
    };
    host == "localhost"
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
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
                resource_uri: None,
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
        let fallback_interval = std::hint::black_box(REPOSITORY_FALLBACK_STATUS_INTERVAL);
        let watcher_starts = std::hint::black_box(MAX_WATCHER_STARTS_PER_TICK);
        let fallback_reads = std::hint::black_box(MAX_FALLBACK_STATUS_READS_PER_TICK);
        let event_debounce = std::hint::black_box(REPOSITORY_EVENT_DEBOUNCE);

        assert!(
            fallback_interval >= Duration::from_secs(30),
            "fallback Git status must remain low-frequency"
        );
        assert!(watcher_starts <= 2);
        assert_eq!(fallback_reads, 1);
        assert!(event_debounce >= Duration::from_millis(200));
    }

    #[test]
    fn repository_monitor_evicts_least_recent_entry_at_capacity() {
        let repository_path = create_git_repository("watcher-capacity");
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let repository = host
            .five_track
            .shared
            .git
            .require_repository(&GitContext::native(
                repository_path.to_string_lossy().to_string(),
            ))
            .unwrap();
        let repository_id = repository_id(&repository);
        for index in 0..MAX_OBSERVED_REPOSITORIES {
            host.observe_repository(&format!("ws_{index:03}"), &repository_id, &repository);
        }
        host.five_track
            .shared
            .observed_repositories
            .lock()
            .unwrap()
            .get_mut(&format!("ws_000|{repository_id}"))
            .unwrap()
            .last_observed_at = Instant::now() - Duration::from_secs(60);
        host.observe_repository(
            &format!("ws_{:03}", MAX_OBSERVED_REPOSITORIES),
            &repository_id,
            &repository,
        );
        let observed = host.five_track.shared.observed_repositories.lock().unwrap();
        assert_eq!(observed.len(), MAX_OBSERVED_REPOSITORIES);
        assert!(!observed.contains_key(&format!("ws_000|{repository_id}")));
        assert!(observed.contains_key(&format!(
            "ws_{:03}|{repository_id}",
            MAX_OBSERVED_REPOSITORIES
        )));
        drop(observed);
        fs::remove_dir_all(repository_path).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn reobserved_repository_invalidates_cache_after_monitor_gap() {
        let repository_path = create_git_repository("watcher-gap");
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let repository = host
            .five_track
            .shared
            .git
            .require_repository(&GitContext::native(
                repository_path.to_string_lossy().to_string(),
            ))
            .expect("resolved repository");
        let (repository_id, cached) = host
            .status_snapshot("ws_watcher_gap", &repository)
            .expect("initial cached snapshot");
        let key = format!("ws_watcher_gap|{repository_id}");
        if let Some(mut observation) = host
            .five_track
            .shared
            .observed_repositories
            .lock()
            .unwrap()
            .remove(&key)
        {
            if let Some(cancel) = observation.watch_cancel.take() {
                cancel.request();
            }
        }

        host.observe_and_arm_repository("ws_watcher_gap", &repository_id, &repository);

        assert!(
            host.five_track.shared.git.generation(&repository) > cached.snapshot.summary.generation
        );
        assert!(!host
            .five_track
            .shared
            .git_status_cache
            .lock()
            .unwrap()
            .contains_key(&repository_id));
        fs::remove_dir_all(repository_path).expect("temporary repository cleanup");
    }

    #[test]
    fn completed_status_scan_never_relabels_stale_results_after_a_worktree_change() {
        let repository_path = create_git_repository("status-overlap-change");
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
        let active = host
            .start_status_scan(&repository, &repository_id, 250)
            .expect("active status scan");
        let scan_generation = active.generation;
        assert!(host
            .five_track
            .shared
            .git_status_scans
            .lock()
            .unwrap()
            .contains_key(&repository_id));
        let snapshot = StatusSnapshot {
            summary: agentmux_vcs::StatusSummary {
                repository_root: repository.root().to_string(),
                branch: Some("main".to_string()),
                head: None,
                upstream: None,
                ahead: 0,
                behind: 0,
                file_count: 0,
                staged_count: 0,
                unstaged_count: 0,
                untracked_count: 0,
                conflict_count: 0,
                generation: scan_generation,
            },
            files: Vec::new(),
        };
        let changed_generation = host
            .five_track
            .shared
            .git
            .mark_repository_changed(&repository);

        let error = match host.install_completed_status_scan(
            "ws_overlap",
            &repository,
            &repository_id,
            scan_generation,
            active.started_at,
            Arc::new(StatusReadResult {
                snapshot,
                metrics: agentmux_vcs::StatusReadMetrics {
                    stdout_bytes: 0,
                    first_change_after: None,
                    completed_after: Duration::ZERO,
                },
            }),
        ) {
            Ok(_) => panic!("overlapping change must reject the stale completed snapshot"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            DesktopHostError::Control(ref control) if control.code == ErrorCode::Conflict
        ));
        assert_eq!(
            host.five_track.shared.git.generation(&repository),
            changed_generation
        );
        assert!(!host
            .five_track
            .shared
            .git_status_cache
            .lock()
            .unwrap()
            .contains_key(&repository_id));
        assert!(!host
            .five_track
            .shared
            .git_status_scans
            .lock()
            .unwrap()
            .contains_key(&repository_id));
        fs::remove_dir_all(repository_path).expect("temporary repository cleanup");
    }

    #[test]
    fn completed_status_scan_covers_only_earlier_watcher_events() {
        let repository_path = create_git_repository("status-covered-event");
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let repository = host
            .five_track
            .shared
            .git
            .require_repository(&GitContext::native(
                repository_path.to_string_lossy().to_string(),
            ))
            .expect("resolved repository");
        let (repository_id, cached) = host
            .status_snapshot("ws_covered", &repository)
            .expect("cached status snapshot");
        let earlier = cached
            .scan_started_at
            .checked_sub(Duration::from_millis(1))
            .expect("earlier instant");
        let during_or_later = cached.scan_started_at + Duration::from_millis(1);

        assert!(completed_status_scan_covers_event(
            &host.five_track.shared,
            &repository_id,
            earlier,
        ));
        assert!(!completed_status_scan_covers_event(
            &host.five_track.shared,
            &repository_id,
            during_or_later,
        ));
        fs::remove_dir_all(repository_path).expect("temporary repository cleanup");
    }

    fn assert_large_git_page_pipeline(file_count: usize, budget: Duration) {
        let files = (0..file_count)
            .map(|index| GitFileChange {
                path: format!("src/generated/file-{index:05}.rs"),
                original_path: None,
                index_status: if index % 3 == 0 { "M" } else { "." }.to_string(),
                worktree_status: if index % 3 == 0 { "." } else { "M" }.to_string(),
                staged: index % 3 == 0,
                unstaged: index % 3 != 0,
                untracked: false,
                conflict: false,
            })
            .collect::<Vec<_>>();
        let snapshot = StatusSnapshot {
            summary: agentmux_vcs::StatusSummary {
                repository_root: r"D:\repo".to_string(),
                branch: Some("main".to_string()),
                head: Some("0123456789abcdef".to_string()),
                upstream: Some("origin/main".to_string()),
                ahead: 0,
                behind: 0,
                file_count,
                staged_count: files.iter().filter(|change| change.staged).count(),
                unstaged_count: files.iter().filter(|change| change.unstaged).count(),
                untracked_count: 0,
                conflict_count: 0,
                generation: 1,
            },
            files,
        };
        let started = Instant::now();
        let indexes = GitStatusPageIndexes::from_snapshot(&snapshot);
        let mut serialized = 0usize;
        for page in indexes.all.chunks(250) {
            let values = page
                .iter()
                .map(|index| git_change_result(&snapshot.files[*index]))
                .collect::<Vec<_>>();
            serialized += serde_json::to_vec(&values).unwrap().len();
        }
        assert!(serialized > file_count * 32);
        assert!(
            started.elapsed() < budget,
            "{file_count} file index/page/serialization pipeline exceeded {budget:?}: {:?}",
            started.elapsed()
        );
        eprintln!(
            "git page pipeline {file_count}: completed={:?}, serialized_bytes={serialized}",
            started.elapsed(),
        );
    }

    #[test]
    fn git_page_pipeline_handles_5k_files_within_budget() {
        assert_large_git_page_pipeline(5_000, Duration::from_millis(500));
    }

    #[test]
    fn git_page_pipeline_handles_10k_files_within_budget() {
        assert_large_git_page_pipeline(10_000, Duration::from_millis(750));
    }

    #[test]
    fn git_page_pipeline_handles_15k_files_within_budget() {
        assert_large_git_page_pipeline(15_000, Duration::from_secs(1));
    }

    #[test]
    #[ignore = "performance gate creates 15,000 files; run through tools/run-performance-gates.ps1"]
    fn git_status_native_15k_returns_first_visible_page_before_completion() {
        let repository = create_git_repository("native-first-visible-15k");
        let generated = repository.join("generated");
        fs::create_dir(&generated).expect("generated fixture directory");
        for index in 0..15_000 {
            fs::write(generated.join(format!("file-{index:05}.txt")), "x")
                .expect("performance fixture");
        }
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        save_bundle(
            &host,
            &workspace_bundle(
                "ws_native_first_visible",
                Some(repository.to_string_lossy().to_string()),
                &[],
            ),
        );

        let started = Instant::now();
        let first: GitStatusPageResult = decode_ok(host.handle_request(test_request(
            "git_native_first",
            METHOD_GIT_STATUS_PAGE,
            &GitStatusPageParams {
                workspace_id: "ws_native_first_visible".to_string(),
                pane_id: None,
                repository_id: None,
                generation: None,
                state: None,
                query: None,
                cursor: None,
                limit: Some(250),
            },
        )));
        let first_visible_after = started.elapsed();
        assert!(!first.changes.is_empty());
        assert!(
            first_visible_after < Duration::from_millis(900),
            "desktop request-to-first-visible-row was {first_visible_after:?}, budget was 900ms"
        );
        let completed: GitStatusPageResult = decode_ok(host.handle_request(test_request(
            "git_native_complete",
            METHOD_GIT_STATUS_PAGE,
            &GitStatusPageParams {
                workspace_id: "ws_native_first_visible".to_string(),
                pane_id: None,
                repository_id: Some(first.repository_id),
                generation: Some(first.generation),
                state: None,
                query: None,
                cursor: None,
                limit: Some(250),
            },
        )));
        let completed_after = started.elapsed();
        assert_eq!(completed.total_count, Some(15_000));
        assert!(completed.summary.is_some());
        assert!(
            completed_after < Duration::from_secs(10),
            "desktop full 15k snapshot completed in {completed_after:?}, budget was 10s"
        );
        eprintln!(
            "desktop native Git 15k: first_visible={first_visible_after:?}, completed={completed_after:?}"
        );
        fs::remove_dir_all(repository).expect("temporary repository cleanup");
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
                pane_id: None,
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
                pane_id: None,
                repository_id: Some(summary.repository_id.clone()),
                generation: Some(summary.generation),
                state: None,
                query: None,
                cursor: None,
                limit: Some(5),
            },
        )));
        assert_eq!(first.changes.len(), 5);
        assert_eq!(first.total_count, Some(12));
        assert_eq!(
            first
                .summary
                .as_ref()
                .map(|value| value.repository_id.as_str()),
            Some(summary.repository_id.as_str())
        );

        let filtered: GitStatusPageResult = decode_ok(host.handle_request(test_request(
            "git_page_query",
            METHOD_GIT_STATUS_PAGE,
            &GitStatusPageParams {
                workspace_id: "ws_git".to_string(),
                pane_id: None,
                repository_id: Some(summary.repository_id.clone()),
                generation: Some(summary.generation),
                state: None,
                query: Some("new-10".to_string()),
                cursor: None,
                limit: Some(5),
            },
        )));
        assert_eq!(filtered.total_count, Some(1));
        assert_eq!(filtered.changes[0].path, "new-10.txt");

        let query_page: GitStatusPageResult = decode_ok(host.handle_request(test_request(
            "git_page_query_cursor",
            METHOD_GIT_STATUS_PAGE,
            &GitStatusPageParams {
                workspace_id: "ws_git".to_string(),
                pane_id: None,
                repository_id: Some(summary.repository_id.clone()),
                generation: Some(summary.generation),
                state: Some("untracked".to_string()),
                query: Some("new-".to_string()),
                cursor: None,
                limit: Some(2),
            },
        )));
        assert_eq!(query_page.total_count, Some(11));
        let mismatched_query_cursor = host.handle_request(test_request(
            "git_page_query_mismatch",
            METHOD_GIT_STATUS_PAGE,
            &GitStatusPageParams {
                workspace_id: "ws_git".to_string(),
                pane_id: None,
                repository_id: Some(summary.repository_id.clone()),
                generation: Some(summary.generation),
                state: Some("untracked".to_string()),
                query: Some("new-1".to_string()),
                cursor: query_page.next_cursor.clone(),
                limit: Some(2),
            },
        ));
        assert_eq!(error_code(mismatched_query_cursor), ErrorCode::Conflict);

        fs::write(repository.join("late.txt"), "late\n").expect("late fixture");
        let second: GitStatusPageResult = decode_ok(host.handle_request(test_request(
            "git_page_2",
            METHOD_GIT_STATUS_PAGE,
            &GitStatusPageParams {
                workspace_id: "ws_git".to_string(),
                pane_id: None,
                repository_id: Some(summary.repository_id.clone()),
                generation: Some(summary.generation),
                state: None,
                query: None,
                cursor: first.next_cursor.clone(),
                limit: Some(5),
            },
        )));
        assert_eq!(second.generation, summary.generation);
        assert_eq!(second.total_count, Some(12));

        let key = format!("ws_git|{}", summary.repository_id);
        signal_repository_changed(&host.five_track.shared, &key, "test", None);
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline
            && host
                .five_track
                .shared
                .observed_repositories
                .lock()
                .unwrap()
                .get(&key)
                .is_some_and(|entry| entry.debounce_cancel.is_some())
        {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            host.five_track
                .shared
                .observed_repositories
                .lock()
                .unwrap()
                .get(&key)
                .is_some_and(|entry| entry.debounce_cancel.is_none()),
            "the trailing-edge change signal must finish before requesting a new snapshot"
        );
        let refreshed: GitStatusSummaryResult = decode_ok(host.handle_request(test_request(
            "git_summary_refreshed",
            METHOD_GIT_STATUS_SUMMARY,
            &GitRepositoryParams {
                workspace_id: "ws_git".to_string(),
                pane_id: None,
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
    fn git_path_mutation_idempotency_reuses_only_the_same_payload() {
        let repository = create_git_repository("mutation-receipt-paths");
        fs::write(repository.join("first.txt"), "first\n").expect("first fixture");
        fs::write(repository.join("second.txt"), "second\n").expect("second fixture");
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        save_bundle(
            &host,
            &workspace_bundle(
                "ws_mutation_receipt",
                Some(repository.to_string_lossy().to_string()),
                &[],
            ),
        );
        let request = GitPathMutationParams {
            workspace_id: "ws_mutation_receipt".to_string(),
            pane_id: None,
            repository_id: None,
            paths: vec!["first.txt".to_string()],
            idempotency_key: Some("stage-first".to_string()),
        };
        let first: IpcGitMutationResult = decode_ok(host.handle_request(test_request(
            "mutation_receipt_first",
            METHOD_GIT_STAGE,
            &request,
        )));
        assert!(!first.reused);
        let retry: IpcGitMutationResult = decode_ok(host.handle_request(test_request(
            "mutation_receipt_retry",
            METHOD_GIT_STAGE,
            &request,
        )));
        assert!(retry.reused);
        assert_eq!(retry.repository_id, first.repository_id);
        assert_eq!(retry.affected_paths, first.affected_paths);

        let conflict = host.handle_request(test_request(
            "mutation_receipt_conflict",
            METHOD_GIT_STAGE,
            &GitPathMutationParams {
                paths: vec!["second.txt".to_string()],
                ..request
            },
        ));
        assert_eq!(error_code(conflict), ErrorCode::Conflict);
        fs::remove_dir_all(repository).expect("temporary repository cleanup");
    }

    #[test]
    fn git_commit_idempotency_receipt_survives_desktop_restart() {
        let repository = create_git_repository("mutation-receipt-commit");
        fs::write(repository.join("tracked.txt"), "changed\n").expect("changed fixture");
        run_git(&repository, &["add", "tracked.txt"]);
        let state_dir = temporary_path("mutation-receipt-store");
        fs::create_dir_all(&state_dir).expect("state directory");
        let database = state_dir.join("agentmux.db");
        let config = state_dir.join("agentmux.json");
        let request = IpcGitCommitParams {
            workspace_id: "ws_commit_receipt".to_string(),
            pane_id: None,
            repository_id: None,
            message: "persist receipt".to_string(),
            amend: false,
            idempotency_key: Some("commit-once".to_string()),
        };

        let first = {
            let host = DesktopControlState::open_with_token_and_config(
                &database,
                DESKTOP_CONTROL_TOKEN,
                &config,
            )
            .expect("persistent desktop state");
            save_bundle(
                &host,
                &workspace_bundle(
                    "ws_commit_receipt",
                    Some(repository.to_string_lossy().to_string()),
                    &[],
                ),
            );
            let result: IpcGitMutationResult = decode_ok(host.handle_request(test_request(
                "commit_receipt_first",
                METHOD_GIT_COMMIT,
                &request,
            )));
            assert!(!result.reused);
            result
        };

        let host = DesktopControlState::open_with_token_and_config(
            &database,
            DESKTOP_CONTROL_TOKEN,
            &config,
        )
        .expect("reopened desktop state");
        let retry: IpcGitMutationResult = decode_ok(host.handle_request(test_request(
            "commit_receipt_retry",
            METHOD_GIT_COMMIT,
            &request,
        )));
        assert!(retry.reused);
        assert_eq!(retry.commit_oid, first.commit_oid);

        let conflict = host.handle_request(test_request(
            "commit_receipt_conflict",
            METHOD_GIT_COMMIT,
            &IpcGitCommitParams {
                message: "different payload".to_string(),
                ..request
            },
        ));
        assert_eq!(error_code(conflict), ErrorCode::Conflict);
        let count = Command::new("git")
            .arg("-C")
            .arg(&repository)
            .args(["rev-list", "--count", "HEAD"])
            .output()
            .expect("git rev-list should start");
        assert!(count.status.success());
        assert_eq!(String::from_utf8_lossy(&count.stdout).trim(), "2");

        drop(host);
        fs::remove_dir_all(repository).expect("temporary repository cleanup");
        fs::remove_dir_all(state_dir).expect("temporary state cleanup");
    }

    #[test]
    fn pane_repository_resolution_rejects_cached_parent_for_nested_repository() {
        let parent = create_git_repository("nested_repository_parent");
        let nested = parent.join("nested");
        fs::create_dir_all(&nested).expect("nested repository directory");
        run_git(&nested, &["init", "-q"]);
        run_git(&nested, &["config", "user.name", "AgentMux Test"]);
        run_git(
            &nested,
            &["config", "user.email", "agentmux@example.invalid"],
        );
        fs::write(nested.join("nested.txt"), "nested\n").expect("nested fixture");
        run_git(&nested, &["add", "nested.txt"]);
        run_git(&nested, &["commit", "-q", "-m", "nested"]);

        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let mut bundle = workspace_bundle(
            "ws_nested_git",
            Some(parent.to_string_lossy().to_string()),
            &[("pane_git", "surface_git", "session_git")],
        );
        save_bundle(&host, &bundle);
        let parent_summary: GitStatusSummaryResult = decode_ok(host.handle_request(test_request(
            "parent_summary",
            METHOD_GIT_STATUS_SUMMARY,
            &GitRepositoryParams {
                workspace_id: "ws_nested_git".to_string(),
                pane_id: Some("pane_git".to_string()),
                repository_id: None,
            },
        )));

        bundle.sessions[0].cwd = Some(nested.to_string_lossy().to_string());
        save_bundle(&host, &bundle);
        let stale_parent = host.handle_request(test_request(
            "stale_parent_page",
            METHOD_GIT_STATUS_PAGE,
            &GitStatusPageParams {
                workspace_id: "ws_nested_git".to_string(),
                pane_id: Some("pane_git".to_string()),
                repository_id: Some(parent_summary.repository_id),
                generation: None,
                state: None,
                query: None,
                cursor: None,
                limit: Some(25),
            },
        ));
        assert_eq!(error_code(stale_parent), ErrorCode::InvalidRequest);

        let nested_summary: GitStatusSummaryResult = decode_ok(host.handle_request(test_request(
            "nested_summary",
            METHOD_GIT_STATUS_SUMMARY,
            &GitRepositoryParams {
                workspace_id: "ws_nested_git".to_string(),
                pane_id: Some("pane_git".to_string()),
                repository_id: None,
            },
        )));
        assert!(paths_equivalent(
            Path::new(&nested_summary.repository_root),
            &nested
        ));
        fs::remove_dir_all(parent).expect("temporary repository cleanup");
    }

    #[test]
    fn review_repository_resolution_stays_bound_to_explicit_pane_after_focus_moves() {
        let repository_a = create_git_repository("review-pane-a");
        let repository_b = create_git_repository("review-pane-b");
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let mut bundle = workspace_bundle(
            "ws_review_focus",
            Some(repository_b.to_string_lossy().to_string()),
            &[
                ("pane_a", "surface_a", "session_a"),
                ("pane_b", "surface_b", "session_b"),
            ],
        );
        bundle.sessions[0].cwd = Some(repository_a.to_string_lossy().to_string());
        bundle.sessions[1].cwd = Some(repository_b.to_string_lossy().to_string());
        bundle.workspace.active_pane_id = "pane_b".to_string();
        save_bundle(&host, &bundle);

        let summary_a: GitStatusSummaryResult = decode_ok(host.handle_request(test_request(
            "review_focus_summary_a",
            METHOD_GIT_STATUS_SUMMARY,
            &GitRepositoryParams {
                workspace_id: "ws_review_focus".to_string(),
                pane_id: Some("pane_a".to_string()),
                repository_id: None,
            },
        )));
        let summary_b: GitStatusSummaryResult = decode_ok(host.handle_request(test_request(
            "review_focus_summary_b",
            METHOD_GIT_STATUS_SUMMARY,
            &GitRepositoryParams {
                workspace_id: "ws_review_focus".to_string(),
                pane_id: Some("pane_b".to_string()),
                repository_id: None,
            },
        )));
        assert_ne!(summary_a.repository_id, summary_b.repository_id);

        let created: GitReviewThreadResult = decode_ok(host.handle_request(test_request(
            "review_focus_create_a",
            METHOD_GIT_REVIEW_THREAD_CREATE,
            &GitReviewThreadCreateParams {
                workspace_id: "ws_review_focus".to_string(),
                pane_id: Some("pane_a".to_string()),
                repository_id: Some(summary_a.repository_id.clone()),
                anchor: GitReviewLineAnchor {
                    path: "tracked.txt".to_string(),
                    side: "right".to_string(),
                    line: 1,
                    start_line: None,
                    base_revision: None,
                    head_revision: Some("HEAD".to_string()),
                    hunk_header: None,
                    diff_hash: None,
                },
                body: "Review pane A only.".to_string(),
                author_session_id: Some("session_a".to_string()),
            },
        )));
        assert_eq!(created.repository_id, summary_a.repository_id);
        assert_eq!(created.author_session_id.as_deref(), Some("session_a"));

        let listed: GitReviewThreadListResult = decode_ok(host.handle_request(test_request(
            "review_focus_list_a",
            METHOD_GIT_REVIEW_THREAD_LIST,
            &GitReviewThreadListParams {
                workspace_id: "ws_review_focus".to_string(),
                pane_id: Some("pane_a".to_string()),
                repository_id: Some(summary_a.repository_id.clone()),
                path: None,
                include_resolved: true,
                include_stale: true,
                limit: Some(25),
            },
        )));
        assert_eq!(listed.threads.len(), 1);
        assert_eq!(listed.threads[0].thread_id, created.thread_id);
        assert_eq!(
            listed.threads[0].author_session_id.as_deref(),
            Some("session_a")
        );

        let focus_pivot = host.handle_request(test_request(
            "review_focus_list_without_pane",
            METHOD_GIT_REVIEW_THREAD_LIST,
            &GitReviewThreadListParams {
                workspace_id: "ws_review_focus".to_string(),
                pane_id: None,
                repository_id: Some(summary_a.repository_id),
                path: None,
                include_resolved: true,
                include_stale: true,
                limit: Some(25),
            },
        ));
        assert_eq!(error_code(focus_pivot), ErrorCode::InvalidRequest);

        fs::remove_dir_all(repository_a).expect("temporary repository cleanup");
        fs::remove_dir_all(repository_b).expect("temporary repository cleanup");
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
    fn worktree_fault_injection_never_adopts_an_ambiguous_destination() {
        let fresh = WorktreeOwnership {
            journal_version: 2,
            source_workspace_id: "ws_source".to_string(),
            repository_host: "native".to_string(),
            ..WorktreeOwnership::default()
        };
        assert_eq!(
            prepared_worktree_recovery(&fresh, false),
            PreparedWorktreeRecovery::Create
        );
        assert_eq!(
            prepared_worktree_recovery(&fresh, true),
            PreparedWorktreeRecovery::RejectPreexisting
        );

        let create_started = WorktreeOwnership {
            journal_version: 2,
            worktree_preexisting: Some(false),
            worktree_create_started: true,
            ..fresh.clone()
        };
        assert_eq!(
            prepared_worktree_recovery(&create_started, false),
            PreparedWorktreeRecovery::Create,
            "a crash before the Git side effect is safe to retry"
        );
        assert_eq!(
            prepared_worktree_recovery(&create_started, true),
            PreparedWorktreeRecovery::RejectUncertain,
            "a crash after Git may have created the worktree must not claim ownership"
        );

        let created = WorktreeOwnership {
            worktree_create_succeeded: true,
            worktree_owned: true,
            branch_owned: true,
            ..create_started
        };
        assert_eq!(
            prepared_worktree_recovery(&created, true),
            PreparedWorktreeRecovery::ContinueOwned
        );
        assert_eq!(
            prepared_worktree_recovery(&created, false),
            PreparedWorktreeRecovery::RejectUncertain,
            "a missing owned worktree is not silently recreated"
        );

        let legacy_prepared = WorktreeOwnership {
            worktree_owned: true,
            ..WorktreeOwnership::default()
        };
        assert_eq!(
            prepared_worktree_recovery(&legacy_prepared, true),
            PreparedWorktreeRecovery::RejectUncertain,
            "legacy Prepared journals wrote ownership before the create result"
        );
    }

    #[test]
    fn prepared_recovery_fault_injection_preserves_ambiguous_registered_worktree() {
        let repository = create_git_repository("ambiguous-worktree");
        let destination = repository
            .parent()
            .unwrap()
            .join(format!("ambiguous-destination-{}", unique_time_id()));
        let destination_text = destination.to_string_lossy().to_string();
        run_git(
            &repository,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "agentmux/ambiguous-recovery",
                &destination_text,
            ],
        );
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let repository_root = host
            .five_track
            .shared
            .git
            .require_repository(&GitContext::native(repository.to_string_lossy()))
            .expect("repository should resolve")
            .root()
            .to_string();
        let params = AgentWorktreeCreateParams {
            workspace_id: "workspace_source".to_string(),
            branch: "agentmux/ambiguous-recovery".to_string(),
            destination: destination_text.clone(),
            base_revision: Some("HEAD".to_string()),
            create_branch: true,
            backend: None,
            backend_profile: None,
            command: Vec::new(),
            cwd: None,
            idempotency_key: "ambiguous-recovery-key".to_string(),
        };
        let ownership = WorktreeOwnership {
            journal_version: 2,
            source_workspace_id: params.workspace_id.clone(),
            repository_host: "native".to_string(),
            worktree_preexisting: Some(false),
            worktree_create_started: true,
            branch_preexisting: Some(false),
            ..WorktreeOwnership::default()
        };
        let operation = PersistedWorktreeOperation {
            operation_id: "operation_ambiguous_recovery".to_string(),
            idempotency_key: params.idempotency_key.clone(),
            repository_root,
            worktree_path: destination_text.clone(),
            branch_name: Some(params.branch.clone()),
            revision: params.base_revision.clone(),
            workspace_id: None,
            surface_id: None,
            session_id: None,
            owner_kind: "agentmux.desktop".to_string(),
            owner_id: Some(params.workspace_id.clone()),
            ownership_json: serde_json::to_string(&ownership).unwrap(),
            request_json: serde_json::to_string(&params).unwrap(),
            state: WorktreeOperationState::Prepared,
            error_code: None,
            error_message: None,
            recovery_json: "{}".to_string(),
            recovery_attempts: 0,
            last_recovery_at: None,
            created_at: "created".to_string(),
            updated_at: "created".to_string(),
            completed_at: None,
            rolled_back_at: None,
        };
        let operation = host
            .store
            .lock()
            .unwrap()
            .create_or_load_worktree_operation(&operation)
            .unwrap();

        assert!(host.resume_worktree_operation(operation).is_err());
        assert!(destination.is_dir(), "ambiguous worktree must be preserved");
        let recovered = host
            .store
            .lock()
            .unwrap()
            .load_worktree_operation("operation_ambiguous_recovery")
            .unwrap()
            .unwrap();
        let recovered_ownership = worktree_ownership(&recovered).unwrap();
        assert!(recovered_ownership.ownership_uncertain);
        assert!(!recovered_ownership.worktree_owned);
        assert!(!recovered_ownership.branch_owned);

        run_git(
            &repository,
            &["worktree", "remove", "--force", &destination_text],
        );
        run_git(
            &repository,
            &["branch", "-D", "agentmux/ambiguous-recovery"],
        );
        fs::remove_dir_all(repository).expect("temporary repository cleanup");
    }

    #[test]
    fn owned_branch_rollback_refuses_missing_or_advanced_journal_heads() {
        assert!(owned_branch_head_is_unchanged(Some("abc123"), "abc123"));
        assert!(!owned_branch_head_is_unchanged(None, "abc123"));
        assert!(!owned_branch_head_is_unchanged(Some("abc123"), "def456"));
    }

    #[test]
    fn interrupted_review_delivery_prefers_duplicate_prevention() {
        assert_eq!(
            interrupted_review_delivery_action("mailbox", true),
            InterruptedReviewDeliveryAction::Confirm,
            "a durable mailbox record proves that the deterministic attempt was dispatched"
        );
        assert_eq!(
            interrupted_review_delivery_action("mailbox", false),
            InterruptedReviewDeliveryAction::Dispatch,
            "an absent deterministic mailbox record is safe to insert after restart"
        );
        assert_eq!(
            interrupted_review_delivery_action("terminal", false),
            InterruptedReviewDeliveryAction::MarkUncertain,
            "terminal input has no acknowledgement and must not be sent twice"
        );
        assert_eq!(
            review_delivery_failure_disposition("terminal"),
            ReviewDeliveryFailureDisposition::MarkUncertain,
            "a terminal write error may follow a partial write"
        );
        assert_eq!(
            review_delivery_failure_disposition("mailbox"),
            ReviewDeliveryFailureDisposition::RetryableFailure,
            "a failed SQLite mailbox transaction cannot partially commit"
        );
    }

    #[test]
    fn legacy_uncertain_review_blocks_same_payload_but_allows_edited_payload() {
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let thread = PersistedGitReviewThread {
            thread_id: "thread_legacy_uncertain".to_string(),
            repository_root: r"D:\repo".to_string(),
            workspace_id: Some("workspace_review".to_string()),
            diff_identity: "repo:diff".to_string(),
            path: "src/lib.rs".to_string(),
            hunk_id: None,
            side: "right".to_string(),
            line_number: Some(1),
            line_anchor: "{}".to_string(),
            stale: false,
            stale_reason: None,
            resolved_at: None,
            author_id: "reviewer".to_string(),
            target_kind: Some("terminal".to_string()),
            target_id: Some("session_worker".to_string()),
            delivery_state: "uncertain".to_string(),
            delivery_error: Some("legacy crash".to_string()),
            created_at: "created".to_string(),
            updated_at: "created".to_string(),
        };
        host.store
            .lock()
            .unwrap()
            .upsert_git_review_thread(&thread)
            .unwrap();

        assert!(host
            .prepare_review_delivery_attempt(&thread, "terminal", "session_worker", "payload-a",)
            .is_err());
        let persisted = host
            .store
            .lock()
            .unwrap()
            .load_git_review_thread(&thread.thread_id)
            .unwrap()
            .unwrap();
        let same_payload = host.prepare_review_delivery_attempt(
            &persisted,
            "terminal",
            "session_worker",
            "payload-a",
        );
        assert!(same_payload.is_err());

        let (edited_attempt, already_sending) = host
            .prepare_review_delivery_attempt(&persisted, "terminal", "session_worker", "payload-b")
            .expect("edited payload should start a new durable attempt");
        assert_eq!(edited_attempt.state, "prepared");
        assert!(!already_sending);
    }

    #[test]
    fn worktree_cwd_rejects_parent_and_link_escape_paths() {
        assert!(validated_worktree_cwd(
            "/mnt/d/worktrees/agent",
            Some("/mnt/d/worktrees/agent/../primary")
        )
        .is_err());
        assert_eq!(
            validated_worktree_cwd("/mnt/d/worktrees/agent", Some("/mnt/d/worktrees/agent/src"))
                .unwrap(),
            "/mnt/d/worktrees/agent/src"
        );

        let root = std::env::temp_dir().join(format!("agentmux-cwd-{}", unique_time_id()));
        let inside = root.join("src");
        let outside = root.parent().unwrap().join("outside");
        fs::create_dir_all(&inside).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let root_text = root.to_string_lossy().to_string();
        let escaped_text = root
            .join("..")
            .join("outside")
            .to_string_lossy()
            .to_string();
        let inside_text = inside.to_string_lossy().to_string();
        assert!(validated_worktree_cwd(&root_text, Some(&escaped_text)).is_err());
        assert!(validated_worktree_cwd(&root_text, Some(&inside_text)).is_ok());
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&outside).unwrap();
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
                pane_id: None,
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

        let duplicate: GitReviewDeliveryResult = decode_ok(host.handle_request(test_request(
            "review_deliver_retry",
            METHOD_GIT_REVIEW_THREAD_DELIVER,
            &GitReviewThreadDeliverParams {
                thread_id: delivered.thread_id.clone(),
                target: "mailbox".to_string(),
                target_session_id: Some("ses_worker".to_string()),
                include_context: true,
            },
        )));
        assert_eq!(duplicate.delivered_at, delivered.delivered_at);
        assert_eq!(
            host.store
                .lock()
                .unwrap()
                .list_team_messages(Some("ws_review"), true)
                .unwrap()
                .len(),
            1,
            "an idempotent retry must not duplicate the mailbox message"
        );

        save_bundle(
            &host,
            &workspace_bundle(
                "ws_other",
                None,
                &[("pane_other", "surface_other", "ses_other")],
            ),
        );
        let cross_workspace = host.handle_request(test_request(
            "review_deliver_cross_workspace",
            METHOD_GIT_REVIEW_THREAD_DELIVER,
            &GitReviewThreadDeliverParams {
                thread_id: delivered.thread_id,
                target: "terminal".to_string(),
                target_session_id: Some("ses_other".to_string()),
                include_context: true,
            },
        ));
        assert_eq!(error_code(cross_workspace), ErrorCode::InvalidRequest);

        fs::remove_dir_all(repository).expect("temporary repository cleanup");
    }

    #[test]
    fn review_threads_are_marked_stale_when_diff_content_changes() {
        let repository = create_git_repository("review-stale");
        fs::write(repository.join("tracked.txt"), "reviewed\n").unwrap();
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        save_bundle(
            &host,
            &workspace_bundle(
                "ws_review_stale",
                Some(repository.to_string_lossy().to_string()),
                &[("pane_worker", "surface_worker", "ses_worker")],
            ),
        );
        let first: IpcGitDiffResult = decode_ok(host.handle_request(test_request(
            "review_diff_first",
            METHOD_GIT_DIFF,
            &IpcGitDiffParams {
                workspace_id: "ws_review_stale".to_string(),
                pane_id: None,
                repository_id: None,
                path: "tracked.txt".to_string(),
                stage: Some("worktree".to_string()),
                context_lines: Some(3),
                generation: None,
            },
        )));
        assert!(!first.diff_hash.is_empty());
        let created: GitReviewThreadResult = decode_ok(host.handle_request(test_request(
            "review_stale_create",
            METHOD_GIT_REVIEW_THREAD_CREATE,
            &GitReviewThreadCreateParams {
                workspace_id: "ws_review_stale".to_string(),
                pane_id: None,
                repository_id: Some(first.repository_id),
                anchor: GitReviewLineAnchor {
                    path: "tracked.txt".to_string(),
                    side: "right".to_string(),
                    line: 1,
                    start_line: None,
                    base_revision: None,
                    head_revision: None,
                    hunk_header: Some("@@ -1 +1 @@".to_string()),
                    diff_hash: Some(first.diff_hash),
                },
                body: "Keep this line.".to_string(),
                author_session_id: None,
            },
        )));
        fs::write(repository.join("tracked.txt"), "changed again\n").unwrap();
        let _: IpcGitDiffResult = decode_ok(host.handle_request(test_request(
            "review_diff_second",
            METHOD_GIT_DIFF,
            &IpcGitDiffParams {
                workspace_id: "ws_review_stale".to_string(),
                pane_id: None,
                repository_id: None,
                path: "tracked.txt".to_string(),
                stage: Some("worktree".to_string()),
                context_lines: Some(3),
                generation: None,
            },
        )));
        let thread = host
            .store
            .lock()
            .unwrap()
            .load_git_review_thread(&created.thread_id)
            .unwrap()
            .unwrap();
        assert!(thread.stale);
        assert_eq!(thread.stale_reason.as_deref(), Some("diff content changed"));
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
    fn development_server_candidates_require_a_local_host() {
        assert!(is_safe_development_server_url("http://localhost:5173"));
        assert!(is_safe_development_server_url("http://127.0.0.1:3000/path"));
        assert!(is_safe_development_server_url("https://[::1]:4173"));
        assert!(!is_safe_development_server_url("https://www.kopus.org"));
        assert!(!is_safe_development_server_url(
            "https://example.com/localhost"
        ));
        assert!(!is_safe_development_server_url(
            "https://user@localhost:3000"
        ));
    }

    #[test]
    fn raw_output_detects_url_and_opens_browser_in_the_same_workspace() {
        assert_eq!(pty_dev_server_source(None), "pty_output_unattributed");
        assert_eq!(pty_dev_server_source(Some(42)), "pty_process_attributed");
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
        assert_eq!(listed.candidates[0].source, "pty_output_unattributed");
        assert_eq!(listed.candidates[0].process_id, None);
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

    struct CandidateRemovingBrowser {
        inner: InMemoryBrowserAutomation,
        shared: Arc<FiveTrackShared>,
        closed: Arc<Mutex<Vec<String>>>,
    }

    impl BrowserAutomation for CandidateRemovingBrowser {
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
            let result = self.inner.execute(command)?;
            self.shared.dev_server_candidates.lock().unwrap().clear();
            Ok(result)
        }
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
    fn browser_split_bookkeeping_failure_closes_surface_and_restores_topology() {
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let original = workspace_bundle(
            "ws_bookkeeping_rollback",
            None,
            &[("pane_terminal", "surface_terminal", "ses_rollback")],
        );
        save_bundle(&host, &original);
        let candidate = host
            .record_dev_server_candidate(DevelopmentServerCandidateParams {
                workspace_id: "ws_bookkeeping_rollback".to_string(),
                session_id: "ses_rollback".to_string(),
                url: "http://127.0.0.1:3000".to_string(),
                source: "test".to_string(),
                detected_at: "2026-07-23T00:00:00Z".to_string(),
                process_id: None,
            })
            .expect("candidate");
        let closed = Arc::new(Mutex::new(Vec::new()));
        *host.browser.lock().unwrap() = Box::new(CandidateRemovingBrowser {
            inner: InMemoryBrowserAutomation::new(),
            shared: Arc::clone(&host.five_track.shared),
            closed: Arc::clone(&closed),
        });

        let response = host.handle_request(test_request(
            "bookkeeping_rollback_open",
            METHOD_DEV_SERVER_CANDIDATE_OPEN_IN_SPLIT,
            &DevelopmentServerCandidateOpenInSplitParams {
                candidate_id: candidate.candidate_id,
                pane_id: Some("pane_terminal".to_string()),
                axis: Some("vertical".to_string()),
                ratio: Some(0.5),
            },
        ));
        assert_eq!(error_code(response), ErrorCode::InvalidRequest);
        let restored = host
            .store
            .lock()
            .unwrap()
            .load_workspace_bundle("ws_bookkeeping_rollback")
            .unwrap()
            .expect("restored workspace");
        assert_eq!(restored, original);
        assert_eq!(closed.lock().unwrap().len(), 1);
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

    #[test]
    #[cfg(windows)]
    fn status_snapshot_arms_native_watcher_before_returning_cached_state() {
        let repository_path = create_git_repository("watcher-before-snapshot");
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let repository = host
            .five_track
            .shared
            .git
            .require_repository(&GitContext::native(
                repository_path.to_string_lossy().to_string(),
            ))
            .expect("resolved repository");
        let (repository_id, cached) = host
            .status_snapshot("ws_watch_before_snapshot", &repository)
            .expect("status snapshot");
        let key = format!("ws_watch_before_snapshot|{repository_id}");
        let mode = host
            .five_track
            .shared
            .observed_repositories
            .lock()
            .unwrap()
            .get(&key)
            .expect("observed repository")
            .monitor_mode;
        assert_eq!(mode, RepositoryMonitorMode::NativeWatcher);

        fs::write(repository_path.join("after-snapshot.txt"), "changed\n").expect("watch fixture");
        let generation = cached.snapshot.summary.generation;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline
            && host.five_track.shared.git.generation(&repository) == generation
        {
            thread::sleep(Duration::from_millis(25));
        }
        assert!(host.five_track.shared.git.generation(&repository) > generation);
        fs::remove_dir_all(repository_path).expect("temporary repository cleanup");
    }

    #[test]
    #[cfg(windows)]
    fn repository_watcher_cancellation_releases_worker_handles() {
        let repository_path = create_git_repository("watcher-cancel-event");
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let cancellation = start_repository_native_watchers(
            Arc::clone(&host.five_track.shared),
            None,
            "ws_watch_cancel|repository".to_string(),
            &repository_path,
        )
        .expect("native watcher");
        assert!(!cancellation.handles.lock().unwrap().is_empty());
        cancellation.request();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && !cancellation.handles.lock().unwrap().is_empty() {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(cancellation.handles.lock().unwrap().is_empty());
        fs::remove_dir_all(repository_path).expect("temporary repository cleanup");
    }

    #[test]
    #[cfg(windows)]
    fn stale_repository_watcher_is_not_adopted_after_root_change() {
        let first_path = create_git_repository("watcher-root-first");
        let second_path = create_git_repository("watcher-root-second");
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let first_repository = host
            .five_track
            .shared
            .git
            .require_repository(&GitContext::native(
                first_path.to_string_lossy().to_string(),
            ))
            .expect("first repository");
        let second_repository = host
            .five_track
            .shared
            .git
            .require_repository(&GitContext::native(
                second_path.to_string_lossy().to_string(),
            ))
            .expect("second repository");
        let repository_id = repository_id(&first_repository);
        let key = format!("ws_watch_root|{repository_id}");
        host.observe_repository("ws_watch_root", &repository_id, &first_repository);
        let snapshot = host
            .five_track
            .shared
            .observed_repositories
            .lock()
            .unwrap()
            .get(&key)
            .cloned()
            .expect("first snapshot");
        host.observe_repository("ws_watch_root", &repository_id, &second_repository);
        let watcher = Arc::new(RepositoryWatchCancellation::new().expect("cancellation event"));

        let adopted = adopt_repository_watcher(
            &mut host.five_track.shared.observed_repositories.lock().unwrap(),
            &key,
            &snapshot,
            Some(&watcher),
        );
        assert!(!adopted);
        watcher.request();
        assert!(watcher.requested.load(Ordering::Acquire));
        let observed = host.five_track.shared.observed_repositories.lock().unwrap();
        let entry = observed.get(&key).expect("current repository");
        assert_eq!(entry.monitor_mode, RepositoryMonitorMode::Pending);
        assert_eq!(entry.scan_root, repository_scan_root(&second_repository));
        assert!(entry.watch_cancel.is_none());
        drop(observed);
        fs::remove_dir_all(first_path).expect("first repository cleanup");
        fs::remove_dir_all(second_path).expect("second repository cleanup");
    }

    #[test]
    #[cfg(windows)]
    fn root_change_cancels_live_worker_before_rearming_new_repository() {
        let first_path = create_git_repository("watcher-live-root-first");
        let second_path = create_git_repository("watcher-live-root-second");
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let first_repository = host
            .five_track
            .shared
            .git
            .require_repository(&GitContext::native(
                first_path.to_string_lossy().to_string(),
            ))
            .expect("first repository");
        let second_repository = host
            .five_track
            .shared
            .git
            .require_repository(&GitContext::native(
                second_path.to_string_lossy().to_string(),
            ))
            .expect("second repository");
        let repository_id = repository_id(&first_repository);
        let key = format!("ws_watch_live_root|{repository_id}");
        host.observe_repository("ws_watch_live_root", &repository_id, &first_repository);
        arm_observed_repository(&host.five_track.shared, None, &key);
        let first_watcher = host
            .five_track
            .shared
            .observed_repositories
            .lock()
            .unwrap()
            .get(&key)
            .and_then(|entry| entry.watch_cancel.clone())
            .expect("first watcher");
        assert!(!first_watcher.handles.lock().unwrap().is_empty());

        host.observe_repository("ws_watch_live_root", &repository_id, &second_repository);
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && !first_watcher.handles.lock().unwrap().is_empty() {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(first_watcher.handles.lock().unwrap().is_empty());
        arm_observed_repository(&host.five_track.shared, None, &key);
        let observed = host.five_track.shared.observed_repositories.lock().unwrap();
        let entry = observed.get(&key).expect("current repository");
        assert_eq!(entry.monitor_mode, RepositoryMonitorMode::NativeWatcher);
        let second_watcher = entry.watch_cancel.as_ref().expect("second watcher");
        assert!(!Arc::ptr_eq(&first_watcher, second_watcher));
        assert_eq!(entry.scan_root, repository_scan_root(&second_repository));
        drop(observed);
        fs::remove_dir_all(first_path).expect("first repository cleanup");
        fs::remove_dir_all(second_path).expect("second repository cleanup");
    }

    #[test]
    #[cfg(windows)]
    fn repository_watcher_failure_only_requeues_the_current_watcher() {
        let repository_path = create_git_repository("watcher-failure");
        let host = DesktopControlState::new_in_memory().expect("desktop state");
        let repository = host
            .five_track
            .shared
            .git
            .require_repository(&GitContext::native(
                repository_path.to_string_lossy().to_string(),
            ))
            .expect("repository");
        let repository_id = repository_id(&repository);
        let key = format!("ws_watch_failure|{repository_id}");
        host.observe_repository("ws_watch_failure", &repository_id, &repository);
        let current = Arc::new(RepositoryWatchCancellation::new().expect("current watcher"));
        let stale = Arc::new(RepositoryWatchCancellation::new().expect("stale watcher"));
        {
            let mut observed = host.five_track.shared.observed_repositories.lock().unwrap();
            let entry = observed.get_mut(&key).expect("observed repository");
            entry.monitor_mode = RepositoryMonitorMode::NativeWatcher;
            entry.watch_cancel = Some(Arc::clone(&current));
        }

        mark_repository_watcher_failed(&host.five_track.shared, &key, &stale);
        {
            let observed = host.five_track.shared.observed_repositories.lock().unwrap();
            let entry = observed.get(&key).expect("observed repository");
            assert_eq!(entry.monitor_mode, RepositoryMonitorMode::NativeWatcher);
            assert!(Arc::ptr_eq(
                entry
                    .watch_cancel
                    .as_ref()
                    .expect("current watcher retained"),
                &current
            ));
        }

        mark_repository_watcher_failed(&host.five_track.shared, &key, &current);
        let observed = host.five_track.shared.observed_repositories.lock().unwrap();
        let entry = observed.get(&key).expect("observed repository");
        assert_eq!(entry.monitor_mode, RepositoryMonitorMode::Pending);
        assert!(entry.watch_cancel.is_none());
        assert!(current.failed());
        drop(observed);
        fs::remove_dir_all(repository_path).expect("repository cleanup");
    }

    #[test]
    fn repository_watcher_coalesces_rapid_changes_at_the_trailing_edge() {
        let repository_path = create_git_repository("watcher-trailing-edge");
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
        let key = format!("ws_watch_trailing|{repository_id}");
        host.observe_repository("ws_watch_trailing", &repository_id, &repository);
        let initial_generation = host.five_track.shared.git.generation(&repository);

        signal_repository_changed(&host.five_track.shared, &key, "first_change", None);
        thread::sleep(REPOSITORY_EVENT_DEBOUNCE / 2);
        signal_repository_changed(&host.five_track.shared, &key, "second_change", None);
        assert_eq!(
            host.five_track
                .shared
                .observed_repositories
                .lock()
                .unwrap()
                .get(&key)
                .and_then(|entry| entry.pending_change_reason.as_deref()),
            Some("second_change"),
            "the most recent event must remain pending until the trailing refresh"
        );

        // A leading-edge implementation would have refreshed before the second event.
        thread::sleep(REPOSITORY_EVENT_DEBOUNCE / 2);
        assert_eq!(
            host.five_track.shared.git.generation(&repository),
            initial_generation,
            "the refresh must wait until the last event in the burst"
        );

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline
            && host.five_track.shared.git.generation(&repository) == initial_generation
        {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            host.five_track.shared.git.generation(&repository) == initial_generation + 1,
            "the second rapid change must trigger the trailing refresh"
        );
        assert!(
            host.five_track
                .shared
                .observed_repositories
                .lock()
                .unwrap()
                .get(&key)
                .is_some_and(|entry| entry.debounce_cancel.is_none()),
            "the completed debounce task must clean itself up"
        );
        fs::remove_dir_all(repository_path).expect("temporary repository cleanup");
    }
}
