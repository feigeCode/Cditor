use super::compatibility::{
    MarkdownDiagnostic, MarkdownDiagnosticSeverity, MarkdownExportMode, MarkdownFidelity,
};
use super::escape::{choose_code_span_delimiter, escape_inline_text, escape_link_destination};
use crate::rich_text::{InlineMark, InlineSpan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineMarkdownExport {
    pub markdown: String,
    pub fidelity: MarkdownFidelity,
    pub diagnostics: Vec<MarkdownDiagnostic>,
}

pub fn export_inline_spans(spans: &[InlineSpan], mode: MarkdownExportMode) -> InlineMarkdownExport {
    let spans = merge_equivalent_spans(spans);
    let mut markdown = String::new();
    let mut diagnostics = Vec::new();
    let mut unsupported = false;

    for span in spans {
        let normalized = normalize_marks(&span.marks, &mut diagnostics);
        let has_code_conflict = normalized.code
            && (normalized.bold
                || normalized.italic
                || normalized.strike
                || normalized.link.is_some());
        if has_code_conflict {
            unsupported = true;
            diagnostics.push(MarkdownDiagnostic {
                severity: MarkdownDiagnosticSeverity::Error,
                code: "markdown.inline.mark_combination_unsupported",
                message: "Inline code cannot preserve emphasis, strike, or link marks in standard Markdown"
                    .to_owned(),
                source_range: None,
                block_id: None,
            });
        }
        if normalized.unsupported {
            unsupported = true;
        }

        let mut rendered = if normalized.code {
            render_code_span(&span.text)
        } else {
            escape_inline_text(&span.text)
        };

        if !normalized.code {
            rendered = match (normalized.bold, normalized.italic) {
                (true, true) => format!("***{rendered}***"),
                (true, false) => format!("**{rendered}**"),
                (false, true) => format!("*{rendered}*"),
                (false, false) => rendered,
            };
            if normalized.strike {
                rendered = format!("~~{rendered}~~");
            }
            if let Some(href) = normalized.link {
                rendered = format!("[{rendered}](<{}>)", escape_link_destination(href.as_str()));
            }
        }
        markdown.push_str(&rendered);
    }

    let fidelity = if unsupported {
        MarkdownFidelity::Unsupported
    } else {
        MarkdownFidelity::Semantic
    };
    if mode == MarkdownExportMode::BestEffort {
        for diagnostic in &mut diagnostics {
            if diagnostic.severity == MarkdownDiagnosticSeverity::Error {
                diagnostic.severity = MarkdownDiagnosticSeverity::Warning;
            }
        }
    }
    InlineMarkdownExport {
        markdown,
        fidelity,
        diagnostics,
    }
}

#[derive(Default)]
struct NormalizedMarks {
    link: Option<String>,
    strike: bool,
    bold: bool,
    italic: bool,
    code: bool,
    unsupported: bool,
}

fn normalize_marks(
    marks: &[InlineMark],
    diagnostics: &mut Vec<MarkdownDiagnostic>,
) -> NormalizedMarks {
    let mut normalized = NormalizedMarks::default();
    for mark in marks {
        match mark {
            InlineMark::Link { href } => {
                if normalized
                    .link
                    .as_ref()
                    .is_some_and(|current| current != href)
                {
                    normalized.unsupported = true;
                    diagnostics.push(unsupported_mark(
                        "markdown.inline.mark_combination_unsupported",
                        "A span cannot preserve multiple different Markdown link destinations",
                    ));
                } else {
                    normalized.link = Some(href.clone());
                }
            }
            InlineMark::Strike => normalized.strike = true,
            InlineMark::Bold => normalized.bold = true,
            InlineMark::Italic => normalized.italic = true,
            InlineMark::Code => normalized.code = true,
            InlineMark::Underline => {
                normalized.unsupported = true;
                diagnostics.push(unsupported_mark(
                    "markdown.inline.underline_unsupported",
                    "Underline cannot be represented safely in standard Markdown",
                ));
            }
            InlineMark::Color(_) => {
                normalized.unsupported = true;
                diagnostics.push(unsupported_mark(
                    "markdown.inline.color_unsupported",
                    "Inline text color cannot be represented safely in standard Markdown",
                ));
            }
            InlineMark::Background(_) => {
                normalized.unsupported = true;
                diagnostics.push(unsupported_mark(
                    "markdown.inline.background_unsupported",
                    "Inline background color cannot be represented safely in standard Markdown",
                ));
            }
        }
    }
    normalized
}

fn unsupported_mark(code: &'static str, message: &'static str) -> MarkdownDiagnostic {
    MarkdownDiagnostic {
        severity: MarkdownDiagnosticSeverity::Error,
        code,
        message: message.to_owned(),
        source_range: None,
        block_id: None,
    }
}

fn render_code_span(text: &str) -> String {
    let delimiter = choose_code_span_delimiter(text);
    let needs_padding = text.starts_with('`')
        || text.starts_with(' ')
        || text.ends_with('`')
        || text.ends_with(' ');
    if needs_padding {
        format!("{delimiter} {text} {delimiter}")
    } else {
        format!("{delimiter}{text}{delimiter}")
    }
}

fn merge_equivalent_spans(spans: &[InlineSpan]) -> Vec<InlineSpan> {
    let mut merged: Vec<InlineSpan> = Vec::new();
    for span in spans {
        if let Some(previous) = merged.last_mut()
            && marks_semantically_equal(&previous.marks, &span.marks)
        {
            previous.text.push_str(&span.text);
        } else {
            merged.push(span.clone());
        }
    }
    merged
}

fn marks_semantically_equal(left: &[InlineMark], right: &[InlineMark]) -> bool {
    left.len() == right.len() && left.iter().all(|mark| right.contains(mark))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_marks_export_in_canonical_order() {
        let result = export_inline_spans(
            &[InlineSpan {
                text: "linked".to_owned(),
                marks: vec![
                    InlineMark::Italic,
                    InlineMark::Link {
                        href: "https://example.com/a(b)".to_owned(),
                    },
                    InlineMark::Bold,
                    InlineMark::Strike,
                ],
            }],
            MarkdownExportMode::Strict,
        );

        assert_eq!(
            result.markdown,
            "[~~***linked***~~](<https://example.com/a(b)>)"
        );
        assert_eq!(result.fidelity, MarkdownFidelity::Semantic);
    }

    #[test]
    fn code_span_chooses_a_safe_delimiter_and_padding() {
        let result = export_inline_spans(
            &[InlineSpan {
                text: "hello `world`".to_owned(),
                marks: vec![InlineMark::Code],
            }],
            MarkdownExportMode::Strict,
        );
        assert_eq!(result.markdown, "`` hello `world` ``");
    }

    #[test]
    fn strict_marks_unsupported_inline_styles() {
        let result = export_inline_spans(
            &[InlineSpan {
                text: "colored".to_owned(),
                marks: vec![InlineMark::Color("#fff".to_owned())],
            }],
            MarkdownExportMode::Strict,
        );
        assert_eq!(result.fidelity, MarkdownFidelity::Unsupported);
        assert_eq!(
            result.diagnostics[0].code,
            "markdown.inline.color_unsupported"
        );
    }
}
