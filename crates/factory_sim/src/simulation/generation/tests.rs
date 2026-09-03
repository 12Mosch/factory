use super::*;

/// Terrain statistics over a broad region with the spawn-bias disk around
/// the origin excluded, so it measures the unbiased biome distribution:
/// how much water there is and how strongly it clumps (fraction of water
/// tiles with at least two orthogonal water neighbours — near zero for
/// independent per-tile rolls, high for coherent lakes). Sampling a large
/// area rather than one window keeps the fraction stable across seeds
/// instead of landing on a single continent or basin.
fn natural_water_stats(seed: u64, catalog: &PrototypeCatalog) -> (f64, f64) {
    let rules = WorldGenerator::from_catalog(catalog);
    // Comfortably beyond the spawn elevation bias's outer radius.
    const SPAWN_EXCLUSION: i64 = 160;
    let half_extent = 256;
    let mask_extent = half_extent + 1;
    let mask_width = (mask_extent * 2 + 1) as usize;
    let mut water_mask = Vec::with_capacity(mask_width * mask_width);
    for y in -mask_extent..=mask_extent {
        for x in -mask_extent..=mask_extent {
            let (_, collision) = generate_terrain(seed, x, y, &rules);
            water_mask.push(!collision.walkable && !collision.buildable);
        }
    }
    let is_water = |x: i64, y: i64| {
        let row = (y + mask_extent) as usize;
        let column = (x + mask_extent) as usize;
        water_mask[row * mask_width + column]
    };

    let mut total = 0u64;
    let mut water = 0u64;
    let mut clustered = 0u64;
    for y in -half_extent..=half_extent {
        for x in -half_extent..=half_extent {
            if x * x + y * y <= SPAWN_EXCLUSION * SPAWN_EXCLUSION {
                continue;
            }
            total += 1;
            if !is_water(x, y) {
                continue;
            }
            water += 1;
            let neighbours = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                .into_iter()
                .filter(|&(nx, ny)| is_water(nx, ny))
                .count();
            if neighbours >= 2 {
                clustered += 1;
            }
        }
    }

    let water_fraction = water as f64 / total as f64;
    let clustered_fraction = if water == 0 {
        0.0
    } else {
        clustered as f64 / water as f64
    };
    (water_fraction, clustered_fraction)
}

#[test]
fn terrain_water_forms_coherent_lakes() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    for seed in [0, 42, 123, 8675309] {
        let (water_fraction, clustered_fraction) = natural_water_stats(seed, &catalog);

        assert!(
            (0.02..0.45).contains(&water_fraction),
            "seed {seed}: water fraction {water_fraction:.3} outside expected range"
        );
        assert!(
            clustered_fraction > 0.8,
            "seed {seed}: only {clustered_fraction:.3} of water tiles sit in \
             coherent bodies; terrain looks like salt-and-pepper noise"
        );
    }
}

#[test]
fn climate_channels_are_independent() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let noise = catalog.world_generation().climate_noise;

    for seed in [0u64, 42, 123] {
        let mut elevation_eq_moisture = 0u64;
        let mut elevation_eq_temperature = 0u64;
        let mut total = 0u64;
        for y in -48..48i64 {
            for x in -48..48i64 {
                let elevation =
                    field_to_percent(climate_field(seed ^ ELEVATION_SALT, x, y, noise.elevation));
                let moisture =
                    field_to_percent(climate_field(seed ^ MOISTURE_SALT, x, y, noise.moisture));
                let temperature = field_to_percent(climate_field(
                    seed ^ TEMPERATURE_SALT,
                    x,
                    y,
                    noise.temperature,
                ));
                total += 1;
                if elevation == moisture {
                    elevation_eq_moisture += 1;
                }
                if elevation == temperature {
                    elevation_eq_temperature += 1;
                }
            }
        }

        // Independent channels drawn from distinct salts rarely land in the
        // same percent bucket; identical channels would match every tile.
        let moisture_agreement = elevation_eq_moisture as f64 / total as f64;
        let temperature_agreement = elevation_eq_temperature as f64 / total as f64;
        assert!(
            moisture_agreement < 0.2,
            "seed {seed}: elevation and moisture agree on {moisture_agreement:.3} of tiles; \
             the channels are not independent"
        );
        assert!(
            temperature_agreement < 0.2,
            "seed {seed}: elevation and temperature agree on {temperature_agreement:.3} of \
             tiles; the channels are not independent"
        );
    }
}

#[test]
fn biome_table_produces_varied_terrain() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let rules = WorldGenerator::from_catalog(&catalog);

    for seed in [0u64, 42, 123, 8675309] {
        let mut tiles = std::collections::BTreeSet::new();
        for y in -96..96i64 {
            for x in -96..96i64 {
                let (tile_id, _) = generate_terrain(seed, x, y, &rules);
                tiles.insert(tile_id);
            }
        }
        assert!(
            tiles.len() >= 4,
            "seed {seed}: only {} distinct biomes appear over the sample area; the climate \
             table is not producing variety",
            tiles.len()
        );
    }
}

#[test]
fn biome_selection_is_deterministic_per_seed() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let rules = WorldGenerator::from_catalog(&catalog);

    for &(x, y) in &[(0i64, 0i64), (17, -33), (-64, 50), (120, -8)] {
        let first = generate_terrain(777, x, y, &rules);
        let second = generate_terrain(777, x, y, &rules);
        assert_eq!(
            first.0, second.0,
            "generate_terrain must be deterministic at ({x}, {y})"
        );
    }
}

#[test]
fn spawn_area_terrain_is_open_ground_for_any_seed() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let rules = WorldGenerator::from_catalog(&catalog);
    let bias = rules
        .spawn_bias
        .expect("base catalog should derive a spawn bias");
    let radius = bias.inner_radius;

    for seed in [0, 42, 123, 8675309, 0xdead_beef] {
        for y in -radius..=radius {
            for x in -radius..=radius {
                if x * x + y * y > radius * radius {
                    continue;
                }
                let (_, collision) = generate_terrain(seed, x, y, &rules);
                assert!(
                    collision.walkable && collision.buildable,
                    "seed {seed}: tile ({x}, {y}) inside the spawn bias radius \
                     {radius} is not open ground"
                );
            }
        }
    }
}

#[test]
fn starting_patches_generate_their_resource_for_any_seed() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let rules = WorldGenerator::from_catalog(&catalog);
    let starting_patches: Vec<_> = catalog
        .world_generation()
        .resources
        .iter()
        .filter_map(|resource| resource.starting_patch)
        .collect();
    assert!(
        !starting_patches.is_empty(),
        "base catalog should configure starting patches"
    );

    for seed in [0, 42, 123, 8675309, 0xdead_beef] {
        for &offset in &starting_patches {
            let (x, y) = (i64::from(offset.x), i64::from(offset.y));
            let coord =
                ChunkCoord::from_tile(x, y).expect("starting patch centers are within chunk range");
            let bounds = GenerationBounds::for_chunk(coord);
            let centers = generate_resource_patch_centers(seed, &rules, bounds);
            let tile = generate_tile(seed, x, y, &rules, &centers);

            // An overlapping random patch may win the tile, but the spawn
            // bias guarantees some resource generates here instead of the
            // patch drowning in a lake.
            assert!(
                tile.resource.is_some(),
                "seed {seed}: no resource at starting patch center ({x}, {y})"
            );
        }
    }
}

#[test]
fn resource_edge_noise_is_coherent_across_neighbouring_tiles() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let rules = WorldGenerator::from_catalog(&catalog);
    let resource = rules
        .resources
        .first()
        .expect("base catalog should configure resources");

    for seed in [0, 42, 123, 8675309] {
        let mut pairs = 0u64;
        let mut equal = 0u64;
        let mut seen_min = i32::MAX;
        let mut seen_max = i32::MIN;
        for y in -96..96i64 {
            for x in -96..96i64 {
                let noise =
                    resource_edge_noise(seed, x, y, resource.resource_item, rules.edge_noise);
                assert!(
                    (-rules.edge_noise..=rules.edge_noise).contains(&noise),
                    "seed {seed}: edge noise {noise} at ({x}, {y}) outside \
                     [-{0}, {0}]",
                    rules.edge_noise
                );
                seen_min = seen_min.min(noise);
                seen_max = seen_max.max(noise);
                let right =
                    resource_edge_noise(seed, x + 1, y, resource.resource_item, rules.edge_noise);
                pairs += 1;
                if noise == right {
                    equal += 1;
                }
            }
        }

        // Independent per-tile hashing agrees with its neighbour about
        // 1/(2*edge_noise+1) of the time (~14% for edge_noise 3); a
        // coherent field agrees far more often.
        let equal_fraction = equal as f64 / pairs as f64;
        assert!(
            equal_fraction > 0.5,
            "seed {seed}: only {equal_fraction:.3} of neighbouring tiles share an \
             edge offset; patch borders look like per-tile fuzz"
        );
        assert!(
            seen_max - seen_min >= rules.edge_noise,
            "seed {seed}: edge noise spread [{seen_min}, {seen_max}] is too flat \
             to shape patch outlines"
        );
    }
}

#[test]
fn resource_richness_falls_smoothly_from_patch_center() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let resource_item = catalog
        .world_generation()
        .resources
        .first()
        .expect("base catalog should configure resources")
        .resource_item;
    let centers = [ResourcePatchCenter {
        resource_item,
        x: 0,
        y: 0,
        radius: 10,
        richness: 300,
    }];

    let amounts: Vec<_> = [0, 1, 5, 9]
        .into_iter()
        .map(|x| {
            resource_at_patch_tile(123, x, 0, &centers, 0)
                .expect("tile should be inside the patch")
                .amount
        })
        .collect();

    assert_eq!(amounts[0], 400);
    assert!(
        amounts.windows(2).all(|pair| pair[0] > pair[1]),
        "resource amounts should decrease monotonically from the center: {amounts:?}"
    );
}

#[test]
fn non_positive_effective_resource_radius_is_excluded() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let resource_item = catalog.world_generation().resources[0].resource_item;
    let centers = [ResourcePatchCenter {
        resource_item,
        x: 0,
        y: 0,
        radius: 0,
        richness: 300,
    }];

    assert_eq!(resource_at_patch_tile(123, 0, 0, &centers, 0), None);
}

#[test]
fn warp_offsets_are_coherent_and_span_their_range() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let rules = WorldGenerator::from_catalog(&catalog);
    let amplitude = i64::from(rules.climate_noise.elevation.scale / 2);
    assert!(
        amplitude > 0,
        "base catalog noise scale should be large enough to warp"
    );

    for seed in [0, 42, 123, 8675309] {
        let warp_seed = seed ^ WARP_X_SALT;
        let mut pairs = 0u64;
        let mut equal = 0u64;
        let mut seen_min = i64::MAX;
        let mut seen_max = i64::MIN;
        for y in -96..96i64 {
            for x in -96..96i64 {
                let offset = warp_offset(warp_seed, x, y, rules.climate_noise.elevation.scale);
                assert!(
                    (-amplitude..=amplitude).contains(&offset),
                    "seed {seed}: warp offset {offset} at ({x}, {y}) outside \
                     [-{amplitude}, {amplitude}]"
                );
                seen_min = seen_min.min(offset);
                seen_max = seen_max.max(offset);
                let right = warp_offset(warp_seed, x + 1, y, rules.climate_noise.elevation.scale);
                pairs += 1;
                if offset == right {
                    equal += 1;
                }
            }
        }

        // Independent per-tile hashing agrees with its neighbour about
        // 1/(2*amplitude+1) of the time; a coherent warp field agrees far
        // more often, which is what keeps warped shores connected.
        let equal_fraction = equal as f64 / pairs as f64;
        assert!(
            equal_fraction > 0.5,
            "seed {seed}: only {equal_fraction:.3} of neighbouring tiles share a \
             warp offset; the warp field is not coherent"
        );
        assert!(
            seen_max - seen_min >= amplitude,
            "seed {seed}: warp offset spread [{seen_min}, {seen_max}] is too flat \
             to reshape coastlines"
        );
    }
}

#[test]
fn domain_warp_displaces_the_terrain_field() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let rules = WorldGenerator::from_catalog(&catalog);

    for seed in [0, 42, 123, 8675309] {
        let mut total = 0u64;
        let mut moved = 0u64;
        for y in -96..96i64 {
            for x in -96..96i64 {
                let unwarped = terrain_field(
                    seed ^ TERRAIN_FIELD_SALT,
                    x,
                    y,
                    rules.climate_noise.elevation.scale,
                    rules.climate_noise.elevation.octaves,
                );
                let warped = warped_terrain_field(
                    seed,
                    x,
                    y,
                    rules.climate_noise.elevation.scale,
                    rules.climate_noise.elevation.octaves,
                );
                total += 1;
                if warped != unwarped {
                    moved += 1;
                }
            }
        }

        let moved_fraction = moved as f64 / total as f64;
        assert!(
            moved_fraction > 0.9,
            "seed {seed}: only {moved_fraction:.3} of tiles changed under domain \
             warping; the warp is a near no-op"
        );
    }
}

#[test]
fn chunk_generation_is_deterministic_and_seed_dependent() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let rules = WorldGenerator::from_catalog(&catalog);
    let coord = ChunkCoord { x: 0, y: 0 };

    assert_eq!(
        generate_chunk(123, coord, &rules),
        generate_chunk(123, coord, &rules)
    );
    assert_ne!(
        generate_chunk(123, coord, &rules),
        generate_chunk(124, coord, &rules)
    );
}

#[test]
fn resource_minability_lookup_preserves_first_matching_rule() {
    let mut catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let first = catalog.world_generation().resources[0];
    catalog.world_generation_mut().resources[0].extraction = ResourceExtraction::Solid;
    let mut duplicate = first;
    duplicate.extraction = ResourceExtraction::Fluid;
    catalog.world_generation_mut().resources.push(duplicate);

    let generator = WorldGenerator::from_catalog(&catalog);
    assert!(generator.resource_is_minable(first.resource_item));

    catalog.world_generation_mut().resources[0].extraction = ResourceExtraction::Fluid;
    catalog
        .world_generation_mut()
        .resources
        .last_mut()
        .expect("duplicate resource should exist")
        .extraction = ResourceExtraction::Solid;
    let generator = WorldGenerator::from_catalog(&catalog);
    assert!(!generator.resource_is_minable(first.resource_item));
}

#[test]
fn grid_cells_roll_density_then_select_one_resource_by_weight() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let rules = WorldGenerator::from_catalog(&catalog);
    let mut counts = vec![0u64; rules.resources.len()];
    let mut occupied = 0u64;
    let mut cells = 0u64;

    for grid_y in -64..64 {
        for grid_x in -64..64 {
            cells += 1;
            let Some(center) = resource_patch_center_for_grid_cell(123, &rules, grid_x, grid_y)
            else {
                continue;
            };
            let hash = hash_resource_center(123, grid_x, grid_y);
            let selected = select_resource_for_grid_cell(&rules.resources, hash)
                .expect("base resource weights should select one resource");
            assert_eq!(center.resource_item, selected.resource_item);

            let index = rules
                .resources
                .iter()
                .position(|resource| resource.resource_item == selected.resource_item)
                .expect("selected resource should be configured");
            counts[index] += 1;
            occupied += 1;
        }
    }

    let actual_density = occupied as f64 / cells as f64;
    let expected_density = f64::from(rules.patch_chance_percent) / 100.0;
    assert!(
        (actual_density - expected_density).abs() < 0.02,
        "patch density was {actual_density:.3}, expected about {expected_density:.3}"
    );

    let total_weight: u64 = rules
        .resources
        .iter()
        .map(|resource| u64::from(resource.selection_weight))
        .sum();
    for (resource, count) in rules.resources.iter().zip(counts) {
        let expected = f64::from(resource.selection_weight) / total_weight as f64;
        let actual = count as f64 / occupied as f64;
        assert!(
            (actual - expected).abs() < 0.025,
            "resource {:?} selected at {actual:.3}, expected about {expected:.3}",
            resource.resource_item
        );
    }
}

#[test]
fn patch_chance_and_selection_weights_handle_empty_cells() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let mut rules = WorldGenerator::from_catalog(&catalog);

    let mut recomposed_rules = rules.clone();
    recomposed_rules.resources[0].selection_weight = u32::MAX;
    for grid_y in -8..8 {
        for grid_x in -8..8 {
            assert_eq!(
                resource_patch_center_for_grid_cell(123, &rules, grid_x, grid_y).is_some(),
                resource_patch_center_for_grid_cell(123, &recomposed_rules, grid_x, grid_y)
                    .is_some(),
                "changing composition changed patch presence at ({grid_x}, {grid_y})"
            );
        }
    }

    rules.patch_chance_percent = 0;
    assert!(resource_patch_center_for_grid_cell(123, &rules, 4, -7).is_none());

    rules.patch_chance_percent = 100;
    assert!(resource_patch_center_for_grid_cell(123, &rules, 4, -7).is_some());

    for resource in &mut rules.resources {
        resource.selection_weight = 0;
    }
    assert!(resource_patch_center_for_grid_cell(123, &rules, 4, -7).is_none());
}

#[test]
fn grid_patch_richness_and_radius_scale_with_distance() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let rules = WorldGenerator::from_catalog(&catalog);
    let scaling = catalog
        .world_generation()
        .distance_scaling
        .expect("base catalog should configure distance scaling");

    // Far enough out that every candidate sits past the radius bonus cap.
    // Search several cells because the density roll intentionally leaves
    // some cells empty.
    let center = (500..520)
        .flat_map(|grid_y| (500..520).map(move |grid_x| (grid_x, grid_y)))
        .find_map(|(grid_x, grid_y)| {
            resource_patch_center_for_grid_cell(123, &rules, grid_x, grid_y)
        })
        .expect("expected a grid patch among the sampled far cells");
    let base = rules
        .resources
        .iter()
        .find(|resource| resource.resource_item == center.resource_item)
        .expect("center resource should be configured");
    assert!(
        center.richness > base.richness * 2,
        "richness {} at ({}, {}) should be well above base {}",
        center.richness,
        center.x,
        center.y,
        base.richness
    );
    assert_eq!(
        center.radius,
        base.radius + i64::from(scaling.max_radius_bonus_tiles),
        "radius bonus at ({}, {}) should be capped",
        center.x,
        center.y
    );
}

#[test]
fn starting_patches_keep_base_richness_and_radius() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let rules = WorldGenerator::from_catalog(&catalog);

    for resource in &rules.resources {
        let Some((x, y)) = resource.starting_patch else {
            continue;
        };
        let coord =
            ChunkCoord::from_tile(x, y).expect("starting patch centers are within chunk range");
        let bounds = GenerationBounds::for_chunk(coord);
        let centers = generate_resource_patch_centers(123, &rules, bounds);
        let center = centers
            .iter()
            .find(|center| {
                center.resource_item == resource.resource_item && center.x == x && center.y == y
            })
            .expect("starting patch center should be generated");

        assert_eq!(center.radius, resource.radius);
        assert_eq!(center.richness, resource.richness);
    }
}

#[test]
fn distance_scaled_patches_do_not_create_chunk_seams() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let rules = WorldGenerator::from_catalog(&catalog);

    for seed in [0, 123] {
        for coord in [ChunkCoord { x: 15, y: 0 }, ChunkCoord { x: -20, y: 20 }] {
            let bounds = GenerationBounds::for_chunk(coord);
            // Centers from a scan wide enough to include everything that
            // could possibly reach the chunk; any tile that differs means
            // the per-chunk scan missed a relevant center.
            let margin = i64::from(rules.grid_cell_size) * 3;
            let expanded = GenerationBounds {
                min_x: bounds.min_x - margin,
                max_x: bounds.max_x + margin,
                min_y: bounds.min_y - margin,
                max_y: bounds.max_y + margin,
            };
            let chunk_centers = generate_resource_patch_centers(seed, &rules, bounds);
            let expanded_centers = generate_resource_patch_centers(seed, &rules, expanded);

            for y in bounds.min_y..=bounds.max_y {
                for x in bounds.min_x..=bounds.max_x {
                    assert_eq!(
                        resource_at_patch_tile(seed, x, y, &chunk_centers, rules.edge_noise),
                        resource_at_patch_tile(seed, x, y, &expanded_centers, rules.edge_noise),
                        "seed {seed}: tile ({x}, {y}) differs between per-chunk \
                         and expanded center scans"
                    );
                }
            }
        }
    }
}

#[test]
fn retained_resource_candidates_can_affect_their_chunk() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let rules = WorldGenerator::from_catalog(&catalog);

    for seed in [0, 123, 987_654_321] {
        for coord in [
            ChunkCoord { x: -2, y: -2 },
            ChunkCoord { x: 0, y: 0 },
            ChunkCoord { x: 2, y: 2 },
            ChunkCoord { x: 17, y: -23 },
        ] {
            let bounds = GenerationBounds::for_chunk(coord);
            let centers = generate_resource_patch_centers(seed, &rules, bounds);

            assert!(
                centers
                    .iter()
                    .all(|center| resource_patch_can_affect_bounds(
                        *center,
                        bounds,
                        rules.edge_noise
                    )),
                "a retained candidate cannot affect chunk {coord:?} for seed {seed}"
            );
        }
    }
}
