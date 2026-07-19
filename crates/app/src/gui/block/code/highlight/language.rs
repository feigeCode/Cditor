use std::collections::HashSet;

use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayload, BlockPayloadView, RichBlockKind};
use cditor_runtime::EditorViewProjection;

use crate::api::SyntaxHighlightLanguage;
use crate::gui::input::CodeLanguageItem;

pub(super) fn provider_language_items(
    languages: Vec<SyntaxHighlightLanguage>,
) -> Vec<CodeLanguageItem> {
    let mut seen = HashSet::new();
    seen.insert("plain text".to_owned());
    seen.insert("text".to_owned());
    seen.insert("math".to_owned());
    let mut items = vec![
        CodeLanguageItem::labeled("plain text", "Plain Text"),
        CodeLanguageItem::labeled("math", "数学公式"),
    ];
    for language in languages {
        let id = language.id.trim().to_ascii_lowercase();
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }
        let label = if language.label.trim().is_empty() {
            id.clone()
        } else {
            language.label.trim().to_owned()
        };
        items.push(CodeLanguageItem::labeled(id, label));
    }
    items
}

pub(super) fn visible_code_blocks(
    projection: &EditorViewProjection,
) -> Vec<(BlockId, u64, &str, String)> {
    projection
        .blocks
        .iter()
        .filter_map(|block| {
            let RichBlockKind::Code { language } = &block.kind else {
                return None;
            };
            let BlockPayloadView::Loaded(payload) = &block.payload else {
                return None;
            };
            let BlockPayload::Code {
                language: payload_language,
                text,
            } = &payload.payload
            else {
                return None;
            };
            let language = normalize_language(payload_language.as_deref().or(language.as_deref()))?;
            Some((
                block.block_id,
                payload.content_version,
                text.as_str(),
                language,
            ))
        })
        .collect()
}

pub(super) fn normalize_language(language: Option<&str>) -> Option<String> {
    let normalized = language?.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "text" | "plain" | "plaintext" | "plain text" => None,
        "rs" => Some("rust".to_owned()),
        "ts" => Some("typescript".to_owned()),
        "js" | "jsx" => Some("javascript".to_owned()),
        "py" => Some("python".to_owned()),
        "golang" => Some("go".to_owned()),
        "kt" => Some("kotlin".to_owned()),
        "c++" => Some("cpp".to_owned()),
        "c#" | "cs" => Some("csharp".to_owned()),
        "htm" => Some("html".to_owned()),
        "yml" => Some("yaml".to_owned()),
        "md" => Some("markdown".to_owned()),
        "shell" | "sh" => Some("bash".to_owned()),
        "docker" => Some("dockerfile".to_owned()),
        "patch" => Some("diff".to_owned()),
        _ => Some(normalized),
    }
}
