use std::{future::Future, pin::Pin};

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentRenderTheme {
    pub dark: bool,
    pub background: u32,
    pub foreground: u32,
    pub border: u32,
    pub muted: u32,
    pub accent: u32,
    pub danger: u32,
    pub font_family: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentRenderRequest {
    pub renderer: String,
    pub source: String,
    pub theme: DocumentRenderTheme,
    pub available_width: f32,
    pub scale_factor: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentRenderArtifact {
    pub media_type: String,
    pub bytes: Vec<u8>,
    pub intrinsic_width: Option<f32>,
    pub intrinsic_height: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentRenderError {
    message: String,
}

impl DocumentRenderError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DocumentRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DocumentRenderError {}

pub type DocumentRenderFuture = Pin<
    Box<dyn Future<Output = Result<DocumentRenderArtifact, DocumentRenderError>> + Send + 'static>,
>;

pub trait DocumentRendererProvider: Send + Sync {
    fn id(&self) -> &str;
    fn revision(&self) -> u64 {
        0
    }
    fn supports(&self, renderer: &str) -> bool;
    fn render(&self, request: DocumentRenderRequest) -> DocumentRenderFuture;
}
