use std::fmt::{Display, Formatter};

use super::{EditorPersistenceError, MarkdownDiagnostic};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorError {
    NotReady,
    Readonly,
    PersistenceNotConfigured,
    InvalidMarkdown(String),
    MarkdownUnsupported {
        diagnostics: Vec<MarkdownDiagnostic>,
    },
    MarkdownSourceOnly {
        diagnostics: Vec<MarkdownDiagnostic>,
    },
    InvalidDocument(String),
    InvalidJson(String),
    UnsupportedSchemaVersion {
        version: u32,
    },
    IncompleteDocument,
    DocumentIdMismatch {
        expected: String,
        actual: String,
    },
    EntityUpdate(String),
    InvalidCommand(String),
    Command(String),
    Persistence(EditorPersistenceError),
}

impl Display for EditorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotReady => formatter.write_str("editor is not ready"),
            Self::Readonly => formatter.write_str("editor is readonly"),
            Self::PersistenceNotConfigured => {
                formatter.write_str("editor persistence is not configured")
            }
            Self::InvalidMarkdown(message) => write!(formatter, "invalid Markdown: {message}"),
            Self::MarkdownUnsupported { diagnostics } => write!(
                formatter,
                "Markdown export would lose unsupported rich-text content ({} diagnostics)",
                diagnostics.len()
            ),
            Self::MarkdownSourceOnly { diagnostics } => write!(
                formatter,
                "Markdown source is only safe in source or read-only preview mode ({} diagnostics)",
                diagnostics.len()
            ),
            Self::InvalidDocument(message) => write!(formatter, "invalid document: {message}"),
            Self::InvalidJson(message) => write!(formatter, "invalid document JSON: {message}"),
            Self::UnsupportedSchemaVersion { version } => {
                write!(
                    formatter,
                    "unsupported editor document schema version {version}"
                )
            }
            Self::IncompleteDocument => {
                formatter.write_str("runtime does not contain every document payload")
            }
            Self::DocumentIdMismatch { expected, actual } => write!(
                formatter,
                "document id mismatch: expected {expected}, received {actual}"
            ),
            Self::EntityUpdate(message) => {
                write!(formatter, "editor entity update failed: {message}")
            }
            Self::InvalidCommand(command_id) => {
                write!(formatter, "unknown editor command id: {command_id}")
            }
            Self::Command(message) => write!(formatter, "editor command failed: {message}"),
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

impl From<crate::api::CditorError> for EditorError {
    fn from(error: crate::api::CditorError) -> Self {
        match error {
            crate::api::CditorError::NotReady => Self::NotReady,
            crate::api::CditorError::Readonly => Self::Readonly,
            other => Self::Command(other.to_string()),
        }
    }
}
