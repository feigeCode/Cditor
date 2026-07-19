use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled,
    StyledImage, div, img, px, rgb,
};
use html5ever::tendril::TendrilSink;
use html5ever::{LocalName, ParseOpts, parse_document};
use markup5ever_rcdom::{Node, NodeData, RcDom};

use crate::gui::GuiTheme;
use crate::gui::image_loader::{
    ImagePlaceholder, ImagePlaceholderState, RenderImageLoadState, gpui_image_source,
    load_render_image_state_from_base, should_use_native_image_source,
};
use crate::gui::platform::EDITOR_MONO_FONT_FAMILY;

pub(crate) const HTML_PREVIEW_TEXT_SIZE_PX: f32 = 16.0;
const HTML_IMAGE_FALLBACK_WIDTH_PX: f32 = 180.0;
const HTML_IMAGE_PLACEHOLDER_HEIGHT_PX: f32 = 96.0;

pub(crate) fn html_source_editor_visible(
    source_mode: bool,
    readonly: bool,
    suppress_text_input: bool,
) -> bool {
    source_mode && !readonly && !suppress_text_input
}

/// Render the focused HTML block as a natural-height source editor.
///
/// The source editor is supplied by the normal rich-text input path so caret,
/// selection, IME, undo, and persistence remain identical to code blocks.
pub(crate) fn render_html_source_editor(
    block_id: u64,
    source_editor: AnyElement,
    theme: GuiTheme,
    view: Entity<crate::gui::app::CditorV2View>,
) -> AnyElement {
    div()
        .id(("html-render-block", block_id))
        .w_full()
        .flex()
        .flex_col()
        .rounded(px(5.0))
        .border_1()
        .border_color(rgb(theme.border))
        .overflow_hidden()
        .child(
            div()
                .relative()
                .w_full()
                .bg(rgb(theme.code_background))
                .font_family(EDITOR_MONO_FONT_FAMILY)
                .text_size(px(13.0))
                .text_color(rgb(theme.code_text))
                .child(
                    div()
                        .px(px(10.0))
                        .pt(px(42.0))
                        .pb(px(10.0))
                        .child(source_editor),
                )
                .child(render_html_editor_toolbar(block_id, theme, view.clone())),
        )
        .into_any_element()
}

fn render_html_editor_toolbar(
    block_id: u64,
    theme: GuiTheme,
    view: Entity<crate::gui::app::CditorV2View>,
) -> AnyElement {
    div()
        .absolute()
        .top(px(6.0))
        .right(px(8.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(html_editor_button(
            block_id,
            "html-editor-preview",
            "预览",
            theme,
            view.clone(),
            |view, block_id, cx| view.preview_html_block_from_gui(block_id, cx),
        ))
        .child(html_editor_button(
            block_id,
            "html-editor-save",
            "保存",
            theme,
            view,
            |view, block_id, cx| view.save_html_block_from_gui(block_id, cx),
        ))
        .into_any_element()
}

fn html_editor_button(
    block_id: u64,
    id: &'static str,
    label: &'static str,
    theme: GuiTheme,
    view: Entity<crate::gui::app::CditorV2View>,
    action: fn(
        &mut crate::gui::app::CditorV2View,
        u64,
        &mut gpui::Context<crate::gui::app::CditorV2View>,
    ),
) -> AnyElement {
    div()
        .id((id, block_id))
        .px(px(7.0))
        .py(px(3.0))
        .rounded(px(3.0))
        .bg(rgb(theme.code_toolbar_background))
        .border_1()
        .border_color(rgb(theme.code_toolbar_border))
        .text_color(rgb(theme.code_toolbar_text))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(theme.code_toolbar_hover)))
        .child(label)
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            let _ = view.update(cx, |view, cx| action(view, block_id, cx));
            cx.stop_propagation();
        })
        .into_any_element()
}

pub(crate) fn render_html_block(
    block_id: u64,
    html: &str,
    theme: GuiTheme,
    media_base_path: Option<&Path>,
    cx: &mut App,
) -> AnyElement {
    let sanitized = cditor_runtime::sanitize_external_html(
        html,
        cditor_runtime::ExternalContentPolicy {
            remote_image_policy: cditor_runtime::RemoteResourcePolicy::Allow,
            file_url_policy: cditor_runtime::FileUrlPolicy::Block,
            ..Default::default()
        },
    );
    let dom = parse_html(&sanitized.html);
    render_node(&dom.document, block_id, theme, media_base_path, cx, 0)
}

fn parse_html(source: &str) -> RcDom {
    parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .read_from(&mut std::io::Cursor::new(source.as_bytes()))
        .unwrap_or_default()
}

fn render_node(
    node: &Rc<Node>,
    block_id: u64,
    theme: GuiTheme,
    media_base_path: Option<&Path>,
    cx: &mut App,
    depth: usize,
) -> AnyElement {
    match &node.data {
        NodeData::Document => div()
            .id(("html", block_id))
            .w_full()
            .flex()
            .flex_col()
            .text_size(px(HTML_PREVIEW_TEXT_SIZE_PX))
            .text_color(rgb(theme.text))
            .children(
                node.children
                    .borrow()
                    .iter()
                    .enumerate()
                    .map(|(index, child)| {
                        render_node(
                            child,
                            block_id.wrapping_add(index as u64),
                            theme,
                            media_base_path,
                            cx,
                            depth,
                        )
                    }),
            )
            .into_any_element(),
        NodeData::Text { contents } => {
            let text = contents
                .borrow()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if text.is_empty() {
                div().into_any_element()
            } else {
                div().child(text).into_any_element()
            }
        }
        NodeData::Element { name, attrs, .. } => {
            let children = node.children.borrow();
            let elements = children
                .iter()
                .enumerate()
                .map(|(index, child)| {
                    render_node(
                        child,
                        block_id.wrapping_add(index as u64),
                        theme,
                        media_base_path,
                        cx,
                        depth + 1,
                    )
                })
                .collect::<Vec<_>>();
            let tag = name.local.as_ref();
            let mut element = match tag {
                "h1" => div()
                    .text_size(px(32.0))
                    .font_weight(gpui::FontWeight::BOLD),
                "h2" => div()
                    .text_size(px(26.0))
                    .font_weight(gpui::FontWeight::BOLD),
                "h3" => div()
                    .text_size(px(21.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD),
                "h4" => div()
                    .text_size(px(18.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD),
                "h5" => div()
                    .text_size(px(16.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD),
                "h6" => div()
                    .text_size(px(14.0))
                    .font_weight(gpui::FontWeight::MEDIUM),
                "blockquote" => div()
                    .border_l_3()
                    .border_color(rgb(theme.quote_bar))
                    .pl(px(14.0))
                    .text_color(rgb(theme.muted)),
                "code" | "pre" => div()
                    .rounded(px(4.0))
                    .bg(rgb(theme.code_background))
                    .px(px(8.0))
                    .py(px(4.0)),
                "hr" => div().h(px(1.0)).my(px(12.0)).bg(rgb(theme.border)),
                "br" => div().h(px(10.0)),
                "img" => {
                    let src = attr(attrs, "src").unwrap_or_default();
                    let alt = attr(attrs, "alt").unwrap_or_default();
                    let requested_width = attr(attrs, "width")
                        .as_deref()
                        .and_then(html_image_requested_width);
                    return render_html_image(
                        HtmlImageRender {
                            source: &src,
                            alt: &alt,
                            requested_width,
                            theme,
                            media_base_path,
                        },
                        cx,
                    );
                }
                "a" => div().text_color(rgb(theme.focused)),
                "strong" | "b" => div().font_weight(gpui::FontWeight::BOLD),
                "em" | "i" => div().italic(),
                "del" | "s" => div().text_color(rgb(theme.muted)),
                _ => div(),
            };
            if matches!(
                tag,
                "div"
                    | "section"
                    | "article"
                    | "main"
                    | "header"
                    | "footer"
                    | "body"
                    | "html"
                    | "ul"
                    | "ol"
                    | "li"
                    | "figure"
                    | "figcaption"
                    | "table"
                    | "tr"
            ) {
                element = element.flex().flex_col();
            }
            if matches!(tag, "p" | "a" | "strong" | "b" | "em" | "i")
                || tag.starts_with('h') && tag.len() == 2
            {
                element = element.flex().flex_row().flex_wrap().items_center();
            }
            if attr(attrs, "align").is_some_and(|align| align.eq_ignore_ascii_case("center")) {
                element = element.items_center().text_center();
            }
            if matches!(
                tag,
                "p" | "div"
                    | "section"
                    | "article"
                    | "main"
                    | "header"
                    | "footer"
                    | "ul"
                    | "ol"
                    | "li"
                    | "figure"
                    | "figcaption"
                    | "table"
                    | "tr"
            ) {
                element = element.pb(px(8.0));
            }
            element.children(elements).into_any_element()
        }
        _ => div().into_any_element(),
    }
}

struct HtmlImageRender<'a> {
    source: &'a str,
    alt: &'a str,
    requested_width: Option<f32>,
    theme: GuiTheme,
    media_base_path: Option<&'a Path>,
}

fn render_html_image(config: HtmlImageRender<'_>, cx: &mut App) -> AnyElement {
    let fallback_width = config
        .requested_width
        .unwrap_or(HTML_IMAGE_FALLBACK_WIDTH_PX);
    if should_use_native_image_source(config.source) {
        return render_native_html_image(config, fallback_width);
    }
    match load_render_image_state_from_base(config.source, config.media_base_path, cx) {
        RenderImageLoadState::Ready(image) => {
            let size = image.size(0);
            let width = config
                .requested_width
                .unwrap_or_else(|| i32::from(size.width).max(1) as f32);
            let aspect = i32::from(size.height).max(1) as f32 / i32::from(size.width).max(1) as f32;
            img(image)
                .w(px(width))
                .h(px(width * aspect))
                .max_w(gpui::relative(1.0))
                .object_fit(gpui::ObjectFit::Contain)
                .into_any_element()
        }
        state => div()
            .w(px(fallback_width))
            .max_w(gpui::relative(1.0))
            .child(
                ImagePlaceholder::for_load_state(config.source, config.alt, config.theme, &state)
                    .expect("non-ready image state must have a placeholder")
                    .height(HTML_IMAGE_PLACEHOLDER_HEIGHT_PX),
            )
            .into_any_element(),
    }
}

fn render_native_html_image(config: HtmlImageRender<'_>, fallback_width: f32) -> AnyElement {
    let fallback_source = config.source.to_owned();
    let fallback_alt = config.alt.to_owned();
    let theme = config.theme;
    let loading_source = fallback_source.clone();
    let loading_alt = fallback_alt.clone();
    let mut image = img(gpui_image_source(config.source, config.media_base_path))
        .max_w(gpui::relative(1.0))
        .object_fit(gpui::ObjectFit::Contain)
        .with_loading(move || {
            div()
                .w(px(fallback_width))
                .max_w(gpui::relative(1.0))
                .child(
                    ImagePlaceholder::new(
                        loading_source.clone(),
                        theme,
                        ImagePlaceholderState::Loading,
                    )
                    .alt(loading_alt.clone())
                    .height(HTML_IMAGE_PLACEHOLDER_HEIGHT_PX),
                )
                .into_any_element()
        })
        .with_fallback(move || {
            div()
                .w(px(fallback_width))
                .max_w(gpui::relative(1.0))
                .child(
                    ImagePlaceholder::new(
                        fallback_source.clone(),
                        theme,
                        ImagePlaceholderState::Failed,
                    )
                    .alt(fallback_alt.clone())
                    .height(HTML_IMAGE_PLACEHOLDER_HEIGHT_PX),
                )
                .into_any_element()
        });
    if let Some(width) = config.requested_width {
        image = image.w(px(width));
    }
    image.into_any_element()
}

fn attr(attrs: &RefCell<Vec<html5ever::Attribute>>, name: &str) -> Option<String> {
    attrs.borrow().iter().find_map(|attribute| {
        (attribute.name.local == LocalName::from(name)).then(|| attribute.value.to_string())
    })
}

fn html_image_requested_width(value: &str) -> Option<f32> {
    value.parse::<f32>().ok().map(|width| width.max(16.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_preview_keeps_typora_body_size() {
        assert_eq!(HTML_PREVIEW_TEXT_SIZE_PX, 16.0);
    }

    #[test]
    fn html_images_only_use_explicit_width_attributes() {
        assert_eq!(html_image_requested_width("120"), Some(120.0));
        assert_eq!(html_image_requested_width("8"), Some(16.0));
        assert_eq!(html_image_requested_width("auto"), None);
    }

    #[test]
    fn html_source_editor_only_appears_for_an_editable_focused_block() {
        assert!(html_source_editor_visible(true, false, false));
        assert!(!html_source_editor_visible(false, false, false));
        assert!(!html_source_editor_visible(true, true, false));
        assert!(!html_source_editor_visible(true, false, true));
    }
}
