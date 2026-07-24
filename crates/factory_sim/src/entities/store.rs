use crate::entities::dense_map::DenseEntityMap;
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
            mining_drills: crate::machines::MiningDrillState => MiningDrill,
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
            inserter_energy: crate::machines::MachineEnergy => _,
            laser_turrets: crate::combat::LaserTurretState => LaserTurret,
            beacons: crate::machines::BeaconState => Beacon,
            solar_panels: crate::power::SolarPanelState => SolarPanel,
            accumulators: crate::power::AccumulatorState => Accumulator,
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
            $(pub(crate) $field: entity_state_map_type!($field, $ty),)*
            pub(crate) occupancy: OccupancyGrid,
            pub(crate) next_entity_id: u64,
        }

        impl EntityStore {
            /// Store without entities; `next_entity_id` seeds the id allocator.
            pub(crate) fn empty(next_entity_id: u64) -> Self {
                Self {
                    entities: Vec::new(),
                    placed_entities: BTreeMap::new(),
                    $($field: Default::default(),)*
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

            /// Module state shared by all productive module-bearing machines.
            ///
            /// Keep per-kind dispatch here so effect resolution, transfers,
            /// pollution, power, and presentation cannot drift independently
            /// when another productive machine type gains module support.
            pub(crate) fn machine_module_state(
                &self,
                entity_id: EntityId,
            ) -> Option<&crate::machines::MachineModuleState> {
                if let Some(state) = self.assembling_machines.get(&entity_id) {
                    Some(&state.modules)
                } else if let Some(state) = self.furnaces.get(&entity_id) {
                    Some(&state.modules)
                } else if let Some(state) = self.mining_drills.get(&entity_id) {
                    Some(&state.modules)
                } else {
                    self.labs.get(&entity_id).map(|state| &state.modules)
                }
            }

            pub(crate) fn machine_module_state_mut(
                &mut self,
                entity_id: EntityId,
            ) -> Option<&mut crate::machines::MachineModuleState> {
                if let Some(state) = self.assembling_machines.get_mut(&entity_id) {
                    Some(&mut state.modules)
                } else if let Some(state) = self.furnaces.get_mut(&entity_id) {
                    Some(&mut state.modules)
                } else if let Some(state) = self.mining_drills.get_mut(&entity_id) {
                    Some(&mut state.modules)
                } else {
                    self.labs.get_mut(&entity_id).map(|state| &mut state.modules)
                }
            }

            /// Module slots for productive machines and passive beacons.
            pub(crate) fn module_slots(
                &self,
                entity_id: EntityId,
            ) -> Option<&crate::machines::ModuleSlots> {
                self.machine_module_state(entity_id)
                    .map(|modules| &modules.slots)
                    .or_else(|| self.beacons.get(&entity_id).map(|state| &state.slots))
            }

            pub(crate) fn module_slots_mut(
                &mut self,
                entity_id: EntityId,
            ) -> Option<&mut crate::machines::ModuleSlots> {
                if self.machine_module_state(entity_id).is_some() {
                    return self
                        .machine_module_state_mut(entity_id)
                        .map(|modules| &mut modules.slots);
                }
                self.beacons
                    .get_mut(&entity_id)
                    .map(|state| &mut state.slots)
            }
        }
    };
}

macro_rules! entity_state_map_type {
    (transport_belts, $ty:ty) => {
        DenseEntityMap<$ty>
    };
    (splitters, $ty:ty) => {
        DenseEntityMap<$ty>
    };
    ($field:ident, $ty:ty) => {
        BTreeMap<EntityId, $ty>
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
    use crate::combat::{
        Damage, EnemySpawnerState, Faction, GunTurretState, HealthState, LaserTurretState,
    };
    use crate::fluids::FluidBoxState;
    use crate::inventory::{test_inventory, test_slot, test_stack};
    use crate::logistics::{BeltItem, BeltSegment, InserterState, SplitterState};
    use crate::machines::{
        AssemblingMachineState, BurnerEnergy, FurnaceState, LabState, MachineEnergy,
        MachineModuleState, MiningDrillState, PumpjackState,
    };
    use crate::player::ManualMiningTarget;
    use crate::power::{
        AccumulatorState, BoilerState, ElectricConsumerState, ElectricPoleState, OffshorePumpState,
        SolarPanelState, SteamEngineState,
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
        // v20: furnace and mining drill energy became MachineEnergy
        // (burner-or-electric); the fixture drill uses the electric variant
        // so both enum arms are pinned.
        // v21: transport items gained stable identities.
        // v22: inserter energy state was appended for burner inserters.
        // v23: laser turret state was appended.
        // v25: module state joined productive machines and beacon state was appended.
        // v26: solar panel and accumulator state maps were appended.
        const EXPECTED_LAYOUT_HASH: u64 = 0xd118_4bc7_c3fb_6e74;

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

        let mut store = EntityStore::empty(21);

        for raw in 1..=20 {
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
        // The drill uses the electric variant and the furnace the burner
        // variant so the golden layout covers both `MachineEnergy` arms.
        store.mining_drills.insert(
            EntityId::new(2),
            MiningDrillState {
                modules: MachineModuleState::with_slot_count(0),
                energy: MachineEnergy::Electric,
                mining_progress_ticks: 7,
                mining_required_ticks: 60,
                resource_target: Some(ManualMiningTarget { x: 2, y: 3 }),
                output_slot: test_slot(test_stack(copper, 2)),
            },
        );
        store.furnaces.insert(
            EntityId::new(3),
            FurnaceState {
                modules: MachineModuleState::with_slot_count(0),
                input_slot: test_slot(test_stack(iron, 3)),
                energy: MachineEnergy::Burner(burner_energy(iron)),
                output_slot: test_slot(test_stack(copper, 1)),
                active_recipe: Some(recipe),
                crafting_progress_ticks: 9,
                crafting_required_ticks: 120,
            },
        );
        store.assembling_machines.insert(
            EntityId::new(4),
            AssemblingMachineState {
                modules: MachineModuleState::with_slot_count(0),
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
                modules: MachineModuleState::with_slot_count(0),
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
        store
            .solar_panels
            .insert(EntityId::new(19), SolarPanelState);
        store.accumulators.insert(
            EntityId::new(20),
            AccumulatorState {
                stored_energy_joules: 4_321,
                energy_remainder_watt_ticks: 17,
            },
        );
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
            id: crate::logistics::BeltItemId::new(1),
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
            .inserter_energy
            .insert(EntityId::new(14), MachineEnergy::Electric);
        store.laser_turrets.insert(
            EntityId::new(18),
            LaserTurretState {
                engaged: true,
                cooldown_remaining_ticks: 23,
            },
        );

        store
    }

    fn burner_energy(fuel_item: ItemId) -> BurnerEnergy {
        BurnerEnergy {
            fuel_slot: test_slot(test_stack(fuel_item, 1)),
            energy_remaining_joules: 42.0,
            energy_usage_watts: 90_000.0,
        }
    }
}
