use std::fmt::{Display, Formatter};

use super::EditorDocument;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorPersistenceError {
    message: String,
}

impl EditorPersistenceError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for EditorPersistenceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EditorPersistenceError {}

pub trait EditorPersistence: Send + Sync + 'static {
    fn load(&self, document_id: &str) -> Result<Option<EditorDocument>, EditorPersistenceError>;

    fn save(&self, request: EditorSaveRequest) -> Result<(), EditorPersistenceError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSaveReason {
    Manual,
    Autosave,
    BeforeReload,
    BeforeClose,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorSaveRequest {
    pub document_id: String,
    pub document: EditorDocument,
    pub document_version: u64,
    pub reason: EditorSaveReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorSaveState {
    Disabled,
    Clean,
    Dirty,
    Saving,
    SaveFailed { message: String },
}

#[derive(Debug, Clone)]
pub(crate) struct IntegrationPersistenceState {
    enabled: bool,
    document_version: u64,
    persisted_version: u64,
    saving_version: Option<u64>,
    last_error: Option<String>,
    autosave_generation: u64,
    load_generation: u64,
}

impl IntegrationPersistenceState {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            document_version: 0,
            persisted_version: 0,
            saving_version: None,
            last_error: None,
            autosave_generation: 0,
            load_generation: 0,
        }
    }

    pub(crate) fn reset_baseline(&mut self) {
        self.document_version = self.document_version.saturating_add(1);
        self.persisted_version = self.document_version;
        self.saving_version = None;
        self.last_error = None;
        self.autosave_generation = self.autosave_generation.saturating_add(1);
    }

    pub(crate) fn mark_changed(&mut self) -> u64 {
        self.document_version = self.document_version.saturating_add(1);
        self.last_error = None;
        self.autosave_generation = self.autosave_generation.saturating_add(1);
        self.document_version
    }

    pub(crate) fn begin_save(&mut self) -> Option<u64> {
        if !self.enabled || self.persisted_version == self.document_version {
            return None;
        }
        let version = self.document_version;
        self.saving_version = Some(version);
        self.last_error = None;
        Some(version)
    }

    pub(crate) fn save_succeeded(&mut self, version: u64) {
        self.persisted_version = self.persisted_version.max(version);
        if self.saving_version == Some(version) {
            self.saving_version = None;
        }
        self.last_error = None;
    }

    pub(crate) fn save_failed(&mut self, version: u64, message: String) {
        if self.saving_version == Some(version) {
            self.saving_version = None;
        }
        self.last_error = Some(message);
    }

    pub(crate) fn public_state(&self) -> EditorSaveState {
        if !self.enabled {
            EditorSaveState::Disabled
        } else if let Some(message) = &self.last_error {
            EditorSaveState::SaveFailed {
                message: message.clone(),
            }
        } else if self.saving_version.is_some() {
            EditorSaveState::Saving
        } else if self.persisted_version == self.document_version {
            EditorSaveState::Clean
        } else {
            EditorSaveState::Dirty
        }
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.persisted_version != self.document_version || self.last_error.is_some()
    }

    pub(crate) fn document_version(&self) -> u64 {
        self.document_version
    }

    pub(crate) fn autosave_generation(&self) -> u64 {
        self.autosave_generation
    }

    pub(crate) fn next_load_generation(&mut self) -> u64 {
        self.load_generation = self.load_generation.saturating_add(1);
        self.load_generation
    }

    pub(crate) fn is_current_load_generation(&self, generation: u64) -> bool {
        self.load_generation == generation
    }
}

#[cfg(test)]
mod tests {
    use super::{EditorSaveState, IntegrationPersistenceState};

    #[test]
    fn older_save_success_does_not_clean_newer_edit() {
        let mut state = IntegrationPersistenceState::new(true);
        state.mark_changed();
        let saving = state.begin_save().unwrap();
        state.mark_changed();
        state.save_succeeded(saving);
        assert_eq!(state.public_state(), EditorSaveState::Dirty);
    }

    #[test]
    fn save_failure_preserves_dirty_version() {
        let mut state = IntegrationPersistenceState::new(true);
        state.mark_changed();
        let saving = state.begin_save().unwrap();
        state.save_failed(saving, "disk full".into());
        assert!(matches!(
            state.public_state(),
            EditorSaveState::SaveFailed { .. }
        ));
        assert!(state.is_dirty());
    }

    #[test]
    fn baseline_is_clean_and_invalidates_old_autosave() {
        let mut state = IntegrationPersistenceState::new(true);
        state.mark_changed();
        let generation = state.autosave_generation();
        state.reset_baseline();
        assert_eq!(state.public_state(), EditorSaveState::Clean);
        assert!(state.autosave_generation() > generation);
    }
}
