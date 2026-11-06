use std::sync::OnceLock;

use super::ArtifactContent;

pub const BUILTIN_SAMPLE_LABEL: &str = "内置示例";

/// Original, offline editorial travel planner. Assets stay split for maintenance,
/// but this assembled document has no external fonts, requests or dependencies.
pub fn demo_html() -> &'static str {
    static DOCUMENT: OnceLock<String> = OnceLock::new();
    DOCUMENT.get_or_init(|| {
        include_str!("../../assets/ai/fieldnotes.html")
            .replace(
                "/*__FIELDNOTES_CSS__*/",
                include_str!("../../assets/ai/fieldnotes.css"),
            )
            .replace(
                "/*__FIELDNOTES_JS__*/",
                include_str!("../../assets/ai/fieldnotes.js"),
            )
    })
}

pub fn builtin_sample() -> ArtifactContent {
    ArtifactContent::Html {
        source: demo_html().to_owned(),
    }
}
