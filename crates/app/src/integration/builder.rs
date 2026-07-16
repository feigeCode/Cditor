use std::sync::Arc;
use std::time::Duration;

use gpui::AppContext;

use crate::api::AiProvider;
use crate::gui::CditorV2View;

use super::handle::EditorHandle;
use super::{EditorDocument, EditorError, EditorEvent, EditorPersistence};

pub struct Editor;

impl Editor {
    pub fn builder() -> EditorBuilder {
        EditorBuilder::default()
    }
}

enum InitialContent {
    Empty,
    Markdown(String),
    Document(EditorDocument),
}

pub struct EditorBuilder {
    document_id: String,
    initial_content: InitialContent,
    readonly: bool,
    debug_overlay: bool,
    persistence: Option<Arc<dyn EditorPersistence>>,
    autosave: Option<Duration>,
    callback: Option<Arc<dyn Fn(EditorEvent) + Send + Sync>>,
    ai_provider: Option<Arc<dyn AiProvider>>,
    ai_enabled: bool,
}

impl Default for EditorBuilder {
    fn default() -> Self {
        Self {
            document_id: "document-1".to_owned(),
            initial_content: InitialContent::Empty,
            readonly: false,
            debug_overlay: false,
            persistence: None,
            autosave: None,
            callback: None,
            ai_provider: None,
            ai_enabled: true,
        }
    }
}

impl EditorBuilder {
    pub fn document_id(mut self, document_id: impl Into<String>) -> Self {
        self.document_id = document_id.into();
        self
    }

    pub fn initial_markdown(mut self, markdown: impl Into<String>) -> Self {
        self.initial_content = InitialContent::Markdown(markdown.into());
        self
    }

    pub fn initial_document(mut self, document: EditorDocument) -> Self {
        self.document_id = document.document_id.clone();
        self.initial_content = InitialContent::Document(document);
        self
    }

    pub fn readonly(mut self, readonly: bool) -> Self {
        self.readonly = readonly;
        self
    }

    pub fn debug_overlay(mut self, debug_overlay: bool) -> Self {
        self.debug_overlay = debug_overlay;
        self
    }

    pub fn persistence<P>(mut self, persistence: P) -> Self
    where
        P: EditorPersistence,
    {
        self.persistence = Some(Arc::new(persistence));
        self
    }

    pub fn persistence_arc(mut self, persistence: Arc<dyn EditorPersistence>) -> Self {
        self.persistence = Some(persistence);
        self
    }

    pub fn autosave(mut self, duration: Duration) -> Self {
        self.autosave = Some(duration.max(Duration::from_millis(100)));
        self
    }

    pub fn on_event<F>(mut self, callback: F) -> Self
    where
        F: Fn(EditorEvent) + Send + Sync + 'static,
    {
        self.callback = Some(Arc::new(callback));
        self
    }

    pub fn ai_provider<P>(mut self, provider: P) -> Self
    where
        P: AiProvider + 'static,
    {
        self.ai_provider = Some(Arc::new(provider));
        self.ai_enabled = true;
        self
    }

    pub fn ai_provider_arc(mut self, provider: Arc<dyn AiProvider>) -> Self {
        self.ai_provider = Some(provider);
        self.ai_enabled = true;
        self
    }

    pub fn without_ai(mut self) -> Self {
        self.ai_enabled = false;
        self
    }

    pub fn build<C: AppContext>(self, cx: &mut C) -> Result<EditorHandle, EditorError> {
        let initial_document = self.resolve_initial_document()?;
        let runtime = initial_document.clone().into_runtime(720.0)?;
        let document_id = self.document_id.clone();
        let persistence = self.persistence.clone();
        let autosave = self.autosave;
        let callback = self.callback.clone();
        let ai_provider = self.ai_provider.clone();
        let ai_enabled = self.ai_enabled;
        let readonly = self.readonly;
        let debug_overlay = self.debug_overlay;
        let has_persistence = persistence.is_some();
        let load_fallback = initial_document.clone();
        let entity = cx.new(|cx| {
            let mut view =
                CditorV2View::from_runtime_with_options(runtime, debug_overlay, readonly, cx);
            view.sdk_configure_ai(ai_provider, ai_enabled);
            view.install_editor_integration(
                document_id.clone(),
                persistence,
                autosave,
                callback.clone(),
            )
            .expect("fresh runtime is ready for integration");
            view
        });
        let handle = EditorHandle::new(entity);
        if has_persistence {
            handle.entity().update(cx, |view, cx| {
                view.start_integration_load(Some(load_fallback), cx)
            })?;
        } else if let Some(callback) = self.callback {
            callback(EditorEvent::Ready {
                document_id: self.document_id,
            });
        }
        Ok(handle)
    }

    pub(crate) fn resolve_initial_document(&self) -> Result<EditorDocument, EditorError> {
        match &self.initial_content {
            InitialContent::Empty => EditorDocument::from_markdown(&self.document_id, ""),
            InitialContent::Markdown(markdown) => {
                EditorDocument::from_markdown(&self.document_id, markdown)
            }
            InitialContent::Document(document) => {
                if document.document_id != self.document_id {
                    return Err(EditorError::DocumentIdMismatch {
                        expected: self.document_id.clone(),
                        actual: document.document_id.clone(),
                    });
                }
                Ok(document.clone())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Editor;

    #[test]
    fn latest_initial_content_option_wins() {
        let document = Editor::builder()
            .document_id("doc-1")
            .initial_markdown("# first")
            .initial_markdown("# second")
            .resolve_initial_document()
            .unwrap();
        assert_eq!(document.to_markdown().unwrap(), "# second");
    }
}
