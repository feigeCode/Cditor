use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, OnceLock};

use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayloadView, RichBlockKind};
use cditor_runtime::EditorViewProjection;
use gpui::{AppContext, Context, RenderImage, Task};

use crate::api::{DocumentRenderRequest, DocumentRenderTheme, DocumentRendererProvider};
use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;

#[cfg(feature = "builtin-mermaid-rendering")]
use super::theme::build_mermaid_theme;

const MAX_MERMAID_SOURCE_BYTES: usize = 256 * 1024;
const MAX_SVG_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_RENDER_WIDTH_PX: f32 = 720.0;

type RenderResult = Result<Arc<RenderImage>, Arc<str>>;

#[derive(Clone)]
pub(crate) enum DocumentRenderStatus {
    Ready(Arc<RenderImage>),
    Rendering { fallback: Option<Arc<RenderImage>> },
    Failed { message: Arc<str> },
}

struct DocumentRenderEntry {
    renderer_id: &'static str,
    content_version: u64,
    source_hash: u64,
    theme: GuiTheme,
    provider_revision: u64,
    result: Arc<OnceLock<RenderResult>>,
    fallback: Option<Arc<RenderImage>>,
    _task: Option<Task<()>>,
}

impl DocumentRenderEntry {
    fn new(
        renderer_id: &'static str,
        content_version: u64,
        source_hash: u64,
        source: String,
        theme: GuiTheme,
        fallback: Option<Arc<RenderImage>>,
        provider: Option<Arc<dyn DocumentRendererProvider>>,
        cx: &mut Context<CditorV2View>,
    ) -> Self {
        let result = Arc::new(OnceLock::new());
        if let Err(message) = validate_source(&source) {
            let provider_revision = provider.as_ref().map_or(0, |provider| provider.revision());
            let _ = result.set(Err(message.into()));
            return Self {
                renderer_id,
                content_version,
                source_hash,
                theme,
                provider_revision,
                result,
                fallback,
                _task: None,
            };
        }

        let result_for_task = result.clone();
        let renderer = cx.svg_renderer();
        let provider_revision = provider.as_ref().map_or(0, |provider| provider.revision());
        let task = cx.spawn(async move |view, cx| {
            let rendered = cx
                .background_spawn(async move {
                    let svg = render_svg(provider, renderer_id, source, theme).await?;
                    validate_svg(&svg)?;
                    renderer
                        .render_single_frame(&svg, 1.0)
                        .map_err(|error| Arc::<str>::from(error.to_string()))
                })
                .await;
            let _ = result_for_task.set(rendered);
            let _ = view.update(cx, |_view, cx| cx.notify());
        });

        Self {
            renderer_id,
            content_version,
            source_hash,
            theme,
            provider_revision,
            result,
            fallback,
            _task: Some(task),
        }
    }

    fn status(&self) -> DocumentRenderStatus {
        match self.result.get() {
            Some(Ok(image)) => DocumentRenderStatus::Ready(image.clone()),
            Some(Err(message)) => DocumentRenderStatus::Failed {
                message: message.clone(),
            },
            None => DocumentRenderStatus::Rendering {
                fallback: self.fallback.clone(),
            },
        }
    }

    pub(crate) fn best_image(&self) -> Option<Arc<RenderImage>> {
        match self.result.get() {
            Some(Ok(image)) => Some(image.clone()),
            _ => self.fallback.clone(),
        }
    }

    fn matches(
        &self,
        content_version: u64,
        source_hash: u64,
        theme: GuiTheme,
        provider_revision: u64,
        renderer_id: &str,
    ) -> bool {
        self.content_version == content_version
            && self.source_hash == source_hash
            && self.theme == theme
            && self.provider_revision == provider_revision
            && self.renderer_id == renderer_id
    }
}

#[derive(Default)]
pub(crate) struct DocumentRenderCache {
    entries: HashMap<BlockId, DocumentRenderEntry>,
    provider: Option<Arc<dyn DocumentRendererProvider>>,
}

impl DocumentRenderCache {
    pub(crate) fn sync_visible_window(
        &mut self,
        projection: &EditorViewProjection,
        theme: GuiTheme,
        cx: &mut Context<CditorV2View>,
    ) {
        let visible = projection
            .blocks
            .iter()
            .filter(|block| matches!(block.kind, RichBlockKind::Mermaid | RichBlockKind::Math))
            .filter_map(|block| {
                let BlockPayloadView::Loaded(payload) = &block.payload else {
                    return None;
                };
                let source = payload.plain_text();
                Some((
                    block.block_id,
                    payload.content_version,
                    source_hash(&source),
                    source,
                    if matches!(block.kind, RichBlockKind::Math) {
                        "math"
                    } else {
                        "mermaid"
                    },
                ))
            })
            .collect::<Vec<_>>();
        let visible_ids = visible
            .iter()
            .map(|(block_id, _, _, _, _)| *block_id)
            .collect::<HashSet<_>>();
        self.entries
            .retain(|block_id, _| visible_ids.contains(block_id));

        for (block_id, content_version, hash, source, renderer_id) in visible {
            if self.entries.get(&block_id).is_some_and(|entry| {
                entry.matches(
                    content_version,
                    hash,
                    theme,
                    self.provider
                        .as_ref()
                        .map_or(0, |provider| provider.revision()),
                    renderer_id,
                )
            }) {
                continue;
            }
            let fallback = self
                .entries
                .remove(&block_id)
                .and_then(|entry| entry.best_image());
            self.entries.insert(
                block_id,
                DocumentRenderEntry::new(
                    renderer_id,
                    content_version,
                    hash,
                    source,
                    theme,
                    fallback,
                    self.provider.clone(),
                    cx,
                ),
            );
        }
    }

    pub(crate) fn status(&self, block_id: BlockId) -> Option<DocumentRenderStatus> {
        self.entries.get(&block_id).map(DocumentRenderEntry::status)
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn configure(&mut self, provider: Option<Arc<dyn DocumentRendererProvider>>) {
        self.provider = provider;
        self.clear();
    }
}

async fn render_svg(
    provider: Option<Arc<dyn DocumentRendererProvider>>,
    renderer_id: &'static str,
    source: String,
    theme: GuiTheme,
) -> Result<Vec<u8>, Arc<str>> {
    if let Some(provider) = provider.filter(|provider| provider.supports(renderer_id)) {
        let artifact = provider
            .render(DocumentRenderRequest {
                renderer: renderer_id.to_owned(),
                source,
                available_width: DEFAULT_RENDER_WIDTH_PX,
                scale_factor: 1.0,
                theme: DocumentRenderTheme {
                    dark: false,
                    background: theme.code_background,
                    foreground: theme.text,
                    border: theme.strong_border,
                    muted: theme.muted,
                    accent: theme.action_accent,
                    danger: theme.danger,
                    font_family: "Inter, ui-sans-serif, system-ui, -apple-system, sans-serif"
                        .to_owned(),
                },
            })
            .await
            .map_err(|error| Arc::<str>::from(error.to_string()))?;
        if artifact.media_type != "image/svg+xml" {
            return Err("文档渲染扩展返回了不支持的媒体类型".into());
        }
        return Ok(artifact.bytes);
    }
    #[cfg(feature = "builtin-mermaid-rendering")]
    {
        if renderer_id != "mermaid" {
            return Err("未安装数学公式渲染扩展".into());
        }
        let render_theme = build_mermaid_theme(theme);
        return mermaid_render::render_to_svg(&source, &render_theme)
            .map(|svg| svg.into_bytes())
            .map_err(|error| Arc::<str>::from(format!("{error:#}")));
    }
    #[cfg(not(feature = "builtin-mermaid-rendering"))]
    Err(if renderer_id == "math" {
        "未安装数学公式渲染扩展".into()
    } else {
        "未安装 Mermaid 文档渲染扩展".into()
    })
}

fn validate_svg(svg: &[u8]) -> Result<(), Arc<str>> {
    if svg.len() > MAX_SVG_BYTES {
        return Err("SVG 输出超过 4 MiB 安全上限".into());
    }
    let text = std::str::from_utf8(svg).map_err(|_| Arc::<str>::from("SVG 输出不是 UTF-8"))?;
    let lower = text.to_ascii_lowercase();
    if !lower.contains("<svg")
        || lower.contains("<script")
        || lower.contains("<foreignobject")
        || lower.contains("file://")
        || lower.contains("javascript:")
        || contains_external_resource_url(&lower)
        || contains_event_attribute(&lower)
    {
        return Err("SVG 输出包含不安全或不受支持的内容".into());
    }
    Ok(())
}

fn contains_event_attribute(svg: &str) -> bool {
    svg.split_ascii_whitespace().any(|token| {
        let token = token.trim_start_matches('<');
        token.starts_with("on")
            && token.split_once('=').is_some_and(|(name, _)| {
                name.len() > 2 && name[2..].chars().all(|ch| ch.is_ascii_alphabetic())
            })
    })
}

fn contains_external_resource_url(svg: &str) -> bool {
    ["href=", "src=", "url(", "xlink:href="]
        .iter()
        .any(|marker| {
            svg.match_indices(marker).any(|(index, _)| {
                let value = svg[index + marker.len()..].trim_start_matches([' ', '\'', '"']);
                value.starts_with("http://") || value.starts_with("https://")
            })
        })
}

fn source_hash(source: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

fn validate_source(source: &str) -> Result<(), String> {
    if source.trim().is_empty() {
        return Err("渲染源码为空".to_owned());
    }
    if source.len() > MAX_MERMAID_SOURCE_BYTES {
        return Err(format!(
            "渲染源码超过 {} KiB 安全上限",
            MAX_MERMAID_SOURCE_BYTES / 1024
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_validation_rejects_empty_and_oversized_input() {
        assert!(validate_source("  \n").is_err());
        assert!(validate_source("flowchart TD\n A --> B").is_ok());
        assert!(validate_source(&"x".repeat(MAX_MERMAID_SOURCE_BYTES + 1)).is_err());
    }

    #[test]
    fn source_hash_changes_with_content() {
        assert_eq!(source_hash("A --> B"), source_hash("A --> B"));
        assert_ne!(source_hash("A --> B"), source_hash("A --> C"));
    }

    #[test]
    fn svg_validation_rejects_active_and_external_content() {
        assert!(validate_svg(b"<svg><path d='M0 0'/></svg>").is_ok());
        assert!(
            validate_svg(b"<svg xmlns='http://www.w3.org/2000/svg'><path d='M0 0'/></svg>").is_ok()
        );
        assert!(validate_svg(b"<svg><script/></svg>").is_err());
        assert!(validate_svg(b"<svg><foreignObject/></svg>").is_err());
        assert!(validate_svg(b"<svg><image href='https://example.com/a'/></svg>").is_err());
        assert!(validate_svg(b"<svg onmouseover='alert(1)'></svg>").is_err());
    }
}
