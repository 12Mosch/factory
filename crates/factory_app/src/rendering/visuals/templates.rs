use factory_data::EntityKind;
use factory_sim::Direction;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum VisualTemplate {
    Entity {
        kind: EntityKind,
        direction: Direction,
        connections: ConnectionMask,
    },
    BeltItem,
    Resource,
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
