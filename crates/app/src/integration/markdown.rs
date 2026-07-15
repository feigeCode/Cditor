use super::EditorDocument;

pub use cditor_core::rich_text::{
    MarkdownCompatibility, MarkdownDiagnostic, MarkdownDiagnosticSeverity, MarkdownExportMode,
    MarkdownExportResult, MarkdownFidelity,
};

#[derive(Debug, Clone, PartialEq)]
pub struct MarkdownImportResult {
    pub document: EditorDocument,
    pub compatibility: MarkdownCompatibility,
    pub diagnostics: Vec<MarkdownDiagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownApplyMode {
    Editable,
    ReadOnlyPreview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentReplaceReason {
    ExternalReload,
    SourceModeCommit,
    Programmatic,
}
