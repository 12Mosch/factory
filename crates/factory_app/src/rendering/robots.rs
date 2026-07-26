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
use factory_sim::{ChunkCoord, Robot, RobotId};
use std::collections::HashSet;

use crate::constants::TILE_SIZE;
use crate::map::resources::VisibleChunks;
use crate::rendering::colors::robot_color;
use crate::resources::SimResource;

const ROBOT_SPRITE_SIZE: f32 = TILE_SIZE * 0.34;
/// Above entities, enemies, and the coverage overlay: robots fly over all of
/// it, and a robot hidden behind the roboport it is docking into would make the
/// docking moment unreadable.
const ROBOT_SPRITE_Z: f32 = 6.0;

#[derive(Component)]
pub(crate) struct RobotSprite {
    robot_id: RobotId,
    /// Simulation position at `synced_tick - 1` and at `synced_tick`, in world
    /// units. Frames between the two ticks are drawn along this segment.
    previous: Vec2,
    current: Vec2,
    synced_tick: u64,
}

/// Mirrors flying robots into render sprites: advances the interpolation
/// segment once per simulation tick, despawns robots that docked or left view,
/// and spawns newcomers.
pub(crate) fn sync_robot_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible: Res<VisibleChunks>,
    fixed_time: Option<Res<Time<Fixed>>>,
    mut sprites: Query<(Entity, &mut RobotSprite, &mut Transform)>,
) {
    let alpha = fixed_time
        .as_deref()
        .map_or(1.0, Time::<Fixed>::overstep_fraction)
        .clamp(0.0, 1.0);
    let sim = sim.read();
    let tick = sim.tick_count();
    let mut seen: HashSet<RobotId> = HashSet::new();

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
            // A jump of anything but one tick means the simulation was
            // replaced, loaded, or paused; blending across it would draw the
            // robot sliding across the map, so the segment restarts instead.
            sprite.previous = if tick == sprite.synced_tick + 1 {
                sprite.current
            } else {
                position
            };
            sprite.current = position;
            sprite.synced_tick = tick;
        }
        transform.translation = sprite
            .previous
            .lerp(sprite.current, alpha)
            .extend(ROBOT_SPRITE_Z);
        seen.insert(sprite.robot_id);
    }

    for robot in sim.robots() {
        if seen.contains(&robot.id) || !robot_is_visible(robot, &visible) {
            continue;
        }
        let position = robot_position(robot);
        commands.spawn((
            Sprite::from_color(robot_color(), Vec2::splat(ROBOT_SPRITE_SIZE)),
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

fn robot_position(robot: &Robot) -> Vec2 {
    let (x, y) = robot.position_tiles();
    Vec2::new(x * TILE_SIZE, y * TILE_SIZE)
}

fn robot_is_visible(robot: &Robot, visible: &VisibleChunks) -> bool {
    let (x, y) = robot.tile();
    ChunkCoord::from_tile(x, y).is_some_and(|coord| visible.chunks.contains(&coord))
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::Simulation;

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
}
