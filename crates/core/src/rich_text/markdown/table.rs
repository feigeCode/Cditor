use super::*;
use crate::rich_text::TableCellAlign;

pub(super) fn is_table_candidate_line(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

pub(super) fn collect_table_candidate_region(lines: &[&str], start: usize) -> usize {
    let mut index = start + 1;
    while index < lines.len() && is_table_candidate_line(lines[index]) {
        index += 1;
    }
    index
}

pub(super) fn parse_table_region(lines: &[&str]) -> Option<TablePayload> {
    if lines.len() < 2 {
        return None;
    }
    let header = split_table_cells(lines[0])?;
    let alignment = split_table_cells(lines[1])?;
    if header.is_empty() || alignment.len() != header.len() {
        return None;
    }
    let alignments = alignment
        .iter()
        .map(|cell| parse_alignment_cell(cell))
        .collect::<Option<Vec<_>>>()?;
    if alignments.len() != header.len() {
        return None;
    }

    let mut rows = Vec::with_capacity(lines.len() - 1);
    rows.push(table_row_from_cells(header, &alignments));
    for line in &lines[2..] {
        let cells = split_table_cells(line)?;
        if cells.len() != rows[0].cells.len() {
            return None;
        }
        rows.push(table_row_from_cells(cells, &alignments));
    }
    Some(TablePayload {
        rows,
        columns: Vec::new(),
        header_rows: 1,
        header_cols: 0,
        header_style: Default::default(),
    })
}

fn split_table_cells(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    let without_left = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let without_edges = without_left.strip_suffix('|').unwrap_or(without_left);
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = without_edges.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'|') {
            cell.push('|');
            let _ = chars.next();
        } else if ch == '|' {
            cells.push(cell.trim().to_owned());
            cell.clear();
        } else {
            cell.push(ch);
        }
    }
    cells.push(cell.trim().to_owned());
    (!cells.is_empty()).then_some(cells)
}

fn parse_alignment_cell(cell: &str) -> Option<TableCellAlign> {
    let trimmed = cell.trim();
    let inner = trimmed.trim_matches(':');
    if inner.is_empty() || !inner.chars().all(|ch| ch == '-') {
        return None;
    }
    Some(match (trimmed.starts_with(':'), trimmed.ends_with(':')) {
        (true, true) => TableCellAlign::Center,
        (false, true) => TableCellAlign::Right,
        _ => TableCellAlign::Left,
    })
}

fn table_row_from_cells(cells: Vec<String>, alignments: &[TableCellAlign]) -> TableRowPayload {
    TableRowPayload {
        cells: cells
            .into_iter()
            .enumerate()
            .map(|(index, cell)| TableCellPayload {
                spans: parse_inline_markdown(&cell),
                align: alignments.get(index).copied().unwrap_or_default(),
                ..Default::default()
            })
            .collect(),
        height: Default::default(),
    }
}

pub(super) fn table_to_plain_markdown(payload: &BlockPayload) -> Option<String> {
    let BlockPayload::Table(table) = payload else {
        return None;
    };
    let first = table.rows.first()?;
    let columns = first.cells.len();
    if columns == 0 {
        return None;
    }
    let mut lines = Vec::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let cells = row
            .cells
            .iter()
            .map(|cell| escape_table_cell(&crate::rich_text::plain_text_from_spans(&cell.spans)))
            .collect::<Vec<_>>();
        lines.push(format!("| {} |", cells.join(" | ")));
        if row_index == 0 {
            lines.push(format!("| {} |", vec!["---"; columns].join(" | ")));
        }
    }
    Some(lines.join("\n"))
}

fn escape_table_cell(cell: &str) -> String {
    cell.replace('|', "\\|")
}
