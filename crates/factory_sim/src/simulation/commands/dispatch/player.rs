use super::*;

pub(super) fn apply(
    sim: &mut Simulation,
    command: &SimCommand,
) -> Result<SimCommandEffect, SimCommandError> {
    match *command {
        SimCommand::SetEnemyRuntimeSettings(settings) => {
            sim.set_enemy_runtime_settings(settings)
                .map_err(SimCommandError::EnemyRuntimeSettings)?;
        }
        SimCommand::MovePlayer {
            direction_x,
            direction_y,
            delta_seconds,
        } => sim.move_player(direction_x, direction_y, delta_seconds),
        SimCommand::SetManualMiningTarget(target) => {
            let gained_item = target.and_then(|target| {
                if let Some(entity_id) = sim.entities.occupancy.entity_at(target.x, target.y) {
                    let placed = sim.entities.placed_entity(entity_id)?;
                    entity_recovery_ops::build_item_for_entity(sim, placed.prototype_id).ok()
                } else {
                    sim.world
                        .tile_at(target.x, target.y)
                        .and_then(|tile| tile.resource.map(|resource| resource.resource_item))
                }
            });
            let count_before = gained_item.map(|item_id| sim.player_inventory.count(item_id));
            sim.update_manual_mining(target);
            return Ok(item_gain_effect(sim, gained_item, count_before));
        }
        SimCommand::CyclePlayerWeapon => {
            sim.cycle_player_weapon().map_err(SimCommandError::Weapon)?;
        }
        SimCommand::AttackWithPlayerWeapon { x, y } => {
            sim.attack_with_player_weapon(x, y)
                .map_err(SimCommandError::Weapon)?;
        }
        SimCommand::RepairEntity { entity_id } => {
            sim.repair_entity(entity_id)
                .map_err(SimCommandError::Repair)?;
        }
        SimCommand::EquipArmor { inventory_slot } => {
            sim.equip_armor(inventory_slot)
                .map_err(SimCommandError::Equipment)?;
        }
        SimCommand::UnequipArmor => {
            sim.unequip_armor().map_err(SimCommandError::Equipment)?;
        }
        SimCommand::InstallEquipment {
            inventory_slot,
            x,
            y,
        } => {
            sim.install_equipment(inventory_slot, x, y)
                .map_err(SimCommandError::Equipment)?;
        }
        SimCommand::RemoveEquipment { x, y } => {
            sim.remove_equipment(x, y)
                .map_err(SimCommandError::Equipment)?;
        }
        _ => unreachable!("non-player command routed to player dispatcher"),
    }
    Ok(SimCommandEffect::None)
}
