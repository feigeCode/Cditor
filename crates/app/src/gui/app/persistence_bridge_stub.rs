use gpui::Context;

use crate::gui::app::cditor_v2_view::CditorV2View;
use crate::gui::persistence::{EditorSaveStatus, mark_dirty_and_schedule_postgres_save};

impl CditorV2View {
    pub(crate) fn mark_dirty(&mut self, cx: &mut Context<Self>) {
        mark_dirty_and_schedule_postgres_save(
            &mut self.postgres_persistence,
            &mut self.save_status,
            cx,
        );
    }
}

pub(in crate::gui::app) fn save_status_for_mode(readonly: bool) -> EditorSaveStatus {
    if readonly {
        EditorSaveStatus::Readonly
    } else {
        EditorSaveStatus::Clean
    }
}
