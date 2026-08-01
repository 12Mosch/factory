//! Small reusable widgets the circuit editors are assembled from.
//!
//! Every editor is built once with a fixed structure and refreshed by updating
//! label text, the same approach the module and machine panels use. That keeps
//! the panels free of snapshot churn while the player edits them.

use bevy::prelude::*;

use super::state::SignalSlot;

pub(crate) const BUTTON_BACKGROUND: Color = Color::srgba(0.16, 0.17, 0.19, 0.96);
pub(crate) const LABEL_COLOR: Color = Color::srgb(0.82, 0.84, 0.86);
pub(crate) const HEADING_COLOR: Color = Color::srgb(0.78, 0.86, 0.96);

/// What a live text node in a circuit editor displays.
///
/// One keyed component rather than a marker type per field: every label writes
/// `Text`, and Bevy proves query disjointness from filters rather than from
/// the components a query fetches, so separate markers would make the refresh
/// system's queries conflict at runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CircuitLabelKind {
    /// Signal held by a slot, or its number when the slot is an operand set
    /// to a constant.
    Signal(SignalSlot),
    /// Whether an operand is in signal or number mode.
    OperandMode(SignalSlot),
    /// Value of one constant combinator row.
    SlotValue(usize),
    Comparator,
    Operation,
    ReadContents,
    ConstantEnabled,
    DeciderOutputValue,
    /// Live summary of the signals reaching the entity.
    Status,
}

#[derive(Component)]
pub(crate) struct CircuitLabel(pub(crate) CircuitLabelKind);

/// Opens the signal picker for a slot.
#[derive(Component)]
pub(crate) struct CircuitSignalButton(pub(crate) SignalSlot);

/// Switches an operand between a signal and a plain number.
#[derive(Component)]
pub(crate) struct CircuitOperandModeButton(pub(crate) SignalSlot);

/// Adjusts an operand's constant by a fixed step.
#[derive(Component)]
pub(crate) struct CircuitConstantStepButton {
    pub(crate) slot: SignalSlot,
    pub(crate) delta: i32,
}

/// Adjusts a constant combinator row's value by a fixed step.
#[derive(Component)]
pub(crate) struct CircuitSlotValueStepButton {
    pub(crate) slot_index: usize,
    pub(crate) delta: i32,
}

#[derive(Component)]
pub(crate) struct CircuitComparatorButton;

#[derive(Component)]
pub(crate) struct CircuitOperationButton;

/// Clears the entity's enable condition, leaving it always on.
#[derive(Component)]
pub(crate) struct CircuitClearConditionButton;

#[derive(Component)]
pub(crate) struct CircuitReadContentsButton;

#[derive(Component)]
pub(crate) struct ConstantCombinatorEnabledButton;

#[derive(Component)]
pub(crate) struct DeciderOutputValueButton;

pub(crate) fn spawn_heading(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, text: &str) {
    parent.spawn((
        Text::new(text.to_string()),
        TextFont::from_font_size(12.0),
        TextColor(HEADING_COLOR),
    ));
}

pub(crate) fn spawn_caption(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, text: &str) {
    parent.spawn((
        Text::new(text.to_string()),
        TextFont::from_font_size(10.0),
        TextColor(LABEL_COLOR),
    ));
}

/// A [`spawn_row`] whose controls fall onto a second line rather than off the
/// side of the panel.
///
/// For rows whose length is not fixed by the layout but by what the thing being
/// edited turns out to be: a wait condition comparing a signal against a number
/// carries twice the controls of one that waits for a full load, and the wide
/// case does not fit the panel the narrow case sized.
pub(crate) fn spawn_wrapping_row(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    build: impl FnOnce(&mut bevy::ecs::hierarchy::ChildSpawnerCommands),
) {
    parent
        .spawn((
            Node {
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(4.0),
                row_gap: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(build);
}

/// A horizontal row the editors lay their controls out in.
pub(crate) fn spawn_row(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    build: impl FnOnce(&mut bevy::ecs::hierarchy::ChildSpawnerCommands),
) {
    parent
        .spawn((
            Node {
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(build);
}

/// A labelled button carrying `marker` on the button and `label` on its text,
/// so the same helper serves both static and live-updating buttons.
pub(crate) fn spawn_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    width: f32,
    text: &str,
    marker: impl Bundle,
    label: impl Bundle,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(width),
                height: Val::Px(20.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(BUTTON_BACKGROUND),
            marker,
        ))
        .with_child((
            Text::new(text.to_string()),
            TextFont::from_font_size(10.0),
            TextColor(Color::WHITE),
            TextLayout::justify(Justify::Center),
            label,
        ));
}

/// The `-10 -1 +1 +10` stepper shared by operand constants and constant
/// combinator rows. `make_marker` adapts it to whichever marker the caller
/// needs, so both uses get the same steps and layout.
pub(crate) fn spawn_stepper<M: Bundle>(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    mut make_marker: impl FnMut(i32) -> M,
) {
    for delta in [-10, -1, 1, 10] {
        let label = if delta > 0 {
            format!("+{delta}")
        } else {
            delta.to_string()
        };
        spawn_button(parent, 26.0, &label, make_marker(delta), ());
    }
}
