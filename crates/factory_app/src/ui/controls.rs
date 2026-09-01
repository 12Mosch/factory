use bevy::prelude::*;

use crate::input::bindings::{
    ActionBindings, BindingInput, InputAction, InputBinding, current_modifiers,
};
use crate::ui::settings::{SettingsTab, SettingsWindowState};

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlRebindState {
    pub capturing: Option<InputAction>,
    skip_frame: bool,
    pub error: Option<String>,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlBindingButton(pub InputAction);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ControlsSnapshot {
    rows: Vec<ControlRowSnapshot>,
    capturing: Option<InputAction>,
    error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ControlRowSnapshot {
    action: InputAction,
    binding: String,
}

type ControlBindingButtonQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static ControlBindingButton),
    (Changed<Interaction>, With<Button>),
>;

pub(crate) fn controls_snapshot(
    bindings: &ActionBindings,
    state: &ControlRebindState,
) -> ControlsSnapshot {
    ControlsSnapshot {
        rows: InputAction::ALL
            .into_iter()
            .map(|action| ControlRowSnapshot {
                action,
                binding: bindings.display_name(action),
            })
            .collect(),
        capturing: state.capturing,
        error: state.error.clone(),
    }
}

pub(crate) fn handle_control_binding_buttons(
    mut buttons: ControlBindingButtonQuery,
    settings: Res<SettingsWindowState>,
    mut state: ResMut<ControlRebindState>,
) {
    if !settings.open || settings.active_tab != SettingsTab::Controls {
        return;
    }
    for (interaction, button) in &mut buttons {
        if *interaction == Interaction::Pressed {
            state.capturing = Some(button.0);
            state.skip_frame = true;
            state.error = None;
        }
    }
}

/// Captures the next non-modifier key or mouse press. The frame that clicked
/// the rebind button is skipped so that click cannot become its own binding.
pub(crate) fn capture_control_binding(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    mut bindings: ResMut<ActionBindings>,
    mut state: ResMut<ControlRebindState>,
    mut settings: ResMut<SettingsWindowState>,
) {
    let Some(action) = state.capturing else {
        return;
    };
    if state.skip_frame {
        state.skip_frame = false;
        return;
    }

    let pressed_key = keyboard.as_deref().and_then(|keys| {
        keys.get_just_pressed()
            .copied()
            .find(|key| !is_modifier(*key))
    });
    if pressed_key == Some(KeyCode::Escape) {
        state.capturing = None;
        state.error = None;
        return;
    }
    let binding = pressed_key
        .map(|key| key_binding(key, keyboard.as_deref()))
        .or_else(|| {
            mouse.as_deref().and_then(|buttons| {
                buttons
                    .get_just_pressed()
                    .copied()
                    .next()
                    .map(|button| InputBinding {
                        input: BindingInput::Mouse(button),
                        modifiers: current_modifiers(keyboard.as_deref()),
                    })
            })
        })
        .or_else(|| {
            keyboard.as_deref().and_then(|keys| {
                keys.get_just_released()
                    .copied()
                    .find(|key| is_modifier(*key))
                    .map(|key| key_binding(key, Some(keys)))
            })
        });
    let Some(binding) = binding else {
        return;
    };

    match bindings.rebind(action, binding) {
        Ok(()) => {
            state.capturing = None;
            state.error = None;
            settings.dirty = true;
        }
        Err(conflict) => {
            state.error = Some(format!(
                "{} is already bound to {} in the same context.",
                binding.display_name(),
                conflict.action.label()
            ));
        }
    }
}

pub(crate) fn spawn_controls_content(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &ControlsSnapshot,
) {
    parent.spawn((
        Text::new("Active controls"),
        TextFont::from_font_size(16.0),
        TextColor(Color::srgb(0.94, 0.95, 0.90)),
    ));
    parent.spawn((
        Text::new(
            "Select a binding, then press a keyboard key or mouse button. Escape cancels capture.",
        ),
        TextFont::from_font_size(12.0),
        TextColor(Color::srgb(0.72, 0.76, 0.69)),
    ));
    if let Some(error) = &snapshot.error {
        parent.spawn((
            Text::new(error.clone()),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(1.0, 0.48, 0.34)),
        ));
    }

    let mut previous_category = None;
    for row in &snapshot.rows {
        let category = row.action.category();
        if previous_category != Some(category) {
            parent.spawn((
                Text::new(category),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.72, 0.86, 0.52)),
                Node {
                    margin: UiRect::top(Val::Px(8.0)),
                    ..default()
                },
            ));
            previous_category = Some(category);
        }
        parent
            .spawn(Node {
                width: Val::Percent(100.0),
                min_height: Val::Px(34.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: Val::Px(10.0),
                ..default()
            })
            .with_children(|line| {
                line.spawn((
                    Text::new(row.action.label()),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::srgb(0.86, 0.88, 0.82)),
                    Node {
                        flex_grow: 1.0,
                        ..default()
                    },
                ));
                let capturing = snapshot.capturing == Some(row.action);
                line.spawn((
                    Button,
                    Node {
                        min_width: Val::Px(170.0),
                        min_height: Val::Px(30.0),
                        padding: UiRect::horizontal(Val::Px(9.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(if capturing {
                        Color::srgb(0.24, 0.30, 0.16)
                    } else {
                        Color::srgb(0.07, 0.09, 0.075)
                    }),
                    BorderColor::all(if capturing {
                        Color::srgb(0.82, 0.94, 0.40)
                    } else {
                        Color::srgb(0.43, 0.53, 0.38)
                    }),
                    ControlBindingButton(row.action),
                ))
                .with_child((
                    Text::new(if capturing {
                        "PRESS A KEY…".to_string()
                    } else {
                        row.binding.clone()
                    }),
                    TextFont::from_font_size(11.0),
                    TextColor(Color::WHITE),
                ));
            });
    }
}

fn is_modifier(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::ControlLeft
            | KeyCode::ControlRight
            | KeyCode::AltLeft
            | KeyCode::AltRight
            | KeyCode::ShiftLeft
            | KeyCode::ShiftRight
            | KeyCode::SuperLeft
            | KeyCode::SuperRight
    )
}

fn key_binding(key: KeyCode, keyboard: Option<&ButtonInput<KeyCode>>) -> InputBinding {
    let mut modifiers = current_modifiers(keyboard);
    match key {
        KeyCode::ControlLeft | KeyCode::ControlRight => modifiers.control = false,
        KeyCode::AltLeft | KeyCode::AltRight => modifiers.alt = false,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => modifiers.shift = false,
        KeyCode::SuperLeft | KeyCode::SuperRight => modifiers.super_key = false,
        _ => {}
    }
    InputBinding {
        input: BindingInput::Key(key),
        modifiers,
    }
}
