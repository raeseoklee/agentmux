//! Safe client-side configuration for the AgentMux MCP stdio server.
//!
//! Planning is the default. Files change only when `McpConfigRequest::install`
//! is explicitly true, and existing files are backed up before atomic replace.

use std::{
    env,
    error::Error,
    ffi::OsStr,
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::{json, Map, Value};

const SERVER_NAME: &str = "agentmux";
const CODEX_BLOCK_BEGIN: &str = "# >>> AgentMux MCP (managed by agentmux) >>>";
const CODEX_BLOCK_END: &str = "# <<< AgentMux MCP (managed by agentmux) <<<";
const CONFIG_INSTALL_RETRY_LIMIT: usize = 4;
static UNIQUE_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum McpClient {
    Codex,
    Claude,
}

impl FromStr for McpClient {
    type Err = McpConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "codex" => Ok(Self::Codex),
            "claude" | "claude-code" => Ok(Self::Claude),
            _ => Err(McpConfigError::InvalidArgument(format!(
                "unknown MCP client '{value}'; expected 'codex' or 'claude'"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum McpProfile {
    Read,
    Standard,
    Full,
}

impl McpProfile {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Standard => "standard",
            Self::Full => "full",
        }
    }
}

impl FromStr for McpProfile {
    type Err = McpConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "read" => Ok(Self::Read),
            "standard" => Ok(Self::Standard),
            "full" => Ok(Self::Full),
            _ => Err(McpConfigError::InvalidArgument(format!(
                "unknown MCP profile '{value}'; expected 'read', 'standard', or 'full'"
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct McpConfigRequest {
    pub(crate) client: McpClient,
    pub(crate) profile: McpProfile,
    pub(crate) executable_path: PathBuf,
    pub(crate) config_path: PathBuf,
    /// Configuration is preview-only unless the caller explicitly sets this.
    pub(crate) install: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum McpConfigStatus {
    Preview,
    Installed,
    Unchanged,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct McpConfigOutcome {
    pub(crate) client: McpClient,
    pub(crate) profile: McpProfile,
    pub(crate) config_path: PathBuf,
    pub(crate) executable_path: PathBuf,
    pub(crate) status: McpConfigStatus,
    pub(crate) changed: bool,
    pub(crate) backup_path: Option<PathBuf>,
    /// The reviewed AgentMux-owned fragment, never the user's full configuration.
    pub(crate) preview: String,
}

#[derive(Debug)]
pub(crate) enum McpConfigError {
    InvalidArgument(String),
    InvalidConfig {
        path: PathBuf,
        message: String,
    },
    Conflict {
        path: PathBuf,
        message: String,
    },
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for McpConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgument(message) => formatter.write_str(message),
            Self::InvalidConfig { path, message } => {
                write!(
                    formatter,
                    "invalid MCP config '{}': {message}",
                    path.display()
                )
            }
            Self::Conflict { path, message } => {
                write!(
                    formatter,
                    "MCP config conflict in '{}': {message}",
                    path.display()
                )
            }
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} '{}': {source}",
                path.display()
            ),
        }
    }
}

impl Error for McpConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub(crate) fn default_config_path(client: McpClient) -> Result<PathBuf, McpConfigError> {
    if client == McpClient::Codex {
        if let Some(codex_home) = nonempty_env("CODEX_HOME") {
            return Ok(PathBuf::from(codex_home).join("config.toml"));
        }
    }

    let user_profile = nonempty_env("USERPROFILE").ok_or_else(|| {
        McpConfigError::InvalidArgument(
            "USERPROFILE is not set; pass an explicit MCP config path".to_string(),
        )
    })?;
    Ok(default_config_path_for_home(
        client,
        Path::new(&user_profile),
    ))
}

/// Generates a reviewed preview and writes only when `request.install` is true.
pub(crate) fn configure(request: &McpConfigRequest) -> Result<McpConfigOutcome, McpConfigError> {
    configure_with_before_install(request, || {})
}

fn configure_with_before_install<F>(
    request: &McpConfigRequest,
    before_install: F,
) -> Result<McpConfigOutcome, McpConfigError>
where
    F: FnOnce(),
{
    validate_executable_path(&request.executable_path, request.install)?;

    let original = read_optional_text(&request.config_path)?;
    let rendered = render_config(request, original.as_deref())?;
    let changed = original.as_deref() != Some(rendered.contents.as_str());

    if !request.install {
        return Ok(outcome(
            request,
            McpConfigStatus::Preview,
            changed,
            None,
            rendered.preview,
        ));
    }

    before_install();
    for _ in 0..CONFIG_INSTALL_RETRY_LIMIT {
        // Re-read and re-merge while installing. Codex or Claude may have
        // updated unrelated settings after the preview snapshot was created.
        let latest = read_optional_text(&request.config_path)?;
        let rendered = render_config(request, latest.as_deref())?;
        if latest.as_deref() == Some(rendered.contents.as_str()) {
            return Ok(outcome(
                request,
                McpConfigStatus::Unchanged,
                false,
                None,
                rendered.preview,
            ));
        }

        match install_contents(
            &request.config_path,
            latest.as_deref().map(str::as_bytes),
            rendered.contents.as_bytes(),
        )? {
            InstallAttempt::Installed(backup_path) => {
                return Ok(outcome(
                    request,
                    McpConfigStatus::Installed,
                    true,
                    backup_path,
                    rendered.preview,
                ));
            }
            InstallAttempt::Stale => continue,
        }
    }

    Err(McpConfigError::Conflict {
        path: request.config_path.clone(),
        message: format!(
            "the file changed during {} consecutive install attempts; no AgentMux update was written",
            CONFIG_INSTALL_RETRY_LIMIT
        ),
    })
}

fn render_config(
    request: &McpConfigRequest,
    original: Option<&str>,
) -> Result<RenderedConfig, McpConfigError> {
    match request.client {
        McpClient::Codex => merge_codex_config(
            original.unwrap_or_default(),
            &request.executable_path,
            request.profile,
            &request.config_path,
        ),
        McpClient::Claude => merge_claude_config(
            original,
            &request.executable_path,
            request.profile,
            &request.config_path,
        ),
    }
}

fn outcome(
    request: &McpConfigRequest,
    status: McpConfigStatus,
    changed: bool,
    backup_path: Option<PathBuf>,
    preview: String,
) -> McpConfigOutcome {
    McpConfigOutcome {
        client: request.client,
        profile: request.profile,
        config_path: request.config_path.clone(),
        executable_path: request.executable_path.clone(),
        status,
        changed,
        backup_path,
        preview,
    }
}

fn nonempty_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn default_config_path_for_home(client: McpClient, home: &Path) -> PathBuf {
    match client {
        McpClient::Codex => home.join(".codex").join("config.toml"),
        McpClient::Claude => home.join(".claude.json"),
    }
}

fn validate_executable_path(path: &Path, require_existing: bool) -> Result<(), McpConfigError> {
    let value = path.to_str().ok_or_else(|| {
        McpConfigError::InvalidArgument(
            "AgentMux executable path must be valid Unicode".to_string(),
        )
    })?;
    if !is_absolute_windows_path(value) {
        return Err(McpConfigError::InvalidArgument(format!(
            "AgentMux executable path must be an absolute Windows path: {}",
            path.display()
        )));
    }
    if !path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Err(McpConfigError::InvalidArgument(format!(
            "AgentMux executable path must end in .exe: {}",
            path.display()
        )));
    }
    if require_existing && !path.is_file() {
        return Err(McpConfigError::InvalidArgument(format!(
            "AgentMux executable does not exist or is not a file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn is_absolute_windows_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/'))
        || value.starts_with("\\\\")
}

fn read_optional_text(path: &Path) -> Result<Option<String>, McpConfigError> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_error("read", path, source)),
    }
}

#[derive(Debug)]
struct RenderedConfig {
    contents: String,
    preview: String,
}

fn merge_codex_config(
    original: &str,
    executable_path: &Path,
    profile: McpProfile,
    path: &Path,
) -> Result<RenderedConfig, McpConfigError> {
    let eol = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let block = codex_block(executable_path, profile, eol)?;
    let begin = match_indices(original, CODEX_BLOCK_BEGIN);
    let end = match_indices(original, CODEX_BLOCK_END);

    let contents = match (begin.as_slice(), end.as_slice()) {
        ([], []) => {
            if contains_unmanaged_codex_entry(original) {
                return Err(McpConfigError::Conflict {
                    path: path.to_path_buf(),
                    message: "an unmanaged [mcp_servers.agentmux] entry already exists; review or remove it before using --install".to_string(),
                });
            }
            append_block(original, &block, eol)
        }
        ([begin_index], [end_index]) if begin_index < end_index => {
            let end_index = end_index + CODEX_BLOCK_END.len();
            format!(
                "{}{}{}",
                &original[..*begin_index],
                block,
                &original[end_index..]
            )
        }
        _ => {
            return Err(McpConfigError::InvalidConfig {
                path: path.to_path_buf(),
                message: "AgentMux managed block markers are missing, duplicated, or out of order"
                    .to_string(),
            })
        }
    };

    Ok(RenderedConfig {
        contents,
        preview: block,
    })
}

fn codex_block(
    executable_path: &Path,
    profile: McpProfile,
    eol: &str,
) -> Result<String, McpConfigError> {
    let command = json_string(executable_path)?;
    let args = serde_json::to_string(&["mcp", "serve", "--profile", profile.as_str()])
        .map_err(|error| McpConfigError::InvalidArgument(error.to_string()))?;
    Ok([
        CODEX_BLOCK_BEGIN.to_string(),
        format!("[mcp_servers.{SERVER_NAME}]"),
        "enabled = true".to_string(),
        format!("command = {command}"),
        format!("args = {args}"),
        CODEX_BLOCK_END.to_string(),
    ]
    .join(eol))
}

fn contains_unmanaged_codex_entry(original: &str) -> bool {
    original.lines().any(|line| {
        let line = line.trim();
        line == "[mcp_servers.agentmux]"
            || line == "[mcp_servers.\"agentmux\"]"
            || line == "[mcp_servers.'agentmux']"
            || line.starts_with("[mcp_servers.agentmux.")
            || line.starts_with("mcp_servers.agentmux")
    })
}

fn append_block(original: &str, block: &str, eol: &str) -> String {
    if original.is_empty() {
        return format!("{block}{eol}");
    }

    let separator = if original.ends_with("\n") {
        eol.to_string()
    } else {
        format!("{eol}{eol}")
    };
    format!("{original}{separator}{block}{eol}")
}

fn match_indices(haystack: &str, needle: &str) -> Vec<usize> {
    haystack
        .match_indices(needle)
        .map(|(index, _)| index)
        .collect()
}

fn merge_claude_config(
    original: Option<&str>,
    executable_path: &Path,
    profile: McpProfile,
    path: &Path,
) -> Result<RenderedConfig, McpConfigError> {
    let mut root = match original {
        Some(contents) if !contents.trim().is_empty() => {
            let contents = contents.strip_prefix('\u{feff}').unwrap_or(contents);
            serde_json::from_str::<Value>(contents).map_err(|error| {
                McpConfigError::InvalidConfig {
                    path: path.to_path_buf(),
                    message: format!("JSON parse failed: {error}"),
                }
            })?
        }
        _ => Value::Object(Map::new()),
    };
    let root_object = root
        .as_object_mut()
        .ok_or_else(|| McpConfigError::InvalidConfig {
            path: path.to_path_buf(),
            message: "the root value must be a JSON object".to_string(),
        })?;
    let servers = root_object
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| McpConfigError::InvalidConfig {
            path: path.to_path_buf(),
            message: "mcpServers must be a JSON object".to_string(),
        })?;

    if let Some(existing) = servers.get(SERVER_NAME) {
        if !is_agentmux_server(existing) {
            return Err(McpConfigError::Conflict {
                path: path.to_path_buf(),
                message: "an unrelated MCP server named 'agentmux' already exists; review or rename it before using --install".to_string(),
            });
        }
    }

    let server = claude_server(executable_path, profile)?;
    servers.insert(SERVER_NAME.to_string(), server.clone());
    let mut contents =
        serde_json::to_string_pretty(&root).map_err(|error| McpConfigError::InvalidConfig {
            path: path.to_path_buf(),
            message: format!("JSON serialization failed: {error}"),
        })?;
    contents.push('\n');

    let mut preview_root = Map::new();
    let mut preview_servers = Map::new();
    preview_servers.insert(SERVER_NAME.to_string(), server);
    preview_root.insert("mcpServers".to_string(), Value::Object(preview_servers));
    let mut preview =
        serde_json::to_string_pretty(&Value::Object(preview_root)).map_err(|error| {
            McpConfigError::InvalidConfig {
                path: path.to_path_buf(),
                message: format!("JSON preview serialization failed: {error}"),
            }
        })?;
    preview.push('\n');

    Ok(RenderedConfig { contents, preview })
}

fn claude_server(executable_path: &Path, profile: McpProfile) -> Result<Value, McpConfigError> {
    Ok(json!({
        "type": "stdio",
        "command": path_text(executable_path)?,
        "args": ["mcp", "serve", "--profile", profile.as_str()],
        "env": {}
    }))
}

fn is_agentmux_server(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let command_matches = object
        .get("command")
        .and_then(Value::as_str)
        .and_then(|command| Path::new(command).file_name())
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.eq_ignore_ascii_case("agentmux.exe"));
    let args_match = object
        .get("args")
        .and_then(Value::as_array)
        .is_some_and(|args| {
            args.first().and_then(Value::as_str) == Some("mcp")
                && args.get(1).and_then(Value::as_str) == Some("serve")
        });
    command_matches && args_match
}

fn json_string(path: &Path) -> Result<String, McpConfigError> {
    serde_json::to_string(path_text(path)?)
        .map_err(|error| McpConfigError::InvalidArgument(error.to_string()))
}

fn path_text(path: &Path) -> Result<&str, McpConfigError> {
    path.to_str().ok_or_else(|| {
        McpConfigError::InvalidArgument(
            "AgentMux executable path must be valid Unicode".to_string(),
        )
    })
}

#[derive(Debug, Eq, PartialEq)]
enum InstallAttempt {
    Installed(Option<PathBuf>),
    Stale,
}

fn install_contents(
    target: &Path,
    expected: Option<&[u8]>,
    contents: &[u8],
) -> Result<InstallAttempt, McpConfigError> {
    install_contents_with(target, expected, contents, atomic_replace)
}

fn install_contents_with<F>(
    target: &Path,
    expected: Option<&[u8]>,
    contents: &[u8],
    replace: F,
) -> Result<InstallAttempt, McpConfigError>
where
    F: FnOnce(&Path, &Path, bool) -> io::Result<()>,
{
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| io_error("create directory", parent, source))?;

    let target_exists = match fs::metadata(target) {
        Ok(metadata) if metadata.is_file() => true,
        Ok(_) => {
            return Err(McpConfigError::InvalidArgument(format!(
                "MCP config path is not a file: {}",
                target.display()
            )))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(source) => return Err(io_error("inspect", target, source)),
    };

    if read_optional_bytes(target)?.as_deref() != expected {
        return Ok(InstallAttempt::Stale);
    }

    let temp_path = unique_sibling(target, "agentmux.tmp");
    write_new_file(&temp_path, contents)?;
    if read_optional_bytes(target)?.as_deref() != expected {
        let _ = fs::remove_file(&temp_path);
        return Ok(InstallAttempt::Stale);
    }

    let backup_path = if target_exists {
        let backup_path = unique_sibling(target, "agentmux.bak");
        fs::copy(target, &backup_path)
            .map_err(|source| io_error("backup", &backup_path, source))?;
        sync_file(&backup_path)?;
        Some(backup_path)
    } else {
        None
    };

    if read_optional_bytes(target)?.as_deref() != expected {
        let _ = fs::remove_file(&temp_path);
        if let Some(backup_path) = &backup_path {
            let _ = fs::remove_file(backup_path);
        }
        return Ok(InstallAttempt::Stale);
    }
    if let Err(source) = replace(&temp_path, target, target_exists) {
        let _ = fs::remove_file(&temp_path);
        return Err(io_error("atomically replace", target, source));
    }

    Ok(InstallAttempt::Installed(backup_path))
}

fn read_optional_bytes(path: &Path) -> Result<Option<Vec<u8>>, McpConfigError> {
    match fs::read(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_error("read", path, source)),
    }
}

fn write_new_file(path: &Path, contents: &[u8]) -> Result<(), McpConfigError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| io_error("create temporary file", path, source))?;
    if let Err(source) = file.write_all(contents).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(io_error("write temporary file", path, source));
    }
    Ok(())
}

fn sync_file(path: &Path) -> Result<(), McpConfigError> {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|source| io_error("sync", path, source))
}

fn unique_sibling(target: &Path, suffix: &str) -> PathBuf {
    let file_name = target
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("agentmux-mcp-config");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = UNIQUE_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    target.with_file_name(format!(
        "{file_name}.{suffix}.{}.{}.{}",
        std::process::id(),
        timestamp,
        counter
    ))
}

#[cfg(windows)]
fn atomic_replace(temp: &Path, target: &Path, target_exists: bool) -> io::Result<()> {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt, ptr};

    type Bool = i32;
    type Dword = u32;
    const REPLACEFILE_WRITE_THROUGH: Dword = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: Dword = 0x0000_0008;

    #[link(name = "Kernel32")]
    extern "system" {
        fn ReplaceFileW(
            replaced_file_name: *const u16,
            replacement_file_name: *const u16,
            backup_file_name: *const u16,
            replace_flags: Dword,
            exclude: *mut c_void,
            reserved: *mut c_void,
        ) -> Bool;
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: Dword,
        ) -> Bool;
    }

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    let temp = wide(temp);
    let target = wide(target);
    // SAFETY: both paths are NUL-terminated UTF-16 buffers that remain alive
    // for the duration of the Win32 call, and all optional pointers are null.
    let success = unsafe {
        if target_exists {
            ReplaceFileW(
                target.as_ptr(),
                temp.as_ptr(),
                ptr::null(),
                REPLACEFILE_WRITE_THROUGH,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        } else {
            MoveFileExW(temp.as_ptr(), target.as_ptr(), MOVEFILE_WRITE_THROUGH)
        }
    };
    if success == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_replace(temp: &Path, target: &Path, target_exists: bool) -> io::Result<()> {
    if !target_exists {
        return fs::rename(temp, target);
    }

    let rollback = unique_sibling(target, "agentmux.rollback");
    fs::rename(target, &rollback)?;
    match fs::rename(temp, target) {
        Ok(()) => {
            fs::remove_file(rollback)?;
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(rollback, target);
            Err(error)
        }
    }
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> McpConfigError {
    McpConfigError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(label: &str) -> Self {
            let path = env::temp_dir().join(format!(
                "agentmux-mcp-config-{label}-{}-{}",
                std::process::id(),
                UNIQUE_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("create test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn request(
        client: McpClient,
        profile: McpProfile,
        executable_path: PathBuf,
        config_path: PathBuf,
        install: bool,
    ) -> McpConfigRequest {
        McpConfigRequest {
            client,
            profile,
            executable_path,
            config_path,
            install,
        }
    }

    #[test]
    fn parses_every_reviewed_profile() {
        assert_eq!("read".parse::<McpProfile>().unwrap(), McpProfile::Read);
        assert_eq!(
            "standard".parse::<McpProfile>().unwrap(),
            McpProfile::Standard
        );
        assert_eq!("full".parse::<McpProfile>().unwrap(), McpProfile::Full);
        assert!("admin".parse::<McpProfile>().is_err());
    }

    #[test]
    fn default_paths_match_codex_and_claude_user_scopes() {
        let home = Path::new(r"C:\Users\tester");
        assert_eq!(
            default_config_path_for_home(McpClient::Codex, home),
            home.join(".codex").join("config.toml")
        );
        assert_eq!(
            default_config_path_for_home(McpClient::Claude, home),
            home.join(".claude.json")
        );
    }

    #[test]
    fn executable_path_must_be_an_absolute_windows_exe() {
        let test_dir = TestDir::new("invalid-executable");
        let config_path = test_dir.path().join("config.toml");

        let relative = configure(&request(
            McpClient::Codex,
            McpProfile::Read,
            PathBuf::from("agentmux.exe"),
            config_path.clone(),
            false,
        ))
        .expect_err("relative executable must fail");
        assert!(relative.to_string().contains("absolute Windows path"));

        let wrong_extension = configure(&request(
            McpClient::Codex,
            McpProfile::Read,
            PathBuf::from(r"C:\AgentMux\agentmux.cmd"),
            config_path,
            false,
        ))
        .expect_err("non-exe executable must fail");
        assert!(wrong_extension.to_string().contains("end in .exe"));
    }

    #[test]
    fn codex_preview_escapes_windows_executable_and_does_not_write() {
        let test_dir = TestDir::new("codex-preview");
        let config_path = test_dir.path().join("config.toml");
        let result = configure(&request(
            McpClient::Codex,
            McpProfile::Standard,
            PathBuf::from(r"C:\Program Files\AgentMux\agentmux.exe"),
            config_path.clone(),
            false,
        ))
        .expect("generate preview");

        assert_eq!(result.status, McpConfigStatus::Preview);
        assert!(result.changed);
        assert!(result
            .preview
            .contains(r#"command = "C:\\Program Files\\AgentMux\\agentmux.exe""#));
        assert!(result
            .preview
            .contains(r#"args = ["mcp","serve","--profile","standard"]"#));
        assert!(!config_path.exists());
    }

    #[test]
    fn codex_managed_block_is_replaced_without_touching_other_settings() {
        let original = format!(
            "model = \"test\"\n\n{}\n[mcp_servers.agentmux]\ncommand = \"C:\\\\old\\\\agentmux.exe\"\nargs = [\"mcp\", \"serve\", \"--profile\", \"read\"]\n{}\n\n[features]\nexample = true\n",
            CODEX_BLOCK_BEGIN, CODEX_BLOCK_END
        );
        let rendered = merge_codex_config(
            &original,
            Path::new(r"D:\Apps\AgentMux\agentmux.exe"),
            McpProfile::Full,
            Path::new(r"C:\Users\tester\.codex\config.toml"),
        )
        .expect("replace managed block");

        assert!(rendered.contents.starts_with("model = \"test\""));
        assert!(rendered.contents.contains("[features]\nexample = true"));
        assert_eq!(rendered.contents.matches(CODEX_BLOCK_BEGIN).count(), 1);
        assert!(rendered.contents.contains("\"full\""));
    }

    #[test]
    fn codex_unmanaged_agentmux_entry_is_a_review_conflict() {
        let error = merge_codex_config(
            "[mcp_servers.agentmux]\ncommand = \"other.exe\"\n",
            Path::new(r"C:\AgentMux\agentmux.exe"),
            McpProfile::Read,
            Path::new(r"C:\Users\tester\.codex\config.toml"),
        )
        .expect_err("conflict expected");
        assert!(error.to_string().contains("unmanaged"));
    }

    #[test]
    fn claude_merge_preserves_other_servers() {
        let rendered = merge_claude_config(
            Some(r#"{"mcpServers":{"docs":{"type":"http","url":"https://example.test/mcp"}},"theme":"dark"}"#),
            Path::new(r"C:\AgentMux\agentmux.exe"),
            McpProfile::Standard,
            Path::new(r"C:\Users\tester\.claude.json"),
        )
        .expect("merge Claude config");
        let value: Value = serde_json::from_str(&rendered.contents).expect("valid JSON");

        assert_eq!(value["theme"], "dark");
        assert_eq!(
            value["mcpServers"]["docs"]["url"],
            "https://example.test/mcp"
        );
        assert_eq!(
            value["mcpServers"]["agentmux"]["command"],
            r"C:\AgentMux\agentmux.exe"
        );
        assert_eq!(value["mcpServers"]["agentmux"]["args"][3], "standard");
    }

    #[test]
    fn claude_merge_accepts_a_utf8_bom() {
        let rendered = merge_claude_config(
            Some("\u{feff}{\"mcpServers\":{}}"),
            Path::new(r"C:\AgentMux\agentmux.exe"),
            McpProfile::Read,
            Path::new(r"C:\Users\tester\.claude.json"),
        )
        .expect("merge BOM-prefixed Claude config");
        let value: Value = serde_json::from_str(&rendered.contents).expect("valid JSON");
        assert_eq!(value["mcpServers"]["agentmux"]["type"], "stdio");
    }

    #[test]
    fn claude_unrelated_server_name_is_a_review_conflict() {
        let error = merge_claude_config(
            Some(r#"{"mcpServers":{"agentmux":{"command":"node","args":["other.js"]}}}"#),
            Path::new(r"C:\AgentMux\agentmux.exe"),
            McpProfile::Read,
            Path::new(r"C:\Users\tester\.claude.json"),
        )
        .expect_err("conflict expected");
        assert!(error.to_string().contains("unrelated"));
    }

    #[test]
    fn install_creates_backup_before_replacing_existing_config() {
        let test_dir = TestDir::new("backup");
        let executable_path = test_dir.path().join("agentmux.exe");
        fs::write(&executable_path, b"test executable").expect("write executable");
        let config_path = test_dir.path().join(".claude.json");
        let original = r#"{"mcpServers":{"docs":{"command":"docs.exe"}}}"#;
        fs::write(&config_path, original).expect("write original config");

        let result = configure(&request(
            McpClient::Claude,
            McpProfile::Read,
            executable_path,
            config_path.clone(),
            true,
        ))
        .expect("install config");

        assert_eq!(result.status, McpConfigStatus::Installed);
        let backup_path = result.backup_path.expect("backup path");
        assert_eq!(fs::read_to_string(backup_path).unwrap(), original);
        let installed: Value =
            serde_json::from_str(&fs::read_to_string(config_path).unwrap()).unwrap();
        assert_eq!(installed["mcpServers"]["agentmux"]["type"], "stdio");
    }

    #[test]
    fn installing_new_codex_config_is_atomic_and_needs_no_backup() {
        let test_dir = TestDir::new("new-codex");
        let executable_path = test_dir.path().join("agentmux.exe");
        fs::write(&executable_path, b"test executable").expect("write executable");
        let config_path = test_dir.path().join("codex").join("config.toml");

        let result = configure(&request(
            McpClient::Codex,
            McpProfile::Standard,
            executable_path,
            config_path.clone(),
            true,
        ))
        .expect("install new config");

        assert_eq!(result.status, McpConfigStatus::Installed);
        assert_eq!(result.backup_path, None);
        let installed = fs::read_to_string(config_path).expect("read installed config");
        assert!(installed.contains("[mcp_servers.agentmux]"));
        assert!(installed.contains("\"standard\""));
        assert!(fs::read_dir(test_dir.path().join("codex"))
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains("agentmux.tmp")));
    }

    #[test]
    fn repeated_install_is_unchanged_and_does_not_create_another_backup() {
        let test_dir = TestDir::new("unchanged");
        let executable_path = test_dir.path().join("agentmux.exe");
        fs::write(&executable_path, b"test executable").expect("write executable");
        let config_path = test_dir.path().join(".claude.json");
        let install_request = request(
            McpClient::Claude,
            McpProfile::Full,
            executable_path,
            config_path,
            true,
        );

        let first = configure(&install_request).expect("first install");
        let second = configure(&install_request).expect("repeat install");

        assert_eq!(first.status, McpConfigStatus::Installed);
        assert_eq!(second.status, McpConfigStatus::Unchanged);
        assert!(!second.changed);
        assert_eq!(second.backup_path, None);
        assert_eq!(
            fs::read_dir(test_dir.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains("agentmux.bak"))
                .count(),
            0
        );
    }

    #[test]
    fn codex_install_remerges_a_concurrent_client_change() {
        let test_dir = TestDir::new("codex-concurrent-remerge");
        let executable_path = test_dir.path().join("agentmux.exe");
        fs::write(&executable_path, b"test executable").expect("write executable");
        let config_path = test_dir.path().join("config.toml");
        fs::write(&config_path, "model = \"before\"\n").expect("write initial config");
        let concurrent = "model = \"before\"\n\n[features]\nconcurrent = true\n";
        let concurrent_path = config_path.clone();

        let result = configure_with_before_install(
            &request(
                McpClient::Codex,
                McpProfile::Standard,
                executable_path,
                config_path.clone(),
                true,
            ),
            || fs::write(&concurrent_path, concurrent).expect("write concurrent Codex change"),
        )
        .expect("install after concurrent change");

        assert_eq!(result.status, McpConfigStatus::Installed);
        let installed = fs::read_to_string(&config_path).expect("read installed config");
        assert!(installed.contains("[features]\nconcurrent = true"));
        assert!(installed.contains("[mcp_servers.agentmux]"));
        assert_eq!(
            fs::read_to_string(result.backup_path.expect("backup latest snapshot")).unwrap(),
            concurrent
        );
    }

    #[test]
    fn claude_install_remerges_a_concurrent_client_change() {
        let test_dir = TestDir::new("claude-concurrent-remerge");
        let executable_path = test_dir.path().join("agentmux.exe");
        fs::write(&executable_path, b"test executable").expect("write executable");
        let config_path = test_dir.path().join(".claude.json");
        fs::write(&config_path, r#"{"theme":"before"}"#).expect("write initial config");
        let concurrent = r#"{"theme":"before","concurrent":{"kept":true}}"#;
        let concurrent_path = config_path.clone();

        let result = configure_with_before_install(
            &request(
                McpClient::Claude,
                McpProfile::Standard,
                executable_path,
                config_path.clone(),
                true,
            ),
            || fs::write(&concurrent_path, concurrent).expect("write concurrent Claude change"),
        )
        .expect("install after concurrent change");

        assert_eq!(result.status, McpConfigStatus::Installed);
        let installed: Value =
            serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(installed["concurrent"]["kept"], true);
        assert_eq!(installed["mcpServers"]["agentmux"]["type"], "stdio");
        assert_eq!(
            fs::read_to_string(result.backup_path.expect("backup latest snapshot")).unwrap(),
            concurrent
        );
    }

    #[test]
    fn compare_before_replace_rejects_a_stale_snapshot_without_writing() {
        let test_dir = TestDir::new("stale-cas");
        let target = test_dir.path().join("config.toml");
        fs::write(&target, "concurrent\n").expect("write current target");

        let attempt =
            install_contents_with(&target, Some(b"original\n"), b"replacement\n", |_, _, _| {
                panic!("replace must not run for a stale snapshot")
            })
            .expect("stale snapshot is not an I/O failure");

        assert_eq!(attempt, InstallAttempt::Stale);
        assert_eq!(fs::read_to_string(&target).unwrap(), "concurrent\n");
        assert_eq!(fs::read_dir(test_dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_atomic_replace_leaves_original_and_backup_intact() {
        let test_dir = TestDir::new("rollback");
        let target = test_dir.path().join("config.toml");
        fs::write(&target, "original\n").expect("write target");

        let error =
            install_contents_with(&target, Some(b"original\n"), b"replacement\n", |_, _, _| {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "simulated replace failure",
                ))
            })
            .expect_err("replace must fail");

        assert!(error.to_string().contains("atomically replace"));
        assert_eq!(fs::read_to_string(&target).unwrap(), "original\n");
        let backups = fs::read_dir(test_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("agentmux.bak"))
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read_to_string(backups[0].path()).unwrap(), "original\n");
        assert!(fs::read_dir(test_dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains("agentmux.tmp")));
    }

    #[test]
    fn invalid_claude_json_is_never_modified_or_backed_up() {
        let test_dir = TestDir::new("invalid-json");
        let executable_path = test_dir.path().join("agentmux.exe");
        fs::write(&executable_path, b"test executable").expect("write executable");
        let config_path = test_dir.path().join(".claude.json");
        fs::write(&config_path, "{ invalid").expect("write invalid config");

        let error = configure(&request(
            McpClient::Claude,
            McpProfile::Read,
            executable_path,
            config_path.clone(),
            true,
        ))
        .expect_err("parse must fail");

        assert!(error.to_string().contains("JSON parse failed"));
        assert_eq!(fs::read_to_string(config_path).unwrap(), "{ invalid");
        assert_eq!(fs::read_dir(test_dir.path()).unwrap().count(), 2);
    }
}
