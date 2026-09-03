use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation) struct GenerationBounds {
    pub(in crate::simulation) min_x: WorldTileCoord,
    pub(in crate::simulation) max_x: WorldTileCoord,
    pub(in crate::simulation) min_y: WorldTileCoord,
    pub(in crate::simulation) max_y: WorldTileCoord,
}

impl GenerationBounds {
    pub(in crate::simulation) fn for_chunk(coord: ChunkCoord) -> Self {
        let (min_x, min_y) = coord.min_tile();
        let max_offset = i64::from(CHUNK_SIZE - 1);
        Self {
            min_x,
            max_x: min_x + max_offset,
            min_y,
            max_y: min_y + max_offset,
        }
    }
}

pub(in crate::simulation) fn generate_world_chunks(
    seed: u64,
    prototypes: &PrototypeCatalog,
    generator: &WorldGenerator,
) -> BTreeMap<ChunkCoord, Chunk> {
    let area = prototypes.world_generation().starting_area;
    let mut chunks = BTreeMap::new();

    for chunk_y in area.min_chunk..=area.max_chunk {
        for chunk_x in area.min_chunk..=area.max_chunk {
            let coord = ChunkCoord {
                x: chunk_x,
                y: chunk_y,
            };
            chunks.insert(coord, generate_chunk(seed, coord, generator));
        }
    }

    chunks
}

pub(in crate::simulation) fn generate_chunk(
    seed: u64,
    coord: ChunkCoord,
    generator: &WorldGenerator,
) -> Chunk {
    let mut tiles = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize);
    let mut pollution_absorption_per_minute_milli = 0;
    let bounds = GenerationBounds::for_chunk(coord);
    let centers = generate_resource_patch_centers(seed, generator, bounds);

    for local_y in 0..CHUNK_SIZE {
        for local_x in 0..CHUNK_SIZE {
            let (x, y) = coord.tile_at(local_x, local_y);
            let tile = generate_tile(seed, x, y, generator, &centers);
            pollution_absorption_per_minute_milli +=
                generator.pollution_absorption_per_minute_milli(tile.tile_id);
            tiles.push(tile);
        }
    }

    Chunk {
        coord,
        tiles,
        pollution_absorption_per_minute_milli,
    }
}

pub(in crate::simulation) fn generate_tile(
    seed: u64,
    x: WorldTileCoord,
    y: WorldTileCoord,
    rules: &WorldGenerator,
    centers: &[ResourcePatchCenter],
) -> TileCell {
    let (tile_id, collision) = generate_terrain(seed, x, y, rules);
    // Resource patches only overlay ground-like terrain.
    let resource = if collision.walkable && collision.buildable {
        resource_at_patch_tile(seed, x, y, centers, rules.edge_noise)
    } else {
        None
    };

    TileCell {
        tile_id,
        collision: apply_resource_collision(rules, collision, resource),
        resource,
    }
}

/// Overlays a resource cell's collision rules onto terrain collision.
/// Generation and post-generation terrain writes share this so a paved
/// resource tile keeps behaving like a resource tile.
pub(in crate::simulation) fn apply_resource_collision(
    rules: &WorldGenerator,
    terrain: TileCollision,
    resource: Option<ResourceCell>,
) -> TileCollision {
    match resource {
        // Resource patches only overlay ground-like terrain.
        Some(resource) if terrain.walkable && terrain.buildable => TileCollision {
            walkable: true,
            buildable: false,
            // Fluid resources are extracted by pumpjacks, never mined.
            minable: rules.resource_is_minable(resource.resource_item),
        },
        _ => terrain,
    }
}
