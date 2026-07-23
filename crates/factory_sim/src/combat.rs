use crate::inventory::Inventory;
use crate::{EnemyId, EntityId};
pub use factory_data::DamageType;
use serde::{Deserialize, Serialize};

pub const GUN_TURRET_AMMO_SLOT_COUNT: usize = 1;
pub const PLAYER_MAX_HEALTH: u32 = 100;

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
