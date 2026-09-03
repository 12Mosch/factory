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
    pub(crate) items: Vec<ItemPrototype>,
    pub(crate) fluids: Vec<FluidPrototype>,
    pub(crate) recipes: Vec<RecipePrototype>,
    pub(crate) entities: Vec<EntityPrototype>,
    pub(crate) tiles: Vec<TilePrototype>,
    pub(crate) technologies: Vec<TechnologyPrototype>,
    /// Signal channels with no item or fluid identity. Empty in catalogs that
    /// carry no circuit-network content.
    #[serde(default)]
    pub(crate) virtual_signals: Vec<VirtualSignalPrototype>,
    pub(crate) world_generation: WorldGenerationConfig,
    pub(crate) enemy_gameplay: Option<EnemyGameplayConfig>,
    #[serde(default)]
    pub(crate) day_night_cycle: Option<DayNightCycleConfig>,
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
    pub fn items(&self) -> &[ItemPrototype] {
        &self.items
    }

    pub fn fluids(&self) -> &[FluidPrototype] {
        &self.fluids
    }

    pub fn recipes(&self) -> &[RecipePrototype] {
        &self.recipes
    }

    pub fn entities(&self) -> &[EntityPrototype] {
        &self.entities
    }

    pub fn tiles(&self) -> &[TilePrototype] {
        &self.tiles
    }

    pub fn technologies(&self) -> &[TechnologyPrototype] {
        &self.technologies
    }

    pub fn virtual_signals(&self) -> &[VirtualSignalPrototype] {
        &self.virtual_signals
    }

    pub fn world_generation(&self) -> &WorldGenerationConfig {
        &self.world_generation
    }

    pub fn enemy_gameplay(&self) -> Option<&EnemyGameplayConfig> {
        self.enemy_gameplay.as_ref()
    }

    pub fn day_night_cycle(&self) -> Option<DayNightCycleConfig> {
        self.day_night_cycle
    }

    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub fn items_mut(&mut self) -> &mut [ItemPrototype] {
        &mut self.items
    }

    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub fn items_vec_mut(&mut self) -> &mut Vec<ItemPrototype> {
        &mut self.items
    }

    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub fn entities_mut(&mut self) -> &mut [EntityPrototype] {
        &mut self.entities
    }

    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub fn tiles_mut(&mut self) -> &mut [TilePrototype] {
        &mut self.tiles
    }

    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub fn replace_tiles(&mut self, tiles: Vec<TilePrototype>) {
        self.tiles = tiles;
    }

    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub fn technologies_mut(&mut self) -> &mut [TechnologyPrototype] {
        &mut self.technologies
    }

    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub fn world_generation_mut(&mut self) -> &mut WorldGenerationConfig {
        &mut self.world_generation
    }

    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub fn set_day_night_cycle(&mut self, config: Option<DayNightCycleConfig>) {
        self.day_night_cycle = config;
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    /// Atomic launch reward for `payload`, or `None` when it is not launchable.
    pub fn rocket_launch_products(&self, payload: ItemId) -> Option<&[crate::model::ItemAmount]> {
        let products = &self.item(payload)?.launch_products;
        (!products.is_empty()).then_some(products)
    }

    /// Items that can be used as rocket cargo, in stable catalog order.
    pub fn rocket_launch_payloads(&self) -> impl Iterator<Item = &ItemPrototype> {
        self.items
            .iter()
            .filter(|item| !item.launch_products.is_empty())
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
