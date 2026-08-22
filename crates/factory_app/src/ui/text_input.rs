use bevy::prelude::*;
use bevy::text::{EditableText, TextEdit};

/// Creates the shared single-line editor used by the game's text fields.
pub(crate) fn single_line_editor(
    initial_value: &str,
    max_characters: Option<usize>,
) -> EditableText {
    EditableText {
        max_characters,
        ..EditableText::new(initial_value)
    }
}

/// Copies an editor value without exposing Parley's split-string storage to
/// the rest of the UI state.
pub(crate) fn editor_value(editor: &EditableText) -> String {
    editor.value().to_string()
}

/// Applies an external state change without disturbing the caret when the
/// editor already contains the requested value.
pub(crate) fn set_editor_value(editor: &mut EditableText, value: &str) {
    if editor.value() == value {
        return;
    }
    editor.clear();
    editor.editor_mut().set_text(value);
    editor.queue_edit(TextEdit::TextEnd(false));
}

pub(crate) fn is_non_control(character: char) -> bool {
    !character.is_control()
}
