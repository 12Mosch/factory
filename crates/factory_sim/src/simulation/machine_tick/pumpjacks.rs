use crate::simulation::fluid_ops::per_tick_milliunits;

use super::*;

impl MachineTickContext<'_> {
    pub(super) fn advance_pumpjacks(&mut self) {
        let pumpjack_ids = self.entities.pumpjacks.keys().copied().collect::<Vec<_>>();

        for entity_id in pumpjack_ids {
            let Some(placed) = self.entities.placed_entity(entity_id).cloned() else {
                continue;
            };
            let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
                continue;
            };
            let Some(pumpjack) = prototype.pumpjack.as_ref() else {
                continue;
            };
            let Some(capacity_milliunits) = prototype
                .fluid_boxes
                .first()
                .map(|fluid_box| fluid_box.capacity_milliunits)
            else {
                continue;
            };
            let resource_item = pumpjack.resource_item;
            let output_fluid = pumpjack.output_fluid;
            let per_tick = per_tick_milliunits(pumpjack.pumping_speed_per_second_milliunits);

            let covers_resource = placed.footprint.tiles().into_iter().any(|(x, y)| {
                self.world
                    .tile_at(x, y)
                    .and_then(|tile| tile.resource)
                    .is_some_and(|resource| resource.resource_item == resource_item)
            });
            if !covers_resource {
                continue;
            }

            let available = {
                let Some(state) = self
                    .entities
                    .fluid_boxes
                    .get(&entity_id)
                    .and_then(|boxes| boxes.first())
                else {
                    continue;
                };
                if state.fluid_id.is_some_and(|fluid| fluid != output_fluid) {
                    continue;
                }
                capacity_milliunits.saturating_sub(state.amount_milliunits)
            };
            let amount = per_tick.min(available);
            if amount == 0 {
                continue;
            }
            if !self.electric_work_allowed(entity_id) {
                continue;
            }

            let state = self
                .entities
                .fluid_boxes
                .get_mut(&entity_id)
                .and_then(|boxes| boxes.first_mut())
                .expect("pumpjack fluid box was checked above");
            state.fluid_id = Some(output_fluid);
            state.amount_milliunits += amount;
            self.mark_fluid_box_dirty(entity_id, 0);
            self.mark_pollution_emitter_active(entity_id);
            self.statistics.record_fluid_produced(output_fluid, amount);
        }
    }
}
