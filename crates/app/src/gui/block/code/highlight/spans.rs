use cditor_core::rich_text::{InlineMark, InlineSpan};

use crate::api::{SyntaxHighlightProvider, SyntaxHighlightStyle};

pub(super) fn highlight_with_provider(
    provider: &dyn SyntaxHighlightProvider,
    language: &str,
    source: &str,
) -> Result<Vec<InlineSpan>, String> {
    let mut runs = provider
        .highlight(language, source)
        .map_err(|error| error.to_string())?;
    runs.sort_by_key(|run| (run.range.start, run.range.end));
    let mut spans = Vec::with_capacity(runs.len().saturating_mul(2).saturating_add(1));
    let mut offset = 0;
    for run in runs {
        validate_range(source, offset, &run.range)?;
        if offset < run.range.start {
            push_span(
                &mut spans,
                InlineSpan::plain(&source[offset..run.range.start]),
            );
        }
        if !run.range.is_empty() {
            push_span(
                &mut spans,
                InlineSpan {
                    text: source[run.range.clone()].to_owned(),
                    marks: marks_for_style(run.style),
                },
            );
        }
        offset = run.range.end;
    }
    if offset < source.len() {
        push_span(&mut spans, InlineSpan::plain(&source[offset..]));
    }
    if spans.is_empty() && !source.is_empty() {
        spans.push(InlineSpan::plain(source));
    }
    Ok(spans)
}

fn validate_range(
    source: &str,
    previous_end: usize,
    range: &std::ops::Range<usize>,
) -> Result<(), String> {
    if range.start > range.end || range.end > source.len() {
        return Err(format!(
            "syntax highlight range {:?} is outside source length {}",
            range,
            source.len()
        ));
    }
    if range.start < previous_end {
        return Err(format!(
            "syntax highlight range {:?} overlaps a previous range",
            range
        ));
    }
    if !source.is_char_boundary(range.start) || !source.is_char_boundary(range.end) {
        return Err(format!(
            "syntax highlight range {:?} is not on a UTF-8 boundary",
            range
        ));
    }
    Ok(())
}

fn marks_for_style(style: SyntaxHighlightStyle) -> Vec<InlineMark> {
    let mut marks = Vec::with_capacity(6);
    if let Some(color) = style.foreground {
        marks.push(InlineMark::Color(format!("#{color:06x}")));
    }
    if let Some(color) = style.background {
        marks.push(InlineMark::Background(format!("#{color:06x}")));
    }
    if style.bold {
        marks.push(InlineMark::Bold);
    }
    if style.italic {
        marks.push(InlineMark::Italic);
    }
    if style.underline {
        marks.push(InlineMark::Underline);
    }
    if style.strikethrough {
        marks.push(InlineMark::Strike);
    }
    marks
}

pub(super) fn rebase_spans(
    old_source: &str,
    old_spans: &[InlineSpan],
    new_source: &str,
) -> Vec<InlineSpan> {
    let prefix = common_prefix_bytes(old_source, new_source);
    let suffix = common_suffix_bytes(&old_source[prefix..], &new_source[prefix..]);
    let old_suffix_start = old_source.len() - suffix;
    let new_suffix_start = new_source.len() - suffix;
    let mut rebased = Vec::new();
    append_span_slice(&mut rebased, old_spans, 0, prefix);
    push_span(
        &mut rebased,
        InlineSpan::plain(&new_source[prefix..new_suffix_start]),
    );
    append_span_slice(&mut rebased, old_spans, old_suffix_start, old_source.len());
    rebased
}

fn common_prefix_bytes(left: &str, right: &str) -> usize {
    left.char_indices()
        .zip(right.chars())
        .take_while(|((_, left), right)| *left == *right)
        .map(|((offset, character), _)| offset + character.len_utf8())
        .last()
        .unwrap_or(0)
}

fn common_suffix_bytes(left: &str, right: &str) -> usize {
    left.char_indices()
        .rev()
        .zip(right.chars().rev())
        .take_while(|((_, left), right)| *left == *right)
        .map(|((offset, _), _)| left.len() - offset)
        .last()
        .unwrap_or(0)
}

fn append_span_slice(
    target: &mut Vec<InlineSpan>,
    spans: &[InlineSpan],
    range_start: usize,
    range_end: usize,
) {
    if range_start >= range_end {
        return;
    }
    let mut offset = 0;
    for span in spans {
        let span_start = offset;
        let span_end = span_start + span.text.len();
        offset = span_end;
        let start = span_start.max(range_start);
        let end = span_end.min(range_end);
        if start < end {
            push_span(
                target,
                InlineSpan {
                    text: span.text[start - span_start..end - span_start].to_owned(),
                    marks: span.marks.clone(),
                },
            );
        }
    }
}

pub(super) fn push_span(target: &mut Vec<InlineSpan>, span: InlineSpan) {
    if span.text.is_empty() {
        return;
    }
    if let Some(previous) = target.last_mut()
        && previous.marks == span.marks
    {
        previous.text.push_str(&span.text);
    } else {
        target.push(span);
    }
}
