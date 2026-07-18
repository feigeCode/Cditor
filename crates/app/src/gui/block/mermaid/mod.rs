mod cache;
mod render;
mod theme;

pub(crate) use cache::{DocumentRenderCache, DocumentRenderStatus};

pub(crate) fn is_math_code_language(language: Option<&str>) -> bool {
    language.is_some_and(|language| {
        matches!(
            language.trim().to_ascii_lowercase().as_str(),
            "math" | "latex" | "tex" | "katex"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::is_math_code_language;

    #[test]
    fn common_math_code_languages_are_renderable() {
        for language in ["math", "latex", "tex", "katex", "LaTeX"] {
            assert!(is_math_code_language(Some(language)), "{language}");
        }
        assert!(!is_math_code_language(Some("rust")));
        assert!(!is_math_code_language(None));
    }
}
pub(crate) use render::{render_math_block, render_mermaid_block};
