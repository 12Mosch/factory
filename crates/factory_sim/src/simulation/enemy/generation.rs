use super::*;

/// independent of terrain and resource noise.
const SPAWNER_PLACEMENT_SALT: u64 = 0x656e_656d_795f_6261;

impl Simulation {
    /// Rolls spawner placement for the exact chunks returned by world
    /// generation. Placement is a pure function of the world seed and chunk
    /// coordinate.
    pub(in crate::simulation) fn seed_enemy_spawners_in_chunks(&mut self, chunks: &[ChunkCoord]) {
        let Some(config) = self.world.prototypes.world_generation.enemy_bases else {
            return;
        };
        for &coord in chunks {
            if self.enemies.seeded_chunks.insert(coord) {
                self.try_place_spawner_in_chunk(coord, &config);
            }
        }
    }

    fn try_place_spawner_in_chunk(
        &mut self,
        coord: ChunkCoord,
        config: &EnemyBaseGenerationConfig,
    ) {
        let Some(gameplay) = self.gameplay().copied() else {
            return;
        };
        let (min_x, min_y) = coord.min_tile();
        let min_distance = i64::from(self.config.world.starting_safe_radius_tiles);

        let roll = splitmix64(
            self.world.seed ^ SPAWNER_PLACEMENT_SALT ^ hash_world(self.world.seed, min_x, min_y),
        );
        let density_chance =
            u64::from(config.frequency_percent) * u64::from(self.config.world.base_density_percent);
        if roll % 10_000 >= density_chance {
            return;
        }

        let Some(prototype) = self.world.prototypes.entity(config.spawner_entity) else {
            return;
        };
        let prototype_size = prototype.size;
        // Keep the footprint fully inside the chunk so seeding one chunk
        // never depends on whether its neighbors exist yet.
        let margin = 2;
        let span_x = i64::from(CHUNK_SIZE) - 2 * margin - i64::from(prototype_size.x);
        let span_y = i64::from(CHUNK_SIZE) - 2 * margin - i64::from(prototype_size.y);
        if span_x <= 0 || span_y <= 0 {
            return;
        }
        let anchor_x = min_x + margin + ((roll >> 8) % span_x as u64) as i64;
        let anchor_y = min_y + margin + ((roll >> 24) % span_y as u64) as i64;
        let id = self.enemies.allocate_base_id();
        let count_range =
            gameplay.generated_colony_max_spawners - gameplay.generated_colony_min_spawners + 1;
        let count =
            gameplay.generated_colony_min_spawners + ((roll >> 40) % u64::from(count_range)) as u8;
        self.enemies.bases.insert(
            id,
            EnemyBase {
                id,
                anchor: coord,
                spawners: BTreeSet::new(),
                creation_tick: self.tick,
                attack_budget_micro: 0,
                staged_units: BTreeSet::new(),
                staging_started_tick: None,
                next_raid_tick: next_scaled_tick(
                    self.tick,
                    gameplay.raid_cooldown_ticks,
                    self.config.runtime.raid_frequency_percent,
                ),
                next_expansion_tick: next_scaled_tick(
                    self.tick,
                    gameplay.expansion_interval_ticks,
                    self.config.runtime.expansion_frequency_percent,
                ),
                next_growth_tick: self.tick + u64::from(gameplay.outpost_growth_interval_ticks),
                pollution_contact: false,
            },
        );
        for index in 0..count {
            let site_roll = splitmix64(roll ^ u64::from(index).wrapping_mul(0x9e37_79b9));
            let radius = i64::from(gameplay.colony_spawner_radius_tiles);
            let dx = (site_roll % (radius as u64 * 2 + 1)) as i64 - radius;
            let dy = ((site_roll >> 16) % (radius as u64 * 2 + 1)) as i64 - radius;
            let x = (anchor_x + dx).clamp(
                min_x + margin,
                min_x + i64::from(CHUNK_SIZE) - margin - i64::from(prototype_size.x),
            );
            let y = (anchor_y + dy).clamp(
                min_y + margin,
                min_y + i64::from(CHUNK_SIZE) - margin - i64::from(prototype_size.y),
            );
            let footprint = EntityFootprint::from_size(
                x,
                y,
                prototype_size.x,
                prototype_size.y,
                Direction::North,
            );
            if footprint_intersects_starting_safe_radius(&footprint, min_distance) {
                continue;
            }
            self.enemies.placement_base = Some(id);
            let _ = placement::place(
                self,
                placement::EntityPlacementRequest {
                    prototype_id: config.spawner_entity,
                    x,
                    y,
                    direction: Direction::North,
                },
            );
            self.enemies.placement_base = None;
        }
        if self
            .enemies
            .bases
            .get(&id)
            .is_some_and(|base| base.spawners.is_empty())
        {
            self.enemies.bases.remove(&id);
        }
    }
}

fn footprint_intersects_starting_safe_radius(
    footprint: &EntityFootprint,
    radius: WorldTileCoord,
) -> bool {
    let max_x = footprint.x + i64::from(footprint.width) - 1;
    let max_y = footprint.y + i64::from(footprint.height) - 1;
    let nearest_x = 0_i64.clamp(footprint.x, max_x);
    let nearest_y = 0_i64.clamp(footprint.y, max_y);
    let distance_squared = i128::from(nearest_x) * i128::from(nearest_x)
        + i128::from(nearest_y) * i128::from(nearest_y);
    distance_squared < i128::from(radius) * i128::from(radius)
}
