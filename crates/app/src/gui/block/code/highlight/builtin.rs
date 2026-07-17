use cditor_core::rich_text::{InlineMark, InlineSpan};
use lumis::highlight::Highlighter;
use lumis::languages::Language;
use lumis::themes::{self, UnderlineStyle};

use super::spans::push_span;

pub(super) fn highlight_source(
    source: &str,
    language: &str,
    theme_name: &str,
) -> Result<Vec<InlineSpan>, String> {
    let language = language_for_name(language)
        .ok_or_else(|| format!("unsupported built-in syntax language: {language}"))?;
    let theme = themes::get(theme_name).map_err(|error| error.to_string())?;
    let highlighted = Highlighter::new(language, Some(theme))
        .highlight(source)
        .map_err(|error| error.to_string())?;
    let mut spans = Vec::<InlineSpan>::with_capacity(highlighted.len());
    for (style, text) in highlighted {
        if text.is_empty() {
            continue;
        }
        let mut marks = Vec::with_capacity(5);
        if let Some(color) = &style.fg {
            marks.push(InlineMark::Color(color.clone()));
        }
        if style.bold {
            marks.push(InlineMark::Bold);
        }
        if style.italic {
            marks.push(InlineMark::Italic);
        }
        if style.text_decoration.underline != UnderlineStyle::None {
            marks.push(InlineMark::Underline);
        }
        if style.text_decoration.strikethrough {
            marks.push(InlineMark::Strike);
        }
        push_span(
            &mut spans,
            InlineSpan {
                text: text.to_owned(),
                marks,
            },
        );
    }
    if spans.is_empty() && !source.is_empty() {
        spans.push(InlineSpan::plain(source));
    }
    Ok(spans)
}

fn language_for_name(language: &str) -> Option<Language> {
    match language {
        "rust" => Some(Language::Rust),
        "typescript" => Some(Language::TypeScript),
        "javascript" => Some(Language::JavaScript),
        "tsx" => Some(Language::Tsx),
        "python" => Some(Language::Python),
        "go" => Some(Language::Go),
        "java" => Some(Language::Java),
        "kotlin" => Some(Language::Kotlin),
        "swift" => Some(Language::Swift),
        "c" => Some(Language::C),
        "cpp" => Some(Language::CPlusPlus),
        "csharp" => Some(Language::CSharp),
        "html" => Some(Language::HTML),
        "css" => Some(Language::CSS),
        "scss" => Some(Language::SCSS),
        "json" => Some(Language::JSON),
        "yaml" => Some(Language::YAML),
        "toml" => Some(Language::Toml),
        "markdown" => Some(Language::Markdown),
        "sql" => Some(Language::SQL),
        "bash" => Some(Language::Bash),
        "zsh" => Some(Language::Zsh),
        "dockerfile" => Some(Language::Dockerfile),
        "diff" => Some(Language::Diff),
        _ => None,
    }
}
