use super::*;

const DIRECT_MODULE_STRENGTH_PERMYRIAD: u16 = 10_000;

pub(in crate::simulation) fn required_ticks_with_modules(
    work_ticks: u32,
    speed_numerator: u32,
    speed_denominator: u32,
    effects: ResolvedModuleEffects,
) -> u32 {
    let numerator = u128::from(work_ticks)
        .saturating_mul(u128::from(speed_denominator))
        .saturating_mul(10_000);
    let denominator = u128::from(speed_numerator)
        .saturating_mul(u128::from(effects.speed_multiplier_permyriad()));
    if denominator == 0 {
        return u32::MAX;
    }
    let ticks = numerator
        .saturating_add(denominator - 1)
        .checked_div(denominator)
        .unwrap_or(u128::from(u32::MAX));
    ticks.clamp(1, u128::from(u32::MAX)) as u32
}

impl Simulation {
    pub(super) fn rebuild_all_module_effects(&mut self) {
        let machine_ids = self
            .entities
            .mining_drills
            .keys()
            .chain(self.entities.furnaces.keys())
            .chain(self.entities.assembling_machines.keys())
            .chain(self.entities.labs.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        for entity_id in machine_ids {
            self.refresh_module_effects(entity_id);
        }
    }

    pub(super) fn refresh_module_effects(&mut self, entity_id: EntityId) {
        let Some(placed) = self.entities.placed_entity(entity_id).cloned() else {
            return;
        };
        let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
            return;
        };
        if prototype.module_slot_count == 0 || prototype.entity_kind == EntityKind::Beacon {
            return;
        }

        let effects = resolve_machine_module_effects(self, entity_id, &placed);
        let (old_required, old_progress, new_required) = if let Some(state) =
            self.entities.assembling_machines.get(&entity_id)
        {
            let new_required = state.selected_recipe.and_then(|recipe_id| {
                self.world.prototypes.recipe(recipe_id).map(|recipe| {
                    required_ticks_with_modules(
                        recipe.crafting_time_ticks,
                        state.crafting_speed_numerator,
                        state.crafting_speed_denominator,
                        effects,
                    )
                })
            });
            (
                state.crafting_required_ticks,
                state.crafting_progress_ticks,
                new_required.unwrap_or(0),
            )
        } else if let Some(state) = self.entities.furnaces.get(&entity_id) {
            let new_required = state.active_recipe.and_then(|recipe_id| {
                self.world.prototypes.recipe(recipe_id).and_then(|recipe| {
                    prototype.furnace.as_ref().map(|furnace| {
                        required_ticks_with_modules(
                            recipe.crafting_time_ticks,
                            furnace.crafting_speed_numerator,
                            furnace.crafting_speed_denominator,
                            effects,
                        )
                    })
                })
            });
            (
                state.crafting_required_ticks,
                state.crafting_progress_ticks,
                new_required.unwrap_or(0),
            )
        } else if let Some(state) = self.entities.mining_drills.get(&entity_id) {
            let base = prototype
                .mining_drill
                .as_ref()
                .map_or(0, |drill| drill.ticks_per_item);
            (
                state.mining_required_ticks,
                state.mining_progress_ticks,
                required_ticks_with_modules(base, 1, 1, effects),
            )
        } else if let Some(state) = self.entities.labs.get(&entity_id) {
            let new_required = state.active_technology.and_then(|technology_id| {
                self.world
                    .prototypes
                    .technology(technology_id)
                    .map(|technology| {
                        required_ticks_with_modules(technology.research_time_ticks, 1, 1, effects)
                    })
            });
            (
                state.required_ticks,
                state.progress_ticks,
                new_required.unwrap_or(0),
            )
        } else {
            return;
        };

        let new_progress = rescale_progress(old_progress, old_required, new_required);
        if let Some(state) = self.entities.assembling_machines.get_mut(&entity_id) {
            state.modules.resolved_effects = effects;
            state.crafting_required_ticks = new_required;
            state.crafting_progress_ticks = new_progress;
        } else if let Some(state) = self.entities.furnaces.get_mut(&entity_id) {
            state.modules.resolved_effects = effects;
            state.crafting_required_ticks = new_required;
            state.crafting_progress_ticks = new_progress;
        } else if let Some(state) = self.entities.mining_drills.get_mut(&entity_id) {
            state.modules.resolved_effects = effects;
            state.mining_required_ticks = new_required;
            state.mining_progress_ticks = new_progress;
        } else if let Some(state) = self.entities.labs.get_mut(&entity_id) {
            state.modules.resolved_effects = effects;
            state.required_ticks = new_required;
            state.progress_ticks = new_progress;
        }

        self.invalidate_consumer_power_demand(entity_id);
        self.refresh_pollution_emitter(entity_id);
    }

    pub(super) fn refresh_machines_covered_by_beacon(&mut self, beacon_id: EntityId) {
        let targets = beacon_target_ids(self, beacon_id);
        for entity_id in targets {
            self.refresh_module_effects(entity_id);
        }
    }

    pub(super) fn refresh_machines_in_beacon_region(
        &mut self,
        footprint: EntityFootprint,
        radius: u16,
    ) {
        let rect = beacon_effect_rect_for_footprint(&footprint, radius);
        let targets = machine_ids_in_effect_rect(self, rect);
        for entity_id in targets {
            self.refresh_module_effects(entity_id);
        }
    }
}

fn rescale_progress(progress: u32, old_required: u32, new_required: u32) -> u32 {
    if old_required == 0 || new_required == 0 {
        return 0;
    }
    (u64::from(progress)
        .saturating_mul(u64::from(new_required))
        .checked_div(u64::from(old_required))
        .unwrap_or(0)
        .min(u64::from(new_required.saturating_sub(1)))) as u32
}

pub(in crate::simulation) fn resolve_machine_module_effects(
    sim: &Simulation,
    entity_id: EntityId,
    placed: &PlacedEntity,
) -> ResolvedModuleEffects {
    let mut resolved = ResolvedModuleEffects::default();
    if let Some(slots) = sim
        .entities
        .machine_module_state(entity_id)
        .map(|modules| &modules.slots)
    {
        add_slot_effects(
            &mut resolved,
            slots,
            &sim.world.prototypes,
            DIRECT_MODULE_STRENGTH_PERMYRIAD,
        );
    }

    for beacon_id in candidate_covering_beacons(sim, placed) {
        let Some(beacon_placed) = sim.entities.placed_entity(beacon_id) else {
            continue;
        };
        let Some(beacon_prototype) = sim.world.prototypes.entity(beacon_placed.prototype_id) else {
            continue;
        };
        let Some(beacon) = beacon_prototype.beacon else {
            continue;
        };
        if !beacon_effect_rect(beacon_placed, beacon.effect_radius_tiles)
            .intersects(&placed.footprint)
        {
            continue;
        }
        if let Some(state) = sim.entities.beacons.get(&beacon_id) {
            add_slot_effects(
                &mut resolved,
                &state.slots,
                &sim.world.prototypes,
                beacon.transmission_permyriad,
            );
        }
    }
    resolved
}

fn add_slot_effects(
    resolved: &mut ResolvedModuleEffects,
    slots: &ModuleSlots,
    catalog: &PrototypeCatalog,
    strength_permyriad: u16,
) {
    for stack in slots.slots().iter().filter_map(|slot| slot.stack()) {
        if let Some(effect) = catalog
            .item(stack.item_id())
            .and_then(|item| item.module_effect)
        {
            resolved.add_effect(effect, strength_permyriad);
        }
    }
}

fn candidate_covering_beacons(sim: &Simulation, machine: &PlacedEntity) -> BTreeSet<EntityId> {
    let max_radius = i64::from(sim.world.max_beacon_effect_radius_tiles);
    let max_x = machine.x + i64::from(machine.footprint.width) - 1;
    let max_y = machine.y + i64::from(machine.footprint.height) - 1;
    sim.entities.occupancy().entity_ids_in_tile_rect(
        machine.x.saturating_sub(max_radius),
        max_x.saturating_add(max_radius),
        machine.y.saturating_sub(max_radius),
        max_y.saturating_add(max_radius),
    )
}

fn beacon_target_ids(sim: &Simulation, beacon_id: EntityId) -> BTreeSet<EntityId> {
    let Some(placed) = sim.entities.placed_entity(beacon_id) else {
        return BTreeSet::new();
    };
    let Some(beacon) = sim
        .world
        .prototypes
        .entity(placed.prototype_id)
        .and_then(|prototype| prototype.beacon)
    else {
        return BTreeSet::new();
    };
    let rect = beacon_effect_rect(placed, beacon.effect_radius_tiles);
    machine_ids_in_effect_rect(sim, rect)
}

fn machine_ids_in_effect_rect(sim: &Simulation, rect: TileRect) -> BTreeSet<EntityId> {
    sim.entities
        .occupancy()
        .entity_ids_in_tile_rect(rect.min_x, rect.max_x, rect.min_y, rect.max_y)
        .into_iter()
        .filter(|entity_id| {
            sim.entities
                .machine_module_state(*entity_id)
                .is_some_and(|modules| {
                    !modules.slots.is_empty()
                        && sim
                            .entities
                            .placed_entity(*entity_id)
                            .is_some_and(|target| rect.intersects(&target.footprint))
                })
        })
        .collect()
}

#[derive(Clone, Copy)]
struct TileRect {
    min_x: WorldTileCoord,
    max_x: WorldTileCoord,
    min_y: WorldTileCoord,
    max_y: WorldTileCoord,
}

impl TileRect {
    fn intersects(self, footprint: &EntityFootprint) -> bool {
        let max_x = footprint.x + i64::from(footprint.width) - 1;
        let max_y = footprint.y + i64::from(footprint.height) - 1;
        self.min_x <= max_x
            && self.max_x >= footprint.x
            && self.min_y <= max_y
            && self.max_y >= footprint.y
    }
}

fn beacon_effect_rect(placed: &PlacedEntity, radius: u16) -> TileRect {
    beacon_effect_rect_for_footprint(&placed.footprint, radius)
}

fn beacon_effect_rect_for_footprint(footprint: &EntityFootprint, radius: u16) -> TileRect {
    let radius = i64::from(radius);
    TileRect {
        min_x: footprint.x.saturating_sub(radius),
        max_x: (footprint.x + i64::from(footprint.width) - 1).saturating_add(radius),
        min_y: footprint.y.saturating_sub(radius),
        max_y: (footprint.y + i64::from(footprint.height) - 1).saturating_add(radius),
    }
}
