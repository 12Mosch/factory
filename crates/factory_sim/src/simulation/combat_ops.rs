use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
struct AccumulatedDamage {
    amount: u64,
    retaliation_target: Option<EntityId>,
}

impl Simulation {
    /// Selects the next distinct weapon currently carried by the player.
    pub fn cycle_player_weapon(&mut self) -> Result<ItemId, PlayerWeaponError> {
        let mut weapons = Vec::new();
        for item_id in self
            .player_inventory
            .slots()
            .iter()
            .filter_map(|slot| slot.stack())
            .map(|stack| stack.item_id())
        {
            if self
                .world
                .prototypes
                .item(item_id)
                .is_some_and(|item| item.weapon.is_some())
                && !weapons.contains(&item_id)
            {
                weapons.push(item_id);
            }
        }
        let selected = match self.player_weapon.selected_weapon {
            Some(current) => weapons
                .iter()
                .position(|item_id| *item_id == current)
                .map_or(weapons.first().copied(), |index| {
                    weapons.get((index + 1) % weapons.len()).copied()
                }),
            None => weapons.first().copied(),
        }
        .ok_or(PlayerWeaponError::NoWeaponsAvailable)?;
        let selected_category = self
            .world
            .prototypes
            .item(selected)
            .and_then(|item| item.weapon)
            .map(|weapon| weapon.ammo_category)
            .expect("selected inventory item was verified as a weapon");
        let loaded_category = self.player_weapon.loaded_ammo.and_then(|item_id| {
            self.world
                .prototypes
                .item(item_id)
                .and_then(|item| item.ammo)
                .map(|ammo| ammo.category)
        });
        if loaded_category.is_some_and(|category| category != selected_category) {
            self.player_weapon.loaded_ammo = None;
            self.player_weapon.loaded_shots = 0;
            self.player_weapon.loaded_damage = Damage::physical(0);
        }
        self.player_weapon.selected_weapon = Some(selected);
        Ok(selected)
    }

    /// Commits one deterministic personal-weapon attack against the hostile
    /// combatant occupying the aimed tile.
    pub fn attack_with_player_weapon(
        &mut self,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) -> Result<CombatantId, PlayerWeaponError> {
        let weapon_item = self
            .player_weapon
            .selected_weapon
            .ok_or(PlayerWeaponError::NoWeaponSelected)?;
        if self.player_inventory.count(weapon_item) == 0 {
            return Err(PlayerWeaponError::WeaponUnavailable(weapon_item));
        }
        let weapon = self
            .world
            .prototypes
            .item(weapon_item)
            .and_then(|item| item.weapon)
            .ok_or(PlayerWeaponError::WeaponUnavailable(weapon_item))?;
        let remaining = self.player_weapon.next_ready_tick.saturating_sub(self.tick);
        if remaining > 0 {
            return Err(PlayerWeaponError::CoolingDown {
                remaining_ticks: u32::try_from(remaining).unwrap_or(u32::MAX),
            });
        }

        let target = self
            .hostile_target_at_tile(x, y)
            .ok_or(PlayerWeaponError::NoHostileTarget)?;
        let player_tile = self.player.tile_position();
        let player_footprint = EntityFootprint::single_tile(player_tile.0, player_tile.1);
        let target_footprint = self
            .combatant_footprint(target)
            .ok_or(PlayerWeaponError::NoHostileTarget)?;
        if player_footprint.chebyshev_distance_to(&target_footprint) > i64::from(weapon.range_tiles)
        {
            return Err(PlayerWeaponError::OutOfRange {
                range_tiles: weapon.range_tiles,
            });
        }

        if self.player_weapon.loaded_shots == 0 {
            load_player_magazine(
                &self.world.prototypes,
                &mut self.player_inventory,
                &mut self.player_weapon,
                weapon.ammo_category,
            )?;
        }

        let damage = self.player_weapon.loaded_damage;
        self.player_weapon.loaded_shots -= 1;
        if self.player_weapon.loaded_shots == 0 {
            self.player_weapon.loaded_ammo = None;
            self.player_weapon.loaded_damage = Damage::physical(0);
        }
        self.player_weapon.next_ready_tick = self.tick + u64::from(weapon.cooldown_ticks);
        self.player_weapon.cooldown_origin = Some(weapon_item);

        let source = CombatSource::new(CombatantId::Player, Faction::Player);
        let mut commands = CombatCommandBuffer::default();
        match weapon.delivery {
            factory_data::WeaponDeliveryPrototype::Hitscan => commands.attack(
                source,
                target,
                AttackDefinition::hitscan(damage, weapon.cooldown_ticks, weapon.range_tiles),
            ),
            factory_data::WeaponDeliveryPrototype::Shotgun {
                pellet_count,
                cone_half_width_permyriad,
            } => {
                let mut targets = self.hostile_combatants_in_cone(
                    player_tile,
                    (x, y),
                    weapon.range_tiles,
                    cone_half_width_permyriad,
                );
                if !targets.contains(&target) {
                    targets.insert(0, target);
                }
                for pellet in 0..usize::from(pellet_count) {
                    commands.push(CombatCommand {
                        source,
                        target: targets[pellet % targets.len()],
                        damage,
                    });
                }
            }
            factory_data::WeaponDeliveryPrototype::Rocket {
                speed_fixed_per_tick,
                explosion_radius_tiles,
            } => {
                self.launch_player_rocket(
                    source,
                    (x, y),
                    damage,
                    speed_fixed_per_tick,
                    explosion_radius_tiles,
                );
            }
            factory_data::WeaponDeliveryPrototype::Flame {
                cone_half_width_permyriad,
                burn_duration_ticks,
                burn_interval_ticks,
            } => {
                let mut targets = self.hostile_combatants_in_cone(
                    player_tile,
                    (x, y),
                    weapon.range_tiles,
                    cone_half_width_permyriad,
                );
                if !targets.contains(&target) {
                    targets.insert(0, target);
                }
                for cone_target in targets {
                    commands.push(CombatCommand {
                        source,
                        target: cone_target,
                        damage,
                    });
                    self.apply_burning(
                        cone_target,
                        source,
                        Damage::new(damage.amount, DamageType::Fire),
                        burn_duration_ticks,
                        burn_interval_ticks,
                    );
                }
            }
        }
        self.resolve_combat_commands(commands);
        Ok(target)
    }

    /// Returns a read-only weapon summary with compatible reserve ammunition.
    pub fn player_weapon_status(&self) -> PlayerWeaponStatus {
        let selected = self.player_weapon.selected_weapon;
        let category = selected.and_then(|item_id| {
            self.world
                .prototypes
                .item(item_id)
                .and_then(|item| item.weapon)
                .map(|weapon| weapon.ammo_category)
        });
        let reserve_shots = category.map_or(0, |category| {
            self.player_inventory
                .slots()
                .iter()
                .filter_map(|slot| slot.stack())
                .filter_map(|stack| {
                    self.world
                        .prototypes
                        .item(stack.item_id())
                        .and_then(|item| item.ammo)
                        .filter(|ammo| ammo.category == category)
                        .map(|ammo| u64::from(stack.count()) * u64::from(ammo.shots_per_item))
                })
                .sum()
        });
        PlayerWeaponStatus {
            selected_weapon: selected,
            loaded_ammo: self.player_weapon.loaded_ammo,
            loaded_shots: self.player_weapon.loaded_shots,
            reserve_shots,
            cooldown_remaining_ticks: u32::try_from(
                self.player_weapon.next_ready_tick.saturating_sub(self.tick),
            )
            .unwrap_or(u32::MAX),
        }
    }

    /// Resolves the deterministic hostile combatant occupying an aimed tile.
    fn hostile_target_at_tile(&self, x: WorldTileCoord, y: WorldTileCoord) -> Option<CombatantId> {
        self.enemies
            .enemies
            .values()
            .find(|enemy| enemy.tile() == (x, y) && Faction::Player.is_hostile_to(enemy.faction()))
            .map(|enemy| CombatantId::Enemy(enemy.id))
            .or_else(|| {
                let entity_id = self.entities.occupancy.entity_at(x, y)?;
                Faction::Player
                    .is_hostile_to(self.faction_of(CombatantId::Entity(entity_id))?)
                    .then_some(CombatantId::Entity(entity_id))
            })
    }

    /// Returns the occupied footprint used for personal-weapon range checks.
    fn combatant_footprint(&self, target: CombatantId) -> Option<EntityFootprint> {
        match target {
            CombatantId::Player => {
                let (x, y) = self.player.tile_position();
                Some(EntityFootprint::single_tile(x, y))
            }
            CombatantId::Entity(entity_id) => self
                .entities
                .placed_entities
                .get(&entity_id)
                .map(|placed| placed.footprint),
            CombatantId::Enemy(enemy_id) => self.enemies.enemies.get(&enemy_id).map(|enemy| {
                let (x, y) = enemy.tile();
                EntityFootprint::single_tile(x, y)
            }),
        }
    }

    /// Durable delayed-combat state used by rendering, diagnostics, and tests.
    pub fn delayed_combat_state(&self) -> &DelayedCombatState {
        &self.delayed_combat
    }

    /// Finds all hostile targets in a fixed-point-free cone. Candidate and
    /// result iteration are stable, while primary-line targets sort first.
    fn hostile_combatants_in_cone(
        &self,
        origin: (WorldTileCoord, WorldTileCoord),
        aim: (WorldTileCoord, WorldTileCoord),
        range_tiles: u32,
        cone_half_width_permyriad: u16,
    ) -> Vec<CombatantId> {
        let aim_x = i128::from(aim.0) - i128::from(origin.0);
        let aim_y = i128::from(aim.1) - i128::from(origin.1);
        let origin_footprint = EntityFootprint::single_tile(origin.0, origin.1);
        let mut candidates = Vec::new();
        for (&enemy_id, enemy) in &self.enemies.enemies {
            if Faction::Player.is_hostile_to(enemy.faction()) {
                candidates.push(CombatantId::Enemy(enemy_id));
            }
        }
        for (entity_id, health) in self.entities.entity_health.iter() {
            if Faction::Player.is_hostile_to(health.faction) {
                candidates.push(CombatantId::Entity(*entity_id));
            }
        }

        let mut in_cone = candidates
            .into_iter()
            .filter_map(|candidate| {
                let footprint = self.combatant_footprint(candidate)?;
                if origin_footprint.chebyshev_distance_to(&footprint) > i64::from(range_tiles) {
                    return None;
                }
                let center_x =
                    i128::from(footprint.x) + i128::from(footprint.width.saturating_sub(1)) / 2;
                let center_y =
                    i128::from(footprint.y) + i128::from(footprint.height.saturating_sub(1)) / 2;
                let target_x = center_x - i128::from(origin.0);
                let target_y = center_y - i128::from(origin.1);
                let dot = aim_x
                    .saturating_mul(target_x)
                    .saturating_add(aim_y.saturating_mul(target_y));
                if dot <= 0 {
                    return None;
                }
                let cross = aim_x
                    .saturating_mul(target_y)
                    .saturating_sub(aim_y.saturating_mul(target_x))
                    .unsigned_abs();
                if cross.saturating_mul(10_000)
                    > dot
                        .unsigned_abs()
                        .saturating_mul(u128::from(cone_half_width_permyriad))
                {
                    return None;
                }
                let distance_squared = target_x
                    .unsigned_abs()
                    .saturating_mul(target_x.unsigned_abs())
                    .saturating_add(
                        target_y
                            .unsigned_abs()
                            .saturating_mul(target_y.unsigned_abs()),
                    );
                Some((cross, distance_squared, candidate))
            })
            .collect::<Vec<_>>();
        in_cone.sort_unstable();
        in_cone
            .into_iter()
            .map(|(_, _, candidate)| candidate)
            .collect()
    }

    fn launch_player_rocket(
        &mut self,
        source: CombatSource,
        impact: (WorldTileCoord, WorldTileCoord),
        damage: Damage,
        speed_fixed_per_tick: u32,
        explosion_radius_tiles: u32,
    ) {
        let id = ProjectileId::new(self.delayed_combat.next_projectile_id);
        self.delayed_combat.next_projectile_id = self
            .delayed_combat
            .next_projectile_id
            .checked_add(1)
            .expect("projectile identity space exhausted");
        let target_x_fixed = tile_center_fixed_saturating(impact.0);
        let target_y_fixed = tile_center_fixed_saturating(impact.1);
        let travel_fixed = self
            .player
            .x_fixed()
            .abs_diff(target_x_fixed)
            .max(self.player.y_fixed().abs_diff(target_y_fixed));
        let speed = u64::from(speed_fixed_per_tick);
        let travel_ticks = travel_fixed.div_ceil(speed).max(1);
        let projectile = ProjectileState {
            id,
            source,
            start_x_fixed: self.player.x_fixed(),
            start_y_fixed: self.player.y_fixed(),
            impact_x: impact.0,
            impact_y: impact.1,
            launched_tick: self.tick,
            impact_tick: self.tick.saturating_add(travel_ticks),
            speed_fixed_per_tick,
            damage,
            explosion_radius_tiles,
        };
        self.delayed_combat.projectiles.insert(id, projectile);
    }

    fn apply_burning(
        &mut self,
        target: CombatantId,
        source: CombatSource,
        damage_per_tick: Damage,
        duration_ticks: u32,
        interval_ticks: u32,
    ) {
        let proposed_next = self.tick.saturating_add(u64::from(interval_ticks));
        let proposed_expiry = self.tick.saturating_add(u64::from(duration_ticks));
        let effects = self.delayed_combat.statuses.entry(target).or_default();
        match effects.burning.as_mut() {
            Some(existing) => {
                existing.expires_tick = existing.expires_tick.max(proposed_expiry);
                if damage_per_tick.amount >= existing.damage_per_tick.amount {
                    existing.source = source;
                    existing.damage_per_tick = damage_per_tick;
                    existing.interval_ticks = interval_ticks;
                    existing.next_damage_tick = existing.next_damage_tick.min(proposed_next);
                }
            }
            None => {
                effects.burning = Some(BurningStatus {
                    source,
                    damage_per_tick,
                    interval_ticks,
                    next_damage_tick: proposed_next,
                    expires_tick: proposed_expiry,
                });
            }
        }
    }

    /// Advances fixed-destination projectiles and interval-based statuses.
    /// Both append to the tick's shared command buffer, preserving simultaneous
    /// combat semantics with enemies, turrets, and armor equipment.
    pub(super) fn advance_delayed_combat(&mut self, commands: &mut CombatCommandBuffer) {
        let arrived = self
            .delayed_combat
            .projectiles
            .iter()
            .filter_map(|(&id, projectile)| (projectile.impact_tick <= self.tick).then_some(id))
            .collect::<Vec<_>>();
        for id in arrived {
            let Some(projectile) = self.delayed_combat.projectiles.remove(&id) else {
                continue;
            };
            for target in self.hostile_combatants_in_radius(
                projectile.source.faction,
                (projectile.impact_x, projectile.impact_y),
                projectile.explosion_radius_tiles,
            ) {
                commands.push(CombatCommand {
                    source: projectile.source,
                    target,
                    damage: projectile.damage,
                });
            }
        }

        let targets = self
            .delayed_combat
            .statuses
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for target in targets {
            if self.combatant_health(target).is_none() {
                self.delayed_combat.statuses.remove(&target);
                continue;
            }
            let remove = {
                let Some(effects) = self.delayed_combat.statuses.get_mut(&target) else {
                    continue;
                };
                let Some(mut burning) = effects.burning else {
                    continue;
                };
                if burning.next_damage_tick <= self.tick
                    && burning.next_damage_tick <= burning.expires_tick
                {
                    commands.push(CombatCommand {
                        source: burning.source,
                        target,
                        damage: burning.damage_per_tick,
                    });
                    burning.next_damage_tick = burning
                        .next_damage_tick
                        .saturating_add(u64::from(burning.interval_ticks));
                }
                effects.burning = Some(burning);
                burning.next_damage_tick > burning.expires_tick
            };
            if remove {
                self.delayed_combat.statuses.remove(&target);
            }
        }
    }

    fn hostile_combatants_in_radius(
        &self,
        source_faction: Faction,
        center: (WorldTileCoord, WorldTileCoord),
        radius_tiles: u32,
    ) -> Vec<CombatantId> {
        let center = EntityFootprint::single_tile(center.0, center.1);
        let radius = i64::from(radius_tiles);
        let mut targets = Vec::new();
        if source_faction.is_hostile_to(self.player.health.faction)
            && center.chebyshev_distance_to(&self.combatant_footprint(CombatantId::Player).unwrap())
                <= radius
        {
            targets.push(CombatantId::Player);
        }
        for (&enemy_id, enemy) in &self.enemies.enemies {
            let target = CombatantId::Enemy(enemy_id);
            if source_faction.is_hostile_to(enemy.faction())
                && self
                    .combatant_footprint(target)
                    .is_some_and(|footprint| center.chebyshev_distance_to(&footprint) <= radius)
            {
                targets.push(target);
            }
        }
        for (entity_id, health) in self.entities.entity_health.iter() {
            let target = CombatantId::Entity(*entity_id);
            if source_faction.is_hostile_to(health.faction)
                && self
                    .combatant_footprint(target)
                    .is_some_and(|footprint| center.chebyshev_distance_to(&footprint) <= radius)
            {
                targets.push(target);
            }
        }
        targets.sort_unstable();
        targets
    }

    pub(super) fn nearest_hostile_to_player(&self, range_tiles: u32) -> Option<CombatantId> {
        let origin = self.player.tile_position();
        let footprint = EntityFootprint::single_tile(origin.0, origin.1);
        defensive_target_from_parts(
            &self.enemies,
            &self.enemy_target_chunks,
            &self.entities.enemy_spawners,
            &self.entities.placed_entities,
            &self.entities.occupancy,
            &footprint,
            range_tiles,
        )
    }

    /// Advances every player defensive turret from one shared target snapshot.
    pub(super) fn advance_defensive_turrets(&mut self, commands: &mut CombatCommandBuffer) {
        if self.onboarding_progress.loaded_gun_turrets == 0
            && self.entities.gun_turrets.iter().any(|(entity_id, state)| {
                self.entities.placed_entities.contains_key(entity_id)
                    && (state.loaded_shots > 0
                        || state
                            .ammo
                            .slots()
                            .iter()
                            .filter_map(|slot| slot.stack())
                            .any(|stack| {
                                self.world
                                    .prototypes
                                    .item(stack.item_id())
                                    .is_some_and(|item| item.ammo.is_some())
                            }))
            })
        {
            self.onboarding_progress
                .record_counter(|progress| &mut progress.loaded_gun_turrets, 1);
        }
        // Units move during their own simulation step, not during turret
        // fire, so one index is valid for this whole pass.
        self.enemy_target_chunks.rebuild(&self.enemies);
        {
            let tick = self.tick;
            let Simulation {
                world,
                entities,
                enemies,
                enemy_target_chunks,
                ..
            } = self;
            let placed_entities = &entities.placed_entities;
            let enemy_spawners = &entities.enemy_spawners;
            let occupancy = &entities.occupancy;

            for (&turret_id, state) in entities.gun_turrets.iter_mut() {
                if tick < state.next_ready_tick {
                    continue;
                }
                let Some(placed) = placed_entities.get(&turret_id) else {
                    continue;
                };
                let Some(turret) = world
                    .prototypes
                    .entity(placed.prototype_id)
                    .and_then(|prototype| prototype.gun_turret)
                else {
                    continue;
                };

                if state.loaded_shots == 0 {
                    load_magazine(world, state, turret.ammo_category);
                }
                if state.loaded_shots == 0 {
                    continue;
                }

                let target = defensive_target_from_parts(
                    enemies,
                    enemy_target_chunks,
                    enemy_spawners,
                    placed_entities,
                    occupancy,
                    &placed.footprint,
                    turret.range_tiles,
                );

                if let Some(target) = target {
                    let attack = AttackDefinition::hitscan(
                        state.loaded_damage,
                        turret.cooldown_ticks,
                        turret.range_tiles,
                    );
                    commands.attack(
                        CombatSource {
                            owner: CombatantId::Entity(turret_id),
                            faction: Faction::Player,
                        },
                        target,
                        attack,
                    );
                    state.loaded_shots -= 1;
                    state.next_ready_tick = tick + u64::from(attack.cooldown_ticks);
                }
            }
        }

        self.advance_laser_turrets(commands);
    }

    /// Advances every laser turret, gating fire on available electric power and
    /// invalidating power demand whenever a turret changes engagement state.
    fn advance_laser_turrets(&mut self, commands: &mut CombatCommandBuffer) {
        let mut demand_changed = Vec::new();
        {
            let Simulation {
                world,
                entities,
                enemies,
                enemy_target_chunks,
                power,
                ..
            } = self;
            let placed_entities = &entities.placed_entities;
            let enemy_spawners = &entities.enemy_spawners;
            let occupancy = &entities.occupancy;
            let electric_consumers = &mut entities.electric_consumers;

            for (&turret_id, state) in entities.laser_turrets.iter_mut() {
                let Some(placed) = placed_entities.get(&turret_id) else {
                    continue;
                };
                let Some(turret) = world
                    .prototypes
                    .entity(placed.prototype_id)
                    .and_then(|prototype| prototype.laser_turret)
                else {
                    continue;
                };
                let target = defensive_target_from_parts(
                    enemies,
                    enemy_target_chunks,
                    enemy_spawners,
                    placed_entities,
                    occupancy,
                    &placed.footprint,
                    turret.range_tiles,
                );

                let Some(target) = target else {
                    if state.engaged {
                        state.engaged = false;
                        state.cooldown_remaining_ticks = 0;
                        demand_changed.push(turret_id);
                    }
                    continue;
                };
                if !state.engaged {
                    state.engaged = true;
                    state.cooldown_remaining_ticks = 0;
                    demand_changed.push(turret_id);
                    // Power for active usage was not included in this tick's
                    // accounting. Firing starts after the next accounting pass.
                    continue;
                }
                if !electric_work_allowed_for(power, electric_consumers, turret_id) {
                    continue;
                }
                if state.cooldown_remaining_ticks > 0 {
                    state.cooldown_remaining_ticks -= 1;
                    if state.cooldown_remaining_ticks > 0 {
                        continue;
                    }
                }
                commands.attack(
                    CombatSource {
                        owner: CombatantId::Entity(turret_id),
                        faction: Faction::Player,
                    },
                    target,
                    AttackDefinition::hitscan(
                        Damage::new(turret.damage, DamageType::Laser),
                        turret.cooldown_ticks,
                        turret.range_tiles,
                    ),
                );
                state.cooldown_remaining_ticks = turret.cooldown_ticks;
            }
        }
        for entity_id in demand_changed {
            self.invalidate_consumer_power_demand(entity_id);
        }
    }

    #[cfg(test)]
    pub(super) fn advance_gun_turrets(&mut self, commands: &mut CombatCommandBuffer) {
        self.advance_defensive_turrets(commands);
    }

    /// Applies every attack committed from the tick's pre-resolution combat
    /// snapshot. A combatant destroyed here still contributes its own queued
    /// attack, so neither side receives an initiative advantage.
    pub fn resolve_combat_commands(&mut self, commands: CombatCommandBuffer) {
        let mut accumulated = BTreeMap::<CombatantId, AccumulatedDamage>::new();
        for command in commands.iter().copied() {
            let Some(target_health) = self.combatant_health(command.target) else {
                continue;
            };
            if !command.source.faction.is_hostile_to(target_health.faction) {
                continue;
            }
            let amount = command.damage.after_resistance(&target_health.resistances);
            if amount == 0 {
                continue;
            }
            let target_damage = accumulated.entry(command.target).or_default();
            target_damage.amount = target_damage.amount.saturating_add(u64::from(amount));
            if let CombatantId::Entity(entity_id) = command.source.owner {
                target_damage.retaliation_target = Some(
                    target_damage
                        .retaliation_target
                        .map_or(entity_id, |current| current.min(entity_id)),
                );
            }
        }

        for (target, damage) in accumulated {
            let amount = u32::try_from(damage.amount).unwrap_or(u32::MAX);
            match target {
                CombatantId::Player => {
                    let amount = self.absorb_player_damage_with_shields(amount);
                    self.player.health.current = self.player.health.current.saturating_sub(amount);
                }
                CombatantId::Entity(entity_id) => {
                    self.apply_entity_damage(entity_id, amount);
                }
                CombatantId::Enemy(enemy_id) => {
                    let Some(enemy) = self.enemies.enemies.get_mut(&enemy_id) else {
                        continue;
                    };
                    enemy.health.current = enemy.health.current.saturating_sub(amount);
                    if enemy.health.current == 0 {
                        self.enemies.enemies.remove(&enemy_id);
                    } else if let Some(retaliation_target) = damage.retaliation_target {
                        // Being fired on pulls a surviving unit onto the
                        // lowest-ID structure that participated in the volley.
                        enemy.target = Some(retaliation_target);
                        enemy.path.clear();
                    }
                }
            }
        }
        let stale_statuses = self
            .delayed_combat
            .statuses
            .keys()
            .copied()
            .filter(|target| self.combatant_health(*target).is_none())
            .collect::<Vec<_>>();
        for target in stale_statuses {
            self.delayed_combat.statuses.remove(&target);
        }
    }

    fn combatant_health(&self, combatant: CombatantId) -> Option<&HealthState> {
        match combatant {
            CombatantId::Player => Some(&self.player.health),
            CombatantId::Entity(entity_id) => self.entities.entity_health.get(&entity_id),
            CombatantId::Enemy(enemy_id) => self
                .enemies
                .enemies
                .get(&enemy_id)
                .map(|enemy| &enemy.health),
        }
    }

    pub fn combatant_health_state(&self, combatant: CombatantId) -> Option<HealthState> {
        self.combatant_health(combatant).copied()
    }

    pub fn faction_of(&self, combatant: CombatantId) -> Option<Faction> {
        self.combatant_health(combatant)
            .map(|health| health.faction)
    }

    /// Applies damage to a placed entity's health; at zero the entity is
    /// violently destroyed (no item recovery). Entities without health state
    /// are indestructible. Zero damage is a no-op. Returns true when the entity
    /// was destroyed.
    pub fn damage_entity(&mut self, entity_id: EntityId, amount: u32) -> bool {
        self.damage_entity_with(entity_id, Damage::physical(amount))
    }

    /// Applies typed damage without a faction check. Environmental damage and
    /// scripted effects use this path; attacks should use a command buffer.
    pub fn damage_entity_with(&mut self, entity_id: EntityId, damage: Damage) -> bool {
        let Some(health) = self.entities.entity_health.get(&entity_id) else {
            return false;
        };
        let amount = damage.after_resistance(&health.resistances);
        self.apply_entity_damage(entity_id, amount)
    }

    fn apply_entity_damage(&mut self, entity_id: EntityId, amount: u32) -> bool {
        // Entities without health state shrug the hit off entirely, so they
        // must not raise an under-attack alarm either.
        if amount == 0 || !self.entities.entity_health.contains_key(&entity_id) {
            return false;
        }
        let warning_location = self
            .entities
            .placed_entities
            .get(&entity_id)
            .and_then(|placed| {
                self.world
                    .prototypes
                    .entity(placed.prototype_id)
                    .filter(|prototype| {
                        !matches!(
                            prototype.entity_kind,
                            EntityKind::EnemySpawner | EntityKind::ResourcePatch
                        )
                    })
                    .map(|_| (placed.x, placed.y))
            });
        let Some(health) = self.entities.entity_health.get_mut(&entity_id) else {
            return false;
        };
        health.current = health.current.saturating_sub(amount);
        let destroyed = health.current == 0;

        if let Some((x, y)) = warning_location {
            self.emit_structure_damage_warning(x, y);
        }

        if destroyed {
            entity_mutation::remove(self, entity_id);
        }
        destroyed
    }

    /// Player repair action: consumes repair pack durability to restore a
    /// nearby entity's health. The app repeats this command while the repair
    /// input is held; a fully repaired target is a no-op success.
    pub fn repair_entity(&mut self, entity_id: EntityId) -> Result<(), RepairError> {
        let placed = self
            .entities
            .placed_entities
            .get(&entity_id)
            .ok_or(RepairError::MissingEntity(entity_id))?;
        let max_health = self
            .entities
            .entity_health
            .get(&entity_id)
            .map(|health| health.maximum)
            .ok_or(RepairError::NotRepairable(entity_id))?;
        let footprint = placed.footprint;

        let (player_tile_x, player_tile_y) = self.player.tile_position();
        let player_footprint = EntityFootprint::single_tile(player_tile_x, player_tile_y);
        let reach = REPAIR_REACH_TILES as i64;
        if player_footprint.chebyshev_distance_to(&footprint) > reach {
            return Err(RepairError::OutOfReach);
        }

        let current = self
            .entities
            .entity_health
            .get(&entity_id)
            .map(|health| health.current)
            .unwrap_or(max_health);
        if current >= max_health {
            return Ok(());
        }

        if self.player.repair_remaining_health == 0 {
            let repair_item = self
                .player_inventory
                .slots()
                .iter()
                .filter_map(|slot| slot.stack())
                .map(|stack| stack.item_id())
                .find(|item_id| {
                    self.world
                        .prototypes
                        .item(*item_id)
                        .is_some_and(|item| item.repair.is_some())
                })
                .ok_or(RepairError::NoRepairPacks)?;
            let restore_health = repair_restore_health(&self.world.prototypes, repair_item)
                .expect("repair item was selected for its repair prototype");
            self.player_inventory
                .remove(repair_item, 1)
                .expect("repair pack was just found in the player inventory");
            self.player.repair_remaining_health = restore_health;
        }

        let heal = REPAIR_HEALTH_PER_ACTION.min(self.player.repair_remaining_health);
        let restored = self.restore_entity_health(entity_id, heal);
        self.player.repair_remaining_health -= restored;
        if self
            .entities
            .entity_health
            .get(&entity_id)
            .is_some_and(|health| health.current == health.maximum)
        {
            robot_ops::cancel_construction_job(self, ConstructionJob::Repair(entity_id));
            self.refresh_robot_network_work_counts();
        }

        Ok(())
    }

    /// Restores at most `amount` health without exceeding the entity maximum.
    /// Player repair and construction robots share this clamp so neither path
    /// can create over-health when damage changes between validation and use.
    pub(in crate::simulation) fn restore_entity_health(
        &mut self,
        entity_id: EntityId,
        amount: u32,
    ) -> u32 {
        let Some(health) = self.entities.entity_health.get_mut(&entity_id) else {
            return 0;
        };
        let restored = amount.min(health.maximum.saturating_sub(health.current));
        health.current += restored;
        restored
    }

    /// Current and maximum health of an entity, when it is damageable.
    pub fn entity_health(&self, entity_id: EntityId) -> Option<(u32, u32)> {
        let health = self.entities.entity_health.get(&entity_id)?;
        Some((health.current, health.maximum))
    }

    pub fn player_health(&self) -> (u32, u32) {
        (self.player.health.current, self.player.health.maximum)
    }
}

pub(in crate::simulation) fn repair_restore_health(
    catalog: &PrototypeCatalog,
    item_id: ItemId,
) -> Option<u32> {
    catalog
        .item(item_id)
        .and_then(|item| item.repair)
        .map(|repair| repair.restore_health)
}

fn defensive_target_from_parts(
    enemies: &EnemySubsystem,
    enemy_chunks: &EnemyChunkIndex,
    enemy_spawners: &BTreeMap<EntityId, EnemySpawnerState>,
    placed_entities: &BTreeMap<EntityId, PlacedEntity>,
    occupancy: &OccupancyGrid,
    footprint: &EntityFootprint,
    range_tiles: u32,
) -> Option<CombatantId> {
    let range = i64::from(range_tiles);
    nearest_enemy_in_range(enemies, enemy_chunks, footprint, range)
        .map(CombatantId::Enemy)
        .or_else(|| {
            nearest_spawner_in_range(enemy_spawners, placed_entities, occupancy, footprint, range)
                .map(CombatantId::Entity)
        })
}

/// Breaks one magazine out of the turret's ammo inventory into loose shots.
fn load_magazine(
    world: &WorldSim,
    state: &mut GunTurretState,
    category: factory_data::AmmoCategory,
) {
    let Some((item_id, ammo)) = state
        .ammo
        .slots()
        .iter()
        .filter_map(|slot| slot.stack())
        .find_map(|stack| {
            world
                .prototypes
                .item(stack.item_id())
                .and_then(|item| item.ammo)
                .filter(|ammo| ammo.category == category)
                .map(|ammo| (stack.item_id(), ammo))
        })
    else {
        return;
    };
    if state.ammo.remove(item_id, 1).is_err() {
        return;
    }
    state.loaded_shots = ammo.shots_per_item;
    state.loaded_damage = Damage::new(ammo.damage_per_shot, ammo.damage_type);
}

/// Consumes the first compatible magazine in stable inventory-slot order.
fn load_player_magazine(
    catalog: &PrototypeCatalog,
    inventory: &mut Inventory,
    state: &mut PlayerWeaponState,
    category: factory_data::AmmoCategory,
) -> Result<(), PlayerWeaponError> {
    let (item_id, ammo) = inventory
        .slots()
        .iter()
        .filter_map(|slot| slot.stack())
        .find_map(|stack| {
            catalog
                .item(stack.item_id())
                .and_then(|item| item.ammo)
                .filter(|ammo| ammo.category == category)
                .map(|ammo| (stack.item_id(), ammo))
        })
        .ok_or(PlayerWeaponError::NoAmmunition)?;
    inventory
        .remove(item_id, 1)
        .expect("compatible ammunition was just found in the player inventory");
    state.loaded_ammo = Some(item_id);
    state.loaded_shots = ammo.shots_per_item;
    state.loaded_damage = Damage::new(ammo.damage_per_shot, ammo.damage_type);
    Ok(())
}

fn tile_center_fixed_saturating(tile: WorldTileCoord) -> i64 {
    tile.saturating_mul(POSITION_SCALE)
        .saturating_add(POSITION_SCALE / 2)
}

/// Validates the durable selected weapon and canonical opened-magazine state.
pub(super) fn validate_player_weapon_state(sim: &Simulation) -> Result<(), SimValidationError> {
    let state = sim.player_weapon;
    let Some(weapon_item) = state.selected_weapon else {
        return if state.loaded_ammo.is_none()
            && state.loaded_shots == 0
            && state.loaded_damage == Damage::physical(0)
            && state.next_ready_tick == 0
            && state.cooldown_origin.is_none()
        {
            Ok(())
        } else {
            Err(SimValidationError::InvalidPlayerWeaponState)
        };
    };
    let weapon = sim
        .world
        .prototypes
        .item(weapon_item)
        .and_then(|item| item.weapon)
        .ok_or(SimValidationError::InvalidPlayerWeaponState)?;
    match state.cooldown_origin {
        None if state.next_ready_tick == 0 => {}
        Some(origin_item) => {
            let origin = sim
                .world
                .prototypes
                .item(origin_item)
                .and_then(|item| item.weapon)
                .ok_or(SimValidationError::InvalidPlayerWeaponState)?;
            if state.next_ready_tick.saturating_sub(sim.tick) > u64::from(origin.cooldown_ticks) {
                return Err(SimValidationError::InvalidPlayerWeaponState);
            }
        }
        None => return Err(SimValidationError::InvalidPlayerWeaponState),
    }
    match (state.loaded_ammo, state.loaded_shots) {
        (None, 0) if state.loaded_damage == Damage::physical(0) => Ok(()),
        (Some(ammo_item), shots) if shots > 0 => {
            let ammo = sim
                .world
                .prototypes
                .item(ammo_item)
                .and_then(|item| item.ammo)
                .ok_or(SimValidationError::InvalidPlayerWeaponState)?;
            if ammo.category != weapon.ammo_category
                || shots > ammo.shots_per_item
                || state.loaded_damage != Damage::new(ammo.damage_per_shot, ammo.damage_type)
            {
                return Err(SimValidationError::InvalidPlayerWeaponState);
            }
            Ok(())
        }
        _ => Err(SimValidationError::InvalidPlayerWeaponState),
    }
}

pub(super) fn validate_delayed_combat_state(sim: &Simulation) -> Result<(), SimValidationError> {
    for (&id, projectile) in &sim.delayed_combat.projectiles {
        if projectile.id != id
            || id.raw() >= sim.delayed_combat.next_projectile_id
            || projectile.damage.amount == 0
            || projectile.speed_fixed_per_tick == 0
            || projectile.explosion_radius_tiles == 0
            || projectile.impact_tick <= projectile.launched_tick
            || projectile.impact_tick <= sim.tick
        {
            return Err(SimValidationError::InvalidDelayedCombatState);
        }
    }
    for (&target, effects) in &sim.delayed_combat.statuses {
        let Some(burning) = effects.burning else {
            return Err(SimValidationError::InvalidDelayedCombatState);
        };
        if sim.combatant_health(target).is_none()
            || burning.damage_per_tick.amount == 0
            || burning.damage_per_tick.damage_type != DamageType::Fire
            || burning.interval_ticks == 0
            || burning.next_damage_tick <= sim.tick
            || burning.next_damage_tick > burning.expires_tick
        {
            return Err(SimValidationError::InvalidDelayedCombatState);
        }
    }
    Ok(())
}

/// Nearest enemy unit whose tile lies within `range` of the turret
/// footprint; ties resolve to the lowest enemy id.
fn nearest_enemy_in_range(
    enemies: &EnemySubsystem,
    enemy_chunks: &EnemyChunkIndex,
    footprint: &EntityFootprint,
    range: i64,
) -> Option<EnemyId> {
    let mut best: Option<(i64, EnemyId)> = None;
    for enemy_id in enemy_chunks.ids_in_expanded_footprint(footprint, range) {
        let Some(enemy) = enemies.enemies.get(enemy_id) else {
            continue;
        };
        let enemy_footprint = EntityFootprint::single_tile(enemy.tile().0, enemy.tile().1);
        let distance = footprint.chebyshev_distance_to(&enemy_footprint);
        if distance > range {
            continue;
        }
        if best.is_none_or(|current| (distance, enemy.id) < current) {
            best = Some((distance, enemy.id));
        }
    }
    best.map(|(_, id)| id)
}

fn nearest_spawner_in_range(
    enemy_spawners: &BTreeMap<EntityId, EnemySpawnerState>,
    placed_entities: &BTreeMap<EntityId, PlacedEntity>,
    occupancy: &OccupancyGrid,
    footprint: &EntityFootprint,
    range: i64,
) -> Option<EntityId> {
    let mut best: Option<(i64, EntityId)> = None;
    let min_x = footprint.x.saturating_sub(range);
    let max_x = footprint
        .x
        .saturating_add(i64::from(footprint.width) - 1)
        .saturating_add(range);
    let min_y = footprint.y.saturating_sub(range);
    let max_y = footprint
        .y
        .saturating_add(i64::from(footprint.height) - 1)
        .saturating_add(range);
    for spawner_id in occupancy.entity_ids_in_tile_rect(min_x, max_x, min_y, max_y) {
        if !enemy_spawners.contains_key(&spawner_id) {
            continue;
        }
        let Some(placed) = placed_entities.get(&spawner_id) else {
            continue;
        };
        let distance = footprint.chebyshev_distance_to(&placed.footprint);
        if distance > range {
            continue;
        }
        if best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, spawner_id));
        }
    }
    best.map(|(_, id)| id)
}

/// Runtime-only spatial index reused by each turret pass. It deliberately
/// compares and hashes as empty derived state so capacity does not affect
/// deterministic simulation identity.
#[derive(Clone, Debug, Default)]
pub(super) struct EnemyChunkIndex {
    chunks: BTreeMap<ChunkCoord, Vec<EnemyId>>,
}

impl EnemyChunkIndex {
    fn rebuild(&mut self, enemies: &EnemySubsystem) {
        for enemy_ids in self.chunks.values_mut() {
            enemy_ids.clear();
        }
        for enemy in enemies.enemies.values() {
            if let Some(coord) = ChunkCoord::from_tile(enemy.tile().0, enemy.tile().1) {
                self.chunks.entry(coord).or_default().push(enemy.id);
            }
        }
        self.chunks.retain(|_, enemy_ids| !enemy_ids.is_empty());
    }

    fn ids_in_expanded_footprint(
        &self,
        footprint: &EntityFootprint,
        range: i64,
    ) -> impl Iterator<Item = &EnemyId> {
        let min_x = footprint.x.saturating_sub(range);
        let max_x = footprint
            .x
            .saturating_add(i64::from(footprint.width) - 1)
            .saturating_add(range);
        let min_y = footprint.y.saturating_sub(range);
        let max_y = footprint
            .y
            .saturating_add(i64::from(footprint.height) - 1)
            .saturating_add(range);
        ChunkCoord::from_tile(min_x, min_y)
            .zip(ChunkCoord::from_tile(max_x, max_y))
            .into_iter()
            .flat_map(move |(min_chunk, max_chunk)| {
                self.chunks
                    .range(
                        ChunkCoord {
                            x: min_chunk.x,
                            y: i32::MIN,
                        }..=ChunkCoord {
                            x: max_chunk.x,
                            y: i32::MAX,
                        },
                    )
                    .filter(move |(coord, _)| coord.y >= min_chunk.y && coord.y <= max_chunk.y)
                    .flat_map(|(_, enemy_ids)| enemy_ids)
            })
    }
}

impl_runtime_only_identity!(EnemyChunkIndex);
