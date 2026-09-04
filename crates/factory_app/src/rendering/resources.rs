use bevy::prelude::{App, ColorMaterial, Entity, Handle, Mesh, Res, ResMut, Resource};
use factory_sim::{ChunkCoord, EntityId};
use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Resource, Default)]
pub struct RenderSyncStats {
    pub player: Duration,
    pub world_tiles: Duration,
    pub resources: Duration,
    pub placed_entities: Duration,
    pub belt_directions: Duration,
    pub belt_items: Duration,
    pub total: Duration,
}

impl RenderSyncStats {
    pub fn record_player(&mut self, elapsed: Duration) {
        self.player = elapsed;
        self.update_total();
    }

    pub fn record_world_tiles(&mut self, elapsed: Duration) {
        self.world_tiles = elapsed;
        self.update_total();
    }

    pub fn record_resources(&mut self, elapsed: Duration) {
        self.resources = elapsed;
        self.update_total();
    }

    pub fn record_placed_entities(&mut self, elapsed: Duration) {
        self.placed_entities = elapsed;
        self.update_total();
    }

    pub fn record_belt_directions(&mut self, elapsed: Duration) {
        self.belt_directions = elapsed;
        self.update_total();
    }

    pub fn record_belt_items(&mut self, elapsed: Duration) {
        self.belt_items = elapsed;
        self.update_total();
    }

    fn update_total(&mut self) {
        self.total = self.player
            + self.world_tiles
            + self.resources
            + self.placed_entities
            + self.belt_directions
            + self.belt_items;
    }
}

macro_rules! render_sync_timing_resource {
    ($name:ident) => {
        #[derive(Resource, Default)]
        pub(crate) struct $name(pub(crate) Duration);
    };
}

render_sync_timing_resource!(PlayerRenderSyncTime);
render_sync_timing_resource!(WorldTilesRenderSyncTime);
render_sync_timing_resource!(ResourcesRenderSyncTime);
render_sync_timing_resource!(PlacedEntitiesRenderSyncTime);
render_sync_timing_resource!(BeltDirectionsRenderSyncTime);
render_sync_timing_resource!(BeltItemsRenderSyncTime);

/// Installs the per-system timing slots and their consolidated debug snapshot.
pub(crate) fn init_render_sync_stats(app: &mut App) -> &mut App {
    app.init_resource::<RenderSyncStats>()
        .init_resource::<PlayerRenderSyncTime>()
        .init_resource::<WorldTilesRenderSyncTime>()
        .init_resource::<ResourcesRenderSyncTime>()
        .init_resource::<PlacedEntitiesRenderSyncTime>()
        .init_resource::<BeltDirectionsRenderSyncTime>()
        .init_resource::<BeltItemsRenderSyncTime>()
}

/// Consolidates independent timing slots after render sync has finished.
///
/// Keeping this single writer out of the measured systems lets Bevy run those
/// systems concurrently whenever their actual rendering data permits it.
pub(crate) fn collect_render_sync_stats(
    player: Res<PlayerRenderSyncTime>,
    world_tiles: Res<WorldTilesRenderSyncTime>,
    resources: Res<ResourcesRenderSyncTime>,
    placed_entities: Res<PlacedEntitiesRenderSyncTime>,
    belt_directions: Res<BeltDirectionsRenderSyncTime>,
    belt_items: Res<BeltItemsRenderSyncTime>,
    mut stats: ResMut<RenderSyncStats>,
) {
    stats.player = player.0;
    stats.world_tiles = world_tiles.0;
    stats.resources = resources.0;
    stats.placed_entities = placed_entities.0;
    stats.belt_directions = belt_directions.0;
    stats.belt_items = belt_items.0;
    stats.update_total();
}

#[derive(Resource)]
pub(crate) struct VisibleEntityIds {
    pub(crate) ids: HashSet<EntityId>,
    pub(crate) visible_revision: u64,
    pub(crate) entity_topology_revision: u64,
}

impl Default for VisibleEntityIds {
    fn default() -> Self {
        Self {
            ids: HashSet::new(),
            visible_revision: u64::MAX,
            entity_topology_revision: u64::MAX,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Resource)]
pub(crate) struct RenderDetail {
    pub(crate) show_resource_amount_labels: bool,
    pub(crate) show_belt_directions: bool,
    pub(crate) show_belt_items: bool,
    pub(crate) show_belt_item_labels: bool,
}

impl Default for RenderDetail {
    fn default() -> Self {
        Self {
            show_resource_amount_labels: true,
            show_belt_directions: true,
            show_belt_items: true,
            show_belt_item_labels: true,
        }
    }
}

#[derive(Resource, Default)]
pub struct WorldRenderCache {
    pub chunk_entities: BTreeMap<ChunkCoord, Entity>,
    pub chunk_meshes: BTreeMap<ChunkCoord, Handle<Mesh>>,
    pub material: Option<Handle<ColorMaterial>>,
    pub last_visible_revision: u64,
    pub last_chunk_revision: u64,
    /// Last terrain-write revision baked into the cached meshes. Runtime tile
    /// mutation changes tiles inside chunks that already exist, which the
    /// chunk revision never observes.
    pub last_terrain_revision: u64,
    pub last_reload_token: u64,
}

#[derive(Resource, Default)]
pub(crate) struct BeltItemRenderPool {
    pub(crate) sprites: Vec<Entity>,
    pub(crate) labels: Vec<Entity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::{IntoSystem, System};
    use std::time::Instant;

    const SCHEDULING_BENCHMARK_WORK: Duration = Duration::from_millis(2);

    fn record_player(mut timing: ResMut<PlayerRenderSyncTime>) {
        std::thread::sleep(SCHEDULING_BENCHMARK_WORK);
        timing.0 = Duration::from_millis(1);
    }

    fn record_world_tiles(mut timing: ResMut<WorldTilesRenderSyncTime>) {
        std::thread::sleep(SCHEDULING_BENCHMARK_WORK);
        timing.0 = Duration::from_millis(2);
    }

    fn record_resources(mut timing: ResMut<ResourcesRenderSyncTime>) {
        std::thread::sleep(SCHEDULING_BENCHMARK_WORK);
        timing.0 = Duration::from_millis(3);
    }

    fn record_placed_entities(mut timing: ResMut<PlacedEntitiesRenderSyncTime>) {
        std::thread::sleep(SCHEDULING_BENCHMARK_WORK);
        timing.0 = Duration::from_millis(4);
    }

    fn record_belt_directions(mut timing: ResMut<BeltDirectionsRenderSyncTime>) {
        std::thread::sleep(SCHEDULING_BENCHMARK_WORK);
        timing.0 = Duration::from_millis(5);
    }

    fn record_belt_items(mut timing: ResMut<BeltItemsRenderSyncTime>) {
        std::thread::sleep(SCHEDULING_BENCHMARK_WORK);
        timing.0 = Duration::from_millis(6);
    }

    #[derive(Resource, Default)]
    struct SharedRenderSyncTime(u64);

    macro_rules! shared_timing_writer {
        ($name:ident) => {
            fn $name(mut timing: ResMut<SharedRenderSyncTime>) {
                std::thread::sleep(SCHEDULING_BENCHMARK_WORK);
                timing.0 += 1;
            }
        };
    }

    shared_timing_writer!(record_shared_player);
    shared_timing_writer!(record_shared_world_tiles);
    shared_timing_writer!(record_shared_resources);
    shared_timing_writer!(record_shared_placed_entities);
    shared_timing_writer!(record_shared_belt_directions);
    shared_timing_writer!(record_shared_belt_items);

    #[test]
    fn render_sync_timing_writers_have_compatible_resource_access() {
        let mut app = App::new();
        init_render_sync_stats(&mut app);
        let accesses = [
            IntoSystem::into_system(record_player).initialize(app.world_mut()),
            IntoSystem::into_system(record_world_tiles).initialize(app.world_mut()),
            IntoSystem::into_system(record_resources).initialize(app.world_mut()),
            IntoSystem::into_system(record_placed_entities).initialize(app.world_mut()),
            IntoSystem::into_system(record_belt_directions).initialize(app.world_mut()),
            IntoSystem::into_system(record_belt_items).initialize(app.world_mut()),
        ];

        for (left_index, left) in accesses.iter().enumerate() {
            for right in &accesses[left_index + 1..] {
                assert!(
                    left.is_compatible(right),
                    "all 15 pairs of render timing writers must remain scheduler-compatible"
                );
            }
        }
    }

    #[test]
    fn render_sync_stats_are_consolidated_from_independent_timings() {
        let mut app = App::new();
        init_render_sync_stats(&mut app);
        app.world_mut().resource_mut::<PlayerRenderSyncTime>().0 = Duration::from_millis(1);
        app.world_mut().resource_mut::<WorldTilesRenderSyncTime>().0 = Duration::from_millis(2);
        app.world_mut().resource_mut::<ResourcesRenderSyncTime>().0 = Duration::from_millis(3);
        app.world_mut()
            .resource_mut::<PlacedEntitiesRenderSyncTime>()
            .0 = Duration::from_millis(4);
        app.world_mut()
            .resource_mut::<BeltDirectionsRenderSyncTime>()
            .0 = Duration::from_millis(5);
        app.world_mut().resource_mut::<BeltItemsRenderSyncTime>().0 = Duration::from_millis(6);
        app.add_systems(bevy::app::Update, collect_render_sync_stats);

        app.update();

        let stats = app.world().resource::<RenderSyncStats>();
        assert_eq!(stats.player, Duration::from_millis(1));
        assert_eq!(stats.world_tiles, Duration::from_millis(2));
        assert_eq!(stats.resources, Duration::from_millis(3));
        assert_eq!(stats.placed_entities, Duration::from_millis(4));
        assert_eq!(stats.belt_directions, Duration::from_millis(5));
        assert_eq!(stats.belt_items, Duration::from_millis(6));
        assert_eq!(stats.total, Duration::from_millis(21));
    }

    #[test]
    #[ignore = "manual scheduler-contention benchmark"]
    fn render_sync_timing_scheduling_contention_benchmark() {
        const WARMUP_FRAMES: usize = 3;
        const MEASUREMENT_FRAMES: usize = 40;

        let mut shared = App::new();
        shared.init_resource::<SharedRenderSyncTime>().add_systems(
            bevy::app::Update,
            (
                record_shared_player,
                record_shared_world_tiles,
                record_shared_resources,
                record_shared_placed_entities,
                record_shared_belt_directions,
                record_shared_belt_items,
            ),
        );
        let mut split = App::new();
        init_render_sync_stats(&mut split).add_systems(
            bevy::app::Update,
            (
                record_player,
                record_world_tiles,
                record_resources,
                record_placed_entities,
                record_belt_directions,
                record_belt_items,
            ),
        );

        for _ in 0..WARMUP_FRAMES {
            shared.update();
            split.update();
        }
        let shared_average = average_update_duration(&mut shared, MEASUREMENT_FRAMES);
        let split_average = average_update_duration(&mut split, MEASUREMENT_FRAMES);
        let speedup = shared_average.as_secs_f64() / split_average.as_secs_f64();

        println!(
            "render_sync_timing_scheduling_contention_benchmark: shared avg {:.3} ms, split avg {:.3} ms, speedup {:.2}x",
            shared_average.as_secs_f64() * 1_000.0,
            split_average.as_secs_f64() * 1_000.0,
            speedup,
        );
        assert!(
            split_average < shared_average,
            "independent timing resources should reduce scheduler wall time"
        );
    }

    fn average_update_duration(app: &mut App, frames: usize) -> Duration {
        let started = Instant::now();
        for _ in 0..frames {
            app.update();
        }
        started.elapsed() / frames as u32
    }
}
