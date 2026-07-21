use std::ops::Range;
use std::path::Path;

use gpui::{
    AnyElement, App, Entity, FocusHandle, InteractiveElement, IntoElement, ObjectFit,
    ParentElement, ScrollDelta, ScrollWheelEvent, StatefulInteractiveElement, Styled, StyledImage,
    div, img, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::image_loader::{
    ImagePlaceholder, ImagePlaceholderState, RasterImageElement, RenderImageLoadState,
    gpui_image_source, is_svg_image_source, load_render_image_state_from_base,
    should_use_native_image_source,
};
use cditor_core::ids::BlockId;
use cditor_core::layout::TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX;
use cditor_core::rich_text::{ImagePayload, InlineSpan, TableCellAlign, plain_text_from_spans};
use cditor_runtime::{TableCellPosition, TableViewState};

use super::cell::{is_active_cell, render_table_cell};
use super::grid::render_table_grid;
use super::reorder::TableReorderPreview;
use super::resize::TableResizePreview;
use super::selection::{TableAxisSelection, TableCellRangeSelection};
use super::style::{
    V1_TABLE_EMPTY_PADDING_PX, V1_TABLE_RADIUS_PX, table_border_color, table_surface_background,
};
use super::text::TableCellTextElement;
use super::trace_table;

const TABLE_CELL_IMAGE_MIN_PREVIEW_HEIGHT_PX: f32 = 96.0;
const TABLE_CELL_IMAGE_MAX_PREVIEW_HEIGHT_PX: f32 = 240.0;
const TABLE_CELL_IMAGE_ASPECT_HEIGHT_RATIO: f32 = 9.0 / 16.0;
const TABLE_CELL_HORIZONTAL_PADDING_PX: f32 = 20.0;

pub(crate) fn render_table_block(
    block_id: BlockId,
    content_version: u64,
    table_view: &TableViewState,
    theme: GuiTheme,
    marked_range: Option<Range<usize>>,
    table_selection: Option<TableAxisSelection>,
    table_range_selection: Option<TableCellRangeSelection>,
    _table_resize_preview: Option<TableResizePreview>,
    _table_reorder_preview: Option<TableReorderPreview>,
    viewport_width_px: f32,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    media_base_path: Option<&Path>,
    cx: &mut App,
) -> AnyElement {
    if table_view.visible_cells.is_empty() {
        trace_table(
            "render.empty",
            format_args!("block={block_id} content_version={content_version}"),
        );
        return render_empty_table(theme);
    }
    trace_table(
        "render.block",
        format_args!(
            "block={block_id} content_version={content_version} rows={} cols={} focused_cell={focused_cell:?} focused_cell_offset={focused_cell_offset:?} marked={marked_range:?}",
            table_view.row_count,
            table_view.col_count,
            focused_cell = table_view.focused_cell,
            focused_cell_offset = table_view.focused_cell_offset,
        ),
    );
    let table_content = div()
        .relative()
        .left(px(table_content_left_px(
            table_view.horizontal_scroll_offset_px,
        )))
        // Fixed track size so the table keeps its intrinsic width and overflows
        // the scroll viewport instead of being squeezed by the flex parent.
        .flex_none()
        .w(px(table_view.width_px))
        .h(px(table_view.height_px))
        .child(
            div()
                .relative()
                .w(px(table_view.width_px))
                .h(px(table_view.height_px))
                .rounded(px(V1_TABLE_RADIUS_PX))
                .bg(rgb(table_surface_background(theme)))
                .overflow_hidden()
                .children(table_view.visible_cells.iter().map(|cell| {
                    let active = is_active_cell(
                        table_view.focused_cell,
                        cell.position.row,
                        cell.position.col,
                    );
                    let content = render_table_cell_content(
                        block_id,
                        content_version,
                        cell.spans.clone(),
                        table_view
                            .table
                            .rows
                            .get(cell.position.row)
                            .and_then(|row| row.cells.get(cell.position.col))
                            .map(|cell| cell.images.as_slice())
                            .unwrap_or(&[]),
                        cell.width_px,
                        active,
                        table_view.focused_cell_offset,
                        table_view.focused_cell_selection_range.clone(),
                        marked_range.clone(),
                        cell.header,
                        theme,
                        view.clone(),
                        focus.clone(),
                        cell.position,
                        cell.align,
                        media_base_path,
                        cx,
                    );
                    render_table_cell(
                        cell,
                        content,
                        theme,
                        table_view.focused_cell,
                        table_selection,
                        table_range_selection,
                        view.clone(),
                        block_id,
                    )
                }))
                .child(render_table_grid(table_view, theme)),
        );

    // Clip the table to the known document content width. Runtime state owns the
    // horizontal offset; GPUI does not maintain a second scroll position.
    let wheel_view = view.clone();
    let wheel_content_width_px = table_view.width_px;
    let viewport = div()
        .id(("table_scroll_container", block_id))
        .w_full()
        .min_w(px(0.0))
        .flex()
        .overflow_hidden()
        .on_scroll_wheel(move |event, _window, cx| {
            let Some(delta_x) = horizontal_table_scroll_delta(event) else {
                return;
            };
            let max_offset_x = (wheel_content_width_px - viewport_width_px).max(0.0);
            wheel_view.update(cx, |view, cx| {
                view.scroll_table_horizontal_from_gui(block_id, delta_x, max_offset_x);
                cx.notify();
            });
            cx.stop_propagation();
        });
    let viewport = viewport.child(table_content);

    // The custom horizontal scrollbar is rendered by the editor overlay layer.
    // This wrapper only reserves chrome height so following blocks are laid out
    // below the table chrome.
    div()
        .relative()
        .w_full()
        .min_w(px(0.0))
        .pb(px(TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX as f32))
        .child(viewport)
        .into_any_element()
}

pub(super) fn horizontal_table_scroll_intent(event: &ScrollWheelEvent) -> bool {
    horizontal_table_scroll_delta(event).is_some()
}

fn horizontal_table_scroll_delta(event: &ScrollWheelEvent) -> Option<f32> {
    let (x, y) = match event.delta {
        ScrollDelta::Pixels(delta) => (f32::from(delta.x), f32::from(delta.y)),
        ScrollDelta::Lines(delta) => (delta.x, delta.y),
    };
    (x.abs() > 0.01 && x.abs() >= y.abs()).then_some(x)
}

fn render_table_cell_content(
    block_id: BlockId,
    content_version: u64,
    spans: Vec<InlineSpan>,
    images: &[ImagePayload],
    cell_width_px: f32,
    active: bool,
    focused_cell_offset: Option<usize>,
    selected_range: Option<Range<usize>>,
    marked_range: Option<Range<usize>>,
    header: bool,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    position: TableCellPosition,
    align: TableCellAlign,
    media_base_path: Option<&Path>,
    cx: &mut App,
) -> AnyElement {
    let text = plain_text_from_spans(&spans);
    if active {
        trace_table(
            "render.active_cell",
            format_args!(
                "block={block_id} row={} col={} text_len={} caret={focused_cell_offset:?} marked={marked_range:?}",
                position.row,
                position.col,
                text.len()
            ),
        );
    }
    let text_element = TableCellTextElement::new(
        block_id,
        content_version,
        position,
        spans,
        active,
        focused_cell_offset,
        active.then_some(selected_range).flatten(),
        active.then_some(marked_range).flatten(),
        header,
        theme,
        view,
        focus,
        align,
    )
    .into_any_element();
    if images.is_empty() {
        return text_element;
    }
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .children(
            images.iter().map(|image| {
                render_table_cell_image(image, cell_width_px, theme, media_base_path, cx)
            }),
        )
        .child(text_element)
        .into_any_element()
}

fn render_table_cell_image(
    image: &ImagePayload,
    cell_width_px: f32,
    theme: GuiTheme,
    media_base_path: Option<&Path>,
    cx: &mut App,
) -> AnyElement {
    let preview_height = table_cell_image_preview_height_px(&image.source, cell_width_px);
    if should_use_native_image_source(&image.source) {
        let loading =
            ImagePlaceholder::new(image.source.clone(), theme, ImagePlaceholderState::Loading)
                .alt(image.alt.clone())
                .height(preview_height)
                .compact();
        let failed =
            ImagePlaceholder::new(image.source.clone(), theme, ImagePlaceholderState::Failed)
                .alt(image.alt.clone())
                .height(preview_height)
                .compact();
        return div()
            .w_full()
            .h(px(preview_height))
            .child(
                img(gpui_image_source(&image.source, media_base_path))
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .with_loading(move || loading.clone().into_any_element())
                    .with_fallback(move || failed.clone().into_any_element()),
            )
            .into_any_element();
    }
    let load_state = load_render_image_state_from_base(&image.source, media_base_path, cx);
    div()
        .w_full()
        .h(px(preview_height))
        .overflow_hidden()
        .child(match load_state {
            RenderImageLoadState::Ready(image) => {
                RasterImageElement::new(image, ObjectFit::Contain, px(0.0)).into_any_element()
            }
            state => ImagePlaceholder::for_load_state(
                image.source.clone(),
                image.alt.clone(),
                theme,
                &state,
            )
            .expect("non-ready image state must have a placeholder")
            .height(preview_height)
            .compact()
            .into_any_element(),
        })
        .into_any_element()
}

fn table_cell_image_preview_height_px(source: &str, cell_width_px: f32) -> f32 {
    if is_svg_image_source(source) {
        return 28.0;
    }
    ((cell_width_px - TABLE_CELL_HORIZONTAL_PADDING_PX) * TABLE_CELL_IMAGE_ASPECT_HEIGHT_RATIO)
        .clamp(
            TABLE_CELL_IMAGE_MIN_PREVIEW_HEIGHT_PX,
            TABLE_CELL_IMAGE_MAX_PREVIEW_HEIGHT_PX,
        )
}

fn render_empty_table(theme: GuiTheme) -> AnyElement {
    div()
        .rounded(px(V1_TABLE_RADIUS_PX))
        .border_1()
        .border_color(rgb(table_border_color(theme)))
        .bg(rgb(table_surface_background(theme)))
        .p(px(V1_TABLE_EMPTY_PADDING_PX))
        .text_color(rgb(theme.muted))
        .child("Empty table")
        .into_any_element()
}

fn table_content_left_px(horizontal_scroll_offset_px: f32) -> f32 {
    horizontal_scroll_offset_px
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_table_cells_render_readable_screenshot_previews() {
        assert!(
            (table_cell_image_preview_height_px("screenshot.png", 406.0) - 217.125).abs() < 0.001
        );
        assert_eq!(table_cell_image_preview_height_px("badge.svg", 430.0), 28.0);
    }

    #[test]
    fn table_content_uses_the_same_horizontal_offset_as_its_overlays() {
        assert_eq!(table_content_left_px(-180.0), -180.0);
        assert_eq!(table_content_left_px(0.0), 0.0);
    }
}
