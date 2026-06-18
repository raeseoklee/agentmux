use std::fmt;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use agentmux_backend::SessionBackend;
use agentmux_backend_conpty::ConptyBackend;
use agentmux_backend_wsl::{WslDirectBackend, WslDirectConfig};
use agentmux_core::{RuntimeControlPlane, TerminalRuntime};
use agentmux_ipc::{
    default_control_token_path, read_control_token, AgentAttentionListResult,
    AgentListAttentionParams, AgentSetStateParams, AgentStateResult, DiagnosticsExportResult,
    EventFrame, EventPollParams, EventPollResult, EventSubscribeParams, EventSubscribeResult,
    NamedPipeEventStream, NotificationDismissParams, NotificationListParams,
    NotificationListResult, NotificationSummaryResult, RecoveryDiagnosticsResult, RequestEnvelope,
    ResponseEnvelope, ResponseOutcome, SessionIdParams, SessionListParams, SessionListResult,
    SessionReadRecentParams, SessionReadRecentResult, SessionSendKeyParams, SessionSendTextParams,
    SessionSpawnParams, SessionSpawnResult, SessionSummaryResult, SessionTerminateParams,
    WorkspaceCloseParams, WorkspaceCloseResult, WorkspaceCreateParams, WorkspaceDetailResult,
    WorkspaceIdParams, WorkspaceListResult, WorkspaceRenameParams, WorkspaceSummaryResult,
    DEFAULT_CONTROL_PIPE_NAME,
};

pub const COMMAND_FAMILIES: &[&str] = &[
    "system",
    "workspace",
    "pane",
    "surface",
    "terminal",
    "notification",
    "events",
    "browser",
    "agent",
    "diagnostics",
    "session",
    "config",
];

pub fn usage() -> String {
    format!(
        "agentmux <{}> <command> [options]\n\nTry: agentmux workspace list\nTry: agentmux workspace create AgentMux --project D:\\work\\repo\nTry: agentmux workspace close <id> --policy fail_if_running --yes\nTry: agentmux session spawn --workspace <id> -- cmd.exe /d /q\nTry: agentmux session list --workspace <id>\nTry: agentmux session terminate <id> --mode soft --yes\nTry: agentmux agent set-state <session-id> waiting_for_input --reason \"needs input\"\nTry: agentmux notification list --severity warning\nTry: agentmux events watch --workspace <id>\nTry: agentmux diagnostics export --json\nTry: agentmux terminal run -- cmd.exe /d /q /c \"echo agentmux\"\nTry: agentmux terminal run --backend wsl-direct --distribution Ubuntu --cwd D:\\work\\repo -- bash -lc pwd",
        COMMAND_FAMILIES.join("|")
    )
}

pub fn run_cli<I, S, W>(args: I, mut output: W) -> Result<(), CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    W: Write,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();

    match args.as_slice() {
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
        [family, rest @ ..] if family == "diagnostics" => {
            let options = parse_no_params_control_options(rest, "diagnostics")?;
            run_diagnostics(options, &mut output)
        }
        [family, command, rest @ ..] if family == "terminal" && command == "run" => {
            let options = parse_terminal_run_options(rest)?;
            run_terminal_command(options, &mut output)
        }
        _ => {
            writeln!(output, "{}", usage())?;
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
            columns,
            rows,
            durability,
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
        "--pipe" => {
            invoke.pipe_name = option_value(args, *index, "--pipe")?.to_string();
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

fn parse_u64_option(value: &str, option: &str) -> Result<u64, CliError> {
    value
        .parse::<u64>()
        .map_err(|_| CliError::InvalidArgs(format!("{option} requires a positive integer.")))
}

fn parse_usize_option(value: &str, option: &str) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .map_err(|_| CliError::InvalidArgs(format!("{option} requires a positive integer.")))
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
    for backend in diagnostics.backend_health {
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
    for queue in diagnostics.queue_pressure {
        writeln!(
            output,
            "{}\t{}\tdepth={}/{}\tdropped={}",
            queue.queue, queue.state, queue.depth, queue.capacity, queue.dropped_count
        )?;
    }
    writeln!(
        output,
        "browser failures: {}",
        diagnostics.browser.failures.len()
    )?;
    writeln!(output, "notifications: {}", diagnostics.notifications.len())?;
    Ok(())
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

    #[test]
    fn usage_mentions_workspace_and_session() {
        let text = usage();
        assert!(text.contains("workspace"));
        assert!(text.contains("session"));
        assert!(text.contains("wsl-direct"));
        assert!(text.contains("diagnostics"));
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
