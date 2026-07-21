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
pub use inline::{InlineMediaFragment, parse_inline_media_fragments};
pub use inline_export::{InlineMarkdownExport, export_inline_spans};
mod inline;
mod table;

use block::{
    block_kind_for_marker, looks_like_single_block_markdown, parse_fence_start, parse_heading,
    parse_numbered_item,
};
use export::block_to_plain_markdown;
use inline::{
    parse_block_image, parse_inline_markdown, parse_inline_markdown_extended,
    parse_inline_markdown_with_images, parse_linked_markdown_image,
};
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
    let round_trip = parsed_document_for_export(&document, options.document_id);
    let exported = export_document_blocks(&round_trip, MarkdownExportMode::BestEffort);
    if exported.markdown != markdown {
        diagnostics.push(MarkdownDiagnostic::source(
            MarkdownDiagnosticSeverity::Error,
            "markdown.source.non_lossless_round_trip",
            "Rich-text editing would rewrite Markdown syntax; preserving source is required",
            0..markdown.len(),
        ));
    }
    let compatibility = MarkdownCompatibility::from_diagnostics(&diagnostics);
    MarkdownParseResult {
        document,
        compatibility,
        diagnostics,
    }
}

fn parsed_document_for_export(
    parsed: &ParsedMarkdownDocument,
    document_id: DocumentId,
) -> RichTextDocument {
    RichTextDocument {
        id: document_id,
        version: super::document::CURRENT_RICH_TEXT_FORMAT_VERSION,
        metadata: Default::default(),
        root_blocks: parsed.root_blocks.clone(),
        blocks: parsed.blocks.clone(),
        structure_version: 1,
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
            || parse_escaped_fence_start(trimmed).is_some()
            || trimmed.starts_with('|')
            || trimmed == "---"
            || trimmed == "***"
            || trimmed == "___"
            || trimmed == "$$"
            || standalone_math_source(trimmed).is_some()
            || math_environment_name(trimmed).is_some()
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
                None if marker == '~' => {
                    diagnostics.push(MarkdownDiagnostic::source(
                        MarkdownDiagnosticSeverity::Info,
                        "markdown.source.tilde_fence_preserved",
                        "Tilde fenced blocks are preserved as editable raw Markdown blocks",
                        offset..offset + line.len(),
                    ));
                    fence = Some((marker, length, offset));
                }
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
            for (table_offset, table_line) in &lines[start..index] {
                detect_linked_images(table_line.trim_start(), *table_offset, &mut diagnostics);
            }
            if index > start + 1 {
                let table_lines = lines[start..index]
                    .iter()
                    .map(|(_, line)| *line)
                    .collect::<Vec<_>>();
                if parse_table_region(&table_lines).is_none() {
                    let end = lines[index - 1].0 + lines[index - 1].1.len();
                    diagnostics.push(MarkdownDiagnostic::source(
                        MarkdownDiagnosticSeverity::Info,
                        "markdown.source.table_fallback_preserved",
                        "An unrecognized table-like region is preserved as an editable raw Markdown block",
                        offset..end,
                    ));
                }
            }
            continue;
        }

        detect_linked_images(trimmed, offset, &mut diagnostics);

        if trimmed.starts_with("[^") && trimmed.contains("]:") {
            diagnostics.push(MarkdownDiagnostic::source(
                MarkdownDiagnosticSeverity::Info,
                "markdown.source.footnote_preserved",
                "Footnote definitions are preserved as editable raw Markdown blocks",
                offset..offset + line.len(),
            ));
        } else if is_reference_definition(trimmed) || contains_reference_link(trimmed) {
            diagnostics.push(MarkdownDiagnostic::source(
                MarkdownDiagnosticSeverity::Info,
                "markdown.source.reference_link_preserved",
                "Reference-style links are preserved as editable raw Markdown blocks",
                offset..offset + line.len(),
            ));
        } else if trimmed.starts_with("$$") {
            diagnostics.push(MarkdownDiagnostic::source(
                MarkdownDiagnosticSeverity::Info,
                "markdown.source.math_preserved",
                "Block math source is preserved by the editable Markdown parser",
                offset..offset + line.len(),
            ));
        } else if trimmed.starts_with(":::") || trimmed.contains("{{") {
            diagnostics.push(MarkdownDiagnostic::source(
                MarkdownDiagnosticSeverity::Info,
                "markdown.source.custom_extension_preserved",
                "Custom Markdown extensions are preserved as editable raw Markdown blocks",
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
            MarkdownDiagnosticSeverity::Info,
            "markdown.source.unclosed_fence_editing",
            "An incomplete fenced block is kept as editable raw Markdown until it is closed",
            start..markdown.len(),
        ));
    }
    if let Some(start) = math_fence {
        diagnostics.push(MarkdownDiagnostic::source(
            MarkdownDiagnosticSeverity::Info,
            "markdown.source.unclosed_math_editing",
            "An incomplete math block is kept as editable raw Markdown until it is closed",
            start..markdown.len(),
        ));
    }
    diagnostics
}

fn detect_linked_images(line: &str, line_offset: usize, diagnostics: &mut Vec<MarkdownDiagnostic>) {
    let mut cursor = 0usize;
    while let Some(relative_start) = line[cursor..].find("[![") {
        let start = cursor + relative_start;
        let Some((_alt, source, link, consumed)) = parse_linked_markdown_image(&line[start..])
        else {
            cursor = start + 3;
            continue;
        };
        let (severity, code, message) = if source == link {
            (
                MarkdownDiagnosticSeverity::Info,
                "markdown.source.self_linked_image_normalized",
                "An image linked to itself is normalized to a plain Markdown image",
            )
        } else {
            (
                MarkdownDiagnosticSeverity::Info,
                "markdown.source.linked_image_preserved",
                "Images linked to a different destination are preserved as inline Markdown",
            )
        };
        diagnostics.push(MarkdownDiagnostic::source(
            severity,
            code,
            message,
            line_offset + start..line_offset + start + consumed,
        ));
        cursor = start + consumed;
    }
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
        MarkdownDiagnosticSeverity::Info,
        "markdown.source.frontmatter_preserved",
        "Frontmatter is preserved as an editable raw Markdown block",
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

fn is_footnote_definition(line: &str) -> bool {
    line.starts_with("[^") && line.contains("]:")
}

fn frontmatter_region_end(lines: &[&str]) -> Option<usize> {
    if lines.first().is_none_or(|line| line.trim() != "---") {
        return None;
    }
    let mut end = lines
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, line)| (line.trim() == "---").then_some(index + 1))?;
    if !lines[1..end - 1].iter().any(|line| line.contains(':')) {
        return None;
    }
    while end < lines.len() && !lines[end].trim().is_empty() {
        end += 1;
    }
    Some(end)
}

fn tilde_fence_region_end(lines: &[&str], start: usize) -> Option<usize> {
    let opening = lines.get(start)?.trim_start();
    let length = opening.chars().take_while(|ch| *ch == '~').count();
    if length < 3 {
        return None;
    }
    lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find_map(|(index, line)| {
            let trimmed = line.trim();
            (trimmed.chars().take_while(|ch| *ch == '~').count() >= length
                && trimmed.chars().all(|ch| ch == '~'))
            .then_some(index + 1)
        })
}

fn custom_container_region_end(lines: &[&str], start: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find_map(|(index, line)| (line.trim() == ":::").then_some(index + 1))
        .unwrap_or(start + 1)
}

fn parse_escaped_fence_start(line: &str) -> Option<&str> {
    let escaped = line.trim_start().strip_prefix('\\')?;
    let (_, fence) = parse_fence_start(escaped)?;
    Some(fence)
}

fn escaped_fence_region_end(lines: &[&str], start: usize) -> Option<usize> {
    let opening_fence = parse_escaped_fence_start(lines.get(start)?)?;
    Some(
        lines
            .iter()
            .enumerate()
            .skip(start + 1)
            .find_map(|(index, line)| {
                let escaped = line.trim().strip_prefix('\\')?;
                is_closing_fence(escaped, opening_fence).then_some(index + 1)
            })
            .unwrap_or(lines.len()),
    )
}

fn source_requires_raw_markdown_block(source: &str) -> bool {
    contains_reference_link(source)
        || source.contains("{{")
        || contains_unescaped_token(source, "++")
}

fn contains_unescaped_token(source: &str, token: &str) -> bool {
    let mut search = 0usize;
    while let Some(relative) = source[search..].find(token) {
        let position = search + relative;
        let backslashes = source[..position]
            .bytes()
            .rev()
            .take_while(|byte| *byte == b'\\')
            .count();
        if backslashes % 2 == 0 {
            return true;
        }
        search = position + token.len();
    }
    false
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

fn collect_raw_html_region(lines: &[&str], start: usize) -> usize {
    let line = lines[start].trim_start();
    let Some(tag) = raw_html_opening_tag(line) else {
        return start + 1;
    };
    let closing = format!("</{tag}");
    if line.trim_end().ends_with("/>") || line.to_ascii_lowercase().contains(&closing) {
        return start + 1;
    }
    lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find_map(|(index, line)| {
            line.trim_start()
                .to_ascii_lowercase()
                .contains(&closing)
                .then_some(index + 1)
        })
        .unwrap_or(start + 1)
}

fn raw_html_opening_tag(line: &str) -> Option<String> {
    let rest = line.strip_prefix('<')?;
    if rest.starts_with('/') || rest.starts_with('!') || rest.starts_with('?') {
        return None;
    }
    let tag = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect::<String>();
    (!tag.is_empty()).then(|| tag.to_ascii_lowercase())
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

    fn raw_markdown_block(&mut self, source: String) -> RichBlockRecord {
        let mut block = self.new_block(
            RichBlockKind::RawMarkdown,
            BlockPayload::RichText {
                spans: vec![InlineSpan::plain(source.clone())],
            },
        );
        block.raw_fallback = Some(source);
        block
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

            if index == 0
                && line.trim() == "---"
                && let Some(end) = frontmatter_region_end(&lines)
            {
                list_stack.clear();
                let source = lines[..end].join("\n");
                document.push_root_block(self.raw_markdown_block(source));
                index = end;
                continue;
            }

            if let Some(end) = escaped_fence_region_end(&lines, index) {
                list_stack.clear();
                document.push_root_block(self.raw_markdown_block(lines[index..end].join("\n")));
                index = end;
                continue;
            }

            if let Some(end) = tilde_fence_region_end(&lines, index) {
                list_stack.clear();
                let source = lines[index..end].join("\n");
                document.push_root_block(self.raw_markdown_block(source));
                index = end;
                continue;
            }
            if line
                .trim_start()
                .chars()
                .take_while(|ch| *ch == '~')
                .count()
                >= 3
            {
                list_stack.clear();
                document.push_root_block(self.raw_markdown_block(lines[index..].join("\n")));
                break;
            }

            if is_footnote_definition(line.trim_start())
                || is_reference_definition(line.trim_start())
            {
                list_stack.clear();
                document.push_root_block(self.raw_markdown_block(line.to_owned()));
                index += 1;
                continue;
            }

            if line.trim_start().starts_with(":::") {
                list_stack.clear();
                let end = custom_container_region_end(&lines, index);
                document.push_root_block(self.raw_markdown_block(lines[index..end].join("\n")));
                index = end;
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
                document.push_root_block(self.raw_markdown_block(lines[start..].join("\n")));
                break;
            }

            if let Some(environment) = math_environment_name(line) {
                list_stack.clear();
                let start = index;
                let closing = format!("\\end{{{environment}}}");
                index += 1;
                while index < lines.len() && lines[index].trim() != closing {
                    index += 1;
                }
                if index < lines.len() {
                    index += 1;
                    document.push_root_block(self.rich_text_block(
                        RichBlockKind::Math,
                        vec![InlineSpan::plain(lines[start..index].join("\n"))],
                    ));
                    continue;
                }
                document.push_root_block(self.raw_markdown_block(lines[start..].join("\n")));
                break;
            }

            if let Some(source) = standalone_math_source(line) {
                list_stack.clear();
                document.push_root_block(
                    self.rich_text_block(RichBlockKind::Math, vec![InlineSpan::plain(source)]),
                );
                index += 1;
                continue;
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
                if region_end > index + 1 {
                    list_stack.clear();
                    document.push_root_block(
                        self.raw_markdown_block(lines[index..region_end].join("\n")),
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
                let region_start = index;
                let mut content = String::new();
                index += 1;
                let mut closed = false;
                while index < lines.len() {
                    let code_line = lines[index];
                    if is_closing_fence(code_line, fence) {
                        index += 1;
                        closed = true;
                        break;
                    }
                    content.push_str(code_line);
                    content.push('\n');
                    index += 1;
                }
                if !closed {
                    document
                        .push_root_block(self.raw_markdown_block(lines[region_start..].join("\n")));
                    break;
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

            if looks_like_raw_html(line.trim_start()) {
                list_stack.clear();
                let region_start = index;
                index = collect_raw_html_region(&lines, index);
                let source = lines[region_start..index].join("\n");
                document.push_root_block(self.new_block(
                    RichBlockKind::Html,
                    BlockPayload::Html {
                        html: source,
                        sanitized: false,
                    },
                ));
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
                let spans = parse_inline_markdown(&source);
                if source_requires_raw_markdown_block(&source)
                    || spans
                        .iter()
                        .any(|span| span.marks.contains(&InlineMark::Code) && span.marks.len() > 1)
                {
                    document.push_root_block(self.raw_markdown_block(source));
                } else {
                    document.push_root_block(self.rich_text_block(RichBlockKind::Paragraph, spans));
                }
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
        self.parse_escaped_fenced_block(markdown)
            .or_else(|| self.parse_math_block(markdown))
            .or_else(|| self.parse_fenced_code_block(markdown))
            .or_else(|| self.parse_incremental_table_block(markdown))
            .or_else(|| self.parse_incremental_quote_or_callout_block(markdown))
    }

    fn parse_escaped_fenced_block(&mut self, markdown: &str) -> Option<RichBlockRecord> {
        let lines = markdown.lines().collect::<Vec<_>>();
        if lines.len() < 2 {
            return None;
        }
        let opening_fence = parse_escaped_fence_start(lines.first()?)?;
        let escaped_closing = lines.last()?.trim().strip_prefix('\\')?;
        is_closing_fence(escaped_closing, opening_fence)
            .then(|| self.raw_markdown_block(markdown.to_owned()))
    }

    fn parse_math_block(&mut self, markdown: &str) -> Option<RichBlockRecord> {
        let lines = markdown.lines().collect::<Vec<_>>();
        if lines.len() == 1 {
            return standalone_math_source(lines[0]).map(|source| {
                self.rich_text_block(RichBlockKind::Math, vec![InlineSpan::plain(source)])
            });
        }
        if let Some(environment) = math_environment_name(lines.first()?) {
            let closing = format!("\\end{{{environment}}}");
            if lines.last()?.trim() == closing {
                return Some(self.rich_text_block(
                    RichBlockKind::Math,
                    vec![InlineSpan::plain(markdown.to_owned())],
                ));
            }
            return None;
        }
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

fn standalone_math_source(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let source = trimmed
        .strip_prefix("$$")
        .and_then(|value| value.strip_suffix("$$"))
        .or_else(|| {
            trimmed
                .strip_prefix("\\[")
                .and_then(|value| value.strip_suffix("\\]"))
        })
        .or_else(|| {
            trimmed
                .strip_prefix('$')
                .and_then(|value| value.strip_suffix('$'))
        })?
        .trim();
    (!source.is_empty()).then(|| source.to_owned())
}

fn math_environment_name(line: &str) -> Option<&str> {
    const ENVIRONMENTS: &[&str] = &[
        "align",
        "align*",
        "aligned",
        "alignat",
        "alignat*",
        "alignedat",
        "gather",
        "gather*",
        "gathered",
        "equation",
        "equation*",
        "split",
        "cases",
        "array",
        "matrix",
        "matrix*",
        "pmatrix",
        "pmatrix*",
        "bmatrix",
        "bmatrix*",
        "Bmatrix",
        "Bmatrix*",
        "vmatrix",
        "vmatrix*",
        "Vmatrix",
        "Vmatrix*",
        "smallmatrix",
        "subarray",
    ];
    let trimmed = line.trim();
    let name = trimmed.strip_prefix("\\begin{")?.strip_suffix('}')?;
    ENVIRONMENTS.contains(&name).then_some(name)
}

fn is_plain_paragraph_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty()
        || trimmed == "---"
        || trimmed == "***"
        || trimmed == "___"
        || trimmed.starts_with('>')
        || trimmed == "$$"
        || standalone_math_source(trimmed).is_some()
        || math_environment_name(trimmed).is_some()
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
