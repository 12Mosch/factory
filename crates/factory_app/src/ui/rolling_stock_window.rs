//! The window a player opens on a piece of rolling stock.
//!
//! Kept apart from the container window rather than folded into it because
//! their subjects are different kinds of thing: a container window is opened on
//! an entity and everything hanging off it — modules, circuit conditions,
//! logistic requests — is keyed by [`factory_sim::EntityId`], which a wagon does
//! not have. What the two do share is the slot grid and the player panel beside
//! it, and those are reused rather than rewritten.
//!
//! Only one of the two is ever open: [`OpenContainer`] holds at most one target,
//! so clicking a wagon closes a chest and vice versa.

use bevy::prelude::*;
use bevy::ui_widgets::ScrollArea;
use factory_data::ItemId;
use factory_sim::{
    InventoryPanel, ROLLING_STOCK_FUEL_SLOT_INDEX, RollingStockId, Simulation, entity_access,
};

use crate::placement::build::entity_display_name as prototype_display_name;
use crate::resources::SimResource;
use crate::ui::inventory_panel::{
    spawn_inventory_transfer_feedback, spawn_labeled_slot, spawn_player_inventory_panel,
    spawn_slot_button,
};
use crate::ui::resources::{InventoryTransferFeedback, OpenContainer};
use crate::ui::train_schedule_panel::{
    ScheduleSnapshot, schedule_snapshot, spawn_train_schedule_panel,
};
use crate::ui::window_sync::{WindowRootQuery, WindowSync, sync_window};

/// The live fluid readout of an open tanker.
///
/// A component the window writes into every frame rather than text baked into
/// the snapshot below. What a wagon is carrying changes on every tick a pump is
/// working, and a snapshot carrying it would compare unequal that often — which
/// means despawning and respawning the whole window, player inventory and slot
/// grid included, sixty times a second, and clearing any transfer feedback with
/// it. The same split every other live readout here uses.
#[derive(Component)]
pub struct RollingStockFluidText;

/// What the window's *structure* was built from: how many slots of each kind it
/// draws, whether it has a tank to report on at all, and the two lines that
/// come and go. Nothing here changes while a train stands still, so the window
/// is built once and left alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RollingStockWindowSnapshot {
    stock_id: RollingStockId,
    title: String,
    cargo_slots: usize,
    fuel_slots: usize,
    /// Filter per cargo slot. The slot buttons draw the names themselves; this
    /// is in the snapshot so that setting or clearing one rebuilds the window
    /// rather than leaving a stale grid behind.
    filters: Vec<Option<ItemId>>,
    /// Whether there is a tank to draw a readout for — not what is in it, which
    /// [`RollingStockFluidText`] carries instead.
    has_fluid_box: bool,
    stopped: bool,
    /// The schedule editor's rows. In the structure snapshot because adding or
    /// removing an entry changes how many rows there are, and editing one
    /// changes what a row says; what the train is *doing* about the schedule is
    /// a live label instead, so the editor does not rebuild under the player's
    /// cursor every tick.
    schedule: Option<ScheduleSnapshot>,
}

pub(crate) fn sync_rolling_stock_window(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut open_container: ResMut<OpenContainer>,
    mut feedback: ResMut<InventoryTransferFeedback>,
    mut roots: WindowRootQuery<RollingStockWindowSnapshot>,
) {
    // A wagon can be mined out from under its own window, so the open target is
    // re-checked against the world every frame the way the container window
    // re-checks its entity.
    let open = open_container
        .rolling_stock
        .filter(|stock_id| sim.read().rolling_stock_piece(*stock_id).is_some());
    if open_container.rolling_stock.is_some() && open.is_none() {
        open_container.rolling_stock = None;
    }

    let result = sync_window(
        &mut commands,
        &mut roots,
        open.is_some(),
        true,
        || {
            let stock_id = open.expect("a snapshot is only built while stock is open");
            rolling_stock_window_snapshot(&sim.read(), stock_id)
        },
        rolling_stock_window_root,
        spawn_rolling_stock_window_contents,
    );
    // Spawned or rebuilt only — see `sync_container_window`, which shares this
    // feedback and is closed for the whole time this window is open.
    if matches!(result, WindowSync::Spawned | WindowSync::Rebuilt) {
        feedback.message = None;
    }
}

fn rolling_stock_window_snapshot(
    sim: &Simulation,
    stock_id: RollingStockId,
) -> RollingStockWindowSnapshot {
    let cargo_slots = entity_access::rolling_stock_panel_slot_count(
        sim,
        Some(stock_id),
        InventoryPanel::RollingStockCargo,
    );
    RollingStockWindowSnapshot {
        stock_id,
        title: sim
            .rolling_stock_piece(stock_id)
            .and_then(|stock| prototype_display_name(sim.catalog(), stock.prototype_id))
            .unwrap_or_else(|| "Rolling stock".to_string()),
        cargo_slots,
        fuel_slots: entity_access::rolling_stock_panel_slot_count(
            sim,
            Some(stock_id),
            InventoryPanel::RollingStockFuel,
        ),
        filters: (0..cargo_slots)
            .map(|slot_index| entity_access::rolling_stock_slot_filter(sim, stock_id, slot_index))
            .collect(),
        has_fluid_box: sim
            .rolling_stock_piece(stock_id)
            .is_some_and(|stock| !stock.fluid_boxes.is_empty()),
        stopped: sim.rolling_stock_is_stopped(stock_id),
        schedule: schedule_snapshot(sim, stock_id),
    }
}

/// Writes what the open tanker is carrying into the readout the window spawned
/// for it, leaving the rest of the window alone.
pub(crate) fn update_rolling_stock_fluid_text(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut texts: Query<&mut Text, With<RollingStockFluidText>>,
) {
    if texts.is_empty() {
        return;
    }
    let sim = sim.read();
    let readout = open_container
        .rolling_stock
        .and_then(|stock_id| fluid_readout(&sim, stock_id))
        .unwrap_or_default();
    for mut text in &mut texts {
        if text.0 != readout {
            text.0.clone_from(&readout);
        }
    }
}

/// What a fluid wagon is carrying, or `None` for a piece with no tank.
fn fluid_readout(sim: &Simulation, stock_id: RollingStockId) -> Option<String> {
    let stock = sim.rolling_stock_piece(stock_id)?;
    let state = stock.fluid_boxes.first()?;
    let capacity = sim
        .catalog()
        .entity(stock.prototype_id)?
        .fluid_boxes
        .first()?
        .capacity_milliunits;
    let name = state
        .fluid_id
        .and_then(|fluid_id| sim.catalog().fluid(fluid_id))
        .map(|fluid| prototype_display_name_for(&fluid.name))
        .unwrap_or_else(|| "Empty".to_string());
    Some(format!(
        "{name}: {} / {}",
        state.amount_milliunits / 1000,
        capacity / 1000
    ))
}

fn prototype_display_name_for(name: &str) -> String {
    crate::ui::formatting::format_recipe_display_name(name)
}

fn rolling_stock_window_root() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(12.0),
            top: Val::Px(12.0),
            padding: UiRect::all(Val::Px(10.0)),
            column_gap: Val::Px(10.0),
            align_items: AlignItems::FlexStart,
            ..default()
        },
        BackgroundColor(Color::srgba(0.03, 0.03, 0.035, 0.88)),
        GlobalZIndex(1100),
    )
}

fn spawn_rolling_stock_window_contents(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &RollingStockWindowSnapshot,
) {
    spawn_player_inventory_panel(root);
    // Bounded and scrolled, because the schedule editor below grows without a
    // ceiling: every stop, every OR alternative, and every ANDed condition adds
    // rows. Unbounded, a long schedule pushes its own remove buttons past the
    // bottom of the screen, where they cannot be reached to shorten it again.
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(270.0),
            max_height: Val::Vh(88.0),
            overflow: Overflow::scroll_y(),
            scrollbar_width: 10.0,
            ..default()
        },
        BackgroundColor(Color::NONE),
        ScrollArea,
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new(snapshot.title.clone()),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        // Why an inserter is not loading this wagon is the first question a
        // player asks at a station, and "the train has not stopped" is the
        // answer often enough to be worth saying outright.
        if !snapshot.stopped {
            panel.spawn((
                Text::new("Moving: inserters and pumps cannot reach it"),
                TextFont::from_font_size(11.0),
                TextColor(Color::srgb(0.98, 0.72, 0.28)),
            ));
        }
        if snapshot.has_fluid_box {
            panel.spawn((
                Text::new(""),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.86, 0.88, 0.82)),
                RollingStockFluidText,
            ));
        }
        if snapshot.fuel_slots > 0 {
            panel
                .spawn((
                    Node {
                        column_gap: Val::Px(6.0),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|slots| {
                    spawn_labeled_slot(
                        slots,
                        "Fuel",
                        InventoryPanel::RollingStockFuel,
                        ROLLING_STOCK_FUEL_SLOT_INDEX,
                    );
                });
        }
        if snapshot.cargo_slots > 0 {
            panel
                .spawn((
                    Node {
                        width: Val::Px(244.0),
                        flex_wrap: FlexWrap::Wrap,
                        row_gap: Val::Px(4.0),
                        column_gap: Val::Px(4.0),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|grid| {
                    for slot_index in 0..snapshot.cargo_slots {
                        spawn_slot_button(grid, InventoryPanel::RollingStockCargo, slot_index);
                    }
                });
            // The slots themselves show which of them are locked, so what is
            // left to say is how to lock one.
            panel.spawn((
                Text::new("Shift-click a slot to reserve it for what it holds"),
                TextFont::from_font_size(11.0),
                TextColor(Color::srgb(0.78, 0.80, 0.78)),
            ));
        }
        // Under the cargo, because a schedule is about the train while
        // everything above it is about this one piece of it.
        if let Some(schedule) = &snapshot.schedule {
            spawn_train_schedule_panel(panel, schedule);
        }
        spawn_inventory_transfer_feedback(panel);
    });
}
