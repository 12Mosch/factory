use glam::IVec2;
use serde::{Deserialize, Serialize};

use crate::ids::{
    EntityPrototypeId, FluidId, ItemId, RecipeId, TechnologyId, TileId, VirtualSignalId,
};

/// Deterministic timing for a world's day/night cycle.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct DayNightCycleConfig {
    pub cycle_length_ticks: u64,
    pub dawn_dusk_ticks: u64,
}

impl DayNightCycleConfig {
    /// Returns whether the timing leaves non-empty full-day, dusk, night, and
    /// dawn portions without overflowing while checking the ramp lengths.
    pub const fn is_valid(self) -> bool {
        if self.cycle_length_ticks == 0 || self.dawn_dusk_ticks == 0 {
            return false;
        }
        match self.dawn_dusk_ticks.checked_mul(4) {
            Some(total_ramp_ticks) => total_ramp_ticks < self.cycle_length_ticks,
            None => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum DamageType {
    Physical,
    Fire,
    Explosion,
    Acid,
    Laser,
}

impl DamageType {
    pub const COUNT: usize = 5;

    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidPrototype {
    pub id: FluidId,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ItemPrototype {
    pub id: ItemId,
    pub name: String,
    pub stack_size: u16,
    pub fuel_value_joules: Option<u64>,
    /// Item left behind when one unit of this fuel is burnt. Only nuclear
    /// reactors honour it: they refuse to start a fuel cell unless the residue
    /// fits, which is what makes spent fuel reprocessing a closed loop. Ordinary
    /// burners (furnaces, boilers, burner inserters) burn residue-free fuels.
    pub burnt_result: Option<ItemId>,
    /// Present when the item can be loaded into gun turrets as ammunition.
    pub ammo: Option<AmmoPrototype>,
    /// Present when the item can be consumed to repair damaged entities.
    pub repair: Option<RepairToolPrototype>,
    /// Present when the item can be equipped as the player's armor.
    pub armor: Option<ArmorPrototype>,
    /// Present when the item can be installed in an equipped armor grid.
    pub equipment: Option<EquipmentPrototype>,
    /// Present when the item can be installed in a machine or beacon module slot.
    pub module_effect: Option<ModuleEffectPrototype>,
    /// Present when the item is a robot a roboport can station and dispatch.
    pub robot: Option<RobotPrototype>,
    /// Present when placing the item rewrites the targeted terrain tile
    /// instead of building an entity (landfill, stone path, concrete).
    pub place_as_tile: Option<TilePlacementPrototype>,
}

/// How an item mutates terrain when placed.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TilePlacementPrototype {
    pub tile: TileId,
    /// Fill items replace non-walkable terrain (water) and may only be placed
    /// there; paving items are the inverse and require solid ground. Keeping
    /// the two disjoint stops landfill from being wasted on dry land and stops
    /// paving from bridging water.
    pub fills_water: bool,
    /// Build-menu placement, mirroring the same fields on buildable entities
    /// so terrain items and buildings share one menu and hotbar.
    pub building_category: BuildingCategory,
    pub building_menu_order: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ModuleEffectPrototype {
    pub speed_delta_permyriad: i32,
    pub productivity_permyriad: u32,
    pub energy_delta_permyriad: i32,
    pub pollution_delta_permyriad: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct AmmoPrototype {
    pub damage_per_shot: u32,
    pub shots_per_item: u32,
    pub damage_type: DamageType,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RepairToolPrototype {
    /// Total health one item restores before it is used up.
    pub restore_health: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ArmorPrototype {
    pub grid_width: u8,
    pub grid_height: u8,
    pub resistances: Vec<DamageResistancePrototype>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct DamageResistancePrototype {
    pub damage_type: DamageType,
    pub flat_reduction: u32,
    pub percent_reduction_permyriad: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EquipmentPrototype {
    pub width: u8,
    pub height: u8,
    pub effect: EquipmentEffectPrototype,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum EquipmentEffectPrototype {
    PowerGeneration {
        power_watts: u64,
    },
    Battery {
        capacity_joules: u64,
    },
    EnergyShield {
        capacity_points: u32,
        max_recharge_watts: u64,
    },
}

/// A signal that carries no item or fluid identity of its own. Concrete
/// virtual signals are plain named channels a player can push numbers down;
/// the wildcard kinds are only meaningful as combinator operands.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct VirtualSignalPrototype {
    pub id: VirtualSignalId,
    pub name: String,
    pub kind: VirtualSignalKind,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum VirtualSignalKind {
    /// An ordinary named channel that can hold a value on a network.
    #[default]
    Concrete,
    /// Combinator wildcard: runs the operation once per input signal.
    Each,
    /// Combinator wildcard: matches when any one input signal satisfies the
    /// comparison.
    Anything,
    /// Combinator wildcard: matches when every input signal satisfies the
    /// comparison.
    Everything,
}

impl VirtualSignalKind {
    pub const fn is_wildcard(self) -> bool {
        !matches!(self, Self::Concrete)
    }
}

/// Circuit-network wire attachment. An entity without this section cannot be
/// wired at all, which is what keeps belts and inserters free of circuit state
/// until the player opts in.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CircuitConnectorPrototype {
    pub ports: CircuitPortLayout,
    /// Maximum wire length, in half tiles, measured between footprint centers.
    /// Half tiles let even-sized footprints keep an exact center like
    /// [`ElectricPolePrototype::wire_reach_tiles_x2`] does for copper wire.
    pub wire_reach_tiles_x2: u16,
    /// Publishes the entity's own contents onto the networks it is wired to.
    pub reads_contents: bool,
    /// Accepts an enable/disable condition gating the entity's own work.
    pub controllable: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum CircuitPortLayout {
    /// One connector shared by both wire colors; the entity both reads and
    /// writes there.
    Single,
    /// Separate input and output connectors, so a combinator's own output
    /// never feeds straight back into its input.
    InputOutput,
}

/// Signal-processing entity. The kind selects which stored configuration and
/// evaluation rule the simulation applies.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CombinatorPrototype {
    pub kind: CombinatorKind,
    /// Signal rows a constant combinator holds; zero for the other kinds.
    pub constant_slot_count: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum CombinatorKind {
    Constant,
    Arithmetic,
    Decider,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RecipePrototype {
    pub id: RecipeId,
    pub name: String,
    pub category: CraftingCategory,
    pub crafting_time_ticks: u32,
    pub ingredients: Vec<ItemAmount>,
    pub products: Vec<ItemAmount>,
    pub fluid_ingredients: Vec<FluidAmount>,
    pub fluid_products: Vec<FluidAmount>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EntityPrototype {
    pub id: EntityPrototypeId,
    pub name: String,
    pub entity_kind: EntityKind,
    pub size: IVec2,
    pub collision_mask: CollisionMask,
    pub build_item: Option<ItemId>,
    pub building_category: Option<BuildingCategory>,
    pub building_menu_order: Option<u16>,
    pub inventory_slot_count: Option<usize>,
    pub module_slot_count: usize,
    pub beacon: Option<BeaconPrototype>,
    pub burner: Option<BurnerPrototype>,
    pub mining_drill: Option<MiningDrillPrototype>,
    pub furnace: Option<FurnacePrototype>,
    pub assembling_machine: Option<AssemblingMachinePrototype>,
    pub transport_belt: Option<TransportBeltPrototype>,
    pub splitter: Option<SplitterPrototype>,
    pub inserter: Option<InserterPrototype>,
    pub electric_pole: Option<ElectricPolePrototype>,
    pub electric_energy_source: Option<ElectricEnergySourcePrototype>,
    pub steam_engine: Option<SteamEnginePrototype>,
    pub solar_panel: Option<SolarPanelPrototype>,
    pub accumulator: Option<AccumulatorPrototype>,
    pub radar: Option<RadarPrototype>,
    pub boiler: Option<BoilerPrototype>,
    pub offshore_pump: Option<OffshorePumpPrototype>,
    pub pump: Option<PumpPrototype>,
    pub pumpjack: Option<PumpjackPrototype>,
    pub underground_pipe: Option<UndergroundPipePrototype>,
    pub fluid_boxes: Vec<FluidBoxPrototype>,
    /// Present on every entity that participates in a heat network.
    pub heat_buffer: Option<HeatBufferPrototype>,
    /// Present when the entity works off heat drawn from its own heat buffer.
    pub heat_energy_source: Option<HeatEnergySourcePrototype>,
    pub nuclear_reactor: Option<NuclearReactorPrototype>,
    /// Present on entities that anchor a robot network; see
    /// [`RoboportPrototype`].
    pub roboport: Option<RoboportPrototype>,
    /// Present on chests that take part in the logistic network covering them;
    /// see [`LogisticChestPrototype`].
    pub logistic_chest: Option<LogisticChestPrototype>,
    /// Present when the entity can take damage and be destroyed.
    pub max_health: Option<u32>,
    /// Pollution emitted into the entity's chunk while it is actively
    /// working, in milli-pollution-units per minute.
    pub pollution_per_minute_milli: Option<u32>,
    pub gun_turret: Option<GunTurretPrototype>,
    pub laser_turret: Option<LaserTurretPrototype>,
    pub enemy_spawner: Option<EnemySpawnerPrototype>,
    /// Present when red and green circuit wires can be attached.
    pub circuit_connector: Option<CircuitConnectorPrototype>,
    /// Present on combinator entities.
    pub combinator: Option<CombinatorPrototype>,
    /// Present on rail pieces; see [`RailPiecePrototype`].
    pub rail_piece: Option<RailPiecePrototype>,
    /// Present on locomotives and wagons; see [`RollingStockPrototype`].
    pub rolling_stock: Option<RollingStockPrototype>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BeaconPrototype {
    pub effect_radius_tiles: u16,
    pub transmission_permyriad: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
pub enum BuildingCategory {
    Logistics,
    Production,
    Power,
    Fluids,
    Storage,
    Defense,
    /// Wires, combinators, and lamps.
    Circuit,
    /// Items that rewrite terrain instead of placing an entity.
    Terrain,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct GunTurretPrototype {
    /// Maximum distance from the turret's footprint to a target, in tiles.
    pub range_tiles: u32,
    pub cooldown_ticks: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct LaserTurretPrototype {
    /// Maximum distance from the turret's footprint to a target, in tiles.
    pub range_tiles: u32,
    pub damage: u32,
    pub cooldown_ticks: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EnemySpawnerPrototype {
    /// Upper bound on units alive at once that this spawner produced.
    pub max_alive_units: u32,
    /// Units kept alive near the spawner without pollution input.
    pub guard_units: u32,
    /// Ticks between free guard spawns while below `guard_units`.
    pub free_spawn_interval_ticks: u32,
    /// Absorbed pollution required to spawn one attacking unit, in
    /// milli-pollution-units.
    pub unit_spawn_pollution_cost_milli: u32,
    /// Pollution drained from the spawner's chunk each tick, in
    /// milli-pollution-units.
    pub pollution_absorption_per_tick_milli: u32,
    pub unit: UnitPrototype,
}

/// Combat stats of the mobile unit an enemy spawner produces.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct UnitPrototype {
    pub max_health: u32,
    pub damage: u32,
    pub attack_cooldown_ticks: u32,
    /// Movement speed in fixed-point position units per tick (1024 = one
    /// tile per tick).
    pub speed_fixed_per_tick: u32,
    /// Distance within which an idle unit acquires player targets, in tiles.
    pub aggro_radius_tiles: u32,
}

/// Deterministic enemy simulation tuning stored in the prototype catalog.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EnemyGameplayConfig {
    pub generated_colony_min_spawners: u8,
    pub generated_colony_max_spawners: u8,
    pub max_spawners_per_colony: u8,
    pub colony_spawner_radius_tiles: u8,
    pub outpost_growth_interval_ticks: u32,
    pub raid_staging_timeout_ticks: u32,
    pub raid_cooldown_ticks: u32,
    pub expansion_minimum_age_ticks: u32,
    pub expansion_interval_ticks: u32,
    pub expansion_retry_ticks: u32,
    pub expansion_min_distance_chunks: u8,
    pub expansion_max_distance_chunks: u8,
    pub expansion_candidate_limit: u16,
    pub expansion_colony_spacing_chunks: u8,
    pub expansion_player_spacing_tiles: u16,
    pub evolution_time_interval_ticks: u32,
    pub evolution_time_points: u16,
    pub evolution_pollution_units_per_point: u16,
    pub evolution_spawner_destroyed_points: u16,
    pub evolution_colony_destroyed_points: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidBoxPrototype {
    pub capacity_milliunits: u64,
    pub filter: Option<FluidId>,
    pub io: FluidBoxIo,
    pub connections: Vec<FluidConnectionPrototype>,
}

/// Recipe-facing role of a fluid box. Passive boxes (pipes, tanks) are
/// `InputOutput`; crafting machines declare which boxes feed fluid
/// ingredients and which receive fluid products. The role only affects
/// recipe matching; network equalization treats all boxes alike.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum FluidBoxIo {
    #[default]
    InputOutput,
    Input,
    Output,
}

/// One tile-edge opening on an entity's footprint: the local tile the opening
/// sits on plus which of that tile's four edges it faces. Fluid boxes and heat
/// buffers both join to neighbours across such edges, so they share this shape
/// and the rotation maths that resolves it into world space.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EdgeConnectionPrototype {
    pub local_offset: IVec2,
    pub side: ConnectionSide,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum ConnectionSide {
    North,
    East,
    South,
    West,
}

/// Fluid-facing names for the shared edge-connection types. Fluid prototypes and
/// the pipe-connection preview read better with them, and they keep the fluid
/// call sites unchanged now that heat networks share the geometry.
pub type FluidConnectionPrototype = EdgeConnectionPrototype;
pub type FluidConnectionSide = ConnectionSide;

/// Fixed-point units per tile for sub-tile geometry declared in prototypes.
///
/// Deliberately the same scale free-moving positions use in the simulation
/// (`factory_sim::POSITION_SCALE`), so a declared travel path and a moving
/// entity are measured in one unit and neither side ever converts.
pub const POSITION_SCALE: i32 = 1024;

/// Cardinal heading used by rail geometry: the direction a train travels when
/// it leaves a rail piece through one of its ends.
///
/// Deliberately *not* [`ConnectionSide`]. That enum names the tile edge a fluid
/// or heat opening sits on, and its world mapping puts `North` at `-y`; rail
/// geometry follows the placement convention the rest of the simulation uses for
/// belts, drills, and inserters, where `North` is `+y`. Keeping the two apart is
/// what stops a rail path from being drawn upside down against its own
/// connections.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum RailHeading {
    North,
    East,
    South,
    West,
}

impl RailHeading {
    pub const fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
        }
    }

    /// Unit step along this heading, in whole units of the caller's choosing.
    pub const fn step(self) -> (i32, i32) {
        match self {
            Self::North => (0, 1),
            Self::East => (1, 0),
            Self::South => (0, -1),
            Self::West => (-1, 0),
        }
    }

    pub const fn is_perpendicular_to(self, other: Self) -> bool {
        let (x, y) = self.step();
        let (other_x, other_y) = other.step();
        x * other_x + y * other_y == 0
    }
}

/// A point in prototype-local sub-tile space: [`POSITION_SCALE`] units per tile,
/// measured from the unrotated footprint's minimum corner with `+x` east and
/// `+y` north.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RailPointPrototype {
    pub x: i32,
    pub y: i32,
}

impl RailPointPrototype {
    /// Squared distance to another point.
    ///
    /// Computed in 128 bits because two coordinates at opposite `i32` extremes
    /// square to more than an `i64` holds: a hand-written catalog must be
    /// *rejected* by validation, not overflow inside it. Every comparison of
    /// rail distances goes through this so none of them can drift into a
    /// narrower intermediate.
    pub fn squared_distance_to(self, other: Self) -> i128 {
        let dx = i128::from(other.x) - i128::from(self.x);
        let dy = i128::from(other.y) - i128::from(self.y);
        dx * dx + dy * dy
    }
}

/// One end of a rail piece: where a train enters or leaves, and which way it is
/// travelling when it does.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RailEndPrototype {
    pub position: RailPointPrototype,
    /// Direction of travel *leaving* the piece here. Two rails connect where one
    /// piece's end meets another's at the same point with the opposite heading,
    /// which is what makes the join a statement about travel rather than about
    /// two footprints happening to touch.
    pub heading: RailHeading,
}

/// The path between a rail piece's two ends.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum RailCurvePrototype {
    /// A straight run. The ends face opposite headings and the segment between
    /// them runs along that axis.
    Straight,
    /// A quarter-circle arc around `center`, taken the short way round: both
    /// ends sit the same distance from the center, each end's radius is
    /// perpendicular to its heading, and the two headings are perpendicular to
    /// each other, so exactly one 90-degree arc joins them.
    QuarterArc { center: RailPointPrototype },
}

/// Sub-tile travel geometry of one rail piece.
///
/// This is the single description of where a train runs on a piece: the
/// simulation rotates it into world space to build the rail graph and to measure
/// edge lengths, and the renderer draws that same curve. Nothing re-derives the
/// shape from a sprite.
///
/// Occupancy stays tile-locked — a piece reserves every tile of its footprint,
/// including the ones its curve only clips — so collision, mining, blueprints,
/// and the map keep working without knowing about sub-tile geometry at all.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RailPiecePrototype {
    /// The two ends, in the order the geometry is written. They are otherwise
    /// interchangeable: a train may run a piece either way, and the rail graph
    /// treats the edge as undirected while remembering the heading at each end.
    pub start: RailEndPrototype,
    pub end: RailEndPrototype,
    pub curve: RailCurvePrototype,
}

/// Numerator and shift of a fixed-point quarter turn (`π / 2`), used to measure
/// arc lengths without floating point. `1_686_629_713 / 2^30` is `π / 2` to
/// nine decimal places, which is exact to the unit for any radius a rail piece
/// can declare.
const QUARTER_TURN_NUMERATOR: i128 = 1_686_629_713;
const QUARTER_TURN_SHIFT: u32 = 30;

impl RailPiecePrototype {
    /// Both ends, for the code that treats them symmetrically.
    pub const fn ends(&self) -> [RailEndPrototype; 2] {
        [self.start, self.end]
    }

    /// Distance from the arc center to either end, or zero for a straight.
    pub fn radius(&self) -> i64 {
        match self.curve {
            RailCurvePrototype::Straight => 0,
            RailCurvePrototype::QuarterArc { center } => distance(center, self.start.position),
        }
    }

    /// Travel length of the piece in fixed-point units. This is the weight the
    /// rail graph carries on the edge for this piece.
    pub fn length(&self) -> i64 {
        match self.curve {
            RailCurvePrototype::Straight => distance(self.start.position, self.end.position),
            RailCurvePrototype::QuarterArc { .. } => {
                ((i128::from(self.radius()) * QUARTER_TURN_NUMERATOR) >> QUARTER_TURN_SHIFT) as i64
            }
        }
    }
}

/// Euclidean distance between two sub-tile points, rounded down to whole
/// fixed-point units. Integer arithmetic throughout keeps the length of a piece
/// a pure function of its declaration on every platform.
///
/// Saturates rather than wrapping, so a catalog declaring absurd coordinates is
/// still rejected by validation instead of producing wrapped geometry.
fn distance(from: RailPointPrototype, to: RailPointPrototype) -> i64 {
    i64::try_from(from.squared_distance_to(to).isqrt()).unwrap_or(i64::MAX)
}

/// One piece of rolling stock: what it weighs, how long it is, and how hard it
/// can brake.
///
/// A train's motion is a function of these totals and nothing else: mass,
/// tractive force, and braking force are summed over the coupled stock and the
/// result drives one shared velocity. Keeping the declaration in force and mass
/// rather than in "acceleration per tick" is what lets a long train be sluggish
/// for the same reason a real one is, and what lets the stopping-distance
/// arithmetic a station needs come out of the same numbers.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RollingStockPrototype {
    /// Distance between the piece's two couplers, in fixed-point units
    /// ([`POSITION_SCALE`] per tile). This is the length of track the piece
    /// occupies and the spacing coupled stock keeps.
    pub length_fixed: i32,
    /// Mass in kilograms. Acceleration is force over the train's total mass, so
    /// this is what a wagon costs the locomotive pulling it.
    pub weight_kilograms: u32,
    /// Braking force this piece contributes to its train, in newtons. Wagons
    /// brake too, which is why a longer train is not simply slower to stop.
    pub braking_force_newtons: u32,
    /// Top speed this piece allows, in fixed-point units per tick. A train runs
    /// at the lowest top speed among its stock.
    pub max_speed_fixed_per_tick: u32,
    /// Present on locomotives; see [`LocomotivePrototype`].
    pub locomotive: Option<LocomotivePrototype>,
}

/// The powered half of a locomotive.
///
/// Fuel is burnt through the ordinary [`BurnerPrototype`] path the entity also
/// declares; what belongs here is the force that fuel buys. A locomotive out of
/// fuel still weighs and still brakes — it simply stops pulling.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct LocomotivePrototype {
    /// Force at the wheels while the throttle is open, in newtons.
    pub tractive_force_newtons: u32,
}

/// Ambient temperature every heat buffer starts and settles at, in degrees.
/// Heat buffer energy is stored relative to this floor, so an idle network holds
/// exactly zero joules and cannot be drained into negative temperatures.
pub const HEAT_AMBIENT_TEMPERATURE_DEGREES: u32 = 15;

/// A thermal mass that joins its neighbours into a heat network.
///
/// `specific_heat_joules_per_degree` is how much energy raises this buffer by
/// one degree; together with `max_temperature_degrees` it fixes the buffer's
/// energy capacity above [`HEAT_AMBIENT_TEMPERATURE_DEGREES`]. Heat networks
/// settle to a common temperature, so a buffer's specific heat is also its
/// share of the network's stored energy.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct HeatBufferPrototype {
    pub specific_heat_joules_per_degree: u64,
    pub max_temperature_degrees: u32,
    pub connections: Vec<EdgeConnectionPrototype>,
}

impl HeatBufferPrototype {
    /// Energy the buffer holds at `max_temperature_degrees`, measured above
    /// ambient. This is the capacity the network solve fills.
    pub fn capacity_joules(&self) -> u64 {
        let degrees_above_ambient = self
            .max_temperature_degrees
            .saturating_sub(HEAT_AMBIENT_TEMPERATURE_DEGREES);
        self.specific_heat_joules_per_degree
            .saturating_mul(u64::from(degrees_above_ambient))
    }
}

/// Energy source that draws from the entity's own heat buffer, the heat
/// counterpart of [`ElectricEnergySourcePrototype`] and [`BurnerPrototype`].
/// The entity only works once its buffer reaches
/// `min_working_temperature_degrees`, which is what makes a cold heat network
/// warm up before it delivers power.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct HeatEnergySourcePrototype {
    pub energy_usage_watts: u64,
    pub min_working_temperature_degrees: u32,
}

/// Burns fuel cells into its own heat buffer instead of into electricity, and
/// leaves the fuel's [`ItemPrototype::burnt_result`] in an output slot.
///
/// `neighbour_bonus_permyriad` is the extra output each reactor sharing a
/// footprint edge contributes, so a row of reactors is worth more than the same
/// reactors spread out.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct NuclearReactorPrototype {
    pub heat_output_watts: u64,
    pub neighbour_bonus_permyriad: u32,
}

/// Anchor of a robot network: a powered building that covers a square of the
/// world and merges with its neighbours into one network.
///
/// The two radii are half-widths measured from the roboport's footprint center,
/// so a radius of `r` covers a `(2r + 1)`-tile square (see
/// [`roboport_coverage_bounds`]). They serve different purposes and are
/// deliberately independent:
///
/// * `logistic_radius_tiles` is the **connection** rule. Two roboports whose
///   logistic squares overlap belong to the same network, which is what lets a
///   player grow one network by placing roboports within reach of each other.
/// * `construction_radius_tiles` is the **coverage** rule. Construction
///   coverage is the union of the member squares, not one network-wide
///   rectangle, so an L-shaped chain of roboports covers an L, not its bounding
///   box.
///
/// `charging_energy_buffer_joules` is the internal buffer a roboport fills from
/// its electric network. Robots charge from that buffer rather than from the
/// network directly, so a roboport draws a steady refill instead of spiking the
/// network every time a robot docks.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RoboportPrototype {
    pub construction_radius_tiles: u16,
    pub logistic_radius_tiles: u16,
    /// Slots holding the network's robots.
    pub robot_slot_count: usize,
    /// Slots holding repair packs and other construction material.
    pub material_slot_count: usize,
    pub charging_energy_buffer_joules: u64,
    /// Robots that can charge here at once. Further arrivals queue, which is
    /// what makes a roboport a throughput limit rather than an infinite one.
    pub charging_pad_count: u8,
    /// Rate one pad delivers to the robot on it, in watts. Drawn from
    /// `charging_energy_buffer_joules`, never from the electric network
    /// directly.
    pub charging_pad_watts: u64,
}

/// Logistic role a chest plays inside the robot network that covers it.
///
/// This is prototype metadata rather than a distinct [`EntityKind`] on purpose:
/// every variant is still a chest with an inventory, a container window, and a
/// circuit connector, and the only thing that differs is what the network is
/// allowed to do with the contents. Keeping one kind means the save format, the
/// transfer paths, and the container UI never learn about logistics at all.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct LogisticChestPrototype {
    pub mode: LogisticChestMode,
    /// Configurable rows the chest exposes. Providers have none, a storage
    /// chest has exactly one (its filter), and requester and buffer chests have
    /// one row per item they ask for.
    pub request_slot_count: u8,
}

/// What a logistic chest offers to, and asks of, its network.
///
/// The two provider modes differ only in urgency: a passive provider waits to
/// be asked, while an active provider wants its contents moved out even when
/// nothing has requested them. Both are pure supply, which is why neither
/// carries request rows.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum LogisticChestMode {
    /// Supplies on demand.
    PassiveProvider,
    /// Supplies on demand and pushes its surplus into storage unasked.
    ActiveProvider,
    /// Accepts what the network has nowhere else to put, and supplies from it.
    /// Its single row is a filter: when set, only that item may be stored here.
    Storage,
    /// Keeps a stock on hand: requests up to its configured amounts and
    /// supplies from what it holds.
    Buffer,
    /// Pure demand: requests up to its configured amounts and supplies nothing.
    Requester,
}

impl LogisticChestMode {
    /// Whether robots may take items out of a chest in this mode.
    pub const fn supplies_network(self) -> bool {
        matches!(
            self,
            Self::PassiveProvider | Self::ActiveProvider | Self::Storage | Self::Buffer
        )
    }

    /// Whether the configured rows are amounts to keep stocked, as opposed to
    /// the storage chest's single filter row.
    pub const fn requests_items(self) -> bool {
        matches!(self, Self::Buffer | Self::Requester)
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::PassiveProvider => "Passive Provider",
            Self::ActiveProvider => "Active Provider",
            Self::Storage => "Storage",
            Self::Buffer => "Buffer",
            Self::Requester => "Requester",
        }
    }
}

/// Flight profile of a robot a roboport stations, dispatches, and charges.
///
/// A robot is an item while it sits in a roboport's robot slots and a
/// free-moving unit while it flies, so the numbers that govern the flight live
/// on the item prototype: taking one out of the robot slots is what creates a
/// unit with this profile, and docking turns it back into the same item.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RobotPrototype {
    /// Role this robot can perform. The two kinds share the stationing,
    /// flight, energy, and charging machinery and differ only in which
    /// dispatcher may claim them: construction jobs never take a logistic
    /// robot, and deliveries never take a construction one.
    pub kind: RobotKind,
    /// Flight speed in fixed-point position units per tick (1024 = one tile per
    /// tick), the same convention [`UnitPrototype::speed_fixed_per_tick`] uses.
    pub speed_fixed_per_tick: u32,
    /// Energy a fully charged robot carries, in joules.
    pub energy_capacity_joules: u64,
    /// Draw while flying, in watts. A robot only spends energy while it moves,
    /// so one hovering in a charging queue cannot strand itself.
    pub flight_energy_usage_watts: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum RobotKind {
    /// Builds ghosts, deconstructs marked entities, and repairs damage. Draws
    /// its payload from the roboport material slots.
    Construction,
    /// Moves items between the logistic chests its network covers. Carries at
    /// most one stack per trip, which is what makes stack size the throughput
    /// knob rather than a separate cargo number on this prototype.
    Logistic,
}

/// Inclusive tile bounds of the square a roboport radius covers, centered on
/// `footprint`.
///
/// Shared by the network builder, the coverage queries, and the presentation
/// overlay so all three agree on exactly which tiles a roboport reaches. The
/// center is the footprint's lower-left-of-center tile for even sizes, matching
/// [`ElectricPolePrototype::supply_area_tiles`] placement.
pub fn roboport_coverage_bounds(
    footprint_x: i64,
    footprint_y: i64,
    footprint_width: i32,
    footprint_height: i32,
    radius_tiles: u16,
) -> (i64, i64, i64, i64) {
    let radius = i64::from(radius_tiles);
    let center_x = footprint_x + i64::from((footprint_width.max(1) - 1) / 2);
    let center_y = footprint_y + i64::from((footprint_height.max(1) - 1) / 2);
    (
        center_x - radius,
        center_y - radius,
        center_x + radius,
        center_y + radius,
    )
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BurnerPrototype {
    pub energy_usage_watts: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct MiningDrillPrototype {
    pub mining_area: IVec2,
    pub ticks_per_item: u32,
}

/// Furnace crafting behavior. The speed fraction scales smelting recipe
/// times the same way assembler crafting speed does; the energy source
/// (burner or electric) comes from the entity's `burner` /
/// `electric_energy_source` sections.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FurnacePrototype {
    pub crafting_speed_numerator: u32,
    pub crafting_speed_denominator: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct AssemblingMachinePrototype {
    pub crafting_speed_numerator: u32,
    pub crafting_speed_denominator: u32,
    pub input_slot_count: usize,
    pub output_slot_count: usize,
    /// Recipe category this machine crafts; recipes of other categories
    /// cannot be selected on it.
    #[serde(default = "default_assembler_crafting_category")]
    pub crafting_category: CraftingCategory,
}

fn default_assembler_crafting_category() -> CraftingCategory {
    CraftingCategory::Crafting
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TransportBeltPrototype {
    pub speed_subtiles_per_tick: u16,
    pub underground: Option<UndergroundBeltPrototype>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SplitterPrototype {
    pub speed_subtiles_per_tick: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct InserterPrototype {
    pub pickup_offset: IVec2,
    pub drop_offset: IVec2,
    pub pickup_ticks: u32,
    pub drop_ticks: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ElectricPolePrototype {
    pub supply_area_tiles: IVec2,
    pub wire_reach_tiles_x2: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ElectricEnergySourcePrototype {
    pub energy_usage_watts: u64,
    pub drain_watts: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SteamEnginePrototype {
    pub max_power_output_watts: u64,
    pub steam_consumption_per_second_milliunits: u64,
}

/// Fuel-free generator whose output scales with the deterministic day/night
/// cycle. Solar panels feed their network without consuming any fluid or item.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SolarPanelPrototype {
    /// Output at full daylight; scaled down by the current daylight ratio.
    pub max_power_output_watts: u64,
}

/// Energy store that charges from network surplus and discharges into network
/// deficit. Capacity and rates are expressed in joules and watts respectively.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct AccumulatorPrototype {
    pub capacity_joules: u64,
    pub max_charge_watts: u64,
    pub max_discharge_watts: u64,
}

/// Electric map scanner with a frequently refreshed nearby area and a slower
/// deterministic long-range sweep.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RadarPrototype {
    pub nearby_reveal_radius_chunks: u16,
    pub nearby_scan_interval_ticks: u32,
    pub far_scan_radius_chunks: u16,
    pub far_scan_interval_ticks: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BoilerPrototype {
    pub water_consumption_per_second_milliunits: u64,
    pub steam_output_per_second_milliunits: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct OffshorePumpPrototype {
    pub pumping_speed_per_second_milliunits: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PumpPrototype {
    pub pumping_speed_per_second_milliunits: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PumpjackPrototype {
    pub pumping_speed_per_second_milliunits: u64,
    /// Resource cell item this pumpjack must be placed over.
    pub resource_item: ItemId,
    /// Fluid produced into the pumpjack's output fluid box.
    pub output_fluid: FluidId,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct UndergroundBeltPrototype {
    pub part: UndergroundBeltPart,
    pub max_distance: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum UndergroundBeltPart {
    Entrance,
    Exit,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct UndergroundPipePrototype {
    pub part: UndergroundBeltPart,
    pub max_distance: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TilePrototype {
    pub id: TileId,
    pub name: String,
    pub collision_mask: CollisionMask,
    /// Pollution absorbed by one tile of this terrain, in
    /// milli-pollution-units per minute.
    pub pollution_absorption_per_minute_milli: u32,
    /// Player walking speed on this terrain as a percentage of the base speed.
    /// 100 is unmodified; paved tiles declare a value above 100.
    pub walking_speed_percent: u16,
    /// Base sRGB color `[r, g, b]` used by the front-end to paint this
    /// terrain. Inert data here (this crate has no rendering dependency); the
    /// renderer reads it to give each biome a visual identity instead of
    /// hard-coding terrain colors.
    pub color: [u8; 3],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TechnologyPrototype {
    pub id: TechnologyId,
    pub name: String,
    pub prerequisites: Vec<TechnologyId>,
    pub science_packs: Vec<ItemAmount>,
    pub required_units: u32,
    pub research_time_ticks: u32,
    pub effects: Vec<TechnologyEffect>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum TechnologyEffect {
    UnlockRecipe(RecipeId),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ItemAmount {
    pub item: ItemId,
    pub amount: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidAmount {
    pub fluid: FluidId,
    pub amount_milliunits: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum CraftingCategory {
    Manual,
    Smelting,
    Crafting,
    OilProcessing,
    Chemistry,
    Centrifuging,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum EntityKind {
    ResourcePatch,
    Furnace,
    MiningDrill,
    AssemblingMachine,
    Inserter,
    TransportBelt,
    Splitter,
    Lab,
    Beacon,
    Chest,
    ElectricPole,
    SteamEngine,
    Boiler,
    OffshorePump,
    Pump,
    Pumpjack,
    Pipe,
    StorageTank,
    Wall,
    GunTurret,
    LaserTurret,
    EnemySpawner,
    SolarPanel,
    Accumulator,
    Radar,
    Lamp,
    ConstantCombinator,
    ArithmeticCombinator,
    DeciderCombinator,
    /// Burns fuel cells into its heat buffer; see [`NuclearReactorPrototype`].
    NuclearReactor,
    /// Passive thermal mass that only carries heat between its neighbours.
    HeatPipe,
    /// Boils water into steam using heat instead of burnt fuel, so it reuses
    /// [`BoilerPrototype`] with a [`HeatEnergySourcePrototype`].
    HeatExchanger,
    /// Anchors a robot network and covers a square of the world; see
    /// [`RoboportPrototype`].
    Roboport,
    /// A straight run of track; see [`RailPiecePrototype`].
    RailStraight,
    /// A quarter-turn of track; see [`RailPiecePrototype`].
    RailCurved,
    /// Powered rolling stock: burns fuel into tractive force; see
    /// [`RollingStockPrototype`].
    Locomotive,
    /// Rolling stock carrying an item inventory.
    CargoWagon,
    /// Rolling stock carrying a fluid box.
    FluidWagon,
    /// Splits the rail graph into blocks and admits one train at a time into
    /// the block beyond it; see [`RailSignalKind::Block`].
    RailSignal,
    /// A signal that only clears when the signal beyond it can also clear; see
    /// [`RailSignalKind::Chain`].
    ChainSignal,
    /// A named stopping place beside the track: it carries the name a schedule
    /// asks for and marks where the train that serves it comes to rest.
    TrainStop,
}

impl EntityKind {
    /// Whether this kind is a piece of track, and therefore declares
    /// [`RailPiecePrototype`] geometry and takes part in the rail graph.
    pub const fn is_rail(self) -> bool {
        matches!(self, Self::RailStraight | Self::RailCurved)
    }

    /// What this kind does at a block boundary, or `None` for anything that is
    /// not a signal.
    ///
    /// The rule a signal follows is the whole of what distinguishes the two
    /// kinds, so it is read off the kind rather than carried in a prototype
    /// section that could disagree with it.
    pub const fn rail_signal_kind(self) -> Option<RailSignalKind> {
        match self {
            Self::RailSignal => Some(RailSignalKind::Block),
            Self::ChainSignal => Some(RailSignalKind::Chain),
            _ => None,
        }
    }

    /// Whether this kind stands beside track and partitions it into blocks.
    pub const fn is_rail_signal(self) -> bool {
        self.rail_signal_kind().is_some()
    }

    /// Whether this kind stands on one tile beside the track and binds to the
    /// rail it is nearest.
    ///
    /// Signals and stops answer alike because the binding rule is the same one:
    /// a single tile with a neighbourhood of track around it, so "the nearest
    /// rail" is a question about eight tiles rather than about the world. The
    /// catalog loader enforces the one-tile footprint the rule rests on.
    pub const fn binds_to_nearby_rail(self) -> bool {
        self.is_rail_signal() || matches!(self, Self::TrainStop)
    }

    /// Whether this kind runs *on* track rather than being track, and therefore
    /// declares [`RollingStockPrototype`] motion metadata.
    ///
    /// Rolling stock is deliberately not tile-locked: it sits between tiles, so
    /// it never enters the occupancy grid and its footprint size is only what
    /// the renderer draws.
    pub const fn is_rolling_stock(self) -> bool {
        matches!(self, Self::Locomotive | Self::CargoWagon | Self::FluidWagon)
    }
}

/// What a signal does when the block it guards cannot be claimed.
///
/// Both kinds partition the track the same way — a block boundary is a signal
/// position, whichever kind stands there — and both admit one train at a time.
/// They differ only in what happens when the claim fails, which is why this is
/// one enum over one placement rule rather than two unrelated entities.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize)]
pub enum RailSignalKind {
    /// Holds the train at the signal and lets it wait there. The ordinary block
    /// signal, and the only kind it is safe to stop a train at.
    Block,
    /// Clears only when the signal beyond the block it guards can itself clear.
    /// Placed where stopping would foul something — the exits of a junction —
    /// so a train that could not get all the way through waits before it rather
    /// than inside it.
    Chain,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CollisionMask {
    pub layers: Vec<CollisionLayer>,
}

/// Format version accepted for [`WorldGenerationConfig`]; configs declaring a
/// different version are rejected at load time instead of being misread.
///
/// Version 2 replaced the single weighted-band `terrain` selector with a
/// data-driven biome table classified from three independent climate channels.
/// Version 3 split resource patch density from resource selection weights.
pub const WORLD_GENERATION_FORMAT_VERSION: u32 = 3;

/// Data-driven world generation rules: terrain distribution, starting area,
/// and resource patch definitions. Loaded from the `world_generation` section
/// of a prototype catalog; a catalog without that section gets the empty
/// default, which generates a bare fallback-tile world without resources.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct WorldGenerationConfig {
    pub version: u32,
    pub starting_area: StartingAreaConfig,
    /// Independent fractal-noise parameters for the elevation, moisture, and
    /// temperature climate channels that drive biome classification.
    pub climate_noise: ClimateNoiseConfig,
    /// Ordered biome table: each tile is classified by finding the first biome
    /// whose elevation/moisture/temperature ranges all contain the sampled
    /// climate. Order encodes priority (specialized biomes first, catch-alls
    /// last); a tile matching no biome falls back to the first tile prototype.
    /// Tile collision behaviour derives from the tile prototype's collision
    /// mask.
    pub biomes: Vec<BiomeConfig>,
    pub patch_grid: ResourcePatchGridConfig,
    /// Distance-based reward for expanding outward; `None` keeps every patch
    /// at its base richness and radius.
    pub distance_scaling: Option<ResourceDistanceScalingConfig>,
    pub resources: Vec<ResourceGenerationConfig>,
    /// Enemy spawner placement rules; `None` generates a world without
    /// enemies.
    pub enemy_bases: Option<EnemyBaseGenerationConfig>,
}

impl Default for WorldGenerationConfig {
    fn default() -> Self {
        Self {
            version: WORLD_GENERATION_FORMAT_VERSION,
            starting_area: StartingAreaConfig {
                min_chunk: 0,
                max_chunk: 0,
            },
            climate_noise: ClimateNoiseConfig::default(),
            biomes: Vec::new(),
            patch_grid: ResourcePatchGridConfig {
                cell_size: 40,
                jitter: 16,
                edge_noise: 3,
                patch_chance_percent: 100,
            },
            distance_scaling: None,
            resources: Vec::new(),
            enemy_bases: None,
        }
    }
}

/// Deterministic per-chunk enemy spawner placement: each generated chunk
/// beyond `min_distance_tiles` from the origin rolls `frequency_percent` for
/// one spawner at a seed-derived position inside the chunk.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EnemyBaseGenerationConfig {
    pub spawner_entity: EntityPrototypeId,
    /// Chance (0-100) that an eligible chunk contains a spawner.
    pub frequency_percent: u8,
    /// Chunks whose center is closer to the origin than this stay clear.
    pub min_distance_tiles: u32,
}

/// Inclusive chunk range generated up front when a world is created.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct StartingAreaConfig {
    pub min_chunk: i32,
    pub max_chunk: i32,
}

/// One biome in the classification table: a terrain tile plus the inclusive
/// climate box it occupies. A tile is classified into the first biome (in
/// declaration order) whose three ranges all contain the sampled climate.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BiomeConfig {
    pub tile: TileId,
    pub elevation: ClimateRange,
    pub moisture: ClimateRange,
    pub temperature: ClimateRange,
}

/// Half-open percent range `[min, max)` (`0..=100`) matched against a climate
/// channel sample. `min` is inclusive, `max` exclusive, so adjacent biomes can
/// tile the range without overlap.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ClimateRange {
    pub min: u8,
    pub max: u8,
}

impl ClimateRange {
    /// Whether `percent` (`0..=100`) falls in `[min, max)`.
    pub fn contains(self, percent: u8) -> bool {
        percent >= self.min && percent < self.max
    }
}

/// Independent fractal-noise parameters for the three climate channels that
/// drive biome selection. Each channel is sampled from its own seed-salted
/// noise field so elevation, moisture, and temperature vary independently.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ClimateNoiseConfig {
    pub elevation: TerrainNoiseConfig,
    pub moisture: TerrainNoiseConfig,
    pub temperature: TerrainNoiseConfig,
}

/// Fractal value-noise parameters for one climate channel. `scale` is the base
/// wavelength in tiles of the lowest-frequency octave; each further octave
/// halves the wavelength and amplitude, adding finer detail such as ragged
/// coastlines.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TerrainNoiseConfig {
    pub scale: u32,
    pub octaves: u32,
}

impl Default for TerrainNoiseConfig {
    fn default() -> Self {
        Self {
            scale: 32,
            octaves: 3,
        }
    }
}

/// Poisson-like placement grid for resource patch centers: one candidate
/// center per `cell_size` tiles, offset by up to `jitter` tiles, with patch
/// edges roughened by up to `edge_noise` tiles.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ResourcePatchGridConfig {
    pub cell_size: i32,
    pub jitter: i32,
    pub edge_noise: i32,
    /// Chance (0-100) that a grid cell contains a non-starting resource patch.
    pub patch_chance_percent: u8,
}

/// Linear distance scaling for grid-placed resource patches, rewarding
/// expansion away from the spawn: for every `interval_tiles` of distance
/// between a patch center and the world origin, the patch gains
/// `richness_bonus_percent` percent of its base richness and
/// `radius_bonus_tiles` tiles of radius. The radius bonus is capped at
/// `max_radius_bonus_tiles` so chunk generation can bound how far away a
/// patch center may still reach into a chunk. Starting patches are spawn
/// guarantees and are never scaled.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ResourceDistanceScalingConfig {
    pub interval_tiles: u32,
    pub richness_bonus_percent: u32,
    pub radius_bonus_tiles: u8,
    pub max_radius_bonus_tiles: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ResourceGenerationConfig {
    pub resource_item: ItemId,
    pub extraction: ResourceExtraction,
    /// Relative weight used to select this resource when a grid cell spawns a
    /// patch. A weight of zero excludes it from random patch selection.
    pub selection_weight: u32,
    pub radius: i32,
    pub richness: u32,
    /// Guaranteed patch center near the origin so starter worlds always
    /// contain the resource; offsets are in tiles.
    pub starting_patch: Option<IVec2>,
}

/// How a generated resource cell is extracted. `Solid` resources are minable
/// by drills and the player; `Fluid` resources are extracted by pumpjacks and
/// excluded from mining. This is authoritative for minability regardless of
/// which machine prototypes exist.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum ResourceExtraction {
    Solid,
    Fluid,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum CollisionLayer {
    Ground,
    Water,
    Resource,
    Building,
    Transport,
}
