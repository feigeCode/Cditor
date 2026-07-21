use cditor_runtime::TableViewState;

pub(crate) fn table_view_for_available_width(
    table: &TableViewState,
    available_width_px: f32,
) -> TableViewState {
    let target_width = available_width_px.max(table.width_px);
    let extra = target_width - table.width_px;
    if extra <= 0.5 || table.col_count == 0 {
        return table.clone();
    }
    let auto_columns = (0..table.col_count)
        .filter(|&index| {
            table.table.columns.get(index).is_none_or(|column| {
                matches!(column.width, cditor_core::rich_text::TableTrackSize::Auto)
            })
        })
        .collect::<Vec<_>>();
    if auto_columns.is_empty() {
        return table.clone();
    }
    let mut adjusted = table.clone();
    let extra_per_column = extra / auto_columns.len() as f32;
    for column in auto_columns {
        if let Some(width) = adjusted.column_widths_px.get_mut(column) {
            *width += extra_per_column;
        }
    }
    let column_offsets = adjusted
        .column_widths_px
        .iter()
        .scan(0.0, |offset, width| {
            let start = *offset;
            *offset += *width;
            Some(start)
        })
        .collect::<Vec<_>>();
    for cell in &mut adjusted.visible_cells {
        cell.x_px = column_offsets
            .get(cell.position.col)
            .copied()
            .unwrap_or(0.0);
        cell.width_px = adjusted.column_widths_px
            [cell.position.col..(cell.position.col + cell.col_span).min(adjusted.col_count)]
            .iter()
            .sum();
    }
    adjusted.width_px = adjusted.column_widths_px.iter().sum();
    adjusted
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{TableCellAlign, TablePayload};
    use cditor_runtime::{TableCellPosition, TableVisibleCell};

    use super::*;

    #[test]
    fn auto_columns_and_cells_expand_to_the_available_width() {
        let projected = table_view_for_available_width(&two_column_table_view(), 400.0);

        assert_eq!(projected.width_px, 400.0);
        assert_eq!(projected.column_widths_px, vec![200.0, 200.0]);
        assert_eq!(projected.visible_cells[0].width_px, 200.0);
        assert_eq!(projected.visible_cells[1].x_px, 200.0);
        assert_eq!(projected.visible_cells[1].width_px, 200.0);
    }

    fn two_column_table_view() -> TableViewState {
        TableViewState {
            table: TablePayload::default(),
            row_count: 1,
            col_count: 2,
            width_px: 240.0,
            height_px: 36.0,
            column_widths_px: vec![120.0, 120.0],
            row_heights_px: vec![36.0],
            horizontal_scroll_offset_px: 0.0,
            visible_cells: vec![visible_cell(0, 0.0), visible_cell(1, 120.0)],
            focused_cell: Some(TableCellPosition { row: 0, col: 0 }),
            focused_cell_offset: Some(0),
            focused_cell_selection_range: None,
        }
    }

    fn visible_cell(col: usize, x_px: f32) -> TableVisibleCell {
        TableVisibleCell {
            position: TableCellPosition { row: 0, col },
            row_span: 1,
            col_span: 1,
            x_px,
            y_px: 0.0,
            width_px: 120.0,
            height_px: 36.0,
            header: false,
            align: TableCellAlign::Left,
            background_color: None,
            spans: Vec::new(),
        }
    }
}
