use bevy::prelude::{App, ColorMaterial, Commands, Entity, Handle, Mesh, Res, ResMut, Resource};
use factory_sim::{ChunkCoord, EntityId};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
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
    /// Entities newly entering the visible membership on this update.
    pub(crate) added: Vec<EntityId>,
    /// Entities leaving visible membership on this update.
    pub(crate) removed: Vec<EntityId>,
    /// Still-visible entities whose appearance may have changed.
    pub(crate) style_dirty: Vec<EntityId>,
    /// Changes only when `ids` changes, so belt-item caches do not wake for an
    /// unrelated style invalidation.
    pub(crate) membership_revision: u64,
    /// A replacement/load invalidates every render-entity registry even when
    /// the new world happens to reuse the same simulation IDs.
    pub(crate) reset: bool,
    pub(crate) visible_revision: u64,
    pub(crate) entity_topology_revision: u64,
}

impl Default for VisibleEntityIds {
    fn default() -> Self {
        Self {
            ids: HashSet::new(),
            added: Vec::new(),
            removed: Vec::new(),
            style_dirty: Vec::new(),
            membership_revision: 0,
            reset: false,
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
    /// Draw at most one representative item per occupied belt when individual
    /// lane contents are below useful screen-space detail.
    pub(crate) aggregate_belt_items: bool,
    pub(crate) show_belt_item_labels: bool,
}

impl Default for RenderDetail {
    fn default() -> Self {
        Self {
            show_resource_amount_labels: true,
            show_belt_directions: true,
            show_belt_items: true,
            aggregate_belt_items: false,
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
    /// Meshes waiting to be rebuilt after a terrain or neighboring chunk
    /// change. Keeping this work durable lets the presentation system advance
    /// it over several frames without losing a simulation revision.
    pub(crate) pending_mesh_rebuilds: BTreeSet<ChunkCoord>,
    /// Missing visible and adjacent chunks waiting for their first mesh.
    pub(crate) pending_mesh_builds: BTreeSet<ChunkCoord>,
    /// Least-recently-used order for meshes without a live render entity.
    pub(crate) inactive_mesh_lru: VecDeque<ChunkCoord>,
    #[cfg(test)]
    pub(crate) mesh_builds_last_sync: usize,
}

/// Maximum unused belt-item sprites (or labels) retained immediately after a
/// sync.
///
/// The dense render-sync workload in `rendering::world::tests` draws
/// `DENSE_BELT_ITEM_RENDER_BELTS` (2,000) belts with two items each, or about
/// 4,000 sprite/label pairs. The cap is the next power of two above that
/// measured peak so a representative dense view can sit fully pooled without
/// an immediate hard eviction, while any larger spike is cut back at once.
pub(crate) const BELT_ITEM_POOL_MAX_UNUSED: usize = 4096;
/// Steady-state reusable reserve of unused sprites (or labels) kept after
/// trimming converges.
///
/// The small render-sync fixture exposes roughly one hundred belts (a few
/// hundred items) in a 3x3-chunk window. A 512-entity reserve therefore covers
/// that steady state with more than 2x headroom for camera panning jitter,
/// while releasing most of a dense 4,000-item spike (about 87%) once demand
/// stays low.
pub(crate) const BELT_ITEM_POOL_SPARE_UNUSED: usize = 512;
/// Gradual release budget per pool per sync once the hard cap is satisfied.
///
/// Draining at most 256 entities per frame converges a 4,000-item spike to the
/// 512-entity reserve in about 14 frames (well under half a second at 60 Hz)
/// without despawning thousands of entities in a single frame. Small dips
/// that recover within a frame or two therefore keep their pooled entities
/// and reuse them without spawn churn.
pub(crate) const BELT_ITEM_POOL_TRIM_PER_SYNC: usize = 256;

const _: () = assert!(
    BELT_ITEM_POOL_SPARE_UNUSED <= BELT_ITEM_POOL_MAX_UNUSED,
    "belt item pool spare reserve must fit inside the hard cap"
);

/// Bounded-retention pool for hidden belt-item sprites and labels.
///
/// Pooling avoids spawn/despawn churn under ordinary camera movement: entities
/// leaving the view are hidden and pushed here, then popped for newly visible
/// items within the same or a later sync. Without a bound, a temporary
/// visibility spike would leave the pool at its historical high-water mark
/// for the rest of the session.
///
/// Retention policy:
/// * only unused (hidden, inactive) entities are ever trimmed; active/visible
///   entities are owned by the belt-item render cache and are never touched
///   by trimming;
/// * whenever the unused count exceeds [`BELT_ITEM_POOL_MAX_UNUSED`], the
///   oldest excess is despawned immediately so an arbitrarily large spike
///   cannot stay resident;
/// * remaining excess above [`BELT_ITEM_POOL_SPARE_UNUSED`] is released
///   gradually, at most [`BELT_ITEM_POOL_TRIM_PER_SYNC`] entities per pool per
///   sync, so brief demand dips reuse pooled entities instead of churning;
/// * trimming runs on every belt-item sync, including frames with no
///   visibility or item changes, so idle low demand still converges to the
///   spare reserve.
#[derive(Resource, Default)]
pub(crate) struct BeltItemRenderPool {
    pub(crate) sprites: Vec<Entity>,
    pub(crate) labels: Vec<Entity>,
}

impl BeltItemRenderPool {
    pub(crate) fn trim_excess(&mut self, commands: &mut Commands) {
        trim_unused_pool(commands, &mut self.sprites);
        trim_unused_pool(commands, &mut self.labels);
    }
}

fn trim_unused_pool(commands: &mut Commands, unused: &mut Vec<Entity>) {
    if unused.len() > BELT_ITEM_POOL_MAX_UNUSED {
        let excess = unused.len() - BELT_ITEM_POOL_MAX_UNUSED;
        for entity in unused.drain(..excess) {
            commands.entity(entity).despawn();
        }
    }
    if unused.len() > BELT_ITEM_POOL_SPARE_UNUSED {
        let excess = (unused.len() - BELT_ITEM_POOL_SPARE_UNUSED).min(BELT_ITEM_POOL_TRIM_PER_SYNC);
        for entity in unused.drain(..excess) {
            commands.entity(entity).despawn();
        }
    }
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

    #[test]
    fn belt_item_pool_trimming_is_bounded_and_gradual() {
        let mut app = App::new();
        app.init_resource::<BeltItemRenderPool>();
        fn trim_pool(mut commands: Commands, mut pool: ResMut<BeltItemRenderPool>) {
            pool.trim_excess(&mut commands);
        }
        app.add_systems(bevy::app::Update, trim_pool);

        // A spike far above the hard cap is cut to the cap immediately, then
        // releases only one gradual budget in the same frame.
        let oversized = BELT_ITEM_POOL_MAX_UNUSED + 1_000;
        let sprites: Vec<Entity> = (0..oversized)
            .map(|_| app.world_mut().spawn_empty().id())
            .collect();
        let labels: Vec<Entity> = (0..oversized)
            .map(|_| app.world_mut().spawn_empty().id())
            .collect();
        *app.world_mut().resource_mut::<BeltItemRenderPool>() = BeltItemRenderPool {
            sprites: sprites.clone(),
            labels: labels.clone(),
        };
        app.update();
        let pool = app.world().resource::<BeltItemRenderPool>();
        assert_eq!(
            pool.sprites.len(),
            BELT_ITEM_POOL_MAX_UNUSED - BELT_ITEM_POOL_TRIM_PER_SYNC
        );
        assert_eq!(
            pool.labels.len(),
            BELT_ITEM_POOL_MAX_UNUSED - BELT_ITEM_POOL_TRIM_PER_SYNC
        );
        // The oldest excess was evicted; the retained tail stays alive.
        for entity in sprites.iter().take(1_000) {
            assert!(app.world().get_entity(*entity).is_err());
        }
        for entity in pool.sprites.iter() {
            assert!(app.world().get_entity(*entity).is_ok());
        }

        // Sustained low demand converges to the spare reserve instead of
        // retaining the high-water mark.
        for _ in 0..64 {
            app.update();
        }
        let pool = app.world().resource::<BeltItemRenderPool>();
        assert_eq!(pool.sprites.len(), BELT_ITEM_POOL_SPARE_UNUSED);
        assert_eq!(pool.labels.len(), BELT_ITEM_POOL_SPARE_UNUSED);
    }

    #[test]
    fn belt_item_pool_below_spare_is_fully_retained() {
        let mut app = App::new();
        app.init_resource::<BeltItemRenderPool>();
        fn trim_pool(mut commands: Commands, mut pool: ResMut<BeltItemRenderPool>) {
            pool.trim_excess(&mut commands);
        }
        app.add_systems(bevy::app::Update, trim_pool);

        let retained = BELT_ITEM_POOL_SPARE_UNUSED - 1;
        let sprites: Vec<Entity> = (0..retained)
            .map(|_| app.world_mut().spawn_empty().id())
            .collect();
        *app.world_mut().resource_mut::<BeltItemRenderPool>() = BeltItemRenderPool {
            sprites: sprites.clone(),
            labels: Vec::new(),
        };
        app.update();
        app.update();
        let pool = app.world().resource::<BeltItemRenderPool>();
        assert_eq!(pool.sprites.len(), retained);
        for entity in sprites {
            assert!(app.world().get_entity(entity).is_ok());
        }
    }

    fn average_update_duration(app: &mut App, frames: usize) -> Duration {
        let started = Instant::now();
        for _ in 0..frames {
            app.update();
        }
        started.elapsed() / frames as u32
    }
}
