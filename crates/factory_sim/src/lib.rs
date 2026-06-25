use factory_data::{ItemId, PrototypeCatalog, TileId};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

pub const CHUNK_SIZE: i32 = 32;
const TEST_WORLD_MIN_CHUNK: i32 = -2;
const TEST_WORLD_MAX_CHUNK: i32 = 1;
const RESOURCE_PATCH_GRID_SIZE: i32 = 40;
const RESOURCE_PATCH_GRID_JITTER: i32 = 16;
const RESOURCE_PATCH_EDGE_NOISE: i32 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tick(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Simulation {
    pub tick: u64,
    pub world: WorldSim,
    pub entities: EntityStore,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WorldSim {
    pub seed: u64,
    pub prototypes: PrototypeCatalog,
    pub chunks: BTreeMap<ChunkCoord, Chunk>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chunk {
    pub coord: ChunkCoord,
    pub tiles: Vec<TileCell>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TileCell {
    pub tile_id: TileId,
    pub collision: TileCollision,
    pub resource: Option<ResourceCell>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileCollision {
    pub walkable: bool,
    pub buildable: bool,
    pub minable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceCell {
    pub resource_item: ItemId,
    pub amount: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MinedResource {
    pub resource_item: ItemId,
    pub amount: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EntityStore {
    entities: Vec<SimEntity>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SimEntity {
    pub id: u64,
    pub x: i64,
    pub y: i64,
}

impl Simulation {
    pub fn new(seed: u64, prototypes: PrototypeCatalog) -> Self {
        Self {
            tick: 0,
            world: WorldSim::new(seed, prototypes),
            entities: EntityStore::new_test_entities(seed),
        }
    }

    pub fn new_test_world(seed: u64) -> Self {
        Self::new(
            seed,
            PrototypeCatalog::load_base().expect("base prototype catalog should load"),
        )
    }

    pub fn tick(&mut self) {
        self.tick += 1;
        self.entities.advance(Tick(self.tick), self.world.seed);
    }

    pub fn tick_count(&self) -> u64 {
        self.tick
    }

    pub fn current_tick(&self) -> Tick {
        Tick(self.tick)
    }

    pub fn seed(&self) -> u64 {
        self.world.seed
    }

    pub fn prototype_count(&self) -> usize {
        self.world.prototypes.item_count()
    }

    pub fn state_hash(&self) -> u64 {
        let mut hasher = StableHasher::default();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

impl WorldSim {
    pub fn new(seed: u64, prototypes: PrototypeCatalog) -> Self {
        let chunks = generate_test_chunks(seed, &prototypes);
        Self {
            seed,
            prototypes,
            chunks,
        }
    }

    pub fn new_seeded(seed: u64) -> Self {
        Self::new(
            seed,
            PrototypeCatalog::load_base().expect("base prototype catalog should load"),
        )
    }

    pub fn tile_at(&self, x: i32, y: i32) -> Option<&TileCell> {
        let (coord, index) = chunk_coord_and_tile_index(x, y);

        self.chunks
            .get(&coord)
            .and_then(|chunk| chunk.tiles.get(index))
    }

    pub fn resource_hash(&self) -> u64 {
        let mut hasher = StableHasher::default();

        for chunk in self.chunks.values() {
            for (index, tile) in chunk.tiles.iter().enumerate() {
                let Some(resource) = tile.resource else {
                    continue;
                };

                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                let x = chunk.coord.x * CHUNK_SIZE + local_x;
                let y = chunk.coord.y * CHUNK_SIZE + local_y;

                x.hash(&mut hasher);
                y.hash(&mut hasher);
                resource.resource_item.hash(&mut hasher);
                resource.amount.hash(&mut hasher);
            }
        }

        hasher.finish()
    }

    pub fn mine_resource_at(&mut self, x: i32, y: i32, amount: u32) -> Option<MinedResource> {
        if amount == 0 {
            return None;
        }

        let ids = WorldPrototypeIds::from_catalog(&self.prototypes);
        let tile = self.tile_at_mut(x, y)?;
        let resource = tile.resource.as_mut()?;
        let mined_amount = amount.min(resource.amount);
        let mined = MinedResource {
            resource_item: resource.resource_item,
            amount: mined_amount,
        };

        resource.amount -= mined_amount;
        if resource.amount == 0 {
            tile.resource = None;
            tile.collision = collision_for_tile(tile.tile_id, ids);
        }

        Some(mined)
    }

    fn tile_at_mut(&mut self, x: i32, y: i32) -> Option<&mut TileCell> {
        let (coord, index) = chunk_coord_and_tile_index(x, y);

        self.chunks
            .get_mut(&coord)
            .and_then(|chunk| chunk.tiles.get_mut(index))
    }
}

fn chunk_coord_and_tile_index(x: i32, y: i32) -> (ChunkCoord, usize) {
    let coord = ChunkCoord {
        x: x.div_euclid(CHUNK_SIZE),
        y: y.div_euclid(CHUNK_SIZE),
    };
    let local_x = x.rem_euclid(CHUNK_SIZE) as usize;
    let local_y = y.rem_euclid(CHUNK_SIZE) as usize;
    let index = local_y * CHUNK_SIZE as usize + local_x;

    (coord, index)
}

impl EntityStore {
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    fn new_test_entities(seed: u64) -> Self {
        Self {
            entities: vec![SimEntity {
                id: 1,
                x: (seed % 97) as i64,
                y: (seed % 53) as i64,
            }],
        }
    }

    fn advance(&mut self, tick: Tick, seed: u64) {
        for entity in &mut self.entities {
            let step = splitmix64(seed ^ entity.id ^ tick.0);
            entity.x += ((step & 0b11) as i64) - 1;
            entity.y += (((step >> 2) & 0b11) as i64) - 1;
        }
    }
}

#[derive(Default)]
struct StableHasher {
    hash: u64,
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, bytes: &[u8]) {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        if self.hash == 0 {
            self.hash = FNV_OFFSET;
        }

        for byte in bytes {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(FNV_PRIME);
        }
    }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

fn generate_test_chunks(seed: u64, prototypes: &PrototypeCatalog) -> BTreeMap<ChunkCoord, Chunk> {
    let ids = WorldPrototypeIds::from_catalog(prototypes);
    let resource_map = generate_resource_map(
        seed,
        ids,
        TEST_WORLD_MIN_CHUNK * CHUNK_SIZE,
        TEST_WORLD_MAX_CHUNK * CHUNK_SIZE + CHUNK_SIZE - 1,
    );
    let mut chunks = BTreeMap::new();

    for chunk_y in TEST_WORLD_MIN_CHUNK..=TEST_WORLD_MAX_CHUNK {
        for chunk_x in TEST_WORLD_MIN_CHUNK..=TEST_WORLD_MAX_CHUNK {
            let coord = ChunkCoord {
                x: chunk_x,
                y: chunk_y,
            };
            chunks.insert(coord, generate_chunk(seed, coord, ids, &resource_map));
        }
    }

    chunks
}

fn generate_chunk(
    seed: u64,
    coord: ChunkCoord,
    ids: WorldPrototypeIds,
    resource_map: &BTreeMap<(i32, i32), ResourceCell>,
) -> Chunk {
    let mut tiles = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize);

    for local_y in 0..CHUNK_SIZE {
        for local_x in 0..CHUNK_SIZE {
            let x = coord.x * CHUNK_SIZE + local_x;
            let y = coord.y * CHUNK_SIZE + local_y;
            tiles.push(generate_tile(seed, x, y, ids, resource_map));
        }
    }

    Chunk { coord, tiles }
}

fn generate_tile(
    seed: u64,
    x: i32,
    y: i32,
    ids: WorldPrototypeIds,
    resource_map: &BTreeMap<(i32, i32), ResourceCell>,
) -> TileCell {
    let (tile_id, mut collision) = generate_terrain(seed, x, y, ids);
    let resource = resource_map.get(&(x, y)).copied();

    if resource.is_some() {
        collision = TileCollision {
            walkable: true,
            buildable: false,
            minable: true,
        };
    }

    TileCell {
        tile_id,
        collision,
        resource,
    }
}

fn generate_terrain(seed: u64, x: i32, y: i32, ids: WorldPrototypeIds) -> (TileId, TileCollision) {
    let terrain_hash = hash_world(seed, x, y);
    let terrain_roll = terrain_hash % 100;
    if terrain_roll < 10 {
        (
            ids.water,
            TileCollision {
                walkable: false,
                buildable: false,
                minable: false,
            },
        )
    } else if terrain_roll < 35 {
        (ids.dirt, ground_collision())
    } else {
        (ids.grass, ground_collision())
    }
}

fn ground_collision() -> TileCollision {
    TileCollision {
        walkable: true,
        buildable: true,
        minable: false,
    }
}

fn collision_for_tile(tile_id: TileId, ids: WorldPrototypeIds) -> TileCollision {
    if tile_id == ids.water {
        TileCollision {
            walkable: false,
            buildable: false,
            minable: false,
        }
    } else {
        ground_collision()
    }
}

fn generate_resource_map(
    seed: u64,
    ids: WorldPrototypeIds,
    min_tile: i32,
    max_tile: i32,
) -> BTreeMap<(i32, i32), ResourceCell> {
    let centers = generate_resource_patch_centers(seed, ids, min_tile, max_tile);
    let mut resources = BTreeMap::new();

    for y in min_tile..=max_tile {
        for x in min_tile..=max_tile {
            let (tile_id, _) = generate_terrain(seed, x, y, ids);
            if tile_id == ids.water {
                continue;
            }

            if let Some(resource) = resource_at_patch_tile(seed, x, y, &centers) {
                resources.insert((x, y), resource);
            }
        }
    }

    resources
}

fn generate_resource_patch_centers(
    seed: u64,
    ids: WorldPrototypeIds,
    min_tile: i32,
    max_tile: i32,
) -> Vec<ResourcePatchCenter> {
    let configs = resource_patch_configs(ids);
    let starting_offsets = [(-22, -14), (18, -12), (-16, 20), (20, 18)];
    let mut centers = Vec::new();

    for (index, config) in configs.iter().enumerate() {
        let (x, y) = starting_offsets[index];
        centers.push(ResourcePatchCenter {
            resource_item: config.resource_item,
            x,
            y,
            radius: config.radius,
            richness: config.richness,
        });
    }

    let min_grid = min_tile.div_euclid(RESOURCE_PATCH_GRID_SIZE) - 1;
    let max_grid = max_tile.div_euclid(RESOURCE_PATCH_GRID_SIZE) + 1;

    for grid_y in min_grid..=max_grid {
        for grid_x in min_grid..=max_grid {
            for config in configs {
                let hash = hash_resource_center(seed, grid_x, grid_y, config.resource_item);
                if hash % 100 >= u64::from(config.frequency_percent) {
                    continue;
                }

                let jitter_x = ((hash >> 8) % (RESOURCE_PATCH_GRID_JITTER * 2 + 1) as u64) as i32
                    - RESOURCE_PATCH_GRID_JITTER;
                let jitter_y = ((hash >> 16) % (RESOURCE_PATCH_GRID_JITTER * 2 + 1) as u64) as i32
                    - RESOURCE_PATCH_GRID_JITTER;

                centers.push(ResourcePatchCenter {
                    resource_item: config.resource_item,
                    x: grid_x * RESOURCE_PATCH_GRID_SIZE + RESOURCE_PATCH_GRID_SIZE / 2 + jitter_x,
                    y: grid_y * RESOURCE_PATCH_GRID_SIZE + RESOURCE_PATCH_GRID_SIZE / 2 + jitter_y,
                    radius: config.radius,
                    richness: config.richness,
                });
            }
        }
    }

    centers
}

fn resource_at_patch_tile(
    seed: u64,
    x: i32,
    y: i32,
    centers: &[ResourcePatchCenter],
) -> Option<ResourceCell> {
    let mut best: Option<ResourceCandidate> = None;

    for center in centers {
        let dx = x - center.x;
        let dy = y - center.y;
        let distance_sq = dx * dx + dy * dy;
        let radius = center.radius + resource_edge_noise(seed, x, y, center.resource_item);
        let radius_sq = radius * radius;

        if distance_sq > radius_sq {
            continue;
        }

        let score = radius_sq - distance_sq;
        if best.is_none_or(|candidate| score > candidate.score) {
            best = Some(ResourceCandidate {
                center: *center,
                distance_sq,
                radius_sq,
                score,
            });
        }
    }

    best.map(|candidate| {
        let radius_sq = candidate.radius_sq.max(1) as u32;
        let distance_sq = candidate.distance_sq.max(0) as u32;
        let falloff = (radius_sq - distance_sq).max(1);
        let base = candidate.center.richness / 3;
        let scaled = candidate.center.richness * falloff / radius_sq;
        let variation =
            (hash_world(seed ^ 0x1d17_5f2c_6b31_f011, x, y) % u64::from(base.max(1))) as u32;

        ResourceCell {
            resource_item: candidate.center.resource_item,
            amount: base + scaled + variation,
        }
    })
}

fn resource_patch_configs(ids: WorldPrototypeIds) -> [ResourcePatchConfig; 4] {
    [
        ResourcePatchConfig {
            resource_item: ids.resources[0],
            frequency_percent: 68,
            radius: 9,
            richness: 700,
        },
        ResourcePatchConfig {
            resource_item: ids.resources[1],
            frequency_percent: 62,
            radius: 8,
            richness: 650,
        },
        ResourcePatchConfig {
            resource_item: ids.resources[2],
            frequency_percent: 55,
            radius: 10,
            richness: 800,
        },
        ResourcePatchConfig {
            resource_item: ids.resources[3],
            frequency_percent: 48,
            radius: 7,
            richness: 520,
        },
    ]
}

fn resource_edge_noise(seed: u64, x: i32, y: i32, resource_item: ItemId) -> i32 {
    let hash = hash_world(
        seed ^ 0x7b5d_1f25_8c92_f6a3 ^ u64::from(resource_item.raw()),
        x,
        y,
    );
    (hash % (RESOURCE_PATCH_EDGE_NOISE * 2 + 1) as u64) as i32 - RESOURCE_PATCH_EDGE_NOISE
}

fn hash_resource_center(seed: u64, grid_x: i32, grid_y: i32, resource_item: ItemId) -> u64 {
    hash_world(
        seed ^ 0xa24b_aed4_963e_e407 ^ u64::from(resource_item.raw()).rotate_left(17),
        grid_x,
        grid_y,
    )
}

#[derive(Clone, Copy)]
struct ResourcePatchConfig {
    resource_item: ItemId,
    frequency_percent: u8,
    radius: i32,
    richness: u32,
}

#[derive(Clone, Copy)]
struct ResourcePatchCenter {
    resource_item: ItemId,
    x: i32,
    y: i32,
    radius: i32,
    richness: u32,
}

#[derive(Clone, Copy)]
struct ResourceCandidate {
    center: ResourcePatchCenter,
    distance_sq: i32,
    radius_sq: i32,
    score: i32,
}

fn hash_world(seed: u64, x: i32, y: i32) -> u64 {
    let x_bits = x as i64 as u64;
    let y_bits = y as i64 as u64;
    splitmix64(seed ^ x_bits.rotate_left(32) ^ y_bits.rotate_left(1))
}

#[derive(Clone, Copy)]
struct WorldPrototypeIds {
    grass: TileId,
    dirt: TileId,
    water: TileId,
    resources: [ItemId; 4],
}

impl WorldPrototypeIds {
    fn from_catalog(prototypes: &PrototypeCatalog) -> Self {
        Self {
            grass: tile_id(prototypes, "grass"),
            dirt: tile_id(prototypes, "dirt"),
            water: tile_id(prototypes, "water"),
            resources: [
                item_id(prototypes, "iron_ore"),
                item_id(prototypes, "copper_ore"),
                item_id(prototypes, "coal"),
                item_id(prototypes, "stone"),
            ],
        }
    }
}

fn tile_id(prototypes: &PrototypeCatalog, name: &str) -> TileId {
    prototypes
        .tiles
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required tile prototype {name:?}"))
}

fn item_id(prototypes: &PrototypeCatalog, name: &str) -> ItemId {
    prototypes
        .items
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required item prototype {name:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn world_tile_lookup_is_stable_across_chunk_boundaries() {
        let world = WorldSim::new_seeded(123);

        let left_of_origin = world.tile_at(-1, 0).expect("-1 should be in chunk -1");
        let previous_chunk_tile = world.tile_at(-33, 0).expect("-33 should be in chunk -2");
        let previous_chunk = world
            .chunks
            .get(&ChunkCoord { x: -2, y: 0 })
            .expect("previous negative chunk should exist");

        assert_eq!(
            left_of_origin,
            &world
                .chunks
                .get(&ChunkCoord { x: -1, y: 0 })
                .expect("left chunk should exist")
                .tiles[31]
        );
        assert!(world.tile_at(-32, 0).is_some());
        assert_eq!(previous_chunk_tile, &previous_chunk.tiles[31]);
    }

    #[test]
    fn generated_chunks_have_expected_shape() {
        let world = WorldSim::new_seeded(123);

        assert_eq!(world.chunks.len(), 16);
        for chunk in world.chunks.values() {
            assert_eq!(chunk.tiles.len(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
        }
    }

    #[test]
    fn resource_generation_is_deterministic() {
        let a = WorldSim::new_seeded(123);
        let b = WorldSim::new_seeded(123);

        assert_eq!(a.resource_hash(), b.resource_hash());
    }

    #[test]
    fn seed_123_contains_all_resource_item_types() {
        let world = WorldSim::new_seeded(123);
        let ids = WorldPrototypeIds::from_catalog(&world.prototypes);
        let resource_items = world
            .chunks
            .values()
            .flat_map(|chunk| chunk.tiles.iter())
            .filter_map(|tile| tile.resource.map(|resource| resource.resource_item))
            .collect::<BTreeSet<_>>();

        for resource_item in ids.resources {
            assert!(
                resource_items.contains(&resource_item),
                "missing generated resource item {resource_item:?}"
            );
        }
    }

    #[test]
    fn mining_decreases_resource_amount() {
        let mut world = WorldSim::new_seeded(123);
        let (x, y, before) = first_resource_tile(&world);

        let mined = world
            .mine_resource_at(x, y, 25)
            .expect("resource tile should be minable");
        let after = world
            .tile_at(x, y)
            .expect("mined tile should still exist")
            .resource
            .expect("resource should remain after partial mining");

        assert_eq!(mined.amount, 25);
        assert_eq!(after.amount, before.amount - 25);
        assert_eq!(after.resource_item, before.resource_item);
    }

    #[test]
    fn over_mining_clears_resource_tile() {
        let mut world = WorldSim::new_seeded(123);
        let (x, y, before) = first_resource_tile(&world);

        let mined = world
            .mine_resource_at(x, y, before.amount + 1)
            .expect("resource tile should be minable");
        let tile = world.tile_at(x, y).expect("mined tile should still exist");

        assert_eq!(mined.amount, before.amount);
        assert!(tile.resource.is_none());
        assert!(tile.collision.buildable);
        assert!(!tile.collision.minable);
    }

    #[test]
    fn resource_hash_changes_after_mining() {
        let mut world = WorldSim::new_seeded(123);
        let before_hash = world.resource_hash();
        let (x, y, _) = first_resource_tile(&world);

        world
            .mine_resource_at(x, y, 1)
            .expect("resource tile should be minable");

        assert_ne!(world.resource_hash(), before_hash);
    }

    fn first_resource_tile(world: &WorldSim) -> (i32, i32, ResourceCell) {
        for chunk in world.chunks.values() {
            for (index, tile) in chunk.tiles.iter().enumerate() {
                if let Some(resource) = tile.resource {
                    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                    return (
                        chunk.coord.x * CHUNK_SIZE + local_x,
                        chunk.coord.y * CHUNK_SIZE + local_y,
                        resource,
                    );
                }
            }
        }

        panic!("expected at least one resource tile");
    }
}
