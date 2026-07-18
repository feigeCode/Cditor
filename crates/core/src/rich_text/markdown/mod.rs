use super::{
    BlockPayload, CalloutVariant, ImagePayload, InlineMark, InlineSpan, RichBlockKind,
    RichBlockRecord, RichTextDocument, TableCellPayload, TablePayload, TableRowPayload,
};
use crate::ids::{BlockId, DocumentId};
use crate::rich_text::MARKDOWN_PARSE_STATS;

mod block;
mod block_export;
mod compatibility;
mod escape;
mod export;
mod inline_export;

pub use block::parse_callout_marker;
pub use block_export::export_document_blocks;
pub use compatibility::{
    MarkdownCompatibility, MarkdownDiagnostic, MarkdownDiagnosticSeverity, MarkdownExportMode,
    MarkdownExportResult, MarkdownFidelity,
};
pub use inline_export::{InlineMarkdownExport, export_inline_spans};
mod inline;
mod table;

use block::{
    block_kind_for_marker, looks_like_single_block_markdown, parse_fence_start, parse_heading,
    parse_numbered_item,
};
use export::block_to_plain_markdown;
use inline::{parse_block_image, parse_inline_markdown, parse_inline_markdown_extended};
use table::{collect_table_candidate_region, is_table_candidate_line, parse_table_region};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedMarkdownDocument {
    pub root_blocks: Vec<BlockId>,
    pub blocks: Vec<RichBlockRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarkdownParseResult {
    pub document: ParsedMarkdownDocument,
    pub compatibility: MarkdownCompatibility,
    pub diagnostics: Vec<MarkdownDiagnostic>,
}

impl ParsedMarkdownDocument {
    pub fn push_root_block(&mut self, block: RichBlockRecord) -> BlockId {
        let id = block.id;
        self.root_blocks.push(id);
        self.blocks.push(block);
        id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkdownImportOptions {
    pub document_id: DocumentId,
    pub first_block_id: BlockId,
}

impl Default for MarkdownImportOptions {
    fn default() -> Self {
        Self {
            document_id: 1,
            first_block_id: 1,
        }
    }
}

#[must_use]
pub fn parse_markdown_document(
    markdown: &str,
    options: MarkdownImportOptions,
) -> ParsedMarkdownDocument {
    let markdown = unwrap_outer_markdown_fence(markdown);
    MARKDOWN_PARSE_STATS.record_full_parse(markdown.len());
    let mut parser = MarkdownParser::new(options);
    parser.parse_document(markdown)
}

#[must_use]
pub fn parse_markdown_document_with_report(
    markdown: &str,
    options: MarkdownImportOptions,
) -> MarkdownParseResult {
    let document = parse_markdown_document(markdown, options);
    let mut diagnostics = analyze_markdown_compatibility(markdown);
    diagnostics.extend(analyze_parsed_document_compatibility(&document));
    let compatibility = MarkdownCompatibility::from_diagnostics(&diagnostics);
    MarkdownParseResult {
        document,
        compatibility,
        diagnostics,
    }
}

#[must_use]
pub fn import_markdown_block_incremental(
    markdown: &str,
    options: MarkdownImportOptions,
) -> Option<RichBlockRecord> {
    let markdown = unwrap_outer_markdown_fence(markdown);
    let trimmed = markdown.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut parser = MarkdownParser::new(options);
    if markdown.contains('\n') || markdown.contains('\r') {
        let block = parser.parse_incremental_multiline_block(markdown);
        if block.is_some() {
            MARKDOWN_PARSE_STATS.record_incremental_parse(markdown.len());
        }
        return block;
    }

    looks_like_single_block_markdown(trimmed).then(|| {
        MARKDOWN_PARSE_STATS.record_incremental_parse(markdown.len());
        parser.parse_markdown_line(markdown)
    })
}

#[must_use]
pub fn import_markdown_inline_incremental(markdown: &str) -> Option<Vec<InlineSpan>> {
    if markdown.contains('\n') || markdown.contains('\r') {
        return None;
    }
    Some(parse_inline_markdown(markdown))
}

#[must_use]
pub fn block_kind_shortcut(marker: &str) -> Option<RichBlockKind> {
    block_kind_for_marker(marker)
}

#[must_use]
pub fn block_kind_shortcut_with_marker_len(text: &str) -> Option<(RichBlockKind, usize)> {
    const MARKERS: &[&str] = &[
        "> [!IMPORTANT] ",
        "> [!WARNING] ",
        "> [!CAUTION] ",
        "> [!NOTE] ",
        "> [!TIP] ",
        "###### ",
        "##### ",
        "#### ",
        "### ",
        "## ",
        "# ",
        "- [ ] ",
        "- [x] ",
        "- [X] ",
        "[ ] ",
        "[x] ",
        "[X] ",
        "--- ",
        "*** ",
        "___ ",
        "- ",
        "* ",
        "+ ",
        "> ",
    ];
    MARKERS
        .iter()
        .find_map(|marker_with_space| {
            text.strip_prefix(marker_with_space).map(|_| {
                let marker = marker_with_space.trim_end();
                (
                    block_kind_for_marker(marker).expect("known markdown marker"),
                    marker_with_space.len(),
                )
            })
        })
        .or_else(|| {
            let digit_count = text.bytes().take_while(u8::is_ascii_digit).count();
            if digit_count == 0 || !text[digit_count..].starts_with(". ") {
                return None;
            }
            Some((RichBlockKind::NumberedList, digit_count + 2))
        })
}

#[must_use]
pub fn code_fence_shortcut(text: &str) -> Option<RichBlockKind> {
    let language = text
        .strip_prefix("```")
        .or_else(|| text.strip_prefix("···"))?
        .trim();
    if language.contains(char::is_whitespace) {
        return None;
    }
    Some(RichBlockKind::Code {
        language: (!language.is_empty()).then(|| language.to_lowercase()),
    })
}

#[must_use]
pub fn looks_like_markdown_paste(text: &str) -> bool {
    let text = unwrap_outer_markdown_fence(text);
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("# ")
            || trimmed.starts_with("## ")
            || trimmed.starts_with("### ")
            || trimmed.starts_with("> ")
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("+ ")
            || trimmed.starts_with("- [ ] ")
            || trimmed.starts_with("- [x] ")
            || trimmed.starts_with("- [X] ")
            || trimmed.starts_with("```")
            || trimmed.starts_with("···")
            || trimmed.starts_with('|')
            || trimmed == "---"
            || trimmed == "***"
            || trimmed == "___"
            || parse_numbered_item(trimmed).is_some()
            || parse_inline_markdown_extended(trimmed).changed
    })
}

fn unwrap_outer_markdown_fence(markdown: &str) -> &str {
    let trimmed = markdown.trim();
    let Some((opening, rest)) = trimmed.split_once('\n') else {
        return markdown;
    };
    let Some((body, closing)) = rest.rsplit_once('\n') else {
        return markdown;
    };
    let opening = opening.trim();
    let closing = closing.trim();
    let markdown_fence = matches!(
        opening.to_ascii_lowercase().as_str(),
        "```markdown" | "```md" | "···markdown" | "···md"
    );
    if markdown_fence && matches!(closing, "```" | "···") {
        body
    } else {
        markdown
    }
}

#[must_use]
pub fn markdown_inline_shortcut_spans(text: &str) -> Option<Vec<InlineSpan>> {
    let spans = parse_inline_markdown_extended(text);
    spans.changed.then_some(spans.spans)
}

#[must_use]
pub fn export_plain_markdown(document: &RichTextDocument) -> String {
    document
        .blocks
        .iter()
        .map(block_to_plain_markdown)
        .collect::<Vec<_>>()
        .join("\n")
}

fn analyze_markdown_compatibility(markdown: &str) -> Vec<MarkdownDiagnostic> {
    let lines = source_lines(markdown);
    let mut diagnostics = Vec::new();
    detect_frontmatter(&lines, &mut diagnostics);

    let mut fence: Option<(char, usize, usize)> = None;
    let mut math_fence: Option<usize> = None;
    let mut index = 0;
    while index < lines.len() {
        let (offset, line) = lines[index];
        let trimmed = line.trim_start();
        if trimmed == "$$" {
            math_fence = match math_fence {
                Some(_) => None,
                None => Some(offset),
            };
            index += 1;
            continue;
        }
        if math_fence.is_some() {
            index += 1;
            continue;
        }
        if let Some((marker, length)) = fence_marker(trimmed) {
            match fence {
                Some((open_marker, open_length, _))
                    if marker == open_marker && length >= open_length =>
                {
                    fence = None;
                }
                None if marker == '~' => diagnostics.push(MarkdownDiagnostic::source(
                    MarkdownDiagnosticSeverity::Error,
                    "markdown.source.tilde_fence_unsupported",
                    "Tilde fenced blocks are not supported by the editable Markdown parser",
                    offset..offset + line.len(),
                )),
                None => fence = Some((marker, length, offset)),
                _ => {}
            }
            index += 1;
            continue;
        }
        if fence.is_some() {
            index += 1;
            continue;
        }

        if trimmed.starts_with('|') {
            let start = index;
            while index < lines.len() && lines[index].1.trim_start().starts_with('|') {
                index += 1;
            }
            if index > start + 1 {
                let table_lines = lines[start..index]
                    .iter()
                    .map(|(_, line)| *line)
                    .collect::<Vec<_>>();
                if parse_table_region(&table_lines).is_none() {
                    let end = lines[index - 1].0 + lines[index - 1].1.len();
                    diagnostics.push(MarkdownDiagnostic::source(
                        MarkdownDiagnosticSeverity::Error,
                        "markdown.source.table_fallback_unsupported",
                        "A table-like region could not be parsed safely and would fall back to paragraphs",
                        offset..end,
                    ));
                }
            }
            continue;
        }

        if trimmed.starts_with("[^") && trimmed.contains("]:") {
            diagnostics.push(MarkdownDiagnostic::source(
                MarkdownDiagnosticSeverity::Error,
                "markdown.source.footnote_unsupported",
                "Footnote definitions are SourceOnly in the first round-trip version",
                offset..offset + line.len(),
            ));
        } else if is_reference_definition(trimmed) || contains_reference_link(trimmed) {
            diagnostics.push(MarkdownDiagnostic::source(
                MarkdownDiagnosticSeverity::Error,
                "markdown.source.reference_link_unsupported",
                "Reference-style links are not supported by the editable Markdown parser",
                offset..offset + line.len(),
            ));
        } else if looks_like_raw_html(trimmed) {
            diagnostics.push(MarkdownDiagnostic::source(
                MarkdownDiagnosticSeverity::Error,
                "markdown.source.raw_html_unsupported",
                "Raw HTML is SourceOnly until its original source can be projected without fallback",
                offset..offset + line.len(),
            ));
        } else if trimmed.starts_with("$$") {
            diagnostics.push(MarkdownDiagnostic::source(
                MarkdownDiagnosticSeverity::Error,
                "markdown.source.math_unsupported",
                "Block math source is not yet supported by the editable Markdown parser",
                offset..offset + line.len(),
            ));
        } else if trimmed.starts_with(":::") || trimmed.contains("{{") {
            diagnostics.push(MarkdownDiagnostic::source(
                MarkdownDiagnosticSeverity::Error,
                "markdown.source.custom_extension_unsupported",
                "Custom Markdown extensions are SourceOnly",
                offset..offset + line.len(),
            ));
        }

        if let Some(number) = ordered_list_number(trimmed)
            && number != 1
        {
            diagnostics.push(MarkdownDiagnostic::source(
                MarkdownDiagnosticSeverity::Info,
                "markdown.source.ordered_list_normalized",
                "Ordered-list markers are normalized to 1.",
                offset..offset + line.len(),
            ));
        }
        if trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            diagnostics.push(MarkdownDiagnostic::source(
                MarkdownDiagnosticSeverity::Info,
                "markdown.source.bullet_normalized",
                "Bullet markers are normalized to -",
                offset..offset + line.len(),
            ));
        }
        if contains_underscore_emphasis(trimmed) {
            diagnostics.push(MarkdownDiagnostic::source(
                MarkdownDiagnosticSeverity::Info,
                "markdown.source.emphasis_normalized",
                "Underscore emphasis is normalized to asterisk emphasis",
                offset..offset + line.len(),
            ));
        }
        index += 1;
    }

    if let Some((_, _, start)) = fence {
        diagnostics.push(MarkdownDiagnostic::source(
            MarkdownDiagnosticSeverity::Error,
            "markdown.source.unclosed_fence",
            "An unclosed fenced block cannot be edited safely in WYSIWYG mode",
            start..markdown.len(),
        ));
    }
    if let Some(start) = math_fence {
        diagnostics.push(MarkdownDiagnostic::source(
            MarkdownDiagnosticSeverity::Error,
            "markdown.source.unclosed_math",
            "An unclosed block math expression cannot be edited safely in WYSIWYG mode",
            start..markdown.len(),
        ));
    }
    diagnostics
}

fn analyze_parsed_document_compatibility(
    document: &ParsedMarkdownDocument,
) -> Vec<MarkdownDiagnostic> {
    let mut diagnostics = Vec::new();
    for block in &document.blocks {
        let span_groups: Vec<&[InlineSpan]> = match &block.payload {
            BlockPayload::RichText { spans } => vec![spans],
            BlockPayload::Table(table) => table
                .rows
                .iter()
                .flat_map(|row| row.cells.iter().map(|cell| cell.spans.as_slice()))
                .collect(),
            _ => Vec::new(),
        };
        for spans in span_groups {
            for span in spans {
                if span.marks.iter().any(|mark| {
                    matches!(
                        mark,
                        InlineMark::Underline | InlineMark::Color(_) | InlineMark::Background(_)
                    )
                }) {
                    diagnostics.push(MarkdownDiagnostic::block(
                        MarkdownDiagnosticSeverity::Error,
                        "markdown.source.inline_mark_unsupported",
                        "The parsed source contains an inline mark that cannot be exported safely to standard Markdown",
                        block.id,
                    ));
                }
                if span.marks.contains(&InlineMark::Code) && span.marks.len() > 1 {
                    diagnostics.push(MarkdownDiagnostic::block(
                        MarkdownDiagnosticSeverity::Error,
                        "markdown.source.inline_mark_combination_unsupported",
                        "Inline code combined with other marks cannot be round-tripped safely",
                        block.id,
                    ));
                }
            }
        }
    }
    diagnostics
}

fn source_lines(markdown: &str) -> Vec<(usize, &str)> {
    let mut offset = 0;
    markdown
        .split_inclusive('\n')
        .map(|line| {
            let start = offset;
            offset += line.len();
            let line = line.strip_suffix('\n').unwrap_or(line);
            let line = line.strip_suffix('\r').unwrap_or(line);
            (start, line)
        })
        .chain((markdown.is_empty() || markdown.ends_with('\n')).then_some((markdown.len(), "")))
        .collect()
}

fn detect_frontmatter(lines: &[(usize, &str)], diagnostics: &mut Vec<MarkdownDiagnostic>) {
    if lines.first().is_none_or(|(_, line)| line.trim() != "---") {
        return;
    }
    let Some(end_index) = lines
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, (_, line))| (line.trim() == "---").then_some(index))
    else {
        return;
    };
    if !lines[1..end_index]
        .iter()
        .any(|(_, line)| line.contains(':'))
    {
        return;
    }
    let end = lines[end_index].0 + lines[end_index].1.len();
    diagnostics.push(MarkdownDiagnostic::source(
        MarkdownDiagnosticSeverity::Error,
        "markdown.source.frontmatter_unsupported",
        "Frontmatter is SourceOnly and must not be rewritten by the rich-text editor",
        0..end,
    ));
}

fn fence_marker(line: &str) -> Option<(char, usize)> {
    let marker = line.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let length = line.chars().take_while(|ch| *ch == marker).count();
    (length >= 3).then_some((marker, length))
}

fn is_reference_definition(line: &str) -> bool {
    line.starts_with('[')
        && !line.starts_with("[^")
        && line.find("]:").is_some_and(|index| index > 1)
}

fn contains_reference_link(line: &str) -> bool {
    line.contains("][") && !line.contains("](")
}

fn looks_like_raw_html(line: &str) -> bool {
    let Some(rest) = line.strip_prefix('<') else {
        return false;
    };
    if (rest.starts_with("https://") || rest.starts_with("http://")) && rest.ends_with('>') {
        return false;
    }
    rest.starts_with("!--")
        || rest.starts_with('/')
        || rest
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic())
}

fn ordered_list_number(line: &str) -> Option<u64> {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || !line[digits..].starts_with(". ") {
        return None;
    }
    line[..digits].parse().ok()
}

fn contains_underscore_emphasis(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.iter().enumerate().any(|(index, byte)| {
        if *byte != b'_' || index + 2 >= bytes.len() {
            return false;
        }
        line[index + 1..].find('_').is_some_and(|relative_end| {
            relative_end > 0 && !line[index + 1..index + 1 + relative_end].trim().is_empty()
        })
    })
}

struct MarkdownParser {
    document_id: DocumentId,
    next_block_id: BlockId,
}

impl MarkdownParser {
    fn new(options: MarkdownImportOptions) -> Self {
        Self {
            document_id: options.document_id,
            next_block_id: options.first_block_id,
        }
    }

    fn next_id(&mut self) -> BlockId {
        let id = self.next_block_id;
        self.next_block_id = self.next_block_id.saturating_add(1);
        id
    }

    fn new_block(&mut self, kind: RichBlockKind, payload: BlockPayload) -> RichBlockRecord {
        let mut block = RichBlockRecord::new(self.next_id(), kind, payload);
        block.document_id = self.document_id;
        block
    }

    fn rich_text_block(&mut self, kind: RichBlockKind, spans: Vec<InlineSpan>) -> RichBlockRecord {
        self.new_block(kind, BlockPayload::RichText { spans })
    }

    fn parse_document(&mut self, markdown: &str) -> ParsedMarkdownDocument {
        let mut document = ParsedMarkdownDocument::default();
        let lines = markdown.lines().collect::<Vec<_>>();
        let mut index = 0usize;
        let mut list_stack: Vec<(usize, BlockId)> = Vec::new();

        while index < lines.len() {
            let line = lines[index];
            if line.trim().is_empty() {
                list_stack.clear();
                index += 1;
                continue;
            }

            if line.trim() == "$$" {
                list_stack.clear();
                let start = index;
                index += 1;
                let mut content = Vec::new();
                while index < lines.len() && lines[index].trim() != "$$" {
                    content.push(lines[index]);
                    index += 1;
                }
                if index < lines.len() {
                    index += 1;
                    document.push_root_block(self.rich_text_block(
                        RichBlockKind::Math,
                        vec![InlineSpan::plain(content.join("\n"))],
                    ));
                    continue;
                }
                index = start;
            }

            if is_table_candidate_line(line) {
                let region_end = collect_table_candidate_region(&lines, index);
                if region_end > index + 1
                    && let Some(table) = parse_table_region(&lines[index..region_end])
                {
                    list_stack.clear();
                    document.push_root_block(
                        self.new_block(RichBlockKind::Table, BlockPayload::Table(table)),
                    );
                    index = region_end;
                    continue;
                }
            }

            if line.trim_start().starts_with('>') {
                let region_start = index;
                while index < lines.len() && lines[index].trim_start().starts_with('>') {
                    index += 1;
                }
                let source = lines[region_start..index].join("\n");
                if let Some(block) = self.parse_incremental_quote_or_callout_block(&source) {
                    list_stack.clear();
                    document.push_root_block(block);
                    continue;
                }
                index = region_start;
            }

            if let Some((language, fence)) = parse_fence_start(line) {
                list_stack.clear();
                let mut content = String::new();
                index += 1;
                while index < lines.len() {
                    let code_line = lines[index];
                    if is_closing_fence(code_line, fence) {
                        index += 1;
                        break;
                    }
                    content.push_str(code_line);
                    content.push('\n');
                    index += 1;
                }
                if content.ends_with('\n') {
                    content.pop();
                }
                let block = if language.as_deref() == Some("mermaid") {
                    self.rich_text_block(RichBlockKind::Mermaid, vec![InlineSpan::plain(content)])
                } else {
                    self.new_block(
                        RichBlockKind::Code {
                            language: language.clone(),
                        },
                        BlockPayload::Code {
                            language,
                            text: content,
                        },
                    )
                };
                document.push_root_block(block);
                continue;
            }

            if let Some((indent, mut block)) = self.parse_list_line(line) {
                push_markdown_list_block(&mut document, &mut list_stack, indent, &mut block);
            } else if is_plain_paragraph_line(line) {
                list_stack.clear();
                let paragraph_start = index;
                index += 1;
                while index < lines.len() && is_plain_paragraph_line(lines[index]) {
                    index += 1;
                }
                let source = lines[paragraph_start..index]
                    .iter()
                    .map(|line| line.trim_start())
                    .collect::<Vec<_>>()
                    .join("\n");
                document.push_root_block(
                    self.rich_text_block(RichBlockKind::Paragraph, parse_inline_markdown(&source)),
                );
                continue;
            } else {
                list_stack.clear();
                let block = self.parse_markdown_line(line);
                document.push_root_block(block);
            }
            index += 1;
        }

        if document.blocks.is_empty() {
            document.push_root_block(self.rich_text_block(
                RichBlockKind::Paragraph,
                vec![InlineSpan::plain(String::new())],
            ));
        }

        document
    }

    fn parse_incremental_multiline_block(&mut self, markdown: &str) -> Option<RichBlockRecord> {
        self.parse_math_block(markdown)
            .or_else(|| self.parse_fenced_code_block(markdown))
            .or_else(|| self.parse_incremental_table_block(markdown))
            .or_else(|| self.parse_incremental_quote_or_callout_block(markdown))
    }

    fn parse_math_block(&mut self, markdown: &str) -> Option<RichBlockRecord> {
        let lines = markdown.lines().collect::<Vec<_>>();
        if lines.len() < 3 || lines.first()?.trim() != "$$" || lines.last()?.trim() != "$$" {
            return None;
        }
        Some(self.rich_text_block(
            RichBlockKind::Math,
            vec![InlineSpan::plain(lines[1..lines.len() - 1].join("\n"))],
        ))
    }

    fn parse_fenced_code_block(&mut self, markdown: &str) -> Option<RichBlockRecord> {
        let lines = markdown.lines().collect::<Vec<_>>();
        if lines.len() < 2 {
            return None;
        }
        let (language, fence) = parse_fence_start(lines.first()?)?;
        if !is_closing_fence(lines.last()?, fence) {
            return None;
        }
        let text = lines[1..lines.len() - 1].join("\n");
        if language.as_deref() == Some("mermaid") {
            Some(self.rich_text_block(RichBlockKind::Mermaid, vec![InlineSpan::plain(text)]))
        } else {
            Some(self.new_block(
                RichBlockKind::Code {
                    language: language.clone(),
                },
                BlockPayload::Code { language, text },
            ))
        }
    }

    fn parse_incremental_table_block(&mut self, markdown: &str) -> Option<RichBlockRecord> {
        let lines = markdown.lines().collect::<Vec<_>>();
        if lines.len() < 2 || !lines.iter().all(|line| is_table_candidate_line(line)) {
            return None;
        }
        let table = parse_table_region(&lines)?;
        Some(self.new_block(RichBlockKind::Table, BlockPayload::Table(table)))
    }

    fn parse_incremental_quote_or_callout_block(
        &mut self,
        markdown: &str,
    ) -> Option<RichBlockRecord> {
        let mut lines = Vec::new();
        for line in markdown.lines() {
            let trimmed = line.trim_start();
            let quote_line = trimmed.strip_prefix('>')?;
            lines.push(
                quote_line
                    .strip_prefix(' ')
                    .unwrap_or(quote_line)
                    .to_string(),
            );
        }
        if lines.is_empty() {
            return None;
        }

        if let Some(variant) = lines.first().and_then(|line| parse_callout_marker(line)) {
            let text = lines.into_iter().skip(1).collect::<Vec<_>>().join("\n");
            return Some(self.rich_text_block(
                RichBlockKind::Callout { variant },
                parse_inline_markdown(&text),
            ));
        }

        Some(self.rich_text_block(
            RichBlockKind::Quote,
            parse_inline_markdown(&lines.join("\n")),
        ))
    }

    fn parse_list_line(&mut self, line: &str) -> Option<(usize, RichBlockRecord)> {
        let indent = line
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .map(|ch| if ch == '\t' { 4 } else { 1 })
            .sum::<usize>();
        let trimmed = line.trim_start();

        if let Some(text) = trimmed.strip_prefix("- [ ] ") {
            return Some((
                indent,
                self.rich_text_block(
                    RichBlockKind::Todo { checked: false },
                    parse_inline_markdown(text.trim_start()),
                ),
            ));
        }
        if let Some(text) = trimmed
            .strip_prefix("- [x] ")
            .or_else(|| trimmed.strip_prefix("- [X] "))
        {
            return Some((
                indent,
                self.rich_text_block(
                    RichBlockKind::Todo { checked: true },
                    parse_inline_markdown(text.trim_start()),
                ),
            ));
        }
        if let Some(text) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
        {
            return Some((
                indent,
                self.rich_text_block(
                    RichBlockKind::BulletedList,
                    parse_inline_markdown(text.trim_start()),
                ),
            ));
        }
        if let Some(text) = parse_numbered_item(trimmed) {
            return Some((
                indent,
                self.rich_text_block(
                    RichBlockKind::NumberedList,
                    parse_inline_markdown(text.trim_start()),
                ),
            ));
        }

        None
    }

    fn parse_markdown_line(&mut self, line: &str) -> RichBlockRecord {
        let trimmed = line.trim_start();
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            return self.new_block(RichBlockKind::Separator, BlockPayload::Empty);
        }

        if let Some((level, text)) = parse_heading(trimmed) {
            return self.rich_text_block(
                RichBlockKind::Heading { level },
                parse_inline_markdown(text),
            );
        }

        if let Some((_, block)) = self.parse_list_line(trimmed) {
            return block;
        }
        if let Some(text) = trimmed.strip_prefix("> ") {
            return self.rich_text_block(RichBlockKind::Quote, parse_inline_markdown(text));
        }
        if let Some((alt, source)) = parse_block_image(trimmed) {
            return self.new_block(
                RichBlockKind::Image,
                BlockPayload::Image(ImagePayload {
                    source,
                    alt,
                    caption: String::new(),
                    display_width_ratio_milli: None,
                }),
            );
        }

        self.rich_text_block(RichBlockKind::Paragraph, parse_inline_markdown(trimmed))
    }
}

fn is_plain_paragraph_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty()
        || trimmed == "---"
        || trimmed == "***"
        || trimmed == "___"
        || trimmed.starts_with('>')
        || parse_fence_start(trimmed).is_some()
        || parse_heading(trimmed).is_some()
        || parse_block_image(trimmed).is_some()
        || is_table_candidate_line(line)
        || parse_numbered_item(trimmed).is_some()
    {
        return false;
    }

    !trimmed.starts_with("- [ ] ")
        && !trimmed.starts_with("- [x] ")
        && !trimmed.starts_with("- [X] ")
        && !trimmed.starts_with("[ ] ")
        && !trimmed.starts_with("[x] ")
        && !trimmed.starts_with("[X] ")
        && !trimmed.starts_with("- ")
        && !trimmed.starts_with("* ")
        && !trimmed.starts_with("+ ")
}

fn push_markdown_list_block(
    document: &mut ParsedMarkdownDocument,
    list_stack: &mut Vec<(usize, BlockId)>,
    indent: usize,
    block: &mut RichBlockRecord,
) {
    while list_stack
        .last()
        .is_some_and(|(stack_indent, _)| *stack_indent >= indent)
    {
        list_stack.pop();
    }

    if let Some((_, parent_id)) = list_stack.last().copied() {
        block.parent_id = Some(parent_id);
        block.depth = list_stack.len() as u16;
        if let Some(parent) = document
            .blocks
            .iter_mut()
            .find(|block| block.id == parent_id)
        {
            parent.children.push(block.id);
        } else {
            document.root_blocks.push(block.id);
        }
    } else {
        block.depth = 0;
        document.root_blocks.push(block.id);
    }

    let block_id = block.id;
    document.blocks.push(block.clone());
    list_stack.push((indent, block_id));
}

fn is_closing_fence(line: &str, opening_fence: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= opening_fence.len()
        && trimmed.bytes().all(|byte| byte == b'`')
        && trimmed.starts_with(opening_fence)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
