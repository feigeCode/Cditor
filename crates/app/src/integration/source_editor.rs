use std::sync::Arc;

use gpui::{AnyElement, App, Window};

#[derive(Clone, Debug)]
pub struct SourceEditorConfig {
    pub document_id: String,
    pub block_id: u64,
    pub language: String,
    pub initial_value: String,
    pub readonly: bool,
    pub line_numbers: bool,
    pub soft_wrap: bool,
}

pub struct SourceEditorSession {
    value: Arc<dyn Fn(&App) -> String>,
    focus: Arc<dyn Fn(&mut Window, &mut App)>,
    render: Arc<dyn Fn(&mut Window, &mut App) -> AnyElement>,
}

impl SourceEditorSession {
    pub fn new(
        value: impl Fn(&App) -> String + 'static,
        focus: impl Fn(&mut Window, &mut App) + 'static,
        render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Self {
            value: Arc::new(value),
            focus: Arc::new(focus),
            render: Arc::new(render),
        }
    }

    pub fn value(&self, cx: &App) -> String {
        (self.value)(cx)
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        (self.focus)(window, cx);
    }

    pub fn render(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        (self.render)(window, cx)
    }
}

pub trait SourceEditorProvider: 'static {
    fn supports_language(&self, language: &str) -> bool;

    fn create(
        &self,
        config: SourceEditorConfig,
        window: &mut Window,
        cx: &mut App,
    ) -> SourceEditorSession;
}
