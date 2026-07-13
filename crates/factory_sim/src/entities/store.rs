use crate::entities::{Direction, EntityFootprint, OccupancyGrid};
use crate::ids::EntityId;
use factory_data::{EntityKind, EntityPrototypeId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Single source of truth for the per-kind entity state maps in [`EntityStore`].
///
/// Each entry is `map_field: StateType => OwnerTag`, where `OwnerTag` is the
/// [`EntityKind`] variant that owns the state, or `_` for auxiliary state that
/// several kinds share (electric consumers, fluid boxes). Entry order defines
/// the save-file field order; append new entries and bump `SAVE_VERSION` when
/// changing it.
///
/// Invoked with a callback macro, it expands the list wherever per-kind
/// bookkeeping is needed: the store struct itself, `EntityReservation`,
/// state insertion/removal, ownership validation, per-state validation, and
/// destroy recovery. Adding a new machine kind means adding one entry here,
/// implementing `EntityStateBehavior` for its state type, and giving it an
/// initial state in `machine_ops/state.rs`; the compiler flags every other
/// site that must react.
macro_rules! for_each_entity_state_map {
    ($callback:ident) => {
        $callback! {
            entity_inventories: crate::inventory::Inventory => Chest,
            burner_mining_drills: crate::machines::BurnerMiningDrillState => MiningDrill,
            furnaces: crate::machines::FurnaceState => Furnace,
            assembling_machines: crate::machines::AssemblingMachineState => AssemblingMachine,
            labs: crate::machines::LabState => Lab,
            electric_poles: crate::power::ElectricPoleState => ElectricPole,
            electric_consumers: crate::power::ElectricConsumerState => _,
            steam_engines: crate::power::SteamEngineState => SteamEngine,
            boilers: crate::power::BoilerState => Boiler,
            offshore_pumps: crate::power::OffshorePumpState => OffshorePump,
            fluid_boxes: Vec<crate::fluids::FluidBoxState> => _,
            transport_belts: crate::logistics::BeltSegment => TransportBelt,
            splitters: crate::logistics::SplitterState => Splitter,
            inserters: crate::logistics::InserterState => Inserter,
            pumpjacks: crate::machines::PumpjackState => Pumpjack,
            gun_turrets: crate::combat::GunTurretState => GunTurret,
            enemy_spawners: crate::combat::EnemySpawnerState => EnemySpawner,
            entity_health: crate::combat::HealthState => _,
        }
    };
}
pub(crate) use for_each_entity_state_map;

macro_rules! machine_kind_check {
    ($self:ident, $entity_id:ident, $field:ident, _) => {};
    ($self:ident, $entity_id:ident, $field:ident, $kind:ident) => {
        if $self.$field.contains_key(&$entity_id) {
            return Some(EntityKind::$kind);
        }
    };
}

macro_rules! define_entity_store {
    ($($field:ident : $ty:ty => $kind:tt),* $(,)?) => {
        #[derive(Clone, Debug, Deserialize, PartialEq, Hash, Serialize)]
        pub struct EntityStore {
            pub(crate) entities: Vec<SimEntity>,
            pub(crate) placed_entities: BTreeMap<EntityId, PlacedEntity>,
            $(pub(crate) $field: BTreeMap<EntityId, $ty>,)*
            pub(crate) occupancy: OccupancyGrid,
            pub(crate) next_entity_id: u64,
        }

        impl EntityStore {
            /// Store without entities; `next_entity_id` seeds the id allocator.
            pub(crate) fn empty(next_entity_id: u64) -> Self {
                Self {
                    entities: Vec::new(),
                    placed_entities: BTreeMap::new(),
                    $($field: BTreeMap::new(),)*
                    occupancy: OccupancyGrid::default(),
                    next_entity_id,
                }
            }

            /// Removes every per-kind state entry owned by `entity_id`.
            pub(crate) fn remove_entity_states(&mut self, entity_id: EntityId) {
                $(self.$field.remove(&entity_id);)*
            }

            /// The machine kind backing `entity_id`, derived from which state
            /// map owns it. Auxiliary state (electric consumers, fluid boxes)
            /// never determines the kind. Returns `None` for entities that are
            /// not placed or carry no per-kind state, so orphaned state
            /// entries are never reported as a valid kind.
            pub fn machine_kind(&self, entity_id: EntityId) -> Option<EntityKind> {
                if !self.placed_entities.contains_key(&entity_id) {
                    return None;
                }
                $(machine_kind_check!(self, entity_id, $field, $kind);)*
                None
            }
        }
    };
}
for_each_entity_state_map!(define_entity_store);

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
    pub x: crate::world::WorldTileCoord,
    pub y: crate::world::WorldTileCoord,
    pub direction: Direction,
    pub footprint: EntityFootprint,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{Damage, EnemySpawnerState, Faction, GunTurretState, HealthState};
    use crate::fluids::FluidBoxState;
    use crate::inventory::{test_inventory, test_stack};
    use crate::logistics::{BeltItem, BeltSegment, InserterState, SplitterState};
    use crate::machines::{
        AssemblingMachineState, BurnerEnergy, BurnerMiningDrillState, FurnaceState, LabState,
        PumpjackState,
    };
    use crate::player::ManualMiningTarget;
    use crate::power::{
        BoilerState, ElectricConsumerState, ElectricPoleState, OffshorePumpState, SteamEngineState,
    };
    use factory_data::{FluidId, ItemId, RecipeId, TechnologyId};

    #[test]
    fn entity_store_serialized_layout_is_stable() {
        // Golden fixture for the serialized `EntityStore` layout. A failure
        // means the save format changed (field order, field types, or state
        // added to the registry): update the constant and bump `SAVE_VERSION`
        // only for intentional format changes.
        // v12: gun turret, enemy spawner, and health state maps joined the
        // registry.
        // v16: EnemySpawnerState dropped absorbed_pollution_micro.
        // v18: gun turret damage and health state became typed combat data.
        const EXPECTED_LAYOUT_HASH: u64 = 0x9ff3_595e_dd96_fbda;

        let bytes =
            bincode::serialize(&populated_entity_store()).expect("entity store should serialize");

        assert_eq!(fnv1a_64(&bytes), EXPECTED_LAYOUT_HASH);
    }

    fn fnv1a_64(bytes: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

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

        let mut store = EntityStore::empty(19);

        for raw in 1..=18 {
            let id = EntityId::new(raw);
            let tile = raw as i64;
            store.entities.push(SimEntity {
                id,
                x: tile * 1024,
                y: (tile + 1) * 1024,
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
            test_inventory(vec![Some(test_stack(iron, 5))]),
        );
        store.burner_mining_drills.insert(
            EntityId::new(2),
            BurnerMiningDrillState {
                energy: burner_energy(iron),
                mining_progress_ticks: 7,
                mining_required_ticks: 60,
                resource_target: Some(ManualMiningTarget { x: 2, y: 3 }),
                output_slot: Some(test_stack(copper, 2)),
            },
        );
        store.furnaces.insert(
            EntityId::new(3),
            FurnaceState {
                input_slot: Some(test_stack(iron, 3)),
                energy: burner_energy(iron),
                output_slot: Some(test_stack(copper, 1)),
                active_recipe: Some(recipe),
                crafting_progress_ticks: 9,
                crafting_required_ticks: 120,
            },
        );
        store.assembling_machines.insert(
            EntityId::new(4),
            AssemblingMachineState {
                selected_recipe: Some(recipe),
                input_inventory: test_inventory(vec![Some(test_stack(iron, 4))]),
                output_inventory: test_inventory(vec![Some(test_stack(copper, 1))]),
                crafting_progress_ticks: 11,
                crafting_required_ticks: 60,
                crafting_speed_numerator: 1,
                crafting_speed_denominator: 2,
            },
        );
        store.labs.insert(
            EntityId::new(5),
            LabState {
                inventory: test_inventory(vec![Some(test_stack(iron, 1))]),
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
                item: test_stack(copper, 1),
            },
        );
        store.pumpjacks.insert(EntityId::new(15), PumpjackState);
        store.gun_turrets.insert(
            EntityId::new(16),
            GunTurretState {
                ammo: test_inventory(vec![Some(test_stack(copper, 7))]),
                loaded_shots: 4,
                loaded_damage: Damage::physical(5),
                next_ready_tick: 17,
            },
        );
        store.enemy_spawners.insert(
            EntityId::new(17),
            EnemySpawnerState {
                next_free_spawn_tick: 1_800,
            },
        );
        store
            .entity_health
            .insert(EntityId::new(18), HealthState::new(42, Faction::Player));

        store
    }

    fn burner_energy(fuel_item: ItemId) -> BurnerEnergy {
        BurnerEnergy {
            fuel_slot: Some(test_stack(fuel_item, 1)),
            energy_remaining_joules: 42.0,
            energy_usage_watts: 90_000.0,
        }
    }
}
