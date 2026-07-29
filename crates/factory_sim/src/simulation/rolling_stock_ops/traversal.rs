//! Moving a point along the rail graph, and turning one back into world space.
//!
//! Everything that asks "where would this be after travelling `d`?" goes
//! through [`travel`], including the per-tick step, the coupling search, and
//! the placement fit check. One walk means a train, the piece behind it, and
//! the rule that says they fit all agree about what the track does — a second
//! implementation is exactly how a wagon ends up half a tile out of line on a
//! curve.
//!
//! The walk is deliberately not a search. An end joins at most one other end
//! ([`RailGraph::neighbor_end`]), so there is never a branch to choose between:
//! travel either continues onto the one joined rail or stops at a free end.
//! Route choice belongs to pathfinding, which does not exist yet.

use crate::rail::{RailCurve, RailPoint};
use crate::rolling_stock::RailPosition;
use crate::simulation::rail_ops::RailGraph;
use crate::simulation::*;

/// Where a walk along the track ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation) struct TravelOutcome {
    pub(in crate::simulation) position: RailPosition,
    /// Distance that could not be travelled because the track ran out. Zero
    /// whenever the walk covered the whole request.
    pub(in crate::simulation) blocked_fixed: i64,
}

impl TravelOutcome {
    pub(in crate::simulation) const fn is_blocked(&self) -> bool {
        self.blocked_fixed != 0
    }
}

/// Moves `position` by `distance_fixed` along the track it faces.
///
/// A negative distance travels backwards without turning the position around,
/// which is what lets one train's stock be advanced by a single signed step
/// whichever way it is driving.
///
/// Travel stops at a free rail end rather than running past it, and reports how
/// much was left over. A caller that must keep a train rigid — every piece
/// moving the same distance — uses that to clip the whole train instead of
/// letting the blocked piece fall behind.
pub(in crate::simulation) fn travel(
    graph: &RailGraph,
    position: RailPosition,
    distance_fixed: i64,
) -> TravelOutcome {
    if distance_fixed < 0 {
        // Travelling backwards is travelling forwards on the reversed position:
        // one walk, and no second set of end conditions to keep in step with
        // the first.
        let outcome = travel(graph, position.reversed(), -distance_fixed);
        return TravelOutcome {
            position: outcome.position.reversed(),
            blocked_fixed: -outcome.blocked_fixed,
        };
    }

    let mut current = position;
    let mut remaining = distance_fixed;
    while remaining > 0 {
        let Some(edge) = graph.edge_for_entity(current.edge) else {
            // The rail under the stock is gone. Reporting the whole request as
            // blocked leaves the position untouched, which is what a validated
            // world's pruning pass then cleans up.
            return TravelOutcome {
                position: current,
                blocked_fixed: remaining,
            };
        };
        let exit_end = usize::from(current.forward);
        let to_end = if current.forward {
            edge.length_fixed - current.distance_fixed
        } else {
            current.distance_fixed
        };

        if remaining <= to_end {
            let distance_fixed = if current.forward {
                current.distance_fixed + remaining
            } else {
                current.distance_fixed - remaining
            };
            return TravelOutcome {
                position: RailPosition {
                    distance_fixed,
                    ..current
                },
                blocked_fixed: 0,
            };
        }

        remaining -= to_end;
        let Some((next_index, arrival_end)) = graph.neighbor_end(edge, exit_end) else {
            // A free end: the train stops here with the rest of its step unpaid.
            return TravelOutcome {
                position: RailPosition {
                    distance_fixed: if current.forward {
                        edge.length_fixed
                    } else {
                        0
                    },
                    ..current
                },
                blocked_fixed: remaining,
            };
        };
        let next = &graph.edges[next_index];
        // Entering through the neighbour's end 0 means starting at distance
        // zero and running up it; entering through end 1 means starting at its
        // far end and running back down.
        let forward = arrival_end == 0;
        current = RailPosition {
            edge: next.entity_id,
            distance_fixed: if forward { 0 } else { next.length_fixed },
            forward,
        };
    }

    TravelOutcome {
        position: current,
        blocked_fixed: 0,
    }
}

/// Reports every rail a run of track covers, from `position` forward over
/// `distance_fixed`, in travel order and starting with the rail `position` is
/// on.
///
/// The same walk [`travel`] makes, asking the other question about it: travel
/// answers where a stretch of track *ends*, and this answers what it *crosses*.
/// A piece of rolling stock is longer than a rail piece, so "which rails is this
/// wagon standing on" has more than one answer and neither end of it is
/// necessarily one of them.
///
/// Stops at a free end, like every other walk here, and visits each rail once
/// per time the run covers it.
pub(in crate::simulation) fn edges_along(
    graph: &RailGraph,
    position: RailPosition,
    distance_fixed: i64,
    mut visit: impl FnMut(EntityId),
) {
    let mut current = position;
    let mut covered = 0_i64;
    loop {
        visit(current.edge);
        let Some(edge) = graph.edge_for_entity(current.edge) else {
            return;
        };
        covered += if current.forward {
            edge.length_fixed - current.distance_fixed
        } else {
            current.distance_fixed
        };
        if covered >= distance_fixed {
            return;
        }
        let Some((next_index, arrival_end)) =
            graph.neighbor_end(edge, usize::from(current.forward))
        else {
            return;
        };
        let next = &graph.edges[next_index];
        let forward = arrival_end == 0;
        current = RailPosition {
            edge: next.entity_id,
            distance_fixed: if forward { 0 } else { next.length_fixed },
            forward,
        };
    }
}

/// World point of a position on the track, in fixed-point units.
///
/// Derived from the rail's own geometry rather than stored on the stock: the
/// rail piece is the one description of where the track runs, and a cached
/// world position would be a second one that a rebuilt graph could contradict.
pub(in crate::simulation) fn world_point(
    sim: &Simulation,
    position: RailPosition,
) -> Option<RailPoint> {
    let geometry = sim.rail_piece_geometry(position.edge)?;
    let length = geometry.length_fixed.max(1);
    let travelled = position.distance_fixed.clamp(0, geometry.length_fixed);

    match geometry.curve {
        RailCurve::Straight => {
            let start = geometry.start.position;
            let end = geometry.end.position;
            Some(RailPoint::new(
                interpolate(start.x, end.x, travelled, length),
                interpolate(start.y, end.y, travelled, length),
            ))
        }
        RailCurve::QuarterArc { center } => Some(arc_point(
            center,
            geometry.start.position,
            geometry.end.position,
            travelled,
            length,
        )),
    }
}

/// A point `travelled / length` of the way from `from` to `to`, rounded toward
/// `from`. Widened to 128 bits before the multiply so a long straight in a
/// far-off chunk cannot wrap.
fn interpolate(from: i64, to: i64, travelled: i64, length: i64) -> i64 {
    let span = i128::from(to) - i128::from(from);
    (i128::from(from) + span * i128::from(travelled) / i128::from(length)) as i64
}

/// A point along a quarter arc, as a fraction of the turn.
///
/// The two end radii are blended and the result pushed back out to the circle,
/// rather than the radius being rotated by `t * 90°`. That keeps the whole path
/// in integers — a trigonometric rotation would put a float in the middle of a
/// path that has to agree with the integer simulation — and it puts the point
/// exactly on the arc, which is the property that stops a wagon from being
/// drawn inside the bend it is taking.
///
/// What it does *not* give is an exact arc-length parameterisation: the point
/// is exact at both ends and at the midpoint, and up to about a twentieth of
/// the turn out in between — a tenth of a tile on the base curve, which is a
/// few pixels. That is acceptable precisely because nothing measures with it.
/// Distances along the track are edge distances ([`travel`]), so where a train
/// *is* never depends on this; the world point is what draws it and what a
/// reach check compares against.
///
/// A note for whoever tightens this: subdividing the quarter by repeated
/// bisection is exact at every step, because normalising the sum of two unit
/// radii lands exactly on the bisector between them.
fn arc_point(
    center: RailPoint,
    start: RailPoint,
    end: RailPoint,
    travelled: i64,
    length: i64,
) -> RailPoint {
    let radius = squared_length(start.x - center.x, start.y - center.y).isqrt();
    let blended_x = interpolate(start.x - center.x, end.x - center.x, travelled, length);
    let blended_y = interpolate(start.y - center.y, end.y - center.y, travelled, length);
    let blended_radius = squared_length(blended_x, blended_y).isqrt().max(1);

    RailPoint::new(
        center.x + (i128::from(blended_x) * radius / blended_radius) as i64,
        center.y + (i128::from(blended_y) * radius / blended_radius) as i64,
    )
}

fn squared_length(dx: i64, dy: i64) -> i128 {
    i128::from(dx) * i128::from(dx) + i128::from(dy) * i128::from(dy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::tests::rolling_stock::{world_with_curved_rail, world_with_rail_run};

    #[test]
    fn travel_inside_one_edge_stays_on_it() {
        let (sim, rails) = world_with_rail_run(3);
        let graph = &sim.rails.graph;
        let start = RailPosition::new(rails[0], 0, true);

        let outcome = travel(graph, start, 512);

        assert_eq!(outcome.position.edge, rails[0]);
        assert_eq!(outcome.position.distance_fixed, 512);
        assert!(!outcome.is_blocked());
    }

    /// The whole point of the walk: a step longer than the rail it starts on
    /// continues onto the joined rail instead of stopping at the seam.
    #[test]
    fn travel_crosses_onto_the_joined_rail() {
        let (sim, rails) = world_with_rail_run(3);
        let graph = &sim.rails.graph;
        let start = RailPosition::new(rails[0], 0, true);

        let outcome = travel(graph, start, 3_072);

        assert_eq!(outcome.position.edge, rails[1]);
        assert_eq!(outcome.position.distance_fixed, 1_024);
        assert!(!outcome.is_blocked());
    }

    #[test]
    fn travel_stops_at_a_free_end_and_reports_the_rest() {
        let (sim, rails) = world_with_rail_run(2);
        let graph = &sim.rails.graph;
        let start = RailPosition::new(rails[0], 0, true);

        // Two 2-tile straights are 4096 units of track; asking for 5000 leaves
        // 904 unpaid at the far end.
        let outcome = travel(graph, start, 5_000);

        assert_eq!(outcome.position.edge, rails[1]);
        assert_eq!(outcome.position.distance_fixed, 2_048);
        assert_eq!(outcome.blocked_fixed, 904);
    }

    /// Walking out and back must land exactly where it started, or a train
    /// changing direction would drift a unit at a time.
    #[test]
    fn travel_is_reversible() {
        let (sim, rails) = world_with_rail_run(4);
        let graph = &sim.rails.graph;
        let start = RailPosition::new(rails[0], 700, true);

        let out = travel(graph, start, 4_000);
        let back = travel(graph, out.position, -4_000);

        assert!(!out.is_blocked());
        assert_eq!(back.position, start);
        assert!(!back.is_blocked());
    }

    #[test]
    fn a_backwards_step_that_runs_out_of_track_reports_it() {
        let (sim, rails) = world_with_rail_run(2);
        let graph = &sim.rails.graph;
        let start = RailPosition::new(rails[0], 500, true);

        let outcome = travel(graph, start, -1_000);

        assert_eq!(outcome.position.distance_fixed, 0);
        assert_eq!(outcome.blocked_fixed, -500);
    }

    /// The world point has to walk up the track the way the rails run: a run of
    /// straights laid north puts every derived point on the same column, spaced
    /// by the distance travelled.
    #[test]
    fn world_points_follow_the_track() {
        let (sim, rails) = world_with_rail_run(2);
        let first = world_point(&sim, RailPosition::new(rails[0], 0, true))
            .expect("a placed rail has geometry");
        let middle = world_point(&sim, RailPosition::new(rails[0], 1_024, true))
            .expect("a placed rail has geometry");
        let seam = world_point(&sim, RailPosition::new(rails[0], 2_048, true))
            .expect("a placed rail has geometry");
        let next = world_point(&sim, RailPosition::new(rails[1], 0, true))
            .expect("a placed rail has geometry");

        assert_eq!(first.x, middle.x);
        assert_eq!(middle.y - first.y, 1_024);
        assert_eq!(seam.y - middle.y, 1_024);
        // The seam is one point reached two ways, so both rails must report it
        // identically or a train would jump when it crossed.
        assert_eq!(seam, next);
    }

    /// A curve's derived points must stay on its arc rather than cutting the
    /// chord: a locomotive drawn inside the bend it is taking is the visible
    /// symptom, and a reach check answered from that point is the invisible one.
    ///
    /// This is about the radius, not about the spacing along the arc — see
    /// [`arc_point`] for why the second is deliberately approximate.
    #[test]
    fn world_points_on_a_curve_stay_on_its_arc() {
        let (sim, curve) = world_with_curved_rail();
        let geometry = sim
            .rail_piece_geometry(curve)
            .expect("a placed curve has geometry");
        let RailCurve::QuarterArc { center } = geometry.curve else {
            panic!("the fixture places a curved rail");
        };
        let radius = geometry.radius_fixed();

        for step in 0..=8 {
            let travelled = geometry.length_fixed * step / 8;
            let point = world_point(&sim, RailPosition::new(curve, travelled, true))
                .expect("a placed rail has geometry");
            let distance = squared_length(point.x - center.x, point.y - center.y).isqrt() as i64;
            assert!(
                (distance - radius).abs() <= radius / 1_000,
                "point {step}/8 along the arc sits {distance} from the centre, not {radius}"
            );
        }
    }
}
