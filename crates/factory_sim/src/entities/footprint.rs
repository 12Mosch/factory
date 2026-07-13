use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EntityFootprint {
    pub x: crate::world::WorldTileCoord,
    pub y: crate::world::WorldTileCoord,
    pub width: i32,
    pub height: i32,
}

impl EntityFootprint {
    /// A one-tile footprint used by mobile actors for spatial comparisons.
    pub const fn single_tile(x: i64, y: i64) -> Self {
        Self {
            x,
            y,
            width: 1,
            height: 1,
        }
    }

    /// Squared Euclidean distance between the nearest tiles in two footprints.
    pub fn distance_squared_to(&self, other: &Self) -> u128 {
        let (dx, dy) = self.axis_distances_to_bounds(
            i128::from(other.x),
            footprint_end(other.x, other.width),
            i128::from(other.y),
            footprint_end(other.y, other.height),
        );
        dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy))
    }

    pub(crate) fn distance_squared_to_bounds(
        &self,
        min_x: i128,
        max_x: i128,
        min_y: i128,
        max_y: i128,
    ) -> u128 {
        let (dx, dy) = self.axis_distances_to_bounds(min_x, max_x, min_y, max_y);
        dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy))
    }

    /// Chebyshev distance between the nearest tiles in two footprints.
    pub fn chebyshev_distance_to(&self, other: &Self) -> i64 {
        let (dx, dy) = self.axis_distances_to(other);
        distance_as_i64(dx.max(dy))
    }

    /// Manhattan distance between the nearest tiles in two footprints.
    pub fn manhattan_distance_to(&self, other: &Self) -> i64 {
        let (dx, dy) = self.axis_distances_to(other);
        distance_as_i64(dx.saturating_add(dy))
    }

    fn axis_distances_to(&self, other: &Self) -> (u128, u128) {
        debug_assert!(self.width > 0 && self.height > 0);
        debug_assert!(other.width > 0 && other.height > 0);
        self.axis_distances_to_bounds(
            i128::from(other.x),
            footprint_end(other.x, other.width),
            i128::from(other.y),
            footprint_end(other.y, other.height),
        )
    }

    fn axis_distances_to_bounds(
        &self,
        min_x: i128,
        max_x: i128,
        min_y: i128,
        max_y: i128,
    ) -> (u128, u128) {
        debug_assert!(self.width > 0 && self.height > 0);
        debug_assert!(min_x <= max_x && min_y <= max_y);
        (
            axis_distance(
                i128::from(self.x),
                footprint_end(self.x, self.width),
                min_x,
                max_x,
            ),
            axis_distance(
                i128::from(self.y),
                footprint_end(self.y, self.height),
                min_y,
                max_y,
            ),
        )
    }
}

fn axis_distance(a_start: i128, a_end: i128, b_start: i128, b_end: i128) -> u128 {
    if a_end < b_start {
        (b_start - a_end).unsigned_abs()
    } else if b_end < a_start {
        (a_start - b_end).unsigned_abs()
    } else {
        0
    }
}

fn footprint_end(start: i64, len: i32) -> i128 {
    i128::from(start) + i128::from(len) - 1
}

fn distance_as_i64(distance: u128) -> i64 {
    i64::try_from(distance).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footprint_distances_use_nearest_edges() {
        let wide = EntityFootprint {
            x: 10,
            y: 20,
            width: 4,
            height: 2,
        };
        let diagonal = EntityFootprint::single_tile(16, 24);

        assert_eq!(wide.distance_squared_to(&diagonal), 18);
        assert_eq!(wide.chebyshev_distance_to(&diagonal), 3);
        assert_eq!(wide.manhattan_distance_to(&diagonal), 6);
        assert_eq!(wide.distance_squared_to(&wide), 0);
    }
}
