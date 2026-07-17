use bevy::prelude::*;
use factory_data::EntityKind;
use factory_sim::{
    CHUNK_SIZE, ChunkCoord, EntityFootprint, MachineStatus, Simulation, ThreatLocation,
};

use crate::map::resources::{
    MapDetailCache, MapDisplaySettings, MapOverlay, MapOverlayLayer, MapOverlayMarkers,
    MapTextureBounds,
};
use crate::rendering::entities::entity_prototype_render_style;

use super::components::{
    FullMapImage, FullMapOverlayButton, FullMapOverlayRoot, FullMapRecenterButton,
    FullMapResourceImage, FullMapRoot, MinimapImage, MinimapOverlayRoot, MinimapResourceImage,
    MinimapRoot,
};
use super::layout::{
    MapTileRect, MapUiRect, map_point_for_world_position, map_rect_for_chunk,
    map_rect_for_footprint, map_rect_for_world_rect,
};

pub(crate) const MINIMAP_FRAME_SIZE: f32 = 184.0;
pub(crate) const MINIMAP_RIGHT_OFFSET: f32 = 14.0;
pub(crate) const MINIMAP_TOP_OFFSET: f32 = 14.0;
const MINIMAP_PADDING: f32 = 4.0;
pub(super) const MINIMAP_CONTENT_SIZE: f32 = MINIMAP_FRAME_SIZE - MINIMAP_PADDING * 2.0;
const MAP_PLAYER_MARKER_SIZE: f32 = 9.0;
const MAP_PING_MARKER_SIZE: f32 = 13.0;
const MAP_WAYPOINT_MARKER_SIZE: f32 = 9.0;

pub(super) fn spawn_overlay_root(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands<'_>,
    marker: impl Bundle,
) {
    parent.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
        marker,
    ));
}

pub(super) struct MapOverlayContext<'a> {
    pub(super) crop_bounds: MapTextureBounds,
    pub(super) image_size: Vec2,
    pub(super) player_position: Vec2,
    pub(super) sim: &'a Simulation,
    pub(super) settings: &'a MapDisplaySettings,
    pub(super) camera_rect: Option<MapTileRect>,
    pub(super) chunk_cursor: Option<ChunkCoord>,
    pub(super) markers: &'a MapOverlayMarkers,
}

#[derive(Bundle, Clone)]
struct MapOverlayPrimitive {
    node: Node,
    background: BackgroundColor,
    border: BorderColor,
    transform: UiTransform,
    z_index: ZIndex,
}

impl MapOverlayPrimitive {
    fn new(
        node: Node,
        background: BackgroundColor,
        border: BorderColor,
        transform: UiTransform,
    ) -> Self {
        Self {
            node,
            background,
            border,
            transform,
            z_index: ZIndex(0),
        }
    }
}

pub(super) fn reconcile_map_overlay(
    commands: &mut Commands,
    overlay_root: Entity,
    details: &mut MapDetailCache,
    changed_layers: [bool; MapOverlayLayer::ALL.len()],
    context: MapOverlayContext,
) {
    for (layer_index, layer) in MapOverlayLayer::ALL.into_iter().enumerate() {
        if !changed_layers[layer_index] {
            continue;
        }

        let mut desired = Vec::new();
        match layer {
            MapOverlayLayer::Navigation => {
                if let Some(rect) = context.camera_rect.and_then(|rect| {
                    map_rect_for_world_rect(context.crop_bounds, context.image_size, rect)
                }) {
                    spawn_rect_overlay(
                        &mut desired,
                        rect,
                        Color::srgba(0.98, 0.92, 0.55, 0.96),
                        Color::srgba(0.98, 0.92, 0.55, 0.10),
                        2.0,
                    );
                }

                if let Some(coord) = context.chunk_cursor
                    && let Some(rect) =
                        map_rect_for_chunk(context.crop_bounds, context.image_size, coord)
                {
                    spawn_rect_overlay(
                        &mut desired,
                        rect,
                        Color::srgba(0.42, 0.88, 1.0, 0.95),
                        Color::srgba(0.20, 0.66, 0.82, 0.16),
                        2.0,
                    );
                }
            }
            MapOverlayLayer::Pollution => spawn_pollution_overlays(&mut desired, &context),
            MapOverlayLayer::Entities => spawn_entity_overlays(&mut desired, &context),
            MapOverlayLayer::Power => spawn_power_overlays(&mut desired, &context),
            MapOverlayLayer::ProductionProblems => {
                spawn_production_problem_overlays(&mut desired, &context);
            }
            MapOverlayLayer::Enemies => spawn_threat_overlays(&mut desired, &context),
            MapOverlayLayer::Construction => spawn_construction_overlays(&mut desired, &context),
            MapOverlayLayer::Player => {
                spawn_point_overlay(
                    &mut desired,
                    context.crop_bounds,
                    context.image_size,
                    context.player_position,
                    MAP_PLAYER_MARKER_SIZE,
                    Color::srgba(0.98, 0.96, 0.74, 0.98),
                    Color::srgba(0.02, 0.02, 0.018, 0.95),
                );
            }
            MapOverlayLayer::Markers => {
                for marker in &context.markers.pings {
                    spawn_point_overlay(
                        &mut desired,
                        context.crop_bounds,
                        context.image_size,
                        marker.position,
                        MAP_PING_MARKER_SIZE,
                        Color::NONE,
                        marker.color,
                    );
                }

                for marker in &context.markers.waypoints {
                    spawn_point_overlay(
                        &mut desired,
                        context.crop_bounds,
                        context.image_size,
                        marker.position,
                        MAP_WAYPOINT_MARKER_SIZE,
                        marker.color,
                        Color::srgba(0.02, 0.02, 0.018, 0.92),
                    );
                }
            }
        }

        for primitive in &mut desired {
            primitive.z_index = ZIndex(layer_index as i32);
        }
        reconcile_overlay_layer(
            commands,
            overlay_root,
            details.layer_entities_mut(overlay_root, layer),
            desired,
        );
    }
}

fn reconcile_overlay_layer(
    commands: &mut Commands,
    overlay_root: Entity,
    entities: &mut Vec<Entity>,
    desired: Vec<MapOverlayPrimitive>,
) {
    let desired_len = desired.len();
    let retained = entities.len().min(desired.len());
    for (entity, primitive) in entities.iter().copied().zip(desired.iter()).take(retained) {
        commands.entity(entity).insert(primitive.clone());
    }
    for primitive in desired.into_iter().skip(retained) {
        let entity = commands.spawn(primitive).id();
        commands.entity(overlay_root).add_child(entity);
        entities.push(entity);
    }
    for entity in entities.drain(desired_len..) {
        commands.entity(entity).despawn();
    }
}

/// Red haze over polluted revealed chunks; opacity scales with the chunk's
/// pollution level.
fn spawn_pollution_overlays(parent: &mut Vec<MapOverlayPrimitive>, context: &MapOverlayContext) {
    // Below this level the haze would be invisible anyway; skip the rect.
    const MIN_VISIBLE_POLLUTION_MICRO: u64 = 100_000;
    // Pollution level rendered at full haze opacity (10 pollution units).
    const FULL_HAZE_POLLUTION_MICRO: u64 = 10_000_000;

    if !context.settings.overlays.is_enabled(MapOverlay::Pollution) {
        return;
    }

    for coord in crop_chunk_coords(context.crop_bounds) {
        if !context.sim.world().chunks.contains_key(&coord) {
            continue;
        }
        let amount_micro = context.sim.pollution().amount_micro(coord);
        if amount_micro < MIN_VISIBLE_POLLUTION_MICRO {
            continue;
        }
        if !context.settings.debug_reveal_all && !context.sim.is_chunk_revealed(coord) {
            continue;
        }
        let Some(rect) = map_rect_for_chunk(context.crop_bounds, context.image_size, coord) else {
            continue;
        };

        let strength = ((amount_micro as f32 / MIN_VISIBLE_POLLUTION_MICRO as f32).ln_1p()
            / (FULL_HAZE_POLLUTION_MICRO as f32 / MIN_VISIBLE_POLLUTION_MICRO as f32).ln_1p())
        .clamp(0.06, 1.0);
        spawn_rect_overlay(
            parent,
            rect,
            Color::NONE,
            Color::srgba(0.82, 0.20, 0.16, 0.30 * strength),
            0.0,
        );
    }
}

fn spawn_threat_overlays(parent: &mut Vec<MapOverlayPrimitive>, context: &MapOverlayContext) {
    if !context.settings.overlays.is_enabled(MapOverlay::Enemies) {
        return;
    }
    let max_x = context.crop_bounds.min_x + i64::from(context.crop_bounds.width) - 1;
    let max_y = context.crop_bounds.min_y + i64::from(context.crop_bounds.height) - 1;
    let snapshot = context.sim.enemy_map_snapshot_in_tile_rect(
        context.crop_bounds.min_x,
        max_x,
        context.crop_bounds.min_y,
        max_y,
    );
    for coord in snapshot.contacted_sectors {
        if let Some(rect) = map_rect_for_chunk(context.crop_bounds, context.image_size, coord) {
            spawn_rect_overlay(
                parent,
                rect,
                Color::srgba(1.0, 0.38, 0.18, 0.82),
                Color::srgba(0.8, 0.12, 0.06, 0.14),
                1.0,
            );
        }
    }
    for (_, x, y) in snapshot.known_bases {
        spawn_point_overlay(
            parent,
            context.crop_bounds,
            context.image_size,
            Vec2::new(x as f32, y as f32),
            10.0,
            Color::srgb(0.95, 0.16, 0.08),
            Color::BLACK,
        );
    }
    for location in snapshot
        .raids
        .into_iter()
        .map(|(_, location)| location)
        .chain(
            snapshot
                .expansions
                .into_iter()
                .map(|(_, location)| location),
        )
    {
        let position = match location {
            ThreatLocation::Exact { x, y } => Vec2::new(x as f32, y as f32),
            ThreatLocation::Sector(coord) => {
                let (x, y) = coord.min_tile();
                Vec2::new(
                    (x + i64::from(CHUNK_SIZE) / 2) as f32,
                    (y + i64::from(CHUNK_SIZE) / 2) as f32,
                )
            }
        };
        spawn_point_overlay(
            parent,
            context.crop_bounds,
            context.image_size,
            position,
            8.0,
            Color::srgb(1.0, 0.6, 0.12),
            Color::BLACK,
        );
    }
}

fn spawn_power_overlays(parent: &mut Vec<MapOverlayPrimitive>, context: &MapOverlayContext) {
    if !context
        .settings
        .overlays
        .is_enabled(MapOverlay::PowerNetworks)
        || context.crop_bounds.width == 0
        || context.crop_bounds.height == 0
    {
        return;
    }
    let max_x = context.crop_bounds.min_x + i64::from(context.crop_bounds.width) - 1;
    let max_y = context.crop_bounds.min_y + i64::from(context.crop_bounds.height) - 1;
    let snapshot = context.sim.power_map_snapshot_in_tile_rect(
        context.crop_bounds.min_x,
        max_x,
        context.crop_bounds.min_y,
        max_y,
    );
    let poles = snapshot
        .poles
        .iter()
        .filter(|pole| {
            context
                .sim
                .entities()
                .placed_entity(pole.entity_id)
                .is_some_and(|placed| {
                    entity_footprint_is_visible(context.sim, context.settings, placed.footprint)
                })
        })
        .map(|pole| {
            (
                pole.entity_id,
                Vec2::new(pole.center_x2 as f32 * 0.5, pole.center_y2 as f32 * 0.5),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    for connection in snapshot.connections {
        let (Some(start), Some(end)) = (
            poles.get(&connection.first_pole_id),
            poles.get(&connection.second_pole_id),
        ) else {
            continue;
        };
        let color = power_network_color(connection.network_id, connection.satisfaction_permyriad);
        spawn_world_line(
            parent,
            context.crop_bounds,
            context.image_size,
            *start,
            *end,
            1.5,
            color,
        );
    }
    for pole in snapshot.poles {
        if !poles.contains_key(&pole.entity_id) {
            continue;
        }
        spawn_point_overlay(
            parent,
            context.crop_bounds,
            context.image_size,
            Vec2::new(pole.center_x2 as f32 * 0.5, pole.center_y2 as f32 * 0.5),
            6.0,
            power_network_color(pole.network_id, pole.satisfaction_permyriad),
            Color::BLACK,
        );
    }
    for consumer in snapshot.consumers {
        if !entity_footprint_is_visible(context.sim, context.settings, consumer.footprint) {
            continue;
        }
        if consumer.network_id.is_none() {
            let center = Vec2::new(
                consumer.footprint.x as f32 + consumer.footprint.width as f32 * 0.5,
                consumer.footprint.y as f32 + consumer.footprint.height as f32 * 0.5,
            );
            spawn_point_overlay(
                parent,
                context.crop_bounds,
                context.image_size,
                center,
                7.0,
                Color::srgb(0.95, 0.10, 0.08),
                Color::BLACK,
            );
        }
    }
}

fn power_network_color(network_id: u32, satisfaction_permyriad: u32) -> Color {
    if satisfaction_permyriad < 5_000 {
        return Color::srgb(0.95, 0.12, 0.08);
    }
    if satisfaction_permyriad < 9_500 {
        return Color::srgb(1.0, 0.58, 0.08);
    }
    const COLORS: [[f32; 3]; 8] = [
        [0.24, 0.78, 1.0],
        [0.38, 0.92, 0.52],
        [0.84, 0.52, 1.0],
        [1.0, 0.82, 0.26],
        [0.20, 0.90, 0.82],
        [0.96, 0.44, 0.68],
        [0.62, 0.72, 1.0],
        [0.72, 0.94, 0.30],
    ];
    let rgb = COLORS[network_id as usize % COLORS.len()];
    Color::srgb(rgb[0], rgb[1], rgb[2])
}

fn spawn_production_problem_overlays(
    parent: &mut Vec<MapOverlayPrimitive>,
    context: &MapOverlayContext,
) {
    if !context
        .settings
        .overlays
        .is_enabled(MapOverlay::ProductionProblems)
        || context.crop_bounds.width == 0
        || context.crop_bounds.height == 0
    {
        return;
    }
    let max_x = context.crop_bounds.min_x + i64::from(context.crop_bounds.width) - 1;
    let max_y = context.crop_bounds.min_y + i64::from(context.crop_bounds.height) - 1;
    for entity_id in context.sim.entities().occupancy().entity_ids_in_tile_rect(
        context.crop_bounds.min_x,
        max_x,
        context.crop_bounds.min_y,
        max_y,
    ) {
        let Some(status) = context.sim.machine_status_for_entity(entity_id) else {
            continue;
        };
        if matches!(status, MachineStatus::Working | MachineStatus::Idle) {
            continue;
        }
        let Some(placed) = context.sim.entities().placed_entity(entity_id) else {
            continue;
        };
        if context
            .sim
            .catalog()
            .entity(placed.prototype_id)
            .is_some_and(|prototype| {
                matches!(
                    prototype.entity_kind,
                    EntityKind::EnemySpawner | EntityKind::ResourcePatch
                )
            })
        {
            continue;
        }
        if !entity_footprint_is_visible(context.sim, context.settings, placed.footprint) {
            continue;
        }
        let color = if status == MachineStatus::NoPower {
            Color::srgb(0.96, 0.12, 0.08)
        } else {
            Color::srgb(1.0, 0.58, 0.08)
        };
        let center = Vec2::new(
            placed.footprint.x as f32 + placed.footprint.width as f32 * 0.5,
            placed.footprint.y as f32 + placed.footprint.height as f32 * 0.5,
        );
        spawn_point_overlay(
            parent,
            context.crop_bounds,
            context.image_size,
            center,
            7.0,
            color,
            Color::BLACK,
        );
    }
}

fn spawn_construction_overlays(parent: &mut Vec<MapOverlayPrimitive>, context: &MapOverlayContext) {
    if !context
        .settings
        .overlays
        .is_enabled(MapOverlay::ConstructionPlans)
        || context.crop_bounds.width == 0
        || context.crop_bounds.height == 0
    {
        return;
    }
    let max_x = context.crop_bounds.min_x + i64::from(context.crop_bounds.width) - 1;
    let max_y = context.crop_bounds.min_y + i64::from(context.crop_bounds.height) - 1;
    let construction = context.sim.construction();
    for ghost_id in construction.ghost_ids_in_tile_rect(
        context.crop_bounds.min_x,
        max_x,
        context.crop_bounds.min_y,
        max_y,
    ) {
        let Some(ghost) = construction.ghost(ghost_id) else {
            continue;
        };
        if !entity_footprint_is_visible(context.sim, context.settings, ghost.footprint) {
            continue;
        }
        if let Some(rect) =
            map_rect_for_footprint(context.crop_bounds, context.image_size, ghost.footprint)
        {
            spawn_rect_overlay(
                parent,
                rect,
                Color::srgba(0.22, 0.70, 1.0, 0.95),
                Color::srgba(0.18, 0.58, 1.0, 0.16),
                1.5,
            );
        }
    }
    for entity_id in context.sim.entities().occupancy().entity_ids_in_tile_rect(
        context.crop_bounds.min_x,
        max_x,
        context.crop_bounds.min_y,
        max_y,
    ) {
        if !construction.is_marked_for_deconstruction(entity_id) {
            continue;
        }
        let Some(placed) = context.sim.entities().placed_entity(entity_id) else {
            continue;
        };
        if !entity_footprint_is_visible(context.sim, context.settings, placed.footprint) {
            continue;
        }
        if let Some(rect) =
            map_rect_for_footprint(context.crop_bounds, context.image_size, placed.footprint)
        {
            let red = Color::srgba(1.0, 0.18, 0.12, 0.95);
            spawn_rect_overlay(parent, rect, red, Color::srgba(0.82, 0.08, 0.05, 0.10), 1.5);
            spawn_ui_line(
                parent,
                Vec2::new(rect.left, rect.top),
                Vec2::new(rect.left + rect.width, rect.top + rect.height),
                1.5,
                red,
            );
            spawn_ui_line(
                parent,
                Vec2::new(rect.left + rect.width, rect.top),
                Vec2::new(rect.left, rect.top + rect.height),
                1.5,
                red,
            );
        }
    }
}

fn crop_chunk_coords(bounds: MapTextureBounds) -> impl Iterator<Item = ChunkCoord> {
    if bounds.width == 0 || bounds.height == 0 {
        return Vec::new().into_iter();
    }
    let max_x = bounds.min_x + i64::from(bounds.width.saturating_sub(1));
    let max_y = bounds.min_y + i64::from(bounds.height.saturating_sub(1));
    let mut coords = Vec::new();
    if let (Some(min), Some(max)) = (
        ChunkCoord::from_tile(bounds.min_x, bounds.min_y),
        ChunkCoord::from_tile(max_x, max_y),
    ) {
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                coords.push(ChunkCoord { x, y });
            }
        }
    }
    coords.into_iter()
}

fn spawn_entity_overlays(parent: &mut Vec<MapOverlayPrimitive>, context: &MapOverlayContext) {
    if context.crop_bounds.width == 0 || context.crop_bounds.height == 0 {
        return;
    }

    let max_x = context.crop_bounds.min_x + i64::from(context.crop_bounds.width) - 1;
    let max_y = context.crop_bounds.min_y + i64::from(context.crop_bounds.height) - 1;
    for entity_id in context.sim.entities().occupancy().entity_ids_in_tile_rect(
        context.crop_bounds.min_x,
        max_x,
        context.crop_bounds.min_y,
        max_y,
    ) {
        let Some(placed) = context.sim.entities().placed_entity(entity_id) else {
            continue;
        };
        if !entity_footprint_is_visible(context.sim, context.settings, placed.footprint) {
            continue;
        }
        let Some((color, _)) = entity_prototype_render_style(
            context.sim.catalog(),
            placed.prototype_id,
            placed.direction,
        ) else {
            continue;
        };
        let Some(rect) =
            map_rect_for_footprint(context.crop_bounds, context.image_size, placed.footprint)
        else {
            continue;
        };

        spawn_rect_overlay(
            parent,
            rect,
            map_color_with_alpha(color, 0.96),
            map_color_with_alpha(color, 0.38),
            1.0,
        );
    }
}

fn entity_footprint_is_visible(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    footprint: EntityFootprint,
) -> bool {
    settings.debug_reveal_all
        || footprint.tiles().into_iter().any(|(x, y)| {
            ChunkCoord::from_tile(x, y).is_some_and(|coord| sim.is_chunk_revealed(coord))
        })
}

fn map_color_with_alpha(color: Color, alpha: f32) -> Color {
    let srgba = color.to_srgba();
    Color::srgba(srgba.red, srgba.green, srgba.blue, alpha)
}

fn spawn_point_overlay(
    parent: &mut Vec<MapOverlayPrimitive>,
    crop_bounds: MapTextureBounds,
    image_size: Vec2,
    position: Vec2,
    size: f32,
    fill: Color,
    border: Color,
) {
    let Some(position) = map_point_for_world_position(crop_bounds, image_size, position) else {
        return;
    };

    parent.push(MapOverlayPrimitive::new(
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(position.x - size * 0.5),
            top: Val::Px(position.y - size * 0.5),
            width: Val::Px(size),
            height: Val::Px(size),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(fill),
        BorderColor::all(border),
        UiTransform::default(),
    ));
}

fn spawn_rect_overlay(
    parent: &mut Vec<MapOverlayPrimitive>,
    rect: MapUiRect,
    border: Color,
    fill: Color,
    border_width: f32,
) {
    parent.push(MapOverlayPrimitive::new(
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(rect.left),
            top: Val::Px(rect.top),
            width: Val::Px(rect.width.max(border_width)),
            height: Val::Px(rect.height.max(border_width)),
            border: UiRect::all(Val::Px(border_width)),
            ..default()
        },
        BackgroundColor(fill),
        BorderColor::all(border),
        UiTransform::default(),
    ));
}

fn spawn_world_line(
    parent: &mut Vec<MapOverlayPrimitive>,
    bounds: MapTextureBounds,
    image_size: Vec2,
    start: Vec2,
    end: Vec2,
    width: f32,
    color: Color,
) {
    let Some((start, end)) = clip_line_to_bounds(bounds, start, end) else {
        return;
    };
    let Some(start) = map_point_for_world_position(bounds, image_size, start) else {
        return;
    };
    let Some(end) = map_point_for_world_position(bounds, image_size, end) else {
        return;
    };
    spawn_ui_line(parent, start, end, width, color);
}

fn spawn_ui_line(
    parent: &mut Vec<MapOverlayPrimitive>,
    start: Vec2,
    end: Vec2,
    width: f32,
    color: Color,
) {
    let delta = end - start;
    let length = delta.length();
    if length <= f32::EPSILON {
        return;
    }
    let midpoint = (start + end) * 0.5;
    parent.push(MapOverlayPrimitive::new(
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(midpoint.x - length * 0.5),
            top: Val::Px(midpoint.y - width * 0.5),
            width: Val::Px(length),
            height: Val::Px(width),
            ..default()
        },
        BackgroundColor(color),
        BorderColor::DEFAULT,
        UiTransform::from_rotation(Rot2::radians(delta.y.atan2(delta.x))),
    ));
}

fn clip_line_to_bounds(bounds: MapTextureBounds, start: Vec2, end: Vec2) -> Option<(Vec2, Vec2)> {
    let min = Vec2::new(bounds.min_x as f32, bounds.min_y as f32);
    let max = Vec2::new(
        (bounds.min_x + i64::from(bounds.width)) as f32 - f32::EPSILON,
        (bounds.min_y + i64::from(bounds.height)) as f32 - f32::EPSILON,
    );
    let delta = end - start;
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    for (p, q) in [
        (-delta.x, start.x - min.x),
        (delta.x, max.x - start.x),
        (-delta.y, start.y - min.y),
        (delta.y, max.y - start.y),
    ] {
        if p.abs() <= f32::EPSILON {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            enter = enter.max(t);
        } else {
            exit = exit.min(t);
        }
        if enter > exit {
            return None;
        }
    }
    Some((start + delta * enter, start + delta * exit))
}

pub(super) fn spawn_minimap(
    commands: &mut Commands,
    surface: Handle<Image>,
    resources: Option<Handle<Image>>,
    texture_rect: Rect,
) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(MINIMAP_RIGHT_OFFSET),
                top: Val::Px(MINIMAP_TOP_OFFSET),
                width: Val::Px(MINIMAP_FRAME_SIZE),
                height: Val::Px(MINIMAP_FRAME_SIZE),
                padding: UiRect::all(Val::Px(MINIMAP_PADDING)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.025, 0.027, 0.88)),
            BorderColor::all(Color::srgba(0.36, 0.38, 0.34, 0.82)),
            GlobalZIndex(1800),
            MinimapRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    position_type: PositionType::Relative,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(Color::BLACK),
            ))
            .with_children(|map| {
                map.spawn((
                    ImageNode {
                        image: surface,
                        rect: Some(texture_rect),
                        image_mode: NodeImageMode::Stretch,
                        ..default()
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    MinimapImage,
                ));
                if let Some(image) = resources {
                    map.spawn((
                        ImageNode {
                            image,
                            rect: Some(texture_rect),
                            image_mode: NodeImageMode::Stretch,
                            ..default()
                        },
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        MinimapResourceImage,
                    ));
                }
                spawn_overlay_root(map, MinimapOverlayRoot);
            });
        });
}

pub(super) fn spawn_full_map(
    commands: &mut Commands,
    surface: Handle<Image>,
    resources: Option<Handle<Image>>,
    texture_rect: Rect,
    display_size: Vec2,
    overlays: crate::map::resources::MapOverlaySettings,
) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::all(Val::Px(28.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.017, 0.018, 0.96)),
            GlobalZIndex(2200),
            FullMapRoot,
        ))
        .with_children(|root| {
            root.spawn((
                ImageNode {
                    image: surface,
                    rect: Some(texture_rect),
                    image_mode: NodeImageMode::Stretch,
                    ..default()
                },
                full_map_image_node(display_size),
                BorderColor::all(Color::srgba(0.42, 0.43, 0.39, 0.9)),
                FullMapImage,
            ))
            .with_children(|image| {
                if let Some(resource_image) = resources {
                    image.spawn((
                        ImageNode {
                            image: resource_image,
                            rect: Some(texture_rect),
                            image_mode: NodeImageMode::Stretch,
                            ..default()
                        },
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        FullMapResourceImage,
                    ));
                }
                spawn_overlay_root(image, FullMapOverlayRoot);
            });
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(28.0),
                    top: Val::Px(24.0),
                    column_gap: Val::Px(8.0),
                    row_gap: Val::Px(8.0),
                    flex_wrap: FlexWrap::Wrap,
                    max_width: Val::Percent(90.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|bar| {
                spawn_overlay_button(bar, MapOverlay::Pollution, "1 Pollution", overlays);
                spawn_overlay_button(bar, MapOverlay::Resources, "2 Resources", overlays);
                spawn_overlay_button(bar, MapOverlay::PowerNetworks, "3 Power", overlays);
                spawn_overlay_button(bar, MapOverlay::ProductionProblems, "4 Problems", overlays);
                spawn_overlay_button(bar, MapOverlay::Enemies, "5 Enemies", overlays);
                spawn_overlay_button(bar, MapOverlay::ConstructionPlans, "6 Plans", overlays);
                spawn_recenter_button(bar);
            });
        });
}

pub(super) fn set_full_map_image_node_size(node: &mut Node, display_size: Vec2) {
    node.width = Val::Px(display_size.x);
    node.height = Val::Px(display_size.y);
}

pub(super) fn full_map_image_node(display_size: Vec2) -> Node {
    Node {
        width: Val::Px(display_size.x),
        height: Val::Px(display_size.y),
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    }
}

pub(super) fn spawn_overlay_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands<'_>,
    overlay: MapOverlay,
    label: &'static str,
    overlays: crate::map::resources::MapOverlaySettings,
) {
    let selected = overlays.is_enabled(overlay);
    parent
        .spawn((
            Button,
            Node {
                height: Val::Px(34.0),
                padding: UiRect::axes(Val::Px(13.0), Val::Px(0.0)),
                border: UiRect::all(Val::Px(1.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(layer_button_color(Interaction::None, selected)),
            BorderColor::all(layer_button_border_color(selected)),
            FullMapOverlayButton { overlay },
        ))
        .with_child((
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(13.0),
                ..default()
            },
            TextColor(Color::srgba(0.90, 0.88, 0.80, 0.96)),
        ));
}

pub(super) fn spawn_recenter_button(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands<'_>) {
    parent
        .spawn((
            Button,
            Node {
                height: Val::Px(34.0),
                padding: UiRect::axes(Val::Px(13.0), Val::Px(0.0)),
                border: UiRect::all(Val::Px(1.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.13, 0.14, 0.13, 0.95)),
            BorderColor::all(Color::srgba(0.38, 0.42, 0.36, 0.85)),
            FullMapRecenterButton,
        ))
        .with_child((
            Text::new("Center"),
            TextFont {
                font_size: FontSize::Px(13.0),
                ..default()
            },
            TextColor(Color::srgba(0.90, 0.88, 0.80, 0.96)),
        ));
}

pub(super) fn layer_button_color(interaction: Interaction, selected: bool) -> Color {
    if selected {
        return Color::srgba(0.30, 0.28, 0.20, 0.98);
    }

    match interaction {
        Interaction::Pressed => Color::srgba(0.22, 0.20, 0.16, 0.98),
        Interaction::Hovered => Color::srgba(0.17, 0.17, 0.15, 0.98),
        Interaction::None => Color::srgba(0.10, 0.11, 0.11, 0.95),
    }
}

pub(super) fn layer_button_border_color(selected: bool) -> Color {
    if selected {
        Color::srgba(0.72, 0.60, 0.36, 0.95)
    } else {
        Color::srgba(0.38, 0.42, 0.36, 0.85)
    }
}
