//! App-owned design history, separate from RP documents and editor undo state.
//!
//! A restore returns a validated snapshot. The UI decides whether it can apply a
//! native snapshot to its current native session, then uses its normal remember /
//! undo flow. This module never opens or replaces the user's project file.

mod html;
pub mod palette;
mod preview;
mod sample;
mod store;
mod types;

pub use html::{MAX_HTML_BYTES, validate_html};
pub use preview::{export_html, open_preview, preview_document, write_preview};
pub use sample::{BUILTIN_SAMPLE_LABEL, builtin_sample, demo_html};
pub use store::{MAX_STORE_BYTES, Store};
pub use types::{
    ArtifactContent, ArtifactKind, ChatRecord, ChatRole, Revision, RevisionOrigin, Scope, Session,
};

#[cfg(test)]
mod tests;
// ；
