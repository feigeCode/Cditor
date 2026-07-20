use std::sync::Arc;

use cditor_core::rich_text::{BlockPayload, BlockPayloadRecord, InlineSpan, RichBlockKind};
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
    preferred_height: Arc<dyn Fn(&App) -> Option<f32>>,
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
            preferred_height: Arc::new(|_| None),
        }
    }

    pub fn with_preferred_height(mut self, height: f32) -> Self {
        self.preferred_height = Arc::new(move |_| Some(height.max(0.0)));
        self
    }

    pub fn with_preferred_height_provider(
        mut self,
        preferred_height: impl Fn(&App) -> f32 + 'static,
    ) -> Self {
        self.preferred_height = Arc::new(move |cx| Some(preferred_height(cx).max(0.0)));
        self
    }

    pub fn preferred_height(&self, cx: &App) -> Option<f32> {
        (self.preferred_height)(cx)
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

pub(crate) fn source_editor_config_for_block(
    document_id: String,
    block: &BlockPayloadRecord,
    readonly: bool,
) -> Option<SourceEditorConfig> {
    let (language, soft_wrap) = match &block.kind {
        RichBlockKind::Html => ("html".to_owned(), true),
        RichBlockKind::Code { language } => {
            (language.clone().unwrap_or_else(|| "text".to_owned()), false)
        }
        RichBlockKind::Math => ("latex".to_owned(), false),
        RichBlockKind::Mermaid => ("mermaid".to_owned(), false),
        RichBlockKind::RawMarkdown => ("markdown".to_owned(), true),
        _ => return None,
    };
    Some(SourceEditorConfig {
        document_id,
        block_id: block.block_id,
        language,
        initial_value: block.plain_text(),
        readonly,
        line_numbers: true,
        soft_wrap,
    })
}

pub(crate) fn replace_source_editor_value(payload: &mut BlockPayload, value: String) -> bool {
    let current = payload.plain_text();
    if current == value {
        return false;
    }
    match payload {
        BlockPayload::RichText { spans } => *spans = vec![InlineSpan::plain(value)],
        BlockPayload::Code { text, .. } => *text = value,
        BlockPayload::Html { html, .. } => *html = value,
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(kind: RichBlockKind, payload: BlockPayload) -> BlockPayloadRecord {
        BlockPayloadRecord {
            block_id: 7,
            content_version: 1,
            kind,
            payload,
        }
    }

    #[test]
    fn source_editor_config_covers_document_source_block_kinds() {
        let cases = [
            (
                record(
                    RichBlockKind::Html,
                    BlockPayload::Html {
                        html: "<p>x</p>".to_owned(),
                        sanitized: false,
                    },
                ),
                "html",
                true,
            ),
            (
                record(
                    RichBlockKind::Code {
                        language: Some("rust".to_owned()),
                    },
                    BlockPayload::Code {
                        language: Some("rust".to_owned()),
                        text: "fn main() {}".to_owned(),
                    },
                ),
                "rust",
                false,
            ),
            (
                record(
                    RichBlockKind::Math,
                    BlockPayload::RichText {
                        spans: vec![InlineSpan::plain("x = 1")],
                    },
                ),
                "latex",
                false,
            ),
            (
                record(
                    RichBlockKind::Mermaid,
                    BlockPayload::RichText {
                        spans: vec![InlineSpan::plain("flowchart TD")],
                    },
                ),
                "mermaid",
                false,
            ),
            (
                record(
                    RichBlockKind::RawMarkdown,
                    BlockPayload::RichText {
                        spans: vec![InlineSpan::plain("# title")],
                    },
                ),
                "markdown",
                true,
            ),
        ];

        for (block, expected_language, expected_soft_wrap) in cases {
            let config = source_editor_config_for_block("doc".to_owned(), &block, false).unwrap();
            assert_eq!(config.block_id, 7);
            assert_eq!(config.language, expected_language);
            assert_eq!(config.soft_wrap, expected_soft_wrap);
            assert!(config.line_numbers);
        }
    }

    #[test]
    fn source_editor_values_update_supported_payloads_without_changing_variants() {
        let mut math = BlockPayload::RichText {
            spans: vec![InlineSpan::plain("x = 1")],
        };
        let mut code = BlockPayload::Code {
            language: Some("rust".to_owned()),
            text: "old".to_owned(),
        };
        let mut html = BlockPayload::Html {
            html: "old".to_owned(),
            sanitized: false,
        };

        assert!(replace_source_editor_value(&mut math, "y = 2".to_owned()));
        assert!(replace_source_editor_value(&mut code, "new".to_owned()));
        assert!(replace_source_editor_value(
            &mut html,
            "<b>new</b>".to_owned()
        ));
        assert_eq!(math.plain_text(), "y = 2");
        assert_eq!(code.plain_text(), "new");
        assert_eq!(html.plain_text(), "<b>new</b>");
    }
}
