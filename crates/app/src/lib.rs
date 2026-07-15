pub mod api;
pub mod gui;
pub mod integration;

pub use api::{
    Cditor, CditorBackend, CditorBuilder, CditorCommand, CditorComponent, CditorError, CditorEvent,
    CditorHandle, CditorOptions, SqliteDurability, SqliteStorageOptions, WorkspaceId,
};
pub use cditor_core as core;
pub use cditor_runtime as runtime;
#[cfg(feature = "postgres")]
pub use cditor_storage_postgres as storage_postgres;
pub use cditor_storage_sqlite as storage_sqlite;
pub use integration::{
    DocumentReplaceReason, Editor, EditorBlock, EditorBuilder, EditorDocument, EditorError,
    EditorEvent, EditorHandle, EditorPersistence, EditorPersistenceError, EditorSaveReason,
    EditorSaveRequest, EditorSaveState, MarkdownApplyMode, MarkdownCompatibility,
    MarkdownDiagnostic, MarkdownDiagnosticSeverity, MarkdownExportMode, MarkdownExportResult,
    MarkdownFidelity, MarkdownImportResult,
};

pub mod storage {
    pub use cditor_storage::*;
    #[cfg(feature = "postgres")]
    pub use cditor_storage_postgres as postgres;
    pub use cditor_storage_sqlite as sqlite;
}
