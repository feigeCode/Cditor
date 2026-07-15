#[cfg(feature = "postgres")]
pub mod api;
pub mod gui;
pub mod integration;

#[cfg(feature = "postgres")]
pub use api::{Cditor, CditorBackend, CditorOptions, WorkspaceId};
pub use cditor_core as core;
pub use cditor_runtime as runtime;
#[cfg(feature = "postgres")]
pub use cditor_storage_postgres as storage_postgres;
pub use integration::{
    Editor, EditorBlock, EditorBuilder, EditorDocument, EditorError, EditorEvent, EditorHandle,
    EditorPersistence, EditorPersistenceError, EditorSaveReason, EditorSaveRequest,
    EditorSaveState,
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
}
