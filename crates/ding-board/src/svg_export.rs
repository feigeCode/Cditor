use std::fmt::Write;
use std::path::{Component, Path};

use super::{Element, ElementKind, Scene, SegmentStyle, bbox, text_extent};

#[derive(Clone, Debug, PartialEq)]
pub struct SvgExportOptions {
    pub padding: f32,
    pub background: Option<u32>,
    pub default_stroke: u32,
    pub default_text: u32,
}

impl Default for SvgExportOptions {
    fn default() -> Self {
        Self {
            padding: 32.0,
            background: Some(0xffffffff),
            default_stroke: 0x24292fff,
            default_text: 0x24292fff,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SvgExportResult {
    pub svg: String,
    pub bounds: [f32; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SvgExportError {
    pub element_id: Option<u64>,
    pub message: String,
}

impl std::fmt::Display for SvgExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(element_id) = self.element_id {
            write!(
                formatter,
                "whiteboard element {element_id}: {}",
                self.message
            )
        } else {
            formatter.write_str(&self.message)
        }
    }
}

impl std::error::Error for SvgExportError {}

pub fn export_scene_svg(
    scene: &Scene,
    options: &SvgExportOptions,
) -> Result<SvgExportResult, SvgExportError> {
    let padding = finite_non_negative(options.padding).ok_or_else(|| SvgExportError {
        element_id: None,
        message: "SVG padding must be finite and non-negative".to_owned(),
    })?;
    let content_bounds = scene_bounds(scene).unwrap_or([0.0, 0.0, 640.0, 360.0]);
    let bounds = [
        content_bounds[0] - padding,
        content_bounds[1] - padding,
        content_bounds[2] + padding,
        content_bounds[3] + padding,
    ];
    let width = (bounds[2] - bounds[0]).max(1.0);
    let height = (bounds[3] - bounds[1]).max(1.0);
    let mut svg = String::new();
    writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}" role="img" aria-label="Cditor whiteboard">"#,
        number(width),
        number(height),
        number(bounds[0]),
        number(bounds[1]),
        number(width),
        number(height)
    )
    .expect("writing to String cannot fail");
    if let Some(background) = options.background {
        let color = svg_color(background);
        writeln!(
            svg,
            r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="{}" fill-opacity="{}"/>"#,
            number(bounds[0]),
            number(bounds[1]),
            number(width),
            number(height),
            color.hex,
            number(color.opacity)
        )
        .expect("writing to String cannot fail");
    }
    for element in &scene.elements {
        render_element(&mut svg, element, options)?;
    }
    svg.push_str("</svg>\n");
    Ok(SvgExportResult { svg, bounds })
}

fn scene_bounds(scene: &Scene) -> Option<[f32; 4]> {
    let mut result: Option<[f32; 4]> = None;
    for element in &scene.elements {
        let (x0, y0, x1, y1) = bbox(&element.kind);
        if ![x0, y0, x1, y1].into_iter().all(f32::is_finite) {
            continue;
        }
        result = Some(match result {
            Some(current) => [
                current[0].min(x0),
                current[1].min(y0),
                current[2].max(x1),
                current[3].max(y1),
            ],
            None => [x0, y0, x1, y1],
        });
    }
    result
}

fn render_element(
    svg: &mut String,
    element: &Element,
    options: &SvgExportOptions,
) -> Result<(), SvgExportError> {
    let stroke = svg_color(element.stroke.unwrap_or(options.default_stroke));
    match &element.kind {
        ElementKind::Draw(draw) => {
            if draw.points.is_empty() {
                return Ok(());
            }
            validate_numbers(element.id, draw.points.iter().flatten().copied())?;
            let points = draw
                .points
                .iter()
                .map(|point| format!("{},{}", number(point[0]), number(point[1])))
                .collect::<Vec<_>>()
                .join(" ");
            writeln!(
                svg,
                r#"  <polyline points="{}" fill="none" stroke="{}" stroke-opacity="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/>"#,
                points,
                stroke.hex,
                number(stroke.opacity),
                number(draw.width.max(0.1))
            )
            .expect("writing to String cannot fail");
        }
        ElementKind::Rect(box_) => {
            validate_box(element.id, box_)?;
            render_box(svg, element, box_, "rect", &stroke, options);
        }
        ElementKind::RoundRect(box_) => {
            validate_box(element.id, box_)?;
            render_box(svg, element, box_, "round_rect", &stroke, options);
        }
        ElementKind::Ellipse(box_) => {
            validate_box(element.id, box_)?;
            render_box(svg, element, box_, "ellipse", &stroke, options);
        }
        ElementKind::Diamond(box_) => {
            validate_box(element.id, box_)?;
            render_box(svg, element, box_, "diamond", &stroke, options);
        }
        ElementKind::Triangle(box_) => {
            validate_box(element.id, box_)?;
            render_box(svg, element, box_, "triangle", &stroke, options);
        }
        ElementKind::Star(box_) => {
            validate_box(element.id, box_)?;
            render_box(svg, element, box_, "star", &stroke, options);
        }
        ElementKind::Hexagon(box_) => {
            validate_box(element.id, box_)?;
            render_box(svg, element, box_, "hexagon", &stroke, options);
        }
        ElementKind::Line(segment) | ElementKind::Arrow(segment) => {
            validate_numbers(
                element.id,
                [
                    segment.x1,
                    segment.y1,
                    segment.x2,
                    segment.y2,
                    segment.width,
                ],
            )?;
            let dash = if segment.style == SegmentStyle::Dashed {
                r#" stroke-dasharray="8 6""#
            } else {
                ""
            };
            writeln!(
                svg,
                r#"  <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-opacity="{}" stroke-width="{}" stroke-linecap="round"{dash}/>"#,
                number(segment.x1),
                number(segment.y1),
                number(segment.x2),
                number(segment.y2),
                stroke.hex,
                number(stroke.opacity),
                number(segment.width.max(0.1))
            )
            .expect("writing to String cannot fail");
            if matches!(element.kind, ElementKind::Arrow(_)) {
                render_arrow_head(svg, segment, &stroke);
            }
        }
        ElementKind::Text(text) => {
            validate_numbers(element.id, [text.x, text.y, text.size, text.rotation])?;
            let (width, height) = text_extent(text);
            render_text(
                svg,
                &text.content,
                text.x,
                text.y,
                text.size.max(1.0),
                text.rotation,
                [text.x + width / 2.0, text.y + height / 2.0],
                element.stroke.unwrap_or(options.default_text),
                "start",
            );
        }
        ElementKind::Embed(embed) => {
            validate_numbers(element.id, [embed.x, embed.y, embed.w, embed.h])?;
            writeln!(
                svg,
                r##"  <rect x="{}" y="{}" width="{}" height="{}" rx="8" fill="#f6f8fa" stroke="{}" stroke-opacity="{}"/>"##,
                number(embed.x),
                number(embed.y),
                number(embed.w.max(1.0)),
                number(embed.h.max(1.0)),
                stroke.hex,
                number(stroke.opacity)
            )
            .expect("writing to String cannot fail");
            render_text(
                svg,
                &embed.title,
                embed.x + 12.0,
                embed.y + 12.0,
                16.0,
                0.0,
                [embed.x + embed.w / 2.0, embed.y + embed.h / 2.0],
                options.default_text,
                "start",
            );
        }
        ElementKind::Image(image) => {
            validate_numbers(
                element.id,
                [image.x, image.y, image.w, image.h, image.rotation],
            )?;
            validate_image_source(&image.src).map_err(|message| SvgExportError {
                element_id: Some(element.id),
                message,
            })?;
            writeln!(
                svg,
                r#"  <image href="{}" x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="xMidYMid meet" transform="{}"/>"#,
                escape_xml_attribute(&image.src),
                number(image.x),
                number(image.y),
                number(image.w.max(1.0)),
                number(image.h.max(1.0)),
                rotation_transform(
                    image.rotation,
                    image.x + image.w / 2.0,
                    image.y + image.h / 2.0
                )
            )
            .expect("writing to String cannot fail");
        }
    }
    Ok(())
}

fn render_box(
    svg: &mut String,
    element: &Element,
    box_: &super::BoxGeom,
    shape: &str,
    stroke: &SvgColor,
    options: &SvgExportOptions,
) {
    let fill = element.fill.map(svg_color);
    let fill_hex = fill.as_ref().map_or("none", |color| color.hex.as_str());
    let fill_opacity = fill.as_ref().map_or(1.0, |color| color.opacity);
    let transform = rotation_transform(box_.rotation, box_.x + box_.w / 2.0, box_.y + box_.h / 2.0);
    match shape {
        "rect" | "round_rect" => {
            let radius = if shape == "round_rect" {
                box_.w.min(box_.h) * 0.16
            } else {
                0.0
            };
            writeln!(
                svg,
                r#"  <rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{}" fill-opacity="{}" stroke="{}" stroke-opacity="{}" stroke-width="{}" transform="{}"/>"#,
                number(box_.x),
                number(box_.y),
                number(box_.w.max(1.0)),
                number(box_.h.max(1.0)),
                number(radius),
                fill_hex,
                number(fill_opacity),
                stroke.hex,
                number(stroke.opacity),
                number(box_.width.max(0.1)),
                transform
            )
            .expect("writing to String cannot fail");
        }
        "ellipse" => {
            writeln!(
                svg,
                r#"  <ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="{}" fill-opacity="{}" stroke="{}" stroke-opacity="{}" stroke-width="{}" transform="{}"/>"#,
                number(box_.x + box_.w / 2.0),
                number(box_.y + box_.h / 2.0),
                number((box_.w / 2.0).max(0.5)),
                number((box_.h / 2.0).max(0.5)),
                fill_hex,
                number(fill_opacity),
                stroke.hex,
                number(stroke.opacity),
                number(box_.width.max(0.1)),
                transform
            )
            .expect("writing to String cannot fail");
        }
        _ => {
            let points = polygon_points(shape, box_)
                .into_iter()
                .map(|point| format!("{},{}", number(point[0]), number(point[1])))
                .collect::<Vec<_>>()
                .join(" ");
            writeln!(
                svg,
                r#"  <polygon points="{}" fill="{}" fill-opacity="{}" stroke="{}" stroke-opacity="{}" stroke-width="{}" stroke-linejoin="round" transform="{}"/>"#,
                points,
                fill_hex,
                number(fill_opacity),
                stroke.hex,
                number(stroke.opacity),
                number(box_.width.max(0.1)),
                transform
            )
            .expect("writing to String cannot fail");
        }
    }
    if let Some(label) = element.label.as_deref().filter(|label| !label.is_empty()) {
        render_text(
            svg,
            label,
            box_.x + box_.w / 2.0,
            box_.y + box_.h / 2.0 - 9.0,
            16.0,
            box_.rotation,
            [box_.x + box_.w / 2.0, box_.y + box_.h / 2.0],
            element.label_color.unwrap_or(options.default_text),
            "middle",
        );
    }
}

fn polygon_points(shape: &str, box_: &super::BoxGeom) -> Vec<[f32; 2]> {
    let x = box_.x;
    let y = box_.y;
    let w = box_.w;
    let h = box_.h;
    match shape {
        "diamond" => vec![
            [x + w / 2.0, y],
            [x + w, y + h / 2.0],
            [x + w / 2.0, y + h],
            [x, y + h / 2.0],
        ],
        "triangle" => vec![[x + w / 2.0, y], [x + w, y + h], [x, y + h]],
        "hexagon" => vec![
            [x + w * 0.25, y],
            [x + w * 0.75, y],
            [x + w, y + h / 2.0],
            [x + w * 0.75, y + h],
            [x + w * 0.25, y + h],
            [x, y + h / 2.0],
        ],
        "star" => {
            let mut points = Vec::with_capacity(10);
            let cx = x + w / 2.0;
            let cy = y + h / 2.0;
            for index in 0..10 {
                let radius_x = if index % 2 == 0 { w / 2.0 } else { w / 4.4 };
                let radius_y = if index % 2 == 0 { h / 2.0 } else { h / 4.4 };
                let angle =
                    -std::f32::consts::FRAC_PI_2 + index as f32 * std::f32::consts::PI / 5.0;
                points.push([cx + radius_x * angle.cos(), cy + radius_y * angle.sin()]);
            }
            points
        }
        _ => Vec::new(),
    }
}

fn render_arrow_head(svg: &mut String, segment: &super::SegGeom, color: &SvgColor) {
    let angle = (segment.y2 - segment.y1).atan2(segment.x2 - segment.x1);
    let length = (segment.width.max(1.0) * 4.0).max(10.0);
    let spread = 0.48;
    let p1 = [
        segment.x2 - length * (angle - spread).cos(),
        segment.y2 - length * (angle - spread).sin(),
    ];
    let p2 = [
        segment.x2 - length * (angle + spread).cos(),
        segment.y2 - length * (angle + spread).sin(),
    ];
    writeln!(
        svg,
        r#"  <polygon points="{},{} {},{} {},{}" fill="{}" fill-opacity="{}"/>"#,
        number(segment.x2),
        number(segment.y2),
        number(p1[0]),
        number(p1[1]),
        number(p2[0]),
        number(p2[1]),
        color.hex,
        number(color.opacity)
    )
    .expect("writing to String cannot fail");
}

#[allow(clippy::too_many_arguments)]
fn render_text(
    svg: &mut String,
    content: &str,
    x: f32,
    y: f32,
    size: f32,
    rotation: f32,
    pivot: [f32; 2],
    color: u32,
    anchor: &str,
) {
    let color = svg_color(color);
    writeln!(
        svg,
        r#"  <text x="{}" y="{}" font-family="sans-serif" font-size="{}" dominant-baseline="hanging" text-anchor="{}" fill="{}" fill-opacity="{}" transform="{}">"#,
        number(x),
        number(y),
        number(size),
        anchor,
        color.hex,
        number(color.opacity),
        rotation_transform(rotation, pivot[0], pivot[1])
    )
    .expect("writing to String cannot fail");
    for (index, line) in content.split('\n').enumerate() {
        writeln!(
            svg,
            r#"    <tspan x="{}" dy="{}">{}</tspan>"#,
            number(x),
            if index == 0 {
                "0".to_owned()
            } else {
                number(size * 1.3)
            },
            escape_xml_text(line)
        )
        .expect("writing to String cannot fail");
    }
    svg.push_str("  </text>\n");
}

fn validate_numbers(
    element_id: u64,
    values: impl IntoIterator<Item = f32>,
) -> Result<(), SvgExportError> {
    if values.into_iter().all(f32::is_finite) {
        Ok(())
    } else {
        Err(SvgExportError {
            element_id: Some(element_id),
            message: "geometry contains a non-finite number".to_owned(),
        })
    }
}

fn validate_box(element_id: u64, box_: &super::BoxGeom) -> Result<(), SvgExportError> {
    validate_numbers(
        element_id,
        [box_.x, box_.y, box_.w, box_.h, box_.width, box_.rotation],
    )
}

fn validate_image_source(source: &str) -> Result<(), String> {
    let source = source.trim();
    let lower = source.to_ascii_lowercase();
    if lower.is_empty() {
        return Err("image source is empty".to_owned());
    }
    if lower.starts_with("javascript:")
        || lower.starts_with("data:text/html")
        || lower.starts_with("file:")
    {
        return Err("image source uses an unsafe URL scheme".to_owned());
    }
    if lower.starts_with("asset:") {
        return Err("internal image source must be materialized before SVG export".to_owned());
    }
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return Ok(());
    }
    let path = Path::new(source);
    if source.contains('\\')
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("image source must be a normalized relative path or HTTP(S) URL".to_owned());
    }
    Ok(())
}

fn rotation_transform(rotation: f32, cx: f32, cy: f32) -> String {
    format!(
        "rotate({} {} {})",
        number(rotation.to_degrees()),
        number(cx),
        number(cy)
    )
}

struct SvgColor {
    hex: String,
    opacity: f32,
}

fn svg_color(color: u32) -> SvgColor {
    SvgColor {
        hex: format!("#{:06x}", color >> 8),
        opacity: (color & 0xff) as f32 / 255.0,
    }
}

fn finite_non_negative(value: f32) -> Option<f32> {
    (value.is_finite() && value >= 0.0).then_some(value)
}

fn number(value: f32) -> String {
    let mut result = format!("{value:.3}");
    while result.contains('.') && result.ends_with('0') {
        result.pop();
    }
    if result.ends_with('.') {
        result.pop();
    }
    if result == "-0" {
        "0".to_owned()
    } else {
        result
    }
}

fn escape_xml_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_xml_attribute(text: &str) -> String {
    escape_xml_text(text)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BoxGeom, Element, ElementKind, Scene};

    #[test]
    fn exports_shapes_and_escapes_labels() {
        let scene = Scene {
            elements: vec![Element {
                id: 1,
                kind: ElementKind::Rect(BoxGeom {
                    x: 10.0,
                    y: 20.0,
                    w: 120.0,
                    h: 60.0,
                    width: 2.0,
                    rotation: 0.0,
                }),
                stroke: Some(0x112233ff),
                fill: Some(0xaabbcc80),
                label: Some("A < B & C".to_owned()),
                label_color: None,
                styles: Vec::new(),
                mindmap: None,
            }],
            ..Scene::default()
        };

        let result = export_scene_svg(&scene, &SvgExportOptions::default()).unwrap();
        assert!(result.svg.contains("<rect"));
        assert!(result.svg.contains("#112233"));
        assert!(result.svg.contains("A &lt; B &amp; C"));
        assert!(!result.svg.contains("A < B"));
    }

    #[test]
    fn rejects_internal_image_sources() {
        let scene = Scene {
            elements: vec![Element {
                id: 9,
                kind: ElementKind::Image(crate::ImageGeom {
                    src: "asset:9".to_owned(),
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                    rotation: 0.0,
                }),
                stroke: None,
                fill: None,
                label: None,
                label_color: None,
                styles: Vec::new(),
                mindmap: None,
            }],
            ..Scene::default()
        };

        let error = export_scene_svg(&scene, &SvgExportOptions::default()).unwrap_err();
        assert_eq!(error.element_id, Some(9));
        assert!(error.message.contains("materialized"));
    }

    #[test]
    fn rejects_image_path_traversal() {
        let scene = Scene {
            elements: vec![Element {
                id: 10,
                kind: ElementKind::Image(crate::ImageGeom {
                    src: "../secret.png".to_owned(),
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                    rotation: 0.0,
                }),
                stroke: None,
                fill: None,
                label: None,
                label_color: None,
                styles: Vec::new(),
                mindmap: None,
            }],
            ..Scene::default()
        };

        let error = export_scene_svg(&scene, &SvgExportOptions::default()).unwrap_err();
        assert_eq!(error.element_id, Some(10));
        assert!(error.message.contains("normalized relative path"));
    }
}
