use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use agentmux_backend::{
    AttachRequest, BackendError, BackendEvent, BackendKind, BackendResult, CommandSpec, InputEvent,
    SessionBackend, SessionHandle, SpawnRequest, TerminalSize, TerminationMode,
};
use agentmux_backend_conpty::ConptyBackend;

mod pipe;
pub use pipe::PipeBackend;

pub const WSL_EXE: &str = "wsl.exe";
pub const DEFAULT_WSL_CWD: &str = "~";
pub const DEFAULT_WSL_LAUNCH_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WslDistribution {
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WslDirectConfig {
    pub distribution: Option<String>,
    pub default_cwd: String,
    pub validate_distribution: bool,
    pub validate_launch: bool,
    pub prefer_wslpath: bool,
    pub launch_timeout: Duration,
}

impl Default for WslDirectConfig {
    fn default() -> Self {
        Self {
            distribution: None,
            default_cwd: DEFAULT_WSL_CWD.to_string(),
            validate_distribution: true,
            validate_launch: true,
            prefer_wslpath: true,
            launch_timeout: DEFAULT_WSL_LAUNCH_TIMEOUT,
        }
    }
}

impl WslDirectConfig {
    pub fn for_distribution(distribution: impl Into<String>) -> Self {
        Self {
            distribution: Some(distribution.into()),
            ..Self::default()
        }
    }

    pub fn for_interactive_terminal() -> Self {
        Self {
            validate_distribution: false,
            validate_launch: false,
            prefer_wslpath: false,
            ..Self::default()
        }
    }

    pub fn for_interactive_distribution(distribution: impl Into<String>) -> Self {
        Self {
            distribution: Some(distribution.into()),
            ..Self::for_interactive_terminal()
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WslDiagnosticCode {
    WslUnavailable,
    NoDistributions,
    MissingDistribution,
    InvalidCwd,
    LaunchTimeout,
}

impl WslDiagnosticCode {
    pub fn as_str(self) -> &'static str {
        match self {
            WslDiagnosticCode::WslUnavailable => "wsl_unavailable",
            WslDiagnosticCode::NoDistributions => "no_wsl_distributions",
            WslDiagnosticCode::MissingDistribution => "wsl_distribution_not_found",
            WslDiagnosticCode::InvalidCwd => "invalid_wsl_cwd",
            WslDiagnosticCode::LaunchTimeout => "wsl_launch_timeout",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WslDiagnostic {
    pub code: WslDiagnosticCode,
    pub message: String,
}

impl WslDiagnostic {
    pub fn wsl_unavailable(message: impl Into<String>) -> Self {
        Self {
            code: WslDiagnosticCode::WslUnavailable,
            message: message.into(),
        }
    }

    pub fn no_distributions() -> Self {
        Self {
            code: WslDiagnosticCode::NoDistributions,
            message: "No WSL distributions were returned by discovery.".to_string(),
        }
    }

    pub fn missing_distribution(distribution: impl AsRef<str>) -> Self {
        Self {
            code: WslDiagnosticCode::MissingDistribution,
            message: format!(
                "WSL distribution '{}' was not found.",
                distribution.as_ref()
            ),
        }
    }

    pub fn invalid_cwd(cwd: impl AsRef<str>) -> Self {
        Self {
            code: WslDiagnosticCode::InvalidCwd,
            message: format!(
                "Unable to resolve '{}' as a WSL working directory.",
                cwd.as_ref()
            ),
        }
    }

    pub fn launch_timeout(distribution: Option<&str>, cwd: &str, timeout: Duration) -> Self {
        let distribution = distribution.unwrap_or("<default>");
        Self {
            code: WslDiagnosticCode::LaunchTimeout,
            message: format!(
                "WSL launch timed out after {} ms for distribution '{}' in '{}'.",
                timeout.as_millis(),
                distribution,
                cwd
            ),
        }
    }
}

pub struct WslDirectBackend<B = ConptyBackend> {
    config: WslDirectConfig,
    inner: B,
}

impl WslDirectBackend<ConptyBackend> {
    pub fn new() -> Self {
        Self::with_config(WslDirectConfig::default())
    }

    pub fn for_distribution(distribution: impl Into<String>) -> Self {
        Self::with_config(WslDirectConfig::for_distribution(distribution))
    }

    pub fn with_config(config: WslDirectConfig) -> Self {
        Self::with_backend(config, ConptyBackend::new())
    }
}

impl Default for WslDirectBackend<ConptyBackend> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B> WslDirectBackend<B> {
    pub fn with_backend(config: WslDirectConfig, inner: B) -> Self {
        Self { config, inner }
    }

    pub fn config(&self) -> &WslDirectConfig {
        &self.config
    }

    pub fn inner(&self) -> &B {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut B {
        &mut self.inner
    }
}

pub fn distribution_discovery_command() -> CommandSpec {
    CommandSpec::with_args(WSL_EXE, vec!["--list".to_string(), "--quiet".to_string()])
}

pub fn distribution_status_command() -> CommandSpec {
    CommandSpec::with_args(WSL_EXE, vec!["--status".to_string()])
}

pub fn distribution_verbose_command() -> CommandSpec {
    CommandSpec::with_args(WSL_EXE, vec!["--list".to_string(), "--verbose".to_string()])
}

pub fn parse_distribution_list(output: &str) -> Vec<WslDistribution> {
    clean_wsl_output(output)
        .lines()
        .filter_map(|line| {
            let mut name = line.trim().trim_start_matches('\u{feff}').trim();
            let is_default = name.starts_with('*');
            if is_default {
                name = name.trim_start_matches('*').trim();
            }
            if name.is_empty() {
                None
            } else {
                Some(WslDistribution {
                    name: name.to_string(),
                    is_default,
                })
            }
        })
        .collect()
}

pub fn parse_default_distribution_status(output: &str) -> Option<String> {
    clean_wsl_output(output).lines().find_map(|line| {
        let line = line.trim().trim_start_matches('\u{feff}').trim();
        let (label, value) = line.split_once(':')?;
        let label = label.trim().to_ascii_lowercase();
        let value = value.trim();
        if label.contains("default") && label.contains("distribution") && !value.is_empty() {
            Some(value.to_string())
        } else {
            None
        }
    })
}

pub fn parse_default_distribution_verbose(output: &str) -> Option<String> {
    clean_wsl_output(output).lines().find_map(|line| {
        let mut line = line.trim().trim_start_matches('\u{feff}').trim();
        if !line.starts_with('*') {
            return None;
        }
        line = line.trim_start_matches('*').trim();
        let name = strip_verbose_distribution_columns(line)?;
        (!name.is_empty()).then(|| name.to_string())
    })
}

pub fn apply_default_distribution(
    distributions: &mut [WslDistribution],
    default_name: &str,
) -> bool {
    let default_name = default_name.trim();
    if default_name.is_empty() {
        return false;
    }

    let mut matched = false;
    for distribution in distributions {
        let is_default = distribution.name.eq_ignore_ascii_case(default_name);
        distribution.is_default = is_default;
        matched |= is_default;
    }
    matched
}

pub fn distributions_or_diagnostic(output: &str) -> Result<Vec<WslDistribution>, WslDiagnostic> {
    let distributions = parse_distribution_list(output);
    if distributions.is_empty() {
        Err(WslDiagnostic::no_distributions())
    } else {
        Ok(distributions)
    }
}

pub fn discover_wsl_distributions() -> Result<Vec<WslDistribution>, WslDiagnostic> {
    let command = distribution_discovery_command();
    let output = hidden_command(&command).output().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            WslDiagnostic::wsl_unavailable("wsl.exe was not found.")
        } else {
            WslDiagnostic::wsl_unavailable(format!(
                "Failed to run wsl.exe distribution discovery: {error}"
            ))
        }
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WslDiagnostic::wsl_unavailable(format!(
            "wsl.exe distribution discovery failed with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut distributions = distributions_or_diagnostic(&stdout)?;
    if let Some(default_name) = discover_default_wsl_distribution() {
        apply_default_distribution(&mut distributions, &default_name);
    }
    Ok(distributions)
}

fn discover_default_wsl_distribution() -> Option<String> {
    let verbose = distribution_verbose_command();
    if let Ok(output) = hidden_command(&verbose).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(default_name) = parse_default_distribution_verbose(&stdout) {
                return Some(default_name);
            }
        }
    }

    let status = distribution_status_command();
    if let Ok(output) = hidden_command(&status).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            return parse_default_distribution_status(&stdout);
        }
    }

    None
}

fn clean_wsl_output(output: &str) -> String {
    output.replace('\0', "")
}

fn strip_verbose_distribution_columns(line: &str) -> Option<&str> {
    let without_version = trim_last_whitespace_token(line)?;
    let without_state = trim_last_whitespace_token(without_version)?;
    Some(without_state.trim())
}

fn trim_last_whitespace_token(value: &str) -> Option<&str> {
    let value = value.trim_end();
    let (index, _) = value
        .char_indices()
        .rev()
        .find(|(_, character)| character.is_whitespace())?;
    Some(value[..index].trim_end())
}

pub fn validate_selected_distribution(
    distributions: &[WslDistribution],
    selected: Option<&str>,
) -> Result<(), WslDiagnostic> {
    let Some(selected) = selected else {
        if distributions.is_empty() {
            return Err(WslDiagnostic::no_distributions());
        }
        return Ok(());
    };

    if distributions
        .iter()
        .any(|distribution| distribution.name == selected)
    {
        Ok(())
    } else {
        Err(WslDiagnostic::missing_distribution(selected))
    }
}

pub fn validate_wsl_distribution(selected: Option<&str>) -> Result<(), WslDiagnostic> {
    let distributions = discover_wsl_distributions()?;
    validate_selected_distribution(&distributions, selected)
}

pub fn direct_launch_command(distribution: &str, cwd: &str, command: CommandSpec) -> CommandSpec {
    direct_launch_command_with_distribution(Some(distribution), cwd, command)
}

pub fn wslpath_command(distribution: Option<&str>, windows_path: &str) -> CommandSpec {
    let mut args = Vec::new();
    if let Some(distribution) = distribution {
        args.push("--distribution".to_string());
        args.push(distribution.to_string());
    }
    args.extend([
        "--exec".to_string(),
        "wslpath".to_string(),
        "-a".to_string(),
        windows_path.to_string(),
    ]);
    CommandSpec::with_args(WSL_EXE, args)
}

pub fn launch_probe_command(distribution: Option<&str>, cwd: &str) -> CommandSpec {
    let mut args = Vec::new();
    if let Some(distribution) = distribution {
        args.push("--distribution".to_string());
        args.push(distribution.to_string());
    }

    args.extend([
        "--cd".to_string(),
        cwd.to_string(),
        "--exec".to_string(),
        "sh".to_string(),
        "-lc".to_string(),
        "exit 0".to_string(),
    ]);
    CommandSpec::with_args(WSL_EXE, args)
}

pub fn direct_launch_command_with_distribution(
    distribution: Option<&str>,
    cwd: &str,
    command: CommandSpec,
) -> CommandSpec {
    let mut args = Vec::new();
    if let Some(distribution) = distribution {
        args.push("--distribution".to_string());
        args.push(distribution.to_string());
    }

    args.extend([
        "--cd".to_string(),
        cwd.to_string(),
        "--exec".to_string(),
        command.executable,
    ]);
    args.extend(command.args);
    CommandSpec::with_args(WSL_EXE, args)
}

pub fn resolve_wsl_cwd(cwd: Option<&str>, default_cwd: &str) -> Result<String, WslDiagnostic> {
    let Some(cwd) = cwd else {
        return Ok(default_cwd.to_string());
    };

    if is_explicit_wsl_path(cwd) {
        return Ok(cwd.to_string());
    }

    fallback_windows_path_to_wsl(cwd).ok_or_else(|| WslDiagnostic::invalid_cwd(cwd))
}

pub fn resolve_wsl_cwd_with_distribution(
    cwd: Option<&str>,
    default_cwd: &str,
    distribution: Option<&str>,
    prefer_wslpath: bool,
) -> Result<String, WslDiagnostic> {
    let Some(cwd) = cwd else {
        return Ok(default_cwd.to_string());
    };

    if is_explicit_wsl_path(cwd) {
        return Ok(cwd.to_string());
    }

    if prefer_wslpath {
        if let Some(resolved) = resolve_windows_path_with_wslpath(distribution, cwd) {
            return Ok(resolved);
        }
    }

    fallback_windows_path_to_wsl(cwd).ok_or_else(|| WslDiagnostic::invalid_cwd(cwd))
}

pub fn ensure_wsl_launch_not_timed_out(
    distribution: Option<&str>,
    cwd: &str,
    timeout: Duration,
) -> Result<(), WslDiagnostic> {
    let command = launch_probe_command(distribution, cwd);
    match run_command_with_timeout(&command, timeout) {
        Ok(TimedCommandStatus::Exited) => Ok(()),
        Ok(TimedCommandStatus::TimedOut) => {
            Err(WslDiagnostic::launch_timeout(distribution, cwd, timeout))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(WslDiagnostic::wsl_unavailable("wsl.exe was not found."))
        }
        Err(error) => Err(WslDiagnostic::wsl_unavailable(format!(
            "Failed to run WSL launch probe: {error}"
        ))),
    }
}

pub fn resolve_windows_path_with_wslpath(
    distribution: Option<&str>,
    windows_path: &str,
) -> Option<String> {
    fallback_windows_path_to_wsl(windows_path)?;
    let command = wslpath_command(distribution, windows_path);
    let output = hidden_command(&command).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let resolved = text.trim();
    if resolved.is_empty() {
        None
    } else {
        Some(resolved.to_string())
    }
}

pub fn is_explicit_wsl_path(path: &str) -> bool {
    path.starts_with('/') || path == "~" || path.starts_with("~/")
}

pub fn fallback_windows_path_to_wsl(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    if bytes.len() < 3 || bytes[1] != b':' {
        return None;
    }

    let drive = bytes[0] as char;
    if !drive.is_ascii_alphabetic() {
        return None;
    }

    let rest = path[2..].trim_start_matches(['\\', '/']);
    let rest = rest.replace('\\', "/");
    Some(format!("/mnt/{}/{}", drive.to_ascii_lowercase(), rest))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TimedCommandStatus {
    Exited,
    TimedOut,
}

fn run_command_with_timeout(
    command: &CommandSpec,
    timeout: Duration,
) -> std::io::Result<TimedCommandStatus> {
    let mut command = hidden_command(command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + timeout;

    loop {
        if child.try_wait()?.is_some() {
            return Ok(TimedCommandStatus::Exited);
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(TimedCommandStatus::TimedOut);
        }

        let sleep_for = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(10));
        thread::sleep(sleep_for);
    }
}

fn hidden_command(spec: &CommandSpec) -> Command {
    let mut command = Command::new(&spec.executable);
    command.args(&spec.args);
    hide_console_window(&mut command);
    command
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

#[allow(dead_code)]
fn legacy_direct_launch_command(
    distribution: &str,
    cwd: &str,
    command: CommandSpec,
) -> CommandSpec {
    let mut args = vec![
        "--distribution".to_string(),
        distribution.to_string(),
        "--cd".to_string(),
        cwd.to_string(),
        "--exec".to_string(),
        command.executable,
    ];
    args.extend(command.args);
    CommandSpec::with_args(WSL_EXE, args)
}

impl<B> SessionBackend for WslDirectBackend<B>
where
    B: SessionBackend,
{
    fn kind(&self) -> BackendKind {
        BackendKind::WslDirect
    }

    fn spawn(&mut self, mut request: SpawnRequest) -> BackendResult<SessionHandle> {
        if request
            .backend
            .is_some_and(|backend| backend != BackendKind::WslDirect)
        {
            return Err(BackendError::unsupported(
                "WSL direct backend cannot spawn the requested backend kind.",
            ));
        }

        let distribution = request
            .backend_profile
            .clone()
            .or_else(|| self.config.distribution.clone());
        let wsl_cwd = resolve_wsl_cwd_with_distribution(
            request.cwd.as_deref(),
            &self.config.default_cwd,
            distribution.as_deref(),
            self.config.prefer_wslpath,
        )
        .map_err(backend_error_from_wsl_diagnostic)?;
        if self.config.validate_distribution {
            validate_wsl_distribution(distribution.as_deref())
                .map_err(backend_error_from_wsl_diagnostic)?;
        }
        if self.config.validate_launch {
            ensure_wsl_launch_not_timed_out(
                distribution.as_deref(),
                &wsl_cwd,
                self.config.launch_timeout,
            )
            .map_err(backend_error_from_wsl_diagnostic)?;
        }
        request.command = direct_launch_command_with_distribution(
            distribution.as_deref(),
            &wsl_cwd,
            request.command,
        );
        request.cwd = None;
        request.backend_profile = None;
        request.backend = Some(BackendKind::Conpty);
        let mut handle = self
            .inner
            .spawn(request)
            .map_err(backend_error_from_wsl_spawn)?;
        handle.backend_kind = BackendKind::WslDirect;
        Ok(handle)
    }

    fn attach(&mut self, _request: AttachRequest) -> BackendResult<SessionHandle> {
        Err(BackendError::unsupported(
            "WSL direct sessions are not attachable.",
        ))
    }

    fn send_input(&mut self, session_id: &str, input: InputEvent) -> BackendResult<()> {
        self.inner.send_input(session_id, input)
    }

    fn resize(&mut self, session_id: &str, size: TerminalSize) -> BackendResult<()> {
        self.inner.resize(session_id, size)
    }

    fn terminate(&mut self, session_id: &str, mode: TerminationMode) -> BackendResult<()> {
        self.inner.terminate(session_id, mode)
    }

    fn set_output_paused(&mut self, session_id: &str, paused: bool) -> BackendResult<()> {
        self.inner.set_output_paused(session_id, paused)
    }

    fn drain_events(&mut self) -> Vec<BackendEvent> {
        self.inner.drain_events()
    }
}

fn backend_error_from_wsl_diagnostic(diagnostic: WslDiagnostic) -> BackendError {
    BackendError::new(diagnostic.code.as_str(), diagnostic.message)
}

fn backend_error_from_wsl_spawn(error: BackendError) -> BackendError {
    if error.code == "timeout" {
        BackendError::new(
            WslDiagnosticCode::LaunchTimeout.as_str(),
            format!("WSL launch timed out: {}", error.message),
        )
    } else {
        error
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentmux_backend::{SessionHandle, SpawnRequest};

    #[test]
    fn discovery_command_uses_wsl_list_quiet() {
        let command = distribution_discovery_command();
        assert_eq!(command.executable, WSL_EXE);
        assert_eq!(
            command.args,
            vec!["--list".to_string(), "--quiet".to_string()]
        );
    }

    #[test]
    fn status_command_uses_wsl_status() {
        let command = distribution_status_command();
        assert_eq!(command.executable, WSL_EXE);
        assert_eq!(command.args, vec!["--status".to_string()]);
    }

    #[test]
    fn verbose_command_uses_wsl_list_verbose() {
        let command = distribution_verbose_command();
        assert_eq!(command.executable, WSL_EXE);
        assert_eq!(
            command.args,
            vec!["--list".to_string(), "--verbose".to_string()]
        );
    }

    #[test]
    fn distribution_parser_handles_quiet_and_default_markers() {
        assert_eq!(
            parse_distribution_list("\u{feff}Ubuntu\0\r\n* Debian\r\n"),
            vec![
                WslDistribution {
                    name: "Ubuntu".to_string(),
                    is_default: false
                },
                WslDistribution {
                    name: "Debian".to_string(),
                    is_default: true
                }
            ]
        );
    }

    #[test]
    fn status_parser_extracts_default_distribution_from_nul_output() {
        assert_eq!(
            parse_default_distribution_status(
                "\u{feff}Default Distribution:\0 Ubuntu\0\r\nDefault Version:\0 2\0\r\n"
            ),
            Some("Ubuntu".to_string())
        );
    }

    #[test]
    fn verbose_parser_extracts_starred_default_distribution() {
        assert_eq!(
            parse_default_distribution_verbose(
                "\u{feff}  NAME      STATE           VERSION\r\n* Ubuntu    Stopped         2\r\n  Debian    Running         2\r\n"
            ),
            Some("Ubuntu".to_string())
        );
    }

    #[test]
    fn apply_default_distribution_marks_matching_distribution() {
        let mut distributions = vec![
            WslDistribution {
                name: "Ubuntu".to_string(),
                is_default: false,
            },
            WslDistribution {
                name: "Debian".to_string(),
                is_default: true,
            },
        ];

        assert!(apply_default_distribution(&mut distributions, "ubuntu"));
        assert_eq!(
            distributions,
            vec![
                WslDistribution {
                    name: "Ubuntu".to_string(),
                    is_default: true,
                },
                WslDistribution {
                    name: "Debian".to_string(),
                    is_default: false,
                },
            ]
        );
    }

    #[test]
    fn empty_distribution_parser_returns_typed_diagnostic() {
        let diagnostic = distributions_or_diagnostic("\r\n").unwrap_err();
        assert_eq!(diagnostic.code, WslDiagnosticCode::NoDistributions);
        assert_eq!(diagnostic.code.as_str(), "no_wsl_distributions");
    }

    #[test]
    fn selected_distribution_validation_reports_missing_distribution() {
        let distributions = vec![WslDistribution {
            name: "Ubuntu".to_string(),
            is_default: true,
        }];

        let diagnostic =
            validate_selected_distribution(&distributions, Some("MissingDistro")).unwrap_err();
        assert_eq!(diagnostic.code, WslDiagnosticCode::MissingDistribution);
        assert_eq!(diagnostic.code.as_str(), "wsl_distribution_not_found");
    }

    #[test]
    fn direct_launch_command_uses_argument_array_shape() {
        let command = direct_launch_command(
            "Ubuntu",
            "/mnt/d/work/repo with spaces",
            CommandSpec::with_args("bash", vec!["-l".to_string()]),
        );

        assert_eq!(command.executable, WSL_EXE);
        assert_eq!(
            command.args,
            vec![
                "--distribution",
                "Ubuntu",
                "--cd",
                "/mnt/d/work/repo with spaces",
                "--exec",
                "bash",
                "-l"
            ]
        );
    }

    #[test]
    fn wslpath_command_uses_distribution_when_present() {
        let command = wslpath_command(Some("Ubuntu"), r"D:\work\repo with spaces");

        assert_eq!(command.executable, WSL_EXE);
        assert_eq!(
            command.args,
            vec![
                "--distribution",
                "Ubuntu",
                "--exec",
                "wslpath",
                "-a",
                r"D:\work\repo with spaces"
            ]
        );
    }

    #[test]
    fn launch_probe_command_uses_distribution_cwd_and_shell_probe() {
        let command = launch_probe_command(Some("Ubuntu"), "/mnt/d/work/repo");

        assert_eq!(command.executable, WSL_EXE);
        assert_eq!(
            command.args,
            vec![
                "--distribution",
                "Ubuntu",
                "--cd",
                "/mnt/d/work/repo",
                "--exec",
                "sh",
                "-lc",
                "exit 0"
            ]
        );
    }

    #[test]
    fn launch_timeout_diagnostic_has_stable_code() {
        let diagnostic = WslDiagnostic::launch_timeout(
            Some("Ubuntu"),
            "/mnt/d/work/repo",
            Duration::from_millis(250),
        );

        assert_eq!(diagnostic.code, WslDiagnosticCode::LaunchTimeout);
        assert_eq!(diagnostic.code.as_str(), "wsl_launch_timeout");
        assert!(diagnostic.message.contains("250 ms"));
        assert!(diagnostic.message.contains("Ubuntu"));
    }

    #[test]
    fn fallback_path_conversion_handles_drive_paths() {
        assert_eq!(
            fallback_windows_path_to_wsl(r"D:\work\repo"),
            Some("/mnt/d/work/repo".to_string())
        );
    }

    #[test]
    fn fallback_path_conversion_ignores_non_windows_paths() {
        assert_eq!(fallback_windows_path_to_wsl("/home/dev/repo"), None);
    }

    #[test]
    fn cwd_resolution_accepts_wsl_and_windows_paths() {
        assert_eq!(
            resolve_wsl_cwd(Some("/home/dev/repo"), DEFAULT_WSL_CWD).unwrap(),
            "/home/dev/repo"
        );
        assert_eq!(
            resolve_wsl_cwd(Some(r"D:\work\repo"), DEFAULT_WSL_CWD).unwrap(),
            "/mnt/d/work/repo"
        );
        assert_eq!(resolve_wsl_cwd(None, DEFAULT_WSL_CWD).unwrap(), "~");
    }

    #[test]
    fn cwd_resolution_can_skip_wslpath_for_deterministic_fallback() {
        assert_eq!(
            resolve_wsl_cwd_with_distribution(
                Some(r"D:\work\repo"),
                DEFAULT_WSL_CWD,
                Some("Ubuntu"),
                false
            )
            .unwrap(),
            "/mnt/d/work/repo"
        );
    }

    #[test]
    fn interactive_terminal_config_skips_preflight_wsl_processes() {
        let config = WslDirectConfig::for_interactive_terminal();
        assert_eq!(config.distribution, None);
        assert!(!config.validate_distribution);
        assert!(!config.validate_launch);
        assert!(!config.prefer_wslpath);
        assert_eq!(config.default_cwd, DEFAULT_WSL_CWD);

        let config = WslDirectConfig::for_interactive_distribution("Ubuntu");
        assert_eq!(config.distribution.as_deref(), Some("Ubuntu"));
        assert!(!config.validate_distribution);
        assert!(!config.validate_launch);
        assert!(!config.prefer_wslpath);
    }

    #[test]
    fn cwd_resolution_rejects_relative_paths() {
        let diagnostic = resolve_wsl_cwd(Some("relative/repo"), DEFAULT_WSL_CWD).unwrap_err();
        assert_eq!(diagnostic.code, WslDiagnosticCode::InvalidCwd);
    }

    #[test]
    fn spawn_translates_request_and_delegates_lifecycle() {
        let fake = RecordingBackend::default();
        let mut config = WslDirectConfig::for_distribution("Ubuntu");
        config.validate_distribution = false;
        config.validate_launch = false;
        config.prefer_wslpath = false;
        let mut backend = WslDirectBackend::with_backend(config, fake);
        let handle = backend
            .spawn(SpawnRequest {
                session_id: "ses_wsl".to_string(),
                workspace_id: None,
                backend: Some(BackendKind::WslDirect),
                backend_profile: Some("Debian".to_string()),
                command: CommandSpec::with_args("bash", vec!["-l".to_string()]),
                cwd: Some(r"D:\work\repo".to_string()),
                env: Vec::new(),
                initial_size: TerminalSize::new(120, 30),
            })
            .unwrap();

        assert_eq!(backend.kind(), BackendKind::WslDirect);
        assert_eq!(handle.backend_native_id.as_deref(), Some("transport-1"));
        assert_eq!(
            backend.inner().last_spawn.as_ref().unwrap().command.args,
            vec![
                "--distribution",
                "Debian",
                "--cd",
                "/mnt/d/work/repo",
                "--exec",
                "bash",
                "-l"
            ]
        );
        assert_eq!(
            backend.inner().last_spawn.as_ref().unwrap().backend,
            Some(BackendKind::Conpty)
        );
        assert_eq!(
            backend.inner().last_spawn.as_ref().unwrap().backend_profile,
            None
        );

        backend
            .send_input("ses_wsl", InputEvent::Text("pwd\n".to_string()))
            .unwrap();
        backend
            .resize("ses_wsl", TerminalSize::new(100, 24))
            .unwrap();
        backend.terminate("ses_wsl", TerminationMode::Soft).unwrap();
        let inner = backend.inner();
        assert_eq!(inner.sent_inputs, 1);
        assert_eq!(inner.last_resize, Some(TerminalSize::new(100, 24)));
        assert_eq!(inner.last_termination, Some(TerminationMode::Soft));
    }

    #[test]
    fn spawn_preserves_invalid_wsl_cwd_backend_code() {
        let fake = RecordingBackend::default();
        let mut config = WslDirectConfig::for_distribution("Ubuntu");
        config.validate_distribution = false;
        config.validate_launch = false;
        let mut backend = WslDirectBackend::with_backend(config, fake);

        let error = backend
            .spawn(SpawnRequest {
                session_id: "ses_wsl_invalid_cwd".to_string(),
                workspace_id: None,
                backend: Some(BackendKind::WslDirect),
                backend_profile: None,
                command: CommandSpec::new("bash"),
                cwd: Some("relative/path".to_string()),
                env: Vec::new(),
                initial_size: TerminalSize::new(120, 30),
            })
            .unwrap_err();

        assert_eq!(error.code, "invalid_wsl_cwd");
    }

    #[test]
    fn spawn_promotes_inner_timeout_to_wsl_launch_timeout() {
        let fake = RecordingBackend {
            spawn_error: Some(BackendError::new("timeout", "inner spawn did not finish")),
            ..RecordingBackend::default()
        };
        let mut config = WslDirectConfig::for_distribution("Ubuntu");
        config.validate_distribution = false;
        config.validate_launch = false;
        config.prefer_wslpath = false;
        let mut backend = WslDirectBackend::with_backend(config, fake);

        let error = backend
            .spawn(SpawnRequest {
                session_id: "ses_wsl_timeout".to_string(),
                workspace_id: None,
                backend: Some(BackendKind::WslDirect),
                backend_profile: None,
                command: CommandSpec::new("bash"),
                cwd: Some(r"D:\work\repo".to_string()),
                env: Vec::new(),
                initial_size: TerminalSize::new(120, 30),
            })
            .unwrap_err();

        assert_eq!(error.code, "wsl_launch_timeout");
        assert!(error.message.contains("inner spawn did not finish"));
    }

    #[derive(Default)]
    struct RecordingBackend {
        last_spawn: Option<SpawnRequest>,
        spawn_error: Option<BackendError>,
        events: Vec<BackendEvent>,
        sent_inputs: usize,
        last_resize: Option<TerminalSize>,
        last_termination: Option<TerminationMode>,
    }

    impl SessionBackend for RecordingBackend {
        fn kind(&self) -> BackendKind {
            BackendKind::Conpty
        }

        fn spawn(&mut self, request: SpawnRequest) -> BackendResult<SessionHandle> {
            if let Some(error) = self.spawn_error.take() {
                return Err(error);
            }

            self.events.push(BackendEvent::Started {
                session_id: request.session_id.clone(),
            });
            self.last_spawn = Some(request.clone());
            Ok(SessionHandle {
                session_id: request.session_id,
                backend_kind: BackendKind::Conpty,
                backend_native_id: Some("transport-1".to_string()),
                transport_pid: Some(1),
            })
        }

        fn attach(&mut self, _request: AttachRequest) -> BackendResult<SessionHandle> {
            Err(BackendError::unsupported(
                "recording backend does not attach",
            ))
        }

        fn send_input(&mut self, _session_id: &str, _input: InputEvent) -> BackendResult<()> {
            self.sent_inputs += 1;
            Ok(())
        }

        fn resize(&mut self, _session_id: &str, size: TerminalSize) -> BackendResult<()> {
            self.last_resize = Some(size);
            Ok(())
        }

        fn terminate(&mut self, _session_id: &str, mode: TerminationMode) -> BackendResult<()> {
            self.last_termination = Some(mode);
            Ok(())
        }

        fn drain_events(&mut self) -> Vec<BackendEvent> {
            std::mem::take(&mut self.events)
        }
    }
}
