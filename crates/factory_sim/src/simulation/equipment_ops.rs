use super::*;
use factory_data::{ArmorPrototype, EquipmentEffectPrototype, EquipmentPrototype};

const PERSONAL_POWER_TICKS_PER_SECOND: u64 = 60;
const JOULES_PER_SHIELD_POINT: u64 = 1_000;

#[derive(Clone, Copy, Debug, Default)]
struct EquipmentTotals {
    generation_watts: u64,
    battery_capacity_joules: u64,
    shield_capacity_joules: u64,
    shield_recharge_watts: u64,
}

impl Simulation {
    pub fn equipment_state(&self) -> &PlayerEquipmentState {
        &self.player_equipment
    }

    pub fn equipped_armor(&self) -> Option<ItemId> {
        self.player_equipment.equipped_armor
    }

    pub fn installed_equipment(&self) -> &[InstalledEquipment] {
        &self.player_equipment.installed
    }

    pub fn personal_stored_energy(&self) -> (u64, u64) {
        let totals = equipment_totals(&self.world.prototypes, &self.player_equipment);
        (
            self.player_equipment.battery_energy_joules,
            totals.battery_capacity_joules,
        )
    }

    pub fn personal_shield_points(&self) -> (u32, u32) {
        let totals = equipment_totals(&self.world.prototypes, &self.player_equipment);
        (
            u32::try_from(self.player_equipment.shield_energy_joules / JOULES_PER_SHIELD_POINT)
                .unwrap_or(u32::MAX),
            u32::try_from(totals.shield_capacity_joules / JOULES_PER_SHIELD_POINT)
                .unwrap_or(u32::MAX),
        )
    }

    pub fn equip_armor(&mut self, inventory_slot: usize) -> Result<(), PlayerEquipmentError> {
        let stack = self
            .player_inventory
            .slot(inventory_slot)
            .ok_or_else(|| inventory_slot_error(&self.player_inventory, inventory_slot))?;
        let item_id = stack.item_id();
        if self
            .world
            .prototypes
            .item(item_id)
            .and_then(|item| item.armor.as_ref())
            .is_none()
        {
            return Err(PlayerEquipmentError::NotArmor(item_id));
        }
        if !self.player_equipment.installed.is_empty() {
            return Err(PlayerEquipmentError::ArmorGridNotEmpty);
        }

        let mut inventory = self.player_inventory.clone();
        inventory
            .item_slot_mut(inventory_slot)
            .expect("validated inventory slot")
            .remove(item_id, 1)
            .expect("selected stack contains one armor");
        if let Some(previous) = self.player_equipment.equipped_armor
            && inventory
                .insert(&self.world.prototypes, previous, 1)
                .is_err()
        {
            return Err(PlayerEquipmentError::InventoryFull);
        }

        self.player_inventory = inventory;
        self.player_equipment.equipped_armor = Some(item_id);
        self.apply_equipped_armor_resistances();
        Ok(())
    }

    pub fn unequip_armor(&mut self) -> Result<(), PlayerEquipmentError> {
        let armor = self
            .player_equipment
            .equipped_armor
            .ok_or(PlayerEquipmentError::NoArmorEquipped)?;
        if !self.player_equipment.installed.is_empty() {
            return Err(PlayerEquipmentError::ArmorGridNotEmpty);
        }
        let mut inventory = self.player_inventory.clone();
        inventory
            .insert(&self.world.prototypes, armor, 1)
            .map_err(|_| PlayerEquipmentError::InventoryFull)?;
        self.player_inventory = inventory;
        self.player_equipment.equipped_armor = None;
        self.clamp_personal_equipment_capacity();
        self.apply_equipped_armor_resistances();
        Ok(())
    }

    pub fn install_equipment(
        &mut self,
        inventory_slot: usize,
        x: u8,
        y: u8,
    ) -> Result<(), PlayerEquipmentError> {
        let armor = self.current_armor_prototype()?;
        let stack = self
            .player_inventory
            .slot(inventory_slot)
            .ok_or_else(|| inventory_slot_error(&self.player_inventory, inventory_slot))?;
        let item_id = stack.item_id();
        let equipment = self
            .world
            .prototypes
            .item(item_id)
            .and_then(|item| item.equipment)
            .ok_or(PlayerEquipmentError::NotEquipment(item_id))?;
        if !rectangle_fits(armor, equipment, x, y) {
            return Err(PlayerEquipmentError::PlacementOutOfBounds);
        }
        if self.player_equipment.installed.iter().any(|installed| {
            let other = equipment_prototype(&self.world.prototypes, installed.item_id);
            rectangles_overlap((x, y, equipment), (installed.x, installed.y, other))
        }) {
            return Err(PlayerEquipmentError::PlacementOverlaps);
        }

        let mut inventory = self.player_inventory.clone();
        inventory
            .item_slot_mut(inventory_slot)
            .expect("validated inventory slot")
            .remove(item_id, 1)
            .expect("selected stack contains one equipment item");
        let mut installed = self.player_equipment.installed.clone();
        installed.push(InstalledEquipment { item_id, x, y });
        sort_installed(&mut installed);

        self.player_inventory = inventory;
        self.player_equipment.installed = installed;
        Ok(())
    }

    pub fn remove_equipment(&mut self, x: u8, y: u8) -> Result<(), PlayerEquipmentError> {
        let index = self
            .player_equipment
            .installed
            .iter()
            .position(|installed| {
                let equipment = equipment_prototype(&self.world.prototypes, installed.item_id);
                x >= installed.x
                    && x < installed.x.saturating_add(equipment.width)
                    && y >= installed.y
                    && y < installed.y.saturating_add(equipment.height)
            })
            .ok_or(PlayerEquipmentError::NoEquipmentAtCell { x, y })?;
        let item_id = self.player_equipment.installed[index].item_id;
        let mut inventory = self.player_inventory.clone();
        inventory
            .insert(&self.world.prototypes, item_id, 1)
            .map_err(|_| PlayerEquipmentError::InventoryFull)?;

        self.player_inventory = inventory;
        self.player_equipment.installed.remove(index);
        self.clamp_personal_equipment_capacity();
        Ok(())
    }

    pub(super) fn advance_player_equipment(&mut self) {
        let totals = equipment_totals(&self.world.prototypes, &self.player_equipment);
        let generated_watt_ticks = self
            .player_equipment
            .generation_remainder_watt_ticks
            .saturating_add(totals.generation_watts);
        let generated_joules = generated_watt_ticks / PERSONAL_POWER_TICKS_PER_SECOND;
        self.player_equipment.generation_remainder_watt_ticks =
            generated_watt_ticks % PERSONAL_POWER_TICKS_PER_SECOND;

        let recharge_watt_ticks = self
            .player_equipment
            .recharge_remainder_watt_ticks
            .saturating_add(totals.shield_recharge_watts);
        let recharge_limit_joules = recharge_watt_ticks / PERSONAL_POWER_TICKS_PER_SECOND;
        self.player_equipment.recharge_remainder_watt_ticks =
            recharge_watt_ticks % PERSONAL_POWER_TICKS_PER_SECOND;

        let available = self
            .player_equipment
            .battery_energy_joules
            .saturating_add(generated_joules);
        let missing_shield = totals
            .shield_capacity_joules
            .saturating_sub(self.player_equipment.shield_energy_joules);
        let recharged = available.min(recharge_limit_joules).min(missing_shield);
        self.player_equipment.shield_energy_joules = self
            .player_equipment
            .shield_energy_joules
            .saturating_add(recharged);
        self.player_equipment.battery_energy_joules = available
            .saturating_sub(recharged)
            .min(totals.battery_capacity_joules);
    }

    pub(super) fn absorb_player_damage_with_shields(&mut self, amount: u32) -> u32 {
        let absorbable = self.player_equipment.shield_energy_joules / JOULES_PER_SHIELD_POINT;
        let absorbed = u64::from(amount).min(absorbable);
        self.player_equipment.shield_energy_joules = self
            .player_equipment
            .shield_energy_joules
            .saturating_sub(absorbed * JOULES_PER_SHIELD_POINT);
        amount - u32::try_from(absorbed).expect("absorbed damage is bounded by u32 input")
    }

    pub(super) fn apply_equipped_armor_resistances(&mut self) {
        self.player.health.resistances =
            equipped_armor_resistance_profile(&self.world.prototypes, &self.player_equipment);
    }

    fn current_armor_prototype(&self) -> Result<&ArmorPrototype, PlayerEquipmentError> {
        self.player_equipment
            .equipped_armor
            .and_then(|item_id| self.world.prototypes.item(item_id))
            .and_then(|item| item.armor.as_ref())
            .ok_or(PlayerEquipmentError::NoArmorEquipped)
    }

    fn clamp_personal_equipment_capacity(&mut self) {
        let totals = equipment_totals(&self.world.prototypes, &self.player_equipment);
        self.player_equipment.battery_energy_joules = self
            .player_equipment
            .battery_energy_joules
            .min(totals.battery_capacity_joules);
        self.player_equipment.shield_energy_joules = self
            .player_equipment
            .shield_energy_joules
            .min(totals.shield_capacity_joules);
    }
}

pub(super) fn validate_player_equipment(sim: &Simulation) -> Result<(), SimValidationError> {
    let state = &sim.player_equipment;
    let armor = match state.equipped_armor {
        Some(item_id) => sim
            .world
            .prototypes
            .item(item_id)
            .and_then(|item| item.armor.as_ref())
            .ok_or(SimValidationError::InvalidPlayerEquipment)?,
        None if state.installed.is_empty() => {
            if state.battery_energy_joules != 0 || state.shield_energy_joules != 0 {
                return Err(SimValidationError::InvalidPlayerEquipment);
            }
            return validate_equipment_remainders_and_resistance(sim);
        }
        None => return Err(SimValidationError::InvalidPlayerEquipment),
    };

    let mut previous = None;
    for installed in &state.installed {
        let key = (installed.y, installed.x, installed.item_id);
        if previous.is_some_and(|previous| previous >= key) {
            return Err(SimValidationError::InvalidPlayerEquipment);
        }
        previous = Some(key);
        let equipment = sim
            .world
            .prototypes
            .item(installed.item_id)
            .and_then(|item| item.equipment)
            .ok_or(SimValidationError::InvalidPlayerEquipment)?;
        if !rectangle_fits(armor, equipment, installed.x, installed.y) {
            return Err(SimValidationError::InvalidPlayerEquipment);
        }
    }
    for (index, first) in state.installed.iter().enumerate() {
        let first_prototype = equipment_prototype(&sim.world.prototypes, first.item_id);
        for second in &state.installed[index + 1..] {
            let second_prototype = equipment_prototype(&sim.world.prototypes, second.item_id);
            if rectangles_overlap(
                (first.x, first.y, first_prototype),
                (second.x, second.y, second_prototype),
            ) {
                return Err(SimValidationError::InvalidPlayerEquipment);
            }
        }
    }
    let totals = equipment_totals(&sim.world.prototypes, state);
    if state.battery_energy_joules > totals.battery_capacity_joules
        || state.shield_energy_joules > totals.shield_capacity_joules
    {
        return Err(SimValidationError::InvalidPlayerEquipment);
    }
    validate_equipment_remainders_and_resistance(sim)
}

fn validate_equipment_remainders_and_resistance(
    sim: &Simulation,
) -> Result<(), SimValidationError> {
    if sim.player_equipment.generation_remainder_watt_ticks >= PERSONAL_POWER_TICKS_PER_SECOND
        || sim.player_equipment.recharge_remainder_watt_ticks >= PERSONAL_POWER_TICKS_PER_SECOND
    {
        return Err(SimValidationError::InvalidPlayerEquipment);
    }
    let expected = equipped_armor_resistance_profile(&sim.world.prototypes, &sim.player_equipment);
    if sim.player.health.resistances != expected {
        return Err(SimValidationError::InvalidPlayerEquipment);
    }
    Ok(())
}

/// Builds the resistance profile granted by the player's equipped armor,
/// returning [`ResistanceProfile::NONE`] when no armor (or no resistances) apply.
fn equipped_armor_resistance_profile(
    catalog: &PrototypeCatalog,
    state: &PlayerEquipmentState,
) -> ResistanceProfile {
    let mut profile = ResistanceProfile::NONE;
    if let Some(armor) = state
        .equipped_armor
        .and_then(|item_id| catalog.item(item_id))
        .and_then(|item| item.armor.as_ref())
    {
        for resistance in &armor.resistances {
            profile = profile.with_resistance(
                resistance.damage_type,
                Resistance::new(
                    resistance.flat_reduction,
                    resistance.percent_reduction_permyriad,
                ),
            );
        }
    }
    profile
}

fn inventory_slot_error(inventory: &Inventory, slot_index: usize) -> PlayerEquipmentError {
    if slot_index < inventory.slots().len() {
        PlayerEquipmentError::EmptyInventorySlot { slot_index }
    } else {
        PlayerEquipmentError::InvalidInventorySlot { slot_index }
    }
}

fn equipment_totals(catalog: &PrototypeCatalog, state: &PlayerEquipmentState) -> EquipmentTotals {
    let mut totals = EquipmentTotals::default();
    for installed in &state.installed {
        match equipment_prototype(catalog, installed.item_id).effect {
            EquipmentEffectPrototype::PowerGeneration { power_watts } => {
                totals.generation_watts = totals.generation_watts.saturating_add(power_watts);
            }
            EquipmentEffectPrototype::Battery { capacity_joules } => {
                totals.battery_capacity_joules = totals
                    .battery_capacity_joules
                    .saturating_add(capacity_joules);
            }
            EquipmentEffectPrototype::EnergyShield {
                capacity_points,
                max_recharge_watts,
            } => {
                totals.shield_capacity_joules = totals
                    .shield_capacity_joules
                    .saturating_add(u64::from(capacity_points) * JOULES_PER_SHIELD_POINT);
                totals.shield_recharge_watts = totals
                    .shield_recharge_watts
                    .saturating_add(max_recharge_watts);
            }
        }
    }
    totals
}

fn equipment_prototype(catalog: &PrototypeCatalog, item_id: ItemId) -> EquipmentPrototype {
    catalog
        .item(item_id)
        .and_then(|item| item.equipment)
        .expect("installed equipment is validated against the catalog")
}

fn rectangle_fits(armor: &ArmorPrototype, equipment: EquipmentPrototype, x: u8, y: u8) -> bool {
    x.checked_add(equipment.width)
        .is_some_and(|right| right <= armor.grid_width)
        && y.checked_add(equipment.height)
            .is_some_and(|bottom| bottom <= armor.grid_height)
}

fn rectangles_overlap(
    first: (u8, u8, EquipmentPrototype),
    second: (u8, u8, EquipmentPrototype),
) -> bool {
    let (first_x, first_y, first_equipment) = first;
    let (second_x, second_y, second_equipment) = second;
    first_x < second_x.saturating_add(second_equipment.width)
        && second_x < first_x.saturating_add(first_equipment.width)
        && first_y < second_y.saturating_add(second_equipment.height)
        && second_y < first_y.saturating_add(first_equipment.height)
}

fn sort_installed(installed: &mut [InstalledEquipment]) {
    installed.sort_by_key(|equipment| (equipment.y, equipment.x, equipment.item_id));
}
