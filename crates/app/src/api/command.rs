use cditor_core::{edit::TransactionId, rich_text::RichBlockKind};

use super::document::BlockInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockTransform {
    Kind(RichBlockKind),
    /// Switch to `kind`, or back to a paragraph when the focused block already
    /// has that kind. This is the behavior expected by configurable Markdown
    /// shortcuts such as "toggle bullet list" and "toggle quote".
    ToggleKind(RichBlockKind),
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CditorCommand {
    Undo,
    Redo,
    SelectAll,
    DeleteSelection,
    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    ToggleStrike,
    ToggleInlineCode,
    InsertParagraphAfter,
    IndentBlock,
    OutdentBlock,
    InsertBlock(BlockInput),
    TransformBlock(BlockTransform),
    DeleteCurrentBlock,
    DeleteSelectedBlocks,
    DuplicateSelectedBlocks,
    ToggleTodoChecked,
    InsertTable { rows: usize, columns: usize },
    InsertImage,
    InsertWhiteboard,
    InsertMermaid,
    FoldHeading,
    UnfoldHeading,
}

impl CditorCommand {
    pub const fn stable_id(&self) -> &'static str {
        match self {
            Self::Undo => "edit.undo",
            Self::Redo => "edit.redo",
            Self::SelectAll => "edit.select_all",
            Self::DeleteSelection => "edit.delete_selection",
            Self::ToggleBold => "format.toggle_bold",
            Self::ToggleItalic => "format.toggle_italic",
            Self::ToggleUnderline => "format.toggle_underline",
            Self::ToggleStrike => "format.toggle_strike",
            Self::ToggleInlineCode => "format.toggle_inline_code",
            Self::InsertParagraphAfter => "block.insert_paragraph_after",
            Self::IndentBlock => "block.indent",
            Self::OutdentBlock => "block.outdent",
            Self::InsertBlock(_) => "block.insert",
            Self::TransformBlock(BlockTransform::Kind(kind)) => set_block_kind_id(kind),
            Self::TransformBlock(BlockTransform::ToggleKind(kind)) => toggle_block_kind_id(kind),
            Self::DeleteCurrentBlock => "block.delete_current",
            Self::DeleteSelectedBlocks => "block.delete_selected",
            Self::DuplicateSelectedBlocks => "block.duplicate_selected",
            Self::ToggleTodoChecked => "block.toggle_todo_checked",
            Self::InsertTable { .. } => "block.insert_table",
            Self::InsertImage => "block.insert_image",
            Self::InsertWhiteboard => "block.insert_whiteboard",
            Self::InsertMermaid => "block.insert_mermaid",
            Self::FoldHeading => "heading.fold",
            Self::UnfoldHeading => "heading.unfold",
        }
    }

    /// Resolve a persisted command id into a parameter-free command suitable
    /// for menus and externally configured keyboard shortcuts.
    ///
    /// Commands that require runtime arguments, such as an arbitrary
    /// `InsertBlock` or a table size, intentionally are not resolved here.
    pub fn from_stable_id(id: &str) -> Option<Self> {
        let command = match id {
            "edit.undo" => Self::Undo,
            "edit.redo" => Self::Redo,
            "edit.select_all" => Self::SelectAll,
            "edit.delete_selection" => Self::DeleteSelection,
            "format.toggle_bold" => Self::ToggleBold,
            "format.toggle_italic" => Self::ToggleItalic,
            "format.toggle_underline" => Self::ToggleUnderline,
            "format.toggle_strike" => Self::ToggleStrike,
            "format.toggle_inline_code" => Self::ToggleInlineCode,
            "block.insert_paragraph_after" => Self::InsertParagraphAfter,
            "block.indent" => Self::IndentBlock,
            "block.outdent" => Self::OutdentBlock,
            "block.delete_current" => Self::DeleteCurrentBlock,
            "block.delete_selected" => Self::DeleteSelectedBlocks,
            "block.duplicate_selected" => Self::DuplicateSelectedBlocks,
            "block.toggle_todo_checked" => Self::ToggleTodoChecked,
            "block.set_paragraph" => transform(RichBlockKind::Paragraph),
            "block.set_heading_1" => heading(1),
            "block.set_heading_2" => heading(2),
            "block.set_heading_3" => heading(3),
            "block.set_heading_4" => heading(4),
            "block.set_heading_5" => heading(5),
            "block.set_heading_6" => heading(6),
            "block.toggle_bullet_list" => toggle(RichBlockKind::BulletedList),
            "block.toggle_ordered_list" => toggle(RichBlockKind::NumberedList),
            "block.toggle_task_list" => toggle(RichBlockKind::Todo { checked: false }),
            "block.toggle_quote" => toggle(RichBlockKind::Quote),
            "block.toggle_callout" => toggle(RichBlockKind::Callout {
                variant: cditor_core::rich_text::CalloutVariant::Note,
            }),
            "block.toggle_toggle" => toggle(RichBlockKind::Toggle),
            "block.toggle_code" => toggle(RichBlockKind::Code { language: None }),
            "block.toggle_math" => toggle(RichBlockKind::Math),
            "block.toggle_mermaid" => toggle(RichBlockKind::Mermaid),
            "heading.fold" => Self::FoldHeading,
            "heading.unfold" => Self::UnfoldHeading,
            _ => return None,
        };
        Some(command)
    }

    /// Metadata used by a host settings page to enumerate commands without
    /// depending on editor internals.
    pub fn shortcut_descriptors() -> Vec<CommandDescriptor> {
        SHORTCUT_COMMANDS
            .iter()
            .map(|(id, title)| CommandDescriptor {
                id: (*id).to_owned(),
                title: (*title).to_owned(),
            })
            .collect()
    }

    pub fn descriptor(&self) -> CommandDescriptor {
        let id = self.stable_id();
        let title = SHORTCUT_COMMANDS
            .iter()
            .find_map(|(candidate, title)| (*candidate == id).then_some(*title))
            .unwrap_or("Cditor Command");
        CommandDescriptor {
            id: id.to_owned(),
            title: title.to_owned(),
        }
    }
}

fn transform(kind: RichBlockKind) -> CditorCommand {
    CditorCommand::TransformBlock(BlockTransform::Kind(kind))
}

fn toggle(kind: RichBlockKind) -> CditorCommand {
    CditorCommand::TransformBlock(BlockTransform::ToggleKind(kind))
}

fn heading(level: u8) -> CditorCommand {
    transform(RichBlockKind::Heading { level })
}

const fn set_block_kind_id(kind: &RichBlockKind) -> &'static str {
    match kind {
        RichBlockKind::Paragraph => "block.set_paragraph",
        RichBlockKind::Heading { level: 1 } => "block.set_heading_1",
        RichBlockKind::Heading { level: 2 } => "block.set_heading_2",
        RichBlockKind::Heading { level: 3 } => "block.set_heading_3",
        RichBlockKind::Heading { level: 4 } => "block.set_heading_4",
        RichBlockKind::Heading { level: 5 } => "block.set_heading_5",
        RichBlockKind::Heading { level: 6 } => "block.set_heading_6",
        _ => "block.transform",
    }
}

const fn toggle_block_kind_id(kind: &RichBlockKind) -> &'static str {
    match kind {
        RichBlockKind::BulletedList => "block.toggle_bullet_list",
        RichBlockKind::NumberedList => "block.toggle_ordered_list",
        RichBlockKind::Todo { .. } => "block.toggle_task_list",
        RichBlockKind::Quote => "block.toggle_quote",
        RichBlockKind::Callout { .. } => "block.toggle_callout",
        RichBlockKind::Toggle => "block.toggle_toggle",
        RichBlockKind::Code { .. } => "block.toggle_code",
        RichBlockKind::Math => "block.toggle_math",
        RichBlockKind::Mermaid => "block.toggle_mermaid",
        _ => "block.transform",
    }
}

const SHORTCUT_COMMANDS: &[(&str, &str)] = &[
    ("edit.undo", "Undo"),
    ("edit.redo", "Redo"),
    ("edit.select_all", "Select All"),
    ("edit.delete_selection", "Delete Selection"),
    ("format.toggle_bold", "Toggle Bold"),
    ("format.toggle_italic", "Toggle Italic"),
    ("format.toggle_underline", "Toggle Underline"),
    ("format.toggle_strike", "Toggle Strikethrough"),
    ("format.toggle_inline_code", "Toggle Inline Code"),
    ("block.set_paragraph", "Paragraph"),
    ("block.set_heading_1", "Heading 1"),
    ("block.set_heading_2", "Heading 2"),
    ("block.set_heading_3", "Heading 3"),
    ("block.set_heading_4", "Heading 4"),
    ("block.set_heading_5", "Heading 5"),
    ("block.set_heading_6", "Heading 6"),
    ("block.toggle_bullet_list", "Toggle Bullet List"),
    ("block.toggle_ordered_list", "Toggle Ordered List"),
    ("block.toggle_task_list", "Toggle Task List"),
    ("block.toggle_quote", "Toggle Quote"),
    ("block.toggle_callout", "Toggle Callout"),
    ("block.toggle_toggle", "Toggle Collapsible Block"),
    ("block.toggle_code", "Toggle Code Block"),
    ("block.toggle_math", "Toggle Math Block"),
    ("block.toggle_mermaid", "Toggle Mermaid Block"),
    ("block.toggle_todo_checked", "Toggle Task Checked"),
    ("block.insert_paragraph_after", "Insert Paragraph Below"),
    ("block.indent", "Indent Block"),
    ("block.outdent", "Outdent Block"),
    ("block.delete_current", "Delete Current Block"),
    ("block.delete_selected", "Delete Selected Blocks"),
    ("block.duplicate_selected", "Duplicate Selected Blocks"),
    ("heading.fold", "Fold Heading"),
    ("heading.unfold", "Unfold Heading"),
];

/// GPUI action used by host-defined keymaps. The host binds any keystroke to a
/// stable command id while the editor keeps the execution logic internally.
#[derive(Debug, Clone, PartialEq, Eq, gpui::Action)]
#[action(namespace = cditor, no_json)]
pub struct CditorCommandAction {
    pub command_id: String,
}

impl CditorCommandAction {
    pub fn new(command_id: impl Into<String>) -> Self {
        Self {
            command_id: command_id.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CditorKeyBinding {
    pub keystrokes: String,
    pub command_id: String,
}

impl CditorKeyBinding {
    pub fn new(keystrokes: impl Into<String>, command_id: impl Into<String>) -> Self {
        Self {
            keystrokes: keystrokes.into(),
            command_id: command_id.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandState {
    pub enabled: bool,
    pub active: bool,
    pub visible: bool,
}

impl CommandState {
    pub const DISABLED: Self = Self {
        enabled: false,
        active: false,
        visible: true,
    };
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CommandOutcome {
    pub changed: bool,
    pub transaction_id: Option<TransactionId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDescriptor {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashItem {
    pub command_id: String,
    pub title: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarItem {
    pub command_id: String,
    pub label: String,
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn shortcut_catalog_ids_are_unique_and_resolvable() {
        let descriptors = CditorCommand::shortcut_descriptors();
        let ids = descriptors
            .iter()
            .map(|descriptor| descriptor.id.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), descriptors.len());
        for descriptor in descriptors {
            let command = CditorCommand::from_stable_id(&descriptor.id).unwrap();
            assert_eq!(command.stable_id(), descriptor.id);
        }
    }

    #[test]
    fn parameterized_commands_are_not_fabricated_from_generic_ids() {
        assert!(CditorCommand::from_stable_id("block.insert").is_none());
        assert!(CditorCommand::from_stable_id("block.insert_table").is_none());
        assert!(CditorCommand::from_stable_id("block.transform").is_none());
    }
}
