use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

const MAX_DIRECTORY_ENTRIES: usize = 4096;
const PAGE_SIZE: usize = 64;

/// A user-supplied root. Tool paths are relative and first reads are snapshotted.
#[derive(Clone)]
pub(super) struct ReferenceFolder {
    root: Arc<Path>,
    snapshots: Arc<Mutex<HashMap<String, ReferenceFile>>>,
}

impl ReferenceFolder {
    pub(super) fn open(path: &Path) -> Result<Self, String> {
        let root = path.canonicalize().map_err(|_| "无法打开参考目录")?;
        if !root.is_dir() || root.file_name().is_none() {
            return Err("请选择具体的项目目录".into());
        }
        Ok(Self {
            root: root.into(),
            snapshots: Default::default(),
        })
    }

    pub(super) fn read(&self, relative: &str, offset: usize) -> Result<Value, String> {
        let mut snapshots = self
            .snapshots
            .lock()
            .map_err(|_| "Reference cache unavailable")?;
        if let Some(file) = snapshots.get(relative) {
            return file.chunk(offset);
        }
        let mut target = self.root.to_path_buf();
        let mut metadata = target
            .symlink_metadata()
            .map_err(|_| "Reference folder unavailable")?;
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err("Reference folder unavailable".into());
        }
        if !relative.is_empty() {
            for part in relative.split('/') {
                if !visible_name(part) {
                    return Err("Use a visible relative path inside the reference folder".into());
                }
                target.push(part);
                metadata = target
                    .symlink_metadata()
                    .map_err(|_| "Reference path unavailable")?;
                if metadata.is_symlink() {
                    return Err("Links are unavailable inside reference folders".into());
                }
            }
        }
        let target = target
            .canonicalize()
            .map_err(|_| "Reference path unavailable")?;
        let resolved = target
            .strip_prefix(self.root.as_ref())
            .map_err(|_| "Reference path is outside the attached folder")?;
        if !resolved
            .components()
            .all(|part| part.as_os_str().to_str().is_some_and(visible_name))
        {
            return Err("Choose a visible path inside the reference folder".into());
        }
        if metadata.is_dir() {
            return list(&target, relative, offset);
        }
        if metadata.is_file() && is_image(&target) {
            return image_result(&target, offset);
        }
        if !metadata.is_file() {
            return Err(
                "Choose a visible source or documentation file from the directory listing".into(),
            );
        }
        let file = ReferenceFile::open(&target)?;
        let bytes = snapshots
            .values()
            .map(ReferenceFile::byte_len)
            .sum::<usize>();
        if snapshots.len() >= PAGE_SIZE || bytes + file.byte_len() > MAX_REFERENCE_BYTES {
            return Err(
                "Folder read limit reached. Attach any additional files explicitly.".into(),
            );
        }
        let result = file.chunk(offset)?;
        snapshots.insert(relative.to_owned(), file);
        Ok(result)
    }
}

fn list(target: &Path, relative: &str, offset: usize) -> Result<Value, String> {
    let mut entries = Vec::new();
    let mut limited = false;
    for (index, entry) in fs::read_dir(target)
        .map_err(|_| "Cannot list reference directory")?
        .enumerate()
    {
        if index == MAX_DIRECTORY_ENTRIES {
            limited = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|name| visible_name(name)) else {
            continue;
        };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() || kind.is_file() {
            entries.push((name.to_owned(), kind.is_dir()));
        }
    }
    entries.sort_unstable();
    let total = entries.len();
    let entries = entries
        .iter()
        .skip(offset)
        .take(PAGE_SIZE)
        .map(|(name, directory)| json!({"name":name,"directory":directory}))
        .collect::<Vec<_>>();
    let next = offset.saturating_add(entries.len());
    Ok(json!({
        "path":relative,"entries":entries,"listing_limited":limited,
        "next_offset":(next < total).then_some(next)
    }))
}

fn visible_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.ends_with(['.', ' '])
        && !name.contains(['\\', ':'])
        && !name.chars().any(char::is_control)
}
