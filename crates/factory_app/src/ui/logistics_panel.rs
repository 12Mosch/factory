//! Logistic chest section of the container window: the chest's role in its
//! network, the rows it configures, and what the network currently holds.
//!
//! The rows reuse the circuit editors' widgets and their signal picker rather
//! than growing a second picker: "choose an item" is the same interaction
//! wherever it appears, and the picker already knows how to restrict itself to
//! items (see [`SignalSlot::is_item_only`]).

use bevy::prelude::*;
use factory_data::{LogisticChestMode, LogisticChestPrototype};
use factory_sim::{EntityId, LogisticRequest, Simulation};

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::circuit::state::SignalSlot;
use crate::ui::circuit::widgets::{
    CircuitLabel, CircuitLabelKind, CircuitSignalButton, LABEL_COLOR, spawn_button, spawn_caption,
    spawn_heading, spawn_row, spawn_stepper,
};
use crate::ui::resources::OpenContainer;

/// Step buttons multiply their delta by this, because logistic requests are
/// counted in stacks-worth of items rather than in single units: stepping a
/// 1200-plate request one plate at a time would be unusable.
const REQUEST_STEP_SCALE: u32 = 10;

/// Adjusts one row's requested amount.
#[derive(Component)]
pub(crate) struct LogisticRequestStepButton {
    pub(crate) slot_index: usize,
    pub(crate) delta: i32,
}

/// What a live text node in the logistic panel displays.
///
/// Keyed like [`CircuitLabel`] and for the same reason: every label writes
/// `Text`, so one component with a discriminant keeps the refresh system to a
/// single query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LogisticLabelKind {
    /// Amount one row asks for.
    RequestCount(usize),
    /// What the network holds of the row's item.
    RequestNetworkStock(usize),
    /// Which network the chest belongs to, or that it belongs to none.
    NetworkStatus,
}

#[derive(Component)]
pub(crate) struct LogisticLabel(pub(crate) LogisticLabelKind);

pub(crate) fn spawn_logistic_chest_panel(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    logistic_chest: LogisticChestPrototype,
) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|panel| {
            spawn_heading(panel, "Logistic Network");
            spawn_caption(panel, mode_summary(logistic_chest.mode));
            panel.spawn((
                Text::new(String::new()),
                TextFont::from_font_size(10.0),
                TextColor(LABEL_COLOR),
                LogisticLabel(LogisticLabelKind::NetworkStatus),
            ));
            for slot_index in 0..usize::from(logistic_chest.request_slot_count) {
                spawn_request_row(panel, logistic_chest.mode, slot_index);
            }
        });
}

fn spawn_request_row(
    panel: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    mode: LogisticChestMode,
    slot_index: usize,
) {
    let slot = SignalSlot::LogisticRequest(slot_index);
    spawn_row(panel, |row| {
        spawn_button(
            row,
            50.0,
            "--",
            CircuitSignalButton(slot),
            CircuitLabel(CircuitLabelKind::Signal(slot)),
        );
        // A storage chest's row is a filter, so it has no amount to step.
        if mode.requests_items() {
            row.spawn((
                Text::new("0".to_string()),
                TextFont::from_font_size(10.0),
                TextColor(Color::WHITE),
                Node {
                    width: Val::Px(44.0),
                    ..default()
                },
                LogisticLabel(LogisticLabelKind::RequestCount(slot_index)),
            ));
            spawn_stepper(row, |delta| LogisticRequestStepButton { slot_index, delta });
        }
    });
    // What the network holds of the row's item goes underneath rather than
    // beside it: the row above is already as wide as the panel.
    panel.spawn((
        Text::new(String::new()),
        TextFont::from_font_size(10.0),
        TextColor(LABEL_COLOR),
        LogisticLabel(LogisticLabelKind::RequestNetworkStock(slot_index)),
    ));
}

fn mode_summary(mode: LogisticChestMode) -> &'static str {
    match mode {
        LogisticChestMode::PassiveProvider => "Passive provider: supplies on request.",
        LogisticChestMode::ActiveProvider => "Active provider: pushes its contents out.",
        LogisticChestMode::Storage => "Storage: accepts leftovers, or only the filtered item.",
        LogisticChestMode::Buffer => "Buffer: keeps a stock and supplies from it.",
        LogisticChestMode::Requester => "Requester: asks the network for its contents.",
    }
}

/// Refreshes every live label in the open logistic chest panel.
pub(crate) fn update_logistic_panel(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut labels: Query<(&LogisticLabel, &mut Text)>,
) {
    let Some(entity_id) = open_container.entity_id else {
        return;
    };
    let sim = sim.read();

    for (label, mut text) in &mut labels {
        text.0 = match label.0 {
            LogisticLabelKind::RequestCount(index) => request(&sim, entity_id, index)
                .map(|request| request.count.to_string())
                .unwrap_or_else(|| "0".to_string()),
            LogisticLabelKind::RequestNetworkStock(index) => {
                network_stock_text(&sim, entity_id, index)
            }
            LogisticLabelKind::NetworkStatus => {
                match sim.logistic_network_id_for_chest(entity_id) {
                    Some(network_id) => format!("Network {network_id}"),
                    None => "No roboport covers this chest".to_string(),
                }
            }
        };
    }
}

/// How much of a row's item the chest's network holds and still wants.
///
/// Read from the logistic index, so opening a chest never walks the network's
/// other chests to answer it.
fn network_stock_text(sim: &Simulation, entity_id: EntityId, slot_index: usize) -> String {
    let Some(item_id) = request(sim, entity_id, slot_index).and_then(|request| request.item) else {
        return String::new();
    };
    let Some(network_id) = sim.logistic_network_id_for_chest(entity_id) else {
        return String::new();
    };
    let totals = sim.logistic_network_item_totals(network_id, item_id);
    format!(
        "network: {} available, {} requested",
        totals.available, totals.requested
    )
}

fn request(sim: &Simulation, entity_id: EntityId, slot_index: usize) -> Option<LogisticRequest> {
    sim.logistic_chest_state(entity_id)?
        .requests
        .get(slot_index)
        .copied()
}

pub(crate) fn handle_logistic_request_step_buttons(
    mut buttons: Query<(&Interaction, &LogisticRequestStepButton), Changed<Interaction>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some((slot_index, delta)) = buttons
        .iter_mut()
        .find(|(interaction, _)| **interaction == Interaction::Pressed)
        .map(|(_, button)| (button.slot_index, button.delta))
    else {
        return;
    };
    let Some(entity_id) = open_container.entity_id else {
        return;
    };
    sounds.write(SoundEvent::UiClick);

    let command = {
        let sim = sim.read();
        // Stepping an empty row would ask for nothing at all, so it is a no-op
        // until the player picks an item for it.
        let Some(current) = request(&sim, entity_id, slot_index).filter(|row| row.item.is_some())
        else {
            return;
        };
        let step = delta.unsigned_abs().saturating_mul(REQUEST_STEP_SCALE);
        let count = if delta >= 0 {
            current.count.saturating_add(step)
        } else {
            current.count.saturating_sub(step)
        };
        SimCommandRequest(factory_sim::SimCommand::SetLogisticRequest {
            entity_id,
            slot_index,
            request: LogisticRequest { count, ..current },
        })
    };
    commands.write(command);
}
