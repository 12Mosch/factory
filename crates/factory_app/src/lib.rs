use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};
use bevy::time::Fixed;
use bevy::window::PrimaryWindow;
use factory_data::{EntityKind, EntityPrototypeId, ItemId, PrototypeCatalog, TileId};
use factory_sim::{
    CHUNK_SIZE, ContainerError, Direction, ItemStack, ManualMiningTarget, PlayerState,
    ResourceCell, Simulation,
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
const SLOT_BUTTON_WIDTH: f32 = 58.0;
const SLOT_BUTTON_HEIGHT: f32 = 38.0;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryPanel {
    Player,
    Container,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerSlotClickError {
    NoOpenContainer,
    Transfer(ContainerError),
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

#[derive(Component)]
struct CursorTileHighlight;

#[derive(Component)]
struct ManualMiningProgressBarBackground;

#[derive(Component)]
struct ManualMiningProgressBarFill;

#[derive(Component)]
struct ContainerWindowRoot;

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
                    handle_debug_chest_placement,
                    handle_container_open_input,
                    handle_container_close_input,
                    update_debug_overlay,
                    sync_resource_debug_rendering,
                    sync_placed_entity_rendering,
                    sync_container_window,
                    handle_container_slot_clicks,
                    update_container_slot_text,
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

fn handle_debug_chest_placement(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &Transform), CursorCameraFilter>,
    mut sim: ResMut<SimResource>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };
    if !keyboard.just_pressed(KeyCode::KeyC) {
        return;
    }

    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        return;
    };
    let chest = find_entity_prototype_id(&sim.sim.world.prototypes, "chest");
    let _ = sim.sim.place_entity(chest, x, y, Direction::North);
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
        if is_chest_entity(&sim.sim, marker.entity_id) {
            let placed = sim
                .sim
                .entities
                .placed_entity(marker.entity_id)
                .expect("validated chest entity should still be placed");
            seen.insert(marker.entity_id);
            transform.translation = tile_translation(placed.x, placed.y, transform.translation.z);
            sprite.color = chest_color();
        } else {
            commands.entity(entity).despawn();
        }
    }

    for placed in sim.sim.entities.placed_entities() {
        if seen.contains(&placed.id) || !is_chest_entity(&sim.sim, placed.id) {
            continue;
        }

        commands.spawn((
            Sprite::from_color(chest_color(), Vec2::splat(CHEST_SPRITE_SIZE)),
            Transform::from_translation(tile_translation(placed.x, placed.y, 3.0)),
            PlacedEntitySprite {
                entity_id: placed.id,
            },
        ));
    }
}

fn sync_container_window(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut open_container: ResMut<OpenContainer>,
    roots: Query<Entity, With<ContainerWindowRoot>>,
) {
    if let Some(entity_id) = open_container.entity_id
        && sim.sim.entity_inventory(entity_id).is_err()
    {
        open_container.entity_id = None;
    }

    if open_container.entity_id.is_none() {
        for entity in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    if !roots.is_empty() {
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
            ContainerWindowRoot,
        ))
        .with_children(|root| {
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

    for (marker, mut text) in &mut texts {
        let inventory = match marker.panel {
            InventoryPanel::Player => Some(&sim.sim.player_inventory),
            InventoryPanel::Container => container_inventory,
        };
        text.0 = inventory
            .and_then(|inventory| inventory.slots.get(marker.slot_index))
            .and_then(|slot| *slot)
            .map(|stack| format_item_stack(stack, &sim.sim.world.prototypes))
            .unwrap_or_default();
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

fn chest_color() -> Color {
    Color::srgb(0.58, 0.42, 0.23)
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

    is_chest_entity(sim, entity_id).then_some(entity_id)
}

pub fn transfer_open_container_slot(
    sim: &mut Simulation,
    open_entity_id: Option<u64>,
    panel: InventoryPanel,
    slot_index: usize,
) -> Result<(), ContainerSlotClickError> {
    let entity_id = open_entity_id.ok_or(ContainerSlotClickError::NoOpenContainer)?;

    match panel {
        InventoryPanel::Player => sim.transfer_player_slot_to_entity(entity_id, slot_index),
        InventoryPanel::Container => sim.transfer_entity_slot_to_player(entity_id, slot_index),
    }
    .map_err(ContainerSlotClickError::Transfer)
}

fn is_chest_entity(sim: &Simulation, entity_id: u64) -> bool {
    let Some(entity) = sim.entities.placed_entity(entity_id) else {
        return false;
    };
    sim.world
        .prototypes
        .entities
        .get(entity.prototype_id.index())
        .is_some_and(|prototype| prototype.entity_kind == EntityKind::Chest)
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
