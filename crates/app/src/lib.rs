pub mod api;
pub mod gui;
pub mod integration;

pub use api::{Cditor, CditorBackend, CditorOptions, WorkspaceId};
pub use cditor_core as core;
pub use cditor_runtime as runtime;
pub use cditor_storage_postgres as storage_postgres;
pub use integration::{EditorBlock, EditorDocument, EditorError};

pub mod storage {
    pub use cditor_storage::*;
    pub use cditor_storage_postgres as postgres;
}
