use gpui::{
    AnyElement, App, Entity, FocusHandle, IntoElement, ParentElement, ScrollHandle, Styled, Window,
    canvas, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::block::block_content::render_block_content;
use crate::gui::block::block_shell::{BlockActionState, block_shell};
use crate::gui::block::code::{CodeHighlightContext, render_code_block};
use crate::gui::block::heading::render_heading;
use crate::gui::block::html::{html_source_editor_visible, render_html_source_editor};
use crate::gui::block::media::schedule_rendered_media_height_report;
use crate::gui::block::paragraph::render_paragraph;
use crate::gui::block::table::{
    TableAxisSelection, TableCellRangeSelection, TableReorderPreview, TableResizePreview,
};
use crate::gui::block::{
    CodeHighlightCache, DocumentRenderCache, WhiteboardThumbnailCache, render_math_block,
    render_mermaid_block,
};
use crate::gui::input::{
    CodeLanguageEditState, focus_block_from_mouse, gutter_mouse_down_from_mouse,
    hover_block_from_mouse, toggle_block_fold_from_mouse, toggle_todo_from_mouse,
};
use crate::gui::platform::EDITOR_MONO_FONT_FAMILY;
use cditor_core::layout::COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX;
use cditor_core::rich_text::RichBlockKind;
use cditor_runtime::ViewBlockSnapshot;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockView {
    pub theme: GuiTheme,
}

impl BlockView {
    pub fn new(theme: GuiTheme) -> Self {
        Self { theme }
    }

    pub(crate) fn render(
        &self,
        block: &ViewBlockSnapshot,
        view: Entity<CditorV2View>,
        focus: FocusHandle,
        code_language_focus: FocusHandle,
        hovered: bool,
        action: BlockActionState,
        table_axis_selection: Option<TableAxisSelection>,
        image_resize_preview_width_px: Option<f32>,
        table_resize_preview: Option<TableResizePreview>,
        table_reorder_preview: Option<TableReorderPreview>,
        table_range_selection: Option<TableCellRangeSelection>,
        code_language_edit: Option<&CodeLanguageEditState>,
        code_theme_menu_open: bool,
        code_highlight_theme: &'static str,
        suppress_document_text_input: bool,
        table_scroll_handle: Option<ScrollHandle>,
        html_source_active: bool,
        readonly: bool,
        media_base_path: Option<&std::path::Path>,
        code_highlights: &CodeHighlightCache,
        document_renders: &DocumentRenderCache,
        mermaid_show_source: bool,
        whiteboard_thumbnails: &WhiteboardThumbnailCache,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.theme;
        let block_id = block.block_id;
        let content = render_kind_content(
            block,
            theme,
            view.clone(),
            focus,
            code_language_focus,
            action,
            table_axis_selection,
            image_resize_preview_width_px,
            table_resize_preview,
            table_reorder_preview,
            table_range_selection,
            code_language_edit,
            code_theme_menu_open,
            code_highlight_theme,
            suppress_document_text_input,
            table_scroll_handle,
            html_source_active,
            readonly,
            media_base_path,
            code_highlights,
            document_renders,
            mermaid_show_source,
            whiteboard_thumbnails,
            window,
            cx,
        );
        let focus_view = view.clone();
        let html_edit_view = view.clone();
        let begins_html_edit = matches!(block.kind, RichBlockKind::Html) && !readonly;
        let hover_view = view.clone();
        let add_view = view.clone();
        let gutter_view = view.clone();
        let on_todo_toggle = matches!(
            block.chrome.prefix,
            cditor_core::block::BlockPrefixSnapshot::Todo { .. }
        )
        .then(|| {
            let todo_view = view.clone();
            Box::new(
                move |event: &gpui::MouseDownEvent,
                      window: &mut gpui::Window,
                      cx: &mut gpui::App| {
                    toggle_todo_from_mouse(&todo_view, block_id, event, window, cx);
                    cx.stop_propagation();
                },
            ) as crate::gui::block::prefix::TodoToggleHandler
        });
        let on_fold_toggle = matches!(
            block.chrome.prefix,
            cditor_core::block::BlockPrefixSnapshot::Heading { .. }
                | cditor_core::block::BlockPrefixSnapshot::Toggle { .. }
        )
        .then(|| {
            let fold_view = view.clone();
            Box::new(
                move |event: &gpui::MouseDownEvent,
                      window: &mut gpui::Window,
                      cx: &mut gpui::App| {
                    toggle_block_fold_from_mouse(&fold_view, block_id, event, window, cx);
                    cx.stop_propagation();
                },
            ) as crate::gui::block::prefix::FoldToggleHandler
        });
        block_shell(
            block,
            theme,
            content,
            hovered,
            action,
            move |event, window, cx| {
                focus_block_from_mouse(&focus_view, block_id, event, window, cx);
                if begins_html_edit {
                    let _ = html_edit_view.update(cx, |view, cx| {
                        view.begin_document_source_from_gui(block_id, cx);
                    });
                } else {
                    let _ = html_edit_view.update(cx, |view, cx| {
                        view.end_html_source_from_gui(cx);
                    });
                }
                cx.stop_propagation();
            },
            Some(Box::new(move |event, _window, cx| {
                hover_block_from_mouse(&hover_view, block_id, event, cx);
            })),
            Some(Box::new(move |_event, window, cx| {
                let _ = add_view.update(cx, |view, cx| {
                    view.insert_paragraph_after_block_from_gui(block_id, window, cx);
                });
                cx.stop_propagation();
            })),
            Some(Box::new(move |event, window, cx| {
                gutter_mouse_down_from_mouse(&gutter_view, block_id, event, window, cx);
                cx.stop_propagation();
            })),
            on_todo_toggle,
            on_fold_toggle,
        )
    }
}

fn render_kind_content(
    block: &ViewBlockSnapshot,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    code_language_focus: FocusHandle,
    action: BlockActionState,
    table_axis_selection: Option<TableAxisSelection>,
    image_resize_preview_width_px: Option<f32>,
    table_resize_preview: Option<TableResizePreview>,
    table_reorder_preview: Option<TableReorderPreview>,
    table_range_selection: Option<TableCellRangeSelection>,
    code_language_edit: Option<&CodeLanguageEditState>,
    code_theme_menu_open: bool,
    code_highlight_theme: &'static str,
    suppress_document_text_input: bool,
    table_scroll_handle: Option<ScrollHandle>,
    html_source_active: bool,
    readonly: bool,
    media_base_path: Option<&std::path::Path>,
    code_highlights: &CodeHighlightCache,
    document_renders: &DocumentRenderCache,
    mermaid_show_source: bool,
    whiteboard_thumbnails: &WhiteboardThumbnailCache,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let math_code_language = match &block.kind {
        RichBlockKind::Code { language }
            if crate::gui::block::mermaid::is_math_code_language(language.as_deref()) =>
        {
            language.as_deref()
        }
        _ => None,
    };
    let show_document_source = if math_code_language.is_some() {
        !mermaid_show_source
    } else {
        mermaid_show_source
    };
    let show_block_source = if matches!(block.kind, RichBlockKind::Html) {
        html_source_active
    } else {
        show_document_source
    };
    let content = render_block_content(
        block,
        theme,
        view.clone(),
        focus,
        image_resize_preview_width_px,
        table_resize_preview,
        table_reorder_preview,
        table_range_selection,
        suppress_document_text_input
            || code_language_edit.is_some()
            || ((matches!(block.kind, RichBlockKind::Mermaid | RichBlockKind::Math)
                || math_code_language.is_some())
                && !show_document_source),
        show_block_source,
        table_axis_selection,
        table_scroll_handle,
        readonly,
        media_base_path,
        code_highlights,
        code_highlight_theme,
        whiteboard_thumbnails,
        cx,
    );
    match block.kind {
        RichBlockKind::Heading { level } => render_heading(level, content),
        RichBlockKind::Quote => content,
        RichBlockKind::Code { ref language } => {
            if let Some(language) = math_code_language {
                return render_math_block(
                    block.block_id,
                    match &block.payload {
                        cditor_core::rich_text::BlockPayloadView::Loaded(payload) => {
                            payload.content_version
                        }
                        _ => 0,
                    },
                    content,
                    show_document_source,
                    Some(language),
                    document_renders,
                    theme,
                    view,
                    cx,
                );
            }
            let language_edit = code_language_edit.filter(|edit| edit.block_id == block.block_id);
            render_code_block(
                block.block_id,
                content,
                theme,
                language.as_deref(),
                language_edit,
                code_theme_menu_open,
                CodeHighlightContext {
                    cache: code_highlights,
                    selected_theme: code_highlight_theme,
                },
                action.action_active,
                view.clone(),
                code_language_focus,
            )
        }
        RichBlockKind::Todo { .. } | RichBlockKind::BulletedList | RichBlockKind::NumberedList => {
            render_paragraph(content)
        }
        RichBlockKind::Table => content,
        RichBlockKind::Math => render_math_block(
            block.block_id,
            match &block.payload {
                cditor_core::rich_text::BlockPayloadView::Loaded(payload) => {
                    payload.content_version
                }
                _ => 0,
            },
            content,
            show_document_source,
            None,
            document_renders,
            theme,
            view,
            cx,
        ),
        RichBlockKind::Mermaid => render_mermaid_block(
            block.block_id,
            match &block.payload {
                cditor_core::rich_text::BlockPayloadView::Loaded(payload) => {
                    payload.content_version
                }
                _ => 0,
            },
            content,
            mermaid_show_source,
            document_renders,
            theme,
            view,
            cx,
        ),
        RichBlockKind::Html => {
            let content_version = match &block.payload {
                cditor_core::rich_text::BlockPayloadView::Loaded(payload) => {
                    payload.content_version
                }
                _ => 0,
            };
            let html_content = if html_source_editor_visible(
                html_source_active,
                readonly,
                suppress_document_text_input,
            ) {
                render_host_html_source_editor(block, view.clone(), readonly, window, cx)
                    .unwrap_or_else(|| {
                        render_html_source_editor(block.block_id, content, theme, view.clone())
                    })
            } else {
                content
            };
            render_html_height_reporter(html_content, block.block_id, content_version, view)
        }
        RichBlockKind::RawMarkdown => div()
            .w_full()
            .rounded(px(3.0))
            .bg(rgb(theme.code_background))
            .font_family(EDITOR_MONO_FONT_FAMILY)
            .text_size(px(13.0))
            .child(content)
            .into_any_element(),
        RichBlockKind::Divider | RichBlockKind::Separator => div()
            .w_full()
            .my(px(11.0))
            .h(px(1.0))
            .bg(rgb(theme.border))
            .into_any_element(),
        _ => render_paragraph(content),
    }
}

fn render_host_html_source_editor(
    block: &ViewBlockSnapshot,
    view: Entity<CditorV2View>,
    readonly: bool,
    window: &mut Window,
    cx: &mut App,
) -> Option<AnyElement> {
    let html = match &block.payload {
        cditor_core::rich_text::BlockPayloadView::Loaded(payload) => match &payload.payload {
            cditor_core::rich_text::BlockPayload::Html { html, .. } => html.clone(),
            _ => return None,
        },
        _ => return None,
    };
    view.update(cx, |view, cx| {
        if !view.source_editor_sessions.contains_key(&block.block_id) {
            let provider = view.source_editor_provider.as_ref()?.clone();
            if !provider.supports_language("html") {
                return None;
            }
            let document_id = view
                .ready_runtime_ref()
                .map(|runtime| runtime.document_id.to_string())
                .unwrap_or_default();
            let session = provider.create(
                crate::integration::SourceEditorConfig {
                    document_id,
                    block_id: block.block_id,
                    language: "html".to_owned(),
                    initial_value: html,
                    readonly,
                    line_numbers: true,
                    soft_wrap: true,
                },
                window,
                cx,
            );
            view.source_editor_sessions.insert(block.block_id, session);
        }
        view.source_editor_sessions
            .get(&block.block_id)
            .map(|session| session.render(window, cx))
    })
}

fn render_html_height_reporter(
    content: AnyElement,
    block_id: u64,
    content_version: u64,
    view: Entity<CditorV2View>,
) -> AnyElement {
    div()
        .relative()
        .w_full()
        .child(content)
        .child(
            canvas(
                move |bounds, _window, cx| {
                    let measured_height = html_block_measured_height(f64::from(bounds.size.height));
                    schedule_rendered_media_height_report(
                        view,
                        block_id,
                        content_version,
                        measured_height,
                        cx,
                    );
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .into_any_element()
}

fn html_block_measured_height(content_height: f64) -> f64 {
    content_height.max(0.0) + COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;

    #[test]
    fn block_view_can_classify_demo_block_kind() {
        let runtime = DocumentRuntime::demo();
        let projection = runtime.projection_for_window();
        let block = &projection.blocks[0];
        let view = BlockView::new(GuiTheme::light());

        assert_eq!(view.theme, GuiTheme::light());
        assert!(matches!(
            block.kind,
            cditor_core::rich_text::RichBlockKind::Heading { .. }
        ));
    }

    #[test]
    fn html_height_report_includes_block_shell_chrome() {
        assert_eq!(
            html_block_measured_height(640.0),
            640.0 + COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX
        );
        assert_eq!(
            html_block_measured_height(-1.0),
            COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX
        );
    }
}
