use super::*;
use super::{incremental::update_map_pixels_incremental, pixels::add_newly_exposed_chunks};
use super::{repaint::refresh_painted_chunks, repaint::repaint_dirty_chunks};
use crate::map::resources::{MapChunkPaintState, MapTextureBounds};
use crate::rendering::map_texture::{UNREVEALED_PIXEL, generate_map_pixels_for_layer};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use factory_sim::{
    CHUNK_SIZE, ChunkCoord, Direction, EntityFootprint, ManualMiningTarget, Simulation, WorldSim,
};
use std::hint::black_box;

fn image_asset(width: u32, height: u32, data: Option<Vec<u8>>) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &UNREVEALED_PIXEL,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.data = data;
    image.sampler = ImageSampler::nearest();
    image
}

#[test]
fn incremental_update_matches_full_render_after_streaming_chunk() {
    let settings_variants = [
        MapDisplaySettings::default(),
        MapDisplaySettings {
            debug_reveal_all: false,
            show_chunk_grid: true,
            ..default()
        },
        MapDisplaySettings {
            debug_reveal_all: true,
            show_chunk_grid: true,
            ..default()
        },
    ];
    for settings in settings_variants {
        assert_incremental_update_matches_full_render_after_streaming_chunk(settings);
    }
}

#[test]
fn radar_reveal_marks_one_in_bounds_chunk_and_matches_full_render() {
    let mut catalog = factory_data::PrototypeCatalog::load_base().expect("base catalog");
    catalog.day_night_cycle = None;
    let radar = catalog
        .entities
        .iter_mut()
        .find(|prototype| prototype.name == "radar")
        .expect("radar prototype");
    radar
        .radar
        .as_mut()
        .expect("radar metadata")
        .nearby_scan_interval_ticks = u32::MAX;
    radar
        .radar
        .as_mut()
        .expect("radar metadata")
        .far_scan_interval_ticks = 60;
    radar
        .electric_energy_source
        .as_mut()
        .expect("radar electric source")
        .energy_usage_watts = 60_000;

    let mut sim = Simulation::new(123, catalog);
    let radar_id = place_solar_powered_radar(&mut sim);
    let footprint = sim
        .entities()
        .placed_entity(radar_id)
        .expect("radar remains placed")
        .footprint;
    let center =
        ChunkCoord::from_tile(footprint.x + 1, footprint.y + 1).expect("radar center chunk");

    let seed = sim.seed();
    for coord in [
        ChunkCoord {
            x: center.x - 6,
            y: center.y - 6,
        },
        ChunkCoord {
            x: center.x + 6,
            y: center.y + 6,
        },
    ] {
        move_player_to_tile(&mut sim, first_walkable_tile_in_chunk(seed, coord));
        for _ in 0..12 {
            sim.tick();
        }
    }
    assert!(
        sim.tick_count() < 60,
        "fixture setup must finish before the first far scan"
    );

    let target = ChunkCoord {
        x: center.x - 4,
        y: center.y + 4,
    };
    sim.ensure_chunk_generated(target);
    assert!(!sim.is_chunk_revealed(target));

    let settings = MapDisplaySettings::default();
    let layer = MapTextureLayer::Surface;
    let initial = generate_map_pixels_for_layer(&sim, &settings, layer);
    assert!(initial.bounds.contains_chunk(target));
    let initial_bounds = initial.bounds;
    let mut cache = MapLayerTextureCache {
        handle: Some(Handle::default()),
        bounds: Some(initial.bounds),
        pixels: Some(initial.data),
        dirty_regions: Default::default(),
        painted_chunks: Default::default(),
        last_chunk_revision: sim.world().chunk_revision(),
        last_resource_revision: sim.world().resource_revision(),
        last_terrain_revision: sim.world().terrain_revision(),
        last_revealed_revision: sim.revealed_revision(),
        last_debug_flags: (settings.debug_reveal_all, settings.show_chunk_grid),
        last_texture_update_tick: sim.tick_count(),
    };
    refresh_painted_chunks(&MapRasterizer::new(&sim, &settings, layer), &mut cache);
    let revision = sim.revealed_revision();

    const REVEAL_WAIT_LIMIT_TICKS: usize = 60;
    for _ in 0..REVEAL_WAIT_LIMIT_TICKS {
        if sim.is_chunk_revealed(target) {
            break;
        }
        sim.tick();
    }
    assert!(
        sim.is_chunk_revealed(target),
        "radar did not reveal {target:?} within {REVEAL_WAIT_LIMIT_TICKS} ticks; tick={}, radar_state={:?}, power_status={:?}",
        sim.tick_count(),
        factory_sim::entity_access::radar_state(&sim, radar_id),
        sim.entity_power_status(radar_id),
    );
    assert_eq!(
        sim.revealed_chunks_since(revision)
            .expect("exact radar reveal history")
            .collect::<Vec<_>>(),
        vec![target]
    );

    update_map_pixels_incremental(
        &MapRasterizer::new(&sim, &settings, layer),
        &mut cache,
        true,
    );
    let full = generate_map_pixels_for_layer(&sim, &settings, layer);
    assert_eq!(cache.bounds, Some(initial_bounds));
    assert_eq!(cache.pixels.as_deref(), Some(full.data.as_slice()));

    let mut expected = crate::map::resources::MapTextureDirtyRegions::default();
    expected.mark_world_chunk(initial_bounds, target);
    assert_eq!(cache.dirty_regions.rects(), expected.rects());
}

fn place_solar_powered_radar(sim: &mut Simulation) -> factory_sim::EntityId {
    let radar = factory_data::entity_prototype_id_by_name(sim.catalog(), "radar");
    let solar = factory_data::entity_prototype_id_by_name(sim.catalog(), "solar_panel");
    let pole = factory_data::entity_prototype_id_by_name(sim.catalog(), "small_electric_pole");

    let (x, y) = sim
        .world()
        .chunks
        .values()
        .flat_map(|chunk| {
            let (min_x, min_y) = chunk.coord.min_tile();
            (0..CHUNK_SIZE * CHUNK_SIZE).map(move |index| {
                (
                    min_x + i64::from(index % CHUNK_SIZE),
                    min_y + i64::from(index / CHUNK_SIZE),
                )
            })
        })
        .find(|&(x, y)| {
            let footprint = EntityFootprint {
                x,
                y,
                width: 7,
                height: 3,
            };
            sim.world().validate_entity_footprint(&footprint).is_ok()
                && footprint.tiles().into_iter().all(|(tile_x, tile_y)| {
                    sim.world()
                        .tile_at(tile_x, tile_y)
                        .is_some_and(|tile| tile.resource.is_none())
                })
                && [(radar, x, y), (pole, x + 3, y + 1), (solar, x + 4, y)]
                    .into_iter()
                    .all(|(prototype_id, x, y)| {
                        factory_sim::placement::validate(
                            sim,
                            factory_sim::placement::EntityPlacementRequest {
                                prototype_id,
                                x,
                                y,
                                direction: Direction::North,
                            },
                        )
                        .is_ok()
                    })
        })
        .expect("clear radar and solar fixture");

    let place = |sim: &mut Simulation, prototype_id, x, y| {
        factory_sim::placement::place(
            sim,
            factory_sim::placement::EntityPlacementRequest {
                prototype_id,
                x,
                y,
                direction: Direction::North,
            },
        )
        .expect("fixture entity should be placeable")
    };
    let radar_id = place(sim, radar, x, y);
    place(sim, pole, x + 3, y + 1);
    place(sim, solar, x + 4, y);
    sim.tick();
    let power_status = sim
        .entity_power_status(radar_id)
        .expect("solar-powered radar fixture should report power state");
    assert!(
        power_status.satisfaction_permyriad > 0,
        "solar-powered radar fixture is disconnected or unpowered: {power_status:?}"
    );
    radar_id
}

fn assert_incremental_update_matches_full_render_after_streaming_chunk(
    settings: MapDisplaySettings,
) {
    let layers = [MapTextureLayer::Surface, MapTextureLayer::Resources];
    let mut sim = Simulation::new_test_world(123);
    let mut caches = layers.map(|layer| {
        let initial = generate_map_pixels_for_layer(&sim, &settings, layer);
        let mut cache = MapLayerTextureCache {
            handle: Some(Handle::default()),
            bounds: Some(initial.bounds),
            pixels: Some(initial.data),
            dirty_regions: Default::default(),
            painted_chunks: Default::default(),
            last_chunk_revision: sim.world().chunk_revision(),
            last_resource_revision: sim.world().resource_revision(),
            last_terrain_revision: sim.world().terrain_revision(),
            last_revealed_revision: sim.revealed_revision(),
            last_debug_flags: (settings.debug_reveal_all, settings.show_chunk_grid),
            last_texture_update_tick: sim.tick_count(),
        };
        let rasterizer = MapRasterizer::new(&sim, &settings, layer);
        refresh_painted_chunks(&rasterizer, &mut cache);
        cache
    });

    let target_chunk = ChunkCoord { x: 0, y: -9 };
    let before_chunk_revision = sim.world().chunk_revision();
    let before_revealed_revision = sim.revealed_revision();
    assert!(
        !sim.world().chunks.contains_key(&target_chunk),
        "target chunk should start outside the generated world"
    );
    let target = first_walkable_tile_in_chunk(sim.seed(), target_chunk);
    move_player_to_tile(&mut sim, target);
    sim.tick();
    assert!(
        sim.world().chunks.contains_key(&target_chunk),
        "moving to the target should stream the chunk"
    );
    assert!(
        sim.world().chunk_revision() > before_chunk_revision,
        "streaming the target chunk should advance chunk revision"
    );
    assert!(
        sim.is_chunk_revealed(target_chunk),
        "ticking at the target should reveal the streamed chunk"
    );
    assert!(
        sim.revealed_revision() != before_revealed_revision,
        "revealing new chunks should advance the revealed revision"
    );

    for (layer, cache) in layers.iter().zip(caches.iter_mut()) {
        let rasterizer = MapRasterizer::new(&sim, &settings, *layer);
        update_map_pixels_incremental(&rasterizer, cache, true);

        let full = generate_map_pixels_for_layer(&sim, &settings, *layer);
        assert_eq!(
            cache.bounds,
            Some(full.bounds),
            "bounds for {layer:?} with {settings:?}"
        );
        assert_eq!(
            cache.pixels.as_deref(),
            Some(full.data.as_slice()),
            "pixels for {layer:?} with {settings:?}"
        );
    }
}

#[test]
fn terrain_rewrite_repaints_only_the_changed_surface_tile() {
    let mut sim = Simulation::new_test_world(123);
    let concrete = factory_data::BasePrototypeIds::from_catalog(sim.catalog())
        .tiles
        .concrete;
    let settings = MapDisplaySettings {
        debug_reveal_all: true,
        show_chunk_grid: false,
        ..default()
    };
    let layer = MapTextureLayer::Surface;
    let initial = generate_map_pixels_for_layer(&sim, &settings, layer);
    let bounds = initial.bounds;
    let mut images = Assets::<Image>::default();
    let handle = images.add(image_asset(bounds.width, bounds.height, None));
    let mut cache = MapLayerTextureCache {
        handle: Some(handle),
        bounds: Some(bounds),
        pixels: Some(initial.data),
        dirty_regions: Default::default(),
        painted_chunks: Default::default(),
        last_chunk_revision: sim.world().chunk_revision(),
        last_resource_revision: sim.world().resource_revision(),
        last_terrain_revision: sim.world().terrain_revision(),
        last_revealed_revision: sim.revealed_revision(),
        last_debug_flags: (settings.debug_reveal_all, settings.show_chunk_grid),
        last_texture_update_tick: sim.tick_count(),
    };
    refresh_painted_chunks(&MapRasterizer::new(&sim, &settings, layer), &mut cache);

    let target = *sim
        .world()
        .chunks
        .keys()
        .next()
        .expect("test world should have chunks");
    let (x, y) = target.tile_at(3, 5);
    sim.set_tile(x, y, concrete)
        .expect("generated tile should accept concrete");

    update_map_pixels_incremental(
        &MapRasterizer::new(&sim, &settings, layer),
        &mut cache,
        false,
    );

    let full = generate_map_pixels_for_layer(&sim, &settings, layer);
    assert_eq!(cache.bounds, Some(bounds));
    assert_eq!(
        cache.pixels.as_deref(),
        Some(full.data.as_slice()),
        "the incremental repaint must match a full rebuild"
    );

    let mut expected = crate::map::resources::MapTextureDirtyRegions::default();
    expected.mark_world_tile(bounds, x, y);
    assert_eq!(
        cache.dirty_regions.rects(),
        expected.rects(),
        "only the rewritten tile should be marked dirty"
    );
}

#[test]
fn changed_chunk_queues_chunk_rect_upload() {
    let sim = Simulation::new_test_world(123);
    let settings = MapDisplaySettings {
        debug_reveal_all: true,
        show_chunk_grid: false,
        ..default()
    };
    let layer = MapTextureLayer::Surface;
    let initial = generate_map_pixels_for_layer(&sim, &settings, layer);
    let mut images = Assets::<Image>::default();
    let handle = images.add(image_asset(
        initial.bounds.width,
        initial.bounds.height,
        None,
    ));
    let mut cache = MapLayerTextureCache {
        handle: Some(handle.clone()),
        bounds: Some(initial.bounds),
        pixels: Some(initial.data),
        dirty_regions: Default::default(),
        painted_chunks: Default::default(),
        last_chunk_revision: sim.world().chunk_revision(),
        last_resource_revision: sim.world().resource_revision(),
        last_terrain_revision: sim.world().terrain_revision(),
        last_revealed_revision: sim.revealed_revision(),
        last_debug_flags: (settings.debug_reveal_all, settings.show_chunk_grid),
        last_texture_update_tick: sim.tick_count(),
    };
    let rasterizer = MapRasterizer::new(&sim, &settings, layer);
    refresh_painted_chunks(&rasterizer, &mut cache);
    let target = *sim
        .world()
        .chunks
        .keys()
        .next()
        .expect("test world should have chunks");
    cache
        .painted_chunks
        .insert(target, MapChunkPaintState { revealed: false });

    repaint_dirty_chunks(&rasterizer, &mut cache, &[target]);
    let expected_rect = {
        let mut regions = crate::map::resources::MapTextureDirtyRegions::default();
        regions.mark_world_chunk(initial.bounds, target);
        regions.rects()[0]
    };
    assert_eq!(cache.dirty_regions.rects(), &[expected_rect]);

    let mut uploads = MapTextureUploadQueue::default();
    upload_layer_texture(&mut cache, &mut images, &mut uploads);

    assert_eq!(uploads.commands.len(), 1);
    assert_eq!(uploads.commands[0].rect, expected_rect);
    assert_eq!(
        uploads.commands[0].data.len(),
        expected_rect.width as usize * expected_rect.height as usize * 4
    );
    assert!(
        images
            .get(handle.id())
            .is_some_and(|image| image.data.is_none())
    );
}

#[test]
fn bounds_shift_marks_only_chunks_in_newly_exposed_strip() {
    let old_bounds = MapTextureBounds {
        min_x: 0,
        min_y: 0,
        width: 64,
        height: 64,
    };
    let new_bounds = MapTextureBounds {
        min_x: 16,
        ..old_bounds
    };
    let mut dirty_chunks = Vec::new();

    add_newly_exposed_chunks(old_bounds, new_bounds, &mut dirty_chunks);

    assert_eq!(
        dirty_chunks,
        vec![ChunkCoord { x: 2, y: 0 }, ChunkCoord { x: 2, y: 1 }]
    );
}

#[test]
#[ignore]
fn bench_incremental_update_on_bounds_growth() {
    const ITERATIONS: usize = 16;

    let mut base_sim = Simulation::new_test_world(123);
    for y in -15..=15 {
        for x in -15..=15 {
            base_sim.ensure_chunk_generated(ChunkCoord { x, y });
        }
    }
    let settings = MapDisplaySettings {
        debug_reveal_all: true,
        show_chunk_grid: false,
        ..default()
    };

    let initial = generate_map_pixels_for_layer(&base_sim, &settings, MapTextureLayer::Surface);
    let mut base_cache = MapLayerTextureCache {
        handle: Some(Handle::default()),
        bounds: Some(initial.bounds),
        pixels: Some(initial.data.clone()),
        dirty_regions: Default::default(),
        painted_chunks: Default::default(),
        last_chunk_revision: base_sim.world().chunk_revision(),
        last_resource_revision: base_sim.world().resource_revision(),
        last_terrain_revision: base_sim.world().terrain_revision(),
        last_revealed_revision: base_sim.revealed_revision(),
        last_debug_flags: (settings.debug_reveal_all, settings.show_chunk_grid),
        last_texture_update_tick: base_sim.tick_count(),
    };
    let rasterizer = MapRasterizer::new(&base_sim, &settings, MapTextureLayer::Surface);
    refresh_painted_chunks(&rasterizer, &mut base_cache);

    let mut samples = Vec::with_capacity(ITERATIONS);
    for iteration in 0..ITERATIONS {
        let mut sim = base_sim.clone();
        let mut cache = MapLayerTextureCache {
            handle: Some(Handle::default()),
            bounds: base_cache.bounds,
            pixels: Some(initial.data.clone()),
            dirty_regions: Default::default(),
            painted_chunks: base_cache.painted_chunks.clone(),
            last_chunk_revision: base_cache.last_chunk_revision,
            last_resource_revision: base_cache.last_resource_revision,
            last_terrain_revision: base_cache.last_terrain_revision,
            last_revealed_revision: base_cache.last_revealed_revision,
            last_debug_flags: base_cache.last_debug_flags,
            last_texture_update_tick: base_cache.last_texture_update_tick,
        };

        // Frontier chunk grows the texture bounds by one chunk column.
        sim.ensure_chunk_generated(ChunkCoord { x: 16, y: 0 });

        let started = std::time::Instant::now();
        let rasterizer = MapRasterizer::new(&sim, &settings, MapTextureLayer::Surface);
        update_map_pixels_incremental(&rasterizer, &mut cache, true);
        samples.push(started.elapsed());

        if iteration + 1 == ITERATIONS {
            let full = generate_map_pixels_for_layer(&sim, &settings, MapTextureLayer::Surface);
            assert_eq!(cache.bounds, Some(full.bounds));
            assert_eq!(cache.pixels.as_deref(), Some(full.data.as_slice()));
        }
    }

    samples.sort_unstable();
    let total = samples.iter().copied().sum::<std::time::Duration>();
    println!(
        "incremental update after bounds growth: avg {:?}, median {:?}, min {:?}, max {:?}",
        total / ITERATIONS as u32,
        samples[ITERATIONS / 2],
        samples[0],
        samples[ITERATIONS - 1]
    );
}

#[test]
#[ignore]
fn bench_resource_tile_partial_upload_vs_full_buffer_upload() {
    const ITERATIONS: usize = 64;

    let mut sim = Simulation::new_test_world(123);
    for y in -40..=40 {
        for x in -40..=40 {
            sim.ensure_chunk_generated(ChunkCoord { x, y });
        }
    }
    let settings = MapDisplaySettings {
        debug_reveal_all: true,
        show_chunk_grid: false,
        ..default()
    };
    let layer = MapTextureLayer::Resources;
    let resource_tile = resource_tile_with_minimum_amount(&sim, ITERATIONS as u32)
        .expect("large generated map should contain enough resource amount");
    move_player_to_tile(&mut sim, resource_tile);

    let initial = generate_map_pixels_for_layer(&sim, &settings, layer);
    let mut images = Assets::<Image>::default();
    let handle = images.add(image_asset(
        initial.bounds.width,
        initial.bounds.height,
        Some(initial.data.clone()),
    ));
    let mut cache = MapLayerTextureCache {
        handle: Some(handle),
        bounds: Some(initial.bounds),
        pixels: Some(initial.data),
        dirty_regions: Default::default(),
        painted_chunks: Default::default(),
        last_chunk_revision: sim.world().chunk_revision(),
        last_resource_revision: sim.world().resource_revision(),
        last_terrain_revision: sim.world().terrain_revision(),
        last_revealed_revision: sim.revealed_revision(),
        last_debug_flags: (settings.debug_reveal_all, settings.show_chunk_grid),
        last_texture_update_tick: sim.tick_count(),
    };
    let rasterizer = MapRasterizer::new(&sim, &settings, layer);
    refresh_painted_chunks(&rasterizer, &mut cache);

    let full_buffer_bytes = cache.pixels.as_ref().expect("pixels").len();
    let full_started = std::time::Instant::now();
    for _ in 0..ITERATIONS {
        let copied = cache.pixels.as_ref().expect("pixels").clone();
        black_box(copied);
    }
    let full_elapsed = full_started.elapsed();

    let mut dirty_elapsed = std::time::Duration::ZERO;
    for _ in 0..ITERATIONS {
        mine_one_resource(&mut sim, resource_tile);
        let dirty_started = std::time::Instant::now();
        let rasterizer = MapRasterizer::new(&sim, &settings, layer);
        update_map_pixels_incremental(&rasterizer, &mut cache, false);

        let mut uploads = MapTextureUploadQueue::default();
        upload_layer_texture(&mut cache, &mut images, &mut uploads);
        dirty_elapsed += dirty_started.elapsed();
        let dirty_upload_bytes = uploads
            .commands
            .iter()
            .map(|command| command.data.len())
            .sum::<usize>();
        assert_eq!(dirty_upload_bytes, 4);
        cache.last_resource_revision = sim.world().resource_revision();
    }

    println!(
        "texture size: {}x{}",
        initial.bounds.width, initial.bounds.height
    );
    println!("full buffer bytes: {full_buffer_bytes}");
    println!("dirty upload bytes: 4");
    println!("old simulated full upload packaging time: {full_elapsed:?}");
    println!(
        "incremental dirty update time: {dirty_elapsed:?} ({:?}/update)",
        dirty_elapsed / ITERATIONS as u32
    );
    println!(
        "byte reduction ratio: {:.2}x",
        full_buffer_bytes as f64 / 4.0
    );
    println!(
        "timing ratio: {:.2}x",
        full_elapsed.as_secs_f64() / dirty_elapsed.as_secs_f64()
    );
}

fn first_walkable_tile_in_chunk(seed: u64, coord: ChunkCoord) -> (i64, i64) {
    let mut world = WorldSim::new_seeded(seed);
    world.ensure_chunk_generated(coord);
    let (min_x, min_y) = coord.min_tile();
    for y in min_y..min_y + i64::from(CHUNK_SIZE) {
        for x in min_x..min_x + i64::from(CHUNK_SIZE) {
            if world
                .tile_at(x, y)
                .is_some_and(|tile| tile.collision.walkable)
            {
                return (x, y);
            }
        }
    }

    panic!("expected a walkable streamed tile");
}

fn move_player_to_tile(sim: &mut Simulation, tile: (i64, i64)) {
    let attempt_move = |sim: &mut Simulation| {
        let (player_x, player_y) = sim.player().position_tiles();
        sim.move_player_by_tiles(
            tile.0 as f32 + 0.5 - player_x,
            tile.1 as f32 + 0.5 - player_y,
        );
    };
    // Streaming each axis can consume up to three extra simulation ticks;
    // callers must include that observable cost in timing budgets.
    for _ in 0..3 {
        attempt_move(sim);
        if sim.player().tile_position() == tile {
            return;
        }
        sim.tick();
    }
    attempt_move(sim);
    assert_eq!(sim.player().tile_position(), tile);
}

fn resource_tile_with_minimum_amount(sim: &Simulation, minimum_amount: u32) -> Option<(i64, i64)> {
    sim.world()
        .chunks
        .values()
        .flat_map(|chunk| {
            chunk
                .tiles
                .iter()
                .enumerate()
                .filter_map(move |(index, tile)| {
                    let resource = tile.resource?;
                    if resource.amount < minimum_amount {
                        return None;
                    }
                    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                    Some(chunk.coord.tile_at(local_x, local_y))
                })
        })
        .next()
}

fn mine_one_resource(sim: &mut Simulation, tile: (i64, i64)) {
    let before = sim.world().resource_revision();
    let target = Some(ManualMiningTarget {
        x: tile.0,
        y: tile.1,
    });
    for _ in 0..1_000 {
        sim.update_manual_mining(target);
        if sim.world().resource_revision() != before {
            return;
        }
    }

    panic!("manual mining did not update resource revision");
}
