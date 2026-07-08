use std::collections::HashMap;

use gpui::{ClipboardItem, Context, KeyDownEvent, Window};

use crate::core::ids::BlockId;
use crate::core::rich_text::InlineMark;
use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::clipboard_assets::image_asset_from_clipboard_item;
use crate::gui::image_preview::close_active_preview_if_escape_enabled;
use crate::gui::input::{GuiInputCommand, command_for_key_down};
use crate::gui::text::RichTextPlatformLayout;
use crate::runtime::DocumentRuntime;

impl CditorV2View {
    pub(in crate::gui::app) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key.as_str() == "escape" && close_active_preview_if_escape_enabled(cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let command = command_for_key_down(event);
        if command.should_stop_propagation() {
            self.apply_input_command(command, cx);
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(in crate::gui::app) fn apply_input_command(
        &mut self,
        command: GuiInputCommand,
        cx: &mut Context<Self>,
    ) {
        if matches!(command, GuiInputCommand::ToggleDebugOverlay) {
            self.show_debug = !self.show_debug;
            return;
        }
        if self.readonly {
            return;
        }
        let should_scroll_focus = !matches!(
            command,
            GuiInputCommand::Ignore
                | GuiInputCommand::ToggleDebugOverlay
                | GuiInputCommand::CopySelection
        );
        {
            let CditorViewState::Ready(runtime) = &mut self.state else {
                return;
            };
            match command {
                GuiInputCommand::Ignore | GuiInputCommand::ToggleDebugOverlay => {}
                GuiInputCommand::SelectAllFocusedText => {
                    runtime.select_focused_text_all();
                }
                GuiInputCommand::CopySelection => {
                    if let Some(text) = runtime.selected_focused_text() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                }
                GuiInputCommand::CutSelection => {
                    if let Some(text) = runtime.selected_focused_text() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        let changed = if runtime.has_cross_block_text_selection() {
                            runtime.delete_document_selection().unwrap_or(false)
                        } else {
                            runtime
                                .replace_text_in_focused_range(None, "")
                                .unwrap_or(false)
                        };
                        if changed {
                            self.mark_dirty(cx);
                        }
                    }
                }
                GuiInputCommand::PasteClipboard => {
                    if let Some(item) = cx.read_from_clipboard() {
                        let changed = if let Some(asset) = image_asset_from_clipboard_item(&item) {
                            runtime
                                .insert_image_asset_after_focused(asset.payload)
                                .is_ok()
                        } else if let Some(text) = item.text() {
                            match runtime.insert_markdown_paste(&text) {
                                Ok(true) => true,
                                Ok(false) | Err(_) => runtime
                                    .replace_text_in_focused_range(None, &text)
                                    .unwrap_or(false),
                            }
                        } else {
                            false
                        };
                        if changed {
                            self.mark_dirty(cx);
                        }
                    }
                }
                GuiInputCommand::UndoFocusedBlock => {
                    if runtime.undo_focused_block().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::RedoFocusedBlock => {
                    if runtime.redo_focused_block().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::InsertParagraphAfterFocused => {
                    if runtime.insert_paragraph_after_focused().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::InsertSoftLineBreak => {
                    if runtime.insert_soft_line_break().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::HandleEnter => {
                    if runtime.handle_enter().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::IndentBlock => {
                    if runtime.indent_focused_block().unwrap_or(false) {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::OutdentBlock => {
                    if runtime.outdent_focused_block().unwrap_or(false) {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::InsertSpaceOrMarkdownShortcut => {
                    if runtime.insert_space_or_markdown_shortcut().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::DeleteBackward => {
                    if runtime.delete_backward().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::DeleteForward => {
                    if runtime.delete_forward().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::MoveCaretLeft { extend_selection } => {
                    let _ = runtime.move_caret_left(extend_selection);
                }
                GuiInputCommand::MoveCaretRight { extend_selection } => {
                    let _ = runtime.move_caret_right(extend_selection);
                }
                GuiInputCommand::MoveCaretUp { extend_selection } => {
                    let moved_in_block = move_caret_vertically_in_focused_block(
                        &self.text_layouts,
                        runtime,
                        -1,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved_in_block {
                        let _ = runtime.move_caret_up(extend_selection);
                    }
                }
                GuiInputCommand::MoveCaretDown { extend_selection } => {
                    let moved_in_block = move_caret_vertically_in_focused_block(
                        &self.text_layouts,
                        runtime,
                        1,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved_in_block {
                        let _ = runtime.move_caret_down(extend_selection);
                    }
                }
                GuiInputCommand::ToggleBold => {
                    if runtime
                        .toggle_inline_mark_on_selection(InlineMark::Bold)
                        .is_ok()
                    {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::ToggleItalic => {
                    if runtime
                        .toggle_inline_mark_on_selection(InlineMark::Italic)
                        .is_ok()
                    {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::ToggleUnderline => {
                    if runtime
                        .toggle_inline_mark_on_selection(InlineMark::Underline)
                        .is_ok()
                    {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::ToggleInlineCode => {
                    if runtime
                        .toggle_inline_mark_on_selection(InlineMark::Code)
                        .is_ok()
                    {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::InsertChar(ch) => {
                    ensure_runtime_focus_for_insert_char(runtime);
                    if runtime.insert_char(ch).is_ok() {
                        self.mark_dirty(cx);
                    }
                }
            }
        }
        if should_scroll_focus && let CditorViewState::Ready(runtime) = &mut self.state {
            let _ = runtime.scroll_focused_block_into_view();
        }
    }
}

pub(in crate::gui::app) fn ensure_runtime_focus_for_insert_char(runtime: &mut DocumentRuntime) {
    if runtime.focused_block_id().is_none()
        && let Some(block_id) = runtime.first_visible_block_id()
    {
        runtime.focus_block(block_id);
    }
}

fn move_caret_vertically_in_focused_block(
    text_layouts: &HashMap<BlockId, RichTextPlatformLayout>,
    runtime: &mut DocumentRuntime,
    direction: i32,
    extend_selection: bool,
) -> Result<bool, String> {
    let Some(block_id) = runtime.focused_block_id() else {
        return Ok(false);
    };
    let Some(cache) = text_layouts.get(&block_id) else {
        return Ok(false);
    };
    let Some(current_content_version) = runtime.block_content_version(block_id) else {
        return Ok(false);
    };
    if cache.content_version != current_content_version {
        return Ok(false);
    }
    let Some(caret) = runtime.caret_offset_for_block(block_id) else {
        return Ok(false);
    };
    let Some(bounds) = crate::gui::text::platform_range_bounds(cache, caret..caret) else {
        return Ok(false);
    };
    let line_height = f32::from(cache.line_height).max(1.0);
    let target = gpui::point(
        bounds.left() + bounds.size.width / 2.0,
        bounds.top() + gpui::px(line_height * direction as f32),
    );
    let next = crate::gui::text::platform_index_for_point(cache, target);
    if next == caret {
        return Ok(false);
    }
    runtime.move_focused_caret_to_offset(block_id, next, extend_selection)?;
    Ok(true)
}
