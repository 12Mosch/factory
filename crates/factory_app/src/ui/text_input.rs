use bevy::prelude::*;
use bevy::text::{EditableText, TextEdit};

/// Runs after Bevy has applied queued text edits and before UI state reads the
/// resulting editor values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SystemSet)]
pub(crate) struct TextInputSanitization;

/// Removes disallowed characters after an edit instead of rejecting the whole
/// insertion, so pastes containing surrounding whitespace still retain their
/// valid content.
#[derive(Component, Clone)]
pub(crate) struct EditableTextSanitizer {
    filter: fn(char) -> bool,
    max_characters: Option<usize>,
    last_valid: String,
    was_composing: bool,
}

impl EditableTextSanitizer {
    /// Creates a sanitizer from a per-character predicate.
    pub(crate) fn new(
        initial_value: &str,
        filter: fn(char) -> bool,
        max_characters: Option<usize>,
    ) -> Self {
        let last_valid = sanitize_value(initial_value, filter, max_characters);
        Self {
            filter,
            max_characters,
            last_valid,
            was_composing: false,
        }
    }
}

/// Creates the shared single-line editor used by the game's text fields.
pub(crate) fn single_line_editor(
    initial_value: &str,
    max_characters: Option<usize>,
    filter: fn(char) -> bool,
) -> (EditableText, EditableTextSanitizer) {
    (
        EditableText::new(initial_value),
        EditableTextSanitizer::new(initial_value, filter, max_characters),
    )
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

/// Returns whether a character is suitable for a single-line name or search
/// field.
pub(crate) fn is_non_control(character: char) -> bool {
    !character.is_control()
}

/// Records composition state before Bevy applies an IME commit, allowing a
/// subsequent Enter handler to distinguish candidate acceptance from submit.
pub(crate) fn capture_editable_text_composition(
    mut editors: Query<(&EditableText, &mut EditableTextSanitizer)>,
) {
    for (editor, mut sanitizer) in &mut editors {
        sanitizer.was_composing = editor.is_composing();
    }
}

/// Returns whether an Enter press may submit this editor rather than merely
/// accepting an active IME composition.
pub(crate) fn can_submit(editor: &EditableText, sanitizer: &EditableTextSanitizer) -> bool {
    !editor.is_composing() && !sanitizer.was_composing
}

/// Strips invalid characters from changed editors before their values are
/// copied into application state.
pub(crate) fn sanitize_editable_text(
    mut editors: Query<(&mut EditableText, &mut EditableTextSanitizer), Changed<EditableText>>,
) {
    for (mut editor, mut sanitizer) in &mut editors {
        let value = editor_value(&editor);
        let filtered = value
            .chars()
            .filter(|character| (sanitizer.filter)(*character))
            .collect::<String>();
        let sanitized = if sanitizer
            .max_characters
            .is_none_or(|max| filtered.chars().count() <= max)
        {
            filtered
        } else {
            sanitizer.last_valid.clone()
        };
        if sanitized != value {
            set_editor_value(&mut editor, &sanitized);
        }
        sanitizer.last_valid = sanitized;
    }
}

/// Produces a bounded valid initial value for the sanitizer's rollback state.
fn sanitize_value(value: &str, filter: fn(char) -> bool, max_characters: Option<usize>) -> String {
    value
        .chars()
        .filter(|character| filter(*character))
        .take(max_characters.unwrap_or(usize::MAX))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizer_retains_valid_characters_from_mixed_input() {
        let mut app = App::new();
        app.add_systems(Update, sanitize_editable_text);
        let entity = app
            .world_mut()
            .spawn((
                EditableText::new("Main Base\r\n"),
                EditableTextSanitizer::new("Main Base", is_non_control, None),
            ))
            .id();

        app.update();

        let editor = app.world().entity(entity).get::<EditableText>().unwrap();
        assert_eq!(editor.value(), "Main Base");
    }

    #[test]
    fn sanitizer_can_extract_digits_from_pasted_seed() {
        let mut app = App::new();
        app.add_systems(Update, sanitize_editable_text);
        let entity = app
            .world_mut()
            .spawn((
                EditableText::new(" 12 34\n"),
                EditableTextSanitizer::new("", |character| character.is_ascii_digit(), Some(4)),
            ))
            .id();

        app.update();

        let editor = app.world().entity(entity).get::<EditableText>().unwrap();
        assert_eq!(editor.value(), "1234");
    }

    #[test]
    fn sanitizer_rejects_edits_whose_valid_content_exceeds_the_limit() {
        let mut app = App::new();
        app.add_systems(Update, sanitize_editable_text);
        let entity = app
            .world_mut()
            .spawn((
                EditableText::new("12345"),
                EditableTextSanitizer::new("1234", |character| character.is_ascii_digit(), Some(4)),
            ))
            .id();

        app.update();

        let editor = app.world().entity(entity).get::<EditableText>().unwrap();
        assert_eq!(editor.value(), "1234");
    }

    #[test]
    fn submission_is_suppressed_when_the_frame_started_in_composition() {
        let editor = EditableText::new("Factory");
        let mut sanitizer = EditableTextSanitizer::new("Factory", is_non_control, None);
        sanitizer.was_composing = true;

        assert!(!can_submit(&editor, &sanitizer));
    }
}
