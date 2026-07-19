mod builder;
mod document;
mod error;
mod events;
mod handle;
mod markdown;
mod markdown_bundle;
mod persistence;

pub use builder::{Editor, EditorBuilder};
pub use document::{EditorBlock, EditorDocument};
pub use error::EditorError;
pub use events::EditorEvent;
pub use handle::EditorHandle;
pub use markdown::{
    DocumentReplaceReason, MarkdownApplyMode, MarkdownAsset, MarkdownAssetError,
    MarkdownAssetResolver, MarkdownAssetRole, MarkdownBundleExportResult, MarkdownBundleOptions,
    MarkdownCompatibility, MarkdownDiagnostic, MarkdownDiagnosticSeverity, MarkdownExportMode,
    MarkdownExportResult, MarkdownFidelity, MarkdownImportResult,
};
pub(crate) use persistence::IntegrationPersistenceState;
pub use persistence::{
    EditorPersistence, EditorPersistenceError, EditorSaveReason, EditorSaveRequest, EditorSaveState,
};

#[cfg(test)]
mod tests {
    use super::{
        EditorDocument, EditorError, MarkdownCompatibility, MarkdownExportMode, MarkdownFidelity,
    };
    use cditor_runtime::DocumentRuntime;

    #[test]
    fn editor_document_json_round_trip_preserves_blocks() {
        let document = EditorDocument::from_markdown("doc-1", "# Title\n\nBody").unwrap();
        let json = document.to_json().unwrap();
        assert_eq!(EditorDocument::from_json(&json).unwrap(), document);
    }

    #[test]
    fn editor_document_rejects_future_schema() {
        let json =
            r#"{"schema_version":999,"document_id":"doc-1","structure_version":1,"blocks":[]}"#;
        assert!(matches!(
            EditorDocument::from_json(json),
            Err(EditorError::UnsupportedSchemaVersion { version: 999 })
        ));
    }

    #[test]
    fn runtime_snapshot_exports_markdown() {
        let runtime = DocumentRuntime::demo();
        let document = EditorDocument::from_runtime("doc-1", &runtime).unwrap();
        assert!(document.to_markdown().unwrap().contains("Cditor"));
    }

    #[test]
    fn markdown_report_and_strict_export_are_public_contracts() {
        let imported = EditorDocument::from_markdown_with_report(
            "doc-1",
            "2. first\n\n**bold** and [link](https://example.com)",
        )
        .unwrap();
        assert!(matches!(
            imported.compatibility,
            MarkdownCompatibility::Editable
        ));
        let exported = imported
            .document
            .export_markdown(MarkdownExportMode::Strict)
            .unwrap();
        assert!(matches!(
            exported.fidelity,
            MarkdownFidelity::Semantic | MarkdownFidelity::Normalized
        ));
        assert!(exported.markdown.contains("**bold**"));
        assert!(exported.markdown.contains("[link](<https://example.com>)"));
    }

    #[test]
    fn older_json_without_new_markdown_fields_still_loads() {
        let document = EditorDocument::from_markdown("doc-1", "Body").unwrap();
        let mut value = serde_json::to_value(&document).unwrap();
        for block in value["blocks"].as_array_mut().unwrap() {
            block.as_object_mut().unwrap().remove("attrs");
            block.as_object_mut().unwrap().remove("raw_fallback");
        }
        let json = serde_json::to_string(&value).unwrap();
        let restored = EditorDocument::from_json(&json).unwrap();
        assert_eq!(restored.to_markdown().unwrap(), "Body");
    }
}
