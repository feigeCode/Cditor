use std::path::{Component, Path};

use cditor_core::rich_text::{
    BlockPayload, InlineSpan, MarkdownCompatibility, MarkdownDiagnostic,
    MarkdownDiagnosticSeverity, MarkdownExportMode, MarkdownFidelity, MarkdownImportOptions,
    RichBlockKind, RichTextDocument, WhiteboardPayload, export_document_blocks,
    parse_markdown_document_with_report,
};
use ding_board::{Scene, SvgExportOptions, export_scene_svg};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::document::runtime_document_id;
use super::{
    EditorDocument, EditorError, MarkdownAsset, MarkdownAssetResolver, MarkdownAssetRole,
    MarkdownBundleExportResult, MarkdownBundleOptions, MarkdownImportResult,
};

const WHITEBOARD_COMMENT_PREFIX: &str = "<!-- cditor:whiteboard ";
const WHITEBOARD_COMMENT_SUFFIX: &str = " -->";
const WHITEBOARD_FORMAT_VERSION: u32 = 1;
const MAX_WHITEBOARD_SOURCE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct WhiteboardManifest {
    version: u32,
    block_id: u64,
    source: String,
    preview: String,
    sha256: String,
}

impl EditorDocument {
    pub fn export_markdown_bundle(
        &self,
        mode: MarkdownExportMode,
        options: &MarkdownBundleOptions,
    ) -> Result<MarkdownBundleExportResult, EditorError> {
        self.validate()?;
        let asset_directory = normalize_asset_directory(&options.asset_directory)?;
        let mut document = self.rich_text_document();
        let mut assets = Vec::new();
        let mut bundle_diagnostics = Vec::new();

        for block in &mut document.blocks {
            if block.kind != RichBlockKind::Whiteboard {
                continue;
            }
            let BlockPayload::Whiteboard(whiteboard) = &block.payload else {
                bundle_diagnostics.push(MarkdownDiagnostic::block(
                    severity_for(mode),
                    "markdown.whiteboard.payload_missing",
                    "Whiteboard block does not contain scene data",
                    block.id,
                ));
                continue;
            };
            if whiteboard.scene_json.len() > MAX_WHITEBOARD_SOURCE_BYTES {
                bundle_diagnostics.push(MarkdownDiagnostic::block(
                    severity_for(mode),
                    "markdown.whiteboard.source_too_large",
                    "Whiteboard scene exceeds the 16 MiB Markdown bundle limit",
                    block.id,
                ));
                continue;
            }
            let scene: Scene = match serde_json::from_str(&whiteboard.scene_json) {
                Ok(scene) => scene,
                Err(error) => {
                    bundle_diagnostics.push(MarkdownDiagnostic::block(
                        severity_for(mode),
                        "markdown.whiteboard.source_invalid",
                        format!("Whiteboard scene JSON is invalid: {error}"),
                        block.id,
                    ));
                    continue;
                }
            };
            let preview = match export_scene_svg(
                &scene,
                &SvgExportOptions {
                    padding: options.preview_padding as f32,
                    ..SvgExportOptions::default()
                },
            ) {
                Ok(preview) => preview,
                Err(error) => {
                    bundle_diagnostics.push(MarkdownDiagnostic::block(
                        severity_for(mode),
                        "markdown.whiteboard.preview_failed",
                        format!("Whiteboard preview could not be generated: {error}"),
                        block.id,
                    ));
                    continue;
                }
            };

            let source_path = format!(
                "{asset_directory}/whiteboard-{}.cditor-board.json",
                block.id
            );
            let preview_path = format!("{asset_directory}/whiteboard-{}.svg", block.id);
            let source_bytes = whiteboard.scene_json.as_bytes().to_vec();
            let manifest = WhiteboardManifest {
                version: WHITEBOARD_FORMAT_VERSION,
                block_id: block.id,
                source: source_path.clone(),
                preview: preview_path.clone(),
                sha256: sha256_hex(&source_bytes),
            };
            let manifest_json = serde_json::to_string(&manifest)
                .map_err(|error| EditorError::InvalidMarkdown(error.to_string()))?;
            if manifest_json.contains("--") {
                return Err(EditorError::InvalidMarkdown(
                    "whiteboard manifest cannot be represented safely in an HTML comment"
                        .to_owned(),
                ));
            }
            let markdown = format!(
                "{WHITEBOARD_COMMENT_PREFIX}{manifest_json}{WHITEBOARD_COMMENT_SUFFIX}\n![Whiteboard](<{preview_path}>)"
            );

            assets.push(MarkdownAsset {
                relative_path: source_path,
                media_type: "application/vnd.cditor.whiteboard+json".to_owned(),
                bytes: source_bytes,
                block_id: block.id,
                role: MarkdownAssetRole::WhiteboardSource,
            });
            assets.push(MarkdownAsset {
                relative_path: preview_path,
                media_type: "image/svg+xml".to_owned(),
                bytes: preview.svg.into_bytes(),
                block_id: block.id,
                role: MarkdownAssetRole::WhiteboardPreview,
            });
            block.kind = RichBlockKind::RawMarkdown;
            block.payload = BlockPayload::RichText {
                spans: vec![InlineSpan::plain(markdown.clone())],
            };
            block.raw_fallback = Some(markdown);
        }

        let mut result = export_document_blocks(&document, mode);
        result.diagnostics.extend(bundle_diagnostics);
        if result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == MarkdownDiagnosticSeverity::Error)
        {
            result.fidelity = MarkdownFidelity::Unsupported;
            if mode == MarkdownExportMode::Strict {
                return Err(EditorError::MarkdownUnsupported {
                    diagnostics: result.diagnostics,
                });
            }
        }
        if result.fidelity == MarkdownFidelity::Unsupported {
            assets.retain(|asset| {
                !result.diagnostics.iter().any(|diagnostic| {
                    diagnostic.block_id == Some(asset.block_id)
                        && diagnostic.severity == MarkdownDiagnosticSeverity::Warning
                })
            });
        }
        Ok(MarkdownBundleExportResult {
            markdown: result.markdown,
            assets,
            fidelity: result.fidelity,
            diagnostics: result.diagnostics,
        })
    }

    pub fn from_markdown_bundle_with_report(
        document_id: impl Into<String>,
        markdown: &str,
        resolver: &dyn MarkdownAssetResolver,
    ) -> Result<MarkdownImportResult, EditorError> {
        let document_id = document_id.into();
        let preprocessed = preprocess_whiteboards(markdown, resolver);
        let runtime_id = runtime_document_id(&document_id);
        let mut parsed = parse_markdown_document_with_report(
            &preprocessed.markdown,
            MarkdownImportOptions {
                document_id: runtime_id,
                first_block_id: 1,
            },
        );
        for block in &mut parsed.document.blocks {
            let token = block.payload.plain_text();
            let Some(index) = parse_whiteboard_token(&token) else {
                continue;
            };
            let Some(scene_json) = preprocessed.scenes.get(index) else {
                continue;
            };
            block.kind = RichBlockKind::Whiteboard;
            block.payload = BlockPayload::Whiteboard(WhiteboardPayload {
                scene_json: scene_json.clone(),
            });
            block.raw_fallback = None;
        }
        parsed.diagnostics.extend(preprocessed.diagnostics);
        parsed.compatibility = MarkdownCompatibility::from_diagnostics(&parsed.diagnostics);

        let rich_document = RichTextDocument {
            id: runtime_id,
            version: cditor_core::rich_text::document::CURRENT_RICH_TEXT_FORMAT_VERSION,
            metadata: Default::default(),
            root_blocks: parsed.document.root_blocks,
            blocks: parsed.document.blocks,
            structure_version: 1,
        };
        Ok(MarkdownImportResult {
            document: Self::from_rich_text_document(document_id, rich_document)?,
            compatibility: parsed.compatibility,
            diagnostics: parsed.diagnostics,
        })
    }
}

struct PreprocessedMarkdown {
    markdown: String,
    scenes: Vec<String>,
    diagnostics: Vec<MarkdownDiagnostic>,
}

fn preprocess_whiteboards(
    markdown: &str,
    resolver: &dyn MarkdownAssetResolver,
) -> PreprocessedMarkdown {
    let lines = markdown.lines().collect::<Vec<_>>();
    let mut output = Vec::with_capacity(lines.len());
    let mut scenes = Vec::new();
    let mut diagnostics = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        let Some(manifest) = parse_manifest(line) else {
            output.push(lines[index].to_owned());
            index += 1;
            continue;
        };
        let preview_line = lines.get(index + 1).copied();
        match load_whiteboard(&manifest, resolver) {
            Ok(scene_json) => {
                if let Err(error) = resolver.read_asset(&manifest.preview) {
                    diagnostics.push(MarkdownDiagnostic {
                        severity: MarkdownDiagnosticSeverity::Warning,
                        code: "markdown.whiteboard.preview_asset_missing",
                        message: format!(
                            "Whiteboard source was restored, but its preview asset could not be read: {error}"
                        ),
                        source_range: None,
                        block_id: Some(manifest.block_id),
                    });
                }
                if preview_line != Some(preview_markdown(&manifest.preview).as_str()) {
                    diagnostics.push(MarkdownDiagnostic {
                        severity: MarkdownDiagnosticSeverity::Warning,
                        code: "markdown.whiteboard.preview_missing",
                        message: "Whiteboard source was restored, but its preview image link is missing or changed"
                            .to_owned(),
                        source_range: None,
                        block_id: Some(manifest.block_id),
                    });
                } else {
                    index += 1;
                }
                let token_index = scenes.len();
                scenes.push(scene_json);
                if output.last().is_some_and(|line| !line.trim().is_empty()) {
                    output.push(String::new());
                }
                output.push(whiteboard_token(token_index));
                output.push(String::new());
            }
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                output.push(lines[index].to_owned());
                if let Some(preview) = preview_line {
                    output.push(preview.to_owned());
                    index += 1;
                }
            }
        }
        index += 1;
    }
    PreprocessedMarkdown {
        markdown: output.join("\n"),
        scenes,
        diagnostics,
    }
}

fn parse_manifest(line: &str) -> Option<WhiteboardManifest> {
    let json = line
        .strip_prefix(WHITEBOARD_COMMENT_PREFIX)?
        .strip_suffix(WHITEBOARD_COMMENT_SUFFIX)?;
    serde_json::from_str(json).ok()
}

fn load_whiteboard(
    manifest: &WhiteboardManifest,
    resolver: &dyn MarkdownAssetResolver,
) -> Result<String, MarkdownDiagnostic> {
    if manifest.version != WHITEBOARD_FORMAT_VERSION {
        return Err(import_error(
            manifest.block_id,
            "markdown.whiteboard.version_unsupported",
            format!(
                "Whiteboard bundle version {} is not supported",
                manifest.version
            ),
        ));
    }
    if let Err(message) = validate_relative_asset_path(&manifest.source) {
        return Err(import_error(
            manifest.block_id,
            "markdown.whiteboard.source_path_invalid",
            message,
        ));
    }
    if let Err(message) = validate_relative_asset_path(&manifest.preview) {
        return Err(import_error(
            manifest.block_id,
            "markdown.whiteboard.preview_path_invalid",
            message,
        ));
    }
    let bytes = resolver.read_asset(&manifest.source).map_err(|error| {
        import_error(
            manifest.block_id,
            "markdown.whiteboard.source_missing",
            format!("Whiteboard source could not be read: {error}"),
        )
    })?;
    if bytes.len() > MAX_WHITEBOARD_SOURCE_BYTES {
        return Err(import_error(
            manifest.block_id,
            "markdown.whiteboard.source_too_large",
            "Whiteboard source exceeds the 16 MiB import limit",
        ));
    }
    if sha256_hex(&bytes) != manifest.sha256 {
        return Err(import_error(
            manifest.block_id,
            "markdown.whiteboard.source_hash_mismatch",
            "Whiteboard source hash does not match the Markdown manifest",
        ));
    }
    let _: Scene = serde_json::from_slice(&bytes).map_err(|error| {
        import_error(
            manifest.block_id,
            "markdown.whiteboard.source_invalid",
            format!("Whiteboard source JSON is invalid: {error}"),
        )
    })?;
    String::from_utf8(bytes).map_err(|error| {
        import_error(
            manifest.block_id,
            "markdown.whiteboard.source_invalid",
            format!("Whiteboard source is not valid UTF-8: {error}"),
        )
    })
}

fn import_error(
    block_id: u64,
    code: &'static str,
    message: impl Into<String>,
) -> MarkdownDiagnostic {
    MarkdownDiagnostic {
        severity: MarkdownDiagnosticSeverity::Error,
        code,
        message: message.into(),
        source_range: None,
        block_id: Some(block_id),
    }
}

fn normalize_asset_directory(directory: &str) -> Result<String, EditorError> {
    validate_relative_asset_path(directory)
        .map_err(|message| EditorError::InvalidDocument(message))?;
    Ok(Path::new(directory)
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/"))
}

fn validate_relative_asset_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("Markdown asset path must not be empty".to_owned());
    }
    if path.contains('\\') || path.contains('\0') {
        return Err("Markdown asset path contains an unsupported character".to_owned());
    }
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(
            "Markdown asset path must be a normalized relative path without '..'".to_owned(),
        );
    }
    Ok(())
}

fn preview_markdown(path: &str) -> String {
    format!("![Whiteboard](<{path}>)")
}

fn whiteboard_token(index: usize) -> String {
    format!("CDITORWHITEBOARD{index}X8F5C1A7E")
}

fn parse_whiteboard_token(token: &str) -> Option<usize> {
    token
        .strip_prefix("CDITORWHITEBOARD")?
        .strip_suffix("X8F5C1A7E")?
        .parse()
        .ok()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn severity_for(mode: MarkdownExportMode) -> MarkdownDiagnosticSeverity {
    if mode == MarkdownExportMode::Strict {
        MarkdownDiagnosticSeverity::Error
    } else {
        MarkdownDiagnosticSeverity::Warning
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockRecord};

    use super::*;
    use crate::integration::MarkdownAssetError;

    #[derive(Default)]
    struct MapResolver(HashMap<String, Vec<u8>>);

    impl MarkdownAssetResolver for MapResolver {
        fn read_asset(&self, relative_path: &str) -> Result<Vec<u8>, MarkdownAssetError> {
            self.0
                .get(relative_path)
                .cloned()
                .ok_or_else(|| MarkdownAssetError::new("missing asset"))
        }
    }

    fn document_with_whiteboard(scene_json: &str) -> EditorDocument {
        let rich = RichTextDocument {
            id: 1,
            version: cditor_core::rich_text::document::CURRENT_RICH_TEXT_FORMAT_VERSION,
            metadata: Default::default(),
            root_blocks: vec![1],
            blocks: vec![RichBlockRecord::whiteboard(1, scene_json)],
            structure_version: 1,
        };
        EditorDocument::from_rich_text_document("doc-1".to_owned(), rich).unwrap()
    }

    #[test]
    fn whiteboard_bundle_exports_preview_and_editable_source() {
        let document =
            document_with_whiteboard(r#"{"camera":{"x":0.0,"y":0.0,"zoom":1.0},"elements":[]}"#);
        let bundle = document
            .export_markdown_bundle(
                MarkdownExportMode::Strict,
                &MarkdownBundleOptions {
                    asset_directory: "note.assets".to_owned(),
                    preview_padding: 24,
                },
            )
            .unwrap();

        assert_eq!(bundle.assets.len(), 2);
        assert!(bundle.markdown.contains("cditor:whiteboard"));
        assert!(
            bundle
                .markdown
                .contains("![Whiteboard](<note.assets/whiteboard-1.svg>)")
        );
        assert!(bundle.assets.iter().any(|asset| {
            asset.role == MarkdownAssetRole::WhiteboardPreview
                && asset.relative_path == "note.assets/whiteboard-1.svg"
                && asset.bytes.starts_with(b"<svg")
        }));
    }

    #[test]
    fn whiteboard_bundle_round_trips_through_asset_resolver() {
        let source =
            r#"{"camera":{"x":1.0,"y":2.0,"zoom":1.0},"elements":[],"future_field":{"kept":true}}"#;
        let document = document_with_whiteboard(source);
        let bundle = document
            .export_markdown_bundle(
                MarkdownExportMode::Strict,
                &MarkdownBundleOptions {
                    asset_directory: "note.assets".to_owned(),
                    ..Default::default()
                },
            )
            .unwrap();
        let resolver = MapResolver(
            bundle
                .assets
                .iter()
                .map(|asset| (asset.relative_path.clone(), asset.bytes.clone()))
                .collect(),
        );

        let imported =
            EditorDocument::from_markdown_bundle_with_report("doc-2", &bundle.markdown, &resolver)
                .unwrap();
        assert_eq!(imported.compatibility, MarkdownCompatibility::Editable);
        assert_eq!(imported.document.blocks.len(), 1);
        let BlockPayloadRecord {
            payload: BlockPayload::Whiteboard(whiteboard),
            ..
        } = &imported.document.blocks[0].payload
        else {
            panic!("expected whiteboard payload");
        };
        let scene: Scene = serde_json::from_str(&whiteboard.scene_json).unwrap();
        assert_eq!(scene.camera.x, 1.0);
        assert_eq!(scene.camera.y, 2.0);
        assert!(whiteboard.scene_json.contains("future_field"));
    }

    #[test]
    fn whiteboard_bundle_stays_a_separate_block_between_paragraphs() {
        let document = document_with_whiteboard("{}");
        let bundle = document
            .export_markdown_bundle(
                MarkdownExportMode::Strict,
                &MarkdownBundleOptions {
                    asset_directory: "note.assets".to_owned(),
                    ..Default::default()
                },
            )
            .unwrap();
        let resolver = MapResolver(
            bundle
                .assets
                .iter()
                .map(|asset| (asset.relative_path.clone(), asset.bytes.clone()))
                .collect(),
        );
        let markdown = format!("before\n{}\nafter", bundle.markdown);

        let imported =
            EditorDocument::from_markdown_bundle_with_report("doc-1", &markdown, &resolver)
                .unwrap();
        assert_eq!(imported.document.blocks.len(), 3);
        assert_eq!(imported.document.blocks[0].payload.plain_text(), "before");
        assert!(matches!(
            imported.document.blocks[1].payload.payload,
            BlockPayload::Whiteboard(_)
        ));
        assert_eq!(imported.document.blocks[2].payload.plain_text(), "after");
    }

    #[test]
    fn missing_whiteboard_source_is_source_only() {
        let source = br#"{"camera":{"x":0.0,"y":0.0,"zoom":1.0},"elements":[]}"#;
        let manifest = WhiteboardManifest {
            version: 1,
            block_id: 7,
            source: "note.assets/whiteboard-7.cditor-board.json".to_owned(),
            preview: "note.assets/whiteboard-7.svg".to_owned(),
            sha256: sha256_hex(source),
        };
        let markdown = format!(
            "{WHITEBOARD_COMMENT_PREFIX}{}{WHITEBOARD_COMMENT_SUFFIX}\n{}",
            serde_json::to_string(&manifest).unwrap(),
            preview_markdown(&manifest.preview)
        );

        let imported = EditorDocument::from_markdown_bundle_with_report(
            "doc-1",
            &markdown,
            &MapResolver::default(),
        )
        .unwrap();
        assert!(imported.compatibility.is_source_only());
        assert!(
            imported
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "markdown.whiteboard.source_missing" })
        );
    }

    #[test]
    fn tampered_whiteboard_source_is_source_only() {
        let source = br#"{"camera":{"x":0.0,"y":0.0,"zoom":1.0},"elements":[]}"#;
        let manifest = WhiteboardManifest {
            version: 1,
            block_id: 7,
            source: "note.assets/whiteboard-7.cditor-board.json".to_owned(),
            preview: "note.assets/whiteboard-7.svg".to_owned(),
            sha256: sha256_hex(source),
        };
        let markdown = format!(
            "{WHITEBOARD_COMMENT_PREFIX}{}{WHITEBOARD_COMMENT_SUFFIX}\n{}",
            serde_json::to_string(&manifest).unwrap(),
            preview_markdown(&manifest.preview)
        );
        let resolver = MapResolver(HashMap::from([
            (manifest.source.clone(), b"{}".to_vec()),
            (manifest.preview.clone(), b"<svg/>".to_vec()),
        ]));

        let imported =
            EditorDocument::from_markdown_bundle_with_report("doc-1", &markdown, &resolver)
                .unwrap();
        assert!(imported.compatibility.is_source_only());
        assert!(
            imported.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "markdown.whiteboard.source_hash_mismatch"
            })
        );
    }

    #[test]
    fn missing_preview_keeps_whiteboard_editable_with_warning() {
        let document = document_with_whiteboard("{}");
        let bundle = document
            .export_markdown_bundle(
                MarkdownExportMode::Strict,
                &MarkdownBundleOptions {
                    asset_directory: "note.assets".to_owned(),
                    ..Default::default()
                },
            )
            .unwrap();
        let resolver = MapResolver(
            bundle
                .assets
                .iter()
                .filter(|asset| asset.role == MarkdownAssetRole::WhiteboardSource)
                .map(|asset| (asset.relative_path.clone(), asset.bytes.clone()))
                .collect(),
        );

        let imported =
            EditorDocument::from_markdown_bundle_with_report("doc-1", &bundle.markdown, &resolver)
                .unwrap();
        assert!(matches!(
            imported.compatibility,
            MarkdownCompatibility::EditableWithNormalization(_)
        ));
        assert!(
            imported.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "markdown.whiteboard.preview_asset_missing"
            })
        );
        assert!(matches!(
            imported.document.blocks[0].payload.payload,
            BlockPayload::Whiteboard(_)
        ));
    }

    #[test]
    fn rejects_asset_path_traversal() {
        let document = document_with_whiteboard("{}");
        let error = document
            .export_markdown_bundle(
                MarkdownExportMode::Strict,
                &MarkdownBundleOptions {
                    asset_directory: "../outside".to_owned(),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(matches!(error, EditorError::InvalidDocument(_)));
    }
}
