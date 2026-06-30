use crate::entities::{Direction, EntityFootprint, OccupancyGrid};
use crate::fluids::FluidBoxState;
use crate::ids::EntityId;
use crate::inventory::Inventory;
use crate::logistics::{BeltSegment, InserterState, SplitterState};
use crate::machines::{AssemblingMachineState, BurnerMiningDrillState, FurnaceState, LabState};
use crate::power::{
    BoilerState, ElectricConsumerState, ElectricPoleState, OffshorePumpState, SteamEngineState,
};
use factory_data::EntityPrototypeId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, PartialEq, Hash, Serialize)]
pub struct EntityStore {
    pub(crate) entities: Vec<SimEntity>,
    pub(crate) placed_entities: BTreeMap<EntityId, PlacedEntity>,
    pub(crate) entity_inventories: BTreeMap<EntityId, Inventory>,
    pub(crate) burner_mining_drills: BTreeMap<EntityId, BurnerMiningDrillState>,
    pub(crate) furnaces: BTreeMap<EntityId, FurnaceState>,
    pub(crate) assembling_machines: BTreeMap<EntityId, AssemblingMachineState>,
    pub(crate) labs: BTreeMap<EntityId, LabState>,
    pub(crate) electric_poles: BTreeMap<EntityId, ElectricPoleState>,
    pub(crate) electric_consumers: BTreeMap<EntityId, ElectricConsumerState>,
    pub(crate) steam_engines: BTreeMap<EntityId, SteamEngineState>,
    pub(crate) boilers: BTreeMap<EntityId, BoilerState>,
    pub(crate) offshore_pumps: BTreeMap<EntityId, OffshorePumpState>,
    pub(crate) fluid_boxes: BTreeMap<EntityId, Vec<FluidBoxState>>,
    pub(crate) transport_belts: BTreeMap<EntityId, BeltSegment>,
    pub(crate) splitters: BTreeMap<EntityId, SplitterState>,
    pub(crate) inserters: BTreeMap<EntityId, InserterState>,
    pub(crate) occupancy: OccupancyGrid,
    pub(crate) next_entity_id: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SimEntity {
    pub id: EntityId,
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PlacedEntity {
    pub id: EntityId,
    pub prototype_id: EntityPrototypeId,
    pub x: i32,
    pub y: i32,
    pub direction: Direction,
    pub footprint: EntityFootprint,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fluids::FluidBoxState;
    use crate::inventory::{Inventory, ItemStack};
    use crate::logistics::{BeltItem, BeltSegment, InserterState, SplitterState};
    use crate::machines::{
        AssemblingMachineState, BurnerEnergy, BurnerMiningDrillState, FurnaceState, LabState,
    };
    use crate::player::ManualMiningTarget;
    use crate::power::{
        BoilerState, ElectricConsumerState, ElectricPoleState, OffshorePumpState, SteamEngineState,
    };
    use factory_data::{FluidId, ItemId, RecipeId, TechnologyId};

    #[test]
    fn entity_store_round_trip_preserves_populated_state() {
        let original = populated_entity_store();
        let bytes = bincode::serialize(&original).expect("entity store should serialize");
        let restored: EntityStore =
            bincode::deserialize(&bytes).expect("entity store should deserialize");

        assert_eq!(restored, original);
    }

    fn populated_entity_store() -> EntityStore {
        let iron = ItemId::new(1);
        let copper = ItemId::new(2);
        let water = FluidId::new(1);
        let recipe = RecipeId::new(1);
        let technology = TechnologyId::new(1);

        let mut store = EntityStore {
            entities: Vec::new(),
            placed_entities: BTreeMap::new(),
            entity_inventories: BTreeMap::new(),
            burner_mining_drills: BTreeMap::new(),
            furnaces: BTreeMap::new(),
            assembling_machines: BTreeMap::new(),
            labs: BTreeMap::new(),
            electric_poles: BTreeMap::new(),
            electric_consumers: BTreeMap::new(),
            steam_engines: BTreeMap::new(),
            boilers: BTreeMap::new(),
            offshore_pumps: BTreeMap::new(),
            fluid_boxes: BTreeMap::new(),
            transport_belts: BTreeMap::new(),
            splitters: BTreeMap::new(),
            inserters: BTreeMap::new(),
            occupancy: OccupancyGrid::default(),
            next_entity_id: 15,
        };

        for raw in 1..=14 {
            let id = EntityId::new(raw);
            let tile = raw as i32;
            store.entities.push(SimEntity {
                id,
                x: i64::from(tile) * 1024,
                y: i64::from(tile + 1) * 1024,
            });
            store.placed_entities.insert(
                id,
                PlacedEntity {
                    id,
                    prototype_id: EntityPrototypeId::new(raw as u16),
                    x: tile,
                    y: tile + 1,
                    direction: Direction::East,
                    footprint: EntityFootprint {
                        x: tile,
                        y: tile + 1,
                        width: 1,
                        height: 1,
                    },
                },
            );
        }

        store
            .occupancy
            .occupied_tiles
            .insert((1, 2), EntityId::new(1));
        store.entity_inventories.insert(
            EntityId::new(1),
            Inventory {
                slots: vec![Some(ItemStack {
                    item_id: iron,
                    count: 5,
                })],
            },
        );
        store.burner_mining_drills.insert(
            EntityId::new(2),
            BurnerMiningDrillState {
                energy: burner_energy(iron),
                mining_progress_ticks: 7,
                mining_required_ticks: 60,
                resource_target: Some(ManualMiningTarget { x: 2, y: 3 }),
                output_slot: Some(ItemStack {
                    item_id: copper,
                    count: 2,
                }),
            },
        );
        store.furnaces.insert(
            EntityId::new(3),
            FurnaceState {
                input_slot: Some(ItemStack {
                    item_id: iron,
                    count: 3,
                }),
                energy: burner_energy(iron),
                output_slot: Some(ItemStack {
                    item_id: copper,
                    count: 1,
                }),
                active_recipe: Some(recipe),
                crafting_progress_ticks: 9,
                crafting_required_ticks: 120,
            },
        );
        store.assembling_machines.insert(
            EntityId::new(4),
            AssemblingMachineState {
                selected_recipe: Some(recipe),
                input_inventory: Inventory {
                    slots: vec![Some(ItemStack {
                        item_id: iron,
                        count: 4,
                    })],
                },
                output_inventory: Inventory {
                    slots: vec![Some(ItemStack {
                        item_id: copper,
                        count: 1,
                    })],
                },
                crafting_progress_ticks: 11,
                crafting_required_ticks: 60,
                crafting_speed_numerator: 1,
                crafting_speed_denominator: 2,
            },
        );
        store.labs.insert(
            EntityId::new(5),
            LabState {
                inventory: Inventory {
                    slots: vec![Some(ItemStack {
                        item_id: iron,
                        count: 1,
                    })],
                },
                active_technology: Some(technology),
                progress_ticks: 13,
                required_ticks: 30,
            },
        );
        store
            .electric_poles
            .insert(EntityId::new(6), ElectricPoleState);
        store.electric_consumers.insert(
            EntityId::new(7),
            ElectricConsumerState {
                work_remainder_permyriad: 123,
            },
        );
        store
            .steam_engines
            .insert(EntityId::new(8), SteamEngineState);
        store.boilers.insert(
            EntityId::new(9),
            BoilerState {
                energy: burner_energy(iron),
            },
        );
        store
            .offshore_pumps
            .insert(EntityId::new(10), OffshorePumpState);
        store.fluid_boxes.insert(
            EntityId::new(11),
            vec![FluidBoxState {
                fluid_id: Some(water),
                amount_milliunits: 12_345,
            }],
        );
        let mut belt = BeltSegment::new(Direction::South, 4);
        belt.lanes[0].items.push(BeltItem {
            item_id: iron,
            position_subtile: 64,
        });
        store.transport_belts.insert(EntityId::new(12), belt);
        store
            .splitters
            .insert(EntityId::new(13), SplitterState::new(Direction::West, 4));
        store.inserters.insert(
            EntityId::new(14),
            InserterState::Holding {
                item: ItemStack {
                    item_id: copper,
                    count: 1,
                },
            },
        );

        store
    }

    fn burner_energy(fuel_item: ItemId) -> BurnerEnergy {
        BurnerEnergy {
            fuel_slot: Some(ItemStack {
                item_id: fuel_item,
                count: 1,
            }),
            energy_remaining_joules: 42.0,
            energy_usage_watts: 90_000.0,
        }
    }
}
