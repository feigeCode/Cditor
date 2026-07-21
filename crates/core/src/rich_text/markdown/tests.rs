use super::*;

#[test]
fn records_markdown_parse_stats_like_v1() {
    MARKDOWN_PARSE_STATS.reset();

    let _ = parse_markdown_document("# Title", MarkdownImportOptions::default());
    let _ = import_markdown_block_incremental("- item", MarkdownImportOptions::default());

    let snapshot = MARKDOWN_PARSE_STATS.snapshot();
    assert!(snapshot.full_parse_count >= 1);
    assert!(snapshot.full_parse_chars >= "# Title".len() as u64);
    assert!(snapshot.incremental_parse_count >= 1);
    assert!(snapshot.incremental_parse_chars >= "- item".len() as u64);
}

#[test]
fn markdown_paste_detection_includes_inline_syntax() {
    assert!(looks_like_markdown_paste("**bold** and `code`"));
    assert!(looks_like_markdown_paste("[link](https://example.com)"));
    assert!(looks_like_markdown_paste("$$\nE = mc^2\n$$"));
    assert!(looks_like_markdown_paste("行间公式：\n$$\nE = mc^2\n$$"));
    assert!(looks_like_markdown_paste(
        "\\```mermaid\nflowchart TD\nA --> B\n\\```"
    ));
    assert!(!looks_like_markdown_paste("plain text"));
}

#[test]
fn outer_markdown_fence_is_unwrapped_before_block_parsing() {
    let parsed = parse_markdown_document(
        "```markdown\n# Title\n- item\n```",
        MarkdownImportOptions::default(),
    );

    assert!(matches!(
        parsed.blocks.first().map(|block| &block.kind),
        Some(RichBlockKind::Heading { level: 1 })
    ));
    assert!(matches!(
        parsed.blocks.get(1).map(|block| &block.kind),
        Some(RichBlockKind::BulletedList)
    ));
}

#[test]
fn escaped_fenced_blocks_remain_verbatim_and_do_not_become_renderable_blocks() {
    for source in [
        "\\```mermaid\nflowchart TD\nA --> B\n\\```",
        "\\```math\nx^2 + y^2\n\\```",
    ] {
        let parsed = parse_markdown_document(source, MarkdownImportOptions::default());

        assert_eq!(1, parsed.blocks.len(), "source={source:?}");
        assert_eq!(RichBlockKind::RawMarkdown, parsed.blocks[0].kind);
        assert_eq!(source, parsed.blocks[0].payload.plain_text());

        let incremental =
            import_markdown_block_incremental(source, MarkdownImportOptions::default())
                .expect("escaped fenced paste should import as one raw block");
        assert_eq!(RichBlockKind::RawMarkdown, incremental.kind);
        assert_eq!(source, incremental.payload.plain_text());

        let exported =
            export_document_blocks(&document_from_parsed(parsed), MarkdownExportMode::Strict);
        assert_eq!(source, exported.markdown);
    }
}

#[test]
fn unescaped_renderable_fences_keep_their_existing_block_kinds() {
    let mermaid = parse_markdown_document(
        "```mermaid\nflowchart TD\nA --> B\n```",
        MarkdownImportOptions::default(),
    );
    assert_eq!(RichBlockKind::Mermaid, mermaid.blocks[0].kind);

    let math = parse_markdown_document("```math\nx^2 + y^2\n```", MarkdownImportOptions::default());
    assert!(matches!(
        math.blocks[0].kind,
        RichBlockKind::Code {
            language: Some(ref language)
        } if language == "math"
    ));
}

#[test]
fn raw_html_is_kept_as_a_renderable_html_block() {
    let source =
        "<div align=\"center\">\n<p><strong>Navop</strong></p>\n\n<p>Native preview</p>\n</div>";
    let result = parse_markdown_document_with_report(source, MarkdownImportOptions::default());

    assert!(matches!(
        result.compatibility,
        MarkdownCompatibility::Editable
    ));
    assert_eq!(1, result.document.blocks.len());
    assert_eq!(RichBlockKind::Html, result.document.blocks[0].kind);
    assert_eq!(source, result.document.blocks[0].payload.plain_text());

    let document = document_from_parsed(result.document);
    let exported = export_document_blocks(&document, MarkdownExportMode::Strict);
    assert_eq!(source, exported.markdown);
}

#[test]
fn sibling_html_regions_remain_independent_typora_style_blocks() {
    let source = concat!(
        "<section>\n<strong>first</strong>\n</section>\n",
        "\n<aside>\nsecond\n</aside>"
    );
    let result = parse_markdown_document_with_report(source, MarkdownImportOptions::default());

    assert!(matches!(
        result.compatibility,
        MarkdownCompatibility::Editable
    ));
    assert_eq!(2, result.document.blocks.len());
    assert!(
        result
            .document
            .blocks
            .iter()
            .all(|block| block.kind == RichBlockKind::Html)
    );
    assert_eq!(
        "<section>\n<strong>first</strong>\n</section>",
        result.document.blocks[0].payload.plain_text()
    );
    assert_eq!(
        "<aside>\nsecond\n</aside>",
        result.document.blocks[1].payload.plain_text()
    );

    let document = document_from_parsed(result.document);
    let exported = export_document_blocks(&document, MarkdownExportMode::Strict);
    assert_eq!(source, exported.markdown);
}

#[test]
fn markdown_table_cells_preserve_images_as_media() {
    let source = "| Logo |\n| --- |\n| ![Navop](https://example.com/navop.png) |";
    let result = parse_markdown_document_with_report(source, MarkdownImportOptions::default());
    let BlockPayload::Table(table) = &result.document.blocks[0].payload else {
        panic!("expected table payload");
    };
    let cell = &table.rows[1].cells[0];

    assert_eq!(1, cell.images.len());
    assert_eq!("Navop", cell.images[0].alt);
    assert_eq!("https://example.com/navop.png", cell.images[0].source);
    assert!(cell.spans.iter().all(|span| span.text.trim().is_empty()));
    let images = cell.images.clone();

    let document = document_from_parsed(result.document);
    let exported = export_document_blocks(&document, MarkdownExportMode::Strict);
    assert!(
        exported
            .markdown
            .contains("![Navop](<https://example.com/navop.png>)")
    );
    let reparsed = parse_markdown_document(&exported.markdown, MarkdownImportOptions::default());
    let BlockPayload::Table(reparsed_table) = &reparsed.blocks[0].payload else {
        panic!("expected reparsed table payload");
    };
    assert_eq!(reparsed_table.rows[1].cells[0].images, images);
}

#[test]
fn syntax_that_would_be_escaped_or_normalized_is_source_only() {
    for source in [
        "> <https://example.com/path_(item)>",
        "Use snake_case(value)",
        "2. second",
        "| Image |\n| --- |\n| [![Database](database.png)](database.png) |",
    ] {
        let result = parse_markdown_document_with_report(source, MarkdownImportOptions::default());
        assert!(
            matches!(result.compatibility, MarkdownCompatibility::SourceOnly(_)),
            "source must remain byte-stable: {source:?}"
        );
    }
}

#[test]
fn markdown_table_cells_render_plain_relative_images() {
    let source =
        "| Database | SSH |\n| --- | --- |\n| ![Database](database.png) | ![SSH](ssh.png) |";
    let result = parse_markdown_document_with_report(source, MarkdownImportOptions::default());
    let BlockPayload::Table(table) = &result.document.blocks[0].payload else {
        panic!("expected table payload");
    };

    assert_eq!(table.rows[1].cells[0].images.len(), 1);
    assert_eq!(table.rows[1].cells[0].images[0].alt, "Database");
    assert_eq!(table.rows[1].cells[0].images[0].source, "database.png");
    assert_eq!(table.rows[1].cells[1].images.len(), 1);
    assert_eq!(table.rows[1].cells[1].images[0].source, "ssh.png");
    assert!(
        table.rows[1]
            .cells
            .iter()
            .all(|cell| { cell.spans.iter().all(|span| span.text.trim().is_empty()) })
    );
}

#[test]
fn markdown_table_cells_render_self_linked_relative_images() {
    let source = "| Database |\n| --- |\n| [![Database](database.png)](database.png) |";
    let result = parse_markdown_document_with_report(source, MarkdownImportOptions::default());
    assert!(matches!(
        result.compatibility,
        MarkdownCompatibility::SourceOnly(_)
    ));
    let BlockPayload::Table(table) = &result.document.blocks[0].payload else {
        panic!("expected table payload");
    };
    let cell = &table.rows[1].cells[0];
    assert_eq!(1, cell.images.len());
    assert_eq!("Database", cell.images[0].alt);
    assert_eq!("database.png", cell.images[0].source);
    assert!(cell.spans.iter().all(|span| span.text.trim().is_empty()));

    let document = document_from_parsed(result.document);
    let exported = export_document_blocks(&document, MarkdownExportMode::Strict);
    assert!(exported.markdown.contains("![Database](<database.png>)"));
    assert!(!exported.markdown.contains("[![Database]"));
}

#[test]
fn linked_images_with_different_targets_remain_editable_and_round_trip() {
    let source = "[![Docs](preview.png)](https://example.com/docs)";
    let result = parse_markdown_document_with_report(source, MarkdownImportOptions::default());
    assert!(matches!(
        result.compatibility,
        MarkdownCompatibility::Editable
    ));
    let BlockPayload::RichText { spans } = &result.document.blocks[0].payload else {
        panic!("expected rich text payload");
    };
    assert_eq!(source, crate::rich_text::plain_text_from_spans(spans));
    let document = document_from_parsed(result.document);
    let exported = export_document_blocks(&document, MarkdownExportMode::Strict);
    assert!(
        exported
            .markdown
            .contains("[![Docs](preview.png)](https://example.com/docs)")
    );
}

#[test]
fn markdown_table_cells_preserve_inline_marks_links_and_images() {
    let source = concat!(
        "| Content |\n",
        "| :--- |\n",
        "| **Bold** *italic* ~~gone~~ `code` [link](https://example.com) ![Badge](https://example.com/badge.svg) |"
    );
    let result = parse_markdown_document_with_report(source, MarkdownImportOptions::default());
    let BlockPayload::Table(table) = &result.document.blocks[0].payload else {
        panic!("expected table payload");
    };
    let cell = &table.rows[1].cells[0];

    assert!(
        cell.spans
            .iter()
            .any(|span| { span.text == "Bold" && span.marks.contains(&InlineMark::Bold) })
    );
    assert!(
        cell.spans
            .iter()
            .any(|span| { span.text == "italic" && span.marks.contains(&InlineMark::Italic) })
    );
    assert!(
        cell.spans
            .iter()
            .any(|span| { span.text == "gone" && span.marks.contains(&InlineMark::Strike) })
    );
    assert!(
        cell.spans
            .iter()
            .any(|span| { span.text == "code" && span.marks.contains(&InlineMark::Code) })
    );
    assert!(cell.spans.iter().any(|span| {
        span.text == "link"
            && matches!(span.marks.as_slice(), [InlineMark::Link { href }] if href == "https://example.com")
    }));
    assert_eq!(cell.images.len(), 1);
    let expected_cell = cell.clone();

    let document = document_from_parsed(result.document);
    let exported = export_document_blocks(&document, MarkdownExportMode::Strict);
    let reparsed = parse_markdown_document(&exported.markdown, MarkdownImportOptions::default());
    let BlockPayload::Table(reparsed_table) = &reparsed.blocks[0].payload else {
        panic!("expected reparsed table payload");
    };
    assert_eq!(reparsed_table.rows[1].cells[0], expected_cell);
}

#[test]
fn block_math_imports_and_round_trips_as_an_editable_math_block() {
    let source = "$$\n\\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}\n$$";
    let result = parse_markdown_document_with_report(source, MarkdownImportOptions::default());

    assert_eq!(MarkdownCompatibility::Editable, result.compatibility);
    assert_eq!(1, result.document.blocks.len());
    assert!(matches!(
        result.document.blocks[0].kind,
        RichBlockKind::Math
    ));
    assert_eq!(
        "\\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}",
        result.document.blocks[0].payload.plain_text()
    );
    let exported = export_document_blocks(
        &document_from_parsed(result.document),
        MarkdownExportMode::Strict,
    );
    assert_eq!(source, exported.markdown);
}

#[test]
fn standalone_math_delimiters_import_as_math_blocks() {
    for source in ["$$E = mc^2$$", "$E = mc^2$", r"\[E = mc^2\]"] {
        let document = parse_markdown_document(source, MarkdownImportOptions::default());
        assert_eq!(1, document.blocks.len(), "source={source}");
        assert_eq!(RichBlockKind::Math, document.blocks[0].kind);
        assert_eq!("E = mc^2", document.blocks[0].payload.plain_text());
    }
}

#[test]
fn align_environment_imports_as_an_editable_math_block() {
    let source = "\\begin{align}\ny &= 2x + 1 \\\\\nz &= x^2 - 3\n\\end{align}";

    assert!(looks_like_markdown_paste(source));
    let document = parse_markdown_document(source, MarkdownImportOptions::default());

    assert_eq!(1, document.blocks.len());
    assert_eq!(RichBlockKind::Math, document.blocks[0].kind);
    assert_eq!(source, document.blocks[0].payload.plain_text());
}

#[test]
fn common_ams_math_environments_import_as_math_blocks() {
    for environment in [
        "align*",
        "aligned",
        "gather",
        "gather*",
        "gathered",
        "equation",
        "equation*",
        "split",
        "cases",
        "matrix",
        "pmatrix",
        "bmatrix",
    ] {
        let source = format!("\\begin{{{environment}}}\nx &= 1\n\\end{{{environment}}}");
        let document = parse_markdown_document(&source, MarkdownImportOptions::default());

        assert_eq!(1, document.blocks.len(), "environment={environment}");
        assert_eq!(
            RichBlockKind::Math,
            document.blocks[0].kind,
            "environment={environment}"
        );
        assert_eq!(source, document.blocks[0].payload.plain_text());
    }
}

#[test]
fn display_math_after_prose_without_blank_line_imports_as_a_separate_block() {
    let source = "行间公式：\n$$\nE = mc^2\n$$\n$$\nx^2 + y^2 = z^2\n$$";
    let document = parse_markdown_document(source, MarkdownImportOptions::default());

    assert_eq!(3, document.blocks.len());
    assert_eq!(RichBlockKind::Paragraph, document.blocks[0].kind);
    assert_eq!("行间公式：", document.blocks[0].payload.plain_text());
    assert_eq!(RichBlockKind::Math, document.blocks[1].kind);
    assert_eq!("E = mc^2", document.blocks[1].payload.plain_text());
    assert_eq!(RichBlockKind::Math, document.blocks[2].kind);
    assert_eq!("x^2 + y^2 = z^2", document.blocks[2].payload.plain_text());
}

#[test]
fn block_shortcuts_match_editor2_markers() {
    assert_eq!(
        block_kind_shortcut("#"),
        Some(RichBlockKind::Heading { level: 1 })
    );
    assert_eq!(
        block_kind_shortcut("[x]"),
        Some(RichBlockKind::Todo { checked: true })
    );
    assert_eq!(
        block_kind_shortcut("- [ ]"),
        Some(RichBlockKind::Todo { checked: false })
    );
    assert_eq!(
        block_kind_shortcut_with_marker_len("- [x] done"),
        Some((RichBlockKind::Todo { checked: true }, 6))
    );
    assert_eq!(
        block_kind_shortcut_with_marker_len("## title"),
        Some((RichBlockKind::Heading { level: 2 }, 3))
    );
    assert_eq!(
        block_kind_shortcut_with_marker_len("42. item"),
        Some((RichBlockKind::NumberedList, 4))
    );
    assert_eq!(
        block_kind_shortcut_with_marker_len("+ item"),
        Some((RichBlockKind::BulletedList, 2))
    );
    assert_eq!(
        block_kind_shortcut_with_marker_len("--- "),
        Some((RichBlockKind::Separator, 4))
    );
    assert_eq!(
        block_kind_shortcut_with_marker_len("> [!WARNING] "),
        Some((
            RichBlockKind::Callout {
                variant: CalloutVariant::Warning,
            },
            13,
        ))
    );
    assert_eq!(
        code_fence_shortcut("```Rust"),
        Some(RichBlockKind::Code {
            language: Some("rust".to_owned())
        })
    );
}

#[test]
fn inline_shortcuts_parse_bold_code_and_link() {
    let spans =
        markdown_inline_shortcut_spans("hello **bold** and `code` plus [zed](https://zed.dev)")
            .expect("inline markdown should parse");
    assert!(
        spans
            .iter()
            .any(|span| span.marks.contains(&InlineMark::Bold) && span.text == "bold")
    );
    assert!(
        spans
            .iter()
            .any(|span| span.marks.contains(&InlineMark::Code) && span.text == "code")
    );
    assert!(spans.iter().any(|span| matches!(span.marks.as_slice(), [InlineMark::Link { href }] if href == "https://zed.dev")));
}

#[test]
fn inline_shortcuts_parse_combined_bold_italic() {
    let spans = markdown_inline_shortcut_spans("***strong emphasis*** and ___also___")
        .expect("combined inline markdown should parse");
    assert_eq!(spans[0].text, "strong emphasis");
    assert_eq!(spans[0].marks, vec![InlineMark::Bold, InlineMark::Italic]);
    assert_eq!(spans[2].text, "also");
    assert_eq!(spans[2].marks, vec![InlineMark::Bold, InlineMark::Italic]);
}

#[test]
fn parses_markdown_document_blocks_tables_and_code() {
    let parsed = parse_markdown_document(
        "# Title\n- item\n  - child\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n```rust\nfn main() {}\n```",
        MarkdownImportOptions::default(),
    );
    assert!(matches!(
        parsed.blocks[0].kind,
        RichBlockKind::Heading { level: 1 }
    ));
    assert!(
        parsed
            .blocks
            .iter()
            .any(|block| matches!(block.kind, RichBlockKind::Table))
    );
    assert!(
        parsed
            .blocks
            .iter()
            .any(|block| matches!(block.kind, RichBlockKind::Code { .. }))
    );
    let child = parsed
        .blocks
        .iter()
        .find(|block| block.depth == 1)
        .expect("nested list child");
    assert!(child.parent_id.is_some());
}

#[test]
fn export_plain_markdown_matches_v1_basic_boundary() {
    let mut document = RichTextDocument::empty(1);
    document.push_root_block(RichBlockRecord::heading(1, 2, "Title"));
    document.push_root_block(RichBlockRecord::bulleted_list(2, "item"));
    document.push_root_block(RichBlockRecord::todo(3, true, "done"));
    document.push_root_block(RichBlockRecord::code_block(
        4,
        Some("rust".to_owned()),
        "fn main() {}",
    ));

    assert_eq!(
        export_plain_markdown(&document),
        "## Title\n- item\n- [x] done\n```rust\nfn main() {}\n```"
    );
}

#[test]
fn table_cells_support_escaped_pipe_like_v1() {
    let parsed = parse_markdown_document(
        "| A | B |\n|---|---|\n| left \\| right | ok |",
        MarkdownImportOptions::default(),
    );
    let table = parsed
        .blocks
        .iter()
        .find_map(|block| match &block.payload {
            BlockPayload::Table(table) => Some(table),
            _ => None,
        })
        .expect("table should parse");
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[1].cells.len(), 2);
    assert_eq!(
        crate::rich_text::plain_text_from_spans(&table.rows[1].cells[0].spans),
        "left | right"
    );
}

#[test]
fn incremental_multiline_callout_is_supported() {
    let block = import_markdown_block_incremental(
        "> [!WARNING]\n> be careful",
        MarkdownImportOptions::default(),
    )
    .expect("callout markdown should parse");
    assert!(matches!(
        block.kind,
        RichBlockKind::Callout {
            variant: CalloutVariant::Warning
        }
    ));
}

fn document_from_parsed(parsed: ParsedMarkdownDocument) -> RichTextDocument {
    RichTextDocument {
        id: 1,
        version: crate::rich_text::document::CURRENT_RICH_TEXT_FORMAT_VERSION,
        metadata: Default::default(),
        root_blocks: parsed.root_blocks,
        blocks: parsed.blocks,
        structure_version: 1,
    }
}

#[test]
fn supported_inline_markdown_round_trips_semantically() {
    let source =
        "plain **bold** *italic* ***both*** ~~strike~~ `code` [**link**](https://example.com/a(b))";
    let first = parse_markdown_document(source, MarkdownImportOptions::default());
    let exported = export_document_blocks(
        &document_from_parsed(first.clone()),
        MarkdownExportMode::Strict,
    );
    assert_eq!(exported.fidelity, MarkdownFidelity::Semantic);
    let second = parse_markdown_document(&exported.markdown, MarkdownImportOptions::default());

    assert_eq!(first.blocks[0].payload, second.blocks[0].payload);
}

#[test]
fn paragraph_soft_line_break_round_trips_without_becoming_a_block_break() {
    let source = "first line\nsecond **line**";
    let first = parse_markdown_document(source, MarkdownImportOptions::default());

    assert_eq!(first.blocks.len(), 1);
    assert_eq!(
        first.blocks[0].payload.plain_text(),
        "first line\nsecond line"
    );

    let exported = export_document_blocks(
        &document_from_parsed(first.clone()),
        MarkdownExportMode::Strict,
    );
    assert_eq!(exported.fidelity, MarkdownFidelity::Semantic);
    assert_eq!(exported.markdown, source);

    let second = parse_markdown_document(&exported.markdown, MarkdownImportOptions::default());
    assert_eq!(first.blocks[0].payload, second.blocks[0].payload);
}

#[test]
fn editor_paragraph_newline_is_supported_by_strict_markdown_export() {
    let mut document = RichTextDocument::empty(1);
    document.push_root_block(RichBlockRecord::paragraph(1, "alpha\nbeta"));

    let exported = export_document_blocks(&document, MarkdownExportMode::Strict);
    assert_eq!(exported.fidelity, MarkdownFidelity::Semantic);
    assert_eq!(exported.markdown, "alpha\nbeta");

    let reparsed = parse_markdown_document(&exported.markdown, MarkdownImportOptions::default());
    assert_eq!(reparsed.blocks.len(), 1);
    assert_eq!(reparsed.blocks[0].payload.plain_text(), "alpha\nbeta");
}

#[test]
fn escaped_plain_text_round_trips_without_becoming_structure() {
    let special = r#"\ * _ ~ ` [ ] ( ) < > # heading - item 1. item 中文 😀"#;
    let mut document = RichTextDocument::empty(1);
    document.push_root_block(RichBlockRecord::paragraph(1, special));

    let exported = export_document_blocks(&document, MarkdownExportMode::Strict);
    let reparsed = parse_markdown_document(&exported.markdown, MarkdownImportOptions::default());
    assert_eq!(reparsed.blocks[0].payload.plain_text(), special);
    assert!(matches!(reparsed.blocks[0].kind, RichBlockKind::Paragraph));
}

#[test]
fn nested_list_structure_round_trips() {
    let source = "- parent\n  - child\n    1. first\n    2. second";
    let first = parse_markdown_document(source, MarkdownImportOptions::default());
    let exported = export_document_blocks(
        &document_from_parsed(first.clone()),
        MarkdownExportMode::Strict,
    );
    let second = parse_markdown_document(&exported.markdown, MarkdownImportOptions::default());

    let first_structure = first
        .blocks
        .iter()
        .map(|block| (block.kind.clone(), block.parent_id, block.depth))
        .collect::<Vec<_>>();
    let second_structure = second
        .blocks
        .iter()
        .map(|block| (block.kind.clone(), block.parent_id, block.depth))
        .collect::<Vec<_>>();
    assert_eq!(first_structure, second_structure);
}

#[test]
fn code_fence_grows_past_embedded_backticks() {
    let mut document = RichTextDocument::empty(1);
    document.push_root_block(RichBlockRecord::code_block(
        1,
        Some("rust".to_owned()),
        "let fence = \"```\";",
    ));
    let exported = export_document_blocks(&document, MarkdownExportMode::Strict);
    assert!(exported.markdown.starts_with("````rust\n"));
    let reparsed = parse_markdown_document(&exported.markdown, MarkdownImportOptions::default());
    assert_eq!(
        reparsed.blocks[0].payload.plain_text(),
        "let fence = \"```\";"
    );
}

#[test]
fn table_marks_pipes_and_alignment_round_trip() {
    let source = "| Name | Value |\n| :--- | ---: |\n| **left \\| right** | `42` |";
    let first = parse_markdown_document(source, MarkdownImportOptions::default());
    let exported = export_document_blocks(
        &document_from_parsed(first.clone()),
        MarkdownExportMode::Strict,
    );
    assert_eq!(exported.fidelity, MarkdownFidelity::Semantic);
    let second = parse_markdown_document(&exported.markdown, MarkdownImportOptions::default());
    assert_eq!(first.blocks[0].payload, second.blocks[0].payload);
}

#[test]
fn image_with_stable_source_round_trips_as_an_image_block() {
    let source = "![diagram](<images/diagram(a).png>)";
    let first = parse_markdown_document(source, MarkdownImportOptions::default());
    assert!(matches!(first.blocks[0].kind, RichBlockKind::Image));
    let exported = export_document_blocks(
        &document_from_parsed(first.clone()),
        MarkdownExportMode::Strict,
    );
    let second = parse_markdown_document(&exported.markdown, MarkdownImportOptions::default());
    assert_eq!(first.blocks[0].payload, second.blocks[0].payload);
}

#[test]
fn compatibility_report_distinguishes_normalization_and_source_only() {
    let normalized = parse_markdown_document_with_report(
        "2. second\n\n_italic_",
        MarkdownImportOptions::default(),
    );
    assert!(matches!(
        normalized.compatibility,
        MarkdownCompatibility::SourceOnly(_)
    ));

    for source in [
        "---\ntitle: Notes\n---\nbody",
        "[^1]: unsupported footnote",
        "[label]: https://example.com",
        "::: custom",
        "++underline++",
        "**`code`**",
        "| A | B |\n| not | alignment |",
    ] {
        let result = parse_markdown_document_with_report(source, MarkdownImportOptions::default());
        assert!(
            matches!(result.compatibility, MarkdownCompatibility::Editable),
            "expected editable compatibility for {source:?}"
        );
        let exported = export_document_blocks(
            &document_from_parsed(result.document),
            MarkdownExportMode::Strict,
        );
        assert_eq!(
            source, exported.markdown,
            "expected exact round trip for {source:?}"
        );
    }

    for incomplete in [
        "```rust\nfn main() {}",
        "~~~rust\nfn main() {}",
        "$$\nx + y",
    ] {
        let result =
            parse_markdown_document_with_report(incomplete, MarkdownImportOptions::default());
        assert!(
            matches!(result.compatibility, MarkdownCompatibility::Editable),
            "expected incomplete source to remain editable for {incomplete:?}"
        );
        assert_eq!(result.document.blocks.len(), 1);
        assert_eq!(result.document.blocks[0].kind, RichBlockKind::RawMarkdown);
        let exported = export_document_blocks(
            &document_from_parsed(result.document),
            MarkdownExportMode::Strict,
        );
        assert_eq!(incomplete, exported.markdown);
    }

    let html = parse_markdown_document_with_report(
        "<custom-tag>body</custom-tag>",
        MarkdownImportOptions::default(),
    );
    assert!(matches!(
        html.compatibility,
        MarkdownCompatibility::Editable
    ));
}

#[test]
fn raw_markdown_edits_export_verbatim_without_using_stale_import_fallback() {
    let source = "[^1]: original footnote";
    let parsed = parse_markdown_document_with_report(source, MarkdownImportOptions::default());
    assert_eq!(parsed.document.blocks[0].kind, RichBlockKind::RawMarkdown);

    let mut document = document_from_parsed(parsed.document);
    document.blocks[0].payload = BlockPayload::RichText {
        spans: vec![InlineSpan::plain("[^1]: edited **without escaping**")],
    };
    let exported = export_document_blocks(&document, MarkdownExportMode::Strict);
    assert_eq!("[^1]: edited **without escaping**", exported.markdown);
    assert!(!exported.markdown.contains("\\*"));
}

#[test]
fn compatibility_only_silences_informational_normalization() {
    let info = MarkdownDiagnostic::source(
        MarkdownDiagnosticSeverity::Info,
        "markdown.source.bullet_normalized",
        "Bullet markers are normalized to -",
        0..1,
    );
    assert!(matches!(
        MarkdownCompatibility::from_diagnostics(&[info]),
        MarkdownCompatibility::Editable
    ));

    let warning = MarkdownDiagnostic::source(
        MarkdownDiagnosticSeverity::Warning,
        "markdown.bundle.preview_regenerated",
        "A generated preview will be rewritten",
        0..1,
    );
    assert!(matches!(
        MarkdownCompatibility::from_diagnostics(&[warning]),
        MarkdownCompatibility::EditableWithNormalization(_)
    ));

    let error = MarkdownDiagnostic::source(
        MarkdownDiagnosticSeverity::Error,
        "markdown.source.unsupported",
        "The source cannot be round-tripped safely",
        0..1,
    );
    assert!(matches!(
        MarkdownCompatibility::from_diagnostics(&[error]),
        MarkdownCompatibility::SourceOnly(_)
    ));
}
