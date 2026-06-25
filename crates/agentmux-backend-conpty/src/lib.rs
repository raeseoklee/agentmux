use agentmux_backend::{
    AttachRequest, BackendError, BackendEvent, BackendKind, BackendResult, CommandSpec,
    ControlCode, InputEvent, NamedKey, SessionBackend, SessionHandle, SpawnRequest, TerminalSize,
    TerminationMode,
};

pub fn default_windows_shell() -> CommandSpec {
    if cfg!(windows) {
        CommandSpec::new("powershell.exe")
    } else {
        CommandSpec::new("pwsh")
    }
}

pub fn input_event_bytes(input: &InputEvent) -> BackendResult<Vec<u8>> {
    match input {
        InputEvent::Text(text) => Ok(text.as_bytes().to_vec()),
        InputEvent::Paste { text, bracketed } => {
            if *bracketed {
                let mut bytes = b"\x1b[200~".to_vec();
                bytes.extend_from_slice(text.as_bytes());
                bytes.extend_from_slice(b"\x1b[201~");
                Ok(bytes)
            } else {
                Ok(text.as_bytes().to_vec())
            }
        }
        InputEvent::Key(key) => named_key_bytes(key),
        InputEvent::Control(ControlCode::Interrupt) => Ok(vec![0x03]),
        InputEvent::Control(ControlCode::EndOfTransmission) => Ok(vec![0x04]),
    }
}

fn named_key_bytes(key: &NamedKey) -> BackendResult<Vec<u8>> {
    let bytes = match key {
        NamedKey::Enter => b"\r".to_vec(),
        NamedKey::Backspace => vec![0x7f],
        NamedKey::Tab => b"\t".to_vec(),
        NamedKey::Escape => vec![0x1b],
        NamedKey::ArrowUp => b"\x1b[A".to_vec(),
        NamedKey::ArrowDown => b"\x1b[B".to_vec(),
        NamedKey::ArrowRight => b"\x1b[C".to_vec(),
        NamedKey::ArrowLeft => b"\x1b[D".to_vec(),
        NamedKey::Function(1) => b"\x1bOP".to_vec(),
        NamedKey::Function(2) => b"\x1bOQ".to_vec(),
        NamedKey::Function(3) => b"\x1bOR".to_vec(),
        NamedKey::Function(4) => b"\x1bOS".to_vec(),
        NamedKey::Function(5) => b"\x1b[15~".to_vec(),
        NamedKey::Function(6) => b"\x1b[17~".to_vec(),
        NamedKey::Function(7) => b"\x1b[18~".to_vec(),
        NamedKey::Function(8) => b"\x1b[19~".to_vec(),
        NamedKey::Function(9) => b"\x1b[20~".to_vec(),
        NamedKey::Function(10) => b"\x1b[21~".to_vec(),
        NamedKey::Function(11) => b"\x1b[23~".to_vec(),
        NamedKey::Function(12) => b"\x1b[24~".to_vec(),
        NamedKey::Function(number) => {
            return Err(BackendError::invalid_request(format!(
                "Function key F{number} is not supported."
            )));
        }
    };

    Ok(bytes)
}

pub fn command_line(command: &CommandSpec) -> String {
    std::iter::once(command.executable.as_str())
        .chain(command.args.iter().map(String::as_str))
        .map(quote_windows_arg)
        .collect::<Vec<_>>()
        .join(" ")
}

fn environment_block(overrides: &[(String, String)]) -> Vec<u16> {
    let mut entries = std::env::vars().collect::<Vec<_>>();
    for (key, value) in overrides {
        if key.is_empty() || key.contains('=') {
            continue;
        }
        if let Some((_, existing_value)) = entries
            .iter_mut()
            .find(|(existing_key, _)| existing_key.eq_ignore_ascii_case(key))
        {
            *existing_value = value.clone();
        } else {
            entries.push((key.clone(), value.clone()));
        }
    }
    entries.sort_by_key(|(key, _)| key.to_ascii_uppercase());

    let mut block = Vec::new();
    for (key, value) in entries {
        block.extend(format!("{key}={value}").encode_utf16());
        block.push(0);
    }
    block.push(0);
    block
}

fn quote_windows_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_string();
    }

    let needs_quotes = arg
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\\'));

    if !needs_quotes {
        return arg.to_string();
    }

    let mut quoted = String::from("\"");
    let mut backslashes = 0;

    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.push_str(&"\\".repeat(backslashes));
                quoted.push(ch);
                backslashes = 0;
            }
        }
    }

    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::ptr::{null, null_mut};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };
    use std::thread;
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
    use windows_sys::Win32::System::Console::{
        ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD, HPCON,
    };
    use windows_sys::Win32::System::Pipes::CreatePipe;
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
        InitializeProcThreadAttributeList, TerminateProcess, UpdateProcThreadAttribute,
        WaitForSingleObject, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT,
        PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, STARTF_USESTDHANDLES,
        STARTUPINFOEXW,
    };

    const STILL_ACTIVE: u32 = 259;

    #[derive(Default)]
    pub struct ConptyBackend {
        sessions: HashMap<String, WindowsSession>,
        events: Arc<Mutex<Vec<BackendEvent>>>,
    }

    struct WindowsSession {
        session_id: String,
        hpc: HPCON,
        input_write: HANDLE,
        process_handle: HANDLE,
        _attribute_list: ProcThreadAttributeList,
        exit_reported: bool,
        output_paused: Arc<AtomicBool>,
    }

    // Windows HANDLE/HPCON values are process handles that can be closed from any thread.
    // Session mutation still requires `&mut ConptyBackend`, so shared app state serializes access.
    unsafe impl Send for WindowsSession {}

    impl Drop for WindowsSession {
        fn drop(&mut self) {
            unsafe {
                if !self.input_write.is_null() {
                    CloseHandle(self.input_write);
                }
                if self.hpc != 0 {
                    ClosePseudoConsole(self.hpc);
                }
                if !self.process_handle.is_null() {
                    CloseHandle(self.process_handle);
                }
            }
        }
    }

    impl ConptyBackend {
        pub fn new() -> Self {
            Self::default()
        }

        fn push_event(&self, event: BackendEvent) {
            if let Ok(mut events) = self.events.lock() {
                events.push(event);
            }
        }

        fn poll_exits(&mut self) {
            let mut exited = Vec::new();

            for session in self.sessions.values_mut() {
                if session.exit_reported {
                    continue;
                }

                let status = unsafe { WaitForSingleObject(session.process_handle, 0) };
                if status != 0 {
                    continue;
                }

                let mut code = STILL_ACTIVE;
                let ok = unsafe { GetExitCodeProcess(session.process_handle, &mut code) };
                if ok == 0 {
                    code = u32::MAX;
                }

                session.exit_reported = true;
                exited.push((
                    session.session_id.clone(),
                    if code == u32::MAX {
                        None
                    } else {
                        Some(code as i32)
                    },
                ));
            }

            for (session_id, code) in exited {
                self.sessions.remove(&session_id);
                self.push_event(BackendEvent::Exited { session_id, code });
            }
        }
    }

    impl SessionBackend for ConptyBackend {
        fn kind(&self) -> BackendKind {
            BackendKind::Conpty
        }

        fn spawn(&mut self, request: SpawnRequest) -> BackendResult<SessionHandle> {
            if request
                .backend
                .is_some_and(|backend| backend != BackendKind::Conpty)
            {
                return Err(BackendError::unsupported(
                    "ConPTY backend cannot spawn the requested backend kind.",
                ));
            }

            let size = coord_from_terminal_size(request.initial_size)?;
            let mut input_read: HANDLE = null_mut();
            let mut input_write: HANDLE = null_mut();
            let mut output_read: HANDLE = null_mut();
            let mut output_write: HANDLE = null_mut();

            unsafe {
                if CreatePipe(&mut input_read, &mut input_write, null(), 0) == 0 {
                    return Err(last_error("CreatePipe(input) failed", "spawn_failed"));
                }

                if CreatePipe(&mut output_read, &mut output_write, null(), 0) == 0 {
                    close_if_nonzero(input_read);
                    close_if_nonzero(input_write);
                    return Err(last_error("CreatePipe(output) failed", "spawn_failed"));
                }
            }

            let mut hpc: HPCON = 0;
            let hr = unsafe { CreatePseudoConsole(size, input_read, output_write, 0, &mut hpc) };

            if failed_hr(hr) {
                unsafe {
                    close_if_nonzero(input_read);
                    close_if_nonzero(input_write);
                    close_if_nonzero(output_read);
                    close_if_nonzero(output_write);
                }
                return Err(BackendError::spawn_failed(format!(
                    "CreatePseudoConsole failed with HRESULT 0x{hr:08x}."
                )));
            }

            unsafe {
                close_if_nonzero(input_read);
                close_if_nonzero(output_write);
            }
            input_read = null_mut();
            output_write = null_mut();

            let mut attribute_list = match ProcThreadAttributeList::new(hpc) {
                Ok(list) => list,
                Err(error) => {
                    unsafe {
                        close_if_nonzero(input_read);
                        close_if_nonzero(input_write);
                        close_if_nonzero(output_read);
                        close_if_nonzero(output_write);
                        ClosePseudoConsole(hpc);
                    }
                    return Err(error);
                }
            };
            let mut startup_info: STARTUPINFOEXW = unsafe { zeroed() };
            startup_info.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
            // Test runners and desktop hosts can have redirected standard handles.
            // Detach those inherited handles so the child talks through ConPTY.
            startup_info.StartupInfo.dwFlags |= STARTF_USESTDHANDLES;
            startup_info.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
            startup_info.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
            startup_info.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
            startup_info.lpAttributeList = attribute_list.as_mut_ptr();

            let mut process_info: PROCESS_INFORMATION = unsafe { zeroed() };
            let mut command_line = wide_null(&command_line(&request.command));
            let mut cwd = request.cwd.as_ref().map(|path| wide_null(path));
            let cwd_ptr = cwd.as_mut().map_or(null(), |value| value.as_ptr());
            let mut environment = environment_block(&request.env);
            let environment_ptr = environment.as_mut_ptr().cast();
            let session_id = request.session_id.clone();
            let events = Arc::clone(&self.events);
            let output_paused = Arc::new(AtomicBool::new(false));
            spawn_output_reader(
                session_id.clone(),
                output_read,
                events,
                Arc::clone(&output_paused),
            );

            let created = unsafe {
                CreateProcessW(
                    null(),
                    command_line.as_mut_ptr(),
                    null(),
                    null(),
                    0,
                    EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                    environment_ptr,
                    cwd_ptr,
                    &startup_info.StartupInfo,
                    &mut process_info,
                )
            };

            if created == 0 {
                unsafe {
                    close_if_nonzero(input_read);
                    close_if_nonzero(input_write);
                    close_if_nonzero(output_write);
                    ClosePseudoConsole(hpc);
                }
                return Err(last_error("CreateProcessW failed", "spawn_failed"));
            }

            unsafe {
                close_if_nonzero(process_info.hThread);
            }

            let process_id = process_info.dwProcessId;
            let session = WindowsSession {
                session_id: session_id.clone(),
                hpc,
                input_write,
                process_handle: process_info.hProcess,
                _attribute_list: attribute_list,
                exit_reported: false,
                output_paused,
            };

            self.sessions.insert(session_id.clone(), session);
            self.push_event(BackendEvent::Started {
                session_id: session_id.clone(),
            });

            Ok(SessionHandle {
                session_id,
                backend_kind: BackendKind::Conpty,
                backend_native_id: Some(process_id.to_string()),
                transport_pid: Some(process_id),
            })
        }

        fn attach(&mut self, _request: AttachRequest) -> BackendResult<SessionHandle> {
            Err(BackendError::unsupported(
                "ConPTY sessions are not attachable.",
            ))
        }

        fn send_input(&mut self, session_id: &str, input: InputEvent) -> BackendResult<()> {
            let bytes = input_event_bytes(&input)?;
            let session = self
                .sessions
                .get(session_id)
                .ok_or_else(|| BackendError::session_not_found(session_id))?;

            let mut written = 0;
            let ok = unsafe {
                WriteFile(
                    session.input_write,
                    bytes.as_ptr(),
                    bytes.len() as u32,
                    &mut written,
                    null_mut(),
                )
            };

            if ok == 0 {
                return Err(last_error("WriteFile(input) failed", "input_failed"));
            }

            if written != bytes.len() as u32 {
                return Err(BackendError::input_failed(format!(
                    "Only wrote {written} of {} input bytes.",
                    bytes.len()
                )));
            }

            Ok(())
        }

        fn resize(&mut self, session_id: &str, size: TerminalSize) -> BackendResult<()> {
            let coord = coord_from_terminal_size(size)?;
            let session = self
                .sessions
                .get(session_id)
                .ok_or_else(|| BackendError::session_not_found(session_id))?;

            let hr = unsafe { ResizePseudoConsole(session.hpc, coord) };
            if failed_hr(hr) {
                return Err(BackendError::resize_failed(format!(
                    "ResizePseudoConsole failed with HRESULT 0x{hr:08x}."
                )));
            }

            self.push_event(BackendEvent::Resized {
                session_id: session_id.to_string(),
                columns: size.columns,
                rows: size.rows,
            });
            Ok(())
        }

        fn terminate(&mut self, session_id: &str, mode: TerminationMode) -> BackendResult<()> {
            match mode {
                TerminationMode::Interrupt => {
                    self.send_input(session_id, InputEvent::Control(ControlCode::Interrupt))
                }
                TerminationMode::Soft => {
                    let session = self
                        .sessions
                        .remove(session_id)
                        .ok_or_else(|| BackendError::session_not_found(session_id))?;
                    drop(session);
                    Ok(())
                }
                TerminationMode::Kill => {
                    let session = self
                        .sessions
                        .remove(session_id)
                        .ok_or_else(|| BackendError::session_not_found(session_id))?;
                    let ok = unsafe { TerminateProcess(session.process_handle, 1) };
                    if ok == 0 {
                        return Err(last_error("TerminateProcess failed", "terminate_failed"));
                    }
                    drop(session);
                    Ok(())
                }
            }
        }

        fn set_output_paused(&mut self, session_id: &str, paused: bool) -> BackendResult<()> {
            let session = self
                .sessions
                .get(session_id)
                .ok_or_else(|| BackendError::session_not_found(session_id))?;
            session.output_paused.store(paused, Ordering::Release);
            Ok(())
        }

        fn drain_events(&mut self) -> Vec<BackendEvent> {
            self.poll_exits();
            if let Ok(mut events) = self.events.lock() {
                std::mem::take(&mut *events)
            } else {
                Vec::new()
            }
        }
    }

    struct ProcThreadAttributeList {
        buffer: Vec<usize>,
    }

    impl ProcThreadAttributeList {
        fn new(hpc: HPCON) -> BackendResult<Self> {
            let mut bytes_required = 0;
            unsafe {
                InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes_required);
            }

            if bytes_required == 0 {
                return Err(last_error(
                    "InitializeProcThreadAttributeList(size) failed",
                    "spawn_failed",
                ));
            }

            let word_size = size_of::<usize>();
            let word_count = bytes_required.div_ceil(word_size);
            let mut list = Self {
                buffer: vec![0; word_count],
            };

            let ok = unsafe {
                InitializeProcThreadAttributeList(list.as_mut_ptr(), 1, 0, &mut bytes_required)
            };

            if ok == 0 {
                return Err(last_error(
                    "InitializeProcThreadAttributeList(init) failed",
                    "spawn_failed",
                ));
            }

            let ok = unsafe {
                UpdateProcThreadAttribute(
                    list.as_mut_ptr(),
                    0,
                    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                    hpc as *const c_void,
                    size_of::<HPCON>(),
                    null_mut(),
                    null(),
                )
            };

            if ok == 0 {
                return Err(last_error(
                    "UpdateProcThreadAttribute(PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE) failed",
                    "spawn_failed",
                ));
            }

            Ok(list)
        }

        fn as_mut_ptr(&mut self) -> *mut c_void {
            self.buffer.as_mut_ptr().cast::<c_void>()
        }
    }

    impl Drop for ProcThreadAttributeList {
        fn drop(&mut self) {
            unsafe {
                DeleteProcThreadAttributeList(self.as_mut_ptr());
            }
        }
    }

    fn spawn_output_reader(
        session_id: String,
        output_read: HANDLE,
        events: Arc<Mutex<Vec<BackendEvent>>>,
        output_paused: Arc<AtomicBool>,
    ) {
        let output_read_value = output_read as isize;
        thread::spawn(move || {
            let output_read = output_read_value as HANDLE;
            let mut buffer = [0u8; 8192];

            loop {
                while output_paused.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(8));
                }
                let mut bytes_read = 0;
                let ok = unsafe {
                    ReadFile(
                        output_read,
                        buffer.as_mut_ptr(),
                        buffer.len() as u32,
                        &mut bytes_read,
                        null_mut(),
                    )
                };

                if ok == 0 || bytes_read == 0 {
                    let message = if ok == 0 {
                        let error = unsafe { GetLastError() };
                        format!("ReadFile(output) ended. GetLastError={error}.")
                    } else {
                        "ReadFile(output) returned zero bytes.".to_string()
                    };

                    if let Ok(mut events) = events.lock() {
                        events.push(BackendEvent::Error {
                            session_id: Some(session_id.clone()),
                            error: BackendError::new("output_read_ended", message),
                        });
                    }
                    break;
                }

                if let Ok(mut events) = events.lock() {
                    events.push(BackendEvent::Output {
                        session_id: session_id.clone(),
                        bytes: buffer[..bytes_read as usize].to_vec(),
                    });
                }
            }

            unsafe {
                CloseHandle(output_read);
            }
        });
    }

    fn coord_from_terminal_size(size: TerminalSize) -> BackendResult<COORD> {
        if size.columns == 0 || size.rows == 0 {
            return Err(BackendError::invalid_request(
                "Terminal size must have non-zero columns and rows.",
            ));
        }

        if size.columns > i16::MAX as u16 || size.rows > i16::MAX as u16 {
            return Err(BackendError::invalid_request(
                "Terminal size exceeds the Windows COORD range.",
            ));
        }

        Ok(COORD {
            X: size.columns as i16,
            Y: size.rows as i16,
        })
    }

    fn failed_hr(hr: i32) -> bool {
        hr < 0
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn close_if_nonzero(handle: HANDLE) {
        if !handle.is_null() {
            CloseHandle(handle);
        }
    }

    fn last_error(context: &str, code: &str) -> BackendError {
        let error = unsafe { GetLastError() };
        BackendError::new(code, format!("{context}. GetLastError={error}."))
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    #[derive(Default)]
    pub struct ConptyBackend {
        events: Vec<BackendEvent>,
    }

    impl ConptyBackend {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl SessionBackend for ConptyBackend {
        fn kind(&self) -> BackendKind {
            BackendKind::Conpty
        }

        fn spawn(&mut self, _request: SpawnRequest) -> BackendResult<SessionHandle> {
            Err(BackendError::unavailable(
                "ConPTY is only available on Windows 10 version 1809 or newer.",
            ))
        }

        fn attach(&mut self, _request: AttachRequest) -> BackendResult<SessionHandle> {
            Err(BackendError::unsupported(
                "ConPTY sessions are not attachable.",
            ))
        }

        fn send_input(&mut self, session_id: &str, _input: InputEvent) -> BackendResult<()> {
            Err(BackendError::session_not_found(session_id))
        }

        fn resize(&mut self, session_id: &str, _size: TerminalSize) -> BackendResult<()> {
            Err(BackendError::session_not_found(session_id))
        }

        fn terminate(&mut self, session_id: &str, _mode: TerminationMode) -> BackendResult<()> {
            Err(BackendError::session_not_found(session_id))
        }

        fn drain_events(&mut self) -> Vec<BackendEvent> {
            std::mem::take(&mut self.events)
        }
    }
}

pub use platform::ConptyBackend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_kind_is_conpty() {
        let backend = ConptyBackend::new();
        assert_eq!(backend.kind(), BackendKind::Conpty);
    }

    #[test]
    fn shell_command_is_not_empty() {
        assert!(!default_windows_shell().executable.is_empty());
    }

    #[test]
    fn input_event_bytes_translate_common_terminal_keys() {
        assert_eq!(
            input_event_bytes(&InputEvent::Key(NamedKey::Enter)).unwrap(),
            b"\r"
        );
        assert_eq!(
            input_event_bytes(&InputEvent::Key(NamedKey::ArrowUp)).unwrap(),
            b"\x1b[A"
        );
        assert_eq!(
            input_event_bytes(&InputEvent::Control(ControlCode::Interrupt)).unwrap(),
            vec![0x03]
        );
    }

    #[test]
    fn bracketed_paste_wraps_payload() {
        let bytes = input_event_bytes(&InputEvent::Paste {
            text: "hello".to_string(),
            bracketed: true,
        })
        .unwrap();

        assert_eq!(bytes, b"\x1b[200~hello\x1b[201~");
    }

    #[test]
    fn command_line_quotes_spaces_and_quotes() {
        let command = CommandSpec::with_args(
            r"C:\Program Files\PowerShell\pwsh.exe",
            vec![
                "-NoLogo".to_string(),
                r#"Write-Output "hello world""#.to_string(),
            ],
        );

        assert_eq!(
            command_line(&command),
            "\"C:\\Program Files\\PowerShell\\pwsh.exe\" -NoLogo \"Write-Output \\\"hello world\\\"\""
        );
    }
}
