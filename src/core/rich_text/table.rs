use super::{InlineSpan, plain_text_from_spans};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TablePayload {
    pub rows: Vec<TableRowPayload>,
    pub header_rows: usize,
    pub header_cols: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableRowPayload {
    pub cells: Vec<TableCellPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableCellPayload {
    pub spans: Vec<InlineSpan>,
}

impl TablePayload {
    pub fn cell_plain_text(&self, row: usize, col: usize) -> Option<String> {
        self.rows
            .get(row)
            .and_then(|row| row.cells.get(col))
            .map(|cell| plain_text_from_spans(&cell.spans))
    }

    pub fn set_cell_plain_text(
        &mut self,
        row: usize,
        col: usize,
        text: impl Into<String>,
    ) -> Option<()> {
        let cell = self.rows.get_mut(row)?.cells.get_mut(col)?;
        cell.spans = vec![InlineSpan::plain(text)];
        Some(())
    }
}
