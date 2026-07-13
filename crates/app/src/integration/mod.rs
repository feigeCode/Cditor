mod document;
mod error;

pub use document::{EditorBlock, EditorDocument};
pub use error::EditorError;

#[cfg(test)]
mod tests {
    use super::{EditorDocument, EditorError};
    use cditor_runtime::DocumentRuntime;

    #[test]
    fn editor_document_json_round_trip_preserves_blocks() {
        let document = EditorDocument::from_markdown("doc-1", "# Title\n\nBody").unwrap();
        let json = document.to_json().unwrap();
        assert_eq!(EditorDocument::from_json(&json).unwrap(), document);
    }

    #[test]
    fn editor_document_rejects_future_schema() {
        let json = r#"{"schema_version":999,"document_id":"doc-1","structure_version":1,"blocks":[]}"#;
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
}
