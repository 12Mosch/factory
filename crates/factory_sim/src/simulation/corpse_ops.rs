use super::*;

impl Simulation {
    pub fn corpses(&self) -> impl Iterator<Item = &PlayerCorpse> {
        self.corpses.values()
    }

    pub fn corpse(&self, id: u64) -> Option<&PlayerCorpse> {
        self.corpses.get(&id)
    }

    pub fn can_reach_corpse(&self, id: u64) -> bool {
        !self.player.is_dead()
            && self.corpse(id).is_some_and(|corpse| {
                self.is_manual_mining_target_in_reach(ManualMiningTarget {
                    x: corpse.x,
                    y: corpse.y,
                })
            })
    }

    pub(super) fn create_player_corpse(&mut self) {
        let (x, y) = self.player.tile_position();
        let mut corpse = PlayerCorpse {
            id: self.player_deaths(),
            created_tick: self.tick,
            x,
            y,
            items: Vec::new(),
            weapon: std::mem::take(&mut self.player_weapon),
            repair_remaining_health: std::mem::take(&mut self.player.repair_remaining_health),
        };
        let catalog = &self.world.prototypes;
        let mut add = |item, count| {
            corpse.items.push(
                ItemAmount::new(catalog, item, count)
                    .expect("corpse contents originate from validated owned items"),
            );
        };
        let inventory = std::mem::replace(&mut self.player_inventory, Inventory::player());
        for stack in inventory.slots().iter().filter_map(|slot| slot.stack()) {
            add(stack.item_id(), u64::from(stack.count()));
        }
        // Dismantle armor into ordinary items. No equipped state remains on the
        // respawned player; energy and robot charge are the explicit penalty.
        let equipment = std::mem::take(&mut self.player_equipment);
        if let Some(armor) = equipment.equipped_armor {
            add(armor, 1);
        }
        for installed in equipment.installed {
            add(installed.item_id, 1);
        }
        // Refund reservations directly to unbounded corpse storage, never into
        // an inventory that might be full. Preserve monotonic crafting job IDs.
        for job in self.crafting_queue.entries.drain(..) {
            let recipe = catalog
                .recipe(job.recipe_id)
                .expect("queued craft references a recipe");
            for ingredient in &recipe.ingredients {
                add(ingredient.item, u64::from(ingredient.amount));
            }
        }
        let personal_ids = self
            .robot_flights
            .robots
            .values()
            .filter(|robot| robot.personal)
            .map(|robot| robot.id)
            .collect::<Vec<_>>();
        for id in personal_ids {
            let robot = self
                .robot_flights
                .robots
                .remove(&id)
                .expect("personal robot was just found");
            add(robot.item_id, 1);
            for stack in robot.payload.into_iter().chain(robot.cargo) {
                add(stack.item_id(), u64::from(stack.count()));
            }
            for amount in robot.bulk_cargo {
                add(amount.item_id(), amount.count());
            }
        }
        self.apply_equipped_armor_resistances();
        // Normal reconciliation releases missing robots' reservations and puts
        // still-valid work back in the world queue for stationary networks.
        self.reconcile_construction_jobs();
        if !corpse.is_empty() {
            self.corpses.insert(corpse.id, corpse);
        }
    }

    pub fn recover_corpse(&mut self, id: u64) -> Result<(), CorpseRecoveryError> {
        if self.player.is_dead() {
            return Err(CorpseRecoveryError::PlayerDead);
        }
        if !self.corpses.contains_key(&id) {
            return Err(CorpseRecoveryError::MissingCorpse);
        }
        if !self.can_reach_corpse(id) {
            return Err(CorpseRecoveryError::OutOfReach);
        }
        let corpse = self
            .corpses
            .get_mut(&id)
            .expect("recovery checked corpse identity");
        let mut changed = false;
        let catalog = &self.world.prototypes;
        corpse.items.retain_mut(|amount| {
            let accepted = self
                .player_inventory
                .insert_partial_amount(catalog, *amount)
                .expect("stored corpse amount is validated");
            changed |= accepted > 0;
            let remaining = amount.count() - accepted;
            if remaining == 0 {
                return false;
            }
            *amount = ItemAmount::new(catalog, amount.item_id(), remaining)
                .expect("partial recovery preserves a valid amount");
            true
        });
        // Opened magazines and repair packs are never converted into full items
        // or overwritten. Conflicting consumables remain in the corpse.
        if corpse.weapon.loaded_shots > 0
            && self.player_weapon.loaded_shots == 0
            && corpse
                .weapon
                .selected_weapon
                .is_some_and(|weapon| self.player_inventory.count(weapon) > 0)
        {
            let next_ready = self.player_weapon.next_ready_tick;
            let origin = self.player_weapon.cooldown_origin;
            self.player_weapon = std::mem::take(&mut corpse.weapon);
            if next_ready > self.player_weapon.next_ready_tick {
                self.player_weapon.next_ready_tick = next_ready;
                self.player_weapon.cooldown_origin = origin;
            }
            changed = true;
        }
        if corpse.repair_remaining_health > 0 && self.player.repair_remaining_health == 0 {
            self.player.repair_remaining_health =
                std::mem::take(&mut corpse.repair_remaining_health);
            changed = true;
        }
        if corpse.is_empty() {
            self.corpses.remove(&id);
        }
        if changed {
            Ok(())
        } else {
            Err(CorpseRecoveryError::NoCapacity)
        }
    }
}

pub(super) fn validate_corpses(sim: &Simulation) -> Result<(), SimValidationError> {
    for (&id, corpse) in &sim.corpses {
        if id == 0
            || id != corpse.id
            || id > sim.player_deaths()
            || corpse.created_tick > sim.tick
            || sim.world.tile_at(corpse.x, corpse.y).is_none()
            || corpse.repair_remaining_health
                > sim
                    .catalog()
                    .items()
                    .iter()
                    .filter_map(|item| item.repair.map(|repair| repair.restore_health))
                    .max()
                    .unwrap_or(0)
            || corpse.is_empty()
        {
            return Err(SimValidationError::InvalidPlayerCorpse { corpse_id: id });
        }
        for amount in &corpse.items {
            ItemAmount::new(sim.catalog(), amount.item_id(), amount.count())
                .map_err(|_| SimValidationError::InvalidPlayerCorpse { corpse_id: id })?;
        }
        super::combat_ops::validate_weapon_state(sim, corpse.weapon)?;
    }
    Ok(())
}
