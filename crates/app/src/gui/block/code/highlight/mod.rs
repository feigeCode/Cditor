//! Viewport-triggered code-block syntax highlighting with a bounded offscreen cache.

mod language;
mod spans;
mod theme;

#[cfg(feature = "builtin-syntax-highlighting")]
mod builtin;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use cditor_core::ids::BlockId;
use cditor_core::rich_text::InlineSpan;
use cditor_runtime::EditorViewProjection;
use gpui::{AppContext, Context, Task};

use crate::api::SyntaxHighlightProvider;
use crate::gui::app::CditorV2View;
use crate::gui::input::CodeLanguageItem;
use crate::gui::input::code_language::code_language_items;

pub(crate) use theme::{
    CODE_THEME_ITEMS, CodeThemeItem, DEFAULT_CODE_HIGHLIGHT_THEME, code_theme_item,
};

use language::visible_code_blocks;
use spans::{highlight_with_provider, rebase_spans};

type HighlightResult = Result<Arc<Vec<InlineSpan>>, Arc<str>>;
const MAX_RETAINED_OFFSCREEN_ENTRIES: usize = 128;

#[derive(Clone, Default)]
enum HighlightEngine {
    #[cfg(feature = "builtin-syntax-highlighting")]
    #[default]
    Builtin,
    External(Arc<dyn SyntaxHighlightProvider>),
    #[cfg_attr(not(feature = "builtin-syntax-highlighting"), default)]
    Disabled,
}

impl HighlightEngine {
    fn revision(&self) -> u64 {
        match self {
            #[cfg(feature = "builtin-syntax-highlighting")]
            Self::Builtin => 0,
            Self::External(provider) => provider.revision(),
            Self::Disabled => 0,
        }
    }

    fn theme_key(&self, selected_theme: &'static str) -> &'static str {
        let _ = selected_theme;
        match self {
            #[cfg(feature = "builtin-syntax-highlighting")]
            Self::Builtin => selected_theme,
            Self::External(_) | Self::Disabled => "",
        }
    }

    fn highlight(
        &self,
        source: &str,
        language: &str,
        selected_theme: &str,
    ) -> Result<Vec<InlineSpan>, String> {
        let _ = selected_theme;
        match self {
            #[cfg(feature = "builtin-syntax-highlighting")]
            Self::Builtin => builtin::highlight_source(source, language, selected_theme),
            Self::External(provider) => {
                highlight_with_provider(provider.as_ref(), language, source)
            }
            Self::Disabled => Ok(vec![InlineSpan::plain(source)]),
        }
    }
}

struct CodeHighlightEntry {
    content_version: u64,
    language: Arc<str>,
    engine_revision: u64,
    theme_key: &'static str,
    source: Arc<str>,
    fallback: Arc<Vec<InlineSpan>>,
    result: Arc<OnceLock<HighlightResult>>,
    last_used_tick: u64,
    _task: Task<()>,
}

impl CodeHighlightEntry {
    fn new(request: HighlightRequest, cx: &mut Context<CditorV2View>) -> Self {
        let source = Arc::<str>::from(request.source);
        let language = Arc::<str>::from(request.language);
        let task_source = source.clone();
        let task_language = language.clone();
        let result = Arc::new(OnceLock::new());
        let task_result = result.clone();
        let engine = request.engine;
        let task = cx.spawn(async move |view, cx| {
            let highlighted = cx
                .background_spawn(async move {
                    engine
                        .highlight(&task_source, &task_language, request.theme_key)
                        .map(Arc::new)
                        .map_err(Arc::<str>::from)
                })
                .await;
            let _ = task_result.set(highlighted);
            let _ = view.update(cx, |view, cx| view.schedule_code_highlight_refresh(cx));
        });
        Self {
            content_version: request.content_version,
            language,
            engine_revision: request.engine_revision,
            theme_key: request.theme_key,
            source,
            fallback: Arc::new(request.fallback),
            result,
            last_used_tick: 0,
            _task: task,
        }
    }

    fn matches(
        &self,
        content_version: u64,
        language: &str,
        engine_revision: u64,
        theme_key: &str,
    ) -> bool {
        self.content_version == content_version
            && self.language.as_ref() == language
            && self.engine_revision == engine_revision
            && self.theme_key == theme_key
    }

    fn spans(&self) -> &[InlineSpan] {
        self.result
            .get()
            .and_then(|result| result.as_ref().ok())
            .map(|spans| spans.as_slice())
            .unwrap_or(self.fallback.as_slice())
    }
}

struct HighlightRequest {
    content_version: u64,
    source: String,
    language: String,
    engine_revision: u64,
    theme_key: &'static str,
    fallback: Vec<InlineSpan>,
    engine: HighlightEngine,
}

pub(crate) struct CodeHighlightCache {
    entries: HashMap<BlockId, CodeHighlightEntry>,
    engine: HighlightEngine,
    enabled: bool,
    sync_tick: u64,
}

impl Default for CodeHighlightCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            engine: HighlightEngine::default(),
            enabled: true,
            sync_tick: 0,
        }
    }
}

impl CodeHighlightCache {
    pub(crate) fn configure(
        &mut self,
        provider: Option<Arc<dyn SyntaxHighlightProvider>>,
        enabled: bool,
    ) {
        self.engine = if let Some(provider) = provider {
            HighlightEngine::External(provider)
        } else {
            HighlightEngine::default()
        };
        self.enabled = enabled;
        self.clear();
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        self.clear();
    }

    pub(crate) fn sync_visible_window(
        &mut self,
        projection: &EditorViewProjection,
        selected_theme: &'static str,
        cx: &mut Context<CditorV2View>,
    ) {
        let trace = std::env::var_os("CDITOR_TRACE_SYNTAX_HIGHLIGHT").is_some();
        let started = Instant::now();
        if !self.enabled || matches!(self.engine, HighlightEngine::Disabled) {
            self.clear();
            return;
        }
        self.sync_tick = self.sync_tick.wrapping_add(1);
        let tick = self.sync_tick;
        let visible = visible_code_blocks(projection);
        let visible_ids = visible.iter().map(|item| item.0).collect::<HashSet<_>>();
        let mut hits = 0usize;
        let mut misses = 0usize;
        let engine_revision = self.engine.revision();
        let theme_key = self.engine.theme_key(selected_theme);
        for (block_id, content_version, source, language) in visible {
            if let Some(entry) = self.entries.get_mut(&block_id)
                && entry.matches(content_version, &language, engine_revision, theme_key)
            {
                entry.last_used_tick = tick;
                hits += 1;
                continue;
            }
            misses += 1;
            let fallback = self
                .entries
                .remove(&block_id)
                .map(|entry| rebase_spans(&entry.source, entry.spans(), &source))
                .unwrap_or_else(|| vec![InlineSpan::plain(source)]);
            let request = HighlightRequest {
                content_version,
                source: source.to_owned(),
                language,
                engine_revision,
                theme_key,
                fallback,
                engine: self.engine.clone(),
            };
            let mut entry = CodeHighlightEntry::new(request, cx);
            entry.last_used_tick = tick;
            self.entries.insert(block_id, entry);
        }

        let mut evicted = 0usize;
        let max_entries = visible_ids
            .len()
            .saturating_add(MAX_RETAINED_OFFSCREEN_ENTRIES);
        while self.entries.len() > max_entries {
            let Some(oldest_id) = self
                .entries
                .iter()
                .filter(|(id, _)| !visible_ids.contains(id))
                .min_by_key(|(_, entry)| entry.last_used_tick)
                .map(|(id, _)| *id)
            else {
                break;
            };
            self.entries.remove(&oldest_id);
            evicted += 1;
        }
        if trace {
            eprintln!(
                "[cditor][syntax-highlight] cache-sync visible={} hits={} misses={} evicted={} cached={} elapsed_us={}",
                hits + misses,
                hits,
                misses,
                evicted,
                self.entries.len(),
                started.elapsed().as_micros()
            );
        }
    }

    pub(crate) fn spans(&self, block_id: BlockId) -> Option<&[InlineSpan]> {
        self.entries.get(&block_id).map(CodeHighlightEntry::spans)
    }

    pub(crate) fn theme_item(&self, selected_theme: &str) -> CodeThemeItem {
        if !self.enabled {
            return code_theme_item(selected_theme);
        }
        match &self.engine {
            HighlightEngine::External(provider) => theme::external_theme_item(provider.palette()),
            #[cfg(feature = "builtin-syntax-highlighting")]
            HighlightEngine::Builtin => code_theme_item(selected_theme),
            HighlightEngine::Disabled => code_theme_item(selected_theme),
        }
    }

    pub(crate) fn uses_builtin_themes(&self) -> bool {
        if !self.enabled {
            return false;
        }
        #[cfg(feature = "builtin-syntax-highlighting")]
        if matches!(self.engine, HighlightEngine::Builtin) {
            return true;
        }
        false
    }

    pub(crate) fn language_items(&self) -> Vec<CodeLanguageItem> {
        match &self.engine {
            HighlightEngine::External(provider) => {
                language::provider_language_items(provider.languages())
            }
            #[cfg(feature = "builtin-syntax-highlighting")]
            HighlightEngine::Builtin => code_language_items(),
            HighlightEngine::Disabled => code_language_items(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}
