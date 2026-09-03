use crate::inventory::Inventory;
use crate::{EnemyId, EntityId};
use factory_data::ItemId;
pub use factory_data::{AmmoCategory, DamageType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const GUN_TURRET_AMMO_SLOT_COUNT: usize = 1;
pub const PLAYER_MAX_HEALTH: u32 = 100;

/// Durable state for the player's selected weapon and opened magazine.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PlayerWeaponState {
    pub selected_weapon: Option<ItemId>,
    pub loaded_ammo: Option<ItemId>,
    pub loaded_shots: u32,
    pub loaded_damage: Damage,
    pub next_ready_tick: u64,
    /// Weapon whose cadence established `next_ready_tick`. It remains the
    /// origin when another compatible weapon is selected during that cooldown.
    pub cooldown_origin: Option<ItemId>,
}

impl Default for PlayerWeaponState {
    /// Creates the canonical state for a player with no selected weapon.
    fn default() -> Self {
        Self {
            selected_weapon: None,
            loaded_ammo: None,
            loaded_shots: 0,
            loaded_damage: Damage::physical(0),
            next_ready_tick: 0,
            cooldown_origin: None,
        }
    }
}

/// Read-only summary used by the HUD without exposing mutable combat state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerWeaponStatus {
    pub selected_weapon: Option<ItemId>,
    pub loaded_ammo: Option<ItemId>,
    pub loaded_shots: u32,
    pub reserve_shots: u64,
    pub cooldown_remaining_ticks: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerWeaponError {
    NoWeaponsAvailable,
    NoWeaponSelected,
    WeaponUnavailable(ItemId),
    NoAmmunition,
    CoolingDown { remaining_ticks: u32 },
    NoHostileTarget,
    OutOfRange { range_tiles: u32 },
}

/// Ownership group used by combat targeting and damage authorization.
///
/// Relations are deliberately defined on the faction rather than at each
/// callsite so new weapons and combatants cannot disagree about friendly
/// fire rules.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Faction {
    Player,
    Enemy,
    Neutral,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum FactionRelation {
    Allied,
    Neutral,
    Hostile,
}

impl Faction {
    pub const fn relation_to(self, other: Self) -> FactionRelation {
        match (self, other) {
            (Self::Player, Self::Enemy) | (Self::Enemy, Self::Player) => FactionRelation::Hostile,
            (Self::Neutral, _) | (_, Self::Neutral) => FactionRelation::Neutral,
            _ => FactionRelation::Allied,
        }
    }

    pub const fn is_hostile_to(self, other: Self) -> bool {
        matches!(self.relation_to(other), FactionRelation::Hostile)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Damage {
    pub amount: u32,
    pub damage_type: DamageType,
}

impl Damage {
    pub const fn new(amount: u32, damage_type: DamageType) -> Self {
        Self {
            amount,
            damage_type,
        }
    }

    pub const fn physical(amount: u32) -> Self {
        Self::new(amount, DamageType::Physical)
    }

    pub fn after_resistance(self, profile: &ResistanceProfile) -> u32 {
        profile.apply(self)
    }
}

/// Flat and proportional mitigation for one damage type.
///
/// Flat reduction is applied first, followed by `percent_reduction_permyriad`
/// (10,000 = 100%). Integer division rounds damage down deterministically.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Resistance {
    pub flat_reduction: u32,
    pub percent_reduction_permyriad: u16,
}

impl Resistance {
    pub const fn new(flat_reduction: u32, percent_reduction_permyriad: u16) -> Self {
        Self {
            flat_reduction,
            percent_reduction_permyriad,
        }
    }

    fn apply(self, amount: u32) -> u32 {
        let after_flat = amount.saturating_sub(self.flat_reduction);
        let reduction = u32::from(self.percent_reduction_permyriad.min(10_000));
        let remaining = 10_000 - reduction;
        let scaled = u64::from(after_flat) * u64::from(remaining) / 10_000;
        u32::try_from(scaled).expect("mitigated u32 damage must fit in u32")
    }
}

/// Compact, allocation-free resistance table for every supported damage type.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ResistanceProfile {
    values: [Resistance; DamageType::COUNT],
}

impl ResistanceProfile {
    pub const NONE: Self = Self {
        values: [Resistance::new(0, 0); DamageType::COUNT],
    };

    pub const fn resistance(self, damage_type: DamageType) -> Resistance {
        self.values[damage_type.index()]
    }

    pub const fn with_resistance(
        mut self,
        damage_type: DamageType,
        resistance: Resistance,
    ) -> Self {
        self.values[damage_type.index()] = resistance;
        self
    }

    pub fn apply(&self, damage: Damage) -> u32 {
        self.values[damage.damage_type.index()].apply(damage.amount)
    }

    pub fn is_valid(&self) -> bool {
        self.values
            .iter()
            .all(|resistance| resistance.percent_reduction_permyriad <= 10_000)
    }
}

/// How an attack reaches its target. Projectile and area variants describe
/// stable simulation data without coupling combat rules to presentation.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum AttackDelivery {
    Melee {
        reach_tiles: u32,
    },
    Hitscan {
        range_tiles: u32,
    },
    Projectile {
        range_tiles: u32,
        speed_fixed_per_tick: u32,
    },
    Area {
        range_tiles: u32,
        radius_tiles: u32,
    },
}

impl AttackDelivery {
    pub const fn range_tiles(self) -> u32 {
        match self {
            Self::Melee { reach_tiles } => reach_tiles,
            Self::Hitscan { range_tiles }
            | Self::Projectile { range_tiles, .. }
            | Self::Area { range_tiles, .. } => range_tiles,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum TargetPriority {
    Nearest,
    UnitsFirst,
    StructuresFirst,
}

/// Immutable rules shared by anything capable of attacking.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct AttackDefinition {
    pub damage: Damage,
    pub cooldown_ticks: u32,
    pub delivery: AttackDelivery,
    pub target_priority: TargetPriority,
}

impl AttackDefinition {
    pub const fn melee(damage: Damage, cooldown_ticks: u32, reach_tiles: u32) -> Self {
        Self {
            damage,
            cooldown_ticks,
            delivery: AttackDelivery::Melee { reach_tiles },
            target_priority: TargetPriority::Nearest,
        }
    }

    pub const fn hitscan(damage: Damage, cooldown_ticks: u32, range_tiles: u32) -> Self {
        Self {
            damage,
            cooldown_ticks,
            delivery: AttackDelivery::Hitscan { range_tiles },
            target_priority: TargetPriority::UnitsFirst,
        }
    }

    pub const fn projectile(
        damage: Damage,
        cooldown_ticks: u32,
        range_tiles: u32,
        speed_fixed_per_tick: u32,
    ) -> Self {
        Self {
            damage,
            cooldown_ticks,
            delivery: AttackDelivery::Projectile {
                range_tiles,
                speed_fixed_per_tick,
            },
            target_priority: TargetPriority::Nearest,
        }
    }

    pub const fn area(
        damage: Damage,
        cooldown_ticks: u32,
        range_tiles: u32,
        radius_tiles: u32,
    ) -> Self {
        Self {
            damage,
            cooldown_ticks,
            delivery: AttackDelivery::Area {
                range_tiles,
                radius_tiles,
            },
            target_priority: TargetPriority::Nearest,
        }
    }

    pub const fn with_target_priority(mut self, target_priority: TargetPriority) -> Self {
        self.target_priority = target_priority;
        self
    }

    pub const fn is_valid(self) -> bool {
        self.damage.amount > 0
            && self.cooldown_ticks > 0
            && self.delivery.range_tiles() > 0
            && !matches!(
                self.delivery,
                AttackDelivery::Projectile {
                    speed_fixed_per_tick: 0,
                    ..
                } | AttackDelivery::Area {
                    radius_tiles: 0,
                    ..
                }
            )
    }
}

/// Stable identity used by attacks, projectiles, and future status effects.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum CombatantId {
    Player,
    Entity(EntityId),
    Enemy(EnemyId),
}

/// Source ownership is captured when an attack is committed. Delayed
/// projectiles therefore remain deterministic even if their owner dies before
/// impact.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CombatSource {
    pub owner: CombatantId,
    pub faction: Faction,
}

impl CombatSource {
    pub const fn new(owner: CombatantId, faction: Faction) -> Self {
        Self { owner, faction }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CombatCommand {
    pub source: CombatSource,
    pub target: CombatantId,
    pub damage: Damage,
}

/// Attacks committed against one simulation snapshot and resolved together.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CombatCommandBuffer {
    commands: Vec<CombatCommand>,
}

impl CombatCommandBuffer {
    pub fn push(&mut self, command: CombatCommand) {
        if command.damage.amount > 0 {
            self.commands.push(command);
        }
    }

    pub fn attack(&mut self, source: CombatSource, target: CombatantId, attack: AttackDefinition) {
        self.push(CombatCommand {
            source,
            target,
            damage: attack.damage,
        });
    }

    pub fn iter(&self) -> impl Iterator<Item = &CombatCommand> {
        self.commands.iter()
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

/// Stable identity for a delayed projectile. IDs are allocated monotonically
/// and projectiles are stored in ID order so simultaneous impacts replay in a
/// stable order after save/load.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ProjectileId(u64);

impl ProjectileId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// A projectile committed to a fixed impact tile. Travel is linear in integer
/// fixed-point space; targets are sampled only when the projectile arrives.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ProjectileState {
    pub id: ProjectileId,
    pub source: CombatSource,
    pub start_x_fixed: i64,
    pub start_y_fixed: i64,
    pub impact_x: i64,
    pub impact_y: i64,
    pub launched_tick: u64,
    pub impact_tick: u64,
    pub speed_fixed_per_tick: u32,
    pub damage: Damage,
    pub explosion_radius_tiles: u32,
}

impl ProjectileState {
    /// Integer-interpolated position suitable for deterministic presentation.
    pub fn position_fixed_at(self, tick: u64) -> (i64, i64) {
        let duration = self.impact_tick.saturating_sub(self.launched_tick).max(1);
        let elapsed = tick.saturating_sub(self.launched_tick).min(duration);
        let target_x = tile_center_fixed(self.impact_x);
        let target_y = tile_center_fixed(self.impact_y);
        (
            interpolate_fixed(self.start_x_fixed, target_x, elapsed, duration),
            interpolate_fixed(self.start_y_fixed, target_y, elapsed, duration),
        )
    }
}

/// Typed fire damage which survives its source and ticks at exact intervals.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BurningStatus {
    pub source: CombatSource,
    pub damage_per_tick: Damage,
    pub interval_ticks: u32,
    pub next_damage_tick: u64,
    pub expires_tick: u64,
}

/// Status effects attached to one combatant. New effect kinds belong here so
/// delayed combat rules share ownership, cleanup, validation, and save logic.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CombatStatusEffects {
    pub burning: Option<BurningStatus>,
}

/// Durable state for delayed combat mechanics.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct DelayedCombatState {
    pub(crate) next_projectile_id: u64,
    pub(crate) projectiles: BTreeMap<ProjectileId, ProjectileState>,
    pub(crate) statuses: BTreeMap<CombatantId, CombatStatusEffects>,
}

impl DelayedCombatState {
    pub fn projectiles(&self) -> impl Iterator<Item = &ProjectileState> {
        self.projectiles.values()
    }

    pub fn status(&self, combatant: CombatantId) -> Option<CombatStatusEffects> {
        self.statuses.get(&combatant).copied()
    }
}

const COMBAT_POSITION_SCALE: i64 = 1024;

fn tile_center_fixed(tile: i64) -> i64 {
    tile.saturating_mul(COMBAT_POSITION_SCALE)
        .saturating_add(COMBAT_POSITION_SCALE / 2)
}

fn interpolate_fixed(start: i64, end: i64, elapsed: u64, duration: u64) -> i64 {
    let delta = i128::from(end) - i128::from(start);
    let offset = delta.saturating_mul(i128::from(elapsed)) / i128::from(duration);
    i64::try_from(i128::from(start).saturating_add(offset)).unwrap_or_else(|_| {
        if offset.is_negative() {
            i64::MIN
        } else {
            i64::MAX
        }
    })
}

/// Runtime state of a placed gun turret. Magazines are loaded one at a time
/// from the ammo inventory; `loaded_shots` tracks the opened magazine, whose
/// per-shot damage is captured at load time so the magazine item itself no
/// longer needs to exist.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct GunTurretState {
    pub ammo: Inventory,
    pub loaded_shots: u32,
    pub loaded_damage: Damage,
    pub next_ready_tick: u64,
}

impl GunTurretState {
    pub fn new() -> Self {
        Self {
            ammo: Inventory::with_slot_count(GUN_TURRET_AMMO_SLOT_COUNT),
            loaded_shots: 0,
            loaded_damage: Damage::physical(0),
            next_ready_tick: 0,
        }
    }
}

impl Default for GunTurretState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct LaserTurretState {
    pub engaged: bool,
    pub cooldown_remaining_ticks: u32,
}

/// Runtime state of an enemy spawner: the schedule for free guard spawns.
/// Pollution absorbed by a spawner is pooled on the owning base's
/// `attack_budget_micro`, not tracked per spawner.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EnemySpawnerState {
    pub next_free_spawn_tick: u64,
}

/// Current health of a placed entity whose prototype declares `max_health`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct HealthState {
    pub current: u32,
    pub maximum: u32,
    pub faction: Faction,
    pub resistances: ResistanceProfile,
}

impl HealthState {
    pub const fn new(maximum: u32, faction: Faction) -> Self {
        Self {
            current: maximum,
            maximum,
            faction,
            resistances: ResistanceProfile::NONE,
        }
    }

    pub fn apply_damage(&mut self, damage: Damage) -> u32 {
        let applied = damage.after_resistance(&self.resistances).min(self.current);
        self.current -= applied;
        applied
    }

    pub fn is_valid(self) -> bool {
        self.maximum > 0 && self.current <= self.maximum && self.resistances.is_valid()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairError {
    MissingEntity(crate::ids::EntityId),
    /// The entity has no health and cannot be repaired.
    NotRepairable(crate::ids::EntityId),
    OutOfReach,
    NoRepairPacks,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resistance_applies_flat_then_proportional_reduction() {
        let profile =
            ResistanceProfile::NONE.with_resistance(DamageType::Fire, Resistance::new(10, 2_500));

        assert_eq!(
            Damage::new(50, DamageType::Fire).after_resistance(&profile),
            30
        );
        assert_eq!(Damage::physical(50).after_resistance(&profile), 50);
    }

    #[test]
    fn faction_relations_are_symmetric_and_neutral_is_not_attackable() {
        assert!(Faction::Player.is_hostile_to(Faction::Enemy));
        assert!(Faction::Enemy.is_hostile_to(Faction::Player));
        assert!(!Faction::Player.is_hostile_to(Faction::Player));
        assert!(!Faction::Enemy.is_hostile_to(Faction::Neutral));
    }
}
