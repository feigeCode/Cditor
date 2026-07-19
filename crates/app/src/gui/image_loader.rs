//! Async image loading and rendering for image blocks and cover-style media.
//!
//! Ported from V1 CoverImages: sources are decoded into `gpui::RenderImage`, cached
//! by source string, and painted with a custom element so `ObjectFit::Cover` can
//! use the same vertical crop positioning semantics as V1.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use futures::AsyncReadExt;
use gpui::{
    AnyElement, App, Bounds, Corners, DevicePixels, Element, ElementId, GlobalElementId,
    ImageSource, InspectorElementId, IntoElement, LayoutId, ObjectFit, ParentElement, Pixels,
    RenderImage, RenderOnce, Size, Style, Styled, Window, div, point, px, relative, rgb, size,
};

use crate::gui::GuiTheme;

#[derive(Clone)]
enum ImageState {
    Loading,
    Ready(Arc<RenderImage>),
    Failed,
}

#[derive(Clone, Debug)]
pub enum RenderImageLoadState {
    Loading,
    Ready(Arc<RenderImage>),
    Failed,
}

impl RenderImageLoadState {
    pub fn placeholder_state(&self) -> Option<ImagePlaceholderState> {
        match self {
            Self::Loading => Some(ImagePlaceholderState::Loading),
            Self::Ready(_) => None,
            Self::Failed => Some(ImagePlaceholderState::Failed),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImagePlaceholderState {
    Loading,
    Failed,
}

#[derive(Clone, IntoElement)]
pub struct ImagePlaceholder {
    source: String,
    alt: String,
    theme: GuiTheme,
    height: f32,
    compact: bool,
    state: ImagePlaceholderState,
}

impl ImagePlaceholder {
    pub fn new(source: impl Into<String>, theme: GuiTheme, state: ImagePlaceholderState) -> Self {
        Self {
            source: source.into(),
            alt: String::new(),
            theme,
            height: 96.0,
            compact: false,
            state,
        }
    }

    pub fn alt(mut self, alt: impl Into<String>) -> Self {
        self.alt = alt.into();
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }
}

impl RenderOnce for ImagePlaceholder {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let title = image_placeholder_title(&self.source, &self.alt);
        let status = match self.state {
            ImagePlaceholderState::Loading => "图片加载中",
            ImagePlaceholderState::Failed => "图片加载失败",
        };
        if self.compact {
            return render_compact_image_placeholder(&self, title, status);
        }
        render_regular_image_placeholder(&self, title, status)
    }
}

fn render_compact_image_placeholder(
    placeholder: &ImagePlaceholder,
    title: String,
    status: &'static str,
) -> AnyElement {
    div()
        .w_full()
        .h(px(placeholder.height))
        .rounded(px(4.0))
        .border_1()
        .border_color(rgb(placeholder.theme.border))
        .bg(rgb(placeholder.theme.hover_surface))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .text_color(rgb(placeholder.theme.muted))
        .child(div().text_size(px(9.0)).child("IMG"))
        .child(
            div()
                .min_w(px(0.0))
                .text_size(px(10.0))
                .text_ellipsis()
                .whitespace_nowrap()
                .child(format!("{title} · {status}")),
        )
        .into_any_element()
}

fn render_regular_image_placeholder(
    placeholder: &ImagePlaceholder,
    title: String,
    status: &'static str,
) -> AnyElement {
    div()
        .w_full()
        .h(px(placeholder.height))
        .rounded(px(4.0))
        .border_1()
        .border_color(rgb(placeholder.theme.border))
        .bg(rgb(placeholder.theme.hover_surface))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .text_color(rgb(placeholder.theme.muted))
        .child(render_image_placeholder_icon(placeholder.theme))
        .child(
            div()
                .max_w(relative(0.8))
                .text_size(px(12.0))
                .text_ellipsis()
                .whitespace_nowrap()
                .child(title),
        )
        .child(div().text_size(px(10.0)).child(status))
        .into_any_element()
}

fn image_cache() -> &'static Mutex<HashMap<String, ImageState>> {
    static CACHE: OnceLock<Mutex<HashMap<String, ImageState>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Resolve a decoded image for `src`, kicking off an off-UI-thread load on first use.
///
/// Returns `Some` once the image is decoded and cached. While loading (or after a
/// failure) it returns `None`, letting the caller render a stable placeholder.
pub fn load_render_image(src: &str, cx: &mut App) -> Option<Arc<RenderImage>> {
    load_render_image_from_base(src, None, cx)
}

pub fn load_render_image_from_base(
    src: &str,
    base_path: Option<&Path>,
    cx: &mut App,
) -> Option<Arc<RenderImage>> {
    match load_render_image_state_from_base(src, base_path, cx) {
        RenderImageLoadState::Ready(image) => Some(image),
        RenderImageLoadState::Loading | RenderImageLoadState::Failed => None,
    }
}

pub fn load_render_image_state_from_base(
    src: &str,
    base_path: Option<&Path>,
    cx: &mut App,
) -> RenderImageLoadState {
    let src = resolve_render_image_source(src, base_path);
    if src.trim().is_empty() {
        return RenderImageLoadState::Failed;
    }

    if let Some(state) = image_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&src).cloned())
    {
        return match state {
            ImageState::Ready(image) => RenderImageLoadState::Ready(image),
            ImageState::Loading => RenderImageLoadState::Loading,
            ImageState::Failed => RenderImageLoadState::Failed,
        };
    }

    if let Ok(mut cache) = image_cache().lock() {
        cache.insert(src.clone(), ImageState::Loading);
    }

    let async_cx = cx.to_async();
    let http_client = cx.http_client();
    let executor = cx.background_executor().clone();
    cx.foreground_executor()
        .spawn(async move {
            let fetch_src = src.clone();
            let bytes = if fetch_src.starts_with("http://") || fetch_src.starts_with("https://") {
                fetch_remote_image_bytes(&fetch_src, http_client).await
            } else {
                executor
                    .spawn(async move { std::fs::read(parse_local_path(&fetch_src)).ok() })
                    .await
            };
            let state = match bytes {
                Some(bytes) => executor
                    .spawn(async move { decode_render_image(&bytes) })
                    .await
                    .map_or(ImageState::Failed, ImageState::Ready),
                None => ImageState::Failed,
            };
            if let Ok(mut cache) = image_cache().lock() {
                cache.insert(src, state);
            }
            async_cx.update(App::refresh_windows);
        })
        .detach();

    RenderImageLoadState::Loading
}

fn image_placeholder_title(source: &str, alt: &str) -> String {
    if !alt.trim().is_empty() {
        alt.trim().to_owned()
    } else {
        source
            .split(['/', '\\'])
            .next_back()
            .filter(|name| !name.is_empty())
            .unwrap_or("图片")
            .to_owned()
    }
}

fn render_image_placeholder_icon(theme: GuiTheme) -> AnyElement {
    div()
        .w(px(30.0))
        .h(px(22.0))
        .rounded(px(3.0))
        .border_1()
        .border_color(rgb(theme.muted))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(9.0))
        .child("IMG")
        .into_any_element()
}

pub fn resolve_render_image_source(src: &str, base_path: Option<&Path>) -> String {
    let src = src.trim();
    if src.is_empty()
        || src.starts_with("http://")
        || src.starts_with("https://")
        || src.starts_with("data:")
        || src.starts_with("asset:")
    {
        return src.to_owned();
    }

    let path = parse_local_path(src);
    if path.is_absolute() {
        return path.to_string_lossy().into_owned();
    }
    resolve_relative_image_path(&path, base_path, std::env::current_dir().ok().as_deref())
        .to_string_lossy()
        .into_owned()
}

fn resolve_relative_image_path(
    path: &Path,
    base_path: Option<&Path>,
    working_directory: Option<&Path>,
) -> PathBuf {
    if let Some(base_path) = base_path {
        let document_relative = base_path.join(path);
        if document_relative.exists() {
            return document_relative;
        }
        if let Some(working_directory) = working_directory {
            let working_relative = working_directory.join(path);
            if working_relative.exists() {
                return working_relative;
            }
        }
        return document_relative;
    }
    working_directory
        .map(|working_directory| working_directory.join(path))
        .unwrap_or_else(|| path.to_path_buf())
}

pub fn gpui_image_source(src: &str, base_path: Option<&Path>) -> ImageSource {
    let resolved = resolve_render_image_source(src, base_path);
    if resolved.starts_with("http://")
        || resolved.starts_with("https://")
        || resolved.starts_with("data:")
    {
        resolved.into()
    } else {
        PathBuf::from(resolved).into()
    }
}

pub fn is_svg_image_source(src: &str) -> bool {
    src.split(['?', '#'])
        .next()
        .is_some_and(|path| path.to_ascii_lowercase().ends_with(".svg"))
}

/// GPUI can decode remote images from their response content, including SVG
/// endpoints such as shields.io that do not expose a `.svg` suffix. Keep
/// explicit raster files on the decoded-image path so their intrinsic sizing
/// and resize behavior remain available.
pub fn should_use_native_image_source(src: &str) -> bool {
    let source = src.trim();
    if source.starts_with("data:") || is_svg_image_source(source) {
        return true;
    }
    if !(source.starts_with("http://") || source.starts_with("https://")) {
        return false;
    }
    let path = source
        .split(['?', '#'])
        .next()
        .unwrap_or(source)
        .rsplit('/')
        .next()
        .unwrap_or_default();
    let Some(extension) = path.rsplit_once('.').map(|(_, extension)| extension) else {
        return true;
    };
    !matches!(
        extension.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tif" | "tiff"
    )
}

async fn fetch_remote_image_bytes(
    src: &str,
    http_client: Arc<dyn gpui::http_client::HttpClient>,
) -> Option<Vec<u8>> {
    let response = http_client
        .get(src, gpui::http_client::AsyncBody::default(), true)
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let mut body = response.into_body();
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes).await.ok()?;
    Some(bytes)
}

fn parse_local_path(src: &str) -> PathBuf {
    let raw = src.strip_prefix("file://").unwrap_or(src);
    if let Some(rest) = raw.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(raw)
}

fn decode_render_image(bytes: &[u8]) -> Option<Arc<RenderImage>> {
    let format = image::guess_format(bytes).ok()?;
    let mut data = image::load_from_memory_with_format(bytes, format)
        .ok()?
        .into_rgba8();
    // gpui paints premultiplied BGRA; V1 swaps R/B after decoding to RGBA.
    for pixel in data.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new([image::Frame::new(data)])))
}

pub struct RasterImageElement {
    image: Arc<RenderImage>,
    fit: ObjectFit,
    radius: Pixels,
    cover_position_y: Option<f32>,
}

impl RasterImageElement {
    #[must_use]
    pub fn new(image: Arc<RenderImage>, fit: ObjectFit, radius: Pixels) -> Self {
        Self {
            image,
            fit,
            radius,
            cover_position_y: None,
        }
    }

    #[must_use]
    pub fn cover(image: Arc<RenderImage>, radius: Pixels, position_y: f32) -> Self {
        Self {
            image,
            fit: ObjectFit::Cover,
            radius,
            cover_position_y: Some(position_y.clamp(0.0, 1.0)),
        }
    }
}

fn positioned_cover_bounds(
    container: Bounds<Pixels>,
    image_size: Size<DevicePixels>,
    position_y: f32,
) -> Bounds<Pixels> {
    let image_width = (image_size.width.0 as f32).max(1.0);
    let image_height = (image_size.height.0 as f32).max(1.0);
    let container_width = f32::from(container.size.width).max(1.0);
    let container_height = f32::from(container.size.height).max(1.0);
    let scale = (container_width / image_width).max(container_height / image_height);
    let scaled_width = px(image_width * scale);
    let scaled_height = px(image_height * scale);
    let overflow_x = (scaled_width - container.size.width).max(px(0.0));
    let overflow_y = (scaled_height - container.size.height).max(px(0.0));

    Bounds {
        origin: point(
            container.origin.x - overflow_x / 2.0,
            container.origin.y - overflow_y * position_y.clamp(0.0, 1.0),
        ),
        size: size(scaled_width, scaled_height),
    }
}

impl IntoElement for RasterImageElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for RasterImageElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        (): &mut (),
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        (): &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) {
        if self.image.frame_count() == 0 {
            return;
        }
        let image_bounds = self.cover_position_y.map_or_else(
            || self.fit.get_bounds(bounds, self.image.size(0)),
            |position_y| positioned_cover_bounds(bounds, self.image.size(0), position_y),
        );
        let corner_radii = Corners::all(self.radius).clamp_radii_for_quad_size(image_bounds.size);
        let _ = window.paint_image(image_bounds, corner_radii, self.image.clone(), 0, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_path_parser_strips_file_scheme() {
        assert_eq!(
            parse_local_path("file:///tmp/a.png"),
            PathBuf::from("/tmp/a.png")
        );
        assert_eq!(parse_local_path("/tmp/a.png"), PathBuf::from("/tmp/a.png"));
    }

    #[test]
    fn relative_image_source_resolves_from_document_directory() {
        assert_eq!(
            resolve_render_image_source(
                "resources/navop-icon.png",
                Some(Path::new("/tmp/project"))
            ),
            "/tmp/project/resources/navop-icon.png"
        );
        assert_eq!(
            resolve_render_image_source(
                "https://example.com/badge.svg",
                Some(Path::new("/tmp/project"))
            ),
            "https://example.com/badge.svg"
        );
    }

    #[test]
    fn missing_document_asset_falls_back_to_the_application_working_directory() {
        let temp = tempfile::tempdir().unwrap();
        let document_directory = temp.path().join("notes");
        let working_directory = temp.path().join("project");
        std::fs::create_dir_all(&document_directory).unwrap();
        std::fs::create_dir_all(working_directory.join("resources")).unwrap();
        std::fs::write(working_directory.join("resources/icon.png"), b"png").unwrap();

        assert_eq!(
            resolve_relative_image_path(
                Path::new("resources/icon.png"),
                Some(&document_directory),
                Some(&working_directory),
            ),
            working_directory.join("resources/icon.png")
        );
    }

    #[test]
    fn document_relative_assets_take_priority_over_working_directory_assets() {
        let temp = tempfile::tempdir().unwrap();
        let document_directory = temp.path().join("notes");
        let working_directory = temp.path().join("project");
        std::fs::create_dir_all(document_directory.join("resources")).unwrap();
        std::fs::create_dir_all(working_directory.join("resources")).unwrap();
        std::fs::write(document_directory.join("resources/icon.png"), b"note").unwrap();
        std::fs::write(working_directory.join("resources/icon.png"), b"project").unwrap();

        assert_eq!(
            resolve_relative_image_path(
                Path::new("resources/icon.png"),
                Some(&document_directory),
                Some(&working_directory),
            ),
            document_directory.join("resources/icon.png")
        );
    }

    #[test]
    fn extensionless_remote_images_use_gpui_content_decoding() {
        assert!(should_use_native_image_source(
            "https://img.shields.io/badge/license-Apache--2.0-blue?style=flat"
        ));
        assert!(should_use_native_image_source(
            "https://example.com/icon.svg"
        ));
        assert!(!should_use_native_image_source(
            "https://example.com/icon.png"
        ));
        assert!(!should_use_native_image_source("resources/icon.png"));
    }

    #[test]
    fn image_placeholder_uses_alt_before_the_source_filename() {
        assert_eq!(
            image_placeholder_title("assets/database.png", "Database"),
            "Database"
        );
        assert_eq!(
            image_placeholder_title("assets/database.png", ""),
            "database.png"
        );
        assert_eq!(image_placeholder_title("", ""), "图片");
    }

    #[test]
    fn image_load_states_map_to_loading_and_failed_placeholders_only() {
        assert_eq!(
            RenderImageLoadState::Loading.placeholder_state(),
            Some(ImagePlaceholderState::Loading)
        );
        assert_eq!(
            RenderImageLoadState::Failed.placeholder_state(),
            Some(ImagePlaceholderState::Failed)
        );
        let ready = Arc::new(RenderImage::new([image::Frame::new(
            image::RgbaImage::new(1, 1),
        )]));
        assert_eq!(RenderImageLoadState::Ready(ready).placeholder_state(), None);
    }

    #[test]
    fn positioned_cover_uses_vertical_position() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(100.0), px(100.0)));
        let image_size = size(DevicePixels(100), DevicePixels(200));

        let top = positioned_cover_bounds(bounds, image_size, 0.0);
        let bottom = positioned_cover_bounds(bounds, image_size, 1.0);

        assert!(bottom.origin.y < top.origin.y);
    }
}
