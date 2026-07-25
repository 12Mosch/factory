use crate::ids::{
    EntityPrototypeId, FluidId, ItemId, RecipeId, TechnologyId, TileId, VirtualSignalId,
};
use crate::model::{
    DayNightCycleConfig, EnemyGameplayConfig, EntityPrototype, FluidPrototype, ItemPrototype,
    RecipePrototype, TechnologyPrototype, TilePrototype, VirtualSignalKind, VirtualSignalPrototype,
    WorldGenerationConfig,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PrototypeCatalog {
    pub items: Vec<ItemPrototype>,
    pub fluids: Vec<FluidPrototype>,
    pub recipes: Vec<RecipePrototype>,
    pub entities: Vec<EntityPrototype>,
    pub tiles: Vec<TilePrototype>,
    pub technologies: Vec<TechnologyPrototype>,
    /// Signal channels with no item or fluid identity. Empty in catalogs that
    /// carry no circuit-network content.
    #[serde(default)]
    pub virtual_signals: Vec<VirtualSignalPrototype>,
    pub world_generation: WorldGenerationConfig,
    pub enemy_gameplay: Option<EnemyGameplayConfig>,
    #[serde(default)]
    pub day_night_cycle: Option<DayNightCycleConfig>,
}

/// Generates a typed lookup method on [`PrototypeCatalog`]. Ids double as
/// vector indices, but the id check guards against a stale id being used
/// against a catalog it was not issued from.
macro_rules! catalog_accessor {
    ($fn_name:ident, $field:ident, $id_ty:ty, $proto_ty:ty) => {
        pub fn $fn_name(&self, id: $id_ty) -> Option<&$proto_ty> {
            self.$field.get(id.index()).filter(|p| p.id == id)
        }
    };
}

impl PrototypeCatalog {
    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    pub fn max_beacon_effect_radius_tiles(&self) -> u16 {
        self.entities
            .iter()
            .filter_map(|prototype| prototype.beacon)
            .map(|beacon| beacon.effect_radius_tiles)
            .max()
            .unwrap_or(0)
    }

    /// Longest circuit wire any prototype can span, in half tiles. Used to
    /// bound the candidate search when the player drags a wire.
    pub fn max_circuit_wire_reach_tiles_x2(&self) -> u16 {
        self.entities
            .iter()
            .filter_map(|prototype| prototype.circuit_connector)
            .map(|connector| connector.wire_reach_tiles_x2)
            .max()
            .unwrap_or(0)
    }

    /// The wildcard virtual signal of `kind`, if the catalog defines one.
    pub fn wildcard_virtual_signal(&self, kind: VirtualSignalKind) -> Option<VirtualSignalId> {
        self.virtual_signals
            .iter()
            .find(|signal| signal.kind == kind)
            .map(|signal| signal.id)
    }

    catalog_accessor!(item, items, ItemId, ItemPrototype);
    catalog_accessor!(
        virtual_signal,
        virtual_signals,
        VirtualSignalId,
        VirtualSignalPrototype
    );
    catalog_accessor!(fluid, fluids, FluidId, FluidPrototype);
    catalog_accessor!(recipe, recipes, RecipeId, RecipePrototype);
    catalog_accessor!(entity, entities, EntityPrototypeId, EntityPrototype);
    catalog_accessor!(tile, tiles, TileId, TilePrototype);
    catalog_accessor!(technology, technologies, TechnologyId, TechnologyPrototype);
}
