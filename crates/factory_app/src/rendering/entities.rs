use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};
use factory_data::{CraftingCategory, EntityKind, EntityPrototypeId, PrototypeCatalog};
use factory_sim::{
    Direction, EntityFootprint, EntityId, PlacedEntity, RailSignalAspect, Simulation,
};
use std::collections::HashSet;
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
use crate::rendering::resources::{RenderSyncStats, VisibleEntityIds};
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
) {
    let entity_topology_revision = sim.read().entity_topology_revision();
    if visible_entity_ids.visible_revision == visible.revision
        && visible_entity_ids.entity_topology_revision == entity_topology_revision
    {
        return;
    }

    visible_entity_ids.ids = visible_entity_ids_for_chunks(&sim.read(), &visible);
    visible_entity_ids.visible_revision = visible.revision;
    visible_entity_ids.entity_topology_revision = entity_topology_revision;
}

pub(crate) fn sync_placed_entity_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible_entity_ids: Res<VisibleEntityIds>,
    mut visual_assets: VisualAssets,
    mut sprites: Query<(Entity, &PlacedEntitySprite, &mut Transform, &mut Sprite)>,
) {
    let sim = sim.read();
    if !visible_entity_ids.is_changed() {
        return;
    }

    let visible_ids = &visible_entity_ids.ids;
    let mut seen = HashSet::new();

    for (entity, marker, mut transform, mut sprite) in &mut sprites {
        if visible_ids.contains(&marker.entity_id)
            && let Some(style) = renderable_entity_visual_style(&sim, marker.entity_id)
        {
            let placed = sim
                .entities()
                .placed_entity(marker.entity_id)
                .expect("validated renderable entity should still be placed");
            seen.insert(marker.entity_id);
            transform.translation = entity_translation(&placed.footprint, transform.translation.z);
            *sprite = visual_assets.entity_sprite(style);
        } else {
            commands.entity(entity).despawn();
        }
    }

    for &entity_id in visible_ids {
        let Some(placed) = sim.entities().placed_entity(entity_id) else {
            continue;
        };
        let Some(style) = renderable_entity_visual_style(&sim, placed.id) else {
            continue;
        };
        if seen.contains(&placed.id) {
            continue;
        }

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
    }
}

/// Refreshes visible rocket silos when their fixed-tick launch phase changes.
///
/// Visibility and entity topology remain stable during a launch, so the general
/// placed-entity sync intentionally stays asleep. Keeping the last rendered
/// phase on just the silo sprites makes this path proportional to the number of
/// visible silos and avoids rebuilding their cached visual on unchanged frames.
pub(crate) fn sync_rocket_silo_rendering(
    sim: Res<SimResource>,
    mut visual_assets: VisualAssets,
    mut sprites: Query<(&PlacedEntitySprite, &mut RocketSiloSprite, &mut Sprite)>,
    mut indicators: Query<
        (&mut Text2d, &mut TextColor, &mut RocketSiloStatusIndicator),
        Without<RocketSiloSprite>,
    >,
) {
    let sim = sim.read();
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
    sprites: Query<(Entity, &PlacedEntitySprite, &mut Transform, &mut Sprite)>,
    mut stats: ResMut<RenderSyncStats>,
) {
    let started = Instant::now();
    sync_placed_entity_rendering(commands, sim, visible_entity_ids, visual_assets, sprites);
    stats.record_placed_entities(started.elapsed());
}

fn visible_entity_ids_for_chunks(sim: &Simulation, visible: &VisibleChunks) -> HashSet<EntityId> {
    let Some(bounds) = visible.tile_bounds else {
        return HashSet::new();
    };
    let max_x = bounds.min_x + i64::from(bounds.width) - 1;
    let max_y = bounds.min_y + i64::from(bounds.height) - 1;
    sim.entities()
        .occupancy()
        .entity_ids_in_tile_rect(bounds.min_x, max_x, bounds.min_y, max_y)
        .into_iter()
        .collect()
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
    if style.kind.is_rail_signal()
        && let Some(aspect) = sim.rail_signal_aspect(entity_id)
    {
        style.base_color = rail_signal_color(aspect);
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
}
