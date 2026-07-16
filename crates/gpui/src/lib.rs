//! Lightweight third-party GPUI editor component.
//!
//! The default dependency graph intentionally excludes Cditor's PostgreSQL
//! backend. Applications provide persistence through [`EditorPersistence`] or
//! manual [`EditorDocument`] import/export.

pub use cditor_app::{
    DocumentReplaceReason, Editor, EditorBlock, EditorBuilder, EditorDocument, EditorError,
    EditorEvent, EditorHandle, EditorPersistence, EditorPersistenceError, EditorSaveReason,
    EditorSaveRequest, EditorSaveState, MarkdownApplyMode, MarkdownAsset, MarkdownAssetError,
    MarkdownAssetResolver, MarkdownAssetRole, MarkdownBundleExportResult, MarkdownBundleOptions,
    MarkdownCompatibility, MarkdownDiagnostic, MarkdownDiagnosticSeverity, MarkdownExportMode,
    MarkdownExportResult, MarkdownFidelity, MarkdownImportResult,
};

/// Advanced implementation types. Prefer [`Editor`] and [`EditorHandle`] for
/// normal embedding so integrations remain insulated from view internals.
pub mod advanced {
    pub use cditor_app::gui::{CditorV2View, GuiTheme};
}

#[cfg(test)]
mod tests {
    use super::{Editor, EditorDocument, EditorSaveState, MarkdownApplyMode, MarkdownExportMode};

    #[test]
    fn stable_component_api_is_available_without_postgres() {
        let document = EditorDocument::from_markdown("third-party-doc", "# Embedded").unwrap();
        let _builder = Editor::builder().initial_document(document);
        let _state = EditorSaveState::Disabled;
        let _export_mode = MarkdownExportMode::Strict;
        let _apply_mode = MarkdownApplyMode::Editable;
    }
}
