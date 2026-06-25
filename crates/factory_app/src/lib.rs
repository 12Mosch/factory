use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::time::Fixed;
use factory_data::{ItemId, PrototypeCatalog, TileId};
use factory_sim::{CHUNK_SIZE, ResourceCell, Simulation};

pub const SIM_TICKS_PER_SECOND: f64 = 60.0;
const TILE_SIZE: f32 = 8.0;
const RESOURCE_SIZE: f32 = 4.0;
const CAMERA_PAN_SPEED: f32 = 720.0;
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

#[derive(Component)]
struct DebugOverlayText;

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
            .add_systems(
                Startup,
                (setup_camera, spawn_world_tiles, setup_debug_overlay),
            )
            .add_systems(FixedUpdate, tick_sim)
            .add_systems(
                Update,
                (
                    pan_camera,
                    zoom_camera,
                    update_ups_stats,
                    update_debug_overlay,
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

            if let Some(resource) = tile.resource {
                commands.spawn((
                    Sprite::from_color(resource_color(resource, ids), Vec2::splat(RESOURCE_SIZE)),
                    Transform::from_translation(tile_translation(world_x, world_y, 1.0)),
                ));
            }
        }
    }
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

fn pan_camera(
    time: Res<Time>,
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut camera: Query<(&Projection, &mut Transform), With<Camera2d>>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };

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

    if direction == Vec2::ZERO {
        return;
    }

    for (projection, mut transform) in &mut camera {
        let scale = orthographic_scale(projection);
        let delta = direction.normalize() * CAMERA_PAN_SPEED * scale * time.delta_secs();
        transform.translation.x += delta.x;
        transform.translation.y += delta.y;
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
    mut overlay: Query<&mut Text, With<DebugOverlayText>>,
) {
    for mut text in &mut overlay {
        text.0 = format!("Tick: {}\nUPS: {:.1}", sim.sim.tick_count(), stats.ups);
    }
}

fn tile_translation(x: i32, y: i32, z: f32) -> Vec3 {
    Vec3::new(
        x as f32 * TILE_SIZE + TILE_SIZE * 0.5,
        y as f32 * TILE_SIZE + TILE_SIZE * 0.5,
        z,
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
    if resource.item_id == ids.iron_ore {
        Color::srgb(0.62, 0.56, 0.50)
    } else if resource.item_id == ids.copper_ore {
        Color::srgb(0.76, 0.36, 0.18)
    } else if resource.item_id == ids.coal {
        Color::srgb(0.08, 0.08, 0.08)
    } else {
        Color::srgb(0.46, 0.43, 0.39)
    }
}

fn orthographic_scale(projection: &Projection) -> f32 {
    match projection {
        Projection::Orthographic(orthographic) => orthographic.scale,
        _ => 1.0,
    }
}

#[derive(Clone, Copy)]
struct RenderPrototypeIds {
    dirt: TileId,
    water: TileId,
    iron_ore: ItemId,
    copper_ore: ItemId,
    coal: ItemId,
}

impl RenderPrototypeIds {
    fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self {
            dirt: find_tile_id(catalog, "dirt"),
            water: find_tile_id(catalog, "water"),
            iron_ore: find_item_id(catalog, "iron_ore"),
            copper_ore: find_item_id(catalog, "copper_ore"),
            coal: find_item_id(catalog, "coal"),
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
