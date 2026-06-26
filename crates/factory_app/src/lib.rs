use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};
use bevy::time::Fixed;
use bevy::window::PrimaryWindow;
use factory_data::{EntityKind, EntityPrototypeId, ItemId, PrototypeCatalog, TileId};
use factory_sim::{
    BELT_SUBTILES_PER_TILE, BURNER_MINING_DRILL_FUEL_SLOT_INDEX,
    BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX, BurnerDrillError, CHUNK_SIZE, ContainerError, Direction,
    EntityFootprint, FURNACE_FUEL_SLOT_INDEX, FURNACE_INPUT_SLOT_INDEX, FURNACE_OUTPUT_SLOT_INDEX,
    FurnaceError, ItemStack, ManualMiningTarget, PlayerState, ResourceCell, Simulation,
};
use std::collections::{BTreeMap, BTreeSet};

pub const SIM_TICKS_PER_SECOND: f64 = 60.0;
const TILE_SIZE: f32 = 8.0;
const RESOURCE_SIZE: f32 = 4.0;
const PLAYER_SPRITE_SIZE: f32 = 6.0;
const MANUAL_MINING_BAR_WIDTH: f32 = TILE_SIZE * 0.8;
const MANUAL_MINING_BAR_HEIGHT: f32 = 1.0;
const MANUAL_MINING_BAR_Y_OFFSET: f32 = TILE_SIZE * 0.68;
const MIN_CAMERA_SCALE: f32 = 0.35;
const MAX_CAMERA_SCALE: f32 = 8.0;
const INITIAL_CAMERA_SCALE: f32 = 2.0;
const CHEST_SPRITE_SIZE: f32 = TILE_SIZE * 0.9;
const BURNER_DRILL_SPRITE_PADDING: f32 = TILE_SIZE * 0.12;
const TRANSPORT_BELT_SPRITE_SIZE: f32 = TILE_SIZE * 0.92;
const BELT_DIRECTION_SHAFT_LENGTH: f32 = TILE_SIZE * 0.46;
const BELT_DIRECTION_SHAFT_WIDTH: f32 = TILE_SIZE * 0.12;
const BELT_DIRECTION_HEAD_SIZE: f32 = TILE_SIZE * 0.22;
const BELT_ITEM_SPRITE_SIZE: f32 = TILE_SIZE * 0.28;
const BELT_ITEM_LABEL_FONT_SIZE: f32 = 3.0;
const SLOT_BUTTON_WIDTH: f32 = 58.0;
const SLOT_BUTTON_HEIGHT: f32 = 38.0;
const MACHINE_BAR_WIDTH: f32 = 180.0;
const MACHINE_BAR_HEIGHT: f32 = 10.0;

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

#[derive(Resource, Default)]
pub struct OpenContainer {
    pub entity_id: Option<u64>,
}

#[derive(Resource, Default)]
pub struct DebugBuildDirection {
    pub direction: Direction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryPanel {
    Player,
    Container,
    BurnerFuel,
    BurnerOutput,
    FurnaceInput,
    FurnaceFuel,
    FurnaceOutput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerSlotClickError {
    NoOpenContainer,
    Transfer(ContainerError),
    BurnerDrill(BurnerDrillError),
    Furnace(FurnaceError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpenMachineKind {
    Chest,
    BurnerDrill,
    Furnace,
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
struct PlacedEntitySprite {
    entity_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum BeltDirectionPart {
    Shaft,
    Head,
}

#[derive(Component)]
struct BeltDirectionSprite {
    entity_id: u64,
    part: BeltDirectionPart,
}

#[derive(Component)]
struct BeltItemSprite {
    entity_id: u64,
    lane_index: usize,
    item_index: usize,
}

#[derive(Component)]
struct BeltItemLabel {
    entity_id: u64,
    lane_index: usize,
    item_index: usize,
}

#[derive(Component)]
struct CursorTileHighlight;

#[derive(Component)]
struct ManualMiningProgressBarBackground;

#[derive(Component)]
struct ManualMiningProgressBarFill;

#[derive(Component)]
struct ContainerWindowRoot {
    entity_id: u64,
    kind: OpenMachineKind,
}

#[derive(Component)]
struct ContainerSlotButton {
    panel: InventoryPanel,
    slot_index: usize,
}

#[derive(Component)]
struct ContainerSlotText {
    panel: InventoryPanel,
    slot_index: usize,
}

#[derive(Component)]
struct BurnerEnergyText;

#[derive(Component)]
struct BurnerProgressFill;

type CursorCameraFilter = (With<Camera2d>, Without<CursorTileHighlight>);
type ManualMiningProgressBarBackgroundFilter = (
    With<ManualMiningProgressBarBackground>,
    Without<ManualMiningProgressBarFill>,
);
type ManualMiningProgressBarBackgroundQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Transform, &'static mut Visibility),
    ManualMiningProgressBarBackgroundFilter,
>;
type ManualMiningProgressBarFillQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Transform,
        &'static mut Visibility,
        &'static mut Sprite,
    ),
    With<ManualMiningProgressBarFill>,
>;
type ContainerSlotInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static ContainerSlotButton),
    (Changed<Interaction>, With<Button>),
>;

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
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<UpsStats>()
            .init_resource::<DebugInventorySelection>()
            .init_resource::<OpenContainer>()
            .init_resource::<DebugBuildDirection>()
            .add_systems(
                Startup,
                (
                    setup_camera,
                    spawn_world_tiles,
                    spawn_player,
                    spawn_cursor_tile_highlight,
                    spawn_manual_mining_progress_bar,
                    setup_debug_overlay,
                ),
            )
            .add_systems(
                FixedUpdate,
                (
                    move_player_from_input,
                    update_manual_mining_from_input,
                    tick_sim,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    zoom_camera,
                    sync_player_sprite,
                    follow_player_camera,
                    update_cursor_tile_highlight,
                    update_manual_mining_progress_bar,
                    update_ups_stats,
                    handle_debug_inventory_input,
                    handle_debug_entity_placement,
                    handle_debug_belt_item_insertion_input,
                    handle_container_open_input,
                    handle_container_close_input,
                    update_debug_overlay,
                    sync_resource_debug_rendering,
                    sync_placed_entity_rendering,
                    sync_belt_direction_rendering,
                    sync_belt_item_rendering,
                    sync_container_window,
                    handle_container_slot_clicks,
                    update_container_slot_text,
                    update_burner_drill_indicators,
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

fn spawn_manual_mining_progress_bar(mut commands: Commands) {
    commands.spawn((
        Sprite::from_color(
            Color::srgba(0.02, 0.02, 0.02, 0.82),
            Vec2::new(MANUAL_MINING_BAR_WIDTH, MANUAL_MINING_BAR_HEIGHT),
        ),
        Transform::from_translation(Vec3::new(0.0, 0.0, 5.0)),
        Visibility::Hidden,
        ManualMiningProgressBarBackground,
    ));
    commands.spawn((
        Sprite::from_color(
            Color::srgb(0.34, 0.82, 0.38),
            Vec2::new(0.0, MANUAL_MINING_BAR_HEIGHT),
        ),
        Transform::from_translation(Vec3::new(0.0, 0.0, 5.1)),
        Visibility::Hidden,
        ManualMiningProgressBarFill,
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

fn update_manual_mining_from_input(
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &Transform), CursorCameraFilter>,
    mut sim: ResMut<SimResource>,
) {
    let target = mouse
        .filter(|mouse| mouse.pressed(MouseButton::Right))
        .and_then(|_| cursor_tile_from_window(&windows, &cameras))
        .map(|(x, y)| ManualMiningTarget { x, y });

    sim.sim.update_manual_mining(target);
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
    let cursor_tile = cursor_tile_from_window(&windows, &cameras);

    for (mut transform, mut visibility) in &mut highlights {
        if let Some((x, y)) = cursor_tile {
            transform.translation = tile_translation(x, y, transform.translation.z);
            *visibility = Visibility::Visible;
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}

fn update_manual_mining_progress_bar(
    sim: Res<SimResource>,
    mut backgrounds: ManualMiningProgressBarBackgroundQuery,
    mut fills: ManualMiningProgressBarFillQuery,
) {
    let progress = sim.sim.manual_mining_progress;

    for (mut transform, mut visibility) in &mut backgrounds {
        if let Some(progress) = progress {
            transform.translation =
                manual_mining_bar_translation(progress.target.x, progress.target.y, 5.0);
            *visibility = Visibility::Visible;
        } else {
            *visibility = Visibility::Hidden;
        }
    }

    for (mut transform, mut visibility, mut sprite) in &mut fills {
        if let Some(progress) = progress {
            let fill_ratio =
                (progress.progress_ticks as f32 / progress.required_ticks as f32).clamp(0.0, 1.0);
            let fill_width = MANUAL_MINING_BAR_WIDTH * fill_ratio;
            let mut translation =
                manual_mining_bar_translation(progress.target.x, progress.target.y, 5.1);
            translation.x += (fill_width - MANUAL_MINING_BAR_WIDTH) * 0.5;
            transform.translation = translation;
            sprite.custom_size = Some(Vec2::new(fill_width, MANUAL_MINING_BAR_HEIGHT));
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

fn handle_debug_entity_placement(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &Transform), CursorCameraFilter>,
    mut sim: ResMut<SimResource>,
    mut build_direction: ResMut<DebugBuildDirection>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };

    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        if keyboard.just_pressed(KeyCode::KeyR) {
            build_direction.direction = next_debug_build_direction(build_direction.direction);
        }
        return;
    };
    let _ = handle_debug_build_action_at_tile(&mut sim.sim, &keyboard, &mut build_direction, x, y);
}

fn handle_debug_belt_item_insertion_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &Transform), CursorCameraFilter>,
    inventory_selection: Res<DebugInventorySelection>,
    mut sim: ResMut<SimResource>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };
    if !keyboard.just_pressed(KeyCode::KeyV) {
        return;
    }

    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        return;
    };

    let _ = handle_debug_belt_item_insertion_at_tile(&mut sim.sim, &inventory_selection, x, y);
}

pub fn handle_debug_build_action_at_tile(
    sim: &mut Simulation,
    keyboard: &ButtonInput<KeyCode>,
    build_direction: &mut DebugBuildDirection,
    x: i32,
    y: i32,
) -> Option<u64> {
    if keyboard.just_pressed(KeyCode::KeyR) {
        build_direction.direction = next_debug_build_direction(build_direction.direction);
    }

    let prototype_name = debug_build_prototype_name(keyboard)?;
    let prototype = find_entity_prototype_id(&sim.world.prototypes, prototype_name);
    sim.place_entity(prototype, x, y, build_direction.direction)
        .ok()
}

pub fn handle_debug_belt_item_insertion_at_tile(
    sim: &mut Simulation,
    inventory_selection: &DebugInventorySelection,
    x: i32,
    y: i32,
) -> Option<()> {
    let item_id = sim
        .world
        .prototypes
        .items
        .get(inventory_selection.selected_index % sim.world.prototypes.items.len().max(1))?
        .id;
    let entity_id = sim.entities.occupancy().entity_at(x, y)?;
    if sim.belt_segment(entity_id).is_err() {
        return None;
    }

    for lane_index in 0..2 {
        if sim
            .insert_item_onto_belt(entity_id, lane_index, item_id)
            .is_ok()
        {
            return Some(());
        }
    }

    None
}

fn debug_build_prototype_name(keyboard: &ButtonInput<KeyCode>) -> Option<&'static str> {
    if keyboard.just_pressed(KeyCode::KeyC) {
        Some("chest")
    } else if keyboard.just_pressed(KeyCode::KeyB) {
        Some("burner_mining_drill")
    } else if keyboard.just_pressed(KeyCode::KeyF) {
        Some("stone_furnace")
    } else if keyboard.just_pressed(KeyCode::KeyT) {
        Some("transport_belt")
    } else {
        None
    }
}

fn next_debug_build_direction(direction: Direction) -> Direction {
    match direction {
        Direction::North => Direction::East,
        Direction::East => Direction::South,
        Direction::South => Direction::West,
        Direction::West => Direction::North,
    }
}

fn handle_container_open_input(
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &Transform), CursorCameraFilter>,
    sim: Res<SimResource>,
    mut open_container: ResMut<OpenContainer>,
) {
    let Some(mouse) = mouse else {
        return;
    };
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    if let Some(entity_id) =
        opened_container_after_world_click(&sim.sim, cursor_tile_from_window(&windows, &cameras))
    {
        open_container.entity_id = Some(entity_id);
    }
}

fn handle_container_close_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut open_container: ResMut<OpenContainer>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };
    if keyboard.just_pressed(KeyCode::Escape) {
        open_container.entity_id = None;
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

fn sync_placed_entity_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut sprites: Query<(Entity, &PlacedEntitySprite, &mut Transform, &mut Sprite)>,
) {
    let mut seen = BTreeSet::new();

    for (entity, marker, mut transform, mut sprite) in &mut sprites {
        if let Some((color, size)) = renderable_entity_style(&sim.sim, marker.entity_id) {
            let placed = sim
                .sim
                .entities
                .placed_entity(marker.entity_id)
                .expect("validated renderable entity should still be placed");
            seen.insert(marker.entity_id);
            transform.translation = entity_translation(&placed.footprint, transform.translation.z);
            sprite.color = color;
            sprite.custom_size = Some(size);
        } else {
            commands.entity(entity).despawn();
        }
    }

    for placed in sim.sim.entities.placed_entities() {
        let Some((color, size)) = renderable_entity_style(&sim.sim, placed.id) else {
            continue;
        };
        if seen.contains(&placed.id) {
            continue;
        }

        commands.spawn((
            Sprite::from_color(color, size),
            Transform::from_translation(entity_translation(&placed.footprint, 3.0)),
            PlacedEntitySprite {
                entity_id: placed.id,
            },
        ));
    }
}

fn sync_belt_direction_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut sprites: Query<(Entity, &BeltDirectionSprite, &mut Transform, &mut Sprite)>,
) {
    let mut seen = BTreeSet::new();

    for (entity, marker, mut transform, mut sprite) in &mut sprites {
        let key = (marker.entity_id, marker.part);
        if let Some((translation, size, color)) =
            belt_direction_render_state(&sim.sim, marker.entity_id, marker.part)
        {
            seen.insert(key);
            transform.translation = translation;
            sprite.color = color;
            sprite.custom_size = Some(size);
        } else {
            commands.entity(entity).despawn();
        }
    }

    for placed in sim.sim.entities.placed_entities() {
        if sim.sim.belt_segment(placed.id).is_err() {
            continue;
        }

        for part in [BeltDirectionPart::Shaft, BeltDirectionPart::Head] {
            let key = (placed.id, part);
            if seen.contains(&key) {
                continue;
            }

            let Some((translation, size, color)) =
                belt_direction_render_state(&sim.sim, placed.id, part)
            else {
                continue;
            };

            commands.spawn((
                Sprite::from_color(color, size),
                Transform::from_translation(translation),
                BeltDirectionSprite {
                    entity_id: placed.id,
                    part,
                },
            ));
        }
    }
}

fn sync_belt_item_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut sprites: Query<
        (Entity, &BeltItemSprite, &mut Transform, &mut Sprite),
        Without<BeltItemLabel>,
    >,
    mut labels: Query<
        (Entity, &BeltItemLabel, &mut Transform, &mut Text2d),
        Without<BeltItemSprite>,
    >,
) {
    let mut seen_sprites = BTreeSet::new();
    let mut seen_labels = BTreeSet::new();

    for (entity, marker, mut transform, mut sprite) in &mut sprites {
        let key = (marker.entity_id, marker.lane_index, marker.item_index);
        if let Some((translation, color)) = belt_item_render_state(
            &sim.sim,
            marker.entity_id,
            marker.lane_index,
            marker.item_index,
        ) {
            seen_sprites.insert(key);
            transform.translation = translation;
            sprite.color = color;
            sprite.custom_size = Some(Vec2::splat(BELT_ITEM_SPRITE_SIZE));
        } else {
            commands.entity(entity).despawn();
        }
    }

    for (entity, marker, mut transform, mut text) in &mut labels {
        let key = (marker.entity_id, marker.lane_index, marker.item_index);
        if let Some((translation, label)) = belt_item_label_render_state(
            &sim.sim,
            marker.entity_id,
            marker.lane_index,
            marker.item_index,
        ) {
            seen_labels.insert(key);
            transform.translation = translation;
            text.0 = label;
        } else {
            commands.entity(entity).despawn();
        }
    }

    for placed in sim.sim.entities.placed_entities() {
        let Ok(segment) = sim.sim.belt_segment(placed.id) else {
            continue;
        };

        for (lane_index, lane) in segment.lanes.iter().enumerate() {
            for item_index in 0..lane.items.len() {
                let key = (placed.id, lane_index, item_index);
                if !seen_sprites.contains(&key) {
                    let Some((translation, color)) =
                        belt_item_render_state(&sim.sim, placed.id, lane_index, item_index)
                    else {
                        continue;
                    };
                    commands.spawn((
                        Sprite::from_color(color, Vec2::splat(BELT_ITEM_SPRITE_SIZE)),
                        Transform::from_translation(translation),
                        BeltItemSprite {
                            entity_id: placed.id,
                            lane_index,
                            item_index,
                        },
                    ));
                }

                if !seen_labels.contains(&key) {
                    let Some((translation, label)) =
                        belt_item_label_render_state(&sim.sim, placed.id, lane_index, item_index)
                    else {
                        continue;
                    };
                    commands.spawn((
                        Text2d::new(label),
                        TextFont::from_font_size(BELT_ITEM_LABEL_FONT_SIZE),
                        TextColor(Color::WHITE),
                        TextLayout::justify(Justify::Center),
                        Transform::from_translation(translation),
                        Anchor::CENTER,
                        Text2dShadow::default(),
                        BeltItemLabel {
                            entity_id: placed.id,
                            lane_index,
                            item_index,
                        },
                    ));
                }
            }
        }
    }
}

fn sync_container_window(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut open_container: ResMut<OpenContainer>,
    roots: Query<(Entity, &ContainerWindowRoot)>,
) {
    let open_kind = open_container
        .entity_id
        .and_then(|entity_id| open_machine_kind(&sim.sim, entity_id));
    if open_container.entity_id.is_some() && open_kind.is_none() {
        open_container.entity_id = None;
    }

    if open_container.entity_id.is_none() {
        for (entity, _) in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    let entity_id = open_container
        .entity_id
        .expect("open container should be set after validation");
    let kind = open_kind.expect("open machine kind should be known after validation");

    for (entity, root) in &roots {
        if root.entity_id != entity_id || root.kind != kind {
            commands.entity(entity).despawn();
        }
    }

    if roots
        .iter()
        .any(|(_, root)| root.entity_id == entity_id && root.kind == kind)
    {
        return;
    }

    commands
        .spawn((
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
            ContainerWindowRoot { entity_id, kind },
        ))
        .with_children(|root| {
            spawn_player_inventory_panel(root);
            match kind {
                OpenMachineKind::Chest => spawn_chest_panel(root),
                OpenMachineKind::BurnerDrill => spawn_burner_drill_panel(root),
                OpenMachineKind::Furnace => spawn_furnace_panel(root),
            }
        });
}

fn spawn_player_inventory_panel(root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new("Player"),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        panel
            .spawn((
                Node {
                    width: Val::Px(500.0),
                    flex_wrap: FlexWrap::Wrap,
                    row_gap: Val::Px(4.0),
                    column_gap: Val::Px(4.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|grid| {
                for slot_index in 0..factory_sim::PLAYER_INVENTORY_SLOT_COUNT {
                    spawn_slot_button(grid, InventoryPanel::Player, slot_index);
                }
            });
    });
}

fn spawn_chest_panel(root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new("Chest"),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
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
                for slot_index in 0..16 {
                    spawn_slot_button(grid, InventoryPanel::Container, slot_index);
                }
            });
    });
}

fn spawn_burner_drill_panel(root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(220.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new("Burner Drill"),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        panel.spawn((
            Text::new("Energy: 0 J"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            BurnerEnergyText,
        ));
        panel
            .spawn((
                Node {
                    width: Val::Px(MACHINE_BAR_WIDTH),
                    height: Val::Px(MACHINE_BAR_HEIGHT),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.10, 0.10, 0.11, 0.96)),
            ))
            .with_child((
                Node {
                    width: Val::Px(0.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.33, 0.74, 0.48)),
                BurnerProgressFill,
            ));
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
                    InventoryPanel::BurnerFuel,
                    BURNER_MINING_DRILL_FUEL_SLOT_INDEX,
                );
                spawn_labeled_slot(
                    slots,
                    "Output",
                    InventoryPanel::BurnerOutput,
                    BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX,
                );
            });
    });
}

fn spawn_furnace_panel(root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(220.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new("Stone Furnace"),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        panel.spawn((
            Text::new("Energy: 0 J"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            BurnerEnergyText,
        ));
        panel
            .spawn((
                Node {
                    width: Val::Px(MACHINE_BAR_WIDTH),
                    height: Val::Px(MACHINE_BAR_HEIGHT),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.10, 0.10, 0.11, 0.96)),
            ))
            .with_child((
                Node {
                    width: Val::Px(0.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.82, 0.48, 0.24)),
                BurnerProgressFill,
            ));
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
                    "Input",
                    InventoryPanel::FurnaceInput,
                    FURNACE_INPUT_SLOT_INDEX,
                );
                spawn_labeled_slot(
                    slots,
                    "Fuel",
                    InventoryPanel::FurnaceFuel,
                    FURNACE_FUEL_SLOT_INDEX,
                );
                spawn_labeled_slot(
                    slots,
                    "Output",
                    InventoryPanel::FurnaceOutput,
                    FURNACE_OUTPUT_SLOT_INDEX,
                );
            });
    });
}

fn spawn_labeled_slot(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    panel: InventoryPanel,
    slot_index: usize,
) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|slot| {
            slot.spawn((
                Text::new(label),
                TextFont::from_font_size(11.0),
                TextColor(Color::srgb(0.78, 0.80, 0.78)),
            ));
            spawn_slot_button(slot, panel, slot_index);
        });
}

fn spawn_slot_button(
    grid: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    panel: InventoryPanel,
    slot_index: usize,
) {
    grid.spawn((
        Button,
        Node {
            width: Val::Px(SLOT_BUTTON_WIDTH),
            height: Val::Px(SLOT_BUTTON_HEIGHT),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            padding: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.14, 0.14, 0.15, 0.96)),
        ContainerSlotButton { panel, slot_index },
    ))
    .with_child((
        Text::new(""),
        TextFont::from_font_size(9.0),
        TextColor(Color::WHITE),
        TextLayout::justify(Justify::Center),
        ContainerSlotText { panel, slot_index },
    ));
}

fn handle_container_slot_clicks(
    mut interactions: ContainerSlotInteractionQuery,
    mut sim: ResMut<SimResource>,
    open_container: Res<OpenContainer>,
) {
    for (interaction, button) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }

        let _ = transfer_open_container_slot(
            &mut sim.sim,
            open_container.entity_id,
            button.panel,
            button.slot_index,
        );
    }
}

fn update_container_slot_text(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut texts: Query<(&ContainerSlotText, &mut Text)>,
) {
    let container_inventory = open_container
        .entity_id
        .and_then(|entity_id| sim.sim.entity_inventory(entity_id).ok());
    let burner_drill_state = open_container
        .entity_id
        .and_then(|entity_id| sim.sim.burner_drill_state(entity_id).ok());
    let furnace_state = open_container
        .entity_id
        .and_then(|entity_id| sim.sim.furnace_state(entity_id).ok());

    for (marker, mut text) in &mut texts {
        let stack = match marker.panel {
            InventoryPanel::Player => sim
                .sim
                .player_inventory
                .slots
                .get(marker.slot_index)
                .and_then(|slot| *slot),
            InventoryPanel::Container => container_inventory
                .and_then(|inventory| inventory.slots.get(marker.slot_index))
                .and_then(|slot| *slot),
            InventoryPanel::BurnerFuel => burner_drill_state.and_then(|state| {
                (marker.slot_index == BURNER_MINING_DRILL_FUEL_SLOT_INDEX)
                    .then_some(state.energy.fuel_slot)
                    .flatten()
            }),
            InventoryPanel::BurnerOutput => burner_drill_state.and_then(|state| {
                (marker.slot_index == BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX)
                    .then_some(state.output_slot)
                    .flatten()
            }),
            InventoryPanel::FurnaceInput => furnace_state.and_then(|state| {
                (marker.slot_index == FURNACE_INPUT_SLOT_INDEX)
                    .then_some(state.input_slot)
                    .flatten()
            }),
            InventoryPanel::FurnaceFuel => furnace_state.and_then(|state| {
                (marker.slot_index == FURNACE_FUEL_SLOT_INDEX)
                    .then_some(state.energy.fuel_slot)
                    .flatten()
            }),
            InventoryPanel::FurnaceOutput => furnace_state.and_then(|state| {
                (marker.slot_index == FURNACE_OUTPUT_SLOT_INDEX)
                    .then_some(state.output_slot)
                    .flatten()
            }),
        };
        text.0 = stack
            .map(|stack| format_item_stack(stack, &sim.sim.world.prototypes))
            .unwrap_or_default();
    }
}

fn update_burner_drill_indicators(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut energy_texts: Query<&mut Text, With<BurnerEnergyText>>,
    mut progress_fills: Query<&mut Node, With<BurnerProgressFill>>,
) {
    let indicator = open_container.entity_id.and_then(|entity_id| {
        match open_machine_kind(&sim.sim, entity_id)? {
            OpenMachineKind::BurnerDrill => {
                let state = sim.sim.burner_drill_state(entity_id).ok()?;
                Some((
                    state.energy.energy_remaining_joules,
                    state.mining_progress_ticks,
                    state.mining_required_ticks,
                ))
            }
            OpenMachineKind::Furnace => {
                let state = sim.sim.furnace_state(entity_id).ok()?;
                Some((
                    state.energy.energy_remaining_joules,
                    state.crafting_progress_ticks,
                    state.crafting_required_ticks,
                ))
            }
            OpenMachineKind::Chest => None,
        }
    });

    for mut text in &mut energy_texts {
        text.0 = indicator
            .map(|(energy_remaining_joules, _, _)| {
                format!(
                    "Energy: {} J",
                    energy_remaining_joules.max(0.0).round() as u64
                )
            })
            .unwrap_or_else(|| "Energy: 0 J".to_string());
    }

    for mut node in &mut progress_fills {
        let progress = indicator
            .map(|(_, progress_ticks, required_ticks)| {
                progress_ticks as f32 / required_ticks.max(1) as f32
            })
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        node.width = Val::Px(MACHINE_BAR_WIDTH * progress);
    }
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

fn entity_translation(footprint: &EntityFootprint, z: f32) -> Vec3 {
    Vec3::new(
        footprint.x as f32 * TILE_SIZE + footprint.width as f32 * TILE_SIZE * 0.5,
        footprint.y as f32 * TILE_SIZE + footprint.height as f32 * TILE_SIZE * 0.5,
        z,
    )
}

fn chest_color() -> Color {
    Color::srgb(0.58, 0.42, 0.23)
}

fn burner_drill_color() -> Color {
    Color::srgb(0.40, 0.43, 0.40)
}

fn furnace_color() -> Color {
    Color::srgb(0.54, 0.45, 0.36)
}

fn transport_belt_color() -> Color {
    Color::srgb(0.93, 0.72, 0.18)
}

fn manual_mining_bar_translation(x: i32, y: i32, z: f32) -> Vec3 {
    let mut translation = tile_translation(x, y, z);
    translation.y += MANUAL_MINING_BAR_Y_OFFSET;
    translation
}

fn player_translation(player: PlayerState, z: f32) -> Vec3 {
    let (x, y) = player.position_tiles();
    Vec3::new(x * TILE_SIZE, y * TILE_SIZE, z)
}

fn cursor_tile_from_window(
    windows: &Query<&Window, With<PrimaryWindow>>,
    cameras: &Query<(&Camera, &Transform), CursorCameraFilter>,
) -> Option<(i32, i32)> {
    windows
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
        .map(world_position_to_tile_coord)
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

pub fn opened_container_after_world_click(
    sim: &Simulation,
    cursor_tile: Option<(i32, i32)>,
) -> Option<u64> {
    let (x, y) = cursor_tile?;
    let entity_id = sim.entities.occupancy().entity_at(x, y)?;

    open_machine_kind(sim, entity_id)
        .is_some()
        .then_some(entity_id)
}

pub fn transfer_open_container_slot(
    sim: &mut Simulation,
    open_entity_id: Option<u64>,
    panel: InventoryPanel,
    slot_index: usize,
) -> Result<(), ContainerSlotClickError> {
    let entity_id = open_entity_id.ok_or(ContainerSlotClickError::NoOpenContainer)?;

    match panel {
        InventoryPanel::Player => {
            if is_burner_drill_entity(sim, entity_id) {
                return sim
                    .transfer_player_slot_to_burner_drill_fuel(entity_id, slot_index)
                    .map_err(ContainerSlotClickError::BurnerDrill);
            }
            if is_furnace_entity(sim, entity_id) {
                return transfer_player_slot_to_furnace(sim, entity_id, slot_index)
                    .map_err(ContainerSlotClickError::Furnace);
            }
            sim.transfer_player_slot_to_entity(entity_id, slot_index)
        }
        InventoryPanel::Container => sim.transfer_entity_slot_to_player(entity_id, slot_index),
        InventoryPanel::BurnerFuel => {
            return sim
                .transfer_burner_drill_fuel_to_player(entity_id)
                .map_err(ContainerSlotClickError::BurnerDrill);
        }
        InventoryPanel::BurnerOutput => {
            return sim
                .transfer_burner_drill_output_to_player(entity_id)
                .map_err(ContainerSlotClickError::BurnerDrill);
        }
        InventoryPanel::FurnaceInput => {
            return sim
                .transfer_furnace_input_to_player(entity_id)
                .map_err(ContainerSlotClickError::Furnace);
        }
        InventoryPanel::FurnaceFuel => {
            return sim
                .transfer_furnace_fuel_to_player(entity_id)
                .map_err(ContainerSlotClickError::Furnace);
        }
        InventoryPanel::FurnaceOutput => {
            return sim
                .transfer_furnace_output_to_player(entity_id)
                .map_err(ContainerSlotClickError::Furnace);
        }
    }
    .map_err(ContainerSlotClickError::Transfer)
}

fn is_burner_drill_entity(sim: &Simulation, entity_id: u64) -> bool {
    open_machine_kind(sim, entity_id) == Some(OpenMachineKind::BurnerDrill)
}

fn is_furnace_entity(sim: &Simulation, entity_id: u64) -> bool {
    open_machine_kind(sim, entity_id) == Some(OpenMachineKind::Furnace)
}

fn transfer_player_slot_to_furnace(
    sim: &mut Simulation,
    entity_id: u64,
    slot_index: usize,
) -> Result<(), FurnaceError> {
    let stack = sim
        .player_inventory
        .slots
        .get(slot_index)
        .ok_or(FurnaceError::InvalidSlot { slot_index })?
        .ok_or(FurnaceError::EmptySlot { slot_index })?;
    let is_fuel = sim
        .world
        .prototypes
        .items
        .get(stack.item_id.index())
        .filter(|prototype| prototype.id == stack.item_id)
        .and_then(|prototype| prototype.fuel_value_joules)
        .is_some();

    if is_fuel {
        sim.transfer_player_slot_to_furnace_fuel(entity_id, slot_index)
    } else {
        sim.transfer_player_slot_to_furnace_input(entity_id, slot_index)
    }
}

fn open_machine_kind(sim: &Simulation, entity_id: u64) -> Option<OpenMachineKind> {
    let entity = sim.entities.placed_entity(entity_id)?;
    let prototype = sim
        .world
        .prototypes
        .entities
        .get(entity.prototype_id.index())?;

    if prototype.entity_kind == EntityKind::Chest {
        Some(OpenMachineKind::Chest)
    } else if prototype.entity_kind == EntityKind::MiningDrill
        && sim.burner_drill_state(entity_id).is_ok()
    {
        Some(OpenMachineKind::BurnerDrill)
    } else if prototype.entity_kind == EntityKind::Furnace && sim.furnace_state(entity_id).is_ok() {
        Some(OpenMachineKind::Furnace)
    } else {
        None
    }
}

fn renderable_entity_style(sim: &Simulation, entity_id: u64) -> Option<(Color, Vec2)> {
    let placed = sim.entities.placed_entity(entity_id)?;
    let prototype = sim
        .world
        .prototypes
        .entities
        .get(placed.prototype_id.index())?;
    if prototype.entity_kind == EntityKind::TransportBelt {
        return Some((
            transport_belt_color(),
            Vec2::splat(TRANSPORT_BELT_SPRITE_SIZE),
        ));
    }

    match open_machine_kind(sim, entity_id) {
        Some(OpenMachineKind::Chest) => Some((chest_color(), Vec2::splat(CHEST_SPRITE_SIZE))),
        Some(OpenMachineKind::BurnerDrill) => Some((
            burner_drill_color(),
            Vec2::new(
                placed.footprint.width as f32 * TILE_SIZE - BURNER_DRILL_SPRITE_PADDING,
                placed.footprint.height as f32 * TILE_SIZE - BURNER_DRILL_SPRITE_PADDING,
            ),
        )),
        Some(OpenMachineKind::Furnace) => Some((
            furnace_color(),
            Vec2::new(
                placed.footprint.width as f32 * TILE_SIZE - BURNER_DRILL_SPRITE_PADDING,
                placed.footprint.height as f32 * TILE_SIZE - BURNER_DRILL_SPRITE_PADDING,
            ),
        )),
        None => None,
    }
}

fn belt_direction_render_state(
    sim: &Simulation,
    entity_id: u64,
    part: BeltDirectionPart,
) -> Option<(Vec3, Vec2, Color)> {
    let placed = sim.entities.placed_entity(entity_id)?;
    let segment = sim.belt_segment(entity_id).ok()?;
    let center = tile_translation(placed.x, placed.y, 3.2);
    let along = direction_render_vector(segment.dir);
    let translation = match part {
        BeltDirectionPart::Shaft => {
            let offset = along * TILE_SIZE * -0.06;
            Vec3::new(center.x + offset.x, center.y + offset.y, center.z)
        }
        BeltDirectionPart::Head => {
            let offset = along * TILE_SIZE * 0.24;
            Vec3::new(center.x + offset.x, center.y + offset.y, center.z + 0.1)
        }
    };
    let size = match part {
        BeltDirectionPart::Shaft if along.x.abs() > 0.0 => {
            Vec2::new(BELT_DIRECTION_SHAFT_LENGTH, BELT_DIRECTION_SHAFT_WIDTH)
        }
        BeltDirectionPart::Shaft => {
            Vec2::new(BELT_DIRECTION_SHAFT_WIDTH, BELT_DIRECTION_SHAFT_LENGTH)
        }
        BeltDirectionPart::Head => Vec2::splat(BELT_DIRECTION_HEAD_SIZE),
    };

    Some((translation, size, belt_direction_color()))
}

fn belt_item_render_state(
    sim: &Simulation,
    entity_id: u64,
    lane_index: usize,
    item_index: usize,
) -> Option<(Vec3, Color)> {
    let placed = sim.entities.placed_entity(entity_id)?;
    let segment = sim.belt_segment(entity_id).ok()?;
    let item = segment.lanes.get(lane_index)?.items.get(item_index)?;
    let center = tile_translation(placed.x, placed.y, 4.0);
    let along = direction_render_vector(segment.dir);
    let perpendicular = Vec2::new(-along.y, along.x);
    let progress = f32::from(item.position_subtile) / f32::from(BELT_SUBTILES_PER_TILE) - 0.5;
    let lane_offset = if lane_index == 0 { -0.18 } else { 0.18 };
    let offset = (along * progress + perpendicular * lane_offset) * TILE_SIZE;
    let color = belt_item_color(item.item_id, &sim.world.prototypes);

    Some((
        Vec3::new(center.x + offset.x, center.y + offset.y, 4.0),
        color,
    ))
}

fn belt_item_label_render_state(
    sim: &Simulation,
    entity_id: u64,
    lane_index: usize,
    item_index: usize,
) -> Option<(Vec3, String)> {
    let (mut translation, _) = belt_item_render_state(sim, entity_id, lane_index, item_index)?;
    let segment = sim.belt_segment(entity_id).ok()?;
    let item = segment.lanes.get(lane_index)?.items.get(item_index)?;
    let name = sim
        .world
        .prototypes
        .items
        .get(item.item_id.index())
        .map(|item| item.name.as_str())
        .unwrap_or("?");

    translation.z += 0.2;
    Some((translation, compact_item_name(name)))
}

fn direction_render_vector(direction: Direction) -> Vec2 {
    match direction {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => Vec2::NEG_Y,
        Direction::West => Vec2::NEG_X,
    }
}

fn belt_direction_color() -> Color {
    Color::srgb(0.30, 0.22, 0.07)
}

fn belt_item_color(item_id: ItemId, catalog: &PrototypeCatalog) -> Color {
    catalog
        .items
        .get(item_id.index())
        .map(|item| item.name.as_str())
        .map(|name| match name {
            "iron_ore" => Color::srgb(0.70, 0.66, 0.58),
            "copper_ore" => Color::srgb(0.86, 0.42, 0.20),
            "coal" => Color::srgb(0.05, 0.05, 0.05),
            "stone" => Color::srgb(0.54, 0.51, 0.47),
            _ => Color::srgb(0.64, 0.82, 0.95),
        })
        .unwrap_or(Color::WHITE)
}

fn format_item_stack(stack: ItemStack, catalog: &PrototypeCatalog) -> String {
    let name = catalog
        .items
        .get(stack.item_id.index())
        .map(|item| item.name.as_str())
        .unwrap_or("unknown");
    format!("{}\n{}", compact_item_name(name), stack.count)
}

fn compact_item_name(name: &str) -> String {
    name.split('_')
        .filter_map(|part| part.chars().next())
        .collect::<String>()
        .to_uppercase()
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

fn find_entity_prototype_id(catalog: &PrototypeCatalog, name: &str) -> EntityPrototypeId {
    catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required entity prototype {name:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn belt_item_render_state_changes_only_when_sim_position_changes() {
        let mut sim = Simulation::new_test_world(123);
        let belt = find_entity_prototype_id(&sim.world.prototypes, "transport_belt");
        let iron_ore = find_item_id(&sim.world.prototypes, "iron_ore");
        let (x, y) = first_placeable_tile(&sim, belt, Direction::East);
        let belt_id = sim
            .place_entity(belt, x, y, Direction::East)
            .expect("belt should be placeable");

        sim.insert_item_onto_belt(belt_id, 0, iron_ore)
            .expect("empty belt should accept item");

        let (before, _) = belt_item_render_state(&sim, belt_id, 0, 0)
            .expect("inserted belt item should have render state");
        let (same_tick, _) = belt_item_render_state(&sim, belt_id, 0, 0)
            .expect("inserted belt item should keep render state");
        assert_eq!(same_tick, before);

        sim.tick();

        let (after_tick, _) = belt_item_render_state(&sim, belt_id, 0, 0)
            .expect("ticked belt item should have render state");
        assert!(after_tick.x > before.x);
        assert_eq!(after_tick.y, before.y);

        let (without_tick, _) = belt_item_render_state(&sim, belt_id, 0, 0)
            .expect("unticked belt item should keep render state");
        assert_eq!(without_tick, after_tick);
    }

    #[test]
    fn belt_direction_render_state_marks_downstream_direction() {
        let mut sim = Simulation::new_test_world(123);
        let belt = find_entity_prototype_id(&sim.world.prototypes, "transport_belt");
        let (x, y) = first_placeable_tile(&sim, belt, Direction::North);
        let belt_id = sim
            .place_entity(belt, x, y, Direction::North)
            .expect("belt should be placeable");

        let (shaft_translation, shaft_size, _) =
            belt_direction_render_state(&sim, belt_id, BeltDirectionPart::Shaft)
                .expect("belt shaft should have render state");
        let (head_translation, head_size, _) =
            belt_direction_render_state(&sim, belt_id, BeltDirectionPart::Head)
                .expect("belt head should have render state");

        assert!(head_translation.y > shaft_translation.y);
        assert!(shaft_size.y > shaft_size.x);
        assert_eq!(head_size, Vec2::splat(BELT_DIRECTION_HEAD_SIZE));
    }

    #[test]
    fn belt_item_label_uses_item_prototype_initials() {
        let mut sim = Simulation::new_test_world(123);
        let belt = find_entity_prototype_id(&sim.world.prototypes, "transport_belt");
        let copper_ore = find_item_id(&sim.world.prototypes, "copper_ore");
        let (x, y) = first_placeable_tile(&sim, belt, Direction::East);
        let belt_id = sim
            .place_entity(belt, x, y, Direction::East)
            .expect("belt should be placeable");

        sim.insert_item_onto_belt(belt_id, 0, copper_ore)
            .expect("empty belt should accept item");

        let (_, label) = belt_item_label_render_state(&sim, belt_id, 0, 0)
            .expect("inserted belt item should have label render state");
        assert_eq!(label, "CO");
    }

    fn first_placeable_tile(
        sim: &Simulation,
        prototype_id: EntityPrototypeId,
        direction: Direction,
    ) -> (i32, i32) {
        for chunk in sim.world.chunks.values() {
            for (index, _) in chunk.tiles.iter().enumerate() {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                let x = chunk.coord.x * CHUNK_SIZE + local_x;
                let y = chunk.coord.y * CHUNK_SIZE + local_y;

                if sim.can_place_entity(prototype_id, x, y, direction).is_ok() {
                    return (x, y);
                }
            }
        }

        panic!("expected at least one placeable tile");
    }
}
