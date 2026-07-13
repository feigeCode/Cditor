use gpui::{App, Entity};

use crate::gui::CditorV2View;

use super::{EditorDocument, EditorError, EditorSaveReason, EditorSaveState};

#[derive(Clone)]
pub struct EditorHandle {
    entity: Entity<CditorV2View>,
}

impl EditorHandle {
    pub(crate) fn new(entity: Entity<CditorV2View>) -> Self {
        Self { entity }
    }

    pub fn entity(&self) -> &Entity<CditorV2View> {
        &self.entity
    }

    pub fn set_markdown(
        &self,
        markdown: impl Into<String>,
        cx: &mut App,
    ) -> Result<(), EditorError> {
        let document_id = self
            .entity
            .read(cx)
            .integration_document_id()
            .ok_or(EditorError::NotReady)?
            .to_owned();
        let document = EditorDocument::from_markdown(document_id, &markdown.into())?;
        self.set_document(document, cx)
    }

    pub fn get_markdown(&self, cx: &App) -> Result<String, EditorError> {
        self.get_document(cx)?.to_markdown()
    }

    pub fn set_document(&self, document: EditorDocument, cx: &mut App) -> Result<(), EditorError> {
        self.entity.update(cx, |view, cx| {
            let expected = view
                .integration_document_id()
                .ok_or(EditorError::NotReady)?
                .to_owned();
            if document.document_id != expected {
                return Err(EditorError::DocumentIdMismatch {
                    expected,
                    actual: document.document_id,
                });
            }
            let runtime = document.into_runtime(720.0)?;
            view.apply_loaded_runtime(runtime);
            view.refresh_integration_baseline();
            cx.notify();
            Ok(())
        })
    }

    pub fn get_document(&self, cx: &App) -> Result<EditorDocument, EditorError> {
        self.entity.read(cx).integration_document()
    }

    pub fn save(&self, cx: &mut App) -> Result<(), EditorError> {
        self.entity.update(cx, |view, cx| {
            view.start_integration_save(EditorSaveReason::Manual, cx)
        })
    }

    pub fn reload(&self, cx: &mut App) -> Result<(), EditorError> {
        self.entity
            .update(cx, |view, cx| view.start_integration_reload(cx))
    }

    pub fn focus(&self, cx: &mut App) -> Result<(), EditorError> {
        self.entity.update(cx, |view, cx| {
            view.request_integration_focus();
            cx.notify();
        });
        Ok(())
    }

    pub fn is_dirty(&self, cx: &App) -> bool {
        self.entity.read(cx).integration_is_dirty()
    }

    pub fn save_state(&self, cx: &App) -> EditorSaveState {
        self.entity.read(cx).integration_save_state()
    }

    pub fn document_version(&self, cx: &App) -> u64 {
        self.entity.read(cx).integration_document_version()
    }

    pub fn set_readonly(&self, readonly: bool, cx: &mut App) -> Result<(), EditorError> {
        self.entity.update(cx, |view, cx| {
            view.set_integration_readonly(readonly);
            cx.notify();
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::EditorHandle;
    use crate::integration::{
        Editor, EditorDocument, EditorPersistence, EditorPersistenceError, EditorSaveRequest,
        EditorSaveState,
    };
    use gpui::{App, TestAppContext};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Clone, Default)]
    struct MockPersistence {
        loaded: Arc<Mutex<Option<EditorDocument>>>,
        saved: Arc<Mutex<Vec<EditorSaveRequest>>>,
        save_error: Arc<Mutex<Option<String>>>,
    }

    impl EditorPersistence for MockPersistence {
        fn load(
            &self,
            _document_id: &str,
        ) -> Result<Option<EditorDocument>, EditorPersistenceError> {
            Ok(self.loaded.lock().unwrap().clone())
        }

        fn save(&self, request: EditorSaveRequest) -> Result<(), EditorPersistenceError> {
            if let Some(message) = self.save_error.lock().unwrap().clone() {
                return Err(EditorPersistenceError::new(message));
            }
            self.saved.lock().unwrap().push(request);
            Ok(())
        }
    }

    fn assert_clone<T: Clone>() {}

    #[test]
    fn editor_handle_is_cloneable() {
        assert_clone::<EditorHandle>();
    }

    #[test]
    fn editor_handle_exposes_persistence_methods() {
        fn compile_contract(handle: &EditorHandle, cx: &mut App) {
            let _ = handle.save(cx);
            let _ = handle.reload(cx);
        }
        let _ = compile_contract;
    }

    #[gpui::test]
    async fn persistence_load_replaces_initial_fallback(cx: &TestAppContext) {
        let persistence = MockPersistence::default();
        *persistence.loaded.lock().unwrap() =
            Some(EditorDocument::from_markdown("doc-1", "# Persisted").unwrap());
        let handle = cx.update(|app| {
            Editor::builder()
                .document_id("doc-1")
                .initial_markdown("# Fallback")
                .persistence(persistence)
                .build(app)
                .unwrap()
        });
        cx.run_until_parked();
        assert_eq!(
            cx.read(|app| handle.get_markdown(app).unwrap()),
            "# Persisted"
        );
    }

    #[gpui::test]
    async fn manual_save_persists_dirty_document(cx: &TestAppContext) {
        let persistence = MockPersistence::default();
        let saved = persistence.saved.clone();
        let handle = cx.update(|app| {
            Editor::builder()
                .document_id("doc-1")
                .initial_markdown("Body")
                .persistence(persistence)
                .build(app)
                .unwrap()
        });
        cx.run_until_parked();
        cx.update(|app| {
            handle.entity().update(app, |view, cx| {
                let runtime = view.integration_runtime_mut().unwrap();
                runtime.focus_block_at_offset(1, 4).unwrap();
                runtime.insert_char('!').unwrap();
                view.sync_integration_document_change(cx);
            });
            handle.save(app).unwrap();
        });
        cx.run_until_parked();
        assert_eq!(saved.lock().unwrap().len(), 1);
        assert_eq!(
            cx.read(|app| handle.save_state(app)),
            EditorSaveState::Clean
        );
    }

    #[gpui::test]
    async fn autosave_debounces_multiple_changes(cx: &TestAppContext) {
        let persistence = MockPersistence::default();
        let saved = persistence.saved.clone();
        let handle = cx.update(|app| {
            Editor::builder()
                .document_id("doc-1")
                .initial_markdown("Body")
                .persistence(persistence)
                .autosave(Duration::from_millis(100))
                .build(app)
                .unwrap()
        });
        cx.run_until_parked();
        cx.update(|app| {
            handle.entity().update(app, |view, cx| {
                let runtime = view.integration_runtime_mut().unwrap();
                runtime.focus_block_at_offset(1, 4).unwrap();
                runtime.insert_char('!').unwrap();
                view.sync_integration_document_change(cx);
                let runtime = view.integration_runtime_mut().unwrap();
                runtime.insert_char('?').unwrap();
                view.sync_integration_document_change(cx);
            });
        });
        cx.run_until_parked();
        assert_eq!(saved.lock().unwrap().len(), 1);
    }

    #[gpui::test]
    async fn save_failure_is_exposed_without_losing_dirty_state(cx: &TestAppContext) {
        let persistence = MockPersistence::default();
        *persistence.save_error.lock().unwrap() = Some("disk full".to_owned());
        let handle = cx.update(|app| {
            Editor::builder()
                .document_id("doc-1")
                .initial_markdown("Body")
                .persistence(persistence)
                .build(app)
                .unwrap()
        });
        cx.run_until_parked();
        cx.update(|app| {
            handle.entity().update(app, |view, cx| {
                let runtime = view.integration_runtime_mut().unwrap();
                runtime.focus_block_at_offset(1, 4).unwrap();
                runtime.insert_char('!').unwrap();
                view.sync_integration_document_change(cx);
            });
            handle.save(app).unwrap();
        });
        cx.run_until_parked();
        assert!(matches!(
            cx.read(|app| handle.save_state(app)),
            EditorSaveState::SaveFailed { .. }
        ));
        assert!(cx.read(|app| handle.is_dirty(app)));
    }

    #[gpui::test]
    async fn reload_replaces_document_from_persistence(cx: &TestAppContext) {
        let persistence = MockPersistence::default();
        *persistence.loaded.lock().unwrap() =
            Some(EditorDocument::from_markdown("doc-1", "First").unwrap());
        let loaded = persistence.loaded.clone();
        let handle = cx.update(|app| {
            Editor::builder()
                .document_id("doc-1")
                .persistence(persistence)
                .build(app)
                .unwrap()
        });
        cx.run_until_parked();
        *loaded.lock().unwrap() = Some(EditorDocument::from_markdown("doc-1", "Second").unwrap());
        cx.update(|app| handle.reload(app).unwrap());
        cx.run_until_parked();
        assert_eq!(cx.read(|app| handle.get_markdown(app).unwrap()), "Second");
    }
}
