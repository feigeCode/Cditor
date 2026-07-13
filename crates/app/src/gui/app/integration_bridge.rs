use std::sync::Arc;
use std::time::Duration;

use gpui::{AppContext, Context};

use crate::gui::app::CditorV2View;
use crate::gui::app::cditor_v2_view::CditorViewState;
use crate::integration::{
    EditorDocument, EditorError, EditorEvent, EditorPersistence, EditorSaveReason,
    EditorSaveRequest, EditorSaveState, IntegrationPersistenceState,
};

pub(crate) type EditorEventCallback = Arc<dyn Fn(EditorEvent) + Send + Sync>;

pub(crate) struct EditorIntegrationController {
    pub(crate) document_id: String,
    pub(crate) persistence: Option<Arc<dyn EditorPersistence>>,
    pub(crate) autosave: Option<Duration>,
    pub(crate) callback: Option<EditorEventCallback>,
    pub(crate) persistence_state: IntegrationPersistenceState,
    fingerprint: u64,
    pending_reload: bool,
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
            pending_reload: false,
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
            pending_reload: false,
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
        let (event, state_event, callback, autosave_request) = {
            let Some(integration) = self.integration.as_mut() else {
                return;
            };
            let event = integration.sync_fingerprint(fingerprint);
            let state_event = event.as_ref().map(|_| EditorEvent::SaveStateChanged {
                state: integration.save_state(),
            });
            let autosave_request = event.as_ref().and_then(|_| {
                integration.autosave.map(|duration| {
                    (
                        integration.persistence_state.autosave_generation(),
                        duration,
                    )
                })
            });
            (
                event,
                state_event,
                integration.callback.clone(),
                autosave_request,
            )
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
        if let Some((generation, duration)) = autosave_request {
            self.schedule_integration_autosave(generation, duration, cx);
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

    pub(crate) fn integration_runtime_mut(
        &mut self,
    ) -> Result<&mut cditor_runtime::DocumentRuntime, EditorError> {
        match &mut self.state {
            CditorViewState::Ready(runtime) => Ok(runtime),
            CditorViewState::Loading { .. } | CditorViewState::LoadFailed { .. } => {
                Err(EditorError::NotReady)
            }
        }
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

    pub(crate) fn start_integration_save(
        &mut self,
        reason: EditorSaveReason,
        cx: &mut Context<Self>,
    ) -> Result<(), EditorError> {
        let document = self.integration_document()?;
        let (persistence, callback, document_id, version, state) = {
            let integration = self.integration.as_mut().ok_or(EditorError::NotReady)?;
            let persistence = integration
                .persistence
                .clone()
                .ok_or(EditorError::PersistenceNotConfigured)?;
            let Some(version) = integration.persistence_state.begin_save() else {
                return Ok(());
            };
            (
                persistence,
                integration.callback.clone(),
                integration.document_id.clone(),
                version,
                integration.save_state(),
            )
        };
        if let Some(callback) = &callback {
            callback(EditorEvent::SaveStateChanged { state });
        }
        let request = EditorSaveRequest {
            document_id: document_id.clone(),
            document,
            document_version: version,
            reason,
        };
        let save_task = cx.background_spawn(async move { persistence.save(request) });
        cx.spawn(async move |view, cx| {
            let result = save_task.await;
            let _ = view.update(cx, |view, cx| {
                let (callback, event, state, reload_after_save) = {
                    let Some(integration) = view.integration.as_mut() else {
                        return;
                    };
                    let callback = integration.callback.clone();
                    match result {
                        Ok(()) => {
                            integration.persistence_state.save_succeeded(version);
                            let reload = reason == EditorSaveReason::BeforeReload
                                || integration.pending_reload;
                            integration.pending_reload = false;
                            (
                                callback,
                                EditorEvent::Saved {
                                    document_id: document_id.clone(),
                                    document_version: version,
                                    reason,
                                },
                                integration.save_state(),
                                reload,
                            )
                        }
                        Err(error) => {
                            let message = error.to_string();
                            integration
                                .persistence_state
                                .save_failed(version, message.clone());
                            integration.pending_reload = false;
                            (
                                callback,
                                EditorEvent::SaveFailed {
                                    document_id: document_id.clone(),
                                    document_version: version,
                                    message,
                                },
                                integration.save_state(),
                                false,
                            )
                        }
                    }
                };
                if let Some(callback) = callback {
                    callback(event);
                    callback(EditorEvent::SaveStateChanged { state });
                }
                cx.notify();
                if reload_after_save {
                    let _ = view.start_integration_load(None, cx);
                }
            });
        })
        .detach();
        Ok(())
    }

    pub(crate) fn start_integration_reload(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), EditorError> {
        let is_dirty = self.integration_is_dirty();
        if is_dirty {
            let integration = self.integration.as_mut().ok_or(EditorError::NotReady)?;
            if integration.persistence.is_none() {
                return Err(EditorError::PersistenceNotConfigured);
            }
            integration.pending_reload = true;
            return self.start_integration_save(EditorSaveReason::BeforeReload, cx);
        }
        self.start_integration_load(None, cx)
    }

    pub(crate) fn start_integration_load(
        &mut self,
        fallback: Option<EditorDocument>,
        cx: &mut Context<Self>,
    ) -> Result<(), EditorError> {
        let (persistence, callback, document_id, generation) = {
            let integration = self.integration.as_mut().ok_or(EditorError::NotReady)?;
            let persistence = integration
                .persistence
                .clone()
                .ok_or(EditorError::PersistenceNotConfigured)?;
            (
                persistence,
                integration.callback.clone(),
                integration.document_id.clone(),
                integration.persistence_state.next_load_generation(),
            )
        };
        let requested_document_id = document_id.clone();
        let load_task = cx.background_spawn(async move {
            persistence
                .load(&requested_document_id)
                .map(|loaded| loaded.or(fallback))
        });
        cx.spawn(async move |view, cx| {
            let result = load_task.await;
            let _ = view.update(cx, |view, cx| {
                let is_current = view.integration.as_ref().is_some_and(|integration| {
                    integration
                        .persistence_state
                        .is_current_load_generation(generation)
                });
                if !is_current {
                    return;
                }
                match result {
                    Ok(document) => {
                        let document = match document {
                            Some(document) => document,
                            None => match EditorDocument::from_markdown(&document_id, "") {
                                Ok(document) => document,
                                Err(error) => {
                                    if let Some(callback) = &callback {
                                        callback(EditorEvent::LoadFailed {
                                            document_id: document_id.clone(),
                                            message: error.to_string(),
                                        });
                                    }
                                    return;
                                }
                            },
                        };
                        if document.document_id != document_id {
                            if let Some(callback) = &callback {
                                callback(EditorEvent::LoadFailed {
                                    document_id: document_id.clone(),
                                    message: EditorError::DocumentIdMismatch {
                                        expected: document_id.clone(),
                                        actual: document.document_id,
                                    }
                                    .to_string(),
                                });
                            }
                            return;
                        }
                        match document.into_runtime(720.0) {
                            Ok(runtime) => {
                                view.apply_loaded_runtime(runtime);
                                view.refresh_integration_baseline();
                                if let Some(callback) = &callback {
                                    callback(EditorEvent::Ready {
                                        document_id: document_id.clone(),
                                    });
                                    callback(EditorEvent::SaveStateChanged {
                                        state: view.integration_save_state(),
                                    });
                                }
                                cx.notify();
                            }
                            Err(error) => {
                                if let Some(callback) = &callback {
                                    callback(EditorEvent::LoadFailed {
                                        document_id: document_id.clone(),
                                        message: error.to_string(),
                                    });
                                }
                            }
                        }
                    }
                    Err(error) => {
                        if let Some(callback) = &callback {
                            callback(EditorEvent::LoadFailed {
                                document_id: document_id.clone(),
                                message: error.to_string(),
                            });
                        }
                    }
                }
            });
        })
        .detach();
        Ok(())
    }

    fn schedule_integration_autosave(
        &mut self,
        generation: u64,
        duration: Duration,
        cx: &mut Context<Self>,
    ) {
        let timer = cx.background_spawn(async move {
            std::thread::sleep(duration);
        });
        cx.spawn(async move |view, cx| {
            let _ = timer.await;
            let _ = view.update(cx, |view, cx| {
                let should_save = view.integration.as_ref().is_some_and(|integration| {
                    integration.persistence.is_some()
                        && integration.persistence_state.is_dirty()
                        && integration.persistence_state.autosave_generation() == generation
                });
                if should_save {
                    let _ = view.start_integration_save(EditorSaveReason::Autosave, cx);
                }
            });
        })
        .detach();
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
