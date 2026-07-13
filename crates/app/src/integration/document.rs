use cditor_core::document::BlockIndexRecord;
use cditor_core::layout::BlockLayoutMeta;
use cditor_core::rich_text::document::CURRENT_RICH_TEXT_FORMAT_VERSION;
use cditor_core::rich_text::{
    BlockPayloadRecord, DocumentMetadata, MarkdownImportOptions, RichBlockRecord, RichTextDocument,
    export_plain_markdown, parse_markdown_document,
};
use cditor_runtime::DocumentRuntime;
use serde::{Deserialize, Serialize};

use super::EditorError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorBlock {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub depth: u16,
    pub kind_tag: u16,
    pub flags: u32,
    pub estimated_height: f64,
    pub payload: BlockPayloadRecord,
}

impl EditorBlock {
    fn from_records(index: BlockIndexRecord, payload: BlockPayloadRecord) -> Self {
        Self {
            id: index.id,
            parent_id: index.parent_id,
            depth: index.depth,
            kind_tag: index.kind_tag,
            flags: index.flags,
            estimated_height: index.layout_meta.estimated_height,
            payload,
        }
    }

    fn index_record(&self) -> BlockIndexRecord {
        BlockIndexRecord::new(
            self.id,
            self.parent_id,
            self.depth,
            self.kind_tag,
            self.flags,
        )
        .with_layout_meta(BlockLayoutMeta::new(self.id, self.estimated_height))
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
        let document_id = document_id.into();
        let runtime_document_id = runtime_document_id(&document_id);
        let parsed = parse_markdown_document(
            markdown,
            MarkdownImportOptions {
                document_id: runtime_document_id,
                first_block_id: 1,
            },
        );
        let mut document = RichTextDocument::empty(runtime_document_id);
        document.root_blocks = parsed.root_blocks;
        document.blocks = parsed.blocks;
        if document.blocks.is_empty() {
            document.push_root_block(RichBlockRecord::paragraph(1, ""));
        }
        Self::from_rich_text_document(document_id, document)
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

    pub fn to_markdown(&self) -> Result<String, EditorError> {
        self.validate()?;
        Ok(export_plain_markdown(&self.rich_text_document()))
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
            .map(|(index, payload)| EditorBlock::from_records(index, payload))
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
        let runtime_id = runtime_document_id(&self.document_id);
        let records = self.blocks.iter().map(EditorBlock::index_record).collect();
        let payloads = self
            .blocks
            .iter()
            .map(|block| block.payload.clone())
            .collect();
        DocumentRuntime::from_index_payload_snapshot(
            runtime_id,
            records,
            payloads,
            self.structure_version,
            viewport_height,
        )
        .map_err(EditorError::InvalidDocument)
    }

    fn from_rich_text_document(
        document_id: String,
        document: RichTextDocument,
    ) -> Result<Self, EditorError> {
        let runtime = DocumentRuntime::from_rich_text_document(document, 720.0);
        Self::from_runtime(document_id, &runtime)
    }

    fn rich_text_document(&self) -> RichTextDocument {
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

    fn validate(&self) -> Result<(), EditorError> {
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
