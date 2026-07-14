use std::time::Duration;

use gpui::Context;

use crate::gui::app::CditorV2View;
use crate::gui::persistence::EditorSaveStatus;

pub const DEFAULT_POSTGRES_SAVE_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub struct PostgresPersistenceTarget;

#[derive(Debug, Default)]
pub struct PostgresPersistenceState;

impl PostgresPersistenceState {
    pub fn disabled() -> Self {
        Self
    }

    pub fn for_target(_target: PostgresPersistenceTarget, _autosave_interval: Duration) -> Self {
        Self
    }

    pub fn is_enabled(&self) -> bool {
        false
    }

    pub fn set_target(
        &mut self,
        _target: Option<PostgresPersistenceTarget>,
        _autosave_interval: Duration,
    ) {
    }

    pub fn mark_loaded_structure_version(&mut self, _structure_version: u64) {}
}

pub fn mark_dirty_and_schedule_postgres_save(
    _persistence: &mut PostgresPersistenceState,
    save_status: &mut EditorSaveStatus,
    _cx: &mut Context<CditorV2View>,
) {
    *save_status = EditorSaveStatus::Dirty;
}
