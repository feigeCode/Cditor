use cditor_core::document::BlockIndexRecord;
use cditor_core::rich_text::document::CURRENT_RICH_TEXT_FORMAT_VERSION;
use cditor_core::rich_text::{
    BlockAttrs, BlockPayloadRecord, DocumentMetadata, MarkdownExportMode, MarkdownExportResult,
    MarkdownImportOptions, RichBlockRecord, RichTextDocument, export_document_blocks,
    parse_markdown_document_with_report,
};
use cditor_runtime::DocumentRuntime;
use serde::{Deserialize, Serialize};

use super::{EditorError, MarkdownImportResult};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorBlock {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub depth: u16,
    pub kind_tag: u16,
    pub flags: u32,
    pub estimated_height: f64,
    pub payload: BlockPayloadRecord,
    #[serde(default)]
    pub attrs: BlockAttrs,
    #[serde(default)]
    pub raw_fallback: Option<String>,
}

impl EditorBlock {
    fn from_records(
        index: BlockIndexRecord,
        payload: BlockPayloadRecord,
        attrs: BlockAttrs,
    ) -> Self {
        let raw_fallback = (payload.kind == cditor_core::rich_text::RichBlockKind::RawMarkdown)
            .then(|| payload.plain_text());
        Self {
            id: index.id,
            parent_id: index.parent_id,
            depth: index.depth,
            kind_tag: index.kind_tag,
            flags: index.flags,
            estimated_height: index.layout_meta.estimated_height,
            payload,
            attrs,
            raw_fallback,
        }
    }

    fn rich_block_record(&self, document_id: u64, structure_version: u64) -> RichBlockRecord {
        let mut block = RichBlockRecord::new(
            self.id,
            self.payload.kind.clone(),
            self.payload.payload.clone(),
        );
        block.document_id = document_id;
        block.parent_id = self.parent_id;
        block.depth = self.depth;
        block.content_version = self.payload.content_version;
        block.structure_version = structure_version;
        block.estimated_height = self.estimated_height;
        block.attrs = self.attrs.clone();
        block.raw_fallback = self.raw_fallback.clone().or_else(|| {
            (block.kind == cditor_core::rich_text::RichBlockKind::RawMarkdown)
                .then(|| block.payload.plain_text())
        });
        block
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorDocument {
    pub schema_version: u32,
    pub document_id: String,
    pub structure_version: u64,
    pub blocks: Vec<EditorBlock>,
}

impl EditorDocument {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    pub fn from_markdown(
        document_id: impl Into<String>,
        markdown: &str,
    ) -> Result<Self, EditorError> {
        Ok(Self::from_markdown_with_report(document_id, markdown)?.document)
    }

    pub fn from_markdown_with_report(
        document_id: impl Into<String>,
        markdown: &str,
    ) -> Result<MarkdownImportResult, EditorError> {
        let document_id = document_id.into();
        let runtime_document_id = runtime_document_id(&document_id);
        let parsed = parse_markdown_document_with_report(
            markdown,
            MarkdownImportOptions {
                document_id: runtime_document_id,
                first_block_id: 1,
            },
        );
        let mut rich_document = RichTextDocument::empty(runtime_document_id);
        rich_document.root_blocks = parsed.document.root_blocks;
        rich_document.blocks = parsed.document.blocks;
        if rich_document.blocks.is_empty() {
            rich_document.push_root_block(RichBlockRecord::paragraph(1, ""));
        }
        Ok(MarkdownImportResult {
            document: Self::from_rich_text_document(document_id, rich_document)?,
            compatibility: parsed.compatibility,
            diagnostics: parsed.diagnostics,
        })
    }

    pub fn from_json(json: &str) -> Result<Self, EditorError> {
        let document: Self = serde_json::from_str(json)
            .map_err(|error| EditorError::InvalidJson(error.to_string()))?;
        document.validate()?;
        Ok(document)
    }

    pub fn to_json(&self) -> Result<String, EditorError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|error| EditorError::InvalidJson(error.to_string()))
    }

    /// Compatibility export for previews and legacy callers.
    ///
    /// This maps to [`MarkdownExportMode::BestEffort`] and may normalize or
    /// degrade unsupported rich-text content. Persistent `.md` sources must
    /// use [`Self::export_markdown`] with [`MarkdownExportMode::Strict`].
    pub fn to_markdown(&self) -> Result<String, EditorError> {
        Ok(self
            .export_markdown(MarkdownExportMode::BestEffort)?
            .markdown)
    }

    pub fn export_markdown(
        &self,
        mode: MarkdownExportMode,
    ) -> Result<MarkdownExportResult, EditorError> {
        self.validate()?;
        let result = export_document_blocks(&self.rich_text_document(), mode);
        if mode == MarkdownExportMode::Strict
            && result.fidelity == cditor_core::rich_text::MarkdownFidelity::Unsupported
        {
            return Err(EditorError::MarkdownUnsupported {
                diagnostics: result.diagnostics,
            });
        }
        Ok(result)
    }

    pub fn from_runtime(
        document_id: impl Into<String>,
        runtime: &DocumentRuntime,
    ) -> Result<Self, EditorError> {
        let document_id = document_id.into();
        let (indexes, payloads) = runtime
            .complete_document_snapshot()
            .ok_or(EditorError::IncompleteDocument)?;
        if indexes.len() != payloads.len() {
            return Err(EditorError::IncompleteDocument);
        }
        let blocks = indexes
            .into_iter()
            .zip(payloads)
            .map(|(index, payload)| {
                let attrs = runtime.block_attrs(index.id);
                EditorBlock::from_records(index, payload, attrs)
            })
            .collect();
        Ok(Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            document_id,
            structure_version: runtime.structure_version(),
            blocks,
        })
    }

    pub fn into_runtime(self, viewport_height: f64) -> Result<DocumentRuntime, EditorError> {
        self.validate()?;
        let mut document = self.rich_text_document();
        document.structure_version = self.structure_version;
        Ok(DocumentRuntime::from_rich_text_document(
            document,
            viewport_height,
        ))
    }

    pub(crate) fn from_rich_text_document(
        document_id: String,
        document: RichTextDocument,
    ) -> Result<Self, EditorError> {
        let runtime = DocumentRuntime::from_rich_text_document(document, 720.0);
        Self::from_runtime(document_id, &runtime)
    }

    pub(crate) fn rich_text_document(&self) -> RichTextDocument {
        let runtime_id = runtime_document_id(&self.document_id);
        RichTextDocument {
            id: runtime_id,
            version: CURRENT_RICH_TEXT_FORMAT_VERSION,
            metadata: DocumentMetadata::default(),
            root_blocks: self
                .blocks
                .iter()
                .filter(|block| block.parent_id.is_none())
                .map(|block| block.id)
                .collect(),
            blocks: self
                .blocks
                .iter()
                .map(|block| block.rich_block_record(runtime_id, self.structure_version))
                .collect(),
            structure_version: self.structure_version,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), EditorError> {
        if self.schema_version != Self::CURRENT_SCHEMA_VERSION {
            return Err(EditorError::UnsupportedSchemaVersion {
                version: self.schema_version,
            });
        }
        if self.document_id.trim().is_empty() {
            return Err(EditorError::InvalidDocument(
                "document_id must not be empty".to_owned(),
            ));
        }
        for block in &self.blocks {
            if block.payload.block_id != block.id {
                return Err(EditorError::InvalidDocument(format!(
                    "payload block id {} does not match block id {}",
                    block.payload.block_id, block.id
                )));
            }
        }
        Ok(())
    }
}

pub(crate) fn runtime_document_id(document_id: &str) -> u64 {
    if let Ok(id) = document_id.parse::<u64>() {
        return id.max(1);
    }
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in document_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash.max(1)
}
