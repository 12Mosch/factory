//! Draws robots in flight above the world layer, smoothed between fixed ticks.
//!
//! Robots move further per tick than anything else on the map and they move in
//! straight lines, so snapping them to the simulation position once per fixed
//! tick — what [`crate::rendering::enemies`] does — reads as a visible stutter
//! at 60 fps and worse below it. Each sprite therefore keeps the last two
//! sampled positions and draws between them, using the fixed-step overstep as
//! the blend factor, the same way belt items are smoothed.
//!
//! Interpolating (between the previous and current tick) rather than
//! extrapolating (past the current one) means a robot is drawn up to one tick
//! behind the simulation and never in a position the simulation did not
//! actually produce, so a robot never overshoots its roboport and snaps back.

use bevy::prelude::*;
use factory_data::RobotKind;
use factory_sim::{Robot, RobotId};
use std::collections::{HashMap, HashSet};

use crate::constants::TILE_SIZE;
use crate::map::resources::VisibleChunks;
use crate::rendering::colors::{logistic_robot_color, robot_color};
use crate::resources::SimResource;

const ROBOT_SPRITE_SIZE: f32 = TILE_SIZE * 0.34;
/// Above entities, enemies, and the coverage overlay: robots fly over all of
/// it, and a robot hidden behind the roboport it is docking into would make the
/// docking moment unreadable.
const ROBOT_SPRITE_Z: f32 = 6.0;

#[derive(Component)]
#[allow(dead_code)] // Retained as render-world identity for diagnostics/tests.
pub(crate) struct RobotSprite {
    robot_id: RobotId,
    /// Simulation position at `synced_tick - 1` and at `synced_tick`, in world
    /// units. Frames between the two ticks are drawn along this segment.
    previous: Vec2,
    current: Vec2,
    synced_tick: u64,
}

#[derive(Default)]
pub(crate) struct RobotRenderState {
    initialized: bool,
    synced_tick: u64,
    visible_revision: u64,
    sim_replacement_revision: u64,
    entities: HashMap<RobotId, Entity>,
    visible_ids: HashSet<RobotId>,
    scratch_ids: Vec<RobotId>,
}

/// Mirrors flying robots into render sprites: advances the interpolation
/// segment once per simulation tick, despawns robots that docked or left view,
/// and spawns newcomers.
pub(crate) fn sync_robot_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible: Res<VisibleChunks>,
    fixed_time: Option<Res<Time<Fixed>>>,
    mut state: Local<RobotRenderState>,
    mut sprites: Query<(&mut RobotSprite, &mut Transform)>,
) {
    let state = &mut *state;
    let alpha = fixed_time
        .as_deref()
        .map_or(1.0, Time::<Fixed>::overstep_fraction)
        .clamp(0.0, 1.0);
    let replacement_revision = sim.replacement_revision();
    let sim = sim.read();
    let tick = sim.tick_count();
    let replaced = !state.initialized || state.sim_replacement_revision != replacement_revision;
    let tick_changed = !state.initialized || state.synced_tick != tick;
    let view_changed = !state.initialized || state.visible_revision != visible.revision;

    if replaced {
        for (_, render_entity) in state.entities.drain() {
            commands.entity(render_entity).despawn();
        }
    }

    if replaced || tick_changed || view_changed {
        state.visible_ids.clear();
        for &chunk in &visible.chunks {
            state
                .visible_ids
                .extend(sim.robot_ids_in_chunk(chunk).iter().copied());
        }

        state.scratch_ids.clear();
        for &robot_id in state.entities.keys() {
            if !state.visible_ids.contains(&robot_id) {
                state.scratch_ids.push(robot_id);
            }
        }
        while let Some(robot_id) = state.scratch_ids.pop() {
            if let Some(render_entity) = state.entities.remove(&robot_id) {
                commands.entity(render_entity).despawn();
            }
        }

        state.scratch_ids.extend(state.visible_ids.iter().copied());
        while let Some(robot_id) = state.scratch_ids.pop() {
            let Some(robot) = sim.robot(robot_id) else {
                continue;
            };
            let position = robot_position(robot);
            if let Some(&render_entity) = state.entities.get(&robot_id) {
                if tick_changed && let Ok((mut sprite, _)) = sprites.get_mut(render_entity) {
                    // A jump of anything but one tick means the simulation was
                    // replaced or loaded; never blend across worlds.
                    sprite.previous = if tick == sprite.synced_tick + 1 {
                        sprite.current
                    } else {
                        position
                    };
                    sprite.current = position;
                    sprite.synced_tick = tick;
                }
                continue;
            }

            let render_entity = commands
                .spawn((
                    Sprite::from_color(
                        robot_sprite_color(sim.catalog(), robot),
                        Vec2::splat(ROBOT_SPRITE_SIZE),
                    ),
                    Transform::from_translation(position.extend(ROBOT_SPRITE_Z)),
                    RobotSprite {
                        robot_id,
                        previous: position,
                        current: position,
                        synced_tick: tick,
                    },
                ))
                .id();
            state.entities.insert(robot_id, render_entity);
        }

        state.initialized = true;
        state.synced_tick = tick;
        state.visible_revision = visible.revision;
        state.sim_replacement_revision = replacement_revision;
    }

    // Interpolation remains frame-based, but is limited to the visible
    // registry. Equality guards keep unchanged transforms out of Bevy's
    // extraction/change-detection path (for example while paused).
    for &render_entity in state.entities.values() {
        let Ok((sprite, mut transform)) = sprites.get_mut(render_entity) else {
            continue;
        };
        let translation = sprite
            .previous
            .lerp(sprite.current, alpha)
            .extend(ROBOT_SPRITE_Z);
        if transform.translation != translation {
            transform.translation = translation;
        }
    }
}

/// Tint one robot is drawn in, taken from the role its item declares.
fn robot_sprite_color(catalog: &factory_data::PrototypeCatalog, robot: &Robot) -> Color {
    let kind = catalog
        .item(robot.item_id)
        .and_then(|item| item.robot)
        .map(|profile| profile.kind);
    match kind {
        Some(RobotKind::Logistic) => logistic_robot_color(),
        // A robot with no flight profile cannot happen in a validated world,
        // and drawing it as a construction robot is the harmless answer.
        Some(RobotKind::Construction) | None => robot_color(),
    }
}

fn robot_position(robot: &Robot) -> Vec2 {
    let (x, y) = robot.position_tiles();
    Vec2::new(x * TILE_SIZE, y * TILE_SIZE)
}

#[cfg(test)]
fn robot_is_visible(robot: &Robot, visible: &VisibleChunks) -> bool {
    let (x, y) = robot.tile();
    factory_sim::ChunkCoord::from_tile(x, y).is_some_and(|coord| visible.chunks.contains(&coord))
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::Simulation;
    use std::collections::HashSet;

    use crate::test_performance::{
        BENCHMARK_LOCK, collect_performance_stats, collect_prepared_performance_stats,
        measure_performance_sample, print_performance_stats,
    };

    const RENDER_BENCHMARK_ROBOTS: usize = 4_000;
    const RENDER_BENCHMARK_FRAMES: usize = 120;

    fn robot_app(sim: Simulation) -> App {
        let chunks = sim.world().chunks.keys().copied().collect();
        let mut app = App::new();
        app.insert_resource(SimResource::new(sim))
            .insert_resource(VisibleChunks {
                chunks,
                ..Default::default()
            })
            .insert_resource(Time::<Fixed>::from_hz(60.0))
            .add_systems(Update, sync_robot_rendering);
        app
    }

    fn sprite_positions(app: &mut App) -> Vec<Vec3> {
        let world = app.world_mut();
        let mut query = world.query::<(&RobotSprite, &Transform)>();
        let mut positions = query
            .iter(world)
            .map(|(sprite, transform)| (sprite.robot_id, transform.translation))
            .collect::<Vec<_>>();
        positions.sort_by_key(|(robot_id, _)| robot_id.raw());
        positions
            .into_iter()
            .map(|(_, translation)| translation)
            .collect()
    }

    #[test]
    fn every_flying_robot_gets_a_sprite_and_docked_ones_lose_theirs() {
        let mut app = robot_app(Simulation::new_robot_flight_fixture(8));
        app.update();
        assert_eq!(sprite_positions(&mut app).len(), 8);

        // Run until at least one errand has finished, so the despawn half of
        // the sync is exercised rather than only the spawn half.
        {
            let mut sim = app.world_mut().resource_mut::<SimResource>();
            let mut simulation = sim.write_for_tests();
            for _ in 0..3_000 {
                if simulation.robot_count() < 8 {
                    break;
                }
                simulation.tick();
            }
        }
        app.update();

        // Robots that docked or flew out of the visible chunks lose their
        // sprite; the ones still in view keep exactly one each.
        let visible_robots = {
            let sim = app.world().resource::<SimResource>().read();
            let visible = app.world().resource::<VisibleChunks>();
            assert!(
                sim.robot_count() < 8,
                "at least one errand should have finished"
            );
            sim.robots()
                .filter(|robot| robot_is_visible(robot, visible))
                .count()
        };
        assert_eq!(sprite_positions(&mut app).len(), visible_robots);
    }

    /// Between two fixed ticks a sprite must move, and it must move along the
    /// segment the simulation actually produced rather than past its end.
    #[test]
    fn sprites_interpolate_between_fixed_ticks() {
        let mut app = robot_app(Simulation::new_robot_flight_fixture(4));
        app.update();
        let start = sprite_positions(&mut app);

        {
            let mut sim = app.world_mut().resource_mut::<SimResource>();
            sim.write_for_tests().tick();
        }
        app.update();
        let after_tick = sprite_positions(&mut app);

        assert_eq!(
            start, after_tick,
            "a fresh tick starts the segment at its beginning"
        );

        // Where the ticked simulation actually put the robots: the far end of
        // the segment the sprites are blending along.
        let ticked = {
            let sim = app.world().resource::<SimResource>().read();
            sim.robots().map(robot_position).collect::<Vec<_>>()
        };
        assert_eq!(ticked.len(), start.len());

        let timestep = app.world().resource::<Time<Fixed>>().timestep();
        app.world_mut()
            .resource_mut::<Time<Fixed>>()
            .accumulate_overstep(timestep / 2);
        app.update();
        let halfway = sprite_positions(&mut app);

        for ((start, halfway), ticked) in start.iter().zip(&halfway).zip(&ticked) {
            let start = start.truncate();
            let halfway = halfway.truncate();
            assert!(
                start.distance(halfway) > f32::EPSILON,
                "a sprite should move between ticks instead of waiting for the next one"
            );
            assert!(
                halfway.distance(start.midpoint(*ticked)) < 0.001,
                "half an overstep should draw half of the segment"
            );
        }
    }

    /// Direct before/after comparison against the former per-frame algorithm.
    /// The offscreen case pins the regression that motivated the chunk index;
    /// the crowded case makes sure the optimized path is measured when it has
    /// thousands of transforms to interpolate as well.
    #[test]
    #[ignore = "manual dense/offscreen render-sync benchmark"]
    fn robot_render_sync_dense_and_offscreen_benchmark() {
        let _guard = BENCHMARK_LOCK
            .lock()
            .expect("benchmark lock should not poison");
        let sim = Simulation::new_robot_flight_fixture(RENDER_BENCHMARK_ROBOTS);
        let crowded = sim
            .world()
            .chunks
            .keys()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let offscreen_chunk = sim
            .world()
            .chunks
            .keys()
            .copied()
            .min_by_key(|chunk| sim.robot_ids_in_chunk(*chunk).len())
            .expect("robot fixture should have generated chunks");
        let offscreen = std::collections::BTreeSet::from([offscreen_chunk]);

        let mut optimized_offscreen = robot_benchmark_app(sim.clone(), offscreen.clone(), false);
        let mut baseline_offscreen = robot_benchmark_app(sim.clone(), offscreen, true);
        let mut optimized_crowded = robot_benchmark_app(sim.clone(), crowded.clone(), false);
        let mut baseline_crowded = robot_benchmark_app(sim, crowded, true);
        for app in [
            &mut optimized_offscreen,
            &mut baseline_offscreen,
            &mut optimized_crowded,
            &mut baseline_crowded,
        ] {
            app.update();
            for _ in 0..10 {
                app.update();
            }
        }

        let optimized_offscreen_unchanged =
            collect_performance_stats(RENDER_BENCHMARK_FRAMES, || optimized_offscreen.update());
        let baseline_offscreen_unchanged =
            collect_performance_stats(RENDER_BENCHMARK_FRAMES, || baseline_offscreen.update());
        let optimized_crowded_unchanged =
            collect_performance_stats(RENDER_BENCHMARK_FRAMES, || optimized_crowded.update());
        let baseline_crowded_unchanged =
            collect_performance_stats(RENDER_BENCHMARK_FRAMES, || baseline_crowded.update());
        let optimized_offscreen_ticked =
            collect_prepared_performance_stats(RENDER_BENCHMARK_FRAMES, || {
                tick_robot_benchmark(&mut optimized_offscreen);
                measure_performance_sample(|| optimized_offscreen.update())
            });
        let baseline_offscreen_ticked =
            collect_prepared_performance_stats(RENDER_BENCHMARK_FRAMES, || {
                tick_robot_benchmark(&mut baseline_offscreen);
                measure_performance_sample(|| baseline_offscreen.update())
            });

        print_performance_stats(
            "robot_render_optimized_offscreen_unchanged",
            optimized_offscreen_unchanged,
        );
        print_performance_stats(
            "robot_render_baseline_offscreen_unchanged",
            baseline_offscreen_unchanged,
        );
        print_performance_stats(
            "robot_render_optimized_crowded_unchanged",
            optimized_crowded_unchanged,
        );
        print_performance_stats(
            "robot_render_baseline_crowded_unchanged",
            baseline_crowded_unchanged,
        );
        print_performance_stats(
            "robot_render_optimized_offscreen_ticked",
            optimized_offscreen_ticked,
        );
        print_performance_stats(
            "robot_render_baseline_offscreen_ticked",
            baseline_offscreen_ticked,
        );
    }

    fn robot_benchmark_app(
        sim: Simulation,
        chunks: std::collections::BTreeSet<factory_sim::ChunkCoord>,
        baseline: bool,
    ) -> App {
        let mut app = App::new();
        app.insert_resource(SimResource::new(sim))
            .insert_resource(VisibleChunks {
                chunks,
                ..Default::default()
            })
            .insert_resource(Time::<Fixed>::from_hz(60.0));
        if baseline {
            app.add_systems(Update, legacy_sync_robot_rendering);
        } else {
            app.add_systems(Update, sync_robot_rendering);
        }
        app
    }

    fn tick_robot_benchmark(app: &mut App) {
        app.world_mut()
            .resource_mut::<SimResource>()
            .write_for_tests()
            .tick();
    }

    fn legacy_sync_robot_rendering(
        mut commands: Commands,
        sim: Res<SimResource>,
        visible: Res<VisibleChunks>,
        mut sprites: Query<(Entity, &mut RobotSprite, &mut Transform)>,
    ) {
        let sim = sim.read();
        let tick = sim.tick_count();
        let mut seen = HashSet::new();
        for (entity, mut sprite, mut transform) in &mut sprites {
            let Some(robot) = sim
                .robot(sprite.robot_id)
                .filter(|robot| robot_is_visible(robot, &visible))
            else {
                commands.entity(entity).despawn();
                continue;
            };
            let position = robot_position(robot);
            if sprite.synced_tick != tick {
                sprite.previous = if tick == sprite.synced_tick + 1 {
                    sprite.current
                } else {
                    position
                };
                sprite.current = position;
                sprite.synced_tick = tick;
            }
            transform.translation = position.extend(ROBOT_SPRITE_Z);
            seen.insert(sprite.robot_id);
        }
        for robot in sim.robots() {
            if seen.contains(&robot.id) || !robot_is_visible(robot, &visible) {
                continue;
            }
            let position = robot_position(robot);
            commands.spawn((
                Sprite::from_color(
                    robot_sprite_color(sim.catalog(), robot),
                    Vec2::splat(ROBOT_SPRITE_SIZE),
                ),
                Transform::from_translation(position.extend(ROBOT_SPRITE_Z)),
                RobotSprite {
                    robot_id: robot.id,
                    previous: position,
                    current: position,
                    synced_tick: tick,
                },
            ));
        }
    }
}
