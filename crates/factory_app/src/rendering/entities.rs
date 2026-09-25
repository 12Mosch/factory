use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};
use factory_data::{CraftingCategory, EntityKind, EntityPrototypeId, PrototypeCatalog};
use factory_sim::{
    Direction, EntityFootprint, EntityId, PlacedEntity, RailSignalAspect, Simulation,
};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::constants::{
    CHEST_SPRITE_SIZE, MINING_DRILL_SPRITE_PADDING, TILE_SIZE, TRANSPORT_BELT_SPRITE_SIZE,
};
use crate::map::resources::VisibleChunks;
use crate::rendering::colors::{
    accumulator_color, arithmetic_combinator_color, assembler_color, beacon_color, boiler_color,
    centrifuge_color, chemical_plant_color, chest_color, constant_combinator_color,
    decider_combinator_color, electric_pole_color, enemy_spawner_color, furnace_color,
    gun_turret_color, heat_exchanger_color, heat_pipe_color, inserter_color, lab_color, lamp_color,
    laser_turret_color, mining_drill_color, nuclear_reactor_color, offshore_pump_color,
    oil_refinery_color, pipe_color, pump_color, pumpjack_color, radar_color, rail_ballast_color,
    rail_signal_color, roboport_color, rocket_silo_color, solar_panel_color, splitter_color,
    steam_engine_color, storage_tank_color, train_stop_color, transport_belt_color, wall_color,
};
use crate::rendering::resources::{PlacedEntitiesRenderSyncTime, VisibleEntityIds};
use crate::rendering::transforms::entity_translation;
use crate::rendering::visuals::{
    ConnectionMask, EntityVisualStyle, VisualAssets, spawn_entity_visual,
};

pub(crate) use crate::rendering::visuals::RocketSiloVisualPhase;
use crate::resources::SimResource;
use crate::ui::accessibility::ReadableWorldLabel;

#[derive(Component)]
pub(crate) struct PlacedEntitySprite {
    pub(crate) entity_id: EntityId,
}

#[derive(Component)]
pub(crate) struct RocketSiloSprite {
    pub(crate) visual_phase: RocketSiloVisualPhase,
    pub(crate) status_indicator: Entity,
}

#[derive(Component)]
pub(crate) struct RocketSiloStatusIndicator {
    pub(crate) operational_state: factory_sim::RocketSiloOperationalState,
}

#[derive(Component)]
pub(crate) struct RailSignalSprite {
    pub(crate) status_indicator: Entity,
}

#[derive(Component)]
pub(crate) struct RailSignalStatusIndicator;

#[derive(SystemParam)]
pub(crate) struct PlacedEntityRenderQueries<'w, 's> {
    sprites: Query<
        'w,
        's,
        (
            &'static mut Transform,
            &'static mut Sprite,
            Option<&'static RailSignalSprite>,
        ),
        With<PlacedEntitySprite>,
    >,
    signal_indicators: Query<'w, 's, &'static mut Text2d, With<RailSignalStatusIndicator>>,
}

#[derive(Default)]
pub(crate) struct RocketSiloRenderSyncState {
    initialized: bool,
    synced_tick: u64,
    sim_replacement_revision: u64,
}

#[derive(Default)]
pub(crate) struct VisibleEntitySyncState {
    initialized: bool,
    visible_revision: u64,
    entity_topology_revision: u64,
    entity_style_revision: u64,
    sim_replacement_revision: u64,
    next_ids: HashSet<EntityId>,
    affected_ids: HashSet<EntityId>,
    added: Vec<EntityId>,
    removed: Vec<EntityId>,
    style_dirty: Vec<EntityId>,
}

#[derive(Default)]
pub(crate) struct PlacedEntityRenderRegistry {
    entities: HashMap<EntityId, Entity>,
    pending_signals: HashSet<EntityId>,
}

impl From<factory_sim::RocketLaunchPhase> for RocketSiloVisualPhase {
    fn from(phase: factory_sim::RocketLaunchPhase) -> Self {
        match phase {
            factory_sim::RocketLaunchPhase::Idle => Self::Idle,
            factory_sim::RocketLaunchPhase::Sealed { .. } => Self::Sealed,
            factory_sim::RocketLaunchPhase::Rising { .. } => Self::Rising,
        }
    }
}

pub(crate) fn update_visible_entity_ids(
    sim: Res<SimResource>,
    visible: Res<VisibleChunks>,
    mut visible_entity_ids: ResMut<VisibleEntityIds>,
    mut state: Local<VisibleEntitySyncState>,
) {
    let state = &mut *state;
    let sim_replacement_revision = sim.replacement_revision();
    let sim = sim.read();
    let entity_topology_revision = sim.entity_topology_revision();
    let entity_style_revision = sim.entity_style_revision();
    let sim_replaced =
        !state.initialized || state.sim_replacement_revision != sim_replacement_revision;
    let visibility_changed = !state.initialized || state.visible_revision != visible.revision;
    let topology_changed =
        !state.initialized || state.entity_topology_revision != entity_topology_revision;
    let style_changed = !state.initialized || state.entity_style_revision != entity_style_revision;
    if !sim_replaced && !visibility_changed && !topology_changed && !style_changed {
        return;
    }

    state.added.clear();
    state.removed.clear();
    state.style_dirty.clear();
    let mut reset = false;

    if sim_replaced || visibility_changed {
        collect_visible_entity_ids(&sim, &visible, &mut state.next_ids);
        if sim_replaced {
            state.removed.extend(visible_entity_ids.ids.iter().copied());
            state.added.extend(state.next_ids.iter().copied());
            reset = true;
        } else {
            state
                .added
                .extend(state.next_ids.difference(&visible_entity_ids.ids).copied());
            state
                .removed
                .extend(visible_entity_ids.ids.difference(&state.next_ids).copied());
        }
    } else {
        state.next_ids.clone_from(&visible_entity_ids.ids);
    }

    if !sim_replaced && topology_changed {
        state.affected_ids.clear();
        if collect_topology_affected_ids(
            &sim,
            state.entity_topology_revision,
            &mut state.affected_ids,
        ) {
            for &entity_id in &state.affected_ids {
                let was_visible = visible_entity_ids.ids.contains(&entity_id);
                let is_visible = sim
                    .entities()
                    .placed_entity(entity_id)
                    .is_some_and(|placed| footprint_is_visible(&placed.footprint, &visible));
                if visibility_changed {
                    if was_visible && state.next_ids.contains(&entity_id) {
                        state.style_dirty.push(entity_id);
                    }
                    continue;
                }
                match (was_visible, is_visible) {
                    (false, true) => {
                        state.next_ids.insert(entity_id);
                        state.added.push(entity_id);
                    }
                    (true, false) => {
                        state.next_ids.remove(&entity_id);
                        state.removed.push(entity_id);
                    }
                    (true, true) => state.style_dirty.push(entity_id),
                    (false, false) => {}
                }
            }
        } else {
            // A deferred consumer fell behind retained history. Recover with a
            // spatial membership query and restyle only the still-visible set.
            if !visibility_changed {
                collect_visible_entity_ids(&sim, &visible, &mut state.next_ids);
                state
                    .added
                    .extend(state.next_ids.difference(&visible_entity_ids.ids).copied());
                state
                    .removed
                    .extend(visible_entity_ids.ids.difference(&state.next_ids).copied());
            }
            state.style_dirty.extend(
                state
                    .next_ids
                    .intersection(&visible_entity_ids.ids)
                    .copied(),
            );
        }
    }

    if !sim_replaced && style_changed {
        if let Some(changed_ids) = sim.entity_style_changes_since(state.entity_style_revision) {
            for entity_id in changed_ids {
                if visible_entity_ids.ids.contains(&entity_id)
                    && state.next_ids.contains(&entity_id)
                {
                    state.style_dirty.push(entity_id);
                }
            }
        } else {
            state
                .style_dirty
                .extend(visible_entity_ids.ids.iter().copied());
        }
    }
    state.style_dirty.sort_unstable();
    state.style_dirty.dedup();

    state.initialized = true;
    state.visible_revision = visible.revision;
    state.entity_topology_revision = entity_topology_revision;
    state.entity_style_revision = entity_style_revision;
    state.sim_replacement_revision = sim_replacement_revision;

    if reset
        || !state.added.is_empty()
        || !state.removed.is_empty()
        || !state.style_dirty.is_empty()
    {
        let membership_changed = reset || !state.added.is_empty() || !state.removed.is_empty();
        visible_entity_ids.ids.clone_from(&state.next_ids);
        visible_entity_ids.added.clone_from(&state.added);
        visible_entity_ids.removed.clone_from(&state.removed);
        visible_entity_ids
            .style_dirty
            .clone_from(&state.style_dirty);
        if membership_changed {
            visible_entity_ids.membership_revision =
                visible_entity_ids.membership_revision.wrapping_add(1);
        }
        visible_entity_ids.reset = reset;
        visible_entity_ids.visible_revision = visible.revision;
        visible_entity_ids.entity_topology_revision = entity_topology_revision;
    }
}

fn collect_topology_affected_ids(
    sim: &Simulation,
    revision: u64,
    affected_ids: &mut HashSet<EntityId>,
) -> bool {
    let Some(changes) = sim.entity_visual_changes_since(revision) else {
        return false;
    };
    for change in changes {
        affected_ids.insert(change.entity_id);
        let min_x = change.min_x.saturating_sub(1);
        let max_x = change.max_x.saturating_add(1);
        let min_y = change.min_y.saturating_sub(1);
        let max_y = change.max_y.saturating_add(1);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if let Some(entity_id) = sim.entities().occupancy().entity_at(x, y) {
                    affected_ids.insert(entity_id);
                }
            }
        }
    }
    true
}

pub(crate) fn sync_placed_entity_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible_entity_ids: Res<VisibleEntityIds>,
    mut visual_assets: VisualAssets,
    mut registry: Local<PlacedEntityRenderRegistry>,
    mut queries: PlacedEntityRenderQueries,
) {
    let visible_changed = visible_entity_ids.is_changed();
    if !visible_changed && registry.pending_signals.is_empty() {
        return;
    }
    let sim = sim.read();
    if !visible_changed {
        retry_pending_rail_signals(&sim, &mut registry, &mut visual_assets, &mut queries);
        return;
    }

    if visible_entity_ids.reset {
        for (_, render_entity) in registry.entities.drain() {
            commands.entity(render_entity).despawn();
        }
        registry.pending_signals.clear();
    }

    for entity_id in &visible_entity_ids.removed {
        if let Some(render_entity) = registry.entities.remove(entity_id) {
            commands.entity(render_entity).despawn();
        }
        registry.pending_signals.remove(entity_id);
    }

    for entity_id in &visible_entity_ids.style_dirty {
        let Some(&render_entity) = registry.entities.get(entity_id) else {
            continue;
        };
        let Some(placed) = sim.entities().placed_entity(*entity_id) else {
            continue;
        };
        let Some(style) = renderable_entity_visual_style(&sim, *entity_id) else {
            continue;
        };
        // A dirty rail graph has no authoritative aspect yet. Keep the last
        // rendered lamp and symbol: rebuilding the graph may reproduce the
        // same aspect and emit no further style revision.
        if style.kind.is_rail_signal() && sim.rail_signal_aspect(*entity_id).is_none() {
            registry.pending_signals.insert(*entity_id);
            continue;
        }
        let aspect = sim.rail_signal_aspect(*entity_id);
        if refresh_placed_entity_sprite(
            render_entity,
            &placed.footprint,
            style,
            aspect,
            &mut visual_assets,
            &mut queries,
        ) {
            registry.pending_signals.remove(entity_id);
        } else if style.kind.is_rail_signal() {
            registry.pending_signals.insert(*entity_id);
        }
    }

    for &entity_id in &visible_entity_ids.added {
        if registry.entities.contains_key(&entity_id) {
            continue;
        }
        let Some(placed) = sim.entities().placed_entity(entity_id) else {
            continue;
        };
        let Some(style) = renderable_entity_visual_style(&sim, placed.id) else {
            continue;
        };
        let render_entity = spawn_entity_visual(
            &mut commands,
            &mut visual_assets,
            style,
            entity_translation(&placed.footprint, 3.0),
            PlacedEntitySprite {
                entity_id: placed.id,
            },
        );
        if style.kind == EntityKind::RocketSilo
            && let Ok(state) = factory_sim::entity_access::rocket_silo_state(&sim, placed.id)
            && let Some(status) = sim.rocket_silo_status_for_entity(placed.id)
        {
            let indicator = spawn_rocket_silo_status_indicator(
                &mut commands,
                status.state,
                placed.footprint.height,
            );
            commands.entity(render_entity).add_child(indicator);
            commands.entity(render_entity).insert(RocketSiloSprite {
                visual_phase: state.launch_phase.into(),
                status_indicator: indicator,
            });
        }
        if style.kind.is_rail_signal() {
            let aspect = sim.rail_signal_aspect(entity_id);
            let indicator = spawn_rail_signal_status_indicator(&mut commands, aspect);
            commands.entity(render_entity).add_child(indicator);
            commands.entity(render_entity).insert(RailSignalSprite {
                status_indicator: indicator,
            });
            if aspect.is_none() {
                registry.pending_signals.insert(entity_id);
            }
        }
        registry.entities.insert(entity_id, render_entity);
    }
    retry_pending_rail_signals(&sim, &mut registry, &mut visual_assets, &mut queries);
}

/// An unchanged rebuilt aspect emits no style revision. Retry deferred signals
/// independently of VisibleEntityIds so camera reveals and topology edits
/// cannot strand a neutral lamp or an old direction/symbol.
fn retry_pending_rail_signals(
    sim: &Simulation,
    registry: &mut PlacedEntityRenderRegistry,
    visual_assets: &mut VisualAssets,
    queries: &mut PlacedEntityRenderQueries,
) {
    let PlacedEntityRenderRegistry {
        entities,
        pending_signals,
    } = registry;
    pending_signals.retain(|entity_id| {
        let Some(&render_entity) = entities.get(entity_id) else {
            return false;
        };
        let Some(aspect) = sim.rail_signal_aspect(*entity_id) else {
            return true;
        };
        let Some(placed) = sim.entities().placed_entity(*entity_id) else {
            return false;
        };
        let Some(style) = renderable_entity_visual_style(sim, *entity_id) else {
            return false;
        };
        !refresh_placed_entity_sprite(
            render_entity,
            &placed.footprint,
            style,
            Some(aspect),
            visual_assets,
            queries,
        )
    });
}

fn refresh_placed_entity_sprite(
    render_entity: Entity,
    footprint: &EntityFootprint,
    style: EntityVisualStyle,
    aspect: Option<RailSignalAspect>,
    visual_assets: &mut VisualAssets,
    queries: &mut PlacedEntityRenderQueries,
) -> bool {
    let Ok((mut transform, mut sprite, signal)) = queries.sprites.get_mut(render_entity) else {
        return false;
    };
    let translation = entity_translation(footprint, transform.translation.z);
    if transform.translation != translation {
        transform.translation = translation;
    }
    *sprite = visual_assets.entity_sprite(style);
    if let Some(signal) = signal
        && let Some(aspect) = aspect
        && let Ok(mut text) = queries.signal_indicators.get_mut(signal.status_indicator)
    {
        text.0 = rail_signal_world_status_symbol(aspect).to_string();
    }
    true
}

/// Refreshes visible rocket silos when their fixed-tick launch phase changes.
///
/// Visibility and entity topology remain stable during a launch, so the general
/// placed-entity sync intentionally stays asleep. Keeping the last rendered
/// phase on just the silo sprites makes this path proportional to the number of
/// visible silos and avoids rebuilding their cached visual on unchanged frames.
pub(crate) fn sync_rocket_silo_rendering(
    sim: Res<SimResource>,
    mut sync_state: Local<RocketSiloRenderSyncState>,
    mut visual_assets: VisualAssets,
    mut sprites: Query<(&PlacedEntitySprite, &mut RocketSiloSprite, &mut Sprite)>,
    mut indicators: Query<
        (&mut Text2d, &mut TextColor, &mut RocketSiloStatusIndicator),
        Without<RocketSiloSprite>,
    >,
) {
    let replacement_revision = sim.replacement_revision();
    let sim = sim.read();
    let tick = sim.tick_count();
    if sync_state.initialized
        && sync_state.synced_tick == tick
        && sync_state.sim_replacement_revision == replacement_revision
    {
        return;
    }
    sync_state.initialized = true;
    sync_state.synced_tick = tick;
    sync_state.sim_replacement_revision = replacement_revision;
    for (placed, mut rendered, mut sprite) in &mut sprites {
        let Ok(state) = factory_sim::entity_access::rocket_silo_state(&sim, placed.entity_id)
        else {
            continue;
        };
        if let Some(status) = sim.rocket_silo_status_for_entity(placed.entity_id)
            && let Ok((mut text, mut color, mut indicator)) =
                indicators.get_mut(rendered.status_indicator)
            && indicator.operational_state != status.state
        {
            text.0 = rocket_silo_world_status_label(status.state).to_string();
            color.0 = rocket_silo_world_status_color(status.state);
            indicator.operational_state = status.state;
        }
        let visual_phase = state.launch_phase.into();
        if visual_phase == rendered.visual_phase {
            continue;
        }
        let Some(style) = renderable_entity_visual_style(&sim, placed.entity_id) else {
            continue;
        };

        *sprite = visual_assets.entity_sprite(style);
        rendered.visual_phase = visual_phase;
    }
}

fn spawn_rocket_silo_status_indicator(
    commands: &mut Commands,
    operational_state: factory_sim::RocketSiloOperationalState,
    footprint_height: i32,
) -> Entity {
    commands
        .spawn((
            Text2d::new(rocket_silo_world_status_label(operational_state)),
            TextFont::from_font_size(6.0),
            TextColor(rocket_silo_world_status_color(operational_state)),
            TextLayout::justify(Justify::Center),
            Transform::from_xyz(0.0, footprint_height as f32 * TILE_SIZE * 0.5 + 7.0, 0.2),
            Anchor::CENTER,
            Text2dShadow::default(),
            ReadableWorldLabel::new(6.0),
            RocketSiloStatusIndicator { operational_state },
        ))
        .id()
}

/// A fixed, color-independent mark above the signal head. ASCII glyphs are
/// supported by the default world font and stay distinct at small zoom levels.
pub(crate) const fn rail_signal_world_status_symbol(aspect: RailSignalAspect) -> &'static str {
    match aspect {
        RailSignalAspect::Clear => ">",
        RailSignalAspect::Reserved => "=",
        RailSignalAspect::Blocked => "X",
    }
}

fn spawn_rail_signal_status_indicator(
    commands: &mut Commands,
    aspect: Option<RailSignalAspect>,
) -> Entity {
    commands
        .spawn((
            Text2d::new(aspect.map_or("", rail_signal_world_status_symbol)),
            TextFont::from_font_size(6.0),
            TextColor(Color::WHITE),
            TextLayout::justify(Justify::Center),
            Transform::from_xyz(0.0, TILE_SIZE * 0.5 + 4.0, 0.2),
            Anchor::CENTER,
            Text2dShadow::default(),
            ReadableWorldLabel::new(6.0),
            RailSignalStatusIndicator,
        ))
        .id()
}

pub(crate) const fn rocket_silo_world_status_label(
    status: factory_sim::RocketSiloOperationalState,
) -> &'static str {
    use factory_sim::RocketSiloOperationalState as State;
    match status {
        State::RecipeLocked => "Recipe locked",
        State::BuildingParts => "Building parts",
        State::MissingIngredients => "Missing ingredients",
        State::NoPower => "No power",
        State::AwaitingPayload => "Awaiting payload",
        State::ReadyToLaunch => "Ready to launch",
        State::Sealing => "Sealing",
        State::Launching => "Launching",
        State::LaunchOutputBlocked => "Launch output blocked",
    }
}

fn rocket_silo_world_status_color(status: factory_sim::RocketSiloOperationalState) -> Color {
    use factory_sim::RocketSiloOperationalState as State;
    match status {
        State::RecipeLocked => Color::srgb(0.72, 0.74, 0.72),
        State::BuildingParts | State::ReadyToLaunch => Color::srgb(0.42, 0.84, 0.55),
        State::MissingIngredients | State::AwaitingPayload => Color::srgb(1.0, 0.72, 0.30),
        State::NoPower => Color::srgb(1.0, 0.30, 0.24),
        State::Sealing => Color::srgb(0.45, 0.72, 1.0),
        State::Launching => Color::srgb(1.0, 0.52, 0.20),
        State::LaunchOutputBlocked => Color::srgb(1.0, 0.40, 0.20),
    }
}

pub(crate) fn measured_sync_placed_entity_rendering(
    commands: Commands,
    sim: Res<SimResource>,
    visible_entity_ids: Res<VisibleEntityIds>,
    visual_assets: VisualAssets,
    registry: Local<PlacedEntityRenderRegistry>,
    queries: PlacedEntityRenderQueries,
    mut timing: ResMut<PlacedEntitiesRenderSyncTime>,
) {
    let started = Instant::now();
    sync_placed_entity_rendering(
        commands,
        sim,
        visible_entity_ids,
        visual_assets,
        registry,
        queries,
    );
    timing.0 = started.elapsed();
}

fn collect_visible_entity_ids(
    sim: &Simulation,
    visible: &VisibleChunks,
    ids: &mut HashSet<EntityId>,
) {
    ids.clear();
    let Some(bounds) = visible.tile_bounds else {
        return;
    };
    let max_x = bounds.min_x + i64::from(bounds.width) - 1;
    let max_y = bounds.min_y + i64::from(bounds.height) - 1;
    sim.entities().occupancy().for_each_entity_id_in_tile_rect(
        bounds.min_x,
        max_x,
        bounds.min_y,
        max_y,
        |entity_id| {
            ids.insert(entity_id);
        },
    );
}

fn footprint_is_visible(footprint: &EntityFootprint, visible: &VisibleChunks) -> bool {
    let min_x = footprint.x.div_euclid(i64::from(factory_sim::CHUNK_SIZE));
    let max_x = (footprint.x + i64::from(footprint.width) - 1)
        .div_euclid(i64::from(factory_sim::CHUNK_SIZE));
    let min_y = footprint.y.div_euclid(i64::from(factory_sim::CHUNK_SIZE));
    let max_y = (footprint.y + i64::from(footprint.height) - 1)
        .div_euclid(i64::from(factory_sim::CHUNK_SIZE));
    (min_y..=max_y).any(|y| {
        (min_x..=max_x).any(|x| {
            let (Ok(x), Ok(y)) = (i32::try_from(x), i32::try_from(y)) else {
                return false;
            };
            visible.chunks.contains(&factory_sim::ChunkCoord { x, y })
        })
    })
}

pub(crate) fn renderable_entity_visual_style(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<EntityVisualStyle> {
    let placed = sim.entities().placed_entity(entity_id)?;
    let mut style =
        entity_prototype_visual_style(sim.catalog(), placed.prototype_id, placed.direction)?;
    style.connections = entity_connection_mask(sim, placed, style.kind);
    if style.kind == EntityKind::Lamp
        && let Some(lit) = factory_sim::entity_access::lamp_is_lit(sim, entity_id)
    {
        style.base_color = lamp_color(lit);
    }
    if style.kind == EntityKind::RocketSilo
        && let Ok(state) = factory_sim::entity_access::rocket_silo_state(sim, entity_id)
    {
        style.rocket_silo_phase = state.launch_phase.into();
        style.base_color = match state.launch_phase {
            factory_sim::RocketLaunchPhase::Idle => rocket_silo_color(),
            // Closed doors darken the pad; the rising phase becomes the warm
            // exhaust colour. Both are direct projections of simulation state.
            factory_sim::RocketLaunchPhase::Sealed { .. } => Color::srgb(0.38, 0.40, 0.43),
            factory_sim::RocketLaunchPhase::Rising { .. } => Color::srgb(0.95, 0.48, 0.12),
        };
    }
    if style.kind.is_rail_signal() {
        style.base_color = sim
            .rail_signal_aspect(entity_id)
            .map(rail_signal_color)
            .unwrap_or(Color::srgb(0.30, 0.32, 0.34));
    }
    Some(style)
}

/// Directions in which the placed entity visually joins a neighbor. Only pipes and belts
/// render connection overlays; every other kind keeps an empty mask so its cached visual
/// is shared across placements.
fn entity_connection_mask(
    sim: &Simulation,
    placed: &PlacedEntity,
    kind: EntityKind,
) -> ConnectionMask {
    match kind {
        EntityKind::Pipe => ConnectionMask::from_directions(
            factory_sim::entity_access::fluid_connection_directions(sim, placed.id),
        ),
        EntityKind::HeatPipe => ConnectionMask::from_directions(
            factory_sim::entity_access::heat_connection_directions(sim, placed.id),
        ),
        EntityKind::TransportBelt => belt_connection_mask(sim, placed),
        _ => ConnectionMask::EMPTY,
    }
}

fn belt_connection_mask(sim: &Simulation, placed: &PlacedEntity) -> ConnectionMask {
    let flow = belt_flow_direction(sim, placed);
    let mut connected = [false; 4];

    for direction in Direction::ALL {
        let (dx, dy) = direction_tile_delta(direction);
        let Some(neighbor_id) = sim
            .entities()
            .occupancy()
            .entity_at(placed.footprint.x + dx, placed.footprint.y + dy)
        else {
            continue;
        };
        let Some(neighbor) = sim.entities().placed_entity(neighbor_id) else {
            continue;
        };
        let Some(prototype) = sim.catalog().entity(neighbor.prototype_id) else {
            continue;
        };
        if !matches!(
            prototype.entity_kind,
            EntityKind::TransportBelt | EntityKind::Splitter
        ) {
            continue;
        }

        let neighbor_flow = belt_flow_direction(sim, neighbor);
        connected[direction.index()] = if direction == flow {
            // Downstream edge: joined unless the neighbor faces us head-on.
            neighbor_flow != direction.opposite()
        } else {
            // Upstream or side edge: joined when the neighbor flows into this tile.
            neighbor_flow == direction.opposite()
        };
    }

    ConnectionMask::from_directions(connected)
}

fn belt_flow_direction(sim: &Simulation, placed: &PlacedEntity) -> Direction {
    factory_sim::entity_access::belt_segment(sim, placed.id)
        .map(|segment| segment.dir)
        .unwrap_or(placed.direction)
}

fn direction_tile_delta(direction: Direction) -> (i64, i64) {
    match direction {
        Direction::North => (0, 1),
        Direction::East => (1, 0),
        Direction::South => (0, -1),
        Direction::West => (-1, 0),
    }
}

pub(crate) fn entity_prototype_render_style(
    catalog: &PrototypeCatalog,
    prototype_id: EntityPrototypeId,
    direction: Direction,
) -> Option<(Color, Vec2)> {
    let style = entity_prototype_visual_style(catalog, prototype_id, direction)?;
    Some((style.base_color, style.size))
}

pub(crate) fn entity_prototype_visual_style(
    catalog: &PrototypeCatalog,
    prototype_id: EntityPrototypeId,
    direction: Direction,
) -> Option<EntityVisualStyle> {
    let prototype = catalog.entity(prototype_id)?;
    let footprint = EntityFootprint::from_size(0, 0, prototype.size.x, prototype.size.y, direction);
    let machine_size = || {
        Vec2::new(
            footprint.width as f32 * TILE_SIZE - MINING_DRILL_SPRITE_PADDING,
            footprint.height as f32 * TILE_SIZE - MINING_DRILL_SPRITE_PADDING,
        )
    };

    match prototype.entity_kind {
        EntityKind::TransportBelt => Some(entity_visual_style(
            transport_belt_color(
                prototype
                    .transport_belt
                    .as_ref()
                    .map(|belt| belt.speed_subtiles_per_tick),
            ),
            Vec2::splat(TRANSPORT_BELT_SPRITE_SIZE),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Splitter => Some(entity_visual_style(
            splitter_color(
                prototype
                    .splitter
                    .as_ref()
                    .map(|splitter| splitter.speed_subtiles_per_tick),
            ),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Chest => Some(entity_visual_style(
            chest_color(&prototype.name),
            Vec2::splat(CHEST_SPRITE_SIZE),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::MiningDrill => Some(entity_visual_style(
            mining_drill_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Furnace => Some(entity_visual_style(
            furnace_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::RocketSilo => Some(entity_visual_style(
            rocket_silo_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::AssemblingMachine => Some(entity_visual_style(
            match prototype
                .assembling_machine
                .as_ref()
                .map(|assembling_machine| assembling_machine.crafting_category)
            {
                Some(CraftingCategory::OilProcessing) => oil_refinery_color(),
                Some(CraftingCategory::Chemistry) => chemical_plant_color(),
                Some(CraftingCategory::Centrifuging) => centrifuge_color(),
                _ => assembler_color(),
            },
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Lab => Some(entity_visual_style(
            lab_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Beacon => Some(entity_visual_style(
            beacon_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Inserter => Some(entity_visual_style(
            inserter_color(prototype.inserter.as_ref(), prototype.burner.is_some()),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::ElectricPole => Some(entity_visual_style(
            electric_pole_color(),
            Vec2::splat(CHEST_SPRITE_SIZE),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::SteamEngine => Some(entity_visual_style(
            steam_engine_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Boiler => Some(entity_visual_style(
            boiler_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::OffshorePump => Some(entity_visual_style(
            offshore_pump_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Pump => Some(entity_visual_style(
            pump_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Pumpjack => Some(entity_visual_style(
            pumpjack_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Pipe => Some(entity_visual_style(
            pipe_color(),
            Vec2::splat(TRANSPORT_BELT_SPRITE_SIZE),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::StorageTank => Some(entity_visual_style(
            storage_tank_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Wall => Some(entity_visual_style(
            wall_color(),
            Vec2::splat(CHEST_SPRITE_SIZE),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::GunTurret => Some(entity_visual_style(
            gun_turret_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::LaserTurret => Some(entity_visual_style(
            laser_turret_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::EnemySpawner => Some(entity_visual_style(
            enemy_spawner_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::NuclearReactor => Some(entity_visual_style(
            nuclear_reactor_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::HeatPipe => Some(entity_visual_style(
            heat_pipe_color(),
            Vec2::splat(TRANSPORT_BELT_SPRITE_SIZE),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::HeatExchanger => Some(entity_visual_style(
            heat_exchanger_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::SolarPanel => Some(entity_visual_style(
            solar_panel_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Accumulator => Some(entity_visual_style(
            accumulator_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Radar => Some(entity_visual_style(
            radar_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::Roboport => Some(entity_visual_style(
            roboport_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::ConstantCombinator => Some(entity_visual_style(
            constant_combinator_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::ArithmeticCombinator => Some(entity_visual_style(
            arithmetic_combinator_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        EntityKind::DeciderCombinator => Some(entity_visual_style(
            decider_combinator_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        // Placeholders and previews have no simulated lamp to read, so the
        // prototype-only style shows the unlit body;
        // `renderable_entity_visual_style` swaps in the live state.
        EntityKind::Lamp => Some(entity_visual_style(
            lamp_color(false),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        // Track fills its footprint exactly — no sprite padding — because the
        // curve is drawn in the same sub-tile coordinates the simulation uses,
        // and a shrunken sprite would move the rails off their own path.
        EntityKind::RailStraight | EntityKind::RailCurved => Some(EntityVisualStyle {
            base_color: rail_ballast_color(),
            size: Vec2::new(
                footprint.width as f32 * TILE_SIZE,
                footprint.height as f32 * TILE_SIZE,
            ),
            kind: prototype.entity_kind,
            direction,
            connections: ConnectionMask::EMPTY,
            rail: factory_sim::rail_ops::piece_geometry(prototype, direction),
            rocket_silo_phase: RocketSiloVisualPhase::Idle,
        }),
        // A signal shows its aspect, so the prototype-only style — a preview, a
        // ghost, a build-menu icon — shows the clear one and
        // `renderable_entity_visual_style` swaps in the live aspect, the same
        // way a lamp's lit state is handled.
        EntityKind::RailSignal | EntityKind::ChainSignal => Some(entity_visual_style(
            rail_signal_color(RailSignalAspect::Clear),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        // A stop is a sign beside the track, drawn like the signal it stands
        // near: one tile, its own colour, and the direction it was dropped in.
        EntityKind::TrainStop => Some(entity_visual_style(
            train_stop_color(),
            machine_size(),
            prototype.entity_kind,
            direction,
        )),
        // Rolling stock is never a placed entity, so the placed-entity renderer
        // never sees one: it is drawn along the track it stands on by
        // [`crate::rendering::rolling_stock`], which is the only renderer that
        // can put a body on a curve.
        EntityKind::Locomotive | EntityKind::CargoWagon | EntityKind::FluidWagon => None,
        EntityKind::ResourcePatch => None,
    }
}

fn entity_visual_style(
    base_color: Color,
    size: Vec2,
    kind: EntityKind,
    direction: Direction,
) -> EntityVisualStyle {
    EntityVisualStyle {
        base_color,
        size,
        kind,
        direction,
        connections: ConnectionMask::EMPTY,
        rail: None,
        rocket_silo_phase: RocketSiloVisualPhase::Idle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::resources::{MapTextureBounds, VisibleChunks};
    use crate::rendering::resources::VisibleEntityIds;
    use crate::resources::SimResource;
    use factory_sim::CHUNK_SIZE;
    use std::collections::BTreeSet;

    #[test]
    fn fluid_entities_have_render_styles() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

        for entity_name in ["pipe", "storage_tank"] {
            let prototype_id = factory_data::entity_prototype_id_by_name(&catalog, entity_name);
            assert!(
                entity_prototype_render_style(&catalog, prototype_id, Direction::North).is_some(),
                "{entity_name} should have a render style"
            );
        }
    }

    #[test]
    fn radar_has_a_render_style() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let prototype_id = factory_data::entity_prototype_id_by_name(&catalog, "radar");
        assert!(entity_prototype_render_style(&catalog, prototype_id, Direction::North).is_some());
    }

    /// The style is hashed into the sprite cache key, so a rail's geometry in it
    /// has to be the prototype-local one. World coordinates would give every
    /// placement of the same piece its own cached texture, which is why this
    /// pins the frame rather than trusting the call site to keep picking it.
    #[test]
    fn rail_visual_geometry_stays_in_the_prototype_local_frame() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

        for entity_name in ["rail_straight", "rail_curved"] {
            let prototype_id = factory_data::entity_prototype_id_by_name(&catalog, entity_name);
            let prototype = catalog
                .entity(prototype_id)
                .expect("the base catalog defines both rail pieces");

            for direction in Direction::ALL {
                let style = entity_prototype_visual_style(&catalog, prototype_id, direction)
                    .unwrap_or_else(|| panic!("{entity_name} should have a visual style"));
                let geometry = style
                    .rail
                    .unwrap_or_else(|| panic!("{entity_name} should carry its travel geometry"));
                let footprint =
                    EntityFootprint::from_size(0, 0, prototype.size.x, prototype.size.y, direction);
                let width = i64::from(footprint.width) * factory_sim::POSITION_SCALE;
                let height = i64::from(footprint.height) * factory_sim::POSITION_SCALE;

                for end in geometry.ends() {
                    assert!(
                        (0..=width).contains(&end.position.x)
                            && (0..=height).contains(&end.position.y),
                        "{entity_name} facing {direction:?} left its own footprint: {:?}",
                        end.position
                    );
                }
            }
        }
    }

    #[test]
    fn topology_delta_dirties_connected_neighbor_without_refreshing_view() {
        let mut sim = Simulation::new_test_world(123);
        let pipe = factory_data::entity_prototype_id_by_name(sim.catalog(), "pipe");
        let (x, y) = sim
            .world()
            .chunks
            .values()
            .flat_map(|chunk| {
                (0..CHUNK_SIZE).flat_map(move |local_y| {
                    (0..CHUNK_SIZE).map(move |local_x| chunk.coord.tile_at(local_x, local_y))
                })
            })
            .find(|&(x, y)| {
                factory_sim::ChunkCoord::from_tile(x, y)
                    == factory_sim::ChunkCoord::from_tile(x + 1, y)
                    && factory_sim::ChunkCoord::from_tile(x, y)
                        == factory_sim::ChunkCoord::from_tile(x + 2, y)
                    && factory_sim::placement::validate(
                        &sim,
                        factory_sim::placement::EntityPlacementRequest {
                            prototype_id: pipe,
                            x,
                            y,
                            direction: Direction::North,
                        },
                    )
                    .is_ok()
                    && factory_sim::placement::validate(
                        &sim,
                        factory_sim::placement::EntityPlacementRequest {
                            prototype_id: pipe,
                            x: x + 2,
                            y,
                            direction: Direction::North,
                        },
                    )
                    .is_ok()
                    && factory_sim::placement::validate(
                        &sim,
                        factory_sim::placement::EntityPlacementRequest {
                            prototype_id: pipe,
                            x: x + 1,
                            y,
                            direction: Direction::North,
                        },
                    )
                    .is_ok()
            })
            .expect("test world should have adjacent pipe tiles");
        let first = factory_sim::placement::place(
            &mut sim,
            factory_sim::placement::EntityPlacementRequest {
                prototype_id: pipe,
                x,
                y,
                direction: Direction::North,
            },
        )
        .expect("first pipe should place");
        let chunk = factory_sim::ChunkCoord::from_tile(x, y).expect("tile should form chunk");
        let (min_x, min_y) = chunk.min_tile();
        let visible = VisibleChunks {
            chunks: BTreeSet::from([chunk]),
            tile_bounds: Some(MapTextureBounds {
                min_x,
                min_y,
                width: CHUNK_SIZE as u32,
                height: CHUNK_SIZE as u32,
            }),
            revision: 1,
        };
        let mut app = App::new();
        app.insert_resource(SimResource::new(sim))
            .insert_resource(visible)
            .init_resource::<VisibleEntityIds>()
            .add_systems(Update, update_visible_entity_ids);
        app.update();

        let second = {
            let mut resource = app.world_mut().resource_mut::<SimResource>();
            factory_sim::placement::place(
                &mut resource.write_for_tests(),
                factory_sim::placement::EntityPlacementRequest {
                    prototype_id: pipe,
                    x: x + 1,
                    y,
                    direction: Direction::North,
                },
            )
            .expect("connected pipe should place")
        };
        app.update();

        let visible_ids = app.world().resource::<VisibleEntityIds>();
        assert_eq!(visible_ids.added, vec![second]);
        assert!(visible_ids.style_dirty.contains(&first));
        assert!(visible_ids.ids.contains(&first));
        assert!(visible_ids.ids.contains(&second));

        let third = {
            let mut resource = app.world_mut().resource_mut::<SimResource>();
            factory_sim::placement::place(
                &mut resource.write_for_tests(),
                factory_sim::placement::EntityPlacementRequest {
                    prototype_id: pipe,
                    x: x + 2,
                    y,
                    direction: Direction::North,
                },
            )
            .expect("third connected pipe should place")
        };
        app.world_mut().resource_mut::<VisibleChunks>().revision += 1;
        app.update();

        let visible_ids = app.world().resource::<VisibleEntityIds>();
        assert_eq!(visible_ids.added, vec![third]);
        assert!(
            visible_ids.style_dirty.contains(&second),
            "a simultaneous camera revision must not consume the neighbor invalidation"
        );
        assert!(visible_ids.ids.contains(&third));
    }
}
