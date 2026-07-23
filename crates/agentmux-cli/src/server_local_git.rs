use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use agentmux_backend_wsl::fallback_windows_path_to_wsl;
use agentmux_ipc::{
    ControlError, ErrorCode, GitAllMutationParams, GitChangeSummaryResult, GitCommitParams,
    GitDiffParams, GitDiffResult, GitMutationResult, GitPathMutationParams, GitRepositoryParams,
    GitStatusPageParams, GitStatusPageResult, GitStatusSummaryResult, RequestEnvelope,
    ResponseEnvelope,
};
use agentmux_vcs::{
    DiffRequest, GitClient, GitContext, GitError, GitFileChange, GitHost, Repository, StatusSummary,
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
const MAX_IDEMPOTENCY_RESULTS: usize = 256;

pub(crate) struct ServerLocalGit {
    client: GitClient,
    repository: Repository,
    repository_id: String,
    workspace_id: String,
    idempotency_results: HashMap<String, GitMutationResult>,
}

impl ServerLocalGit {
    pub(crate) fn probe(
        workspace_id: &str,
        backend: Option<&str>,
        backend_profile: Option<&str>,
        cwd: Option<&str>,
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
            idempotency_results: HashMap::new(),
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

    fn status(&self, request: &RequestEnvelope) -> Result<ResponseEnvelope, ControlError> {
        let params: GitRepositoryParams = request.parse_params()?;
        self.validate_selector(&params.workspace_id, params.repository_id.as_deref())?;
        let snapshot = self
            .client
            .read_status(&self.repository)
            .map_err(vcs_error)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &LegacyGitStatusResult {
                is_repository: true,
                repository_root: Some(snapshot.summary.repository_root),
                branch: snapshot.summary.branch,
                head: snapshot.summary.head,
                upstream: snapshot.summary.upstream,
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

    fn status_summary(&self, request: &RequestEnvelope) -> Result<ResponseEnvelope, ControlError> {
        let params: GitRepositoryParams = request.parse_params()?;
        self.validate_selector(&params.workspace_id, params.repository_id.as_deref())?;
        let summary = self
            .client
            .status_summary(&self.repository)
            .map_err(vcs_error)?;
        Ok(ResponseEnvelope::ok_typed(
            request.id.clone(),
            &self.summary_result(summary),
        ))
    }

    fn status_page(&self, request: &RequestEnvelope) -> Result<ResponseEnvelope, ControlError> {
        let params: GitStatusPageParams = request.parse_params()?;
        params.validate()?;
        self.validate_selector(&params.workspace_id, params.repository_id.as_deref())?;
        let snapshot = self
            .client
            .read_status(&self.repository)
            .map_err(vcs_error)?;
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
        let limit = params.limit.unwrap_or(250).clamp(1, 500);
        let end = offset.saturating_add(limit).min(matching.len());
        let changes = matching[offset..end]
            .iter()
            .map(|change| change_result(change))
            .collect();
        let summary = self.summary_result(snapshot.summary);
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
        if let Some(result) =
            self.reused_result(request.method.as_str(), params.idempotency_key.as_deref())
        {
            return Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result));
        }
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
            &result,
        );
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
        if let Some(result) =
            self.reused_result(request.method.as_str(), params.idempotency_key.as_deref())
        {
            return Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result));
        }
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
            &result,
        );
        Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result))
    }

    fn commit(&mut self, request: &RequestEnvelope) -> Result<ResponseEnvelope, ControlError> {
        let params: GitCommitParams = request.parse_params()?;
        params.validate()?;
        self.validate_selector(&params.workspace_id, params.repository_id.as_deref())?;
        if params.amend {
            return Err(invalid_request(
                "Amend is not available through local server mode.",
            ));
        }
        if let Some(result) =
            self.reused_result(request.method.as_str(), params.idempotency_key.as_deref())
        {
            return Ok(ResponseEnvelope::ok_typed(request.id.clone(), &result));
        }
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
            &result,
        );
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
    ) -> Option<GitMutationResult> {
        let key = mutation_key(method, &self.repository_id, idempotency_key?);
        self.idempotency_results
            .get(&key)
            .cloned()
            .map(|mut result| {
                result.reused = true;
                result
            })
    }

    fn remember_result(
        &mut self,
        method: &str,
        idempotency_key: Option<&str>,
        result: &GitMutationResult,
    ) {
        let Some(idempotency_key) = idempotency_key else {
            return;
        };
        if self.idempotency_results.len() >= MAX_IDEMPOTENCY_RESULTS {
            if let Some(key) = self.idempotency_results.keys().next().cloned() {
                self.idempotency_results.remove(&key);
            }
        }
        self.idempotency_results.insert(
            mutation_key(method, &self.repository_id, idempotency_key),
            result.clone(),
        );
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

fn mutation_key(method: &str, repository_id: &str, idempotency_key: &str) -> String {
    format!("{method}|{repository_id}|{idempotency_key}")
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

    fn run_git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .expect("git command");
        assert!(status.success(), "git command failed: {args:?}");
    }

    fn decode<T: serde::de::DeserializeOwned>(response: ResponseEnvelope) -> T {
        match response.outcome {
            ResponseOutcome::Ok { result_json } => serde_json::from_str(&result_json).unwrap(),
            ResponseOutcome::Error(error) => panic!("control request failed: {}", error.message),
        }
    }
}
