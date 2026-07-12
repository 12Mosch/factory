use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::components::{
    BuildCancelButton, BuildMenuButton, BuildRotateButton, BuildRotateButtonText, BuildSlotButton,
    BuildSlotCountText, BuildSlotLabelText, BuildStatusText,
};
use super::style::{
    action_background_color, action_border_color, direction_abbreviation, slot_background_color,
};
use crate::audio::SoundEvent;
use crate::build::resources::{
    BuildPlacementPreviewState, BuildPlacementState, BuildPlacementStatus, HotbarState,
    PastePlacementPreviewState, PlannerState, PlannerTool,
};
use crate::input::build::select_build_slot;
use crate::input::panels::world_input_blocked;
use crate::input::resources::AppInputState;
use crate::placement::build::{
    build_status_from_issues, build_status_from_preview, next_direction,
};
use crate::resources::SimResource;
use crate::ui::resources::TechnologyWindowState;
use crate::utils::compact_item_name;

type BuildSlotInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static BuildSlotButton),
    (Changed<Interaction>, With<Button>),
>;
type BuildRotateInteractionQuery<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (
        Changed<Interaction>,
        With<Button>,
        With<BuildRotateButton>,
        Without<BuildSlotButton>,
    ),
>;
type BuildCancelInteractionQuery<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (
        Changed<Interaction>,
        With<Button>,
        With<BuildCancelButton>,
        Without<BuildSlotButton>,
        Without<BuildRotateButton>,
    ),
>;
type BuildSlotVisualQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static BuildSlotButton,
        &'static Interaction,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
>;
type BuildCountTextQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static BuildSlotCountText,
        &'static mut Text,
        &'static mut TextColor,
    ),
    Without<BuildSlotLabelText>,
>;
type BuildLabelTextQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static BuildSlotLabelText,
        &'static mut Text,
        &'static mut TextColor,
    ),
    Without<BuildSlotCountText>,
>;
type BuildRotateButtonVisualQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Interaction,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
    (
        With<BuildRotateButton>,
        Without<BuildCancelButton>,
        Without<BuildMenuButton>,
        Without<BuildSlotButton>,
    ),
>;
type BuildCancelButtonVisualQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Interaction,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
    (
        With<BuildCancelButton>,
        Without<BuildRotateButton>,
        Without<BuildMenuButton>,
        Without<BuildSlotButton>,
    ),
>;
type BuildMenuButtonVisualQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Interaction,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
    (
        With<BuildMenuButton>,
        Without<BuildRotateButton>,
        Without<BuildCancelButton>,
        Without<BuildSlotButton>,
    ),
>;

#[derive(SystemParam)]
pub(crate) struct BuildBarButtonState<'w> {
    sim: Res<'w, SimResource>,
    input_state: Option<Res<'w, AppInputState>>,
    technology_window: Option<Res<'w, TechnologyWindowState>>,
    hotbar: Res<'w, HotbarState>,
    build_state: ResMut<'w, BuildPlacementState>,
    planner: ResMut<'w, PlannerState>,
    sounds: MessageWriter<'w, SoundEvent>,
}

pub(crate) fn handle_build_bar_button_clicks(
    mut slot_interactions: BuildSlotInteractionQuery,
    mut rotate_interactions: BuildRotateInteractionQuery,
    mut cancel_interactions: BuildCancelInteractionQuery,
    mut state: BuildBarButtonState,
) {
    if world_input_blocked(state.input_state.as_deref())
        || state
            .technology_window
            .as_deref()
            .is_some_and(|window| window.open)
    {
        return;
    }

    for (interaction, button) in &mut slot_interactions {
        if *interaction == Interaction::Pressed {
            state.sounds.write(SoundEvent::UiClick);
            select_build_slot(
                &state.sim.read(),
                state.technology_window.as_deref(),
                &state.hotbar,
                &mut state.build_state,
                &mut state.planner,
                button.slot_index,
            );
        }
    }

    for interaction in &mut rotate_interactions {
        if *interaction == Interaction::Pressed && state.build_state.selected.is_some() {
            state.sounds.write(SoundEvent::UiClick);
            state.build_state.direction = next_direction(state.build_state.direction);
        }
    }

    for interaction in &mut cancel_interactions {
        if *interaction == Interaction::Pressed {
            state.sounds.write(SoundEvent::UiClick);
            state.build_state.selected = None;
            state.build_state.last_status = BuildPlacementStatus::Ready;
        }
    }
}

pub(crate) fn update_build_bar_visuals(
    sim: Res<SimResource>,
    hotbar: Res<HotbarState>,
    build_state: Res<BuildPlacementState>,
    mut slot_buttons: BuildSlotVisualQuery,
    mut count_texts: BuildCountTextQuery,
    mut label_texts: BuildLabelTextQuery,
) {
    for (button, interaction, mut background, mut border) in &mut slot_buttons {
        let Some(selection) = hotbar.slot(button.slot_index) else {
            *background = BackgroundColor(Color::srgba(0.055, 0.055, 0.055, 0.80));
            *border = BorderColor::all(Color::srgba(0.30, 0.30, 0.28, 0.50));
            continue;
        };
        let selected = build_state.selected == Some(selection);
        let unlocked = sim.read().is_entity_unlocked(selection.prototype_id);
        let available = unlocked && sim.read().player_inventory().count(selection.item_id) > 0;
        *background = BackgroundColor(slot_background_color(*interaction, selected, available));
        *border = BorderColor::all(if selected {
            Color::srgb(0.94, 0.66, 0.20)
        } else {
            Color::srgba(0.44, 0.43, 0.39, 0.70)
        });
    }

    for (marker, mut text, mut color) in &mut count_texts {
        let Some(selection) = hotbar.slot(marker.slot_index) else {
            text.0.clear();
            continue;
        };
        let count = sim.read().player_inventory().count(selection.item_id);
        text.0 = count.to_string();
        *color = TextColor(if count == 0 {
            Color::srgb(0.62, 0.58, 0.52)
        } else {
            Color::srgb(0.91, 0.92, 0.86)
        });
    }

    for (marker, mut text, mut color) in &mut label_texts {
        let Some(selection) = hotbar.slot(marker.slot_index) else {
            text.0.clear();
            continue;
        };
        let sim = sim.read();
        let prototype = sim.catalog().entity(selection.prototype_id);
        text.0 = prototype
            .map(|prototype| compact_item_name(&prototype.name))
            .unwrap_or_default();
        let available = prototype.is_some()
            && sim.is_entity_unlocked(selection.prototype_id)
            && sim.player_inventory().count(selection.item_id) > 0;
        *color = TextColor(if available {
            Color::WHITE
        } else {
            Color::srgb(0.56, 0.55, 0.52)
        });
    }
}

pub(crate) fn update_build_bar_action_visuals(
    build_state: Res<BuildPlacementState>,
    mut rotate_buttons: BuildRotateButtonVisualQuery,
    mut rotate_texts: Query<&mut Text, With<BuildRotateButtonText>>,
    mut cancel_buttons: BuildCancelButtonVisualQuery,
    mut menu_buttons: BuildMenuButtonVisualQuery,
) {
    for (interaction, mut background, mut border) in &mut menu_buttons {
        *background = BackgroundColor(action_background_color(*interaction, true));
        *border = BorderColor::all(action_border_color(true));
    }

    for (interaction, mut background, mut border) in &mut rotate_buttons {
        let active = build_state.selected.is_some();
        *background = BackgroundColor(action_background_color(*interaction, active));
        *border = BorderColor::all(action_border_color(active));
    }
    for mut text in &mut rotate_texts {
        text.0 = format!("Rotate {}", direction_abbreviation(build_state.direction));
    }

    for (interaction, mut background, mut border) in &mut cancel_buttons {
        let active = build_state.selected.is_some();
        *background = BackgroundColor(action_background_color(*interaction, active));
        *border = BorderColor::all(action_border_color(active));
    }
}

pub(crate) fn update_build_status_text(
    build_state: Res<BuildPlacementState>,
    preview_state: Res<BuildPlacementPreviewState>,
    planner: Res<PlannerState>,
    paste_preview: Res<PastePlacementPreviewState>,
    sim: Res<SimResource>,
    mut texts: Query<(&mut Text, &mut TextColor), With<BuildStatusText>>,
) {
    let paste_status = (planner.tool == PlannerTool::Paste && paste_preview.active).then(|| {
        if paste_preview.issues.is_empty() {
            let count = planner
                .clipboard
                .as_ref()
                .map(|blueprint| blueprint.entities.len())
                .unwrap_or(0);
            BuildPlacementStatus::Placed(format!("Ready to paste {count} entities"))
        } else {
            build_status_from_issues(sim.read().catalog(), &paste_preview.issues)
                .unwrap_or(BuildPlacementStatus::Ready)
        }
    });
    let live_status = paste_status.or_else(|| {
        build_state
            .selected
            .and(preview_state.preview.as_ref())
            .and_then(|preview| build_status_from_preview(sim.read().catalog(), preview))
    });
    let status = live_status.as_ref().unwrap_or(&build_state.last_status);

    let (message, color) = match status {
        BuildPlacementStatus::Ready => ("Ready".to_string(), Color::srgb(0.78, 0.80, 0.76)),
        BuildPlacementStatus::Placed(message) => (message.clone(), Color::srgb(0.56, 0.92, 0.55)),
        BuildPlacementStatus::CannotPlace(message) => {
            (message.clone(), Color::srgb(0.96, 0.33, 0.27))
        }
        BuildPlacementStatus::MissingInventory(message) => {
            (message.clone(), Color::srgb(0.98, 0.72, 0.28))
        }
        BuildPlacementStatus::Locked(message) => (message.clone(), Color::srgb(0.98, 0.72, 0.28)),
    };

    for (mut text, mut text_color) in &mut texts {
        text.0 = message.clone();
        *text_color = TextColor(color);
    }
}
