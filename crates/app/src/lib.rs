pub mod api;
pub mod gui;
pub mod integration;

pub use api::{
    AiCancellationToken, AiModelDescriptor, AiProvider, AiProviderError, AiRequest, AiStreamEvent,
    AiStreamSender, AiTaskKind, Cditor, CditorBackend, CditorBuilder, CditorCommand,
    CditorCommandAction, CditorComponent, CditorError, CditorEvent, CditorHandle, CditorKeyBinding,
    CditorOptions, CommandDescriptor, CommandOutcome, CommandState, DocumentRenderArtifact,
    DocumentRenderError, DocumentRenderFuture, DocumentRenderRequest, DocumentRenderTheme,
    DocumentRendererProvider, SyntaxHighlightError, SyntaxHighlightLanguage,
    SyntaxHighlightPalette, SyntaxHighlightProvider, SyntaxHighlightRun, SyntaxHighlightStyle,
    ThemeProvider, WorkspaceId,
};
pub use api::{EditorBlockExportPresentation, EditorBlockExportState, EditorViewExportState};
#[cfg(feature = "sqlite")]
pub use api::{SqliteDurability, SqliteStorageOptions};
pub use cditor_ai as ai;
pub use cditor_core as core;
pub use cditor_runtime as runtime;
#[cfg(feature = "postgres")]
pub use cditor_storage_postgres as storage_postgres;
#[cfg(feature = "sqlite")]
pub use cditor_storage_sqlite as storage_sqlite;
pub use integration::{
    DocumentReplaceReason, Editor, EditorBlock, EditorBuilder, EditorDocument, EditorError,
    EditorEvent, EditorHandle, EditorPersistence, EditorPersistenceError, EditorSaveReason,
    EditorSaveRequest, EditorSaveState, MarkdownApplyMode, MarkdownAsset, MarkdownAssetError,
    MarkdownAssetResolver, MarkdownAssetRole, MarkdownBundleExportResult, MarkdownBundleOptions,
    MarkdownCompatibility, MarkdownDiagnostic, MarkdownDiagnosticSeverity, MarkdownExportMode,
    MarkdownExportResult, MarkdownFidelity, MarkdownImportResult,
};

/// Installs Cditor's application-level key bindings for embedded editors.
///
/// Hosts must call this once during GPUI application initialization before
/// creating an [`Editor`].
pub fn init(cx: &mut gpui::App) {
    gui::input::bind_cditor_keys(cx);
}

/// Initializes embedded editors when the host owns all configurable command
/// shortcuts.
///
/// This installs text input, navigation, deletion, Enter/Tab and clipboard
/// behavior, but does not install Undo/Redo, Select All, formatting or block
/// command shortcuts. Register those afterwards with [`bind_command_keys`].
pub fn init_for_external_keymap(cx: &mut gpui::App) {
    gui::input::bind_cditor_core_keys(cx);
}

/// Adds host-defined key bindings for Cditor's stable command ids.
///
/// Call this after [`init`] so bindings loaded from the host settings layer can
/// override Cditor's defaults. Navigation, text input and clipboard bindings
/// remain owned by the editor; only Markdown/editing commands need entries
/// here.
pub fn bind_command_keys(
    cx: &mut gpui::App,
    bindings: impl IntoIterator<Item = CditorKeyBinding>,
) -> Result<(), CditorError> {
    gui::input::bind_cditor_command_keys(cx, bindings)
}

pub mod storage {
    pub use cditor_storage::*;
    #[cfg(feature = "postgres")]
    pub use cditor_storage_postgres as postgres;
    #[cfg(feature = "sqlite")]
    pub use cditor_storage_sqlite as sqlite;
}
