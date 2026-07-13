use std::fmt::{Display, Formatter};

use super::EditorPersistenceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorError {
    NotReady,
    PersistenceNotConfigured,
    InvalidMarkdown(String),
    InvalidDocument(String),
    InvalidJson(String),
    UnsupportedSchemaVersion { version: u32 },
    IncompleteDocument,
    DocumentIdMismatch { expected: String, actual: String },
    EntityUpdate(String),
    Persistence(EditorPersistenceError),
}

impl Display for EditorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotReady => formatter.write_str("editor is not ready"),
            Self::PersistenceNotConfigured => {
                formatter.write_str("editor persistence is not configured")
            }
            Self::InvalidMarkdown(message) => write!(formatter, "invalid Markdown: {message}"),
            Self::InvalidDocument(message) => write!(formatter, "invalid document: {message}"),
            Self::InvalidJson(message) => write!(formatter, "invalid document JSON: {message}"),
            Self::UnsupportedSchemaVersion { version } => {
                write!(formatter, "unsupported editor document schema version {version}")
            }
            Self::IncompleteDocument => {
                formatter.write_str("runtime does not contain every document payload")
            }
            Self::DocumentIdMismatch { expected, actual } => write!(
                formatter,
                "document id mismatch: expected {expected}, received {actual}"
            ),
            Self::EntityUpdate(message) => write!(formatter, "editor entity update failed: {message}"),
            Self::Persistence(error) => write!(formatter, "editor persistence failed: {error}"),
        }
    }
}

impl std::error::Error for EditorError {}

impl From<EditorPersistenceError> for EditorError {
    fn from(error: EditorPersistenceError) -> Self {
        Self::Persistence(error)
    }
}
