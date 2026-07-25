//! Draws circuit wires between connected entities.
//!
//! Wires are thin rotated sprites rather than entity-attached decorations,
//! because a wire belongs to a pair of entities and has to survive either one
//! scrolling out of view.

use bevy::prelude::*;
use factory_sim::{CircuitNode, CircuitWire, ConnectorPort, Simulation, WireColor};
use std::collections::HashMap;

use crate::constants::TILE_SIZE;
use crate::map::resources::VisibleChunks;
use crate::rendering::colors::circuit_wire_color;
use crate::resources::SimResource;

/// Above entity sprites so wires stay readable over dense builds.
const WIRE_Z: f32 = 9.0;
const WIRE_THICKNESS: f32 = 2.0;
/// Perpendicular offset applied per color so a red and a green wire between
/// the same pair of entities do not overdraw each other.
const WIRE_COLOR_SEPARATION: f32 = 2.5;
/// Offset applied to a combinator's input and output connectors so the two
/// ports are visually distinct on a one-tile-wide body.
const PORT_SEPARATION: f32 = TILE_SIZE * 0.3;

#[derive(Component)]
pub(crate) struct CircuitWireSprite;

/// Last-synced revisions. Wires only change when entities or their
/// connections change, both of which bump the entity topology revision.
#[derive(Resource, Default)]
pub(crate) struct CircuitWireRenderState {
    synced: Option<(u64, u64)>,
}

pub(crate) fn sync_circuit_wire_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible: Res<VisibleChunks>,
    mut state: ResMut<CircuitWireRenderState>,
    existing: Query<Entity, With<CircuitWireSprite>>,
) {
    let sim = sim.read();
    let revisions = (sim.entity_topology_revision(), visible.revision);
    if state.synced == Some(revisions) {
        return;
    }
    state.synced = Some(revisions);

    // Wires are rare compared with entities, so a full respawn per change is
    // cheaper than tracking per-wire sprite identity.
    for entity in &existing {
        commands.entity(entity).despawn();
    }

    let Some(bounds) = visible.tile_bounds else {
        return;
    };
    let max_x = bounds.min_x + i64::from(bounds.width) - 1;
    let max_y = bounds.min_y + i64::from(bounds.height) - 1;
    let wires = sim.circuit_wires_in_tile_rect(bounds.min_x, max_x, bounds.min_y, max_y);
    if wires.is_empty() {
        return;
    }

    // Count wires per entity pair so the two colors can be pushed apart only
    // where they would actually overlap.
    let mut pair_colors: HashMap<(CircuitNode, CircuitNode), u8> = HashMap::new();
    for wire in &wires {
        *pair_colors.entry((wire.first, wire.second)).or_default() |= 1 << wire.color.index();
    }

    for wire in &wires {
        let Some((start, end)) = wire_endpoints(&sim, wire) else {
            continue;
        };
        let shared_pair = pair_colors
            .get(&(wire.first, wire.second))
            .is_some_and(|mask| mask.count_ones() > 1);
        let (start, end) = if shared_pair {
            offset_for_color(start, end, wire.color)
        } else {
            (start, end)
        };

        let delta = end - start;
        let length = delta.length();
        if length <= f32::EPSILON {
            continue;
        }
        commands.spawn((
            Sprite::from_color(
                circuit_wire_color(wire.color),
                Vec2::new(length, WIRE_THICKNESS),
            ),
            Transform::from_translation(((start + end) * 0.5).extend(WIRE_Z))
                .with_rotation(Quat::from_rotation_z(delta.y.atan2(delta.x))),
            CircuitWireSprite,
        ));
    }
}

/// World-space attachment points of a wire's two connectors.
fn wire_endpoints(sim: &Simulation, wire: &CircuitWire) -> Option<(Vec2, Vec2)> {
    Some((
        connector_position(sim, wire.first)?,
        connector_position(sim, wire.second)?,
    ))
}

pub(crate) fn connector_position(sim: &Simulation, node: CircuitNode) -> Option<Vec2> {
    let placed = sim.entities().placed_entity(node.entity_id)?;
    let center = Vec2::new(
        placed.footprint.x as f32 * TILE_SIZE + placed.footprint.width as f32 * TILE_SIZE * 0.5,
        placed.footprint.y as f32 * TILE_SIZE + placed.footprint.height as f32 * TILE_SIZE * 0.5,
    );
    Some(match node.port {
        ConnectorPort::Single => center,
        ConnectorPort::Input => center - Vec2::new(0.0, PORT_SEPARATION),
        ConnectorPort::Output => center + Vec2::new(0.0, PORT_SEPARATION),
    })
}

/// Shifts a segment sideways so parallel wires of different colors stay apart.
fn offset_for_color(start: Vec2, end: Vec2, color: WireColor) -> (Vec2, Vec2) {
    let Some(direction) = (end - start).try_normalize() else {
        return (start, end);
    };
    let normal = Vec2::new(-direction.y, direction.x);
    let sign = match color {
        WireColor::Red => 1.0,
        WireColor::Green => -1.0,
    };
    let offset = normal * (WIRE_COLOR_SEPARATION * sign);
    (start + offset, end + offset)
}
