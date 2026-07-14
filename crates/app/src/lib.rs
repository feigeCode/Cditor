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

pub mod storage {
    pub use cditor_storage::*;
    #[cfg(feature = "postgres")]
    pub use cditor_storage_postgres as postgres;
}
