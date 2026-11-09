use super::{Diagnostics, EngineConfig, EngineKind};
use serde_json::Value;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::time::{Duration, Instant};

/// Provider output stays small even when input includes base64 image blocks.
pub(super) const MAX_FRAME: usize = 1024 * 1024;
pub(super) const MAX_INPUT_FRAME: usize = 20 * 1024 * 1024;

/// Resolve known npm packages without passing user input to cmd.exe or a shell.
pub(super) fn cli_command(config: &EngineConfig) -> Result<Command, String> {
    let name = match config.kind {
        EngineKind::Codex => "codex",
        EngineKind::Claude => "claude",
        EngineKind::Antigravity => "agy",
        _ => return Err("API engines do not have a CLI".into()),
    };
    let executable = config
        .executable
        .clone()
        .or_else(|| find_executable(name))
        .or_else(|| {
            if config.kind != EngineKind::Antigravity {
                return None;
            }
            let home = directories::BaseDirs::new()?;
            executable_on_path(name, home.home_dir().join(".local/bin").as_os_str())
        })
        .ok_or("AI CLI not found; install it or select its executable")?;
    #[cfg(windows)]
    let executable = if config.kind == EngineKind::Codex {
        let desktop_bin = directories::BaseDirs::new().map(|dirs| {
            dirs.data_local_dir()
                .join("OpenAI")
                .join("Codex")
                .join("bin")
        });
        codex_with_tool_host(
            executable,
            &std::env::var_os("PATH").unwrap_or_default(),
            desktop_bin.as_deref(),
        )
    } else {
        executable
    };
    if executable
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
        || executable.extension().is_none() && cfg!(windows)
    {
        let parent = executable.parent().ok_or("Invalid CLI executable")?;
        let package = match config.kind {
            EngineKind::Codex => "node_modules/@openai/codex/bin/codex.js",
            EngineKind::Claude => "node_modules/@anthropic-ai/claude-code/cli.js",
            _ => return Err("请选择 Antigravity 的原生 agy.exe 文件".into()),
        };
        let script = parent.join(package);
        if script.is_file() {
            let node = parent.join("node.exe");
            let node = if node.is_file() {
                node
            } else {
                find_executable("node").ok_or("Node.js is required for this CLI installation")?
            };
            let mut command = Command::new(node);
            command.arg(script);
            return Ok(command);
        }
        return Err("Unsupported CLI shim; choose the native executable or install the standard npm package".into());
    }
    if executable
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("js") || e.eq_ignore_ascii_case("mjs"))
    {
        let mut command = Command::new(find_executable("node").ok_or("Node.js is required")?);
        command.arg(executable);
        Ok(command)
    } else {
        Ok(Command::new(executable))
    }
}
pub(super) fn plugin_command(path: &Path) -> Result<Command, String> {
    if !path.is_absolute() || !path.is_file() {
        return Err("自定义引擎文件不存在，请在 AI 设置中重新选择".into());
    }
    let extension = path.extension().and_then(OsStr::to_str).unwrap_or_default();
    if extension.eq_ignore_ascii_case("mjs") {
        let node = find_executable("node").ok_or("运行 .mjs 引擎需要在 PATH 中安装 Node.js")?;
        let mut command = Command::new(node);
        command.arg("--").arg(path);
        return Ok(command);
    }
    if cfg!(windows) && !extension.eq_ignore_ascii_case("exe") {
        return Err("Windows 自定义引擎需要 .exe 或 .mjs 文件".into());
    }
    Ok(Command::new(path))
}

fn find_executable(name: &str) -> Option<PathBuf> {
    executable_on_path(name, &std::env::var_os("PATH").unwrap_or_default())
}

fn executable_on_path(name: &str, path: &OsStr) -> Option<PathBuf> {
    let extensions: &[&str] = if cfg!(windows) {
        &["exe", "cmd", ""]
    } else {
        &[""]
    };
    std::env::split_paths(path)
        .flat_map(|dir| {
            extensions.iter().map(move |extension| {
                dir.join(if extension.is_empty() {
                    name.to_owned()
                } else {
                    format!("{name}.{extension}")
                })
            })
        })
        .find(|candidate| candidate.is_file())
}

#[cfg(windows)]
fn codex_with_tool_host(executable: PathBuf, path: &OsStr, desktop_bin: Option<&Path>) -> PathBuf {
    const HOST: &str = "codex-code-mode-host.exe";
    if !executable
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        || executable.with_file_name(HOST).is_file()
    {
        return executable;
    }
    if let Ok(target) = executable.canonicalize()
        && target.with_file_name(HOST).is_file()
    {
        return target;
    }
    let Some(file_id) = windows_file_id(&executable) else {
        return executable;
    };

    // The Desktop CLI alias and its installation can be hard links to one file.
    let desktop_installs = desktop_bin
        .and_then(|root| root.read_dir().ok())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path());
    std::env::split_paths(path)
        .chain(desktop_installs)
        .map(|dir| dir.join("codex.exe"))
        .find(|candidate| {
            candidate.with_file_name(HOST).is_file() && windows_file_id(candidate) == Some(file_id)
        })
        .unwrap_or(executable)
}

#[cfg(windows)]
fn windows_file_id(path: &Path) -> Option<(u32, u32, u32)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };

    let file = std::fs::File::open(path).ok()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // The file owns the live handle and the API writes into the initialized struct.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return None;
    }
    Some((
        info.dwVolumeSerialNumber,
        info.nFileIndexHigh,
        info.nFileIndexLow,
    ))
}

pub(super) fn diagnostics(config: &EngineConfig) -> Result<Diagnostics, String> {
    if config.kind == EngineKind::Plugin {
        let result = plugin_command(
            config
                .executable
                .as_deref()
                .unwrap_or_else(|| Path::new("")),
        );
        return Ok(Diagnostics {
            available: result.is_ok(),
            version: None,
            login_hint: "自定义引擎由本机文件启动并管理认证".into(),
            action_hint: result
                .err()
                .unwrap_or_else(|| "发送消息时检查引擎协议版本".into()),
        });
    }
    if config.kind == EngineKind::OpenAiCompatible {
        return Ok(Diagnostics {
            available: true,
            version: None,
            login_hint: "API key is stored separately in the OS vault or for this session only"
                .into(),
            action_hint: "Choose a model and send a test prompt to verify the endpoint".into(),
        });
    }
    let mut command = match cli_command(config) {
        Ok(c) => c,
        Err(_) => {
            return Ok(Diagnostics {
                available: false,
                version: None,
                login_hint: "Login status is not inspected".into(),
                action_hint: if config.kind == EngineKind::Antigravity {
                    "安装 Antigravity CLI 1.5.3 或更新版本，运行 agy 登录；也可在高级设置指定可执行文件"
                } else {
                    "Install the CLI, select its executable, then log in from a terminal"
                }
                .into(),
            });
        }
    };
    command.arg("--version");
    let (success, output) = capture_status(&mut command, 4096)?;
    let version = output
        .lines()
        .next()
        .map(str::trim)
        .filter(|s| {
            s.len() <= 128
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || " ._-()".contains(c))
        })
        .map(str::to_owned);
    let compatible = version
        .as_deref()
        .is_some_and(|v| version_supported(config.kind, v));
    if config.kind == EngineKind::Antigravity {
        let status = (success && compatible)
            .then(|| super::discover_models(config))
            .transpose();
        return Ok(Diagnostics {
            available: matches!(&status, Ok(Some(_))),
            version,
            login_hint: match status {
                Ok(Some(_)) => "Antigravity CLI 已返回账户的模型目录".into(),
                Err(error) => error,
                Ok(None) => "当前 CLI 版本无法连接".into(),
            },
            action_hint: "使用 Antigravity CLI 1.5.3 或更新版本；在终端运行 agy 登录".into(),
        });
    }
    let mut login_command = cli_command(config)?;
    match config.kind {
        EngineKind::Codex => {
            login_command.args(["login", "status"]);
        }
        _ => {
            login_command.args(["auth", "status", "--json"]);
        }
    }
    let (login_success, login_output) = capture_status(&mut login_command, 16384)?;
    let logged_in = if config.kind == EngineKind::Claude {
        serde_json::from_str::<Value>(&login_output)
            .ok()
            .and_then(|v| v["loggedIn"].as_bool())
            .unwrap_or(false)
            && login_success
    } else {
        login_success
    };
    Ok(Diagnostics {
        available: success && compatible && logged_in,
        version,
        login_hint: if logged_in {
            "CLI reports logged in; the runtime did not read any login credential"
        } else {
            "CLI is not logged in or status could not be verified"
        }
        .into(),
        action_hint: if !compatible {
            "Update CLI: Codex 0.147+ or Claude Code 2.1+ is required"
        } else if config.kind == EngineKind::Codex {
            "Run codex login in a terminal if authentication is required"
        } else {
            "Run claude auth login in a terminal if authentication is required"
        }
        .into(),
    })
}
pub(super) fn version_supported(kind: EngineKind, text: &str) -> bool {
    let Some(version) = text
        .split_whitespace()
        .find(|s| s.chars().next().is_some_and(|c| c.is_ascii_digit()))
    else {
        return false;
    };
    let numbers: Vec<u64> = version
        .split('.')
        .take(3)
        .filter_map(|p| p.parse().ok())
        .collect();
    match numbers.as_slice() {
        [major, minor, ..] if kind == EngineKind::Codex => *major > 0 || *minor >= 147,
        [major, minor, patch, ..] if kind == EngineKind::Antigravity => {
            (*major, *minor, *patch) >= (1, 5, 3)
        }
        [major, minor, ..] => *major > 2 || *major == 2 && *minor >= 1,
        _ => false,
    }
}
fn capture_status(command: &mut Command, limit: u64) -> Result<(bool, String), String> {
    let mut child = spawn_raw(command, Path::new("."))?;
    let stdout = child.stdout.take().ok_or("Cannot capture CLI version")?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.take(limit).read_to_string(&mut text);
        let _ = tx.send(text);
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => {
                std::thread::park_timeout(Duration::from_millis(20))
            }
            _ => {
                kill_tree(child.id());
                let _ = child.wait();
                break false;
            }
        }
    };
    Ok((
        success,
        rx.recv_timeout(Duration::from_millis(100))
            .unwrap_or_default(),
    ))
}

fn spawn_raw(command: &mut Command, cwd: &Path) -> Result<Child, String> {
    command
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .spawn()
        .map_err(|_| "Cannot start AI CLI; check executable and working directory".into())
}

pub(super) struct Process {
    child: Child,
    pub writer: SyncSender<Value>,
    pub lines: Receiver<Result<Value, String>>,
    stop: Arc<AtomicBool>,
    #[cfg(windows)]
    _job: WindowsJob,
}
impl Process {
    pub fn spawn(command: &mut Command, cwd: &Path) -> Result<Self, String> {
        let mut child = spawn_raw(command, cwd)?;
        #[cfg(windows)]
        let job = match WindowsJob::attach(&child) {
            Ok(job) => job,
            Err(error) => {
                kill_tree(child.id());
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let mut stdin = child.stdin.take().ok_or("Cannot open CLI input")?;
        let stdout = child.stdout.take().ok_or("Cannot open CLI output")?;
        let stop = Arc::new(AtomicBool::new(false));
        let writer_stop = stop.clone();
        let (writer, writes) = mpsc::sync_channel::<Value>(64);
        let (tx, lines) = mpsc::sync_channel(64);
        let write_errors = tx.clone();
        std::thread::spawn(move || {
            while !writer_stop.load(Ordering::Acquire) {
                match writes.recv_timeout(Duration::from_millis(50)) {
                    Ok(value) => {
                        let mut bytes = match encode_input(&value) {
                            Ok(bytes) => bytes,
                            Err(error) => {
                                let _ = write_errors.send(Err(error));
                                break;
                            }
                        };
                        bytes.push(b'\n');
                        if stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_err() {
                            let _ = write_errors.send(Err("Cannot write AI CLI input".into()));
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => break,
                }
            }
        });
        let reader_stop = stop.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                if reader_stop.load(Ordering::Acquire) {
                    break;
                }
                let result = read_cli_frame(&mut reader);
                match result {
                    Ok(Some(value)) => {
                        if tx.send(Ok(value)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        // The writer also owns a sender for write failures; EOF
                        // must be explicit or its sender would hide disconnection.
                        let _ =
                            tx.send(Err("AI CLI closed output before completing the turn".into()));
                        break;
                    }
                    Err(error) => {
                        let _ = tx.send(Err(error));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            writer,
            lines,
            stop,
            #[cfg(windows)]
            _job: job,
        })
    }
    pub fn send(&self, value: Value) -> Result<(), String> {
        // Count serialized bytes (including JSON escapes) before enqueueing. A
        // rejected frame must report an error rather than silently closing stdin.
        validate_input(&value)?;
        self.writer
            .try_send(value)
            .map_err(|_| "AI CLI input channel is unavailable".into())
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        kill_tree(self.child.id());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Preflight without allocating another copy of a potentially large image frame.
struct FrameSize {
    bytes: usize,
    limit: usize,
}
impl Write for FrameSize {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes) {
            return Err(std::io::Error::other("AI frame exceeded the size limit"));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn validate_input(value: &Value) -> Result<(), String> {
    serde_json::to_writer(
        &mut FrameSize {
            bytes: 0,
            limit: MAX_INPUT_FRAME,
        },
        value,
    )
    .map_err(|_| "AI CLI input exceeded the size limit".into())
}
fn encode_input(value: &Value) -> Result<Vec<u8>, String> {
    // The writer channel also accepts small cancellation frames directly.
    // Validate here as well so even these cannot silently close the pipe.
    validate_input(value)?;
    serde_json::to_vec(value).map_err(|_| "Cannot encode AI CLI input".into())
}

/// Native CLIs can echo images and return large inherited MCP catalogs. Accept
/// these envelopes transiently, keeping the retained frame limit at 1 MiB.
fn read_cli_frame(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
    let Some(bytes) = read_frame_with_limit(reader, MAX_INPUT_FRAME)? else {
        return Ok(None);
    };
    let mut value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "AI CLI emitted invalid JSON".to_owned())?;
    let discarded_echo = discard_input_echoes(&mut value);
    let discarded_inventory = discard_codex_mcp_schemas(&mut value);
    let discarded_acp_payload = discard_acp_payload(&mut value);
    if bytes.len() > MAX_FRAME && !discarded_echo && !discarded_inventory && !discarded_acp_payload
    {
        return Err("AI stream frame exceeded the size limit".into());
    }
    serde_json::to_writer(
        &mut FrameSize {
            bytes: 0,
            limit: MAX_FRAME,
        },
        &value,
    )
    .map_err(|_| "AI stream frame exceeded the size limit".to_owned())?;
    Ok(Some(value))
}

fn discard_acp_payload(frame: &mut Value) -> bool {
    if frame["method"] != "session/update" {
        return false;
    }
    let Some(update) = frame
        .pointer_mut("/params/update")
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let before = update.len();
    match update.get("sessionUpdate").and_then(Value::as_str) {
        Some("user_message_chunk") => {
            update.remove("content");
        }
        Some("tool_call" | "tool_call_update") => {
            update.retain(|key, _| {
                matches!(
                    key.as_str(),
                    "sessionUpdate" | "toolCallId" | "name" | "status"
                )
            });
        }
        _ => {}
    }
    update.len() != before
}

fn discard_codex_mcp_schemas(frame: &mut Value) -> bool {
    if frame["id"] != super::codex::MCP_INVENTORY_ID {
        return false;
    }
    let Some(servers) = frame
        .pointer_mut("/result/data")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut discarded = false;
    for server in servers.iter_mut().filter_map(Value::as_object_mut) {
        // The adapter needs only these identities to disable inherited servers.
        let fields = server.len();
        server.retain(|field, _| field == "name");
        discarded |= server.len() != fields;
    }
    discarded
}
fn discard_input_echoes(frame: &mut Value) -> bool {
    let mut discarded = false;
    if frame["type"] == "user"
        && let Some(content) = frame
            .pointer_mut("/message/content")
            .and_then(Value::as_array_mut)
    {
        // Claude user messages are otherwise input echoes. Actual MCP tool
        // results remain intact and are subject to the normal output limit.
        let previous_len = content.len();
        content.retain(|block| block["type"] == "tool_result");
        discarded |= content.len() != previous_len;
        for result in content.iter_mut() {
            if let Some(blocks) = result.get_mut("content").and_then(Value::as_array_mut) {
                let before = blocks.len();
                blocks.retain(|block| block["type"] != "image");
                discarded |= blocks.len() != before;
            }
        }
    }
    if matches!(
        frame["method"].as_str(),
        Some("item/started" | "item/completed")
    ) && let Some(item) = frame.pointer_mut("/params/item")
    {
        discarded |= discard_codex_user_item(item);
    }
    for pointer in ["/params/turn", "/result/turn"] {
        if let Some(turn) = frame.pointer_mut(pointer) {
            discarded |= discard_turn_echoes(turn);
        }
    }
    for pointer in ["/params/thread/turns", "/result/thread/turns"] {
        if let Some(turns) = frame.pointer_mut(pointer).and_then(Value::as_array_mut) {
            for turn in turns {
                discarded |= discard_turn_echoes(turn);
            }
        }
    }
    discarded
}
fn discard_turn_echoes(turn: &mut Value) -> bool {
    let mut discarded = false;
    if let Some(items) = turn.get_mut("items").and_then(Value::as_array_mut) {
        for item in items {
            discarded |= discard_codex_user_item(item);
        }
    }
    discarded
}
fn discard_codex_user_item(item: &mut Value) -> bool {
    if item["type"] == "userMessage"
        && let Some(content) = item.get_mut("content").and_then(Value::as_array_mut)
        && !content.is_empty()
    {
        content.clear();
        true
    } else {
        false
    }
}
pub(super) fn read_frame(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, String> {
    read_frame_with_limit(reader, MAX_FRAME)
}
fn read_frame_with_limit(
    reader: &mut impl BufRead,
    limit: usize,
) -> Result<Option<Vec<u8>>, String> {
    let mut bytes = Vec::new();
    let n = reader
        .take((limit + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| "Cannot read AI stream")?;
    if n == 0 {
        return Ok(None);
    }
    if n > limit {
        return Err("AI stream frame exceeded the size limit".into());
    }
    Ok(Some(bytes))
}
fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .status();
    }
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
#[cfg(windows)]
struct WindowsJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl WindowsJob {
    fn attach(child: &Child) -> Result<Self, String> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        // SAFETY: Handles remain owned here/by Child; structures have the exact API size.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err("Cannot create AI process isolation job".into());
            }
            let job = Self(handle);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle() as _) == 0
            {
                return Err("Cannot isolate AI process tree; refusing unsafe startup".into());
            }
            Ok(job)
        }
    }
}
#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frames_are_bounded() {
        let mut input = std::io::Cursor::new(vec![b'x'; MAX_FRAME + 2]);
        assert!(read_frame(&mut input).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn codex_finds_its_desktop_tool_host_even_when_the_install_is_off_path() {
        let root = std::env::temp_dir().join(format!("ai-codex-path-{}", uuid::Uuid::new_v4()));
        let shim = root.join("shim");
        let other = root.join("other");
        let desktop_bin = root.join("desktop-bin");
        let installed = desktop_bin.join("version");
        for dir in [&shim, &other, &installed] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let shim_cli = shim.join("codex.exe");
        let other_cli = other.join("codex.exe");
        let installed_cli = installed.join("codex.exe");
        let other_host = other.join("codex-code-mode-host.exe");
        let host = installed.join("codex-code-mode-host.exe");
        std::fs::write(&shim_cli, b"codex").unwrap();
        std::fs::write(&other_cli, b"other").unwrap();
        std::fs::write(&other_host, []).unwrap();
        std::fs::hard_link(&shim_cli, &installed_cli).unwrap();

        let path = std::env::join_paths([&shim, &other]).unwrap();
        let discovered = executable_on_path("codex", &path).unwrap();
        assert_eq!(discovered, shim_cli);
        assert_eq!(
            codex_with_tool_host(discovered.clone(), &path, Some(&desktop_bin)),
            shim_cli
        );
        std::fs::write(&host, []).unwrap();
        assert_eq!(
            codex_with_tool_host(discovered, &path, Some(&desktop_bin)),
            installed_cli
        );

        for file in [shim_cli, other_cli, installed_cli, other_host, host] {
            std::fs::remove_file(file).unwrap();
        }
        for dir in [shim, other, installed, desktop_bin, root] {
            std::fs::remove_dir(dir).unwrap();
        }
    }
    #[test]
    fn codex_inventory_keeps_server_names_and_cursor_with_large_schemas() {
        use serde_json::json;
        let frame = json!({
            "id": super::super::codex::MCP_INVENTORY_ID,
            "result": {
                "data": [{"name":"inherited","tools":{"large":"x".repeat(2 * MAX_FRAME)}}],
                "nextCursor": "more"
            }
        });
        let bytes = serde_json::to_vec(&frame).unwrap();
        let retained = read_cli_frame(&mut std::io::Cursor::new(bytes))
            .unwrap()
            .unwrap();
        assert_eq!(
            retained["result"],
            json!({"data":[{"name":"inherited"}],"nextCursor":"more"})
        );
    }
    #[test]
    fn image_input_frames_have_separate_limits_and_count_json_escapes() {
        let frame = serde_json::json!({"type":"image","url":"x".repeat(MAX_FRAME + 1)});
        let encoded = encode_input(&frame).unwrap();
        assert!(encoded.len() > MAX_FRAME);
        assert!(encoded.len() < MAX_INPUT_FRAME);
        let escaped = serde_json::json!("\n".repeat(MAX_INPUT_FRAME / 2));
        assert_eq!(
            validate_input(&escaped).unwrap_err(),
            "AI CLI input exceeded the size limit"
        );
        assert!(encode_input(&escaped).is_err());
        let boundary = serde_json::json!("x".repeat(MAX_INPUT_FRAME - 2));
        assert!(validate_input(&boundary).is_ok());
        let oversized = serde_json::json!("private-image-data".repeat(MAX_INPUT_FRAME / 18 + 1));
        let error = validate_input(&oversized).unwrap_err();
        assert!(!error.contains("private-image-data"));
    }
    #[test]
    fn cli_image_echo_envelopes_are_discarded_but_public_outputs_stay_bounded() {
        use serde_json::json;
        let image = json!({"type":"image","url":format!("data:image/png;base64,{}", "private-image".repeat(MAX_FRAME / 12 + 1))});
        let item = json!({"type":"userMessage","id":"input","content":[image]});
        for frame in [
            json!({"method":"item/started","params":{"item":item}}),
            json!({"method":"item/completed","params":{"item":item}}),
            json!({"method":"turn/completed","params":{"turn":{"id":"turn","items":[item]}}}),
            json!({"id":3,"result":{"turn":{"id":"turn","items":[item]}}}),
            json!({"id":2,"result":{"thread":{"id":"thread","turns":[{"id":"turn","items":[item]}]}}}),
            json!({"type":"user","message":{"content":[image, {"type":"text","text":"private-input"}]}}),
        ] {
            let bytes = serde_json::to_vec(&frame).unwrap();
            assert!(bytes.len() > MAX_FRAME);
            let output = read_cli_frame(&mut std::io::Cursor::new(bytes))
                .unwrap()
                .unwrap();
            let serialized = output.to_string();
            assert!(!serialized.contains("private-image"));
            assert!(!serialized.contains("private-input"));
            assert!(serialized.len() < MAX_FRAME);
        }
        for frame in [
            json!({"method":"item/agentMessage/delta","params":{"delta":"x".repeat(MAX_FRAME)}}),
            json!({"type":"user","message":{"content":[{"type":"tool_result","content":"x".repeat(MAX_FRAME)}]}}),
            json!({"type":"assistant","message":{"content":[image]}}),
        ] {
            let bytes = serde_json::to_vec(&frame).unwrap();
            assert!(read_cli_frame(&mut std::io::Cursor::new(bytes)).is_err());
        }
        let tool_result = json!({"type":"tool_result","content":"kept tool output"});
        let frame = json!({"type":"user","message":{"content":[image, tool_result]}});
        let output = read_cli_frame(&mut std::io::Cursor::new(
            serde_json::to_vec(&frame).unwrap(),
        ))
        .unwrap()
        .unwrap();
        assert_eq!(output["message"]["content"], json!([tool_result]));
        assert!(
            read_cli_frame(&mut std::io::Cursor::new(vec![b'x'; MAX_INPUT_FRAME + 1])).is_err()
        );
    }
    #[test]
    fn cmd_shim_never_uses_command_interpreter() {
        let dir = std::env::temp_dir().join(format!("ai-cli-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("node_modules/@openai/codex/bin")).unwrap();
        std::fs::write(dir.join("node_modules/@openai/codex/bin/codex.js"), "").unwrap();
        std::fs::write(dir.join("node.exe"), "").unwrap();
        let mut config = EngineConfig::new(EngineKind::Codex);
        config.executable = Some(dir.join("codex.cmd"));
        let command = cli_command(&config).unwrap();
        assert_eq!(command.get_program(), dir.join("node.exe"));
        assert_eq!(command.get_args().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
