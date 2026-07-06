use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgb};

use crate::core::rich_text::TablePayload;
use crate::gui::GuiTheme;
use crate::gui::rich_text::render_inline_spans;

pub const V1_TABLE_RADIUS_PX: f32 = 8.0;
pub const V1_TABLE_CELL_MIN_WIDTH_PX: f32 = 120.0;
pub const V1_TABLE_CELL_PADDING_X_PX: f32 = 10.0;
pub const V1_TABLE_CELL_PADDING_Y_PX: f32 = 8.0;
pub const V1_TABLE_EMPTY_PADDING_PX: f32 = 8.0;
pub const V1_TABLE_HEADER_BACKGROUND: u32 = 0xf1f5f9;
pub const V1_TABLE_ACTIVE_BORDER: u32 = 0x60a5fa;

pub fn render_table_block(table: &TablePayload, theme: GuiTheme) -> AnyElement {
    if table.rows.is_empty() {
        return render_empty_table(theme);
    }

    div()
        .relative()
        .w_full()
        .rounded(px(V1_TABLE_RADIUS_PX))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.surface))
        .overflow_hidden()
        .child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .children(table.rows.iter().enumerate().map(|(row_index, row)| {
                    div()
                        .flex()
                        .bg(rgb(if is_header_row(table, row_index) {
                            theme.table_header_background
                        } else {
                            theme.surface
                        }))
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .w_full()
                        .children(row.cells.iter().enumerate().map(|(cell_index, cell)| {
                            render_table_cell(
                                table,
                                row_index,
                                cell_index,
                                render_inline_spans(&cell.spans, theme),
                                theme,
                            )
                        }))
                })),
        )
        .into_any_element()
}

fn render_empty_table(theme: GuiTheme) -> AnyElement {
    div()
        .rounded(px(V1_TABLE_RADIUS_PX))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.surface))
        .p(px(V1_TABLE_EMPTY_PADDING_PX))
        .text_color(rgb(theme.muted))
        .child("Empty table")
        .into_any_element()
}

fn render_table_cell(
    table: &TablePayload,
    row_index: usize,
    cell_index: usize,
    content: AnyElement,
    theme: GuiTheme,
) -> AnyElement {
    let header = is_header_cell(table, row_index, cell_index);
    div()
        .flex_1()
        .min_w(px(V1_TABLE_CELL_MIN_WIDTH_PX))
        .px(px(V1_TABLE_CELL_PADDING_X_PX))
        .py(px(V1_TABLE_CELL_PADDING_Y_PX))
        .border_r_1()
        .border_color(rgb(theme.border))
        .bg(rgb(if header {
            theme.table_header_background
        } else {
            theme.surface
        }))
        .child(content)
        .into_any_element()
}

fn is_header_row(table: &TablePayload, row_index: usize) -> bool {
    row_index < table.header_rows.max(usize::from(table.header_rows == 0))
}

fn is_header_cell(table: &TablePayload, row_index: usize, cell_index: usize) -> bool {
    is_header_row(table, row_index) || cell_index < table.header_cols
}

#[cfg(test)]
mod tests {
    use crate::core::rich_text::{InlineSpan, TableCellPayload, TableRowPayload};

    use super::*;

    #[test]
    fn v1_table_geometry_constants_match_editor2() {
        assert_eq!(V1_TABLE_RADIUS_PX, 8.0);
        assert_eq!(V1_TABLE_CELL_MIN_WIDTH_PX, 120.0);
        assert_eq!(V1_TABLE_CELL_PADDING_X_PX, 10.0);
        assert_eq!(V1_TABLE_CELL_PADDING_Y_PX, 8.0);
        assert_eq!(V1_TABLE_HEADER_BACKGROUND, 0xf1f5f9);
        assert_eq!(V1_TABLE_ACTIVE_BORDER, 0x60a5fa);
    }

    #[test]
    fn table_header_detection_follows_payload_header_rows_and_cols() {
        let table = TablePayload {
            rows: vec![TableRowPayload {
                cells: vec![
                    TableCellPayload {
                        spans: vec![InlineSpan::plain("A")],
                    },
                    TableCellPayload {
                        spans: vec![InlineSpan::plain("B")],
                    },
                ],
            }],
            header_rows: 1,
            header_cols: 1,
        };

        assert!(is_header_cell(&table, 0, 0));
        assert!(is_header_cell(&table, 0, 1));
        assert!(is_header_cell(&table, 1, 0));
        assert!(!is_header_cell(&table, 1, 1));
    }

    #[test]
    fn table_renderer_accepts_empty_and_non_empty_payloads() {
        let _ = render_table_block(&TablePayload::default(), GuiTheme::light());
        let table = TablePayload {
            rows: vec![TableRowPayload {
                cells: vec![TableCellPayload {
                    spans: vec![InlineSpan::plain("cell")],
                }],
            }],
            header_rows: 1,
            header_cols: 0,
        };
        let _ = render_table_block(&table, GuiTheme::light());
    }
}
