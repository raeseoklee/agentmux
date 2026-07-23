use std::collections::BTreeMap;
use std::io::Write;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use agentmux_ipc::{
    AgentWorktreeCreateParams, AgentWorktreeListParams, AgentWorktreeRecoverParams,
    AgentWorktreeRemoveParams, ControlCaller, DevelopmentServerCandidateDismissParams,
    DevelopmentServerCandidateListParams, DevelopmentServerCandidateOpenInSplitParams,
    GitAllMutationParams, GitCommitParams, GitDiffParams, GitPathMutationParams,
    GitRepositoryParams, GitReviewCommentCreateParams, GitReviewCommentIdParams,
    GitReviewCommentListParams, GitReviewCommentUpdateParams, GitReviewLineAnchor,
    GitReviewThreadCreateParams, GitReviewThreadDeliverParams, GitReviewThreadIdParams,
    GitReviewThreadListParams, GitReviewThreadMarkStaleParams, GitReviewThreadUpdateParams,
    GitStatusPageParams, METHOD_AGENT_WORKTREE_CREATE, METHOD_AGENT_WORKTREE_LIST,
    METHOD_AGENT_WORKTREE_RECOVER, METHOD_AGENT_WORKTREE_REMOVE,
    METHOD_DEV_SERVER_CANDIDATE_DISMISS, METHOD_DEV_SERVER_CANDIDATE_LIST,
    METHOD_DEV_SERVER_CANDIDATE_OPEN_IN_SPLIT, METHOD_GIT_COMMIT, METHOD_GIT_DIFF,
    METHOD_GIT_DISCARD, METHOD_GIT_REVIEW_COMMENT_CREATE, METHOD_GIT_REVIEW_COMMENT_DELETE,
    METHOD_GIT_REVIEW_COMMENT_LIST, METHOD_GIT_REVIEW_COMMENT_UPDATE,
    METHOD_GIT_REVIEW_THREAD_CREATE, METHOD_GIT_REVIEW_THREAD_DELETE,
    METHOD_GIT_REVIEW_THREAD_DELIVER, METHOD_GIT_REVIEW_THREAD_LIST,
    METHOD_GIT_REVIEW_THREAD_MARK_STALE, METHOD_GIT_REVIEW_THREAD_UPDATE, METHOD_GIT_STAGE,
    METHOD_GIT_STAGE_ALL, METHOD_GIT_STATUS_PAGE, METHOD_GIT_STATUS_SUMMARY, METHOD_GIT_UNSTAGE,
    METHOD_GIT_UNSTAGE_ALL,
};

use super::mcp_config::{
    self, McpClient, McpConfigRequest, McpConfigStatus, McpProfile as McpConfigProfile,
};
use super::{
    agent_integration_doctor_result_json, agent_integration_install_result_json,
    ensure_windows_user_path_contains, inspect_agent_integrations, install_agent_integration_shims,
    invoke_control_with_caller, path_to_wsl_value, resolve_cmuxterm_base_dir,
    resolve_control_token, AgentIntegrationKind, CliError, ControlInvokeOptions, ResponseOutcome,
};

const MCP_PROFILE_READ: &str = "read";
const MCP_PROFILE_STANDARD: &str = "standard";
const MCP_PROFILE_FULL: &str = "full";
const MAX_TERMINAL_READ_BYTES: usize = 1_048_576;
const MAX_EVENT_COUNT: usize = 500;
const DEFAULT_BROWSER_READ_BYTES: usize = 262_144;
const MAX_BROWSER_READ_BYTES: usize = 1_048_576;
const MAX_AGENT_TEAM_WORKERS: usize = 8;
const MAX_STANDARD_REVIEW_SCOPE_THREADS: usize = 500;
const READ_TOOL_NAMES: &[&str] = &[
    "agentmux_context",
    "workspace_list",
    "workspace_get",
    "session_list",
    "terminal_read",
    "agent_attention_list",
    "agent_worker_list",
    "agent_team_list",
    "agent_integration_status",
    "event_poll",
    "browser_snapshot",
    "browser_get",
    "team_task_list",
    "team_message_list",
    "diagnostics_summary",
    "agent_worktree_list",
    "git_status_summary",
    "git_status_page",
    "git_diff",
    "git_review_thread_list",
    "git_review_comment_list",
    "development_server_candidate_list",
];
const STANDARD_TOOL_NAMES: &[&str] = &[
    "pane_focus",
    "terminal_open",
    "terminal_split",
    "terminal_send_text",
    "terminal_send_key",
    "browser_open",
    "browser_open_split",
    "browser_navigate",
    "browser_click",
    "browser_fill",
    "team_message_send",
    "team_task_claim",
    "team_task_complete",
    "agent_set_state",
    "agent_worker_start",
    "agent_team_start",
    "agent_team_spawn",
    "agent_team_reflow",
    "agent_worker_send",
    "agent_worktree_create",
    "agent_worktree_recover",
    "git_review_thread_create",
    "git_review_thread_update",
    "git_review_thread_mark_stale",
    "git_review_comment_create",
    "git_review_comment_update",
    "git_review_comment_delete",
    "git_stage",
    "git_unstage",
    "git_commit",
    "git_review_thread_deliver",
    "development_server_candidate_dismiss",
    "development_server_candidate_open_in_split",
];
#[cfg(test)]
const STANDARD_DESTRUCTIVE_TOOL_NAMES: &[&str] = &[
    "terminal_open",
    "terminal_split",
    "terminal_send_text",
    "terminal_send_key",
    "browser_open",
    "browser_open_split",
    "browser_navigate",
    "browser_click",
    "browser_fill",
    "team_task_claim",
    "team_task_complete",
    "agent_set_state",
    "agent_worker_start",
    "agent_team_start",
    "agent_team_spawn",
    "agent_team_reflow",
    "agent_worker_send",
    "agent_worktree_create",
    "agent_worktree_recover",
    "git_stage",
    "git_unstage",
    "git_commit",
    "git_review_thread_deliver",
    "git_review_comment_delete",
    "development_server_candidate_open_in_split",
];
#[cfg(test)]
const STANDARD_ADDITIVE_TOOL_NAMES: &[&str] = &[
    "pane_focus",
    "team_message_send",
    "git_review_thread_create",
    "git_review_thread_update",
    "git_review_thread_mark_stale",
    "git_review_comment_create",
    "git_review_comment_update",
    "development_server_candidate_dismiss",
];
const FULL_TOOL_NAMES: &[&str] = &[
    "workspace_close",
    "pane_close",
    "surface_close",
    "session_terminate",
    "config_update",
    "browser_evaluate",
    "action_run",
    "notification_clear",
    "agent_worker_stop",
    "agent_team_release",
    "agent_integration_setup",
    "agent_worktree_remove",
    "git_stage_all",
    "git_unstage_all",
    "git_discard",
    "git_review_thread_delete",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum McpProfile {
    Read,
    Standard,
    Full,
}

impl McpProfile {
    fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            MCP_PROFILE_READ => Ok(Self::Read),
            MCP_PROFILE_STANDARD => Ok(Self::Standard),
            MCP_PROFILE_FULL => Ok(Self::Full),
            _ => Err(CliError::InvalidArgs(format!(
                "unknown MCP profile '{value}'; expected 'read', 'standard', or 'full'."
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Read => MCP_PROFILE_READ,
            Self::Standard => MCP_PROFILE_STANDARD,
            Self::Full => MCP_PROFILE_FULL,
        }
    }

    fn allows_standard(self) -> bool {
        matches!(self, Self::Standard | Self::Full)
    }

    fn allows_full(self) -> bool {
        matches!(self, Self::Full)
    }

    fn allows_tool(self, name: &str) -> bool {
        match Self::required_for_tool(name) {
            Some(Self::Read) => true,
            Some(Self::Standard) => self.allows_standard(),
            Some(Self::Full) => self.allows_full(),
            None => false,
        }
    }

    fn required_for_tool(name: &str) -> Option<Self> {
        if FULL_TOOL_NAMES.contains(&name) {
            Some(Self::Full)
        } else if STANDARD_TOOL_NAMES.contains(&name) {
            Some(Self::Standard)
        } else if READ_TOOL_NAMES.contains(&name) {
            Some(Self::Read)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
struct McpOptions {
    invoke: ControlInvokeOptions,
    profile: McpProfile,
}

impl McpOptions {
    fn from_env() -> Self {
        Self {
            invoke: ControlInvokeOptions::from_env(),
            profile: McpProfile::Read,
        }
    }
}

pub(super) fn run_command<W>(command: &str, args: &[String], output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        writeln!(output, "{}", command_usage(command))?;
        return Ok(());
    }

    match command {
        "serve" => run_serve(parse_options(args, false)?),
        "doctor" => {
            let (options, json_output) = parse_doctor_options(args)?;
            run_doctor(options, json_output, output)
        }
        "setup" => run_setup(parse_setup_options(args)?, output),
        "help" | "--help" | "-h" => {
            writeln!(output, "{}", command_usage(""))?;
            Ok(())
        }
        _ => Err(CliError::InvalidArgs(format!(
            "unknown MCP command '{command}'; expected 'serve', 'doctor', or 'setup'."
        ))),
    }
}

fn command_usage(command: &str) -> &'static str {
    match command {
        "serve" => {
            "agentmux mcp serve [--profile read|standard|full] [--pipe <name>] [--token <token> | --token-file <path>]"
        }
        "doctor" => {
            "agentmux mcp doctor [--profile read|standard|full] [--pipe <name>] [--token <token> | --token-file <path>] [--json]"
        }
        "setup" => {
            "agentmux mcp setup --client codex|claude [--profile read|standard|full] [--config <path>] [--executable <agentmux.exe>] [--install] [--json]"
        }
        _ => "agentmux mcp <serve|doctor|setup> [options]",
    }
}

#[derive(Debug)]
struct McpSetupOptions {
    client: McpClient,
    profile: McpConfigProfile,
    config_path: Option<std::path::PathBuf>,
    executable_path: Option<std::path::PathBuf>,
    install: bool,
    json_output: bool,
}

fn parse_setup_options(args: &[String]) -> Result<McpSetupOptions, CliError> {
    let mut client = None;
    let mut profile = McpConfigProfile::Read;
    let mut config_path = None;
    let mut executable_path = None;
    let mut install = false;
    let mut json_output = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--client" => {
                client = Some(option_value(args, index, "--client")?.parse().map_err(
                    |error: mcp_config::McpConfigError| CliError::InvalidArgs(error.to_string()),
                )?);
                index += 2;
            }
            "--profile" => {
                profile = option_value(args, index, "--profile")?.parse().map_err(
                    |error: mcp_config::McpConfigError| CliError::InvalidArgs(error.to_string()),
                )?;
                index += 2;
            }
            "--config" => {
                config_path = Some(std::path::PathBuf::from(option_value(
                    args, index, "--config",
                )?));
                index += 2;
            }
            "--executable" => {
                executable_path = Some(std::path::PathBuf::from(option_value(
                    args,
                    index,
                    "--executable",
                )?));
                index += 2;
            }
            "--install" => {
                install = true;
                index += 1;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown MCP setup option '{value}'."
                )))
            }
        }
    }
    let client = client.ok_or_else(|| {
        CliError::InvalidArgs("mcp setup requires --client codex|claude.".to_string())
    })?;
    Ok(McpSetupOptions {
        client,
        profile,
        config_path,
        executable_path,
        install,
        json_output,
    })
}

fn run_setup<W>(options: McpSetupOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let config_path = match options.config_path {
        Some(path) => path,
        None => mcp_config::default_config_path(options.client)
            .map_err(|error| CliError::Control(error.to_string()))?,
    };
    let executable_path = match options.executable_path {
        Some(path) => path,
        None => std::env::current_exe().map_err(CliError::Io)?,
    };
    let result = mcp_config::configure(&McpConfigRequest {
        client: options.client,
        profile: options.profile,
        executable_path,
        config_path,
        install: options.install,
    })
    .map_err(|error| CliError::Control(error.to_string()))?;

    if options.json_output {
        writeln!(
            output,
            "{}",
            serde_json::to_string_pretty(&result).map_err(|error| {
                CliError::Control(format!("failed to encode MCP setup result: {error}"))
            })?
        )?;
    } else {
        let status = match result.status {
            McpConfigStatus::Preview => "preview",
            McpConfigStatus::Installed => "installed",
            McpConfigStatus::Unchanged => "unchanged",
        };
        writeln!(output, "MCP client: {:?}", result.client)?;
        writeln!(output, "Profile: {}", result.profile.as_str())?;
        writeln!(output, "Config: {}", result.config_path.display())?;
        writeln!(output, "Status: {status}")?;
        if let Some(backup) = result.backup_path {
            writeln!(output, "Backup: {}", backup.display())?;
        }
        writeln!(output, "\n{}", result.preview)?;
        if !options.install {
            writeln!(
                output,
                "\nPreview only. Re-run with --install to apply this configuration."
            )?;
        }
    }
    Ok(())
}

fn parse_doctor_options(args: &[String]) -> Result<(McpOptions, bool), CliError> {
    let json_output = args.iter().any(|arg| arg == "--json");
    let filtered = args
        .iter()
        .filter(|arg| arg.as_str() != "--json")
        .cloned()
        .collect::<Vec<_>>();
    Ok((parse_options(&filtered, true)?, json_output))
}

fn parse_options(args: &[String], _doctor: bool) -> Result<McpOptions, CliError> {
    let mut options = McpOptions::from_env();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--profile" => {
                options.profile = McpProfile::parse(option_value(args, index, "--profile")?)?;
                index += 2;
            }
            "--pipe" => {
                options.invoke.pipe_name = option_value(args, index, "--pipe")?.to_string();
                index += 2;
            }
            "--token" => {
                options.invoke.token = Some(option_value(args, index, "--token")?.to_string());
                index += 2;
            }
            "--token-file" => {
                options.invoke.token_path =
                    Some(option_value(args, index, "--token-file")?.to_string());
                index += 2;
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown MCP option '{value}'."
                )))
            }
        }
    }

    if options.invoke.token.is_some() && options.invoke.token_path.is_some() {
        return Err(CliError::InvalidArgs(
            "--token and --token-file cannot be used together.".to_string(),
        ));
    }
    Ok(options)
}

fn option_value<'a>(args: &'a [String], index: usize, option: &str) -> Result<&'a str, CliError> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| CliError::InvalidArgs(format!("{option} requires a value.")))
}

fn run_serve(options: McpOptions) -> Result<(), CliError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| CliError::Control(format!("failed to start MCP runtime: {error}")))?;
    runtime.block_on(async move {
        let server = AgentMuxMcpServer::new(options);
        let service = server
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|error| {
                CliError::Control(format!("failed to start MCP stdio server: {error}"))
            })?;
        service
            .waiting()
            .await
            .map_err(|error| CliError::Control(format!("MCP stdio server stopped: {error}")))?;
        Ok(())
    })
}

#[derive(Debug, Serialize)]
struct DoctorControlPlane {
    reachable: bool,
    control_schema: Option<String>,
    method_count: Option<usize>,
}

#[derive(Debug, Serialize)]
struct DoctorResult {
    ok: bool,
    profile: &'static str,
    transport: &'static str,
    pipe_name: String,
    token_source: &'static str,
    control_plane: DoctorControlPlane,
    errors: Vec<String>,
}

fn run_doctor<W>(options: McpOptions, json_output: bool, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let token_source = if options.invoke.token.is_some() {
        "environment_or_argument"
    } else if options.invoke.token_path.is_some() {
        "explicit_file"
    } else {
        "default_file"
    };
    let mut errors = Vec::new();
    if let Err(error) = resolve_control_token(&options.invoke) {
        errors.push(error.to_string());
    }

    let capabilities = if errors.is_empty() {
        NamedPipeTransport::new(options.invoke.clone())
            .invoke("system.capabilities", json!({}))
            .map_err(|error| {
                errors.push(error);
            })
            .ok()
    } else {
        None
    };
    let result = DoctorResult {
        ok: errors.is_empty(),
        profile: options.profile.as_str(),
        transport: "stdio",
        pipe_name: options.invoke.pipe_name,
        token_source,
        control_plane: DoctorControlPlane {
            reachable: capabilities.is_some(),
            control_schema: capabilities
                .as_ref()
                .and_then(|value| value.get("control_schema"))
                .and_then(Value::as_str)
                .map(ToString::to_string),
            method_count: capabilities
                .as_ref()
                .and_then(|value| value.get("methods"))
                .and_then(Value::as_array)
                .map(Vec::len),
        },
        errors,
    };

    if json_output {
        writeln!(
            output,
            "{}",
            serde_json::to_string_pretty(&result).map_err(|error| {
                CliError::Control(format!("failed to encode MCP doctor result: {error}"))
            })?
        )?;
    } else {
        writeln!(output, "MCP profile: {}", result.profile)?;
        writeln!(output, "Transport: {}", result.transport)?;
        writeln!(output, "Control pipe: {}", result.pipe_name)?;
        writeln!(
            output,
            "Control plane: {}",
            if result.control_plane.reachable {
                "reachable"
            } else {
                "unavailable"
            }
        )?;
        for error in &result.errors {
            writeln!(output, "Error: {error}")?;
        }
    }

    if result.ok {
        Ok(())
    } else {
        Err(CliError::Control(
            "AgentMux MCP doctor found an unavailable control plane.".to_string(),
        ))
    }
}

trait ControlTransport: Send + Sync {
    fn invoke(&self, method: &str, params: Value) -> Result<Value, String>;
}

#[derive(Clone)]
struct NamedPipeTransport {
    options: ControlInvokeOptions,
    caller: Option<ControlCaller>,
}

impl NamedPipeTransport {
    fn new(options: ControlInvokeOptions) -> Self {
        Self {
            options,
            caller: None,
        }
    }

    fn for_mcp(options: ControlInvokeOptions, profile: McpProfile) -> Self {
        Self::for_mcp_source(options, profile, "mcp")
    }

    fn for_mcp_source(options: ControlInvokeOptions, profile: McpProfile, source: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Self {
            options,
            caller: Some(ControlCaller {
                source: source.to_string(),
                profile: Some(profile.as_str().to_string()),
                client_session_id: Some(format!("mcp-{}-{nonce}", std::process::id())),
            }),
        }
    }
}

impl ControlTransport for NamedPipeTransport {
    fn invoke(&self, method: &str, params: Value) -> Result<Value, String> {
        let response =
            invoke_control_with_caller(method, &params, &self.options, self.caller.clone())
                .map_err(|e| e.to_string())?;
        match response.outcome {
            ResponseOutcome::Ok { result_json } => serde_json::from_str(&result_json)
                .map_err(|error| format!("invalid control response for '{method}': {error}")),
            ResponseOutcome::Error(error) => {
                Err(format!("{}: {}", error.code.as_str(), error.message))
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct AgentMuxMcpServer {
    profile: McpProfile,
    control: Arc<dyn ControlTransport>,
    standard_caller_binding: Option<StandardCallerBinding>,
    team_operations: Arc<tokio::sync::Mutex<()>>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl AgentMuxMcpServer {
    fn new(options: McpOptions) -> Self {
        Self::with_transport_and_binding(
            options.profile,
            Arc::new(NamedPipeTransport::for_mcp(options.invoke, options.profile)),
            StandardCallerBinding::from_environment(),
        )
    }

    fn with_transport(profile: McpProfile, control: Arc<dyn ControlTransport>) -> Self {
        Self::with_transport_and_binding(profile, control, None)
    }

    fn with_transport_and_binding(
        profile: McpProfile,
        control: Arc<dyn ControlTransport>,
        standard_caller_binding: Option<StandardCallerBinding>,
    ) -> Self {
        let mut tool_router = Self::tool_router();
        let denied = tool_router
            .list_all()
            .into_iter()
            .filter_map(|tool| {
                (!profile.allows_tool(tool.name.as_ref())).then(|| tool.name.into_owned())
            })
            .collect::<Vec<_>>();
        for name in denied {
            tool_router.remove_route(&name);
        }
        Self {
            profile,
            control,
            standard_caller_binding,
            team_operations: Arc::new(tokio::sync::Mutex::new(())),
            tool_router,
        }
    }

    pub(super) fn for_http(
        options: ControlInvokeOptions,
        profile: super::mcp_http::McpAccessProfile,
    ) -> Self {
        let profile = match profile {
            super::mcp_http::McpAccessProfile::Read => McpProfile::Read,
            super::mcp_http::McpAccessProfile::Standard => McpProfile::Standard,
            super::mcp_http::McpAccessProfile::Full => McpProfile::Full,
        };
        Self::with_transport(
            profile,
            Arc::new(NamedPipeTransport::for_mcp_source(
                options, profile, "mcp-http",
            )),
        )
    }

    async fn call(&self, method: &'static str, params: Value) -> CallToolResult {
        match self.invoke_value(method, params).await {
            Ok(value) => CallToolResult::structured(value),
            Err(message) => control_error_result(method, message),
        }
    }

    async fn invoke_value(&self, method: &'static str, params: Value) -> Result<Value, String> {
        let control = Arc::clone(&self.control);
        match tokio::task::spawn_blocking(move || control.invoke(method, params)).await {
            Ok(result) => result,
            Err(error) => Err(format!("control task failed: {error}")),
        }
    }

    async fn resolve_context(
        &self,
        workspace_id: Option<String>,
    ) -> Result<ResolvedContext, String> {
        let value = self
            .invoke_value("system.identify", json!({ "workspace_id": workspace_id }))
            .await?;
        serde_json::from_value(value)
            .map_err(|error| format!("invalid system.identify response: {error}"))
    }

    async fn resolve_standard_caller_scope(
        &self,
        tool_name: &'static str,
        requested_workspace_id: Option<&str>,
        requested_pane_id: Option<&str>,
    ) -> Result<StandardCallerScope, CallToolResult> {
        if self.profile != McpProfile::Standard {
            return Err(standard_scope_denied_result(
                tool_name,
                "caller-scoped authorization is available only to the standard profile",
            ));
        }
        let binding = self.standard_caller_binding.as_ref().ok_or_else(|| {
            standard_scope_denied_result(
                tool_name,
                "the MCP connection is not bound to a verifiable AgentMux pane",
            )
        })?;
        let context = self
            .invoke_value(
                "system.identify",
                json!({
                    "workspace_id": binding.workspace_id,
                    "pane_id": binding.pane_id,
                    "surface_id": binding.surface_id,
                }),
            )
            .await
            .and_then(|value| {
                serde_json::from_value::<ResolvedContext>(value)
                    .map_err(|error| format!("invalid system.identify response: {error}"))
            })
            .map_err(|message| control_error_result("system.identify", message))?;
        let workspace_id = context
            .workspace_id
            .ok_or_else(|| missing_context_result("workspace_id"))?;
        if workspace_id != binding.workspace_id {
            return Err(standard_scope_denied_result(
                tool_name,
                "the resolved workspace does not match the MCP connection binding",
            ));
        }
        if requested_workspace_id.is_some_and(|requested| requested != workspace_id) {
            return Err(standard_scope_denied_result(
                tool_name,
                "the requested workspace is not the caller's active workspace",
            ));
        }
        let pane_id = context
            .pane_id
            .ok_or_else(|| missing_context_result("pane_id"))?;
        if pane_id != binding.pane_id {
            return Err(standard_scope_denied_result(
                tool_name,
                "the resolved pane does not match the MCP connection binding",
            ));
        }
        if requested_pane_id.is_some_and(|requested| requested != pane_id) {
            return Err(standard_scope_denied_result(
                tool_name,
                "the requested pane is not the caller's active pane",
            ));
        }
        let session_id = context
            .session_id
            .ok_or_else(|| missing_context_result("session_id"))?;
        if context.surface_id.as_deref() != Some(binding.surface_id.as_str()) {
            return Err(standard_scope_denied_result(
                tool_name,
                "the resolved surface does not match the MCP connection binding",
            ));
        }
        let summary = self
            .invoke_value(
                METHOD_GIT_STATUS_SUMMARY,
                json!(GitRepositoryParams {
                    workspace_id: workspace_id.clone(),
                    pane_id: Some(pane_id.clone()),
                    repository_id: None,
                }),
            )
            .await
            .map_err(|message| control_error_result(METHOD_GIT_STATUS_SUMMARY, message))?;
        let repository_id = summary
            .get("repository_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                standard_scope_denied_result(
                    tool_name,
                    "the caller's active pane does not resolve to a Git repository",
                )
            })?;
        if summary.get("workspace_id").and_then(Value::as_str) != Some(workspace_id.as_str()) {
            return Err(standard_scope_denied_result(
                tool_name,
                "the resolved repository does not belong to the caller's active workspace",
            ));
        }
        Ok(StandardCallerScope {
            workspace_id,
            pane_id,
            session_id,
            repository_id: repository_id.to_string(),
        })
    }

    async fn authorize_standard_git_scope(
        &self,
        tool_name: &'static str,
        workspace_id: &str,
        pane_id: Option<&str>,
        repository_id: Option<&str>,
    ) -> Result<StandardCallerScope, CallToolResult> {
        let scope = self
            .resolve_standard_caller_scope(tool_name, Some(workspace_id), pane_id)
            .await?;
        if repository_id.is_some_and(|repository_id| repository_id != scope.repository_id) {
            return Err(standard_scope_denied_result(
                tool_name,
                "the requested repository is not selected by the caller's active pane",
            ));
        }
        Ok(scope)
    }

    async fn standard_owned_review_threads(
        &self,
        tool_name: &'static str,
    ) -> Result<(StandardCallerScope, Vec<Value>), CallToolResult> {
        let scope = self
            .resolve_standard_caller_scope(tool_name, None, None)
            .await?;
        let result = self
            .invoke_value(
                METHOD_GIT_REVIEW_THREAD_LIST,
                json!(GitReviewThreadListParams {
                    workspace_id: scope.workspace_id.clone(),
                    pane_id: Some(scope.pane_id.clone()),
                    repository_id: Some(scope.repository_id.clone()),
                    path: None,
                    include_resolved: true,
                    include_stale: true,
                    limit: Some(MAX_STANDARD_REVIEW_SCOPE_THREADS),
                }),
            )
            .await
            .map_err(|message| control_error_result(METHOD_GIT_REVIEW_THREAD_LIST, message))?;
        let threads = result
            .get("threads")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| {
                control_error_result(
                    METHOD_GIT_REVIEW_THREAD_LIST,
                    "invalid review thread list response".to_string(),
                )
            })?;
        Ok((scope, threads))
    }

    async fn authorize_standard_review_thread(
        &self,
        tool_name: &'static str,
        thread_id: &str,
    ) -> Result<StandardCallerScope, CallToolResult> {
        let (scope, threads) = self.standard_owned_review_threads(tool_name).await?;
        let Some(thread) = threads
            .iter()
            .find(|thread| thread.get("thread_id").and_then(Value::as_str) == Some(thread_id))
        else {
            return Err(standard_scope_denied_result(
                tool_name,
                "the review thread is outside the caller's active repository",
            ));
        };
        if !review_thread_matches_scope(thread, &scope) {
            return Err(standard_scope_denied_result(
                tool_name,
                "the review thread is outside the caller's active workspace or repository",
            ));
        }
        if review_thread_owner_session_id(thread) != Some(scope.session_id.as_str()) {
            return Err(standard_scope_denied_result(
                tool_name,
                "the review thread was not created by the caller's active agent session",
            ));
        }
        Ok(scope)
    }

    async fn authorize_standard_review_comment(
        &self,
        tool_name: &'static str,
        comment_id: &str,
    ) -> Result<StandardCallerScope, CallToolResult> {
        let (scope, threads) = self.standard_owned_review_threads(tool_name).await?;
        for thread in &threads {
            if !review_thread_matches_scope(thread, &scope) {
                continue;
            }
            let Some(comment) =
                thread
                    .get("comments")
                    .and_then(Value::as_array)
                    .and_then(|comments| {
                        comments.iter().find(|comment| {
                            comment.get("comment_id").and_then(Value::as_str) == Some(comment_id)
                        })
                    })
            else {
                continue;
            };
            if review_thread_owner_session_id(thread) != Some(scope.session_id.as_str()) {
                return Err(standard_scope_denied_result(
                    tool_name,
                    "the review comment belongs to a thread not owned by the caller's active agent session",
                ));
            }
            if comment.get("author_session_id").and_then(Value::as_str)
                != Some(scope.session_id.as_str())
            {
                return Err(standard_scope_denied_result(
                    tool_name,
                    "the review comment was not authored by the caller's active agent session",
                ));
            }
            return Ok(scope);
        }
        Err(standard_scope_denied_result(
            tool_name,
            "the review comment is outside the caller's active repository",
        ))
    }

    async fn authorize_standard_review_delivery(
        &self,
        tool_name: &'static str,
        request: &GitReviewThreadDeliverParams,
    ) -> Result<(), CallToolResult> {
        let scope = self
            .authorize_standard_review_thread(tool_name, &request.thread_id)
            .await?;

        let target_session_id = request.target_session_id.as_deref().ok_or_else(|| {
            control_error_result(tool_name, "target_session_id is required".to_string())
        })?;
        let sessions = self
            .invoke_value(
                "session.list",
                json!({ "workspace_id": scope.workspace_id }),
            )
            .await
            .map_err(|message| control_error_result("session.list", message))?;
        let target_is_owned = sessions
            .get("sessions")
            .and_then(Value::as_array)
            .is_some_and(|sessions| {
                sessions.iter().any(|session| {
                    session.get("session_id").and_then(Value::as_str) == Some(target_session_id)
                })
            });
        if !target_is_owned {
            return Err(standard_scope_denied_result(
                tool_name,
                "the delivery target is not owned by the caller's active workspace",
            ));
        }
        Ok(())
    }

    async fn resolve_agent_integration_distribution_for_workspace(
        &self,
        workspace_id: Option<String>,
        explicit: Option<String>,
    ) -> Result<Option<String>, String> {
        if let Some(distribution) = explicit
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(Some(distribution.to_string()));
        }

        let (workspace_id, workspace_required) = match workspace_id {
            Some(workspace_id) if !workspace_id.trim().is_empty() => (Some(workspace_id), true),
            _ => self
                .resolve_context(None)
                .await
                .ok()
                .and_then(|context| context.workspace_id)
                .map_or((None, false), |workspace_id| (Some(workspace_id), false)),
        };
        if let Some(workspace_id) = workspace_id {
            match self
                .invoke_value("workspace.get", json!({ "workspace_id": workspace_id }))
                .await
            {
                Ok(workspace) => {
                    if let Some(distribution) = workspace
                        .pointer("/workspace/default_wsl_distribution")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    {
                        return Ok(Some(distribution.to_string()));
                    }
                }
                Err(message) if workspace_required => return Err(message),
                Err(_) => {}
            }
        }

        Ok(resolve_agent_integration_distribution(None))
    }

    async fn resolve_workspace_and_pane(
        &self,
        workspace_id: Option<String>,
        pane_id: Option<String>,
    ) -> Result<(String, String), CallToolResult> {
        let context = self
            .resolve_context(workspace_id.clone())
            .await
            .map_err(|message| control_error_result("system.identify", message))?;
        let workspace_id = workspace_id
            .or(context.workspace_id)
            .ok_or_else(|| missing_context_result("workspace_id"))?;
        let pane_id = pane_id
            .or(context.pane_id)
            .ok_or_else(|| missing_context_result("pane_id"))?;
        Ok((workspace_id, pane_id))
    }

    async fn resolve_session_id(
        &self,
        workspace_id: Option<String>,
        session_id: Option<String>,
    ) -> Result<String, CallToolResult> {
        if let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) {
            return Ok(session_id);
        }
        self.resolve_context(workspace_id)
            .await
            .map_err(|message| control_error_result("system.identify", message))?
            .session_id
            .ok_or_else(|| missing_context_result("session_id"))
    }

    async fn resolve_worker_session_id(
        &self,
        workspace_id: Option<String>,
        session_id: Option<String>,
    ) -> Result<String, CallToolResult> {
        let session_id = self
            .resolve_session_id(workspace_id.clone(), session_id)
            .await?;
        let value = self
            .invoke_value("agent.list", json!({ "workspace_id": workspace_id }))
            .await
            .map_err(|message| control_error_result("agent.list", message))?;
        let is_worker = value
            .get("sessions")
            .and_then(Value::as_array)
            .is_some_and(|sessions| {
                sessions.iter().any(|session| {
                    session.get("session_id").and_then(Value::as_str) == Some(session_id.as_str())
                        && is_managed_worker_session(session)
                })
            });
        if !is_worker {
            return Err(control_error_result(
                "agent.worker.resolve",
                format!("session '{session_id}' is not an AgentMux-managed pane worker"),
            ));
        }
        Ok(session_id)
    }

    /// Return the caller's current AgentMux workspace, pane, surface, session, cwd, and backend context.
    #[tool(
        name = "agentmux_context",
        annotations(
            title = "AgentMux context",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn agentmux_context(
        &self,
        Parameters(params): Parameters<ContextParams>,
    ) -> CallToolResult {
        let mut result = self.call("system.identify", json!(params)).await;
        if result.is_error == Some(true) {
            return result;
        }
        if let Some(Value::Object(mut value)) = result.structured_content.take() {
            value.insert("mcp_profile".to_string(), json!(self.profile.as_str()));
            return CallToolResult::structured(Value::Object(value));
        }
        result
    }

    /// List all AgentMux workspaces visible to the authenticated local desktop control plane.
    #[tool(
        name = "workspace_list",
        annotations(
            title = "List workspaces",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspace_list(&self) -> CallToolResult {
        self.call("workspace.list", json!({})).await
    }

    /// Get one workspace including its panes, surfaces, active selection, and persisted layout.
    #[tool(
        name = "workspace_get",
        annotations(
            title = "Get workspace",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspace_get(
        &self,
        Parameters(params): Parameters<WorkspaceGetParams>,
    ) -> CallToolResult {
        self.call("workspace.get", json!(params)).await
    }

    /// List terminal sessions, optionally restricted to a workspace.
    #[tool(
        name = "session_list",
        annotations(
            title = "List sessions",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn session_list(
        &self,
        Parameters(params): Parameters<WorkspaceFilterParams>,
    ) -> CallToolResult {
        self.call("session.list", json!(params)).await
    }

    /// Read bounded recent terminal output for cold-start context or resynchronization.
    #[tool(
        name = "terminal_read",
        annotations(
            title = "Read terminal output",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn terminal_read(
        &self,
        Parameters(params): Parameters<TerminalReadParams>,
    ) -> CallToolResult {
        self.call(
            "session.read_recent",
            json!({
                "session_id": params.session_id,
                "max_bytes": params.max_bytes.unwrap_or(65_536).clamp(1, MAX_TERMINAL_READ_BYTES),
            }),
        )
        .await
    }

    /// List sessions that are waiting for input, permission, or other user attention.
    #[tool(
        name = "agent_attention_list",
        annotations(
            title = "List agent attention",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn agent_attention_list(
        &self,
        Parameters(params): Parameters<WorkspaceFilterParams>,
    ) -> CallToolResult {
        self.call("agent.list_attention", json!(params)).await
    }

    /// List AgentMux-managed agent workers, including tmux-compatible team panes and direct Codex workers.
    #[tool(
        name = "agent_worker_list",
        annotations(
            title = "List agent workers",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn agent_worker_list(
        &self,
        Parameters(params): Parameters<AgentWorkerListToolParams>,
    ) -> CallToolResult {
        let mut value = match self
            .invoke_value("agent.list", json!({ "workspace_id": params.workspace_id }))
            .await
        {
            Ok(value) => value,
            Err(message) => return control_error_result("agent.list", message),
        };
        let integration = params
            .integration
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(sessions) = value.get_mut("sessions").and_then(Value::as_array_mut) {
            sessions.retain(|session| {
                let worker_name = session
                    .pointer("/telemetry/session")
                    .and_then(Value::as_str);
                if !is_managed_worker_session(session) {
                    return false;
                }
                let Some(integration) = integration else {
                    return true;
                };
                worker_name.is_some_and(|name| {
                    name == integration || name.starts_with(&format!("{integration}:"))
                })
            });
        }
        CallToolResult::structured(value)
    }

    /// List adaptive teams reconstructed from persisted agent telemetry.
    #[tool(
        name = "agent_team_list",
        annotations(
            title = "List adaptive agent teams",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn agent_team_list(
        &self,
        Parameters(params): Parameters<AgentTeamListToolParams>,
    ) -> CallToolResult {
        let listed = match self
            .invoke_value("agent.list", json!({ "workspace_id": params.workspace_id }))
            .await
        {
            Ok(value) => value,
            Err(message) => return control_error_result("agent.list", message),
        };
        let mut team_ids = listed
            .get("sessions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|session| {
                session
                    .pointer("/telemetry/team_id")
                    .and_then(Value::as_str)
            })
            .filter(|team_id| {
                params
                    .team_id
                    .as_deref()
                    .is_none_or(|requested| requested == *team_id)
            })
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        team_ids.sort();
        team_ids.dedup();
        let mut teams = Vec::with_capacity(team_ids.len());
        for team_id in team_ids {
            match self.load_agent_team(&team_id).await {
                Ok(team) => teams.push(agent_team_summary(&team, false)),
                Err(message) => {
                    teams.push(json!({ "team_id": team_id, "status": "invalid", "error": message }))
                }
            }
        }
        CallToolResult::structured(json!({ "teams": teams }))
    }

    /// Diagnose Claude Teams, OMX, OMC, or OMO tmux-compatible integration readiness without changing files.
    #[tool(
        name = "agent_integration_status",
        annotations(
            title = "Check agent integration",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn agent_integration_status(
        &self,
        Parameters(params): Parameters<AgentIntegrationStatusToolParams>,
    ) -> CallToolResult {
        let distribution = match self
            .resolve_agent_integration_distribution_for_workspace(
                params.workspace_id.clone(),
                params.distribution.clone(),
            )
            .await
        {
            Ok(distribution) => distribution,
            Err(message) => return control_error_result("workspace.get", message),
        };
        let result = tokio::task::spawn_blocking(move || {
            let kind = params
                .integration
                .as_deref()
                .map(AgentIntegrationKind::parse)
                .transpose()
                .map_err(|error| error.to_string())?;
            let base_dir = resolve_cmuxterm_base_dir(None).map_err(|error| error.to_string())?;
            let bin_dir = base_dir.join("bin");
            Ok::<Value, String>(agent_integration_doctor_result_json(
                &inspect_agent_integrations(&base_dir, &bin_dir, kind, distribution.as_deref()),
            ))
        })
        .await;
        match result {
            Ok(Ok(value)) => CallToolResult::structured(value),
            Ok(Err(message)) => control_error_result("agent.integration.status", message),
            Err(error) => control_error_result(
                "agent.integration.status",
                format!("integration status task failed: {error}"),
            ),
        }
    }

    /// Poll bounded AgentMux events with optional workspace, session, and event-type filters.
    #[tool(
        name = "event_poll",
        annotations(
            title = "Poll events",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn event_poll(
        &self,
        Parameters(mut params): Parameters<EventPollToolParams>,
    ) -> CallToolResult {
        params.max_events = Some(params.max_events.unwrap_or(100).clamp(1, MAX_EVENT_COUNT));
        self.call("events.poll", json!(params)).await
    }

    /// Capture the current DOM snapshot for an AgentMux browser surface.
    #[tool(
        name = "browser_snapshot",
        annotations(
            title = "Snapshot browser DOM",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn browser_snapshot(
        &self,
        Parameters(params): Parameters<BrowserSnapshotParams>,
    ) -> CallToolResult {
        let max_bytes = params
            .max_bytes
            .unwrap_or(DEFAULT_BROWSER_READ_BYTES)
            .clamp(1, MAX_BROWSER_READ_BYTES);
        let result = self
            .call(
                "browser.dom_snapshot",
                json!({
                    "surface_id": params.surface_id,
                    "frame_id": params.frame_id,
                }),
            )
            .await;
        bound_string_result(result, "html", max_bytes)
    }

    /// Read text, HTML, value, or an attribute from an element in an AgentMux browser surface.
    #[tool(
        name = "browser_get",
        annotations(
            title = "Read browser element",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn browser_get(
        &self,
        Parameters(params): Parameters<BrowserGetToolParams>,
    ) -> CallToolResult {
        let max_bytes = params
            .max_bytes
            .unwrap_or(DEFAULT_BROWSER_READ_BYTES)
            .clamp(1, MAX_BROWSER_READ_BYTES);
        let result = self
            .call(
                "browser.get",
                json!({
                    "surface_id": params.surface_id,
                    "selector": params.selector,
                    "kind": params.kind,
                    "attribute": params.attribute,
                    "frame_id": params.frame_id,
                }),
            )
            .await;
        bound_string_result(result, "value", max_bytes)
    }

    /// List shared agent-team tasks, optionally restricted to a workspace.
    #[tool(
        name = "team_task_list",
        annotations(
            title = "List team tasks",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn team_task_list(
        &self,
        Parameters(params): Parameters<WorkspaceFilterParams>,
    ) -> CallToolResult {
        self.call("team.task.list", json!(params)).await
    }

    /// List shared agent-team messages, optionally excluding messages already marked as read.
    #[tool(
        name = "team_message_list",
        annotations(
            title = "List team messages",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn team_message_list(
        &self,
        Parameters(params): Parameters<TeamMessageListToolParams>,
    ) -> CallToolResult {
        self.call("team.message.list", json!(params)).await
    }

    /// Return a privacy-conscious summary of runtime, recovery, queue, and output-stream health.
    #[tool(
        name = "diagnostics_summary",
        annotations(
            title = "Summarize diagnostics",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn diagnostics_summary(&self) -> CallToolResult {
        let diagnostics = match self.invoke_value("diagnostics.export", json!({})).await {
            Ok(value) => value,
            Err(message) => return control_error_result("diagnostics.export", message),
        };
        let audit = match self
            .invoke_value("diagnostics.control_audit", json!({ "limit": 100 }))
            .await
        {
            Ok(value) => value,
            Err(message) => return control_error_result("diagnostics.control_audit", message),
        };
        summarize_diagnostics(diagnostics, audit)
    }

    /// List AgentMux-managed worktree operations and their recovery state.
    #[tool(
        name = "agent_worktree_list",
        annotations(
            title = "List agent worktrees",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn agent_worktree_list(
        &self,
        Parameters(params): Parameters<AgentWorktreeListToolParams>,
    ) -> CallToolResult {
        let request = AgentWorktreeListParams {
            workspace_id: params.workspace_id,
            include_completed: params.include_completed,
        };
        self.call(METHOD_AGENT_WORKTREE_LIST, json!(request)).await
    }

    /// Read a compact Git repository summary including branch, counts, generation, and refresh time.
    #[tool(
        name = "git_status_summary",
        annotations(
            title = "Get Git status summary",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_status_summary(
        &self,
        Parameters(params): Parameters<GitRepositoryToolParams>,
    ) -> CallToolResult {
        if let Err(error) = validate_git_repository(&params) {
            return invalid_ipc_params_result(METHOD_GIT_STATUS_SUMMARY, error);
        }
        self.call(
            METHOD_GIT_STATUS_SUMMARY,
            json!(GitRepositoryParams {
                workspace_id: params.workspace_id,
                pane_id: params.pane_id,
                repository_id: params.repository_id,
            }),
        )
        .await
    }

    /// Page through Git changes using the repository generation and opaque cursor returned by Git status APIs.
    #[tool(
        name = "git_status_page",
        annotations(
            title = "Page Git changes",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_status_page(
        &self,
        Parameters(params): Parameters<GitStatusPageToolParams>,
    ) -> CallToolResult {
        let request = GitStatusPageParams {
            workspace_id: params.workspace_id,
            pane_id: params.pane_id,
            repository_id: params.repository_id,
            state: params.state,
            query: params.query,
            cursor: params.cursor,
            limit: params.limit,
            generation: params.generation,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_GIT_STATUS_PAGE, error);
        }
        self.call(METHOD_GIT_STATUS_PAGE, json!(request)).await
    }

    /// Read one bounded Git diff for a changed repository path.
    #[tool(
        name = "git_diff",
        annotations(
            title = "Read Git diff",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_diff(&self, Parameters(params): Parameters<GitDiffToolParams>) -> CallToolResult {
        let request = GitDiffParams {
            workspace_id: params.workspace_id,
            pane_id: params.pane_id,
            repository_id: params.repository_id,
            path: params.path,
            stage: params.stage,
            context_lines: params.context_lines,
            generation: params.generation,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_GIT_DIFF, error);
        }
        self.call(METHOD_GIT_DIFF, json!(request)).await
    }

    /// List persisted review threads for a repository or path.
    #[tool(
        name = "git_review_thread_list",
        annotations(
            title = "List Git review threads",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_review_thread_list(
        &self,
        Parameters(params): Parameters<GitReviewThreadListToolParams>,
    ) -> CallToolResult {
        let mut request = GitReviewThreadListParams {
            workspace_id: params.workspace_id,
            pane_id: params.pane_id,
            repository_id: params.repository_id,
            path: params.path,
            include_resolved: params.include_resolved,
            include_stale: params.include_stale,
            limit: params.limit,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_GIT_REVIEW_THREAD_LIST, error);
        }
        if self.profile == McpProfile::Standard {
            let scope = match self
                .authorize_standard_git_scope(
                    "git_review_thread_list",
                    &request.workspace_id,
                    request.pane_id.as_deref(),
                    request.repository_id.as_deref(),
                )
                .await
            {
                Ok(scope) => scope,
                Err(result) => return result,
            };
            request.workspace_id = scope.workspace_id;
            request.pane_id = Some(scope.pane_id);
            request.repository_id = Some(scope.repository_id);
        }
        self.call(METHOD_GIT_REVIEW_THREAD_LIST, json!(request))
            .await
    }

    /// List comments belonging to one persisted Git review thread.
    #[tool(
        name = "git_review_comment_list",
        annotations(
            title = "List Git review comments",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn git_review_comment_list(
        &self,
        Parameters(params): Parameters<GitReviewCommentListToolParams>,
    ) -> CallToolResult {
        let request = GitReviewCommentListParams {
            thread_id: params.thread_id,
            limit: params.limit,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_GIT_REVIEW_COMMENT_LIST, error);
        }
        self.call(METHOD_GIT_REVIEW_COMMENT_LIST, json!(request))
            .await
    }

    /// List detected local development servers without opening a browser surface.
    #[tool(
        name = "development_server_candidate_list",
        annotations(
            title = "List development servers",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn development_server_candidate_list(
        &self,
        Parameters(params): Parameters<DevelopmentServerCandidateListToolParams>,
    ) -> CallToolResult {
        let request = DevelopmentServerCandidateListParams {
            workspace_id: params.workspace_id,
            session_id: params.session_id,
            include_dismissed: params.include_dismissed,
            limit: params.limit,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_DEV_SERVER_CANDIDATE_LIST, error);
        }
        self.call(METHOD_DEV_SERVER_CANDIDATE_LIST, json!(request))
            .await
    }

    /// Create an isolated Git worktree, AgentMux workspace surface, terminal, and optional agent command as one host-side saga.
    #[tool(
        name = "agent_worktree_create",
        annotations(
            title = "Create agent worktree",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn agent_worktree_create(
        &self,
        Parameters(params): Parameters<AgentWorktreeCreateToolParams>,
    ) -> CallToolResult {
        let request = AgentWorktreeCreateParams {
            workspace_id: params.workspace_id,
            branch: params.branch,
            destination: params.destination,
            base_revision: params.base_revision,
            create_branch: params.create_branch,
            backend: params.backend,
            backend_profile: params.backend_profile,
            command: params.command,
            cwd: params.cwd,
            idempotency_key: params.idempotency_key,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_AGENT_WORKTREE_CREATE, error);
        }
        self.call(METHOD_AGENT_WORKTREE_CREATE, json!(request))
            .await
    }

    /// Resume an interrupted AgentMux-owned worktree operation by operation or idempotency key.
    #[tool(
        name = "agent_worktree_recover",
        annotations(
            title = "Recover agent worktree",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn agent_worktree_recover(
        &self,
        Parameters(params): Parameters<AgentWorktreeRecoverToolParams>,
    ) -> CallToolResult {
        let request = AgentWorktreeRecoverParams {
            operation_id: params.operation_id,
            idempotency_key: params.idempotency_key,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_AGENT_WORKTREE_RECOVER, error);
        }
        self.call(METHOD_AGENT_WORKTREE_RECOVER, json!(request))
            .await
    }

    /// Add a line-anchored review thread to the local AgentMux review store.
    #[tool(
        name = "git_review_thread_create",
        annotations(
            title = "Create Git review thread",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_review_thread_create(
        &self,
        Parameters(params): Parameters<GitReviewThreadCreateToolParams>,
    ) -> CallToolResult {
        let mut request = GitReviewThreadCreateParams {
            workspace_id: params.workspace_id,
            pane_id: params.pane_id,
            repository_id: params.repository_id,
            anchor: params.anchor.into(),
            body: params.body,
            author_session_id: params.author_session_id,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_GIT_REVIEW_THREAD_CREATE, error);
        }
        if self.profile == McpProfile::Standard {
            let scope = match self
                .authorize_standard_git_scope(
                    "git_review_thread_create",
                    &request.workspace_id,
                    request.pane_id.as_deref(),
                    request.repository_id.as_deref(),
                )
                .await
            {
                Ok(scope) => scope,
                Err(result) => return result,
            };
            if request
                .author_session_id
                .as_deref()
                .is_some_and(|author| author != scope.session_id)
            {
                return standard_scope_denied_result(
                    "git_review_thread_create",
                    "the requested author is not the caller's active agent session",
                );
            }
            request.workspace_id = scope.workspace_id;
            request.pane_id = Some(scope.pane_id);
            request.repository_id = Some(scope.repository_id);
            request.author_session_id = Some(scope.session_id);
        }
        self.call(METHOD_GIT_REVIEW_THREAD_CREATE, json!(request))
            .await
    }

    /// Resolve or re-anchor an existing local Git review thread.
    #[tool(
        name = "git_review_thread_update",
        annotations(
            title = "Update Git review thread",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_review_thread_update(
        &self,
        Parameters(params): Parameters<GitReviewThreadUpdateToolParams>,
    ) -> CallToolResult {
        let request = GitReviewThreadUpdateParams {
            thread_id: params.thread_id,
            resolved: params.resolved,
            anchor: params.anchor.map(Into::into),
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_GIT_REVIEW_THREAD_UPDATE, error);
        }
        if self.profile == McpProfile::Standard {
            if let Err(result) = self
                .authorize_standard_review_thread("git_review_thread_update", &request.thread_id)
                .await
            {
                return result;
            }
        }
        self.call(METHOD_GIT_REVIEW_THREAD_UPDATE, json!(request))
            .await
    }

    /// Mark a review thread stale when its diff anchor no longer matches current Git content.
    #[tool(
        name = "git_review_thread_mark_stale",
        annotations(
            title = "Mark Git review thread stale",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_review_thread_mark_stale(
        &self,
        Parameters(params): Parameters<GitReviewThreadMarkStaleToolParams>,
    ) -> CallToolResult {
        let request = GitReviewThreadMarkStaleParams {
            thread_id: params.thread_id,
            stale: params.stale,
            reason: params.reason,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_GIT_REVIEW_THREAD_MARK_STALE, error);
        }
        if self.profile == McpProfile::Standard {
            if let Err(result) = self
                .authorize_standard_review_thread(
                    "git_review_thread_mark_stale",
                    &request.thread_id,
                )
                .await
            {
                return result;
            }
        }
        self.call(METHOD_GIT_REVIEW_THREAD_MARK_STALE, json!(request))
            .await
    }

    /// Add a comment to an existing local Git review thread.
    #[tool(
        name = "git_review_comment_create",
        annotations(
            title = "Create Git review comment",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_review_comment_create(
        &self,
        Parameters(params): Parameters<GitReviewCommentCreateToolParams>,
    ) -> CallToolResult {
        let mut request = GitReviewCommentCreateParams {
            thread_id: params.thread_id,
            body: params.body,
            author_session_id: params.author_session_id,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_GIT_REVIEW_COMMENT_CREATE, error);
        }
        if self.profile == McpProfile::Standard {
            let scope = match self
                .authorize_standard_review_thread("git_review_comment_create", &request.thread_id)
                .await
            {
                Ok(scope) => scope,
                Err(result) => return result,
            };
            if request
                .author_session_id
                .as_deref()
                .is_some_and(|author| author != scope.session_id)
            {
                return standard_scope_denied_result(
                    "git_review_comment_create",
                    "the requested author is not the caller's active agent session",
                );
            }
            request.author_session_id = Some(scope.session_id);
        }
        self.call(METHOD_GIT_REVIEW_COMMENT_CREATE, json!(request))
            .await
    }

    /// Edit the body of a local Git review comment.
    #[tool(
        name = "git_review_comment_update",
        annotations(
            title = "Update Git review comment",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_review_comment_update(
        &self,
        Parameters(params): Parameters<GitReviewCommentUpdateToolParams>,
    ) -> CallToolResult {
        let request = GitReviewCommentUpdateParams {
            comment_id: params.comment_id,
            body: params.body,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_GIT_REVIEW_COMMENT_UPDATE, error);
        }
        if self.profile == McpProfile::Standard {
            if let Err(result) = self
                .authorize_standard_review_comment("git_review_comment_update", &request.comment_id)
                .await
            {
                return result;
            }
        }
        self.call(METHOD_GIT_REVIEW_COMMENT_UPDATE, json!(request))
            .await
    }

    /// Dismiss a local development-server candidate without stopping the detected server process.
    #[tool(
        name = "development_server_candidate_dismiss",
        annotations(
            title = "Dismiss development server",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn development_server_candidate_dismiss(
        &self,
        Parameters(params): Parameters<DevelopmentServerCandidateDismissToolParams>,
    ) -> CallToolResult {
        let request = DevelopmentServerCandidateDismissParams {
            candidate_id: params.candidate_id,
            reason: params.reason,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_DEV_SERVER_CANDIDATE_DISMISS, error);
        }
        self.call(METHOD_DEV_SERVER_CANDIDATE_DISMISS, json!(request))
            .await
    }

    /// Split a pane and open a detected local development server in the embedded browser.
    #[tool(
        name = "development_server_candidate_open_in_split",
        annotations(
            title = "Open development server in split",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn development_server_candidate_open_in_split(
        &self,
        Parameters(params): Parameters<DevelopmentServerCandidateOpenInSplitToolParams>,
    ) -> CallToolResult {
        let request = DevelopmentServerCandidateOpenInSplitParams {
            candidate_id: params.candidate_id,
            pane_id: params.pane_id,
            axis: params.axis,
            ratio: params.ratio,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_DEV_SERVER_CANDIDATE_OPEN_IN_SPLIT, error);
        }
        self.call(METHOD_DEV_SERVER_CANDIDATE_OPEN_IN_SPLIT, json!(request))
            .await
    }

    /// Remove an AgentMux-owned worktree and the resources created for it. This cannot remove arbitrary directories.
    #[tool(
        name = "agent_worktree_remove",
        annotations(
            title = "Remove agent worktree",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn agent_worktree_remove(
        &self,
        Parameters(params): Parameters<AgentWorktreeRemoveToolParams>,
    ) -> CallToolResult {
        let request = AgentWorktreeRemoveParams {
            worktree_id: params.worktree_id,
            force: params.force,
            idempotency_key: params.idempotency_key,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_AGENT_WORKTREE_REMOVE, error);
        }
        self.call(METHOD_AGENT_WORKTREE_REMOVE, json!(request))
            .await
    }

    /// Stage selected paths in a repository.
    #[tool(
        name = "git_stage",
        annotations(
            title = "Stage Git paths",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_stage(
        &self,
        Parameters(params): Parameters<GitPathMutationToolParams>,
    ) -> CallToolResult {
        self.git_path_mutation(METHOD_GIT_STAGE, params).await
    }

    /// Unstage selected paths without discarding working-tree content.
    #[tool(
        name = "git_unstage",
        annotations(
            title = "Unstage Git paths",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_unstage(
        &self,
        Parameters(params): Parameters<GitPathMutationToolParams>,
    ) -> CallToolResult {
        self.git_path_mutation(METHOD_GIT_UNSTAGE, params).await
    }

    /// Discard selected working-tree paths. This operation is irreversible through AgentMux.
    #[tool(
        name = "git_discard",
        annotations(
            title = "Discard Git paths",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_discard(
        &self,
        Parameters(params): Parameters<GitPathMutationToolParams>,
    ) -> CallToolResult {
        self.git_path_mutation(METHOD_GIT_DISCARD, params).await
    }

    /// Stage every changed path in a repository.
    #[tool(
        name = "git_stage_all",
        annotations(
            title = "Stage all Git changes",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_stage_all(
        &self,
        Parameters(params): Parameters<GitAllMutationToolParams>,
    ) -> CallToolResult {
        self.git_all_mutation(METHOD_GIT_STAGE_ALL, params).await
    }

    /// Unstage every staged path in a repository without discarding working-tree content.
    #[tool(
        name = "git_unstage_all",
        annotations(
            title = "Unstage all Git changes",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_unstage_all(
        &self,
        Parameters(params): Parameters<GitAllMutationToolParams>,
    ) -> CallToolResult {
        self.git_all_mutation(METHOD_GIT_UNSTAGE_ALL, params).await
    }

    /// Create a Git commit from staged changes using the supplied commit message.
    #[tool(
        name = "git_commit",
        annotations(
            title = "Commit Git changes",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_commit(
        &self,
        Parameters(params): Parameters<GitCommitToolParams>,
    ) -> CallToolResult {
        let mut request = GitCommitParams {
            workspace_id: params.workspace_id,
            pane_id: params.pane_id,
            repository_id: params.repository_id,
            message: params.message,
            amend: params.amend,
            idempotency_key: params.idempotency_key,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_GIT_COMMIT, error);
        }
        if self.profile == McpProfile::Standard {
            if request.amend {
                return standard_scope_denied_result(
                    "git_commit",
                    "amending an existing commit is an administrative Git operation",
                );
            }
            let scope = match self
                .authorize_standard_git_scope(
                    "git_commit",
                    &request.workspace_id,
                    request.pane_id.as_deref(),
                    request.repository_id.as_deref(),
                )
                .await
            {
                Ok(scope) => scope,
                Err(result) => return result,
            };
            request.workspace_id = scope.workspace_id;
            request.pane_id = Some(scope.pane_id);
            request.repository_id = Some(scope.repository_id);
        }
        self.call(METHOD_GIT_COMMIT, json!(request)).await
    }

    /// Permanently delete a local Git review thread and its comments.
    #[tool(
        name = "git_review_thread_delete",
        annotations(
            title = "Delete Git review thread",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_review_thread_delete(
        &self,
        Parameters(params): Parameters<GitReviewThreadIdToolParams>,
    ) -> CallToolResult {
        let request = GitReviewThreadIdParams {
            thread_id: params.thread_id,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_GIT_REVIEW_THREAD_DELETE, error);
        }
        self.call(METHOD_GIT_REVIEW_THREAD_DELETE, json!(request))
            .await
    }

    /// Deliver a review thread to a selected AgentMux mailbox or terminal target.
    #[tool(
        name = "git_review_thread_deliver",
        annotations(
            title = "Deliver Git review thread",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn git_review_thread_deliver(
        &self,
        Parameters(params): Parameters<GitReviewThreadDeliverToolParams>,
    ) -> CallToolResult {
        let request = GitReviewThreadDeliverParams {
            thread_id: params.thread_id,
            target: params.target,
            target_session_id: params.target_session_id,
            include_context: params.include_context,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_GIT_REVIEW_THREAD_DELIVER, error);
        }
        if self.profile == McpProfile::Standard {
            if let Err(result) = self
                .authorize_standard_review_delivery("git_review_thread_deliver", &request)
                .await
            {
                return result;
            }
        }
        self.call(METHOD_GIT_REVIEW_THREAD_DELIVER, json!(request))
            .await
    }

    /// Permanently delete a local Git review comment.
    #[tool(
        name = "git_review_comment_delete",
        annotations(
            title = "Delete Git review comment",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn git_review_comment_delete(
        &self,
        Parameters(params): Parameters<GitReviewCommentIdToolParams>,
    ) -> CallToolResult {
        let request = GitReviewCommentIdParams {
            comment_id: params.comment_id,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(METHOD_GIT_REVIEW_COMMENT_DELETE, error);
        }
        if self.profile == McpProfile::Standard {
            if let Err(result) = self
                .authorize_standard_review_comment("git_review_comment_delete", &request.comment_id)
                .await
            {
                return result;
            }
        }
        self.call(METHOD_GIT_REVIEW_COMMENT_DELETE, json!(request))
            .await
    }

    async fn git_path_mutation(
        &self,
        method: &'static str,
        params: GitPathMutationToolParams,
    ) -> CallToolResult {
        let mut request = GitPathMutationParams {
            workspace_id: params.workspace_id,
            pane_id: params.pane_id,
            repository_id: params.repository_id,
            paths: params.paths,
            idempotency_key: params.idempotency_key,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(method, error);
        }
        if self.profile == McpProfile::Standard {
            let tool_name = match method {
                METHOD_GIT_STAGE => "git_stage",
                METHOD_GIT_UNSTAGE => "git_unstage",
                _ => {
                    return standard_scope_denied_result(
                        "git mutation",
                        "this Git operation is not available to the standard profile",
                    )
                }
            };
            let scope = match self
                .authorize_standard_git_scope(
                    tool_name,
                    &request.workspace_id,
                    request.pane_id.as_deref(),
                    request.repository_id.as_deref(),
                )
                .await
            {
                Ok(scope) => scope,
                Err(result) => return result,
            };
            request.workspace_id = scope.workspace_id;
            request.pane_id = Some(scope.pane_id);
            request.repository_id = Some(scope.repository_id);
        }
        self.call(method, json!(request)).await
    }

    async fn git_all_mutation(
        &self,
        method: &'static str,
        params: GitAllMutationToolParams,
    ) -> CallToolResult {
        let request = GitAllMutationParams {
            workspace_id: params.workspace_id,
            pane_id: params.pane_id,
            repository_id: params.repository_id,
            idempotency_key: params.idempotency_key,
        };
        if let Err(error) = request.validate() {
            return invalid_ipc_params_result(method, error);
        }
        self.call(method, json!(request)).await
    }

    /// Focus an AgentMux pane, resolving the current caller context when IDs are omitted.
    #[tool(
        name = "pane_focus",
        annotations(
            title = "Focus pane",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn pane_focus(
        &self,
        Parameters(params): Parameters<PaneFocusToolParams>,
    ) -> CallToolResult {
        let (workspace_id, pane_id) = match self
            .resolve_workspace_and_pane(params.workspace_id, params.pane_id)
            .await
        {
            Ok(context) => context,
            Err(result) => return result,
        };
        self.call(
            "pane.focus",
            json!({ "workspace_id": workspace_id, "pane_id": pane_id }),
        )
        .await
    }

    /// Open a terminal in a new tab or an existing pane using host-side backend defaults.
    #[tool(
        name = "terminal_open",
        annotations(
            title = "Open terminal",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn terminal_open(
        &self,
        Parameters(mut params): Parameters<TerminalOpenToolParams>,
    ) -> CallToolResult {
        let context = match self.resolve_context(params.workspace_id.clone()).await {
            Ok(context) => context,
            Err(message) => return control_error_result("system.identify", message),
        };
        let Some(workspace_id) = params.workspace_id.take().or(context.workspace_id) else {
            return missing_context_result("workspace_id");
        };
        let placement = params.placement.take().unwrap_or_else(|| {
            if params.pane_id.is_some() {
                "active_pane".to_string()
            } else {
                "new_tab".to_string()
            }
        });
        let pane_id = if placement == "active_pane" {
            params.pane_id.take().or(context.pane_id)
        } else {
            params.pane_id.take()
        };
        if placement == "active_pane" && pane_id.is_none() {
            return missing_context_result("pane_id");
        }
        self.call(
            "terminal.open",
            json!({
                "workspace_id": workspace_id,
                "pane_id": pane_id,
                "backend": params.backend,
                "backend_profile": params.backend_profile,
                "command": params.command,
                "cwd": params.cwd,
                "env": [],
                "columns": params.columns,
                "rows": params.rows,
                "durability": params.durability,
                "placement": placement,
            }),
        )
        .await
    }

    /// Atomically split a pane and optionally clone its terminal backend, profile, cwd, and size.
    #[tool(
        name = "terminal_split",
        annotations(
            title = "Split terminal",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn terminal_split(
        &self,
        Parameters(params): Parameters<TerminalSplitToolParams>,
    ) -> CallToolResult {
        let (workspace_id, pane_id) = match self
            .resolve_workspace_and_pane(params.workspace_id, params.pane_id)
            .await
        {
            Ok(context) => context,
            Err(result) => return result,
        };
        self.call(
            "terminal.split",
            json!({
                "workspace_id": workspace_id,
                "pane_id": pane_id,
                "axis": params.axis,
                "ratio": params.ratio,
                "behavior": params.behavior,
                "backend": params.backend,
                "backend_profile": params.backend_profile,
                "command": params.command,
                "cwd": params.cwd,
                "columns": params.columns,
                "rows": params.rows,
                "durability": params.durability,
            }),
        )
        .await
    }

    /// Send literal text to a terminal session without shell interpolation.
    #[tool(
        name = "terminal_send_text",
        annotations(
            title = "Send terminal text",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn terminal_send_text(
        &self,
        Parameters(params): Parameters<TerminalSendTextToolParams>,
    ) -> CallToolResult {
        let session_id = match self
            .resolve_session_id(params.workspace_id, params.session_id)
            .await
        {
            Ok(session_id) => session_id,
            Err(result) => return result,
        };
        self.call(
            "session.send_text",
            json!({ "session_id": session_id, "text": params.text }),
        )
        .await
    }

    /// Send a named key such as enter, escape, tab, up, or ctrl+c to a terminal session.
    #[tool(
        name = "terminal_send_key",
        annotations(
            title = "Send terminal key",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn terminal_send_key(
        &self,
        Parameters(params): Parameters<TerminalSendKeyToolParams>,
    ) -> CallToolResult {
        let session_id = match self
            .resolve_session_id(params.workspace_id, params.session_id)
            .await
        {
            Ok(session_id) => session_id,
            Err(result) => return result,
        };
        self.call(
            "session.send_key",
            json!({ "session_id": session_id, "key": params.key }),
        )
        .await
    }

    /// Open an embedded browser in a new tab or an existing pane and optionally navigate it.
    #[tool(
        name = "browser_open",
        annotations(
            title = "Open browser",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn browser_open(
        &self,
        Parameters(params): Parameters<BrowserOpenToolParams>,
    ) -> CallToolResult {
        let context = match self.resolve_context(params.workspace_id.clone()).await {
            Ok(context) => context,
            Err(message) => return control_error_result("system.identify", message),
        };
        let Some(workspace_id) = params.workspace_id.or(context.workspace_id) else {
            return missing_context_result("workspace_id");
        };
        let placement = params.placement.unwrap_or_else(|| {
            if params.pane_id.is_some() {
                "active_pane".to_string()
            } else {
                "new_tab".to_string()
            }
        });
        let pane_id = params.pane_id.or_else(|| {
            (placement == "active_pane")
                .then_some(context.pane_id)
                .flatten()
        });
        let surface = match self
            .invoke_value(
                "surface.create_browser",
                json!({
                    "workspace_id": workspace_id,
                    "pane_id": pane_id,
                    "profile": params.profile.unwrap_or_else(|| "default".to_string()),
                    "placement": placement,
                }),
            )
            .await
        {
            Ok(surface) => surface,
            Err(message) => return control_error_result("surface.create_browser", message),
        };
        let Some(surface_id) = surface.get("surface_id").and_then(Value::as_str) else {
            return control_error_result(
                "surface.create_browser",
                "control plane returned no surface_id".to_string(),
            );
        };
        if let Some(url) = params.url {
            match self
                .invoke_value(
                    "browser.navigate",
                    json!({ "surface_id": surface_id, "url": url }),
                )
                .await
            {
                Ok(navigation) => {
                    return CallToolResult::structured(json!({
                        "surface": surface,
                        "navigation": navigation,
                    }))
                }
                Err(message) => return control_error_result("browser.navigate", message),
            }
        }
        CallToolResult::structured(surface)
    }

    /// Split the current pane, mount an embedded browser in the new pane, and roll back on failure.
    #[tool(
        name = "browser_open_split",
        annotations(
            title = "Open browser split",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn browser_open_split(
        &self,
        Parameters(params): Parameters<BrowserOpenSplitToolParams>,
    ) -> CallToolResult {
        let (workspace_id, source_pane_id) = match self
            .resolve_workspace_and_pane(params.workspace_id, params.pane_id)
            .await
        {
            Ok(context) => context,
            Err(result) => return result,
        };
        let detail = match self
            .invoke_value(
                "pane.split",
                json!({
                    "workspace_id": workspace_id,
                    "pane_id": source_pane_id,
                    "axis": params.axis,
                    "ratio": params.ratio,
                }),
            )
            .await
        {
            Ok(detail) => detail,
            Err(message) => return control_error_result("pane.split", message),
        };
        let Some(target_pane_id) = empty_split_target(&detail, &source_pane_id) else {
            return control_error_result(
                "pane.split",
                "control plane returned no empty split target".to_string(),
            );
        };
        let surface = self
            .invoke_value(
                "surface.create_browser",
                json!({
                    "workspace_id": workspace_id,
                    "pane_id": target_pane_id,
                    "profile": params.profile.unwrap_or_else(|| "default".to_string()),
                    "placement": "active_pane",
                }),
            )
            .await;
        let surface = match surface {
            Ok(surface) => surface,
            Err(message) => {
                let _ = self
                    .invoke_value(
                        "pane.close",
                        json!({
                            "workspace_id": workspace_id,
                            "pane_id": target_pane_id,
                            "surface_policy": "detach_surface",
                        }),
                    )
                    .await;
                return control_error_result("surface.create_browser", message);
            }
        };
        let Some(surface_id) = surface.get("surface_id").and_then(Value::as_str) else {
            return control_error_result(
                "surface.create_browser",
                "control plane returned no surface_id".to_string(),
            );
        };
        if let Some(url) = params.url {
            if let Err(message) = self
                .invoke_value(
                    "browser.navigate",
                    json!({ "surface_id": surface_id, "url": url }),
                )
                .await
            {
                let _ = self
                    .invoke_value(
                        "pane.close",
                        json!({
                            "workspace_id": workspace_id,
                            "pane_id": target_pane_id,
                            "surface_policy": "close_surface",
                        }),
                    )
                    .await;
                return control_error_result("browser.navigate", message);
            }
        }
        CallToolResult::structured(json!({
            "workspace_id": workspace_id,
            "source_pane_id": source_pane_id,
            "pane_id": target_pane_id,
            "surface": surface,
        }))
    }

    /// Navigate an embedded browser surface.
    #[tool(
        name = "browser_navigate",
        annotations(
            title = "Navigate browser",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn browser_navigate(
        &self,
        Parameters(params): Parameters<BrowserNavigateToolParams>,
    ) -> CallToolResult {
        self.call("browser.navigate", json!(params)).await
    }

    /// Click an element or coordinates in an embedded browser surface.
    #[tool(
        name = "browser_click",
        annotations(
            title = "Click browser",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn browser_click(
        &self,
        Parameters(params): Parameters<BrowserClickToolParams>,
    ) -> CallToolResult {
        self.call("browser.click", json!(params)).await
    }

    /// Replace the value of an input element in an embedded browser surface.
    #[tool(
        name = "browser_fill",
        annotations(
            title = "Fill browser input",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn browser_fill(
        &self,
        Parameters(params): Parameters<BrowserFillToolParams>,
    ) -> CallToolResult {
        self.call("browser.fill", json!(params)).await
    }

    /// Send a message through the shared Agent Team mailbox.
    #[tool(
        name = "team_message_send",
        annotations(
            title = "Send team message",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn team_message_send(
        &self,
        Parameters(mut params): Parameters<TeamMessageSendToolParams>,
    ) -> CallToolResult {
        let context = match self.resolve_context(params.workspace_id.clone()).await {
            Ok(context) => context,
            Err(message) => return control_error_result("system.identify", message),
        };
        let Some(workspace_id) = params.workspace_id.take().or(context.workspace_id) else {
            return missing_context_result("workspace_id");
        };
        self.call(
            "team.message.send",
            json!({
                "workspace_id": workspace_id,
                "thread_id": params.thread_id,
                "from_session_id": params.from_session_id.or(context.session_id),
                "to_session_id": params.to_session_id,
                "body": params.body,
                "kind": params.kind,
            }),
        )
        .await
    }

    /// Claim a shared team task for an explicit or current session.
    #[tool(
        name = "team_task_claim",
        annotations(
            title = "Claim team task",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn team_task_claim(
        &self,
        Parameters(params): Parameters<TeamTaskClaimToolParams>,
    ) -> CallToolResult {
        let session_id = match self
            .resolve_session_id(params.workspace_id, params.session_id)
            .await
        {
            Ok(session_id) => session_id,
            Err(result) => return result,
        };
        self.call(
            "team.task.claim",
            json!({ "task_id": params.task_id, "session_id": session_id }),
        )
        .await
    }

    /// Complete a shared team task and unblock its dependents.
    #[tool(
        name = "team_task_complete",
        annotations(
            title = "Complete team task",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn team_task_complete(
        &self,
        Parameters(params): Parameters<TeamTaskCompleteToolParams>,
    ) -> CallToolResult {
        self.call("team.task.complete", json!({ "task_id": params.task_id }))
            .await
    }

    /// Publish an agent lifecycle state for an explicit or current session.
    #[tool(
        name = "agent_set_state",
        annotations(
            title = "Set agent state",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn agent_set_state(
        &self,
        Parameters(params): Parameters<AgentSetStateToolParams>,
    ) -> CallToolResult {
        let session_id = match self
            .resolve_session_id(params.workspace_id, params.session_id)
            .await
        {
            Ok(session_id) => session_id,
            Err(result) => return result,
        };
        self.call(
            "agent.set_state",
            json!({
                "session_id": session_id,
                "state": params.state,
                "reason": params.reason,
                "telemetry": params.telemetry,
            }),
        )
        .await
    }

    async fn register_agent_worker_state(
        &self,
        kind: AgentWorkerKind,
        worker_name: Option<&str>,
        session_id: &str,
        pane_id: Option<&str>,
        team: Option<&AgentTeamWorkerRegistration>,
    ) -> Result<(), String> {
        let label = kind.label();
        let worker_session = worker_name.map_or_else(
            || format!("{}:worker", kind.key()),
            |name| format!("{}:{name}", kind.key()),
        );
        let reason = worker_name.map_or_else(
            || format!("{label} worker started"),
            |name| format!("{label} worker '{name}' started"),
        );
        let mut telemetry = json!({
            "activity": kind.activity(),
            "session": worker_session,
            "ctx": pane_id,
        });
        if let (Some(team), Some(telemetry)) = (team, telemetry.as_object_mut()) {
            telemetry.insert("team_id".to_string(), json!(team.team_id));
            telemetry.insert("team_role".to_string(), json!("worker"));
            telemetry.insert("worker_name".to_string(), json!(team.worker_name));
            telemetry.insert("parent_session_id".to_string(), json!(team.main_session_id));
            telemetry.insert(
                "layout_root_pane_id".to_string(),
                json!(team.layout_root_pane_id),
            );
            telemetry.insert("main_ratio".to_string(), json!(team.main_ratio.to_string()));
            telemetry.insert("max_workers".to_string(), json!(team.max_workers as u16));
            telemetry.insert("worker_index".to_string(), json!(team.worker_index as u16));
            telemetry.insert("team_generation".to_string(), json!(team.generation));
            telemetry.insert("team_auto_adopt".to_string(), json!(team.auto_adopt_tmux));
            telemetry.insert(
                "team_member_idempotency_key".to_string(),
                json!(team.member_idempotency_key),
            );
        }
        self.invoke_value(
            "agent.set_state",
            json!({
                "session_id": session_id,
                "state": "running",
                "reason": reason,
                "telemetry": telemetry,
            }),
        )
        .await
        .map(|_| ())
    }

    async fn start_agent_worker_resolved(
        &self,
        params: AgentWorkerStartToolParams,
        context: ResolvedContext,
        register_agent_state: bool,
        team: Option<&AgentTeamWorkerRegistration>,
    ) -> CallToolResult {
        let Some(workspace_id) = params.workspace_id.or(context.workspace_id) else {
            return missing_context_result("workspace_id");
        };
        let placement = params
            .placement
            .as_deref()
            .unwrap_or("split")
            .to_ascii_lowercase();
        if !matches!(placement.as_str(), "split" | "new_tab") {
            return control_error_result(
                "agent.worker.start",
                "placement must be 'split' or 'new_tab'".to_string(),
            );
        }
        let worker_name = match params.name.as_deref() {
            Some(name) if name.trim().is_empty() => {
                return control_error_result(
                    "agent.worker.start",
                    "worker name must not be empty".to_string(),
                );
            }
            Some(name) => Some(name.trim().to_string()),
            None => None,
        };
        let kind = AgentWorkerKind::from(params.kind);
        let command = match build_agent_worker_command(kind, params.args) {
            Ok(command) => command,
            Err(message) => return control_error_result("agent.worker.start", message),
        };
        let cwd = params.cwd.or(context.cwd);
        let team_env = team.map(agent_team_launch_env).unwrap_or_default();
        let launch = if placement == "new_tab" {
            self.invoke_value(
                "terminal.open",
                json!({
                    "workspace_id": workspace_id.clone(),
                    "pane_id": Value::Null,
                    "backend": "wsl-direct",
                    "backend_profile": params.distribution,
                    "command": command,
                    "cwd": cwd,
                    "env": team_env,
                    "columns": params.columns,
                    "rows": params.rows,
                    "durability": params.durability.unwrap_or_else(|| "durable".to_string()),
                    "placement": "new_tab",
                }),
            )
            .await
        } else {
            let Some(pane_id) = params.pane_id.or(context.pane_id) else {
                return missing_context_result("pane_id");
            };
            self.invoke_value(
                "terminal.split",
                json!({
                    "workspace_id": workspace_id.clone(),
                    "pane_id": pane_id,
                    "axis": params.axis.unwrap_or_else(|| "vertical".to_string()),
                    "ratio": params.ratio,
                    "behavior": "clone_current",
                    "backend": "wsl-direct",
                    "backend_profile": params.distribution,
                    "command": command,
                    "cwd": cwd,
                    "env": team_env,
                    "columns": params.columns,
                    "rows": params.rows,
                    "durability": params.durability.unwrap_or_else(|| "durable".to_string()),
                }),
            )
            .await
        };
        let launch = match launch {
            Ok(value) => value,
            Err(message) => return control_error_result("agent.worker.start", message),
        };
        let pane_id = launch
            .get("pane_id")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let Some(session_id) = launch
            .get("session_id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
        else {
            let pane_closed = match pane_id.as_deref() {
                Some(pane_id) => self
                    .invoke_value(
                        "pane.close",
                        json!({
                            "workspace_id": workspace_id,
                            "pane_id": pane_id,
                            "surface_policy": "close_surface",
                        }),
                    )
                    .await
                    .is_ok(),
                None => false,
            };
            return control_error_result(
                "agent.worker.start",
                format!(
                    "terminal launch returned no session_id; compensation: pane_closed={pane_closed}"
                ),
            );
        };
        if register_agent_state {
            if let Err(message) = self
                .register_agent_worker_state(
                    kind,
                    worker_name.as_deref(),
                    &session_id,
                    pane_id.as_deref(),
                    team,
                )
                .await
            {
                let terminate = self
                    .invoke_value(
                        "session.terminate",
                        json!({ "session_id": session_id, "mode": "kill" }),
                    )
                    .await;
                let close_pane = match pane_id.as_deref() {
                    Some(pane_id) => {
                        self.invoke_value(
                            "pane.close",
                            json!({
                                "workspace_id": workspace_id,
                                "pane_id": pane_id,
                                "surface_policy": "close_surface",
                            }),
                        )
                        .await
                    }
                    None => Ok(json!({ "skipped": true })),
                };
                return control_error_result(
                    "agent.worker.start",
                    format!(
                        "worker metadata registration failed: {message}; compensation: session_terminated={}, pane_closed={}",
                        terminate.is_ok(),
                        close_pane.is_ok()
                    ),
                );
            }
        }
        CallToolResult::structured(json!({
            "name": worker_name,
            "kind": kind.key(),
            "controller": kind.controller(),
            "workspace_id": workspace_id,
            "session_id": session_id,
            "pane_id": launch.get("pane_id"),
            "surface_id": launch.get("surface_id"),
            "placement": placement,
            "agent_state_registered": register_agent_state,
        }))
    }

    /// Start a Claude Teams lead, tmux-compatible integration, or independent Codex worker in a new AgentMux pane.
    #[tool(
        name = "agent_worker_start",
        annotations(
            title = "Start agent worker",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn agent_worker_start(
        &self,
        Parameters(params): Parameters<AgentWorkerStartToolParams>,
    ) -> CallToolResult {
        let context = match self.resolve_context(params.workspace_id.clone()).await {
            Ok(context) => context,
            Err(message) => return control_error_result("system.identify", message),
        };
        self.start_agent_worker_resolved(params, context, true, None)
            .await
    }

    async fn load_agent_team(&self, team_id: &str) -> Result<LoadedAgentTeam, String> {
        let value = self
            .invoke_value("agent.list", json!({ "workspace_id": Value::Null }))
            .await?;
        let sessions = value
            .get("sessions")
            .and_then(Value::as_array)
            .ok_or_else(|| "agent.list returned no sessions array".to_string())?;
        let members = sessions
            .iter()
            .filter(|session| {
                session
                    .pointer("/telemetry/team_id")
                    .and_then(Value::as_str)
                    == Some(team_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        let main = members
            .iter()
            .find(|session| {
                session
                    .pointer("/telemetry/team_role")
                    .and_then(Value::as_str)
                    == Some("main")
            })
            .cloned()
            .ok_or_else(|| format!("agent team '{team_id}' was not found"))?;
        let telemetry = main
            .get("telemetry")
            .and_then(Value::as_object)
            .ok_or_else(|| format!("agent team '{team_id}' has no manifest telemetry"))?;
        let workspace_id = main
            .get("workspace_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("agent team '{team_id}' has no workspace_id"))?
            .to_string();
        let main_session_id = main
            .get("session_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("agent team '{team_id}' has no main session"))?
            .to_string();
        let layout_root_pane_id = telemetry
            .get("layout_root_pane_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("agent team '{team_id}' has no layout root"))?
            .to_string();
        let main_ratio = telemetry_f64(telemetry.get("main_ratio")).unwrap_or(0.5);
        let max_workers = telemetry
            .get("max_workers")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(MAX_AGENT_TEAM_WORKERS)
            .clamp(1, MAX_AGENT_TEAM_WORKERS);
        Ok(LoadedAgentTeam {
            team_id: team_id.to_string(),
            workspace_id,
            main_session_id,
            layout_root_pane_id,
            main_ratio,
            max_workers,
            generation: telemetry
                .get("team_generation")
                .and_then(Value::as_u64)
                .unwrap_or(1),
            status: telemetry
                .get("team_status")
                .and_then(Value::as_str)
                .unwrap_or("active")
                .to_string(),
            mutation_id: telemetry
                .get("team_mutation_id")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            auto_adopt_tmux: telemetry
                .get("team_auto_adopt")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            idempotency_key: telemetry
                .get("team_idempotency_key")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            mode: telemetry
                .get("team_mode")
                .and_then(Value::as_str)
                .unwrap_or("adaptive")
                .to_string(),
            default_worker_kind: telemetry
                .get("default_worker_kind")
                .and_then(Value::as_str)
                .unwrap_or("codex-pane")
                .to_string(),
            distribution: telemetry
                .get("distribution")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            cwd: telemetry
                .get("team_cwd")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            durability: telemetry
                .get("durability")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            main,
            members,
        })
    }

    async fn find_agent_team_by_idempotency(
        &self,
        workspace_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<LoadedAgentTeam>, String> {
        let listed = self
            .invoke_value("agent.list", json!({ "workspace_id": workspace_id }))
            .await?;
        let team_id = listed
            .get("sessions")
            .and_then(Value::as_array)
            .and_then(|sessions| {
                sessions.iter().find_map(|session| {
                    let telemetry = session.get("telemetry")?;
                    (telemetry.get("team_role").and_then(Value::as_str) == Some("main")
                        && telemetry
                            .get("team_idempotency_key")
                            .and_then(Value::as_str)
                            == Some(idempotency_key))
                    .then(|| {
                        telemetry
                            .get("team_id")
                            .and_then(Value::as_str)
                            .map(ToString::to_string)
                    })
                    .flatten()
                })
            });
        match team_id {
            Some(team_id) => self.load_agent_team(&team_id).await.map(Some),
            None => Ok(None),
        }
    }

    async fn find_agent_team_for_main_session(
        &self,
        workspace_id: &str,
        main_session_id: &str,
    ) -> Result<Option<LoadedAgentTeam>, String> {
        let listed = self
            .invoke_value("agent.list", json!({ "workspace_id": workspace_id }))
            .await?;
        let owner = listed
            .get("sessions")
            .and_then(Value::as_array)
            .and_then(|sessions| {
                sessions.iter().find_map(|session| {
                    (session.get("session_id").and_then(Value::as_str) == Some(main_session_id))
                        .then(|| session.get("telemetry"))
                        .flatten()
                        .and_then(|telemetry| telemetry.get("team_id"))
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
            });
        match owner {
            Some(team_id) => self.load_agent_team(&team_id).await.map(Some),
            None => Ok(None),
        }
    }

    async fn agent_team_main_state_payload(
        &self,
        team: &LoadedAgentTeam,
        team_status: &str,
    ) -> Result<Value, String> {
        let listed = self
            .invoke_value("agent.list", json!({ "workspace_id": team.workspace_id }))
            .await?;
        let current = listed
            .get("sessions")
            .and_then(Value::as_array)
            .and_then(|sessions| {
                sessions.iter().find(|session| {
                    session.get("session_id").and_then(Value::as_str)
                        == Some(team.main_session_id.as_str())
                })
            });
        if let Some(existing_team_id) = current
            .and_then(|session| session.pointer("/telemetry/team_id"))
            .and_then(Value::as_str)
            .filter(|team_id| *team_id != team.team_id)
        {
            return Err(format!(
                "main session '{}' is already owned by team '{existing_team_id}'",
                team.main_session_id
            ));
        }
        let state = current
            .and_then(|session| session.get("state"))
            .and_then(Value::as_str)
            .unwrap_or("running");
        let reason = current
            .and_then(|session| session.get("reason"))
            .cloned()
            .unwrap_or_else(|| json!("Adaptive agent team active"));
        let mut telemetry = current
            .and_then(|session| session.get("telemetry"))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        telemetry
            .entry("activity".to_string())
            .or_insert_with(|| json!("agent"));
        telemetry.insert("team_id".to_string(), json!(team.team_id));
        telemetry.insert("team_role".to_string(), json!("main"));
        telemetry.insert("team_mode".to_string(), json!(team.mode));
        telemetry.insert("team_status".to_string(), json!(team_status));
        telemetry.insert(
            "team_layout".to_string(),
            json!(AgentTeamLayoutParam::MainLeftWorkersRight.key()),
        );
        telemetry.insert(
            "layout_root_pane_id".to_string(),
            json!(team.layout_root_pane_id),
        );
        telemetry.insert("main_ratio".to_string(), json!(team.main_ratio.to_string()));
        telemetry.insert("max_workers".to_string(), json!(team.max_workers as u16));
        telemetry.insert("team_generation".to_string(), json!(team.generation));
        telemetry.remove("team_mutation_id");
        telemetry.remove("team_mutation_owner_id");
        telemetry.insert("team_auto_adopt".to_string(), json!(team.auto_adopt_tmux));
        telemetry.insert(
            "default_worker_kind".to_string(),
            json!(team.default_worker_kind),
        );
        telemetry.insert("distribution".to_string(), json!(team.distribution));
        telemetry.insert("team_cwd".to_string(), json!(team.cwd));
        telemetry.insert("durability".to_string(), json!(team.durability));
        telemetry.insert(
            "team_idempotency_key".to_string(),
            json!(team.idempotency_key),
        );
        Ok(json!({
            "session_id": team.main_session_id,
            "state": state,
            "reason": reason,
            "telemetry": telemetry,
        }))
    }

    async fn claim_agent_team_provisioning(
        &self,
        team: &mut LoadedAgentTeam,
        mutation_id: &str,
    ) -> Result<AgentTeamReservation, String> {
        // The host owns this compare-and-claim operation. Do not write a provisional
        // manifest first: that creates a race where a second starter can overwrite it.
        let generation = team.generation.saturating_add(1);
        let result = self
            .invoke_value(
                "agent.team.reserve",
                json!({
                        "team_id": team.team_id,
                        "main_session_id": team.main_session_id,
                        "expected_generation": team.generation,
                    "next_generation": generation,
                    "mutation_id": mutation_id,
                    "claim": true,
                    "claim_telemetry": {
                        "activity": "agent",
                        "team_id": team.team_id,
                        "team_role": "main",
                        "team_mode": team.mode,
                        "team_status": "provisioning",
                        "team_layout": AgentTeamLayoutParam::MainLeftWorkersRight.key(),
                        "layout_root_pane_id": team.layout_root_pane_id,
                        "main_ratio": team.main_ratio.to_string(),
                        "max_workers": team.max_workers as u16,
                        "team_generation": generation,
                        "team_auto_adopt": team.auto_adopt_tmux,
                        "team_idempotency_key": team.idempotency_key,
                        "default_worker_kind": team.default_worker_kind,
                        "distribution": team.distribution,
                        "team_cwd": team.cwd,
                        "durability": team.durability,
                    },
                }),
            )
            .await?;
        team.team_id = result
            .get("team_id")
            .and_then(Value::as_str)
            .unwrap_or(team.team_id.as_str())
            .to_string();
        team.generation = result
            .get("generation")
            .and_then(Value::as_u64)
            .unwrap_or(generation);
        team.status = "provisioning".to_string();
        team.mutation_id = Some(mutation_id.to_string());
        Ok(AgentTeamReservation::from_control_result(
            &result,
            team.generation,
            mutation_id,
        ))
    }

    async fn reserve_agent_team_generation(
        &self,
        team: &LoadedAgentTeam,
        mutation_id: &str,
    ) -> Result<AgentTeamReservation, String> {
        let next_generation = team.generation.saturating_add(1);
        let result = self
            .invoke_value(
                "agent.team.reserve",
                json!({
                    "team_id": team.team_id,
                    "main_session_id": team.main_session_id,
                    "expected_generation": team.generation,
                    "next_generation": next_generation,
                    "mutation_id": mutation_id,
                }),
            )
            .await?;
        Ok(AgentTeamReservation::from_control_result(
            &result,
            next_generation,
            mutation_id,
        ))
    }

    async fn settle_agent_team_mutation(
        &self,
        team: &LoadedAgentTeam,
        mutation_id: &str,
        status: &str,
    ) -> Result<(), String> {
        let payload = self.agent_team_main_state_payload(team, status).await?;
        let telemetry = payload
            .get("telemetry")
            .cloned()
            .ok_or_else(|| "team settlement payload has no telemetry".to_string())?;
        self.invoke_value(
            "agent.team.settle",
            json!({
                "team_id": team.team_id,
                "main_session_id": team.main_session_id,
                "generation": team.generation,
                "mutation_id": mutation_id,
                "telemetry": telemetry,
            }),
        )
        .await
        .map(|_| ())
    }

    async fn settle_agent_team_after_failed_mutation(
        &self,
        team: &LoadedAgentTeam,
        mutation_id: &str,
    ) {
        let _ = self
            .settle_agent_team_mutation(team, mutation_id, "layout_dirty")
            .await;
    }

    async fn recover_agent_team_if_abandoned(
        &self,
        team: &LoadedAgentTeam,
    ) -> Result<bool, String> {
        if team.status != "provisioning" {
            return Ok(false);
        }
        let mutation_id = team
            .mutation_id
            .as_deref()
            .ok_or_else(|| "provisioning team has no mutation id".to_string())?;
        self.invoke_value(
            "agent.team.recover",
            json!({
                "team_id": team.team_id,
                "main_session_id": team.main_session_id,
                "generation": team.generation,
                "mutation_id": mutation_id,
            }),
        )
        .await
        .map(|_| true)
    }

    async fn clear_agent_team_membership(&self, member: &Value) -> Result<(), String> {
        let session_id = member
            .get("session_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "team member has no session_id".to_string())?;
        let state = member
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("running");
        let reason = member.get("reason").cloned().unwrap_or(Value::Null);
        let mut telemetry = member
            .get("telemetry")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for key in AGENT_TEAM_TELEMETRY_KEYS {
            telemetry.remove(*key);
        }
        self.invoke_value(
            "agent.set_state",
            json!({
                "session_id": session_id,
                "state": state,
                "reason": reason,
                "telemetry": telemetry,
            }),
        )
        .await
        .map(|_| ())
    }

    async fn reflow_agent_team(
        &self,
        team: &mut LoadedAgentTeam,
        dry_run: bool,
    ) -> Result<Value, String> {
        let current = self.load_agent_team(&team.team_id).await?;
        if current.generation != team.generation {
            return Ok(agent_team_layout_conflict(
                &team.team_id,
                format!(
                    "team generation changed during reflow: expected {}, current {}",
                    team.generation, current.generation
                ),
            ));
        }
        let topology = self
            .invoke_value(
                "workspace.get",
                json!({ "workspace_id": team.workspace_id }),
            )
            .await?;
        let Some((_, main_pane_id)) = worker_location_for_session(&topology, &team.main_session_id)
        else {
            return Err(format!(
                "could not resolve main session '{}' to a pane",
                team.main_session_id
            ));
        };
        let panes = agent_team_topology_panes(&topology);
        let root_id = team.layout_root_pane_id.clone();
        let Some(root) = panes.iter().find(|pane| pane.pane_id == root_id) else {
            return Ok(agent_team_layout_conflict(
                &team.team_id,
                format!("team layout root '{root_id}' no longer exists"),
            ));
        };
        if root.kind == "leaf" {
            let managed_members = agent_team_managed_members(team);
            if !managed_members.is_empty() {
                return Ok(agent_team_layout_conflict(
                    &team.team_id,
                    format!(
                        "layout root '{root_id}' collapsed while {} managed members remain",
                        managed_members.len()
                    ),
                ));
            }
            return Ok(json!({
                "team_id": team.team_id,
                "status": "empty",
                "layout_dirty": false,
                "mutated": false,
                "updates": [],
            }));
        }
        if root.split_axis.as_deref() != Some("vertical") {
            return Ok(agent_team_layout_conflict(
                &team.team_id,
                format!("layout root '{root_id}' is not a vertical split"),
            ));
        }
        let children = agent_team_children(&panes, &root_id);
        if children.len() != 2 {
            return Ok(agent_team_layout_conflict(
                &team.team_id,
                format!("layout root '{root_id}' does not have exactly two children"),
            ));
        }
        let main_child_index = children
            .iter()
            .position(|child| agent_team_subtree_contains(&panes, &child.pane_id, &main_pane_id));
        let Some(main_child_index) = main_child_index else {
            return Ok(agent_team_layout_conflict(
                &team.team_id,
                "main pane is outside the managed layout root".to_string(),
            ));
        };
        let worker_root = &children[1 - main_child_index];
        if agent_team_split_nodes(&panes, &worker_root.pane_id)
            .iter()
            .any(|pane| pane.split_axis.as_deref() != Some("horizontal"))
        {
            return Ok(agent_team_layout_conflict(
                &team.team_id,
                "managed worker subtree contains a non-horizontal split".to_string(),
            ));
        }
        let worker_leaf_ids = agent_team_leaf_ids(&panes, &worker_root.pane_id);
        let managed_members = agent_team_managed_members(team);
        let unresolved_members = managed_members
            .iter()
            .filter_map(|member| {
                let name = member
                    .get("telemetry")
                    .and_then(|telemetry| telemetry.get("worker_name"))
                    .and_then(Value::as_str)
                    .unwrap_or("unnamed worker");
                match member.get("session_id").and_then(Value::as_str) {
                    Some(session_id)
                        if worker_location_for_session(&topology, session_id).is_none() =>
                    {
                        Some(session_id.to_string())
                    }
                    None => Some(format!("{name} (missing session_id)")),
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        if !unresolved_members.is_empty() {
            return Ok(agent_team_layout_conflict(
                &team.team_id,
                format!(
                    "managed sessions could not be resolved to panes: {}",
                    unresolved_members.join(", ")
                ),
            ));
        }
        let managed_leaf_ids = managed_members
            .iter()
            .filter_map(|member| member.get("session_id").and_then(Value::as_str))
            .filter_map(|session_id| worker_location_for_session(&topology, session_id))
            .map(|(_, pane_id)| pane_id)
            .collect::<std::collections::HashSet<_>>();
        let foreign = worker_leaf_ids
            .iter()
            .filter(|pane_id| !managed_leaf_ids.contains(*pane_id))
            .cloned()
            .collect::<Vec<_>>();
        if !foreign.is_empty() {
            return Ok(agent_team_layout_conflict(
                &team.team_id,
                format!(
                    "foreign panes found under managed worker subtree: {}",
                    foreign.join(", ")
                ),
            ));
        }
        let missing = managed_leaf_ids
            .iter()
            .filter(|pane_id| !worker_leaf_ids.contains(*pane_id))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Ok(agent_team_layout_conflict(
                &team.team_id,
                format!(
                    "team worker panes are outside the managed subtree: {}",
                    missing.join(", ")
                ),
            ));
        }
        let root_ratio = if main_child_index == 0 {
            team.main_ratio
        } else {
            1.0 - team.main_ratio
        };
        let mut updates = vec![(root_id, root_ratio, root.split_ratio.unwrap_or(0.5))];
        agent_team_collect_reflow_updates(&panes, &worker_root.pane_id, &mut updates);
        // Re-read immediately before applying layout updates. A successful reservation from a
        // competing mutation makes this topology stale even if the first snapshot was valid.
        let latest = self.load_agent_team(&team.team_id).await?;
        if latest.generation != team.generation {
            return Ok(agent_team_layout_conflict(
                &team.team_id,
                format!(
                    "team generation changed before layout apply: expected {}, current {}",
                    team.generation, latest.generation
                ),
            ));
        }
        if !dry_run {
            let mut applied = Vec::with_capacity(updates.len());
            for (pane_id, ratio, previous_ratio) in &updates {
                if let Err(message) = self
                    .invoke_value(
                        "pane.resize_layout",
                        json!({
                            "workspace_id": team.workspace_id,
                            "pane_id": pane_id,
                            "ratio": ratio,
                        }),
                    )
                    .await
                {
                    let mut restored = true;
                    for (applied_pane_id, applied_previous_ratio) in applied.iter().rev() {
                        restored &= self
                            .invoke_value(
                                "pane.resize_layout",
                                json!({
                                    "workspace_id": team.workspace_id,
                                    "pane_id": applied_pane_id,
                                    "ratio": applied_previous_ratio,
                                }),
                            )
                            .await
                            .is_ok();
                    }
                    return Err(format!(
                        "layout resize failed at pane '{pane_id}': {message}; previous ratios restored={restored}"
                    ));
                }
                applied.push((pane_id.clone(), *previous_ratio));
            }
        }
        Ok(json!({
            "team_id": team.team_id,
            "status": "ok",
            "layout_dirty": false,
            "mutated": !dry_run,
            "updates": updates
                .into_iter()
                .map(|(pane_id, ratio, previous_ratio)| json!({
                    "pane_id": pane_id,
                    "ratio": ratio,
                    "previous_ratio": previous_ratio,
                }))
                .collect::<Vec<_>>(),
        }))
    }

    async fn rollback_agent_team_workers(
        &self,
        workspace_id: &str,
        workers: &[StartedAgentTeamWorker],
    ) -> Vec<Value> {
        let mut rollback = Vec::with_capacity(workers.len());
        for worker in workers.iter().rev() {
            let session_id = worker
                .result
                .get("session_id")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            let topology = self
                .invoke_value("workspace.get", json!({ "workspace_id": workspace_id }))
                .await;
            let resolved_pane_id = session_id.as_deref().and_then(|session_id| {
                topology
                    .as_ref()
                    .ok()
                    .and_then(|value| worker_location_for_session(value, session_id))
                    .map(|(_, pane_id)| pane_id)
            });
            let topology_resolved = resolved_pane_id.is_some();
            let pane_id = resolved_pane_id.or_else(|| worker.rollback_pane_id.clone());
            let session_terminated = match session_id.as_deref() {
                Some(session_id) => self
                    .invoke_value(
                        "session.terminate",
                        json!({ "session_id": session_id, "mode": "kill" }),
                    )
                    .await
                    .is_ok(),
                None => false,
            };
            let pane_closed = match pane_id.as_deref() {
                Some(pane_id) => self
                    .invoke_value(
                        "pane.close",
                        json!({
                            "workspace_id": workspace_id,
                            "pane_id": pane_id,
                            "surface_policy": "close_surface",
                        }),
                    )
                    .await
                    .is_ok(),
                None => false,
            };
            rollback.push(json!({
                "name": worker.result.get("name"),
                "session_id": session_id,
                "pane_id": pane_id,
                "layout_anchor_pane_id": worker.rollback_pane_id.as_deref(),
                "topology_resolved": topology_resolved,
                "session_terminated": session_terminated,
                "pane_closed": pane_closed,
            }));
        }
        rollback
    }

    /// Create a main-left, equal-height worker-stack-right layout and start all named workers.
    #[tool(
        name = "agent_team_start",
        annotations(
            title = "Start visible agent team",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn agent_team_start(
        &self,
        Parameters(params): Parameters<AgentTeamStartToolParams>,
    ) -> CallToolResult {
        let _team_guard = self.team_operations.lock().await;
        let context = match self.resolve_context(params.workspace_id.clone()).await {
            Ok(context) => context,
            Err(message) => return control_error_result("system.identify", message),
        };
        let Some(workspace_id) = params
            .workspace_id
            .clone()
            .or_else(|| context.workspace_id.clone())
        else {
            return missing_context_result("workspace_id");
        };
        let Some(main_pane_id) = params.pane_id.clone().or_else(|| context.pane_id.clone()) else {
            return missing_context_result("pane_id");
        };
        if let Some(idempotency_key) = params
            .idempotency_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            match self
                .find_agent_team_by_idempotency(&workspace_id, idempotency_key)
                .await
            {
                Ok(Some(team)) => {
                    if team.status == "provisioning" {
                        match self.recover_agent_team_if_abandoned(&team).await {
                            Ok(true) => match self.load_agent_team(&team.team_id).await {
                                Ok(recovered) => {
                                    return CallToolResult::structured(
                                        agent_team_summary_with_recovery(&recovered, true),
                                    );
                                }
                                Err(message) => {
                                    return control_error_result("agent.list", message);
                                }
                            },
                            Ok(false) => {}
                            Err(_) => {
                                return CallToolResult::structured(agent_team_summary(&team, true));
                            }
                        }
                    }
                    return CallToolResult::structured(agent_team_summary(&team, true));
                }
                Ok(None) => {}
                Err(message) => return control_error_result("agent.list", message),
            }
        }
        let mode = params.mode.unwrap_or(AgentTeamModeParam::Adaptive);
        let max_workers = params
            .max_workers
            .map(usize::from)
            .unwrap_or(MAX_AGENT_TEAM_WORKERS);
        if params.workers.len() > max_workers {
            return control_error_result(
                "agent.team.start",
                format!("workers must not exceed max_workers ({max_workers})"),
            );
        }
        if mode == AgentTeamModeParam::Fixed && params.workers.is_empty() {
            return control_error_result(
                "agent.team.start",
                "fixed teams require at least one seed worker".to_string(),
            );
        }
        let layout = params
            .layout
            .unwrap_or(AgentTeamLayoutParam::MainLeftWorkersRight);
        let main_ratio = params.main_ratio.unwrap_or(0.5);
        if !(0.1..=0.9).contains(&main_ratio) {
            return control_error_result(
                "agent.team.start",
                "main_ratio must be between 0.1 and 0.9".to_string(),
            );
        }

        let mut names = std::collections::HashSet::with_capacity(params.workers.len());
        for worker in &params.workers {
            let name = worker.name.trim();
            if name.is_empty() || name.chars().count() > 64 {
                return control_error_result(
                    "agent.team.start",
                    "each worker name must contain between 1 and 64 characters".to_string(),
                );
            }
            if !names.insert(name.to_ascii_lowercase()) {
                return control_error_result(
                    "agent.team.start",
                    format!("worker name '{name}' is duplicated"),
                );
            }
        }

        let initial_topology = match self
            .invoke_value(
                "workspace.get",
                json!({ "workspace_id": workspace_id.clone() }),
            )
            .await
        {
            Ok(topology) => topology,
            Err(message) => return control_error_result("workspace.get", message),
        };
        let Some((_, main_session_id)) =
            terminal_location_for_pane(&initial_topology, &main_pane_id)
        else {
            return control_error_result(
                "agent.team.start",
                format!("main pane '{main_pane_id}' must be a leaf pane with a terminal session"),
            );
        };

        match self
            .find_agent_team_for_main_session(&workspace_id, &main_session_id)
            .await
        {
            Ok(Some(existing)) => {
                let requested_key = params.idempotency_key.as_deref().map(str::trim);
                if requested_key.is_some_and(|key| !key.is_empty())
                    && existing.idempotency_key.as_deref() == requested_key
                {
                    if existing.status == "provisioning" {
                        match self.recover_agent_team_if_abandoned(&existing).await {
                            Ok(true) => match self.load_agent_team(&existing.team_id).await {
                                Ok(recovered) => {
                                    return CallToolResult::structured(
                                        agent_team_summary_with_recovery(&recovered, true),
                                    );
                                }
                                Err(message) => {
                                    return control_error_result("agent.list", message);
                                }
                            },
                            Ok(false) => {}
                            Err(_) => {
                                return CallToolResult::structured(agent_team_summary(
                                    &existing, true,
                                ));
                            }
                        }
                    }
                    return CallToolResult::structured(agent_team_summary(&existing, true));
                }
                if existing.status == "provisioning" {
                    return match self.recover_agent_team_if_abandoned(&existing).await {
                        Ok(true) => match self.load_agent_team(&existing.team_id).await {
                            Ok(recovered) => CallToolResult::structured(
                                agent_team_summary_with_recovery(&recovered, true),
                            ),
                            Err(message) => control_error_result("agent.list", message),
                        },
                        Ok(false) => {
                            CallToolResult::structured(agent_team_summary(&existing, true))
                        }
                        Err(message) => control_error_result("agent.team.recover", message),
                    };
                }
                return control_error_result(
                    "agent.team.start",
                    format!(
                        "main session '{main_session_id}' is already owned by team '{}'",
                        existing.team_id
                    ),
                );
            }
            Ok(None) => {}
            Err(message) => return control_error_result("agent.list", message),
        }

        let team_id = new_agent_team_id();
        let team_cwd = params.cwd.clone().or_else(|| context.cwd.clone());
        let default_worker_kind = params
            .default_worker_kind
            .unwrap_or(AgentWorkerKindParam::CodexPane);
        let mut team = LoadedAgentTeam {
            team_id: team_id.clone(),
            workspace_id: workspace_id.clone(),
            main_session_id: main_session_id.clone(),
            layout_root_pane_id: main_pane_id.clone(),
            main_ratio,
            max_workers,
            generation: 0,
            status: "new".to_string(),
            mutation_id: None,
            auto_adopt_tmux: params.auto_adopt_tmux.unwrap_or(true),
            idempotency_key: params.idempotency_key.clone(),
            mode: mode.key().to_string(),
            default_worker_kind: default_worker_kind.key().to_string(),
            distribution: params.distribution.clone(),
            cwd: team_cwd.clone(),
            durability: params.durability.clone(),
            main: json!({ "session_id": main_session_id, "workspace_id": workspace_id }),
            members: Vec::new(),
        };
        let start_mutation_id =
            new_agent_team_mutation_id("start", &team.team_id, params.idempotency_key.as_deref());
        let reservation = match self
            .claim_agent_team_provisioning(&mut team, &start_mutation_id)
            .await
        {
            Ok(reservation) => reservation,
            Err(message) => return control_error_result("agent.team.reserve", message),
        };
        if !reservation.acquired {
            return CallToolResult::structured(json!({
                "team_id": team.team_id,
                "mode": team.mode,
                "status": "provisioning",
                "generation": team.generation,
                "max_workers": team.max_workers,
                "workspace_id": team.workspace_id,
                "worker_count": 0,
                "workers": [],
                "reused": true,
                "message": "an idempotent team start is already in progress"
            }));
        }
        let start_mutation_id = reservation.mutation_id.clone();
        let mut split_source_pane_id = main_pane_id.clone();
        let mut started_workers = Vec::with_capacity(params.workers.len());
        let worker_count = params.workers.len();

        for (index, worker) in params.workers.into_iter().enumerate() {
            let worker_name = worker.name.trim().to_string();
            let worker_kind = AgentWorkerKind::from(worker.kind);
            let registration = AgentTeamWorkerRegistration {
                team_id: team_id.clone(),
                main_session_id: main_session_id.clone(),
                layout_root_pane_id: main_pane_id.clone(),
                main_ratio,
                max_workers,
                worker_name: worker_name.clone(),
                worker_index: index + 1,
                generation: team.generation,
                auto_adopt_tmux: team.auto_adopt_tmux,
                member_idempotency_key: None,
            };
            let (axis, ratio) = if index == 0 {
                ("vertical", main_ratio)
            } else {
                let represented_worker_count = worker_count - index + 1;
                ("horizontal", 1.0 / represented_worker_count as f64)
            };
            let launch_result = self
                .start_agent_worker_resolved(
                    AgentWorkerStartToolParams {
                        workspace_id: Some(workspace_id.clone()),
                        pane_id: Some(split_source_pane_id.clone()),
                        name: Some(worker_name.clone()),
                        kind: worker.kind,
                        distribution: worker.distribution.or_else(|| params.distribution.clone()),
                        placement: Some("split".to_string()),
                        axis: Some(axis.to_string()),
                        ratio: Some(ratio),
                        cwd: worker.cwd.or_else(|| team_cwd.clone()),
                        args: worker.args,
                        columns: params.columns,
                        rows: params.rows,
                        durability: worker.durability.or_else(|| params.durability.clone()),
                    },
                    ResolvedContext {
                        workspace_id: Some(workspace_id.clone()),
                        pane_id: Some(split_source_pane_id.clone()),
                        surface_id: None,
                        session_id: None,
                        cwd: team_cwd.clone(),
                    },
                    false,
                    Some(&registration),
                )
                .await;
            if launch_result.is_error == Some(true) {
                let message = call_tool_result_error_message(&launch_result);
                let rollback = self
                    .rollback_agent_team_workers(&workspace_id, &started_workers)
                    .await;
                self.settle_agent_team_after_failed_mutation(&team, &start_mutation_id)
                    .await;
                return agent_team_error_result(worker_name, index, message, rollback);
            }
            let Some(launch) = launch_result.structured_content else {
                let rollback = self
                    .rollback_agent_team_workers(&workspace_id, &started_workers)
                    .await;
                self.settle_agent_team_after_failed_mutation(&team, &start_mutation_id)
                    .await;
                return agent_team_error_result(
                    worker_name,
                    index,
                    "worker launch returned no structured result".to_string(),
                    rollback,
                );
            };
            let next_pane_id = launch
                .get("pane_id")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            started_workers.push(StartedAgentTeamWorker {
                name: worker_name.clone(),
                kind: worker_kind,
                rollback_pane_id: next_pane_id.clone(),
                result: launch,
            });
            let Some(next_pane_id) = next_pane_id else {
                let rollback = self
                    .rollback_agent_team_workers(&workspace_id, &started_workers)
                    .await;
                self.settle_agent_team_after_failed_mutation(&team, &start_mutation_id)
                    .await;
                return agent_team_error_result(
                    worker_name,
                    index,
                    "split worker launch returned no pane_id".to_string(),
                    rollback,
                );
            };
            split_source_pane_id = next_pane_id;
        }

        let topology = match self
            .invoke_value("workspace.get", json!({ "workspace_id": workspace_id }))
            .await
        {
            Ok(topology) => topology,
            Err(message) => {
                let rollback = self
                    .rollback_agent_team_workers(&workspace_id, &started_workers)
                    .await;
                self.settle_agent_team_after_failed_mutation(&team, &start_mutation_id)
                    .await;
                return agent_team_error_result(
                    "topology-reconcile".to_string(),
                    worker_count,
                    format!("could not reconcile final worker panes: {message}"),
                    rollback,
                );
            }
        };

        for index in 0..started_workers.len() {
            let worker_name = started_workers[index].name.clone();
            let worker_kind = started_workers[index].kind;
            let Some(session_id) = started_workers[index]
                .result
                .get("session_id")
                .and_then(Value::as_str)
                .map(ToString::to_string)
            else {
                let rollback = self
                    .rollback_agent_team_workers(&workspace_id, &started_workers)
                    .await;
                self.settle_agent_team_after_failed_mutation(&team, &start_mutation_id)
                    .await;
                return agent_team_error_result(
                    worker_name,
                    index,
                    "worker result lost its session_id during topology reconciliation".to_string(),
                    rollback,
                );
            };
            let Some((surface_id, pane_id)) = worker_location_for_session(&topology, &session_id)
            else {
                let rollback = self
                    .rollback_agent_team_workers(&workspace_id, &started_workers)
                    .await;
                self.settle_agent_team_after_failed_mutation(&team, &start_mutation_id)
                    .await;
                return agent_team_error_result(
                    worker_name,
                    index,
                    format!("could not resolve the final leaf pane for session '{session_id}'"),
                    rollback,
                );
            };
            if let Value::Object(result) = &mut started_workers[index].result {
                result.insert("pane_id".to_string(), json!(pane_id));
                result.insert("surface_id".to_string(), json!(surface_id));
            }
            if let Err(message) = self
                .register_agent_worker_state(
                    worker_kind,
                    Some(&worker_name),
                    &session_id,
                    Some(&pane_id),
                    Some(&AgentTeamWorkerRegistration {
                        team_id: team_id.clone(),
                        main_session_id: main_session_id.clone(),
                        layout_root_pane_id: main_pane_id.clone(),
                        main_ratio,
                        max_workers,
                        worker_name: worker_name.clone(),
                        worker_index: index + 1,
                        generation: team.generation,
                        auto_adopt_tmux: team.auto_adopt_tmux,
                        member_idempotency_key: None,
                    }),
                )
                .await
            {
                let rollback = self
                    .rollback_agent_team_workers(&workspace_id, &started_workers)
                    .await;
                self.settle_agent_team_after_failed_mutation(&team, &start_mutation_id)
                    .await;
                return agent_team_error_result(
                    worker_name,
                    index,
                    format!("worker metadata registration failed: {message}"),
                    rollback,
                );
            }
            if let Value::Object(result) = &mut started_workers[index].result {
                result.insert("agent_state_registered".to_string(), json!(true));
            }
        }

        let main_location = worker_location_for_session(&topology, &main_session_id);
        let worker_results = started_workers
            .into_iter()
            .map(|worker| worker.result)
            .collect::<Vec<_>>();
        if let Err(message) = self
            .settle_agent_team_mutation(&team, &start_mutation_id, "ready")
            .await
        {
            let rollback_workers = worker_results
                .iter()
                .map(|result| StartedAgentTeamWorker {
                    name: result
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("worker")
                        .to_string(),
                    kind: AgentWorkerKind::CodexPane,
                    rollback_pane_id: result
                        .get("pane_id")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                    result: result.clone(),
                })
                .collect::<Vec<_>>();
            let rollback = self
                .rollback_agent_team_workers(&workspace_id, &rollback_workers)
                .await;
            self.settle_agent_team_after_failed_mutation(&team, &start_mutation_id)
                .await;
            return agent_team_error_result(
                "team-manifest".to_string(),
                worker_count,
                format!("could not persist adaptive team manifest: {message}"),
                rollback,
            );
        }

        CallToolResult::structured(json!({
            "team_id": team_id,
            "mode": mode.key(),
            "status": "ready",
            "generation": team.generation,
            "max_workers": max_workers,
            "auto_adopt_tmux": team.auto_adopt_tmux,
            "workspace_id": workspace_id,
            "layout": layout.key(),
            "main": {
                "layout_root_pane_id": main_pane_id,
                "pane_id": main_location.as_ref().map(|(_, pane_id)| pane_id),
                "surface_id": main_location.as_ref().map(|(surface_id, _)| surface_id),
                "session_id": main_session_id,
                "ratio": main_ratio,
            },
            "worker_count": worker_results.len(),
            "workers": worker_results,
            "rollback_policy": "reverse-created-workers-on-failure",
        }))
    }

    /// Add one worker to an adaptive team and safely reflow the managed worker subtree.
    #[tool(
        name = "agent_team_spawn",
        annotations(
            title = "Spawn adaptive team worker",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn agent_team_spawn(
        &self,
        Parameters(params): Parameters<AgentTeamSpawnToolParams>,
    ) -> CallToolResult {
        let _team_guard = self.team_operations.lock().await;
        let mut team = match self.load_agent_team(&params.team_id).await {
            Ok(team) => team,
            Err(message) => return control_error_result("agent.team.spawn", message),
        };
        if team.status == "provisioning" {
            if let Err(message) = self.recover_agent_team_if_abandoned(&team).await {
                return control_error_result("agent.team.recover", message);
            }
            team = match self.load_agent_team(&params.team_id).await {
                Ok(team) => team,
                Err(message) => return control_error_result("agent.list", message),
            };
        }
        if team.mode != "adaptive" {
            return control_error_result(
                "agent.team.spawn",
                format!(
                    "team '{}' is fixed and cannot accept dynamic workers",
                    team.team_id
                ),
            );
        }
        if let Some(key) = params
            .idempotency_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if let Some(existing) = team.members.iter().find(|member| {
                member
                    .pointer("/telemetry/team_member_idempotency_key")
                    .and_then(Value::as_str)
                    == Some(key)
            }) {
                return CallToolResult::structured(json!({
                    "team_id": team.team_id,
                    "generation": team.generation,
                    "reused": true,
                    "worker": existing,
                }));
            }
        }
        if let Some(expected) = params.expected_generation {
            if expected != team.generation {
                return agent_team_generation_conflict(&team, expected);
            }
        }
        let managed_member_count = agent_team_managed_members(&team).len();
        let top_workers = agent_team_top_workers(&team)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        if managed_member_count >= team.max_workers {
            return control_error_result(
                "agent.team.spawn",
                format!(
                    "team '{}' reached max_workers ({}) across all managed members",
                    team.team_id, team.max_workers,
                ),
            );
        }
        let existing_names = team
            .members
            .iter()
            .filter_map(|member| {
                member
                    .pointer("/telemetry/worker_name")
                    .and_then(Value::as_str)
            })
            .map(|name| name.to_ascii_lowercase())
            .collect::<std::collections::HashSet<_>>();
        let worker_name = match params.name.as_deref().map(str::trim) {
            Some("") => {
                return control_error_result(
                    "agent.team.spawn",
                    "worker name must not be empty".to_string(),
                )
            }
            Some(name) if name.chars().count() > 64 => {
                return control_error_result(
                    "agent.team.spawn",
                    "worker name must contain at most 64 characters".to_string(),
                )
            }
            Some(name) => name.to_string(),
            None => next_agent_team_worker_name(&existing_names),
        };
        if existing_names.contains(&worker_name.to_ascii_lowercase()) {
            return control_error_result(
                "agent.team.spawn",
                format!("worker name '{worker_name}' is already in use"),
            );
        }
        let worker_kind_param = match params.kind {
            Some(kind) => kind,
            None => match AgentWorkerKindParam::from_key(&team.default_worker_kind) {
                Some(kind) => kind,
                None => {
                    return control_error_result(
                        "agent.team.spawn",
                        format!(
                            "team default worker kind '{}' is invalid",
                            team.default_worker_kind
                        ),
                    )
                }
            },
        };
        let topology = match self
            .invoke_value(
                "workspace.get",
                json!({ "workspace_id": team.workspace_id }),
            )
            .await
        {
            Ok(value) => value,
            Err(message) => return control_error_result("workspace.get", message),
        };
        let (anchor_pane_id, axis, ratio) = if top_workers.is_empty() {
            let Some((_, pane_id)) = worker_location_for_session(&topology, &team.main_session_id)
            else {
                return control_error_result(
                    "agent.team.spawn",
                    "could not resolve the team main pane".to_string(),
                );
            };
            team.layout_root_pane_id = pane_id.clone();
            (pane_id, "vertical", team.main_ratio)
        } else {
            let anchor = top_workers
                .iter()
                .max_by_key(|member| {
                    member
                        .pointer("/telemetry/worker_index")
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                })
                .expect("non-empty top workers");
            let Some(session_id) = anchor.get("session_id").and_then(Value::as_str) else {
                return control_error_result(
                    "agent.team.spawn",
                    "the last team worker has no session_id".to_string(),
                );
            };
            let Some((_, pane_id)) = worker_location_for_session(&topology, session_id) else {
                return control_error_result(
                    "agent.team.spawn",
                    format!("could not resolve worker session '{session_id}' to a pane"),
                );
            };
            (pane_id, "horizontal", 0.5)
        };
        let worker_index = top_workers
            .iter()
            .filter_map(|member| {
                member
                    .pointer("/telemetry/worker_index")
                    .and_then(Value::as_u64)
            })
            .max()
            .unwrap_or(0) as usize
            + 1;
        let mutation_id =
            new_agent_team_mutation_id("spawn", &team.team_id, params.idempotency_key.as_deref());
        let reservation = match self
            .reserve_agent_team_generation(&team, &mutation_id)
            .await
        {
            Ok(reservation) => reservation,
            Err(message) => return control_error_result("agent.team.reserve", message),
        };
        if !reservation.acquired {
            return CallToolResult::structured(json!({
                "team_id": team.team_id,
                "generation": reservation.generation,
                "status": "provisioning",
                "reused": true,
                "message": "the same worker mutation is already in progress",
            }));
        }
        let next_generation = reservation.generation;
        let mutation_id = reservation.mutation_id;
        team.generation = next_generation;
        team.status = "provisioning".to_string();
        team.mutation_id = Some(mutation_id.clone());
        let mut registration = AgentTeamWorkerRegistration {
            team_id: team.team_id.clone(),
            main_session_id: team.main_session_id.clone(),
            layout_root_pane_id: team.layout_root_pane_id.clone(),
            main_ratio: team.main_ratio,
            max_workers: team.max_workers,
            worker_name: worker_name.clone(),
            worker_index,
            generation: next_generation,
            auto_adopt_tmux: team.auto_adopt_tmux,
            member_idempotency_key: params.idempotency_key.clone(),
        };
        let launch = self
            .start_agent_worker_resolved(
                AgentWorkerStartToolParams {
                    workspace_id: Some(team.workspace_id.clone()),
                    pane_id: Some(anchor_pane_id),
                    name: Some(worker_name.clone()),
                    kind: worker_kind_param,
                    distribution: params.distribution.or_else(|| team.distribution.clone()),
                    placement: Some("split".to_string()),
                    axis: Some(axis.to_string()),
                    ratio: Some(ratio),
                    cwd: params.cwd.or_else(|| team.cwd.clone()),
                    args: params.args,
                    columns: params.columns,
                    rows: params.rows,
                    durability: params.durability.or_else(|| team.durability.clone()),
                },
                ResolvedContext {
                    workspace_id: Some(team.workspace_id.clone()),
                    pane_id: None,
                    surface_id: None,
                    session_id: None,
                    cwd: team.cwd.clone(),
                },
                false,
                Some(&registration),
            )
            .await;
        if launch.is_error == Some(true) {
            self.settle_agent_team_after_failed_mutation(&team, &mutation_id)
                .await;
            return launch;
        }
        let Some(mut worker) = launch.structured_content else {
            self.settle_agent_team_after_failed_mutation(&team, &mutation_id)
                .await;
            return control_error_result(
                "agent.team.spawn",
                "worker launch returned no structured result".to_string(),
            );
        };
        let Some(session_id) = worker
            .get("session_id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
        else {
            let rollback = self
                .rollback_agent_team_workers(
                    &team.workspace_id,
                    &[StartedAgentTeamWorker {
                        name: worker_name,
                        kind: AgentWorkerKind::from(worker_kind_param),
                        rollback_pane_id: worker
                            .get("pane_id")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        result: worker,
                    }],
                )
                .await;
            self.settle_agent_team_after_failed_mutation(&team, &mutation_id)
                .await;
            return agent_team_error_result(
                "launch-session".to_string(),
                worker_index,
                "worker launch returned no session_id".to_string(),
                rollback,
            );
        };
        let topology = match self
            .invoke_value(
                "workspace.get",
                json!({ "workspace_id": team.workspace_id }),
            )
            .await
        {
            Ok(value) => value,
            Err(message) => {
                let rollback = self
                    .rollback_agent_team_workers(
                        &team.workspace_id,
                        &[StartedAgentTeamWorker {
                            name: worker_name,
                            kind: AgentWorkerKind::from(worker_kind_param),
                            rollback_pane_id: worker
                                .get("pane_id")
                                .and_then(Value::as_str)
                                .map(ToString::to_string),
                            result: worker,
                        }],
                    )
                    .await;
                self.settle_agent_team_after_failed_mutation(&team, &mutation_id)
                    .await;
                return agent_team_error_result(
                    "topology-reconcile".to_string(),
                    worker_index,
                    message,
                    rollback,
                );
            }
        };
        let Some((surface_id, pane_id)) = worker_location_for_session(&topology, &session_id)
        else {
            let rollback = self
                .rollback_agent_team_workers(
                    &team.workspace_id,
                    &[StartedAgentTeamWorker {
                        name: worker_name,
                        kind: AgentWorkerKind::from(worker_kind_param),
                        rollback_pane_id: worker
                            .get("pane_id")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        result: worker,
                    }],
                )
                .await;
            self.settle_agent_team_after_failed_mutation(&team, &mutation_id)
                .await;
            return agent_team_error_result(
                "topology-reconcile".to_string(),
                worker_index,
                format!("could not resolve the new worker session '{session_id}'"),
                rollback,
            );
        };
        if let Value::Object(result) = &mut worker {
            result.insert("pane_id".to_string(), json!(pane_id));
            result.insert("surface_id".to_string(), json!(surface_id));
        }
        if top_workers.is_empty() {
            if let Some((_, main_pane_id)) =
                worker_location_for_session(&topology, &team.main_session_id)
            {
                let panes = agent_team_topology_panes(&topology);
                if let Some(layout_root_pane_id) =
                    agent_team_lowest_common_ancestor(&panes, &main_pane_id, &pane_id)
                {
                    team.layout_root_pane_id = layout_root_pane_id;
                    registration.layout_root_pane_id = team.layout_root_pane_id.clone();
                }
            }
        }
        if let Err(message) = self
            .register_agent_worker_state(
                AgentWorkerKind::from(worker_kind_param),
                Some(&worker_name),
                &session_id,
                Some(&pane_id),
                Some(&registration),
            )
            .await
        {
            let rollback = self
                .rollback_agent_team_workers(
                    &team.workspace_id,
                    &[StartedAgentTeamWorker {
                        name: worker_name,
                        kind: AgentWorkerKind::from(worker_kind_param),
                        rollback_pane_id: Some(pane_id),
                        result: worker,
                    }],
                )
                .await;
            self.settle_agent_team_after_failed_mutation(&team, &mutation_id)
                .await;
            return agent_team_error_result(
                "metadata-registration".to_string(),
                worker_index,
                message,
                rollback,
            );
        }
        let mut refreshed = match self.load_agent_team(&team.team_id).await {
            Ok(team) => team,
            Err(message) => {
                let rollback = self
                    .rollback_agent_team_workers(
                        &team.workspace_id,
                        &[StartedAgentTeamWorker {
                            name: worker_name,
                            kind: AgentWorkerKind::from(worker_kind_param),
                            rollback_pane_id: Some(pane_id),
                            result: worker,
                        }],
                    )
                    .await;
                self.settle_agent_team_after_failed_mutation(&team, &mutation_id)
                    .await;
                return agent_team_error_result(
                    "team-refresh".to_string(),
                    worker_index,
                    message,
                    rollback,
                );
            }
        };
        refreshed.generation = next_generation;
        refreshed.layout_root_pane_id = team.layout_root_pane_id;
        let reflow = match self.reflow_agent_team(&mut refreshed, false).await {
            Ok(value) if value.get("status").and_then(Value::as_str) == Some("ok") => value,
            Ok(value) => {
                let rollback = self
                    .rollback_agent_team_workers(
                        &refreshed.workspace_id,
                        &[StartedAgentTeamWorker {
                            name: worker_name,
                            kind: AgentWorkerKind::from(worker_kind_param),
                            rollback_pane_id: Some(pane_id),
                            result: worker,
                        }],
                    )
                    .await;
                self.settle_agent_team_after_failed_mutation(&refreshed, &mutation_id)
                    .await;
                return agent_team_error_result(
                    "layout-reflow".to_string(),
                    worker_index,
                    value.to_string(),
                    rollback,
                );
            }
            Err(message) => {
                let rollback = self
                    .rollback_agent_team_workers(
                        &refreshed.workspace_id,
                        &[StartedAgentTeamWorker {
                            name: worker_name,
                            kind: AgentWorkerKind::from(worker_kind_param),
                            rollback_pane_id: Some(pane_id),
                            result: worker,
                        }],
                    )
                    .await;
                self.settle_agent_team_after_failed_mutation(&refreshed, &mutation_id)
                    .await;
                return agent_team_error_result(
                    "layout-reflow".to_string(),
                    worker_index,
                    message,
                    rollback,
                );
            }
        };
        if let Some(key) = params.idempotency_key.as_deref() {
            if let Some(member) = refreshed.members.iter_mut().find(|member| {
                member.get("session_id").and_then(Value::as_str) == Some(session_id.as_str())
            }) {
                let state = member
                    .get("state")
                    .and_then(Value::as_str)
                    .unwrap_or("running");
                let reason = member.get("reason").cloned().unwrap_or(Value::Null);
                let mut telemetry = member
                    .get("telemetry")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                telemetry.insert("team_member_idempotency_key".to_string(), json!(key));
                if let Err(message) = self
                    .invoke_value(
                        "agent.set_state",
                        json!({
                            "session_id": session_id,
                            "state": state,
                            "reason": reason,
                            "telemetry": telemetry,
                        }),
                    )
                    .await
                {
                    return control_error_result("agent.set_state", message);
                }
            }
        }
        refreshed.generation = next_generation;
        refreshed.status = "provisioning".to_string();
        refreshed.mutation_id = Some(mutation_id.clone());
        if let Err(message) = self
            .settle_agent_team_mutation(&refreshed, &mutation_id, "ready")
            .await
        {
            return control_error_result("agent.team.settle", message);
        }
        CallToolResult::structured(json!({
            "team_id": refreshed.team_id,
            "generation": refreshed.generation,
            "worker": worker,
            "worker_count": agent_team_managed_members(&refreshed).len(),
            "layout": reflow,
            "reused": false,
        }))
    }

    /// Recompute equal worker ratios without moving foreign panes.
    #[tool(
        name = "agent_team_reflow",
        annotations(
            title = "Reflow adaptive agent team",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn agent_team_reflow(
        &self,
        Parameters(params): Parameters<AgentTeamIdToolParams>,
    ) -> CallToolResult {
        let _team_guard = self.team_operations.lock().await;
        let mut team = match self.load_agent_team(&params.team_id).await {
            Ok(team) => team,
            Err(message) => return control_error_result("agent.team.reflow", message),
        };
        if let Some(expected) = params.expected_generation {
            if expected != team.generation {
                return agent_team_generation_conflict(&team, expected);
            }
        }
        let dry_run = params.dry_run.unwrap_or(false);
        if dry_run {
            if team.status == "provisioning" {
                return control_error_result(
                    "agent.team.reflow",
                    "team mutation is still provisioning; dry-run does not recover it".to_string(),
                );
            }
            return match self.reflow_agent_team(&mut team, true).await {
                Ok(value) => CallToolResult::structured(value),
                Err(message) => control_error_result("agent.team.reflow", message),
            };
        }
        if team.status == "provisioning" {
            if let Err(message) = self.recover_agent_team_if_abandoned(&team).await {
                return control_error_result("agent.team.recover", message);
            }
            team = match self.load_agent_team(&params.team_id).await {
                Ok(team) => team,
                Err(message) => return control_error_result("agent.list", message),
            };
        }
        let mutation_id = format!(
            "reflow:{}:{}",
            team.team_id,
            team.generation.saturating_add(1)
        );
        let reservation = match self
            .reserve_agent_team_generation(&team, &mutation_id)
            .await
        {
            Ok(reservation) => reservation,
            Err(message) => return control_error_result("agent.team.reserve", message),
        };
        if !reservation.acquired {
            return CallToolResult::structured(json!({
                "team_id": team.team_id,
                "generation": reservation.generation,
                "status": "provisioning",
                "reused": true,
                "message": "the same reflow mutation is already in progress",
            }));
        }
        let next_generation = reservation.generation;
        team.generation = next_generation;
        team.status = "provisioning".to_string();
        team.mutation_id = Some(mutation_id.clone());
        let layout = match self.reflow_agent_team(&mut team, false).await {
            Ok(value) => value,
            Err(message) => {
                self.settle_agent_team_after_failed_mutation(&team, &mutation_id)
                    .await;
                return control_error_result("agent.team.reflow", message);
            }
        };
        let status = if layout.get("status").and_then(Value::as_str) == Some("ok") {
            "ready"
        } else {
            "layout_dirty"
        };
        if let Err(message) = self
            .settle_agent_team_mutation(&team, &mutation_id, status)
            .await
        {
            return control_error_result("agent.team.settle", message);
        }
        CallToolResult::structured(json!({
            "team_id": team.team_id,
            "generation": next_generation,
            "layout": layout,
            "status": status,
        }))
    }

    /// Send literal instructions to an AgentMux-managed worker and optionally submit them with Enter.
    #[tool(
        name = "agent_worker_send",
        annotations(
            title = "Send to agent worker",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn agent_worker_send(
        &self,
        Parameters(params): Parameters<AgentWorkerSendToolParams>,
    ) -> CallToolResult {
        let session_id = match self
            .resolve_worker_session_id(params.workspace_id, params.session_id)
            .await
        {
            Ok(session_id) => session_id,
            Err(result) => return result,
        };
        if let Err(message) = self
            .invoke_value(
                "session.send_text",
                json!({ "session_id": session_id, "text": params.text }),
            )
            .await
        {
            return control_error_result("session.send_text", message);
        }
        let submitted = params.submit.unwrap_or(true);
        if submitted {
            if let Err(message) = self
                .invoke_value(
                    "session.send_key",
                    json!({ "session_id": session_id, "key": "enter" }),
                )
                .await
            {
                return control_error_result("session.send_key", message);
            }
        }
        CallToolResult::structured(json!({
            "session_id": session_id,
            "submitted": submitted,
        }))
    }

    /// Stop an AgentMux-managed pane worker. Full profile only.
    #[tool(
        name = "agent_worker_stop",
        annotations(
            title = "Stop agent worker",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn agent_worker_stop(
        &self,
        Parameters(params): Parameters<AgentWorkerStopToolParams>,
    ) -> CallToolResult {
        let session_id = match self
            .resolve_worker_session_id(params.workspace_id, params.session_id)
            .await
        {
            Ok(session_id) => session_id,
            Err(result) => return result,
        };
        self.call(
            "session.terminate",
            json!({
                "session_id": session_id,
                "mode": params.mode.unwrap_or_else(|| "soft".to_string()),
            }),
        )
        .await
    }

    /// Release a worker owned by an adaptive team, close its pane, and reflow remaining workers.
    #[tool(
        name = "agent_team_release",
        annotations(
            title = "Release adaptive team worker",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn agent_team_release(
        &self,
        Parameters(params): Parameters<AgentTeamReleaseToolParams>,
    ) -> CallToolResult {
        let _team_guard = self.team_operations.lock().await;
        let mut team = match self.load_agent_team(&params.team_id).await {
            Ok(team) => team,
            Err(message) => return control_error_result("agent.team.release", message),
        };
        if team.status == "provisioning" {
            if let Err(message) = self.recover_agent_team_if_abandoned(&team).await {
                return control_error_result("agent.team.recover", message);
            }
            team = match self.load_agent_team(&params.team_id).await {
                Ok(team) => team,
                Err(message) => return control_error_result("agent.list", message),
            };
        }
        if let Some(expected) = params.expected_generation {
            if expected != team.generation {
                return agent_team_generation_conflict(&team, expected);
            }
        }
        if params.session_id.is_none() && params.name.is_none() {
            return control_error_result(
                "agent.team.release",
                "session_id or name is required".to_string(),
            );
        }
        let candidates = team
            .members
            .iter()
            .filter(|member| {
                member
                    .pointer("/telemetry/team_role")
                    .and_then(Value::as_str)
                    != Some("main")
            })
            .filter(|member| {
                params.session_id.as_deref().is_none_or(|session_id| {
                    member.get("session_id").and_then(Value::as_str) == Some(session_id)
                })
            })
            .filter(|member| {
                params.name.as_deref().is_none_or(|name| {
                    member
                        .pointer("/telemetry/worker_name")
                        .and_then(Value::as_str)
                        .is_some_and(|worker_name| worker_name.eq_ignore_ascii_case(name))
                })
            })
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return control_error_result(
                "agent.team.release",
                format!(
                    "worker selector matched {} team members; expected exactly one",
                    candidates.len()
                ),
            );
        }
        let member = candidates[0].clone();
        let session_id = member
            .get("session_id")
            .and_then(Value::as_str)
            .expect("selected member has session id")
            .to_string();
        let topology = match self
            .invoke_value(
                "workspace.get",
                json!({ "workspace_id": team.workspace_id }),
            )
            .await
        {
            Ok(value) => value,
            Err(message) => return control_error_result("workspace.get", message),
        };
        let pane_id =
            worker_location_for_session(&topology, &session_id).map(|(_, pane_id)| pane_id);
        let mutation_id = format!("release:{session_id}");
        let reservation = match self
            .reserve_agent_team_generation(&team, &mutation_id)
            .await
        {
            Ok(reservation) => reservation,
            Err(message) => return control_error_result("agent.team.reserve", message),
        };
        if !reservation.acquired {
            return CallToolResult::structured(json!({
                "team_id": team.team_id,
                "generation": reservation.generation,
                "status": "provisioning",
                "reused": true,
                "message": "the same release mutation is already in progress",
            }));
        }
        let next_generation = reservation.generation;
        team.generation = next_generation;
        team.status = "provisioning".to_string();
        team.mutation_id = Some(mutation_id.clone());
        let already_terminal = member
            .get("state")
            .and_then(Value::as_str)
            .is_some_and(|state| matches!(state, "exited" | "failed" | "terminated"));
        let terminated = already_terminal
            || self
                .invoke_value(
                    "session.terminate",
                    json!({
                        "session_id": session_id,
                        "mode": params.mode.unwrap_or_else(|| "soft".to_string()),
                    }),
                )
                .await
                .is_ok();
        if !terminated {
            self.settle_agent_team_after_failed_mutation(&team, &mutation_id)
                .await;
            return control_error_result(
                "agent.team.release",
                format!("could not terminate session '{session_id}'; team membership was retained"),
            );
        }
        let pane_closed = match pane_id.as_deref() {
            Some(pane_id) => self
                .invoke_value(
                    "pane.close",
                    json!({
                        "workspace_id": team.workspace_id,
                        "pane_id": pane_id,
                        "surface_policy": "close_surface",
                    }),
                )
                .await
                .is_ok(),
            None => false,
        };
        if let Err(message) = self.clear_agent_team_membership(&member).await {
            self.settle_agent_team_after_failed_mutation(&team, &mutation_id)
                .await;
            return control_error_result(
                "agent.team.release",
                format!(
                    "session '{session_id}' terminated but team membership could not be cleared: {message}"
                ),
            );
        }
        let mut refreshed = match self.load_agent_team(&team.team_id).await {
            Ok(team) => team,
            Err(message) => {
                self.settle_agent_team_after_failed_mutation(&team, &mutation_id)
                    .await;
                return control_error_result("agent.list", message);
            }
        };
        refreshed.generation = next_generation;
        refreshed.status = "provisioning".to_string();
        refreshed.mutation_id = Some(mutation_id.clone());
        let layout = match self.reflow_agent_team(&mut refreshed, false).await {
            Ok(value) => value,
            Err(message) => json!({
                "status": "layout_dirty",
                "layout_dirty": true,
                "mutated": false,
                "error": message,
            }),
        };
        let final_status = if layout.get("layout_dirty").and_then(Value::as_bool) == Some(true) {
            "layout_dirty"
        } else {
            "ready"
        };
        if let Err(message) = self
            .settle_agent_team_mutation(&refreshed, &mutation_id, final_status)
            .await
        {
            return control_error_result("agent.team.settle", message);
        }
        CallToolResult::structured(json!({
            "team_id": refreshed.team_id,
            "generation": refreshed.generation,
            "released_session_id": session_id,
            "pane_id": pane_id,
            "session_terminated": terminated,
            "pane_closed": pane_closed,
            "status": if pane_closed { "released" } else { "released_with_open_pane" },
            "worker_count": agent_team_managed_members(&refreshed).len(),
            "layout": layout,
        }))
    }

    /// Install AgentMux tmux-compatibility shims and report integration readiness. Full profile only.
    #[tool(
        name = "agent_integration_setup",
        annotations(
            title = "Set up agent integration",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn agent_integration_setup(
        &self,
        Parameters(params): Parameters<AgentIntegrationSetupToolParams>,
    ) -> CallToolResult {
        let result = tokio::task::spawn_blocking(move || {
            let kind = params
                .integration
                .as_deref()
                .map(AgentIntegrationKind::parse)
                .transpose()
                .map_err(|error| error.to_string())?;
            let distribution =
                resolve_agent_integration_distribution(params.distribution.as_deref());
            let base_dir = resolve_cmuxterm_base_dir(None).map_err(|error| error.to_string())?;
            let bin_dir = base_dir.join("bin");
            let mut installed = install_agent_integration_shims(&base_dir, &bin_dir, None, None)
                .map_err(|error| error.to_string())?;
            if params.add_to_user_path.unwrap_or(false) {
                installed.user_path = Some(
                    ensure_windows_user_path_contains(&bin_dir)
                        .map_err(|error| error.to_string())?,
                );
            }
            let status =
                inspect_agent_integrations(&base_dir, &bin_dir, kind, distribution.as_deref());
            Ok::<Value, String>(json!({
                "install": agent_integration_install_result_json(&installed),
                "status": agent_integration_doctor_result_json(&status),
            }))
        })
        .await;
        match result {
            Ok(Ok(value)) => CallToolResult::structured(value),
            Ok(Err(message)) => control_error_result("agent.integration.setup", message),
            Err(error) => control_error_result(
                "agent.integration.setup",
                format!("integration setup task failed: {error}"),
            ),
        }
    }

    /// Close a workspace. Full profile only.
    #[tool(
        name = "workspace_close",
        annotations(
            title = "Close workspace",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn workspace_close(
        &self,
        Parameters(params): Parameters<WorkspaceCloseToolParams>,
    ) -> CallToolResult {
        self.call("workspace.close", json!({
            "workspace_id": params.workspace_id,
            "close_policy": params.close_policy.unwrap_or_else(|| "fail_if_running".to_string()),
        })).await
    }

    /// Close a pane and apply the requested surface policy. Full profile only.
    #[tool(
        name = "pane_close",
        annotations(
            title = "Close pane",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn pane_close(
        &self,
        Parameters(params): Parameters<PaneCloseToolParams>,
    ) -> CallToolResult {
        let (workspace_id, pane_id) = match self
            .resolve_workspace_and_pane(params.workspace_id, params.pane_id)
            .await
        {
            Ok(context) => context,
            Err(result) => return result,
        };
        self.call("pane.close", json!({
            "workspace_id": workspace_id,
            "pane_id": pane_id,
            "surface_policy": params.surface_policy.unwrap_or_else(|| "fail_if_session_running".to_string()),
        })).await
    }

    /// Close a surface. Full profile only.
    #[tool(
        name = "surface_close",
        annotations(
            title = "Close surface",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn surface_close(
        &self,
        Parameters(params): Parameters<SurfaceCloseToolParams>,
    ) -> CallToolResult {
        let context = match self.resolve_context(params.workspace_id.clone()).await {
            Ok(context) => context,
            Err(message) => return control_error_result("system.identify", message),
        };
        let Some(workspace_id) = params.workspace_id.or(context.workspace_id) else {
            return missing_context_result("workspace_id");
        };
        let Some(surface_id) = params.surface_id.or(context.surface_id) else {
            return missing_context_result("surface_id");
        };
        self.call(
            "surface.close",
            json!({ "workspace_id": workspace_id, "surface_id": surface_id }),
        )
        .await
    }

    /// Terminate a terminal session. Full profile only.
    #[tool(
        name = "session_terminate",
        annotations(
            title = "Terminate session",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn session_terminate(
        &self,
        Parameters(params): Parameters<SessionTerminateToolParams>,
    ) -> CallToolResult {
        let session_id = match self
            .resolve_session_id(params.workspace_id, params.session_id)
            .await
        {
            Ok(session_id) => session_id,
            Err(result) => return result,
        };
        self.call("session.terminate", json!({ "session_id": session_id, "mode": params.mode.unwrap_or_else(|| "soft".to_string()) })).await
    }

    /// Apply a structured AgentMux configuration update. Full profile only.
    #[tool(
        name = "config_update",
        annotations(
            title = "Update configuration",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn config_update(
        &self,
        Parameters(params): Parameters<ConfigUpdateToolParams>,
    ) -> CallToolResult {
        let Value::Object(mut update) = json!(params.update) else {
            return control_error_result(
                "config.update",
                "update must be a JSON object".to_string(),
            );
        };
        if let Some(workspace_id) = params.workspace_id {
            update.insert("workspace_id".to_string(), Value::String(workspace_id));
        }
        self.call("config.update", Value::Object(update)).await
    }

    /// Evaluate JavaScript in an embedded browser surface. Full profile only.
    #[tool(
        name = "browser_evaluate",
        annotations(
            title = "Evaluate browser JavaScript",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn browser_evaluate(
        &self,
        Parameters(params): Parameters<BrowserEvaluateToolParams>,
    ) -> CallToolResult {
        self.call("browser.evaluate", json!(params)).await
    }

    /// Run a configured AgentMux action. Full profile only.
    #[tool(
        name = "action_run",
        annotations(
            title = "Run configured action",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn action_run(
        &self,
        Parameters(params): Parameters<ActionRunToolParams>,
    ) -> CallToolResult {
        self.call("actions.run", json!(params)).await
    }

    /// Clear matching notifications. Full profile only.
    #[tool(
        name = "notification_clear",
        annotations(
            title = "Clear notifications",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn notification_clear(
        &self,
        Parameters(params): Parameters<NotificationClearToolParams>,
    ) -> CallToolResult {
        self.call("notification.clear", json!(params)).await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AgentMuxMcpServer {
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let tool_name = request.name.as_ref();
        if !self.profile.allows_tool(tool_name) {
            if let Some(required) = McpProfile::required_for_tool(tool_name) {
                return Ok(control_error_result(
                    "mcp.profile",
                    format!(
                        "tool '{tool_name}' requires the '{}' MCP profile; current profile is '{}'",
                        required.as_str(),
                        self.profile.as_str()
                    ),
                ));
            }
        }

        let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(call).await
    }

    fn get_info(&self) -> ServerInfo {
        let profile_boundary = match self.profile {
            McpProfile::Read => "This profile is read-only.",
            McpProfile::Standard => {
                "This is a trusted write and command-execution profile: it can execute arbitrary terminal commands, send terminal input, mutate browser-visible external systems, and update shared agent coordination state."
            }
            McpProfile::Full => {
                "This administrative profile includes trusted command execution plus destructive lifecycle, configuration, action, and browser JavaScript operations."
            }
        };
        let description = match self.profile {
            McpProfile::Read => {
                "Read-only MCP bridge to the AgentMux desktop control plane".to_string()
            }
            McpProfile::Standard => {
                "Trusted write and command-execution MCP bridge to the AgentMux desktop control plane"
                    .to_string()
            }
            McpProfile::Full => {
                "Administrative and destructive MCP bridge to the AgentMux desktop control plane"
                    .to_string()
            }
        };
        let instructions = format!(
            "Authenticated local AgentMux control using the '{}' capability profile. {profile_boundary} Use agentmux_context first when workspace, pane, surface, or session IDs were not supplied explicitly. Grant standard only to trusted clients and use full only for approved administrative operations.",
            self.profile.as_str(),
        );
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("agentmux", env!("CARGO_PKG_VERSION"))
                    .with_title("AgentMux MCP")
                    .with_description(description)
                    .with_website_url("https://github.com/raeseoklee/agentmux"),
            )
            .with_instructions(instructions)
    }
}

fn control_error_result(method: &str, message: String) -> CallToolResult {
    CallToolResult::structured_error(json!({
        "error": {
            "code": "agentmux_control_error",
            "method": method,
            "message": message,
        }
    }))
}

fn standard_scope_denied_result(tool_name: &str, reason: &str) -> CallToolResult {
    control_error_result(
        "mcp.profile",
        format!(
            "tool '{tool_name}' requires the 'full' MCP profile outside the caller-owned scope: {reason}"
        ),
    )
}

fn review_thread_matches_scope(thread: &Value, scope: &StandardCallerScope) -> bool {
    thread.get("workspace_id").and_then(Value::as_str) == Some(scope.workspace_id.as_str())
        && thread.get("repository_id").and_then(Value::as_str) == Some(scope.repository_id.as_str())
}

fn review_thread_owner_session_id(thread: &Value) -> Option<&str> {
    thread
        .get("comments")
        .and_then(Value::as_array)
        .and_then(|comments| comments.first())
        .and_then(|comment| comment.get("author_session_id"))
        .and_then(Value::as_str)
}

fn invalid_ipc_params_result(method: &str, error: agentmux_ipc::ControlError) -> CallToolResult {
    CallToolResult::structured_error(json!({
        "error": {
            "code": error.code.as_str(),
            "method": method,
            "message": format!("invalid tool input: {}", error.message),
            "details": error.details_json,
        }
    }))
}

fn validate_git_repository(
    params: &GitRepositoryToolParams,
) -> Result<(), agentmux_ipc::ControlError> {
    GitStatusPageParams {
        workspace_id: params.workspace_id.clone(),
        pane_id: params.pane_id.clone(),
        repository_id: params.repository_id.clone(),
        state: None,
        query: None,
        cursor: None,
        limit: None,
        generation: None,
    }
    .validate()
}

fn call_tool_result_error_message(result: &CallToolResult) -> String {
    result
        .structured_content
        .as_ref()
        .and_then(|value| value.pointer("/error/message"))
        .and_then(Value::as_str)
        .unwrap_or("agent worker launch failed without an error message")
        .to_string()
}

fn agent_team_error_result(
    worker_name: String,
    worker_index: usize,
    message: String,
    rollback: Vec<Value>,
) -> CallToolResult {
    CallToolResult::structured_error(json!({
        "error": {
            "code": "agentmux_agent_team_start_failed",
            "method": "agent.team.start",
            "message": message,
            "failed_worker": {
                "name": worker_name,
                "index": worker_index,
            },
            "rollback": rollback,
        }
    }))
}

fn missing_context_result(field: &str) -> CallToolResult {
    control_error_result(
        "system.identify",
        format!("{field} was not supplied and could not be resolved from AgentMux caller context"),
    )
}

fn worker_location_for_session(topology: &Value, session_id: &str) -> Option<(String, String)> {
    let surface_id = topology
        .get("surfaces")?
        .as_array()?
        .iter()
        .find(|surface| surface.get("session_id").and_then(Value::as_str) == Some(session_id))?
        .get("surface_id")?
        .as_str()?;
    let pane_id = topology
        .get("panes")?
        .as_array()?
        .iter()
        .find(|pane| {
            pane.get("kind").and_then(Value::as_str) == Some("leaf")
                && pane.get("mounted_surface_id").and_then(Value::as_str) == Some(surface_id)
        })?
        .get("pane_id")?
        .as_str()?;
    Some((surface_id.to_string(), pane_id.to_string()))
}

fn terminal_location_for_pane(topology: &Value, pane_id: &str) -> Option<(String, String)> {
    let surface_id = topology
        .get("panes")?
        .as_array()?
        .iter()
        .find(|pane| {
            pane.get("pane_id").and_then(Value::as_str) == Some(pane_id)
                && pane.get("kind").and_then(Value::as_str) == Some("leaf")
        })?
        .get("mounted_surface_id")?
        .as_str()?;
    let session_id = topology
        .get("surfaces")?
        .as_array()?
        .iter()
        .find(|surface| surface.get("surface_id").and_then(Value::as_str) == Some(surface_id))?
        .get("session_id")?
        .as_str()?;
    Some((surface_id.to_string(), session_id.to_string()))
}

fn empty_split_target(detail: &Value, source_pane_id: &str) -> Option<String> {
    detail
        .get("panes")?
        .as_array()?
        .iter()
        .filter(|pane| pane.get("parent_pane_id").and_then(Value::as_str) == Some(source_pane_id))
        .find(|pane| pane.get("mounted_surface_id").is_none_or(Value::is_null))
        .and_then(|pane| pane.get("pane_id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn summarize_diagnostics(value: Value, audit: Value) -> CallToolResult {
    CallToolResult::structured(json!({
        "generated_at": value.get("generated_at"),
        "format_version": value.get("format_version"),
        "recovery": value.get("recovery"),
        "backend_health": value.get("backend_health"),
        "queue_pressure": value.get("queue_pressure"),
        "output_stream": value.get("output_stream"),
        "control_audit": audit.get("records"),
    }))
}

fn bound_string_result(
    mut result: CallToolResult,
    field: &str,
    max_bytes: usize,
) -> CallToolResult {
    if result.is_error == Some(true) {
        return result;
    }
    let Some(Value::Object(mut value)) = result.structured_content.take() else {
        return control_error_result(
            "browser.read",
            "control plane returned no structured browser result".to_string(),
        );
    };
    let Some(text) = value.get(field).and_then(Value::as_str) else {
        return control_error_result(
            "browser.read",
            format!("control plane browser result did not contain '{field}'"),
        );
    };
    let original_byte_count = text.len();
    let returned = truncate_utf8(text, max_bytes).to_string();
    let returned_byte_count = returned.len();
    value.insert(field.to_string(), Value::String(returned));
    value.insert(
        "original_byte_count".to_string(),
        json!(original_byte_count),
    );
    value.insert(
        "returned_byte_count".to_string(),
        json!(returned_byte_count),
    );
    value.insert(
        "truncated".to_string(),
        json!(returned_byte_count < original_byte_count),
    );
    CallToolResult::structured(Value::Object(value))
}

fn truncate_utf8(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct ContextParams {
    /// Optional workspace override; otherwise AgentMux resolves the caller environment.
    workspace_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct WorkspaceGetParams {
    workspace_id: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct WorkspaceFilterParams {
    workspace_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct TerminalReadParams {
    session_id: String,
    /// Number of recent bytes to return. Defaults to 65536 and is capped at 1048576.
    max_bytes: Option<usize>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct EventPollToolParams {
    workspace_id: Option<String>,
    session_id: Option<String>,
    types: Option<Vec<String>>,
    /// Maximum events to return. Defaults to 100 and is capped at 500.
    max_events: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct BrowserSnapshotParams {
    surface_id: String,
    frame_id: Option<String>,
    /// Maximum HTML bytes to return. Defaults to 262144 and is capped at 1048576.
    max_bytes: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct BrowserGetToolParams {
    surface_id: String,
    selector: String,
    /// One of text, html, value, or attribute.
    kind: Option<String>,
    attribute: Option<String>,
    frame_id: Option<String>,
    /// Maximum value bytes to return. Defaults to 262144 and is capped at 1048576.
    max_bytes: Option<usize>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct TeamMessageListToolParams {
    workspace_id: Option<String>,
    include_read: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct ResolvedContext {
    workspace_id: Option<String>,
    pane_id: Option<String>,
    surface_id: Option<String>,
    session_id: Option<String>,
    cwd: Option<String>,
}

#[derive(Clone, Debug)]
struct StandardCallerBinding {
    workspace_id: String,
    pane_id: String,
    surface_id: String,
}

impl StandardCallerBinding {
    fn from_environment() -> Option<Self> {
        let value = |primary: &str, fallback: &str| {
            std::env::var(primary)
                .ok()
                .or_else(|| std::env::var(fallback).ok())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        Some(Self {
            workspace_id: value("AGENTMUX_WORKSPACE_ID", "CMUX_WORKSPACE_ID")?,
            pane_id: value("AGENTMUX_PANE_ID", "CMUX_PANE_ID")?,
            surface_id: value("AGENTMUX_SURFACE_ID", "CMUX_SURFACE_ID")?,
        })
    }
}

#[derive(Debug)]
struct StandardCallerScope {
    workspace_id: String,
    pane_id: String,
    session_id: String,
    repository_id: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct PaneFocusToolParams {
    workspace_id: Option<String>,
    pane_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct TerminalOpenToolParams {
    workspace_id: Option<String>,
    pane_id: Option<String>,
    backend: Option<String>,
    backend_profile: Option<String>,
    #[serde(default)]
    command: Vec<String>,
    cwd: Option<String>,
    columns: Option<u16>,
    rows: Option<u16>,
    durability: Option<String>,
    placement: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct TerminalSplitToolParams {
    workspace_id: Option<String>,
    pane_id: Option<String>,
    /// horizontal creates a top/bottom layout; vertical creates a left/right layout.
    axis: String,
    ratio: Option<f64>,
    /// clone_current (default) or empty.
    behavior: Option<String>,
    backend: Option<String>,
    backend_profile: Option<String>,
    #[serde(default)]
    command: Vec<String>,
    cwd: Option<String>,
    columns: Option<u16>,
    rows: Option<u16>,
    durability: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct TerminalSendTextToolParams {
    workspace_id: Option<String>,
    session_id: Option<String>,
    text: String,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct TerminalSendKeyToolParams {
    workspace_id: Option<String>,
    session_id: Option<String>,
    key: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct BrowserOpenToolParams {
    workspace_id: Option<String>,
    pane_id: Option<String>,
    url: Option<String>,
    profile: Option<String>,
    placement: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct BrowserOpenSplitToolParams {
    workspace_id: Option<String>,
    pane_id: Option<String>,
    url: Option<String>,
    profile: Option<String>,
    axis: String,
    ratio: Option<f64>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct BrowserNavigateToolParams {
    surface_id: String,
    url: String,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct BrowserClickToolParams {
    surface_id: String,
    selector: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
    frame_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct BrowserFillToolParams {
    surface_id: String,
    selector: String,
    text: String,
    frame_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct TeamMessageSendToolParams {
    workspace_id: Option<String>,
    thread_id: Option<String>,
    from_session_id: Option<String>,
    to_session_id: Option<String>,
    body: String,
    kind: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct TeamTaskClaimToolParams {
    workspace_id: Option<String>,
    task_id: String,
    session_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct TeamTaskCompleteToolParams {
    task_id: String,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct AgentSetStateToolParams {
    workspace_id: Option<String>,
    session_id: Option<String>,
    state: String,
    reason: Option<String>,
    telemetry: Option<AgentTelemetryToolParams>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct AgentTelemetryToolParams {
    activity: Option<String>,
    session: Option<String>,
    cost: Option<String>,
    tokens: Option<String>,
    cache: Option<String>,
    rate: Option<String>,
    ctx: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct AgentWorkerListToolParams {
    workspace_id: Option<String>,
    /// Optional worker kind: codex-pane, claude-teams, omo, omx, or omc.
    integration: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct AgentIntegrationStatusToolParams {
    /// Optional workspace whose configured WSL distribution should be diagnosed.
    workspace_id: Option<String>,
    /// Optional integration filter: claude-teams, omo, omx, or omc.
    integration: Option<String>,
    distribution: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct AgentWorkerStartToolParams {
    workspace_id: Option<String>,
    pane_id: Option<String>,
    /// Optional stable worker role used in agent status metadata.
    name: Option<String>,
    /// codex-pane starts an independent Codex CLI. Other values start tmux-compatible integrations.
    kind: AgentWorkerKindParam,
    distribution: Option<String>,
    /// split (default) or new_tab.
    placement: Option<String>,
    /// horizontal creates top/bottom panes; vertical creates left/right panes.
    axis: Option<String>,
    ratio: Option<f64>,
    cwd: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    columns: Option<u16>,
    rows: Option<u16>,
    durability: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct AgentTeamStartToolParams {
    workspace_id: Option<String>,
    /// Main terminal pane. Defaults to the calling agent's active pane.
    pane_id: Option<String>,
    /// Currently supported layout: main-left-workers-right.
    layout: Option<AgentTeamLayoutParam>,
    /// Width assigned to the main pane. Defaults to 0.5.
    #[schemars(range(min = 0.1, max = 0.9))]
    main_ratio: Option<f64>,
    /// adaptive (default) permits workers to be added and released on demand.
    mode: Option<AgentTeamModeParam>,
    /// Automatically adopt descendants created through the AgentMux tmux shim. Defaults to true.
    auto_adopt_tmux: Option<bool>,
    /// Optional caller-stable key used to discover an existing team after retries.
    idempotency_key: Option<String>,
    /// Safety ceiling for top-level workers. Defaults to 8.
    #[schemars(range(min = 1, max = 8))]
    max_workers: Option<u8>,
    /// Worker kind used when agent_team_spawn omits kind.
    default_worker_kind: Option<AgentWorkerKindParam>,
    distribution: Option<String>,
    cwd: Option<String>,
    columns: Option<u16>,
    rows: Option<u16>,
    durability: Option<String>,
    /// Optional seed workers. Adaptive teams may start empty.
    #[serde(default)]
    #[schemars(length(max = 8))]
    workers: Vec<AgentTeamWorkerSpec>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum AgentTeamModeParam {
    Adaptive,
    Fixed,
}

impl AgentTeamModeParam {
    fn key(self) -> &'static str {
        match self {
            Self::Adaptive => "adaptive",
            Self::Fixed => "fixed",
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct AgentTeamWorkerSpec {
    /// Unique role name shown in AgentMux agent status metadata.
    #[schemars(length(min = 1, max = 64))]
    name: String,
    kind: AgentWorkerKindParam,
    distribution: Option<String>,
    cwd: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    durability: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum AgentTeamLayoutParam {
    MainLeftWorkersRight,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct AgentTeamListToolParams {
    workspace_id: Option<String>,
    team_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct AgentTeamSpawnToolParams {
    team_id: String,
    expected_generation: Option<u64>,
    idempotency_key: Option<String>,
    /// Optional role name. AgentMux generates worker-N when omitted.
    name: Option<String>,
    /// Defaults to the team's configured worker kind.
    kind: Option<AgentWorkerKindParam>,
    distribution: Option<String>,
    cwd: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    columns: Option<u16>,
    rows: Option<u16>,
    durability: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct AgentTeamIdToolParams {
    team_id: String,
    expected_generation: Option<u64>,
    /// Return the planned ratios without mutating layout. Defaults to false.
    dry_run: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct AgentTeamReleaseToolParams {
    team_id: String,
    expected_generation: Option<u64>,
    session_id: Option<String>,
    name: Option<String>,
    /// soft (default), graceful, or kill.
    mode: Option<String>,
}

impl AgentTeamLayoutParam {
    fn key(self) -> &'static str {
        match self {
            Self::MainLeftWorkersRight => "main-left-workers-right",
        }
    }
}

const AGENT_TEAM_TELEMETRY_KEYS: &[&str] = &[
    "team_id",
    "team_role",
    "worker_name",
    "parent_session_id",
    "layout_root_pane_id",
    "main_ratio",
    "max_workers",
    "worker_index",
    "team_mode",
    "team_status",
    "team_layout",
    "team_generation",
    "team_mutation_id",
    "team_mutation_owner_id",
    "team_auto_adopt",
    "team_idempotency_key",
    "team_member_idempotency_key",
    "default_worker_kind",
    "distribution",
    "team_cwd",
    "durability",
];

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct AgentWorkerSendToolParams {
    workspace_id: Option<String>,
    session_id: Option<String>,
    text: String,
    /// Submit the text with Enter. Defaults to true.
    submit: Option<bool>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct AgentWorkerStopToolParams {
    workspace_id: Option<String>,
    session_id: Option<String>,
    /// soft (default), graceful, or kill.
    mode: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct AgentIntegrationSetupToolParams {
    /// Optional integration to diagnose after installing all shared shims.
    integration: Option<String>,
    distribution: Option<String>,
    /// Add the shim directory to the Windows user PATH. Defaults to false.
    add_to_user_path: Option<bool>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum AgentWorkerKindParam {
    CodexPane,
    ClaudeTeams,
    Omo,
    Omx,
    Omc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentWorkerKind {
    CodexPane,
    Integration(AgentIntegrationKind),
}

#[derive(Debug)]
struct StartedAgentTeamWorker {
    name: String,
    kind: AgentWorkerKind,
    rollback_pane_id: Option<String>,
    result: Value,
}

#[derive(Clone, Debug)]
struct AgentTeamWorkerRegistration {
    team_id: String,
    main_session_id: String,
    layout_root_pane_id: String,
    main_ratio: f64,
    max_workers: usize,
    worker_name: String,
    worker_index: usize,
    generation: u64,
    auto_adopt_tmux: bool,
    member_idempotency_key: Option<String>,
}

#[derive(Clone, Debug)]
struct AgentTeamReservation {
    generation: u64,
    mutation_id: String,
    acquired: bool,
}

impl AgentTeamReservation {
    fn from_control_result(result: &Value, generation: u64, mutation_id: &str) -> Self {
        Self {
            generation: result
                .get("generation")
                .and_then(Value::as_u64)
                .unwrap_or(generation),
            mutation_id: result
                .get("mutation_id")
                .and_then(Value::as_str)
                .unwrap_or(mutation_id)
                .to_string(),
            acquired: result
                .get("acquired")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| {
                    !result
                        .get("reused")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                }),
        }
    }
}

#[derive(Clone, Debug)]
struct LoadedAgentTeam {
    team_id: String,
    workspace_id: String,
    main_session_id: String,
    layout_root_pane_id: String,
    main_ratio: f64,
    max_workers: usize,
    generation: u64,
    status: String,
    mutation_id: Option<String>,
    auto_adopt_tmux: bool,
    idempotency_key: Option<String>,
    mode: String,
    default_worker_kind: String,
    distribution: Option<String>,
    cwd: Option<String>,
    durability: Option<String>,
    main: Value,
    members: Vec<Value>,
}

impl From<AgentWorkerKindParam> for AgentWorkerKind {
    fn from(value: AgentWorkerKindParam) -> Self {
        match value {
            AgentWorkerKindParam::CodexPane => Self::CodexPane,
            AgentWorkerKindParam::ClaudeTeams => {
                Self::Integration(AgentIntegrationKind::ClaudeTeams)
            }
            AgentWorkerKindParam::Omo => Self::Integration(AgentIntegrationKind::Omo),
            AgentWorkerKindParam::Omx => Self::Integration(AgentIntegrationKind::Omx),
            AgentWorkerKindParam::Omc => Self::Integration(AgentIntegrationKind::Omc),
        }
    }
}

impl AgentWorkerKindParam {
    fn key(self) -> &'static str {
        AgentWorkerKind::from(self).key()
    }

    fn from_key(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "codex-pane" => Some(Self::CodexPane),
            "claude-teams" => Some(Self::ClaudeTeams),
            "omo" => Some(Self::Omo),
            "omx" => Some(Self::Omx),
            "omc" => Some(Self::Omc),
            _ => None,
        }
    }
}

impl AgentWorkerKind {
    fn key(self) -> &'static str {
        match self {
            Self::CodexPane => "codex-pane",
            Self::Integration(kind) => kind.command_name(),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::CodexPane => "Codex pane",
            Self::Integration(AgentIntegrationKind::ClaudeTeams) => "Claude Teams",
            Self::Integration(AgentIntegrationKind::Omo) => "OMO",
            Self::Integration(AgentIntegrationKind::Omx) => "OMX",
            Self::Integration(AgentIntegrationKind::Omc) => "OMC",
        }
    }

    fn controller(self) -> &'static str {
        match self {
            Self::CodexPane => "agentmux-pane-worker",
            Self::Integration(_) => "agentmux-tmux-compat",
        }
    }

    fn activity(self) -> &'static str {
        match self {
            Self::CodexPane => "agent",
            Self::Integration(_) => "agent_team",
        }
    }
}

fn new_agent_team_id() -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("team-{}-{nonce:x}", std::process::id())
}

fn new_agent_team_mutation_id(kind: &str, team_id: &str, stable_key: Option<&str>) -> String {
    let stable_key = stable_key.map(str::trim).filter(|value| !value.is_empty());
    match stable_key {
        Some(key) => format!("{kind}:{team_id}:key:{key}"),
        None => {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            format!("{kind}:{team_id}:{}:{nonce:x}", std::process::id())
        }
    }
}

fn telemetry_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
    })
}

fn agent_team_launch_env(team: &AgentTeamWorkerRegistration) -> Vec<Value> {
    [
        ("AGENTMUX_TEAM_ID", team.team_id.clone()),
        ("AGENTMUX_TEAM_ROLE", "worker".to_string()),
        (
            "AGENTMUX_TEAM_MAIN_SESSION_ID",
            team.main_session_id.clone(),
        ),
        (
            "AGENTMUX_TEAM_LAYOUT_ROOT_PANE_ID",
            team.layout_root_pane_id.clone(),
        ),
        ("AGENTMUX_TEAM_MAIN_RATIO", team.main_ratio.to_string()),
        ("AGENTMUX_TEAM_MAX_WORKERS", team.max_workers.to_string()),
        ("AGENTMUX_TEAM_WORKER_NAME", team.worker_name.clone()),
        (
            "AGENTMUX_TEAM_AUTO_ADOPT",
            if team.auto_adopt_tmux { "1" } else { "0" }.to_string(),
        ),
    ]
    .into_iter()
    .map(|(key, value)| json!({ "key": key, "value": value }))
    .collect()
}

fn agent_team_top_workers(team: &LoadedAgentTeam) -> Vec<&Value> {
    team.members
        .iter()
        .filter(|member| {
            member
                .pointer("/telemetry/team_role")
                .and_then(Value::as_str)
                == Some("worker")
        })
        .collect()
}

fn agent_team_managed_members(team: &LoadedAgentTeam) -> Vec<&Value> {
    team.members
        .iter()
        .filter(|member| {
            member
                .pointer("/telemetry/team_role")
                .and_then(Value::as_str)
                != Some("main")
        })
        .collect()
}

fn next_agent_team_worker_name(existing: &std::collections::HashSet<String>) -> String {
    (1..=MAX_AGENT_TEAM_WORKERS + 1)
        .map(|index| format!("worker-{index}"))
        .find(|name| !existing.contains(name))
        .unwrap_or_else(|| format!("worker-{}", existing.len() + 1))
}

fn agent_team_summary(team: &LoadedAgentTeam, reused: bool) -> Value {
    let workers = team
        .members
        .iter()
        .filter(|member| {
            member
                .pointer("/telemetry/team_role")
                .and_then(Value::as_str)
                != Some("main")
        })
        .cloned()
        .collect::<Vec<_>>();
    json!({
        "team_id": team.team_id,
        "workspace_id": team.workspace_id,
        "mode": team.mode,
        "status": team
            .main
            .pointer("/telemetry/team_status")
            .and_then(Value::as_str)
            .unwrap_or("active"),
        "generation": team.generation,
        "max_workers": team.max_workers,
        "auto_adopt_tmux": team.auto_adopt_tmux,
        "main": team.main,
        "worker_count": agent_team_managed_members(team).len(),
        "descendant_count": workers.len().saturating_sub(agent_team_top_workers(team).len()),
        "workers": workers,
        "reused": reused,
    })
}

fn agent_team_summary_with_recovery(team: &LoadedAgentTeam, reused: bool) -> Value {
    let mut summary = agent_team_summary(team, reused);
    if let Value::Object(fields) = &mut summary {
        fields.insert("recovered".to_string(), json!(true));
        fields.insert(
            "message".to_string(),
            json!("an abandoned provisioning reservation was recovered as layout_dirty"),
        );
    }
    summary
}

fn agent_team_generation_conflict(team: &LoadedAgentTeam, expected: u64) -> CallToolResult {
    CallToolResult::structured_error(json!({
        "error": {
            "code": "generation_conflict",
            "method": "agent.team.generation",
            "message": format!(
                "team '{}' generation conflict: expected {expected}, current {}",
                team.team_id, team.generation
            ),
            "team_id": team.team_id,
            "expected_generation": expected,
            "current_generation": team.generation,
        }
    }))
}

#[derive(Clone, Debug)]
struct AgentTeamTopologyPane {
    pane_id: String,
    parent_pane_id: Option<String>,
    kind: String,
    split_axis: Option<String>,
    split_ratio: Option<f64>,
}

fn agent_team_topology_panes(topology: &Value) -> Vec<AgentTeamTopologyPane> {
    topology
        .get("panes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|pane| {
            Some(AgentTeamTopologyPane {
                pane_id: pane.get("pane_id")?.as_str()?.to_string(),
                parent_pane_id: pane
                    .get("parent_pane_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                kind: pane
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("leaf")
                    .to_string(),
                split_axis: pane
                    .get("split_axis")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                split_ratio: pane.get("split_ratio").and_then(Value::as_f64),
            })
        })
        .collect()
}

fn agent_team_children<'a>(
    panes: &'a [AgentTeamTopologyPane],
    parent_id: &str,
) -> Vec<&'a AgentTeamTopologyPane> {
    panes
        .iter()
        .filter(|pane| pane.parent_pane_id.as_deref() == Some(parent_id))
        .collect()
}

fn agent_team_subtree_contains(
    panes: &[AgentTeamTopologyPane],
    root_id: &str,
    target_id: &str,
) -> bool {
    root_id == target_id
        || agent_team_children(panes, root_id)
            .iter()
            .any(|child| agent_team_subtree_contains(panes, &child.pane_id, target_id))
}

fn agent_team_lowest_common_ancestor(
    panes: &[AgentTeamTopologyPane],
    first_pane_id: &str,
    second_pane_id: &str,
) -> Option<String> {
    let parents = panes
        .iter()
        .map(|pane| (pane.pane_id.as_str(), pane.parent_pane_id.as_deref()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut first_ancestors = std::collections::HashSet::new();
    let mut current = Some(first_pane_id);
    while let Some(pane_id) = current {
        first_ancestors.insert(pane_id);
        current = parents.get(pane_id).copied().flatten();
    }
    let mut current = Some(second_pane_id);
    while let Some(pane_id) = current {
        if first_ancestors.contains(pane_id) {
            return Some(pane_id.to_string());
        }
        current = parents.get(pane_id).copied().flatten();
    }
    None
}

fn agent_team_leaf_ids(panes: &[AgentTeamTopologyPane], root_id: &str) -> Vec<String> {
    let Some(root) = panes.iter().find(|pane| pane.pane_id == root_id) else {
        return Vec::new();
    };
    if root.kind == "leaf" {
        return vec![root.pane_id.clone()];
    }
    agent_team_children(panes, root_id)
        .into_iter()
        .flat_map(|child| agent_team_leaf_ids(panes, &child.pane_id))
        .collect()
}

fn agent_team_split_nodes<'a>(
    panes: &'a [AgentTeamTopologyPane],
    root_id: &str,
) -> Vec<&'a AgentTeamTopologyPane> {
    let Some(root) = panes.iter().find(|pane| pane.pane_id == root_id) else {
        return Vec::new();
    };
    if root.kind == "leaf" {
        return Vec::new();
    }
    let mut splits = vec![root];
    for child in agent_team_children(panes, root_id) {
        splits.extend(agent_team_split_nodes(panes, &child.pane_id));
    }
    splits
}

fn agent_team_collect_reflow_updates(
    panes: &[AgentTeamTopologyPane],
    root_id: &str,
    updates: &mut Vec<(String, f64, f64)>,
) {
    let Some(root) = panes.iter().find(|pane| pane.pane_id == root_id) else {
        return;
    };
    if root.kind == "leaf" {
        return;
    }
    let children = agent_team_children(panes, root_id);
    if children.len() != 2 {
        return;
    }
    let first_count = agent_team_leaf_ids(panes, &children[0].pane_id).len();
    let second_count = agent_team_leaf_ids(panes, &children[1].pane_id).len();
    let total = first_count + second_count;
    if total > 0 {
        let ratio = (first_count as f64 / total as f64).clamp(0.1, 0.9);
        updates.push((root.pane_id.clone(), ratio, root.split_ratio.unwrap_or(0.5)));
    }
    for child in children {
        agent_team_collect_reflow_updates(panes, &child.pane_id, updates);
    }
}

fn agent_team_layout_conflict(team_id: &str, conflict: String) -> Value {
    json!({
        "team_id": team_id,
        "status": "layout_conflict",
        "layout_dirty": true,
        "mutated": false,
        "conflicts": [conflict],
    })
}

fn is_managed_worker_session(session: &Value) -> bool {
    let activity = session
        .pointer("/telemetry/activity")
        .and_then(Value::as_str);
    let worker_name = session
        .pointer("/telemetry/session")
        .and_then(Value::as_str);
    activity == Some("agent_team")
        || worker_name.is_some_and(|name| name.starts_with("codex-pane:"))
}

fn resolve_agent_integration_distribution(explicit: Option<&str>) -> Option<String> {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(super::wsl_distribution_from_env)
        .or_else(|| {
            super::discover_wsl_distributions_from_backend()
                .ok()
                .and_then(|distributions| {
                    distributions
                        .iter()
                        .find(|distribution| distribution.is_default)
                        .or_else(|| distributions.first())
                        .map(|distribution| distribution.name.clone())
                })
        })
}

fn build_agent_worker_command(
    kind: AgentWorkerKind,
    mut args: Vec<String>,
) -> Result<Vec<String>, String> {
    match kind {
        AgentWorkerKind::CodexPane => {
            let mut command = vec!["codex".to_string()];
            if !args.iter().any(|arg| arg == "--no-alt-screen") {
                command.push("--no-alt-screen".to_string());
            }
            command.append(&mut args);
            Ok(command)
        }
        AgentWorkerKind::Integration(integration) => {
            let launcher = std::env::current_exe()
                .map_err(|error| format!("could not locate agentmux executable: {error}"))?;
            let launcher = path_to_wsl_value(&launcher).map_err(|error| error.to_string())?;
            let mut command = vec![
                launcher,
                "integrations".to_string(),
                "launch".to_string(),
                integration.command_name().to_string(),
            ];
            command.append(&mut args);
            Ok(command)
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct WorkspaceCloseToolParams {
    workspace_id: String,
    close_policy: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct PaneCloseToolParams {
    workspace_id: Option<String>,
    pane_id: Option<String>,
    surface_policy: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct SurfaceCloseToolParams {
    workspace_id: Option<String>,
    surface_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct SessionTerminateToolParams {
    workspace_id: Option<String>,
    session_id: Option<String>,
    mode: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct ConfigUpdateToolParams {
    workspace_id: Option<String>,
    update: ConfigUpdateToolValue,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct ConfigUpdateToolValue {
    appearance: Option<ConfigAppearanceToolUpdate>,
    locale: Option<ConfigLocaleToolUpdate>,
    updates: Option<ConfigUpdatesToolUpdate>,
    shortcuts: Option<ConfigShortcutsToolUpdate>,
    ui: Option<ConfigUiToolUpdate>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct ConfigAppearanceToolUpdate {
    theme: Option<String>,
    accent_key: Option<String>,
    font_size: Option<f64>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct ConfigLocaleToolUpdate {
    language: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct ConfigUpdatesToolUpdate {
    auto_check: Option<bool>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct ConfigShortcutsToolUpdate {
    bindings: Option<BTreeMap<String, ShortcutBindingToolValue>>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
#[serde(untagged)]
enum ShortcutBindingToolValue {
    One(String),
    Sequence(Vec<String>),
    Disabled(Option<()>),
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct ConfigUiToolUpdate {
    workspace_plus_action: Option<String>,
    surface_tab_plus_action: Option<String>,
    surface_tab_actions: Option<Vec<String>>,
    text_box_max_lines: Option<u8>,
    terminal_inner_margin: Option<u8>,
    terminal_gpu_acceleration: Option<String>,
    terminal_start_directory: Option<String>,
    terminal_start_custom_cwd: Option<String>,
    terminal_split_behavior: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct BrowserEvaluateToolParams {
    surface_id: String,
    script: String,
    frame_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct ActionRunToolParams {
    action_id: String,
    workspace_id: Option<String>,
    pane_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct NotificationClearToolParams {
    workspace_id: Option<String>,
    severity: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct GitRepositoryToolParams {
    /// AgentMux workspace containing the repository.
    workspace_id: String,
    /// Optional terminal pane whose live working directory selects the repository.
    pane_id: Option<String>,
    /// Optional repository identity when a workspace contains more than one repository.
    repository_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct GitStatusPageToolParams {
    workspace_id: String,
    /// Optional terminal pane whose live working directory selects the repository.
    pane_id: Option<String>,
    repository_id: Option<String>,
    /// Filter by a host-defined change state such as staged or unstaged.
    state: Option<String>,
    /// Case-insensitive repository-relative path query.
    query: Option<String>,
    /// Opaque cursor from a previous git_status_page response.
    cursor: Option<String>,
    /// Page size between 1 and 500.
    limit: Option<usize>,
    /// Repository generation to keep pages coherent while the repository changes.
    generation: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct GitDiffToolParams {
    workspace_id: String,
    /// Optional terminal pane whose live working directory selects the repository.
    pane_id: Option<String>,
    repository_id: Option<String>,
    /// Repository-relative path from git_status_page.
    path: String,
    /// Optional host-defined diff stage selector.
    stage: Option<String>,
    /// Context lines, from 0 through 200.
    context_lines: Option<u16>,
    generation: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct GitPathMutationToolParams {
    workspace_id: String,
    /// Optional terminal pane whose live working directory selects the repository.
    pane_id: Option<String>,
    repository_id: Option<String>,
    /// One to 500 repository-relative paths.
    paths: Vec<String>,
    /// Optional client-generated idempotency key.
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct GitAllMutationToolParams {
    workspace_id: String,
    /// Optional terminal pane whose live working directory selects the repository.
    pane_id: Option<String>,
    repository_id: Option<String>,
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct GitCommitToolParams {
    workspace_id: String,
    /// Optional terminal pane whose live working directory selects the repository.
    pane_id: Option<String>,
    repository_id: Option<String>,
    /// Commit message for already staged changes.
    message: String,
    #[serde(default)]
    amend: bool,
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct AgentWorktreeCreateToolParams {
    workspace_id: String,
    /// New or existing Git branch name for the isolated worktree.
    branch: String,
    /// Empty destination path owned by the AgentMux worktree operation.
    destination: String,
    base_revision: Option<String>,
    #[serde(default)]
    create_branch: bool,
    backend: Option<String>,
    backend_profile: Option<String>,
    /// Optional agent command; arguments are passed without shell interpolation.
    #[serde(default)]
    command: Vec<String>,
    cwd: Option<String>,
    /// Required client-generated idempotency key for safe retries.
    idempotency_key: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct AgentWorktreeListToolParams {
    workspace_id: Option<String>,
    #[serde(default)]
    include_completed: bool,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct AgentWorktreeRecoverToolParams {
    operation_id: Option<String>,
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct AgentWorktreeRemoveToolParams {
    worktree_id: String,
    #[serde(default)]
    force: bool,
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct GitReviewLineAnchorToolParams {
    /// Repository-relative file path.
    path: String,
    /// Diff side, typically old or new.
    side: String,
    /// One-based line number on the selected side.
    line: u32,
    start_line: Option<u32>,
    base_revision: Option<String>,
    head_revision: Option<String>,
    hunk_header: Option<String>,
    diff_hash: Option<String>,
}

impl From<GitReviewLineAnchorToolParams> for GitReviewLineAnchor {
    fn from(value: GitReviewLineAnchorToolParams) -> Self {
        Self {
            path: value.path,
            side: value.side,
            line: value.line,
            start_line: value.start_line,
            base_revision: value.base_revision,
            head_revision: value.head_revision,
            hunk_header: value.hunk_header,
            diff_hash: value.diff_hash,
        }
    }
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct GitReviewThreadListToolParams {
    workspace_id: String,
    pane_id: Option<String>,
    repository_id: Option<String>,
    path: Option<String>,
    #[serde(default)]
    include_resolved: bool,
    #[serde(default)]
    include_stale: bool,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct GitReviewThreadCreateToolParams {
    workspace_id: String,
    pane_id: Option<String>,
    repository_id: Option<String>,
    anchor: GitReviewLineAnchorToolParams,
    body: String,
    /// Full profile may name an author. Standard requires this to match the caller and always records the active caller session.
    author_session_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct GitReviewThreadUpdateToolParams {
    thread_id: String,
    resolved: Option<bool>,
    anchor: Option<GitReviewLineAnchorToolParams>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct GitReviewThreadIdToolParams {
    thread_id: String,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct GitReviewThreadMarkStaleToolParams {
    thread_id: String,
    stale: bool,
    reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct GitReviewThreadDeliverToolParams {
    thread_id: String,
    /// Host-supported delivery target, such as mailbox or terminal.
    target: String,
    target_session_id: Option<String>,
    #[serde(default)]
    include_context: bool,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct GitReviewCommentListToolParams {
    thread_id: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct GitReviewCommentCreateToolParams {
    thread_id: String,
    body: String,
    /// Full profile may name an author. Standard requires this to match the caller and always records the active caller session.
    author_session_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct GitReviewCommentUpdateToolParams {
    comment_id: String,
    body: String,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct GitReviewCommentIdToolParams {
    comment_id: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
struct DevelopmentServerCandidateListToolParams {
    workspace_id: Option<String>,
    session_id: Option<String>,
    #[serde(default)]
    include_dismissed: bool,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct DevelopmentServerCandidateDismissToolParams {
    candidate_id: String,
    reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct DevelopmentServerCandidateOpenInSplitToolParams {
    candidate_id: String,
    pane_id: Option<String>,
    /// Split axis accepted by the desktop host.
    axis: Option<String>,
    /// Split ratio from 0.05 through 0.95.
    ratio: Option<f64>,
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};
    use std::sync::Mutex;

    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    use super::*;

    #[derive(Default)]
    struct FakeTransport {
        responses: HashMap<String, Value>,
        calls: Mutex<Vec<(String, Value)>>,
    }

    struct ScriptedTransport {
        responses: Mutex<VecDeque<(String, Result<Value, String>)>>,
        calls: Mutex<Vec<(String, Value)>>,
    }

    impl ControlTransport for FakeTransport {
        fn invoke(&self, method: &str, params: Value) -> Result<Value, String> {
            self.calls
                .lock()
                .expect("fake calls lock")
                .push((method.to_string(), params));
            self.responses
                .get(method)
                .cloned()
                .ok_or_else(|| format!("missing fake response for {method}"))
        }
    }

    impl ControlTransport for ScriptedTransport {
        fn invoke(&self, method: &str, params: Value) -> Result<Value, String> {
            self.calls
                .lock()
                .expect("scripted calls lock")
                .push((method.to_string(), params));
            let Some((expected_method, result)) = self
                .responses
                .lock()
                .expect("scripted responses lock")
                .pop_front()
            else {
                return Err(format!("missing scripted response for {method}"));
            };
            if expected_method != method {
                return Err(format!(
                    "expected scripted method '{expected_method}', received '{method}'"
                ));
            }
            result
        }
    }

    fn test_server(
        responses: impl IntoIterator<Item = (&'static str, Value)>,
    ) -> (AgentMuxMcpServer, Arc<FakeTransport>) {
        test_server_for_profile(McpProfile::Read, responses)
    }

    fn test_server_for_profile(
        profile: McpProfile,
        responses: impl IntoIterator<Item = (&'static str, Value)>,
    ) -> (AgentMuxMcpServer, Arc<FakeTransport>) {
        let transport = Arc::new(FakeTransport {
            responses: responses
                .into_iter()
                .map(|(method, response)| (method.to_string(), response))
                .collect(),
            calls: Mutex::new(Vec::new()),
        });
        let server = AgentMuxMcpServer::with_transport_and_binding(
            profile,
            Arc::clone(&transport) as Arc<dyn ControlTransport>,
            (profile == McpProfile::Standard).then(standard_caller_binding),
        );
        (server, transport)
    }

    fn scripted_server_for_profile(
        profile: McpProfile,
        responses: impl IntoIterator<Item = (&'static str, Result<Value, String>)>,
    ) -> (AgentMuxMcpServer, Arc<ScriptedTransport>) {
        let transport = Arc::new(ScriptedTransport {
            responses: Mutex::new(
                responses
                    .into_iter()
                    .map(|(method, response)| (method.to_string(), response))
                    .collect(),
            ),
            calls: Mutex::new(Vec::new()),
        });
        let server = AgentMuxMcpServer::with_transport_and_binding(
            profile,
            Arc::clone(&transport) as Arc<dyn ControlTransport>,
            (profile == McpProfile::Standard).then(standard_caller_binding),
        );
        (server, transport)
    }

    fn standard_caller_binding() -> StandardCallerBinding {
        StandardCallerBinding {
            workspace_id: "workspace-1".to_string(),
            pane_id: "pane-1".to_string(),
            surface_id: "surface-1".to_string(),
        }
    }

    fn standard_caller_context() -> Value {
        json!({
            "workspace_id": "workspace-1",
            "pane_id": "pane-1",
            "surface_id": "surface-1",
            "session_id": "session-agent",
        })
    }

    fn standard_active_repository() -> Value {
        json!({
            "workspace_id": "workspace-1",
            "repository_id": "repo-1",
        })
    }

    fn standard_owned_review_threads() -> Value {
        json!({
            "threads": [{
                "thread_id": "review-owned",
                "workspace_id": "workspace-1",
                "repository_id": "repo-1",
                "comments": [
                    {
                        "comment_id": "comment-root",
                        "author_session_id": "session-agent"
                    },
                    {
                        "comment_id": "comment-owned",
                        "author_session_id": "session-agent"
                    },
                    {
                        "comment_id": "comment-peer",
                        "author_session_id": "session-peer"
                    }
                ],
            }],
        })
    }

    #[test]
    fn parser_accepts_reviewed_profiles_and_rejects_conflicting_credentials() {
        for (value, expected) in [
            ("read", McpProfile::Read),
            ("standard", McpProfile::Standard),
            ("full", McpProfile::Full),
        ] {
            let args = vec!["--profile".to_string(), value.to_string()];
            assert_eq!(parse_options(&args, false).unwrap().profile, expected);
        }

        let credentials = vec![
            "--token".to_string(),
            "secret".to_string(),
            "--token-file".to_string(),
            "token.txt".to_string(),
        ];
        assert!(parse_options(&credentials, false)
            .unwrap_err()
            .to_string()
            .contains("cannot be used together"));
    }

    #[test]
    fn mcp_root_help_is_available_without_a_subcommand() {
        let mut output = Vec::new();
        super::super::run_cli(["mcp"], &mut output).expect("render MCP help");
        let output = String::from_utf8(output).expect("UTF-8 help");
        assert!(output.contains("agentmux mcp <serve|doctor|setup>"));
    }

    #[tokio::test]
    async fn read_profile_exposes_only_the_reviewed_tools() {
        let (server, _) = test_server([]);
        let tools = server.tool_router.list_all();
        let mut names = tools
            .iter()
            .map(|tool| tool.name.to_string())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(
            names,
            vec![
                "agent_attention_list",
                "agent_integration_status",
                "agent_team_list",
                "agent_worker_list",
                "agent_worktree_list",
                "agentmux_context",
                "browser_get",
                "browser_snapshot",
                "development_server_candidate_list",
                "diagnostics_summary",
                "event_poll",
                "git_diff",
                "git_review_comment_list",
                "git_review_thread_list",
                "git_status_page",
                "git_status_summary",
                "session_list",
                "team_message_list",
                "team_task_list",
                "terminal_read",
                "workspace_get",
                "workspace_list",
            ]
        );
        assert!(tools.iter().all(|tool| {
            tool.annotations
                .as_ref()
                .and_then(|value| value.read_only_hint)
                == Some(true)
        }));
        assert!(tools
            .iter()
            .filter(|tool| tool.name.starts_with("browser_"))
            .all(|tool| tool
                .annotations
                .as_ref()
                .and_then(|value| value.open_world_hint)
                == Some(true)));
    }

    #[test]
    fn profiles_expose_exact_capability_and_risk_contracts() {
        let (read, _) = test_server_for_profile(McpProfile::Read, []);
        let (standard, _) = test_server_for_profile(McpProfile::Standard, []);
        let (full, _) = test_server_for_profile(McpProfile::Full, []);
        let names = |server: &AgentMuxMcpServer| {
            server
                .tool_router
                .list_all()
                .iter()
                .map(|tool| tool.name.to_string())
                .collect::<std::collections::BTreeSet<_>>()
        };
        let read_names = names(&read);
        let standard_names = names(&standard);
        let full_names = names(&full);

        let declared_read_names = READ_TOOL_NAMES
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            read_names,
            declared_read_names
                .iter()
                .map(|name| (*name).to_string())
                .collect()
        );
        assert_eq!(
            standard_names.len(),
            READ_TOOL_NAMES.len() + STANDARD_TOOL_NAMES.len()
        );
        assert_eq!(
            full_names.len(),
            READ_TOOL_NAMES.len() + STANDARD_TOOL_NAMES.len() + FULL_TOOL_NAMES.len()
        );
        assert!(read_names.is_subset(&standard_names));
        assert!(standard_names.is_subset(&full_names));
        for name in STANDARD_TOOL_NAMES {
            assert!(standard_names.contains(*name));
            assert!(!read_names.contains(*name));
        }
        for name in FULL_TOOL_NAMES {
            assert!(full_names.contains(*name));
            assert!(!standard_names.contains(*name));
        }
        let classified_standard_names = STANDARD_DESTRUCTIVE_TOOL_NAMES
            .iter()
            .chain(STANDARD_ADDITIVE_TOOL_NAMES)
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let declared_standard_names = STANDARD_TOOL_NAMES
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(classified_standard_names, declared_standard_names);
        for name in STANDARD_DESTRUCTIVE_TOOL_NAMES {
            let tool = standard
                .tool_router
                .list_all()
                .into_iter()
                .find(|tool| tool.name.as_ref() == *name)
                .expect("standard trusted-write tool");
            assert_eq!(
                tool.annotations
                    .as_ref()
                    .and_then(|annotations| annotations.read_only_hint),
                Some(false),
                "{name} must be marked as a write tool"
            );
            assert_eq!(
                tool.annotations
                    .as_ref()
                    .and_then(|annotations| annotations.destructive_hint),
                Some(true),
                "{name} can execute commands or mutate state and must be marked destructive"
            );
        }
        for name in STANDARD_ADDITIVE_TOOL_NAMES {
            let tool = standard
                .tool_router
                .list_all()
                .into_iter()
                .find(|tool| tool.name.as_ref() == *name)
                .expect("standard additive-write tool");
            assert_eq!(
                tool.annotations
                    .as_ref()
                    .and_then(|annotations| annotations.read_only_hint),
                Some(false),
                "{name} must be marked as a write tool"
            );
            assert_eq!(
                tool.annotations
                    .as_ref()
                    .and_then(|annotations| annotations.destructive_hint),
                Some(false),
                "{name} is intentionally additive or reversible"
            );
        }
        for name in FULL_TOOL_NAMES {
            let tool = full
                .tool_router
                .list_all()
                .into_iter()
                .find(|tool| tool.name.as_ref() == *name)
                .expect("full-profile tool");
            assert_eq!(
                tool.annotations
                    .as_ref()
                    .and_then(|annotations| annotations.destructive_hint),
                Some(true),
                "{name} must be marked destructive"
            );
        }

        let standard_info = standard.get_info();
        assert_eq!(
            standard_info.server_info.description.as_deref(),
            Some(
                "Trusted write and command-execution MCP bridge to the AgentMux desktop control plane"
            )
        );
        let standard_instructions = standard_info.instructions.expect("instructions");
        assert!(standard_instructions.contains("trusted write and command-execution profile"));
        assert!(standard_instructions.contains("execute arbitrary terminal commands"));
        let full_instructions = full.get_info().instructions.expect("instructions");
        assert!(full_instructions.contains("administrative profile"));
        assert!(full_instructions.contains("destructive lifecycle"));
    }

    #[test]
    fn profiles_authorize_each_tool_call_independently_of_router_visibility() {
        assert!(McpProfile::Read.allows_tool("workspace_list"));
        assert!(McpProfile::Read.allows_tool("git_status_page"));
        assert!(!McpProfile::Read.allows_tool("git_stage"));
        assert!(!McpProfile::Read.allows_tool("git_commit"));
        assert!(!McpProfile::Read.allows_tool("git_review_thread_deliver"));
        assert!(!McpProfile::Read.allows_tool("terminal_open"));
        assert!(!McpProfile::Read.allows_tool("agent_worktree_create"));
        assert!(!McpProfile::Read.allows_tool("workspace_close"));

        assert!(McpProfile::Standard.allows_tool("workspace_list"));
        assert!(McpProfile::Standard.allows_tool("terminal_open"));
        assert!(McpProfile::Standard.allows_tool("agent_worktree_create"));
        assert!(McpProfile::Standard.allows_tool("git_stage"));
        assert!(McpProfile::Standard.allows_tool("git_unstage"));
        assert!(McpProfile::Standard.allows_tool("git_commit"));
        assert!(McpProfile::Standard.allows_tool("git_review_thread_deliver"));
        assert!(McpProfile::Standard.allows_tool("git_review_comment_delete"));
        assert!(!McpProfile::Standard.allows_tool("git_stage_all"));
        assert!(!McpProfile::Standard.allows_tool("git_discard"));
        assert!(!McpProfile::Standard.allows_tool("workspace_close"));
        assert!(!McpProfile::Standard.allows_tool("agent_team_release"));

        assert!(McpProfile::Full.allows_tool("workspace_list"));
        assert!(McpProfile::Full.allows_tool("terminal_open"));
        assert!(McpProfile::Full.allows_tool("git_commit"));
        assert!(McpProfile::Full.allows_tool("agent_worktree_remove"));
        assert!(McpProfile::Full.allows_tool("workspace_close"));
        assert!(McpProfile::Full.allows_tool("agent_team_release"));
        assert!(!McpProfile::Full.allows_tool("unclassified_future_tool"));
    }

    #[tokio::test]
    async fn standard_git_mutation_is_bound_to_the_active_workspace_and_pane() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "system.identify",
                    json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-1",
                        "surface_id": "surface-1",
                        "session_id": "session-agent",
                    }),
                ),
                (
                    METHOD_GIT_STATUS_SUMMARY,
                    json!({
                        "workspace_id": "workspace-1",
                        "repository_id": "repo-1",
                    }),
                ),
                (METHOD_GIT_STAGE, json!({"generation": 2})),
            ],
        );
        let result = server
            .git_stage(Parameters(GitPathMutationToolParams {
                workspace_id: "workspace-1".to_string(),
                pane_id: None,
                repository_id: Some("repo-1".to_string()),
                paths: vec!["src/lib.rs".to_string()],
                idempotency_key: Some("stage-1".to_string()),
            }))
            .await;
        assert_eq!(result.is_error, Some(false));
        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].0, "system.identify");
        assert_eq!(calls[1].0, METHOD_GIT_STATUS_SUMMARY);
        assert_eq!(calls[1].1["pane_id"], "pane-1");
        assert_eq!(calls[2].0, METHOD_GIT_STAGE);
        assert_eq!(calls[2].1["workspace_id"], "workspace-1");
        assert_eq!(calls[2].1["pane_id"], "pane-1");
        assert_eq!(calls[2].1["repository_id"], "repo-1");
    }

    #[tokio::test]
    async fn standard_git_mutation_rejects_cross_workspace_and_unrelated_pane_targets() {
        for (workspace_id, pane_id, expected) in [
            ("workspace-2", Some("pane-1"), "active workspace"),
            ("workspace-1", Some("pane-2"), "active pane"),
        ] {
            let (server, transport) = test_server_for_profile(
                McpProfile::Standard,
                [(
                    "system.identify",
                    json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-1",
                        "surface_id": "surface-1",
                        "session_id": "session-agent",
                    }),
                )],
            );
            let result = server
                .git_unstage(Parameters(GitPathMutationToolParams {
                    workspace_id: workspace_id.to_string(),
                    pane_id: pane_id.map(ToString::to_string),
                    repository_id: None,
                    paths: vec!["src/lib.rs".to_string()],
                    idempotency_key: None,
                }))
                .await;
            assert_eq!(result.is_error, Some(true));
            let message = result
                .structured_content
                .as_ref()
                .and_then(|value| value.pointer("/error/message"))
                .and_then(Value::as_str)
                .expect("scope denial message");
            assert!(message.contains("requires the 'full' MCP profile"));
            assert!(message.contains(expected));
            let calls = transport.calls.lock().expect("fake calls lock");
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].0, "system.identify");
        }
    }

    #[tokio::test]
    async fn standard_git_mutation_rejects_unrelated_repository_target() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "system.identify",
                    json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-1",
                        "surface_id": "surface-1",
                        "session_id": "session-agent",
                    }),
                ),
                (
                    METHOD_GIT_STATUS_SUMMARY,
                    json!({
                        "workspace_id": "workspace-1",
                        "repository_id": "repo-active",
                    }),
                ),
            ],
        );
        let result = server
            .git_stage(Parameters(GitPathMutationToolParams {
                workspace_id: "workspace-1".to_string(),
                pane_id: Some("pane-1".to_string()),
                repository_id: Some("repo-other".to_string()),
                paths: vec!["src/lib.rs".to_string()],
                idempotency_key: None,
            }))
            .await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.structured_content.as_ref().is_some_and(|value| value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .is_some_and(|message| message.contains("not selected by the caller's active pane"))));
        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].0, METHOD_GIT_STATUS_SUMMARY);
    }

    #[tokio::test]
    async fn standard_commit_allows_active_repository_but_rejects_amend() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "system.identify",
                    json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-1",
                        "surface_id": "surface-1",
                        "session_id": "session-agent",
                    }),
                ),
                (
                    METHOD_GIT_STATUS_SUMMARY,
                    json!({
                        "workspace_id": "workspace-1",
                        "repository_id": "repo-1",
                    }),
                ),
                (METHOD_GIT_COMMIT, json!({"commit_oid": "abc123"})),
            ],
        );
        let result = server
            .git_commit(Parameters(GitCommitToolParams {
                workspace_id: "workspace-1".to_string(),
                pane_id: Some("pane-1".to_string()),
                repository_id: Some("repo-1".to_string()),
                message: "Agent-owned change".to_string(),
                amend: false,
                idempotency_key: None,
            }))
            .await;
        assert_eq!(result.is_error, Some(false));
        assert_eq!(transport.calls.lock().expect("fake calls lock").len(), 3);

        let (server, transport) = test_server_for_profile(McpProfile::Standard, []);
        let denied = server
            .git_commit(Parameters(GitCommitToolParams {
                workspace_id: "workspace-1".to_string(),
                pane_id: Some("pane-1".to_string()),
                repository_id: None,
                message: "Rewrite history".to_string(),
                amend: true,
                idempotency_key: None,
            }))
            .await;
        assert_eq!(denied.is_error, Some(true));
        assert!(denied.structured_content.as_ref().is_some_and(|value| value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .is_some_and(|message| message.contains("requires the 'full' MCP profile"))));
        assert!(transport.calls.lock().expect("fake calls lock").is_empty());
    }

    #[tokio::test]
    async fn standard_review_create_binds_workspace_repository_and_author_to_caller() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Standard,
            [
                ("system.identify", standard_caller_context()),
                (METHOD_GIT_STATUS_SUMMARY, standard_active_repository()),
                (
                    METHOD_GIT_REVIEW_THREAD_CREATE,
                    json!({"thread_id": "review-owned"}),
                ),
            ],
        );
        let result = server
            .git_review_thread_create(Parameters(GitReviewThreadCreateToolParams {
                workspace_id: "workspace-1".to_string(),
                pane_id: None,
                repository_id: None,
                anchor: GitReviewLineAnchorToolParams {
                    path: "src/lib.rs".to_string(),
                    side: "new".to_string(),
                    line: 42,
                    start_line: None,
                    base_revision: None,
                    head_revision: None,
                    hunk_header: None,
                    diff_hash: None,
                },
                body: "Caller-owned review".to_string(),
                author_session_id: None,
            }))
            .await;
        assert_eq!(result.is_error, Some(false));
        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].0, "system.identify");
        assert_eq!(calls[0].1["workspace_id"], "workspace-1");
        assert_eq!(calls[0].1["pane_id"], "pane-1");
        assert_eq!(calls[0].1["surface_id"], "surface-1");
        assert_eq!(calls[2].0, METHOD_GIT_REVIEW_THREAD_CREATE);
        assert_eq!(calls[2].1["workspace_id"], "workspace-1");
        assert_eq!(calls[2].1["pane_id"], "pane-1");
        assert_eq!(calls[2].1["repository_id"], "repo-1");
        assert_eq!(calls[2].1["author_session_id"], "session-agent");
    }

    #[tokio::test]
    async fn standard_review_list_binds_repository_lookup_to_caller_pane() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Standard,
            [
                ("system.identify", standard_caller_context()),
                (METHOD_GIT_STATUS_SUMMARY, standard_active_repository()),
                (METHOD_GIT_REVIEW_THREAD_LIST, json!({ "threads": [] })),
            ],
        );
        let result = server
            .git_review_thread_list(Parameters(GitReviewThreadListToolParams {
                workspace_id: "workspace-1".to_string(),
                pane_id: None,
                repository_id: None,
                path: None,
                include_resolved: true,
                include_stale: true,
                limit: Some(25),
            }))
            .await;
        assert_eq!(result.is_error, Some(false));
        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[2].0, METHOD_GIT_REVIEW_THREAD_LIST);
        assert_eq!(calls[2].1["workspace_id"], "workspace-1");
        assert_eq!(calls[2].1["pane_id"], "pane-1");
        assert_eq!(calls[2].1["repository_id"], "repo-1");
    }

    #[tokio::test]
    async fn standard_review_create_rejects_forged_author_and_foreign_scope() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Standard,
            [
                ("system.identify", standard_caller_context()),
                (METHOD_GIT_STATUS_SUMMARY, standard_active_repository()),
            ],
        );
        let forged = server
            .git_review_thread_create(Parameters(GitReviewThreadCreateToolParams {
                workspace_id: "workspace-1".to_string(),
                pane_id: None,
                repository_id: Some("repo-1".to_string()),
                anchor: GitReviewLineAnchorToolParams {
                    path: "src/lib.rs".to_string(),
                    side: "new".to_string(),
                    line: 1,
                    start_line: None,
                    base_revision: None,
                    head_revision: None,
                    hunk_header: None,
                    diff_hash: None,
                },
                body: "Forged author".to_string(),
                author_session_id: Some("session-peer".to_string()),
            }))
            .await;
        assert_eq!(forged.is_error, Some(true));
        assert!(forged.structured_content.as_ref().is_some_and(|value| value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .is_some_and(|message| message.contains("requested author"))));
        assert_eq!(transport.calls.lock().expect("fake calls lock").len(), 2);

        for (workspace_id, repository_id, expected) in [
            ("workspace-foreign", None, "active workspace"),
            ("workspace-1", Some("repo-foreign"), "active pane"),
        ] {
            let (server, transport) = test_server_for_profile(
                McpProfile::Standard,
                [
                    ("system.identify", standard_caller_context()),
                    (METHOD_GIT_STATUS_SUMMARY, standard_active_repository()),
                ],
            );
            let result = server
                .git_review_thread_create(Parameters(GitReviewThreadCreateToolParams {
                    workspace_id: workspace_id.to_string(),
                    pane_id: None,
                    repository_id: repository_id.map(ToString::to_string),
                    anchor: GitReviewLineAnchorToolParams {
                        path: "src/lib.rs".to_string(),
                        side: "new".to_string(),
                        line: 1,
                        start_line: None,
                        base_revision: None,
                        head_revision: None,
                        hunk_header: None,
                        diff_hash: None,
                    },
                    body: "Foreign scope".to_string(),
                    author_session_id: None,
                }))
                .await;
            assert_eq!(result.is_error, Some(true));
            assert!(result.structured_content.as_ref().is_some_and(|value| value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .is_some_and(|message| message.contains(expected))));
            assert!(transport
                .calls
                .lock()
                .expect("fake calls lock")
                .iter()
                .all(|(method, _)| method != METHOD_GIT_REVIEW_THREAD_CREATE));
        }
    }

    #[tokio::test]
    async fn standard_review_authority_does_not_follow_pane_focus() {
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [
                ("system.identify", Ok(standard_caller_context())),
                ("pane.focus", Ok(json!({"focused": true}))),
                (
                    "system.identify",
                    Ok(json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-peer",
                        "surface_id": "surface-peer",
                        "session_id": "session-peer",
                    })),
                ),
            ],
        );

        let focused = server
            .pane_focus(Parameters(PaneFocusToolParams {
                workspace_id: Some("workspace-1".to_string()),
                pane_id: Some("pane-peer".to_string()),
            }))
            .await;
        assert_eq!(focused.is_error, Some(false));

        let denied = server
            .git_review_thread_update(Parameters(GitReviewThreadUpdateToolParams {
                thread_id: "review-peer".to_string(),
                resolved: Some(true),
                anchor: None,
            }))
            .await;
        assert_eq!(denied.is_error, Some(true));
        assert!(denied.structured_content.as_ref().is_some_and(|value| value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .is_some_and(|message| message.contains("does not match the MCP connection binding"))));

        let calls = transport.calls.lock().expect("scripted calls lock");
        assert_eq!(calls[1].0, "pane.focus");
        assert_eq!(calls[2].0, "system.identify");
        assert_eq!(calls[2].1["workspace_id"], "workspace-1");
        assert_eq!(calls[2].1["pane_id"], "pane-1");
        assert_eq!(calls[2].1["surface_id"], "surface-1");
        assert!(calls
            .iter()
            .all(|(method, _)| method != METHOD_GIT_REVIEW_THREAD_UPDATE));
    }

    #[tokio::test]
    async fn standard_review_mutation_without_verified_pane_binding_fails_closed() {
        let transport = Arc::new(FakeTransport {
            responses: HashMap::new(),
            calls: Mutex::new(Vec::new()),
        });
        let server = AgentMuxMcpServer::with_transport(
            McpProfile::Standard,
            Arc::clone(&transport) as Arc<dyn ControlTransport>,
        );

        let denied = server
            .git_review_thread_update(Parameters(GitReviewThreadUpdateToolParams {
                thread_id: "review-forged".to_string(),
                resolved: Some(true),
                anchor: None,
            }))
            .await;
        assert_eq!(denied.is_error, Some(true));
        assert!(denied.structured_content.as_ref().is_some_and(|value| value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .is_some_and(|message| message.contains("not bound to a verifiable AgentMux pane"))));
        assert!(transport.calls.lock().expect("fake calls lock").is_empty());
    }

    #[tokio::test]
    async fn standard_review_mutations_allow_only_caller_owned_threads_and_comments() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Standard,
            [
                ("system.identify", standard_caller_context()),
                (METHOD_GIT_STATUS_SUMMARY, standard_active_repository()),
                (
                    METHOD_GIT_REVIEW_THREAD_LIST,
                    standard_owned_review_threads(),
                ),
                (
                    METHOD_GIT_REVIEW_THREAD_UPDATE,
                    json!({"thread_id": "review-owned"}),
                ),
                (
                    METHOD_GIT_REVIEW_THREAD_MARK_STALE,
                    json!({"thread_id": "review-owned"}),
                ),
                (
                    METHOD_GIT_REVIEW_COMMENT_CREATE,
                    json!({"comment_id": "comment-new"}),
                ),
                (
                    METHOD_GIT_REVIEW_COMMENT_UPDATE,
                    json!({"comment_id": "comment-owned"}),
                ),
                (
                    METHOD_GIT_REVIEW_COMMENT_DELETE,
                    json!({"comment_id": "comment-owned"}),
                ),
            ],
        );

        let update = server
            .git_review_thread_update(Parameters(GitReviewThreadUpdateToolParams {
                thread_id: "review-owned".to_string(),
                resolved: Some(true),
                anchor: None,
            }))
            .await;
        let stale = server
            .git_review_thread_mark_stale(Parameters(GitReviewThreadMarkStaleToolParams {
                thread_id: "review-owned".to_string(),
                stale: true,
                reason: Some("head changed".to_string()),
            }))
            .await;
        let create = server
            .git_review_comment_create(Parameters(GitReviewCommentCreateToolParams {
                thread_id: "review-owned".to_string(),
                body: "Follow-up".to_string(),
                author_session_id: None,
            }))
            .await;
        let update_comment = server
            .git_review_comment_update(Parameters(GitReviewCommentUpdateToolParams {
                comment_id: "comment-owned".to_string(),
                body: "Updated".to_string(),
            }))
            .await;
        let delete_comment = server
            .git_review_comment_delete(Parameters(GitReviewCommentIdToolParams {
                comment_id: "comment-owned".to_string(),
            }))
            .await;

        for result in [update, stale, create, update_comment, delete_comment] {
            assert_eq!(result.is_error, Some(false));
        }
        let calls = transport.calls.lock().expect("fake calls lock");
        let create_call = calls
            .iter()
            .find(|(method, _)| method == METHOD_GIT_REVIEW_COMMENT_CREATE)
            .expect("comment create call");
        assert_eq!(create_call.1["author_session_id"], "session-agent");
        assert!(calls
            .iter()
            .any(|(method, _)| method == METHOD_GIT_REVIEW_COMMENT_DELETE));
    }

    #[tokio::test]
    async fn standard_review_mutations_reject_foreign_thread_repository_and_comment() {
        let foreign_repository_threads = json!({
            "threads": [{
                "thread_id": "review-foreign-repo",
                "workspace_id": "workspace-1",
                "repository_id": "repo-foreign",
                "comments": [{
                    "comment_id": "comment-foreign-repo",
                    "author_session_id": "session-agent"
                }],
            }],
        });
        let foreign_owner_threads = json!({
            "threads": [{
                "thread_id": "review-peer",
                "workspace_id": "workspace-1",
                "repository_id": "repo-1",
                "comments": [{
                    "comment_id": "comment-peer-root",
                    "author_session_id": "session-peer"
                }],
            }],
        });
        for (thread_id, threads, expected) in [
            (
                "review-missing",
                standard_owned_review_threads(),
                "outside the caller's active repository",
            ),
            (
                "review-foreign-repo",
                foreign_repository_threads,
                "active workspace or repository",
            ),
            (
                "review-peer",
                foreign_owner_threads,
                "not created by the caller",
            ),
        ] {
            let (server, transport) = test_server_for_profile(
                McpProfile::Standard,
                [
                    ("system.identify", standard_caller_context()),
                    (METHOD_GIT_STATUS_SUMMARY, standard_active_repository()),
                    (METHOD_GIT_REVIEW_THREAD_LIST, threads),
                ],
            );
            let result = server
                .git_review_thread_update(Parameters(GitReviewThreadUpdateToolParams {
                    thread_id: thread_id.to_string(),
                    resolved: Some(true),
                    anchor: None,
                }))
                .await;
            assert_eq!(result.is_error, Some(true));
            assert!(result.structured_content.as_ref().is_some_and(|value| value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .is_some_and(|message| message.contains(expected))));
            assert_eq!(transport.calls.lock().expect("fake calls lock").len(), 3);
        }

        for tool in ["update", "delete"] {
            let (server, transport) = test_server_for_profile(
                McpProfile::Standard,
                [
                    ("system.identify", standard_caller_context()),
                    (METHOD_GIT_STATUS_SUMMARY, standard_active_repository()),
                    (
                        METHOD_GIT_REVIEW_THREAD_LIST,
                        standard_owned_review_threads(),
                    ),
                ],
            );
            let result = if tool == "update" {
                server
                    .git_review_comment_update(Parameters(GitReviewCommentUpdateToolParams {
                        comment_id: "comment-peer".to_string(),
                        body: "Forged edit".to_string(),
                    }))
                    .await
            } else {
                server
                    .git_review_comment_delete(Parameters(GitReviewCommentIdToolParams {
                        comment_id: "comment-peer".to_string(),
                    }))
                    .await
            };
            assert_eq!(result.is_error, Some(true));
            assert!(result.structured_content.as_ref().is_some_and(|value| value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .is_some_and(|message| message.contains("not authored by the caller"))));
            assert_eq!(transport.calls.lock().expect("fake calls lock").len(), 3);
        }

        let (server, transport) = test_server_for_profile(
            McpProfile::Standard,
            [
                ("system.identify", standard_caller_context()),
                (METHOD_GIT_STATUS_SUMMARY, standard_active_repository()),
                (
                    METHOD_GIT_REVIEW_THREAD_LIST,
                    standard_owned_review_threads(),
                ),
            ],
        );
        let forged_comment = server
            .git_review_comment_create(Parameters(GitReviewCommentCreateToolParams {
                thread_id: "review-owned".to_string(),
                body: "Forged author".to_string(),
                author_session_id: Some("session-peer".to_string()),
            }))
            .await;
        assert_eq!(forged_comment.is_error, Some(true));
        assert!(forged_comment
            .structured_content
            .as_ref()
            .is_some_and(|value| value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .is_some_and(|message| message.contains("requested author"))));
        assert_eq!(transport.calls.lock().expect("fake calls lock").len(), 3);
    }

    #[tokio::test]
    async fn full_review_mutations_preserve_cross_workspace_and_author_authority() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Full,
            [
                (
                    METHOD_GIT_REVIEW_THREAD_CREATE,
                    json!({"thread_id": "review-admin"}),
                ),
                (
                    METHOD_GIT_REVIEW_THREAD_UPDATE,
                    json!({"thread_id": "review-foreign"}),
                ),
                (
                    METHOD_GIT_REVIEW_COMMENT_DELETE,
                    json!({"comment_id": "comment-foreign"}),
                ),
            ],
        );
        let result = server
            .git_review_thread_create(Parameters(GitReviewThreadCreateToolParams {
                workspace_id: "workspace-foreign".to_string(),
                pane_id: None,
                repository_id: Some("repo-foreign".to_string()),
                anchor: GitReviewLineAnchorToolParams {
                    path: "src/admin.rs".to_string(),
                    side: "new".to_string(),
                    line: 9,
                    start_line: None,
                    base_revision: None,
                    head_revision: None,
                    hunk_header: None,
                    diff_hash: None,
                },
                body: "Administrative review".to_string(),
                author_session_id: Some("session-foreign".to_string()),
            }))
            .await;
        assert_eq!(result.is_error, Some(false));
        let update = server
            .git_review_thread_update(Parameters(GitReviewThreadUpdateToolParams {
                thread_id: "review-foreign".to_string(),
                resolved: Some(true),
                anchor: None,
            }))
            .await;
        assert_eq!(update.is_error, Some(false));
        let delete = server
            .git_review_comment_delete(Parameters(GitReviewCommentIdToolParams {
                comment_id: "comment-foreign".to_string(),
            }))
            .await;
        assert_eq!(delete.is_error, Some(false));
        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].0, METHOD_GIT_REVIEW_THREAD_CREATE);
        assert_eq!(calls[0].1["workspace_id"], "workspace-foreign");
        assert_eq!(calls[0].1["repository_id"], "repo-foreign");
        assert_eq!(calls[0].1["author_session_id"], "session-foreign");
        assert_eq!(calls[1].0, METHOD_GIT_REVIEW_THREAD_UPDATE);
        assert_eq!(calls[2].0, METHOD_GIT_REVIEW_COMMENT_DELETE);
    }

    #[tokio::test]
    async fn standard_review_delivery_requires_agent_owned_thread_and_workspace_target() {
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "system.identify",
                    Ok(json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-1",
                        "surface_id": "surface-1",
                        "session_id": "session-agent",
                    })),
                ),
                (
                    METHOD_GIT_STATUS_SUMMARY,
                    Ok(json!({
                        "workspace_id": "workspace-1",
                        "repository_id": "repo-1",
                    })),
                ),
                (
                    METHOD_GIT_REVIEW_THREAD_LIST,
                    Ok(json!({
                        "threads": [{
                            "thread_id": "review-1",
                            "workspace_id": "workspace-1",
                            "repository_id": "repo-1",
                            "comments": [{"author_session_id": "session-agent"}],
                        }],
                    })),
                ),
                (
                    "session.list",
                    Ok(json!({
                        "sessions": [{"session_id": "session-worker"}],
                    })),
                ),
                (
                    METHOD_GIT_REVIEW_THREAD_DELIVER,
                    Ok(json!({
                        "thread_id": "review-1",
                        "target_session_id": "session-worker",
                    })),
                ),
            ],
        );
        let result = server
            .git_review_thread_deliver(Parameters(GitReviewThreadDeliverToolParams {
                thread_id: "review-1".to_string(),
                target: "mailbox".to_string(),
                target_session_id: Some("session-worker".to_string()),
                include_context: true,
            }))
            .await;
        assert_eq!(result.is_error, Some(false));
        let calls = transport.calls.lock().expect("scripted calls lock");
        assert_eq!(calls.len(), 5);
        assert_eq!(calls[1].0, METHOD_GIT_STATUS_SUMMARY);
        assert_eq!(calls[1].1["pane_id"], "pane-1");
        assert_eq!(calls[2].1["repository_id"], "repo-1");
        assert_eq!(calls[3].1["workspace_id"], "workspace-1");
        assert_eq!(calls[4].0, METHOD_GIT_REVIEW_THREAD_DELIVER);
    }

    #[tokio::test]
    async fn standard_review_delivery_rejects_unrelated_thread_owner() {
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "system.identify",
                    Ok(json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-1",
                        "surface_id": "surface-1",
                        "session_id": "session-agent",
                    })),
                ),
                (
                    METHOD_GIT_STATUS_SUMMARY,
                    Ok(json!({
                        "workspace_id": "workspace-1",
                        "repository_id": "repo-1",
                    })),
                ),
                (
                    METHOD_GIT_REVIEW_THREAD_LIST,
                    Ok(json!({
                        "threads": [{
                            "thread_id": "review-1",
                            "workspace_id": "workspace-1",
                            "repository_id": "repo-1",
                            "comments": [{"author_session_id": "session-other"}],
                        }],
                    })),
                ),
            ],
        );
        let result = server
            .git_review_thread_deliver(Parameters(GitReviewThreadDeliverToolParams {
                thread_id: "review-1".to_string(),
                target: "terminal".to_string(),
                target_session_id: Some("session-worker".to_string()),
                include_context: false,
            }))
            .await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.structured_content.as_ref().is_some_and(|value| value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .is_some_and(|message| message.contains("not created by the caller"))));
        assert_eq!(
            transport.calls.lock().expect("scripted calls lock").len(),
            3
        );
    }

    #[tokio::test]
    async fn standard_review_delivery_rejects_cross_workspace_target() {
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "system.identify",
                    Ok(json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-1",
                        "surface_id": "surface-1",
                        "session_id": "session-agent",
                    })),
                ),
                (
                    METHOD_GIT_STATUS_SUMMARY,
                    Ok(json!({
                        "workspace_id": "workspace-1",
                        "repository_id": "repo-1",
                    })),
                ),
                (
                    METHOD_GIT_REVIEW_THREAD_LIST,
                    Ok(json!({
                        "threads": [{
                            "thread_id": "review-1",
                            "workspace_id": "workspace-1",
                            "repository_id": "repo-1",
                            "comments": [{"author_session_id": "session-agent"}],
                        }],
                    })),
                ),
                (
                    "session.list",
                    Ok(json!({
                        "sessions": [{"session_id": "session-local"}],
                    })),
                ),
            ],
        );
        let result = server
            .git_review_thread_deliver(Parameters(GitReviewThreadDeliverToolParams {
                thread_id: "review-1".to_string(),
                target: "mailbox".to_string(),
                target_session_id: Some("session-foreign".to_string()),
                include_context: false,
            }))
            .await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.structured_content.as_ref().is_some_and(|value| {
            value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .is_some_and(|message| {
                    message.contains("not owned by the caller's active workspace")
                })
        }));
        assert_eq!(
            transport.calls.lock().expect("scripted calls lock").len(),
            4
        );
    }

    #[tokio::test]
    async fn full_git_commit_preserves_cross_workspace_and_amend_authority() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Full,
            [(METHOD_GIT_COMMIT, json!({"commit_oid": "abc123"}))],
        );
        let result = server
            .git_commit(Parameters(GitCommitToolParams {
                workspace_id: "workspace-admin".to_string(),
                pane_id: Some("pane-admin".to_string()),
                repository_id: Some("repo-admin".to_string()),
                message: "Administrative amend".to_string(),
                amend: true,
                idempotency_key: None,
            }))
            .await;
        assert_eq!(result.is_error, Some(false));
        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, METHOD_GIT_COMMIT);
        assert_eq!(calls[0].1["workspace_id"], "workspace-admin");
        assert_eq!(calls[0].1["pane_id"], "pane-admin");
        assert_eq!(calls[0].1["amend"], true);
    }

    #[test]
    fn command_and_external_interaction_tools_are_marked_open_world() {
        let (full, _) = test_server_for_profile(McpProfile::Full, []);
        for name in [
            "terminal_open",
            "terminal_split",
            "terminal_send_text",
            "terminal_send_key",
            "browser_open",
            "browser_open_split",
            "browser_navigate",
            "browser_click",
            "browser_fill",
            "browser_evaluate",
            "action_run",
            "agent_worker_start",
            "agent_team_start",
            "agent_worker_send",
            "agent_worktree_create",
            "agent_worktree_recover",
            "development_server_candidate_open_in_split",
            "git_review_thread_deliver",
        ] {
            let tool = full
                .tool_router
                .list_all()
                .into_iter()
                .find(|tool| tool.name.as_ref() == name)
                .unwrap_or_else(|| panic!("missing tool {name}"));
            assert_eq!(
                tool.annotations
                    .as_ref()
                    .and_then(|annotations| annotations.open_world_hint),
                Some(true),
                "{name} can execute commands or interact with external entities"
            );
        }
    }

    #[test]
    fn agent_worker_start_schema_exposes_only_explicit_worker_kinds() {
        let schema = schemars::schema_for!(AgentWorkerStartToolParams);
        let value = serde_json::to_value(schema).expect("worker start schema");
        assert_eq!(
            value.pointer("/$defs/AgentWorkerKindParam/enum"),
            Some(&json!(["codex-pane", "claude-teams", "omo", "omx", "omc"]))
        );
    }

    #[test]
    fn agent_team_start_schema_exposes_layout_and_runtime_bounds() {
        let schema = schemars::schema_for!(AgentTeamStartToolParams);
        let value = serde_json::to_value(schema).expect("agent team start schema");
        assert_eq!(
            value.pointer("/$defs/AgentTeamLayoutParam/enum"),
            Some(&json!(["main-left-workers-right"]))
        );
        assert_eq!(
            value.pointer("/properties/main_ratio/minimum"),
            Some(&json!(0.1))
        );
        assert_eq!(
            value.pointer("/properties/main_ratio/maximum"),
            Some(&json!(0.9))
        );
        assert_eq!(value.pointer("/properties/workers/minItems"), None);
        assert_eq!(
            value.pointer("/properties/workers/maxItems"),
            Some(&json!(8))
        );
        assert_eq!(
            value.pointer("/$defs/AgentTeamWorkerSpec/properties/name/minLength"),
            Some(&json!(1))
        );
        assert_eq!(
            value.pointer("/$defs/AgentTeamWorkerSpec/properties/name/maxLength"),
            Some(&json!(64))
        );
    }

    #[tokio::test]
    async fn diagnostics_summary_includes_bounded_control_audit() {
        let (server, transport) = test_server([
            (
                "diagnostics.export",
                json!({
                    "generated_at": "now",
                    "format_version": 1,
                    "recovery": {},
                    "backend_health": {},
                    "queue_pressure": {},
                    "output_stream": {}
                }),
            ),
            (
                "diagnostics.control_audit",
                json!({ "records": [{ "method": "terminal.split", "source": "mcp" }] }),
            ),
        ]);
        let result = server.diagnostics_summary().await;
        assert_eq!(result.is_error, Some(false));
        assert_eq!(
            result.structured_content.unwrap()["control_audit"][0]["method"],
            "terminal.split"
        );
        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls[1].0, "diagnostics.control_audit");
        assert_eq!(calls[1].1["limit"], 100);
    }

    #[tokio::test]
    async fn terminal_read_clamps_the_requested_buffer() {
        let (server, transport) = test_server([("session.read_recent", json!({"text": "ok"}))]);
        let result = server
            .terminal_read(Parameters(TerminalReadParams {
                session_id: "session-1".to_string(),
                max_bytes: Some(usize::MAX),
            }))
            .await;
        assert_eq!(result.is_error, Some(false));
        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "session.read_recent");
        assert_eq!(calls[0].1["max_bytes"], MAX_TERMINAL_READ_BYTES);
    }

    #[tokio::test]
    async fn git_status_page_validates_and_delegates_the_shared_ipc_contract() {
        let (server, transport) = test_server([(
            METHOD_GIT_STATUS_PAGE,
            json!({"workspace_id": "workspace-1", "changes": []}),
        )]);
        let result = server
            .git_status_page(Parameters(GitStatusPageToolParams {
                workspace_id: "workspace-1".to_string(),
                pane_id: Some("pane-1".to_string()),
                repository_id: Some("repo-1".to_string()),
                state: Some("unstaged".to_string()),
                query: Some("src/".to_string()),
                cursor: Some("cursor-1".to_string()),
                limit: Some(100),
                generation: Some(7),
            }))
            .await;
        assert_eq!(result.is_error, Some(false));
        {
            let calls = transport.calls.lock().expect("fake calls lock");
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].0, METHOD_GIT_STATUS_PAGE);
            assert_eq!(calls[0].1["pane_id"], "pane-1");
            assert_eq!(calls[0].1["repository_id"], "repo-1");
            assert_eq!(calls[0].1["query"], "src/");
            assert_eq!(calls[0].1["generation"], 7);
        }

        let (server, transport) = test_server([]);
        let result = server
            .git_status_page(Parameters(GitStatusPageToolParams {
                workspace_id: " ".to_string(),
                pane_id: None,
                repository_id: None,
                state: None,
                query: None,
                cursor: None,
                limit: Some(501),
                generation: None,
            }))
            .await;
        assert_eq!(result.is_error, Some(true));
        assert_eq!(transport.calls.lock().expect("fake calls lock").len(), 0);
    }

    #[tokio::test]
    async fn worktree_and_review_tools_delegate_to_the_exact_ipc_methods() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Full,
            [
                (
                    METHOD_AGENT_WORKTREE_CREATE,
                    json!({"operation_id": "op-1"}),
                ),
                (
                    METHOD_GIT_REVIEW_THREAD_CREATE,
                    json!({"thread_id": "review-1"}),
                ),
            ],
        );
        let worktree = server
            .agent_worktree_create(Parameters(AgentWorktreeCreateToolParams {
                workspace_id: "workspace-1".to_string(),
                branch: "agent/review".to_string(),
                destination: "D:\\worktrees\\review".to_string(),
                base_revision: Some("HEAD".to_string()),
                create_branch: true,
                backend: Some("wsl-direct".to_string()),
                backend_profile: None,
                command: vec!["codex".to_string()],
                cwd: None,
                idempotency_key: "worktree-1".to_string(),
            }))
            .await;
        assert_eq!(worktree.is_error, Some(false));
        let review = server
            .git_review_thread_create(Parameters(GitReviewThreadCreateToolParams {
                workspace_id: "workspace-1".to_string(),
                pane_id: None,
                repository_id: None,
                anchor: GitReviewLineAnchorToolParams {
                    path: "src/lib.rs".to_string(),
                    side: "new".to_string(),
                    line: 42,
                    start_line: None,
                    base_revision: None,
                    head_revision: None,
                    hunk_header: None,
                    diff_hash: None,
                },
                body: "Please handle the error path.".to_string(),
                author_session_id: None,
            }))
            .await;
        assert_eq!(review.is_error, Some(false));
        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls[0].0, METHOD_AGENT_WORKTREE_CREATE);
        assert_eq!(calls[1].0, METHOD_GIT_REVIEW_THREAD_CREATE);
        assert_eq!(calls[1].1["anchor"]["line"], 42);
    }

    #[tokio::test]
    async fn agent_worker_list_filters_generic_agent_sessions() {
        let (server, _) = test_server([(
            "agent.list",
            json!({
                "sessions": [
                    {"session_id": "generic", "telemetry": {"activity": "agent", "session": "claude"}},
                    {"session_id": "codex", "telemetry": {"activity": "agent", "session": "codex-pane:worker"}},
                    {"session_id": "team", "telemetry": {"activity": "agent_team", "session": "claude-teams:worker"}},
                    {"session_id": "shell", "telemetry": {"activity": "terminal"}}
                ]
            }),
        )]);
        let result = server
            .agent_worker_list(Parameters(AgentWorkerListToolParams::default()))
            .await;
        let sessions = result.structured_content.unwrap()["sessions"]
            .as_array()
            .cloned()
            .expect("worker sessions");
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0]["session_id"], "codex");
        assert_eq!(sessions[1]["session_id"], "team");
    }

    #[tokio::test]
    async fn agent_integration_status_rejects_an_unknown_explicit_workspace() {
        let (server, transport) = test_server([]);
        let result = server
            .agent_integration_status(Parameters(AgentIntegrationStatusToolParams {
                workspace_id: Some("missing-workspace".to_string()),
                integration: Some("claude-teams".to_string()),
                distribution: None,
            }))
            .await;

        assert_eq!(result.is_error, Some(true));
        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "workspace.get");
        assert_eq!(calls[0].1["workspace_id"], "missing-workspace");
    }

    #[tokio::test]
    async fn agent_worker_start_splits_and_registers_typed_metadata() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "system.identify",
                    json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-source",
                        "cwd": "/workspace/project"
                    }),
                ),
                (
                    "terminal.split",
                    json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-worker",
                        "surface_id": "surface-worker",
                        "session_id": "session-worker"
                    }),
                ),
                ("agent.set_state", json!({"session_id": "session-worker"})),
            ],
        );
        let result = server
            .agent_worker_start(Parameters(AgentWorkerStartToolParams {
                workspace_id: None,
                pane_id: None,
                name: None,
                kind: AgentWorkerKindParam::ClaudeTeams,
                distribution: Some("Ubuntu".to_string()),
                placement: None,
                axis: None,
                ratio: Some(0.4),
                cwd: None,
                args: vec!["--model".to_string(), "opus".to_string()],
                columns: None,
                rows: None,
                durability: None,
            }))
            .await;
        assert_eq!(result.is_error, Some(false));
        let value = result.structured_content.expect("worker result");
        assert_eq!(value["kind"], "claude-teams");
        assert_eq!(value["controller"], "agentmux-tmux-compat");
        assert_eq!(value["session_id"], "session-worker");

        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].0, "system.identify");
        assert_eq!(calls[1].0, "terminal.split");
        assert_eq!(calls[1].1["workspace_id"], "workspace-1");
        assert_eq!(calls[1].1["pane_id"], "pane-source");
        assert_eq!(calls[1].1["behavior"], "clone_current");
        assert_eq!(calls[1].1["backend"], "wsl-direct");
        let command = calls[1].1["command"].as_array().expect("worker command");
        assert_eq!(command[1], "integrations");
        assert_eq!(command[2], "launch");
        assert_eq!(command[3], "claude-teams");
        assert_eq!(calls[2].0, "agent.set_state");
        assert_eq!(calls[2].1["telemetry"]["activity"], "agent_team");
        assert_eq!(calls[2].1["telemetry"]["session"], "claude-teams:worker");
    }

    #[tokio::test]
    async fn adaptive_agent_team_can_start_without_seed_workers() {
        let topology = json!({
            "panes": [
                {"pane_id": "pane-main", "kind": "leaf", "mounted_surface_id": "surface-main"}
            ],
            "surfaces": [
                {"surface_id": "surface-main", "session_id": "session-main"}
            ]
        });
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "system.identify",
                    Ok(json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-main",
                        "session_id": "session-main",
                        "cwd": "/workspace/project"
                    })),
                ),
                ("agent.list", Ok(json!({"sessions": []}))),
                ("workspace.get", Ok(topology.clone())),
                ("agent.list", Ok(json!({"sessions": []}))),
                ("agent.team.reserve", Ok(json!({"claimed": true}))),
                ("workspace.get", Ok(topology)),
                (
                    "agent.list",
                    Ok(json!({
                        "sessions": [{
                            "session_id": "session-main",
                            "workspace_id": "workspace-1",
                            "state": "running",
                            "telemetry": {"activity": "agent", "session": "codex"}
                        }]
                    })),
                ),
                (
                    "agent.team.settle",
                    Ok(json!({"session_id": "session-main"})),
                ),
            ],
        );
        let result = server
            .agent_team_start(Parameters(AgentTeamStartToolParams {
                workspace_id: None,
                pane_id: None,
                layout: None,
                main_ratio: None,
                mode: None,
                auto_adopt_tmux: None,
                idempotency_key: Some("project-analysis".to_string()),
                max_workers: Some(4),
                default_worker_kind: Some(AgentWorkerKindParam::CodexPane),
                distribution: None,
                cwd: None,
                columns: None,
                rows: None,
                durability: None,
                workers: Vec::new(),
            }))
            .await;

        assert_eq!(result.is_error, Some(false));
        let value = result.structured_content.expect("adaptive team result");
        assert_eq!(value["mode"], "adaptive");
        assert_eq!(value["worker_count"], 0);
        assert_eq!(value["max_workers"], 4);
        assert_eq!(value["generation"], 1);
        assert_eq!(value["status"], "ready");
        let calls = transport.calls.lock().expect("scripted calls lock");
        assert!(calls.iter().all(|(method, _)| method != "terminal.split"));
        assert_eq!(calls[4].0, "agent.team.reserve");
        assert_eq!(calls[4].1["claim"], true);
        let manifest = calls.last().expect("main manifest call");
        assert_eq!(manifest.0, "agent.team.settle");
        assert_eq!(manifest.1["telemetry"]["team_mode"], "adaptive");
        assert_eq!(
            manifest.1["telemetry"]["team_idempotency_key"],
            "project-analysis"
        );
    }

    #[tokio::test]
    async fn agent_team_spawn_rejects_a_stale_generation_before_mutation() {
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [(
                "agent.list",
                Ok(json!({
                    "sessions": [{
                        "session_id": "session-main",
                        "workspace_id": "workspace-1",
                        "state": "running",
                        "telemetry": {
                            "team_id": "team-1",
                            "team_role": "main",
                            "team_mode": "adaptive",
                            "layout_root_pane_id": "pane-main",
                            "team_generation": 3
                        }
                    }]
                })),
            )],
        );
        let result = server
            .agent_team_spawn(Parameters(AgentTeamSpawnToolParams {
                team_id: "team-1".to_string(),
                expected_generation: Some(2),
                idempotency_key: None,
                name: None,
                kind: None,
                distribution: None,
                cwd: None,
                args: Vec::new(),
                columns: None,
                rows: None,
                durability: None,
            }))
            .await;

        assert_eq!(result.is_error, Some(true));
        let value = result.structured_content.expect("generation conflict");
        assert_eq!(value["error"]["code"], "generation_conflict");
        assert_eq!(value["error"]["current_generation"], 3);
        let calls = transport.calls.lock().expect("scripted calls lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "agent.list");
    }

    #[tokio::test]
    async fn agent_team_spawn_reused_reservation_does_not_repeat_side_effects() {
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "agent.list",
                    Ok(json!({
                        "sessions": [{
                            "session_id": "session-main",
                            "workspace_id": "workspace-1",
                            "state": "running",
                            "telemetry": {
                                "team_id": "team-1",
                                "team_role": "main",
                                "team_mode": "adaptive",
                                "layout_root_pane_id": "pane-main",
                                "team_generation": 1,
                                "max_workers": 2,
                                "default_worker_kind": "codex-pane"
                            }
                        }]
                    })),
                ),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [{"pane_id": "pane-main", "kind": "leaf", "mounted_surface_id": "surface-main"}],
                        "surfaces": [{"surface_id": "surface-main", "session_id": "session-main"}]
                    })),
                ),
                (
                    "agent.team.reserve",
                    Ok(json!({
                        "generation": 2,
                        "mutation_id": "spawn:team-1:docs",
                        "reused": true,
                        "acquired": false
                    })),
                ),
            ],
        );

        let result = server
            .agent_team_spawn(Parameters(AgentTeamSpawnToolParams {
                team_id: "team-1".to_string(),
                expected_generation: Some(1),
                idempotency_key: Some("docs".to_string()),
                name: Some("docs".to_string()),
                kind: None,
                distribution: None,
                cwd: None,
                args: Vec::new(),
                columns: None,
                rows: None,
                durability: None,
            }))
            .await;

        assert_eq!(result.is_error, Some(false));
        let value = result.structured_content.expect("in-progress reservation");
        assert_eq!(value["status"], "provisioning");
        assert_eq!(value["reused"], true);
        let calls = transport.calls.lock().expect("scripted calls lock");
        assert_eq!(calls.len(), 3);
        assert!(calls.iter().all(|(method, _)| {
            method != "terminal.split"
                && method != "pane.resize_layout"
                && method != "agent.team.settle"
        }));
    }

    #[tokio::test]
    async fn agent_team_spawn_reuses_a_completed_idempotency_key_before_generation_check() {
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [(
                "agent.list",
                Ok(json!({
                    "sessions": [
                        {
                            "session_id": "session-main",
                            "workspace_id": "workspace-1",
                            "state": "running",
                            "telemetry": {
                                "team_id": "team-1",
                                "team_role": "main",
                                "team_mode": "adaptive",
                                "layout_root_pane_id": "pane-root",
                                "team_generation": 3
                            }
                        },
                        {
                            "session_id": "session-worker",
                            "workspace_id": "workspace-1",
                            "state": "running",
                            "telemetry": {
                                "team_id": "team-1",
                                "team_role": "worker",
                                "team_member_idempotency_key": "spawn-frontend"
                            }
                        }
                    ]
                })),
            )],
        );
        let result = server
            .agent_team_spawn(Parameters(AgentTeamSpawnToolParams {
                team_id: "team-1".to_string(),
                expected_generation: Some(2),
                idempotency_key: Some("spawn-frontend".to_string()),
                name: Some("frontend".to_string()),
                kind: None,
                distribution: None,
                cwd: None,
                args: Vec::new(),
                columns: None,
                rows: None,
                durability: None,
            }))
            .await;

        assert_eq!(result.is_error, Some(false));
        let value = result.structured_content.expect("reused worker");
        assert_eq!(value["reused"], true);
        assert_eq!(value["generation"], 3);
        let calls = transport.calls.lock().expect("scripted calls lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "agent.list");
    }

    #[tokio::test]
    async fn agent_team_spawn_reserves_then_registers_and_reflows_a_worker() {
        let initial = json!({
            "sessions": [{
                "session_id": "session-main",
                "workspace_id": "workspace-1",
                "state": "running",
                "telemetry": {
                    "team_id": "team-1",
                    "team_role": "main",
                    "team_mode": "adaptive",
                    "layout_root_pane_id": "pane-main",
                    "team_generation": 1,
                    "main_ratio": "0.5",
                    "max_workers": 2,
                    "default_worker_kind": "codex-pane"
                }
            }]
        });
        let refreshed = json!({
            "sessions": [
                {
                    "session_id": "session-main",
                    "workspace_id": "workspace-1",
                    "state": "running",
                    "telemetry": {
                        "team_id": "team-1",
                        "team_role": "main",
                        "team_mode": "adaptive",
                        "layout_root_pane_id": "pane-root",
                        "team_generation": 2,
                        "main_ratio": "0.5",
                        "max_workers": 2,
                        "default_worker_kind": "codex-pane"
                    }
                },
                {
                    "session_id": "session-worker",
                    "workspace_id": "workspace-1",
                    "state": "running",
                    "telemetry": {
                        "team_id": "team-1",
                        "team_role": "worker",
                        "worker_name": "backend",
                        "worker_index": 1,
                        "team_generation": 2
                    }
                }
            ]
        });
        let split_topology = json!({
            "panes": [
                {"pane_id": "pane-root", "kind": "split", "split_axis": "vertical", "split_ratio": 0.5},
                {"pane_id": "pane-main", "parent_pane_id": "pane-root", "kind": "leaf", "mounted_surface_id": "surface-main"},
                {"pane_id": "pane-worker", "parent_pane_id": "pane-root", "kind": "leaf", "mounted_surface_id": "surface-worker"}
            ],
            "surfaces": [
                {"surface_id": "surface-main", "session_id": "session-main"},
                {"surface_id": "surface-worker", "session_id": "session-worker"}
            ]
        });
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [
                ("agent.list", Ok(initial)),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [{"pane_id": "pane-main", "kind": "leaf", "mounted_surface_id": "surface-main"}],
                        "surfaces": [{"surface_id": "surface-main", "session_id": "session-main"}]
                    })),
                ),
                ("agent.team.reserve", Ok(json!({"generation": 2}))),
                (
                    "terminal.split",
                    Ok(json!({
                        "pane_id": "pane-worker-anchor",
                        "surface_id": "surface-worker",
                        "session_id": "session-worker"
                    })),
                ),
                ("workspace.get", Ok(split_topology.clone())),
                (
                    "agent.set_state",
                    Ok(json!({"session_id": "session-worker"})),
                ),
                ("agent.list", Ok(refreshed.clone())),
                ("agent.list", Ok(refreshed.clone())),
                ("workspace.get", Ok(split_topology)),
                ("agent.list", Ok(refreshed.clone())),
                ("pane.resize_layout", Ok(json!({"resized": true}))),
                ("agent.list", Ok(refreshed)),
                (
                    "agent.team.settle",
                    Ok(json!({"session_id": "session-main"})),
                ),
            ],
        );
        let result = server
            .agent_team_spawn(Parameters(AgentTeamSpawnToolParams {
                team_id: "team-1".to_string(),
                expected_generation: Some(1),
                idempotency_key: None,
                name: Some("backend".to_string()),
                kind: None,
                distribution: None,
                cwd: None,
                args: Vec::new(),
                columns: None,
                rows: None,
                durability: None,
            }))
            .await;

        assert_eq!(result.is_error, Some(false));
        let value = result.structured_content.expect("spawned worker");
        assert_eq!(value["generation"], 2);
        assert_eq!(value["worker_count"], 1);
        assert_eq!(value["reused"], false);
        let calls = transport.calls.lock().expect("scripted calls lock");
        assert_eq!(calls[2].0, "agent.team.reserve");
        assert_eq!(calls[3].0, "terminal.split");
        assert_eq!(calls[10].0, "pane.resize_layout");
    }

    #[tokio::test]
    async fn agent_team_spawn_counts_descendants_against_max_workers() {
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [(
                "agent.list",
                Ok(json!({
                    "sessions": [
                        {
                            "session_id": "session-main",
                            "workspace_id": "workspace-1",
                            "state": "running",
                            "telemetry": {
                                "team_id": "team-1",
                                "team_role": "main",
                                "team_mode": "adaptive",
                                "layout_root_pane_id": "pane-root",
                                "team_generation": 1,
                                "max_workers": 1
                            }
                        },
                        {
                            "session_id": "session-descendant",
                            "workspace_id": "workspace-1",
                            "state": "running",
                            "telemetry": {
                                "team_id": "team-1",
                                "team_role": "descendant",
                                "worker_name": "nested-worker"
                            }
                        }
                    ]
                })),
            )],
        );
        let result = server
            .agent_team_spawn(Parameters(AgentTeamSpawnToolParams {
                team_id: "team-1".to_string(),
                expected_generation: Some(1),
                idempotency_key: None,
                name: Some("another-worker".to_string()),
                kind: None,
                distribution: None,
                cwd: None,
                args: Vec::new(),
                columns: None,
                rows: None,
                durability: None,
            }))
            .await;

        assert_eq!(result.is_error, Some(true));
        let value = result.structured_content.expect("capacity error");
        assert!(value["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("all managed members")));
        let calls = transport.calls.lock().expect("scripted calls lock");
        assert_eq!(calls.len(), 1);
    }

    #[tokio::test]
    async fn agent_team_reflow_returns_conflict_when_layout_root_is_lost() {
        let team_state = json!({
            "sessions": [{
                "session_id": "session-main",
                "workspace_id": "workspace-1",
                "state": "running",
                "telemetry": {
                    "team_id": "team-1",
                    "team_role": "main",
                    "team_mode": "adaptive",
                    "layout_root_pane_id": "pane-root-lost",
                    "team_generation": 1
                }
            }]
        });
        let reserved_state = json!({
            "sessions": [{
                "session_id": "session-main",
                "workspace_id": "workspace-1",
                "state": "running",
                "telemetry": {
                    "team_id": "team-1",
                    "team_role": "main",
                    "team_mode": "adaptive",
                    "layout_root_pane_id": "pane-root-lost",
                    "team_generation": 2,
                    "team_status": "provisioning"
                }
            }]
        });
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [
                ("agent.list", Ok(team_state)),
                ("agent.team.reserve", Ok(json!({"generation": 2}))),
                ("agent.list", Ok(reserved_state.clone())),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [{"pane_id": "pane-main", "kind": "leaf", "mounted_surface_id": "surface-main"}],
                        "surfaces": [{"surface_id": "surface-main", "session_id": "session-main"}]
                    })),
                ),
                ("agent.list", Ok(reserved_state)),
                (
                    "agent.team.settle",
                    Ok(json!({"session_id": "session-main"})),
                ),
            ],
        );
        let result = server
            .agent_team_reflow(Parameters(AgentTeamIdToolParams {
                team_id: "team-1".to_string(),
                expected_generation: Some(1),
                dry_run: None,
            }))
            .await;

        assert_eq!(result.is_error, Some(false));
        let value = result.structured_content.expect("root conflict");
        assert_eq!(value["status"], "layout_dirty");
        assert_eq!(value["layout"]["status"], "layout_conflict");
        assert!(value["layout"]["conflicts"][0]
            .as_str()
            .is_some_and(|message| message.contains("no longer exists")));
        let calls = transport.calls.lock().expect("scripted calls lock");
        assert!(calls
            .iter()
            .all(|(method, _)| method != "pane.resize_layout"));
    }

    #[tokio::test]
    async fn agent_team_reflow_rejects_a_collapsed_root_with_managed_workers() {
        let state = json!({
            "sessions": [
                {
                    "session_id": "session-main",
                    "workspace_id": "workspace-1",
                    "state": "running",
                    "telemetry": {
                        "team_id": "team-1",
                        "team_role": "main",
                        "team_mode": "adaptive",
                        "layout_root_pane_id": "pane-main",
                        "team_generation": 1
                    }
                },
                {
                    "session_id": "session-worker",
                    "workspace_id": "workspace-1",
                    "state": "running",
                    "telemetry": {
                        "team_id": "team-1",
                        "team_role": "worker",
                        "worker_name": "worker-1"
                    }
                }
            ]
        });
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [
                ("agent.list", Ok(state.clone())),
                ("agent.list", Ok(state)),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [
                            {"pane_id": "pane-main", "kind": "leaf", "mounted_surface_id": "surface-main"},
                            {"pane_id": "pane-worker", "kind": "leaf", "mounted_surface_id": "surface-worker"}
                        ],
                        "surfaces": [
                            {"surface_id": "surface-main", "session_id": "session-main"},
                            {"surface_id": "surface-worker", "session_id": "session-worker"}
                        ]
                    })),
                ),
            ],
        );

        let result = server
            .agent_team_reflow(Parameters(AgentTeamIdToolParams {
                team_id: "team-1".to_string(),
                expected_generation: Some(1),
                dry_run: Some(true),
            }))
            .await;

        assert_eq!(result.is_error, Some(false));
        let value = result.structured_content.expect("collapsed-root conflict");
        assert_eq!(value["status"], "layout_conflict");
        assert!(value["conflicts"][0]
            .as_str()
            .is_some_and(|message| message.contains("collapsed")));
        let calls = transport.calls.lock().expect("scripted calls lock");
        assert!(calls
            .iter()
            .all(|(method, _)| method != "pane.resize_layout"));
    }

    #[tokio::test]
    async fn agent_team_reflow_rejects_a_managed_member_without_a_session() {
        let state = json!({
            "sessions": [
                {
                    "session_id": "session-main",
                    "workspace_id": "workspace-1",
                    "state": "running",
                    "telemetry": {
                        "team_id": "team-1",
                        "team_role": "main",
                        "team_mode": "adaptive",
                        "layout_root_pane_id": "pane-root",
                        "team_generation": 1
                    }
                },
                {
                    "workspace_id": "workspace-1",
                    "state": "running",
                    "telemetry": {
                        "team_id": "team-1",
                        "team_role": "worker",
                        "worker_name": "orphaned-worker"
                    }
                }
            ]
        });
        let (server, _) = scripted_server_for_profile(
            McpProfile::Standard,
            [
                ("agent.list", Ok(state.clone())),
                ("agent.list", Ok(state)),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [
                            {"pane_id": "pane-root", "kind": "split", "split_axis": "vertical", "split_ratio": 0.5},
                            {"pane_id": "pane-main", "parent_pane_id": "pane-root", "kind": "leaf", "mounted_surface_id": "surface-main"},
                            {"pane_id": "pane-worker", "parent_pane_id": "pane-root", "kind": "leaf", "mounted_surface_id": "surface-worker"}
                        ],
                        "surfaces": [
                            {"surface_id": "surface-main", "session_id": "session-main"},
                            {"surface_id": "surface-worker", "session_id": "unmanaged-session"}
                        ]
                    })),
                ),
            ],
        );

        let result = server
            .agent_team_reflow(Parameters(AgentTeamIdToolParams {
                team_id: "team-1".to_string(),
                expected_generation: Some(1),
                dry_run: Some(true),
            }))
            .await;

        assert_eq!(result.is_error, Some(false));
        let value = result.structured_content.expect("missing-session conflict");
        assert_eq!(value["status"], "layout_conflict");
        assert!(value["conflicts"][0]
            .as_str()
            .is_some_and(|message| message.contains("missing session_id")));
    }

    #[tokio::test]
    async fn agent_team_release_retains_membership_when_termination_fails() {
        let members = json!({
            "sessions": [
                {
                    "session_id": "session-main",
                    "workspace_id": "workspace-1",
                    "state": "running",
                    "telemetry": {
                        "team_id": "team-1",
                        "team_role": "main",
                        "team_mode": "adaptive",
                        "layout_root_pane_id": "pane-root",
                        "team_generation": 1
                    }
                },
                {
                    "session_id": "session-worker",
                    "workspace_id": "workspace-1",
                    "state": "running",
                    "telemetry": {
                        "team_id": "team-1",
                        "team_role": "worker",
                        "worker_name": "backend"
                    }
                }
            ]
        });
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Full,
            [
                ("agent.list", Ok(members.clone())),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [{"pane_id": "pane-worker", "kind": "leaf", "mounted_surface_id": "surface-worker"}],
                        "surfaces": [{"surface_id": "surface-worker", "session_id": "session-worker"}]
                    })),
                ),
                ("agent.team.reserve", Ok(json!({"generation": 2}))),
                (
                    "session.terminate",
                    Err("synthetic terminate failure".to_string()),
                ),
                ("agent.list", Ok(members)),
                ("agent.list", Ok(json!({"sessions": []}))),
                (
                    "agent.team.settle",
                    Ok(json!({"session_id": "session-main"})),
                ),
            ],
        );
        let result = server
            .agent_team_release(Parameters(AgentTeamReleaseToolParams {
                team_id: "team-1".to_string(),
                expected_generation: Some(1),
                session_id: Some("session-worker".to_string()),
                name: None,
                mode: None,
            }))
            .await;

        assert_eq!(result.is_error, Some(true));
        let value = result.structured_content.expect("release error");
        assert!(value["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("membership was retained")));
        let calls = transport.calls.lock().expect("scripted calls lock");
        assert!(calls.iter().all(|(method, params)| {
            method != "pane.close"
                && !(method == "agent.set_state" && params["session_id"] == "session-worker")
        }));
    }

    #[tokio::test]
    async fn agent_team_release_terminates_then_clears_membership_and_reflows() {
        let before = json!({
            "sessions": [
                {
                    "session_id": "session-main",
                    "workspace_id": "workspace-1",
                    "state": "running",
                    "telemetry": {
                        "team_id": "team-1",
                        "team_role": "main",
                        "team_mode": "adaptive",
                        "layout_root_pane_id": "pane-root",
                        "team_generation": 1
                    }
                },
                {
                    "session_id": "session-worker",
                    "workspace_id": "workspace-1",
                    "state": "running",
                    "telemetry": {
                        "team_id": "team-1",
                        "team_role": "worker",
                        "worker_name": "backend"
                    }
                }
            ]
        });
        let after = json!({
            "sessions": [{
                "session_id": "session-main",
                "workspace_id": "workspace-1",
                "state": "running",
                "telemetry": {
                    "team_id": "team-1",
                    "team_role": "main",
                    "team_mode": "adaptive",
                    "layout_root_pane_id": "pane-root",
                    "team_generation": 2
                }
            }]
        });
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Full,
            [
                ("agent.list", Ok(before)),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [
                            {"pane_id": "pane-root", "kind": "split", "split_axis": "vertical", "split_ratio": 0.5},
                            {"pane_id": "pane-main", "parent_pane_id": "pane-root", "kind": "leaf", "mounted_surface_id": "surface-main"},
                            {"pane_id": "pane-worker", "parent_pane_id": "pane-root", "kind": "leaf", "mounted_surface_id": "surface-worker"}
                        ],
                        "surfaces": [
                            {"surface_id": "surface-main", "session_id": "session-main"},
                            {"surface_id": "surface-worker", "session_id": "session-worker"}
                        ]
                    })),
                ),
                ("agent.team.reserve", Ok(json!({"generation": 2}))),
                ("session.terminate", Ok(json!({"terminated": true}))),
                ("pane.close", Ok(json!({"closed": true}))),
                (
                    "agent.set_state",
                    Ok(json!({"session_id": "session-worker"})),
                ),
                ("agent.list", Ok(after.clone())),
                ("agent.list", Ok(after.clone())),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [{"pane_id": "pane-root", "kind": "leaf", "mounted_surface_id": "surface-main"}],
                        "surfaces": [{"surface_id": "surface-main", "session_id": "session-main"}]
                    })),
                ),
                ("agent.list", Ok(after)),
                (
                    "agent.team.settle",
                    Ok(json!({"session_id": "session-main"})),
                ),
            ],
        );
        let result = server
            .agent_team_release(Parameters(AgentTeamReleaseToolParams {
                team_id: "team-1".to_string(),
                expected_generation: Some(1),
                session_id: Some("session-worker".to_string()),
                name: None,
                mode: Some("soft".to_string()),
            }))
            .await;

        assert_eq!(result.is_error, Some(false));
        let value = result.structured_content.expect("released worker");
        assert_eq!(value["status"], "released");
        assert_eq!(value["worker_count"], 0);
        assert_eq!(value["layout"]["status"], "empty");
        let calls = transport.calls.lock().expect("scripted calls lock");
        assert_eq!(calls[3].0, "session.terminate");
        assert_eq!(calls[4].0, "pane.close");
        assert_eq!(calls[5].0, "agent.set_state");
        assert_eq!(calls[5].1["session_id"], "session-worker");
    }

    #[tokio::test]
    async fn agent_team_reflow_preserves_foreign_panes() {
        let team_state = json!({
            "sessions": [{
                "session_id": "session-main",
                "workspace_id": "workspace-1",
                "state": "running",
                "telemetry": {
                    "team_id": "team-1",
                    "team_role": "main",
                    "team_mode": "adaptive",
                    "layout_root_pane_id": "pane-root",
                    "team_generation": 1
                }
            }]
        });
        let reserved_state = json!({
            "sessions": [{
                "session_id": "session-main",
                "workspace_id": "workspace-1",
                "state": "running",
                "telemetry": {
                    "team_id": "team-1",
                    "team_role": "main",
                    "team_mode": "adaptive",
                    "layout_root_pane_id": "pane-root",
                    "team_generation": 2,
                    "team_status": "provisioning"
                }
            }]
        });
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [
                ("agent.list", Ok(team_state)),
                ("agent.team.reserve", Ok(json!({"generation": 2}))),
                ("agent.list", Ok(reserved_state.clone())),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [
                            {"pane_id": "pane-root", "kind": "split", "split_axis": "vertical", "split_ratio": 0.5},
                            {"pane_id": "pane-main", "parent_pane_id": "pane-root", "kind": "leaf", "mounted_surface_id": "surface-main"},
                            {"pane_id": "pane-foreign", "parent_pane_id": "pane-root", "kind": "leaf", "mounted_surface_id": "surface-foreign"}
                        ],
                        "surfaces": [
                            {"surface_id": "surface-main", "session_id": "session-main"},
                            {"surface_id": "surface-foreign", "session_id": "session-foreign"}
                        ]
                    })),
                ),
                ("agent.list", Ok(reserved_state)),
                (
                    "agent.team.settle",
                    Ok(json!({"session_id": "session-main"})),
                ),
            ],
        );
        let result = server
            .agent_team_reflow(Parameters(AgentTeamIdToolParams {
                team_id: "team-1".to_string(),
                expected_generation: Some(1),
                dry_run: None,
            }))
            .await;

        assert_eq!(result.is_error, Some(false));
        let value = result.structured_content.expect("layout conflict result");
        assert_eq!(value["status"], "layout_dirty");
        assert_eq!(value["generation"], 2);
        assert_eq!(value["layout"]["status"], "layout_conflict");
        assert!(value["layout"]["conflicts"][0]
            .as_str()
            .is_some_and(|message| message.contains("foreign panes")));
        let calls = transport.calls.lock().expect("scripted calls lock");
        assert!(calls
            .iter()
            .all(|(method, _)| method != "pane.resize_layout"));
    }

    #[tokio::test]
    async fn agent_team_start_builds_main_left_with_equal_worker_stack() {
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "system.identify",
                    Ok(json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-other",
                        "session_id": "session-other",
                        "cwd": "/workspace/project"
                    })),
                ),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [
                            {"pane_id": "pane-main", "kind": "leaf", "mounted_surface_id": "surface-main"}
                        ],
                        "surfaces": [
                            {"surface_id": "surface-main", "session_id": "session-main"}
                        ]
                    })),
                ),
                ("agent.list", Ok(json!({"sessions": []}))),
                ("agent.team.reserve", Ok(json!({"claimed": true}))),
                (
                    "terminal.split",
                    Ok(json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-worker-1",
                        "surface_id": "surface-worker-1",
                        "session_id": "session-worker-1"
                    })),
                ),
                (
                    "terminal.split",
                    Ok(json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-worker-2",
                        "surface_id": "surface-worker-2",
                        "session_id": "session-worker-2"
                    })),
                ),
                (
                    "terminal.split",
                    Ok(json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-worker-3",
                        "surface_id": "surface-worker-3",
                        "session_id": "session-worker-3"
                    })),
                ),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [
                            {"pane_id": "pane-main-leaf", "kind": "leaf", "mounted_surface_id": "surface-main"},
                            {"pane_id": "pane-worker-1-leaf", "kind": "leaf", "mounted_surface_id": "surface-worker-1"},
                            {"pane_id": "pane-worker-2-leaf", "kind": "leaf", "mounted_surface_id": "surface-worker-2"},
                            {"pane_id": "pane-worker-3", "kind": "leaf", "mounted_surface_id": "surface-worker-3"}
                        ],
                        "surfaces": [
                            {"surface_id": "surface-main", "session_id": "session-main"},
                            {"surface_id": "surface-worker-1", "session_id": "session-worker-1"},
                            {"surface_id": "surface-worker-2", "session_id": "session-worker-2"},
                            {"surface_id": "surface-worker-3", "session_id": "session-worker-3"}
                        ]
                    })),
                ),
                (
                    "agent.set_state",
                    Ok(json!({"session_id": "session-worker-1"})),
                ),
                (
                    "agent.set_state",
                    Ok(json!({"session_id": "session-worker-2"})),
                ),
                (
                    "agent.set_state",
                    Ok(json!({"session_id": "session-worker-3"})),
                ),
                (
                    "agent.list",
                    Ok(json!({
                        "sessions": [{
                            "session_id": "session-main",
                            "workspace_id": "workspace-1",
                            "state": "running",
                            "telemetry": {"activity": "agent", "session": "codex"}
                        }]
                    })),
                ),
                (
                    "agent.team.settle",
                    Ok(json!({"session_id": "session-main"})),
                ),
            ],
        );
        let result = server
            .agent_team_start(Parameters(AgentTeamStartToolParams {
                workspace_id: None,
                pane_id: Some("pane-main".to_string()),
                layout: None,
                main_ratio: Some(0.55),
                mode: None,
                auto_adopt_tmux: None,
                idempotency_key: None,
                max_workers: None,
                default_worker_kind: None,
                distribution: Some("Ubuntu".to_string()),
                cwd: None,
                columns: None,
                rows: None,
                durability: None,
                workers: vec![
                    AgentTeamWorkerSpec {
                        name: "backend".to_string(),
                        kind: AgentWorkerKindParam::CodexPane,
                        distribution: None,
                        cwd: None,
                        args: vec!["--model".to_string(), "gpt-5.6-sol".to_string()],
                        durability: None,
                    },
                    AgentTeamWorkerSpec {
                        name: "frontend".to_string(),
                        kind: AgentWorkerKindParam::CodexPane,
                        distribution: None,
                        cwd: None,
                        args: Vec::new(),
                        durability: None,
                    },
                    AgentTeamWorkerSpec {
                        name: "tests".to_string(),
                        kind: AgentWorkerKindParam::CodexPane,
                        distribution: None,
                        cwd: None,
                        args: Vec::new(),
                        durability: None,
                    },
                ],
            }))
            .await;
        assert_eq!(result.is_error, Some(false));
        let value = result.structured_content.expect("team result");
        assert_eq!(value["layout"], "main-left-workers-right");
        assert_eq!(value["main"]["session_id"], "session-main");
        assert_eq!(value["main"]["pane_id"], "pane-main-leaf");
        assert_eq!(value["main"]["layout_root_pane_id"], "pane-main");
        assert_eq!(value["worker_count"], 3);
        assert_eq!(value["workers"][0]["name"], "backend");
        assert_eq!(value["workers"][0]["pane_id"], "pane-worker-1-leaf");
        assert_eq!(value["workers"][1]["pane_id"], "pane-worker-2-leaf");
        assert_eq!(value["workers"][2]["pane_id"], "pane-worker-3");

        let calls = transport.calls.lock().expect("scripted calls lock");
        let split_calls = [&calls[4], &calls[5], &calls[6]];
        assert_eq!(split_calls[0].1["pane_id"], "pane-main");
        assert_eq!(split_calls[0].1["axis"], "vertical");
        assert_eq!(split_calls[0].1["ratio"], 0.55);
        assert_eq!(split_calls[1].1["pane_id"], "pane-worker-1");
        assert_eq!(split_calls[1].1["axis"], "horizontal");
        assert!((split_calls[1].1["ratio"].as_f64().unwrap() - (1.0 / 3.0)).abs() < 1e-9);
        assert_eq!(split_calls[2].1["pane_id"], "pane-worker-2");
        assert_eq!(split_calls[2].1["axis"], "horizontal");
        assert_eq!(split_calls[2].1["ratio"], 0.5);
        assert_eq!(calls[7].0, "workspace.get");
        assert_eq!(calls[8].1["telemetry"]["session"], "codex-pane:backend");
        assert_eq!(calls[8].1["telemetry"]["ctx"], "pane-worker-1-leaf");
        assert_eq!(calls[9].1["telemetry"]["session"], "codex-pane:frontend");
        assert_eq!(calls[9].1["telemetry"]["ctx"], "pane-worker-2-leaf");
        assert_eq!(calls[10].1["telemetry"]["session"], "codex-pane:tests");
    }

    #[tokio::test]
    async fn agent_team_start_rolls_back_created_workers_in_reverse_order() {
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "system.identify",
                    Ok(json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-main",
                        "cwd": "/workspace/project"
                    })),
                ),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [
                            {"pane_id": "pane-main", "kind": "leaf", "mounted_surface_id": "surface-main"}
                        ],
                        "surfaces": [
                            {"surface_id": "surface-main", "session_id": "session-main"}
                        ]
                    })),
                ),
                ("agent.list", Ok(json!({"sessions": []}))),
                ("agent.team.reserve", Ok(json!({"claimed": true}))),
                (
                    "terminal.split",
                    Ok(json!({
                        "pane_id": "pane-worker-1",
                        "session_id": "session-worker-1"
                    })),
                ),
                (
                    "terminal.split",
                    Ok(json!({
                        "pane_id": "pane-worker-2",
                        "session_id": "session-worker-2"
                    })),
                ),
                (
                    "terminal.split",
                    Err("synthetic third worker failure".to_string()),
                ),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [
                            {"pane_id": "pane-worker-1-leaf", "kind": "leaf", "mounted_surface_id": "surface-worker-1"},
                            {"pane_id": "pane-worker-2-leaf", "kind": "leaf", "mounted_surface_id": "surface-worker-2"}
                        ],
                        "surfaces": [
                            {"surface_id": "surface-worker-1", "session_id": "session-worker-1"},
                            {"surface_id": "surface-worker-2", "session_id": "session-worker-2"}
                        ]
                    })),
                ),
                ("session.terminate", Ok(json!({"terminated": true}))),
                (
                    "pane.close",
                    Err("synthetic pane close failure".to_string()),
                ),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [
                            {"pane_id": "pane-worker-1-leaf", "kind": "leaf", "mounted_surface_id": "surface-worker-1"},
                            {"pane_id": "pane-worker-2-leaf", "kind": "leaf", "mounted_surface_id": "surface-worker-2"}
                        ],
                        "surfaces": [
                            {"surface_id": "surface-worker-1", "session_id": "session-worker-1"},
                            {"surface_id": "surface-worker-2", "session_id": "session-worker-2"}
                        ]
                    })),
                ),
                ("session.terminate", Ok(json!({"terminated": true}))),
                ("pane.close", Ok(json!({"closed": true}))),
                (
                    "agent.list",
                    Ok(json!({
                        "sessions": [{
                            "session_id": "session-main",
                            "workspace_id": "workspace-1",
                            "state": "running",
                            "telemetry": {"team_id": "team-rollback", "team_role": "main"}
                        }]
                    })),
                ),
                (
                    "agent.list",
                    Ok(json!({
                        "sessions": [{
                            "session_id": "session-main",
                            "workspace_id": "workspace-1",
                            "state": "running",
                            "telemetry": {"team_id": "team-rollback", "team_role": "main"}
                        }]
                    })),
                ),
                ("agent.set_state", Ok(json!({"session_id": "session-main"}))),
            ],
        );
        let workers = ["one", "two", "three"]
            .into_iter()
            .map(|name| AgentTeamWorkerSpec {
                name: name.to_string(),
                kind: AgentWorkerKindParam::CodexPane,
                distribution: None,
                cwd: None,
                args: Vec::new(),
                durability: None,
            })
            .collect();
        let result = server
            .agent_team_start(Parameters(AgentTeamStartToolParams {
                workspace_id: None,
                pane_id: None,
                layout: Some(AgentTeamLayoutParam::MainLeftWorkersRight),
                main_ratio: None,
                mode: None,
                auto_adopt_tmux: None,
                idempotency_key: None,
                max_workers: None,
                default_worker_kind: None,
                distribution: None,
                cwd: None,
                columns: None,
                rows: None,
                durability: None,
                workers,
            }))
            .await;
        assert_eq!(result.is_error, Some(true));
        let value = result.structured_content.expect("team error");
        assert_eq!(value["error"]["failed_worker"]["name"], "three");
        assert_eq!(value["error"]["rollback"][0]["name"], "two");
        assert_eq!(value["error"]["rollback"][1]["name"], "one");
        assert_eq!(value["error"]["rollback"][0]["pane_closed"], false);
        assert_eq!(value["error"]["rollback"][1]["pane_closed"], true);

        let calls = transport.calls.lock().expect("scripted calls lock");
        assert_eq!(calls[7].0, "workspace.get");
        assert_eq!(calls[8].0, "session.terminate");
        assert_eq!(calls[8].1["session_id"], "session-worker-2");
        assert_eq!(calls[9].0, "pane.close");
        assert_eq!(calls[9].1["pane_id"], "pane-worker-2-leaf");
        assert_eq!(calls[10].0, "workspace.get");
        assert_eq!(calls[11].1["session_id"], "session-worker-1");
        assert_eq!(calls[12].1["pane_id"], "pane-worker-1-leaf");
    }

    #[tokio::test]
    async fn agent_team_start_rolls_back_the_worker_whose_registration_fails() {
        let topology = json!({
            "panes": [
                {"pane_id": "pane-main-leaf", "kind": "leaf", "mounted_surface_id": "surface-main"},
                {"pane_id": "pane-worker-leaf", "kind": "leaf", "mounted_surface_id": "surface-worker"}
            ],
            "surfaces": [
                {"surface_id": "surface-main", "session_id": "session-main"},
                {"surface_id": "surface-worker", "session_id": "session-worker"}
            ]
        });
        let (server, transport) = scripted_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "system.identify",
                    Ok(json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-main",
                        "cwd": "/workspace/project"
                    })),
                ),
                (
                    "workspace.get",
                    Ok(json!({
                        "panes": [
                            {"pane_id": "pane-main", "kind": "leaf", "mounted_surface_id": "surface-main"}
                        ],
                        "surfaces": [
                            {"surface_id": "surface-main", "session_id": "session-main"}
                        ]
                    })),
                ),
                ("agent.list", Ok(json!({"sessions": []}))),
                ("agent.team.reserve", Ok(json!({"claimed": true}))),
                (
                    "terminal.split",
                    Ok(json!({
                        "pane_id": "pane-worker-anchor",
                        "surface_id": "surface-worker",
                        "session_id": "session-worker"
                    })),
                ),
                ("workspace.get", Ok(topology.clone())),
                (
                    "agent.set_state",
                    Err("synthetic metadata failure".to_string()),
                ),
                ("workspace.get", Ok(topology)),
                ("session.terminate", Ok(json!({"terminated": true}))),
                ("pane.close", Ok(json!({"closed": true}))),
            ],
        );
        let result = server
            .agent_team_start(Parameters(AgentTeamStartToolParams {
                workspace_id: None,
                pane_id: None,
                layout: None,
                main_ratio: None,
                mode: None,
                auto_adopt_tmux: None,
                idempotency_key: None,
                max_workers: None,
                default_worker_kind: None,
                distribution: None,
                cwd: None,
                columns: None,
                rows: None,
                durability: None,
                workers: vec![AgentTeamWorkerSpec {
                    name: "backend".to_string(),
                    kind: AgentWorkerKindParam::CodexPane,
                    distribution: None,
                    cwd: None,
                    args: Vec::new(),
                    durability: None,
                }],
            }))
            .await;
        assert_eq!(result.is_error, Some(true));
        let value = result.structured_content.expect("team error");
        assert_eq!(value["error"]["failed_worker"]["name"], "backend");
        assert_eq!(value["error"]["rollback"][0]["name"], "backend");
        assert_eq!(
            value["error"]["rollback"][0]["session_id"],
            "session-worker"
        );
        assert_eq!(value["error"]["rollback"][0]["pane_id"], "pane-worker-leaf");
        assert_eq!(value["error"]["rollback"][0]["pane_closed"], true);

        let calls = transport.calls.lock().expect("scripted calls lock");
        assert_eq!(calls[6].0, "agent.set_state");
        assert_eq!(calls[7].0, "workspace.get");
        assert_eq!(calls[8].1["session_id"], "session-worker");
        assert_eq!(calls[9].1["pane_id"], "pane-worker-leaf");
    }

    #[tokio::test]
    async fn agent_worker_send_submits_literal_text_and_enter() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "agent.list",
                    json!({
                        "sessions": [{
                            "session_id": "session-worker",
                            "telemetry": {
                                "activity": "agent_team",
                                "session": "claude-teams:worker"
                            }
                        }]
                    }),
                ),
                ("session.send_text", json!({"accepted": true})),
                ("session.send_key", json!({"accepted": true})),
            ],
        );
        let result = server
            .agent_worker_send(Parameters(AgentWorkerSendToolParams {
                workspace_id: None,
                session_id: Some("session-worker".to_string()),
                text: "Review the failing test".to_string(),
                submit: None,
            }))
            .await;
        assert_eq!(result.is_error, Some(false));
        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls[0].0, "agent.list");
        assert_eq!(calls[1].0, "session.send_text");
        assert_eq!(calls[1].1["text"], "Review the failing test");
        assert_eq!(calls[2].0, "session.send_key");
        assert_eq!(calls[2].1["key"], "enter");
    }

    #[tokio::test]
    async fn agent_worker_send_rejects_a_generic_terminal_session() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Standard,
            [(
                "agent.list",
                json!({
                    "sessions": [{
                        "session_id": "session-shell",
                        "telemetry": {"activity": "terminal"}
                    }]
                }),
            )],
        );
        let result = server
            .agent_worker_send(Parameters(AgentWorkerSendToolParams {
                workspace_id: None,
                session_id: Some("session-shell".to_string()),
                text: "exit".to_string(),
                submit: None,
            }))
            .await;
        assert_eq!(result.is_error, Some(true));
        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "agent.list");
    }

    #[tokio::test]
    async fn agent_worker_start_compensates_when_metadata_registration_fails() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "system.identify",
                    json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-source",
                        "cwd": "/workspace/project"
                    }),
                ),
                (
                    "terminal.split",
                    json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-worker",
                        "surface_id": "surface-worker",
                        "session_id": "session-worker"
                    }),
                ),
                ("session.terminate", json!({"terminated": true})),
                ("pane.close", json!({"closed": true})),
            ],
        );
        let result = server
            .agent_worker_start(Parameters(AgentWorkerStartToolParams {
                workspace_id: None,
                pane_id: None,
                name: None,
                kind: AgentWorkerKindParam::CodexPane,
                distribution: Some("Ubuntu".to_string()),
                placement: None,
                axis: None,
                ratio: None,
                cwd: None,
                args: Vec::new(),
                columns: None,
                rows: None,
                durability: None,
            }))
            .await;
        assert_eq!(result.is_error, Some(true));
        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls[2].0, "agent.set_state");
        assert_eq!(calls[3].0, "session.terminate");
        assert_eq!(calls[4].0, "pane.close");
    }

    #[tokio::test]
    async fn agent_worker_start_closes_a_pane_when_launch_omits_session_id() {
        let (server, transport) = test_server_for_profile(
            McpProfile::Standard,
            [
                (
                    "system.identify",
                    json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-source",
                        "cwd": "/workspace/project"
                    }),
                ),
                (
                    "terminal.split",
                    json!({
                        "workspace_id": "workspace-1",
                        "pane_id": "pane-worker",
                        "surface_id": "surface-worker"
                    }),
                ),
                ("pane.close", json!({"closed": true})),
            ],
        );
        let result = server
            .agent_worker_start(Parameters(AgentWorkerStartToolParams {
                workspace_id: None,
                pane_id: None,
                name: Some("worker".to_string()),
                kind: AgentWorkerKindParam::CodexPane,
                distribution: Some("Ubuntu".to_string()),
                placement: None,
                axis: None,
                ratio: None,
                cwd: None,
                args: Vec::new(),
                columns: None,
                rows: None,
                durability: None,
            }))
            .await;
        assert_eq!(result.is_error, Some(true));
        let calls = transport.calls.lock().expect("fake calls lock");
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[2].0, "pane.close");
        assert_eq!(calls[2].1["pane_id"], "pane-worker");
    }

    #[tokio::test]
    async fn browser_snapshot_bounds_utf8_without_splitting_a_character() {
        let (server, transport) = test_server([(
            "browser.dom_snapshot",
            json!({"surface_id": "surface-1", "html": "ab한글"}),
        )]);
        let result = server
            .browser_snapshot(Parameters(BrowserSnapshotParams {
                surface_id: "surface-1".to_string(),
                frame_id: None,
                max_bytes: Some(6),
            }))
            .await;
        assert_eq!(result.is_error, Some(false));
        let value = result
            .structured_content
            .expect("structured browser result");
        assert_eq!(value["html"], "ab한");
        assert_eq!(value["original_byte_count"], 8);
        assert_eq!(value["returned_byte_count"], 5);
        assert_eq!(value["truncated"], true);
        let calls = transport.calls.lock().expect("fake calls lock");
        assert!(calls[0].1.get("max_bytes").is_none());
    }

    #[tokio::test]
    async fn protocol_initialize_list_and_call_work_over_async_stdio_framing() {
        use tokio::io::{split, BufReader};

        let (server, _) = test_server([("workspace.list", json!({"workspaces": []}))]);
        let (client, service_side) = tokio::io::duplex(64 * 1024);
        let (service_read, service_write) = split(service_side);
        let service_task = tokio::spawn(async move {
            let service = server
                .serve((service_read, service_write))
                .await
                .expect("serve test MCP transport");
            service.waiting().await.expect("wait for test MCP service");
        });
        let (client_read, mut client_write) = split(client);
        let mut lines = BufReader::new(client_read).lines();

        let initialize_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "agentmux-test", "version": "1"}
            }
        });
        write_protocol_message(&mut client_write, &initialize_request).await;
        let initialize: Value =
            serde_json::from_str(&read_protocol_message(&mut lines, "initialize response").await)
                .expect("parse initialize");
        assert_eq!(initialize["id"], 1);
        assert_eq!(initialize["result"]["serverInfo"]["name"], "agentmux");

        write_protocol_message(
            &mut client_write,
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        )
        .await;
        write_protocol_message(
            &mut client_write,
            &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        )
        .await;
        let listed: Value =
            serde_json::from_str(&read_protocol_message(&mut lines, "tools/list response").await)
                .expect("parse tools");
        assert_eq!(
            listed["result"]["tools"].as_array().map(Vec::len),
            Some(READ_TOOL_NAMES.len())
        );

        write_protocol_message(
            &mut client_write,
            &json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {"name": "workspace_list", "arguments": {}}
            }),
        )
        .await;
        let called: Value =
            serde_json::from_str(&read_protocol_message(&mut lines, "tools/call response").await)
                .expect("parse tool call");
        assert_eq!(called["result"]["isError"], false);
        assert_eq!(
            called["result"]["structuredContent"]["workspaces"],
            json!([])
        );

        write_protocol_message(
            &mut client_write,
            &json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {"name": "terminal_open", "arguments": {}}
            }),
        )
        .await;
        let denied: Value = serde_json::from_str(
            &read_protocol_message(&mut lines, "profile authorization response").await,
        )
        .expect("parse authorization response");
        assert_eq!(denied["result"]["isError"], true);
        assert_eq!(
            denied["result"]["structuredContent"]["error"]["method"],
            "mcp.profile"
        );
        assert!(denied["result"]["structuredContent"]["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("requires the 'standard' MCP profile")));

        drop(client_write);
        drop(lines);
        service_task.await.expect("join MCP service task");
    }

    async fn write_protocol_message<W>(writer: &mut W, message: &Value)
    where
        W: tokio::io::AsyncWrite + Unpin,
    {
        let mut frame = serde_json::to_vec(message).expect("serialize protocol message");
        frame.push(b'\n');
        writer
            .write_all(&frame)
            .await
            .expect("write protocol message");
    }

    async fn read_protocol_message<R>(
        lines: &mut tokio::io::Lines<tokio::io::BufReader<R>>,
        label: &str,
    ) -> String
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        tokio::time::timeout(std::time::Duration::from_secs(2), lines.next_line())
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for {label}"))
            .unwrap_or_else(|error| panic!("failed to read {label}: {error}"))
            .unwrap_or_else(|| panic!("stream ended before {label}"))
    }
}
