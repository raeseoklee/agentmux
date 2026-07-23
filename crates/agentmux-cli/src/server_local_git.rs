use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(not(test))]
use std::{fs, path::PathBuf};

use agentmux_backend_wsl::fallback_windows_path_to_wsl;
use agentmux_ipc::{
    ControlError, ErrorCode, GitAllMutationParams, GitChangeSummaryResult, GitCommitParams,
    GitDiffParams, GitDiffResult, GitMutationResult, GitPathMutationParams, GitRepositoryParams,
    GitStatusPageParams, GitStatusPageResult, GitStatusSummaryResult, RequestEnvelope,
    ResponseEnvelope,
};
use agentmux_store::{GitMutationReceiptLookup, PersistedGitMutationReceipt, SqliteStore};
use agentmux_vcs::{
    DiffRequest, GitClient, GitContext, GitError, GitFileChange, GitHost, Repository, StatusScan,
    StatusScanFirstPage, StatusSnapshot, StatusSummary,
};

const LOCAL_GIT_READ_METHODS: &[&str] = &[
    "git.status",
    "git.status_summary",
    "git.status_page",
    "git.diff",
];
const LOCAL_GIT_MUTATION_METHODS: &[&str] = &[
    "git.stage",
    "git.unstage",
    "git.stage_all",
    "git.unstage_all",
    "git.commit",
];
const MAX_IDEMPOTENCY_RECEIPTS: usize = 512;

pub(crate) struct ServerLocalGit {
    client: GitClient,
    repository: Repository,
    repository_id: String,
    workspace_id: String,
    receipt_store: SqliteStore,
    status_snapshot: Option<Arc<StatusSnapshot>>,
    status_scan: Option<LocalStatusScan>,
    status_refresh_count: u64,
}

#[derive(Clone)]
struct LocalStatusScan {
    generation: u64,
    scan: StatusScan,
}

impl ServerLocalGit {
    pub(crate) fn probe(
        workspace_id: &str,
        backend: Option<&str>,
        backend_profile: Option<&str>,
        cwd: Option<&str>,
    ) -> Option<Self> {
        Self::probe_with_store(
            workspace_id,
            backend,
            backend_profile,
            cwd,
            open_server_git_receipt_store().ok()?,
        )
    }

    fn probe_with_store(
        workspace_id: &str,
        backend: Option<&str>,
        backend_profile: Option<&str>,
        cwd: Option<&str>,
        receipt_store: SqliteStore,
    ) -> Option<Self> {
        let context = local_git_context(backend, backend_profile, cwd)?;
        let client = GitClient::default();
        let repository = client.resolve_repository(&context).ok()??;
        let repository_id = repository_id(&repository);
        Some(Self {
            client,
            repository,
            repository_id,
            workspace_id: workspace_id.to_string(),
            receipt_store,
            status_snapshot: None,
            status_scan: None,
            status_refresh_count: 0,
        })
    }

    pub(crate) fn methods() -> &'static [&'static str] {
        &[
            "git.status",
            "git.status_summary",
            "git.status_page",
            "git.diff",
            "git.stage",
            "git.unstage",
            "git.stage_all",
            "git.unstage_all",
            "git.commit",
        ]
    }

    pub(crate) fn supports_method(method: &str) -> bool {
        Self::is_read_method(method) || Self::is_mutation_method(method)
    }

    pub(crate) fn is_read_method(method: &str) -> bool {
        LOCAL_GIT_READ_METHODS.contains(&method)
    }

    pub(crate) fn is_mutation_method(method: &str) -> bool {
        LOCAL_GIT_MUTATION_METHODS.contains(&method)
    }

    pub(crate) fn handle_request(&mut self, request: RequestEnvelope) -> ResponseEnvelope {
        let result = match request.method.as_str() {
            "git.status" => self.status(&request),
            "git.status_summary" => self.status_summary(&request),
            "git.status_page" => self.status_page(&request),
            "git.diff" => self.diff(&request),
            "git.stage" => self.path_mutation(&request, true),
            "git.unstage" => self.path_mutation(&request, false),
            "git.stage_all" => self.all_mutation(&request, true),
            "git.unstage_all" => self.all_mutation(&request, false),
            "git.commit" => self.commit(&request),
            _ => Err(ControlError::new(
                ErrorCode::UnsupportedMethod,
                "Git method is unavailable in local server mode.",
            )),
        };
        match result {
            Ok(response) => response,
            Err(error) => ResponseEnvelope::error(request.id, error),
        }
    }

    fn status(&mut self, request: &RequestEnvelope) -> Result<ResponseEnvelope, ControlError> {
        let params: GitRepositoryParams = request.parse_params()?;
        self.validate_selector(&params.workspace_id, params.repository_id.as_deref())?;
        let snapshot = self.completed_snapshot(None)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &LegacyGitStatusResult {
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
                    .map(LegacyGitFileChangeResult::from)
                    .collect(),
            },
        ))
    }

    fn status_summary(
        &mut self,
        request: &RequestEnvelope,
    ) -> Result<ResponseEnvelope, ControlError> {
        let params: GitRepositoryParams = request.parse_params()?;
        self.validate_selector(&params.workspace_id, params.repository_id.as_deref())?;
        let summary = self.completed_snapshot(None)?.summary.clone();
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &self.summary_result(summary),
        ))
    }

    fn status_page(&mut self, request: &RequestEnvelope) -> Result<ResponseEnvelope, ControlError> {
        let params: GitStatusPageParams = request.parse_params()?;
        params.validate()?;
        self.validate_selector(&params.workspace_id, params.repository_id.as_deref())?;
        if params.cursor.is_some() && params.generation.is_none() {
            return Err(invalid_request(
                "Git status cursors require their snapshot generation.",
            ));
        }
        let limit = params.limit.unwrap_or(250).clamp(1, 500);
        if params.cursor.is_none() && params.generation.is_none() {
            let active = self.refresh_scan(limit)?;
            return match active.scan.wait_for_first_page().map_err(vcs_error)? {
                StatusScanFirstPage::Prefix(prefix) => {
                    let changes = prefix
                        .changes
                        .iter()
                        .filter(|change| {
                            change_matches_state(change, params.state.as_deref())
                                && change_matches_query(change, params.query.as_deref())
                        })
                        .map(change_result)
                        .collect();
                    Ok(ResponseEnvelope::ok_typed(
                        request.id.clone(),
                        &GitStatusPageResult {
                            workspace_id: self.workspace_id.clone(),
                            repository_id: self.repository_id.clone(),
                            generation: prefix.generation,
                            summary: None,
                            changes,
                            next_cursor: None,
                            total_count: None,
                        },
                    ))
                }
                StatusScanFirstPage::Complete(result) => {
                    let snapshot = self.install_completed_scan(active.generation, result)?;
                    self.complete_page(request, &params, &snapshot, limit)
                }
            };
        }

        let snapshot = self.completed_snapshot(params.generation)?;
        self.complete_page(request, &params, &snapshot, limit)
    }

    fn complete_page(
        &self,
        request: &RequestEnvelope,
        params: &GitStatusPageParams,
        snapshot: &StatusSnapshot,
        limit: usize,
    ) -> Result<ResponseEnvelope, ControlError> {
        validate_generation(params.generation, snapshot.summary.generation)?;
        let matching = snapshot
            .files
            .iter()
            .filter(|change| {
                change_matches_state(change, params.state.as_deref())
                    && change_matches_query(change, params.query.as_deref())
            })
            .collect::<Vec<_>>();
        let generation = snapshot.summary.generation;
        let cursor_fingerprint = status_page_cursor_fingerprint(
            generation,
            params.state.as_deref(),
            params.query.as_deref(),
        );
        let offset = parse_cursor(params.cursor.as_deref(), generation, cursor_fingerprint)?;
        if offset > matching.len() {
            return Err(invalid_request(format!(
                "Git status cursor {offset} exceeds {} changes.",
                matching.len()
            )));
        }
        let end = offset.saturating_add(limit).min(matching.len());
        let changes = matching[offset..end]
            .iter()
            .map(|change| change_result(change))
            .collect();
        let summary = self.summary_result(snapshot.summary.clone());
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &GitStatusPageResult {
                workspace_id: self.workspace_id.clone(),
                repository_id: self.repository_id.clone(),
                generation,
                summary: Some(summary),
                changes,
                next_cursor: (end < matching.len())
                    .then(|| format_cursor(generation, cursor_fingerprint, end)),
                total_count: Some(matching.len()),
            },
        ))
    }

    fn refresh_scan(&mut self, first_page_limit: usize) -> Result<LocalStatusScan, ControlError> {
        if let Some(active) = self.status_scan.clone() {
            return Ok(active);
        }
        self.status_snapshot = None;
        let generation = self.client.mark_repository_changed(&self.repository);
        let scan = self
            .client
            .start_status_scan(&self.repository, first_page_limit)
            .map_err(vcs_error)?;
        let active = LocalStatusScan { generation, scan };
        self.status_refresh_count = self.status_refresh_count.saturating_add(1);
        self.status_scan = Some(active.clone());
        Ok(active)
    }

    fn completed_snapshot(
        &mut self,
        requested_generation: Option<u64>,
    ) -> Result<Arc<StatusSnapshot>, ControlError> {
        if let Some(snapshot) = self.status_snapshot.clone() {
            validate_generation(requested_generation, snapshot.summary.generation)?;
            return Ok(snapshot);
        }
        let active = if let Some(active) = self.status_scan.clone() {
            active
        } else {
            if requested_generation.is_some() {
                return Err(ControlError::new(
                    ErrorCode::Conflict,
                    "Git status snapshot is no longer available; refresh page zero.",
                ));
            }
            self.refresh_scan(250)?
        };
        validate_generation(requested_generation, active.generation)?;
        let result = active.scan.wait_for_completion().map_err(vcs_error)?;
        self.install_completed_scan(active.generation, result)
    }

    fn install_completed_scan(
        &mut self,
        generation: u64,
        result: Arc<agentmux_vcs::StatusReadResult>,
    ) -> Result<Arc<StatusSnapshot>, ControlError> {
        if result.snapshot.summary.generation != generation
            || self.client.generation(&self.repository) != generation
        {
            self.status_scan = None;
            return Err(ControlError::new(
                ErrorCode::Conflict,
                "Git repository changed while its status snapshot was loading; refresh and retry.",
            ));
        }
        let snapshot = Arc::new(result.snapshot.clone());
        self.status_snapshot = Some(snapshot.clone());
        if self
            .status_scan
            .as_ref()
            .is_some_and(|active| active.generation == generation)
        {
            self.status_scan = None;
        }
        Ok(snapshot)
    }

    fn invalidate_status(&mut self) {
        if let Some(active) = self.status_scan.take() {
            active.scan.cancel();
        }
        self.status_snapshot = None;
    }

    fn diff(&self, request: &RequestEnvelope) -> Result<ResponseEnvelope, ControlError> {
        let raw = serde_json::from_str::<serde_json::Value>(&request.params_json)
            .map_err(|error| invalid_request(format!("Invalid Git diff params: {error}")))?;
        if raw.get("stage").is_none()
            && (raw.get("staged").is_some() || raw.get("untracked").is_some())
        {
            return self.legacy_diff(request);
        }
        let params: GitDiffParams = request.parse_params()?;
        params.validate()?;
        self.validate_selector(&params.workspace_id, params.repository_id.as_deref())?;
        validate_generation(params.generation, self.client.generation(&self.repository))?;
        let (staged, untracked) = match params.stage.as_deref() {
            None | Some("unstaged" | "worktree") => (false, false),
            Some("staged" | "index") => (true, false),
            Some("untracked") => (false, true),
            Some(value) => {
                return Err(invalid_request(format!(
                    "Unsupported Git diff stage '{value}'."
                )))
            }
        };
        let result = self
            .client
            .diff(
                &self.repository,
                &DiffRequest {
                    path: params.path.clone(),
                    staged,
                    untracked,
                },
            )
            .map_err(vcs_error)?;
        let diff_hash = format!("{:016x}", stable_hash(&result.patch));
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &GitDiffResult {
                workspace_id: self.workspace_id.clone(),
                repository_id: self.repository_id.clone(),
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

    fn legacy_diff(&self, request: &RequestEnvelope) -> Result<ResponseEnvelope, ControlError> {
        let params: LegacyGitDiffParams = request.parse_params()?;
        self.validate_selector(&params.workspace_id, None)?;
        let result = self
            .client
            .diff(
                &self.repository,
                &DiffRequest {
                    path: params.path,
                    staged: params.staged,
                    untracked: params.untracked,
                },
            )
            .map_err(vcs_error)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &LegacyGitDiffResult {
                path: result.path,
                staged: result.staged,
                patch: result.patch,
                truncated: result.truncated,
            },
        ))
    }

    fn path_mutation(
        &mut self,
        request: &RequestEnvelope,
        stage: bool,
    ) -> Result<ResponseEnvelope, ControlError> {
        let params: GitPathMutationParams = request.parse_params()?;
        params.validate()?;
        self.validate_selector(&params.workspace_id, params.repository_id.as_deref())?;
        let fingerprint = mutation_request_fingerprint(&params)?;
        if let Some(result) = self.reused_result(
            request.method.as_str(),
            params.idempotency_key.as_deref(),
            &fingerprint,
        )? {
            return Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result));
        }
        self.invalidate_status();
        let generation = if stage {
            self.client.stage(&self.repository, &params.paths)
        } else {
            self.client.unstage(&self.repository, &params.paths)
        }
        .map_err(vcs_error)?;
        let result = GitMutationResult {
            workspace_id: self.workspace_id.clone(),
            repository_id: self.repository_id.clone(),
            generation,
            affected_paths: params.paths,
            commit_oid: None,
            reused: false,
        };
        self.remember_result(
            request.method.as_str(),
            params.idempotency_key.as_deref(),
            fingerprint,
            &result,
        )?;
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn all_mutation(
        &mut self,
        request: &RequestEnvelope,
        stage: bool,
    ) -> Result<ResponseEnvelope, ControlError> {
        let params: GitAllMutationParams = request.parse_params()?;
        params.validate()?;
        self.validate_selector(&params.workspace_id, params.repository_id.as_deref())?;
        let fingerprint = mutation_request_fingerprint(&params)?;
        if let Some(result) = self.reused_result(
            request.method.as_str(),
            params.idempotency_key.as_deref(),
            &fingerprint,
        )? {
            return Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result));
        }
        self.invalidate_status();
        let generation = if stage {
            self.client.stage(&self.repository, &[])
        } else {
            self.client.unstage(&self.repository, &[])
        }
        .map_err(vcs_error)?;
        let result = GitMutationResult {
            workspace_id: self.workspace_id.clone(),
            repository_id: self.repository_id.clone(),
            generation,
            affected_paths: Vec::new(),
            commit_oid: None,
            reused: false,
        };
        self.remember_result(
            request.method.as_str(),
            params.idempotency_key.as_deref(),
            fingerprint,
            &result,
        )?;
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn commit(&mut self, request: &RequestEnvelope) -> Result<ResponseEnvelope, ControlError> {
        let params: GitCommitParams = request.parse_params()?;
        params.validate()?;
        self.validate_selector(&params.workspace_id, params.repository_id.as_deref())?;
        let fingerprint = mutation_request_fingerprint(&params)?;
        if let Some(result) = self.reused_result(
            request.method.as_str(),
            params.idempotency_key.as_deref(),
            &fingerprint,
        )? {
            return Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result));
        }
        if params.amend {
            return Err(invalid_request(
                "Amend is not available through local server mode.",
            ));
        }
        self.invalidate_status();
        let commit = self
            .client
            .commit(&self.repository, &params.message)
            .map_err(vcs_error)?;
        let result = GitMutationResult {
            workspace_id: self.workspace_id.clone(),
            repository_id: self.repository_id.clone(),
            generation: commit.generation,
            affected_paths: Vec::new(),
            commit_oid: Some(commit.commit),
            reused: false,
        };
        self.remember_result(
            request.method.as_str(),
            params.idempotency_key.as_deref(),
            fingerprint,
            &result,
        )?;
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn validate_selector(
        &self,
        workspace_id: &str,
        repository_id: Option<&str>,
    ) -> Result<(), ControlError> {
        if workspace_id != self.workspace_id {
            return Err(ControlError::new(
                ErrorCode::WorkspaceNotFound,
                "Git request does not target the local server workspace.",
            ));
        }
        if repository_id.is_some_and(|value| value != self.repository_id) {
            return Err(invalid_request(
                "repository_id does not identify the local server repository",
            ));
        }
        Ok(())
    }

    fn summary_result(&self, summary: StatusSummary) -> GitStatusSummaryResult {
        GitStatusSummaryResult {
            workspace_id: self.workspace_id.clone(),
            repository_id: self.repository_id.clone(),
            repository_root: summary.repository_root,
            branch: summary.branch,
            head_oid: summary.head,
            upstream: summary.upstream,
            ahead: summary.ahead,
            behind: summary.behind,
            staged_count: summary.staged_count,
            unstaged_count: summary.unstaged_count,
            untracked_count: summary.untracked_count,
            conflicted_count: summary.conflict_count,
            generation: summary.generation,
            refreshed_at: refreshed_at(),
        }
    }

    fn reused_result(
        &self,
        method: &str,
        idempotency_key: Option<&str>,
        request_fingerprint: &str,
    ) -> Result<Option<GitMutationResult>, ControlError> {
        let Some(idempotency_key) = idempotency_key else {
            return Ok(None);
        };
        match self
            .receipt_store
            .load_git_mutation_receipt(
                method,
                &self.repository_id,
                idempotency_key,
                request_fingerprint,
            )
            .map_err(receipt_store_error)?
        {
            GitMutationReceiptLookup::Missing => Ok(None),
            GitMutationReceiptLookup::Match(receipt) => {
                let mut result = serde_json::from_str::<GitMutationResult>(&receipt.response_json)
                    .map_err(|_| receipt_store_error("stored response is invalid"))?;
                result.reused = true;
                Ok(Some(result))
            }
            GitMutationReceiptLookup::FingerprintMismatch { .. } => Err(idempotency_conflict()),
        }
    }

    fn remember_result(
        &mut self,
        method: &str,
        idempotency_key: Option<&str>,
        request_fingerprint: String,
        result: &GitMutationResult,
    ) -> Result<(), ControlError> {
        let Some(idempotency_key) = idempotency_key else {
            return Ok(());
        };
        let receipt = PersistedGitMutationReceipt {
            method: method.to_string(),
            repository_id: self.repository_id.clone(),
            idempotency_key: idempotency_key.to_string(),
            request_fingerprint,
            response_json: serde_json::to_string(result)
                .map_err(|_| receipt_store_error("could not encode the Git mutation response"))?,
            created_at: refreshed_at(),
        };
        match self
            .receipt_store
            .create_or_load_git_mutation_receipt(&receipt)
            .map_err(receipt_store_error)?
        {
            GitMutationReceiptLookup::Match(_) => {
                self.receipt_store
                    .prune_git_mutation_receipts(MAX_IDEMPOTENCY_RECEIPTS)
                    .map_err(receipt_store_error)?;
                Ok(())
            }
            GitMutationReceiptLookup::FingerprintMismatch { .. } => Err(idempotency_conflict()),
            GitMutationReceiptLookup::Missing => Err(receipt_store_error(
                "successful Git mutation receipt was not persisted",
            )),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct LegacyGitFileChangeResult {
    path: String,
    original_path: Option<String>,
    index_status: String,
    worktree_status: String,
    staged: bool,
    unstaged: bool,
    untracked: bool,
    conflict: bool,
}

impl From<&GitFileChange> for LegacyGitFileChangeResult {
    fn from(change: &GitFileChange) -> Self {
        Self {
            path: change.path.clone(),
            original_path: change.original_path.clone(),
            index_status: change.index_status.clone(),
            worktree_status: change.worktree_status.clone(),
            staged: change.staged,
            unstaged: change.unstaged,
            untracked: change.untracked,
            conflict: change.conflict,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct LegacyGitStatusResult {
    is_repository: bool,
    repository_root: Option<String>,
    branch: Option<String>,
    head: Option<String>,
    upstream: Option<String>,
    ahead: u64,
    behind: u64,
    files: Vec<LegacyGitFileChangeResult>,
}

#[derive(serde::Deserialize)]
struct LegacyGitDiffParams {
    workspace_id: String,
    path: String,
    #[serde(default)]
    staged: bool,
    #[serde(default)]
    untracked: bool,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct LegacyGitDiffResult {
    path: String,
    staged: bool,
    patch: String,
    truncated: bool,
}

fn local_git_context(
    backend: Option<&str>,
    backend_profile: Option<&str>,
    cwd: Option<&str>,
) -> Option<GitContext> {
    let cwd = cwd
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|path| path.to_string_lossy().to_string())
        })?;
    if backend == Some("wsl-direct") {
        let cwd = if cwd.starts_with('/') {
            cwd
        } else {
            fallback_windows_path_to_wsl(&cwd)?
        };
        return Some(GitContext::wsl(cwd, backend_profile.map(ToOwned::to_owned)));
    }
    if backend.is_some_and(|value| value != "conpty") {
        return None;
    }
    let path = Path::new(&cwd);
    let cwd = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();
    Some(GitContext::native(cwd))
}

fn repository_id(repository: &Repository) -> String {
    let host = match repository.host() {
        GitHost::Native => "native".to_string(),
        GitHost::Wsl { distribution } => {
            format!("wsl:{}", distribution.as_deref().unwrap_or_default())
        }
    };
    format!("repo_{:016x}", stable_hash(&(host, repository.root())))
}

fn stable_hash<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn idempotency_conflict() -> ControlError {
    ControlError::new(
        ErrorCode::Conflict,
        "idempotency_key was already used with a different Git mutation request.",
    )
}

fn receipt_store_error(error: impl std::fmt::Display) -> ControlError {
    ControlError::new(
        ErrorCode::BackendUnavailable,
        format!("Git mutation receipt store is unavailable: {error}"),
    )
}

#[cfg(test)]
fn open_server_git_receipt_store() -> Result<SqliteStore, String> {
    SqliteStore::in_memory().map_err(|error| error.to_string())
}

#[cfg(not(test))]
fn open_server_git_receipt_store() -> Result<SqliteStore, String> {
    let path = default_server_git_receipt_store_path()?;
    SqliteStore::open(path).map_err(|error| error.to_string())
}

#[cfg(not(test))]
fn default_server_git_receipt_store_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("AGENTMUX_STORE_PATH") {
        return Ok(PathBuf::from(path));
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| "unable to resolve AgentMux store path".to_string())?;
    let directory = base.join("AgentMux");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("failed to create AgentMux store directory: {error}"))?;
    Ok(directory.join("agentmux.sqlite3"))
}

fn mutation_request_fingerprint<T>(params: &T) -> Result<String, ControlError>
where
    T: serde::Serialize,
{
    serde_json::to_string(params).map_err(|_| {
        ControlError::new(
            ErrorCode::InvalidRequest,
            "Could not fingerprint Git mutation request.",
        )
    })
}

fn validate_generation(requested: Option<u64>, actual: u64) -> Result<(), ControlError> {
    if requested.is_some_and(|value| value != actual) {
        return Err(ControlError::new(
            ErrorCode::Conflict,
            format!("Git repository generation changed to {actual}; refresh and retry."),
        ));
    }
    Ok(())
}

fn status_page_cursor_fingerprint(
    generation: u64,
    state: Option<&str>,
    query: Option<&str>,
) -> u64 {
    let state = state.unwrap_or("all").trim().to_ascii_lowercase();
    let query = query.unwrap_or_default().trim().to_ascii_lowercase();
    stable_hash(&(generation, state, query))
}

fn format_cursor(generation: u64, fingerprint: u64, offset: usize) -> String {
    format!("v1:{generation}:{fingerprint:016x}:{offset}")
}

fn parse_cursor(
    cursor: Option<&str>,
    generation: u64,
    fingerprint: u64,
) -> Result<usize, ControlError> {
    let Some(cursor) = cursor.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(0);
    };
    let mut fields = cursor.split(':');
    let version = fields.next();
    let cursor_generation = fields.next().and_then(|value| value.parse::<u64>().ok());
    let cursor_fingerprint = fields
        .next()
        .and_then(|value| u64::from_str_radix(value, 16).ok());
    let offset = fields.next().and_then(|value| value.parse::<usize>().ok());
    if fields.next().is_some()
        || version != Some("v1")
        || cursor_generation != Some(generation)
        || cursor_fingerprint != Some(fingerprint)
    {
        return Err(ControlError::new(
            ErrorCode::Conflict,
            "Git status cursor belongs to a different generation, state, or query; refresh and retry.",
        ));
    }
    offset.ok_or_else(|| invalid_request("Git status cursor is invalid."))
}

fn change_matches_state(change: &GitFileChange, state: Option<&str>) -> bool {
    match state.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("all") => true,
        Some("staged") => change.staged,
        Some("unstaged") => change.unstaged,
        Some("untracked") => change.untracked,
        Some("conflicted" | "conflict") => change.conflict,
        Some(_) => false,
    }
}

fn change_matches_query(change: &GitFileChange, query: Option<&str>) -> bool {
    let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    let query = query.to_ascii_lowercase();
    change.path.to_ascii_lowercase().contains(&query)
        || change
            .original_path
            .as_deref()
            .is_some_and(|path| path.to_ascii_lowercase().contains(&query))
}

fn change_result(change: &GitFileChange) -> GitChangeSummaryResult {
    GitChangeSummaryResult {
        path: change.path.clone(),
        original_path: change.original_path.clone(),
        status: if change.conflict {
            "U".to_string()
        } else if change.untracked {
            "?".to_string()
        } else if change.staged && change.index_status != "." {
            change.index_status.clone()
        } else if change.worktree_status != "." {
            change.worktree_status.clone()
        } else {
            "M".to_string()
        },
        staged: change.staged,
        unstaged: change.unstaged,
        untracked: change.untracked,
        conflicted: change.conflict,
        is_binary: false,
        additions: None,
        deletions: None,
    }
}

fn diff_is_binary(diff: &str) -> bool {
    diff.contains("GIT binary patch") || diff.lines().any(|line| line.starts_with("Binary files "))
}

fn refreshed_at() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs().to_string())
        .unwrap_or_default()
}

fn invalid_request(message: impl Into<String>) -> ControlError {
    ControlError::new(ErrorCode::InvalidRequest, message)
}

fn vcs_error(error: GitError) -> ControlError {
    let code = match error {
        GitError::Timeout { .. } => ErrorCode::Timeout,
        GitError::NotRepository
        | GitError::InvalidBranch(_)
        | GitError::InvalidRevision(_)
        | GitError::InvalidPath(_)
        | GitError::InvalidCommitMessage(_)
        | GitError::InvalidWorktreeDestination(_)
        | GitError::InvalidWorktreeRequest(_)
        | GitError::InvalidStatusPage(_)
        | GitError::InvalidOutput(_) => ErrorCode::InvalidRequest,
        GitError::Io { .. }
        | GitError::OutputLimit { .. }
        | GitError::StatusEntryLimit { .. }
        | GitError::CommandFailed { .. }
        | GitError::StateUnavailable(_) => ErrorCode::BackendDegraded,
    };
    ControlError::new(code, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    use agentmux_ipc::ResponseOutcome;

    #[test]
    fn local_git_method_contract_separates_reads_and_mutations() {
        assert!(ServerLocalGit::is_read_method("git.status"));
        assert!(ServerLocalGit::is_read_method("git.status_page"));
        assert!(ServerLocalGit::is_mutation_method("git.commit"));
        assert!(!ServerLocalGit::supports_method("git.discard"));
        assert!(!ServerLocalGit::supports_method("agent.worktree.remove"));
    }

    #[test]
    fn local_git_context_converts_windows_cwd_for_wsl() {
        let context = local_git_context(Some("wsl-direct"), Some("Ubuntu"), Some(r"D:\work\repo"))
            .expect("WSL context");
        assert_eq!(context.cwd, "/mnt/d/work/repo");
        assert_eq!(
            context.host,
            GitHost::Wsl {
                distribution: Some("Ubuntu".to_string())
            }
        );
    }

    #[test]
    fn local_git_status_cursor_is_bound_to_generation_state_and_query() {
        let fingerprint = status_page_cursor_fingerprint(7, Some("unstaged"), Some("Cursor"));
        let cursor = format_cursor(7, fingerprint, 500);
        assert_eq!(parse_cursor(Some(&cursor), 7, fingerprint).unwrap(), 500);

        let changed_query = status_page_cursor_fingerprint(7, Some("unstaged"), Some("other"));
        let error = parse_cursor(Some(&cursor), 7, changed_query).unwrap_err();
        assert_eq!(error.code, ErrorCode::Conflict);
        assert!(parse_cursor(Some(&cursor), 8, fingerprint).is_err());
    }

    #[test]
    #[ignore = "runs Git and must be isolated from tests that temporarily mutate process PATH"]
    fn local_git_probe_and_mutation_use_the_shared_vcs_client() {
        if Command::new("git").arg("--version").status().is_err() {
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "agentmux-server-local-git-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        run_git(&root, &["init", "-q"]);
        run_git(&root, &["config", "user.name", "AgentMux Test"]);
        run_git(&root, &["config", "user.email", "agentmux@example.invalid"]);
        fs::write(root.join("tracked.txt"), "base\n").unwrap();
        run_git(&root, &["add", "tracked.txt"]);
        run_git(&root, &["commit", "-q", "-m", "base"]);
        fs::write(root.join("tracked.txt"), "changed\n").unwrap();

        let mut git = ServerLocalGit::probe(
            "ws_server",
            Some("conpty"),
            None,
            Some(&root.to_string_lossy()),
        )
        .expect("repository probe");
        let status: LegacyGitStatusResult = decode(git.handle_request(RequestEnvelope::new(
            "status",
            "git.status",
            r#"{"workspace_id":"ws_server"}"#,
            "token",
        )));
        assert!(status.is_repository);
        assert_eq!(status.files.len(), 1);

        let diff: LegacyGitDiffResult = decode(git.handle_request(RequestEnvelope::new(
            "legacy-diff",
            "git.diff",
            r#"{"workspace_id":"ws_server","path":"tracked.txt","staged":false}"#,
            "token",
        )));
        assert!(diff.patch.contains("changed"));

        let page: GitStatusPageResult = decode(git.handle_request(RequestEnvelope::new(
            "page",
            "git.status_page",
            r#"{"workspace_id":"ws_server","query":"tracked","limit":25}"#,
            "token",
        )));
        let page = if page.total_count.is_none() {
            let summary: GitStatusSummaryResult = decode(git.handle_request(RequestEnvelope::new(
                "page-summary",
                "git.status_summary",
                r#"{"workspace_id":"ws_server"}"#,
                "token",
            )));
            decode(
                git.handle_request(RequestEnvelope::new(
                    "page-complete",
                    "git.status_page",
                    serde_json::json!({
                        "workspace_id": "ws_server",
                        "repository_id": summary.repository_id,
                        "generation": summary.generation,
                        "query": "tracked",
                        "limit": 25
                    })
                    .to_string(),
                    "token",
                )),
            )
        } else {
            page
        };
        assert_eq!(page.total_count, Some(1));
        assert!(page.changes[0].unstaged);

        let mutation: GitMutationResult = decode(git.handle_request(RequestEnvelope::new(
            "stage",
            "git.stage",
            r#"{"workspace_id":"ws_server","paths":["tracked.txt"],"idempotency_key":"stage-1"}"#,
            "token",
        )));
        assert!(!mutation.reused);
        let replay: GitMutationResult = decode(git.handle_request(RequestEnvelope::new(
            "stage-replay",
            "git.stage",
            r#"{"workspace_id":"ws_server","paths":["tracked.txt"],"idempotency_key":"stage-1"}"#,
            "token",
        )));
        assert!(replay.reused);

        let _: GitMutationResult = decode(git.handle_request(RequestEnvelope::new(
            "unstage",
            "git.unstage",
            r#"{"workspace_id":"ws_server","paths":["tracked.txt"]}"#,
            "token",
        )));
        let _: GitMutationResult = decode(git.handle_request(RequestEnvelope::new(
            "stage-all",
            "git.stage_all",
            r#"{"workspace_id":"ws_server"}"#,
            "token",
        )));

        let commit: GitMutationResult = decode(git.handle_request(RequestEnvelope::new(
            "commit",
            "git.commit",
            r#"{"workspace_id":"ws_server","message":"track B local commit","idempotency_key":"commit-1"}"#,
            "token",
        )));
        assert!(commit.commit_oid.is_some());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "runs native Git and creates an idempotency fixture"]
    fn local_git_path_mutation_idempotency_binds_the_request_payload() {
        let Some((root, mut git)) = idempotency_fixture("path") else {
            return;
        };

        let first: GitMutationResult = decode(git.handle_request(RequestEnvelope::new(
            "stage-first",
            "git.stage",
            r#"{"workspace_id":"ws_server","paths":["first.txt"],"idempotency_key":"path-key"}"#,
            "token",
        )));
        assert!(!first.reused);

        let replay: GitMutationResult = decode(git.handle_request(RequestEnvelope::new(
            "stage-first-replay",
            "git.stage",
            r#"{"workspace_id":"ws_server","paths":["first.txt"],"idempotency_key":"path-key"}"#,
            "token",
        )));
        assert!(replay.reused);
        assert_eq!(replay.generation, first.generation);

        let error = control_error(git.handle_request(RequestEnvelope::new(
            "stage-second-with-reused-key",
            "git.stage",
            r#"{"workspace_id":"ws_server","paths":["second.txt"],"idempotency_key":"path-key"}"#,
            "token",
        )));
        assert_eq!(error.code, ErrorCode::Conflict);
        assert_eq!(
            error.message,
            "idempotency_key was already used with a different Git mutation request."
        );
        assert_eq!(
            git_output(&root, &["diff", "--cached", "--name-only"]).trim(),
            "first.txt"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "runs native Git and creates an idempotency fixture"]
    fn local_git_commit_idempotency_binds_the_request_payload() {
        let Some((root, mut git)) = idempotency_fixture("commit") else {
            return;
        };

        let _: GitMutationResult = decode(git.handle_request(RequestEnvelope::new(
            "stage",
            "git.stage",
            r#"{"workspace_id":"ws_server","paths":["first.txt"]}"#,
            "token",
        )));
        let commit: GitMutationResult = decode(git.handle_request(RequestEnvelope::new(
            "commit-first",
            "git.commit",
            r#"{"workspace_id":"ws_server","message":"first server commit","idempotency_key":"commit-key"}"#,
            "token",
        )));
        assert!(!commit.reused);
        assert!(commit.commit_oid.is_some());

        let replay: GitMutationResult = decode(git.handle_request(RequestEnvelope::new(
            "commit-first-replay",
            "git.commit",
            r#"{"workspace_id":"ws_server","message":"first server commit","idempotency_key":"commit-key"}"#,
            "token",
        )));
        assert!(replay.reused);
        assert_eq!(replay.commit_oid, commit.commit_oid);

        let error = control_error(git.handle_request(RequestEnvelope::new(
            "commit-second-with-reused-key",
            "git.commit",
            r#"{"workspace_id":"ws_server","message":"different server commit","idempotency_key":"commit-key"}"#,
            "token",
        )));
        assert_eq!(error.code, ErrorCode::Conflict);
        assert_eq!(
            error.message,
            "idempotency_key was already used with a different Git mutation request."
        );
        assert_eq!(
            git_output(&root, &["log", "-1", "--format=%s"]).trim(),
            "first server commit"
        );
        assert_eq!(
            git_output(&root, &["rev-list", "--count", "HEAD"]).trim(),
            "2"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "runs native Git and verifies a durable idempotency receipt"]
    fn local_git_commit_idempotency_survives_server_restart() {
        if Command::new("git").arg("--version").status().is_err() {
            return;
        }
        let suffix = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(format!("agentmux-server-local-git-restart-{suffix}"));
        let state =
            std::env::temp_dir().join(format!("agentmux-server-local-git-receipts-{suffix}"));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&state).unwrap();
        run_git(&root, &["init", "-q"]);
        run_git(&root, &["config", "user.name", "AgentMux Test"]);
        run_git(&root, &["config", "user.email", "agentmux@example.invalid"]);
        fs::write(root.join("tracked.txt"), "base\n").unwrap();
        run_git(&root, &["add", "tracked.txt"]);
        run_git(&root, &["commit", "-q", "-m", "base"]);
        fs::write(root.join("tracked.txt"), "changed\n").unwrap();
        run_git(&root, &["add", "tracked.txt"]);
        let database = state.join("agentmux.sqlite3");
        let request = r#"{"workspace_id":"ws_server","message":"durable server commit","idempotency_key":"durable-commit-key"}"#;

        let commit = {
            let store = SqliteStore::open(&database).expect("receipt store");
            let mut git = ServerLocalGit::probe_with_store(
                "ws_server",
                Some("conpty"),
                None,
                Some(&root.to_string_lossy()),
                store,
            )
            .expect("repository probe");
            let result: GitMutationResult = decode(git.handle_request(RequestEnvelope::new(
                "commit-before-restart",
                "git.commit",
                request,
                "token",
            )));
            assert!(!result.reused);
            result
        };

        let store = SqliteStore::open(&database).expect("reopened receipt store");
        let mut restarted = ServerLocalGit::probe_with_store(
            "ws_server",
            Some("conpty"),
            None,
            Some(&root.to_string_lossy()),
            store,
        )
        .expect("repository probe after restart");
        let replay: GitMutationResult = decode(restarted.handle_request(RequestEnvelope::new(
            "commit-after-restart",
            "git.commit",
            request,
            "token",
        )));
        assert!(replay.reused);
        assert_eq!(replay.commit_oid, commit.commit_oid);

        let conflict = control_error(restarted.handle_request(RequestEnvelope::new(
            "commit-after-restart-conflict",
            "git.commit",
            r#"{"workspace_id":"ws_server","message":"different request","idempotency_key":"durable-commit-key"}"#,
            "token",
        )));
        assert_eq!(conflict.code, ErrorCode::Conflict);
        assert_eq!(
            git_output(&root, &["rev-list", "--count", "HEAD"]).trim(),
            "2"
        );

        drop(restarted);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(state).unwrap();
    }

    #[test]
    #[ignore = "runs native Git and creates a paging fixture"]
    fn local_git_pages_reuse_one_snapshot_across_external_changes() {
        if Command::new("git").arg("--version").status().is_err() {
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "agentmux-server-local-git-paging-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        run_git(&root, &["init", "-q"]);
        for index in 0..600 {
            fs::write(root.join(format!("file-{index:04}.txt")), "fixture\n").unwrap();
        }
        let mut git = ServerLocalGit::probe(
            "ws_server",
            Some("conpty"),
            None,
            Some(&root.to_string_lossy()),
        )
        .expect("repository probe");

        let prefix: GitStatusPageResult = decode(git.handle_request(RequestEnvelope::new(
            "prefix",
            "git.status_page",
            r#"{"workspace_id":"ws_server","limit":25}"#,
            "token",
        )));
        assert!(!prefix.changes.is_empty());
        assert_eq!(git.status_refresh_count, 1);

        let summary: GitStatusSummaryResult = decode(git.handle_request(RequestEnvelope::new(
            "summary",
            "git.status_summary",
            r#"{"workspace_id":"ws_server"}"#,
            "token",
        )));
        let first: GitStatusPageResult = decode(
            git.handle_request(RequestEnvelope::new(
                "first-complete",
                "git.status_page",
                serde_json::json!({
                    "workspace_id": "ws_server",
                    "repository_id": summary.repository_id,
                    "generation": prefix.generation,
                    "limit": 25
                })
                .to_string(),
                "token",
            )),
        );
        assert_eq!(first.total_count, Some(600));
        assert_eq!(git.status_refresh_count, 1);

        fs::write(root.join("late-external.txt"), "late\n").unwrap();
        let second: GitStatusPageResult = decode(
            git.handle_request(RequestEnvelope::new(
                "second",
                "git.status_page",
                serde_json::json!({
                    "workspace_id": "ws_server",
                    "repository_id": first.repository_id,
                    "generation": first.generation,
                    "cursor": first.next_cursor,
                    "limit": 25
                })
                .to_string(),
                "token",
            )),
        );
        assert_eq!(second.total_count, Some(600));
        assert!(second
            .changes
            .iter()
            .all(|change| change.path != "late-external.txt"));
        assert_eq!(git.status_refresh_count, 1);

        let refreshed_prefix: GitStatusPageResult =
            decode(git.handle_request(RequestEnvelope::new(
                "refresh-prefix",
                "git.status_page",
                r#"{"workspace_id":"ws_server","limit":25}"#,
                "token",
            )));
        assert!(refreshed_prefix.generation > first.generation);
        let _: GitStatusSummaryResult = decode(git.handle_request(RequestEnvelope::new(
            "refresh-summary",
            "git.status_summary",
            r#"{"workspace_id":"ws_server"}"#,
            "token",
        )));
        let late: GitStatusPageResult = decode(
            git.handle_request(RequestEnvelope::new(
                "late-query",
                "git.status_page",
                serde_json::json!({
                    "workspace_id": "ws_server",
                    "generation": refreshed_prefix.generation,
                    "query": "late-external",
                    "limit": 25
                })
                .to_string(),
                "token",
            )),
        );
        assert_eq!(late.total_count, Some(1));
        assert_eq!(late.changes[0].path, "late-external.txt");
        assert_eq!(git.status_refresh_count, 2);

        fs::remove_dir_all(root).unwrap();
    }

    fn run_git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .expect("git command");
        assert!(status.success(), "git command failed: {args:?}");
    }

    fn git_output(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .expect("git command output");
        assert!(output.status.success(), "git command failed: {args:?}");
        String::from_utf8(output.stdout).expect("UTF-8 git output")
    }

    fn idempotency_fixture(label: &str) -> Option<(std::path::PathBuf, ServerLocalGit)> {
        if Command::new("git").arg("--version").status().is_err() {
            return None;
        }
        let root = std::env::temp_dir().join(format!(
            "agentmux-server-local-git-idempotency-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        run_git(&root, &["init", "-q"]);
        run_git(&root, &["config", "user.name", "AgentMux Test"]);
        run_git(&root, &["config", "user.email", "agentmux@example.invalid"]);
        fs::write(root.join("first.txt"), "base\n").unwrap();
        fs::write(root.join("second.txt"), "base\n").unwrap();
        run_git(&root, &["add", "first.txt", "second.txt"]);
        run_git(&root, &["commit", "-q", "-m", "base"]);
        fs::write(root.join("first.txt"), "first changed\n").unwrap();
        fs::write(root.join("second.txt"), "second changed\n").unwrap();
        let git = ServerLocalGit::probe(
            "ws_server",
            Some("conpty"),
            None,
            Some(&root.to_string_lossy()),
        )
        .expect("repository probe");
        Some((root, git))
    }

    fn control_error(response: ResponseEnvelope) -> ControlError {
        match response.outcome {
            ResponseOutcome::Error(error) => error,
            ResponseOutcome::Ok { result_json } => {
                panic!("expected control error, received result: {result_json}")
            }
        }
    }

    fn decode<T: serde::de::DeserializeOwned>(response: ResponseEnvelope) -> T {
        match response.outcome {
            ResponseOutcome::Ok { result_json } => serde_json::from_str(&result_json).unwrap(),
            ResponseOutcome::Error(error) => panic!("control request failed: {}", error.message),
        }
    }
}
