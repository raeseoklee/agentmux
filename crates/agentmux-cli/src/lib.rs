use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use agentmux_backend::SessionBackend;
use agentmux_backend_conpty::ConptyBackend;
use agentmux_backend_wsl::{
    discover_wsl_distributions as discover_wsl_distributions_from_backend,
    fallback_windows_path_to_wsl, WslDirectBackend, WslDirectConfig,
};
use agentmux_core::{RuntimeControlPlane, TerminalRuntime};
use agentmux_ipc::{
    default_control_token_path, read_control_token, AckResult, ActionListParams, ActionListResult,
    ActionRunParams, ActionRunResult, AgentAttentionListResult, AgentListAttentionParams,
    AgentSetStateParams, AgentStateResult, AgentTelemetry, AppConfigDiagnosticsParams,
    AppConfigDiagnosticsResult, AppConfigGetParams, AppConfigMigrateProjectParams,
    AppConfigMigrateProjectResult, AppConfigResult, BrowserActionResult, BrowserCheckParams,
    BrowserClickParams, BrowserConsoleParams, BrowserConsoleResult, BrowserCookiesResult,
    BrowserDiagnosticsParams, BrowserDiagnosticsResult, BrowserDialogsParams, BrowserDialogsResult,
    BrowserDomSnapshotParams, BrowserDomSnapshotResult, BrowserDownloadsParams,
    BrowserDownloadsResult, BrowserErrorsParams, BrowserErrorsResult, BrowserEvaluateParams,
    BrowserEvaluateResult, BrowserFillParams, BrowserFindParams, BrowserFindResult,
    BrowserFocusParams, BrowserFramesResult, BrowserGetParams, BrowserGetResult,
    BrowserHighlightParams, BrowserHistoryResult, BrowserHoverParams, BrowserNavigateParams,
    BrowserNavigationResult, BrowserPressParams, BrowserScreenshotParams, BrowserScreenshotResult,
    BrowserScrollParams, BrowserSelectParams, BrowserStorageResult, BrowserSurfaceParams,
    BrowserTypeParams, BrowserWaitForSelectorParams, BrowserWaitForSelectorResult,
    BrowserZoomParams, ControlError, DiagnosticsExportResult, ErrorCode, EventFrame,
    EventPollParams, EventPollResult, EventSubscribeParams, EventSubscribeResult,
    NamedPipeEventStream, NotificationClearParams, NotificationClearResult,
    NotificationCreateParams, NotificationDismissParams, NotificationListParams,
    NotificationListResult, NotificationSummaryResult, PaneCloseParams, PaneFocusParams,
    PaneSplitParams, PaneSummaryResult, ProfileListResult, ProfileSummaryResult,
    RecoveryDiagnosticsResult, RequestEnvelope, ResponseEnvelope, ResponseOutcome, SessionIdParams,
    SessionListParams, SessionListResult, SessionOutputPressureParams, SessionReadRecentParams,
    SessionReadRecentResult, SessionResizeParams, SessionSendKeyParams, SessionSendTextParams,
    SessionSnapshotParams, SessionSnapshotResult, SessionSpawnParams, SessionSpawnResult,
    SessionSummaryResult, SessionTerminateParams, SidebarLogAddParams, SidebarLogListParams,
    SidebarLogListResult, SidebarProgressSetParams, SidebarStateResult, SidebarStatusKeyParams,
    SidebarStatusListResult, SidebarStatusSetParams, SidebarWorkspaceParams, SurfaceCloseParams,
    SurfaceCreateBrowserParams, SurfaceSummaryResult, SystemCapabilitiesResult,
    SystemIdentifyParams, SystemIdentifyResult, WorkspaceCloseParams, WorkspaceCloseResult,
    WorkspaceCreateParams, WorkspaceDetailResult, WorkspaceGroupCreateParams,
    WorkspaceGroupIdParams, WorkspaceGroupListParams, WorkspaceGroupListResult,
    WorkspaceGroupMemberParams, WorkspaceGroupResult, WorkspaceGroupUpdateParams,
    WorkspaceIdParams, WorkspaceListResult, WorkspaceRenameParams, WorkspaceSummaryResult,
    DEFAULT_CONTROL_PIPE_NAME,
};
use tungstenite::{accept_hdr, Error as WsError, Message as WsMessage};

const AGENTMUX_CONFIG_SCHEMA_JSON: &str =
    include_str!("../../../docs/schemas/agentmux.config.schema.json");

pub const COMMAND_FAMILIES: &[&str] = &[
    "system",
    "workspace",
    "workspace-group",
    "pane",
    "surface",
    "terminal",
    "server",
    "ssh",
    "notification",
    "events",
    "browser",
    "agent",
    "actions",
    "diagnostics",
    "session",
    "config",
    "notify",
    "set-status",
    "clear-status",
    "list-status",
    "set-progress",
    "clear-progress",
    "log",
    "clear-log",
    "list-log",
    "sidebar-state",
    "capabilities",
    "identify",
    "ping",
    "list-workspaces",
    "new-workspace",
    "current-workspace",
    "close-workspace",
    "list-surfaces",
    "new-split",
    "send",
    "send-key",
    "list-notifications",
    "__tmux-compat",
    "integrations",
    "claude-teams",
    "omo",
    "omx",
    "omc",
];

pub fn usage() -> String {
    usage_for("agentmux")
}

pub fn usage_for(program_name: &str) -> String {
    format!(
        concat!(
            "{program_name} <{}> <command> [options]\n\n",
            "Try: {program_name} workspace list\n",
            "Try: {program_name} workspace create AgentMux --project D:\\work\\repo\n",
            "Try: {program_name} workspace group create Agents --workspace <id>\n",
            "Try: {program_name} workspace close <id> --policy fail_if_running --yes\n",
            "Try: {program_name} session spawn --workspace <id> -- cmd.exe /d /q\n",
            "Try: {program_name} session list --workspace <id>\n",
            "Try: {program_name} session terminate <id> --mode soft --yes\n",
            "Try: {program_name} agent set-state <session-id> waiting_for_input --reason \"needs input\"\n",
            "Try: {program_name} actions list --workspace <id> --json\n",
            "Try: {program_name} actions run custom.verify --workspace <id> --json\n",
            "Try: {program_name} browser open --workspace <id> --placement new-tab\n",
            "Try: {program_name} browser navigate <surface-id> https://example.com\n",
            "Try: {program_name} browser reload <surface-id>\n",
            "Try: {program_name} browser fill <surface-id> #q -- \"hello\"\n",
            "Try: {program_name} browser get <surface-id> #q --kind text\n",
            "Try: {program_name} browser zoom <surface-id> 125\n",
            "Try: {program_name} browser wait-for-selector <surface-id> #ready --timeout-ms 5000\n",
            "Try: {program_name} browser evaluate <surface-id> [--frame <frame-id>] -- document.title\n",
            "Try: {program_name} ssh deploy@host.example:22 --workspace <id>\n",
            "Try: {program_name} notification list --severity warning\n",
            "Try: {program_name} events watch --workspace <id>\n",
            "Try: {program_name} diagnostics export --json\n",
            "Try: {program_name} config reload --json\n",
            "Try: {program_name} config migrate-cmux --workspace <id>\n",
            "Try: {program_name} config diagnostics --workspace <id>\n",
            "Try: {program_name} config schema --output agentmux.config.schema.json\n",
            "Try: {program_name} terminal run -- cmd.exe /d /q /c \"echo agentmux\"\n",
            "Try: {program_name} terminal run --backend wsl-direct --distribution Ubuntu --cwd D:\\work\\repo -- bash -lc pwd\n",
            "Try: {program_name} server --workspace <id> --port 8765\n",
            "Try: {program_name} notify --title \"Build\" --body \"Done\"\n",
            "Try: {program_name} set-status build compiling --priority 80\n",
            "Try: {program_name} set-progress 0.5 --label \"Building\"\n",
            "Try: {program_name} log --level success -- \"All tests passed\"\n",
            "Try: {program_name} sidebar-state --json\n",
            "Try: {program_name} identify --json\n",
            "Try: cmux list-workspaces --json\n",
            "Try: cmux current-workspace\n",
            "Try: cmux ping\n",
            "Try: cmux claude-teams\n",
            "Try: cmux omo --continue\n",
            "Try: cmux integrations install-shims --user-path\n",
            "Try: cmux integrations setup omo --install-packages --distribution Ubuntu\n",
            "Try: cmux integrations doctor omo --distribution Ubuntu --json"
        ),
        COMMAND_FAMILIES.join("|"),
        program_name = program_name
    )
}

pub fn run_cli<I, S, W>(args: I, output: W) -> Result<(), CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    W: Write,
{
    run_cli_with_program("agentmux", args, output)
}

pub fn run_cli_with_program<I, S, W>(
    program_name: &str,
    args: I,
    mut output: W,
) -> Result<(), CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    W: Write,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();

    match args.as_slice() {
        [family, group, command, rest @ ..] if family == "workspace" && group == "group" => {
            run_workspace_group_command(command, rest, &mut output)
        }
        [family, command, rest @ ..] if family == "workspace-group" => {
            run_workspace_group_command(command, rest, &mut output)
        }
        [family, command, rest @ ..] if family == "workspace" && command == "create" => {
            let options = parse_workspace_create_options(rest)?;
            run_workspace_create(options, &mut output)
        }
        [family, command, rest @ ..] if family == "workspace" && command == "list" => {
            let options = parse_no_params_control_options(rest, "workspace list")?;
            run_workspace_list(options, &mut output)
        }
        [family, command, rest @ ..] if family == "workspace" && command == "get" => {
            let options = parse_workspace_get_options(rest)?;
            run_workspace_get(options, &mut output)
        }
        [family, command, rest @ ..] if family == "workspace" && command == "rename" => {
            let options = parse_workspace_rename_options(rest)?;
            run_workspace_rename(options, &mut output)
        }
        [family, command, rest @ ..] if family == "workspace" && command == "close" => {
            let options = parse_workspace_close_options(rest)?;
            run_workspace_close(options, &mut output)
        }
        [family, command, rest @ ..] if family == "session" && command == "spawn" => {
            let options = parse_session_spawn_options(rest)?;
            run_session_spawn(options, &mut output)
        }
        [family, command, rest @ ..] if family == "session" && command == "list" => {
            let options = parse_session_list_options(rest)?;
            run_session_list(options, &mut output)
        }
        [family, command, rest @ ..] if family == "session" && command == "get" => {
            let options = parse_session_get_options(rest)?;
            run_session_get(options, &mut output)
        }
        [family, command, rest @ ..] if family == "session" && command == "send-text" => {
            let options = parse_session_send_text_options(rest)?;
            run_session_send_text(options, &mut output)
        }
        [family, command, rest @ ..] if family == "session" && command == "send-key" => {
            let options = parse_session_send_key_options(rest)?;
            run_session_send_key(options, &mut output)
        }
        [family, command, rest @ ..] if family == "session" && command == "read-recent" => {
            let options = parse_session_read_recent_options(rest)?;
            run_session_read_recent(options, &mut output)
        }
        [family, command, rest @ ..] if family == "session" && command == "terminate" => {
            let options = parse_session_terminate_options(rest)?;
            run_session_terminate(options, &mut output)
        }
        [family, command, rest @ ..] if family == "agent" && command == "set-state" => {
            let options = parse_agent_set_state_options(rest)?;
            run_agent_set_state(options, &mut output)
        }
        [family, command, rest @ ..] if family == "agent" && command == "get-state" => {
            let options = parse_agent_get_state_options(rest)?;
            run_agent_get_state(options, &mut output)
        }
        [family, command, rest @ ..] if family == "agent" && command == "list-attention" => {
            let options = parse_agent_list_attention_options(rest)?;
            run_agent_list_attention(options, &mut output)
        }
        [family, command, rest @ ..] if family == "agent" && command == "clear-attention" => {
            let options = parse_agent_clear_attention_options(rest)?;
            run_agent_clear_attention(options, &mut output)
        }
        [family, command, rest @ ..] if family == "notification" && command == "list" => {
            let options = parse_notification_list_options(rest)?;
            run_notification_list(options, &mut output)
        }
        [family, command, rest @ ..] if family == "notification" && command == "dismiss" => {
            let options = parse_notification_dismiss_options(rest)?;
            run_notification_dismiss(options, &mut output)
        }
        [family, command, rest @ ..] if family == "events" && command == "poll" => {
            let options = parse_events_poll_options(rest)?;
            run_events_poll(options, &mut output)
        }
        [family, command, rest @ ..] if family == "events" && command == "watch" => {
            let options = parse_events_watch_options(rest)?;
            run_events_watch(options, &mut output)
        }
        [family, command, rest @ ..] if family == "diagnostics" && command == "export" => {
            let options = parse_no_params_control_options(rest, "diagnostics export")?;
            run_diagnostics_export(options, &mut output)
        }
        [family, command, rest @ ..] if family == "config" && command == "get" => {
            let options = parse_config_get_options(rest, "config get")?;
            run_config_get(options, &mut output)
        }
        [family, command, rest @ ..] if family == "config" && command == "reload" => {
            let options = parse_config_get_options(rest, "config reload")?;
            run_config_reload(options, &mut output)
        }
        [family, command, rest @ ..] if family == "config" && command == "migrate-cmux" => {
            let options = parse_config_migrate_project_options(rest)?;
            run_config_migrate_project(options, &mut output)
        }
        [family, command, rest @ ..] if family == "config" && command == "diagnostics" => {
            let options = parse_config_diagnostics_options(rest)?;
            run_config_diagnostics(options, &mut output)
        }
        [family, command, rest @ ..] if family == "config" && command == "schema" => {
            let options = parse_config_schema_options(rest)?;
            run_config_schema(options, &mut output)
        }
        [family, command, rest @ ..] if family == "actions" && command == "list" => {
            let options = parse_action_list_options(rest)?;
            run_actions_list(options, &mut output)
        }
        [family, command, rest @ ..] if family == "actions" && command == "run" => {
            let options = parse_action_run_options(rest)?;
            run_actions_run(options, &mut output)
        }
        [family, command, rest @ ..] if family == "browser" => {
            run_browser_command(command, rest, &mut output)
        }
        [family, rest @ ..] if family == "ssh" => {
            let options = parse_ssh_options(rest)?;
            run_ssh(options, &mut output)
        }
        [family, rest @ ..] if family == "diagnostics" => {
            let options = parse_no_params_control_options(rest, "diagnostics")?;
            run_diagnostics(options, &mut output)
        }
        [family, command, rest @ ..] if family == "terminal" && command == "run" => {
            let options = parse_terminal_run_options(rest)?;
            run_terminal_command(options, &mut output)
        }
        [family, rest @ ..] if family == "server" => {
            let options = parse_server_options(rest)?;
            run_server(options, &mut output)
        }
        [command, rest @ ..] if command == "notify" => {
            let options = parse_notify_options(rest)?;
            run_notify(options, &mut output)
        }
        [command, rest @ ..] if command == "clear-notifications" => {
            let options = parse_notification_clear_options(rest)?;
            run_notification_clear(options, &mut output)
        }
        [command, rest @ ..] if command == "set-status" => {
            let options = parse_sidebar_status_set_options(rest)?;
            run_sidebar_set_status(options, &mut output)
        }
        [command, rest @ ..] if command == "clear-status" => {
            let options = parse_sidebar_status_key_options(rest, "clear-status")?;
            run_sidebar_clear_status(options, &mut output)
        }
        [command, rest @ ..] if command == "list-status" => {
            let options = parse_sidebar_workspace_options(rest, "list-status")?;
            run_sidebar_list_status(options, &mut output)
        }
        [command, rest @ ..] if command == "set-progress" => {
            let options = parse_sidebar_progress_set_options(rest)?;
            run_sidebar_set_progress(options, &mut output)
        }
        [command, rest @ ..] if command == "clear-progress" => {
            let options = parse_sidebar_workspace_options(rest, "clear-progress")?;
            run_sidebar_clear_progress(options, &mut output)
        }
        [command, rest @ ..] if command == "log" => {
            let options = parse_sidebar_log_options(rest)?;
            run_sidebar_log(options, &mut output)
        }
        [command, rest @ ..] if command == "clear-log" => {
            let options = parse_sidebar_workspace_options(rest, "clear-log")?;
            run_sidebar_clear_log(options, &mut output)
        }
        [command, rest @ ..] if command == "list-log" => {
            let options = parse_sidebar_log_list_options(rest)?;
            run_sidebar_list_log(options, &mut output)
        }
        [command, rest @ ..] if command == "sidebar-state" => {
            let options = parse_sidebar_workspace_options(rest, "sidebar-state")?;
            run_sidebar_state(options, &mut output)
        }
        [command, rest @ ..] if command == "capabilities" => {
            let options = parse_no_params_control_options(rest, "capabilities")?;
            run_capabilities(options, &mut output)
        }
        [command, rest @ ..] if command == "identify" => {
            let options = parse_identify_options(rest)?;
            run_identify(options, &mut output)
        }
        [command, rest @ ..] if command == "ping" => {
            let options = parse_no_params_control_options(rest, "ping")?;
            run_ping(options, &mut output)
        }
        [command, rest @ ..] if command == "list-workspaces" => {
            let options = parse_no_params_control_options(rest, "list-workspaces")?;
            run_workspace_list(options, &mut output)
        }
        [command, rest @ ..] if command == "new-workspace" => {
            let options = parse_cmux_new_workspace_options(rest)?;
            run_workspace_create(options, &mut output)
        }
        [command, rest @ ..] if command == "current-workspace" => {
            let options = parse_cmux_workspace_query_options(rest, "current-workspace")?;
            run_cmux_current_workspace(options, &mut output)
        }
        [command, rest @ ..] if command == "close-workspace" => {
            let options = parse_cmux_workspace_close_options(rest)?;
            run_workspace_close(options, &mut output)
        }
        [command, rest @ ..] if command == "list-surfaces" => {
            let options = parse_cmux_workspace_query_options(rest, "list-surfaces")?;
            run_cmux_list_surfaces(options, &mut output)
        }
        [command, rest @ ..] if command == "new-split" => {
            let options = parse_cmux_pane_split_options(rest)?;
            run_cmux_new_split(options, &mut output)
        }
        [command, rest @ ..] if command == "send" => {
            let options = parse_cmux_active_send_text_options(rest)?;
            run_cmux_send_text(options, &mut output)
        }
        [command, rest @ ..] if command == "send-key" => {
            let options = parse_cmux_active_send_key_options(rest)?;
            run_cmux_send_key(options, &mut output)
        }
        [command, rest @ ..] if command == "list-notifications" => {
            let options = parse_notification_list_options(rest)?;
            run_notification_list(options, &mut output)
        }
        [family, command, rest @ ..] if family == "integrations" && command == "setup" => {
            let options = parse_agent_integration_setup_options(rest, "integrations setup")?;
            run_agent_integration_setup(options, &mut output)
        }
        [family, command, rest @ ..] if family == "integrations" && command == "env" => {
            let options = parse_agent_integration_setup_options(rest, "integrations env")?;
            run_agent_integration_env(options, &mut output)
        }
        [family, command, rest @ ..] if family == "integrations" && command == "install-shims" => {
            let options = parse_agent_integration_install_options(rest)?;
            run_agent_integration_install_shims(options, &mut output)
        }
        [family, command, rest @ ..] if family == "integrations" && command == "doctor" => {
            let options = parse_agent_integration_doctor_options(rest)?;
            run_agent_integration_doctor(options, &mut output)
        }
        [command, rest @ ..] if command == "__tmux-compat" => {
            let options = parse_tmux_compat_options(rest)?;
            run_tmux_compat(options, &mut output)
        }
        [command, rest @ ..] if AgentIntegrationKind::from_command(command).is_some() => {
            let kind = AgentIntegrationKind::from_command(command).expect("kind checked");
            let options = parse_agent_integration_launch_options(kind, rest)?;
            run_agent_integration_launch(options, &mut output)
        }
        _ => {
            writeln!(output, "{}", usage_for(program_name))?;
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ControlInvokeOptions {
    json: bool,
    pipe_name: String,
    token: Option<String>,
    token_path: Option<String>,
}

impl ControlInvokeOptions {
    fn from_env() -> Self {
        Self {
            json: false,
            pipe_name: std::env::var("AGENTMUX_CONTROL_PIPE")
                .or_else(|_| std::env::var("CMUX_SOCKET_PATH"))
                .unwrap_or_else(|_| DEFAULT_CONTROL_PIPE_NAME.to_string()),
            token: std::env::var("AGENTMUX_CONTROL_TOKEN").ok(),
            token_path: std::env::var("AGENTMUX_CONTROL_TOKEN_PATH").ok(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorkspaceCreateOptions {
    invoke: ControlInvokeOptions,
    params: WorkspaceCreateParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorkspaceGetOptions {
    invoke: ControlInvokeOptions,
    params: WorkspaceIdParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorkspaceRenameOptions {
    invoke: ControlInvokeOptions,
    params: WorkspaceRenameParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorkspaceCloseOptions {
    invoke: ControlInvokeOptions,
    params: WorkspaceCloseParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorkspaceGroupCreateOptions {
    invoke: ControlInvokeOptions,
    params: WorkspaceGroupCreateParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorkspaceGroupUpdateOptions {
    invoke: ControlInvokeOptions,
    params: WorkspaceGroupUpdateParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorkspaceGroupDeleteOptions {
    invoke: ControlInvokeOptions,
    params: WorkspaceGroupIdParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorkspaceGroupMemberOptions {
    invoke: ControlInvokeOptions,
    params: WorkspaceGroupMemberParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CmuxWorkspaceQueryOptions {
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct CmuxPaneSplitOptions {
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    direction: String,
    ratio: Option<f64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CmuxActiveSendTextOptions {
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CmuxActiveSendKeyOptions {
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    key: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentIntegrationKind {
    ClaudeTeams,
    Omo,
    Omx,
    Omc,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentIntegrationLaunchOptions {
    invoke: ControlInvokeOptions,
    kind: AgentIntegrationKind,
    workspace_id: Option<String>,
    base_dir: Option<String>,
    args: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentIntegrationSetupOptions {
    invoke: ControlInvokeOptions,
    kind: AgentIntegrationKind,
    workspace_id: Option<String>,
    base_dir: Option<String>,
    install_packages: bool,
    distribution: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentIntegrationInstallOptions {
    invoke: ControlInvokeOptions,
    base_dir: Option<String>,
    bin_dir: Option<String>,
    powershell_profile: Option<String>,
    shell_profile: Option<String>,
    user_path: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentIntegrationDoctorOptions {
    invoke: ControlInvokeOptions,
    kind: Option<AgentIntegrationKind>,
    base_dir: Option<String>,
    bin_dir: Option<String>,
    distribution: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentIntegrationRuntime {
    kind: AgentIntegrationKind,
    base_dir: PathBuf,
    shim_dir: PathBuf,
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    shadow_config_dir: Option<PathBuf>,
    node_options_restore_module: Option<PathBuf>,
    package_install: Option<OmoPackageInstallResult>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OmoPackageInstallResult {
    status: &'static str,
    package_dir: PathBuf,
    package_manager: Option<String>,
    distribution: Option<String>,
    command: Vec<String>,
    node_modules_status: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TmuxAgentTeamMetadata {
    integration: String,
    agent_state: AgentSetStateParams,
    sidebar_status: SidebarStatusSetParams,
    sidebar_log: SidebarLogAddParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentIntegrationInstallResult {
    base_dir: PathBuf,
    bin_dir: PathBuf,
    wrappers: Vec<PathBuf>,
    powershell_snippet: PathBuf,
    shell_snippet: PathBuf,
    powershell_profile: Option<PathBuf>,
    shell_profile: Option<PathBuf>,
    user_path: Option<WindowsUserPathUpdate>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WindowsUserPathUpdate {
    status: &'static str,
    bin_dir: PathBuf,
    detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentIntegrationDoctorResult {
    base_dir: PathBuf,
    bin_dir: PathBuf,
    bin_dir_on_path: bool,
    wsl_distribution: Option<String>,
    integrations: Vec<AgentIntegrationDoctorItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentIntegrationDoctorItem {
    kind: AgentIntegrationKind,
    command: String,
    executable: String,
    status: &'static str,
    install_hint: &'static str,
    checks: Vec<AgentIntegrationDoctorCheck>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentIntegrationDoctorCheck {
    name: &'static str,
    ok: bool,
    detail: String,
    fix: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TmuxCompatOptions {
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    command: TmuxCompatCommand,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TmuxCompatCommand {
    DisplayMessage {
        format: Option<String>,
    },
    ListPanes {
        all_workspaces: bool,
        format: Option<String>,
    },
    ListWindows {
        all_workspaces: bool,
        format: Option<String>,
    },
    ListSessions {
        format: Option<String>,
    },
    HasSession {
        target: Option<String>,
    },
    SelectPane {
        target_pane_id: Option<String>,
    },
    SelectWindow {
        target_window: Option<String>,
    },
    SwitchClient {
        target: Option<String>,
    },
    RenameWindow {
        target_window: Option<String>,
        name: String,
    },
    RenameSession {
        target_session: Option<String>,
        name: String,
    },
    CapturePane {
        target_pane_id: Option<String>,
        max_bytes: usize,
    },
    KillPane {
        target_pane_id: Option<String>,
    },
    KillWindow {
        target_window: Option<String>,
    },
    SendKeys {
        target_pane_id: Option<String>,
        keys: Vec<String>,
    },
    SplitWindow {
        target_pane_id: Option<String>,
        axis: String,
        command: Vec<String>,
        format: Option<String>,
    },
    NewWindow {
        command: Vec<String>,
        format: Option<String>,
    },
    NewSession {
        session_name: Option<String>,
        cwd: Option<String>,
        command: Vec<String>,
        format: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SessionSpawnOptions {
    invoke: ControlInvokeOptions,
    params: SessionSpawnParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SessionListOptions {
    invoke: ControlInvokeOptions,
    params: SessionListParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SessionGetOptions {
    invoke: ControlInvokeOptions,
    params: SessionIdParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SessionSendTextOptions {
    invoke: ControlInvokeOptions,
    params: SessionSendTextParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SessionSendKeyOptions {
    invoke: ControlInvokeOptions,
    params: SessionSendKeyParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SessionReadRecentOptions {
    invoke: ControlInvokeOptions,
    params: SessionReadRecentParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SessionTerminateOptions {
    invoke: ControlInvokeOptions,
    params: SessionTerminateParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ServerOptions {
    invoke: ControlInvokeOptions,
    mode: ServerMode,
    host: String,
    port: u16,
    allow_remote: bool,
    workspace_id: Option<String>,
    backend: Option<String>,
    backend_profile: Option<String>,
    cwd: Option<String>,
    command: Vec<String>,
    columns: u16,
    rows: u16,
    max_recent_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServerMode {
    Local,
    DesktopBridge,
}

impl ServerMode {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::DesktopBridge => "desktop-bridge",
        }
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct ServerSpawnRequest {
    workspace_id: Option<String>,
    backend: Option<String>,
    backend_profile: Option<String>,
    distribution: Option<String>,
    cwd: Option<String>,
    command: Option<Vec<String>>,
    command_line: Option<String>,
    columns: Option<u16>,
    rows: Option<u16>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ServerSendTextRequest {
    text: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ServerSendKeyRequest {
    key: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ServerResizeRequest {
    columns: Option<u16>,
    rows: Option<u16>,
}

#[derive(Debug, serde::Deserialize)]
struct ServerTmuxCheckRequest {
    distribution: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentSetStateOptions {
    invoke: ControlInvokeOptions,
    params: AgentSetStateParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentGetStateOptions {
    invoke: ControlInvokeOptions,
    params: SessionIdParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentListAttentionOptions {
    invoke: ControlInvokeOptions,
    params: AgentListAttentionParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentClearAttentionOptions {
    invoke: ControlInvokeOptions,
    params: SessionIdParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NotificationListOptions {
    invoke: ControlInvokeOptions,
    params: NotificationListParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NotificationDismissOptions {
    invoke: ControlInvokeOptions,
    params: NotificationDismissParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NotificationCreateOptions {
    invoke: ControlInvokeOptions,
    params: NotificationCreateParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NotificationClearOptions {
    invoke: ControlInvokeOptions,
    params: NotificationClearParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SidebarStatusSetOptions {
    invoke: ControlInvokeOptions,
    params: SidebarStatusSetParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SidebarStatusKeyOptions {
    invoke: ControlInvokeOptions,
    params: SidebarStatusKeyParams,
}

#[derive(Clone, Debug, PartialEq)]
struct SidebarProgressSetOptions {
    invoke: ControlInvokeOptions,
    params: SidebarProgressSetParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SidebarWorkspaceOptions {
    invoke: ControlInvokeOptions,
    params: SidebarWorkspaceParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SidebarLogOptions {
    invoke: ControlInvokeOptions,
    params: SidebarLogAddParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SidebarLogListOptions {
    invoke: ControlInvokeOptions,
    params: SidebarLogListParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IdentifyOptions {
    invoke: ControlInvokeOptions,
    params: SystemIdentifyParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConfigGetOptions {
    invoke: ControlInvokeOptions,
    params: AppConfigGetParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConfigMigrateProjectOptions {
    invoke: ControlInvokeOptions,
    params: AppConfigMigrateProjectParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConfigDiagnosticsOptions {
    invoke: ControlInvokeOptions,
    params: AppConfigDiagnosticsParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConfigSchemaOptions {
    json: bool,
    output_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActionListOptions {
    invoke: ControlInvokeOptions,
    params: ActionListParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActionRunOptions {
    invoke: ControlInvokeOptions,
    params: ActionRunParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserOpenOptions {
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    pane_id: Option<String>,
    profile: Option<String>,
    placement: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserNavigateOptions {
    invoke: ControlInvokeOptions,
    params: BrowserNavigateParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserSurfaceCommandOptions {
    invoke: ControlInvokeOptions,
    params: BrowserSurfaceParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserScreenshotOptions {
    invoke: ControlInvokeOptions,
    params: BrowserScreenshotParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserDomSnapshotOptions {
    invoke: ControlInvokeOptions,
    params: BrowserDomSnapshotParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserConsoleOptions {
    invoke: ControlInvokeOptions,
    params: BrowserConsoleParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserDialogsOptions {
    invoke: ControlInvokeOptions,
    params: BrowserDialogsParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserDownloadsOptions {
    invoke: ControlInvokeOptions,
    params: BrowserDownloadsParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserErrorsOptions {
    invoke: ControlInvokeOptions,
    params: BrowserErrorsParams,
}

#[derive(Clone, Debug, PartialEq)]
struct BrowserClickOptions {
    invoke: ControlInvokeOptions,
    params: BrowserClickParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserTypeOptions {
    invoke: ControlInvokeOptions,
    params: BrowserTypeParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserFillOptions {
    invoke: ControlInvokeOptions,
    params: BrowserFillParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserPressOptions {
    invoke: ControlInvokeOptions,
    params: BrowserPressParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserSelectOptions {
    invoke: ControlInvokeOptions,
    params: BrowserSelectParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserScrollOptions {
    invoke: ControlInvokeOptions,
    params: BrowserScrollParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserHoverOptions {
    invoke: ControlInvokeOptions,
    params: BrowserHoverParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserCheckOptions {
    invoke: ControlInvokeOptions,
    params: BrowserCheckParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserGetOptions {
    invoke: ControlInvokeOptions,
    params: BrowserGetParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserFindOptions {
    invoke: ControlInvokeOptions,
    params: BrowserFindParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserHighlightOptions {
    invoke: ControlInvokeOptions,
    params: BrowserHighlightParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserFocusOptions {
    invoke: ControlInvokeOptions,
    params: BrowserFocusParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserZoomOptions {
    invoke: ControlInvokeOptions,
    params: BrowserZoomParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserWaitForSelectorOptions {
    invoke: ControlInvokeOptions,
    params: BrowserWaitForSelectorParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserEvaluateOptions {
    invoke: ControlInvokeOptions,
    params: BrowserEvaluateParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BrowserDiagnosticsOptions {
    invoke: ControlInvokeOptions,
    params: BrowserDiagnosticsParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SshOptions {
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    pane_id: Option<String>,
    target: String,
    placement: Option<String>,
    columns: u16,
    rows: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EventPollOptions {
    invoke: ControlInvokeOptions,
    params: EventPollParams,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EventWatchOptions {
    invoke: ControlInvokeOptions,
    params: EventSubscribeParams,
    interval_ms: u64,
    once: bool,
    limit: Option<usize>,
}

fn parse_workspace_create_options(args: &[String]) -> Result<WorkspaceCreateOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut name = None;
    let mut project_root = None;
    let mut backend_profile = None;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--project" => {
                project_root = Some(option_value(args, index, "--project")?.to_string());
                index += 2;
            }
            "--backend-profile" | "--distribution" => {
                backend_profile =
                    Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown workspace create option '{value}'."
                )));
            }
            value => {
                if name.is_some() {
                    return Err(CliError::InvalidArgs(
                        "workspace create accepts exactly one name.".to_string(),
                    ));
                }
                name = Some(value.to_string());
                index += 1;
            }
        }
    }

    let name = name.ok_or_else(|| {
        CliError::InvalidArgs("workspace create requires a workspace name.".to_string())
    })?;

    Ok(WorkspaceCreateOptions {
        invoke,
        params: WorkspaceCreateParams {
            name,
            project_root,
            backend_profile,
        },
    })
}

fn parse_no_params_control_options(
    args: &[String],
    command_name: &str,
) -> Result<ControlInvokeOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        return Err(CliError::InvalidArgs(format!(
            "{command_name} does not accept argument '{}'.",
            args[index]
        )));
    }
    Ok(invoke)
}

fn parse_config_get_options(
    args: &[String],
    command_name: &str,
) -> Result<ConfigGetOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown {command_name} option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "{command_name} does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(ConfigGetOptions {
        invoke,
        params: AppConfigGetParams { workspace_id },
    })
}

fn parse_config_migrate_project_options(
    args: &[String],
) -> Result<ConfigMigrateProjectOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut overwrite = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--force" | "--overwrite" => {
                overwrite = Some(true);
                index += 1;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown config migrate-cmux option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "config migrate-cmux does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(ConfigMigrateProjectOptions {
        invoke,
        params: AppConfigMigrateProjectParams {
            workspace_id,
            overwrite,
        },
    })
}

fn parse_config_diagnostics_options(args: &[String]) -> Result<ConfigDiagnosticsOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown config diagnostics option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "config diagnostics does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(ConfigDiagnosticsOptions {
        invoke,
        params: AppConfigDiagnosticsParams { workspace_id },
    })
}

fn parse_config_schema_options(args: &[String]) -> Result<ConfigSchemaOptions, CliError> {
    let mut json = false;
    let mut output_path = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                json = true;
                index += 1;
            }
            "--output" | "-o" => {
                output_path = Some(PathBuf::from(option_value(
                    args,
                    index,
                    args[index].as_str(),
                )?));
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown config schema option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "config schema does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(ConfigSchemaOptions { json, output_path })
}

fn parse_action_list_options(args: &[String]) -> Result<ActionListOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown actions list option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "actions list does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(ActionListOptions {
        invoke,
        params: ActionListParams { workspace_id },
    })
}

fn parse_action_run_options(args: &[String]) -> Result<ActionRunOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut pane_id = pane_from_env();
    let mut action_id = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--pane" => {
                pane_id = Some(normalize_tmux_pane_id(option_value(args, index, "--pane")?));
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown actions run option '{value}'."
                )));
            }
            value => {
                if action_id.is_some() {
                    return Err(CliError::InvalidArgs(
                        "actions run accepts exactly one action id.".to_string(),
                    ));
                }
                action_id = Some(value.to_string());
                index += 1;
            }
        }
    }
    Ok(ActionRunOptions {
        invoke,
        params: ActionRunParams {
            action_id: action_id.ok_or_else(|| {
                CliError::InvalidArgs("actions run requires an action id.".to_string())
            })?,
            workspace_id,
            pane_id,
        },
    })
}

fn parse_browser_open_options(args: &[String]) -> Result<BrowserOpenOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut pane_id = pane_from_env();
    let mut profile = None;
    let mut placement = Some("new_tab".to_string());
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--pane" => {
                pane_id = Some(normalize_tmux_pane_id(option_value(args, index, "--pane")?));
                index += 2;
            }
            "--profile" => {
                profile = Some(option_value(args, index, "--profile")?.to_string());
                index += 2;
            }
            "--placement" => {
                placement = Some(normalize_browser_cli_placement(option_value(
                    args,
                    index,
                    "--placement",
                )?)?);
                index += 2;
            }
            "--new-tab" => {
                placement = Some("new_tab".to_string());
                index += 1;
            }
            "--active-pane" => {
                placement = Some("active_pane".to_string());
                index += 1;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser open option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "browser open does not accept argument '{value}'. Use browser navigate <surface-id> <url> after opening a surface."
                )));
            }
        }
    }

    Ok(BrowserOpenOptions {
        invoke,
        workspace_id,
        pane_id,
        profile,
        placement,
    })
}

fn parse_browser_navigate_options(args: &[String]) -> Result<BrowserNavigateOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut url = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser navigate option '{value}'."
                )));
            }
            value => {
                if surface_id.is_none() {
                    surface_id = Some(value.to_string());
                } else if url.is_none() {
                    url = Some(value.to_string());
                } else {
                    return Err(CliError::InvalidArgs(
                        "browser navigate accepts exactly a surface id and URL.".to_string(),
                    ));
                }
                index += 1;
            }
        }
    }

    Ok(BrowserNavigateOptions {
        invoke,
        params: BrowserNavigateParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser navigate requires a surface id.".to_string())
            })?,
            url: url.ok_or_else(|| {
                CliError::InvalidArgs("browser navigate requires a URL.".to_string())
            })?,
        },
    })
}

fn parse_browser_surface_command_options(
    args: &[String],
    command_name: &str,
) -> Result<BrowserSurfaceCommandOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser {command_name} option '{value}'."
                )));
            }
            value => {
                if surface_id.is_some() {
                    return Err(CliError::InvalidArgs(format!(
                        "browser {command_name} accepts exactly one surface id."
                    )));
                }
                surface_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    Ok(BrowserSurfaceCommandOptions {
        invoke,
        params: BrowserSurfaceParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs(format!("browser {command_name} requires a surface id."))
            })?,
        },
    })
}

fn parse_browser_screenshot_options(args: &[String]) -> Result<BrowserScreenshotOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut format = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--format" => {
                format = Some(option_value(args, index, "--format")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser screenshot option '{value}'."
                )));
            }
            value => {
                if surface_id.is_some() {
                    return Err(CliError::InvalidArgs(
                        "browser screenshot accepts exactly one surface id.".to_string(),
                    ));
                }
                surface_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    Ok(BrowserScreenshotOptions {
        invoke,
        params: BrowserScreenshotParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser screenshot requires a surface id.".to_string())
            })?,
            format,
        },
    })
}

fn parse_browser_dom_snapshot_options(
    args: &[String],
) -> Result<BrowserDomSnapshotOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut frame_id = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser dom-snapshot option '{value}'."
                )));
            }
            value => {
                if surface_id.is_some() {
                    return Err(CliError::InvalidArgs(
                        "browser dom-snapshot accepts exactly one surface id.".to_string(),
                    ));
                }
                surface_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    Ok(BrowserDomSnapshotOptions {
        invoke,
        params: BrowserDomSnapshotParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser dom-snapshot requires a surface id.".to_string())
            })?,
            frame_id,
        },
    })
}

fn parse_browser_console_options(args: &[String]) -> Result<BrowserConsoleOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut limit = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--limit" => {
                limit = Some(parse_usize_option(
                    option_value(args, index, "--limit")?,
                    "--limit",
                )?);
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser console option '{value}'."
                )));
            }
            value => {
                if surface_id.is_some() {
                    return Err(CliError::InvalidArgs(
                        "browser console accepts exactly one surface id.".to_string(),
                    ));
                }
                surface_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    Ok(BrowserConsoleOptions {
        invoke,
        params: BrowserConsoleParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser console requires a surface id.".to_string())
            })?,
            limit,
        },
    })
}

fn parse_browser_dialogs_options(args: &[String]) -> Result<BrowserDialogsOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut limit = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--limit" => {
                limit = Some(parse_usize_option(
                    option_value(args, index, "--limit")?,
                    "--limit",
                )?);
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser dialogs option '{value}'."
                )));
            }
            value => {
                if surface_id.is_some() {
                    return Err(CliError::InvalidArgs(
                        "browser dialogs accepts exactly one surface id.".to_string(),
                    ));
                }
                surface_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    Ok(BrowserDialogsOptions {
        invoke,
        params: BrowserDialogsParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser dialogs requires a surface id.".to_string())
            })?,
            limit,
        },
    })
}

fn parse_browser_downloads_options(args: &[String]) -> Result<BrowserDownloadsOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut limit = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--limit" => {
                limit = Some(parse_usize_option(
                    option_value(args, index, "--limit")?,
                    "--limit",
                )?);
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser downloads option '{value}'."
                )));
            }
            value => {
                if surface_id.is_some() {
                    return Err(CliError::InvalidArgs(
                        "browser downloads accepts exactly one surface id.".to_string(),
                    ));
                }
                surface_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    Ok(BrowserDownloadsOptions {
        invoke,
        params: BrowserDownloadsParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser downloads requires a surface id.".to_string())
            })?,
            limit,
        },
    })
}

fn parse_browser_errors_options(args: &[String]) -> Result<BrowserErrorsOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut limit = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--limit" => {
                limit = Some(parse_usize_option(
                    option_value(args, index, "--limit")?,
                    "--limit",
                )?);
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser errors option '{value}'."
                )));
            }
            value => {
                if surface_id.is_some() {
                    return Err(CliError::InvalidArgs(
                        "browser errors accepts exactly one surface id.".to_string(),
                    ));
                }
                surface_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    Ok(BrowserErrorsOptions {
        invoke,
        params: BrowserErrorsParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser errors requires a surface id.".to_string())
            })?,
            limit,
        },
    })
}

fn parse_browser_click_options(args: &[String]) -> Result<BrowserClickOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut selector = None;
    let mut x = None;
    let mut y = None;
    let mut frame_id = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--selector" => {
                selector = Some(option_value(args, index, "--selector")?.to_string());
                index += 2;
            }
            "--x" => {
                x = Some(parse_f64_option(option_value(args, index, "--x")?, "--x")?);
                index += 2;
            }
            "--y" => {
                y = Some(parse_f64_option(option_value(args, index, "--y")?, "--y")?);
                index += 2;
            }
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser click option '{value}'."
                )));
            }
            value => {
                if surface_id.is_none() {
                    surface_id = Some(value.to_string());
                } else if selector.is_none() {
                    selector = Some(value.to_string());
                } else {
                    return Err(CliError::InvalidArgs(
                        "browser click accepts a surface id plus either --selector or --x/--y."
                            .to_string(),
                    ));
                }
                index += 1;
            }
        }
    }

    if selector.is_none() && (x.is_none() || y.is_none()) {
        return Err(CliError::InvalidArgs(
            "browser click requires --selector <css> or both --x and --y.".to_string(),
        ));
    }

    Ok(BrowserClickOptions {
        invoke,
        params: BrowserClickParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser click requires a surface id.".to_string())
            })?,
            selector,
            x,
            y,
            frame_id,
        },
    })
}

fn parse_browser_type_options(args: &[String]) -> Result<BrowserTypeOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut selector = None;
    let mut text = None;
    let mut frame_id = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--" {
            index += 1;
            text = Some(args[index..].join(" "));
            break;
        }

        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--selector" => {
                selector = Some(option_value(args, index, "--selector")?.to_string());
                index += 2;
            }
            "--text" => {
                text = Some(option_value(args, index, "--text")?.to_string());
                index += 2;
            }
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser type option '{value}'."
                )));
            }
            value => {
                positional.push(value.to_string());
                index += 1;
            }
        }
    }

    if surface_id.is_none() && !positional.is_empty() {
        surface_id = Some(positional.remove(0));
    }
    if selector.is_none() && !positional.is_empty() {
        selector = Some(positional.remove(0));
    }
    if text.is_none() && !positional.is_empty() {
        text = Some(positional.join(" "));
        positional.clear();
    }
    if !positional.is_empty() {
        return Err(CliError::InvalidArgs(
            "browser type accepts a surface id, selector, and text.".to_string(),
        ));
    }

    Ok(BrowserTypeOptions {
        invoke,
        params: BrowserTypeParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser type requires a surface id.".to_string())
            })?,
            selector: selector.ok_or_else(|| {
                CliError::InvalidArgs("browser type requires a selector.".to_string())
            })?,
            text: text
                .ok_or_else(|| CliError::InvalidArgs("browser type requires text.".to_string()))?,
            frame_id,
        },
    })
}

fn parse_browser_fill_options(args: &[String]) -> Result<BrowserFillOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut selector = None;
    let mut text = None;
    let mut frame_id = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--" {
            index += 1;
            text = Some(args[index..].join(" "));
            break;
        }

        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--selector" => {
                selector = Some(option_value(args, index, "--selector")?.to_string());
                index += 2;
            }
            "--text" => {
                text = Some(option_value(args, index, "--text")?.to_string());
                index += 2;
            }
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser fill option '{value}'."
                )));
            }
            value => {
                positional.push(value.to_string());
                index += 1;
            }
        }
    }

    if surface_id.is_none() && !positional.is_empty() {
        surface_id = Some(positional.remove(0));
    }
    if selector.is_none() && !positional.is_empty() {
        selector = Some(positional.remove(0));
    }
    if text.is_none() && !positional.is_empty() {
        text = Some(positional.join(" "));
        positional.clear();
    }
    if !positional.is_empty() {
        return Err(CliError::InvalidArgs(
            "browser fill accepts a surface id, selector, and text.".to_string(),
        ));
    }

    Ok(BrowserFillOptions {
        invoke,
        params: BrowserFillParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser fill requires a surface id.".to_string())
            })?,
            selector: selector.ok_or_else(|| {
                CliError::InvalidArgs("browser fill requires a selector.".to_string())
            })?,
            text: text
                .ok_or_else(|| CliError::InvalidArgs("browser fill requires text.".to_string()))?,
            frame_id,
        },
    })
}

fn parse_browser_press_options(args: &[String]) -> Result<BrowserPressOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut selector = None;
    let mut key = None;
    let mut frame_id = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--selector" => {
                selector = Some(option_value(args, index, "--selector")?.to_string());
                index += 2;
            }
            "--key" => {
                key = Some(option_value(args, index, "--key")?.to_string());
                index += 2;
            }
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser press option '{value}'."
                )));
            }
            value => {
                positional.push(value.to_string());
                index += 1;
            }
        }
    }

    if surface_id.is_none() && !positional.is_empty() {
        surface_id = Some(positional.remove(0));
    }
    if selector.is_none() && !positional.is_empty() {
        selector = Some(positional.remove(0));
    }
    if key.is_none() && !positional.is_empty() {
        key = Some(positional.remove(0));
    }
    if !positional.is_empty() {
        return Err(CliError::InvalidArgs(
            "browser press accepts a surface id, selector, and key.".to_string(),
        ));
    }

    Ok(BrowserPressOptions {
        invoke,
        params: BrowserPressParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser press requires a surface id.".to_string())
            })?,
            selector: selector.ok_or_else(|| {
                CliError::InvalidArgs("browser press requires a selector.".to_string())
            })?,
            key: key.ok_or_else(|| {
                CliError::InvalidArgs("browser press requires a key.".to_string())
            })?,
            frame_id,
        },
    })
}

fn parse_browser_select_options(args: &[String]) -> Result<BrowserSelectOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut selector = None;
    let mut values = Vec::new();
    let mut frame_id = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--selector" => {
                selector = Some(option_value(args, index, "--selector")?.to_string());
                index += 2;
            }
            "--value" => {
                values.push(option_value(args, index, "--value")?.to_string());
                index += 2;
            }
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser select option '{value}'."
                )));
            }
            value => {
                positional.push(value.to_string());
                index += 1;
            }
        }
    }

    if surface_id.is_none() && !positional.is_empty() {
        surface_id = Some(positional.remove(0));
    }
    if selector.is_none() && !positional.is_empty() {
        selector = Some(positional.remove(0));
    }
    if values.is_empty() && !positional.is_empty() {
        values.append(&mut positional);
    }
    if values.is_empty() {
        return Err(CliError::InvalidArgs(
            "browser select requires at least one value.".to_string(),
        ));
    }

    Ok(BrowserSelectOptions {
        invoke,
        params: BrowserSelectParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser select requires a surface id.".to_string())
            })?,
            selector: selector.ok_or_else(|| {
                CliError::InvalidArgs("browser select requires a selector.".to_string())
            })?,
            values,
            frame_id,
        },
    })
}

fn parse_browser_scroll_options(args: &[String]) -> Result<BrowserScrollOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut selector = None;
    let mut x = None;
    let mut y = None;
    let mut frame_id = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--selector" => {
                selector = Some(option_value(args, index, "--selector")?.to_string());
                index += 2;
            }
            "--x" => {
                x = Some(parse_i32_option(option_value(args, index, "--x")?, "--x")?);
                index += 2;
            }
            "--y" => {
                y = Some(parse_i32_option(option_value(args, index, "--y")?, "--y")?);
                index += 2;
            }
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser scroll option '{value}'."
                )));
            }
            value => {
                if surface_id.is_some() {
                    return Err(CliError::InvalidArgs(
                        "browser scroll accepts exactly one surface id plus options.".to_string(),
                    ));
                }
                surface_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    Ok(BrowserScrollOptions {
        invoke,
        params: BrowserScrollParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser scroll requires a surface id.".to_string())
            })?,
            selector,
            x,
            y,
            frame_id,
        },
    })
}

fn parse_browser_hover_options(args: &[String]) -> Result<BrowserHoverOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut selector = None;
    let mut frame_id = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--selector" => {
                selector = Some(option_value(args, index, "--selector")?.to_string());
                index += 2;
            }
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser hover option '{value}'."
                )));
            }
            value => {
                positional.push(value.to_string());
                index += 1;
            }
        }
    }

    if surface_id.is_none() && !positional.is_empty() {
        surface_id = Some(positional.remove(0));
    }
    if selector.is_none() && !positional.is_empty() {
        selector = Some(positional.remove(0));
    }
    if !positional.is_empty() {
        return Err(CliError::InvalidArgs(
            "browser hover accepts a surface id and selector.".to_string(),
        ));
    }

    Ok(BrowserHoverOptions {
        invoke,
        params: BrowserHoverParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser hover requires a surface id.".to_string())
            })?,
            selector: selector.ok_or_else(|| {
                CliError::InvalidArgs("browser hover requires a selector.".to_string())
            })?,
            frame_id,
        },
    })
}

fn parse_browser_check_options(args: &[String]) -> Result<BrowserCheckOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut selector = None;
    let mut checked = None;
    let mut frame_id = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--selector" => {
                selector = Some(option_value(args, index, "--selector")?.to_string());
                index += 2;
            }
            "--checked" => {
                checked = Some(parse_bool_option(
                    option_value(args, index, "--checked")?,
                    "--checked",
                )?);
                index += 2;
            }
            "--unchecked" => {
                checked = Some(false);
                index += 1;
            }
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser check option '{value}'."
                )));
            }
            value => {
                positional.push(value.to_string());
                index += 1;
            }
        }
    }

    if surface_id.is_none() && !positional.is_empty() {
        surface_id = Some(positional.remove(0));
    }
    if selector.is_none() && !positional.is_empty() {
        selector = Some(positional.remove(0));
    }
    if checked.is_none() && !positional.is_empty() {
        checked = Some(parse_bool_option(&positional.remove(0), "checked")?);
    }
    if !positional.is_empty() {
        return Err(CliError::InvalidArgs(
            "browser check accepts a surface id, selector, and optional checked value.".to_string(),
        ));
    }

    Ok(BrowserCheckOptions {
        invoke,
        params: BrowserCheckParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser check requires a surface id.".to_string())
            })?,
            selector: selector.ok_or_else(|| {
                CliError::InvalidArgs("browser check requires a selector.".to_string())
            })?,
            checked,
            frame_id,
        },
    })
}

fn parse_browser_get_options(args: &[String]) -> Result<BrowserGetOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut selector = None;
    let mut kind = None;
    let mut attribute = None;
    let mut frame_id = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--selector" => {
                selector = Some(option_value(args, index, "--selector")?.to_string());
                index += 2;
            }
            "--kind" => {
                kind = Some(option_value(args, index, "--kind")?.to_string());
                index += 2;
            }
            "--attribute" | "--attr" => {
                attribute = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser get option '{value}'."
                )));
            }
            value => {
                positional.push(value.to_string());
                index += 1;
            }
        }
    }

    if surface_id.is_none() && !positional.is_empty() {
        surface_id = Some(positional.remove(0));
    }
    if selector.is_none() && !positional.is_empty() {
        selector = Some(positional.remove(0));
    }
    if kind.is_none() && !positional.is_empty() {
        kind = Some(positional.remove(0));
    }
    if !positional.is_empty() {
        return Err(CliError::InvalidArgs(
            "browser get accepts a surface id, selector, and optional kind.".to_string(),
        ));
    }

    Ok(BrowserGetOptions {
        invoke,
        params: BrowserGetParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser get requires a surface id.".to_string())
            })?,
            selector: selector.ok_or_else(|| {
                CliError::InvalidArgs("browser get requires a selector.".to_string())
            })?,
            kind,
            attribute,
            frame_id,
        },
    })
}

fn parse_browser_find_options(args: &[String]) -> Result<BrowserFindOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut query = None;
    let mut selector = None;
    let mut limit = None;
    let mut frame_id = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--query" => {
                query = Some(option_value(args, index, "--query")?.to_string());
                index += 2;
            }
            "--selector" => {
                selector = Some(option_value(args, index, "--selector")?.to_string());
                index += 2;
            }
            "--limit" => {
                limit = Some(parse_u16_option(
                    option_value(args, index, "--limit")?,
                    "--limit",
                )?);
                index += 2;
            }
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser find option '{value}'."
                )));
            }
            value => {
                positional.push(value.to_string());
                index += 1;
            }
        }
    }

    if surface_id.is_none() && !positional.is_empty() {
        surface_id = Some(positional.remove(0));
    }
    if query.is_none() && !positional.is_empty() {
        query = Some(positional.remove(0));
    }
    if selector.is_none() && !positional.is_empty() {
        selector = Some(positional.remove(0));
    }
    if !positional.is_empty() {
        return Err(CliError::InvalidArgs(
            "browser find accepts a surface id, query, and optional selector.".to_string(),
        ));
    }

    Ok(BrowserFindOptions {
        invoke,
        params: BrowserFindParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser find requires a surface id.".to_string())
            })?,
            query: query.ok_or_else(|| {
                CliError::InvalidArgs("browser find requires a query.".to_string())
            })?,
            selector,
            limit,
            frame_id,
        },
    })
}

fn parse_browser_highlight_options(args: &[String]) -> Result<BrowserHighlightOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut selector = None;
    let mut duration_ms = None;
    let mut frame_id = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--selector" => {
                selector = Some(option_value(args, index, "--selector")?.to_string());
                index += 2;
            }
            "--duration-ms" | "--duration" => {
                duration_ms = Some(parse_u64_option(
                    option_value(args, index, args[index].as_str())?,
                    args[index].as_str(),
                )?);
                index += 2;
            }
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser highlight option '{value}'."
                )));
            }
            value => {
                positional.push(value.to_string());
                index += 1;
            }
        }
    }

    if surface_id.is_none() && !positional.is_empty() {
        surface_id = Some(positional.remove(0));
    }
    if selector.is_none() && !positional.is_empty() {
        selector = Some(positional.remove(0));
    }
    if !positional.is_empty() {
        return Err(CliError::InvalidArgs(
            "browser highlight accepts a surface id and selector.".to_string(),
        ));
    }

    Ok(BrowserHighlightOptions {
        invoke,
        params: BrowserHighlightParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser highlight requires a surface id.".to_string())
            })?,
            selector: selector.ok_or_else(|| {
                CliError::InvalidArgs("browser highlight requires a selector.".to_string())
            })?,
            duration_ms,
            frame_id,
        },
    })
}

fn parse_browser_focus_options(args: &[String]) -> Result<BrowserFocusOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut selector = None;
    let mut frame_id = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--selector" => {
                selector = Some(option_value(args, index, "--selector")?.to_string());
                index += 2;
            }
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser focus option '{value}'."
                )));
            }
            value => {
                positional.push(value.to_string());
                index += 1;
            }
        }
    }

    if surface_id.is_none() && !positional.is_empty() {
        surface_id = Some(positional.remove(0));
    }
    if selector.is_none() && !positional.is_empty() {
        selector = Some(positional.remove(0));
    }
    if !positional.is_empty() {
        return Err(CliError::InvalidArgs(
            "browser focus accepts a surface id and selector.".to_string(),
        ));
    }

    Ok(BrowserFocusOptions {
        invoke,
        params: BrowserFocusParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser focus requires a surface id.".to_string())
            })?,
            selector: selector.ok_or_else(|| {
                CliError::InvalidArgs("browser focus requires a selector.".to_string())
            })?,
            frame_id,
        },
    })
}

fn parse_browser_zoom_options(args: &[String]) -> Result<BrowserZoomOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut percent = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--percent" => {
                percent = Some(parse_u16_option(
                    option_value(args, index, "--percent")?,
                    "--percent",
                )?);
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser zoom option '{value}'."
                )));
            }
            value => {
                positional.push(value.to_string());
                index += 1;
            }
        }
    }

    if surface_id.is_none() && !positional.is_empty() {
        surface_id = Some(positional.remove(0));
    }
    if percent.is_none() && !positional.is_empty() {
        percent = Some(parse_u16_option(&positional.remove(0), "percent")?);
    }
    if !positional.is_empty() {
        return Err(CliError::InvalidArgs(
            "browser zoom accepts a surface id and percent.".to_string(),
        ));
    }
    if let Some(percent) = percent {
        if !(25..=500).contains(&percent) {
            return Err(CliError::InvalidArgs(
                "browser zoom percent must be between 25 and 500.".to_string(),
            ));
        }
    }

    Ok(BrowserZoomOptions {
        invoke,
        params: BrowserZoomParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser zoom requires a surface id.".to_string())
            })?,
            percent: percent.ok_or_else(|| {
                CliError::InvalidArgs("browser zoom requires a percent.".to_string())
            })?,
        },
    })
}

fn parse_browser_wait_for_selector_options(
    args: &[String],
) -> Result<BrowserWaitForSelectorOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut selector = None;
    let mut timeout_ms = None;
    let mut frame_id = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--selector" => {
                selector = Some(option_value(args, index, "--selector")?.to_string());
                index += 2;
            }
            "--timeout-ms" | "--timeout" => {
                timeout_ms = Some(parse_u64_option(
                    option_value(args, index, args[index].as_str())?,
                    args[index].as_str(),
                )?);
                index += 2;
            }
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser wait-for-selector option '{value}'."
                )));
            }
            value => {
                positional.push(value.to_string());
                index += 1;
            }
        }
    }

    if surface_id.is_none() && !positional.is_empty() {
        surface_id = Some(positional.remove(0));
    }
    if selector.is_none() && !positional.is_empty() {
        selector = Some(positional.remove(0));
    }
    if !positional.is_empty() {
        return Err(CliError::InvalidArgs(
            "browser wait-for-selector accepts a surface id and selector.".to_string(),
        ));
    }

    Ok(BrowserWaitForSelectorOptions {
        invoke,
        params: BrowserWaitForSelectorParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs(
                    "browser wait-for-selector requires a surface id.".to_string(),
                )
            })?,
            selector: selector.ok_or_else(|| {
                CliError::InvalidArgs("browser wait-for-selector requires a selector.".to_string())
            })?,
            timeout_ms,
            frame_id,
        },
    })
}

fn parse_browser_evaluate_options(args: &[String]) -> Result<BrowserEvaluateOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut surface_id = None;
    let mut script = None;
    let mut frame_id = None;
    let mut script_parts = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--" {
            index += 1;
            script = Some(args[index..].join(" "));
            break;
        }

        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--frame" | "--frame-id" | "--frame_id" => {
                frame_id = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            "--script" => {
                script = Some(option_value(args, index, "--script")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser evaluate option '{value}'."
                )));
            }
            value => {
                if surface_id.is_none() {
                    surface_id = Some(value.to_string());
                } else {
                    script_parts.push(value.to_string());
                }
                index += 1;
            }
        }
    }

    if script.is_none() && !script_parts.is_empty() {
        script = Some(script_parts.join(" "));
    }

    Ok(BrowserEvaluateOptions {
        invoke,
        params: BrowserEvaluateParams {
            surface_id: surface_id.ok_or_else(|| {
                CliError::InvalidArgs("browser evaluate requires a surface id.".to_string())
            })?,
            script: script.ok_or_else(|| {
                CliError::InvalidArgs("browser evaluate requires a script.".to_string())
            })?,
            frame_id,
        },
    })
}

fn parse_browser_diagnostics_options(
    args: &[String],
) -> Result<BrowserDiagnosticsOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut surface_id = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--surface" => {
                surface_id = Some(option_value(args, index, "--surface")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown browser diagnostics option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "browser diagnostics does not accept argument '{value}'."
                )));
            }
        }
    }

    Ok(BrowserDiagnosticsOptions {
        invoke,
        params: BrowserDiagnosticsParams {
            workspace_id,
            surface_id,
        },
    })
}

fn parse_ssh_options(args: &[String]) -> Result<SshOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut pane_id = pane_from_env();
    let mut target = None;
    let mut placement = Some("new_tab".to_string());
    let mut columns = 120;
    let mut rows = 30;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--pane" => {
                pane_id = Some(normalize_tmux_pane_id(option_value(args, index, "--pane")?));
                index += 2;
            }
            "--target" | "--profile" => {
                target = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            "--placement" => {
                placement = Some(normalize_ssh_cli_placement(option_value(
                    args,
                    index,
                    "--placement",
                )?)?);
                index += 2;
            }
            "--new-tab" => {
                placement = Some("new_tab".to_string());
                index += 1;
            }
            "--active-pane" => {
                placement = Some("active_pane".to_string());
                index += 1;
            }
            "--columns" => {
                columns = parse_u16_option(option_value(args, index, "--columns")?, "--columns")?;
                index += 2;
            }
            "--rows" => {
                rows = parse_u16_option(option_value(args, index, "--rows")?, "--rows")?;
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown ssh option '{value}'."
                )));
            }
            value => {
                if target.is_some() {
                    return Err(CliError::InvalidArgs(
                        "ssh accepts exactly one target or profile name.".to_string(),
                    ));
                }
                target = Some(value.to_string());
                index += 1;
            }
        }
    }

    Ok(SshOptions {
        invoke,
        workspace_id,
        pane_id,
        target: target.ok_or_else(|| {
            CliError::InvalidArgs(
                "ssh requires a target: user@host[:port] or saved profile name/id.".to_string(),
            )
        })?,
        placement,
        columns,
        rows,
    })
}

fn normalize_browser_cli_placement(value: &str) -> Result<String, CliError> {
    match value {
        "new-tab" | "new_tab" => Ok("new_tab".to_string()),
        "active-pane" | "active_pane" => Ok("active_pane".to_string()),
        other => Err(CliError::InvalidArgs(format!(
            "browser placement must be new-tab or active-pane; got '{other}'."
        ))),
    }
}

fn normalize_ssh_cli_placement(value: &str) -> Result<String, CliError> {
    match value {
        "new-tab" | "new_tab" => Ok("new_tab".to_string()),
        "active-pane" | "active_pane" => Ok("active_pane".to_string()),
        other => Err(CliError::InvalidArgs(format!(
            "ssh placement must be new-tab or active-pane; got '{other}'."
        ))),
    }
}

fn parse_workspace_get_options(args: &[String]) -> Result<WorkspaceGetOptions, CliError> {
    let (invoke, workspace_id) = parse_one_id_command(args, "workspace get", "workspace id")?;
    Ok(WorkspaceGetOptions {
        invoke,
        params: WorkspaceIdParams { workspace_id },
    })
}

fn parse_workspace_rename_options(args: &[String]) -> Result<WorkspaceRenameOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = None;
    let mut name = None;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        let value = args[index].as_str();
        if value.starts_with("--") {
            return Err(CliError::InvalidArgs(format!(
                "unknown workspace rename option '{value}'."
            )));
        }

        if workspace_id.is_none() {
            workspace_id = Some(value.to_string());
        } else if name.is_none() {
            name = Some(value.to_string());
        } else {
            return Err(CliError::InvalidArgs(
                "workspace rename accepts exactly a workspace id and name.".to_string(),
            ));
        }
        index += 1;
    }

    Ok(WorkspaceRenameOptions {
        invoke,
        params: WorkspaceRenameParams {
            workspace_id: workspace_id.ok_or_else(|| {
                CliError::InvalidArgs("workspace rename requires a workspace id.".to_string())
            })?,
            name: name.ok_or_else(|| {
                CliError::InvalidArgs("workspace rename requires a new name.".to_string())
            })?,
        },
    })
}

fn parse_workspace_close_options(args: &[String]) -> Result<WorkspaceCloseOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = None;
    let mut close_policy = "fail_if_running".to_string();
    let mut confirmed = false;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--policy" | "--close-policy" => {
                close_policy = option_value(args, index, args[index].as_str())?.to_string();
                index += 2;
            }
            "--yes" | "--confirm" => {
                confirmed = true;
                index += 1;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown workspace close option '{value}'."
                )));
            }
            value => {
                if workspace_id.is_some() {
                    return Err(CliError::InvalidArgs(
                        "workspace close accepts exactly one workspace id.".to_string(),
                    ));
                }
                workspace_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    require_confirmation(
        confirmed,
        "workspace close requires --yes because it removes a workspace.",
    )?;
    validate_workspace_close_policy(&close_policy)?;

    Ok(WorkspaceCloseOptions {
        invoke,
        params: WorkspaceCloseParams {
            workspace_id: workspace_id.ok_or_else(|| {
                CliError::InvalidArgs("workspace close requires a workspace id.".to_string())
            })?,
            close_policy,
        },
    })
}

fn parse_workspace_group_create_options(
    args: &[String],
) -> Result<WorkspaceGroupCreateOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut name = None;
    let mut anchor_workspace_id = None;
    let mut workspace_ids = Vec::new();
    let mut collapsed = None;
    let mut pinned = None;
    let mut color = None;
    let mut icon = None;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--anchor" | "--anchor-workspace" => {
                anchor_workspace_id =
                    Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            "--workspace" => {
                workspace_ids.push(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--collapsed" => {
                collapsed = Some(true);
                index += 1;
            }
            "--expanded" => {
                collapsed = Some(false);
                index += 1;
            }
            "--pinned" => {
                pinned = Some(true);
                index += 1;
            }
            "--unpinned" => {
                pinned = Some(false);
                index += 1;
            }
            "--color" => {
                color = Some(option_value(args, index, "--color")?.to_string());
                index += 2;
            }
            "--icon" => {
                icon = Some(option_value(args, index, "--icon")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown workspace group create option '{value}'."
                )));
            }
            value => {
                if name.is_some() {
                    return Err(CliError::InvalidArgs(
                        "workspace group create accepts exactly one name.".to_string(),
                    ));
                }
                name = Some(value.to_string());
                index += 1;
            }
        }
    }

    Ok(WorkspaceGroupCreateOptions {
        invoke,
        params: WorkspaceGroupCreateParams {
            name: name.ok_or_else(|| {
                CliError::InvalidArgs("workspace group create requires a group name.".to_string())
            })?,
            anchor_workspace_id,
            workspace_ids: (!workspace_ids.is_empty()).then_some(workspace_ids),
            collapsed,
            pinned,
            color,
            icon,
        },
    })
}

fn parse_workspace_group_update_options(
    args: &[String],
) -> Result<WorkspaceGroupUpdateOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut group_id = None;
    let mut name = None;
    let mut anchor_workspace_id = None;
    let mut collapsed = None;
    let mut pinned = None;
    let mut color = None;
    let mut icon = None;
    let mut sort_order = None;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--name" => {
                name = Some(option_value(args, index, "--name")?.to_string());
                index += 2;
            }
            "--anchor" | "--anchor-workspace" => {
                anchor_workspace_id =
                    Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            "--collapsed" => {
                collapsed = Some(true);
                index += 1;
            }
            "--expanded" => {
                collapsed = Some(false);
                index += 1;
            }
            "--pinned" => {
                pinned = Some(true);
                index += 1;
            }
            "--unpinned" => {
                pinned = Some(false);
                index += 1;
            }
            "--color" => {
                color = Some(option_value(args, index, "--color")?.to_string());
                index += 2;
            }
            "--icon" => {
                icon = Some(option_value(args, index, "--icon")?.to_string());
                index += 2;
            }
            "--sort-order" => {
                sort_order = Some(parse_i64_option(
                    option_value(args, index, "--sort-order")?,
                    "--sort-order",
                )?);
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown workspace group update option '{value}'."
                )));
            }
            value => {
                if group_id.is_none() {
                    group_id = Some(value.to_string());
                } else if name.is_none() {
                    name = Some(value.to_string());
                } else {
                    return Err(CliError::InvalidArgs(
                        "workspace group update accepts a group id and optional name.".to_string(),
                    ));
                }
                index += 1;
            }
        }
    }

    Ok(WorkspaceGroupUpdateOptions {
        invoke,
        params: WorkspaceGroupUpdateParams {
            group_id: group_id.ok_or_else(|| {
                CliError::InvalidArgs("workspace group update requires a group id.".to_string())
            })?,
            name,
            anchor_workspace_id,
            collapsed,
            pinned,
            color,
            icon,
            sort_order,
        },
    })
}

fn parse_workspace_group_delete_options(
    args: &[String],
) -> Result<WorkspaceGroupDeleteOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut group_id = None;
    let mut confirmed = false;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--yes" | "--confirm" => {
                confirmed = true;
                index += 1;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown workspace group delete option '{value}'."
                )));
            }
            value => {
                if group_id.is_some() {
                    return Err(CliError::InvalidArgs(
                        "workspace group delete accepts exactly one group id.".to_string(),
                    ));
                }
                group_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    require_confirmation(
        confirmed,
        "workspace group delete requires --yes because it removes a group.",
    )?;

    Ok(WorkspaceGroupDeleteOptions {
        invoke,
        params: WorkspaceGroupIdParams {
            group_id: group_id.ok_or_else(|| {
                CliError::InvalidArgs("workspace group delete requires a group id.".to_string())
            })?,
        },
    })
}

fn parse_workspace_group_member_options(
    args: &[String],
    command_name: &str,
    allow_position: bool,
) -> Result<WorkspaceGroupMemberOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut group_id = None;
    let mut workspace_id = None;
    let mut position = None;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--position" if allow_position => {
                position = Some(parse_i64_option(
                    option_value(args, index, "--position")?,
                    "--position",
                )?);
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown {command_name} option '{value}'."
                )));
            }
            value => {
                if group_id.is_none() {
                    group_id = Some(value.to_string());
                } else if workspace_id.is_none() {
                    workspace_id = Some(value.to_string());
                } else {
                    return Err(CliError::InvalidArgs(format!(
                        "{command_name} accepts exactly a group id and workspace id."
                    )));
                }
                index += 1;
            }
        }
    }

    Ok(WorkspaceGroupMemberOptions {
        invoke,
        params: WorkspaceGroupMemberParams {
            group_id: group_id.ok_or_else(|| {
                CliError::InvalidArgs(format!("{command_name} requires a group id."))
            })?,
            workspace_id: workspace_id.ok_or_else(|| {
                CliError::InvalidArgs(format!("{command_name} requires a workspace id."))
            })?,
            position,
        },
    })
}

fn parse_cmux_new_workspace_options(args: &[String]) -> Result<WorkspaceCreateOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut name = None;
    let mut project_root = None;
    let mut backend_profile = None;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--name" => {
                name = Some(option_value(args, index, "--name")?.to_string());
                index += 2;
            }
            "--project" => {
                project_root = Some(option_value(args, index, "--project")?.to_string());
                index += 2;
            }
            "--backend-profile" | "--distribution" => {
                backend_profile =
                    Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown new-workspace option '{value}'."
                )));
            }
            value => {
                if name.is_some() {
                    return Err(CliError::InvalidArgs(
                        "new-workspace accepts at most one name.".to_string(),
                    ));
                }
                name = Some(value.to_string());
                index += 1;
            }
        }
    }

    Ok(WorkspaceCreateOptions {
        invoke,
        params: WorkspaceCreateParams {
            name: name.unwrap_or_else(|| "Workspace".to_string()),
            project_root,
            backend_profile,
        },
    })
}

fn parse_cmux_workspace_query_options(
    args: &[String],
    command_name: &str,
) -> Result<CmuxWorkspaceQueryOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown {command_name} option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "{command_name} does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(CmuxWorkspaceQueryOptions {
        invoke,
        workspace_id,
    })
}

fn parse_cmux_workspace_close_options(args: &[String]) -> Result<WorkspaceCloseOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut close_policy = "fail_if_running".to_string();
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--policy" | "--close-policy" => {
                close_policy = option_value(args, index, args[index].as_str())?.to_string();
                index += 2;
            }
            "--yes" | "--confirm" => {
                index += 1;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown close-workspace option '{value}'."
                )));
            }
            value => {
                if workspace_id.is_some() {
                    return Err(CliError::InvalidArgs(
                        "close-workspace accepts exactly one workspace id.".to_string(),
                    ));
                }
                workspace_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    validate_workspace_close_policy(&close_policy)?;

    Ok(WorkspaceCloseOptions {
        invoke,
        params: WorkspaceCloseParams {
            workspace_id: workspace_id.ok_or_else(|| {
                CliError::InvalidArgs(
                    "close-workspace requires --workspace or a workspace id.".to_string(),
                )
            })?,
            close_policy,
        },
    })
}

fn parse_cmux_pane_split_options(args: &[String]) -> Result<CmuxPaneSplitOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut direction = None;
    let mut ratio = None;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--ratio" => {
                ratio = Some(parse_f64_option(
                    option_value(args, index, "--ratio")?,
                    "--ratio",
                )?);
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown new-split option '{value}'."
                )));
            }
            value => {
                if direction.is_some() {
                    return Err(CliError::InvalidArgs(
                        "new-split accepts exactly one direction.".to_string(),
                    ));
                }
                direction = Some(value.to_string());
                index += 1;
            }
        }
    }

    let direction = direction.ok_or_else(|| {
        CliError::InvalidArgs(
            "new-split requires a direction: left, right, up, or down.".to_string(),
        )
    })?;
    cmux_split_axis(&direction)?;

    Ok(CmuxPaneSplitOptions {
        invoke,
        workspace_id,
        direction,
        ratio,
    })
}

fn parse_cmux_active_send_text_options(
    args: &[String],
) -> Result<CmuxActiveSendTextOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut text_parts = Vec::new();
    let mut index = 0;

    while index < args.len() {
        if args[index] == "--" {
            text_parts.extend(args[index + 1..].iter().cloned());
            break;
        }
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown send option '{value}'."
                )));
            }
            value => {
                text_parts.push(value.to_string());
                index += 1;
            }
        }
    }

    if text_parts.is_empty() {
        return Err(CliError::InvalidArgs("send requires text.".to_string()));
    }

    Ok(CmuxActiveSendTextOptions {
        invoke,
        workspace_id,
        text: text_parts.join(" "),
    })
}

fn parse_cmux_active_send_key_options(
    args: &[String],
) -> Result<CmuxActiveSendKeyOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut key = None;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown send-key option '{value}'."
                )));
            }
            value => {
                if key.is_some() {
                    return Err(CliError::InvalidArgs(
                        "send-key accepts exactly one key.".to_string(),
                    ));
                }
                key = Some(value.to_string());
                index += 1;
            }
        }
    }

    Ok(CmuxActiveSendKeyOptions {
        invoke,
        workspace_id,
        key: key.ok_or_else(|| CliError::InvalidArgs("send-key requires a key.".to_string()))?,
    })
}

fn parse_agent_integration_setup_options(
    args: &[String],
    command_name: &str,
) -> Result<AgentIntegrationSetupOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut base_dir = None;
    let mut kind = None;
    let mut install_packages = false;
    let mut distribution = None;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--base-dir" => {
                base_dir = Some(option_value(args, index, "--base-dir")?.to_string());
                index += 2;
            }
            "--distribution" => {
                distribution = Some(option_value(args, index, "--distribution")?.to_string());
                index += 2;
            }
            "--install-packages" => {
                install_packages = true;
                index += 1;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown {command_name} option '{value}'."
                )));
            }
            value => {
                if kind.is_some() {
                    return Err(CliError::InvalidArgs(format!(
                        "{command_name} accepts exactly one integration name."
                    )));
                }
                kind = Some(AgentIntegrationKind::parse(value)?);
                index += 1;
            }
        }
    }

    Ok(AgentIntegrationSetupOptions {
        invoke,
        kind: kind.ok_or_else(|| {
            CliError::InvalidArgs(format!(
                "{command_name} requires an integration name: claude-teams, omo, omx, or omc."
            ))
        })?,
        workspace_id,
        base_dir,
        install_packages,
        distribution,
    })
}

fn parse_agent_integration_install_options(
    args: &[String],
) -> Result<AgentIntegrationInstallOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut base_dir = None;
    let mut bin_dir = None;
    let mut powershell_profile = None;
    let mut shell_profile = None;
    let mut user_path = false;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--base-dir" => {
                base_dir = Some(option_value(args, index, "--base-dir")?.to_string());
                index += 2;
            }
            "--bin-dir" => {
                bin_dir = Some(option_value(args, index, "--bin-dir")?.to_string());
                index += 2;
            }
            "--powershell-profile" => {
                powershell_profile =
                    Some(option_value(args, index, "--powershell-profile")?.to_string());
                index += 2;
            }
            "--shell-profile" => {
                shell_profile = Some(option_value(args, index, "--shell-profile")?.to_string());
                index += 2;
            }
            "--user-path" => {
                user_path = true;
                index += 1;
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown integrations install-shims option '{value}'."
                )));
            }
        }
    }

    Ok(AgentIntegrationInstallOptions {
        invoke,
        base_dir,
        bin_dir,
        powershell_profile,
        shell_profile,
        user_path,
    })
}

fn parse_agent_integration_doctor_options(
    args: &[String],
) -> Result<AgentIntegrationDoctorOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut base_dir = None;
    let mut bin_dir = None;
    let mut distribution = None;
    let mut kind = None;
    let mut saw_selector = false;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--base-dir" => {
                base_dir = Some(option_value(args, index, "--base-dir")?.to_string());
                index += 2;
            }
            "--bin-dir" => {
                bin_dir = Some(option_value(args, index, "--bin-dir")?.to_string());
                index += 2;
            }
            "--distribution" => {
                distribution = Some(option_value(args, index, "--distribution")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown integrations doctor option '{value}'."
                )));
            }
            "all" => {
                if saw_selector {
                    return Err(CliError::InvalidArgs(
                        "integrations doctor accepts at most one integration name.".to_string(),
                    ));
                }
                saw_selector = true;
                kind = None;
                index += 1;
            }
            value => {
                if saw_selector {
                    return Err(CliError::InvalidArgs(
                        "integrations doctor accepts at most one integration name.".to_string(),
                    ));
                }
                saw_selector = true;
                kind = Some(AgentIntegrationKind::parse(value)?);
                index += 1;
            }
        }
    }

    Ok(AgentIntegrationDoctorOptions {
        invoke,
        kind,
        base_dir,
        bin_dir,
        distribution,
    })
}

fn parse_agent_integration_launch_options(
    kind: AgentIntegrationKind,
    args: &[String],
) -> Result<AgentIntegrationLaunchOptions, CliError> {
    Ok(AgentIntegrationLaunchOptions {
        invoke: ControlInvokeOptions::from_env(),
        kind,
        workspace_id: workspace_from_env(),
        base_dir: None,
        args: args.to_vec(),
    })
}

fn parse_tmux_compat_options(args: &[String]) -> Result<TmuxCompatOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown __tmux-compat option '{value}'."
                )));
            }
            _ => break,
        }
    }

    let subcommand = args
        .get(index)
        .ok_or_else(|| CliError::InvalidArgs("__tmux-compat requires a tmux command.".to_string()))?
        .as_str();
    let rest = &args[index + 1..];
    let command = match subcommand {
        "display-message" | "display" => parse_tmux_display_message(rest)?,
        "list-panes" | "list-pane" => parse_tmux_list_panes(rest)?,
        "list-windows" | "list-window" | "listw" => parse_tmux_list_windows(rest)?,
        "list-sessions" | "list-session" | "ls" => parse_tmux_list_sessions(rest)?,
        "has-session" | "has" => parse_tmux_has_session(rest)?,
        "select-pane" | "selectp" => parse_tmux_select_pane(rest)?,
        "select-window" | "selectw" => parse_tmux_select_window(rest)?,
        "switch-client" | "switchc" => parse_tmux_switch_client(rest)?,
        "rename-window" | "renamew" => parse_tmux_rename_window(rest)?,
        "rename-session" | "rename" => parse_tmux_rename_session(rest)?,
        "capture-pane" | "capturep" => parse_tmux_capture_pane(rest)?,
        "kill-pane" | "killp" => parse_tmux_kill_pane(rest)?,
        "kill-window" | "killw" => parse_tmux_kill_window(rest)?,
        "send-keys" | "send" => parse_tmux_send_keys(rest)?,
        "split-window" | "splitw" => parse_tmux_split_window(rest)?,
        "new-window" | "neww" => parse_tmux_new_window(rest)?,
        "new-session" | "new" => parse_tmux_new_session(rest)?,
        other => {
            return Err(CliError::InvalidArgs(format!(
                "unsupported __tmux-compat command '{other}'."
            )))
        }
    };

    Ok(TmuxCompatOptions {
        invoke,
        workspace_id,
        command,
    })
}

fn parse_tmux_display_message(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let mut format = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-p" => {
                index += 1;
            }
            "-F" => {
                format = Some(option_value(args, index, "-F")?.to_string());
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown display-message option '{value}'."
                )));
            }
            value => {
                format = Some(value.to_string());
                index += 1;
            }
        }
    }
    Ok(TmuxCompatCommand::DisplayMessage { format })
}

fn parse_tmux_list_panes(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let mut all_workspaces = false;
    let mut format = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-a" => {
                all_workspaces = true;
                index += 1;
            }
            "-s" => {
                index += 1;
            }
            "-F" => {
                format = Some(option_value(args, index, "-F")?.to_string());
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown list-panes option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "list-panes does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(TmuxCompatCommand::ListPanes {
        all_workspaces,
        format,
    })
}

fn parse_tmux_list_windows(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let mut all_workspaces = false;
    let mut format = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-a" => {
                all_workspaces = true;
                index += 1;
            }
            "-F" => {
                format = Some(option_value(args, index, "-F")?.to_string());
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown list-windows option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "list-windows does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(TmuxCompatCommand::ListWindows {
        all_workspaces,
        format,
    })
}

fn parse_tmux_list_sessions(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let mut format = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-F" => {
                format = Some(option_value(args, index, "-F")?.to_string());
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown list-sessions option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "list-sessions does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(TmuxCompatCommand::ListSessions { format })
}

fn parse_tmux_has_session(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let mut target = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-t" => {
                target = Some(option_value(args, index, "-t")?.to_string());
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown has-session option '{value}'."
                )));
            }
            value => {
                if target.is_some() {
                    return Err(CliError::InvalidArgs(
                        "has-session accepts at most one target.".to_string(),
                    ));
                }
                target = Some(value.to_string());
                index += 1;
            }
        }
    }
    Ok(TmuxCompatCommand::HasSession { target })
}

fn parse_tmux_select_pane(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let target_pane_id = parse_optional_tmux_target(args, "select-pane")?;
    Ok(TmuxCompatCommand::SelectPane { target_pane_id })
}

fn parse_tmux_select_window(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let target_window = parse_optional_tmux_window_target(args, "select-window")?;
    Ok(TmuxCompatCommand::SelectWindow { target_window })
}

fn parse_tmux_switch_client(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let mut target = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-t" => {
                target = Some(option_value(args, index, "-t")?.to_string());
                index += 2;
            }
            "-c" | "-E" => {
                index += 1;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown switch-client option '{value}'."
                )));
            }
            value => {
                if target.is_some() {
                    return Err(CliError::InvalidArgs(
                        "switch-client accepts at most one target.".to_string(),
                    ));
                }
                target = Some(value.to_string());
                index += 1;
            }
        }
    }
    Ok(TmuxCompatCommand::SwitchClient { target })
}

fn parse_tmux_rename_window(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let mut target_window = None;
    let mut name = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-t" => {
                target_window = Some(option_value(args, index, "-t")?.to_string());
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown rename-window option '{value}'."
                )));
            }
            value => {
                if name.is_some() {
                    return Err(CliError::InvalidArgs(
                        "rename-window accepts exactly one new name.".to_string(),
                    ));
                }
                name = Some(value.to_string());
                index += 1;
            }
        }
    }
    Ok(TmuxCompatCommand::RenameWindow {
        target_window,
        name: name.ok_or_else(|| {
            CliError::InvalidArgs("rename-window requires a new name.".to_string())
        })?,
    })
}

fn parse_tmux_rename_session(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let mut target_session = None;
    let mut name = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-t" => {
                target_session = Some(option_value(args, index, "-t")?.to_string());
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown rename-session option '{value}'."
                )));
            }
            value => {
                if name.is_some() {
                    return Err(CliError::InvalidArgs(
                        "rename-session accepts exactly one new name.".to_string(),
                    ));
                }
                name = Some(value.to_string());
                index += 1;
            }
        }
    }
    Ok(TmuxCompatCommand::RenameSession {
        target_session,
        name: name.ok_or_else(|| {
            CliError::InvalidArgs("rename-session requires a new name.".to_string())
        })?,
    })
}

fn parse_tmux_capture_pane(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let mut target_pane_id = None;
    let mut max_bytes = 8192usize;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-p" | "-e" | "-J" => {
                index += 1;
            }
            "-S" => {
                let _ = option_value(args, index, "-S")?;
                index += 2;
            }
            "-t" => {
                target_pane_id = Some(normalize_tmux_pane_id(option_value(args, index, "-t")?));
                index += 2;
            }
            "--max-bytes" => {
                max_bytes =
                    parse_usize_option(option_value(args, index, "--max-bytes")?, "--max-bytes")?;
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown capture-pane option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "capture-pane does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(TmuxCompatCommand::CapturePane {
        target_pane_id,
        max_bytes,
    })
}

fn parse_tmux_kill_pane(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let target_pane_id = parse_optional_tmux_target(args, "kill-pane")?;
    Ok(TmuxCompatCommand::KillPane { target_pane_id })
}

fn parse_tmux_kill_window(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let target_window = parse_optional_tmux_window_target(args, "kill-window")?;
    Ok(TmuxCompatCommand::KillWindow { target_window })
}

fn parse_tmux_send_keys(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let mut target_pane_id = None;
    let mut keys = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-t" => {
                target_pane_id = Some(normalize_tmux_pane_id(option_value(args, index, "-t")?));
                index += 2;
            }
            "-l" => {
                index += 1;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown send-keys option '{value}'."
                )));
            }
            value => {
                keys.push(value.to_string());
                index += 1;
            }
        }
    }
    if keys.is_empty() {
        return Err(CliError::InvalidArgs(
            "send-keys requires at least one key or text argument.".to_string(),
        ));
    }
    Ok(TmuxCompatCommand::SendKeys {
        target_pane_id,
        keys,
    })
}

fn parse_tmux_split_window(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let mut target_pane_id = None;
    let mut axis = "vertical".to_string();
    let mut command = Vec::new();
    let mut format = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-t" => {
                target_pane_id = Some(normalize_tmux_pane_id(option_value(args, index, "-t")?));
                index += 2;
            }
            "-h" => {
                axis = "horizontal".to_string();
                index += 1;
            }
            "-v" => {
                axis = "vertical".to_string();
                index += 1;
            }
            "-P" => {
                index += 1;
            }
            "-F" => {
                format = Some(option_value(args, index, "-F")?.to_string());
                index += 2;
            }
            "--" => {
                command.extend(args[index + 1..].iter().cloned());
                break;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown split-window option '{value}'."
                )));
            }
            _ => {
                command.extend(args[index..].iter().cloned());
                break;
            }
        }
    }
    Ok(TmuxCompatCommand::SplitWindow {
        target_pane_id,
        axis,
        command,
        format,
    })
}

fn parse_tmux_new_window(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let mut command = Vec::new();
    let mut format = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-n" | "-c" => {
                let _ = option_value(args, index, args[index].as_str())?;
                index += 2;
            }
            "-P" => {
                index += 1;
            }
            "-F" => {
                format = Some(option_value(args, index, "-F")?.to_string());
                index += 2;
            }
            "--" => {
                command.extend(args[index + 1..].iter().cloned());
                break;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown new-window option '{value}'."
                )));
            }
            _ => {
                command.extend(args[index..].iter().cloned());
                break;
            }
        }
    }
    Ok(TmuxCompatCommand::NewWindow { command, format })
}

fn parse_tmux_new_session(args: &[String]) -> Result<TmuxCompatCommand, CliError> {
    let mut session_name = None;
    let mut cwd = None;
    let mut command = Vec::new();
    let mut format = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-d" | "-A" => {
                index += 1;
            }
            "-s" => {
                session_name = Some(option_value(args, index, "-s")?.to_string());
                index += 2;
            }
            "-c" => {
                cwd = Some(option_value(args, index, "-c")?.to_string());
                index += 2;
            }
            "-n" => {
                let _ = option_value(args, index, "-n")?;
                index += 2;
            }
            "-P" => {
                index += 1;
            }
            "-F" => {
                format = Some(option_value(args, index, "-F")?.to_string());
                index += 2;
            }
            "--" => {
                command.extend(args[index + 1..].iter().cloned());
                break;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown new-session option '{value}'."
                )));
            }
            _ => {
                command.extend(args[index..].iter().cloned());
                break;
            }
        }
    }
    Ok(TmuxCompatCommand::NewSession {
        session_name,
        cwd,
        command,
        format,
    })
}

fn parse_optional_tmux_target(
    args: &[String],
    command_name: &str,
) -> Result<Option<String>, CliError> {
    let mut target_pane_id = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-t" => {
                target_pane_id = Some(normalize_tmux_pane_id(option_value(args, index, "-t")?));
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown {command_name} option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "{command_name} does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(target_pane_id)
}

fn parse_optional_tmux_window_target(
    args: &[String],
    command_name: &str,
) -> Result<Option<String>, CliError> {
    let mut target_window = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-t" => {
                target_window = Some(normalize_tmux_window_target(option_value(
                    args, index, "-t",
                )?));
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown {command_name} option '{value}'."
                )));
            }
            value => {
                if target_window.is_some() {
                    return Err(CliError::InvalidArgs(format!(
                        "{command_name} accepts only one target window."
                    )));
                }
                target_window = Some(normalize_tmux_window_target(value));
                index += 1;
            }
        }
    }
    Ok(target_window)
}

fn parse_session_spawn_options(args: &[String]) -> Result<SessionSpawnOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = None;
    let mut backend = None;
    let mut backend_profile = None;
    let mut cwd = None;
    let mut columns = 120;
    let mut rows = 30;
    let mut durability = Some("ephemeral".to_string());
    let mut index = 0;

    while index < args.len() {
        if args[index] == "--" {
            index += 1;
            break;
        }

        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--backend" => {
                backend = Some(option_value(args, index, "--backend")?.to_string());
                index += 2;
            }
            "--backend-profile" | "--distribution" => {
                backend_profile =
                    Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            "--cwd" => {
                cwd = Some(option_value(args, index, "--cwd")?.to_string());
                index += 2;
            }
            "--columns" => {
                columns = parse_u16_option(option_value(args, index, "--columns")?, "--columns")?;
                index += 2;
            }
            "--rows" => {
                rows = parse_u16_option(option_value(args, index, "--rows")?, "--rows")?;
                index += 2;
            }
            "--durability" => {
                durability = Some(option_value(args, index, "--durability")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown session spawn option '{value}'."
                )));
            }
            _ => break,
        }
    }

    let command = args[index..].to_vec();
    if command.is_empty() {
        return Err(CliError::InvalidArgs(
            "session spawn requires a command after '--'.".to_string(),
        ));
    }

    Ok(SessionSpawnOptions {
        invoke,
        params: SessionSpawnParams {
            workspace_id: workspace_id.ok_or_else(|| {
                CliError::InvalidArgs("session spawn requires --workspace <id>.".to_string())
            })?,
            backend,
            backend_profile,
            command,
            cwd,
            env: Vec::new(),
            columns,
            rows,
            durability,
            placement: None,
            pane_id: None,
        },
    })
}

fn parse_session_list_options(args: &[String]) -> Result<SessionListOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = None;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown session list option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "session list does not accept argument '{value}'."
                )));
            }
        }
    }

    Ok(SessionListOptions {
        invoke,
        params: SessionListParams { workspace_id },
    })
}

fn parse_session_get_options(args: &[String]) -> Result<SessionGetOptions, CliError> {
    let (invoke, session_id) = parse_one_id_command(args, "session get", "session id")?;
    Ok(SessionGetOptions {
        invoke,
        params: SessionIdParams { session_id },
    })
}

fn parse_session_send_text_options(args: &[String]) -> Result<SessionSendTextOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut session_id = None;
    let mut text_parts = Vec::new();
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        if args[index] == "--" {
            text_parts.extend(args[index + 1..].iter().cloned());
            break;
        }

        if session_id.is_none() {
            session_id = Some(args[index].clone());
        } else {
            text_parts.push(args[index].clone());
        }
        index += 1;
    }

    if text_parts.is_empty() {
        return Err(CliError::InvalidArgs(
            "session send-text requires text.".to_string(),
        ));
    }

    Ok(SessionSendTextOptions {
        invoke,
        params: SessionSendTextParams {
            session_id: session_id.ok_or_else(|| {
                CliError::InvalidArgs("session send-text requires a session id.".to_string())
            })?,
            text: text_parts.join(" "),
        },
    })
}

fn parse_session_send_key_options(args: &[String]) -> Result<SessionSendKeyOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut session_id = None;
    let mut key = None;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        let value = args[index].as_str();
        if value.starts_with("--") {
            return Err(CliError::InvalidArgs(format!(
                "unknown session send-key option '{value}'."
            )));
        }

        if session_id.is_none() {
            session_id = Some(value.to_string());
        } else if key.is_none() {
            key = Some(value.to_string());
        } else {
            return Err(CliError::InvalidArgs(
                "session send-key accepts exactly a session id and key.".to_string(),
            ));
        }
        index += 1;
    }

    Ok(SessionSendKeyOptions {
        invoke,
        params: SessionSendKeyParams {
            session_id: session_id.ok_or_else(|| {
                CliError::InvalidArgs("session send-key requires a session id.".to_string())
            })?,
            key: key.ok_or_else(|| {
                CliError::InvalidArgs("session send-key requires a key.".to_string())
            })?,
        },
    })
}

fn parse_session_read_recent_options(
    args: &[String],
) -> Result<SessionReadRecentOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut session_id = None;
    let mut max_bytes = 8192;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--max-bytes" => {
                max_bytes = option_value(args, index, "--max-bytes")?
                    .parse::<usize>()
                    .map_err(|_| {
                        CliError::InvalidArgs(
                            "--max-bytes requires a positive integer.".to_string(),
                        )
                    })?;
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown session read-recent option '{value}'."
                )));
            }
            value => {
                if session_id.is_some() {
                    return Err(CliError::InvalidArgs(
                        "session read-recent accepts exactly one session id.".to_string(),
                    ));
                }
                session_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    Ok(SessionReadRecentOptions {
        invoke,
        params: SessionReadRecentParams {
            session_id: session_id.ok_or_else(|| {
                CliError::InvalidArgs("session read-recent requires a session id.".to_string())
            })?,
            max_bytes,
        },
    })
}

fn parse_session_terminate_options(args: &[String]) -> Result<SessionTerminateOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut session_id = None;
    let mut mode = "soft".to_string();
    let mut confirmed = false;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--mode" => {
                mode = option_value(args, index, "--mode")?.to_string();
                index += 2;
            }
            "--yes" | "--confirm" => {
                confirmed = true;
                index += 1;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown session terminate option '{value}'."
                )));
            }
            value => {
                if session_id.is_some() {
                    return Err(CliError::InvalidArgs(
                        "session terminate accepts exactly one session id.".to_string(),
                    ));
                }
                session_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    require_confirmation(
        confirmed,
        "session terminate requires --yes because it stops a running session.",
    )?;
    validate_termination_mode(&mode)?;

    Ok(SessionTerminateOptions {
        invoke,
        params: SessionTerminateParams {
            session_id: session_id.ok_or_else(|| {
                CliError::InvalidArgs("session terminate requires a session id.".to_string())
            })?,
            mode,
        },
    })
}

fn parse_server_options(args: &[String]) -> Result<ServerOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut mode = ServerMode::Local;
    let mut host = "127.0.0.1".to_string();
    let mut port = 8765;
    let mut allow_remote = false;
    let mut workspace_id = workspace_from_env();
    let mut backend = Some("conpty".to_string());
    let mut backend_profile = None;
    let mut cwd = None;
    let mut columns = 120;
    let mut rows = 36;
    let mut max_recent_bytes = 1024 * 1024;
    let mut index = 0;

    if args.first().map(String::as_str) == Some("start") {
        index = 1;
    }

    while index < args.len() {
        if args[index] == "--" {
            index += 1;
            break;
        }

        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--host" => {
                host = option_value(args, index, "--host")?.to_string();
                index += 2;
            }
            "--port" => {
                port = parse_u16_option(option_value(args, index, "--port")?, "--port")?;
                index += 2;
            }
            "--allow-remote" => {
                allow_remote = true;
                index += 1;
            }
            "--mode" => {
                mode = parse_server_mode(option_value(args, index, "--mode")?)?;
                index += 2;
            }
            "--desktop-control" => {
                mode = ServerMode::DesktopBridge;
                index += 1;
            }
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--backend" => {
                let value = option_value(args, index, "--backend")?;
                backend = Some(backend_label(&parse_backend_option(value)?).to_string());
                index += 2;
            }
            "--backend-profile" | "--distribution" => {
                backend_profile =
                    Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            "--cwd" => {
                cwd = Some(option_value(args, index, "--cwd")?.to_string());
                index += 2;
            }
            "--columns" => {
                columns = parse_u16_option(option_value(args, index, "--columns")?, "--columns")?;
                index += 2;
            }
            "--rows" => {
                rows = parse_u16_option(option_value(args, index, "--rows")?, "--rows")?;
                index += 2;
            }
            "--max-recent-bytes" => {
                max_recent_bytes = parse_usize_option(
                    option_value(args, index, "--max-recent-bytes")?,
                    "--max-recent-bytes",
                )?;
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown server option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "server does not accept argument '{value}' before '--'."
                )));
            }
        }
    }

    if backend_profile.is_some() && backend.as_deref() != Some("wsl-direct") {
        return Err(CliError::InvalidArgs(
            "--distribution requires --backend wsl-direct.".to_string(),
        ));
    }

    if !allow_remote && !is_loopback_host(&host) {
        return Err(CliError::InvalidArgs(
            "server binds to loopback by default; pass --allow-remote to use a non-loopback host."
                .to_string(),
        ));
    }

    let command = if index < args.len() {
        args[index..].to_vec()
    } else {
        default_server_command(backend.as_deref())
    };

    Ok(ServerOptions {
        invoke,
        mode,
        host,
        port,
        allow_remote,
        workspace_id,
        backend,
        backend_profile,
        cwd,
        command,
        columns,
        rows,
        max_recent_bytes,
    })
}

fn parse_server_mode(value: &str) -> Result<ServerMode, CliError> {
    match value {
        "local" => Ok(ServerMode::Local),
        "desktop" | "desktop-bridge" => Ok(ServerMode::DesktopBridge),
        other => Err(CliError::InvalidArgs(format!(
            "unsupported server mode '{other}'."
        ))),
    }
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn default_server_command(backend: Option<&str>) -> Vec<String> {
    match backend {
        Some("conpty") => vec!["powershell.exe".to_string(), "-NoLogo".to_string()],
        _ => vec!["bash".to_string(), "-l".to_string()],
    }
}

fn parse_agent_set_state_options(args: &[String]) -> Result<AgentSetStateOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut session_id = None;
    let mut state = None;
    let mut reason = None;
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--reason" => {
                reason = Some(option_value(args, index, "--reason")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown agent set-state option '{value}'."
                )));
            }
            value => {
                if session_id.is_none() {
                    session_id = Some(value.to_string());
                } else if state.is_none() {
                    state = Some(value.to_string());
                } else {
                    return Err(CliError::InvalidArgs(
                        "agent set-state accepts exactly a session id and state.".to_string(),
                    ));
                }
                index += 1;
            }
        }
    }

    Ok(AgentSetStateOptions {
        invoke,
        params: AgentSetStateParams {
            session_id: session_id.ok_or_else(|| {
                CliError::InvalidArgs("agent set-state requires a session id.".to_string())
            })?,
            state: state.ok_or_else(|| {
                CliError::InvalidArgs("agent set-state requires a state.".to_string())
            })?,
            reason,
            telemetry: None,
        },
    })
}

fn parse_agent_get_state_options(args: &[String]) -> Result<AgentGetStateOptions, CliError> {
    let (invoke, session_id) = parse_one_id_command(args, "agent get-state", "session id")?;
    Ok(AgentGetStateOptions {
        invoke,
        params: SessionIdParams { session_id },
    })
}

fn parse_agent_clear_attention_options(
    args: &[String],
) -> Result<AgentClearAttentionOptions, CliError> {
    let (invoke, session_id) = parse_one_id_command(args, "agent clear-attention", "session id")?;
    Ok(AgentClearAttentionOptions {
        invoke,
        params: SessionIdParams { session_id },
    })
}

fn parse_agent_list_attention_options(
    args: &[String],
) -> Result<AgentListAttentionOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown agent list-attention option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "agent list-attention does not accept argument '{value}'."
                )));
            }
        }
    }

    Ok(AgentListAttentionOptions {
        invoke,
        params: AgentListAttentionParams { workspace_id },
    })
}

fn parse_notification_list_options(args: &[String]) -> Result<NotificationListOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = None;
    let mut severity = None;
    let mut include_dismissed = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--severity" => {
                severity = Some(option_value(args, index, "--severity")?.to_string());
                index += 2;
            }
            "--include-dismissed" => {
                include_dismissed = Some(true);
                index += 1;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown notification list option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "notification list does not accept argument '{value}'."
                )));
            }
        }
    }

    Ok(NotificationListOptions {
        invoke,
        params: NotificationListParams {
            workspace_id,
            severity,
            include_dismissed,
        },
    })
}

fn parse_notification_dismiss_options(
    args: &[String],
) -> Result<NotificationDismissOptions, CliError> {
    let (invoke, notification_id) =
        parse_one_id_command(args, "notification dismiss", "notification id")?;
    Ok(NotificationDismissOptions {
        invoke,
        params: NotificationDismissParams { notification_id },
    })
}

fn parse_notify_options(args: &[String]) -> Result<NotificationCreateOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut title = None;
    let mut body = None;
    let mut subtitle = None;
    let mut severity = None;
    let mut workspace_id = workspace_from_env();
    let mut session_id = std::env::var("AGENTMUX_SESSION_ID").ok();
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--title" => {
                title = Some(option_value(args, index, "--title")?.to_string());
                index += 2;
            }
            "--body" => {
                body = Some(option_value(args, index, "--body")?.to_string());
                index += 2;
            }
            "--subtitle" => {
                subtitle = Some(option_value(args, index, "--subtitle")?.to_string());
                index += 2;
            }
            "--severity" | "--level" => {
                severity = Some(option_value(args, index, args[index].as_str())?.to_string());
                index += 2;
            }
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--session" => {
                session_id = Some(option_value(args, index, "--session")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown notify option '{value}'."
                )));
            }
            value => {
                body = Some(value.to_string());
                index += 1;
            }
        }
    }

    let title = title.unwrap_or_else(|| "AgentMux".to_string());
    Ok(NotificationCreateOptions {
        invoke,
        params: NotificationCreateParams {
            title,
            body,
            subtitle,
            severity,
            workspace_id,
            session_id,
        },
    })
}

fn parse_notification_clear_options(args: &[String]) -> Result<NotificationClearOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut severity = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--severity" => {
                severity = Some(option_value(args, index, "--severity")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown clear-notifications option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "clear-notifications does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(NotificationClearOptions {
        invoke,
        params: NotificationClearParams {
            workspace_id,
            severity,
        },
    })
}

fn parse_sidebar_status_set_options(args: &[String]) -> Result<SidebarStatusSetOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut key = None;
    let mut label = None;
    let mut icon = None;
    let mut color = None;
    let mut priority = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--icon" => {
                icon = Some(option_value(args, index, "--icon")?.to_string());
                index += 2;
            }
            "--color" => {
                color = Some(option_value(args, index, "--color")?.to_string());
                index += 2;
            }
            "--priority" => {
                priority = Some(parse_i64_option(
                    option_value(args, index, "--priority")?,
                    "--priority",
                )?);
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown set-status option '{value}'."
                )));
            }
            value => {
                if key.is_none() {
                    key = Some(value.to_string());
                } else if label.is_none() {
                    label = Some(value.to_string());
                } else {
                    return Err(CliError::InvalidArgs(
                        "set-status accepts exactly a key and label.".to_string(),
                    ));
                }
                index += 1;
            }
        }
    }
    Ok(SidebarStatusSetOptions {
        invoke,
        params: SidebarStatusSetParams {
            workspace_id,
            key: key
                .ok_or_else(|| CliError::InvalidArgs("set-status requires a key.".to_string()))?,
            label: label
                .ok_or_else(|| CliError::InvalidArgs("set-status requires a label.".to_string()))?,
            icon,
            color,
            priority,
        },
    })
}

fn parse_sidebar_status_key_options(
    args: &[String],
    command_name: &str,
) -> Result<SidebarStatusKeyOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut key = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown {command_name} option '{value}'."
                )));
            }
            value => {
                if key.is_some() {
                    return Err(CliError::InvalidArgs(format!(
                        "{command_name} accepts exactly one key."
                    )));
                }
                key = Some(value.to_string());
                index += 1;
            }
        }
    }
    Ok(SidebarStatusKeyOptions {
        invoke,
        params: SidebarStatusKeyParams {
            workspace_id,
            key: key
                .ok_or_else(|| CliError::InvalidArgs(format!("{command_name} requires a key.")))?,
        },
    })
}

fn parse_sidebar_progress_set_options(
    args: &[String],
) -> Result<SidebarProgressSetOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut value = None;
    let mut label = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--label" => {
                label = Some(option_value(args, index, "--label")?.to_string());
                index += 2;
            }
            value_arg if value_arg.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown set-progress option '{value_arg}'."
                )));
            }
            value_arg => {
                if value.is_some() {
                    return Err(CliError::InvalidArgs(
                        "set-progress accepts exactly one value.".to_string(),
                    ));
                }
                value = Some(parse_f64_option(value_arg, "progress")?);
                index += 1;
            }
        }
    }
    Ok(SidebarProgressSetOptions {
        invoke,
        params: SidebarProgressSetParams {
            workspace_id,
            value: value.ok_or_else(|| {
                CliError::InvalidArgs("set-progress requires a value.".to_string())
            })?,
            label,
        },
    })
}

fn parse_sidebar_workspace_options(
    args: &[String],
    command_name: &str,
) -> Result<SidebarWorkspaceOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "{command_name} does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(SidebarWorkspaceOptions {
        invoke,
        params: SidebarWorkspaceParams { workspace_id },
    })
}

fn parse_sidebar_log_options(args: &[String]) -> Result<SidebarLogOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut level = None;
    let mut source = None;
    let mut message_parts = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--" {
            message_parts.extend(args[index + 1..].iter().cloned());
            break;
        }
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--level" => {
                level = Some(option_value(args, index, "--level")?.to_string());
                index += 2;
            }
            "--source" => {
                source = Some(option_value(args, index, "--source")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown log option '{value}'."
                )));
            }
            value => {
                message_parts.push(value.to_string());
                index += 1;
            }
        }
    }
    Ok(SidebarLogOptions {
        invoke,
        params: SidebarLogAddParams {
            workspace_id,
            level,
            source,
            message: message_parts.join(" "),
        },
    })
}

fn parse_sidebar_log_list_options(args: &[String]) -> Result<SidebarLogListOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut limit = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--limit" => {
                limit = Some(parse_usize_option(
                    option_value(args, index, "--limit")?,
                    "--limit",
                )?);
                index += 2;
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "list-log does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(SidebarLogListOptions {
        invoke,
        params: SidebarLogListParams {
            workspace_id,
            limit,
        },
    })
}

fn parse_identify_options(args: &[String]) -> Result<IdentifyOptions, CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = workspace_from_env();
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }
        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "identify does not accept argument '{value}'."
                )));
            }
        }
    }
    Ok(IdentifyOptions {
        invoke,
        params: SystemIdentifyParams { workspace_id },
    })
}

fn parse_events_poll_options(args: &[String]) -> Result<EventPollOptions, CliError> {
    parse_event_poll_options(args, false).map(|(poll, _)| poll)
}

fn parse_events_watch_options(args: &[String]) -> Result<EventWatchOptions, CliError> {
    let (poll, watch) = parse_event_poll_options(args, true)?;
    Ok(EventWatchOptions {
        invoke: poll.invoke,
        params: EventSubscribeParams {
            workspace_id: poll.params.workspace_id,
            session_id: poll.params.session_id,
            types: poll.params.types,
            after_event_id: watch.after_event_id,
        },
        interval_ms: watch.interval_ms,
        once: watch.once,
        limit: watch.limit,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EventWatchParseOptions {
    interval_ms: u64,
    once: bool,
    limit: Option<usize>,
    after_event_id: Option<String>,
}

fn parse_event_poll_options(
    args: &[String],
    allow_watch_options: bool,
) -> Result<(EventPollOptions, EventWatchParseOptions), CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut workspace_id = None;
    let mut session_id = None;
    let mut event_types = Vec::new();
    let mut max_events = None;
    let mut watch = EventWatchParseOptions {
        interval_ms: 1000,
        once: false,
        limit: None,
        after_event_id: None,
    };
    let mut index = 0;

    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        match args[index].as_str() {
            "--workspace" => {
                workspace_id = Some(option_value(args, index, "--workspace")?.to_string());
                index += 2;
            }
            "--session" => {
                session_id = Some(option_value(args, index, "--session")?.to_string());
                index += 2;
            }
            "--type" => {
                event_types.push(option_value(args, index, "--type")?.to_string());
                index += 2;
            }
            "--max-events" => {
                max_events = Some(parse_usize_option(
                    option_value(args, index, "--max-events")?,
                    "--max-events",
                )?);
                index += 2;
            }
            "--interval-ms" if allow_watch_options => {
                watch.interval_ms =
                    parse_u64_option(option_value(args, index, "--interval-ms")?, "--interval-ms")?;
                index += 2;
            }
            "--once" if allow_watch_options => {
                watch.once = true;
                index += 1;
            }
            "--limit" if allow_watch_options => {
                watch.limit = Some(parse_usize_option(
                    option_value(args, index, "--limit")?,
                    "--limit",
                )?);
                index += 2;
            }
            "--after-event" if allow_watch_options => {
                watch.after_event_id =
                    Some(option_value(args, index, "--after-event")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown events option '{value}'."
                )));
            }
            value => {
                return Err(CliError::InvalidArgs(format!(
                    "events command does not accept argument '{value}'."
                )));
            }
        }
    }

    Ok((
        EventPollOptions {
            invoke,
            params: EventPollParams {
                workspace_id,
                session_id,
                types: (!event_types.is_empty()).then_some(event_types),
                max_events,
            },
        },
        watch,
    ))
}

fn parse_one_id_command(
    args: &[String],
    command_name: &str,
    id_name: &str,
) -> Result<(ControlInvokeOptions, String), CliError> {
    let mut invoke = ControlInvokeOptions::from_env();
    let mut id = None;
    let mut index = 0;
    while index < args.len() {
        if parse_common_control_option(args, &mut index, &mut invoke)? {
            continue;
        }

        let value = args[index].as_str();
        if value.starts_with("--") {
            return Err(CliError::InvalidArgs(format!(
                "unknown {command_name} option '{value}'."
            )));
        }
        if id.is_some() {
            return Err(CliError::InvalidArgs(format!(
                "{command_name} accepts exactly one {id_name}."
            )));
        }
        id = Some(value.to_string());
        index += 1;
    }

    let id =
        id.ok_or_else(|| CliError::InvalidArgs(format!("{command_name} requires a {id_name}.")))?;
    Ok((invoke, id))
}

fn parse_common_control_option(
    args: &[String],
    index: &mut usize,
    invoke: &mut ControlInvokeOptions,
) -> Result<bool, CliError> {
    match args[*index].as_str() {
        "--json" => {
            invoke.json = true;
            *index += 1;
            Ok(true)
        }
        "--pipe" | "--socket" => {
            invoke.pipe_name = option_value(args, *index, args[*index].as_str())?.to_string();
            *index += 2;
            Ok(true)
        }
        "--token-path" => {
            invoke.token_path = Some(option_value(args, *index, "--token-path")?.to_string());
            *index += 2;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn parse_u16_option(value: &str, option: &str) -> Result<u16, CliError> {
    value
        .parse::<u16>()
        .map_err(|_| CliError::InvalidArgs(format!("{option} requires a positive integer.")))
}

fn parse_i32_option(value: &str, option: &str) -> Result<i32, CliError> {
    value
        .parse::<i32>()
        .map_err(|_| CliError::InvalidArgs(format!("{option} requires an integer.")))
}

fn parse_u64_option(value: &str, option: &str) -> Result<u64, CliError> {
    value
        .parse::<u64>()
        .map_err(|_| CliError::InvalidArgs(format!("{option} requires a positive integer.")))
}

fn parse_bool_option(value: &str, option: &str) -> Result<bool, CliError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" | "checked" => Ok(true),
        "false" | "0" | "no" | "off" | "unchecked" => Ok(false),
        _ => Err(CliError::InvalidArgs(format!(
            "{option} requires true or false."
        ))),
    }
}

fn parse_usize_option(value: &str, option: &str) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .map_err(|_| CliError::InvalidArgs(format!("{option} requires a positive integer.")))
}

fn parse_i64_option(value: &str, option: &str) -> Result<i64, CliError> {
    value
        .parse::<i64>()
        .map_err(|_| CliError::InvalidArgs(format!("{option} requires an integer.")))
}

fn parse_f64_option(value: &str, option: &str) -> Result<f64, CliError> {
    value
        .parse::<f64>()
        .map_err(|_| CliError::InvalidArgs(format!("{option} requires a number.")))
}

fn workspace_from_env() -> Option<String> {
    std::env::var("AGENTMUX_WORKSPACE_ID")
        .ok()
        .or_else(|| std::env::var("CMUX_WORKSPACE_ID").ok())
        .filter(|value| !value.trim().is_empty())
}

fn pane_from_env() -> Option<String> {
    std::env::var("AGENTMUX_PANE_ID")
        .ok()
        .or_else(|| std::env::var("CMUX_PANE_ID").ok())
        .or_else(|| {
            std::env::var("TMUX_PANE")
                .ok()
                .map(|value| normalize_tmux_pane_id(&value))
        })
        .filter(|value| !value.trim().is_empty())
}

fn normalize_tmux_pane_id(value: &str) -> String {
    value.trim().trim_start_matches('%').to_string()
}

fn normalize_tmux_window_target(value: &str) -> String {
    let trimmed = value.trim();
    let target = trimmed.rsplit(':').next().unwrap_or(trimmed);
    target.trim_start_matches('@').to_string()
}

fn normalize_tmux_session_target(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('$')
        .trim_start_matches('=')
        .to_string()
}

fn split_tmux_session_window_target(value: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return (None, None);
    };
    if let Some((session, window)) = value.rsplit_once(':') {
        let session = normalize_tmux_session_target(session);
        let window = normalize_tmux_window_target(window);
        return (
            (!session.is_empty()).then_some(session),
            (!window.is_empty()).then_some(window),
        );
    }
    let session = normalize_tmux_session_target(value);
    ((!session.is_empty()).then_some(session), None)
}

#[derive(Debug, Default, Eq, PartialEq)]
struct TmuxPaneTargetParts {
    session: Option<String>,
    window: Option<String>,
    pane: Option<String>,
}

fn split_tmux_session_window_pane_target(value: &str) -> TmuxPaneTargetParts {
    let value = value.trim();
    if value.is_empty() {
        return TmuxPaneTargetParts::default();
    }

    let (session, body, had_session_separator) =
        if let Some((session, body)) = value.rsplit_once(':') {
            (
                {
                    let session = normalize_tmux_session_target(session);
                    (!session.is_empty()).then_some(session)
                },
                body.trim(),
                true,
            )
        } else {
            (None, value, false)
        };

    if body.is_empty() {
        return TmuxPaneTargetParts {
            session,
            window: None,
            pane: None,
        };
    }

    if let Some((window, pane)) = body.rsplit_once('.') {
        let window = normalize_tmux_window_target(window);
        let pane = normalize_tmux_pane_id(pane);
        return TmuxPaneTargetParts {
            session,
            window: (!window.is_empty()).then_some(window),
            pane: (!pane.is_empty()).then_some(pane),
        };
    }

    if had_session_separator && !body.starts_with('%') {
        let window = normalize_tmux_window_target(body);
        return TmuxPaneTargetParts {
            session,
            window: (!window.is_empty()).then_some(window),
            pane: None,
        };
    }

    let pane = normalize_tmux_pane_id(body);
    TmuxPaneTargetParts {
        session,
        window: None,
        pane: (!pane.is_empty()).then_some(pane),
    }
}

fn agentmux_pane_to_tmux_pane(pane_id: &str) -> String {
    format!("%{pane_id}")
}

fn require_confirmation(confirmed: bool, message: &str) -> Result<(), CliError> {
    if confirmed {
        Ok(())
    } else {
        Err(CliError::InvalidArgs(message.to_string()))
    }
}

fn validate_workspace_close_policy(policy: &str) -> Result<(), CliError> {
    if matches!(
        policy,
        "detach_sessions" | "terminate_sessions" | "fail_if_running"
    ) {
        Ok(())
    } else {
        Err(CliError::InvalidArgs(format!(
            "--policy must be detach_sessions, terminate_sessions, or fail_if_running; got '{policy}'."
        )))
    }
}

fn validate_termination_mode(mode: &str) -> Result<(), CliError> {
    if matches!(mode, "soft" | "interrupt" | "kill") {
        Ok(())
    } else {
        Err(CliError::InvalidArgs(format!(
            "--mode must be soft, interrupt, or kill; got '{mode}'."
        )))
    }
}

fn run_workspace_create<W>(options: WorkspaceCreateOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("workspace.create", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let workspace: WorkspaceSummaryResult = response_result(&response)?;
    writeln!(output, "{}\t{}", workspace.workspace_id, workspace.name)?;
    Ok(())
}

fn run_workspace_list<W>(options: ControlInvokeOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("workspace.list", &serde_json::json!({}), &options)?;
    if options.json {
        return write_json_response(&response, output);
    }
    let result: WorkspaceListResult = response_result(&response)?;
    if result.workspaces.is_empty() {
        writeln!(output, "No workspaces.")?;
        return Ok(());
    }
    for workspace in result.workspaces {
        writeln!(
            output,
            "{}\t{}\t{}",
            workspace.workspace_id,
            workspace.name,
            workspace.project_root.unwrap_or_else(|| "-".to_string())
        )?;
    }
    Ok(())
}

fn run_workspace_get<W>(options: WorkspaceGetOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("workspace.get", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let detail: WorkspaceDetailResult = response_result(&response)?;
    write_workspace_detail(&detail, output)
}

fn run_workspace_rename<W>(options: WorkspaceRenameOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("workspace.rename", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let workspace: WorkspaceSummaryResult = response_result(&response)?;
    writeln!(output, "{}\t{}", workspace.workspace_id, workspace.name)?;
    Ok(())
}

fn run_workspace_close<W>(options: WorkspaceCloseOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("workspace.close", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: WorkspaceCloseResult = response_result(&response)?;
    writeln!(output, "closed\t{}\t{}", result.workspace_id, result.closed)?;
    Ok(())
}

fn run_workspace_group_command<W>(
    command: &str,
    args: &[String],
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    match command {
        "list" => {
            let options = parse_no_params_control_options(args, "workspace group list")?;
            run_workspace_group_list(options, output)
        }
        "create" => {
            let options = parse_workspace_group_create_options(args)?;
            run_workspace_group_create(options, output)
        }
        "update" | "rename" => {
            let options = parse_workspace_group_update_options(args)?;
            run_workspace_group_update(options, output)
        }
        "delete" | "remove-group" => {
            let options = parse_workspace_group_delete_options(args)?;
            run_workspace_group_delete(options, output)
        }
        "add" | "add-workspace" => {
            let options = parse_workspace_group_member_options(args, "workspace group add", true)?;
            run_workspace_group_add_workspace(options, output)
        }
        "remove" | "remove-workspace" => {
            let options =
                parse_workspace_group_member_options(args, "workspace group remove", false)?;
            run_workspace_group_remove_workspace(options, output)
        }
        other => Err(CliError::InvalidArgs(format!(
            "unknown workspace group command '{other}'."
        ))),
    }
}

fn run_workspace_group_list<W>(
    options: ControlInvokeOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control(
        "workspace_group.list",
        &WorkspaceGroupListParams {},
        &options,
    )?;
    if options.json {
        return write_json_response(&response, output);
    }
    let result: WorkspaceGroupListResult = response_result(&response)?;
    if result.groups.is_empty() {
        writeln!(output, "No workspace groups.")?;
        return Ok(());
    }
    for group in result.groups {
        write_workspace_group_summary(&group, output)?;
    }
    Ok(())
}

fn run_workspace_group_create<W>(
    options: WorkspaceGroupCreateOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("workspace_group.create", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let group: WorkspaceGroupResult = response_result(&response)?;
    write_workspace_group_summary(&group, output)
}

fn run_workspace_group_update<W>(
    options: WorkspaceGroupUpdateOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("workspace_group.update", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let group: WorkspaceGroupResult = response_result(&response)?;
    write_workspace_group_summary(&group, output)
}

fn run_workspace_group_delete<W>(
    options: WorkspaceGroupDeleteOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("workspace_group.delete", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: AckResult = response_result(&response)?;
    writeln!(
        output,
        "deleted\t{}\t{}",
        options.params.group_id, result.ok
    )?;
    Ok(())
}

fn run_workspace_group_add_workspace<W>(
    options: WorkspaceGroupMemberOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control(
        "workspace_group.add_workspace",
        &options.params,
        &options.invoke,
    )?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let group: WorkspaceGroupResult = response_result(&response)?;
    write_workspace_group_summary(&group, output)
}

fn run_workspace_group_remove_workspace<W>(
    options: WorkspaceGroupMemberOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control(
        "workspace_group.remove_workspace",
        &options.params,
        &options.invoke,
    )?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let group: WorkspaceGroupResult = response_result(&response)?;
    write_workspace_group_summary(&group, output)
}

fn write_workspace_group_summary<W>(
    group: &WorkspaceGroupResult,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let members = group
        .members
        .iter()
        .map(|member| member.workspace_id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    writeln!(
        output,
        "{}\t{}\t{}\t{}\t{}",
        group.group_id,
        group.name,
        if group.collapsed {
            "collapsed"
        } else {
            "expanded"
        },
        group.anchor_workspace_id.as_deref().unwrap_or("-"),
        if members.is_empty() {
            "-".to_string()
        } else {
            members
        }
    )?;
    Ok(())
}

fn run_session_spawn<W>(options: SessionSpawnOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("session.spawn", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: SessionSpawnResult = response_result(&response)?;
    writeln!(output, "{}", result.session_id)?;
    Ok(())
}

fn run_session_list<W>(options: SessionListOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("session.list", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: SessionListResult = response_result(&response)?;
    if result.sessions.is_empty() {
        writeln!(output, "No sessions.")?;
        return Ok(());
    }
    for session in result.sessions {
        write_session_summary(&session, output)?;
    }
    Ok(())
}

fn run_session_get<W>(options: SessionGetOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("session.get", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let session: SessionSummaryResult = response_result(&response)?;
    write_session_summary(&session, output)
}

fn run_session_send_text<W>(options: SessionSendTextOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("session.send_text", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<serde_json::Value>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

fn run_session_send_key<W>(options: SessionSendKeyOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("session.send_key", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<serde_json::Value>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

fn run_session_read_recent<W>(
    options: SessionReadRecentOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("session.read_recent", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: SessionReadRecentResult = response_result(&response)?;
    write!(output, "{}", result.text)?;
    Ok(())
}

fn run_session_terminate<W>(
    options: SessionTerminateOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("session.terminate", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<serde_json::Value>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

const SERVER_LOCAL_TOKEN: &str = "server-local-token";
const SERVER_LOCAL_WORKSPACE_ID: &str = "ws_server";

struct ServerState {
    options: ServerOptions,
    local_control: Option<LocalServerControl>,
}

impl ServerState {
    fn new(options: ServerOptions) -> Self {
        let local_control = match options.mode {
            ServerMode::Local => Some(LocalServerControl::new(&options)),
            ServerMode::DesktopBridge => None,
        };
        Self {
            options,
            local_control,
        }
    }

    fn default_workspace_id(&self) -> Option<String> {
        self.options.workspace_id.clone().or_else(|| {
            (self.options.mode == ServerMode::Local).then(|| SERVER_LOCAL_WORKSPACE_ID.to_string())
        })
    }

    fn invoke<T>(&mut self, method: &str, params: &T) -> Result<ResponseEnvelope, CliError>
    where
        T: serde::Serialize,
    {
        match self.options.mode {
            ServerMode::DesktopBridge => invoke_control(method, params, &self.options.invoke),
            ServerMode::Local => {
                let params_json = serde_json::to_string(params).map_err(|error| {
                    CliError::Control(format!("failed to encode params: {error}"))
                })?;
                let request = request(
                    &format!("server_{}", method.replace('.', "_")),
                    method,
                    &params_json,
                    SERVER_LOCAL_TOKEN,
                );
                let control = self.local_control.as_mut().ok_or_else(|| {
                    CliError::Control("local server runtime is not initialized.".to_string())
                })?;
                Ok(control.handle_request(request))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalServerBackend {
    Conpty,
    WslDirect,
}

struct LocalServerControl {
    conpty: RuntimeControlPlane<ConptyBackend>,
    wsl_direct: RuntimeControlPlane<WslDirectBackend>,
    default_backend: LocalServerBackend,
    session_routes: HashMap<String, LocalServerBackend>,
}

impl LocalServerControl {
    fn new(options: &ServerOptions) -> Self {
        let default_backend = local_server_backend_for_default(options.backend.as_deref());
        let wsl_config = match options.backend_profile.as_deref() {
            Some(distribution) => WslDirectConfig::for_distribution(distribution),
            None => WslDirectConfig::default(),
        };
        Self {
            conpty: RuntimeControlPlane::new(
                TerminalRuntime::new(ConptyBackend::new()),
                SERVER_LOCAL_TOKEN,
            ),
            wsl_direct: RuntimeControlPlane::new(
                TerminalRuntime::new(WslDirectBackend::with_config(wsl_config)),
                SERVER_LOCAL_TOKEN,
            ),
            default_backend,
            session_routes: HashMap::new(),
        }
    }

    fn handle_request(&mut self, request: RequestEnvelope) -> ResponseEnvelope {
        match request.method.as_str() {
            "session.spawn" => self.handle_spawn_request(request),
            "session.list" => self.handle_list_request(request),
            "session.get"
            | "session.read_recent"
            | "session.snapshot"
            | "session.send_text"
            | "session.send_key"
            | "session.resize"
            | "session.report_output_pressure"
            | "session.terminate" => self.handle_session_request(request),
            _ => self
                .control_mut(self.default_backend)
                .handle_request(request),
        }
    }

    fn handle_spawn_request(&mut self, request: RequestEnvelope) -> ResponseEnvelope {
        let backend = match request
            .parse_params::<SessionSpawnParams>()
            .and_then(|params| {
                local_server_backend_for_spawn(params.backend.as_deref(), self.default_backend)
            }) {
            Ok(backend) => backend,
            Err(error) => return ResponseEnvelope::error(request.id.clone(), error),
        };
        let response = self.control_mut(backend).handle_request(request);
        if let Ok(result) = response_result::<SessionSpawnResult>(&response) {
            self.session_routes.insert(result.session_id, backend);
        }
        response
    }

    fn handle_list_request(&mut self, request: RequestEnvelope) -> ResponseEnvelope {
        let conpty_response = self.conpty.handle_request(request.clone());
        let wsl_response = self.wsl_direct.handle_request(request.clone());
        let mut sessions = match response_result::<SessionListResult>(&conpty_response) {
            Ok(result) => result.sessions,
            Err(error) => {
                return ResponseEnvelope::error(
                    request.id.clone(),
                    ControlError::new(ErrorCode::InvalidRequest, error.to_string()),
                )
            }
        };
        match response_result::<SessionListResult>(&wsl_response) {
            Ok(result) => sessions.extend(result.sessions),
            Err(error) => {
                return ResponseEnvelope::error(
                    request.id.clone(),
                    ControlError::new(ErrorCode::InvalidRequest, error.to_string()),
                )
            }
        }
        ResponseEnvelope::ok_typed(request.id, &SessionListResult { sessions })
    }

    fn handle_session_request(&mut self, request: RequestEnvelope) -> ResponseEnvelope {
        let session_id = match local_server_session_id(&request) {
            Ok(session_id) => session_id,
            Err(error) => return ResponseEnvelope::error(request.id.clone(), error),
        };
        let Some(backend) = self.session_routes.get(&session_id).copied() else {
            return self.handle_unrouted_session_request(request);
        };
        let is_terminate = request.method == "session.terminate";
        let response = self.control_mut(backend).handle_request(request);
        if is_terminate && response_is_ok(&response) {
            self.session_routes.remove(&session_id);
        }
        response
    }

    fn handle_unrouted_session_request(&mut self, request: RequestEnvelope) -> ResponseEnvelope {
        let first = self.conpty.handle_request(request.clone());
        if response_is_ok(&first) {
            return first;
        }
        let second = self.wsl_direct.handle_request(request);
        if response_is_ok(&second) {
            return second;
        }
        first
    }

    fn control_mut(&mut self, backend: LocalServerBackend) -> &mut dyn LocalServerControlPlane {
        match backend {
            LocalServerBackend::Conpty => &mut self.conpty,
            LocalServerBackend::WslDirect => &mut self.wsl_direct,
        }
    }
}

trait LocalServerControlPlane {
    fn handle_request(&mut self, request: RequestEnvelope) -> ResponseEnvelope;
}

impl<B> LocalServerControlPlane for RuntimeControlPlane<B>
where
    B: SessionBackend,
{
    fn handle_request(&mut self, request: RequestEnvelope) -> ResponseEnvelope {
        RuntimeControlPlane::handle_request(self, request)
    }
}

fn local_server_backend_for_default(backend: Option<&str>) -> LocalServerBackend {
    match backend {
        Some("conpty") => LocalServerBackend::Conpty,
        _ => LocalServerBackend::WslDirect,
    }
}

fn local_server_backend_for_spawn(
    backend: Option<&str>,
    default_backend: LocalServerBackend,
) -> Result<LocalServerBackend, ControlError> {
    match backend {
        None => Ok(default_backend),
        Some("conpty") => Ok(LocalServerBackend::Conpty),
        Some("wsl-direct") => Ok(LocalServerBackend::WslDirect),
        Some("wsl-tmux-control") => Err(ControlError::new(
            ErrorCode::InvalidRequest,
            "Local server mode does not support durable WSL tmux sessions yet. Start server mode with --desktop-control to use the desktop tmux control plane.",
        )),
        Some(other) => Err(ControlError::new(
            ErrorCode::InvalidRequest,
            format!("Unknown backend '{other}'."),
        )),
    }
}

fn local_server_session_id(request: &RequestEnvelope) -> Result<String, ControlError> {
    match request.method.as_str() {
        "session.get" => request
            .parse_params::<SessionIdParams>()
            .map(|params| params.session_id),
        "session.read_recent" => request
            .parse_params::<SessionReadRecentParams>()
            .map(|params| params.session_id),
        "session.snapshot" => request
            .parse_params::<SessionSnapshotParams>()
            .map(|params| params.session_id),
        "session.send_text" => request
            .parse_params::<SessionSendTextParams>()
            .map(|params| params.session_id),
        "session.send_key" => request
            .parse_params::<SessionSendKeyParams>()
            .map(|params| params.session_id),
        "session.resize" => request
            .parse_params::<SessionResizeParams>()
            .map(|params| params.session_id),
        "session.report_output_pressure" => request
            .parse_params::<SessionOutputPressureParams>()
            .map(|params| params.session_id),
        "session.terminate" => request
            .parse_params::<SessionTerminateParams>()
            .map(|params| params.session_id),
        other => Err(ControlError::new(
            ErrorCode::UnsupportedMethod,
            format!("Unsupported routed session method '{other}'."),
        )),
    }
}

fn response_is_ok(response: &ResponseEnvelope) -> bool {
    matches!(response.outcome, ResponseOutcome::Ok { .. })
}

fn run_server<W>(options: ServerOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let listener =
        TcpListener::bind(format!("{}:{}", options.host, options.port)).map_err(|error| {
            CliError::Io(io::Error::new(
                error.kind(),
                format!("failed to bind server: {error}"),
            ))
        })?;
    let local_addr = listener.local_addr()?;
    let url = format_server_url(&options.host, local_addr.port());
    let state = ServerState::new(options);

    if state.options.invoke.json {
        write_json_value(
            &serde_json::json!({
                "url": url,
                "host": state.options.host.clone(),
                "port": local_addr.port(),
                "mode": state.options.mode.as_str(),
                "workspace_id": state.default_workspace_id(),
                "backend": state.options.backend.clone(),
                "backend_profile": state.options.backend_profile.clone(),
                "allow_remote": state.options.allow_remote,
                "control_pipe": state.options.invoke.pipe_name.clone(),
            }),
            output,
        )?;
    } else {
        writeln!(output, "AgentMux server listening on {url}")?;
        writeln!(
            output,
            "Mode: {}{}",
            state.options.mode.as_str(),
            if state.options.mode == ServerMode::DesktopBridge {
                format!(" via {}", state.options.invoke.pipe_name)
            } else {
                String::new()
            }
        )?;
        if state.options.mode == ServerMode::DesktopBridge {
            writeln!(
                output,
                "Open the URL in a browser. Keep AgentMux desktop running for workspace-backed sessions."
            )?;
        } else {
            writeln!(output, "Open the URL in a browser.")?;
        }
    }

    let shared_state = Arc::new(Mutex::new(state));
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&shared_state);
                thread::spawn(move || {
                    if let Err(error) = handle_server_stream(stream, state) {
                        eprintln!("agentmux server request failed: {error}");
                    }
                });
            }
            Err(error) => eprintln!("agentmux server accept failed: {error}"),
        }
    }

    Ok(())
}

fn format_server_url(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("http://[{host}]:{port}/")
    } else {
        format!("http://{host}:{port}/")
    }
}

fn handle_server_stream(
    mut stream: TcpStream,
    state: Arc<Mutex<ServerState>>,
) -> Result<(), CliError> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    if is_websocket_upgrade(&stream)? {
        return handle_server_websocket(stream, state);
    }

    let request = match read_http_request(&mut stream).and_then(|raw| parse_http_request(&raw)) {
        Ok(request) => request,
        Err(error) => {
            let response = api_error_response(400, &format!("Bad request: {error}"));
            return write_http_response(&mut stream, &response);
        }
    };
    let response = match state.lock() {
        Ok(mut state) => route_server_request(&request, &mut state),
        Err(_) => api_error_response(503, "Server state is unavailable."),
    };
    write_http_response(&mut stream, &response)
}

fn is_websocket_upgrade(stream: &TcpStream) -> Result<bool, CliError> {
    let mut buffer = [0_u8; 4096];
    let read = stream.peek(&mut buffer)?;
    if read == 0 {
        return Ok(false);
    }
    let head = String::from_utf8_lossy(&buffer[..read]).to_ascii_lowercase();
    Ok(head.starts_with("get ")
        && head.contains("\r\nupgrade: websocket")
        && head.contains("\r\nsec-websocket-key:"))
}

// The accept_hdr handshake callback must return tungstenite's
// Result<Response, ErrorResponse>; that Err variant (an http::Response) is
// large and imposed by the upstream API, so the large-Err lint can't be
// resolved by boxing here.
#[allow(clippy::result_large_err)]
fn handle_server_websocket(
    stream: TcpStream,
    state: Arc<Mutex<ServerState>>,
) -> Result<(), CliError> {
    let target = Arc::new(Mutex::new(None::<String>));
    let captured_target = Arc::clone(&target);
    let mut socket = accept_hdr(
        stream,
        move |request: &tungstenite::handshake::server::Request, response| {
            if let Ok(mut target) = captured_target.lock() {
                *target = Some(request.uri().to_string());
            }
            Ok(response)
        },
    )
    .map_err(|error| CliError::Control(format!("websocket handshake error: {error}")))?;

    let _ = socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(8)));
    let _ = socket
        .get_mut()
        .set_write_timeout(Some(Duration::from_secs(5)));

    let target = target
        .lock()
        .ok()
        .and_then(|target| target.clone())
        .unwrap_or_else(|| "/".to_string());
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_string(), Some(query.to_string())),
        None => (target, None),
    };
    let Some(session_id) = session_id_from_path(&path, "/stream") else {
        let _ = socket.close(None);
        return Ok(());
    };
    let mut offset = initial_websocket_output_offset(&session_id, query.as_deref(), &state)?;

    loop {
        match socket.read() {
            Ok(WsMessage::Text(text)) => {
                handle_server_websocket_message(&session_id, &text, &state)?;
            }
            Ok(WsMessage::Binary(_)) | Ok(WsMessage::Ping(_)) | Ok(WsMessage::Pong(_)) => {}
            Ok(WsMessage::Close(_)) => break,
            Ok(WsMessage::Frame(_)) => {}
            Err(WsError::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(WsError::ConnectionClosed) | Err(WsError::AlreadyClosed) => break,
            Err(error) => return Err(websocket_error(error)),
        }

        let had_output =
            send_server_websocket_output(&session_id, &state, &mut socket, &mut offset)?;
        thread::sleep(Duration::from_millis(if had_output { 3 } else { 12 }));
    }

    Ok(())
}

fn websocket_error(error: WsError) -> CliError {
    CliError::Control(format!("websocket error: {error}"))
}

fn initial_websocket_output_offset(
    session_id: &str,
    query: Option<&str>,
    state: &Arc<Mutex<ServerState>>,
) -> Result<u64, CliError> {
    if let Some(offset) = query_param(query, "since_offset").and_then(|value| value.parse().ok()) {
        return Ok(offset);
    }
    let params = SessionSnapshotParams {
        session_id: session_id.to_string(),
        since_offset: None,
    };
    let result = {
        let mut state = state
            .lock()
            .map_err(|_| CliError::Control("Server state is unavailable.".to_string()))?;
        state
            .invoke("session.snapshot", &params)
            .and_then(|response| response_result::<SessionSnapshotResult>(&response))?
    };
    Ok(result.end_offset)
}

fn send_server_websocket_output(
    session_id: &str,
    state: &Arc<Mutex<ServerState>>,
    socket: &mut tungstenite::WebSocket<TcpStream>,
    offset: &mut u64,
) -> Result<bool, CliError> {
    let params = SessionSnapshotParams {
        session_id: session_id.to_string(),
        since_offset: Some(*offset),
    };
    let result = {
        let mut state = state
            .lock()
            .map_err(|_| CliError::Control("Server state is unavailable.".to_string()))?;
        state
            .invoke("session.snapshot", &params)
            .and_then(|response| response_result::<SessionSnapshotResult>(&response))?
    };
    if result.end_offset == *offset && result.bytes_base64.is_empty() {
        return Ok(false);
    }
    let frame_type = if result.base_offset > *offset {
        "reset"
    } else {
        "output"
    };
    *offset = result.end_offset;
    socket
        .send(WsMessage::Text(
            serde_json::json!({
                "type": frame_type,
                "session_id": result.session_id,
                "from_offset": result.base_offset,
                "end_offset": result.end_offset,
                "bytes_base64": result.bytes_base64,
            })
            .to_string(),
        ))
        .map_err(websocket_error)?;
    Ok(true)
}

fn handle_server_websocket_message(
    session_id: &str,
    text: &str,
    state: &Arc<Mutex<ServerState>>,
) -> Result<(), CliError> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| CliError::InvalidArgs(format!("invalid websocket message: {error}")))?;
    let Some(message_type) = value.get("type").and_then(|value| value.as_str()) else {
        return Ok(());
    };
    let (method, params) = match message_type {
        "input" => {
            let Some(text) = value.get("text").and_then(|value| value.as_str()) else {
                return Ok(());
            };
            (
                "session.send_text",
                serde_json::to_value(SessionSendTextParams {
                    session_id: session_id.to_string(),
                    text: text.to_string(),
                })
                .map_err(|error| CliError::Control(error.to_string()))?,
            )
        }
        "key" => {
            let Some(key) = value.get("key").and_then(|value| value.as_str()) else {
                return Ok(());
            };
            (
                "session.send_key",
                serde_json::to_value(SessionSendKeyParams {
                    session_id: session_id.to_string(),
                    key: key.to_string(),
                })
                .map_err(|error| CliError::Control(error.to_string()))?,
            )
        }
        "resize" => {
            let columns = value
                .get("columns")
                .and_then(|value| value.as_u64())
                .and_then(|value| u16::try_from(value).ok())
                .unwrap_or(120);
            let rows = value
                .get("rows")
                .and_then(|value| value.as_u64())
                .and_then(|value| u16::try_from(value).ok())
                .unwrap_or(30);
            (
                "session.resize",
                serde_json::to_value(SessionResizeParams {
                    session_id: session_id.to_string(),
                    columns,
                    rows,
                })
                .map_err(|error| CliError::Control(error.to_string()))?,
            )
        }
        "terminate" => (
            "session.terminate",
            serde_json::to_value(SessionTerminateParams {
                session_id: session_id.to_string(),
                mode: "soft".to_string(),
            })
            .map_err(|error| CliError::Control(error.to_string()))?,
        ),
        "pressure" => (
            "session.report_output_pressure",
            serde_json::to_value(SessionOutputPressureParams {
                session_id: session_id.to_string(),
                queued_bytes: value
                    .get("queuedBytes")
                    .or_else(|| value.get("queued_bytes"))
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0),
                max_queued_bytes: value
                    .get("maxQueuedBytes")
                    .or_else(|| value.get("max_queued_bytes"))
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0),
                backpressure_events: value
                    .get("backpressureEvents")
                    .or_else(|| value.get("backpressure_events"))
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0),
                write_in_flight: value
                    .get("writeInFlight")
                    .or_else(|| value.get("write_in_flight"))
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false),
            })
            .map_err(|error| CliError::Control(error.to_string()))?,
        ),
        _ => return Ok(()),
    };
    let mut state = state
        .lock()
        .map_err(|_| CliError::Control("Server state is unavailable.".to_string()))?;
    let response = state.invoke(method, &params)?;
    response_result::<serde_json::Value>(&response)?;
    Ok(())
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    query: Option<String>,
    body: String,
}

#[derive(Debug)]
struct HttpResponse {
    status_code: u16,
    reason: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> Result<String, CliError> {
    const MAX_HTTP_REQUEST_BYTES: usize = 1024 * 1024;
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let mut expected_len = None;

    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_HTTP_REQUEST_BYTES {
            return Err(CliError::InvalidArgs("request is too large.".to_string()));
        }

        if expected_len.is_none() {
            if let Some(header_end) = http_header_end(&buffer) {
                let content_length = parse_content_length(&buffer[..header_end])?;
                expected_len = Some(header_end + 4 + content_length);
            }
        }

        if expected_len.is_some_and(|len| buffer.len() >= len) {
            break;
        }
    }

    Ok(String::from_utf8_lossy(&buffer).to_string())
}

fn http_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &[u8]) -> Result<usize, CliError> {
    let headers = String::from_utf8_lossy(headers);
    for line in headers.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            return value.trim().parse::<usize>().map_err(|_| {
                CliError::InvalidArgs("Content-Length must be a positive integer.".to_string())
            });
        }
    }
    Ok(0)
}

fn parse_http_request(raw: &str) -> Result<HttpRequest, CliError> {
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .ok_or_else(|| CliError::InvalidArgs("missing HTTP header terminator.".to_string()))?;
    let mut lines = head.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| CliError::InvalidArgs("missing HTTP request line.".to_string()))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| CliError::InvalidArgs("missing HTTP method.".to_string()))?
        .to_string();
    let target = parts
        .next()
        .ok_or_else(|| CliError::InvalidArgs("missing HTTP target.".to_string()))?;
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_string(), Some(query.to_string())),
        None => (target.to_string(), None),
    };

    Ok(HttpRequest {
        method,
        path,
        query,
        body: body.to_string(),
    })
}

fn route_server_request(request: &HttpRequest, state: &mut ServerState) -> HttpResponse {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => server_desktop_index_response(&state.options),
        ("GET", "/api/state") => server_state_response(state),
        ("GET", "/api/sessions") => server_sessions_response(request, state),
        ("GET", "/api/wsl/distributions") => server_wsl_distributions_response(),
        ("POST", "/api/tmux/check") => server_tmux_check_response(request),
        ("POST", "/api/spawn") => server_spawn_response(request, state),
        ("OPTIONS", _) => empty_response(204),
        _ => {
            if request.method == "GET" {
                if let Some(session_id) = session_id_from_path(&request.path, "/snapshot") {
                    return server_snapshot_response(&session_id, request, state);
                }
                if let Some(session_id) = session_id_from_path(&request.path, "/recent") {
                    return server_read_recent_response(&session_id, request, state);
                }
            }
            if request.method == "POST" {
                if let Some(session_id) = session_id_from_path(&request.path, "/send") {
                    return server_send_text_response(&session_id, request, state);
                }
                if let Some(session_id) = session_id_from_path(&request.path, "/key") {
                    return server_send_key_response(&session_id, request, state);
                }
                if let Some(session_id) = session_id_from_path(&request.path, "/resize") {
                    return server_resize_response(&session_id, request, state);
                }
                if let Some(session_id) = session_id_from_path(&request.path, "/terminate") {
                    return server_terminate_response(&session_id, state);
                }
            }
            if request.method == "GET" {
                if let Some(response) = server_static_asset_response(&request.path) {
                    return response;
                }
            }
            api_error_response(404, "Not found.")
        }
    }
}

fn server_state_response(state: &mut ServerState) -> HttpResponse {
    match load_server_state(state) {
        Ok(value) => api_json_response(200, value),
        Err(error) => api_error_response(503, &server_control_error_message(error, &state.options)),
    }
}

fn load_server_state(state: &mut ServerState) -> Result<serde_json::Value, CliError> {
    let (workspaces, selected_workspace_id) = match state.options.mode {
        ServerMode::DesktopBridge => {
            let workspace_response = state.invoke("workspace.list", &serde_json::json!({}))?;
            let workspaces: WorkspaceListResult = response_result(&workspace_response)?;
            let selected_workspace_id = state.options.workspace_id.clone().or_else(|| {
                workspaces
                    .workspaces
                    .first()
                    .map(|workspace| workspace.workspace_id.clone())
            });
            (
                serde_json::json!(workspaces.workspaces),
                selected_workspace_id,
            )
        }
        ServerMode::Local => {
            let workspace_id = state
                .default_workspace_id()
                .unwrap_or_else(|| SERVER_LOCAL_WORKSPACE_ID.to_string());
            (
                serde_json::json!([{
                    "workspace_id": workspace_id.clone(),
                    "name": "Server",
                    "root_pane_id": "root",
                    "active_pane_id": "root",
                    "project_root": state.options.cwd.clone(),
                    "environment_profile_id": null,
                    "description": null,
                    "icon": null,
                    "color": null,
                    "default_wsl_distribution": state.options.backend_profile.clone(),
                    "default_agent_command": null
                }]),
                Some(workspace_id),
            )
        }
    };
    let sessions = if let Some(workspace_id) = selected_workspace_id.as_deref() {
        let session_response = state.invoke(
            "session.list",
            &SessionListParams {
                workspace_id: Some(workspace_id.to_string()),
            },
        )?;
        response_result::<SessionListResult>(&session_response)?.sessions
    } else {
        Vec::new()
    };

    Ok(serde_json::json!({
        "mode": state.options.mode.as_str(),
        "control_pipe": if state.options.mode == ServerMode::DesktopBridge {
            Some(state.options.invoke.pipe_name.clone())
        } else {
            None
        },
        "default_workspace_id": selected_workspace_id,
        "workspaces": workspaces,
        "sessions": sessions,
        "defaults": server_defaults_json(&state.options),
    }))
}

fn server_sessions_response(request: &HttpRequest, state: &mut ServerState) -> HttpResponse {
    let workspace_id =
        query_param(request.query.as_deref(), "workspace").or_else(|| state.default_workspace_id());
    let params = SessionListParams { workspace_id };
    match state
        .invoke("session.list", &params)
        .and_then(|response| response_result::<SessionListResult>(&response))
    {
        Ok(result) => api_json_response(200, serde_json::json!({ "sessions": result.sessions })),
        Err(error) => api_error_response(503, &server_control_error_message(error, &state.options)),
    }
}

fn server_wsl_distributions_response() -> HttpResponse {
    match discover_wsl_distributions_from_backend() {
        Ok(distributions) => api_json_response(
            200,
            serde_json::json!({
                "distributions": distributions
                    .into_iter()
                    .map(|distribution| {
                        serde_json::json!({
                            "name": distribution.name,
                            "is_default": distribution.is_default
                        })
                    })
                    .collect::<Vec<_>>()
            }),
        ),
        Err(diagnostic) => api_error_response(503, &diagnostic.message),
    }
}

fn server_tmux_check_response(request: &HttpRequest) -> HttpResponse {
    let parsed = match parse_json_body::<ServerTmuxCheckRequest>(&request.body) {
        Ok(value) => value,
        Err(error) => return api_error_response(400, &error.to_string()),
    };
    api_json_response(200, server_tmux_diagnostics(parsed.distribution.as_deref()))
}

fn server_tmux_diagnostics(distribution: Option<&str>) -> serde_json::Value {
    let mut command = Command::new("wsl.exe");
    if let Some(distribution) = distribution.filter(|value| !value.trim().is_empty()) {
        command.arg("--distribution").arg(distribution);
    }
    command.args([
        "--exec",
        "sh",
        "-lc",
        "command -v tmux >/dev/null 2>&1 && tmux -V",
    ]);

    match command.output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout)
                .replace('\0', "")
                .trim()
                .to_string();
            serde_json::json!({
                "available": true,
                "distribution": distribution,
                "version": if version.is_empty() { None } else { Some(version.clone()) },
                "message": if version.is_empty() {
                    "tmux is available in WSL.".to_string()
                } else {
                    format!("tmux is available in WSL: {version}")
                }
            })
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr)
                .replace('\0', "")
                .trim()
                .to_string();
            serde_json::json!({
                "available": false,
                "distribution": distribution,
                "version": null,
                "message": if stderr.is_empty() {
                    "tmux was not found in the selected WSL distribution. Install it with `sudo apt update && sudo apt install -y tmux`.".to_string()
                } else {
                    format!(
                        "tmux was not found in the selected WSL distribution. Install it with `sudo apt update && sudo apt install -y tmux`. ({stderr})"
                    )
                }
            })
        }
        Err(error) => serde_json::json!({
            "available": false,
            "distribution": distribution,
            "version": null,
            "message": format!(
                "Could not check tmux through wsl.exe. Install WSL first, then install tmux with `sudo apt update && sudo apt install -y tmux`. ({error})"
            )
        }),
    }
}

fn server_spawn_response(request: &HttpRequest, state: &mut ServerState) -> HttpResponse {
    let parsed = match parse_json_body::<ServerSpawnRequest>(&request.body) {
        Ok(value) => value,
        Err(error) => return api_error_response(400, &error.to_string()),
    };
    let workspace_id = match parsed.workspace_id.or_else(|| state.default_workspace_id()) {
        Some(workspace_id) if !workspace_id.trim().is_empty() => workspace_id,
        _ => return api_error_response(400, "Select a workspace before starting a terminal."),
    };
    let backend = parsed.backend.or_else(|| state.options.backend.clone());
    let backend_profile = parsed
        .backend_profile
        .or(parsed.distribution)
        .or_else(|| state.options.backend_profile.clone());
    if backend_profile.is_some() && backend.as_deref() != Some("wsl-direct") {
        return api_error_response(400, "A WSL distribution requires backend wsl-direct.");
    }
    let command = match parsed.command {
        Some(command) if !command.is_empty() => command,
        _ => match parsed.command_line.as_deref() {
            Some(command_line) if !command_line.trim().is_empty() => {
                match split_command_line(command_line) {
                    Ok(command) => command,
                    Err(error) => return api_error_response(400, &error.to_string()),
                }
            }
            _ => state.options.command.clone(),
        },
    };
    if command.is_empty() {
        return api_error_response(400, "Command cannot be empty.");
    }

    let params = SessionSpawnParams {
        workspace_id,
        backend,
        backend_profile,
        command,
        cwd: parsed.cwd.or_else(|| state.options.cwd.clone()),
        env: Vec::new(),
        columns: parsed.columns.unwrap_or(state.options.columns),
        rows: parsed.rows.unwrap_or(state.options.rows),
        durability: Some("ephemeral".to_string()),
        placement: None,
        pane_id: None,
    };

    match state
        .invoke("session.spawn", &params)
        .and_then(|response| response_result::<SessionSpawnResult>(&response))
    {
        Ok(result) => {
            api_json_response(200, serde_json::json!({ "session_id": result.session_id }))
        }
        Err(error) => api_error_response(503, &server_control_error_message(error, &state.options)),
    }
}

fn server_read_recent_response(
    session_id: &str,
    request: &HttpRequest,
    state: &mut ServerState,
) -> HttpResponse {
    let max_bytes = query_param(request.query.as_deref(), "max_bytes")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(state.options.max_recent_bytes);
    let params = SessionReadRecentParams {
        session_id: session_id.to_string(),
        max_bytes,
    };
    match state
        .invoke("session.read_recent", &params)
        .and_then(|response| response_result::<SessionReadRecentResult>(&response))
    {
        Ok(result) => api_json_response(
            200,
            serde_json::json!({
                "session_id": result.session_id,
                "text": result.text,
                "byte_count": result.byte_count,
            }),
        ),
        Err(error) => api_error_response(503, &server_control_error_message(error, &state.options)),
    }
}

fn server_snapshot_response(
    session_id: &str,
    request: &HttpRequest,
    state: &mut ServerState,
) -> HttpResponse {
    let since_offset = query_param(request.query.as_deref(), "since_offset")
        .and_then(|value| value.parse::<u64>().ok());
    let params = SessionSnapshotParams {
        session_id: session_id.to_string(),
        since_offset,
    };
    match state
        .invoke("session.snapshot", &params)
        .and_then(|response| response_result::<SessionSnapshotResult>(&response))
    {
        Ok(result) => api_json_response(
            200,
            serde_json::json!({
                "session_id": result.session_id,
                "base_offset": result.base_offset,
                "end_offset": result.end_offset,
                "bytes_base64": result.bytes_base64,
            }),
        ),
        Err(error) => api_error_response(503, &server_control_error_message(error, &state.options)),
    }
}

fn server_send_text_response(
    session_id: &str,
    request: &HttpRequest,
    state: &mut ServerState,
) -> HttpResponse {
    let parsed = match parse_json_body::<ServerSendTextRequest>(&request.body) {
        Ok(value) => value,
        Err(error) => return api_error_response(400, &error.to_string()),
    };
    let Some(text) = parsed.text else {
        return api_error_response(400, "Missing text.");
    };
    let params = SessionSendTextParams {
        session_id: session_id.to_string(),
        text,
    };
    match state
        .invoke("session.send_text", &params)
        .and_then(|response| response_result::<serde_json::Value>(&response))
    {
        Ok(result) => api_json_response(200, result),
        Err(error) => api_error_response(503, &server_control_error_message(error, &state.options)),
    }
}

fn server_send_key_response(
    session_id: &str,
    request: &HttpRequest,
    state: &mut ServerState,
) -> HttpResponse {
    let parsed = match parse_json_body::<ServerSendKeyRequest>(&request.body) {
        Ok(value) => value,
        Err(error) => return api_error_response(400, &error.to_string()),
    };
    let Some(key) = parsed.key else {
        return api_error_response(400, "Missing key.");
    };
    let params = SessionSendKeyParams {
        session_id: session_id.to_string(),
        key,
    };
    match state
        .invoke("session.send_key", &params)
        .and_then(|response| response_result::<serde_json::Value>(&response))
    {
        Ok(result) => api_json_response(200, result),
        Err(error) => api_error_response(503, &server_control_error_message(error, &state.options)),
    }
}

fn server_resize_response(
    session_id: &str,
    request: &HttpRequest,
    state: &mut ServerState,
) -> HttpResponse {
    let parsed = match parse_json_body::<ServerResizeRequest>(&request.body) {
        Ok(value) => value,
        Err(error) => return api_error_response(400, &error.to_string()),
    };
    let params = agentmux_ipc::SessionResizeParams {
        session_id: session_id.to_string(),
        columns: parsed.columns.unwrap_or(state.options.columns),
        rows: parsed.rows.unwrap_or(state.options.rows),
    };
    match state
        .invoke("session.resize", &params)
        .and_then(|response| response_result::<serde_json::Value>(&response))
    {
        Ok(result) => api_json_response(200, result),
        Err(error) => api_error_response(503, &server_control_error_message(error, &state.options)),
    }
}

fn server_terminate_response(session_id: &str, state: &mut ServerState) -> HttpResponse {
    let params = SessionTerminateParams {
        session_id: session_id.to_string(),
        mode: "soft".to_string(),
    };
    match state
        .invoke("session.terminate", &params)
        .and_then(|response| response_result::<serde_json::Value>(&response))
    {
        Ok(result) => api_json_response(200, result),
        Err(error) => api_error_response(503, &server_control_error_message(error, &state.options)),
    }
}

fn parse_json_body<T>(body: &str) -> Result<T, CliError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str(body.trim())
        .map_err(|error| CliError::InvalidArgs(format!("Invalid JSON request body: {error}")))
}

fn split_command_line(input: &str) -> Result<Vec<String>, CliError> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut quote = None;

    while let Some(ch) = chars.next() {
        match ch {
            '"' | '\'' => {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(ch);
                } else {
                    current.push(ch);
                }
            }
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                } else {
                    current.push(ch);
                }
            }
            value if value.is_whitespace() && quote.is_none() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            value => current.push(value),
        }
    }

    if let Some(ch) = quote {
        return Err(CliError::InvalidArgs(format!(
            "unterminated quote '{ch}' in command."
        )));
    }
    if !current.is_empty() {
        args.push(current);
    }
    if args.is_empty() {
        return Err(CliError::InvalidArgs(
            "Command cannot be empty.".to_string(),
        ));
    }
    Ok(args)
}

fn session_id_from_path(path: &str, suffix: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/session/")?;
    let raw_session_id = rest.strip_suffix(suffix)?;
    let raw_session_id = raw_session_id.strip_suffix('/').unwrap_or(raw_session_id);
    if raw_session_id.is_empty() || raw_session_id.contains('/') {
        return None;
    }
    Some(url_decode(raw_session_id))
}

fn query_param(query: Option<&str>, name: &str) -> Option<String> {
    query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(key, value)| (url_decode(key) == name).then(|| url_decode(value)))
}

fn url_decode(input: &str) -> String {
    let mut output = String::new();
    let mut bytes = input.as_bytes().iter().copied();
    while let Some(byte) = bytes.next() {
        match byte {
            b'+' => output.push(' '),
            b'%' => {
                let high = bytes.next();
                let low = bytes.next();
                match (high.and_then(hex_value), low.and_then(hex_value)) {
                    (Some(high), Some(low)) => output.push((high * 16 + low) as char),
                    _ => output.push('%'),
                }
            }
            value => output.push(value as char),
        }
    }
    output
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn server_control_error_message(error: CliError, options: &ServerOptions) -> String {
    let mut message = error.to_string();
    if message.contains("control pipe") || message.contains("AgentMux control pipe") {
        message.push_str(" Start the AgentMux Windows app, then retry this web action.");
    }
    if options.backend.as_deref() == Some("wsl-direct") {
        message.push_str(
            " For WSL terminals, install WSL and at least one Linux distribution before spawning.",
        );
    }
    message
}

fn server_defaults_json(options: &ServerOptions) -> serde_json::Value {
    serde_json::json!({
        "workspace_id": options.workspace_id.clone().unwrap_or_else(|| {
            if options.mode == ServerMode::Local {
                SERVER_LOCAL_WORKSPACE_ID.to_string()
            } else {
                String::new()
            }
        }),
        "backend": options.backend.clone(),
        "backend_profile": options.backend_profile.clone(),
        "cwd": options.cwd.clone(),
        "command": options.command.clone(),
        "command_line": options.command.join(" "),
        "columns": options.columns,
        "rows": options.rows,
        "max_recent_bytes": options.max_recent_bytes,
    })
}

fn api_json_response(status_code: u16, result: serde_json::Value) -> HttpResponse {
    let body = serde_json::json!({
        "ok": true,
        "result": result,
    });
    json_response(status_code, body)
}

fn api_error_response(status_code: u16, message: &str) -> HttpResponse {
    json_response(
        status_code,
        serde_json::json!({
            "ok": false,
            "error": message,
        }),
    )
}

fn json_response(status_code: u16, body: serde_json::Value) -> HttpResponse {
    let body = serde_json::to_string(&body)
        .unwrap_or_else(|_| r#"{"ok":false,"error":"failed to encode response"}"#.to_string());
    HttpResponse {
        status_code,
        reason: http_reason(status_code),
        content_type: "application/json; charset=utf-8",
        body: body.into_bytes(),
    }
}

fn html_response(status_code: u16, body: String) -> HttpResponse {
    HttpResponse {
        status_code,
        reason: http_reason(status_code),
        content_type: "text/html; charset=utf-8",
        body: body.into_bytes(),
    }
}

fn empty_response(status_code: u16) -> HttpResponse {
    HttpResponse {
        status_code,
        reason: http_reason(status_code),
        content_type: "text/plain; charset=utf-8",
        body: Vec::new(),
    }
}

fn bytes_response(status_code: u16, content_type: &'static str, body: Vec<u8>) -> HttpResponse {
    HttpResponse {
        status_code,
        reason: http_reason(status_code),
        content_type,
        body,
    }
}

fn server_desktop_index_response(options: &ServerOptions) -> HttpResponse {
    let Some(index_path) = desktop_ui_file_path("index.html") else {
        return html_response(503, missing_desktop_ui_html());
    };
    match fs::read_to_string(&index_path) {
        Ok(html) => html_response(200, inject_server_bootstrap(html, options)),
        Err(error) => html_response(
            503,
            format!(
                "<!doctype html><meta charset=\"utf-8\"><title>AgentMux</title><body>Failed to read desktop UI bundle: {error}</body>"
            ),
        ),
    }
}

fn server_static_asset_response(request_path: &str) -> Option<HttpResponse> {
    let relative_path = request_path.strip_prefix('/')?;
    if relative_path.is_empty() || relative_path.starts_with("api/") {
        return None;
    }
    let file_path = desktop_ui_file_path(relative_path)?;
    if !file_path.is_file() {
        return None;
    }
    let body = fs::read(&file_path).ok()?;
    Some(bytes_response(200, content_type_for_path(&file_path), body))
}

fn desktop_ui_file_path(relative_path: &str) -> Option<PathBuf> {
    let decoded = url_decode(relative_path);
    let path = Path::new(&decoded);
    if path.components().any(|component| {
        !matches!(
            component,
            std::path::Component::Normal(_) | std::path::Component::CurDir
        )
    }) {
        return None;
    }
    Some(desktop_ui_dist_dir()?.join(path))
}

fn desktop_ui_dist_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join("apps").join("desktop").join("dist"));
        candidates.push(current_dir.join("dist"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("apps").join("desktop").join("dist"));
            candidates.push(exe_dir.join("..").join("apps").join("desktop").join("dist"));
            candidates.push(exe_dir.join("dist"));
        }
    }
    candidates
        .into_iter()
        .find(|path| path.join("index.html").is_file())
}

fn inject_server_bootstrap(mut html: String, options: &ServerOptions) -> String {
    let bootstrap = serde_json::to_string(&serde_json::json!({
        "baseUrl": "",
        "mode": options.mode.as_str(),
        "defaults": server_defaults_json(options),
    }))
    .unwrap_or_else(|_| "{}".to_string());
    let script = format!("<script>window.__AGENTMUX_SERVER__ = {bootstrap};</script>");
    if let Some(index) = html.find("</head>") {
        html.insert_str(index, &script);
        html
    } else {
        format!("{script}{html}")
    }
}

fn missing_desktop_ui_html() -> String {
    r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>AgentMux</title>
</head>
<body>
  <main style="font:14px/1.5 system-ui,sans-serif;padding:24px;max-width:760px">
    <h1>AgentMux desktop UI bundle is missing</h1>
    <p>Run <code>npm --prefix apps/desktop run build</code>, then restart <code>agentmux server</code>.</p>
  </main>
</body>
</html>"#
        .to_string()
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("svg") => "image/svg+xml; charset=utf-8",
        Some("txt") => "text/plain; charset=utf-8",
        Some("ttf") => "font/ttf",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn http_reason(status_code: u16) -> &'static str {
    match status_code {
        200 => "OK",
        204 => "No Content",
        304 => "Not Modified",
        400 => "Bad Request",
        404 => "Not Found",
        503 => "Service Unavailable",
        _ => "OK",
    }
}

fn write_http_response(stream: &mut TcpStream, response: &HttpResponse) -> Result<(), CliError> {
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        response.status_code,
        response.reason,
        response.content_type,
        response.body.len()
    )?;
    stream.write_all(&response.body)?;
    Ok(())
}

#[allow(dead_code)]
fn server_index_html(options: &ServerOptions) -> String {
    let bootstrap = serde_json::to_string(&serde_json::json!({
        "mode": options.mode.as_str(),
        "defaults": server_defaults_json(options),
    }))
    .unwrap_or_else(|_| "{}".to_string());
    SERVER_INDEX_TEMPLATE.replace("__BOOTSTRAP_JSON__", &bootstrap)
}

#[allow(dead_code)]
const SERVER_INDEX_TEMPLATE: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>AgentMux Web Terminal</title>
  <style>
    :root {
      color-scheme: dark;
      --bg: #111318;
      --panel: #171a21;
      --panel-2: #1f242c;
      --line: #313844;
      --text: #eff3f6;
      --muted: #aab4c0;
      --accent: #5eb6f6;
      --danger: #ff7a7a;
      --ok: #76d39b;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      min-height: 100vh;
      background: var(--bg);
      color: var(--text);
      font: 14px/1.45 "Segoe UI", system-ui, sans-serif;
    }
    button, input, select {
      font: inherit;
    }
    .app {
      min-height: 100vh;
      display: grid;
      grid-template-rows: auto 1fr auto;
    }
    header {
      display: grid;
      grid-template-columns: auto minmax(200px, 1fr) auto;
      gap: 12px;
      align-items: center;
      padding: 12px 16px;
      border-bottom: 1px solid var(--line);
      background: #151922;
    }
    h1 {
      margin: 0;
      font-size: 16px;
      font-weight: 650;
      letter-spacing: 0;
    }
    .controls {
      display: grid;
      grid-template-columns: minmax(150px, .9fr) minmax(150px, .9fr) minmax(100px, .55fr) minmax(120px, .65fr) minmax(160px, 1fr);
      gap: 8px;
      align-items: center;
    }
    select, input {
      width: 100%;
      min-height: 34px;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: var(--panel);
      color: var(--text);
      padding: 6px 9px;
      outline: none;
    }
    select:focus, input:focus {
      border-color: var(--accent);
    }
    .actions {
      display: flex;
      gap: 8px;
      justify-content: flex-end;
    }
    button {
      min-height: 34px;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: var(--panel-2);
      color: var(--text);
      padding: 6px 11px;
      cursor: pointer;
      white-space: nowrap;
    }
    button.primary {
      border-color: #357fb5;
      background: #176da8;
    }
    button.danger {
      border-color: #8e454c;
      background: #5c242a;
    }
    button:disabled {
      opacity: .45;
      cursor: default;
    }
    main {
      min-height: 0;
      display: grid;
      grid-template-rows: 1fr auto;
    }
    #terminal {
      margin: 0;
      padding: 14px 16px;
      overflow: auto;
      min-height: 0;
      background: #090b0f;
      color: #e6edf3;
      font: 13px/1.45 "Cascadia Mono", Consolas, monospace;
      white-space: pre-wrap;
      word-break: break-word;
    }
    .compose {
      display: grid;
      grid-template-columns: 1fr auto auto auto;
      gap: 8px;
      padding: 10px 16px;
      border-top: 1px solid var(--line);
      background: #151922;
    }
    footer {
      display: flex;
      justify-content: space-between;
      gap: 12px;
      padding: 8px 16px;
      border-top: 1px solid var(--line);
      color: var(--muted);
      background: #11151d;
      font-size: 12px;
    }
    #status[data-kind="error"] { color: var(--danger); }
    #status[data-kind="ok"] { color: var(--ok); }
    @media (max-width: 860px) {
      header {
        grid-template-columns: 1fr;
      }
      .controls {
        grid-template-columns: 1fr 1fr;
      }
      .actions {
        justify-content: flex-start;
      }
      .compose {
        grid-template-columns: 1fr auto;
      }
    }
  </style>
</head>
<body>
  <div class="app">
    <header>
      <h1>AgentMux Web Terminal</h1>
      <div class="controls">
        <select id="workspace"></select>
        <select id="session"></select>
        <select id="backend">
          <option value="wsl-direct">WSL</option>
          <option value="conpty">ConPTY</option>
        </select>
        <input id="distribution" placeholder="Distribution">
        <input id="command" autocomplete="off" spellcheck="false" placeholder="Command">
      </div>
      <div class="actions">
        <button id="refresh">Refresh</button>
        <button id="start" class="primary">Start</button>
        <button id="terminate" class="danger">Close</button>
      </div>
    </header>
    <main>
      <pre id="terminal"></pre>
      <div class="compose">
        <input id="input" autocomplete="off" spellcheck="false">
        <button id="send">Send</button>
        <button id="enter">Enter</button>
        <button id="break">Ctrl+C</button>
      </div>
    </main>
    <footer>
      <span id="status">Starting</span>
      <span id="mode">local</span>
    </footer>
  </div>
  <script id="bootstrap" type="application/json">__BOOTSTRAP_JSON__</script>
  <script>
    const bootstrap = JSON.parse(document.getElementById('bootstrap').textContent);
    const defaults = bootstrap.defaults || {};
    const state = { sessionId: null, timer: null, loading: false };
    const el = (id) => document.getElementById(id);

    function setStatus(message, kind = '') {
      el('status').textContent = message;
      el('status').dataset.kind = kind;
    }

    async function api(path, options = {}) {
      const response = await fetch(path, {
        ...options,
        headers: {
          ...(options.headers || {}),
          ...(options.body ? { 'Content-Type': 'application/json' } : {}),
        },
      });
      const data = await response.json();
      if (!response.ok || data.ok === false) {
        throw new Error(data.error || response.statusText);
      }
      return data.result;
    }

    function option(text, value, selected) {
      const node = document.createElement('option');
      node.value = value || '';
      node.textContent = text;
      node.selected = selected;
      return node;
    }

    function renderWorkspaces(workspaces, selectedId) {
      const select = el('workspace');
      select.replaceChildren();
      if (!workspaces.length) {
        select.appendChild(option('No workspace', '', true));
        return;
      }
      for (const workspace of workspaces) {
        select.appendChild(option(workspace.name, workspace.workspace_id, workspace.workspace_id === selectedId));
      }
    }

    function renderSessions(sessions) {
      const select = el('session');
      select.replaceChildren();
      select.appendChild(option('New session', '', !state.sessionId));
      for (const session of sessions) {
        const label = `${session.session_id} (${session.state})`;
        select.appendChild(option(label, session.session_id, session.session_id === state.sessionId));
      }
    }

    async function loadState() {
      const data = await api('/api/state');
      renderWorkspaces(data.workspaces || [], data.default_workspace_id || '');
      renderSessions(data.sessions || []);
      el('backend').value = defaults.backend || 'wsl-direct';
      el('distribution').value = defaults.backend_profile || '';
      el('command').value = defaults.command_line || '';
      el('mode').textContent = data.control_pipe ? `${data.mode} | ${data.control_pipe}` : data.mode;
      setStatus('Ready', 'ok');
    }

    async function loadSessions() {
      const workspace = el('workspace').value;
      const data = await api(`/api/sessions?workspace=${encodeURIComponent(workspace)}`);
      renderSessions(data.sessions || []);
    }

    async function startSession() {
      const payload = {
        workspace_id: el('workspace').value,
        backend: el('backend').value,
        backend_profile: el('distribution').value || null,
        command_line: el('command').value,
      };
      const data = await api('/api/spawn', { method: 'POST', body: JSON.stringify(payload) });
      state.sessionId = data.session_id;
      el('session').value = state.sessionId;
      await loadSessions();
      startPolling();
      setStatus(`Attached ${state.sessionId}`, 'ok');
    }

    async function pollOnce() {
      if (!state.sessionId || state.loading) return;
      state.loading = true;
      try {
        const data = await api(`/api/session/${encodeURIComponent(state.sessionId)}/recent?max_bytes=${defaults.max_recent_bytes || 1048576}`);
        const terminal = el('terminal');
        const shouldStick = terminal.scrollTop + terminal.clientHeight >= terminal.scrollHeight - 32;
        terminal.textContent = data.text || '';
        if (shouldStick) terminal.scrollTop = terminal.scrollHeight;
        setStatus(`Live ${state.sessionId}`, 'ok');
      } catch (error) {
        setStatus(error.message, 'error');
      } finally {
        state.loading = false;
      }
    }

    function startPolling() {
      if (state.timer) clearInterval(state.timer);
      state.timer = setInterval(pollOnce, 600);
      pollOnce();
    }

    async function sendText(text) {
      if (!state.sessionId || !text) return;
      await api(`/api/session/${encodeURIComponent(state.sessionId)}/send`, {
        method: 'POST',
        body: JSON.stringify({ text }),
      });
      await pollOnce();
    }

    async function terminateSession() {
      if (!state.sessionId) return;
      await api(`/api/session/${encodeURIComponent(state.sessionId)}/terminate`, { method: 'POST' });
      state.sessionId = null;
      await loadSessions();
      setStatus('Closed', 'ok');
    }

    el('refresh').addEventListener('click', () => loadState().catch((error) => setStatus(error.message, 'error')));
    el('workspace').addEventListener('change', () => loadSessions().catch((error) => setStatus(error.message, 'error')));
    el('session').addEventListener('change', () => {
      state.sessionId = el('session').value || null;
      if (state.sessionId) startPolling();
    });
    el('start').addEventListener('click', () => startSession().catch((error) => setStatus(error.message, 'error')));
    el('terminate').addEventListener('click', () => terminateSession().catch((error) => setStatus(error.message, 'error')));
    el('send').addEventListener('click', async () => {
      const input = el('input');
      await sendText(input.value);
      input.value = '';
    });
    el('enter').addEventListener('click', async () => {
      const input = el('input');
      await sendText(`${input.value}\r`);
      input.value = '';
    });
    el('break').addEventListener('click', () => sendText('\u0003').catch((error) => setStatus(error.message, 'error')));
    el('input').addEventListener('keydown', async (event) => {
      if (event.key === 'Enter') {
        event.preventDefault();
        const input = el('input');
        await sendText(`${input.value}\r`);
        input.value = '';
      }
    });

    loadState().catch((error) => setStatus(error.message, 'error'));
  </script>
</body>
</html>
"#;

fn run_agent_set_state<W>(options: AgentSetStateOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("agent.set_state", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: AgentStateResult = response_result(&response)?;
    write_agent_state(&result, output)
}

fn run_agent_get_state<W>(options: AgentGetStateOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("agent.get_state", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: AgentStateResult = response_result(&response)?;
    write_agent_state(&result, output)
}

fn run_agent_list_attention<W>(
    options: AgentListAttentionOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("agent.list_attention", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: AgentAttentionListResult = response_result(&response)?;
    if result.sessions.is_empty() {
        writeln!(output, "No sessions need attention.")?;
        return Ok(());
    }
    for session in result.sessions {
        write_agent_state(&session, output)?;
    }
    Ok(())
}

fn run_agent_clear_attention<W>(
    options: AgentClearAttentionOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("agent.clear_attention", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<serde_json::Value>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

fn run_notification_list<W>(
    options: NotificationListOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("notification.list", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: NotificationListResult = response_result(&response)?;
    if result.notifications.is_empty() {
        writeln!(output, "No notifications.")?;
        return Ok(());
    }
    for notification in result.notifications {
        write_notification(&notification, output)?;
    }
    Ok(())
}

fn run_notification_dismiss<W>(
    options: NotificationDismissOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("notification.dismiss", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<serde_json::Value>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

fn run_notify<W>(options: NotificationCreateOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("notification.create", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let notification: NotificationSummaryResult = response_result(&response)?;
    write_notification(&notification, output)
}

fn run_notification_clear<W>(
    options: NotificationClearOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("notification.clear", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: NotificationClearResult = response_result(&response)?;
    writeln!(output, "cleared\t{}", result.cleared)?;
    Ok(())
}

fn run_sidebar_set_status<W>(
    options: SidebarStatusSetOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("sidebar.set_status", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: agentmux_ipc::SidebarStatusResult = response_result(&response)?;
    writeln!(
        output,
        "{}\t{}\t{}\t{}",
        result.workspace_id, result.key, result.label, result.priority
    )?;
    Ok(())
}

fn run_sidebar_clear_status<W>(
    options: SidebarStatusKeyOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("sidebar.clear_status", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<serde_json::Value>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

fn run_sidebar_list_status<W>(
    options: SidebarWorkspaceOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("sidebar.list_status", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: SidebarStatusListResult = response_result(&response)?;
    if result.statuses.is_empty() {
        writeln!(output, "No sidebar status.")?;
        return Ok(());
    }
    for status in result.statuses {
        writeln!(
            output,
            "{}\t{}\t{}\t{}",
            status.workspace_id, status.key, status.label, status.priority
        )?;
    }
    Ok(())
}

fn run_sidebar_set_progress<W>(
    options: SidebarProgressSetOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("sidebar.set_progress", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: agentmux_ipc::SidebarProgressResult = response_result(&response)?;
    writeln!(
        output,
        "{}\t{:.0}%\t{}",
        result.workspace_id,
        result.value * 100.0,
        result.label.as_deref().unwrap_or("-")
    )?;
    Ok(())
}

fn run_sidebar_clear_progress<W>(
    options: SidebarWorkspaceOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("sidebar.clear_progress", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<serde_json::Value>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

fn run_sidebar_log<W>(options: SidebarLogOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("sidebar.log", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let log: agentmux_ipc::SidebarLogResult = response_result(&response)?;
    write_sidebar_log(&log, output)
}

fn run_sidebar_clear_log<W>(
    options: SidebarWorkspaceOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("sidebar.clear_log", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<serde_json::Value>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

fn run_sidebar_list_log<W>(options: SidebarLogListOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("sidebar.list_log", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: SidebarLogListResult = response_result(&response)?;
    if result.logs.is_empty() {
        writeln!(output, "No sidebar log entries.")?;
        return Ok(());
    }
    for log in result.logs {
        write_sidebar_log(&log, output)?;
    }
    Ok(())
}

fn run_sidebar_state<W>(options: SidebarWorkspaceOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("sidebar.state", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: SidebarStateResult = response_result(&response)?;
    writeln!(output, "workspace\t{}", result.workspace_id)?;
    if let Some(cwd) = result.cwd {
        writeln!(output, "cwd\t{cwd}")?;
    }
    for status in result.statuses {
        writeln!(output, "status\t{}\t{}", status.key, status.label)?;
    }
    if let Some(progress) = result.progress {
        writeln!(
            output,
            "progress\t{:.0}%\t{}",
            progress.value * 100.0,
            progress.label.as_deref().unwrap_or("-")
        )?;
    }
    for log in result.logs {
        write_sidebar_log(&log, output)?;
    }
    Ok(())
}

fn run_capabilities<W>(options: ControlInvokeOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("system.capabilities", &serde_json::json!({}), &options)?;
    if options.json {
        return write_json_response(&response, output);
    }
    let result: SystemCapabilitiesResult = response_result(&response)?;
    writeln!(
        output,
        "{}\t{}\tmethods={}",
        result.product,
        result.access_mode,
        result.methods.len()
    )?;
    for method in result.methods {
        writeln!(output, "{method}")?;
    }
    Ok(())
}

fn run_identify<W>(options: IdentifyOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("system.identify", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: SystemIdentifyResult = response_result(&response)?;
    writeln!(output, "in_agentmux\t{}", result.in_agentmux)?;
    writeln!(
        output,
        "workspace\t{}",
        result.workspace_id.as_deref().unwrap_or("-")
    )?;
    writeln!(output, "pane\t{}", result.pane_id.as_deref().unwrap_or("-"))?;
    writeln!(
        output,
        "surface\t{}",
        result.surface_id.as_deref().unwrap_or("-")
    )?;
    writeln!(
        output,
        "session\t{}",
        result.session_id.as_deref().unwrap_or("-")
    )?;
    writeln!(
        output,
        "backend\t{}",
        result.backend_kind.as_deref().unwrap_or("-")
    )?;
    writeln!(output, "pipe\t{}", result.control_pipe)?;
    Ok(())
}

fn run_ping<W>(options: ControlInvokeOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("system.ping", &serde_json::json!({}), &options)?;
    if options.json {
        return write_json_response(&response, output);
    }
    response_result::<serde_json::Value>(&response)?;
    writeln!(output, "pong")?;
    Ok(())
}

fn run_cmux_current_workspace<W>(
    options: CmuxWorkspaceQueryOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let params = SystemIdentifyParams {
        workspace_id: options.workspace_id,
    };
    let response = invoke_control("system.identify", &params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: SystemIdentifyResult = response_result(&response)?;
    let workspace_id = require_context_field(result.workspace_id, "workspace")?;
    writeln!(output, "{workspace_id}")?;
    Ok(())
}

fn run_cmux_list_surfaces<W>(
    options: CmuxWorkspaceQueryOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&options.invoke, options.workspace_id)?;
    let workspace_id = require_context_field(context.workspace_id, "workspace")?;
    let params = WorkspaceIdParams { workspace_id };
    let response = invoke_control("workspace.get", &params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let detail: WorkspaceDetailResult = response_result(&response)?;
    if detail.surfaces.is_empty() {
        writeln!(output, "No surfaces.")?;
        return Ok(());
    }
    for surface in detail.surfaces {
        writeln!(
            output,
            "{}\t{}\t{}\t{}",
            surface.surface_id,
            surface.surface_type,
            surface.title,
            surface.session_id.as_deref().unwrap_or("-")
        )?;
    }
    Ok(())
}

fn run_cmux_new_split<W>(options: CmuxPaneSplitOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&options.invoke, options.workspace_id)?;
    let workspace_id = require_context_field(context.workspace_id, "workspace")?;
    let pane_id = require_context_field(context.pane_id, "pane")?;
    let params = PaneSplitParams {
        workspace_id,
        pane_id,
        axis: cmux_split_axis(&options.direction)?.to_string(),
        ratio: options.ratio,
    };
    let response = invoke_control("pane.split", &params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let detail: WorkspaceDetailResult = response_result(&response)?;
    write_workspace_detail(&detail, output)
}

fn run_cmux_send_text<W>(options: CmuxActiveSendTextOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&options.invoke, options.workspace_id)?;
    let session_id = require_context_field(context.session_id, "terminal session")?;
    let params = SessionSendTextParams {
        session_id,
        text: options.text,
    };
    let response = invoke_control("session.send_text", &params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<serde_json::Value>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

fn run_cmux_send_key<W>(options: CmuxActiveSendKeyOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&options.invoke, options.workspace_id)?;
    let session_id = require_context_field(context.session_id, "terminal session")?;
    let params = SessionSendKeyParams {
        session_id,
        key: options.key,
    };
    let response = invoke_control("session.send_key", &params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<serde_json::Value>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

fn run_agent_integration_setup<W>(
    options: AgentIntegrationSetupOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let base_dir = resolve_cmuxterm_base_dir(options.base_dir.as_deref())?;
    let mut runtime = setup_agent_integration_files(options.kind, &base_dir, Vec::new())?;
    if options.install_packages && options.kind == AgentIntegrationKind::Omo {
        let distribution = options
            .distribution
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .or_else(wsl_distribution_from_env);
        runtime.package_install = Some(ensure_omo_package_installed_for_distribution(
            &base_dir,
            distribution.as_deref(),
        )?);
    }
    if options.invoke.json {
        return write_json_value(&agent_integration_runtime_json(&runtime), output);
    }
    writeln!(
        output,
        "{}\t{}",
        options.kind.command_name(),
        runtime.shim_dir.display()
    )?;
    if let Some(shadow_config_dir) = runtime.shadow_config_dir {
        writeln!(output, "shadow-config\t{}", shadow_config_dir.display())?;
    }
    if let Some(package_install) = runtime.package_install {
        writeln!(
            output,
            "package-install\t{}\t{}",
            package_install.status,
            package_install.package_dir.display()
        )?;
        writeln!(
            output,
            "node-modules\t{}",
            package_install.node_modules_status
        )?;
        if let Some(distribution) = package_install.distribution {
            writeln!(output, "package-install-distribution\t{distribution}")?;
        }
    }
    Ok(())
}

fn run_agent_integration_env<W>(
    options: AgentIntegrationSetupOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let runtime = prepare_agent_integration_runtime(
        options.kind,
        &options.invoke,
        options.workspace_id,
        options.base_dir.as_deref(),
        Vec::new(),
        false,
    )?;
    let distribution = options
        .distribution
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    if let Some(distribution) = distribution {
        let launcher = std::env::current_exe().map_err(CliError::Io)?;
        let spec = build_agent_integration_wsl_command(&runtime, &distribution, &launcher)?;
        if options.invoke.json {
            return write_json_value(
                &serde_json::json!({
                    "integration": runtime.kind.command_name(),
                    "distribution": distribution,
                    "executable": spec.executable,
                    "args": spec.args,
                }),
                output,
            );
        }
        writeln!(output, "wsl-distribution\t{distribution}")?;
        writeln!(output, "wsl-command\t{}", spec.executable)?;
        for arg in spec.args {
            writeln!(output, "wsl-arg\t{arg}")?;
        }
        return Ok(());
    }
    if options.invoke.json {
        return write_json_value(&agent_integration_runtime_json(&runtime), output);
    }
    for (key, value) in runtime.env {
        writeln!(output, "{key}={value}")?;
    }
    Ok(())
}

fn run_agent_integration_install_shims<W>(
    options: AgentIntegrationInstallOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let base_dir = resolve_cmuxterm_base_dir(options.base_dir.as_deref())?;
    let bin_dir = options
        .bin_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| base_dir.join("bin"));
    let mut result = install_agent_integration_shims(
        &base_dir,
        &bin_dir,
        options.powershell_profile.as_deref().map(Path::new),
        options.shell_profile.as_deref().map(Path::new),
    )?;
    if options.user_path {
        result.user_path = Some(ensure_windows_user_path_contains(&bin_dir)?);
    }
    if options.invoke.json {
        return write_json_value(&agent_integration_install_result_json(&result), output);
    }
    writeln!(output, "bin\t{}", result.bin_dir.display())?;
    writeln!(
        output,
        "powershell-snippet\t{}",
        result.powershell_snippet.display()
    )?;
    writeln!(output, "shell-snippet\t{}", result.shell_snippet.display())?;
    if let Some(profile) = result.powershell_profile {
        writeln!(output, "powershell-profile\t{}", profile.display())?;
    }
    if let Some(profile) = result.shell_profile {
        writeln!(output, "shell-profile\t{}", profile.display())?;
    }
    if let Some(user_path) = result.user_path {
        writeln!(
            output,
            "user-path\t{}\t{}",
            user_path.status, user_path.detail
        )?;
    }
    Ok(())
}

fn run_agent_integration_doctor<W>(
    options: AgentIntegrationDoctorOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let base_dir = resolve_cmuxterm_base_dir(options.base_dir.as_deref())?;
    let bin_dir = options
        .bin_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| base_dir.join("bin"));
    let wsl_distribution = options
        .distribution
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(wsl_distribution_from_env);
    let result = inspect_agent_integrations(
        &base_dir,
        &bin_dir,
        options.kind,
        wsl_distribution.as_deref(),
    );
    if options.invoke.json {
        return write_json_value(&agent_integration_doctor_result_json(&result), output);
    }

    writeln!(output, "base\t{}", result.base_dir.display())?;
    writeln!(output, "bin\t{}", result.bin_dir.display())?;
    if let Some(distribution) = result.wsl_distribution.as_deref() {
        writeln!(output, "wsl\t{distribution}")?;
    }
    writeln!(
        output,
        "path\t{}",
        if result.bin_dir_on_path {
            "bin directory is on PATH"
        } else {
            "bin directory is not on PATH"
        }
    )?;
    for item in result.integrations {
        writeln!(
            output,
            "{}\t{}\t{}",
            item.kind.command_name(),
            item.status,
            item.install_hint
        )?;
        for check in item.checks {
            writeln!(
                output,
                "  {}\t{}\t{}",
                if check.ok { "ok" } else { "missing" },
                check.name,
                check.detail
            )?;
            if let Some(fix) = check.fix {
                writeln!(output, "  fix\t{}\t{}", check.name, fix)?;
            }
        }
    }
    Ok(())
}

fn run_agent_integration_launch<W>(
    options: AgentIntegrationLaunchOptions,
    _output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let runtime = prepare_agent_integration_runtime(
        options.kind,
        &options.invoke,
        options.workspace_id,
        options.base_dir.as_deref(),
        options.args,
        options.kind == AgentIntegrationKind::Omo,
    )?;
    let status = run_agent_integration_runtime(&runtime).map_err(|error| {
        CliError::Control(format!(
            "failed to launch {} integration command '{}': {error}",
            runtime.kind.command_name(),
            runtime.command
        ))
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::Control(format!(
            "{} integration command '{}' exited with status {}.",
            runtime.kind.command_name(),
            runtime.command,
            status
        )))
    }
}

fn run_agent_integration_runtime(
    runtime: &AgentIntegrationRuntime,
) -> io::Result<std::process::ExitStatus> {
    if let Some(distribution) = wsl_distribution_from_env() {
        let launcher = std::env::current_exe()?;
        let spec = build_agent_integration_wsl_command(runtime, &distribution, &launcher)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
        return Command::new(spec.executable).args(spec.args).status();
    }

    let mut command = Command::new(&runtime.command);
    command.args(&runtime.args);
    for (key, value) in &runtime.env {
        command.env(key, value);
    }
    command.status()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AgentIntegrationLaunchSpec {
    executable: String,
    args: Vec<String>,
}

fn build_agent_integration_wsl_command(
    runtime: &AgentIntegrationRuntime,
    distribution: &str,
    launcher: &Path,
) -> Result<AgentIntegrationLaunchSpec, CliError> {
    let shim_dir = path_to_wsl_value(&runtime.shim_dir)?;
    let launcher = path_to_wsl_value(launcher)?;
    let mut args = vec![
        "--distribution".to_string(),
        distribution.to_string(),
        "--exec".to_string(),
        "env".to_string(),
    ];
    for (key, value) in agent_integration_wsl_env(runtime, &launcher)? {
        args.push(format!("{key}={value}"));
    }
    args.extend([
        "sh".to_string(),
        "-lc".to_string(),
        "export PATH=\"$1:$PATH\"; shift; exec \"$@\"".to_string(),
        "agentmux-integration-launch".to_string(),
        shim_dir,
        runtime.command.clone(),
    ]);
    args.extend(runtime.args.iter().cloned());
    Ok(AgentIntegrationLaunchSpec {
        executable: "wsl.exe".to_string(),
        args,
    })
}

fn agent_integration_wsl_env(
    runtime: &AgentIntegrationRuntime,
    launcher_wsl_path: &str,
) -> Result<Vec<(String, String)>, CliError> {
    let mut env = Vec::new();
    for (key, value) in &runtime.env {
        match key.as_str() {
            "PATH" => {}
            "OPENCODE_CONFIG_DIR" => {
                env.push((key.clone(), path_to_wsl_value(Path::new(value))?));
            }
            "NODE_OPTIONS" if runtime.kind == AgentIntegrationKind::Omc => {
                let restore_module =
                    runtime
                        .node_options_restore_module
                        .as_ref()
                        .ok_or_else(|| {
                            CliError::Control(
                                "OMC integration is missing the NODE_OPTIONS restore module."
                                    .to_string(),
                            )
                        })?;
                let restore_module = path_to_wsl_value(restore_module)?;
                env.push((
                    key.clone(),
                    node_options_with_required_module(
                        std::env::var("NODE_OPTIONS").ok().as_deref(),
                        Path::new(&restore_module),
                    ),
                ));
            }
            _ => env.push((key.clone(), value.clone())),
        }
    }
    env.push(("CMUX_EXE".to_string(), launcher_wsl_path.to_string()));
    env.push(("AGENTMUX_EXE".to_string(), launcher_wsl_path.to_string()));
    Ok(env)
}

fn wsl_distribution_from_env() -> Option<String> {
    std::env::var("AGENTMUX_WSL_DISTRIBUTION")
        .ok()
        .or_else(|| std::env::var("CMUX_WSL_DISTRIBUTION").ok())
        .or_else(|| std::env::var("WSL_DISTRO_NAME").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn path_to_wsl_value(path: &Path) -> Result<String, CliError> {
    let value = path_to_env_value(path);
    if value.starts_with('/') || value.starts_with("~/") || value == "~" {
        return Ok(value);
    }
    fallback_windows_path_to_wsl(&value).ok_or_else(|| {
        CliError::Control(format!(
            "Could not convert Windows path '{}' to a WSL path.",
            path.display()
        ))
    })
}

fn run_tmux_compat<W>(options: TmuxCompatOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let TmuxCompatOptions {
        invoke,
        workspace_id,
        command,
    } = options;
    match command {
        TmuxCompatCommand::DisplayMessage { format } => {
            run_tmux_display_message(invoke, workspace_id, format, output)
        }
        TmuxCompatCommand::ListPanes {
            all_workspaces,
            format,
        } => run_tmux_list_panes(invoke, workspace_id, all_workspaces, format, output),
        TmuxCompatCommand::ListWindows {
            all_workspaces,
            format,
        } => run_tmux_list_windows(invoke, workspace_id, all_workspaces, format, output),
        TmuxCompatCommand::ListSessions { format } => {
            run_tmux_list_sessions(invoke, workspace_id, format, output)
        }
        TmuxCompatCommand::HasSession { target } => {
            run_tmux_has_session(invoke, workspace_id, target, output)
        }
        TmuxCompatCommand::SelectPane { target_pane_id } => {
            run_tmux_select_pane(invoke, workspace_id, target_pane_id, output)
        }
        TmuxCompatCommand::SelectWindow { target_window } => {
            run_tmux_select_window(invoke, workspace_id, target_window, output)
        }
        TmuxCompatCommand::SwitchClient { target } => {
            run_tmux_switch_client(invoke, workspace_id, target, output)
        }
        TmuxCompatCommand::RenameWindow {
            target_window,
            name,
        } => run_tmux_rename_window(invoke, workspace_id, target_window, name, output),
        TmuxCompatCommand::RenameSession {
            target_session,
            name,
        } => run_tmux_rename_session(invoke, workspace_id, target_session, name, output),
        TmuxCompatCommand::CapturePane {
            target_pane_id,
            max_bytes,
        } => run_tmux_capture_pane(invoke, workspace_id, target_pane_id, max_bytes, output),
        TmuxCompatCommand::KillPane { target_pane_id } => {
            run_tmux_kill_pane(invoke, workspace_id, target_pane_id, output)
        }
        TmuxCompatCommand::KillWindow { target_window } => {
            run_tmux_kill_window(invoke, workspace_id, target_window, output)
        }
        TmuxCompatCommand::SendKeys {
            target_pane_id,
            keys,
        } => run_tmux_send_keys(invoke, workspace_id, target_pane_id, keys, output),
        TmuxCompatCommand::SplitWindow {
            target_pane_id,
            axis,
            command,
            format,
        } => run_tmux_split_window(
            invoke,
            workspace_id,
            target_pane_id,
            axis,
            command,
            format,
            output,
        ),
        TmuxCompatCommand::NewWindow { command, format } => {
            run_tmux_new_window(invoke, workspace_id, command, format, output)
        }
        TmuxCompatCommand::NewSession {
            session_name,
            cwd,
            command,
            format,
        } => run_tmux_new_session(
            invoke,
            workspace_id,
            session_name,
            cwd,
            command,
            format,
            output,
        ),
    }
}

fn run_tmux_display_message<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    format: Option<String>,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&invoke, workspace_id)?;
    let pane_id = pane_from_env()
        .or(context.pane_id.clone())
        .ok_or_else(|| CliError::Control("No active tmux-compatible pane context.".to_string()))?;
    let text = render_tmux_format(format.as_deref(), &context, &pane_id);
    writeln!(output, "{text}")?;
    Ok(())
}

fn run_tmux_list_panes<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    all_workspaces: bool,
    format: Option<String>,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let (_active_workspace_id, active_pane_id) =
        tmux_active_workspace_and_pane(&invoke, workspace_id.clone());
    let details = load_tmux_list_workspace_details(&invoke, workspace_id, all_workspaces)?;
    if invoke.json {
        let panes = details
            .iter()
            .flat_map(|detail| tmux_pane_rows(detail, active_pane_id.as_deref()))
            .map(|row| {
                serde_json::json!({
                    "pane_id": agentmux_pane_to_tmux_pane(&row.pane.pane_id),
                    "agentmux_pane_id": row.pane.pane_id,
                    "pane_index": row.pane_index,
                    "window_index": row.window_index,
                    "window_id": row.window_id,
                    "window_name": row.window_name,
                    "session_id": row.session_id,
                    "session_name": row.session_name,
                    "active": row.pane.pane_id == active_pane_id.as_deref().unwrap_or_default(),
                    "surface_id": row.pane.mounted_surface_id,
                })
            })
            .collect::<Vec<_>>();
        return write_json_value(&serde_json::json!({ "panes": panes }), output);
    }
    for detail in &details {
        for row in tmux_pane_rows(detail, active_pane_id.as_deref()) {
            let context = SystemIdentifyResult {
                in_agentmux: true,
                workspace_id: Some(detail.workspace.workspace_id.clone()),
                pane_id: Some(row.pane.pane_id.clone()),
                surface_id: row.pane.mounted_surface_id.clone(),
                session_id: session_id_for_pane(detail, &row.pane.pane_id),
                cwd: detail.workspace.project_root.clone(),
                backend_kind: None,
                control_pipe: invoke.pipe_name.clone(),
            };
            writeln!(
                output,
                "{}",
                render_tmux_pane_format(format.as_deref(), &context, &row)
            )?;
        }
    }
    Ok(())
}

fn run_tmux_list_windows<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    all_workspaces: bool,
    format: Option<String>,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let (active_workspace_id, _) = tmux_active_workspace_and_pane(&invoke, workspace_id.clone());
    let details = load_tmux_list_workspace_details(&invoke, workspace_id, all_workspaces)?;
    if invoke.json {
        let windows = details
            .iter()
            .flat_map(|detail| tmux_window_rows(detail, active_workspace_id.as_deref()))
            .map(|row| {
                serde_json::json!({
                    "window_index": row.window_index,
                    "window_id": row.root_pane_id,
                    "active": row.active,
                    "name": row.window_name,
                    "session_id": row.session_id,
                    "session_name": row.session_name,
                })
            })
            .collect::<Vec<_>>();
        return write_json_value(&serde_json::json!({ "windows": windows }), output);
    }
    for detail in &details {
        for row in tmux_window_rows(detail, active_workspace_id.as_deref()) {
            writeln!(
                output,
                "{}",
                render_tmux_window_format_from_row(format.as_deref(), &row)
            )?;
        }
    }
    Ok(())
}

fn run_tmux_list_sessions<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    format: Option<String>,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let active_workspace_id = identify_context(&invoke, workspace_id)
        .ok()
        .and_then(|context| context.workspace_id);
    let response = invoke_control("workspace.list", &serde_json::json!({}), &invoke)?;
    let result: WorkspaceListResult = response_result(&response)?;
    let details = result
        .workspaces
        .iter()
        .map(|workspace| load_workspace_detail(&invoke, &workspace.workspace_id))
        .collect::<Result<Vec<_>, _>>()?;
    if invoke.json {
        let sessions = result
            .workspaces
            .iter()
            .zip(details.iter())
            .map(|(workspace, detail)| {
                serde_json::json!({
                    "session_id": workspace.workspace_id,
                    "session_name": workspace.name,
                    "session_windows": tmux_session_window_count(detail),
                    "attached": active_workspace_id.as_deref() == Some(workspace.workspace_id.as_str()),
                })
            })
            .collect::<Vec<_>>();
        return write_json_value(&serde_json::json!({ "sessions": sessions }), output);
    }
    for (workspace, detail) in result.workspaces.iter().zip(details.iter()) {
        writeln!(
            output,
            "{}",
            render_tmux_session_format(
                format.as_deref(),
                workspace,
                tmux_session_window_count(detail),
                active_workspace_id.as_deref() == Some(workspace.workspace_id.as_str()),
            )
        )?;
    }
    Ok(())
}

fn run_tmux_has_session<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    target: Option<String>,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&invoke, workspace_id)?;
    let current_workspace_id = require_context_field(context.workspace_id.clone(), "workspace")?;
    let (target_session, target_window) = split_tmux_session_window_target(target.as_deref());
    let workspace_id = resolve_tmux_session_workspace_id(
        &invoke,
        &current_workspace_id,
        target_session.as_deref(),
    )?;
    if let Some(target_window) = target_window {
        let detail = load_workspace_detail(&invoke, &workspace_id)?;
        let _root_pane_id = resolve_tmux_window_root_id(&detail, Some(&target_window))?;
    }
    if invoke.json {
        return write_json_value(
            &serde_json::json!({
                "ok": true,
                "workspace_id": workspace_id,
            }),
            output,
        );
    }
    Ok(())
}

fn run_tmux_select_pane<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    target_pane_id: Option<String>,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let (workspace_id, pane_id) =
        resolve_tmux_workspace_and_pane(&invoke, workspace_id, target_pane_id)?;
    let params = PaneFocusParams {
        workspace_id,
        pane_id,
    };
    let response = invoke_control("pane.focus", &params, &invoke)?;
    if invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<WorkspaceDetailResult>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

fn run_tmux_select_window<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    target_window: Option<String>,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&invoke, workspace_id)?;
    let workspace_id = require_context_field(context.workspace_id.clone(), "workspace")?;
    let detail = load_workspace_detail(&invoke, &workspace_id)?;
    let root_pane_id = resolve_tmux_window_root_id(&detail, target_window.as_deref())?;
    let pane_id = first_leaf_id_in_detail(&detail, &root_pane_id).ok_or_else(|| {
        CliError::Control(format!(
            "Window '{}' does not contain a focusable AgentMux pane.",
            root_pane_id
        ))
    })?;
    let response = invoke_control(
        "pane.focus",
        &PaneFocusParams {
            workspace_id,
            pane_id,
        },
        &invoke,
    )?;
    if invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<WorkspaceDetailResult>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

fn run_tmux_switch_client<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    target: Option<String>,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&invoke, workspace_id)?;
    let current_workspace_id = require_context_field(context.workspace_id.clone(), "workspace")?;
    let (target_session, target_window) = split_tmux_session_window_target(target.as_deref());
    let workspace_id = resolve_tmux_session_workspace_id(
        &invoke,
        &current_workspace_id,
        target_session.as_deref(),
    )?;
    let detail = load_workspace_detail(&invoke, &workspace_id)?;
    let root_pane_id = resolve_tmux_window_root_id(&detail, target_window.as_deref())?;
    let pane_id = first_leaf_id_in_detail(&detail, &root_pane_id).ok_or_else(|| {
        CliError::Control(format!(
            "Window '{}' does not contain a focusable AgentMux pane.",
            root_pane_id
        ))
    })?;
    let response = invoke_control(
        "pane.focus",
        &PaneFocusParams {
            workspace_id,
            pane_id,
        },
        &invoke,
    )?;
    if invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<WorkspaceDetailResult>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

fn run_tmux_rename_window<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    target_window: Option<String>,
    name: String,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&invoke, workspace_id)?;
    let current_workspace_id = require_context_field(context.workspace_id.clone(), "workspace")?;
    let (target_session, target_window) =
        split_tmux_session_window_target(target_window.as_deref());
    let workspace_id = resolve_tmux_session_workspace_id(
        &invoke,
        &current_workspace_id,
        target_session.as_deref(),
    )?;
    let detail = load_workspace_detail(&invoke, &workspace_id)?;
    let _root_pane_id = resolve_tmux_window_root_id(&detail, target_window.as_deref())?;
    rename_tmux_workspace(invoke, workspace_id, name, output)
}

fn run_tmux_rename_session<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    target_session: Option<String>,
    name: String,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&invoke, workspace_id)?;
    let current_workspace_id = require_context_field(context.workspace_id.clone(), "workspace")?;
    let workspace_id = resolve_tmux_session_workspace_id(
        &invoke,
        &current_workspace_id,
        target_session.as_deref(),
    )?;
    rename_tmux_workspace(invoke, workspace_id, name, output)
}

fn rename_tmux_workspace<W>(
    invoke: ControlInvokeOptions,
    workspace_id: String,
    name: String,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    if name.trim().is_empty() {
        return Err(CliError::InvalidArgs(
            "tmux rename requires a non-empty name.".to_string(),
        ));
    }
    let response = invoke_control(
        "workspace.rename",
        &WorkspaceRenameParams { workspace_id, name },
        &invoke,
    )?;
    if invoke.json {
        return write_json_response(&response, output);
    }
    let workspace: WorkspaceSummaryResult = response_result(&response)?;
    writeln!(output, "{}\t{}", workspace.workspace_id, workspace.name)?;
    Ok(())
}

fn run_tmux_capture_pane<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    target_pane_id: Option<String>,
    max_bytes: usize,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let (workspace_id, pane_id) =
        resolve_tmux_workspace_and_pane(&invoke, workspace_id, target_pane_id)?;
    let detail = load_workspace_detail(&invoke, &workspace_id)?;
    let session_id = session_id_for_pane(&detail, &pane_id).ok_or_else(|| {
        CliError::Control(format!(
            "Pane '{}' does not have a mounted terminal session.",
            agentmux_pane_to_tmux_pane(&pane_id)
        ))
    })?;
    let response = invoke_control(
        "session.read_recent",
        &SessionReadRecentParams {
            session_id,
            max_bytes,
        },
        &invoke,
    )?;
    if invoke.json {
        return write_json_response(&response, output);
    }
    let result: SessionReadRecentResult = response_result(&response)?;
    write!(output, "{}", result.text)?;
    Ok(())
}

fn run_tmux_kill_pane<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    target_pane_id: Option<String>,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let (workspace_id, pane_id) =
        resolve_tmux_workspace_and_pane(&invoke, workspace_id, target_pane_id)?;
    let response = invoke_control(
        "pane.close",
        &PaneCloseParams {
            workspace_id,
            pane_id,
            surface_policy: "close_surface".to_string(),
        },
        &invoke,
    )?;
    if invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<WorkspaceDetailResult>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

fn run_tmux_kill_window<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    target_window: Option<String>,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&invoke, workspace_id)?;
    let workspace_id = require_context_field(context.workspace_id.clone(), "workspace")?;
    let detail = load_workspace_detail(&invoke, &workspace_id)?;
    let roots = detail
        .panes
        .iter()
        .filter(|pane| pane.parent_pane_id.is_none())
        .count();
    if roots <= 1 {
        return Err(CliError::Control(
            "Cannot kill the last AgentMux window in a workspace.".to_string(),
        ));
    }
    let root_pane_id = resolve_tmux_window_root_id(&detail, target_window.as_deref())?;
    let surface_id = surface_id_for_window_root(&detail, &root_pane_id).ok_or_else(|| {
        CliError::Control(format!(
            "Window '{}' does not have a mounted surface to close.",
            root_pane_id
        ))
    })?;
    let response = invoke_control(
        "surface.close",
        &SurfaceCloseParams {
            workspace_id,
            surface_id,
        },
        &invoke,
    )?;
    if invoke.json {
        return write_json_response(&response, output);
    }
    response_result::<WorkspaceDetailResult>(&response)?;
    writeln!(output, "ok")?;
    Ok(())
}

fn run_tmux_send_keys<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    target_pane_id: Option<String>,
    keys: Vec<String>,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let (workspace_id, pane_id) =
        resolve_tmux_workspace_and_pane(&invoke, workspace_id, target_pane_id)?;
    let detail = load_workspace_detail(&invoke, &workspace_id)?;
    let session_id = session_id_for_pane(&detail, &pane_id).ok_or_else(|| {
        CliError::Control(format!(
            "Pane '{}' does not have a mounted terminal session.",
            agentmux_pane_to_tmux_pane(&pane_id)
        ))
    })?;

    let mut text_buffer = Vec::new();
    for key in keys {
        if let Some(agentmux_key) = tmux_key_to_agentmux_key(&key) {
            flush_tmux_text_buffer(&invoke, &session_id, &mut text_buffer)?;
            invoke_control(
                "session.send_key",
                &SessionSendKeyParams {
                    session_id: session_id.clone(),
                    key: agentmux_key.to_string(),
                },
                &invoke,
            )?;
        } else {
            text_buffer.push(key);
        }
    }
    flush_tmux_text_buffer(&invoke, &session_id, &mut text_buffer)?;

    if invoke.json {
        return write_json_value(&serde_json::json!({ "ok": true }), output);
    }
    writeln!(output, "ok")?;
    Ok(())
}

fn run_tmux_new_window<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    command: Vec<String>,
    format: Option<String>,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&invoke, workspace_id)?;
    let workspace_id = require_context_field(context.workspace_id.clone(), "workspace")?;
    let command = tmux_command_or_default_shell(command, &context);
    let response = invoke_control(
        "session.spawn",
        &SessionSpawnParams {
            workspace_id: workspace_id.clone(),
            backend: context.backend_kind.clone(),
            backend_profile: None,
            command,
            cwd: context.cwd.clone(),
            env: Vec::new(),
            columns: 120,
            rows: 30,
            durability: Some("ephemeral".to_string()),
            placement: Some("new_tab".to_string()),
            pane_id: None,
        },
        &invoke,
    )?;
    let result: SessionSpawnResult = response_result(&response)?;
    let detail = load_workspace_detail(&invoke, &workspace_id)?;
    let pane_id = pane_id_for_session(&detail, &result.session_id)
        .unwrap_or_else(|| detail.workspace.active_pane_id.clone());
    record_tmux_agent_team_metadata(
        &invoke,
        &workspace_id,
        &result.session_id,
        None,
        "new-window",
        Some(&pane_id),
    );
    if invoke.json {
        return write_json_response(&response, output);
    }
    let context = SystemIdentifyResult {
        in_agentmux: true,
        workspace_id: Some(workspace_id),
        pane_id: Some(pane_id.clone()),
        surface_id: detail
            .panes
            .iter()
            .find(|pane| pane.pane_id == pane_id)
            .and_then(|pane| pane.mounted_surface_id.clone()),
        session_id: Some(result.session_id),
        cwd: context.cwd,
        backend_kind: context.backend_kind,
        control_pipe: invoke.pipe_name.clone(),
    };
    writeln!(
        output,
        "{}",
        render_tmux_format(format.as_deref(), &context, &pane_id)
    )?;
    Ok(())
}

fn run_tmux_new_session<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    session_name: Option<String>,
    cwd: Option<String>,
    command: Vec<String>,
    format: Option<String>,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&invoke, workspace_id)?;
    let name = session_name.unwrap_or_else(|| "AgentMux".to_string());
    let cwd = cwd.or(context.cwd.clone());
    let command = tmux_command_or_default_shell(command, &context);
    let response = invoke_control(
        "workspace.create",
        &WorkspaceCreateParams {
            name: name.clone(),
            project_root: cwd.clone(),
            backend_profile: None,
        },
        &invoke,
    )?;
    let workspace: WorkspaceSummaryResult = response_result(&response)?;
    let mut session_id = None;
    let mut surface_id = None;
    if !command.is_empty() {
        let spawn = invoke_control(
            "session.spawn",
            &SessionSpawnParams {
                workspace_id: workspace.workspace_id.clone(),
                backend: context.backend_kind.clone(),
                backend_profile: None,
                command,
                cwd: cwd.clone(),
                env: Vec::new(),
                columns: 120,
                rows: 30,
                durability: Some("ephemeral".to_string()),
                placement: Some("active_pane".to_string()),
                pane_id: Some(workspace.active_pane_id.clone()),
            },
            &invoke,
        )?;
        let spawn_result: SessionSpawnResult = response_result(&spawn)?;
        session_id = Some(spawn_result.session_id.clone());
        record_tmux_agent_team_metadata(
            &invoke,
            &workspace.workspace_id,
            &spawn_result.session_id,
            None,
            "new-session",
            Some(&workspace.active_pane_id),
        );
        let detail = load_workspace_detail(&invoke, &workspace.workspace_id)?;
        surface_id = detail
            .panes
            .iter()
            .find(|pane| pane.pane_id == workspace.active_pane_id)
            .and_then(|pane| pane.mounted_surface_id.clone());
    }
    if invoke.json {
        return write_json_value(
            &serde_json::json!({
                "workspace_id": workspace.workspace_id,
                "session_name": name,
                "pane_id": agentmux_pane_to_tmux_pane(&workspace.active_pane_id),
                "agentmux_pane_id": workspace.active_pane_id,
                "surface_id": surface_id,
                "session_id": session_id,
            }),
            output,
        );
    }
    let context = SystemIdentifyResult {
        in_agentmux: true,
        workspace_id: Some(workspace.workspace_id.clone()),
        pane_id: Some(workspace.active_pane_id.clone()),
        surface_id,
        session_id,
        cwd,
        backend_kind: context.backend_kind,
        control_pipe: invoke.pipe_name.clone(),
    };
    let text = format
        .as_deref()
        .map(|format| render_tmux_format(Some(format), &context, &workspace.active_pane_id))
        .unwrap_or(workspace.workspace_id);
    writeln!(output, "{text}")?;
    Ok(())
}

fn run_tmux_split_window<W>(
    invoke: ControlInvokeOptions,
    workspace_id: Option<String>,
    target_pane_id: Option<String>,
    axis: String,
    command: Vec<String>,
    format: Option<String>,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&invoke, workspace_id.clone())?;
    let workspace_id = require_context_field(context.workspace_id.clone(), "workspace")?;
    let pane_id = target_pane_id
        .or_else(pane_from_env)
        .or(context.pane_id.clone())
        .ok_or_else(|| CliError::Control("No active tmux-compatible pane context.".to_string()))?;
    let split = invoke_control(
        "pane.split",
        &PaneSplitParams {
            workspace_id: workspace_id.clone(),
            pane_id: pane_id.clone(),
            axis,
            ratio: None,
        },
        &invoke,
    )?;
    let detail: WorkspaceDetailResult = response_result(&split)?;
    let new_pane_id = split_child_pane_id(&detail, &pane_id).ok_or_else(|| {
        CliError::Control("Could not resolve the newly split AgentMux pane.".to_string())
    })?;

    let mut session_id = None;
    let command = tmux_command_or_default_shell(command, &context);
    if !command.is_empty() {
        let spawn = invoke_control(
            "session.spawn",
            &SessionSpawnParams {
                workspace_id: workspace_id.clone(),
                backend: context.backend_kind.clone(),
                backend_profile: None,
                command,
                cwd: context.cwd.clone(),
                env: Vec::new(),
                columns: 120,
                rows: 30,
                durability: Some("ephemeral".to_string()),
                placement: Some("active_pane".to_string()),
                pane_id: Some(new_pane_id.clone()),
            },
            &invoke,
        )?;
        let result: SessionSpawnResult = response_result(&spawn)?;
        record_tmux_agent_team_metadata(
            &invoke,
            &workspace_id,
            &result.session_id,
            Some(&pane_id),
            "split-window",
            Some(&new_pane_id),
        );
        if invoke.json {
            return write_json_response(&spawn, output);
        }
        session_id = Some(result.session_id);
    }

    let context = SystemIdentifyResult {
        in_agentmux: true,
        workspace_id: Some(workspace_id),
        pane_id: Some(new_pane_id.clone()),
        surface_id: detail
            .panes
            .iter()
            .find(|pane| pane.pane_id == new_pane_id)
            .and_then(|pane| pane.mounted_surface_id.clone()),
        session_id,
        cwd: context.cwd,
        backend_kind: context.backend_kind,
        control_pipe: invoke.pipe_name.clone(),
    };
    writeln!(
        output,
        "{}",
        render_tmux_format(format.as_deref(), &context, &new_pane_id)
    )?;
    Ok(())
}

fn tmux_command_or_default_shell(
    command: Vec<String>,
    context: &SystemIdentifyResult,
) -> Vec<String> {
    if command.is_empty() {
        tmux_default_shell_command(context)
    } else {
        command
    }
}

fn tmux_default_shell_command(context: &SystemIdentifyResult) -> Vec<String> {
    let backend_kind = context
        .backend_kind
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if backend_kind.contains("wsl") {
        return vec!["bash".to_string()];
    }
    if backend_kind == "conpty" {
        return windows_default_shell_command();
    }
    if std::env::var_os("WSL_DISTRO_NAME").is_some() {
        return vec!["bash".to_string()];
    }
    if cfg!(windows) {
        windows_default_shell_command()
    } else {
        posix_default_shell_command()
    }
}

fn windows_default_shell_command() -> Vec<String> {
    std::env::var("COMSPEC")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| vec![value])
        .unwrap_or_else(|| vec!["cmd.exe".to_string()])
}

fn posix_default_shell_command() -> Vec<String> {
    std::env::var("SHELL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| vec![value])
        .unwrap_or_else(|| vec!["sh".to_string()])
}

fn record_tmux_agent_team_metadata(
    invoke: &ControlInvokeOptions,
    workspace_id: &str,
    session_id: &str,
    parent_pane_id: Option<&str>,
    action: &str,
    pane_id: Option<&str>,
) {
    let Some(integration) = agent_integration_from_env() else {
        return;
    };
    let metadata = build_tmux_agent_team_metadata(
        &integration,
        workspace_id,
        session_id,
        parent_pane_id,
        action,
        pane_id,
    );
    let _ = invoke_control("agent.set_state", &metadata.agent_state, invoke);
    let _ = invoke_control("sidebar.set_status", &metadata.sidebar_status, invoke);
    let _ = invoke_control("sidebar.log", &metadata.sidebar_log, invoke);
}

fn agent_integration_from_env() -> Option<String> {
    std::env::var("AGENTMUX_AGENT_INTEGRATION")
        .ok()
        .or_else(|| std::env::var("CMUX_AGENT_INTEGRATION").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn build_tmux_agent_team_metadata(
    integration: &str,
    workspace_id: &str,
    session_id: &str,
    parent_pane_id: Option<&str>,
    action: &str,
    pane_id: Option<&str>,
) -> TmuxAgentTeamMetadata {
    let integration = integration.trim().to_string();
    let pane_label = pane_id
        .map(agentmux_pane_to_tmux_pane)
        .unwrap_or_else(|| "unknown-pane".to_string());
    let parent_label = parent_pane_id.map(agentmux_pane_to_tmux_pane);
    let reason = match parent_label {
        Some(parent) => format!("{integration} {action} worker {pane_label} from {parent}"),
        None => format!("{integration} {action} worker {pane_label}"),
    };
    let sidebar_label = format!("{integration} team active");
    let sidebar_message = format!("{reason}; session {session_id}");
    TmuxAgentTeamMetadata {
        integration: integration.clone(),
        agent_state: AgentSetStateParams {
            session_id: session_id.to_string(),
            state: "running".to_string(),
            reason: Some(reason),
            telemetry: Some(AgentTelemetry {
                activity: Some("agent_team".to_string()),
                session: Some(format!("{integration}:{action}")),
                cost: None,
                tokens: None,
                cache: None,
                rate: None,
                ctx: pane_id.map(str::to_string),
            }),
        },
        sidebar_status: SidebarStatusSetParams {
            workspace_id: Some(workspace_id.to_string()),
            key: format!("agent-team.{}", metadata_key_fragment(&integration)),
            label: sidebar_label,
            icon: Some("bot".to_string()),
            color: Some("#2f80ed".to_string()),
            priority: Some(70),
        },
        sidebar_log: SidebarLogAddParams {
            workspace_id: Some(workspace_id.to_string()),
            level: Some("info".to_string()),
            source: Some(integration.clone()),
            message: sidebar_message,
        },
    }
}

fn metadata_key_fragment(value: &str) -> String {
    let key = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    if key.is_empty() {
        "unknown".to_string()
    } else {
        key
    }
}

fn run_events_poll<W>(options: EventPollOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("events.poll", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: EventPollResult = response_result(&response)?;
    if result.events.is_empty() {
        writeln!(output, "No events.")?;
        return Ok(());
    }
    write_event_frames(&result.events, output)
}

fn run_events_watch<W>(options: EventWatchOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let mut params = options.params;
    let mut printed = 0usize;
    loop {
        let (response, mut stream) = subscribe_control_events(&params, &options.invoke)?;
        let result: EventSubscribeResult = response_result(&response)?;
        params.after_event_id = Some(result.cursor.clone());

        if options.invoke.json {
            write_json_response(&response, output)?;
        }

        while let Some(event) = stream.read_event().map_err(CliError::Io)? {
            params.after_event_id = Some(event.event_id.clone());
            printed += 1;
            if options.invoke.json {
                write_event_frame_json(&event, output)?;
            } else {
                write_event_frames(std::slice::from_ref(&event), output)?;
            }

            if options.once || options.limit.is_some_and(|limit| printed >= limit) {
                return Ok(());
            }
        }

        if options.once {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(options.interval_ms));
    }
}

fn run_diagnostics<W>(options: ControlInvokeOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("diagnostics.recovery", &serde_json::json!({}), &options)?;
    if options.json {
        return write_json_response(&response, output);
    }
    let diagnostics: RecoveryDiagnosticsResult = response_result(&response)?;
    writeln!(output, "workspaces: {}", diagnostics.workspace_count)?;
    writeln!(output, "panes: {}", diagnostics.pane_count)?;
    writeln!(output, "surfaces: {}", diagnostics.surface_count)?;
    writeln!(output, "sessions: {}", diagnostics.session_count)?;
    for session in diagnostics.sessions {
        writeln!(
            output,
            "{}\t{}\t{}\t{}",
            session.session_id, session.state, session.backend_kind, session.durability
        )?;
    }
    Ok(())
}

fn run_diagnostics_export<W>(options: ControlInvokeOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("diagnostics.export", &serde_json::json!({}), &options)?;
    if options.json {
        return write_json_response(&response, output);
    }
    let diagnostics: DiagnosticsExportResult = response_result(&response)?;
    writeln!(
        output,
        "diagnostics: {}\t{}",
        diagnostics.format_version, diagnostics.generated_at
    )?;
    writeln!(
        output,
        "resources: workspaces={}\tpanes={}\tsurfaces={}\tsessions={}",
        diagnostics.recovery.workspace_count,
        diagnostics.recovery.pane_count,
        diagnostics.recovery.surface_count,
        diagnostics.recovery.session_count
    )?;
    writeln!(output, "backend health:")?;
    for backend in &diagnostics.backend_health {
        writeln!(
            output,
            "{}\t{}\tactive={}\trecovering={}\tfailed={}",
            backend.backend_kind,
            backend.health,
            backend.active_sessions,
            backend.recovering_sessions,
            backend.failed_sessions
        )?;
    }
    writeln!(output, "queue pressure:")?;
    for queue in &diagnostics.queue_pressure {
        writeln!(
            output,
            "{}\t{}\tdepth={}/{}\tdropped={}",
            queue.queue, queue.state, queue.depth, queue.capacity, queue.dropped_count
        )?;
    }
    writeln!(
        output,
        "output stream: sessions={}\tsubscriptions={}\tframes={}\tbytes={}\tfailures={}\tclosed={}\tpumps={}/{}+{}\tlast_frame={}",
        diagnostics.output_stream.active_sessions,
        diagnostics.output_stream.active_subscriptions,
        diagnostics.output_stream.frames_sent,
        diagnostics.output_stream.bytes_sent,
        diagnostics.output_stream.send_failures,
        diagnostics.output_stream.closed_channels,
        diagnostics.output_stream.pump_runs,
        diagnostics.output_stream.pump_active_runs,
        diagnostics.output_stream.pump_idle_runs,
        diagnostics
            .output_stream
            .last_frame_at
            .as_deref()
            .unwrap_or("never")
    )?;
    writeln!(
        output,
        "browser failures: {}",
        diagnostics.browser.failures.len()
    )?;
    writeln!(output, "notifications: {}", diagnostics.notifications.len())?;
    Ok(())
}

fn run_config_get<W>(options: ConfigGetOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("config.get", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let config: AppConfigResult = response_result(&response)?;
    write_app_config(&config, output)
}

fn run_config_reload<W>(options: ConfigGetOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("config.reload", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let config: AppConfigResult = response_result(&response)?;
    write_app_config(&config, output)
}

fn run_config_migrate_project<W>(
    options: ConfigMigrateProjectOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("config.migrate_project", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: AppConfigMigrateProjectResult = response_result(&response)?;
    writeln!(output, "source\t{}", result.source_path)?;
    writeln!(output, "target\t{}", result.target_path)?;
    writeln!(output, "overwritten\t{}", result.overwritten)?;
    write_app_config(&result.config, output)
}

fn run_config_diagnostics<W>(
    options: ConfigDiagnosticsOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("config.diagnostics", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: AppConfigDiagnosticsResult = response_result(&response)?;
    for entry in result.entries {
        let status = if entry.valid { "ok" } else { "invalid" };
        writeln!(
            output,
            "{}\t{}\tactive={}\texists={}\t{}\t{}",
            entry.source,
            status,
            entry.active,
            entry.exists,
            entry.path.as_deref().unwrap_or("-"),
            entry.message
        )?;
    }
    Ok(())
}

fn run_config_schema<W>(options: ConfigSchemaOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    if let Some(path) = options.output_path {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(&path, AGENTMUX_CONFIG_SCHEMA_JSON)?;
        if options.json {
            let value = serde_json::json!({
                "path": path.display().to_string(),
            });
            let json = serde_json::to_string_pretty(&value).map_err(|error| {
                CliError::Control(format!("failed to render config schema result: {error}"))
            })?;
            writeln!(output, "{json}")?;
        } else {
            writeln!(output, "schema\t{}", path.display())?;
        }
        return Ok(());
    }

    output.write_all(AGENTMUX_CONFIG_SCHEMA_JSON.as_bytes())?;
    if !AGENTMUX_CONFIG_SCHEMA_JSON.ends_with('\n') {
        writeln!(output)?;
    }
    Ok(())
}

fn run_actions_list<W>(options: ActionListOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("actions.list", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: ActionListResult = response_result(&response)?;
    for action in result.actions {
        writeln!(
            output,
            "{}\t{}\t{}\t{}",
            action.id, action.group, action.source, action.title
        )?;
    }
    Ok(())
}

fn run_actions_run<W>(options: ActionRunOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("actions.run", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: ActionRunResult = response_result(&response)?;
    writeln!(output, "action\t{}", result.action_id)?;
    writeln!(output, "type\t{}", result.result_type)?;
    if let Some(workspace_id) = result.workspace_id {
        writeln!(output, "workspace\t{workspace_id}")?;
    }
    if let Some(session_id) = result.session_id {
        writeln!(output, "session\t{session_id}")?;
    }
    if let Some(surface_id) = result.surface_id {
        writeln!(output, "surface\t{surface_id}")?;
    }
    if let Some(pane_id) = result.pane_id {
        writeln!(output, "pane\t{pane_id}")?;
    }
    if let Some(message) = result.message {
        writeln!(output, "message\t{message}")?;
    }
    Ok(())
}

fn run_browser_command<W>(command: &str, args: &[String], output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    match command {
        "open" | "new" | "new-tab" => run_browser_open(parse_browser_open_options(args)?, output),
        "navigate" | "nav" => run_browser_navigate(parse_browser_navigate_options(args)?, output),
        "reload" | "refresh" => run_browser_surface_navigation(
            "browser.reload",
            parse_browser_surface_command_options(args, "reload")?,
            output,
        ),
        "back" | "go-back" | "go_back" => run_browser_surface_navigation(
            "browser.back",
            parse_browser_surface_command_options(args, "back")?,
            output,
        ),
        "forward" | "go-forward" | "go_forward" => run_browser_surface_navigation(
            "browser.forward",
            parse_browser_surface_command_options(args, "forward")?,
            output,
        ),
        "current-url" | "current_url" | "url" => run_browser_surface_navigation(
            "browser.current_url",
            parse_browser_surface_command_options(args, "current-url")?,
            output,
        ),
        "screenshot" | "capture" => {
            run_browser_screenshot(parse_browser_screenshot_options(args)?, output)
        }
        "dom-snapshot" | "dom_snapshot" | "dom" => {
            run_browser_dom_snapshot(parse_browser_dom_snapshot_options(args)?, output)
        }
        "frames" | "frame-tree" | "frame_tree" => run_browser_frames(
            parse_browser_surface_command_options(args, "frames")?,
            output,
        ),
        "storage" | "storage-snapshot" | "storage_snapshot" => run_browser_storage(
            parse_browser_surface_command_options(args, "storage")?,
            output,
        ),
        "cookies" | "cookie" => run_browser_cookies(
            parse_browser_surface_command_options(args, "cookies")?,
            output,
        ),
        "downloads" | "download" => {
            run_browser_downloads(parse_browser_downloads_options(args)?, output)
        }
        "history" => run_browser_history(
            parse_browser_surface_command_options(args, "history")?,
            output,
        ),
        "console" | "logs" => run_browser_console(parse_browser_console_options(args)?, output),
        "dialogs" | "dialog" => run_browser_dialogs(parse_browser_dialogs_options(args)?, output),
        "errors" | "error-events" | "error_events" => {
            run_browser_errors(parse_browser_errors_options(args)?, output)
        }
        "click" => run_browser_click(parse_browser_click_options(args)?, output),
        "type" => run_browser_type(parse_browser_type_options(args)?, output),
        "fill" => run_browser_fill(parse_browser_fill_options(args)?, output),
        "press" => run_browser_press(parse_browser_press_options(args)?, output),
        "select" => run_browser_select(parse_browser_select_options(args)?, output),
        "scroll" => run_browser_scroll(parse_browser_scroll_options(args)?, output),
        "hover" => run_browser_hover(parse_browser_hover_options(args)?, output),
        "check" => run_browser_check(parse_browser_check_options(args)?, output),
        "uncheck" => run_browser_uncheck(parse_browser_check_options(args)?, output),
        "get" => run_browser_get(parse_browser_get_options(args)?, output),
        "find" => run_browser_find(parse_browser_find_options(args)?, output),
        "highlight" => run_browser_highlight(parse_browser_highlight_options(args)?, output),
        "focus" => run_browser_focus(parse_browser_focus_options(args)?, output),
        "zoom" => run_browser_zoom(parse_browser_zoom_options(args)?, output),
        "wait" | "wait-for-selector" | "wait_for_selector" => {
            run_browser_wait_for_selector(parse_browser_wait_for_selector_options(args)?, output)
        }
        "evaluate" | "eval" => run_browser_evaluate(parse_browser_evaluate_options(args)?, output),
        "diagnostics" | "diagnose" => {
            run_browser_diagnostics(parse_browser_diagnostics_options(args)?, output)
        }
        other => Err(CliError::InvalidArgs(format!(
            "unknown browser command '{other}'. Use open, navigate, reload, back, forward, current-url, screenshot, dom-snapshot, frames, storage, cookies, downloads, history, console, dialogs, errors, click, type, fill, press, select, scroll, hover, check, get, find, highlight, focus, zoom, wait-for-selector, evaluate, or diagnostics."
        ))),
    }
}

fn run_browser_open<W>(options: BrowserOpenOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&options.invoke, options.workspace_id)?;
    let workspace_id = require_context_field(context.workspace_id.clone(), "workspace")?;
    let pane_id = if options.placement.as_deref() == Some("active_pane") {
        options.pane_id.or(context.pane_id)
    } else {
        options.pane_id
    };
    let params = SurfaceCreateBrowserParams {
        workspace_id,
        pane_id,
        profile: options.profile,
        placement: options.placement,
    };
    let response = invoke_control("surface.create_browser", &params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: SurfaceSummaryResult = response_result(&response)?;
    writeln!(
        output,
        "{}\t{}\t{}\t{}",
        result.surface_id,
        result.surface_type,
        result.title,
        result.browser_id.as_deref().unwrap_or("-")
    )?;
    Ok(())
}

fn run_browser_navigate<W>(options: BrowserNavigateOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.navigate", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserNavigationResult = response_result(&response)?;
    writeln!(output, "{}\t{}", result.surface_id, result.url)?;
    Ok(())
}

fn run_browser_surface_navigation<W>(
    method: &'static str,
    options: BrowserSurfaceCommandOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control(method, &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserNavigationResult = response_result(&response)?;
    writeln!(output, "{}\t{}", result.surface_id, result.url)?;
    Ok(())
}

fn run_browser_screenshot<W>(
    options: BrowserScreenshotOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.screenshot", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserScreenshotResult = response_result(&response)?;
    writeln!(
        output,
        "{}\t{}\t{}\t{}",
        result.surface_id, result.format, result.image_handle, result.byte_count
    )?;
    Ok(())
}

fn run_browser_dom_snapshot<W>(
    options: BrowserDomSnapshotOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.dom_snapshot", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserDomSnapshotResult = response_result(&response)?;
    output.write_all(result.html.as_bytes())?;
    if !result.html.ends_with('\n') {
        writeln!(output)?;
    }
    Ok(())
}

fn run_browser_frames<W>(
    options: BrowserSurfaceCommandOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.frames", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserFramesResult = response_result(&response)?;
    writeln!(output, "{}\t{}", result.surface_id, result.frames.len())?;
    for frame in result.frames {
        writeln!(
            output,
            "frame\t{}\t{}\t{}\t{}",
            frame.frame_id,
            frame.parent_frame_id.as_deref().unwrap_or("-"),
            frame.name.as_deref().unwrap_or("-"),
            frame.url
        )?;
    }
    Ok(())
}

fn run_browser_storage<W>(
    options: BrowserSurfaceCommandOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.storage", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserStorageResult = response_result(&response)?;
    writeln!(
        output,
        "{}\tlocal\t{}",
        result.surface_id,
        result.local_storage.len()
    )?;
    for entry in result.local_storage {
        writeln!(output, "local\t{}\t{}", entry.key, entry.value)?;
    }
    writeln!(
        output,
        "{}\tsession\t{}",
        result.surface_id,
        result.session_storage.len()
    )?;
    for entry in result.session_storage {
        writeln!(output, "session\t{}\t{}", entry.key, entry.value)?;
    }
    Ok(())
}

fn run_browser_cookies<W>(
    options: BrowserSurfaceCommandOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.cookies", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserCookiesResult = response_result(&response)?;
    writeln!(output, "{}\t{}", result.surface_id, result.cookies.len())?;
    for cookie in result.cookies {
        writeln!(
            output,
            "cookie\t{}\t{}\t{}\t{}\t{}\t{}",
            cookie.name,
            cookie.domain,
            cookie.path,
            cookie.secure,
            cookie.http_only,
            cookie.same_site.as_deref().unwrap_or("-")
        )?;
    }
    Ok(())
}

fn run_browser_downloads<W>(
    options: BrowserDownloadsOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.downloads", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserDownloadsResult = response_result(&response)?;
    writeln!(
        output,
        "{}\t{}\t{}",
        result.surface_id,
        result.directory,
        result.downloads.len()
    )?;
    for download in result.downloads {
        writeln!(
            output,
            "download\t{}\t{}\t{}\t{}\t{}",
            download.complete,
            download.byte_count,
            download.modified_at.as_deref().unwrap_or("-"),
            download.file_name,
            download.path
        )?;
    }
    Ok(())
}

fn run_browser_console<W>(options: BrowserConsoleOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.console", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserConsoleResult = response_result(&response)?;
    writeln!(output, "{}\t{}", result.surface_id, result.messages.len())?;
    for message in result.messages {
        writeln!(
            output,
            "console\t{}\t{}\t{}",
            message.level, message.timestamp, message.text
        )?;
    }
    Ok(())
}

fn run_browser_history<W>(
    options: BrowserSurfaceCommandOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.history", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserHistoryResult = response_result(&response)?;
    writeln!(
        output,
        "{}\t{}\t{}",
        result.surface_id,
        result.current_index,
        result.entries.len()
    )?;
    for entry in result.entries {
        writeln!(
            output,
            "history\t{}\t{}\t{}",
            entry.id, entry.title, entry.url
        )?;
    }
    Ok(())
}

fn run_browser_dialogs<W>(options: BrowserDialogsOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.dialogs", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserDialogsResult = response_result(&response)?;
    writeln!(output, "{}\t{}", result.surface_id, result.messages.len())?;
    for message in result.messages {
        writeln!(
            output,
            "dialog\t{}\t{}\t{}\t{}\t{}",
            message.dialog_type,
            message.timestamp,
            message.default_value.as_deref().unwrap_or("-"),
            message.response.as_deref().unwrap_or("-"),
            message.message
        )?;
    }
    Ok(())
}

fn run_browser_errors<W>(options: BrowserErrorsOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.errors", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserErrorsResult = response_result(&response)?;
    writeln!(output, "{}\t{}", result.surface_id, result.events.len())?;
    for event in result.events {
        writeln!(
            output,
            "error\t{}\t{}\t{}:{}\t{}",
            event.kind, event.timestamp, event.source, event.line, event.message
        )?;
    }
    Ok(())
}

fn run_browser_click<W>(options: BrowserClickOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.click", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserActionResult = response_result(&response)?;
    writeln!(output, "{}\t{}", result.surface_id, result.ok)?;
    Ok(())
}

fn run_browser_type<W>(options: BrowserTypeOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.type", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserActionResult = response_result(&response)?;
    writeln!(output, "{}\t{}", result.surface_id, result.ok)?;
    Ok(())
}

fn run_browser_fill<W>(options: BrowserFillOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    run_browser_action_method("browser.fill", &options.params, &options.invoke, output)
}

fn run_browser_press<W>(options: BrowserPressOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    run_browser_action_method("browser.press", &options.params, &options.invoke, output)
}

fn run_browser_select<W>(options: BrowserSelectOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    run_browser_action_method("browser.select", &options.params, &options.invoke, output)
}

fn run_browser_scroll<W>(options: BrowserScrollOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    run_browser_action_method("browser.scroll", &options.params, &options.invoke, output)
}

fn run_browser_hover<W>(options: BrowserHoverOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    run_browser_action_method("browser.hover", &options.params, &options.invoke, output)
}

fn run_browser_check<W>(options: BrowserCheckOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    run_browser_action_method("browser.check", &options.params, &options.invoke, output)
}

fn run_browser_uncheck<W>(mut options: BrowserCheckOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    options.params.checked = Some(false);
    run_browser_action_method("browser.check", &options.params, &options.invoke, output)
}

fn run_browser_get<W>(options: BrowserGetOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.get", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserGetResult = response_result(&response)?;
    writeln!(output, "{}", result.value)?;
    Ok(())
}

fn run_browser_find<W>(options: BrowserFindOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.find", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserFindResult = response_result(&response)?;
    writeln!(
        output,
        "{}\t{}\t{}",
        result.surface_id, result.query, result.count
    )?;
    for item in result.matches {
        writeln!(output, "match\t{item}")?;
    }
    Ok(())
}

fn run_browser_highlight<W>(
    options: BrowserHighlightOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    run_browser_action_method(
        "browser.highlight",
        &options.params,
        &options.invoke,
        output,
    )
}

fn run_browser_focus<W>(options: BrowserFocusOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.focus", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserActionResult = response_result(&response)?;
    writeln!(output, "{}\t{}", result.surface_id, result.ok)?;
    Ok(())
}

fn run_browser_zoom<W>(options: BrowserZoomOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.zoom", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserActionResult = response_result(&response)?;
    writeln!(output, "{}\t{}", result.surface_id, result.ok)?;
    Ok(())
}

fn run_browser_action_method<W, T>(
    method: &'static str,
    params: &T,
    invoke: &ControlInvokeOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
    T: serde::Serialize,
{
    let response = invoke_control(method, params, invoke)?;
    if invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserActionResult = response_result(&response)?;
    writeln!(output, "{}\t{}", result.surface_id, result.ok)?;
    Ok(())
}

fn run_browser_wait_for_selector<W>(
    options: BrowserWaitForSelectorOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control(
        "browser.wait_for_selector",
        &options.params,
        &options.invoke,
    )?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserWaitForSelectorResult = response_result(&response)?;
    writeln!(
        output,
        "{}\t{}\t{}",
        result.surface_id, result.selector, result.elapsed_ms
    )?;
    Ok(())
}

fn run_browser_evaluate<W>(options: BrowserEvaluateOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("browser.evaluate", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserEvaluateResult = response_result(&response)?;
    writeln!(output, "{}", result.value_json)?;
    Ok(())
}

fn run_browser_diagnostics<W>(
    options: BrowserDiagnosticsOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    let response = invoke_control("diagnostics.browser", &options.params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: BrowserDiagnosticsResult = response_result(&response)?;
    if result.failures.is_empty() {
        writeln!(output, "No browser failures.")?;
        return Ok(());
    }
    for failure in result.failures {
        writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}",
            failure.occurred_at,
            failure.surface_id.as_deref().unwrap_or("-"),
            failure.operation,
            failure.code,
            failure.message
        )?;
    }
    Ok(())
}

fn run_ssh<W>(options: SshOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let context = identify_context(&options.invoke, options.workspace_id)?;
    let workspace_id = require_context_field(context.workspace_id.clone(), "workspace")?;
    let pane_id = if options.placement.as_deref() == Some("active_pane") {
        options.pane_id.or(context.pane_id)
    } else {
        options.pane_id
    };
    let target = resolve_ssh_target(&options.invoke, &options.target)?;
    let params = SessionSpawnParams {
        workspace_id,
        backend: Some("ssh".to_string()),
        backend_profile: Some(target),
        command: Vec::new(),
        cwd: None,
        env: Vec::new(),
        columns: options.columns,
        rows: options.rows,
        durability: Some("ephemeral".to_string()),
        placement: options.placement,
        pane_id,
    };
    let response = invoke_control("session.spawn", &params, &options.invoke)?;
    if options.invoke.json {
        return write_json_response(&response, output);
    }
    let result: SessionSpawnResult = response_result(&response)?;
    writeln!(output, "{}", result.session_id)?;
    Ok(())
}

fn resolve_ssh_target(invoke: &ControlInvokeOptions, target: &str) -> Result<String, CliError> {
    let trimmed = target.trim();
    if trimmed.contains('@') {
        return Ok(trimmed.to_string());
    }

    let response = invoke_control("profile.list", &serde_json::json!({}), invoke)?;
    let result: ProfileListResult = response_result(&response)?;
    let exact = result
        .profiles
        .iter()
        .find(|profile| profile.profile_id == trimmed || profile.name == trimmed);
    if let Some(profile) = exact {
        return Ok(profile_to_ssh_target(profile));
    }

    let lower = trimmed.to_ascii_lowercase();
    let mut insensitive = result
        .profiles
        .iter()
        .filter(|profile| {
            profile.profile_id.to_ascii_lowercase() == lower
                || profile.name.to_ascii_lowercase() == lower
        })
        .collect::<Vec<_>>();
    if insensitive.len() == 1 {
        return Ok(profile_to_ssh_target(insensitive.remove(0)));
    }

    Err(CliError::Control(format!(
        "SSH target '{trimmed}' is not a saved profile name/id. Use user@host[:port] or create a matching SSH profile."
    )))
}

fn profile_to_ssh_target(profile: &ProfileSummaryResult) -> String {
    match profile.port {
        Some(port) => format!("{}@{}:{}", profile.user, profile.host, port),
        None => format!("{}@{}", profile.user, profile.host),
    }
}

fn identify_context(
    invoke: &ControlInvokeOptions,
    workspace_id: Option<String>,
) -> Result<SystemIdentifyResult, CliError> {
    let response = invoke_control(
        "system.identify",
        &SystemIdentifyParams { workspace_id },
        invoke,
    )?;
    response_result(&response)
}

fn require_context_field(value: Option<String>, label: &str) -> Result<String, CliError> {
    value.ok_or_else(|| {
        CliError::Control(format!(
            "No active {label} is available in the current AgentMux context."
        ))
    })
}

impl AgentIntegrationKind {
    fn all() -> [Self; 4] {
        [Self::ClaudeTeams, Self::Omo, Self::Omx, Self::Omc]
    }

    fn parse(value: &str) -> Result<Self, CliError> {
        Self::from_command(value).ok_or_else(|| {
            CliError::InvalidArgs(format!(
                "unknown integration '{value}'; expected claude-teams, omo, omx, or omc."
            ))
        })
    }

    fn from_command(value: &str) -> Option<Self> {
        match value {
            "claude-teams" => Some(Self::ClaudeTeams),
            "omo" => Some(Self::Omo),
            "omx" => Some(Self::Omx),
            "omc" => Some(Self::Omc),
            _ => None,
        }
    }

    fn command_name(self) -> &'static str {
        match self {
            Self::ClaudeTeams => "claude-teams",
            Self::Omo => "omo",
            Self::Omx => "omx",
            Self::Omc => "omc",
        }
    }

    fn install_hint(self) -> &'static str {
        match self {
            Self::ClaudeTeams => {
                "Install Claude Code in the AgentMux WSL shell, then launch with claude-teams."
            }
            Self::Omo => {
                "Install OpenCode and oh-my-opencode in the AgentMux WSL shell, then launch with omo."
            }
            Self::Omx => {
                "Install oh-my-codex and make omx available in the AgentMux WSL shell."
            }
            Self::Omc => {
                "Install Claude Code and oh-my-claudecode, then launch with omc."
            }
        }
    }

    fn shim_dir_name(self) -> &'static str {
        match self {
            Self::ClaudeTeams => "claude-teams-bin",
            Self::Omo => "omo-bin",
            Self::Omx => "omx-bin",
            Self::Omc => "omc-bin",
        }
    }

    fn executable(self) -> &'static str {
        match self {
            Self::ClaudeTeams => "claude",
            Self::Omo => "opencode",
            Self::Omx => "omx",
            Self::Omc => "omc",
        }
    }
}

fn prepare_agent_integration_runtime(
    kind: AgentIntegrationKind,
    invoke: &ControlInvokeOptions,
    workspace_id: Option<String>,
    base_dir: Option<&str>,
    args: Vec<String>,
    install_packages: bool,
) -> Result<AgentIntegrationRuntime, CliError> {
    let base_dir = resolve_cmuxterm_base_dir(base_dir)?;
    let mut runtime = setup_agent_integration_files(kind, &base_dir, args)?;
    if install_packages && kind == AgentIntegrationKind::Omo {
        let distribution = wsl_distribution_from_env();
        runtime.package_install = Some(ensure_omo_package_installed_for_distribution(
            &base_dir,
            distribution.as_deref(),
        )?);
    }
    let context = identify_context(invoke, workspace_id)?;
    let workspace_id = require_context_field(context.workspace_id.clone(), "workspace")?;
    let pane_id = pane_from_env()
        .or(context.pane_id.clone())
        .ok_or_else(|| CliError::Control("No active AgentMux pane is available.".to_string()))?;
    let token = resolve_control_token(invoke)?;
    let tmux =
        std::env::var("TMUX").unwrap_or_else(|_| format!("agentmux,{workspace_id},{pane_id}"));
    let tmux_pane =
        std::env::var("TMUX_PANE").unwrap_or_else(|_| agentmux_pane_to_tmux_pane(&pane_id));
    let path = prepend_path(&runtime.shim_dir, std::env::var_os("PATH"));

    runtime.env.extend([
        (
            "AGENTMUX_CONTROL_PIPE".to_string(),
            invoke.pipe_name.clone(),
        ),
        ("AGENTMUX_CONTROL_TOKEN".to_string(), token),
        ("AGENTMUX_WORKSPACE_ID".to_string(), workspace_id.clone()),
        ("AGENTMUX_PANE_ID".to_string(), pane_id.clone()),
        ("CMUX_SOCKET_PATH".to_string(), invoke.pipe_name.clone()),
        ("CMUX_WORKSPACE_ID".to_string(), workspace_id),
        ("CMUX_PANE_ID".to_string(), pane_id),
        (
            "AGENTMUX_AGENT_INTEGRATION".to_string(),
            kind.command_name().to_string(),
        ),
        (
            "CMUX_AGENT_INTEGRATION".to_string(),
            kind.command_name().to_string(),
        ),
        ("TMUX".to_string(), tmux),
        ("TMUX_PANE".to_string(), tmux_pane),
        ("PATH".to_string(), path),
    ]);

    match kind {
        AgentIntegrationKind::ClaudeTeams => {
            runtime.env.push((
                "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS".to_string(),
                "1".to_string(),
            ));
        }
        AgentIntegrationKind::Omo => {
            if let Some(shadow_config_dir) = runtime.shadow_config_dir.as_ref() {
                runtime.env.push((
                    "OPENCODE_CONFIG_DIR".to_string(),
                    path_to_env_value(shadow_config_dir),
                ));
            }
        }
        AgentIntegrationKind::Omc => {
            if let Some(restore_module) = runtime.node_options_restore_module.as_ref() {
                let original_node_options = std::env::var("NODE_OPTIONS").ok();
                runtime.env.push((
                    "AGENTMUX_ORIGINAL_NODE_OPTIONS".to_string(),
                    original_node_options.clone().unwrap_or_default(),
                ));
                runtime.env.push((
                    "CMUX_ORIGINAL_NODE_OPTIONS".to_string(),
                    original_node_options.clone().unwrap_or_default(),
                ));
                runtime.env.push((
                    "NODE_OPTIONS".to_string(),
                    node_options_with_required_module(
                        original_node_options.as_deref(),
                        restore_module,
                    ),
                ));
            }
        }
        AgentIntegrationKind::Omx => {}
    }

    Ok(runtime)
}

fn setup_agent_integration_files(
    kind: AgentIntegrationKind,
    base_dir: &Path,
    args: Vec<String>,
) -> Result<AgentIntegrationRuntime, CliError> {
    fs::create_dir_all(base_dir).map_err(|error| {
        CliError::Io(io::Error::new(
            error.kind(),
            format!(
                "failed to create cmux compatibility directory '{}': {error}",
                base_dir.display()
            ),
        ))
    })?;
    let shim_dir = base_dir.join(kind.shim_dir_name());
    fs::create_dir_all(&shim_dir).map_err(|error| {
        CliError::Io(io::Error::new(
            error.kind(),
            format!(
                "failed to create tmux shim directory '{}': {error}",
                shim_dir.display()
            ),
        ))
    })?;
    write_tmux_shim_files(&shim_dir)?;
    let shadow_config_dir = match kind {
        AgentIntegrationKind::Omo => Some(setup_omo_shadow_config(base_dir)?),
        AgentIntegrationKind::ClaudeTeams
        | AgentIntegrationKind::Omx
        | AgentIntegrationKind::Omc => None,
    };
    let node_options_restore_module = if kind == AgentIntegrationKind::Omc {
        Some(setup_omc_restore_module(base_dir)?)
    } else {
        None
    };

    Ok(AgentIntegrationRuntime {
        kind,
        base_dir: base_dir.to_path_buf(),
        shim_dir,
        command: kind.executable().to_string(),
        args,
        env: Vec::new(),
        shadow_config_dir,
        node_options_restore_module,
        package_install: None,
    })
}

fn write_tmux_shim_files(shim_dir: &Path) -> Result<(), CliError> {
    let script_path = shim_dir.join("tmux");
    let script = "#!/usr/bin/env sh\nif [ -n \"$CMUX_EXE\" ]; then\n  exec \"$CMUX_EXE\" __tmux-compat \"$@\"\nfi\nif command -v cmux >/dev/null 2>&1; then\n  exec cmux __tmux-compat \"$@\"\nfi\nexec cmux.exe __tmux-compat \"$@\"\n";
    fs::write(&script_path, script).map_err(CliError::Io)?;
    set_executable_if_supported(&script_path)?;

    let cmd_path = shim_dir.join("tmux.cmd");
    fs::write(
        &cmd_path,
        "@echo off\r\nif not \"%CMUX_EXE%\"==\"\" (\r\n  \"%CMUX_EXE%\" __tmux-compat %*\r\n  exit /b %ERRORLEVEL%\r\n)\r\ncmux.exe __tmux-compat %*\r\n",
    )
    .map_err(CliError::Io)?;
    Ok(())
}

fn setup_omo_shadow_config(base_dir: &Path) -> Result<PathBuf, CliError> {
    let shadow_dir = base_dir.join("omo-config");
    fs::create_dir_all(&shadow_dir).map_err(CliError::Io)?;
    let source_dir = resolve_opencode_config_dir()?;

    write_omo_opencode_shadow_config(
        &source_dir.join("opencode.json"),
        &shadow_dir.join("opencode.json"),
    )?;
    write_omo_plugin_shadow_config(
        &source_dir.join("oh-my-opencode.json"),
        &shadow_dir.join("oh-my-opencode.json"),
    )?;

    for file_name in ["package.json", "bun.lock", "bun.lockb"] {
        let source = source_dir.join(file_name);
        if source.is_file() {
            let _ = fs::copy(&source, shadow_dir.join(file_name));
        }
    }
    try_link_dir(
        &source_dir.join("node_modules"),
        &shadow_dir.join("node_modules"),
    );
    Ok(shadow_dir)
}

fn write_omo_opencode_shadow_config(source: &Path, destination: &Path) -> Result<(), CliError> {
    if let Ok(text) = fs::read_to_string(source) {
        if let Some(merged) = merge_opencode_plugin_config_text(&text, "oh-my-opencode") {
            return write_text_file_with_newline(destination, &merged);
        }
    }

    let mut opencode_config = read_json_file(source).unwrap_or_else(|| serde_json::json!({}));
    ensure_json_string_array_contains(&mut opencode_config, "plugin", "oh-my-opencode");
    write_pretty_json(destination, &opencode_config)
}

fn write_omo_plugin_shadow_config(source: &Path, destination: &Path) -> Result<(), CliError> {
    if let Ok(text) = fs::read_to_string(source) {
        if let Some(merged) = merge_omo_tmux_config_text(&text) {
            return write_text_file_with_newline(destination, &merged);
        }
    }

    let mut omo_config = read_json_file(source).unwrap_or_else(|| serde_json::json!({}));
    ensure_nested_bool(&mut omo_config, &["tmux", "enabled"], true);
    write_pretty_json(destination, &omo_config)
}

fn ensure_omo_package_installed_for_distribution(
    base_dir: &Path,
    distribution: Option<&str>,
) -> Result<OmoPackageInstallResult, CliError> {
    match distribution
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(distribution) => ensure_omo_package_installed_in_wsl(base_dir, distribution),
        None => ensure_omo_package_installed(base_dir),
    }
}

fn ensure_omo_package_installed(base_dir: &Path) -> Result<OmoPackageInstallResult, CliError> {
    let shadow_dir = base_dir.join("omo-config");
    fs::create_dir_all(&shadow_dir).map_err(CliError::Io)?;
    let node_modules_status = ensure_shadow_node_modules_isolated(&shadow_dir)?;
    let package_dir = shadow_dir.join("node_modules").join("oh-my-opencode");
    if package_dir.is_dir() {
        return Ok(OmoPackageInstallResult {
            status: "already-installed",
            package_dir,
            package_manager: None,
            distribution: None,
            command: Vec::new(),
            node_modules_status,
        });
    }

    ensure_omo_package_manifest(&shadow_dir)?;

    let (package_manager, executable, args) = resolve_omo_package_manager().ok_or_else(|| {
        CliError::Control(
            "Could not install oh-my-opencode because neither bun nor npm was found on PATH."
                .to_string(),
        )
    })?;
    let output = Command::new(&executable)
        .args(&args)
        .current_dir(&shadow_dir)
        .output()
        .map_err(|error| {
            CliError::Control(format!(
                "failed to run {package_manager} for oh-my-opencode install: {error}"
            ))
        })?;
    if !output.status.success() {
        return Err(CliError::Control(format!(
            "{package_manager} failed to install oh-my-opencode with status {}. {}",
            output.status,
            command_output_excerpt(&output)
        )));
    }

    Ok(OmoPackageInstallResult {
        status: "installed",
        package_dir,
        package_manager: Some(package_manager.to_string()),
        distribution: None,
        command: std::iter::once(executable.display().to_string())
            .chain(args)
            .collect(),
        node_modules_status,
    })
}

fn ensure_omo_package_installed_in_wsl(
    base_dir: &Path,
    distribution: &str,
) -> Result<OmoPackageInstallResult, CliError> {
    let shadow_dir = base_dir.join("omo-config");
    fs::create_dir_all(&shadow_dir).map_err(CliError::Io)?;
    let node_modules_status = ensure_shadow_node_modules_isolated(&shadow_dir)?;
    let package_dir = shadow_dir.join("node_modules").join("oh-my-opencode");
    if package_dir.is_dir() {
        return Ok(OmoPackageInstallResult {
            status: "already-installed",
            package_dir,
            package_manager: None,
            distribution: Some(distribution.to_string()),
            command: Vec::new(),
            node_modules_status,
        });
    }

    ensure_omo_package_manifest(&shadow_dir)?;

    let spec = build_wsl_omo_package_install_command(distribution, &shadow_dir)?;
    let output = Command::new(&spec.executable)
        .args(&spec.args)
        .output()
        .map_err(|error| {
            CliError::Control(format!(
                "failed to run WSL {distribution} for oh-my-opencode install: {error}"
            ))
        })?;
    if !output.status.success() {
        return Err(CliError::Control(format!(
            "WSL {distribution} failed to install oh-my-opencode with status {}. {}",
            output.status,
            command_output_excerpt(&output)
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let status = match extract_output_marker(&stdout, "AGENTMUX_OMO_STATUS") {
        Some("already-installed") => "already-installed",
        _ => "installed",
    };
    let package_manager = extract_output_marker(&stdout, "AGENTMUX_OMO_MANAGER")
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    Ok(OmoPackageInstallResult {
        status,
        package_dir,
        package_manager,
        distribution: Some(distribution.to_string()),
        command: std::iter::once(spec.executable).chain(spec.args).collect(),
        node_modules_status,
    })
}

fn ensure_shadow_node_modules_isolated(shadow_dir: &Path) -> Result<&'static str, CliError> {
    let node_modules = shadow_dir.join("node_modules");
    if is_symlink(&node_modules) {
        fs::remove_file(&node_modules).map_err(CliError::Io)?;
        fs::create_dir_all(&node_modules).map_err(CliError::Io)?;
        return Ok("symlink-replaced");
    }
    fs::create_dir_all(&node_modules).map_err(CliError::Io)?;
    Ok("isolated")
}

fn extract_output_marker<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then(|| value.trim())
    })
}

fn build_wsl_omo_package_install_command(
    distribution: &str,
    shadow_dir: &Path,
) -> Result<WslDoctorCommand, CliError> {
    let shadow_dir = path_to_wsl_value(shadow_dir)?;
    let script = r#"set -eu
cd "$1"
if [ -d node_modules/oh-my-opencode ]; then
  printf 'AGENTMUX_OMO_STATUS=already-installed\n'
  printf 'AGENTMUX_OMO_MANAGER=\n'
  exit 0
fi
if [ -L node_modules ]; then
  rm node_modules
fi
mkdir -p node_modules
if [ ! -f package.json ]; then
  printf '%s\n' '{"private":true,"dependencies":{}}' > package.json
fi
if command -v bun >/dev/null 2>&1; then
  manager=bun
  bun add oh-my-opencode
elif command -v npm >/dev/null 2>&1; then
  manager=npm
  npm install oh-my-opencode --save
else
  printf 'Neither bun nor npm was found on WSL PATH\n' >&2
  exit 127
fi
printf '\nAGENTMUX_OMO_STATUS=installed\n'
printf 'AGENTMUX_OMO_MANAGER=%s\n' "$manager"
"#;
    Ok(build_wsl_shell_command(
        distribution,
        script,
        "agentmux-omo-install",
        &[shadow_dir],
    ))
}

fn ensure_omo_package_manifest(shadow_dir: &Path) -> Result<(), CliError> {
    let package_json = shadow_dir.join("package.json");
    if package_json.is_file() {
        return Ok(());
    }
    write_pretty_json(
        &package_json,
        &serde_json::json!({
            "private": true,
            "dependencies": {},
        }),
    )
}

fn resolve_omo_package_manager() -> Option<(&'static str, PathBuf, Vec<String>)> {
    find_executable_on_path("bun")
        .map(|path| {
            (
                "bun",
                path,
                vec!["add".to_string(), "oh-my-opencode".to_string()],
            )
        })
        .or_else(|| {
            find_executable_on_path("npm").map(|path| {
                (
                    "npm",
                    path,
                    vec![
                        "install".to_string(),
                        "oh-my-opencode".to_string(),
                        "--save".to_string(),
                    ],
                )
            })
        })
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn command_output_excerpt(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let text = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    if text.is_empty() {
        return "No output was captured.".to_string();
    }
    text.chars().take(800).collect()
}

fn setup_omc_restore_module(base_dir: &Path) -> Result<PathBuf, CliError> {
    let module_path = base_dir.join("omc-restore-node-options.cjs");
    let script = r#"'use strict';

const original = Object.prototype.hasOwnProperty.call(process.env, 'AGENTMUX_ORIGINAL_NODE_OPTIONS')
  ? process.env.AGENTMUX_ORIGINAL_NODE_OPTIONS
  : process.env.CMUX_ORIGINAL_NODE_OPTIONS;

if (original && original.length > 0) {
  process.env.NODE_OPTIONS = original;
} else {
  delete process.env.NODE_OPTIONS;
}

delete process.env.AGENTMUX_ORIGINAL_NODE_OPTIONS;
delete process.env.CMUX_ORIGINAL_NODE_OPTIONS;
"#;
    fs::write(&module_path, script).map_err(CliError::Io)?;
    Ok(module_path)
}

fn install_agent_integration_shims(
    base_dir: &Path,
    bin_dir: &Path,
    powershell_profile: Option<&Path>,
    shell_profile: Option<&Path>,
) -> Result<AgentIntegrationInstallResult, CliError> {
    fs::create_dir_all(bin_dir).map_err(|error| {
        CliError::Io(io::Error::new(
            error.kind(),
            format!(
                "failed to create integration bin directory '{}': {error}",
                bin_dir.display()
            ),
        ))
    })?;
    for kind in AgentIntegrationKind::all() {
        setup_agent_integration_files(kind, base_dir, Vec::new())?;
    }

    let launcher = std::env::current_exe().map_err(CliError::Io)?;
    let mut wrappers = Vec::new();
    for kind in AgentIntegrationKind::all() {
        wrappers.extend(write_agent_integration_entrypoint(
            bin_dir, kind, &launcher,
        )?);
    }

    let powershell_snippet = base_dir.join("agentmux-integrations.ps1");
    let shell_snippet = base_dir.join("agentmux-integrations.sh");
    let powershell_block = powershell_path_block(bin_dir);
    let shell_block = shell_path_block(bin_dir);
    fs::write(&powershell_snippet, format!("{powershell_block}\n")).map_err(CliError::Io)?;
    fs::write(&shell_snippet, format!("{shell_block}\n")).map_err(CliError::Io)?;

    if let Some(profile) = powershell_profile {
        write_managed_profile_block(
            profile,
            "# >>> AgentMux cmux integration shims >>>",
            "# <<< AgentMux cmux integration shims <<<",
            &powershell_block,
        )?;
    }
    if let Some(profile) = shell_profile {
        write_managed_profile_block(
            profile,
            "# >>> AgentMux cmux integration shims >>>",
            "# <<< AgentMux cmux integration shims <<<",
            &shell_block,
        )?;
    }

    Ok(AgentIntegrationInstallResult {
        base_dir: base_dir.to_path_buf(),
        bin_dir: bin_dir.to_path_buf(),
        wrappers,
        powershell_snippet,
        shell_snippet,
        powershell_profile: powershell_profile.map(Path::to_path_buf),
        shell_profile: shell_profile.map(Path::to_path_buf),
        user_path: None,
    })
}

fn inspect_agent_integrations(
    base_dir: &Path,
    bin_dir: &Path,
    kind: Option<AgentIntegrationKind>,
    wsl_distribution: Option<&str>,
) -> AgentIntegrationDoctorResult {
    let kinds = kind
        .map(|kind| vec![kind])
        .unwrap_or_else(|| AgentIntegrationKind::all().to_vec());
    let bin_dir_on_path = path_contains_dir(bin_dir);
    let integrations = kinds
        .into_iter()
        .map(|kind| {
            inspect_agent_integration(base_dir, bin_dir, bin_dir_on_path, wsl_distribution, kind)
        })
        .collect();

    AgentIntegrationDoctorResult {
        base_dir: base_dir.to_path_buf(),
        bin_dir: bin_dir.to_path_buf(),
        bin_dir_on_path,
        wsl_distribution: wsl_distribution.map(ToString::to_string),
        integrations,
    }
}

fn inspect_agent_integration(
    base_dir: &Path,
    bin_dir: &Path,
    bin_dir_on_path: bool,
    wsl_distribution: Option<&str>,
    kind: AgentIntegrationKind,
) -> AgentIntegrationDoctorItem {
    let command = kind.command_name();
    let executable = kind.executable();
    let shim_dir = base_dir.join(kind.shim_dir_name());
    let posix_wrapper = bin_dir.join(command);
    let cmd_wrapper = bin_dir.join(format!("{command}.cmd"));
    let mut checks = Vec::new();

    let wrapper_ok = posix_wrapper.is_file() || cmd_wrapper.is_file();
    checks.push(AgentIntegrationDoctorCheck {
        name: "persistent-wrapper",
        ok: wrapper_ok,
        detail: format!("{} or {}", posix_wrapper.display(), cmd_wrapper.display()),
        fix: (!wrapper_ok).then(|| "Run `cmux integrations install-shims`.".to_string()),
    });

    let tmux_shim = shim_dir.join("tmux");
    let tmux_cmd_shim = shim_dir.join("tmux.cmd");
    let tmux_ok = tmux_shim.is_file() || tmux_cmd_shim.is_file();
    checks.push(AgentIntegrationDoctorCheck {
        name: "tmux-shim",
        ok: tmux_ok,
        detail: format!("{} or {}", tmux_shim.display(), tmux_cmd_shim.display()),
        fix: (!tmux_ok).then(|| {
            format!("Run `cmux integrations setup {command}` or `cmux integrations install-shims`.")
        }),
    });

    let wrapper_on_path = find_executable_on_path(command).is_some();
    checks.push(AgentIntegrationDoctorCheck {
        name: "wrapper-on-path",
        ok: wrapper_on_path,
        detail: if bin_dir_on_path {
            format!("{command} was resolved from PATH")
        } else {
            format!("{} is not currently on PATH", bin_dir.display())
        },
        fix: (!wrapper_on_path).then(|| {
            "Source the generated AgentMux PATH snippet or rerun install-shims with a profile path."
                .to_string()
        }),
    });

    if let Some(distribution) = wsl_distribution {
        push_wsl_agent_integration_checks(
            &mut checks,
            distribution,
            kind,
            base_dir,
            &shim_dir,
            executable,
        );
    } else {
        let executable_path = find_executable_on_path(executable);
        let executable_detail = executable_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| format!("{executable} was not found on the current PATH"));
        let executable_ok = executable_path.is_some();
        checks.push(AgentIntegrationDoctorCheck {
            name: "underlying-executable",
            ok: executable_ok,
            detail: executable_detail,
            fix: (!executable_ok).then(|| kind.install_hint().to_string()),
        });
    }

    match kind {
        AgentIntegrationKind::Omo => {
            let opencode_config = base_dir.join("omo-config").join("opencode.json");
            let omo_config = base_dir.join("omo-config").join("oh-my-opencode.json");
            let shadow_ok = opencode_config.is_file() && omo_config.is_file();
            checks.push(AgentIntegrationDoctorCheck {
                name: "omo-shadow-config",
                ok: shadow_ok,
                detail: format!("{} and {}", opencode_config.display(), omo_config.display()),
                fix: (!shadow_ok).then(|| {
                    "Run `cmux integrations setup omo` to create the shadow OpenCode config."
                        .to_string()
                }),
            });
            push_omo_shadow_config_content_checks(&mut checks, &opencode_config, &omo_config);
            let package_dir = base_dir
                .join("omo-config")
                .join("node_modules")
                .join("oh-my-opencode");
            let node_modules = base_dir.join("omo-config").join("node_modules");
            let node_modules_is_symlink = is_symlink(&node_modules);
            checks.push(AgentIntegrationDoctorCheck {
                name: "omo-node-modules-isolated",
                ok: !node_modules_is_symlink,
                detail: if node_modules_is_symlink {
                    format!(
                        "{} is a symlink and may point back to the user's original OpenCode packages",
                        node_modules.display()
                    )
                } else if node_modules.is_dir() {
                    format!("{} is an isolated shadow directory", node_modules.display())
                } else {
                    format!(
                        "{} has not been created yet; package setup will create an isolated shadow directory",
                        node_modules.display()
                    )
                },
                fix: node_modules_is_symlink.then(|| {
                    "Run `cmux integrations setup omo --install-packages` to replace the shadow node_modules symlink with an isolated directory."
                        .to_string()
                }),
            });
            let package_ok = package_dir.is_dir();
            let package_manager = wsl_distribution
                .map(inspect_wsl_package_manager)
                .unwrap_or_else(|| {
                    resolve_omo_package_manager()
                        .map(|(name, path, _)| format!("{name} at {}", path.display()))
                        .unwrap_or_else(|| "bun or npm was not found on PATH".to_string())
                });
            checks.push(AgentIntegrationDoctorCheck {
                name: "omo-package",
                ok: package_ok,
                detail: if package_ok {
                    package_dir.display().to_string()
                } else {
                    package_manager
                },
                fix: (!package_ok).then(|| {
                    "Run `cmux integrations setup omo --install-packages` or launch `cmux omo`."
                        .to_string()
                }),
            });
        }
        AgentIntegrationKind::Omc => {
            let restore_module = base_dir.join("omc-restore-node-options.cjs");
            let restore_ok = restore_module.is_file();
            checks.push(AgentIntegrationDoctorCheck {
                name: "omc-node-options-restore",
                ok: restore_ok,
                detail: restore_module.display().to_string(),
                fix: (!restore_ok).then(|| {
                    "Run `cmux integrations setup omc` to create the NODE_OPTIONS restore module."
                        .to_string()
                }),
            });
        }
        AgentIntegrationKind::ClaudeTeams | AgentIntegrationKind::Omx => {}
    }

    let status = if checks.iter().all(|check| check.ok) {
        "ready"
    } else {
        "needs-attention"
    };

    AgentIntegrationDoctorItem {
        kind,
        command: command.to_string(),
        executable: executable.to_string(),
        status,
        install_hint: kind.install_hint(),
        checks,
    }
}

fn push_omo_shadow_config_content_checks(
    checks: &mut Vec<AgentIntegrationDoctorCheck>,
    opencode_config: &Path,
    omo_config: &Path,
) {
    match read_json_file_with_error(opencode_config) {
        Ok(value) => {
            let plugin_ok = json_array_contains_string(&value, "plugin", "oh-my-opencode");
            checks.push(AgentIntegrationDoctorCheck {
                name: "omo-opencode-plugin",
                ok: plugin_ok,
                detail: if plugin_ok {
                    format!("{} includes oh-my-opencode", opencode_config.display())
                } else {
                    format!(
                        "{} does not include oh-my-opencode in the plugin array",
                        opencode_config.display()
                    )
                },
                fix: (!plugin_ok).then(|| {
                    "Run `cmux integrations setup omo` to refresh the shadow OpenCode config."
                        .to_string()
                }),
            });
        }
        Err(detail) => checks.push(AgentIntegrationDoctorCheck {
            name: "omo-opencode-plugin",
            ok: false,
            detail,
            fix: Some(
                "Run `cmux integrations setup omo` to recreate the shadow OpenCode config."
                    .to_string(),
            ),
        }),
    }

    match read_json_file_with_error(omo_config) {
        Ok(value) => {
            let tmux_ok = json_nested_bool(&value, &["tmux", "enabled"]) == Some(true);
            checks.push(AgentIntegrationDoctorCheck {
                name: "omo-tmux-enabled",
                ok: tmux_ok,
                detail: if tmux_ok {
                    format!("{} has tmux.enabled=true", omo_config.display())
                } else {
                    format!("{} does not have tmux.enabled=true", omo_config.display())
                },
                fix: (!tmux_ok).then(|| {
                    "Run `cmux integrations setup omo` to enable tmux mode in the shadow config."
                        .to_string()
                }),
            });
        }
        Err(detail) => checks.push(AgentIntegrationDoctorCheck {
            name: "omo-tmux-enabled",
            ok: false,
            detail,
            fix: Some(
                "Run `cmux integrations setup omo` to recreate the shadow OpenCode config."
                    .to_string(),
            ),
        }),
    }
}

fn read_json_file_with_error(path: &Path) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_str(&strip_jsonc_for_json(&text))
        .map_err(|error| format!("failed to parse {} as JSON: {error}", path.display()))
}

fn json_array_contains_string(value: &serde_json::Value, key: &str, item: &str) -> bool {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|items| items.iter().any(|value| value.as_str() == Some(item)))
        .unwrap_or(false)
}

fn json_nested_bool(value: &serde_json::Value, path: &[&str]) -> Option<bool> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_bool()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WslDoctorCommand {
    executable: String,
    args: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WslDoctorProbe {
    ok: bool,
    detail: String,
}

fn push_wsl_agent_integration_checks(
    checks: &mut Vec<AgentIntegrationDoctorCheck>,
    distribution: &str,
    kind: AgentIntegrationKind,
    base_dir: &Path,
    shim_dir: &Path,
    executable: &str,
) {
    let distribution_probe = run_wsl_doctor_probe(distribution, "printf '%s' ok", &[]);
    let distribution_ok = distribution_probe.ok;
    checks.push(AgentIntegrationDoctorCheck {
        name: "wsl-distribution",
        ok: distribution_ok,
        detail: format_wsl_probe_detail(distribution, &distribution_probe),
        fix: (!distribution_ok).then(|| {
            format!("Install or enable the '{distribution}' WSL distribution, then rerun doctor.")
        }),
    });
    if !distribution_ok {
        return;
    }

    let executable_probe = run_wsl_doctor_probe(
        distribution,
        "if command -v -- \"$1\"; then exit 0; fi; printf '%s was not found on WSL PATH' \"$1\" >&2; exit 127",
        &[executable.to_string()],
    );
    let executable_ok = executable_probe.ok;
    checks.push(AgentIntegrationDoctorCheck {
        name: "underlying-executable",
        ok: executable_ok,
        detail: format_wsl_probe_detail(distribution, &executable_probe),
        fix: (!executable_ok).then(|| kind.install_hint().to_string()),
    });

    push_wsl_path_probe_check(
        checks,
        "wsl-tmux-shim",
        distribution,
        shim_dir,
        "if test -f \"$1/tmux\"; then printf '%s/tmux' \"$1\"; else printf 'tmux shim was not found at %s/tmux' \"$1\" >&2; exit 1; fi",
        format!(
            "Run `cmux integrations setup {}` or `cmux integrations install-shims`, then retry from WSL.",
            kind.command_name()
        ),
    );

    match kind {
        AgentIntegrationKind::Omo => {
            push_wsl_path_probe_check(
                checks,
                "wsl-omo-shadow-config",
                distribution,
                &base_dir.join("omo-config"),
                "if test -f \"$1/opencode.json\" && test -f \"$1/oh-my-opencode.json\"; then printf '%s' \"$1\"; else printf 'OMO shadow config was not found at %s' \"$1\" >&2; exit 1; fi",
                "Run `cmux integrations setup omo` to create the WSL-visible shadow config."
                    .to_string(),
            );
        }
        AgentIntegrationKind::Omc => {
            push_wsl_path_probe_check(
                checks,
                "wsl-omc-node-options-restore",
                distribution,
                &base_dir.join("omc-restore-node-options.cjs"),
                "if test -f \"$1\"; then printf '%s' \"$1\"; else printf 'OMC NODE_OPTIONS restore module was not found at %s' \"$1\" >&2; exit 1; fi",
                "Run `cmux integrations setup omc` to create the WSL-visible restore module."
                    .to_string(),
            );
        }
        AgentIntegrationKind::ClaudeTeams | AgentIntegrationKind::Omx => {}
    }
}

fn push_wsl_path_probe_check(
    checks: &mut Vec<AgentIntegrationDoctorCheck>,
    name: &'static str,
    distribution: &str,
    path: &Path,
    script: &str,
    fix: String,
) {
    let path = match path_to_wsl_value(path) {
        Ok(path) => path,
        Err(error) => {
            checks.push(AgentIntegrationDoctorCheck {
                name,
                ok: false,
                detail: error.to_string(),
                fix: Some(fix),
            });
            return;
        }
    };
    let probe = run_wsl_doctor_probe(distribution, script, &[path]);
    checks.push(AgentIntegrationDoctorCheck {
        name,
        ok: probe.ok,
        detail: format_wsl_probe_detail(distribution, &probe),
        fix: (!probe.ok).then_some(fix),
    });
}

fn inspect_wsl_package_manager(distribution: &str) -> String {
    let probe = run_wsl_doctor_probe(
        distribution,
        "command -v bun || command -v npm || { printf 'bun or npm was not found on WSL PATH' >&2; exit 127; }",
        &[],
    );
    if probe.ok {
        format_wsl_probe_detail(distribution, &probe)
    } else {
        format!(
            "WSL {distribution}: bun or npm was not found ({})",
            probe.detail
        )
    }
}

fn run_wsl_doctor_probe(
    distribution: &str,
    script: &str,
    script_args: &[String],
) -> WslDoctorProbe {
    let spec = build_wsl_doctor_command(distribution, script, script_args);
    match Command::new(&spec.executable).args(&spec.args).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let detail = if output.status.success() {
                if stdout.is_empty() {
                    "ok".to_string()
                } else {
                    stdout
                }
            } else if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("wsl.exe exited with status {}", output.status)
            };
            WslDoctorProbe {
                ok: output.status.success(),
                detail,
            }
        }
        Err(error) => WslDoctorProbe {
            ok: false,
            detail: format!("failed to run wsl.exe: {error}"),
        },
    }
}

fn build_wsl_doctor_command(
    distribution: &str,
    script: &str,
    script_args: &[String],
) -> WslDoctorCommand {
    build_wsl_shell_command(distribution, script, "agentmux-doctor", script_args)
}

fn build_wsl_shell_command(
    distribution: &str,
    script: &str,
    shell_name: &str,
    script_args: &[String],
) -> WslDoctorCommand {
    let mut args = vec![
        "--distribution".to_string(),
        distribution.to_string(),
        "--exec".to_string(),
        "sh".to_string(),
        "-lc".to_string(),
        script.to_string(),
        shell_name.to_string(),
    ];
    args.extend(script_args.iter().cloned());
    WslDoctorCommand {
        executable: "wsl.exe".to_string(),
        args,
    }
}

fn format_wsl_probe_detail(distribution: &str, probe: &WslDoctorProbe) -> String {
    format!("WSL {distribution}: {}", probe.detail)
}

fn write_agent_integration_entrypoint(
    bin_dir: &Path,
    kind: AgentIntegrationKind,
    launcher: &Path,
) -> Result<Vec<PathBuf>, CliError> {
    let command = kind.command_name();
    let posix_path = bin_dir.join(command);
    let posix = format!(
        "#!/usr/bin/env sh\nif command -v cmux >/dev/null 2>&1; then\n  exec cmux {command} \"$@\"\nfi\nexec cmux.exe {command} \"$@\"\n"
    );
    fs::write(&posix_path, posix).map_err(CliError::Io)?;
    set_executable_if_supported(&posix_path)?;

    let cmd_path = bin_dir.join(format!("{command}.cmd"));
    let cmd = format!("@echo off\r\n\"{}\" {command} %*\r\n", launcher.display());
    fs::write(&cmd_path, cmd).map_err(CliError::Io)?;

    Ok(vec![posix_path, cmd_path])
}

fn powershell_path_block(bin_dir: &Path) -> String {
    let escaped = path_to_env_value(bin_dir).replace('\'', "''");
    format!(
        "# >>> AgentMux cmux integration shims >>>\n$agentmuxCmuxShimBin = '{escaped}'\nif (($env:Path -split ';') -notcontains $agentmuxCmuxShimBin) {{\n  $env:Path = \"$agentmuxCmuxShimBin;$env:Path\"\n}}\n# <<< AgentMux cmux integration shims <<<"
    )
}

fn shell_path_block(bin_dir: &Path) -> String {
    let quoted = shell_single_quote(&path_to_env_value(bin_dir));
    format!(
        "# >>> AgentMux cmux integration shims >>>\nagentmux_cmux_shim_bin={quoted}\ncase \":$PATH:\" in\n  *\":$agentmux_cmux_shim_bin:\"*) ;;\n  *) export PATH=\"$agentmux_cmux_shim_bin:$PATH\" ;;\nesac\n# <<< AgentMux cmux integration shims <<<"
    )
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(windows)]
fn ensure_windows_user_path_contains(bin_dir: &Path) -> Result<WindowsUserPathUpdate, CliError> {
    let bin_dir_value = path_to_env_value(bin_dir);
    let current = query_windows_user_path()?;
    let Some(next) = next_windows_user_path_value(current.as_deref(), &bin_dir_value) else {
        return Ok(WindowsUserPathUpdate {
            status: "already-present",
            bin_dir: bin_dir.to_path_buf(),
            detail: format!("{bin_dir_value} is already present in the user PATH."),
        });
    };
    write_windows_user_path(&next)?;
    Ok(WindowsUserPathUpdate {
        status: "updated",
        bin_dir: bin_dir.to_path_buf(),
        detail: format!(
            "{bin_dir_value} was added to the user PATH. Restart terminals to inherit it."
        ),
    })
}

#[cfg(not(windows))]
fn ensure_windows_user_path_contains(bin_dir: &Path) -> Result<WindowsUserPathUpdate, CliError> {
    Err(CliError::Control(format!(
        "user PATH registration is only available on Windows. Add '{}' to PATH manually.",
        bin_dir.display()
    )))
}

#[cfg(windows)]
fn query_windows_user_path() -> Result<Option<String>, CliError> {
    let output = Command::new("reg.exe")
        .args(["query", r"HKCU\Environment", "/v", "Path"])
        .output()
        .map_err(|error| CliError::Control(format!("failed to query user PATH: {error}")))?;
    if !output.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_reg_query_value(&stdout, "Path"))
}

#[cfg(windows)]
fn write_windows_user_path(value: &str) -> Result<(), CliError> {
    let output = Command::new("reg.exe")
        .args([
            "add",
            r"HKCU\Environment",
            "/v",
            "Path",
            "/t",
            "REG_EXPAND_SZ",
            "/d",
            value,
            "/f",
        ])
        .output()
        .map_err(|error| CliError::Control(format!("failed to update user PATH: {error}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(CliError::Control(format!(
            "failed to update user PATH with status {}. {}",
            output.status,
            command_output_excerpt(&output)
        )))
    }
}

fn parse_reg_query_value(output: &str, value_name: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let trimmed = line.trim();
        let rest = trimmed.strip_prefix(value_name)?.trim_start();
        let rest = rest
            .strip_prefix("REG_EXPAND_SZ")
            .or_else(|| rest.strip_prefix("REG_SZ"))?
            .trim_start();
        (!rest.is_empty()).then(|| rest.to_string())
    })
}

fn next_windows_user_path_value(current: Option<&str>, bin_dir: &str) -> Option<String> {
    let normalized_bin = normalize_path_segment_for_compare(bin_dir);
    let current = current.unwrap_or("").trim();
    let contains = current.split(';').any(|entry| {
        normalize_path_segment_for_compare(entry) == normalized_bin && !entry.trim().is_empty()
    });
    if contains {
        return None;
    }
    if current.is_empty() {
        Some(bin_dir.to_string())
    } else {
        Some(format!("{};{}", current.trim_end_matches(';'), bin_dir))
    }
}

fn normalize_path_segment_for_compare(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .to_ascii_lowercase()
}

fn write_managed_profile_block(
    profile: &Path,
    start_marker: &str,
    end_marker: &str,
    block: &str,
) -> Result<(), CliError> {
    if let Some(parent) = profile.parent() {
        fs::create_dir_all(parent).map_err(CliError::Io)?;
    }
    let existing = fs::read_to_string(profile).unwrap_or_default();
    let next = replace_managed_block(&existing, start_marker, end_marker, block);
    fs::write(profile, next).map_err(CliError::Io)
}

fn replace_managed_block(
    existing: &str,
    start_marker: &str,
    end_marker: &str,
    block: &str,
) -> String {
    let trimmed_block = block.trim_end();
    if let Some(start) = existing.find(start_marker) {
        if let Some(relative_end) = existing[start..].find(end_marker) {
            let end = start + relative_end + end_marker.len();
            let mut next = String::new();
            next.push_str(existing[..start].trim_end());
            if !next.is_empty() {
                next.push_str("\n\n");
            }
            next.push_str(trimmed_block);
            let suffix = existing[end..].trim_start_matches(['\r', '\n']);
            if !suffix.is_empty() {
                next.push_str("\n\n");
                next.push_str(suffix);
            } else {
                next.push('\n');
            }
            return next;
        }
    }
    let mut next = existing.trim_end().to_string();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str(trimmed_block);
    next.push('\n');
    next
}

fn read_json_file(path: &Path) -> Option<serde_json::Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&strip_jsonc_for_json(&text)).ok()
}

fn write_pretty_json(path: &Path, value: &serde_json::Value) -> Result<(), CliError> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| CliError::Control(format!("failed to encode json: {error}")))?;
    fs::write(path, format!("{text}\n")).map_err(CliError::Io)
}

fn write_text_file_with_newline(path: &Path, text: &str) -> Result<(), CliError> {
    let mut text = text.trim_end().to_string();
    text.push('\n');
    fs::write(path, text).map_err(CliError::Io)
}

fn merge_opencode_plugin_config_text(input: &str, plugin: &str) -> Option<String> {
    let (object_start, object_end) = find_top_level_jsonc_object_span(input)?;
    if let Some(plugin_span) =
        find_jsonc_object_property_value_span(input, object_start, object_end, "plugin")
    {
        let plugin_value = &input[plugin_span.start..plugin_span.end];
        let parsed: serde_json::Value =
            serde_json::from_str(&strip_jsonc_for_json(plugin_value)).ok()?;
        if parsed
            .as_array()
            .map(|items| items.iter().any(|value| value.as_str() == Some(plugin)))
            .unwrap_or(false)
        {
            return Some(input.to_string());
        }
        if input.as_bytes().get(plugin_span.start).copied() != Some(b'[') {
            return None;
        }
        let close_index = plugin_span.end.checked_sub(1)?;
        let close_indent = line_indent_before(input, close_index);
        let item_indent = format!("{close_indent}  ");
        let comma = if jsonc_needs_comma_before_insert(input, plugin_span.start + 1, close_index) {
            ","
        } else {
            ""
        };
        let insertion = format!("{comma}\n{item_indent}\"{plugin}\"\n{close_indent}");
        let mut next = input.to_string();
        next.insert_str(close_index, &insertion);
        return Some(next);
    }

    insert_jsonc_top_level_property(
        input,
        object_start,
        object_end,
        &format!("\"plugin\": [\"{plugin}\"]"),
    )
}

fn merge_omo_tmux_config_text(input: &str) -> Option<String> {
    let (object_start, object_end) = find_top_level_jsonc_object_span(input)?;
    if let Some(tmux_span) =
        find_jsonc_object_property_value_span(input, object_start, object_end, "tmux")
    {
        if input.as_bytes().get(tmux_span.start).copied() != Some(b'{') {
            return None;
        }
        if let Some(enabled_span) =
            find_jsonc_object_property_value_span(input, tmux_span.start, tmux_span.end, "enabled")
        {
            let enabled_value = strip_jsonc_for_json(&input[enabled_span.start..enabled_span.end]);
            if enabled_value.trim() == "true" {
                return Some(input.to_string());
            }
            let mut next = input.to_string();
            next.replace_range(enabled_span.start..enabled_span.end, "true");
            return Some(next);
        }

        return insert_jsonc_object_property(
            input,
            tmux_span.start,
            tmux_span.end,
            "\"enabled\": true",
        );
    }

    insert_jsonc_top_level_property(
        input,
        object_start,
        object_end,
        "\"tmux\": {\"enabled\": true}",
    )
}

fn insert_jsonc_top_level_property(
    input: &str,
    object_start: usize,
    object_end: usize,
    property_text: &str,
) -> Option<String> {
    insert_jsonc_object_property(input, object_start, object_end, property_text)
}

fn insert_jsonc_object_property(
    input: &str,
    object_start: usize,
    object_end: usize,
    property_text: &str,
) -> Option<String> {
    let close_index = object_end.checked_sub(1)?;
    let close_indent = line_indent_before(input, close_index);
    let property_indent = format!("{close_indent}  ");
    let comma = if jsonc_needs_comma_before_insert(input, object_start + 1, close_index) {
        ","
    } else {
        ""
    };
    let insertion = format!("{comma}\n{property_indent}{property_text}\n{close_indent}");
    let mut next = input.to_string();
    next.insert_str(close_index, &insertion);
    Some(next)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct JsoncValueSpan {
    start: usize,
    end: usize,
}

fn find_top_level_jsonc_object_span(input: &str) -> Option<(usize, usize)> {
    let start = skip_jsonc_ws_and_comments(input, 0)?;
    (input.as_bytes().get(start).copied() == Some(b'{'))
        .then_some(())
        .and_then(|_| find_jsonc_value_end(input, start).map(|end| (start, end)))
}

fn find_jsonc_object_property_value_span(
    input: &str,
    object_start: usize,
    object_end: usize,
    key: &str,
) -> Option<JsoncValueSpan> {
    if input.as_bytes().get(object_start).copied() != Some(b'{') {
        return None;
    }
    let close_index = object_end.checked_sub(1)?;
    let mut index = object_start + 1;
    while index < close_index {
        index = skip_jsonc_ws_and_comments(input, index)?;
        if index >= close_index || input.as_bytes().get(index).copied() == Some(b'}') {
            return None;
        }
        let (property_name, property_end) = parse_jsonc_string_token(input, index)?;
        index = skip_jsonc_ws_and_comments(input, property_end)?;
        if input.as_bytes().get(index).copied() != Some(b':') {
            return None;
        }
        let value_start = skip_jsonc_ws_and_comments(input, index + 1)?;
        let value_end = find_jsonc_value_end(input, value_start)?;
        if property_name == key {
            return Some(JsoncValueSpan {
                start: value_start,
                end: value_end,
            });
        }
        index = skip_jsonc_ws_and_comments(input, value_end)?;
        if input.as_bytes().get(index).copied() == Some(b',') {
            index += 1;
        }
    }
    None
}

fn find_jsonc_value_end(input: &str, start: usize) -> Option<usize> {
    match input.as_bytes().get(start).copied()? {
        b'{' => find_jsonc_bracket_end(input, start, b'{', b'}'),
        b'[' => find_jsonc_bracket_end(input, start, b'[', b']'),
        b'"' => parse_jsonc_string_token(input, start).map(|(_, end)| end),
        _ => {
            let mut index = start;
            while index < input.len() {
                match input.as_bytes()[index] {
                    b',' | b'}' | b']' => break,
                    b'/' if input.as_bytes().get(index + 1).copied() == Some(b'/') => break,
                    b'/' if input.as_bytes().get(index + 1).copied() == Some(b'*') => break,
                    _ => index += 1,
                }
            }
            Some(input[..index].trim_end().len())
        }
    }
}

fn find_jsonc_bracket_end(input: &str, start: usize, open: u8, close: u8) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut index = start;
    let mut depth = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let (_, end) = parse_jsonc_string_token(input, index)?;
                index = end;
            }
            b'/' if bytes.get(index + 1).copied() == Some(b'/') => {
                index = skip_jsonc_line_comment(input, index + 2);
            }
            b'/' if bytes.get(index + 1).copied() == Some(b'*') => {
                index = skip_jsonc_block_comment(input, index + 2);
            }
            value if value == open => {
                depth += 1;
                index += 1;
            }
            value if value == close => {
                depth = depth.checked_sub(1)?;
                index += 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => index += 1,
        }
    }
    None
}

fn parse_jsonc_string_token(input: &str, start: usize) -> Option<(String, usize)> {
    let bytes = input.as_bytes();
    if bytes.get(start).copied() != Some(b'"') {
        return None;
    }
    let mut value = String::new();
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => {
                let escaped = *bytes.get(index + 1)?;
                value.push(escaped as char);
                index += 2;
            }
            b'"' => return Some((value, index + 1)),
            byte => {
                value.push(byte as char);
                index += 1;
            }
        }
    }
    None
}

fn skip_jsonc_ws_and_comments(input: &str, mut index: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    loop {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        match (bytes.get(index).copied(), bytes.get(index + 1).copied()) {
            (Some(b'/'), Some(b'/')) => index = skip_jsonc_line_comment(input, index + 2),
            (Some(b'/'), Some(b'*')) => index = skip_jsonc_block_comment(input, index + 2),
            _ => return Some(index),
        }
    }
}

fn skip_jsonc_line_comment(input: &str, mut index: usize) -> usize {
    let bytes = input.as_bytes();
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

fn skip_jsonc_block_comment(input: &str, mut index: usize) -> usize {
    let bytes = input.as_bytes();
    while index + 1 < bytes.len() {
        if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            return index + 2;
        }
        index += 1;
    }
    bytes.len()
}

fn jsonc_needs_comma_before_insert(input: &str, content_start: usize, close_index: usize) -> bool {
    let content = &input[content_start..close_index];
    if strip_jsonc_for_json(content).trim().is_empty() {
        return false;
    }
    content
        .trim_end()
        .as_bytes()
        .last()
        .copied()
        .is_some_and(|byte| byte != b',')
}

fn line_indent_before(input: &str, index: usize) -> String {
    let line_start = input[..index].rfind('\n').map(|pos| pos + 1).unwrap_or(0);
    input[line_start..index]
        .chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .collect()
}

fn strip_jsonc_for_json(input: &str) -> String {
    remove_json_trailing_commas(&strip_jsonc_comments(input))
}

fn strip_jsonc_comments(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let start = index;
                index += 1;
                let mut escaped = false;
                while index < bytes.len() {
                    let byte = bytes[index];
                    index += 1;
                    if escaped {
                        escaped = false;
                    } else if byte == b'\\' {
                        escaped = true;
                    } else if byte == b'"' {
                        break;
                    }
                }
                output.push_str(&input[start..index]);
            }
            b'/' if bytes.get(index + 1).copied() == Some(b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1).copied() == Some(b'*') => {
                index += 2;
                while index + 1 < bytes.len() {
                    if bytes[index] == b'*' && bytes[index + 1] == b'/' {
                        index += 2;
                        break;
                    }
                    if bytes[index] == b'\n' {
                        output.push('\n');
                    }
                    index += 1;
                }
            }
            byte => {
                output.push(byte as char);
                index += 1;
            }
        }
    }
    output
}

fn remove_json_trailing_commas(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'"' {
            let start = index;
            index += 1;
            let mut escaped = false;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    break;
                }
            }
            output.push_str(&input[start..index]);
            continue;
        }
        if bytes[index] == b',' {
            let mut lookahead = index + 1;
            while bytes.get(lookahead).is_some_and(u8::is_ascii_whitespace) {
                lookahead += 1;
            }
            if matches!(bytes.get(lookahead), Some(b'}' | b']')) {
                index += 1;
                continue;
            }
        }
        output.push(bytes[index] as char);
        index += 1;
    }
    output
}

fn ensure_json_string_array_contains(value: &mut serde_json::Value, key: &str, item: &str) {
    if !value.is_object() {
        *value = serde_json::json!({});
    }
    let object = value.as_object_mut().expect("object set above");
    let entry = object
        .entry(key.to_string())
        .or_insert_with(|| serde_json::json!([]));
    if !entry.is_array() {
        *entry = serde_json::json!([]);
    }
    let array = entry.as_array_mut().expect("array set above");
    if !array.iter().any(|value| value.as_str() == Some(item)) {
        array.push(serde_json::Value::String(item.to_string()));
    }
}

fn ensure_nested_bool(value: &mut serde_json::Value, path: &[&str], enabled: bool) {
    if path.is_empty() {
        *value = serde_json::Value::Bool(enabled);
        return;
    }
    if !value.is_object() {
        *value = serde_json::json!({});
    }
    let mut current = value;
    for key in &path[..path.len() - 1] {
        let object = current.as_object_mut().expect("object set above");
        current = object
            .entry((*key).to_string())
            .or_insert_with(|| serde_json::json!({}));
        if !current.is_object() {
            *current = serde_json::json!({});
        }
    }
    let leaf_key = path[path.len() - 1];
    current
        .as_object_mut()
        .expect("object set above")
        .insert(leaf_key.to_string(), serde_json::Value::Bool(enabled));
}

fn resolve_cmuxterm_base_dir(base_dir: Option<&str>) -> Result<PathBuf, CliError> {
    if let Some(base_dir) = base_dir {
        return Ok(PathBuf::from(base_dir));
    }
    if let Some(base_dir) =
        std::env::var_os("AGENTMUX_CMUXTERM_HOME").or_else(|| std::env::var_os("CMUXTERM_HOME"))
    {
        return Ok(PathBuf::from(base_dir));
    }
    let home = home_dir_path()?;
    Ok(home.join(".cmuxterm"))
}

fn resolve_opencode_config_dir() -> Result<PathBuf, CliError> {
    if let Some(path) = std::env::var_os("OPENCODE_CONFIG_DIR") {
        return Ok(PathBuf::from(path));
    }
    Ok(home_dir_path()?.join(".config").join("opencode"))
}

fn home_dir_path() -> Result<PathBuf, CliError> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| {
            CliError::Control(
                "Could not resolve a home directory for cmux compatibility files.".to_string(),
            )
        })
}

fn prepend_path(dir: &Path, existing_path: Option<std::ffi::OsString>) -> String {
    let mut paths = vec![dir.to_path_buf()];
    if let Some(existing_path) = existing_path {
        paths.extend(std::env::split_paths(&existing_path));
    }
    std::env::join_paths(paths)
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|_| {
            let existing = std::env::var("PATH").unwrap_or_default();
            format!("{};{}", dir.display(), existing)
        })
}

fn path_contains_dir(dir: &Path) -> bool {
    let expected = normalize_path_for_compare(dir);
    std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path).any(|entry| normalize_path_for_compare(&entry) == expected)
        })
        .unwrap_or(false)
}

fn find_executable_on_path(command: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in executable_names(command) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn normalize_path_for_compare(path: &Path) -> String {
    let mut value = path.to_string_lossy().replace('/', "\\");
    while value.ends_with('\\') && value.len() > 3 {
        value.pop();
    }
    #[cfg(windows)]
    {
        value.make_ascii_lowercase();
    }
    value
}

#[cfg(windows)]
fn executable_names(command: &str) -> Vec<String> {
    let mut names = vec![command.to_string()];
    let command_lower = command.to_ascii_lowercase();
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    for extension in pathext
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let extension = if extension.starts_with('.') {
            extension.to_string()
        } else {
            format!(".{extension}")
        };
        if !command_lower.ends_with(&extension.to_ascii_lowercase()) {
            names.push(format!("{command}{extension}"));
        }
    }
    names
}

#[cfg(not(windows))]
fn executable_names(command: &str) -> Vec<String> {
    vec![command.to_string()]
}

fn node_options_with_required_module(existing: Option<&str>, module_path: &Path) -> String {
    let module = shell_quote_for_node_options(&path_to_env_value(module_path));
    let require = format!("--require {module}");
    match existing.map(str::trim).filter(|value| !value.is_empty()) {
        Some(existing) => format!("{require} {existing}"),
        None => require,
    }
}

fn shell_quote_for_node_options(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '\\' | '/' | '_' | '-' | '.'))
    {
        return value.to_string();
    }
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn path_to_env_value(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn agent_integration_runtime_json(runtime: &AgentIntegrationRuntime) -> serde_json::Value {
    let env = runtime
        .env
        .iter()
        .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
        .collect::<serde_json::Map<_, _>>();
    serde_json::json!({
        "integration": runtime.kind.command_name(),
        "command": runtime.command,
        "args": runtime.args,
        "base_dir": runtime.base_dir,
        "shim_dir": runtime.shim_dir,
        "shadow_config_dir": runtime.shadow_config_dir,
        "node_options_restore_module": runtime.node_options_restore_module,
        "package_install": runtime.package_install.as_ref().map(omo_package_install_json),
        "env": env,
    })
}

fn omo_package_install_json(result: &OmoPackageInstallResult) -> serde_json::Value {
    serde_json::json!({
        "status": result.status,
        "package_dir": result.package_dir,
        "package_manager": result.package_manager,
        "distribution": result.distribution,
        "command": result.command,
        "node_modules_status": result.node_modules_status,
    })
}

fn agent_integration_install_result_json(
    result: &AgentIntegrationInstallResult,
) -> serde_json::Value {
    serde_json::json!({
        "base_dir": result.base_dir,
        "bin_dir": result.bin_dir,
        "wrappers": result.wrappers,
        "powershell_snippet": result.powershell_snippet,
        "shell_snippet": result.shell_snippet,
        "powershell_profile": result.powershell_profile,
        "shell_profile": result.shell_profile,
        "user_path": result.user_path.as_ref().map(windows_user_path_update_json),
    })
}

fn windows_user_path_update_json(result: &WindowsUserPathUpdate) -> serde_json::Value {
    serde_json::json!({
        "status": result.status,
        "bin_dir": result.bin_dir,
        "detail": result.detail,
    })
}

fn agent_integration_doctor_result_json(
    result: &AgentIntegrationDoctorResult,
) -> serde_json::Value {
    let integrations = result
        .integrations
        .iter()
        .map(|item| {
            let checks = item
                .checks
                .iter()
                .map(|check| {
                    serde_json::json!({
                        "name": check.name,
                        "ok": check.ok,
                        "detail": check.detail,
                        "fix": check.fix,
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "integration": item.kind.command_name(),
                "command": item.command,
                "executable": item.executable,
                "status": item.status,
                "install_hint": item.install_hint,
                "checks": checks,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "base_dir": result.base_dir,
        "bin_dir": result.bin_dir,
        "bin_dir_on_path": result.bin_dir_on_path,
        "wsl_distribution": result.wsl_distribution,
        "integrations": integrations,
    })
}

#[cfg(unix)]
fn set_executable_if_supported(path: &Path) -> Result<(), CliError> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).map_err(CliError::Io)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(CliError::Io)
}

#[cfg(not(unix))]
fn set_executable_if_supported(_path: &Path) -> Result<(), CliError> {
    Ok(())
}

#[cfg(unix)]
fn try_link_dir(source: &Path, destination: &Path) {
    if source.is_dir() && !destination.exists() {
        let _ = std::os::unix::fs::symlink(source, destination);
    }
}

#[cfg(windows)]
fn try_link_dir(source: &Path, destination: &Path) {
    if source.is_dir() && !destination.exists() {
        let _ = std::os::windows::fs::symlink_dir(source, destination);
    }
}

#[cfg(not(any(unix, windows)))]
fn try_link_dir(_source: &Path, _destination: &Path) {}

fn cmux_split_axis(direction: &str) -> Result<&'static str, CliError> {
    match direction {
        "left" | "right" => Ok("horizontal"),
        "up" | "down" => Ok("vertical"),
        other => Err(CliError::InvalidArgs(format!(
            "new-split direction must be left, right, up, or down; got '{other}'."
        ))),
    }
}

fn invoke_control<T>(
    method: &str,
    params: &T,
    options: &ControlInvokeOptions,
) -> Result<ResponseEnvelope, CliError>
where
    T: serde::Serialize,
{
    let params_json = serde_json::to_string(params)
        .map_err(|error| CliError::Control(format!("failed to encode params: {error}")))?;
    let token = resolve_control_token(options)?;
    let response = agentmux_ipc::send_named_pipe_request(
        &options.pipe_name,
        &request(
            &format!("cli_{}", method.replace('.', "_")),
            method,
            &params_json,
            &token,
        ),
        Duration::from_secs(5),
    )
    .map_err(|error| {
        CliError::Control(format!(
            "failed to reach AgentMux control pipe '{}': {error}",
            options.pipe_name
        ))
    })?;
    Ok(response)
}

fn subscribe_control_events(
    params: &EventSubscribeParams,
    options: &ControlInvokeOptions,
) -> Result<(ResponseEnvelope, NamedPipeEventStream), CliError> {
    let params_json = serde_json::to_string(params)
        .map_err(|error| CliError::Control(format!("failed to encode params: {error}")))?;
    let token = resolve_control_token(options)?;
    agentmux_ipc::subscribe_named_pipe_events(
        &options.pipe_name,
        &request(
            "cli_events_subscribe",
            "events.subscribe",
            &params_json,
            &token,
        ),
        Duration::from_secs(5),
    )
    .map_err(|error| {
        CliError::Control(format!(
            "failed to subscribe to AgentMux control pipe '{}': {error}",
            options.pipe_name
        ))
    })
}

fn resolve_control_token(options: &ControlInvokeOptions) -> Result<String, CliError> {
    if let Some(token) = &options.token {
        return Ok(token.clone());
    }

    let path = match &options.token_path {
        Some(path) => std::path::PathBuf::from(path),
        None => default_control_token_path().map_err(|error| {
            CliError::Control(format!(
                "failed to resolve AgentMux control token path: {error}"
            ))
        })?,
    };
    read_control_token(&path).map_err(|error| {
        CliError::Control(format!(
            "failed to read AgentMux control token '{}': {error}",
            path.display()
        ))
    })
}

fn write_json_response<W>(response: &ResponseEnvelope, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    writeln!(
        output,
        "{}",
        serde_json::to_string_pretty(response)
            .map_err(|error| CliError::Control(format!("failed to encode response: {error}")))?
    )?;
    Ok(())
}

fn write_json_value<W>(value: &serde_json::Value, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    writeln!(
        output,
        "{}",
        serde_json::to_string_pretty(value)
            .map_err(|error| CliError::Control(format!("failed to encode json: {error}")))?
    )?;
    Ok(())
}

fn response_result<T>(response: &ResponseEnvelope) -> Result<T, CliError>
where
    T: serde::de::DeserializeOwned,
{
    let result_json = match &response.outcome {
        ResponseOutcome::Ok { result_json } => result_json,
        ResponseOutcome::Error(error) => {
            return Err(CliError::Control(format!(
                "{}: {}",
                error.code.as_str(),
                error.message
            )))
        }
    };
    serde_json::from_str(result_json)
        .map_err(|error| CliError::Control(format!("invalid response json: {error}")))
}

fn load_workspace_detail(
    invoke: &ControlInvokeOptions,
    workspace_id: &str,
) -> Result<WorkspaceDetailResult, CliError> {
    let response = invoke_control(
        "workspace.get",
        &WorkspaceIdParams {
            workspace_id: workspace_id.to_string(),
        },
        invoke,
    )?;
    response_result(&response)
}

fn load_tmux_list_workspace_details(
    invoke: &ControlInvokeOptions,
    workspace_id: Option<String>,
    all_workspaces: bool,
) -> Result<Vec<WorkspaceDetailResult>, CliError> {
    if all_workspaces {
        let response = invoke_control("workspace.list", &serde_json::json!({}), invoke)?;
        let result: WorkspaceListResult = response_result(&response)?;
        return result
            .workspaces
            .iter()
            .map(|workspace| load_workspace_detail(invoke, &workspace.workspace_id))
            .collect();
    }

    let context = identify_context(invoke, workspace_id)?;
    let workspace_id = require_context_field(context.workspace_id, "workspace")?;
    Ok(vec![load_workspace_detail(invoke, &workspace_id)?])
}

fn tmux_active_workspace_and_pane(
    invoke: &ControlInvokeOptions,
    workspace_id: Option<String>,
) -> (Option<String>, Option<String>) {
    identify_context(invoke, workspace_id)
        .map(|context| (context.workspace_id, pane_from_env().or(context.pane_id)))
        .unwrap_or((None, None))
}

fn resolve_tmux_session_workspace_id(
    invoke: &ControlInvokeOptions,
    current_workspace_id: &str,
    target_session: Option<&str>,
) -> Result<String, CliError> {
    let Some(target_session) = target_session
        .map(normalize_tmux_session_target)
        .filter(|value| !value.is_empty() && value != ".")
    else {
        return Ok(current_workspace_id.to_string());
    };

    if target_session == current_workspace_id {
        return Ok(current_workspace_id.to_string());
    }

    if let Ok(detail) = load_workspace_detail(invoke, current_workspace_id) {
        if detail.workspace.name == target_session {
            return Ok(current_workspace_id.to_string());
        }
    }

    let response = invoke_control("workspace.list", &serde_json::json!({}), invoke)?;
    let result: WorkspaceListResult = response_result(&response)?;
    result
        .workspaces
        .into_iter()
        .find(|workspace| {
            workspace.workspace_id == target_session || workspace.name == target_session
        })
        .map(|workspace| workspace.workspace_id)
        .ok_or_else(|| {
            CliError::Control(format!(
                "Could not resolve tmux session target '{target_session}' in AgentMux."
            ))
        })
}

fn resolve_tmux_workspace_and_pane(
    invoke: &ControlInvokeOptions,
    workspace_id: Option<String>,
    target_pane_id: Option<String>,
) -> Result<(String, String), CliError> {
    let context = identify_context(invoke, workspace_id)?;
    let current_workspace_id = require_context_field(context.workspace_id, "workspace")?;
    let active_pane_id = pane_from_env().or(context.pane_id);
    let target = target_pane_id
        .as_deref()
        .map(split_tmux_session_window_pane_target)
        .unwrap_or_default();
    let workspace_id = resolve_tmux_session_workspace_id(
        invoke,
        &current_workspace_id,
        target.session.as_deref(),
    )?;
    let detail = load_workspace_detail(invoke, &workspace_id)?;
    let default_active_pane_id = if workspace_id == current_workspace_id {
        active_pane_id
            .as_deref()
            .unwrap_or(&detail.workspace.active_pane_id)
    } else {
        &detail.workspace.active_pane_id
    };
    let pane_id = resolve_tmux_pane_id_in_detail(
        &detail,
        target.window.as_deref(),
        target.pane.as_deref(),
        default_active_pane_id,
    )?;
    Ok((workspace_id, pane_id))
}

fn resolve_tmux_pane_id_in_detail(
    detail: &WorkspaceDetailResult,
    target_window: Option<&str>,
    target_pane: Option<&str>,
    active_pane_id: &str,
) -> Result<String, CliError> {
    let root_pane_id = match target_window {
        Some(window) => resolve_tmux_window_root_id(detail, Some(window))?,
        None => root_pane_id_for_detail_pane(detail, active_pane_id)
            .unwrap_or_else(|| detail.workspace.root_pane_id.clone()),
    };
    let mut leaves = Vec::new();
    collect_leaf_panes(detail, &root_pane_id, &mut leaves);
    if leaves.is_empty() {
        return Err(CliError::Control(format!(
            "Window '{}' does not contain a focusable AgentMux pane.",
            root_pane_id
        )));
    }

    let Some(target_pane) = target_pane.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(active_or_first_tmux_leaf_pane(&leaves, active_pane_id));
    };
    if target_pane == "." || target_pane == "!" {
        return Ok(active_or_first_tmux_leaf_pane(&leaves, active_pane_id));
    }

    let pane_id = normalize_tmux_pane_id(target_pane);
    if detail.panes.iter().any(|pane| pane.pane_id == pane_id) {
        if leaves.iter().any(|pane| pane.pane_id == pane_id) {
            return Ok(pane_id);
        }
        if let Some(first_leaf) = first_leaf_id_in_detail(detail, &pane_id) {
            if target_window.is_none() || leaves.iter().any(|pane| pane.pane_id == first_leaf) {
                return Ok(first_leaf);
            }
        }
    }

    if let Ok(index) = pane_id.parse::<usize>() {
        return leaves
            .get(index)
            .map(|pane| pane.pane_id.clone())
            .ok_or_else(|| {
                CliError::Control(format!("No AgentMux pane exists at index {index}."))
            });
    }

    Err(CliError::Control(format!(
        "Could not resolve tmux pane target '{target_pane}' in AgentMux."
    )))
}

fn active_or_first_tmux_leaf_pane(leaves: &[&PaneSummaryResult], active_pane_id: &str) -> String {
    if let Some(active) = leaves.iter().find(|pane| pane.pane_id == active_pane_id) {
        return active.pane_id.clone();
    }
    leaves
        .first()
        .map(|pane| pane.pane_id.clone())
        .unwrap_or_else(|| active_pane_id.to_string())
}

fn session_id_for_pane(detail: &WorkspaceDetailResult, pane_id: &str) -> Option<String> {
    let surface_id = detail
        .panes
        .iter()
        .find(|pane| pane.pane_id == pane_id)
        .and_then(|pane| pane.mounted_surface_id.as_deref())?;
    detail
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == surface_id)
        .and_then(|surface| surface.session_id.clone())
}

fn pane_id_for_session(detail: &WorkspaceDetailResult, session_id: &str) -> Option<String> {
    let surface_id = detail
        .surfaces
        .iter()
        .find(|surface| surface.session_id.as_deref() == Some(session_id))
        .map(|surface| surface.surface_id.as_str())?;
    detail
        .panes
        .iter()
        .find(|pane| pane.mounted_surface_id.as_deref() == Some(surface_id))
        .map(|pane| pane.pane_id.clone())
}

fn split_child_pane_id(detail: &WorkspaceDetailResult, parent_pane_id: &str) -> Option<String> {
    let mut fallback = None;
    for pane in detail
        .panes
        .iter()
        .filter(|pane| pane.parent_pane_id.as_deref() == Some(parent_pane_id))
    {
        if pane.mounted_surface_id.is_none() {
            return Some(pane.pane_id.clone());
        }
        fallback = Some(pane.pane_id.clone());
    }
    fallback
}

fn first_leaf_id_in_detail(detail: &WorkspaceDetailResult, pane_id: &str) -> Option<String> {
    let pane = detail.panes.iter().find(|pane| pane.pane_id == pane_id)?;
    if pane.kind == "leaf" {
        return Some(pane.pane_id.clone());
    }
    detail
        .panes
        .iter()
        .filter(|candidate| candidate.parent_pane_id.as_deref() == Some(pane_id))
        .find_map(|child| first_leaf_id_in_detail(detail, &child.pane_id))
}

fn root_pane_id_for_detail_pane(detail: &WorkspaceDetailResult, pane_id: &str) -> Option<String> {
    let mut pane = detail.panes.iter().find(|pane| pane.pane_id == pane_id)?;
    let mut guard = 0usize;
    while let Some(parent_pane_id) = pane.parent_pane_id.as_deref() {
        pane = detail
            .panes
            .iter()
            .find(|candidate| candidate.pane_id == parent_pane_id)?;
        guard += 1;
        if guard > 100 {
            return None;
        }
    }
    Some(pane.pane_id.clone())
}

fn resolve_tmux_window_root_id(
    detail: &WorkspaceDetailResult,
    target_window: Option<&str>,
) -> Result<String, CliError> {
    let roots = detail
        .panes
        .iter()
        .filter(|pane| pane.parent_pane_id.is_none())
        .collect::<Vec<_>>();
    if roots.is_empty() {
        return Err(CliError::Control(
            "Workspace does not have any AgentMux windows.".to_string(),
        ));
    }
    let active_root = root_pane_id_for_detail_pane(detail, &detail.workspace.active_pane_id)
        .unwrap_or_else(|| detail.workspace.root_pane_id.clone());

    let Some(target_window) = target_window else {
        return Ok(active_root);
    };
    let target = normalize_tmux_window_target(target_window);
    if target.is_empty() {
        return Ok(active_root);
    }
    if target == "." || target == "!" {
        return Ok(active_root);
    }
    if target == detail.workspace.name {
        return Ok(active_root);
    }
    if let Ok(index) = target.parse::<usize>() {
        return roots
            .get(index)
            .map(|pane| pane.pane_id.clone())
            .ok_or_else(|| {
                CliError::Control(format!("No AgentMux window exists at index {index}."))
            });
    }
    let pane_target = normalize_tmux_pane_id(&target);
    if roots.iter().any(|pane| pane.pane_id == pane_target) {
        return Ok(pane_target);
    }
    if let Some(root_id) = root_pane_id_for_detail_pane(detail, &pane_target) {
        return Ok(root_id);
    }
    Err(CliError::Control(format!(
        "Could not resolve tmux window target '{target_window}' in AgentMux."
    )))
}

fn pane_subtree_ids_in_detail(detail: &WorkspaceDetailResult, pane_id: &str) -> Vec<String> {
    let mut ids = vec![pane_id.to_string()];
    let children = detail
        .panes
        .iter()
        .filter(|pane| pane.parent_pane_id.as_deref() == Some(pane_id))
        .map(|pane| pane.pane_id.clone())
        .collect::<Vec<_>>();
    for child_id in children {
        ids.extend(pane_subtree_ids_in_detail(detail, &child_id));
    }
    ids
}

fn surface_id_for_window_root(
    detail: &WorkspaceDetailResult,
    root_pane_id: &str,
) -> Option<String> {
    let pane_ids = pane_subtree_ids_in_detail(detail, root_pane_id);
    detail
        .panes
        .iter()
        .filter(|pane| pane_ids.contains(&pane.pane_id))
        .find_map(|pane| pane.mounted_surface_id.clone())
}

struct TmuxWindowRow {
    session_id: String,
    session_name: String,
    root_pane_id: String,
    window_index: usize,
    window_name: String,
    active: bool,
}

struct TmuxPaneRow<'a> {
    pane: &'a PaneSummaryResult,
    session_id: String,
    session_name: String,
    window_id: String,
    window_index: usize,
    window_name: String,
    pane_index: usize,
}

#[derive(Clone, Copy, Default)]
struct TmuxPaneFormatValues<'a> {
    pane_index: Option<usize>,
    window_index: Option<usize>,
    window_id: Option<&'a str>,
    window_name: Option<&'a str>,
    session_name: Option<&'a str>,
}

fn tmux_window_rows(
    detail: &WorkspaceDetailResult,
    active_workspace_id: Option<&str>,
) -> Vec<TmuxWindowRow> {
    let active_root = root_pane_id_for_detail_pane(detail, &detail.workspace.active_pane_id)
        .unwrap_or_else(|| detail.workspace.root_pane_id.clone());
    tmux_root_panes(detail)
        .into_iter()
        .enumerate()
        .map(|(window_index, pane)| TmuxWindowRow {
            session_id: detail.workspace.workspace_id.clone(),
            session_name: detail.workspace.name.clone(),
            root_pane_id: pane.pane_id.clone(),
            window_index,
            window_name: detail.workspace.name.clone(),
            active: active_workspace_id == Some(detail.workspace.workspace_id.as_str())
                && pane.pane_id == active_root,
        })
        .collect()
}

fn tmux_pane_rows<'a>(
    detail: &'a WorkspaceDetailResult,
    _active_pane_id: Option<&str>,
) -> Vec<TmuxPaneRow<'a>> {
    let mut rows = Vec::new();
    for (window_index, root) in tmux_root_panes(detail).into_iter().enumerate() {
        let mut leaves = Vec::new();
        collect_leaf_panes(detail, &root.pane_id, &mut leaves);
        for (pane_index, pane) in leaves.into_iter().enumerate() {
            rows.push(TmuxPaneRow {
                pane,
                session_id: detail.workspace.workspace_id.clone(),
                session_name: detail.workspace.name.clone(),
                window_id: root.pane_id.clone(),
                window_index,
                window_name: detail.workspace.name.clone(),
                pane_index,
            });
        }
    }
    rows
}

fn tmux_root_panes(detail: &WorkspaceDetailResult) -> Vec<&PaneSummaryResult> {
    detail
        .panes
        .iter()
        .filter(|pane| pane.parent_pane_id.is_none())
        .collect()
}

fn collect_leaf_panes<'a>(
    detail: &'a WorkspaceDetailResult,
    pane_id: &str,
    leaves: &mut Vec<&'a PaneSummaryResult>,
) {
    let Some(pane) = detail.panes.iter().find(|pane| pane.pane_id == pane_id) else {
        return;
    };
    if pane.kind == "leaf" {
        leaves.push(pane);
        return;
    }
    let children = detail
        .panes
        .iter()
        .filter(|candidate| candidate.parent_pane_id.as_deref() == Some(pane_id))
        .map(|pane| pane.pane_id.clone())
        .collect::<Vec<_>>();
    for child in children {
        collect_leaf_panes(detail, &child, leaves);
    }
}

fn render_tmux_format(
    format: Option<&str>,
    context: &SystemIdentifyResult,
    pane_id: &str,
) -> String {
    render_tmux_format_with_values(format, context, pane_id, TmuxPaneFormatValues::default())
}

fn render_tmux_pane_format(
    format: Option<&str>,
    context: &SystemIdentifyResult,
    row: &TmuxPaneRow<'_>,
) -> String {
    render_tmux_format_with_values(
        format,
        context,
        &row.pane.pane_id,
        TmuxPaneFormatValues {
            pane_index: Some(row.pane_index),
            window_index: Some(row.window_index),
            window_id: Some(&row.window_id),
            window_name: Some(&row.window_name),
            session_name: Some(&row.session_name),
        },
    )
}

fn render_tmux_format_with_values(
    format: Option<&str>,
    context: &SystemIdentifyResult,
    pane_id: &str,
    values: TmuxPaneFormatValues<'_>,
) -> String {
    let tmux_pane_id = agentmux_pane_to_tmux_pane(pane_id);
    let template = format.unwrap_or("#{pane_id}");
    let session_name = values
        .session_name
        .or(context.workspace_id.as_deref())
        .unwrap_or("agentmux");
    let window_name = values
        .window_name
        .or(context.workspace_id.as_deref())
        .unwrap_or("agentmux");
    template
        .replace("#{pane_id}", &tmux_pane_id)
        .replace(
            "#{pane_index}",
            &values
                .pane_index
                .map(|index| index.to_string())
                .unwrap_or_default(),
        )
        .replace(
            "#{pane_active}",
            if context.pane_id.as_deref() == Some(pane_id) {
                "1"
            } else {
                "0"
            },
        )
        .replace("#{pane_current_path}", context.cwd.as_deref().unwrap_or(""))
        .replace(
            "#{session_id}",
            context.workspace_id.as_deref().unwrap_or(""),
        )
        .replace("#{session_name}", session_name)
        .replace(
            "#{window_index}",
            &values
                .window_index
                .map(|index| index.to_string())
                .unwrap_or_default(),
        )
        .replace("#{window_id}", values.window_id.unwrap_or(""))
        .replace("#{window_name}", window_name)
}

#[cfg(test)]
fn render_tmux_window_format(
    format: Option<&str>,
    detail: &WorkspaceDetailResult,
    root_pane_id: &str,
    index: usize,
) -> String {
    let template = format.unwrap_or("#{window_index}: #{window_name}");
    template
        .replace("#{window_index}", &index.to_string())
        .replace("#{window_id}", root_pane_id)
        .replace("#{window_name}", &detail.workspace.name)
        .replace("#{session_id}", &detail.workspace.workspace_id)
        .replace("#{session_name}", &detail.workspace.name)
        .replace(
            "#{window_active}",
            if detail.workspace.root_pane_id == root_pane_id {
                "1"
            } else {
                "0"
            },
        )
}

fn render_tmux_window_format_from_row(format: Option<&str>, row: &TmuxWindowRow) -> String {
    let template = format.unwrap_or("#{window_index}: #{window_name}");
    template
        .replace("#{window_index}", &row.window_index.to_string())
        .replace("#{window_id}", &row.root_pane_id)
        .replace("#{window_name}", &row.window_name)
        .replace("#{session_id}", &row.session_id)
        .replace("#{session_name}", &row.session_name)
        .replace("#{window_active}", if row.active { "1" } else { "0" })
}

fn render_tmux_session_format(
    format: Option<&str>,
    workspace: &WorkspaceSummaryResult,
    window_count: usize,
    attached: bool,
) -> String {
    let template = format.unwrap_or("#{session_name}: #{session_windows} windows");
    template
        .replace("#{session_id}", &workspace.workspace_id)
        .replace("#{session_name}", &workspace.name)
        .replace("#{session_windows}", &window_count.to_string())
        .replace("#{session_attached}", if attached { "1" } else { "0" })
}

fn tmux_session_window_count(detail: &WorkspaceDetailResult) -> usize {
    detail
        .panes
        .iter()
        .filter(|pane| pane.parent_pane_id.is_none())
        .count()
}

fn tmux_key_to_agentmux_key(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "enter" | "return" | "c-m" => Some("enter"),
        "tab" | "c-i" => Some("tab"),
        "escape" | "esc" => Some("escape"),
        "backspace" | "bspace" | "bs" | "c-h" => Some("backspace"),
        "delete" | "dc" => Some("delete"),
        "up" | "up-arrow" => Some("up"),
        "down" | "down-arrow" => Some("down"),
        "left" | "left-arrow" => Some("left"),
        "right" | "right-arrow" => Some("right"),
        _ => None,
    }
}

fn flush_tmux_text_buffer(
    invoke: &ControlInvokeOptions,
    session_id: &str,
    text_buffer: &mut Vec<String>,
) -> Result<(), CliError> {
    if text_buffer.is_empty() {
        return Ok(());
    }
    let text = text_buffer.join(" ");
    text_buffer.clear();
    invoke_control(
        "session.send_text",
        &SessionSendTextParams {
            session_id: session_id.to_string(),
            text,
        },
        invoke,
    )?;
    Ok(())
}

fn write_workspace_detail<W>(detail: &WorkspaceDetailResult, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    writeln!(
        output,
        "{}\t{}",
        detail.workspace.workspace_id, detail.workspace.name
    )?;
    writeln!(
        output,
        "project\t{}",
        detail.workspace.project_root.as_deref().unwrap_or("-")
    )?;
    writeln!(output, "panes\t{}", detail.panes.len())?;
    writeln!(output, "surfaces\t{}", detail.surfaces.len())?;
    writeln!(output, "sessions\t{}", detail.sessions.len())?;
    for session in &detail.sessions {
        write_session_summary(session, output)?;
    }
    Ok(())
}

fn write_app_config<W>(config: &AppConfigResult, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    writeln!(output, "path\t{}", config.config_path)?;
    writeln!(
        output,
        "project\t{}\tloaded={}",
        config.project_config_path.as_deref().unwrap_or("-"),
        config.project_config_loaded
    )?;
    writeln!(output, "format\t{}", config.format_version)?;
    writeln!(
        output,
        "appearance\t{}\t{}\t{}",
        config.appearance.theme, config.appearance.accent_key, config.appearance.font_size
    )?;
    writeln!(output, "shortcuts\t{}", config.shortcuts.bindings.len())?;
    for (action_id, binding) in &config.shortcuts.bindings {
        let binding_json = serde_json::to_string(binding)
            .map_err(|error| CliError::Control(format!("failed to encode binding: {error}")))?;
        writeln!(output, "shortcut\t{action_id}\t{binding_json}")?;
    }
    writeln!(output, "actions\t{}", config.actions.custom.len())?;
    for action in &config.actions.custom {
        writeln!(
            output,
            "action\t{}\t{}\t{}",
            action.id, action.target, action.title
        )?;
    }
    Ok(())
}

fn write_session_summary<W>(session: &SessionSummaryResult, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    writeln!(
        output,
        "{}\t{}\t{}\t{}",
        session.session_id, session.workspace_id, session.backend_kind, session.state
    )?;
    Ok(())
}

fn write_agent_state<W>(state: &AgentStateResult, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    writeln!(
        output,
        "{}\t{}\t{}\tattention={}\t{}",
        state.session_id,
        state.workspace_id,
        state.state,
        state.attention,
        state.reason.as_deref().unwrap_or("-")
    )?;
    Ok(())
}

fn write_notification<W>(
    notification: &NotificationSummaryResult,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    writeln!(
        output,
        "{}\t{}\t{}\t{}\t{}\t{}",
        notification.notification_id,
        notification.severity,
        notification.notification_type,
        notification.workspace_id.as_deref().unwrap_or("-"),
        notification.session_id.as_deref().unwrap_or("-"),
        notification.message
    )?;
    Ok(())
}

fn write_sidebar_log<W>(
    log: &agentmux_ipc::SidebarLogResult,
    output: &mut W,
) -> Result<(), CliError>
where
    W: Write,
{
    writeln!(
        output,
        "{}\t{}\t{}\t{}",
        log.workspace_id,
        log.level,
        log.source.as_deref().unwrap_or("-"),
        log.message
    )?;
    Ok(())
}

fn write_event_frames<W>(events: &[EventFrame], output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    for event in events {
        writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}",
            event.event_id,
            event.event_type,
            event.workspace_id.as_deref().unwrap_or("-"),
            event.session_id.as_deref().unwrap_or("-"),
            event.data_json
        )?;
    }
    Ok(())
}

fn write_event_frame_json<W>(event: &EventFrame, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let json = serde_json::to_string(event)
        .map_err(|error| CliError::Control(format!("failed to encode event frame: {error}")))?;
    writeln!(output, "{json}")?;
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TerminalBackendOption {
    Conpty,
    WslDirect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TerminalRunOptions {
    backend: TerminalBackendOption,
    distribution: Option<String>,
    cwd: Option<String>,
    command: Vec<String>,
}

#[derive(Debug)]
pub enum CliError {
    Io(io::Error),
    InvalidArgs(String),
    Control(String),
    Timeout(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Io(error) => write!(f, "{error}"),
            CliError::InvalidArgs(message)
            | CliError::Control(message)
            | CliError::Timeout(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CliError {}

impl From<io::Error> for CliError {
    fn from(value: io::Error) -> Self {
        CliError::Io(value)
    }
}

fn parse_terminal_run_options(args: &[String]) -> Result<TerminalRunOptions, CliError> {
    let mut backend = TerminalBackendOption::Conpty;
    let mut distribution = None;
    let mut cwd = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--" => {
                index += 1;
                break;
            }
            "--backend" => {
                let value = option_value(args, index, "--backend")?;
                backend = parse_backend_option(value)?;
                index += 2;
            }
            "--distribution" => {
                distribution = Some(option_value(args, index, "--distribution")?.to_string());
                index += 2;
            }
            "--cwd" => {
                cwd = Some(option_value(args, index, "--cwd")?.to_string());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown terminal run option '{value}'."
                )));
            }
            _ => break,
        }
    }

    let command = args[index..].to_vec();
    if command.is_empty() {
        return Err(CliError::InvalidArgs(
            "terminal run requires a command after '--'.".to_string(),
        ));
    }

    if distribution.is_some() && backend != TerminalBackendOption::WslDirect {
        return Err(CliError::InvalidArgs(
            "--distribution requires --backend wsl-direct.".to_string(),
        ));
    }

    Ok(TerminalRunOptions {
        backend,
        distribution,
        cwd,
        command,
    })
}

fn option_value<'a>(args: &'a [String], index: usize, option: &str) -> Result<&'a str, CliError> {
    args.get(index + 1)
        .filter(|value| !value.is_empty())
        .map(String::as_str)
        .ok_or_else(|| CliError::InvalidArgs(format!("{option} requires a value.")))
}

fn parse_backend_option(value: &str) -> Result<TerminalBackendOption, CliError> {
    match value {
        "conpty" => Ok(TerminalBackendOption::Conpty),
        "wsl-direct" => Ok(TerminalBackendOption::WslDirect),
        other => Err(CliError::InvalidArgs(format!(
            "unsupported terminal backend '{other}'."
        ))),
    }
}

fn run_terminal_command<W>(options: TerminalRunOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    match options.backend {
        TerminalBackendOption::Conpty => {
            run_terminal_command_with_backend(ConptyBackend::new(), options, output)
        }
        TerminalBackendOption::WslDirect => {
            let config = match options.distribution.as_deref() {
                Some(distribution) => WslDirectConfig::for_distribution(distribution),
                None => WslDirectConfig::default(),
            };
            run_terminal_command_with_backend(
                WslDirectBackend::with_config(config),
                options,
                output,
            )
        }
    }
}

fn run_terminal_command_with_backend<B, W>(
    backend: B,
    options: TerminalRunOptions,
    output: &mut W,
) -> Result<(), CliError>
where
    B: SessionBackend,
    W: Write,
{
    let token = "cli-local-token";
    let runtime = TerminalRuntime::new(backend);
    let mut control = RuntimeControlPlane::new(runtime, token);
    let command_json = serde_json::to_string(&options.command)
        .map_err(|error| CliError::Control(format!("failed to encode command: {error}")))?;
    let cwd_json = serde_json::to_string(&options.cwd)
        .map_err(|error| CliError::Control(format!("failed to encode cwd: {error}")))?;
    let backend_json = serde_json::to_string(backend_label(&options.backend))
        .map_err(|error| CliError::Control(format!("failed to encode backend: {error}")))?;
    let backend_profile_json = serde_json::to_string(&options.distribution)
        .map_err(|error| CliError::Control(format!("failed to encode backend profile: {error}")))?;
    let spawn_params = format!(
        r#"{{"workspace_id":"ws_cli","backend":{backend_json},"backend_profile":{backend_profile_json},"command":{command_json},"cwd":{cwd_json},"columns":120,"rows":30,"durability":"ephemeral"}}"#
    );
    let spawn = control.handle_request(request("req_spawn", "session.spawn", &spawn_params, token));
    let session_id = json_field(&spawn, "session_id")?;

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last_text = String::new();

    loop {
        control.collect_events();
        last_text = read_recent_text(&mut control, &session_id, token).unwrap_or(last_text);

        let summary = control.handle_request(request(
            "req_get",
            "session.get",
            &format!(r#"{{"session_id":"{session_id}"}}"#),
            token,
        ));
        let state = json_field(&summary, "state")?;
        if state == "exited" || state == "failed" {
            let settle_deadline = Instant::now() + Duration::from_millis(250);
            while Instant::now() < settle_deadline {
                control.collect_events();
                if let Some(text) = read_recent_text(&mut control, &session_id, token) {
                    last_text = text;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            break;
        }

        if Instant::now() >= deadline {
            return Err(CliError::Timeout(format!(
                "timed out waiting for session {session_id}"
            )));
        }

        std::thread::sleep(Duration::from_millis(25));
    }

    write!(output, "{}", strip_vt_sequences(&last_text))?;
    Ok(())
}

fn backend_label(backend: &TerminalBackendOption) -> &'static str {
    match backend {
        TerminalBackendOption::Conpty => "conpty",
        TerminalBackendOption::WslDirect => "wsl-direct",
    }
}

fn strip_vt_sequences(input: &str) -> String {
    let mut output = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            output.push(ch);
            continue;
        }

        match chars.peek().copied() {
            Some('[') => {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                let mut previous_escape = false;
                for next in chars.by_ref() {
                    if next == '\x07' {
                        break;
                    }
                    if previous_escape && next == '\\' {
                        break;
                    }
                    previous_escape = next == '\x1b';
                }
            }
            Some(_) => {
                chars.next();
            }
            None => {}
        }
    }

    output.replace("\r\n", "\n")
}

fn read_recent_text<B>(
    control: &mut RuntimeControlPlane<B>,
    session_id: &str,
    token: &str,
) -> Option<String>
where
    B: agentmux_backend::SessionBackend,
{
    let recent = control.handle_request(request(
        "req_recent",
        "session.read_recent",
        &format!(r#"{{"session_id":"{session_id}","max_bytes":1048576}}"#),
        token,
    ));

    json_field(&recent, "text").ok()
}

fn request(id: &str, method: &str, params_json: &str, token: &str) -> RequestEnvelope {
    RequestEnvelope::new(id, method, params_json, token)
}

fn json_field(response: &ResponseEnvelope, field: &str) -> Result<String, CliError> {
    let result_json = match &response.outcome {
        ResponseOutcome::Ok { result_json } => result_json,
        ResponseOutcome::Error(error) => {
            return Err(CliError::Control(format!(
                "{}: {}",
                error.code.as_str(),
                error.message
            )))
        }
    };

    let value = serde_json::from_str::<serde_json::Value>(result_json)
        .map_err(|error| CliError::Control(format!("invalid response json: {error}")))?;
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| CliError::Control(format!("missing string field '{field}'")))
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn usage_mentions_workspace_and_session() {
        let text = usage();
        assert!(text.contains("workspace"));
        assert!(text.contains("session"));
        assert!(text.contains("server"));
        assert!(text.contains("wsl-direct"));
        assert!(text.contains("diagnostics"));
        assert!(text.contains("config reload"));
        assert!(text.contains("config schema"));
        assert!(text.contains("actions list"));
        assert!(text.contains("notify"));
        assert!(text.contains("sidebar-state"));
    }

    #[test]
    fn server_options_default_to_loopback_conpty_local() {
        let options = parse_server_options(&[
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--port".to_string(),
            "0".to_string(),
        ])
        .unwrap();

        assert_eq!(options.host, "127.0.0.1");
        assert_eq!(options.port, 0);
        assert_eq!(options.mode, ServerMode::Local);
        assert!(!options.allow_remote);
        assert_eq!(options.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(options.backend.as_deref(), Some("conpty"));
        assert_eq!(
            options.command,
            vec!["powershell.exe".to_string(), "-NoLogo".to_string()]
        );
    }

    #[test]
    fn server_options_parse_explicit_command_and_remote_guard() {
        let blocked = parse_server_options(&["--host".to_string(), "0.0.0.0".to_string()]);
        assert!(blocked.is_err());

        let options = parse_server_options(&[
            "start".to_string(),
            "--host".to_string(),
            "0.0.0.0".to_string(),
            "--allow-remote".to_string(),
            "--backend".to_string(),
            "conpty".to_string(),
            "--".to_string(),
            "powershell.exe".to_string(),
            "-NoLogo".to_string(),
        ])
        .unwrap();
        assert_eq!(options.host, "0.0.0.0");
        assert_eq!(options.mode, ServerMode::Local);
        assert!(options.allow_remote);
        assert_eq!(options.backend.as_deref(), Some("conpty"));
        assert_eq!(
            options.command,
            vec!["powershell.exe".to_string(), "-NoLogo".to_string()]
        );
    }

    #[test]
    fn server_options_accept_desktop_bridge_mode() {
        let options = parse_server_options(&[
            "--mode".to_string(),
            "desktop-bridge".to_string(),
            "--workspace".to_string(),
            "ws_1".to_string(),
        ])
        .unwrap();

        assert_eq!(options.mode, ServerMode::DesktopBridge);
        assert_eq!(options.workspace_id.as_deref(), Some("ws_1"));
    }

    #[test]
    fn server_helpers_parse_command_line_and_session_routes() {
        assert_eq!(
            split_command_line(r#"bash -lc "echo hello world""#).unwrap(),
            vec![
                "bash".to_string(),
                "-lc".to_string(),
                "echo hello world".to_string()
            ]
        );
        assert_eq!(
            session_id_from_path("/api/session/sess_1/recent", "/recent").as_deref(),
            Some("sess_1")
        );
        assert_eq!(
            session_id_from_path("/api/session/sess_1/snapshot", "/snapshot").as_deref(),
            Some("sess_1")
        );
        assert_eq!(
            session_id_from_path("/api/session/sess_1/stream", "/stream").as_deref(),
            Some("sess_1")
        );
        assert_eq!(
            session_id_from_path("/api/session/sess%201/send", "/send").as_deref(),
            Some("sess 1")
        );
        assert_eq!(
            local_server_session_id(&request(
                "req_snapshot",
                "session.snapshot",
                r#"{"session_id":"sess_1","since_offset":0}"#,
                SERVER_LOCAL_TOKEN,
            ))
            .unwrap(),
            "sess_1"
        );
        assert_eq!(
            local_server_session_id(&request(
                "req_pressure",
                "session.report_output_pressure",
                r#"{"session_id":"sess_1","queued_bytes":1048576,"max_queued_bytes":1048576,"backpressure_events":1,"write_in_flight":true}"#,
                SERVER_LOCAL_TOKEN,
            ))
            .unwrap(),
            "sess_1"
        );
    }

    #[test]
    fn local_server_routes_explicit_wsl_spawn_even_when_default_is_conpty() {
        assert_eq!(
            local_server_backend_for_default(Some("conpty")),
            LocalServerBackend::Conpty
        );
        assert_eq!(
            local_server_backend_for_spawn(Some("wsl-direct"), LocalServerBackend::Conpty).unwrap(),
            LocalServerBackend::WslDirect
        );
        assert_eq!(
            local_server_backend_for_spawn(None, LocalServerBackend::Conpty).unwrap(),
            LocalServerBackend::Conpty
        );
        assert!(local_server_backend_for_spawn(
            Some("wsl-tmux-control"),
            LocalServerBackend::Conpty
        )
        .is_err());
    }

    #[test]
    fn usage_mentions_cmux_compat_aliases() {
        let text = usage_for("cmux");
        assert!(text.contains("cmux <"));
        assert!(text.contains("cmux list-workspaces"));
        assert!(text.contains("cmux current-workspace"));
        assert!(text.contains("cmux ping"));
    }

    #[test]
    fn unknown_command_uses_requested_program_name() {
        let mut output = Vec::new();
        run_cli_with_program("cmux", ["not-a-command"], &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.starts_with("cmux <"));
    }

    #[test]
    fn common_control_options_accept_cmux_socket_flag() {
        let options = parse_no_params_control_options(
            &[
                "--socket".to_string(),
                r"\\.\pipe\agentmux-cmux-test".to_string(),
                "--json".to_string(),
            ],
            "ping",
        )
        .unwrap();

        assert!(options.json);
        assert_eq!(options.pipe_name, r"\\.\pipe\agentmux-cmux-test");
    }

    #[test]
    fn cmux_workspace_aliases_parse_doc_shapes() {
        let create = parse_cmux_new_workspace_options(&[
            "--json".to_string(),
            "--project".to_string(),
            r"D:\work\repo".to_string(),
            "--distribution".to_string(),
            "Ubuntu".to_string(),
        ])
        .unwrap();
        assert!(create.invoke.json);
        assert_eq!(create.params.name, "Workspace");
        assert_eq!(create.params.project_root.as_deref(), Some(r"D:\work\repo"));
        assert_eq!(create.params.backend_profile.as_deref(), Some("Ubuntu"));

        let close = parse_cmux_workspace_close_options(&[
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--policy".to_string(),
            "detach_sessions".to_string(),
        ])
        .unwrap();
        assert_eq!(close.params.workspace_id, "ws_1");
        assert_eq!(close.params.close_policy, "detach_sessions");
    }

    #[test]
    fn cmux_split_and_active_send_parse_current_context_options() {
        let split = parse_cmux_pane_split_options(&[
            "right".to_string(),
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--ratio".to_string(),
            "0.4".to_string(),
        ])
        .unwrap();
        assert_eq!(split.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(cmux_split_axis(&split.direction).unwrap(), "horizontal");
        assert_eq!(split.ratio, Some(0.4));

        let send = parse_cmux_active_send_text_options(&[
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--".to_string(),
            "echo".to_string(),
            "hello".to_string(),
        ])
        .unwrap();
        assert_eq!(send.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(send.text, "echo hello");

        let key = parse_cmux_active_send_key_options(&[
            "enter".to_string(),
            "--workspace".to_string(),
            "ws_1".to_string(),
        ])
        .unwrap();
        assert_eq!(key.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(key.key, "enter");
    }

    #[test]
    fn agent_integration_setup_creates_tmux_shims_and_shadow_config() {
        let _env_lock = ENV_LOCK.lock().unwrap();
        let base_dir = std::env::temp_dir().join(format!(
            "agentmux-cli-integration-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source_dir = base_dir.join("source-opencode");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(
            source_dir.join("opencode.json"),
            r#"{"plugin":["existing-plugin"]}"#,
        )
        .unwrap();
        std::fs::write(
            source_dir.join("oh-my-opencode.json"),
            r#"{"tmux":{"enabled":false}}"#,
        )
        .unwrap();

        let previous = std::env::var_os("OPENCODE_CONFIG_DIR");
        std::env::set_var("OPENCODE_CONFIG_DIR", &source_dir);
        let runtime =
            setup_agent_integration_files(AgentIntegrationKind::Omo, &base_dir, Vec::new())
                .unwrap();
        if let Some(previous) = previous {
            std::env::set_var("OPENCODE_CONFIG_DIR", previous);
        } else {
            std::env::remove_var("OPENCODE_CONFIG_DIR");
        }

        assert_eq!(runtime.command, "opencode");
        assert!(runtime.shim_dir.join("tmux").is_file());
        assert!(runtime.shim_dir.join("tmux.cmd").is_file());
        let tmux_shim = std::fs::read_to_string(runtime.shim_dir.join("tmux")).unwrap();
        assert!(tmux_shim.contains("CMUX_EXE"));
        assert!(tmux_shim.contains("exec \"$CMUX_EXE\" __tmux-compat \"$@\""));
        let tmux_cmd_shim = std::fs::read_to_string(runtime.shim_dir.join("tmux.cmd")).unwrap();
        assert!(tmux_cmd_shim.contains("%CMUX_EXE%"));
        assert!(tmux_cmd_shim.contains("\"%CMUX_EXE%\" __tmux-compat %*"));
        let shadow_dir = runtime.shadow_config_dir.unwrap();
        let opencode_config: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(shadow_dir.join("opencode.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            opencode_config["plugin"].as_array().unwrap(),
            &vec![
                serde_json::Value::String("existing-plugin".to_string()),
                serde_json::Value::String("oh-my-opencode".to_string())
            ]
        );
        let omo_config: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(shadow_dir.join("oh-my-opencode.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(omo_config["tmux"]["enabled"], true);

        let _ = std::fs::remove_dir_all(base_dir);
    }

    #[test]
    fn agent_integration_setup_preserves_jsonc_shadow_config_shape() {
        let _env_lock = ENV_LOCK.lock().unwrap();
        let base_dir = std::env::temp_dir().join(format!(
            "agentmux-cli-integration-jsonc-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source_dir = base_dir.join("source-opencode");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(
            source_dir.join("opencode.json"),
            r#"{
  // keep this project note
  "plugin": [
    "existing-plugin"
  ],
  "theme": "dark",
}
"#,
        )
        .unwrap();
        std::fs::write(
            source_dir.join("oh-my-opencode.json"),
            r#"{
  // keep tmux note
  "tmux": {
    "enabled": false,
  },
}
"#,
        )
        .unwrap();

        let previous = std::env::var_os("OPENCODE_CONFIG_DIR");
        std::env::set_var("OPENCODE_CONFIG_DIR", &source_dir);
        let runtime =
            setup_agent_integration_files(AgentIntegrationKind::Omo, &base_dir, Vec::new())
                .unwrap();
        if let Some(previous) = previous {
            std::env::set_var("OPENCODE_CONFIG_DIR", previous);
        } else {
            std::env::remove_var("OPENCODE_CONFIG_DIR");
        }

        let shadow_dir = runtime.shadow_config_dir.unwrap();
        let opencode_text = std::fs::read_to_string(shadow_dir.join("opencode.json")).unwrap();
        assert!(opencode_text.contains("// keep this project note"));
        assert!(opencode_text.contains("\"existing-plugin\""));
        assert!(opencode_text.contains("\"oh-my-opencode\""));
        assert!(read_json_file_with_error(&shadow_dir.join("opencode.json")).is_ok());

        let omo_text = std::fs::read_to_string(shadow_dir.join("oh-my-opencode.json")).unwrap();
        assert!(omo_text.contains("// keep tmux note"));
        assert!(omo_text.contains("\"enabled\": true"));
        assert!(!omo_text.contains("\"enabled\": false"));
        assert!(read_json_file_with_error(&shadow_dir.join("oh-my-opencode.json")).is_ok());

        let _ = std::fs::remove_dir_all(base_dir);
    }

    #[test]
    fn omc_setup_creates_restore_module_and_node_options() {
        let base_dir = std::env::temp_dir().join(format!(
            "agentmux-cli-omc-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let runtime =
            setup_agent_integration_files(AgentIntegrationKind::Omc, &base_dir, Vec::new())
                .unwrap();
        let restore_module = runtime.node_options_restore_module.unwrap();
        assert!(restore_module.is_file());
        let script = std::fs::read_to_string(&restore_module).unwrap();
        assert!(script.contains("AGENTMUX_ORIGINAL_NODE_OPTIONS"));
        assert!(script.contains("delete process.env.NODE_OPTIONS"));

        let node_options =
            node_options_with_required_module(Some("--max-old-space-size=4096"), &restore_module);
        assert!(node_options.contains("--require"));
        assert!(node_options.contains("--max-old-space-size=4096"));

        let spaced =
            node_options_with_required_module(None, Path::new(r"C:\Users\Test User\x.cjs"));
        assert!(spaced.contains("\"C:\\\\Users\\\\Test User\\\\x.cjs\""));

        let _ = std::fs::remove_dir_all(base_dir);
    }

    #[test]
    fn agent_integration_wsl_command_converts_paths_and_launches_linux_command() {
        let runtime = AgentIntegrationRuntime {
            kind: AgentIntegrationKind::Omo,
            base_dir: PathBuf::from(r"D:\agentmux-cmuxterm"),
            shim_dir: PathBuf::from(r"D:\agentmux-cmuxterm\omo-bin"),
            command: "opencode".to_string(),
            args: vec!["--continue".to_string()],
            env: vec![
                (
                    "PATH".to_string(),
                    r"D:\agentmux-cmuxterm\omo-bin".to_string(),
                ),
                (
                    "OPENCODE_CONFIG_DIR".to_string(),
                    r"D:\agentmux-cmuxterm\omo-config".to_string(),
                ),
                (
                    "AGENTMUX_CONTROL_PIPE".to_string(),
                    r"\\.\pipe\agentmux-test".to_string(),
                ),
            ],
            shadow_config_dir: Some(PathBuf::from(r"D:\agentmux-cmuxterm\omo-config")),
            node_options_restore_module: None,
            package_install: None,
        };

        let spec = build_agent_integration_wsl_command(
            &runtime,
            "Ubuntu",
            Path::new(r"D:\Workspace\irae\agentmux\target\debug\cmux.exe"),
        )
        .unwrap();

        assert_eq!(spec.executable, "wsl.exe");
        assert_eq!(spec.args[0], "--distribution");
        assert_eq!(spec.args[1], "Ubuntu");
        assert_eq!(spec.args[2], "--exec");
        assert_eq!(spec.args[3], "env");
        assert!(!spec.args.iter().any(|arg| arg.starts_with("PATH=")));
        assert!(spec
            .args
            .contains(&"OPENCODE_CONFIG_DIR=/mnt/d/agentmux-cmuxterm/omo-config".to_string()));
        assert!(spec.args.contains(
            &"CMUX_EXE=/mnt/d/Workspace/irae/agentmux/target/debug/cmux.exe".to_string()
        ));
        assert!(spec.args.contains(
            &"AGENTMUX_EXE=/mnt/d/Workspace/irae/agentmux/target/debug/cmux.exe".to_string()
        ));
        assert!(spec
            .args
            .contains(&"export PATH=\"$1:$PATH\"; shift; exec \"$@\"".to_string()));
        assert_eq!(
            spec.args[spec.args.len() - 3..],
            [
                "/mnt/d/agentmux-cmuxterm/omo-bin".to_string(),
                "opencode".to_string(),
                "--continue".to_string()
            ]
        );
    }

    #[test]
    fn path_to_wsl_value_converts_windows_drive_paths() {
        assert_eq!(
            path_to_wsl_value(Path::new(r"D:\work\repo")).unwrap(),
            "/mnt/d/work/repo"
        );
        assert_eq!(
            path_to_wsl_value(Path::new("/home/user/repo")).unwrap(),
            "/home/user/repo"
        );
    }

    #[test]
    fn wsl_doctor_command_uses_distribution_and_script_arguments() {
        let spec =
            build_wsl_doctor_command("Ubuntu", "command -v -- \"$1\"", &["opencode".to_string()]);

        assert_eq!(spec.executable, "wsl.exe");
        assert_eq!(
            spec.args,
            vec![
                "--distribution".to_string(),
                "Ubuntu".to_string(),
                "--exec".to_string(),
                "sh".to_string(),
                "-lc".to_string(),
                "command -v -- \"$1\"".to_string(),
                "agentmux-doctor".to_string(),
                "opencode".to_string(),
            ]
        );
    }

    #[test]
    fn wsl_omo_package_install_command_uses_wsl_shadow_config_path() {
        let spec = build_wsl_omo_package_install_command(
            "Ubuntu",
            Path::new(r"D:\agentmux-cmuxterm\omo-config"),
        )
        .unwrap();

        assert_eq!(spec.executable, "wsl.exe");
        assert_eq!(spec.args[0], "--distribution");
        assert_eq!(spec.args[1], "Ubuntu");
        assert_eq!(spec.args[2], "--exec");
        assert_eq!(spec.args[3], "sh");
        assert_eq!(spec.args[4], "-lc");
        assert!(spec.args[5].contains("bun add oh-my-opencode"));
        assert!(spec.args[5].contains("npm install oh-my-opencode --save"));
        assert_eq!(spec.args[6], "agentmux-omo-install");
        assert_eq!(spec.args[7], "/mnt/d/agentmux-cmuxterm/omo-config");
    }

    #[test]
    fn windows_user_path_helpers_append_without_duplicates() {
        assert_eq!(
            next_windows_user_path_value(
                Some(r"C:\Windows;D:\agentmux-cmuxterm\bin"),
                r"D:\agentmux-cmuxterm\bin"
            ),
            None
        );
        assert_eq!(
            next_windows_user_path_value(
                Some(r"C:\Windows;D:\agentmux-cmuxterm\bin\"),
                r"d:\AgentMux-CmuxTerm\bin"
            ),
            None
        );
        assert_eq!(
            next_windows_user_path_value(Some(r"C:\Windows"), r"D:\agentmux-cmuxterm\bin"),
            Some(r"C:\Windows;D:\agentmux-cmuxterm\bin".to_string())
        );
        assert_eq!(
            next_windows_user_path_value(None, r"D:\agentmux-cmuxterm\bin"),
            Some(r"D:\agentmux-cmuxterm\bin".to_string())
        );
    }

    #[test]
    fn parse_reg_query_value_reads_path_values() {
        let output = r#"
HKEY_CURRENT_USER\Environment
    Path    REG_EXPAND_SZ    C:\Users\Test\bin;%USERPROFILE%\tools
"#;
        assert_eq!(
            parse_reg_query_value(output, "Path").as_deref(),
            Some(r"C:\Users\Test\bin;%USERPROFILE%\tools")
        );
    }

    #[test]
    fn agent_integration_install_shims_writes_wrappers_and_profiles() {
        let _env_lock = ENV_LOCK.lock().unwrap();
        let base_dir = std::env::temp_dir().join(format!(
            "agentmux-cli-install-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source_dir = base_dir.join("source-opencode");
        std::fs::create_dir_all(&source_dir).unwrap();
        let previous = std::env::var_os("OPENCODE_CONFIG_DIR");
        std::env::set_var("OPENCODE_CONFIG_DIR", &source_dir);

        let bin_dir = base_dir.join("bin");
        let ps_profile = base_dir.join("Microsoft.PowerShell_profile.ps1");
        let sh_profile = base_dir.join(".bashrc");
        let result = install_agent_integration_shims(
            &base_dir,
            &bin_dir,
            Some(&ps_profile),
            Some(&sh_profile),
        )
        .unwrap();
        let result_again = install_agent_integration_shims(
            &base_dir,
            &bin_dir,
            Some(&ps_profile),
            Some(&sh_profile),
        )
        .unwrap();

        if let Some(previous) = previous {
            std::env::set_var("OPENCODE_CONFIG_DIR", previous);
        } else {
            std::env::remove_var("OPENCODE_CONFIG_DIR");
        }

        assert_eq!(result.bin_dir, bin_dir);
        assert!(result.wrappers.iter().any(|path| path.ends_with("omo.cmd")));
        assert!(result
            .wrappers
            .iter()
            .any(|path| path.ends_with("claude-teams")));
        assert!(bin_dir.join("omc.cmd").is_file());
        assert!(base_dir.join("agentmux-integrations.ps1").is_file());
        assert!(base_dir.join("agentmux-integrations.sh").is_file());
        assert!(base_dir.join("omo-bin").join("tmux.cmd").is_file());

        let ps_text = std::fs::read_to_string(&ps_profile).unwrap();
        assert_eq!(
            ps_text.matches("AgentMux cmux integration shims").count(),
            2
        );
        let sh_text = std::fs::read_to_string(&sh_profile).unwrap();
        assert!(sh_text.contains("agentmux_cmux_shim_bin"));
        assert_eq!(result_again.wrappers.len(), result.wrappers.len());

        let _ = std::fs::remove_dir_all(base_dir);
    }

    #[test]
    fn omo_package_install_uses_shadow_config_directory() {
        let _env_lock = ENV_LOCK.lock().unwrap();
        let base_dir = std::env::temp_dir().join(format!(
            "agentmux-cli-omo-install-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source_dir = base_dir.join("source-opencode");
        std::fs::create_dir_all(&source_dir).unwrap();
        let previous_config = std::env::var_os("OPENCODE_CONFIG_DIR");
        std::env::set_var("OPENCODE_CONFIG_DIR", &source_dir);
        setup_agent_integration_files(AgentIntegrationKind::Omo, &base_dir, Vec::new()).unwrap();

        let tools_dir = base_dir.join("tools");
        std::fs::create_dir_all(&tools_dir).unwrap();
        std::fs::write(
            tools_dir.join("bun.cmd"),
            "@echo off\r\nmkdir node_modules\\oh-my-opencode\r\necho {}> node_modules\\oh-my-opencode\\package.json\r\nexit /b 0\r\n",
        )
        .unwrap();
        let previous_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &tools_dir);

        let result = ensure_omo_package_installed(&base_dir).unwrap();

        if let Some(previous_config) = previous_config {
            std::env::set_var("OPENCODE_CONFIG_DIR", previous_config);
        } else {
            std::env::remove_var("OPENCODE_CONFIG_DIR");
        }
        if let Some(previous_path) = previous_path {
            std::env::set_var("PATH", previous_path);
        } else {
            std::env::remove_var("PATH");
        }

        assert_eq!(result.status, "installed");
        assert_eq!(result.package_manager.as_deref(), Some("bun"));
        assert_eq!(result.node_modules_status, "isolated");
        assert!(base_dir
            .join("omo-config")
            .join("node_modules")
            .join("oh-my-opencode")
            .is_dir());
        assert!(base_dir.join("omo-config").join("package.json").is_file());

        let _ = std::fs::remove_dir_all(base_dir);
    }

    #[test]
    fn omo_package_install_replaces_shadow_node_modules_symlink() {
        let _env_lock = ENV_LOCK.lock().unwrap();
        let base_dir = std::env::temp_dir().join(format!(
            "agentmux-cli-omo-install-symlink-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source_dir = base_dir.join("source-opencode");
        let source_node_modules = source_dir.join("node_modules");
        std::fs::create_dir_all(source_node_modules.join("oh-my-opencode")).unwrap();
        std::fs::write(
            source_node_modules
                .join("oh-my-opencode")
                .join("package.json"),
            "{}",
        )
        .unwrap();
        let previous_config = std::env::var_os("OPENCODE_CONFIG_DIR");
        std::env::set_var("OPENCODE_CONFIG_DIR", &source_dir);
        setup_agent_integration_files(AgentIntegrationKind::Omo, &base_dir, Vec::new()).unwrap();

        let shadow_node_modules = base_dir.join("omo-config").join("node_modules");
        if !is_symlink(&shadow_node_modules) {
            if let Some(previous_config) = previous_config {
                std::env::set_var("OPENCODE_CONFIG_DIR", previous_config);
            } else {
                std::env::remove_var("OPENCODE_CONFIG_DIR");
            }
            let _ = std::fs::remove_dir_all(base_dir);
            return;
        }

        let bin_dir = base_dir.join("bin");
        install_agent_integration_shims(&base_dir, &bin_dir, None, None).unwrap();
        let doctor =
            inspect_agent_integrations(&base_dir, &bin_dir, Some(AgentIntegrationKind::Omo), None);
        let item = &doctor.integrations[0];
        assert!(item.checks.iter().any(|check| {
            check.name == "omo-node-modules-isolated"
                && !check.ok
                && check.detail.contains("is a symlink")
        }));

        let tools_dir = base_dir.join("tools");
        std::fs::create_dir_all(&tools_dir).unwrap();
        std::fs::write(
            tools_dir.join("bun.cmd"),
            "@echo off\r\nmkdir node_modules\\oh-my-opencode\r\necho {}> node_modules\\oh-my-opencode\\package.json\r\nexit /b 0\r\n",
        )
        .unwrap();
        #[cfg(not(windows))]
        {
            std::fs::write(
                tools_dir.join("bun"),
                "#!/usr/bin/env sh\nmkdir -p node_modules/oh-my-opencode\nprintf '{}' > node_modules/oh-my-opencode/package.json\n",
            )
            .unwrap();
            set_executable_if_supported(&tools_dir.join("bun")).unwrap();
        }
        let previous_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &tools_dir);

        let result = ensure_omo_package_installed(&base_dir).unwrap();

        if let Some(previous_config) = previous_config {
            std::env::set_var("OPENCODE_CONFIG_DIR", previous_config);
        } else {
            std::env::remove_var("OPENCODE_CONFIG_DIR");
        }
        if let Some(previous_path) = previous_path {
            std::env::set_var("PATH", previous_path);
        } else {
            std::env::remove_var("PATH");
        }

        assert_eq!(result.status, "installed");
        assert_eq!(result.node_modules_status, "symlink-replaced");
        assert!(!is_symlink(&shadow_node_modules));
        assert!(shadow_node_modules
            .join("oh-my-opencode")
            .join("package.json")
            .is_file());

        let _ = std::fs::remove_dir_all(base_dir);
    }

    #[test]
    fn agent_integration_doctor_reports_ready_installation_shape() {
        let _env_lock = ENV_LOCK.lock().unwrap();
        let base_dir = std::env::temp_dir().join(format!(
            "agentmux-cli-doctor-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source_dir = base_dir.join("source-opencode");
        std::fs::create_dir_all(&source_dir).unwrap();
        let previous_config = std::env::var_os("OPENCODE_CONFIG_DIR");
        std::env::set_var("OPENCODE_CONFIG_DIR", &source_dir);

        let bin_dir = base_dir.join("bin");
        install_agent_integration_shims(&base_dir, &bin_dir, None, None).unwrap();
        std::fs::create_dir_all(
            base_dir
                .join("omo-config")
                .join("node_modules")
                .join("oh-my-opencode"),
        )
        .unwrap();

        let tools_dir = base_dir.join("tools");
        std::fs::create_dir_all(&tools_dir).unwrap();
        std::fs::write(tools_dir.join("opencode"), "").unwrap();
        std::fs::write(tools_dir.join("opencode.cmd"), "@echo off\r\n").unwrap();

        let previous_path = std::env::var_os("PATH");
        let mut paths = vec![bin_dir.clone(), tools_dir];
        if let Some(previous_path) = previous_path.clone() {
            paths.extend(std::env::split_paths(&previous_path));
        }
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());

        let result =
            inspect_agent_integrations(&base_dir, &bin_dir, Some(AgentIntegrationKind::Omo), None);

        if let Some(previous_config) = previous_config {
            std::env::set_var("OPENCODE_CONFIG_DIR", previous_config);
        } else {
            std::env::remove_var("OPENCODE_CONFIG_DIR");
        }
        if let Some(previous_path) = previous_path {
            std::env::set_var("PATH", previous_path);
        } else {
            std::env::remove_var("PATH");
        }

        assert!(result.bin_dir_on_path);
        assert_eq!(result.integrations.len(), 1);
        let item = &result.integrations[0];
        assert_eq!(item.kind, AgentIntegrationKind::Omo);
        assert_eq!(item.status, "ready");
        assert!(item
            .checks
            .iter()
            .any(|check| check.name == "omo-shadow-config" && check.ok));
        assert!(item
            .checks
            .iter()
            .any(|check| check.name == "omo-opencode-plugin" && check.ok));
        assert!(item
            .checks
            .iter()
            .any(|check| check.name == "omo-tmux-enabled" && check.ok));
        assert!(item
            .checks
            .iter()
            .any(|check| check.name == "omo-node-modules-isolated" && check.ok));
        let json = agent_integration_doctor_result_json(&result);
        assert_eq!(json["integrations"][0]["status"], "ready");
        assert_eq!(json["wsl_distribution"], serde_json::Value::Null);

        let _ = std::fs::remove_dir_all(base_dir);
    }

    #[test]
    fn agent_integration_doctor_reports_broken_omo_shadow_config_content() {
        let _env_lock = ENV_LOCK.lock().unwrap();
        let base_dir = std::env::temp_dir().join(format!(
            "agentmux-cli-doctor-broken-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source_dir = base_dir.join("source-opencode");
        std::fs::create_dir_all(&source_dir).unwrap();
        let previous_config = std::env::var_os("OPENCODE_CONFIG_DIR");
        std::env::set_var("OPENCODE_CONFIG_DIR", &source_dir);

        let bin_dir = base_dir.join("bin");
        install_agent_integration_shims(&base_dir, &bin_dir, None, None).unwrap();
        let shadow_dir = base_dir.join("omo-config");
        std::fs::create_dir_all(&shadow_dir).unwrap();
        std::fs::write(shadow_dir.join("opencode.json"), r#"{"plugin":["other"]}"#).unwrap();
        std::fs::write(
            shadow_dir.join("oh-my-opencode.json"),
            r#"{"tmux":{"enabled":false}}"#,
        )
        .unwrap();

        let tools_dir = base_dir.join("tools");
        std::fs::create_dir_all(&tools_dir).unwrap();
        std::fs::write(tools_dir.join("opencode"), "").unwrap();
        std::fs::write(tools_dir.join("opencode.cmd"), "@echo off\r\n").unwrap();

        let previous_path = std::env::var_os("PATH");
        let mut paths = vec![bin_dir.clone(), tools_dir];
        if let Some(previous_path) = previous_path.clone() {
            paths.extend(std::env::split_paths(&previous_path));
        }
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());

        let result =
            inspect_agent_integrations(&base_dir, &bin_dir, Some(AgentIntegrationKind::Omo), None);

        if let Some(previous_config) = previous_config {
            std::env::set_var("OPENCODE_CONFIG_DIR", previous_config);
        } else {
            std::env::remove_var("OPENCODE_CONFIG_DIR");
        }
        if let Some(previous_path) = previous_path {
            std::env::set_var("PATH", previous_path);
        } else {
            std::env::remove_var("PATH");
        }

        let item = &result.integrations[0];
        assert_eq!(item.status, "needs-attention");
        assert!(item
            .checks
            .iter()
            .any(|check| check.name == "omo-opencode-plugin" && !check.ok));
        assert!(item
            .checks
            .iter()
            .any(|check| check.name == "omo-tmux-enabled" && !check.ok));

        let _ = std::fs::remove_dir_all(base_dir);
    }

    #[test]
    fn agent_integration_options_parse_setup_shape() {
        let options = parse_agent_integration_setup_options(
            &[
                "--json".to_string(),
                "--base-dir".to_string(),
                r"D:\agentmux-cmuxterm".to_string(),
                "--distribution".to_string(),
                "Ubuntu".to_string(),
                "--install-packages".to_string(),
                "claude-teams".to_string(),
            ],
            "integrations setup",
        )
        .unwrap();
        assert!(options.invoke.json);
        assert_eq!(options.base_dir.as_deref(), Some(r"D:\agentmux-cmuxterm"));
        assert_eq!(options.kind, AgentIntegrationKind::ClaudeTeams);
        assert!(options.install_packages);
        assert_eq!(options.distribution.as_deref(), Some("Ubuntu"));

        let launch = parse_agent_integration_launch_options(
            AgentIntegrationKind::Omx,
            &["--madmax".to_string(), "--high".to_string()],
        )
        .unwrap();
        assert_eq!(launch.kind, AgentIntegrationKind::Omx);
        assert_eq!(
            launch.args,
            vec!["--madmax".to_string(), "--high".to_string()]
        );

        let install = parse_agent_integration_install_options(&[
            "--json".to_string(),
            "--base-dir".to_string(),
            r"D:\agentmux-cmuxterm".to_string(),
            "--bin-dir".to_string(),
            r"D:\agentmux-cmuxterm\bin".to_string(),
            "--powershell-profile".to_string(),
            r"D:\profile.ps1".to_string(),
            "--user-path".to_string(),
        ])
        .unwrap();
        assert!(install.invoke.json);
        assert_eq!(install.base_dir.as_deref(), Some(r"D:\agentmux-cmuxterm"));
        assert_eq!(
            install.bin_dir.as_deref(),
            Some(r"D:\agentmux-cmuxterm\bin")
        );
        assert_eq!(
            install.powershell_profile.as_deref(),
            Some(r"D:\profile.ps1")
        );
        assert!(install.user_path);

        let doctor = parse_agent_integration_doctor_options(&[
            "--json".to_string(),
            "--base-dir".to_string(),
            r"D:\agentmux-cmuxterm".to_string(),
            "--distribution".to_string(),
            "Ubuntu".to_string(),
            "omo".to_string(),
        ])
        .unwrap();
        assert!(doctor.invoke.json);
        assert_eq!(doctor.kind, Some(AgentIntegrationKind::Omo));
        assert_eq!(doctor.base_dir.as_deref(), Some(r"D:\agentmux-cmuxterm"));
        assert_eq!(doctor.distribution.as_deref(), Some("Ubuntu"));
    }

    #[test]
    fn browser_cli_options_parse_control_shapes() {
        let open = parse_browser_open_options(&[
            "--json".to_string(),
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--pane".to_string(),
            "%pane_1".to_string(),
            "--placement".to_string(),
            "active-pane".to_string(),
            "--profile".to_string(),
            "default".to_string(),
        ])
        .unwrap();
        assert!(open.invoke.json);
        assert_eq!(open.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(open.pane_id.as_deref(), Some("pane_1"));
        assert_eq!(open.placement.as_deref(), Some("active_pane"));
        assert_eq!(open.profile.as_deref(), Some("default"));

        let navigate = parse_browser_navigate_options(&[
            "surf_1".to_string(),
            "https://example.com".to_string(),
        ])
        .unwrap();
        assert_eq!(navigate.params.surface_id, "surf_1");
        assert_eq!(navigate.params.url, "https://example.com");

        let reload =
            parse_browser_surface_command_options(&["surf_1".to_string()], "reload").unwrap();
        assert_eq!(reload.params.surface_id, "surf_1");

        let frames =
            parse_browser_surface_command_options(&["surf_1".to_string()], "frames").unwrap();
        assert_eq!(frames.params.surface_id, "surf_1");

        let storage =
            parse_browser_surface_command_options(&["surf_1".to_string()], "storage").unwrap();
        assert_eq!(storage.params.surface_id, "surf_1");

        let cookies =
            parse_browser_surface_command_options(&["surf_1".to_string()], "cookies").unwrap();
        assert_eq!(cookies.params.surface_id, "surf_1");

        let downloads = parse_browser_downloads_options(&[
            "surf_1".to_string(),
            "--limit".to_string(),
            "25".to_string(),
        ])
        .unwrap();
        assert_eq!(downloads.params.surface_id, "surf_1");
        assert_eq!(downloads.params.limit, Some(25));

        let history =
            parse_browser_surface_command_options(&["surf_1".to_string()], "history").unwrap();
        assert_eq!(history.params.surface_id, "surf_1");

        let console = parse_browser_console_options(&[
            "surf_1".to_string(),
            "--limit".to_string(),
            "25".to_string(),
        ])
        .unwrap();
        assert_eq!(console.params.surface_id, "surf_1");
        assert_eq!(console.params.limit, Some(25));

        let dialogs = parse_browser_dialogs_options(&[
            "surf_1".to_string(),
            "--limit".to_string(),
            "25".to_string(),
        ])
        .unwrap();
        assert_eq!(dialogs.params.surface_id, "surf_1");
        assert_eq!(dialogs.params.limit, Some(25));

        let errors = parse_browser_errors_options(&[
            "surf_1".to_string(),
            "--limit".to_string(),
            "25".to_string(),
        ])
        .unwrap();
        assert_eq!(errors.params.surface_id, "surf_1");
        assert_eq!(errors.params.limit, Some(25));

        let screenshot = parse_browser_screenshot_options(&[
            "--format".to_string(),
            "jpeg".to_string(),
            "surf_1".to_string(),
        ])
        .unwrap();
        assert_eq!(screenshot.params.surface_id, "surf_1");
        assert_eq!(screenshot.params.format.as_deref(), Some("jpeg"));

        let dom_snapshot = parse_browser_dom_snapshot_options(&[
            "surf_1".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
        ])
        .unwrap();
        assert_eq!(dom_snapshot.params.surface_id, "surf_1");
        assert_eq!(dom_snapshot.params.frame_id.as_deref(), Some("frame_1"));

        let click_selector = parse_browser_click_options(&[
            "surf_1".to_string(),
            "--selector".to_string(),
            "#submit".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
        ])
        .unwrap();
        assert_eq!(click_selector.params.selector.as_deref(), Some("#submit"));
        assert_eq!(click_selector.params.x, None);
        assert_eq!(click_selector.params.y, None);
        assert_eq!(click_selector.params.frame_id.as_deref(), Some("frame_1"));

        let click_point = parse_browser_click_options(&[
            "surf_1".to_string(),
            "--x".to_string(),
            "12".to_string(),
            "--y".to_string(),
            "24".to_string(),
        ])
        .unwrap();
        assert_eq!(click_point.params.selector, None);
        assert_eq!(click_point.params.x, Some(12.0));
        assert_eq!(click_point.params.y, Some(24.0));

        let typed = parse_browser_type_options(&[
            "surf_1".to_string(),
            "#q".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
            "--".to_string(),
            "hello".to_string(),
            "world".to_string(),
        ])
        .unwrap();
        assert_eq!(typed.params.surface_id, "surf_1");
        assert_eq!(typed.params.selector, "#q");
        assert_eq!(typed.params.text, "hello world");
        assert_eq!(typed.params.frame_id.as_deref(), Some("frame_1"));

        let filled = parse_browser_fill_options(&[
            "surf_1".to_string(),
            "#q".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
            "--".to_string(),
            "hello".to_string(),
            "world".to_string(),
        ])
        .unwrap();
        assert_eq!(filled.params.surface_id, "surf_1");
        assert_eq!(filled.params.selector, "#q");
        assert_eq!(filled.params.text, "hello world");
        assert_eq!(filled.params.frame_id.as_deref(), Some("frame_1"));

        let pressed = parse_browser_press_options(&[
            "surf_1".to_string(),
            "#q".to_string(),
            "Enter".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
        ])
        .unwrap();
        assert_eq!(pressed.params.key, "Enter");
        assert_eq!(pressed.params.frame_id.as_deref(), Some("frame_1"));

        let selected = parse_browser_select_options(&[
            "surf_1".to_string(),
            "#choice".to_string(),
            "one".to_string(),
            "two".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
        ])
        .unwrap();
        assert_eq!(
            selected.params.values,
            vec!["one".to_string(), "two".to_string()]
        );
        assert_eq!(selected.params.frame_id.as_deref(), Some("frame_1"));

        let scrolled = parse_browser_scroll_options(&[
            "surf_1".to_string(),
            "--selector".to_string(),
            "#list".to_string(),
            "--y".to_string(),
            "400".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
        ])
        .unwrap();
        assert_eq!(scrolled.params.selector.as_deref(), Some("#list"));
        assert_eq!(scrolled.params.y, Some(400));
        assert_eq!(scrolled.params.frame_id.as_deref(), Some("frame_1"));

        let hovered = parse_browser_hover_options(&[
            "surf_1".to_string(),
            "#submit".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
        ])
        .unwrap();
        assert_eq!(hovered.params.selector, "#submit");
        assert_eq!(hovered.params.frame_id.as_deref(), Some("frame_1"));

        let checked = parse_browser_check_options(&[
            "surf_1".to_string(),
            "#agree".to_string(),
            "false".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
        ])
        .unwrap();
        assert_eq!(checked.params.checked, Some(false));
        assert_eq!(checked.params.frame_id.as_deref(), Some("frame_1"));

        let got = parse_browser_get_options(&[
            "surf_1".to_string(),
            "#title".to_string(),
            "--kind".to_string(),
            "attribute".to_string(),
            "--attribute".to_string(),
            "href".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
        ])
        .unwrap();
        assert_eq!(got.params.surface_id, "surf_1");
        assert_eq!(got.params.selector, "#title");
        assert_eq!(got.params.kind.as_deref(), Some("attribute"));
        assert_eq!(got.params.attribute.as_deref(), Some("href"));
        assert_eq!(got.params.frame_id.as_deref(), Some("frame_1"));

        let found = parse_browser_find_options(&[
            "surf_1".to_string(),
            "agentmux".to_string(),
            "--selector".to_string(),
            "main".to_string(),
            "--limit".to_string(),
            "5".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
        ])
        .unwrap();
        assert_eq!(found.params.surface_id, "surf_1");
        assert_eq!(found.params.query, "agentmux");
        assert_eq!(found.params.selector.as_deref(), Some("main"));
        assert_eq!(found.params.limit, Some(5));
        assert_eq!(found.params.frame_id.as_deref(), Some("frame_1"));

        let highlighted = parse_browser_highlight_options(&[
            "surf_1".to_string(),
            "#q".to_string(),
            "--duration-ms".to_string(),
            "750".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
        ])
        .unwrap();
        assert_eq!(highlighted.params.surface_id, "surf_1");
        assert_eq!(highlighted.params.selector, "#q");
        assert_eq!(highlighted.params.duration_ms, Some(750));
        assert_eq!(highlighted.params.frame_id.as_deref(), Some("frame_1"));

        let focused = parse_browser_focus_options(&[
            "surf_1".to_string(),
            "#q".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
        ])
        .unwrap();
        assert_eq!(focused.params.surface_id, "surf_1");
        assert_eq!(focused.params.selector, "#q");
        assert_eq!(focused.params.frame_id.as_deref(), Some("frame_1"));

        let zoomed =
            parse_browser_zoom_options(&["surf_1".to_string(), "125".to_string()]).unwrap();
        assert_eq!(zoomed.params.surface_id, "surf_1");
        assert_eq!(zoomed.params.percent, 125);

        let wait = parse_browser_wait_for_selector_options(&[
            "surf_1".to_string(),
            "#ready".to_string(),
            "--timeout-ms".to_string(),
            "1500".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
        ])
        .unwrap();
        assert_eq!(wait.params.surface_id, "surf_1");
        assert_eq!(wait.params.selector, "#ready");
        assert_eq!(wait.params.timeout_ms, Some(1500));
        assert_eq!(wait.params.frame_id.as_deref(), Some("frame_1"));

        let evaluated = parse_browser_evaluate_options(&[
            "surf_1".to_string(),
            "--frame".to_string(),
            "frame_1".to_string(),
            "--".to_string(),
            "document.title".to_string(),
        ])
        .unwrap();
        assert_eq!(evaluated.params.surface_id, "surf_1");
        assert_eq!(evaluated.params.script, "document.title");
        assert_eq!(evaluated.params.frame_id.as_deref(), Some("frame_1"));

        let diagnostics = parse_browser_diagnostics_options(&[
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--surface".to_string(),
            "surf_1".to_string(),
        ])
        .unwrap();
        assert_eq!(diagnostics.params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(diagnostics.params.surface_id.as_deref(), Some("surf_1"));
    }

    #[test]
    fn ssh_cli_options_parse_direct_and_profile_targets() {
        let direct = parse_ssh_options(&[
            "--json".to_string(),
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--pane".to_string(),
            "%pane_1".to_string(),
            "--placement".to_string(),
            "active-pane".to_string(),
            "--columns".to_string(),
            "100".to_string(),
            "--rows".to_string(),
            "40".to_string(),
            "deploy@example.com:2222".to_string(),
        ])
        .unwrap();
        assert!(direct.invoke.json);
        assert_eq!(direct.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(direct.pane_id.as_deref(), Some("pane_1"));
        assert_eq!(direct.placement.as_deref(), Some("active_pane"));
        assert_eq!(direct.columns, 100);
        assert_eq!(direct.rows, 40);
        assert_eq!(direct.target, "deploy@example.com:2222");

        let profile = parse_ssh_options(&[
            "--profile".to_string(),
            "prod-server".to_string(),
            "--new-tab".to_string(),
        ])
        .unwrap();
        assert_eq!(profile.target, "prod-server");
        assert_eq!(profile.placement.as_deref(), Some("new_tab"));

        assert_eq!(
            profile_to_ssh_target(&ProfileSummaryResult {
                profile_id: "prof_1".to_string(),
                name: "prod-server".to_string(),
                host: "example.com".to_string(),
                user: "deploy".to_string(),
                port: Some(2222),
            }),
            "deploy@example.com:2222"
        );
        assert_eq!(
            profile_to_ssh_target(&ProfileSummaryResult {
                profile_id: "prof_2".to_string(),
                name: "staging".to_string(),
                host: "staging.example.com".to_string(),
                user: "ops".to_string(),
                port: None,
            }),
            "ops@staging.example.com"
        );
    }

    #[test]
    fn tmux_compat_parses_core_translation_commands() {
        let split = parse_tmux_compat_options(&[
            "--workspace".to_string(),
            "ws_1".to_string(),
            "split-window".to_string(),
            "-h".to_string(),
            "-t".to_string(),
            "%pane_left".to_string(),
            "--".to_string(),
            "bash".to_string(),
            "-lc".to_string(),
            "echo worker".to_string(),
        ])
        .unwrap();
        assert_eq!(split.workspace_id.as_deref(), Some("ws_1"));
        assert!(matches!(
            split.command,
            TmuxCompatCommand::SplitWindow {
                target_pane_id: Some(ref pane),
                ref axis,
                ref command,
                format: None,
                ..
            } if pane == "pane_left" && axis == "horizontal" && command == &vec![
                "bash".to_string(),
                "-lc".to_string(),
                "echo worker".to_string()
            ]
        ));

        let split_default_shell = parse_tmux_compat_options(&[
            "split-window".to_string(),
            "-v".to_string(),
            "-P".to_string(),
            "-F".to_string(),
            "#{pane_id}".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            split_default_shell.command,
            TmuxCompatCommand::SplitWindow {
                ref command,
                format: Some(ref format),
                ..
            } if command.is_empty() && format == "#{pane_id}"
        ));

        let send = parse_tmux_compat_options(&[
            "send-keys".to_string(),
            "-t".to_string(),
            "%pane_right".to_string(),
            "echo hello".to_string(),
            "Enter".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            send.command,
            TmuxCompatCommand::SendKeys {
                target_pane_id: Some(ref pane),
                ref keys
            } if pane == "pane_right" && keys == &vec![
                "echo hello".to_string(),
                "Enter".to_string()
            ]
        ));

        let capture = parse_tmux_compat_options(&[
            "capture-pane".to_string(),
            "-p".to_string(),
            "-S".to_string(),
            "-200".to_string(),
            "-t".to_string(),
            "%pane_right".to_string(),
            "--max-bytes".to_string(),
            "16384".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            capture.command,
            TmuxCompatCommand::CapturePane {
                target_pane_id: Some(ref pane),
                max_bytes: 16384
            } if pane == "pane_right"
        ));

        let kill = parse_tmux_compat_options(&[
            "kill-pane".to_string(),
            "-t".to_string(),
            "%pane_right".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            kill.command,
            TmuxCompatCommand::KillPane {
                target_pane_id: Some(ref pane)
            } if pane == "pane_right"
        ));

        let new_window = parse_tmux_compat_options(&[
            "new-window".to_string(),
            "-n".to_string(),
            "worker".to_string(),
            "-P".to_string(),
            "-F".to_string(),
            "#{pane_id}".to_string(),
            "--".to_string(),
            "bash".to_string(),
            "-lc".to_string(),
            "echo new".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            new_window.command,
            TmuxCompatCommand::NewWindow {
                ref command,
                format: Some(ref format)
            } if command == &vec![
                "bash".to_string(),
                "-lc".to_string(),
                "echo new".to_string()
            ] && format == "#{pane_id}"
        ));

        let new_window_default_shell =
            parse_tmux_compat_options(&["new-window".to_string()]).unwrap();
        assert!(matches!(
            new_window_default_shell.command,
            TmuxCompatCommand::NewWindow {
                ref command,
                format: None
            } if command.is_empty()
        ));

        let select_window = parse_tmux_compat_options(&[
            "select-window".to_string(),
            "-t".to_string(),
            ":1".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            select_window.command,
            TmuxCompatCommand::SelectWindow {
                target_window: Some(ref target)
            } if target == "1"
        ));

        let kill_window = parse_tmux_compat_options(&[
            "kill-window".to_string(),
            "-t".to_string(),
            "@pane_root".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            kill_window.command,
            TmuxCompatCommand::KillWindow {
                target_window: Some(ref target)
            } if target == "pane_root"
        ));

        let switch_client = parse_tmux_compat_options(&[
            "switch-client".to_string(),
            "-t".to_string(),
            "workers:1".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            switch_client.command,
            TmuxCompatCommand::SwitchClient {
                target: Some(ref target)
            } if target == "workers:1"
        ));

        let rename_window = parse_tmux_compat_options(&[
            "rename-window".to_string(),
            "-t".to_string(),
            "workers:1".to_string(),
            "Build".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            rename_window.command,
            TmuxCompatCommand::RenameWindow {
                target_window: Some(ref target),
                ref name
            } if target == "workers:1" && name == "Build"
        ));

        let rename_session = parse_tmux_compat_options(&[
            "rename-session".to_string(),
            "-t".to_string(),
            "workers".to_string(),
            "Renamed".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            rename_session.command,
            TmuxCompatCommand::RenameSession {
                target_session: Some(ref target),
                ref name
            } if target == "workers" && name == "Renamed"
        ));

        let new_session = parse_tmux_compat_options(&[
            "new-session".to_string(),
            "-d".to_string(),
            "-s".to_string(),
            "workers".to_string(),
            "-c".to_string(),
            "/work".to_string(),
            "-F".to_string(),
            "#{session_name}".to_string(),
            "--".to_string(),
            "bash".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            new_session.command,
            TmuxCompatCommand::NewSession {
                session_name: Some(ref name),
                cwd: Some(ref cwd),
                ref command,
                format: Some(ref format)
            } if name == "workers"
                && cwd == "/work"
                && command == &vec!["bash".to_string()]
                && format == "#{session_name}"
        ));

        let list_sessions = parse_tmux_compat_options(&[
            "list-sessions".to_string(),
            "-F".to_string(),
            "#{session_name}:#{session_windows}".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            list_sessions.command,
            TmuxCompatCommand::ListSessions {
                format: Some(ref format)
            } if format == "#{session_name}:#{session_windows}"
        ));

        let list_panes = parse_tmux_compat_options(&[
            "list-panes".to_string(),
            "-a".to_string(),
            "-F".to_string(),
            "#{session_name}:#{window_index}.#{pane_index}:#{pane_id}".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            list_panes.command,
            TmuxCompatCommand::ListPanes {
                all_workspaces: true,
                format: Some(ref format)
            } if format == "#{session_name}:#{window_index}.#{pane_index}:#{pane_id}"
        ));

        let list_windows = parse_tmux_compat_options(&[
            "list-windows".to_string(),
            "-a".to_string(),
            "-F".to_string(),
            "#{session_name}:#{window_index}:#{window_id}".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            list_windows.command,
            TmuxCompatCommand::ListWindows {
                all_workspaces: true,
                format: Some(ref format)
            } if format == "#{session_name}:#{window_index}:#{window_id}"
        ));

        let has_session = parse_tmux_compat_options(&[
            "has-session".to_string(),
            "-t".to_string(),
            "workers:1".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            has_session.command,
            TmuxCompatCommand::HasSession {
                target: Some(ref target)
            } if target == "workers:1"
        ));
    }

    #[test]
    fn tmux_compat_helpers_render_fake_tmux_pane_ids() {
        assert_eq!(normalize_tmux_pane_id("%pane_123"), "pane_123");
        assert_eq!(agentmux_pane_to_tmux_pane("pane_123"), "%pane_123");
        assert_eq!(tmux_key_to_agentmux_key("C-m"), Some("enter"));
        assert_eq!(
            split_tmux_session_window_target(Some("workers:1")),
            (Some("workers".to_string()), Some("1".to_string()))
        );
        assert_eq!(
            split_tmux_session_window_target(Some(":2")),
            (None, Some("2".to_string()))
        );
        assert_eq!(
            split_tmux_session_window_pane_target("workers:1.2"),
            TmuxPaneTargetParts {
                session: Some("workers".to_string()),
                window: Some("1".to_string()),
                pane: Some("2".to_string()),
            }
        );
        assert_eq!(
            split_tmux_session_window_pane_target(":1"),
            TmuxPaneTargetParts {
                session: None,
                window: Some("1".to_string()),
                pane: None,
            }
        );
        assert_eq!(
            split_tmux_session_window_pane_target("%pane_left"),
            TmuxPaneTargetParts {
                session: None,
                window: None,
                pane: Some("pane_left".to_string()),
            }
        );
        let pane_detail = WorkspaceDetailResult {
            workspace: WorkspaceSummaryResult {
                workspace_id: "ws_1".to_string(),
                name: "Project".to_string(),
                root_pane_id: "pane_root".to_string(),
                active_pane_id: "pane_left".to_string(),
                project_root: None,
                environment_profile_id: None,
                description: None,
                icon: None,
                color: None,
                default_wsl_distribution: None,
                default_agent_command: None,
            },
            panes: vec![
                PaneSummaryResult {
                    pane_id: "pane_root".to_string(),
                    workspace_id: "ws_1".to_string(),
                    parent_pane_id: None,
                    kind: "split".to_string(),
                    split_axis: Some("vertical".to_string()),
                    split_ratio: Some(0.5),
                    mounted_surface_id: None,
                },
                PaneSummaryResult {
                    pane_id: "pane_left".to_string(),
                    workspace_id: "ws_1".to_string(),
                    parent_pane_id: Some("pane_root".to_string()),
                    kind: "leaf".to_string(),
                    split_axis: None,
                    split_ratio: None,
                    mounted_surface_id: Some("surf_left".to_string()),
                },
                PaneSummaryResult {
                    pane_id: "pane_right".to_string(),
                    workspace_id: "ws_1".to_string(),
                    parent_pane_id: Some("pane_root".to_string()),
                    kind: "leaf".to_string(),
                    split_axis: None,
                    split_ratio: None,
                    mounted_surface_id: Some("surf_right".to_string()),
                },
                PaneSummaryResult {
                    pane_id: "pane_tab".to_string(),
                    workspace_id: "ws_1".to_string(),
                    parent_pane_id: None,
                    kind: "leaf".to_string(),
                    split_axis: None,
                    split_ratio: None,
                    mounted_surface_id: Some("surf_tab".to_string()),
                },
            ],
            surfaces: Vec::new(),
            sessions: Vec::new(),
        };
        assert_eq!(
            resolve_tmux_pane_id_in_detail(&pane_detail, Some("0"), Some("1"), "pane_left")
                .unwrap(),
            "pane_right"
        );
        assert_eq!(
            resolve_tmux_pane_id_in_detail(&pane_detail, Some("1"), None, "pane_left").unwrap(),
            "pane_tab"
        );
        assert_eq!(
            resolve_tmux_pane_id_in_detail(&pane_detail, None, Some("."), "pane_left").unwrap(),
            "pane_left"
        );
        assert_eq!(
            resolve_tmux_window_root_id(&pane_detail, Some("!")).unwrap(),
            "pane_root"
        );
        let context = SystemIdentifyResult {
            in_agentmux: true,
            workspace_id: Some("ws_1".to_string()),
            pane_id: Some("pane_123".to_string()),
            surface_id: None,
            session_id: None,
            cwd: Some("/work".to_string()),
            backend_kind: None,
            control_pipe: DEFAULT_CONTROL_PIPE_NAME.to_string(),
        };
        assert_eq!(
            render_tmux_format(
                Some("#{session_name}:#{pane_id}:#{pane_current_path}:#{pane_active}"),
                &context,
                "pane_123"
            ),
            "ws_1:%pane_123:/work:1"
        );

        let detail = WorkspaceDetailResult {
            workspace: WorkspaceSummaryResult {
                workspace_id: "ws_1".to_string(),
                name: "Project".to_string(),
                root_pane_id: "pane_root".to_string(),
                active_pane_id: "pane_left".to_string(),
                project_root: None,
                environment_profile_id: None,
                description: None,
                icon: None,
                color: None,
                default_wsl_distribution: None,
                default_agent_command: None,
            },
            panes: Vec::new(),
            surfaces: Vec::new(),
            sessions: Vec::new(),
        };
        assert_eq!(
            render_tmux_window_format(
                Some("#{window_index}:#{window_id}:#{window_name}:#{window_active}"),
                &detail,
                "pane_root",
                0
            ),
            "0:pane_root:Project:1"
        );
        assert_eq!(
            render_tmux_session_format(
                Some("#{session_id}:#{session_name}:#{session_windows}:#{session_attached}"),
                &detail.workspace,
                2,
                true,
            ),
            "ws_1:Project:2:1"
        );
        let pane = PaneSummaryResult {
            pane_id: "pane_left".to_string(),
            workspace_id: "ws_1".to_string(),
            parent_pane_id: Some("pane_root".to_string()),
            kind: "leaf".to_string(),
            split_axis: None,
            split_ratio: None,
            mounted_surface_id: Some("surf_left".to_string()),
        };
        let pane_row = TmuxPaneRow {
            pane: &pane,
            session_id: "ws_1".to_string(),
            session_name: "Project".to_string(),
            window_id: "pane_root".to_string(),
            window_index: 1,
            window_name: "Project".to_string(),
            pane_index: 2,
        };
        assert_eq!(
            render_tmux_pane_format(
                Some(
                    "#{session_id}:#{session_name}:#{window_index}:#{window_id}:#{window_name}:#{pane_index}:#{pane_id}"
                ),
                &context,
                &pane_row,
            ),
            "ws_1:Project:1:pane_root:Project:2:%pane_left"
        );

        let metadata = build_tmux_agent_team_metadata(
            "claude-teams",
            "ws_1",
            "ses_worker",
            Some("pane_main"),
            "split-window",
            Some("pane_worker"),
        );
        assert_eq!(metadata.integration, "claude-teams");
        assert_eq!(metadata.agent_state.state, "running");
        assert_eq!(metadata.agent_state.session_id, "ses_worker");
        assert_eq!(
            metadata.agent_state.reason.as_deref(),
            Some("claude-teams split-window worker %pane_worker from %pane_main")
        );
        assert_eq!(
            metadata
                .agent_state
                .telemetry
                .as_ref()
                .and_then(|telemetry| telemetry.activity.as_deref()),
            Some("agent_team")
        );
        assert_eq!(metadata.sidebar_status.key, "agent-team.claude-teams");
        assert_eq!(metadata.sidebar_status.label, "claude-teams team active");
        assert!(metadata.sidebar_log.message.contains("ses_worker"));
    }

    #[test]
    fn tmux_compat_default_shell_command_follows_backend_context() {
        let _env_lock = ENV_LOCK.lock().unwrap();
        let previous_comspec = std::env::var_os("COMSPEC");
        let previous_wsl = std::env::var_os("WSL_DISTRO_NAME");
        std::env::set_var("COMSPEC", r"C:\Windows\System32\cmd.exe");
        std::env::remove_var("WSL_DISTRO_NAME");

        let mut context = SystemIdentifyResult {
            in_agentmux: true,
            workspace_id: Some("ws_1".to_string()),
            pane_id: Some("pane_123".to_string()),
            surface_id: None,
            session_id: None,
            cwd: Some("/work".to_string()),
            backend_kind: Some("wsl-direct".to_string()),
            control_pipe: DEFAULT_CONTROL_PIPE_NAME.to_string(),
        };
        assert_eq!(
            tmux_command_or_default_shell(Vec::new(), &context),
            vec!["bash".to_string()]
        );

        context.backend_kind = Some("wsl-tmux-control".to_string());
        assert_eq!(
            tmux_command_or_default_shell(Vec::new(), &context),
            vec!["bash".to_string()]
        );

        context.backend_kind = Some("conpty".to_string());
        assert_eq!(
            tmux_command_or_default_shell(Vec::new(), &context),
            vec![r"C:\Windows\System32\cmd.exe".to_string()]
        );
        assert_eq!(
            tmux_command_or_default_shell(vec!["agent".to_string()], &context),
            vec!["agent".to_string()]
        );

        if let Some(previous_comspec) = previous_comspec {
            std::env::set_var("COMSPEC", previous_comspec);
        } else {
            std::env::remove_var("COMSPEC");
        }
        if let Some(previous_wsl) = previous_wsl {
            std::env::set_var("WSL_DISTRO_NAME", previous_wsl);
        } else {
            std::env::remove_var("WSL_DISTRO_NAME");
        }
    }

    #[test]
    fn config_reload_parses_control_options() {
        let options = parse_config_get_options(
            &[
                "--json".to_string(),
                "--pipe".to_string(),
                r"\\.\pipe\agentmux-test".to_string(),
                "--workspace".to_string(),
                "ws_1".to_string(),
            ],
            "config reload",
        )
        .unwrap();

        assert!(options.invoke.json);
        assert_eq!(options.invoke.pipe_name, r"\\.\pipe\agentmux-test");
        assert_eq!(options.params.workspace_id.as_deref(), Some("ws_1"));
    }

    #[test]
    fn config_migrate_cmux_parses_workspace_force_and_json_output() {
        let options = parse_config_migrate_project_options(&[
            "--json".to_string(),
            "--pipe".to_string(),
            r"\\.\pipe\agentmux-test".to_string(),
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--force".to_string(),
        ])
        .unwrap();

        assert!(options.invoke.json);
        assert_eq!(options.invoke.pipe_name, r"\\.\pipe\agentmux-test");
        assert_eq!(options.params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(options.params.overwrite, Some(true));
    }

    #[test]
    fn config_diagnostics_parses_workspace_and_json_output() {
        let options = parse_config_diagnostics_options(&[
            "--json".to_string(),
            "--pipe".to_string(),
            r"\\.\pipe\agentmux-test".to_string(),
            "--workspace".to_string(),
            "ws_1".to_string(),
        ])
        .unwrap();

        assert!(options.invoke.json);
        assert_eq!(options.invoke.pipe_name, r"\\.\pipe\agentmux-test");
        assert_eq!(options.params.workspace_id.as_deref(), Some("ws_1"));
    }

    #[test]
    fn config_schema_parses_output_and_json_options() {
        let options = parse_config_schema_options(&[
            "--json".to_string(),
            "--output".to_string(),
            "agentmux.config.schema.json".to_string(),
        ])
        .unwrap();

        assert!(options.json);
        assert_eq!(
            options.output_path.as_deref(),
            Some(Path::new("agentmux.config.schema.json"))
        );
    }

    #[test]
    fn config_schema_outputs_valid_json_schema() {
        let mut output = Vec::new();
        run_cli(["config", "schema"], &mut output).unwrap();

        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(
            value.get("$id").and_then(serde_json::Value::as_str),
            Some("https://agentmux.local/schemas/agentmux.config.schema.json")
        );
        assert!(value.get("$defs").is_some());
    }

    #[test]
    fn actions_list_parses_workspace_and_json_output() {
        let options = parse_action_list_options(&[
            "--json".to_string(),
            "--pipe".to_string(),
            r"\\.\pipe\agentmux-test".to_string(),
            "--workspace".to_string(),
            "ws_1".to_string(),
        ])
        .unwrap();

        assert!(options.invoke.json);
        assert_eq!(options.invoke.pipe_name, r"\\.\pipe\agentmux-test");
        assert_eq!(options.params.workspace_id.as_deref(), Some("ws_1"));
    }

    #[test]
    fn actions_run_parses_workspace_pane_and_json_output() {
        let options = parse_action_run_options(&[
            "--json".to_string(),
            "--pipe".to_string(),
            r"\\.\pipe\agentmux-test".to_string(),
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--pane".to_string(),
            "%pane_1".to_string(),
            "custom.verify".to_string(),
        ])
        .unwrap();

        assert!(options.invoke.json);
        assert_eq!(options.invoke.pipe_name, r"\\.\pipe\agentmux-test");
        assert_eq!(options.params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(options.params.pane_id.as_deref(), Some("pane_1"));
        assert_eq!(options.params.action_id, "custom.verify");
    }

    #[test]
    fn terminal_run_requires_command() {
        let mut output = Vec::new();
        let result = run_cli(["terminal", "run", "--"], &mut output);
        assert!(matches!(result, Err(CliError::InvalidArgs(_))));
    }

    #[test]
    fn terminal_run_parses_wsl_direct_options() {
        let options = parse_terminal_run_options(&[
            "--backend".to_string(),
            "wsl-direct".to_string(),
            "--distribution".to_string(),
            "Ubuntu".to_string(),
            "--cwd".to_string(),
            r"D:\work\repo".to_string(),
            "--".to_string(),
            "bash".to_string(),
            "-lc".to_string(),
            "pwd".to_string(),
        ])
        .unwrap();

        assert_eq!(options.backend, TerminalBackendOption::WslDirect);
        assert_eq!(options.distribution.as_deref(), Some("Ubuntu"));
        assert_eq!(options.cwd.as_deref(), Some(r"D:\work\repo"));
        assert_eq!(options.command, vec!["bash", "-lc", "pwd"]);
    }

    #[test]
    fn distribution_option_requires_wsl_direct_backend() {
        let error = parse_terminal_run_options(&[
            "--distribution".to_string(),
            "Ubuntu".to_string(),
            "--".to_string(),
            "bash".to_string(),
        ])
        .unwrap_err();

        assert!(matches!(error, CliError::InvalidArgs(_)));
        assert!(error.to_string().contains("wsl-direct"));
    }

    #[test]
    fn workspace_create_parses_control_options() {
        let options = parse_workspace_create_options(&[
            "AgentMux".to_string(),
            "--project".to_string(),
            r"D:\Workspace\irae\agentmux".to_string(),
            "--backend-profile".to_string(),
            "Ubuntu".to_string(),
            "--json".to_string(),
            "--pipe".to_string(),
            r"\\.\pipe\agentmux-test".to_string(),
        ])
        .unwrap();

        assert!(options.invoke.json);
        assert_eq!(options.invoke.pipe_name, r"\\.\pipe\agentmux-test");
        assert_eq!(options.params.name, "AgentMux");
        assert_eq!(
            options.params.project_root.as_deref(),
            Some(r"D:\Workspace\irae\agentmux")
        );
        assert_eq!(options.params.backend_profile.as_deref(), Some("Ubuntu"));
    }

    #[test]
    fn workspace_group_cli_parses_create_update_and_member_options() {
        let create = parse_workspace_group_create_options(&[
            "Agents".to_string(),
            "--anchor".to_string(),
            "ws_a".to_string(),
            "--workspace".to_string(),
            "ws_a".to_string(),
            "--workspace".to_string(),
            "ws_b".to_string(),
            "--collapsed".to_string(),
            "--pinned".to_string(),
            "--color".to_string(),
            "#F97316".to_string(),
            "--icon".to_string(),
            "AG".to_string(),
            "--json".to_string(),
        ])
        .unwrap();
        assert!(create.invoke.json);
        assert_eq!(create.params.name, "Agents");
        assert_eq!(create.params.anchor_workspace_id.as_deref(), Some("ws_a"));
        assert_eq!(
            create.params.workspace_ids.as_ref().unwrap(),
            &vec!["ws_a".to_string(), "ws_b".to_string()]
        );
        assert_eq!(create.params.collapsed, Some(true));
        assert_eq!(create.params.pinned, Some(true));
        assert_eq!(create.params.color.as_deref(), Some("#F97316"));
        assert_eq!(create.params.icon.as_deref(), Some("AG"));

        let update = parse_workspace_group_update_options(&[
            "wsg_1".to_string(),
            "Core agents".to_string(),
            "--expanded".to_string(),
            "--unpinned".to_string(),
            "--sort-order".to_string(),
            "7".to_string(),
        ])
        .unwrap();
        assert_eq!(update.params.group_id, "wsg_1");
        assert_eq!(update.params.name.as_deref(), Some("Core agents"));
        assert_eq!(update.params.collapsed, Some(false));
        assert_eq!(update.params.pinned, Some(false));
        assert_eq!(update.params.sort_order, Some(7));

        let member = parse_workspace_group_member_options(
            &[
                "wsg_1".to_string(),
                "ws_c".to_string(),
                "--position".to_string(),
                "3".to_string(),
            ],
            "workspace group add",
            true,
        )
        .unwrap();
        assert_eq!(member.params.group_id, "wsg_1");
        assert_eq!(member.params.workspace_id, "ws_c");
        assert_eq!(member.params.position, Some(3));

        let delete_error =
            parse_workspace_group_delete_options(&["wsg_1".to_string()]).unwrap_err();
        assert!(delete_error.to_string().contains("--yes"));
    }

    #[test]
    fn workspace_close_requires_confirmation() {
        let error = parse_workspace_close_options(&["ws_1".to_string()]).unwrap_err();
        assert!(error.to_string().contains("--yes"));
    }

    #[test]
    fn workspace_close_parses_policy_and_confirmation() {
        let options = parse_workspace_close_options(&[
            "ws_1".to_string(),
            "--policy".to_string(),
            "terminate_sessions".to_string(),
            "--yes".to_string(),
            "--json".to_string(),
        ])
        .unwrap();

        assert!(options.invoke.json);
        assert_eq!(options.params.workspace_id, "ws_1");
        assert_eq!(options.params.close_policy, "terminate_sessions");
    }

    #[test]
    fn control_token_resolves_from_explicit_token_file() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "agentmux-cli-token-{}-{nanos}.token",
            std::process::id(),
        ));
        std::fs::write(&path, "file-token\n").unwrap();
        let options = ControlInvokeOptions {
            json: false,
            pipe_name: DEFAULT_CONTROL_PIPE_NAME.to_string(),
            token: None,
            token_path: Some(path.to_string_lossy().to_string()),
        };

        assert_eq!(resolve_control_token(&options).unwrap(), "file-token");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn session_spawn_parses_control_params() {
        let options = parse_session_spawn_options(&[
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--backend".to_string(),
            "wsl-tmux-control".to_string(),
            "--distribution".to_string(),
            "Ubuntu".to_string(),
            "--cwd".to_string(),
            r"D:\work\repo".to_string(),
            "--columns".to_string(),
            "100".to_string(),
            "--rows".to_string(),
            "40".to_string(),
            "--durability".to_string(),
            "durable".to_string(),
            "--".to_string(),
            "bash".to_string(),
            "-lc".to_string(),
            "pwd".to_string(),
        ])
        .unwrap();

        assert_eq!(options.params.workspace_id, "ws_1");
        assert_eq!(options.params.backend.as_deref(), Some("wsl-tmux-control"));
        assert_eq!(options.params.backend_profile.as_deref(), Some("Ubuntu"));
        assert_eq!(options.params.cwd.as_deref(), Some(r"D:\work\repo"));
        assert_eq!(options.params.columns, 100);
        assert_eq!(options.params.rows, 40);
        assert_eq!(options.params.durability.as_deref(), Some("durable"));
        assert_eq!(options.params.command, vec!["bash", "-lc", "pwd"]);
    }

    #[test]
    fn session_list_parses_workspace_filter() {
        let options = parse_session_list_options(&[
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--json".to_string(),
        ])
        .unwrap();

        assert!(options.invoke.json);
        assert_eq!(options.params.workspace_id.as_deref(), Some("ws_1"));
    }

    #[test]
    fn session_terminate_requires_confirmation() {
        let error = parse_session_terminate_options(&["ses_1".to_string()]).unwrap_err();
        assert!(error.to_string().contains("--yes"));
    }

    #[test]
    fn session_terminate_parses_mode_and_confirmation() {
        let options = parse_session_terminate_options(&[
            "ses_1".to_string(),
            "--mode".to_string(),
            "kill".to_string(),
            "--confirm".to_string(),
            "--json".to_string(),
        ])
        .unwrap();

        assert!(options.invoke.json);
        assert_eq!(options.params.session_id, "ses_1");
        assert_eq!(options.params.mode, "kill");
    }

    #[test]
    fn agent_set_state_parses_reason() {
        let options = parse_agent_set_state_options(&[
            "ses_1".to_string(),
            "waiting_for_input".to_string(),
            "--reason".to_string(),
            "approval needed".to_string(),
            "--json".to_string(),
        ])
        .unwrap();

        assert!(options.invoke.json);
        assert_eq!(options.params.session_id, "ses_1");
        assert_eq!(options.params.state, "waiting_for_input");
        assert_eq!(options.params.reason.as_deref(), Some("approval needed"));
    }

    #[test]
    fn notification_list_parses_filters() {
        let options = parse_notification_list_options(&[
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--severity".to_string(),
            "warning".to_string(),
            "--include-dismissed".to_string(),
        ])
        .unwrap();

        assert_eq!(options.params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(options.params.severity.as_deref(), Some("warning"));
        assert_eq!(options.params.include_dismissed, Some(true));
    }

    #[test]
    fn notify_parses_cmux_style_metadata() {
        let options = parse_notify_options(&[
            "--title".to_string(),
            "Build".to_string(),
            "--body".to_string(),
            "Done".to_string(),
            "--severity".to_string(),
            "success".to_string(),
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--session".to_string(),
            "ses_1".to_string(),
            "--json".to_string(),
        ])
        .unwrap();

        assert!(options.invoke.json);
        assert_eq!(options.params.title, "Build");
        assert_eq!(options.params.body.as_deref(), Some("Done"));
        assert_eq!(options.params.severity.as_deref(), Some("success"));
        assert_eq!(options.params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(options.params.session_id.as_deref(), Some("ses_1"));
    }

    #[test]
    fn sidebar_status_parses_key_label_and_display_options() {
        let options = parse_sidebar_status_set_options(&[
            "build".to_string(),
            "compiling".to_string(),
            "--icon".to_string(),
            "hammer".to_string(),
            "--color".to_string(),
            "#ff9500".to_string(),
            "--priority".to_string(),
            "80".to_string(),
            "--workspace".to_string(),
            "ws_1".to_string(),
        ])
        .unwrap();

        assert_eq!(options.params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(options.params.key, "build");
        assert_eq!(options.params.label, "compiling");
        assert_eq!(options.params.icon.as_deref(), Some("hammer"));
        assert_eq!(options.params.color.as_deref(), Some("#ff9500"));
        assert_eq!(options.params.priority, Some(80));
    }

    #[test]
    fn sidebar_progress_and_log_parse_script_friendly_args() {
        let progress = parse_sidebar_progress_set_options(&[
            "0.42".to_string(),
            "--label".to_string(),
            "Building".to_string(),
            "--workspace".to_string(),
            "ws_1".to_string(),
        ])
        .unwrap();
        assert_eq!(progress.params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(progress.params.value, 0.42);
        assert_eq!(progress.params.label.as_deref(), Some("Building"));

        let log = parse_sidebar_log_options(&[
            "--level".to_string(),
            "success".to_string(),
            "--source".to_string(),
            "test".to_string(),
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--".to_string(),
            "All".to_string(),
            "tests".to_string(),
            "passed".to_string(),
        ])
        .unwrap();
        assert_eq!(log.params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(log.params.level.as_deref(), Some("success"));
        assert_eq!(log.params.source.as_deref(), Some("test"));
        assert_eq!(log.params.message, "All tests passed");
    }

    #[test]
    fn identify_parses_workspace_and_json_output() {
        let options = parse_identify_options(&[
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--json".to_string(),
        ])
        .unwrap();

        assert!(options.invoke.json);
        assert_eq!(options.params.workspace_id.as_deref(), Some("ws_1"));
    }

    #[test]
    fn events_watch_parses_polling_filters() {
        let options = parse_events_watch_options(&[
            "--workspace".to_string(),
            "ws_1".to_string(),
            "--session".to_string(),
            "ses_1".to_string(),
            "--type".to_string(),
            "session.state_changed".to_string(),
            "--type".to_string(),
            "session.output".to_string(),
            "--max-events".to_string(),
            "5".to_string(),
            "--interval-ms".to_string(),
            "50".to_string(),
            "--once".to_string(),
            "--limit".to_string(),
            "2".to_string(),
            "--after-event".to_string(),
            "evt_00000012".to_string(),
        ])
        .unwrap();

        assert_eq!(options.params.workspace_id.as_deref(), Some("ws_1"));
        assert_eq!(options.params.session_id.as_deref(), Some("ses_1"));
        assert_eq!(
            options.params.types,
            Some(vec![
                "session.state_changed".to_string(),
                "session.output".to_string()
            ])
        );
        assert_eq!(
            options.params.after_event_id.as_deref(),
            Some("evt_00000012")
        );
        assert_eq!(options.interval_ms, 50);
        assert!(options.once);
        assert_eq!(options.limit, Some(2));
    }

    #[test]
    fn session_read_recent_defaults_to_script_friendly_size() {
        let options = parse_session_read_recent_options(&["ses_1".to_string()]).unwrap();

        assert_eq!(options.params.session_id, "ses_1");
        assert_eq!(options.params.max_bytes, 8192);
    }

    #[test]
    #[cfg(windows)]
    fn terminal_run_prints_conpty_output() {
        let mut output = Vec::new();
        run_cli(
            [
                "terminal",
                "run",
                "--",
                "cmd.exe",
                "/d",
                "/q",
                "/c",
                "echo agentmux-cli-test",
            ],
            &mut output,
        )
        .unwrap();

        assert!(String::from_utf8_lossy(&output).contains("agentmux-cli-test"));
    }

    #[test]
    fn strips_common_vt_sequences_for_cli_output() {
        assert_eq!(
            strip_vt_sequences("\x1b[?25l\x1b[2J\x1b[Hhello\r\n\x1b]0;title\x07\x1b[?25h"),
            "hello\n"
        );
    }
}
