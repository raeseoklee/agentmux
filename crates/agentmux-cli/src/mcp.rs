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

use agentmux_ipc::ControlCaller;

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
const READ_TOOL_NAMES: &[&str] = &[
    "agentmux_context",
    "workspace_list",
    "workspace_get",
    "session_list",
    "terminal_read",
    "agent_attention_list",
    "agent_worker_list",
    "agent_integration_status",
    "event_poll",
    "browser_snapshot",
    "browser_get",
    "team_task_list",
    "team_message_list",
    "diagnostics_summary",
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
    "agent_worker_send",
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
    "agent_worker_send",
];
#[cfg(test)]
const STANDARD_ADDITIVE_TOOL_NAMES: &[&str] = &["pane_focus", "team_message_send"];
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
    "agent_integration_setup",
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
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl AgentMuxMcpServer {
    fn new(options: McpOptions) -> Self {
        Self::with_transport(
            options.profile,
            Arc::new(NamedPipeTransport::for_mcp(options.invoke, options.profile)),
        )
    }

    fn with_transport(profile: McpProfile, control: Arc<dyn ControlTransport>) -> Self {
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
        let kind = AgentWorkerKind::from(params.kind);
        let command = match build_agent_worker_command(kind, params.args) {
            Ok(command) => command,
            Err(message) => return control_error_result("agent.worker.start", message),
        };
        let cwd = params.cwd.or(context.cwd);
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
                    "env": [],
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
        let Some(session_id) = launch
            .get("session_id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
        else {
            return control_error_result(
                "agent.worker.start",
                "terminal launch returned no session_id".to_string(),
            );
        };
        let pane_id = launch
            .get("pane_id")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let label = kind.label();
        if let Err(message) = self
            .invoke_value(
                "agent.set_state",
                json!({
                    "session_id": session_id.clone(),
                    "state": "running",
                    "reason": format!("{label} worker started"),
                    "telemetry": {
                        "activity": kind.activity(),
                        "session": format!("{}:worker", kind.key()),
                        "ctx": pane_id,
                    },
                }),
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
        CallToolResult::structured(json!({
            "kind": kind.key(),
            "controller": kind.controller(),
            "workspace_id": workspace_id,
            "session_id": session_id,
            "pane_id": launch.get("pane_id"),
            "surface_id": launch.get("surface_id"),
            "placement": placement,
            "agent_state_registered": true,
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

fn missing_context_result(field: &str) -> CallToolResult {
    control_error_result(
        "system.identify",
        format!("{field} was not supplied and could not be resolved from AgentMux caller context"),
    )
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    use super::*;

    #[derive(Default)]
    struct FakeTransport {
        responses: HashMap<String, Value>,
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
        let server = AgentMuxMcpServer::with_transport(
            profile,
            Arc::clone(&transport) as Arc<dyn ControlTransport>,
        );
        (server, transport)
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
                "agent_worker_list",
                "agentmux_context",
                "browser_get",
                "browser_snapshot",
                "diagnostics_summary",
                "event_poll",
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
        assert!(!McpProfile::Read.allows_tool("terminal_open"));
        assert!(!McpProfile::Read.allows_tool("workspace_close"));

        assert!(McpProfile::Standard.allows_tool("workspace_list"));
        assert!(McpProfile::Standard.allows_tool("terminal_open"));
        assert!(!McpProfile::Standard.allows_tool("workspace_close"));

        assert!(McpProfile::Full.allows_tool("workspace_list"));
        assert!(McpProfile::Full.allows_tool("terminal_open"));
        assert!(McpProfile::Full.allows_tool("workspace_close"));
        assert!(!McpProfile::Full.allows_tool("unclassified_future_tool"));
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
            "agent_worker_send",
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
        assert_eq!(listed["result"]["tools"].as_array().map(Vec::len), Some(14));

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
