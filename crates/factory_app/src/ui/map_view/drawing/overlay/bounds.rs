use crate::map::resources::MapTextureBounds;

pub(super) fn inclusive_max_tile(bounds: MapTextureBounds) -> Option<(i64, i64)> {
    let width_offset = i64::from(bounds.width.checked_sub(1)?);
    let height_offset = i64::from(bounds.height.checked_sub(1)?);

    Some((
        bounds.min_x.checked_add(width_offset)?,
        bounds.min_y.checked_add(height_offset)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inclusive_max_tile_rejects_empty_and_overflowing_bounds() {
        assert_eq!(
            inclusive_max_tile(MapTextureBounds {
                min_x: i64::MIN,
                min_y: i64::MIN,
                width: 0,
                height: 1,
            }),
            None
        );
        assert_eq!(
            inclusive_max_tile(MapTextureBounds {
                min_x: i64::MAX,
                min_y: 0,
                width: 2,
                height: 1,
            }),
            None
        );
    }

    #[test]
    fn inclusive_max_tile_returns_inclusive_coordinates() {
        assert_eq!(
            inclusive_max_tile(MapTextureBounds {
                min_x: -10,
                min_y: 20,
                width: 4,
                height: 3,
            }),
            Some((-7, 22))
        );
    }
}
