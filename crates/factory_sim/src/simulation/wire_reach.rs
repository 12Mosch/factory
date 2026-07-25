//! Geometry shared by the two wire systems: copper wire between electric
//! poles and red/green circuit wire between connectors.
//!
//! Distances are measured in half tiles (`_x2`) between footprint centers, so
//! an even-sized footprint still has an exact integer center and reach checks
//! stay in integer arithmetic.

use super::{EntityFootprint, WorldTileCoord};

/// Center of `footprint` in half-tile units.
pub(in crate::simulation) fn footprint_center_x2(
    footprint: &EntityFootprint,
) -> (WorldTileCoord, WorldTileCoord) {
    (
        footprint.x.saturating_mul(2) + i64::from(footprint.width),
        footprint.y.saturating_mul(2) + i64::from(footprint.height),
    )
}

/// Whether two half-tile centers lie within `reach_x2` of each other.
///
/// Widened to `i128` because squaring two world coordinates can exceed `i64`
/// long before the coordinates themselves do.
pub(in crate::simulation) fn centers_within_reach_x2(
    first: (WorldTileCoord, WorldTileCoord),
    second: (WorldTileCoord, WorldTileCoord),
    reach_x2: i64,
) -> bool {
    if reach_x2 < 0 {
        return false;
    }
    let dx = i128::from(first.0) - i128::from(second.0);
    let dy = i128::from(first.1) - i128::from(second.1);
    dx * dx + dy * dy <= i128::from(reach_x2) * i128::from(reach_x2)
}
