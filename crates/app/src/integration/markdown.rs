use super::EditorDocument;
use std::fmt::{Display, Formatter};

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
pub enum MarkdownAssetRole {
    WhiteboardPreview,
    WhiteboardSource,
    WhiteboardEmbeddedImage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownAsset {
    pub relative_path: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
    pub block_id: u64,
    pub role: MarkdownAssetRole,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarkdownBundleExportResult {
    pub markdown: String,
    pub assets: Vec<MarkdownAsset>,
    pub fidelity: MarkdownFidelity,
    pub diagnostics: Vec<MarkdownDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownBundleOptions {
    pub asset_directory: String,
    pub preview_padding: u32,
}

impl Default for MarkdownBundleOptions {
    fn default() -> Self {
        Self {
            asset_directory: "document.assets".to_owned(),
            preview_padding: 32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownAssetError {
    pub message: String,
}

impl MarkdownAssetError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for MarkdownAssetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for MarkdownAssetError {}

pub trait MarkdownAssetResolver: Send + Sync {
    fn read_asset(&self, relative_path: &str) -> Result<Vec<u8>, MarkdownAssetError>;
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
