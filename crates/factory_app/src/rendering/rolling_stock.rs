//! Draws rolling stock along the track it stands on, smoothed between fixed
//! ticks.
//!
//! A train moves further per tick than anything else on the map — a locomotive
//! at full speed covers well over a tile — so snapping to the simulation
//! position once per fixed tick reads as a visible stutter at 60 fps and worse
//! below it. Each sprite therefore keeps the last two sampled bodies and draws
//! between them, using the fixed-step overstep as the blend factor, exactly as
//! [`crate::rendering::robots`] and the belt items do.
//!
//! Interpolating between the previous and current tick rather than
//! extrapolating past the current one means a train is drawn up to one tick
//! behind the simulation and never in a place the simulation did not actually
//! produce, so a train never overshoots the buffer at the end of a line and
//! snaps back.
//!
//! The body is drawn between the piece's two ends rather than as a footprint
//! around its centre. Those ends come from the rail geometry, so a wagon on a
//! curve lies along the curve instead of across it — the same reason the
//! simulation keeps a position on an edge rather than a free `(x, y)`.

use bevy::prelude::*;
use factory_sim::{POSITION_SCALE, RailPoint, RollingStockId};
use std::collections::HashSet;

use crate::constants::TILE_SIZE;
use crate::map::resources::VisibleChunks;
use crate::rendering::colors::rolling_stock_color;
use crate::resources::SimResource;

/// Width of a body across the track, in world units. Narrower than the two
/// tiles the prototypes declare so two trains on parallel sidings read as two
/// trains rather than as one block.
const STOCK_BODY_WIDTH: f32 = TILE_SIZE * 1.2;
/// Above track, entities, and the rail overlay: a train runs over all of it,
/// and a locomotive hidden behind its own rails would be unreadable.
const STOCK_SPRITE_Z: f32 = 6.5;

/// One sampled body: where the piece was and which way it lay.
#[derive(Clone, Copy)]
struct StockBody {
    center: Vec2,
    /// Rotation of the body about its centre, from the line between its ends.
    angle: f32,
    length: f32,
}

#[derive(Component)]
pub(crate) struct RollingStockSprite {
    stock_id: RollingStockId,
    /// Simulation body at `synced_tick - 1` and at `synced_tick`. Frames
    /// between the two ticks are drawn along this pair.
    previous: StockBody,
    current: StockBody,
    synced_tick: u64,
}

/// Mirrors rolling stock into render sprites: advances the interpolation pair
/// once per simulation tick, despawns stock that was mined or left view, and
/// spawns newcomers.
pub(crate) fn sync_rolling_stock_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible: Res<VisibleChunks>,
    fixed_time: Option<Res<Time<Fixed>>>,
    mut sprites: Query<(Entity, &mut RollingStockSprite, &mut Transform, &mut Sprite)>,
) {
    let alpha = fixed_time
        .as_deref()
        .map_or(1.0, Time::<Fixed>::overstep_fraction)
        .clamp(0.0, 1.0);
    let sim = sim.read();
    let tick = sim.tick_count();
    let mut seen: HashSet<RollingStockId> = HashSet::new();

    for (entity, mut sprite, mut transform, mut image) in &mut sprites {
        let Some(body) = visible_body(&sim, sprite.stock_id, &visible) else {
            commands.entity(entity).despawn();
            continue;
        };

        if sprite.synced_tick != tick {
            // A jump of anything but one tick means the simulation was
            // replaced, loaded, or paused; blending across it would draw the
            // train sliding across the map, so the pair restarts instead.
            sprite.previous = if tick == sprite.synced_tick + 1 {
                sprite.current
            } else {
                body
            };
            sprite.current = body;
            sprite.synced_tick = tick;
        }

        apply_body(
            &mut transform,
            &mut image,
            blend(sprite.previous, sprite.current, alpha),
        );
        seen.insert(sprite.stock_id);
    }

    for stock in sim.rolling_stock() {
        if seen.contains(&stock.id) {
            continue;
        }
        let Some(body) = visible_body(&sim, stock.id, &visible) else {
            continue;
        };
        let color = sim
            .catalog()
            .entity(stock.prototype_id)
            .map(|prototype| rolling_stock_color(prototype.entity_kind))
            .unwrap_or_else(|| rolling_stock_color(factory_data::EntityKind::CargoWagon));

        let mut sprite = Sprite::from_color(color, Vec2::new(body.length, STOCK_BODY_WIDTH));
        let mut transform = Transform::default();
        apply_body(&mut transform, &mut sprite, body);
        commands.spawn((
            sprite,
            transform,
            RollingStockSprite {
                stock_id: stock.id,
                previous: body,
                current: body,
                synced_tick: tick,
            },
        ));
    }
}

fn apply_body(transform: &mut Transform, sprite: &mut Sprite, body: StockBody) {
    transform.translation = body.center.extend(STOCK_SPRITE_Z);
    transform.rotation = Quat::from_rotation_z(body.angle);
    sprite.custom_size = Some(Vec2::new(body.length, STOCK_BODY_WIDTH));
}

/// Blends two sampled bodies. The angle is blended as the shorter turn between
/// the two, so a piece running through the seam where an angle wraps past π
/// turns the short way rather than spinning all the way round.
fn blend(previous: StockBody, current: StockBody, alpha: f32) -> StockBody {
    let turn = (current.angle - previous.angle + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    StockBody {
        center: previous.center.lerp(current.center, alpha),
        angle: previous.angle + turn * alpha,
        length: previous.length + (current.length - previous.length) * alpha,
    }
}

/// The body of a piece of stock, or `None` when it is off screen or no longer
/// in the world.
fn visible_body(
    sim: &factory_sim::Simulation,
    stock_id: RollingStockId,
    visible: &VisibleChunks,
) -> Option<StockBody> {
    let (x, y) = sim.rolling_stock_tile(stock_id)?;
    if !factory_sim::ChunkCoord::from_tile(x, y)
        .is_some_and(|coord| visible.chunks.contains(&coord))
    {
        return None;
    }
    let (back, front) = sim.rolling_stock_body(stock_id)?;
    let (back, front) = (world_position(back), world_position(front));
    let along = front - back;
    Some(StockBody {
        center: back.midpoint(front),
        // A body whose ends coincide has no direction of its own; drawing it
        // unrotated is the harmless answer, and only a zero-length prototype
        // the loader rejects could produce one.
        angle: if along.length_squared() > f32::EPSILON {
            along.y.atan2(along.x)
        } else {
            0.0
        },
        length: along.length().max(TILE_SIZE * 0.5),
    })
}

/// Sub-tile world geometry in render coordinates, the same mapping the rail
/// overlay uses so a train and the track under it never disagree.
fn world_position(point: RailPoint) -> Vec2 {
    let scale = POSITION_SCALE as f32;
    Vec2::new(
        point.x as f32 / scale * TILE_SIZE,
        point.y as f32 / scale * TILE_SIZE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::Simulation;

    fn rolling_stock_app(sim: Simulation) -> App {
        let chunks = sim.world().chunks.keys().copied().collect();
        let mut app = App::new();
        app.insert_resource(SimResource::new(sim))
            .insert_resource(VisibleChunks {
                chunks,
                ..Default::default()
            })
            .insert_resource(Time::<Fixed>::from_hz(60.0))
            .add_systems(Update, sync_rolling_stock_rendering);
        app
    }

    fn sprite_positions(app: &mut App) -> Vec<Vec3> {
        let world = app.world_mut();
        let mut query = world.query::<(&RollingStockSprite, &Transform)>();
        let mut positions = query
            .iter(world)
            .map(|(sprite, transform)| (sprite.stock_id, transform.translation))
            .collect::<Vec<_>>();
        positions.sort_by_key(|(stock_id, _)| stock_id.raw());
        positions
            .into_iter()
            .map(|(_, translation)| translation)
            .collect()
    }

    #[test]
    fn every_piece_of_stock_gets_a_sprite_and_a_mined_one_loses_its() {
        let mut app = rolling_stock_app(Simulation::new_rolling_stock_fixture(4));
        app.update();
        let drawn = sprite_positions(&mut app).len();
        assert_eq!(drawn, 12, "four three-piece trains are twelve bodies");

        let mined = {
            let mut sim = app.world_mut().resource_mut::<SimResource>();
            let mut simulation = sim.write_for_tests();
            let stock_id = simulation
                .rolling_stock()
                .next()
                .expect("the fixture placed stock")
                .id;
            simulation
                .mine_rolling_stock(stock_id)
                .expect("a parked wagon comes off the rails");
            stock_id
        };
        app.update();

        assert_eq!(sprite_positions(&mut app).len(), drawn - 1);
        assert!(
            app.world()
                .resource::<SimResource>()
                .read()
                .rolling_stock_piece(mined)
                .is_none()
        );
    }

    /// Between two fixed ticks a sprite must move, and it must move along the
    /// segment the simulation actually produced rather than past its end.
    #[test]
    fn sprites_interpolate_between_fixed_ticks() {
        let mut app = rolling_stock_app(Simulation::new_rolling_stock_fixture(2));
        // Run the fixture up to speed first: a train still at a standstill
        // would pass an interpolation test by not moving at all.
        {
            let mut sim = app.world_mut().resource_mut::<SimResource>();
            let mut simulation = sim.write_for_tests();
            for _ in 0..120 {
                simulation.tick();
            }
        }
        app.update();
        let start = sprite_positions(&mut app);

        {
            let mut sim = app.world_mut().resource_mut::<SimResource>();
            sim.write_for_tests().tick();
        }
        app.update();
        assert_eq!(
            start,
            sprite_positions(&mut app),
            "a fresh tick starts the pair at its beginning"
        );

        let ticked = {
            let sim = app.world().resource::<SimResource>().read();
            let visible = app.world().resource::<VisibleChunks>();
            let mut bodies = sim
                .rolling_stock()
                .filter_map(|stock| {
                    visible_body(&sim, stock.id, visible).map(|body| (stock.id, body.center))
                })
                .collect::<Vec<_>>();
            bodies.sort_by_key(|(stock_id, _)| stock_id.raw());
            bodies
                .into_iter()
                .map(|(_, center)| center)
                .collect::<Vec<_>>()
        };

        let timestep = app.world().resource::<Time<Fixed>>().timestep();
        app.world_mut()
            .resource_mut::<Time<Fixed>>()
            .accumulate_overstep(timestep / 2);
        app.update();
        let halfway = sprite_positions(&mut app);

        assert_eq!(halfway.len(), ticked.len());
        let mut moved = 0;
        for ((start, halfway), ticked) in start.iter().zip(&halfway).zip(&ticked) {
            let (start, halfway) = (start.truncate(), halfway.truncate());
            if start.distance(*ticked) <= f32::EPSILON {
                continue;
            }
            moved += 1;
            assert!(
                halfway.distance(start.midpoint(*ticked)) < 0.01,
                "half an overstep should draw half of the segment"
            );
        }
        assert!(moved > 0, "the fixture's trains should be moving");
    }
}
