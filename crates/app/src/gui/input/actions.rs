use gpui::{App, KeyBinding, Keystroke, actions};

use crate::api::{CditorCommand, CditorCommandAction, CditorError, CditorKeyBinding};

pub const CDITOR_KEY_CONTEXT: &str = "CditorEditor";

actions!(
    cditor,
    [
        Newline,
        SoftLineBreak,
        NewlineBelow,
        Tab,
        Backtab,
        Cancel,
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        MoveToLineStart,
        MoveToLineEnd,
        SelectToLineStart,
        SelectToLineEnd,
        Backspace,
        Delete,
        SelectAll,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        ToggleBold,
        ToggleItalic,
        ToggleUnderline,
        ToggleInlineCode,
        Duplicate,
    ]
);

/// Install the editor keymap once during application initialization.
///
/// `secondary` is GPUI's cross-platform primary editing modifier: Command on
/// macOS and Control on Windows/Linux. This mirrors Zed's action/keymap path
/// and deliberately leaves native key translation to GPUI.
pub fn bind_cditor_keys(cx: &mut App) {
    bind_cditor_core_keys(cx);
    bind_cditor_default_command_keys(cx);
}

/// Install only the non-configurable editing mechanics: text navigation,
/// deletion, clipboard, Enter/Tab and platform aliases. Hosts with their own
/// keymap should combine this with [`bind_cditor_command_keys`].
pub fn bind_cditor_core_keys(cx: &mut App) {
    let context = Some(CDITOR_KEY_CONTEXT);
    let bindings = vec![
        KeyBinding::new("enter", Newline, context),
        KeyBinding::new("shift-enter", SoftLineBreak, context),
        KeyBinding::new("secondary-enter", NewlineBelow, context),
        KeyBinding::new("tab", Tab, context),
        KeyBinding::new("shift-tab", Backtab, context),
        KeyBinding::new("escape", Cancel, context),
        KeyBinding::new("left", MoveLeft, context),
        KeyBinding::new("right", MoveRight, context),
        KeyBinding::new("up", MoveUp, context),
        KeyBinding::new("down", MoveDown, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("shift-up", SelectUp, context),
        KeyBinding::new("shift-down", SelectDown, context),
        KeyBinding::new("home", MoveToLineStart, context),
        KeyBinding::new("end", MoveToLineEnd, context),
        KeyBinding::new("shift-home", SelectToLineStart, context),
        KeyBinding::new("shift-end", SelectToLineEnd, context),
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("shift-backspace", Backspace, context),
        KeyBinding::new("delete", Delete, context),
        KeyBinding::new("secondary-c", Copy, context),
        KeyBinding::new("secondary-x", Cut, context),
        KeyBinding::new("secondary-v", Paste, context),
    ];
    cx.bind_keys(bindings);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        // Zed's default Windows editing aliases.
        KeyBinding::new("shift-delete", Cut, context),
        KeyBinding::new("secondary-insert", Copy, context),
        KeyBinding::new("shift-insert", Paste, context),
    ]);
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        // Native macOS/Emacs navigation aliases used by Zed.
        KeyBinding::new("ctrl-h", Backspace, context),
        KeyBinding::new("ctrl-d", Delete, context),
        KeyBinding::new("ctrl-b", MoveLeft, context),
        KeyBinding::new("ctrl-f", MoveRight, context),
        KeyBinding::new("ctrl-p", MoveUp, context),
        KeyBinding::new("ctrl-n", MoveDown, context),
        KeyBinding::new("ctrl-a", MoveToLineStart, context),
        KeyBinding::new("ctrl-e", MoveToLineEnd, context),
        KeyBinding::new("secondary-left", MoveToLineStart, context),
        KeyBinding::new("secondary-right", MoveToLineEnd, context),
        KeyBinding::new("secondary-shift-left", SelectToLineStart, context),
        KeyBinding::new("secondary-shift-right", SelectToLineEnd, context),
    ]);
}

/// Install Cditor's built-in command shortcuts. Kept separate from
/// [`bind_cditor_core_keys`] so a host settings system can own this layer.
pub fn bind_cditor_default_command_keys(cx: &mut App) {
    cx.bind_keys([
        command_key("secondary-a", "edit.select_all"),
        command_key("secondary-z", "edit.undo"),
        command_key("secondary-shift-z", "edit.redo"),
        command_key("secondary-b", "format.toggle_bold"),
        command_key("secondary-i", "format.toggle_italic"),
        command_key("secondary-u", "format.toggle_underline"),
        command_key("secondary-e", "format.toggle_inline_code"),
        command_key("secondary-d", "block.duplicate_selected"),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([command_key("secondary-y", "edit.redo")]);

    fn command_key(keystrokes: &str, command_id: &str) -> KeyBinding {
        KeyBinding::new(
            keystrokes,
            CditorCommandAction::new(command_id),
            Some(CDITOR_KEY_CONTEXT),
        )
    }
}

/// Bind host-configured keystrokes to stable Cditor command ids.
///
/// This intentionally accepts data rather than a fixed Rust action type per
/// command so a settings file can define and replace bindings at runtime.
pub fn bind_cditor_command_keys(
    cx: &mut App,
    bindings: impl IntoIterator<Item = CditorKeyBinding>,
) -> Result<(), CditorError> {
    let mut resolved = Vec::new();
    for binding in bindings {
        if CditorCommand::from_stable_id(&binding.command_id).is_none() {
            return Err(CditorError::InvalidInput(format!(
                "unknown shortcut command id: {}",
                binding.command_id
            )));
        }
        if binding.keystrokes.trim().is_empty() {
            return Err(CditorError::InvalidInput(
                "shortcut keystrokes cannot be empty".to_owned(),
            ));
        }
        for keystroke in binding.keystrokes.split_whitespace() {
            Keystroke::parse(keystroke).map_err(|error| {
                CditorError::InvalidInput(format!(
                    "invalid shortcut keystroke {keystroke:?}: {error}"
                ))
            })?;
        }
        resolved.push(KeyBinding::new(
            &binding.keystrokes,
            CditorCommandAction::new(binding.command_id),
            Some(CDITOR_KEY_CONTEXT),
        ));
    }
    cx.bind_keys(resolved);
    Ok(())
}

#[cfg(test)]
mod tests {
    use gpui::Keystroke;

    #[test]
    fn secondary_modifier_uses_the_host_editing_convention() {
        let parsed = Keystroke::parse("secondary-a").unwrap();
        #[cfg(target_os = "macos")]
        assert!(parsed.modifiers.platform && !parsed.modifiers.control);
        #[cfg(not(target_os = "macos"))]
        assert!(parsed.modifiers.control && !parsed.modifiers.platform);
    }
}
