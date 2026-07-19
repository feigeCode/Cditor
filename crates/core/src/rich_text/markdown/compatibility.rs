use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownFidelity {
    Semantic,
    Normalized,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownDiagnostic {
    pub severity: MarkdownDiagnosticSeverity,
    pub code: &'static str,
    pub message: String,
    pub source_range: Option<Range<usize>>,
    pub block_id: Option<u64>,
}

impl MarkdownDiagnostic {
    pub fn source(
        severity: MarkdownDiagnosticSeverity,
        code: &'static str,
        message: impl Into<String>,
        source_range: Range<usize>,
    ) -> Self {
        Self {
            severity,
            code,
            message: message.into(),
            source_range: Some(source_range),
            block_id: None,
        }
    }

    pub fn block(
        severity: MarkdownDiagnosticSeverity,
        code: &'static str,
        message: impl Into<String>,
        block_id: u64,
    ) -> Self {
        Self {
            severity,
            code,
            message: message.into(),
            source_range: None,
            block_id: Some(block_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownCompatibility {
    Editable,
    EditableWithNormalization(Vec<MarkdownDiagnostic>),
    SourceOnly(Vec<MarkdownDiagnostic>),
}

impl MarkdownCompatibility {
    pub fn from_diagnostics(diagnostics: &[MarkdownDiagnostic]) -> Self {
        if diagnostics.is_empty() {
            Self::Editable
        } else if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == MarkdownDiagnosticSeverity::Error)
        {
            Self::SourceOnly(diagnostics.to_vec())
        } else if diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity == MarkdownDiagnosticSeverity::Info)
        {
            // Informational diagnostics describe syntax spelling changes such as
            // `2.` -> `1.` or `_emphasis_` -> `*emphasis*`. These are semantic
            // round-trips and should not interrupt normal WYSIWYG editing.
            Self::Editable
        } else {
            Self::EditableWithNormalization(diagnostics.to_vec())
        }
    }

    pub fn diagnostics(&self) -> &[MarkdownDiagnostic] {
        match self {
            Self::Editable => &[],
            Self::EditableWithNormalization(diagnostics) | Self::SourceOnly(diagnostics) => {
                diagnostics
            }
        }
    }

    pub fn is_source_only(&self) -> bool {
        matches!(self, Self::SourceOnly(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownExportMode {
    Strict,
    BestEffort,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownExportResult {
    pub markdown: String,
    pub fidelity: MarkdownFidelity,
    pub diagnostics: Vec<MarkdownDiagnostic>,
}
