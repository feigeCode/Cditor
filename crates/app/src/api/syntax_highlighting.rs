use std::{fmt, ops::Range};

/// A paint-only syntax style for a byte range in a code block.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SyntaxHighlightStyle {
    /// Packed `0xRRGGBB` foreground color.
    pub foreground: Option<u32>,
    /// Packed `0xRRGGBB` background color.
    pub background: Option<u32>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

/// A syntax style applied to a UTF-8 byte range in the original source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxHighlightRun {
    pub range: Range<usize>,
    pub style: SyntaxHighlightStyle,
}

/// Base colors used by the code-block container when a host provides highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxHighlightPalette {
    /// Packed `0xRRGGBB` background color.
    pub background: u32,
    /// Packed `0xRRGGBB` foreground color.
    pub foreground: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxHighlightError {
    pub message: String,
}

impl SyntaxHighlightError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SyntaxHighlightError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SyntaxHighlightError {}

/// Supplies paint-only syntax styles without owning editor text or scheduling.
pub trait SyntaxHighlightProvider: Send + Sync {
    /// Stable provider identifier used by builder diagnostics and equality.
    fn id(&self) -> &str;

    /// Changes whenever theme or language-registry state can change results.
    fn revision(&self) -> u64;

    fn palette(&self) -> SyntaxHighlightPalette;

    /// Returns style ranges into `source`. Missing ranges render with the palette foreground.
    fn highlight(
        &self,
        language: &str,
        source: &str,
    ) -> Result<Vec<SyntaxHighlightRun>, SyntaxHighlightError>;
}
