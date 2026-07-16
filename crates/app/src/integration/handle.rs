use gpui::{AppContext, Entity};

use crate::gui::CditorV2View;

use super::{
    DocumentReplaceReason, EditorDocument, EditorError, EditorSaveReason, EditorSaveState,
    MarkdownApplyMode, MarkdownAssetResolver, MarkdownBundleExportResult, MarkdownBundleOptions,
    MarkdownCompatibility, MarkdownExportMode, MarkdownExportResult, MarkdownImportResult,
};

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

    pub fn set_markdown<C: AppContext>(
        &self,
        markdown: impl Into<String>,
        cx: &mut C,
    ) -> Result<(), EditorError> {
        self.apply_markdown(markdown, MarkdownApplyMode::Editable, cx)
            .map(|_| ())
    }

    pub fn get_markdown<C: AppContext>(&self, cx: &C) -> Result<String, EditorError> {
        Ok(self
            .export_markdown(MarkdownExportMode::BestEffort, cx)?
            .markdown)
    }

    pub fn export_markdown<C: AppContext>(
        &self,
        mode: MarkdownExportMode,
        cx: &C,
    ) -> Result<MarkdownExportResult, EditorError> {
        self.get_document(cx)?.export_markdown(mode)
    }

    pub fn export_markdown_bundle<C: AppContext>(
        &self,
        mode: MarkdownExportMode,
        options: &MarkdownBundleOptions,
        cx: &C,
    ) -> Result<MarkdownBundleExportResult, EditorError> {
        self.get_document(cx)?.export_markdown_bundle(mode, options)
    }

    pub fn apply_markdown<C: AppContext>(
        &self,
        markdown: impl Into<String>,
        mode: MarkdownApplyMode,
        cx: &mut C,
    ) -> Result<MarkdownImportResult, EditorError> {
        let document_id = self
            .entity
            .read_with(cx, |view, _| {
                view.integration_document_id().map(str::to_owned)
            })
            .ok_or(EditorError::NotReady)?;
        let result = EditorDocument::from_markdown_with_report(document_id, &markdown.into())?;
        if mode == MarkdownApplyMode::Editable
            && matches!(result.compatibility, MarkdownCompatibility::SourceOnly(_))
        {
            return Err(EditorError::MarkdownSourceOnly {
                diagnostics: result.diagnostics.clone(),
            });
        }
        let readonly = mode == MarkdownApplyMode::ReadOnlyPreview;
        self.entity.update(cx, |view, cx| {
            view.replace_integration_document(
                result.document.clone(),
                DocumentReplaceReason::SourceModeCommit,
                Some(readonly),
                cx,
            )
        })?;
        Ok(result)
    }

    pub fn apply_markdown_bundle<C: AppContext>(
        &self,
        markdown: impl Into<String>,
        resolver: &dyn MarkdownAssetResolver,
        mode: MarkdownApplyMode,
        cx: &mut C,
    ) -> Result<MarkdownImportResult, EditorError> {
        let document_id = self
            .entity
            .read_with(cx, |view, _| {
                view.integration_document_id().map(str::to_owned)
            })
            .ok_or(EditorError::NotReady)?;
        let result = EditorDocument::from_markdown_bundle_with_report(
            document_id,
            &markdown.into(),
            resolver,
        )?;
        if mode == MarkdownApplyMode::Editable
            && matches!(result.compatibility, MarkdownCompatibility::SourceOnly(_))
        {
            return Err(EditorError::MarkdownSourceOnly {
                diagnostics: result.diagnostics.clone(),
            });
        }
        let readonly = mode == MarkdownApplyMode::ReadOnlyPreview;
        self.entity.update(cx, |view, cx| {
            view.replace_integration_document(
                result.document.clone(),
                DocumentReplaceReason::SourceModeCommit,
                Some(readonly),
                cx,
            )
        })?;
        Ok(result)
    }

    pub fn set_document<C: AppContext>(
        &self,
        document: EditorDocument,
        cx: &mut C,
    ) -> Result<(), EditorError> {
        self.replace_document(document, DocumentReplaceReason::Programmatic, cx)
    }

    pub fn replace_document<C: AppContext>(
        &self,
        document: EditorDocument,
        reason: DocumentReplaceReason,
        cx: &mut C,
    ) -> Result<(), EditorError> {
        self.entity.update(cx, |view, cx| {
            view.replace_integration_document(document, reason, None, cx)
        })
    }

    pub fn get_document<C: AppContext>(&self, cx: &C) -> Result<EditorDocument, EditorError> {
        self.entity
            .read_with(cx, |view, _| view.integration_document())
    }

    pub fn save<C: AppContext>(&self, cx: &mut C) -> Result<(), EditorError> {
        self.entity.update(cx, |view, cx| {
            view.start_integration_save(EditorSaveReason::Manual, cx)
        })
    }

    pub fn reload<C: AppContext>(&self, cx: &mut C) -> Result<(), EditorError> {
        self.entity
            .update(cx, |view, cx| view.start_integration_reload(cx))
    }

    pub fn focus<C: AppContext>(&self, cx: &mut C) -> Result<(), EditorError> {
        self.entity.update(cx, |view, cx| {
            view.request_integration_focus();
            cx.notify();
        });
        Ok(())
    }

    pub fn is_dirty<C: AppContext>(&self, cx: &C) -> bool {
        self.entity
            .read_with(cx, |view, _| view.integration_is_dirty())
    }

    pub fn save_state<C: AppContext>(&self, cx: &C) -> EditorSaveState {
        self.entity
            .read_with(cx, |view, _| view.integration_save_state())
    }

    pub fn document_version<C: AppContext>(&self, cx: &C) -> u64 {
        self.entity
            .read_with(cx, |view, _| view.integration_document_version())
    }

    pub fn set_readonly<C: AppContext>(
        &self,
        readonly: bool,
        cx: &mut C,
    ) -> Result<(), EditorError> {
        self.entity.update(cx, |view, cx| {
            view.set_integration_readonly(readonly);
            cx.notify();
        });
        Ok(())
    }

    pub fn is_readonly<C: AppContext>(&self, cx: &C) -> bool {
        self.entity
            .read_with(cx, |view, _| view.integration_is_readonly())
    }
}

#[cfg(test)]
mod tests {
    use super::EditorHandle;
    use crate::integration::{
        DocumentReplaceReason, Editor, EditorDocument, EditorError, EditorEvent, EditorPersistence,
        EditorPersistenceError, EditorSaveRequest, EditorSaveState, MarkdownApplyMode,
        MarkdownCompatibility, MarkdownExportMode,
    };
    use cditor_core::rich_text::{BlockPayload, InlineMark, InlineSpan};
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

    #[gpui::test]
    async fn markdown_apply_rejects_source_only_editing_but_allows_readonly_preview(
        cx: &TestAppContext,
    ) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorded = events.clone();
        let persistence = MockPersistence::default();
        let saved = persistence.saved.clone();
        let handle = cx.update(|app| {
            Editor::builder()
                .document_id("doc-1")
                .initial_markdown("Original")
                .persistence(persistence)
                .autosave(Duration::from_millis(100))
                .on_event(move |event| recorded.lock().unwrap().push(event))
                .build(app)
                .unwrap()
        });
        cx.run_until_parked();
        events.lock().unwrap().clear();

        let editable = cx.update(|app| {
            handle.apply_markdown(
                "[^1]: unsupported footnote",
                MarkdownApplyMode::Editable,
                app,
            )
        });
        assert!(matches!(
            editable,
            Err(EditorError::MarkdownSourceOnly { .. })
        ));
        assert_eq!(cx.read(|app| handle.get_markdown(app).unwrap()), "Original");

        let preview = cx
            .update(|app| {
                handle.apply_markdown(
                    "[^1]: unsupported footnote",
                    MarkdownApplyMode::ReadOnlyPreview,
                    app,
                )
            })
            .unwrap();
        cx.run_until_parked();
        assert!(matches!(
            preview.compatibility,
            MarkdownCompatibility::SourceOnly(_)
        ));
        assert!(cx.read(|app| handle.is_readonly(app)));
        assert!(!cx.read(|app| handle.is_dirty(app)));
        assert!(events.lock().unwrap().iter().any(|event| matches!(
            event,
            EditorEvent::DocumentReplaced {
                reason: DocumentReplaceReason::SourceModeCommit,
                ..
            }
        )));
        assert!(
            !events
                .lock()
                .unwrap()
                .iter()
                .any(|event| matches!(event, EditorEvent::Changed { .. }))
        );
        assert!(saved.lock().unwrap().is_empty());
    }

    #[gpui::test]
    async fn strict_markdown_export_rejects_unsupported_inline_marks(cx: &TestAppContext) {
        let handle = cx.update(|app| {
            Editor::builder()
                .document_id("doc-1")
                .initial_markdown("Body")
                .build(app)
                .unwrap()
        });
        cx.update(|app| {
            let mut document = handle.get_document(app).unwrap();
            document.blocks[0].payload.payload = BlockPayload::RichText {
                spans: vec![InlineSpan {
                    text: "Body".to_owned(),
                    marks: vec![InlineMark::Underline],
                }],
            };
            handle.set_document(document, app).unwrap();
        });

        let result = cx.read(|app| handle.export_markdown(MarkdownExportMode::Strict, app));
        assert!(matches!(
            result,
            Err(EditorError::MarkdownUnsupported { .. })
        ));
    }
}
