use factory_data::EntityKind;
use factory_sim::{
    Direction, POSITION_SCALE, RailCurveGeometry, RailPieceGeometry, RailPoint, WorldTileCoord,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum VisualTemplate {
    Entity {
        kind: EntityKind,
        direction: Direction,
        connections: ConnectionMask,
        /// Track geometry, for the two rail kinds. Carried in the cache key
        /// because it is what the sprite is drawn from; it is a pure function of
        /// the kind and direction, so it costs no extra cache entries.
        rail: Option<RailVisual>,
    },
    BeltItem,
    Resource,
}

/// A rail piece's travel path as the renderer needs it: the simulation's own
/// fixed-point geometry, measured from the footprint's lower-left corner.
///
/// This is a carrier, not a second authoring. The numbers come from
/// [`factory_sim::rail_geometry_in_footprint`], so the track that is drawn is
/// the track the graph connects and a train would run on. Integer units keep it
/// hashable for the sprite cache and keep the drawn path exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RailVisual {
    pub(crate) entry: (i64, i64),
    pub(crate) exit: (i64, i64),
    pub(crate) curve: RailVisualCurve,
    /// Footprint extents in the same fixed-point units.
    pub(crate) footprint: (i64, i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RailVisualCurve {
    Straight,
    Arc {
        center: (i64, i64),
        radius_fixed: u32,
    },
}

impl RailVisual {
    pub(crate) fn from_geometry(
        geometry: RailPieceGeometry,
        footprint_width: i32,
        footprint_height: i32,
    ) -> Self {
        let point = |position: RailPoint| (position.x, position.y);
        Self {
            entry: point(geometry.endpoints[0].position),
            exit: point(geometry.endpoints[1].position),
            curve: match geometry.curve {
                RailCurveGeometry::Straight => RailVisualCurve::Straight,
                RailCurveGeometry::Arc {
                    center,
                    radius_fixed,
                } => RailVisualCurve::Arc {
                    center: point(center),
                    radius_fixed,
                },
            },
            footprint: (
                WorldTileCoord::from(footprint_width) * POSITION_SCALE,
                WorldTileCoord::from(footprint_height) * POSITION_SCALE,
            ),
        }
    }
}

/// Bit set of cardinal directions (indexed by [`Direction::index`]) in which an entity
/// visually joins its neighbor — pipe arms, belt couplings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct ConnectionMask(u8);

impl ConnectionMask {
    pub(crate) const EMPTY: Self = Self(0);

    pub(crate) fn from_directions(connected: [bool; 4]) -> Self {
        let mut bits = 0;
        for (index, is_connected) in connected.into_iter().enumerate() {
            if is_connected {
                bits |= 1 << index;
            }
        }
        Self(bits)
    }

    pub(crate) fn contains(self, direction: Direction) -> bool {
        self.0 & (1 << direction.index()) != 0
    }

    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// True when exactly two opposite directions are set — a straight run.
    pub(crate) fn is_straight_run(self) -> bool {
        self.0 == 0b0101 || self.0 == 0b1010
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_mask_round_trips_directions() {
        let mask = ConnectionMask::from_directions([true, false, false, true]);

        assert!(mask.contains(Direction::North));
        assert!(!mask.contains(Direction::East));
        assert!(!mask.contains(Direction::South));
        assert!(mask.contains(Direction::West));
        assert!(!mask.is_empty());
        assert!(ConnectionMask::EMPTY.is_empty());
    }
}
