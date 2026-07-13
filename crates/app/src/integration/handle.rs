use gpui::{App, Entity};

use crate::gui::CditorV2View;

use super::{EditorDocument, EditorError, EditorSaveState};

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

    pub fn set_document(
        &self,
        document: EditorDocument,
        cx: &mut App,
    ) -> Result<(), EditorError> {
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

    fn assert_clone<T: Clone>() {}

    #[test]
    fn editor_handle_is_cloneable() {
        assert_clone::<EditorHandle>();
    }
}
