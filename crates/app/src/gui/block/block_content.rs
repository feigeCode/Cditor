use gpui::{
    AnyElement, App, Entity, FocusHandle, IntoElement, ObjectFit, ParentElement, ScrollHandle,
    Styled, StyledImage, div, img, px, rgb,
};

use crate::gui::app::CditorV2View;
use crate::gui::block::html::render_html_block;
use crate::gui::block::media::render_image_block;
use crate::gui::block::placeholder::{
    render_empty_ai_hint, render_error, render_loading, render_placeholder,
};
use crate::gui::block::table::render_table_block;
use crate::gui::block::table::{
    TableAxisSelection, TableCellRangeSelection, TableReorderPreview, TableResizePreview,
};
use crate::gui::block::{
    CodeHighlightCache, WhiteboardThumbnailCache, render_whiteboard_thumbnail,
};
use crate::gui::document::DEFAULT_DOCUMENT_CONTENT_WIDTH_PX;
use crate::gui::image_loader::{
    RasterImageElement, gpui_image_source, is_svg_image_source, load_render_image_from_base,
};
use crate::gui::text::{RichTextElement, RichTextLayoutInput};
use crate::gui::{GuiTheme, rich_text::render_payload_text};
use cditor_core::edit::SelectionRange;
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadView, InlineMediaFragment, parse_inline_media_fragments,
    plain_text_from_spans,
};
use cditor_runtime::ViewBlockSnapshot;

pub(crate) fn render_block_content(
    block: &ViewBlockSnapshot,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    image_resize_preview_width_px: Option<f32>,
    table_resize_preview: Option<TableResizePreview>,
    table_reorder_preview: Option<TableReorderPreview>,
    table_range_selection: Option<TableCellRangeSelection>,
    suppress_text_input: bool,
    table_selection: Option<TableAxisSelection>,
    table_scroll_handle: Option<ScrollHandle>,
    readonly: bool,
    media_base_path: Option<&std::path::Path>,
    code_highlights: &CodeHighlightCache,
    code_highlight_theme: &'static str,
    whiteboard_thumbnails: &WhiteboardThumbnailCache,
    cx: &mut App,
) -> AnyElement {
    match &block.payload {
        BlockPayloadView::Loaded(payload) => {
            if let Some(table_view) = &block.table_view {
                return render_table_block(
                    block.block_id,
                    payload.content_version,
                    table_view,
                    theme,
                    block.marked_range.clone(),
                    table_selection,
                    table_range_selection,
                    table_resize_preview,
                    table_reorder_preview,
                    table_scroll_handle,
                    view.clone(),
                    focus.clone(),
                    media_base_path,
                    cx,
                );
            }
            if let BlockPayload::Table(_table) = &payload.payload {
                return crate::gui::rich_text::render_payload_text(payload, theme);
            }
            if let BlockPayload::Image(image) = &payload.payload {
                return render_image_block(
                    block.block_id,
                    payload.content_version,
                    image,
                    theme,
                    view,
                    image_resize_preview_width_px,
                    media_base_path,
                    cx,
                );
            }
            if let BlockPayload::Html { html, .. } = &payload.payload {
                return render_html_block(block.block_id, html, theme, media_base_path, cx);
            }
            if matches!(payload.payload, BlockPayload::Whiteboard(_)) {
                return render_whiteboard_thumbnail(
                    block.block_id,
                    whiteboard_thumbnails,
                    theme,
                    view,
                );
            }
            if readonly && let BlockPayload::RichText { spans } = &payload.payload {
                let source = plain_text_from_spans(spans);
                if source.contains("![") {
                    let fragments = parse_inline_media_fragments(&source);
                    if fragments
                        .iter()
                        .any(|fragment| matches!(fragment, InlineMediaFragment::Image(_)))
                    {
                        return render_inline_media_fragments(
                            fragments,
                            theme,
                            media_base_path,
                            cx,
                        );
                    }
                }
            }
            if let Some(mut input) = RichTextLayoutInput::from_snapshot(
                block,
                f64::from(DEFAULT_DOCUMENT_CONTENT_WIDTH_PX),
                1,
                1,
            ) {
                if matches!(
                    block.kind,
                    cditor_core::rich_text::RichBlockKind::Code { .. }
                ) && let Some(spans) = code_highlights.spans(block.block_id)
                {
                    input.spans = spans.to_vec();
                }
                let text_len = input.spans.iter().map(|span| span.text.len()).sum();
                let selection_range = if block.selection_overlay {
                    None
                } else {
                    text_selection_range(&block.selection_range, text_len)
                };
                let text_theme = if matches!(
                    block.kind,
                    cditor_core::rich_text::RichBlockKind::Code { .. }
                ) {
                    GuiTheme {
                        code_text: code_highlights.theme_item(code_highlight_theme).foreground,
                        ..theme
                    }
                } else {
                    theme
                };
                let text_element = RichTextElement::new(input, text_theme)
                    .with_base_text_color(
                        block.attrs.color.as_deref().and_then(parse_block_hex_color),
                    )
                    .with_caret(caret_for_text_input(
                        block.caret_offset,
                        suppress_text_input,
                    ))
                    .with_marked_range(block.marked_range.clone())
                    .with_selection_range(selection_range)
                    .with_input_handler(
                        view,
                        focus,
                        text_input_active(block.focused, suppress_text_input),
                    )
                    .render();
                if should_show_empty_ai_hint(block, suppress_text_input, text_len) {
                    div()
                        .relative()
                        .w_full()
                        .min_h(px(24.0))
                        .child(text_element)
                        .child(render_empty_ai_hint(&block.kind, theme))
                        .into_any_element()
                } else {
                    text_element
                }
            } else {
                render_payload_text(payload, theme)
            }
        }
        BlockPayloadView::Placeholder { .. } => render_placeholder(block, theme),
        BlockPayloadView::Loading { .. } => render_loading(block, theme),
        BlockPayloadView::Error { message } => render_error(message, theme),
    }
}

fn render_inline_media_fragments(
    fragments: Vec<InlineMediaFragment>,
    theme: GuiTheme,
    media_base_path: Option<&std::path::Path>,
    cx: &mut App,
) -> AnyElement {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(4.0))
        .children(fragments.into_iter().map(|fragment| match fragment {
            InlineMediaFragment::Text(spans) => {
                crate::gui::rich_text::render_inline_spans(&spans, theme)
            }
            InlineMediaFragment::Image(image) => {
                if is_svg_image_source(&image.source) {
                    return img(gpui_image_source(&image.source, media_base_path))
                        .h(px(24.0))
                        .max_w(px(480.0))
                        .object_fit(ObjectFit::Contain)
                        .into_any_element();
                }
                if let Some(render_image) =
                    load_render_image_from_base(&image.source, media_base_path, cx)
                {
                    let size = render_image.size(0);
                    let aspect = (size.width.0 as f32 / size.height.0.max(1) as f32).max(0.1);
                    let height = 24.0;
                    div()
                        .w(px((height * aspect).clamp(24.0, 480.0)))
                        .h(px(height))
                        .child(RasterImageElement::new(
                            render_image,
                            ObjectFit::Contain,
                            px(0.0),
                        ))
                        .into_any_element()
                } else {
                    div()
                        .h(px(24.0))
                        .px(px(6.0))
                        .flex()
                        .items_center()
                        .rounded(px(3.0))
                        .bg(rgb(theme.hover_surface))
                        .text_color(rgb(theme.muted))
                        .child(if image.alt.trim().is_empty() {
                            "Image".to_owned()
                        } else {
                            image.alt
                        })
                        .into_any_element()
                }
            }
        }))
        .into_any_element()
}

fn parse_block_hex_color(value: &str) -> Option<u32> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

fn should_show_empty_ai_hint(
    block: &ViewBlockSnapshot,
    suppress_text_input: bool,
    text_len: usize,
) -> bool {
    block.focused
        && !suppress_text_input
        && text_len == 0
        && matches!(
            block.kind,
            cditor_core::rich_text::RichBlockKind::Paragraph
                | cditor_core::rich_text::RichBlockKind::Heading { .. }
                | cditor_core::rich_text::RichBlockKind::Quote
                | cditor_core::rich_text::RichBlockKind::Todo { .. }
                | cditor_core::rich_text::RichBlockKind::BulletedList
                | cditor_core::rich_text::RichBlockKind::NumberedList
                | cditor_core::rich_text::RichBlockKind::Toggle
                | cditor_core::rich_text::RichBlockKind::Callout { .. }
        )
}

fn text_input_active(block_focused: bool, suppress_text_input: bool) -> bool {
    block_focused && !suppress_text_input
}

fn caret_for_text_input(caret_offset: Option<usize>, suppress_text_input: bool) -> Option<usize> {
    (!suppress_text_input).then_some(caret_offset).flatten()
}

fn text_selection_range(
    selection: &Option<SelectionRange>,
    text_len: usize,
) -> Option<std::ops::Range<usize>> {
    match selection {
        Some(SelectionRange::Partial(range)) => Some(range.clone()),
        Some(SelectionRange::Full) => Some(0..text_len),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;

    #[test]
    fn block_content_accepts_loaded_demo_payload() {
        let runtime = DocumentRuntime::demo();
        let projection = runtime.projection_for_window();
        let block = projection
            .blocks
            .iter()
            .find(|block| {
                matches!(
                    block.payload,
                    cditor_core::rich_text::BlockPayloadView::Loaded(_)
                )
            })
            .unwrap();

        assert!(
            RichTextLayoutInput::from_snapshot(
                block,
                f64::from(DEFAULT_DOCUMENT_CONTENT_WIDTH_PX),
                1,
                1,
            )
            .is_some()
        );
    }

    #[test]
    fn ai_hint_only_appears_for_focused_empty_paragraphs() {
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![cditor_core::rich_text::BlockPayloadRecord::rich_text(
                1,
                cditor_core::rich_text::RichBlockKind::Paragraph,
                "",
            )],
            720.0,
        );
        let mut block = runtime.projection_for_window().blocks[0].clone();
        block.focused = true;
        assert!(should_show_empty_ai_hint(&block, false, 0));
        assert!(!should_show_empty_ai_hint(&block, false, 1));
        assert!(!should_show_empty_ai_hint(&block, true, 0));
        block.focused = false;
        assert!(!should_show_empty_ai_hint(&block, false, 0));
    }

    #[test]
    fn code_language_input_suppresses_body_text_input() {
        assert!(text_input_active(true, false));
        assert!(!text_input_active(true, true));
        assert_eq!(caret_for_text_input(Some(3), false), Some(3));
        assert_eq!(caret_for_text_input(Some(3), true), None);
    }

    #[test]
    fn full_document_text_fragment_selects_only_the_block_text_range() {
        assert_eq!(
            text_selection_range(&Some(SelectionRange::Full), 6),
            Some(0..6)
        );
        assert_eq!(
            text_selection_range(&Some(SelectionRange::Partial(2..4)), 6),
            Some(2..4)
        );
        assert_eq!(text_selection_range(&None, 6), None);
    }
}
