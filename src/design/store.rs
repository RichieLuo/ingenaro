use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::ai::ComposerPreferences;

use super::{
    ArtifactContent, ArtifactKind, ChatRecord, ChatRole, ChatStep, Revision, RevisionOrigin, Scope,
    Session, types::text,
};

pub const MAX_STORE_BYTES: usize = 64 * 1024 * 1024;
const MAX_SESSIONS: usize = 64;
const MAX_MESSAGES: usize = 256;
const MAX_REVISIONS: usize = 64;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    format: u32,
    next_id: u64,
    sessions: Vec<Session>,
}

/// Single-writer app-data repository. Mutations are persisted before success and
/// rolled back in memory on failure. Keep one Store alive for the app lifetime.
/// Size/count limits reject new work explicitly; old revisions are never pruned.
pub struct Store {
    directory: PathBuf,
    data: Data,
    _lock: File,
}

impl Store {
    /// `directory` is app-owned storage, never the directory of an RP document.
    pub fn open(directory: impl AsRef<Path>) -> Result<Self> {
        let directory = directory.as_ref().to_path_buf();
        fs::create_dir_all(&directory).context("无法创建设计记录目录")?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("store.lock"))
            .context("无法打开设计记录锁")?;
        lock.try_lock().context("设计记录正由另一个窗口使用")?;
        let path = directory.join("store.json");
        // A staged file is only promoted on explicit successful commit. Crashes
        // leave the previous complete snapshot; never guess which snapshot won.
        let data = match File::open(&path) {
            Ok(file) => {
                ensure!(
                    file.metadata()?.len() <= MAX_STORE_BYTES as u64,
                    "设计记录超过 64 MiB 限制"
                );
                let mut bytes = Vec::new();
                file.take(MAX_STORE_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)?;
                ensure!(bytes.len() <= MAX_STORE_BYTES, "设计记录过大");
                let data: Data = serde_json::from_slice(&bytes)
                    .context("设计记录损坏；原文件已保留，请勿覆盖")?;
                validate_data(&data)?;
                data
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Data {
                format: 1,
                next_id: 1,
                sessions: Vec::new(),
            },
            Err(error) => return Err(error).context("无法读取设计记录"),
        };
        Ok(Self {
            directory,
            data,
            _lock: lock,
        })
    }

    pub fn open_default() -> Result<Self> {
        let directories = directories::ProjectDirs::from("", "Ingenaro", "Ingenaro")
            .context("无法定位 Ingenaro 应用数据目录")?;
        Self::open(directories.data_local_dir().join("design"))
    }

    pub fn sessions(&self, scope: &Scope) -> Vec<&Session> {
        let mut sessions: Vec<_> = self
            .data
            .sessions
            .iter()
            .filter(|s| &s.scope == scope)
            .collect();
        sessions.sort_by_key(|session| std::cmp::Reverse((session.updated_at_ms, session.id)));
        sessions
    }

    pub fn session(&self, scope: &Scope, session_id: u64) -> Result<&Session> {
        self.data
            .sessions
            .iter()
            .find(|s| s.id == session_id && &s.scope == scope)
            .context("当前项目和引擎下找不到该设计会话")
    }

    pub fn create_session(
        &mut self,
        scope: &Scope,
        kind: ArtifactKind,
        title: &str,
    ) -> Result<u64> {
        self.create_session_with_preferences(scope, kind, title, None)
    }

    pub fn create_session_with_preferences(
        &mut self,
        scope: &Scope,
        kind: ArtifactKind,
        title: &str,
        preferences: Option<ComposerPreferences>,
    ) -> Result<u64> {
        scope.validate()?;
        text(title, 512, "会话标题")?;
        if let Some(preferences) = &preferences {
            preferences.validate().map_err(anyhow::Error::msg)?;
        }
        ensure!(
            self.data.sessions.len() < MAX_SESSIONS,
            "设计会话已达 64 个，请先删除不需要的会话"
        );
        self.transact(|data| {
            let id = allocate(data)?;
            let now = now_ms();
            data.sessions.push(Session {
                id,
                scope: scope.clone(),
                title: title.into(),
                kind,
                created_at_ms: now,
                updated_at_ms: now,
                composer_preferences: preferences,
                messages: Vec::new(),
                revisions: Vec::new(),
            });
            Ok(id)
        })
    }

    pub fn set_composer_preferences(
        &mut self,
        scope: &Scope,
        id: u64,
        preferences: ComposerPreferences,
    ) -> Result<()> {
        preferences.validate().map_err(anyhow::Error::msg)?;
        if self.session(scope, id)?.composer_preferences.as_ref() == Some(&preferences) {
            return Ok(());
        }
        self.transact(|data| {
            session_mut(data, scope, id)?.composer_preferences = Some(preferences);
            Ok(())
        })
    }

    pub fn set_session_title(&mut self, scope: &Scope, id: u64, title: &str) -> Result<()> {
        text(title, 512, "会话标题")?;
        self.transact(|data| {
            session_mut(data, scope, id)?.title = title.into();
            Ok(())
        })
    }

    pub fn append_message(
        &mut self,
        scope: &Scope,
        session_id: u64,
        role: ChatRole,
        message: &str,
    ) -> Result<u64> {
        self.append_chat(scope, session_id, role, message, &[])
    }

    pub fn append_response(
        &mut self,
        scope: &Scope,
        session_id: u64,
        message: &str,
        steps: &[ChatStep],
    ) -> Result<u64> {
        self.append_chat(scope, session_id, ChatRole::Assistant, message, steps)
    }

    fn append_chat(
        &mut self,
        scope: &Scope,
        session_id: u64,
        role: ChatRole,
        message: &str,
        steps: &[ChatStep],
    ) -> Result<u64> {
        validate_chat(role, message, steps)?;
        ensure!(
            self.session(scope, session_id)?.messages.len() < MAX_MESSAGES,
            "单会话对话已达 256 条，请新建设计会话"
        );
        self.transact(|data| {
            let id = allocate(data)?;
            let session = session_mut(data, scope, session_id)?;
            let now = now_ms();
            session.messages.push(ChatRecord {
                id,
                role,
                text: message.into(),
                created_at_ms: now,
                steps: steps.to_vec(),
            });
            session.updated_at_ms = now;
            Ok(id)
        })
    }

    pub fn append_revision(
        &mut self,
        scope: &Scope,
        session_id: u64,
        title: &str,
        content: ArtifactContent,
        origin: RevisionOrigin,
    ) -> Result<u64> {
        text(title, 512, "版本标题")?;
        content.validate()?;
        let session = self.session(scope, session_id)?;
        ensure!(session.kind == content.kind(), "版本类型与会话引擎不匹配");
        ensure!(
            session.revisions.len() < MAX_REVISIONS,
            "单会话版本已达 64 个，请新建设计会话"
        );
        self.transact(|data| {
            let id = allocate(data)?;
            let session = session_mut(data, scope, session_id)?;
            let now = now_ms();
            session.revisions.push(Revision {
                id,
                title: title.into(),
                created_at_ms: now,
                origin,
                content,
            });
            session.updated_at_ms = now;
            Ok(id)
        })
    }

    pub fn revision(&self, scope: &Scope, session_id: u64, revision_id: u64) -> Result<&Revision> {
        self.session(scope, session_id)?
            .revisions
            .iter()
            .find(|r| r.id == revision_id)
            .context("当前会话下找不到该设计版本")
    }

    /// Returns an independent validated snapshot, never writes to a project file.
    /// Native-session compatibility must be checked by the UI before `remember`.
    pub fn restore(
        &self,
        scope: &Scope,
        session_id: u64,
        revision_id: u64,
    ) -> Result<ArtifactContent> {
        let content = &self.revision(scope, session_id, revision_id)?.content;
        content.validate()?;
        Ok(content.clone())
    }

    pub fn delete_session(&mut self, scope: &Scope, session_id: u64) -> Result<()> {
        self.session(scope, session_id)?;
        self.transact(|data| {
            data.sessions
                .retain(|s| !(s.id == session_id && &s.scope == scope));
            Ok(())
        })
    }

    fn transact<T>(&mut self, edit: impl FnOnce(&mut Data) -> Result<T>) -> Result<T> {
        let mut next = self.data.clone();
        let result = edit(&mut next)?;
        let bytes = bounded_json(&next, MAX_STORE_BYTES)?;
        // The JSON parser's bounded nesting limit also applies on next launch.
        // Reject an otherwise valid but too-deep native snapshot before commit,
        // instead of creating a store that cannot subsequently be reopened.
        serde_json::from_slice::<Data>(&bytes).context("设计记录嵌套过深，无法安全持久化")?;
        let staged = self.directory.join("store.pending");
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&staged)
            .context("无法暂存设计记录")?;
        file.write_all(&bytes).context("设计记录写入失败")?;
        file.sync_all().context("设计记录同步失败")?;
        drop(file);
        replace(&staged, &self.directory.join("store.json"))?;
        self.data = next;
        Ok(result)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn allocate(data: &mut Data) -> Result<u64> {
    let id = data.next_id;
    data.next_id = id.checked_add(1).context("设计记录 ID 已用尽")?;
    Ok(id)
}

fn session_mut<'a>(data: &'a mut Data, scope: &Scope, session_id: u64) -> Result<&'a mut Session> {
    data.sessions
        .iter_mut()
        .find(|s| s.id == session_id && &s.scope == scope)
        .context("当前项目和引擎下找不到该设计会话")
}

fn validate_data(data: &Data) -> Result<()> {
    ensure!(data.format == 1, "不支持的设计记录版本：{}", data.format);
    ensure!(data.sessions.len() <= MAX_SESSIONS, "设计会话数量过多");
    let mut ids = HashSet::new();
    let mut check_id = |id| {
        ensure!(
            id > 0 && id < data.next_id && ids.insert(id),
            "设计记录 ID 无效或重复"
        );
        Ok(())
    };
    for session in &data.sessions {
        check_id(session.id)?;
        session.scope.validate()?;
        text(&session.title, 512, "会话标题")?;
        if let Some(preferences) = &session.composer_preferences {
            preferences.validate().map_err(anyhow::Error::msg)?;
        }
        ensure!(
            session.messages.len() <= MAX_MESSAGES && session.revisions.len() <= MAX_REVISIONS,
            "会话内容数量过多"
        );
        for message in &session.messages {
            check_id(message.id)?;
            validate_chat(message.role, &message.text, &message.steps)?;
        }
        for revision in &session.revisions {
            check_id(revision.id)?;
            text(&revision.title, 512, "版本标题")?;
            ensure!(
                revision.content.kind() == session.kind,
                "版本类型与会话类型不一致"
            );
            revision.content.validate()?;
        }
    }
    ensure!(data.next_id > 0, "设计记录序号无效");
    Ok(())
}

fn validate_chat(role: ChatRole, message: &str, steps: &[ChatStep]) -> Result<()> {
    if role == ChatRole::Assistant && !steps.is_empty() {
        ensure!(
            message.len() <= 4 * 1024 * 1024 && !message.contains('\0'),
            "助手回复内容无效"
        );
    } else {
        let limit = if role == ChatRole::Assistant {
            4 * 1024 * 1024
        } else {
            128 * 1024
        };
        text(message, limit, "对话内容")?;
    }
    let mut after = 0;
    for step in steps {
        ensure!(
            step.after_text >= after && message.is_char_boundary(step.after_text),
            "对话步骤位置无效"
        );
        after = step.after_text;
    }
    Ok(())
}

/// Bound the allocation during serialization rather than after building a huge JSON buffer.
pub(super) fn bounded_json(value: &impl Serialize, max: usize) -> Result<Vec<u8>> {
    struct Limited {
        bytes: Vec<u8>,
        max: usize,
    }
    impl Write for Limited {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.max.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other("设计存储容量不足；原记录未修改"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Limited {
        bytes: Vec::new(),
        max,
    };
    serde_json::to_writer(&mut output, value).context("设计数据超过容量限制或无法序列化")?;
    Ok(output.bytes)
}

#[cfg(not(target_os = "windows"))]
fn replace(source: &Path, destination: &Path) -> Result<()> {
    fs::rename(source, destination).context("无法提交设计记录")
}

#[cfg(target_os = "windows")]
fn replace(source: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    // std::fs::rename cannot replace on Windows. MoveFileExW performs a same-
    // volume replace without a remove-then-rename gap and preserves the old file
    // on failure. No additional dependency/features are required for this FFI.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both paths are owned, NUL-terminated UTF-16 and live for the call.
    let success = unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 1 | 8) };
    if success == 0 {
        anyhow::bail!("无法提交设计记录：{}", std::io::Error::last_os_error());
    }
    Ok(())
}
