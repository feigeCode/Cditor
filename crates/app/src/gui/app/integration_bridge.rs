use std::sync::Arc;
use std::time::Duration;

use gpui::Context;

use crate::gui::app::CditorV2View;
use crate::integration::{
    EditorDocument, EditorError, EditorEvent, EditorPersistence, EditorSaveState,
    IntegrationPersistenceState,
};

pub(crate) type EditorEventCallback = Arc<dyn Fn(EditorEvent) + Send + Sync>;

pub(crate) struct EditorIntegrationController {
    pub(crate) document_id: String,
    pub(crate) persistence: Option<Arc<dyn EditorPersistence>>,
    pub(crate) autosave: Option<Duration>,
    pub(crate) callback: Option<EditorEventCallback>,
    pub(crate) persistence_state: IntegrationPersistenceState,
    fingerprint: u64,
}

impl EditorIntegrationController {
    pub(crate) fn new(
        document_id: String,
        fingerprint: u64,
        persistence: Option<Arc<dyn EditorPersistence>>,
        autosave: Option<Duration>,
        callback: Option<EditorEventCallback>,
    ) -> Self {
        Self {
            document_id,
            persistence_state: IntegrationPersistenceState::new(persistence.is_some()),
            persistence,
            autosave,
            callback,
            fingerprint,
        }
    }

    pub(crate) fn sync_fingerprint(&mut self, fingerprint: u64) -> Option<EditorEvent> {
        if self.fingerprint == fingerprint {
            return None;
        }
        self.fingerprint = fingerprint;
        let document_version = self.persistence_state.mark_changed();
        Some(EditorEvent::Changed {
            document_id: self.document_id.clone(),
            document_version,
        })
    }

    pub(crate) fn reset_baseline(&mut self, fingerprint: u64) {
        self.fingerprint = fingerprint;
        self.persistence_state.reset_baseline();
    }

    pub(crate) fn save_state(&self) -> EditorSaveState {
        self.persistence_state.public_state()
    }

    #[cfg(test)]
    fn for_test(document_id: &str, fingerprint: u64) -> Self {
        Self {
            document_id: document_id.to_owned(),
            persistence: None,
            autosave: None,
            callback: None,
            persistence_state: IntegrationPersistenceState::new(true),
            fingerprint,
        }
    }
}

impl CditorV2View {
    pub(crate) fn install_editor_integration(
        &mut self,
        document_id: String,
        persistence: Option<Arc<dyn EditorPersistence>>,
        autosave: Option<Duration>,
        callback: Option<EditorEventCallback>,
    ) -> Result<(), EditorError> {
        let fingerprint = self
            .ready_runtime_ref()
            .ok_or(EditorError::NotReady)?
            .document_content_fingerprint();
        self.integration = Some(EditorIntegrationController::new(
            document_id,
            fingerprint,
            persistence,
            autosave,
            callback,
        ));
        Ok(())
    }

    pub(crate) fn sync_integration_document_change(&mut self, cx: &mut Context<Self>) {
        let Some(fingerprint) = self
            .ready_runtime_ref()
            .map(|runtime| runtime.document_content_fingerprint())
        else {
            return;
        };
        let (event, state_event, callback) = {
            let Some(integration) = self.integration.as_mut() else {
                return;
            };
            let event = integration.sync_fingerprint(fingerprint);
            let state_event = event.as_ref().map(|_| EditorEvent::SaveStateChanged {
                state: integration.save_state(),
            });
            (event, state_event, integration.callback.clone())
        };
        let changed = event.is_some();
        if let Some(callback) = callback {
            if let Some(event) = event {
                callback(event);
            }
            if let Some(event) = state_event {
                callback(event);
            }
        }
        if changed {
            cx.notify();
        }
    }

    pub(crate) fn refresh_integration_baseline(&mut self) {
        let Some(fingerprint) = self
            .ready_runtime_ref()
            .map(|runtime| runtime.document_content_fingerprint())
        else {
            return;
        };
        if let Some(integration) = self.integration.as_mut() {
            integration.reset_baseline(fingerprint);
        }
    }

    pub(crate) fn integration_document(&self) -> Result<EditorDocument, EditorError> {
        let integration = self.integration.as_ref().ok_or(EditorError::NotReady)?;
        let runtime = self.ready_runtime_ref().ok_or(EditorError::NotReady)?;
        EditorDocument::from_runtime(integration.document_id.clone(), runtime)
    }

    pub(crate) fn integration_document_id(&self) -> Option<&str> {
        self.integration
            .as_ref()
            .map(|integration| integration.document_id.as_str())
    }

    pub(crate) fn integration_save_state(&self) -> EditorSaveState {
        self.integration
            .as_ref()
            .map(EditorIntegrationController::save_state)
            .unwrap_or(EditorSaveState::Disabled)
    }

    pub(crate) fn integration_document_version(&self) -> u64 {
        self.integration
            .as_ref()
            .map(|integration| integration.persistence_state.document_version())
            .unwrap_or(0)
    }

    pub(crate) fn integration_is_dirty(&self) -> bool {
        self.integration
            .as_ref()
            .is_some_and(|integration| integration.persistence_state.is_dirty())
    }

    pub(crate) fn set_integration_readonly(&mut self, readonly: bool) {
        self.readonly = readonly;
    }

    pub(crate) fn request_integration_focus(&mut self) {
        self.integration_focus_requested = true;
    }
}

#[cfg(test)]
mod tests {
    use super::EditorIntegrationController;
    use crate::integration::{EditorEvent, EditorSaveState};

    #[test]
    fn fingerprint_change_marks_document_dirty_once() {
        let mut controller = EditorIntegrationController::for_test("doc-1", 10);
        assert!(matches!(
            controller.sync_fingerprint(11),
            Some(EditorEvent::Changed {
                document_version: 1,
                ..
            })
        ));
        assert_eq!(controller.save_state(), EditorSaveState::Dirty);
        assert!(controller.sync_fingerprint(11).is_none());
    }
}
