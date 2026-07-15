pub mod api;
pub mod gui;
pub mod integration;

pub use api::{
    Cditor, CditorBackend, CditorBuilder, CditorCommand, CditorComponent, CditorError, CditorEvent,
    CditorHandle, CditorOptions, WorkspaceId,
};
#[cfg(feature = "sqlite")]
pub use api::{SqliteDurability, SqliteStorageOptions};
pub use cditor_core as core;
pub use cditor_runtime as runtime;
#[cfg(feature = "postgres")]
pub use cditor_storage_postgres as storage_postgres;
#[cfg(feature = "sqlite")]
pub use cditor_storage_sqlite as storage_sqlite;
pub use integration::{
    DocumentReplaceReason, Editor, EditorBlock, EditorBuilder, EditorDocument, EditorError,
    EditorEvent, EditorHandle, EditorPersistence, EditorPersistenceError, EditorSaveReason,
    EditorSaveRequest, EditorSaveState, MarkdownApplyMode, MarkdownCompatibility,
    MarkdownDiagnostic, MarkdownDiagnosticSeverity, MarkdownExportMode, MarkdownExportResult,
    MarkdownFidelity, MarkdownImportResult,
};

/// Installs Cditor's application-level key bindings for embedded editors.
///
/// Hosts must call this once during GPUI application initialization before
/// creating an [`Editor`].
pub fn init(cx: &mut gpui::App) {
    gui::input::bind_cditor_keys(cx);
}

pub mod storage {
    pub use cditor_storage::*;
    #[cfg(feature = "postgres")]
    pub use cditor_storage_postgres as postgres;
    #[cfg(feature = "sqlite")]
    pub use cditor_storage_sqlite as sqlite;
}
