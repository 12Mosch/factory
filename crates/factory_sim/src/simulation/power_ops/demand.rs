use std::collections::BTreeMap;

use super::*;

impl Simulation {
    pub(super) fn consumer_power_demands(&self) -> BTreeMap<EntityId, (u64, u64)> {
        self.entities
            .electric_consumers
            .keys()
            .filter_map(|entity_id| {
                let placed = self.entities.placed_entity(*entity_id)?;
                let energy_source = self
                    .world
                    .prototypes
                    .entity(placed.prototype_id)?
                    .electric_energy_source
                    .as_ref()?;
                let active_usage_watts = if self.electric_consumer_can_work(*entity_id) {
                    energy_source.energy_usage_watts
                } else {
                    0
                };
                Some((*entity_id, (active_usage_watts, energy_source.drain_watts)))
            })
            .collect()
    }

    pub(super) fn consumer_power_statuses(
        &self,
        network_ids_by_entity: &BTreeMap<EntityId, u32>,
        consumer_demands: BTreeMap<EntityId, (u64, u64)>,
    ) -> BTreeMap<EntityId, EntityPowerStatus> {
        consumer_demands
            .into_iter()
            .map(|(entity_id, (active_usage_watts, drain_watts))| {
                let network_id = network_ids_by_entity.get(&entity_id).copied();
                let satisfaction_permyriad = network_id
                    .and_then(|network_id| self.power.networks.get(network_id as usize))
                    .map(|network| network.satisfaction_permyriad)
                    .unwrap_or(0);
                (
                    entity_id,
                    EntityPowerStatus {
                        network_id,
                        satisfaction_permyriad,
                        active_usage_watts,
                        drain_watts,
                    },
                )
            })
            .collect()
    }

    pub(super) fn electric_consumer_can_work(&self, entity_id: EntityId) -> bool {
        if let Ok(state) = self.entities.assembler_state(entity_id) {
            return self.assembler_can_work(state);
        }
        if let Ok(state) = self.entities.lab_state(entity_id) {
            return self.lab_can_work(state);
        }
        if let (Some(placed), Ok(state)) = (
            self.entities.placed_entity(entity_id),
            self.entities.inserter_state(entity_id),
        ) {
            return self.inserter_can_work(placed, state);
        }

        false
    }

    pub(super) fn assembler_can_work(&self, state: &AssemblingMachineState) -> bool {
        let Some(recipe) = selected_assembler_recipe(&self.world.prototypes, &self.research, state)
        else {
            return false;
        };

        assembler_has_ingredients(&state.input_inventory, &recipe.ingredients)
            && assembler_output_can_accept(
                &self.world.prototypes,
                &state.output_inventory,
                &recipe.products,
            )
    }

    pub(super) fn lab_can_work(&self, state: &LabState) -> bool {
        let Some(technology_id) = state.active_technology.or(self.research.active) else {
            return false;
        };
        let Some(technology) = self.world.prototypes.technology(technology_id) else {
            return false;
        };

        lab_has_science_packs(&state.inventory, &technology.science_packs)
    }

    pub(super) fn inserter_can_work(&self, placed: &PlacedEntity, state: &InserterState) -> bool {
        let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
            return false;
        };
        let Some(inserter) = prototype.inserter.as_ref() else {
            return false;
        };
        let (pickup_tile, drop_tile) = inserter_transfer_tiles_for_prototype(placed, inserter);

        match *state {
            InserterState::WaitingForItem => {
                let Some(item_id) = peek_inserter_source_item(&self.entities, pickup_tile) else {
                    return false;
                };
                inserter_target_can_accept(
                    &self.world.prototypes,
                    &self.research,
                    &self.entities,
                    drop_tile,
                    ItemStack { item_id, count: 1 },
                )
            }
            InserterState::Picking { .. } | InserterState::Dropping { .. } => true,
            InserterState::Holding { item } => inserter_target_can_accept(
                &self.world.prototypes,
                &self.research,
                &self.entities,
                drop_tile,
                item,
            ),
        }
    }
}
