use factory_data::{ItemId, PrototypeCatalog, TileId};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

pub const CHUNK_SIZE: i32 = 32;
const TEST_WORLD_MIN_CHUNK: i32 = -2;
const TEST_WORLD_MAX_CHUNK: i32 = 1;

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
    pub item_id: ItemId,
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
        let coord = ChunkCoord {
            x: x.div_euclid(CHUNK_SIZE),
            y: y.div_euclid(CHUNK_SIZE),
        };
        let local_x = x.rem_euclid(CHUNK_SIZE) as usize;
        let local_y = y.rem_euclid(CHUNK_SIZE) as usize;
        let index = local_y * CHUNK_SIZE as usize + local_x;

        self.chunks
            .get(&coord)
            .and_then(|chunk| chunk.tiles.get(index))
    }
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
    let mut chunks = BTreeMap::new();

    for chunk_y in TEST_WORLD_MIN_CHUNK..=TEST_WORLD_MAX_CHUNK {
        for chunk_x in TEST_WORLD_MIN_CHUNK..=TEST_WORLD_MAX_CHUNK {
            let coord = ChunkCoord {
                x: chunk_x,
                y: chunk_y,
            };
            chunks.insert(coord, generate_chunk(seed, coord, ids));
        }
    }

    chunks
}

fn generate_chunk(seed: u64, coord: ChunkCoord, ids: WorldPrototypeIds) -> Chunk {
    let mut tiles = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize);

    for local_y in 0..CHUNK_SIZE {
        for local_x in 0..CHUNK_SIZE {
            let x = coord.x * CHUNK_SIZE + local_x;
            let y = coord.y * CHUNK_SIZE + local_y;
            tiles.push(generate_tile(seed, x, y, ids));
        }
    }

    Chunk { coord, tiles }
}

fn generate_tile(seed: u64, x: i32, y: i32, ids: WorldPrototypeIds) -> TileCell {
    let terrain_hash = hash_world(seed, x, y);
    let terrain_roll = terrain_hash % 100;
    let (tile_id, mut collision) = if terrain_roll < 10 {
        (
            ids.water,
            TileCollision {
                walkable: false,
                buildable: false,
                minable: false,
            },
        )
    } else if terrain_roll < 35 {
        (
            ids.dirt,
            TileCollision {
                walkable: true,
                buildable: true,
                minable: false,
            },
        )
    } else {
        (
            ids.grass,
            TileCollision {
                walkable: true,
                buildable: true,
                minable: false,
            },
        )
    };

    let resource = if tile_id == ids.water {
        None
    } else {
        generate_resource(seed, x, y, ids.resources)
    };

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

fn generate_resource(seed: u64, x: i32, y: i32, resource_ids: [ItemId; 4]) -> Option<ResourceCell> {
    let resource_hash = hash_world(seed ^ 0xa24b_aed4_963e_e407, x, y);
    if resource_hash % 100 >= 8 {
        return None;
    }

    let item_id = resource_ids[((resource_hash >> 8) as usize) % resource_ids.len()];
    let amount = 200 + ((resource_hash >> 16) % 801) as u32;
    Some(ResourceCell { item_id, amount })
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

    #[test]
    fn world_tile_lookup_is_stable_across_chunk_boundaries() {
        let world = WorldSim::new_seeded(123);

        let left_of_origin = world.tile_at(-1, 0).expect("-1 should be in chunk -1");
        let regenerated = generate_tile(
            world.seed,
            -1,
            0,
            WorldPrototypeIds::from_catalog(&world.prototypes),
        );

        assert_eq!(left_of_origin, &regenerated);
        assert!(world.tile_at(-32, 0).is_some());

        let previous_chunk_tile = world.tile_at(-33, 0).expect("-33 should be in chunk -2");
        let previous_chunk = world
            .chunks
            .get(&ChunkCoord { x: -2, y: 0 })
            .expect("previous negative chunk should exist");
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
}
