use bevy::prelude::*;
use factory_data::{BasePrototypeIds, InserterPrototype, ItemId, PrototypeCatalog, TileId};
use factory_sim::ResourceCell;

pub(crate) fn chest_color(prototype_name: &str) -> Color {
    match prototype_name {
        "iron_chest" => Color::srgb(0.46, 0.50, 0.52),
        "steel_chest" => Color::srgb(0.64, 0.69, 0.72),
        _ => Color::srgb(0.64, 0.43, 0.22),
    }
}

pub(crate) fn mining_drill_color() -> Color {
    Color::srgb(0.38, 0.44, 0.39)
}

pub(crate) fn furnace_color() -> Color {
    Color::srgb(0.55, 0.47, 0.38)
}

pub(crate) fn assembler_color() -> Color {
    Color::srgb(0.22, 0.50, 0.60)
}

pub(crate) fn oil_refinery_color() -> Color {
    Color::srgb(0.42, 0.38, 0.30)
}

pub(crate) fn chemical_plant_color() -> Color {
    Color::srgb(0.30, 0.55, 0.35)
}

pub(crate) fn pumpjack_color() -> Color {
    Color::srgb(0.35, 0.30, 0.28)
}

pub(crate) fn lab_color() -> Color {
    Color::srgb(0.48, 0.36, 0.66)
}

pub(crate) fn beacon_color() -> Color {
    Color::srgb(0.24, 0.58, 0.72)
}

pub(crate) fn electric_pole_color() -> Color {
    Color::srgb(0.78, 0.61, 0.30)
}

pub(crate) fn steam_engine_color() -> Color {
    Color::srgb(0.40, 0.54, 0.57)
}

pub(crate) fn boiler_color() -> Color {
    Color::srgb(0.62, 0.35, 0.22)
}

pub(crate) fn solar_panel_color() -> Color {
    Color::srgb(0.18, 0.38, 0.58)
}

pub(crate) fn accumulator_color() -> Color {
    Color::srgb(0.34, 0.48, 0.40)
}

pub(crate) fn radar_color() -> Color {
    Color::srgb(0.42, 0.48, 0.43)
}

pub(crate) fn offshore_pump_color() -> Color {
    Color::srgb(0.14, 0.48, 0.68)
}

pub(crate) fn pump_color() -> Color {
    Color::srgb(0.22, 0.62, 0.78)
}

pub(crate) fn pipe_color() -> Color {
    Color::srgb(0.50, 0.57, 0.58)
}

pub(crate) fn storage_tank_color() -> Color {
    Color::srgb(0.53, 0.62, 0.64)
}

pub(crate) fn wall_color() -> Color {
    Color::srgb(0.76, 0.78, 0.74)
}

pub(crate) fn gun_turret_color() -> Color {
    Color::srgb(0.62, 0.54, 0.30)
}

pub(crate) fn laser_turret_color() -> Color {
    Color::srgb(0.12, 0.58, 0.78)
}

pub(crate) fn enemy_spawner_color() -> Color {
    Color::srgb(0.52, 0.22, 0.38)
}

pub(crate) fn enemy_unit_color() -> Color {
    Color::srgb(0.72, 0.26, 0.22)
}

pub(crate) fn transport_belt_color(speed_subtiles_per_tick: Option<u16>) -> Color {
    match speed_subtiles_per_tick {
        Some(16) => Color::srgb(0.82, 0.24, 0.16),
        Some(24) => Color::srgb(0.15, 0.42, 0.86),
        _ => Color::srgb(0.86, 0.61, 0.16),
    }
}

pub(crate) fn splitter_color(speed_subtiles_per_tick: Option<u16>) -> Color {
    match speed_subtiles_per_tick {
        Some(16) => Color::srgb(0.70, 0.20, 0.15),
        Some(24) => Color::srgb(0.13, 0.34, 0.72),
        _ => Color::srgb(0.76, 0.49, 0.16),
    }
}

pub(crate) fn inserter_color(inserter: Option<&InserterPrototype>, is_burner: bool) -> Color {
    if is_burner {
        return Color::srgb(0.48, 0.30, 0.16);
    }
    match inserter {
        Some(inserter)
            if inserter
                .pickup_offset
                .x
                .abs()
                .max(inserter.pickup_offset.y.abs())
                > 1
                || inserter
                    .drop_offset
                    .x
                    .abs()
                    .max(inserter.drop_offset.y.abs())
                    > 1 =>
        {
            Color::srgb(0.88, 0.34, 0.14)
        }
        Some(inserter) if inserter.pickup_ticks <= 12 && inserter.drop_ticks <= 12 => {
            Color::srgb(0.17, 0.43, 0.86)
        }
        _ => Color::srgb(0.72, 0.60, 0.27),
    }
}

/// Per-`TileId` base terrain colors resolved from the prototype catalog. The
/// data lives in `factory_data`; the renderer only turns each biome's declared
/// sRGB triple into a Bevy [`Color`] and applies coherent shade variation, so
/// terrain visuals stay data-driven rather than hard-coded here.
#[derive(Clone)]
pub(crate) struct TileColorTable {
    colors: std::sync::Arc<[Color]>,
}

impl TileColorTable {
    pub(crate) fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        let mut colors = vec![
            Color::default();
            catalog
                .tiles
                .iter()
                .map(|tile| tile.id.index() + 1)
                .max()
                .unwrap_or_default()
        ];
        for tile in &catalog.tiles {
            let [r, g, b] = tile.color;
            colors[tile.id.index()] = Color::srgb_u8(r, g, b);
        }
        Self {
            colors: colors.into(),
        }
    }

    /// Base color for a tile, or a glaring magenta if the id is unknown so a
    /// missing palette entry is obvious rather than invisible.
    fn base(&self, tile_id: TileId) -> Color {
        self.colors
            .get(tile_id.index())
            .copied()
            .unwrap_or(Color::srgb_u8(255, 0, 255))
    }
}

pub(crate) fn tile_color(
    tile_id: TileId,
    colors: &TileColorTable,
    seed: u64,
    x: i64,
    y: i64,
) -> Color {
    // Coherent per-tile shade jitter around the biome's declared base color,
    // keeping the subtle terrain texture the old hard-coded shades provided.
    let factor = match coherent_variant(seed, x, y, 8, 0x6a09_e667_f3bc_c909) {
        -1 => 0.93,
        1 => 1.07,
        _ => 1.0,
    };
    scaled_color(colors.base(tile_id), factor)
}

/// Multiply a color's RGB by `factor`, clamping to the displayable range.
fn scaled_color(color: Color, factor: f32) -> Color {
    let color = color.to_srgba();
    Color::srgba(
        (color.red * factor).min(1.0),
        (color.green * factor).min(1.0),
        (color.blue * factor).min(1.0),
        color.alpha,
    )
}

pub(crate) fn resource_color_variant(
    resource: ResourceCell,
    ids: RenderPrototypeIds,
    seed: u64,
    x: i64,
    y: i64,
) -> Color {
    let factor = match coherent_variant(seed, x, y, 5, 0xbb67_ae85_84ca_a73b) {
        -1 => 0.90,
        1 => 1.06,
        _ => 0.98,
    };
    let color = resource_color(resource, ids).to_srgba();
    Color::srgba(
        (color.red * factor).min(1.0),
        (color.green * factor).min(1.0),
        (color.blue * factor).min(1.0),
        color.alpha,
    )
}

pub(crate) fn tile_hash(seed: u64, x: i64, y: i64, salt: u64) -> u64 {
    let mut value = seed
        ^ (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ salt;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn coherent_variant(seed: u64, x: i64, y: i64, scale: i64, salt: u64) -> i8 {
    let cell_x = x.div_euclid(scale);
    let cell_y = y.div_euclid(scale);
    let tx = smoothstep(x.rem_euclid(scale) as f32 / scale as f32);
    let ty = smoothstep(y.rem_euclid(scale) as f32 / scale as f32);
    let sample =
        |sample_x, sample_y| (tile_hash(seed, sample_x, sample_y, salt) & 0xffff) as f32 / 65_535.0;
    let south = sample(cell_x, cell_y).lerp(sample(cell_x + 1, cell_y), tx);
    let north = sample(cell_x, cell_y + 1).lerp(sample(cell_x + 1, cell_y + 1), tx);
    match south.lerp(north, ty) {
        value if value < 0.36 => -1,
        value if value > 0.64 => 1,
        _ => 0,
    }
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

pub(crate) fn resource_color(resource: ResourceCell, ids: RenderPrototypeIds) -> Color {
    if resource.resource_item == ids.iron_ore {
        Color::srgb(0.72, 0.66, 0.58)
    } else if resource.resource_item == ids.copper_ore {
        Color::srgb(0.86, 0.42, 0.20)
    } else if resource.resource_item == ids.coal {
        Color::srgb(0.10, 0.10, 0.095)
    } else if resource.resource_item == ids.stone {
        Color::srgb(0.54, 0.51, 0.46)
    } else if resource.resource_item == ids.crude_oil {
        Color::srgb(0.16, 0.10, 0.22)
    } else {
        Color::srgb(0.82, 0.78, 0.66)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct RenderPrototypeIds {
    dirt: TileId,
    water: TileId,
    iron_ore: ItemId,
    copper_ore: ItemId,
    coal: ItemId,
    stone: ItemId,
    crude_oil: ItemId,
}

impl RenderPrototypeIds {
    pub(crate) fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        let ids = BasePrototypeIds::from_catalog(catalog);
        Self {
            dirt: ids.tiles.dirt,
            water: ids.tiles.water,
            iron_ore: ids.items.iron_ore,
            copper_ore: ids.items.copper_ore,
            coal: ids.items.coal,
            stone: ids.items.stone,
            crude_oil: ids.items.crude_oil,
        }
    }

    pub(crate) fn is_water(self, tile_id: TileId) -> bool {
        tile_id == self.water
    }

    pub(crate) fn is_dirt(self, tile_id: TileId) -> bool {
        tile_id == self.dirt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_colors_are_indexed_by_tile_id() {
        let mut catalog = PrototypeCatalog::load_base().expect("base catalog should load");
        let mut high_id_tile = catalog.tiles[0].clone();
        high_id_tile.id = TileId::new(2);
        high_id_tile.color = [10, 20, 30];
        let mut low_id_tile = catalog.tiles[1].clone();
        low_id_tile.id = TileId::new(0);
        low_id_tile.color = [40, 50, 60];
        catalog.tiles = vec![high_id_tile, low_id_tile];

        let colors = TileColorTable::from_catalog(&catalog);

        assert_eq!(colors.base(TileId::new(0)), Color::srgb_u8(40, 50, 60));
        assert_eq!(colors.base(TileId::new(1)), Color::default());
        assert_eq!(colors.base(TileId::new(2)), Color::srgb_u8(10, 20, 30));
    }
}
