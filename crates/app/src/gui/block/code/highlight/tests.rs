use super::*;
use crate::api::{
    SyntaxHighlightError, SyntaxHighlightLanguage, SyntaxHighlightPalette, SyntaxHighlightProvider,
    SyntaxHighlightRun, SyntaxHighlightStyle,
};
use cditor_core::rich_text::{InlineMark, InlineSpan, plain_text_from_spans};
use std::sync::Arc;

struct TestProvider;

impl SyntaxHighlightProvider for TestProvider {
    fn id(&self) -> &str {
        "test"
    }

    fn revision(&self) -> u64 {
        7
    }

    fn palette(&self) -> SyntaxHighlightPalette {
        SyntaxHighlightPalette {
            background: 0x101010,
            foreground: 0xf0f0f0,
        }
    }

    fn languages(&self) -> Vec<SyntaxHighlightLanguage> {
        vec![SyntaxHighlightLanguage::new("zig", "Zig")]
    }

    fn highlight(
        &self,
        _language: &str,
        _source: &str,
    ) -> Result<Vec<SyntaxHighlightRun>, SyntaxHighlightError> {
        Ok(vec![SyntaxHighlightRun {
            range: 0..2,
            style: SyntaxHighlightStyle {
                foreground: Some(0xff0000),
                bold: true,
                ..Default::default()
            },
        }])
    }
}

#[test]
fn external_provider_ranges_preserve_source_and_fill_plain_gaps() {
    let source = "fn main";
    let spans = spans::highlight_with_provider(&TestProvider, "rust", source)
        .expect("valid provider output");
    assert_eq!(plain_text_from_spans(&spans), source);
    assert_eq!(spans[0].text, "fn");
    assert!(spans[0].marks.contains(&InlineMark::Bold));
    assert_eq!(spans[1], InlineSpan::plain(" main"));
}

#[test]
fn external_provider_rejects_ranges_outside_utf8_boundaries() {
    struct InvalidProvider;
    impl SyntaxHighlightProvider for InvalidProvider {
        fn id(&self) -> &str {
            "invalid"
        }

        fn revision(&self) -> u64 {
            0
        }

        fn palette(&self) -> SyntaxHighlightPalette {
            TestProvider.palette()
        }

        fn highlight(
            &self,
            _language: &str,
            _source: &str,
        ) -> Result<Vec<SyntaxHighlightRun>, SyntaxHighlightError> {
            Ok(vec![SyntaxHighlightRun {
                range: 1..2,
                style: SyntaxHighlightStyle::default(),
            }])
        }
    }

    let error = spans::highlight_with_provider(&InvalidProvider, "rust", "你")
        .expect_err("invalid UTF-8 range must be rejected");
    assert!(error.contains("UTF-8"));
}

#[test]
fn disabling_and_reenabling_preserves_external_provider() {
    let mut cache = CodeHighlightCache::default();
    cache.configure(Some(Arc::new(TestProvider)), true);
    assert_eq!(cache.theme_item("dracula").background, 0x101010);

    cache.set_enabled(false);
    assert!(!cache.uses_builtin_themes());
    cache.set_enabled(true);

    assert_eq!(cache.theme_item("dracula").background, 0x101010);
    assert!(!cache.uses_builtin_themes());
}

#[test]
fn external_provider_catalog_keeps_host_math_renderer_entry() {
    let mut cache = CodeHighlightCache::default();
    cache.configure(Some(Arc::new(TestProvider)), true);

    let items = cache.language_items();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].value, "plain text");
    assert_eq!(items[1].value, "math");
    assert_eq!(items[1].label, "数学公式");
    assert_eq!(items[2].value, "zig");
    assert!(!items.iter().any(|item| item.value == "typescript"));
}

#[test]
fn language_aliases_are_normalized_without_rejecting_extensions() {
    assert_eq!(
        language::normalize_language(Some("JSX")).as_deref(),
        Some("javascript")
    );
    assert_eq!(
        language::normalize_language(Some("shell")).as_deref(),
        Some("bash")
    );
    assert_eq!(
        language::normalize_language(Some("custom-wasm")).as_deref(),
        Some("custom-wasm")
    );
    assert_eq!(language::normalize_language(Some("plain text")), None);
}

#[test]
fn rebase_preserves_exact_text_for_unicode_edits() {
    let old_source = "const 名 = 1;";
    let old_spans = vec![InlineSpan {
        text: old_source.to_owned(),
        marks: vec![InlineMark::Color("#ff0000".to_owned())],
    }];
    for new_source in ["const 新名 = 1;", "const 名 = 2;", "", "界"] {
        let rebased = spans::rebase_spans(old_source, &old_spans, new_source);
        assert_eq!(plain_text_from_spans(&rebased), new_source);
    }
}

#[cfg(feature = "builtin-syntax-highlighting")]
#[test]
fn bundled_javascript_highlight_preserves_source_and_adds_colors() {
    let source = "const x = 1; // 你好 👋";
    let spans =
        builtin::highlight_source(source, "javascript", "dracula").expect("highlight succeeds");
    assert_eq!(plain_text_from_spans(&spans), source);
    assert!(
        spans
            .iter()
            .flat_map(|span| &span.marks)
            .any(|mark| { matches!(mark, InlineMark::Color(_)) })
    );
}

#[cfg(feature = "builtin-syntax-highlighting")]
#[test]
fn bundled_theme_menu_items_resolve_in_lumis() {
    assert!(
        CODE_THEME_ITEMS
            .iter()
            .all(|item| lumis::themes::get(item.id).is_ok())
    );
    assert!(
        CODE_THEME_ITEMS
            .iter()
            .any(|item| item.id == DEFAULT_CODE_HIGHLIGHT_THEME)
    );
}
