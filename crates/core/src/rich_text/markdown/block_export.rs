use std::collections::{HashMap, HashSet};

use super::compatibility::{
    MarkdownDiagnostic, MarkdownDiagnosticSeverity, MarkdownExportMode, MarkdownExportResult,
    MarkdownFidelity,
};
use super::escape::{
    choose_code_fence, escape_block_start, escape_link_destination, escape_link_label,
    escape_table_cell,
};
use super::inline_export::export_inline_spans;
use crate::rich_text::{
    BlockAttrs, BlockPayload, CalloutVariant, RichBlockKind, RichBlockRecord, RichTextDocument,
    TableCellAlign, TableCellMerge, TablePayload, TableTrackSize, TextAlign,
};

pub fn export_document_blocks(
    document: &RichTextDocument,
    mode: MarkdownExportMode,
) -> MarkdownExportResult {
    let mut exporter = BlockExporter::new(document, mode);
    let markdown = exporter.export();
    let fidelity = if exporter.unsupported {
        MarkdownFidelity::Unsupported
    } else if exporter.normalized {
        MarkdownFidelity::Normalized
    } else {
        MarkdownFidelity::Semantic
    };
    MarkdownExportResult {
        markdown: if mode == MarkdownExportMode::Strict && exporter.unsupported {
            String::new()
        } else {
            markdown
        },
        fidelity,
        diagnostics: exporter.diagnostics,
    }
}

struct BlockExporter<'a> {
    document: &'a RichTextDocument,
    mode: MarkdownExportMode,
    by_id: HashMap<u64, &'a RichBlockRecord>,
    children: HashMap<Option<u64>, Vec<u64>>,
    diagnostics: Vec<MarkdownDiagnostic>,
    unsupported: bool,
    normalized: bool,
    rendering: HashSet<u64>,
    rendered: HashSet<u64>,
}

impl<'a> BlockExporter<'a> {
    fn new(document: &'a RichTextDocument, mode: MarkdownExportMode) -> Self {
        let by_id = document
            .blocks
            .iter()
            .map(|block| (block.id, block))
            .collect::<HashMap<_, _>>();
        let mut children: HashMap<Option<u64>, Vec<u64>> = HashMap::new();
        for block in &document.blocks {
            children.entry(block.parent_id).or_default().push(block.id);
        }
        Self {
            document,
            mode,
            by_id,
            children,
            diagnostics: Vec::new(),
            unsupported: false,
            normalized: false,
            rendering: HashSet::new(),
            rendered: HashSet::new(),
        }
    }

    fn export(&mut self) -> String {
        let mut roots = Vec::new();
        let mut seen = HashSet::new();
        for block_id in &self.document.root_blocks {
            if self.by_id.contains_key(block_id) && seen.insert(*block_id) {
                roots.push(*block_id);
            }
        }
        if let Some(fallback_roots) = self.children.get(&None) {
            for block_id in fallback_roots {
                if seen.insert(*block_id) {
                    roots.push(*block_id);
                }
            }
        }
        let mut rendered = self.render_sequence(&roots, 0);
        let remaining = self
            .document
            .blocks
            .iter()
            .map(|block| block.id)
            .filter(|block_id| !self.rendered.contains(block_id))
            .collect::<Vec<_>>();
        if !remaining.is_empty() {
            for block_id in &remaining {
                self.unsupported(
                    *block_id,
                    "markdown.document.structure_unsupported",
                    "Block is unreachable from the document root or participates in an invalid parent cycle",
                );
            }
            if self.mode == MarkdownExportMode::BestEffort {
                let fallback = self.render_sequence(&remaining, 0);
                if !fallback.is_empty() {
                    if !rendered.is_empty() {
                        rendered.push_str("\n\n");
                    }
                    rendered.push_str(&fallback);
                }
            }
        }
        rendered
    }

    fn render_sequence(&mut self, block_ids: &[u64], indent: usize) -> String {
        let mut rendered = String::new();
        let mut previous_was_list = false;
        for block_id in block_ids {
            let Some(block) = self.by_id.get(block_id).copied() else {
                continue;
            };
            let is_list = is_list_kind(&block.kind);
            if !rendered.is_empty() {
                rendered.push_str(if previous_was_list && is_list {
                    "\n"
                } else {
                    "\n\n"
                });
            }
            rendered.push_str(&self.render_block(block, indent));
            previous_was_list = is_list;
        }
        rendered
    }

    fn render_block(&mut self, block: &RichBlockRecord, indent: usize) -> String {
        if self.rendered.contains(&block.id) {
            return String::new();
        }
        if !self.rendering.insert(block.id) {
            self.unsupported(
                block.id,
                "markdown.document.parent_cycle_unsupported",
                "Block parent relationships contain a cycle",
            );
            return String::new();
        }
        self.check_block_attrs(block.id, &block.attrs);
        let text = match &block.kind {
            RichBlockKind::Paragraph => {
                escape_block_start(&self.render_rich_text(block, "paragraph"))
            }
            RichBlockKind::Heading { level } => {
                let level = if (1..=6).contains(level) {
                    *level
                } else {
                    self.normalized = true;
                    self.diagnostic(
                        block.id,
                        "markdown.block.heading_level_normalized",
                        "Heading level was clamped to the Markdown range 1 through 6",
                    );
                    (*level).clamp(1, 6)
                };
                format!(
                    "{} {}",
                    "#".repeat(usize::from(level)),
                    self.render_rich_text(block, "heading")
                )
            }
            RichBlockKind::BulletedList => {
                format!("- {}", self.render_rich_text(block, "bulleted list"))
            }
            RichBlockKind::NumberedList => {
                format!("1. {}", self.render_rich_text(block, "numbered list"))
            }
            RichBlockKind::Todo { checked } => format!(
                "- [{}] {}",
                if *checked { "x" } else { " " },
                self.render_rich_text(block, "todo")
            ),
            RichBlockKind::Quote => prefix_lines(&self.render_rich_text(block, "quote"), "> ", ">"),
            RichBlockKind::Callout { variant } => {
                let content = prefix_lines(&self.render_rich_text(block, "callout"), "> ", ">");
                format!("> [{}]\n{content}", callout_marker(*variant))
            }
            RichBlockKind::Code { language } => self.render_code(block, language.as_deref()),
            RichBlockKind::Table => self.render_table(block),
            RichBlockKind::Divider | RichBlockKind::Separator => "---".to_owned(),
            RichBlockKind::RawMarkdown => match &block.raw_fallback {
                Some(raw) => raw.clone(),
                None => {
                    self.unsupported(
                        block.id,
                        "markdown.block.raw_fallback_missing",
                        "Raw Markdown block no longer has its original source",
                    );
                    block.payload.plain_text()
                }
            },
            RichBlockKind::Mermaid => {
                let source = self.render_raw_source(block, "Mermaid");
                fenced_block(&source, Some("mermaid"))
            }
            RichBlockKind::Math => {
                let source = self.render_raw_source(block, "Math");
                format!("$$\n{source}\n$$")
            }
            RichBlockKind::Image => self.render_image(block),
            RichBlockKind::Html => match &block.payload {
                BlockPayload::Html { html, .. } => html.clone(),
                _ => {
                    self.unsupported(
                        block.id,
                        "markdown.block.html_source_missing",
                        "HTML block does not contain its original HTML source",
                    );
                    block.payload.plain_text()
                }
            },
            RichBlockKind::Toggle => self.unsupported_block_fallback(
                block,
                "markdown.block.toggle_unsupported",
                "Toggle blocks cannot be represented safely in standard Markdown",
            ),
            RichBlockKind::File | RichBlockKind::Attachment => self.unsupported_block_fallback(
                block,
                "markdown.block.attachment_unsupported",
                "File and attachment blocks require a rich-text document",
            ),
            RichBlockKind::Whiteboard => self.unsupported_block_fallback(
                block,
                "markdown.block.whiteboard_unsupported",
                "Whiteboard blocks require a rich-text document",
            ),
            RichBlockKind::MindMap => self.unsupported_block_fallback(
                block,
                "markdown.block.mindmap_unsupported",
                "Mind-map blocks require a rich-text document",
            ),
            RichBlockKind::Embed => self.unsupported_block_fallback(
                block,
                "markdown.block.embed_unsupported",
                "Embed blocks cannot be represented safely in standard Markdown",
            ),
            RichBlockKind::Database => self.unsupported_block_fallback(
                block,
                "markdown.block.database_unsupported",
                "Database blocks require a rich-text document",
            ),
            RichBlockKind::Custom(_) => self.unsupported_block_fallback(
                block,
                "markdown.block.custom_unsupported",
                "Custom blocks require a rich-text document",
            ),
            RichBlockKind::FootnoteDefinition => self.unsupported_block_fallback(
                block,
                "markdown.block.footnote_unsupported",
                "Footnotes are SourceOnly in the first Markdown round-trip version",
            ),
            RichBlockKind::Comment => self.unsupported_block_fallback(
                block,
                "markdown.block.comment_unsupported",
                "Comment blocks cannot be represented safely in standard Markdown",
            ),
        };

        let line_prefix = " ".repeat(indent);
        let mut rendered = prefix_lines(&text, &line_prefix, &line_prefix);
        if let Some(children) = self.children.get(&Some(block.id)).cloned()
            && !children.is_empty()
        {
            let children_are_lists = children.iter().all(|child_id| {
                self.by_id
                    .get(child_id)
                    .is_some_and(|child| is_list_kind(&child.kind))
            });
            if !is_list_kind(&block.kind) || !children_are_lists {
                self.unsupported(
                    block.id,
                    "markdown.document.child_structure_unsupported",
                    "Only nested list-item children can be round-tripped safely in the current Markdown model",
                );
            }
            let child_indent = if is_list_kind(&block.kind) {
                indent + 2
            } else {
                indent
            };
            let child_text = self.render_sequence(&children, child_indent);
            if !child_text.is_empty() {
                rendered.push('\n');
                rendered.push_str(&child_text);
            }
        }
        self.rendering.remove(&block.id);
        self.rendered.insert(block.id);
        rendered
    }

    fn render_rich_text(&mut self, block: &RichBlockRecord, label: &str) -> String {
        let BlockPayload::RichText { spans } = &block.payload else {
            self.unsupported(
                block.id,
                "markdown.block.payload_mismatch",
                format!("The {label} block does not contain rich-text spans"),
            );
            return block.payload.plain_text();
        };
        if spans.iter().any(|span| span.text.contains('\n'))
            && !matches!(
                block.kind,
                RichBlockKind::Quote | RichBlockKind::Callout { .. }
            )
        {
            self.unsupported(
                block.id,
                "markdown.inline.soft_break_unsupported",
                "Soft line breaks in this block kind cannot be round-tripped safely by the current Markdown parser",
            );
        }
        let mut inline = export_inline_spans(spans, self.mode);
        if inline.fidelity == MarkdownFidelity::Unsupported {
            self.unsupported = true;
        }
        for diagnostic in &mut inline.diagnostics {
            diagnostic.block_id = Some(block.id);
        }
        self.diagnostics.extend(inline.diagnostics);
        inline.markdown
    }

    fn render_code(&mut self, block: &RichBlockRecord, kind_language: Option<&str>) -> String {
        let (payload_language, text) = match &block.payload {
            BlockPayload::Code { language, text } => (language.as_deref(), text.as_str()),
            _ => {
                self.unsupported(
                    block.id,
                    "markdown.block.code_payload_missing",
                    "Code block does not contain code source",
                );
                return fenced_block(&block.payload.plain_text(), None);
            }
        };
        let language = payload_language.or(kind_language);
        let language = language.filter(|language| valid_language_tag(language));
        if payload_language.or(kind_language).is_some() && language.is_none() {
            self.unsupported(
                block.id,
                "markdown.block.code_language_invalid",
                "Code block language contains whitespace or fence characters",
            );
        }
        fenced_block(text, language)
    }

    fn render_raw_source(&mut self, block: &RichBlockRecord, label: &str) -> String {
        if let BlockPayload::RichText { spans } = &block.payload
            && spans.iter().any(|span| !span.marks.is_empty())
        {
            self.unsupported(
                block.id,
                "markdown.block.source_marks_unsupported",
                format!("{label} source contains rich-text marks that cannot be preserved"),
            );
        }
        block.payload.plain_text()
    }

    fn render_image(&mut self, block: &RichBlockRecord) -> String {
        let BlockPayload::Image(image) = &block.payload else {
            return self.unsupported_block_fallback(
                block,
                "markdown.block.image_source_missing",
                "Image block does not contain a stable source",
            );
        };
        if image.source.trim().is_empty() || image.source.starts_with("asset:") {
            self.unsupported(
                block.id,
                "markdown.block.image_source_unsupported",
                "Image source is internal and cannot be written safely to Markdown",
            );
        }
        if !image.caption.is_empty() || image.display_width_ratio_milli.is_some() {
            self.unsupported(
                block.id,
                "markdown.block.image_metadata_unsupported",
                "Image captions and display-width metadata cannot be preserved in standard Markdown",
            );
        }
        format!(
            "![{}](<{}>)",
            escape_link_label(&image.alt),
            escape_link_destination(&image.source)
        )
    }

    fn render_table(&mut self, block: &RichBlockRecord) -> String {
        let BlockPayload::Table(table) = &block.payload else {
            return self.unsupported_block_fallback(
                block,
                "markdown.block.table_payload_missing",
                "Table block does not contain table data",
            );
        };
        export_table(self, block.id, table)
    }

    fn check_block_attrs(&mut self, block_id: u64, attrs: &BlockAttrs) {
        if attrs.color.is_some()
            || attrs.background_color.is_some()
            || attrs.text_align != TextAlign::Start
            || attrs.indent != 0
            || attrs.folded
            || attrs.locked
            || !attrs.custom.is_empty()
        {
            self.unsupported(
                block_id,
                "markdown.block.attrs_unsupported",
                "Block color, alignment, indentation, folded state, lock state, or custom attributes cannot be preserved in standard Markdown",
            );
        }
    }

    fn unsupported_block_fallback(
        &mut self,
        block: &RichBlockRecord,
        code: &'static str,
        message: &'static str,
    ) -> String {
        self.unsupported(block.id, code, message);
        block.payload.plain_text()
    }

    fn unsupported(&mut self, block_id: u64, code: &'static str, message: impl Into<String>) {
        self.unsupported = true;
        let severity = if self.mode == MarkdownExportMode::Strict {
            MarkdownDiagnosticSeverity::Error
        } else {
            MarkdownDiagnosticSeverity::Warning
        };
        self.diagnostics
            .push(MarkdownDiagnostic::block(severity, code, message, block_id));
    }

    fn diagnostic(&mut self, block_id: u64, code: &'static str, message: impl Into<String>) {
        self.diagnostics.push(MarkdownDiagnostic::block(
            MarkdownDiagnosticSeverity::Warning,
            code,
            message,
            block_id,
        ));
    }
}

fn export_table(exporter: &mut BlockExporter<'_>, block_id: u64, table: &TablePayload) -> String {
    let column_count = table.column_count();
    if table.rows.is_empty() || column_count == 0 {
        exporter.unsupported(
            block_id,
            "markdown.table.empty_unsupported",
            "Empty tables cannot be represented in pipe-table Markdown",
        );
        return String::new();
    }
    if table.header_rows > 1 || table.header_cols > 0 {
        exporter.unsupported(
            block_id,
            "markdown.table.header_unsupported",
            "Markdown tables support exactly one header row and no header columns",
        );
    } else if table.header_rows == 0 {
        exporter.normalized = true;
        exporter.diagnostic(
            block_id,
            "markdown.table.header_normalized",
            "The first table row was normalized into a Markdown header row",
        );
    }
    if table.header_style != Default::default()
        || table
            .columns
            .iter()
            .any(|column| column.width != TableTrackSize::Auto)
        || table
            .rows
            .iter()
            .any(|row| row.height != TableTrackSize::Auto)
    {
        exporter.unsupported(
            block_id,
            "markdown.table.layout_unsupported",
            "Table header styling and explicit row or column sizes cannot be preserved in Markdown",
        );
    }
    if table.rows.iter().flat_map(|row| &row.cells).any(|cell| {
        !matches!(cell.merge, TableCellMerge::Unmerged) || cell.style != Default::default()
    }) {
        exporter.unsupported(
            block_id,
            "markdown.table.merge_or_style_unsupported",
            "Merged cells and cell background styles cannot be preserved in Markdown",
        );
    }

    let mut alignments = vec![TableCellAlign::Left; column_count];
    for (col, alignment_slot) in alignments.iter_mut().enumerate() {
        let mut column_alignment = None;
        for row in &table.rows {
            let alignment = row
                .cells
                .get(col)
                .map(|cell| cell.align)
                .unwrap_or(TableCellAlign::Left);
            if let Some(current) = column_alignment
                && current != alignment
            {
                exporter.unsupported(
                    block_id,
                    "markdown.table.mixed_alignment_unsupported",
                    "Markdown table alignment is column-wide, but this table contains mixed alignment in one column",
                );
                break;
            }
            column_alignment = Some(alignment);
        }
        *alignment_slot = column_alignment.unwrap_or(TableCellAlign::Left);
    }

    let mut lines = Vec::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let mut cells = Vec::with_capacity(column_count);
        for col in 0..column_count {
            let Some(cell) = row.cells.get(col) else {
                cells.push(String::new());
                continue;
            };
            if cell.spans.iter().any(|span| span.text.contains('\n')) {
                exporter.unsupported(
                    block_id,
                    "markdown.table.multiline_cell_unsupported",
                    "Multiline table cells cannot be represented safely in pipe-table Markdown",
                );
            }
            let mut inline = export_inline_spans(&cell.spans, exporter.mode);
            if inline.fidelity == MarkdownFidelity::Unsupported {
                exporter.unsupported = true;
            }
            for diagnostic in &mut inline.diagnostics {
                diagnostic.block_id = Some(block_id);
            }
            exporter.diagnostics.extend(inline.diagnostics);
            cells.push(escape_table_cell(&inline.markdown));
        }
        lines.push(format!("| {} |", cells.join(" | ")));
        if row_index == 0 {
            let separators = alignments
                .iter()
                .map(|alignment| match alignment {
                    TableCellAlign::Left => ":---",
                    TableCellAlign::Center => ":---:",
                    TableCellAlign::Right => "---:",
                })
                .collect::<Vec<_>>();
            lines.push(format!("| {} |", separators.join(" | ")));
        }
    }
    lines.join("\n")
}

fn is_list_kind(kind: &RichBlockKind) -> bool {
    matches!(
        kind,
        RichBlockKind::BulletedList | RichBlockKind::NumberedList | RichBlockKind::Todo { .. }
    )
}

fn prefix_lines(text: &str, nonempty_prefix: &str, empty_prefix: &str) -> String {
    text.split('\n')
        .map(|line| {
            if line.is_empty() {
                empty_prefix.to_owned()
            } else {
                format!("{nonempty_prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn fenced_block(text: &str, language: Option<&str>) -> String {
    let fence = choose_code_fence(text);
    format!("{fence}{}\n{text}\n{fence}", language.unwrap_or_default())
}

fn valid_language_tag(language: &str) -> bool {
    !language.is_empty() && !language.chars().any(char::is_whitespace) && !language.contains('`')
}

fn callout_marker(variant: CalloutVariant) -> &'static str {
    match variant {
        CalloutVariant::Note | CalloutVariant::Info => "!NOTE",
        CalloutVariant::Tip | CalloutVariant::Success => "!TIP",
        CalloutVariant::Important => "!IMPORTANT",
        CalloutVariant::Warning | CalloutVariant::Danger => "!WARNING",
        CalloutVariant::Caution => "!CAUTION",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rich_text::{
        FilePayload, InlineMark, InlineSpan, RichBlockRecord, TableCellPayload, TableRowPayload,
    };

    #[test]
    fn nested_lists_are_exported_from_parent_relationships() {
        let mut document = RichTextDocument::empty(1);
        let parent = RichBlockRecord::bulleted_list(1, "parent");
        let child = RichBlockRecord::numbered_list(2, "child").with_parent(1, 1);
        document.root_blocks.push(1);
        document.blocks.push(parent);
        document.blocks.push(child);

        let result = export_document_blocks(&document, MarkdownExportMode::Strict);
        assert_eq!(result.markdown, "- parent\n  1. child");
        assert_eq!(result.fidelity, MarkdownFidelity::Semantic);
    }

    #[test]
    fn strict_export_refuses_unsupported_rich_content() {
        let mut document = RichTextDocument::empty(1);
        document.push_root_block(RichBlockRecord::whiteboard(1, "{}"));

        let result = export_document_blocks(&document, MarkdownExportMode::Strict);
        assert!(result.markdown.is_empty());
        assert_eq!(result.fidelity, MarkdownFidelity::Unsupported);
        assert_eq!(result.diagnostics[0].block_id, Some(1));
    }

    #[test]
    fn inline_marks_survive_block_export() {
        let mut document = RichTextDocument::empty(1);
        let mut block = RichBlockRecord::paragraph(1, "");
        block.payload = BlockPayload::RichText {
            spans: vec![InlineSpan {
                text: "bold".to_owned(),
                marks: vec![InlineMark::Bold],
            }],
        };
        document.push_root_block(block);

        let result = export_document_blocks(&document, MarkdownExportMode::Strict);
        assert_eq!(result.markdown, "**bold**");
    }

    #[test]
    fn unsupported_inline_styles_all_block_strict_export() {
        for mark in [
            InlineMark::Underline,
            InlineMark::Color("#fff".to_owned()),
            InlineMark::Background("#000".to_owned()),
        ] {
            let mut document = RichTextDocument::empty(1);
            let mut block = RichBlockRecord::paragraph(1, "");
            block.payload = BlockPayload::RichText {
                spans: vec![InlineSpan {
                    text: "styled".to_owned(),
                    marks: vec![mark],
                }],
            };
            document.push_root_block(block);
            let result = export_document_blocks(&document, MarkdownExportMode::Strict);
            assert!(result.markdown.is_empty());
            assert_eq!(result.fidelity, MarkdownFidelity::Unsupported);
            assert_eq!(result.diagnostics[0].block_id, Some(1));
        }
    }

    #[test]
    fn rich_only_block_kinds_all_block_strict_export() {
        let blocks = vec![
            RichBlockRecord::new(
                1,
                RichBlockKind::Attachment,
                BlockPayload::File(FilePayload {
                    name: "file".to_owned(),
                    source: "asset:1".to_owned(),
                    size_bytes: None,
                }),
            ),
            RichBlockRecord::whiteboard(2, "{}"),
            RichBlockRecord::new(3, RichBlockKind::Database, BlockPayload::Empty),
            RichBlockRecord::new(
                4,
                RichBlockKind::Custom("host".to_owned()),
                BlockPayload::Empty,
            ),
        ];
        for mut block in blocks {
            let block_id = block.id;
            block.document_id = 1;
            let mut document = RichTextDocument::empty(1);
            document.push_root_block(block);
            let result = export_document_blocks(&document, MarkdownExportMode::Strict);
            assert!(result.markdown.is_empty());
            assert_eq!(result.diagnostics[0].block_id, Some(block_id));
        }
    }

    #[test]
    fn merged_table_blocks_strict_export() {
        let mut table = TablePayload {
            rows: vec![TableRowPayload {
                cells: vec![TableCellPayload::plain("a"), TableCellPayload::plain("b")],
                height: Default::default(),
            }],
            header_rows: 1,
            ..Default::default()
        };
        table.rows[0].cells[0].merge = TableCellMerge::Origin {
            row_span: 1,
            col_span: 2,
        };
        table.rows[0].cells[1].merge = TableCellMerge::Covered {
            origin_row: 0,
            origin_col: 0,
        };
        let mut document = RichTextDocument::empty(1);
        document.push_root_block(RichBlockRecord::table(1, table));

        let result = export_document_blocks(&document, MarkdownExportMode::Strict);
        assert!(result.markdown.is_empty());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "markdown.table.merge_or_style_unsupported")
        );
    }

    #[test]
    fn best_effort_keeps_a_degraded_result_and_warning() {
        let mut document = RichTextDocument::empty(1);
        document.push_root_block(RichBlockRecord::whiteboard(1, "{}"));

        let result = export_document_blocks(&document, MarkdownExportMode::BestEffort);
        assert_eq!(result.markdown, "whiteboard");
        assert_eq!(result.fidelity, MarkdownFidelity::Unsupported);
        assert_eq!(
            result.diagnostics[0].severity,
            MarkdownDiagnosticSeverity::Warning
        );
    }

    #[test]
    fn block_visual_attrs_block_strict_export() {
        let mut document = RichTextDocument::empty(1);
        let mut block = RichBlockRecord::paragraph(1, "styled");
        block.attrs.color = Some("#ff0000".to_owned());
        document.push_root_block(block);

        let result = export_document_blocks(&document, MarkdownExportMode::Strict);
        assert!(result.markdown.is_empty());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "markdown.block.attrs_unsupported")
        );
    }
}
