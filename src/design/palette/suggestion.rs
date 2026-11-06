use super::{ColorRole, HexColor, MAX_SWATCHES, Palette, Swatch};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ColorEdit {
    Add {
        role: ColorRole,
        hex: HexColor,
    },
    Update {
        index: usize,
        role: Option<ColorRole>,
        hex: Option<HexColor>,
    },
    Remove {
        index: usize,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaletteProposal {
    pub edits: Vec<ColorEdit>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub rationale: String,
}

impl PaletteProposal {
    pub fn parse(reply: &str) -> Result<Self, String> {
        let reply = reply.trim();
        let reply = reply
            .strip_prefix("```json")
            .or_else(|| reply.strip_prefix("```"))
            .and_then(|body| body.trim_end().strip_suffix("```"))
            .unwrap_or(reply);
        if reply.len() > 32 * 1024 {
            return Err("配色建议过大，请缩短描述后重试".into());
        }
        serde_json::from_str(reply).map_err(|_| "AI 未返回有效的配色建议，可以重试".into())
    }
}

pub struct ReviewedEdits {
    pub draft: Palette,
    pub changes: Vec<String>,
    pub conflicts: Vec<String>,
    pub unlocked_draft: Palette,
}

/// All indices refer to the same initial draft. A locked row is never changed
/// in the default result, even if an agent explicitly asks to replace it.
pub fn review_edits(base: &Palette, edits: &[ColorEdit]) -> Result<ReviewedEdits, String> {
    if edits.len() > 2 * MAX_SWATCHES {
        return Err("配色建议包含过多改动".into());
    }
    let mut safe = base.0.iter().copied().map(Some).collect::<Vec<_>>();
    let mut unlocked = safe.clone();
    let mut changes = Vec::new();
    let mut conflicts = Vec::new();
    let mut visited = HashSet::new();
    for edit in edits {
        let (index, replacement) = match *edit {
            ColorEdit::Add { role, hex } => {
                let swatch = Swatch::new(hex, role);
                changes.push(format!("新增 {}", swatch.label()));
                safe.push(Some(swatch));
                unlocked.push(Some(swatch));
                continue;
            }
            ColorEdit::Update { index, role, hex } => {
                let old = original(base, index)?;
                (
                    index,
                    Some(Swatch {
                        role: role.unwrap_or(old.role),
                        hex: hex.unwrap_or(old.hex),
                        ..old
                    }),
                )
            }
            ColorEdit::Remove { index } => (index, None),
        };
        let old = original(base, index)?;
        if !visited.insert(index) {
            return Err(format!("配色建议多次编辑了色块 {index}"));
        }
        if replacement == Some(old) {
            continue;
        }
        let description = match replacement {
            Some(next) => format!(
                "{} → {}",
                old.label(),
                Swatch {
                    locked: false,
                    ..next
                }
                .label()
            ),
            None => format!("移除 {}", old.label()),
        };
        unlocked[index - 1] = replacement.map(|swatch| Swatch {
            locked: false,
            ..swatch
        });
        if old.locked {
            conflicts.push(description);
        } else {
            safe[index - 1] = replacement;
            changes.push(description);
        }
    }
    let mut draft = Palette(safe.into_iter().flatten().collect());
    let unlocked_draft = Palette(unlocked.into_iter().flatten().collect());
    unlocked_draft.validate()?;
    if draft.validate().is_err() {
        // Keeping a locked row may fill the only slot needed by its replacement.
        // Keep the original draft intact until the user has resolved the conflict.
        draft = base.clone();
        changes.clear();
    }
    Ok(ReviewedEdits {
        draft,
        changes,
        conflicts,
        unlocked_draft,
    })
}

fn original(base: &Palette, index: usize) -> Result<Swatch, String> {
    base.0
        .get(index.wrapping_sub(1))
        .copied()
        .ok_or_else(|| format!("草案中没有色块 {index}"))
}
