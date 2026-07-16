use crate::api::AiModelDescriptor;

use super::{DocumentReplaceReason, EditorSaveReason, EditorSaveState};

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditorEvent {
    Ready {
        document_id: String,
    },
    Changed {
        document_id: String,
        document_version: u64,
    },
    DocumentReplaced {
        document_id: String,
        reason: DocumentReplaceReason,
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
    AiModelChanged {
        model: AiModelDescriptor,
    },
}
