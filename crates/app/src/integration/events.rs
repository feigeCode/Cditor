use super::{EditorSaveReason, EditorSaveState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEvent {
    Ready {
        document_id: String,
    },
    Changed {
        document_id: String,
        document_version: u64,
    },
    SaveStateChanged {
        state: EditorSaveState,
    },
    Saved {
        document_id: String,
        document_version: u64,
        reason: EditorSaveReason,
    },
    SaveFailed {
        document_id: String,
        document_version: u64,
        message: String,
    },
    LoadFailed {
        document_id: String,
        message: String,
    },
}
