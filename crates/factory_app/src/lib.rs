use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};
use bevy::time::Fixed;
use bevy::window::PrimaryWindow;
use factory_data::{ItemId, PrototypeCatalog, TileId};
use factory_sim::{CHUNK_SIZE, PlayerState, ResourceCell, Simulation};
use std::collections::{BTreeMap, BTreeSet};

pub const SIM_TICKS_PER_SECOND: f64 = 60.0;
const TILE_SIZE: f32 = 8.0;
const RESOURCE_SIZE: f32 = 4.0;
const PLAYER_SPRITE_SIZE: f32 = 6.0;
const MIN_CAMERA_SCALE: f32 = 0.35;
const MAX_CAMERA_SCALE: f32 = 8.0;
const INITIAL_CAMERA_SCALE: f32 = 2.0;

pub struct FactoryAppPlugin;

#[derive(Resource)]
pub struct SimResource {
    pub sim: Simulation,
}

#[derive(Resource, Default)]
pub struct UpsStats {
    elapsed: f64,
    fixed_ticks: u32,
    pub ups: f64,
}

#[derive(Resource, Default)]
pub struct DebugInventorySelection {
    pub selected_index: usize,
}

#[derive(Component)]
struct DebugOverlayText;

#[derive(Component)]
struct ResourceSprite {
    x: i32,
    y: i32,
}

#[derive(Component)]
struct ResourceAmountLabel {
    x: i32,
    y: i32,
}

#[derive(Component)]
struct PlayerSprite;

#[derive(Component)]
struct CursorTileHighlight;

type CursorCameraFilter = (With<Camera2d>, Without<CursorTileHighlight>);

impl Plugin for FactoryAppPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(SIM_TICKS_PER_SECOND))
            .insert_resource(SimResource {
                sim: Simulation::new(
                    123,
                    PrototypeCatalog::load_base().expect("base prototype catalog should load"),
                ),
            })
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<UpsStats>()
            .init_resource::<DebugInventorySelection>()
            .add_systems(
                Startup,
                (
                    setup_camera,
                    spawn_world_tiles,
                    spawn_player,
                    spawn_cursor_tile_highlight,
                    setup_debug_overlay,
                ),
            )
            .add_systems(FixedUpdate, (move_player_from_input, tick_sim).chain())
            .add_systems(
                Update,
                (
                    zoom_camera,
                    sync_player_sprite,
                    follow_player_camera,
                    update_cursor_tile_highlight,
                    update_ups_stats,
                    handle_debug_inventory_input,
                    update_debug_overlay,
                    sync_resource_debug_rendering,
                ),
            );
    }
}

pub fn app() -> App {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(FactoryAppPlugin);
    app
}

pub fn run() {
    app().run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: INITIAL_CAMERA_SCALE,
            ..OrthographicProjection::default_2d()
        }),
    ));
}

fn spawn_world_tiles(mut commands: Commands, sim: Res<SimResource>) {
    let ids = RenderPrototypeIds::from_catalog(&sim.sim.world.prototypes);

    for chunk in sim.sim.world.chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            let world_x = chunk.coord.x * CHUNK_SIZE + local_x;
            let world_y = chunk.coord.y * CHUNK_SIZE + local_y;
            let translation = tile_translation(world_x, world_y, 0.0);

            commands.spawn((
                Sprite::from_color(tile_color(tile.tile_id, ids), Vec2::splat(TILE_SIZE)),
                Transform::from_translation(translation),
            ));
        }
    }
}

fn spawn_player(mut commands: Commands, sim: Res<SimResource>) {
    commands.spawn((
        Sprite::from_color(
            Color::srgb(0.92, 0.84, 0.42),
            Vec2::splat(PLAYER_SPRITE_SIZE),
        ),
        Transform::from_translation(player_translation(sim.sim.player, 4.0)),
        PlayerSprite,
    ));
}

fn spawn_cursor_tile_highlight(mut commands: Commands) {
    commands.spawn((
        Sprite::from_color(Color::srgba(1.0, 1.0, 1.0, 0.28), Vec2::splat(TILE_SIZE)),
        Transform::from_translation(Vec3::new(0.0, 0.0, 3.0)),
        Visibility::Hidden,
        CursorTileHighlight,
    ));
}

fn setup_debug_overlay(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                left: Val::Px(12.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.02, 0.72)),
            GlobalZIndex(1000),
        ))
        .with_child((
            Text::new("Tick: 0\nUPS: 0.0"),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
            DebugOverlayText,
        ));
}

fn tick_sim(mut sim: ResMut<SimResource>, mut ups: ResMut<UpsStats>) {
    sim.sim.tick();
    ups.fixed_ticks += 1;
}

fn move_player_from_input(
    time: Res<Time<Fixed>>,
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut sim: ResMut<SimResource>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };

    let direction = movement_direction_from_keyboard(&keyboard);
    if direction != Vec2::ZERO {
        sim.sim
            .move_player(direction.x, direction.y, time.delta_secs());
    }
}

fn zoom_camera(
    mouse_scroll: Option<Res<AccumulatedMouseScroll>>,
    mut camera: Query<&mut Projection, With<Camera2d>>,
) {
    let Some(mouse_scroll) = mouse_scroll else {
        return;
    };

    let scroll = mouse_scroll.delta.y;
    if scroll == 0.0 {
        return;
    }

    for mut projection in &mut camera {
        let Projection::Orthographic(orthographic) = &mut *projection else {
            continue;
        };

        let zoom_factor = (1.0 - scroll * 0.12).clamp(0.5, 1.5);
        orthographic.scale =
            (orthographic.scale * zoom_factor).clamp(MIN_CAMERA_SCALE, MAX_CAMERA_SCALE);
    }
}

fn sync_player_sprite(
    sim: Res<SimResource>,
    mut players: Query<&mut Transform, With<PlayerSprite>>,
) {
    for mut transform in &mut players {
        transform.translation = player_translation(sim.sim.player, transform.translation.z);
    }
}

fn follow_player_camera(
    sim: Res<SimResource>,
    mut cameras: Query<&mut Transform, (With<Camera2d>, Without<PlayerSprite>)>,
) {
    let player = player_translation(sim.sim.player, 0.0);

    for mut transform in &mut cameras {
        transform.translation.x = player.x;
        transform.translation.y = player.y;
    }
}

fn update_cursor_tile_highlight(
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &Transform), CursorCameraFilter>,
    mut highlights: Query<(&mut Transform, &mut Visibility), With<CursorTileHighlight>>,
) {
    let cursor_tile = windows
        .single()
        .ok()
        .and_then(Window::cursor_position)
        .and_then(|cursor_position| {
            let (camera, camera_transform) = cameras.single().ok()?;
            let camera_global = GlobalTransform::from(*camera_transform);
            camera
                .viewport_to_world_2d(&camera_global, cursor_position)
                .ok()
        })
        .map(world_position_to_tile_coord);

    for (mut transform, mut visibility) in &mut highlights {
        if let Some((x, y)) = cursor_tile {
            transform.translation = tile_translation(x, y, transform.translation.z);
            *visibility = Visibility::Visible;
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}

fn update_ups_stats(time: Res<Time<Real>>, mut stats: ResMut<UpsStats>) {
    let delta = time.delta_secs_f64();
    if delta <= 0.0 {
        return;
    }

    stats.elapsed += delta;
    if stats.elapsed >= 1.0 {
        stats.ups = f64::from(stats.fixed_ticks) / stats.elapsed;
        stats.elapsed = 0.0;
        stats.fixed_ticks = 0;
    }
}

fn update_debug_overlay(
    sim: Res<SimResource>,
    stats: Res<UpsStats>,
    inventory_selection: Res<DebugInventorySelection>,
    mut overlay: Query<&mut Text, With<DebugOverlayText>>,
) {
    let catalog = &sim.sim.world.prototypes;
    let (selected_name, selected_count) =
        selected_inventory_item_state(&sim.sim, &inventory_selection, catalog);

    for mut text in &mut overlay {
        text.0 = format!(
            "Tick: {}\nUPS: {:.1}\nItem: {}\nCount: {}",
            sim.sim.tick_count(),
            stats.ups,
            selected_name,
            selected_count
        );
    }
}

fn handle_debug_inventory_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut inventory_selection: ResMut<DebugInventorySelection>,
    mut sim: ResMut<SimResource>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };

    let item_count = sim.sim.world.prototypes.items.len();
    if item_count == 0 {
        inventory_selection.selected_index = 0;
        return;
    }

    inventory_selection.selected_index %= item_count;

    if keyboard.just_pressed(KeyCode::BracketLeft) {
        inventory_selection.selected_index =
            (inventory_selection.selected_index + item_count - 1) % item_count;
    }
    if keyboard.just_pressed(KeyCode::BracketRight) {
        inventory_selection.selected_index = (inventory_selection.selected_index + 1) % item_count;
    }

    let selected_item = sim.sim.world.prototypes.items[inventory_selection.selected_index].id;
    if keyboard.just_pressed(KeyCode::KeyI) {
        let sim = &mut sim.sim;
        let catalog = &sim.world.prototypes;
        let inventory = &mut sim.player_inventory;
        let _ = inventory.insert(catalog, selected_item, 1);
    }
    if keyboard.just_pressed(KeyCode::KeyO) {
        let _ = sim.sim.player_inventory.remove(selected_item, 1);
    }
}

fn selected_inventory_item_state(
    sim: &Simulation,
    inventory_selection: &DebugInventorySelection,
    catalog: &PrototypeCatalog,
) -> (String, u32) {
    let Some(item) = catalog
        .items
        .get(inventory_selection.selected_index % catalog.items.len().max(1))
    else {
        return ("<none>".to_string(), 0);
    };

    (item.name.clone(), sim.player_inventory.count(item.id))
}

fn sync_resource_debug_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut sprites: Query<(Entity, &ResourceSprite, &mut Sprite)>,
    mut labels: Query<(Entity, &ResourceAmountLabel, &mut Text2d)>,
) {
    let ids = RenderPrototypeIds::from_catalog(&sim.sim.world.prototypes);
    let resources = collect_resource_tiles(&sim.sim);
    let mut seen_sprites = BTreeSet::new();
    let mut seen_labels = BTreeSet::new();

    for (entity, marker, mut sprite) in &mut sprites {
        let coord = (marker.x, marker.y);
        if let Some(resource) = resources.get(&coord) {
            seen_sprites.insert(coord);
            sprite.color = resource_color(*resource, ids);
        } else {
            commands.entity(entity).despawn();
        }
    }

    for (entity, marker, mut text) in &mut labels {
        let coord = (marker.x, marker.y);
        if let Some(resource) = resources.get(&coord) {
            seen_labels.insert(coord);
            text.0 = format_resource_amount(resource.amount);
        } else {
            commands.entity(entity).despawn();
        }
    }

    for ((x, y), resource) in resources {
        if !seen_sprites.contains(&(x, y)) {
            commands.spawn((
                Sprite::from_color(resource_color(resource, ids), Vec2::splat(RESOURCE_SIZE)),
                Transform::from_translation(tile_translation(x, y, 1.0)),
                ResourceSprite { x, y },
            ));
        }

        if !seen_labels.contains(&(x, y)) {
            commands.spawn((
                Text2d::new(format_resource_amount(resource.amount)),
                TextFont::from_font_size(4.0),
                TextColor(Color::WHITE),
                TextLayout::justify(Justify::Center),
                Transform::from_translation(tile_translation(x, y, 2.0)),
                Anchor::CENTER,
                Text2dShadow::default(),
                ResourceAmountLabel { x, y },
            ));
        }
    }
}

fn collect_resource_tiles(sim: &Simulation) -> BTreeMap<(i32, i32), ResourceCell> {
    let mut resources = BTreeMap::new();

    for chunk in sim.world.chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            if let Some(resource) = tile.resource {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                resources.insert(
                    (
                        chunk.coord.x * CHUNK_SIZE + local_x,
                        chunk.coord.y * CHUNK_SIZE + local_y,
                    ),
                    resource,
                );
            }
        }
    }

    resources
}

fn format_resource_amount(amount: u32) -> String {
    amount.to_string()
}

fn tile_translation(x: i32, y: i32, z: f32) -> Vec3 {
    Vec3::new(
        x as f32 * TILE_SIZE + TILE_SIZE * 0.5,
        y as f32 * TILE_SIZE + TILE_SIZE * 0.5,
        z,
    )
}

fn player_translation(player: PlayerState, z: f32) -> Vec3 {
    let (x, y) = player.position_tiles();
    Vec3::new(x * TILE_SIZE, y * TILE_SIZE, z)
}

fn movement_direction_from_keyboard(keyboard: &ButtonInput<KeyCode>) -> Vec2 {
    let mut direction = Vec2::ZERO;
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }

    direction
}

pub fn world_position_to_tile_coord(world_position: Vec2) -> (i32, i32) {
    (
        (world_position.x / TILE_SIZE).floor() as i32,
        (world_position.y / TILE_SIZE).floor() as i32,
    )
}

fn tile_color(tile_id: TileId, ids: RenderPrototypeIds) -> Color {
    if tile_id == ids.water {
        Color::srgb(0.12, 0.34, 0.62)
    } else if tile_id == ids.dirt {
        Color::srgb(0.47, 0.38, 0.24)
    } else {
        Color::srgb(0.24, 0.50, 0.25)
    }
}

fn resource_color(resource: ResourceCell, ids: RenderPrototypeIds) -> Color {
    if resource.resource_item == ids.iron_ore {
        Color::srgb(0.62, 0.56, 0.50)
    } else if resource.resource_item == ids.copper_ore {
        Color::srgb(0.76, 0.36, 0.18)
    } else if resource.resource_item == ids.coal {
        Color::srgb(0.08, 0.08, 0.08)
    } else if resource.resource_item == ids.stone {
        Color::srgb(0.46, 0.43, 0.39)
    } else {
        Color::srgb(0.82, 0.78, 0.68)
    }
}

#[derive(Clone, Copy)]
struct RenderPrototypeIds {
    dirt: TileId,
    water: TileId,
    iron_ore: ItemId,
    copper_ore: ItemId,
    coal: ItemId,
    stone: ItemId,
}

impl RenderPrototypeIds {
    fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self {
            dirt: find_tile_id(catalog, "dirt"),
            water: find_tile_id(catalog, "water"),
            iron_ore: find_item_id(catalog, "iron_ore"),
            copper_ore: find_item_id(catalog, "copper_ore"),
            coal: find_item_id(catalog, "coal"),
            stone: find_item_id(catalog, "stone"),
        }
    }
}

fn find_tile_id(catalog: &PrototypeCatalog, name: &str) -> TileId {
    catalog
        .tiles
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required tile prototype {name:?}"))
}

fn find_item_id(catalog: &PrototypeCatalog, name: &str) -> ItemId {
    catalog
        .items
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required item prototype {name:?}"))
}
