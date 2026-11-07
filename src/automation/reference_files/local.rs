use super::folder::ReferenceFolder;
use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

const MAX_PATH_BYTES: usize = 4096;

#[derive(Clone)]
enum Reference {
    File(ReferenceFile),
    Folder(ReferenceFolder),
    Image(std::path::PathBuf),
}

/// Authorizes only paths the model requests that were supplied in this turn's user message.
#[derive(Clone, Default)]
pub(in crate::automation) struct LocalReferences {
    prompt: Arc<str>,
    opened: Arc<Mutex<HashMap<String, Reference>>>,
}

impl LocalReferences {
    pub(in crate::automation) fn new(prompt: &str) -> Self {
        Self {
            prompt: prompt.into(),
            ..Self::default()
        }
    }

    pub(super) fn read(
        &self,
        local_path: &str,
        relative: &str,
        offset: usize,
    ) -> Result<Value, String> {
        let path = Path::new(local_path);
        if !path.is_absolute()
            || local_path.len() > MAX_PATH_BYTES
            || relative.len() > MAX_PATH_BYTES
            || local_path.chars().any(char::is_control)
            || !mentions_path(&self.prompt, local_path)
        {
            return Err("Use the complete absolute local path supplied in the current user message. Quote paths with spaces.".into());
        }

        let reference = {
            let mut opened = self
                .opened
                .lock()
                .map_err(|_| "Reference cache unavailable")?;
            if let Some(reference) = opened.get(local_path) {
                reference.clone()
            } else {
                if opened.len() >= MAX_REFERENCE_FILES {
                    return Err("Local path limit reached for this message".into());
                }
                let metadata = path
                    .metadata()
                    .map_err(|_| "User-supplied local path is unavailable")?;
                let reference = if metadata.is_dir() {
                    Reference::Folder(ReferenceFolder::open(path)?)
                } else if is_image(path) {
                    Reference::Image(path.to_path_buf())
                } else {
                    Reference::File(ReferenceFile::open(path)?)
                };
                opened.insert(local_path.to_owned(), reference.clone());
                reference
            }
        };
        match reference {
            Reference::Folder(folder) => folder.read(relative, offset),
            Reference::File(file) if relative.is_empty() => file.chunk(offset),
            Reference::Image(path) if relative.is_empty() => image_result(&path, offset),
            Reference::Image(_) | Reference::File(_) => {
                Err("Relative paths require a user-supplied directory".into())
            }
        }
    }
}

fn mentions_path(prompt: &str, path: &str) -> bool {
    fn boundary(c: char) -> bool {
        c.is_whitespace() || "([{<>,;!?)]}，。；：！？、（）【】《》".contains(c)
    }
    fn unquoted_contains(text: &str, path: &str) -> bool {
        text.match_indices(path).any(|(index, _)| {
            text[..index].chars().next_back().is_none_or(boundary)
                && text[index + path.len()..]
                    .chars()
                    .next()
                    .is_none_or(boundary)
        })
    }

    for mut line in prompt.lines() {
        if line.trim() == path {
            return true;
        }
        while let Some((index, quote)) = line.char_indices().find(|(_, c)| "`\"'“‘".contains(*c))
        {
            let before = &line[..index];
            let after = &line[index + quote.len_utf8()..];
            if unquoted_contains(before, path) {
                return true;
            }
            let close = match quote {
                '“' => '”',
                '‘' => '’',
                _ => quote,
            };
            let Some((quoted, remainder)) = after.split_once(close) else {
                line = after;
                continue;
            };
            if quoted == path {
                return true;
            }
            line = remainder;
        }
        if unquoted_contains(line, path) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_path_from_the_user_message_without_granting_ancestors_or_url_paths() {
        assert!(mentions_path(
            "参考 /workspace/backend。",
            "/workspace/backend"
        ));
        assert!(mentions_path(
            "参考`/workspace/admin panel`里的前端",
            "/workspace/admin panel"
        ));
        assert!(!mentions_path(
            "参考`/workspace/admin panel`里的前端",
            "/workspace/admin"
        ));
        assert!(!mentions_path(
            "参考 /workspace/backend/admin.js",
            "/workspace/backend"
        ));
        assert!(!mentions_path(
            "打开 https://example.test/workspace/backend",
            "/workspace/backend"
        ));
    }
}
