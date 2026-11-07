use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

mod folder;
mod local;
pub(super) use local::LocalReferences;

pub(super) const READ_TOOL: &str = "rpshop_read_reference_file";
pub const MAX_REFERENCE_FILES: usize = 8;
pub const MAX_REFERENCE_BYTES: usize = 2 * 1024 * 1024;
const MAX_FILE_BYTES: usize = 512 * 1024;
const CHUNK_BYTES: usize = 16 * 1024;

/// In-memory snapshot of a user-selected text file. The agent receives no path.
#[derive(Clone)]
pub struct ReferenceFile {
    pub name: String,
    text: Arc<str>,
}

impl ReferenceFile {
    pub fn open(path: &Path) -> Result<Self, String> {
        let name = path
            .file_name()
            .ok_or("无效的参考文件名")?
            .to_string_lossy()
            .chars()
            .filter(|c| !c.is_control())
            .take(160)
            .collect::<String>();
        let unreadable = || format!("无法读取参考文件「{name}」");
        if !fs::metadata(path).map_err(|_| unreadable())?.is_file() {
            return Err(format!("「{name}」不是普通文件"));
        }
        let file = File::open(path).map_err(|_| unreadable())?;
        if !file.metadata().map_err(|_| unreadable())?.is_file() {
            return Err(format!("「{name}」不是普通文件"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| unreadable())?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(format!("「{name}」超过单文件 512 KiB 上限"));
        }
        let text = String::from_utf8(bytes)
            .ok()
            .filter(|text| {
                !text
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
            })
            .ok_or_else(|| format!("「{name}」需要是 UTF-8 文本文件"))?;
        Ok(Self {
            name,
            text: text.into(),
        })
    }

    pub fn byte_len(&self) -> usize {
        self.text.len()
    }
}

fn image_result(path: &Path, offset: usize) -> Result<Value, String> {
    if offset != 0 {
        return Err("Images do not support offsets".into());
    }
    let image = crate::ai::ImageAttachment::from_file(path).map_err(|e| e.to_string())?;
    Ok(
        json!({"name":image.name(), "_rpshop_image":image.data_url(), "width":image.width(), "height":image.height()}),
    )
}

fn is_image(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        matches!(
            s.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp"
        )
    })
}

pub(super) fn read(
    files: &[ReferenceFile],
    local: &LocalReferences,
    args: &Value,
) -> Result<Value, String> {
    super::fields(args, &["file_id", "local_path", "path", "offset"])?;
    let offset = match args.get("offset") {
        None => 0,
        Some(value) => value
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or("offset must be a nonnegative integer")?,
    };
    if let Some(local_path) = args.get("local_path") {
        if args.get("file_id").is_some() {
            return Err("Use either local_path or file_id".into());
        }
        let local_path = local_path.as_str().ok_or("local_path must be a string")?;
        let path = match args.get("path") {
            None => "",
            Some(value) => value.as_str().ok_or("path must be a relative string")?,
        };
        return local.read(local_path, path, offset);
    }
    if args.get("path").is_some() {
        return Err("A relative path requires local_path".into());
    }
    let Some(file_id) = args.get("file_id") else {
        if args.get("offset").is_some() {
            return Err("offset requires file_id or local_path".into());
        }
        let entries = files
            .iter()
            .enumerate()
            .map(|(i, file)| json!({"file_id":i + 1,"name":file.name,"bytes":file.byte_len()}))
            .collect::<Vec<_>>();
        return Ok(json!({"files":entries}));
    };
    let file = entry(files, file_id)
        .ok_or("Unknown file_id. List the files attached to this message first.")?;
    let mut result = file.chunk(offset)?;
    result["file_id"] = file_id.clone();
    Ok(result)
}

fn entry<'a, T>(entries: &'a [T], id: &Value) -> Option<&'a T> {
    id.as_u64()
        .and_then(|id| id.checked_sub(1))
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| entries.get(index))
}

impl ReferenceFile {
    fn chunk(&self, offset: usize) -> Result<Value, String> {
        if !self.text.is_char_boundary(offset) {
            return Err("Invalid offset. Use next_offset from the previous result.".into());
        }
        let mut end = (offset + CHUNK_BYTES).min(self.byte_len());
        while !self.text.is_char_boundary(end) {
            end -= 1;
        }
        Ok(json!({
            "name":self.name,
            "offset":offset,
            "next_offset":(end < self.byte_len()).then_some(end),
            "text":&self.text[offset..end],
        }))
    }
}
