//! Preview-first, Windows-oriented hook installation for Claude Code and Codex.
//!
//! The installer only manages hook entries tagged with `AgentMux`. It merges
//! JSON objects, copies a timestamped backup before every write, and writes via
//! a sibling temporary file plus rename. Existing user hook commands and Codex
//! `notify` configuration are never replaced.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

use super::CliError;

const MANAGED_DESCRIPTION: &str = "AgentMux lifecycle bridge (managed by AgentMux)";
const EVENTS: &[(&str, &str)] = &[
    ("SessionStart", "started"),
    ("UserPromptSubmit", "running"),
    ("PermissionRequest", "waiting_for_input"),
    ("Notification", "waiting_for_input"),
    ("Stop", "completed"),
    ("SubagentStart", "started"),
    ("SubagentStop", "completed"),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Provider {
    Claude,
    Codex,
}

impl Provider {
    fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "claude" | "claude-code" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            _ => Err(CliError::InvalidArgs(
                "--provider must be claude, codex, or all.".to_string(),
            )),
        }
    }

    fn source(self) -> &'static str {
        match self {
            Self::Claude => "claude_hook",
            Self::Codex => "codex_hook",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
        }
    }
}

#[derive(Debug)]
struct HookInstallOptions {
    providers: Vec<Provider>,
    confirmed: bool,
    json: bool,
    home: Option<PathBuf>,
}

pub(super) fn run_command<W>(args: &[String], output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let (command, rest) = args.split_first().ok_or_else(|| {
        CliError::InvalidArgs("agent hooks requires install, preview, or status.".to_string())
    })?;
    let mut options = parse_options(rest)?;
    match command.as_str() {
        "preview" | "status" => {
            options.confirmed = false;
            run_install(options, output)
        }
        "install" => run_install(options, output),
        other => Err(CliError::InvalidArgs(format!(
            "unknown agent hooks command '{other}'. Expected install, preview, or status."
        ))),
    }
}

fn parse_options(args: &[String]) -> Result<HookInstallOptions, CliError> {
    let mut providers = vec![Provider::Claude, Provider::Codex];
    let mut confirmed = false;
    let mut json = false;
    let mut home = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--provider" => {
                let value = value(args, index, "--provider")?;
                providers = if value == "all" {
                    vec![Provider::Claude, Provider::Codex]
                } else {
                    vec![Provider::parse(value)?]
                };
                index += 2;
            }
            "--yes" | "--confirm" => {
                confirmed = true;
                index += 1;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            // Test and enterprise automation hook this root explicitly. Normal
            // users should omit it so their Windows profile directories are used.
            "--home" => {
                home = Some(PathBuf::from(value(args, index, "--home")?));
                index += 2;
            }
            option if option.starts_with("--") => {
                return Err(CliError::InvalidArgs(format!(
                    "unknown agent hooks option '{option}'."
                )));
            }
            argument => {
                return Err(CliError::InvalidArgs(format!(
                    "agent hooks does not accept argument '{argument}'."
                )));
            }
        }
    }
    Ok(HookInstallOptions {
        providers,
        confirmed,
        json,
        home,
    })
}

fn run_install<W>(options: HookInstallOptions, output: &mut W) -> Result<(), CliError>
where
    W: Write,
{
    let home = options.home.unwrap_or_else(resolve_home);
    let adapter = adapter_path(&home);
    let mut plans = Vec::new();
    for provider in options.providers {
        plans.push(plan_provider(provider, &home, &adapter)?);
    }

    if options.json {
        let items = plans
            .iter()
            .map(|plan| {
                json!({
                    "provider": plan.provider.label(),
                    "config_path": plan.config_path,
                    "adapter_path": adapter,
                    "changed": plan.changed,
                    "conflict": plan.conflict,
                    "preview": plan.preview,
                    "will_write": options.confirmed && plan.changed && plan.conflict.is_none(),
                })
            })
            .collect::<Vec<_>>();
        writeln!(
            output,
            "{}",
            serde_json::to_string_pretty(&items).map_err(json_error)?
        )?;
    } else {
        for plan in &plans {
            writeln!(
                output,
                "{}: {}",
                plan.provider.label(),
                plan.config_path.display()
            )?;
            if let Some(conflict) = &plan.conflict {
                writeln!(output, "  conflict: {conflict}")?;
            } else if plan.changed {
                writeln!(output, "  preview: {}", plan.preview)?;
                writeln!(
                    output,
                    "  {}",
                    if options.confirmed {
                        "will install"
                    } else {
                        "dry run; re-run with --yes to install"
                    }
                )?;
            } else {
                writeln!(output, "  already installed")?;
            }
        }
    }

    if !options.confirmed {
        return Ok(());
    }
    if plans.iter().any(|plan| plan.conflict.is_some()) {
        return Err(CliError::InvalidArgs(
            "hook installation stopped because an existing user command needs manual review; no files were written."
                .to_string(),
        ));
    }

    let adapter_changed = write_adapter_if_needed(&adapter)?;
    for plan in plans {
        if plan.changed {
            backup_and_atomic_write(&plan.config_path, &plan.updated)?;
        }
    }
    if !options.json {
        writeln!(
            output,
            "AgentMux hook adapter {} at {}.",
            if adapter_changed {
                "installed"
            } else {
                "already present"
            },
            adapter.display()
        )?;
        writeln!(
            output,
            "Review non-managed Codex hooks with /hooks before they run."
        )?;
    }
    Ok(())
}

struct ProviderPlan {
    provider: Provider,
    config_path: PathBuf,
    updated: String,
    changed: bool,
    conflict: Option<String>,
    preview: String,
}

fn plan_provider(
    provider: Provider,
    home: &Path,
    adapter: &Path,
) -> Result<ProviderPlan, CliError> {
    let config_path = match provider {
        Provider::Claude => home.join(".claude").join("settings.json"),
        Provider::Codex => home.join(".codex").join("hooks.json"),
    };
    let raw = read_or_empty_object(&config_path)?;
    let mut root: Value = serde_json::from_str(&raw).map_err(|error| {
        CliError::InvalidArgs(format!(
            "{} is not valid JSON; AgentMux will not overwrite it: {error}",
            config_path.display()
        ))
    })?;
    let root_object = root.as_object_mut().ok_or_else(|| {
        CliError::InvalidArgs(format!(
            "{} must contain a JSON object; AgentMux will not overwrite it.",
            config_path.display()
        ))
    })?;

    if provider == Provider::Codex {
        let config_toml = home.join(".codex").join("config.toml");
        if config_toml.exists() {
            let content = fs::read_to_string(&config_toml).map_err(CliError::Io)?;
            if content
                .lines()
                .any(|line| line.trim_start().starts_with("notify"))
            {
                return Ok(ProviderPlan {
                    provider,
                    config_path,
                    updated: raw,
                    changed: false,
                    conflict: Some(format!(
                        "{} contains an existing Codex notify command. It is preserved; install manually after reviewing its chaining behavior.",
                        config_toml.display()
                    )),
                    preview: "no write".to_string(),
                });
            }
        }
    }

    let hooks = ensure_object(root_object, "hooks")?;
    let mut added = 0usize;
    for (event, state) in EVENTS {
        let entries = ensure_array(hooks, event)?;
        if entries.iter().any(is_agentmux_hook_group) {
            continue;
        }
        entries.push(agentmux_hook_group(provider, event, state, adapter));
        added += 1;
    }
    let updated = serde_json::to_string_pretty(&root).map_err(json_error)? + "\n";
    Ok(ProviderPlan {
        provider,
        config_path,
        changed: added > 0,
        conflict: None,
        preview: format!("add {added} lifecycle event handler(s)"),
        updated,
    })
}

fn ensure_object<'a>(
    object: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Map<String, Value>, CliError> {
    let value = object.entry(key.to_string()).or_insert_with(|| json!({}));
    value.as_object_mut().ok_or_else(|| {
        CliError::InvalidArgs(format!(
            "existing '{key}' must be an object; refusing to overwrite it."
        ))
    })
}

fn ensure_array<'a>(
    object: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Vec<Value>, CliError> {
    let value = object.entry(key.to_string()).or_insert_with(|| json!([]));
    value.as_array_mut().ok_or_else(|| {
        CliError::InvalidArgs(format!(
            "existing hook event '{key}' must be an array; refusing to overwrite it."
        ))
    })
}

fn agentmux_hook_group(provider: Provider, event: &str, state: &str, adapter: &Path) -> Value {
    let command_windows = format!(
        "powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{}\" --provider {} --event {} --state {}",
        adapter.display(), provider.source(), event, state
    );
    let handler = match provider {
        Provider::Claude => json!({
            "type": "command",
            "shell": "powershell",
            "command": command_windows,
            "timeout": 10,
            "statusMessage": MANAGED_DESCRIPTION,
        }),
        Provider::Codex => json!({
            "type": "command",
            "commandWindows": command_windows,
            "timeout": 10,
            "statusMessage": MANAGED_DESCRIPTION,
        }),
    };
    json!({
        "matcher": "",
        "hooks": [handler]
    })
}

fn is_agentmux_hook_group(value: &Value) -> bool {
    value
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hooks| {
            hooks.iter().any(|handler| {
                handler.get("statusMessage").and_then(Value::as_str) == Some(MANAGED_DESCRIPTION)
            })
        })
}

fn adapter_path(home: &Path) -> PathBuf {
    home.join("AppData")
        .join("Roaming")
        .join("AgentMux")
        .join("hooks")
        .join("agentmux-hook-adapter.ps1")
}

fn write_adapter_if_needed(path: &Path) -> Result<bool, CliError> {
    let content = hook_adapter_script();
    if fs::read_to_string(path).ok().as_deref() == Some(content.as_str()) {
        return Ok(false);
    }
    backup_and_atomic_write(path, &content)?;
    Ok(true)
}

fn hook_adapter_script() -> String {
    r#"param(
  [Parameter(Mandatory = $true)][string]$provider,
  [Parameter(Mandatory = $true)][string]$event,
  [Parameter(Mandatory = $true)][string]$state
)
$ErrorActionPreference = 'Stop'
$raw = [Console]::In.ReadToEnd()
$payload = if ([string]::IsNullOrWhiteSpace($raw)) { $null } else { $raw | ConvertFrom-Json }
$workspace = $env:AGENTMUX_WORKSPACE_ID
$session = if ($payload -and $payload.session_id) { [string]$payload.session_id } elseif ($env:AGENTMUX_SESSION_ID) { $env:AGENTMUX_SESSION_ID } else { '' }
if ([string]::IsNullOrWhiteSpace($workspace) -or [string]::IsNullOrWhiteSpace($session)) { exit 0 }
$sequence = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$observed = [DateTime]::UtcNow.ToString('o')
$reason = if ($payload -and $payload.hook_event_name) { [string]$payload.hook_event_name } else { $event }
$argv = @('agent', 'hook-state', '--workspace', $workspace, '--session', $session, '--sequence', "$sequence", '--state', $state, '--source', $provider, '--observed-at', $observed, '--reason', $reason)
& agentmux @argv *> $null
exit 0
"#.to_string()
}

fn read_or_empty_object(path: &Path) -> Result<String, CliError> {
    if path.exists() {
        fs::read_to_string(path).map_err(CliError::Io)
    } else {
        Ok("{}".to_string())
    }
}

fn backup_and_atomic_write(path: &Path, content: &str) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(CliError::Io)?;
    }
    if path.exists() {
        let backup = path.with_extension(format!("bak-{}", timestamp()));
        fs::copy(path, &backup).map_err(CliError::Io)?;
    }
    let temporary = path.with_extension(format!("tmp-{}", timestamp()));
    fs::write(&temporary, content).map_err(CliError::Io)?;
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        CliError::Io(error)
    })
}

fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
fn resolve_home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}
fn value<'a>(args: &'a [String], index: usize, option: &str) -> Result<&'a str, CliError> {
    args.get(index + 1)
        .map(String::as_str)
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| CliError::InvalidArgs(format!("{option} requires a value.")))
}
fn json_error(error: serde_json::Error) -> CliError {
    CliError::Control(format!("failed to serialize hook configuration: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("agentmux-hooks-{name}-{}", timestamp()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn claude_plan_preserves_unrelated_hook_groups() {
        let home = temp_home("claude");
        let config = home.join(".claude").join("settings.json");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, r#"{"hooks":{"PostToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":"user-tool"}]}]}}"#).unwrap();
        let plan = plan_provider(Provider::Claude, &home, &adapter_path(&home)).unwrap();
        let value: Value = serde_json::from_str(&plan.updated).unwrap();
        assert_eq!(
            value["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
            "user-tool"
        );
        assert!(value["hooks"]["SessionStart"].is_array());
        let handler = &value["hooks"]["SessionStart"][0]["hooks"][0];
        assert_eq!(handler["shell"], "powershell");
        assert!(handler["command"]
            .as_str()
            .is_some_and(|command| command.contains("agentmux-hook-adapter.ps1")));
        assert!(handler.get("commandWindows").is_none());
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn codex_plan_uses_windows_command_field() {
        let home = temp_home("codex-command");
        let plan = plan_provider(Provider::Codex, &home, &adapter_path(&home)).unwrap();
        let value: Value = serde_json::from_str(&plan.updated).unwrap();
        let handler = &value["hooks"]["SessionStart"][0]["hooks"][0];
        assert!(handler["commandWindows"]
            .as_str()
            .is_some_and(|command| command.contains("agentmux-hook-adapter.ps1")));
        assert!(handler.get("command").is_none());
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn codex_notify_conflict_never_writes() {
        let home = temp_home("codex-notify");
        let config = home.join(".codex").join("config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "notify = [\"existing-command\"]\n").unwrap();
        let plan = plan_provider(Provider::Codex, &home, &adapter_path(&home)).unwrap();
        assert!(plan.conflict.is_some());
        assert!(!plan.changed);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn adapter_uses_argument_array_without_payload_interpolation() {
        let script = hook_adapter_script();
        assert!(script.contains("$argv = @("));
        assert!(script.contains("& agentmux @argv"));
        assert!(!script.contains("Invoke-Expression"));
    }
}
