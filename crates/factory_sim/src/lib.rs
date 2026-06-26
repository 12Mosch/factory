use factory_data::{
    CraftingCategory, EntityKind, EntityPrototypeId, ItemId, PrototypeCatalog, RecipeId, TileId,
};
use smallvec::SmallVec;
use std::collections::{BTreeMap, VecDeque};
use std::hash::{Hash, Hasher};

pub const CHUNK_SIZE: i32 = 32;
pub const PLAYER_MOVEMENT_SPEED_TILES_PER_SECOND: f32 = 5.0;
pub const PLAYER_MINING_SPEED: f32 = 0.5;
pub const ORE_MINING_TIME_SECONDS: f32 = 1.0;
pub const MANUAL_MINING_REACH_TILES: f32 = 2.5;
pub const MANUAL_MINING_TICKS_PER_ITEM: u32 =
    (ORE_MINING_TIME_SECONDS / PLAYER_MINING_SPEED * FIXED_SIM_TICKS_PER_SECOND) as u32;
pub const PLAYER_INVENTORY_SLOT_COUNT: usize = 80;
const FIXED_SIM_TICKS_PER_SECOND: f32 = 60.0;
const PLAYER_POSITION_SCALE: i64 = 1024;
const TEST_WORLD_MIN_CHUNK: i32 = -2;
const TEST_WORLD_MAX_CHUNK: i32 = 1;
const RESOURCE_PATCH_GRID_SIZE: i32 = 40;
const RESOURCE_PATCH_GRID_JITTER: i32 = 16;
const RESOURCE_PATCH_EDGE_NOISE: i32 = 3;
pub const BURNER_MINING_DRILL_FUEL_SLOT_INDEX: usize = 0;
pub const BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX: usize = 0;
pub const FURNACE_INPUT_SLOT_INDEX: usize = 0;
pub const FURNACE_FUEL_SLOT_INDEX: usize = 0;
pub const FURNACE_OUTPUT_SLOT_INDEX: usize = 0;
pub const ASSEMBLING_MACHINE_INPUT_SLOT_COUNT: usize = 4;
pub const ASSEMBLING_MACHINE_OUTPUT_SLOT_COUNT: usize = 1;
pub const BELT_SUBTILES_PER_TILE: u16 = 256;
pub const BELT_ITEM_SPACING_SUBTILES: u16 = 64;
pub const BASIC_BELT_SPEED_SUBTILES_PER_TICK: u16 = 8;
pub const BASIC_INSERTER_PICKUP_TICKS: u32 = 35;
pub const BASIC_INSERTER_DROP_TICKS: u32 = 35;
const FIXED_SIM_TICKS_PER_SECOND_F64: f64 = 60.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tick(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Inventory {
    pub slots: Vec<Option<ItemStack>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ItemStack {
    pub item_id: ItemId,
    pub count: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InserterState {
    WaitingForItem,
    Picking { ticks_left: u32 },
    Holding { item: ItemStack },
    Dropping { ticks_left: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryError {
    UnknownItem,
    InsufficientSpace,
    InsufficientItems,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CraftingQueue {
    pub entries: VecDeque<CraftingJob>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CraftingJob {
    pub recipe_id: RecipeId,
    pub remaining_ticks: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CraftingError {
    MissingRecipe(RecipeId),
    NotManualRecipe(RecipeId),
    InsufficientIngredients,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Simulation {
    pub tick: u64,
    pub world: WorldSim,
    pub entities: EntityStore,
    pub player: PlayerState,
    pub player_inventory: Inventory,
    pub manual_mining_progress: Option<ManualMiningProgress>,
    pub crafting_queue: CraftingQueue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerState {
    x: i64,
    y: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ManualMiningTarget {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ManualMiningProgress {
    pub target: ManualMiningTarget,
    pub progress_ticks: u32,
    pub required_ticks: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WorldSim {
    pub seed: u64,
    pub prototypes: PrototypeCatalog,
    pub chunks: BTreeMap<ChunkCoord, Chunk>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chunk {
    pub coord: ChunkCoord,
    pub tiles: Vec<TileCell>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TileCell {
    pub tile_id: TileId,
    pub collision: TileCollision,
    pub resource: Option<ResourceCell>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileCollision {
    pub walkable: bool,
    pub buildable: bool,
    pub minable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceCell {
    pub resource_item: ItemId,
    pub amount: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MinedResource {
    pub resource_item: ItemId,
    pub amount: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BurnerMiningDrillState {
    pub energy: BurnerEnergy,
    pub mining_progress_ticks: u32,
    pub mining_required_ticks: u32,
    pub resource_target: Option<ManualMiningTarget>,
    pub output_slot: Option<ItemStack>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FurnaceState {
    pub input_slot: Option<ItemStack>,
    pub energy: BurnerEnergy,
    pub output_slot: Option<ItemStack>,
    pub active_recipe: Option<RecipeId>,
    pub crafting_progress_ticks: u32,
    pub crafting_required_ticks: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AssemblingMachineState {
    pub selected_recipe: Option<RecipeId>,
    pub input_inventory: Inventory,
    pub output_inventory: Inventory,
    pub crafting_progress_ticks: u32,
    pub crafting_required_ticks: u32,
    pub crafting_speed_numerator: u32,
    pub crafting_speed_denominator: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AssemblerIngredientStatus {
    pub item: ItemId,
    pub required: u32,
    pub available: u32,
    pub missing: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BeltSegment {
    pub dir: Direction,
    pub lanes: [BeltLane; 2],
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BeltLane {
    pub items: SmallVec<[BeltItem; 8]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeltItem {
    pub item_id: ItemId,
    pub position_subtile: u16,
}

#[derive(Clone, Debug)]
pub struct BurnerEnergy {
    pub fuel_slot: Option<ItemStack>,
    pub energy_remaining_joules: f64,
    pub energy_usage_watts: f64,
}

impl PartialEq for BurnerEnergy {
    fn eq(&self, other: &Self) -> bool {
        self.fuel_slot == other.fuel_slot
            && self.energy_remaining_joules.to_bits() == other.energy_remaining_joules.to_bits()
            && self.energy_usage_watts.to_bits() == other.energy_usage_watts.to_bits()
    }
}

impl Eq for BurnerEnergy {}

impl Hash for BurnerEnergy {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.fuel_slot.hash(state);
        self.energy_remaining_joules.to_bits().hash(state);
        self.energy_usage_watts.to_bits().hash(state);
    }
}

impl BeltSegment {
    pub fn new(dir: Direction) -> Self {
        Self {
            dir,
            lanes: [BeltLane::default(), BeltLane::default()],
        }
    }
}

impl Default for BeltSegment {
    fn default() -> Self {
        Self::new(Direction::default())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BurnerDrillError {
    MissingEntity(u64),
    NotBurnerDrill(u64),
    InvalidFuel(ItemId),
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    InsufficientSpace,
    UnknownItem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FurnaceError {
    MissingEntity(u64),
    NotFurnace(u64),
    InvalidInput(ItemId),
    InvalidFuel(ItemId),
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    InsufficientSpace,
    UnknownItem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssemblerError {
    MissingEntity(u64),
    NotAssembler(u64),
    MissingRecipe(RecipeId),
    InvalidRecipe(RecipeId),
    RecipeChangeRequiresEmpty { entity_id: u64 },
    InvalidInput(ItemId),
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    InsufficientSpace,
    UnknownItem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeltError {
    MissingEntity(u64),
    NotTransportBelt(u64),
    InvalidLane { lane_index: usize },
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InserterError {
    MissingEntity(u64),
    NotInserter(u64),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EntityStore {
    entities: Vec<SimEntity>,
    placed_entities: BTreeMap<u64, PlacedEntity>,
    entity_inventories: BTreeMap<u64, Inventory>,
    burner_mining_drills: BTreeMap<u64, BurnerMiningDrillState>,
    furnaces: BTreeMap<u64, FurnaceState>,
    assembling_machines: BTreeMap<u64, AssemblingMachineState>,
    transport_belts: BTreeMap<u64, BeltSegment>,
    inserters: BTreeMap<u64, InserterState>,
    occupancy: OccupancyGrid,
    next_entity_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SimEntity {
    pub id: u64,
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlacedEntity {
    pub id: u64,
    pub prototype_id: EntityPrototypeId,
    pub x: i32,
    pub y: i32,
    pub direction: Direction,
    pub footprint: EntityFootprint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrillOutputTarget {
    InternalSlot,
    Inventory(u64),
    Belt(u64),
    Blocked,
}

struct EntityReservation {
    prototype_id: EntityPrototypeId,
    x: i32,
    y: i32,
    direction: Direction,
    footprint: EntityFootprint,
    inventory_slot_count: Option<usize>,
    burner_mining_drill: Option<BurnerMiningDrillState>,
    furnace: Option<FurnaceState>,
    assembling_machine: Option<AssemblingMachineState>,
    transport_belt: Option<BeltSegment>,
    inserter: Option<InserterState>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Direction {
    #[default]
    North,
    East,
    South,
    West,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntityFootprint {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct OccupancyGrid {
    // maps occupied tile -> entity id
    occupied_tiles: BTreeMap<(i32, i32), u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildError {
    MissingPrototype(EntityPrototypeId),
    InvalidFootprint { width: i32, height: i32 },
    OutsideGeneratedChunks { x: i32, y: i32 },
    TileBlocked { x: i32, y: i32 },
    EntityOccupied { x: i32, y: i32, entity_id: u64 },
    MissingEntity(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerError {
    MissingEntity(u64),
    NotContainer(u64),
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    InsufficientSpace,
    UnknownItem,
}

impl Inventory {
    pub fn with_slot_count(slot_count: usize) -> Self {
        Self {
            slots: vec![None; slot_count],
        }
    }

    pub fn player() -> Self {
        Self::with_slot_count(PLAYER_INVENTORY_SLOT_COUNT)
    }

    pub fn can_insert(&self, catalog: &PrototypeCatalog, item_id: ItemId, count: u16) -> bool {
        if count == 0 {
            return true;
        }

        let Some(stack_size) = item_stack_size(catalog, item_id) else {
            return false;
        };

        self.insert_capacity(item_id, stack_size) >= u32::from(count)
    }

    pub fn insert(
        &mut self,
        catalog: &PrototypeCatalog,
        item_id: ItemId,
        count: u16,
    ) -> Result<(), InventoryError> {
        if count == 0 {
            return Ok(());
        }

        let stack_size = item_stack_size(catalog, item_id).ok_or(InventoryError::UnknownItem)?;
        if self.insert_capacity(item_id, stack_size) < u32::from(count) {
            return Err(InventoryError::InsufficientSpace);
        }

        let mut remaining = u32::from(count);

        for stack in self.slots.iter_mut().flatten() {
            if stack.item_id != item_id || stack.count >= stack_size {
                continue;
            }

            let available = u32::from(stack_size - stack.count);
            let inserted = remaining.min(available) as u16;
            stack.count += inserted;
            remaining -= u32::from(inserted);

            if remaining == 0 {
                return Ok(());
            }
        }

        for slot in &mut self.slots {
            if slot.is_some() {
                continue;
            }

            let inserted = remaining.min(u32::from(stack_size)) as u16;
            *slot = Some(ItemStack {
                item_id,
                count: inserted,
            });
            remaining -= u32::from(inserted);

            if remaining == 0 {
                return Ok(());
            }
        }

        Ok(())
    }

    pub fn can_remove(&self, item_id: ItemId, count: u16) -> bool {
        count == 0 || self.count(item_id) >= u32::from(count)
    }

    pub fn remove(&mut self, item_id: ItemId, count: u16) -> Result<(), InventoryError> {
        if count == 0 {
            return Ok(());
        }

        if !self.can_remove(item_id, count) {
            return Err(InventoryError::InsufficientItems);
        }

        let mut remaining = count;
        for slot in &mut self.slots {
            let Some(stack) = slot else {
                continue;
            };

            if stack.item_id != item_id {
                continue;
            }

            let removed = remaining.min(stack.count);
            stack.count -= removed;
            remaining -= removed;

            if stack.count == 0 {
                *slot = None;
            }

            if remaining == 0 {
                return Ok(());
            }
        }

        Ok(())
    }

    pub fn count(&self, item_id: ItemId) -> u32 {
        self.slots
            .iter()
            .filter_map(|slot| slot.as_ref())
            .filter(|stack| stack.item_id == item_id)
            .map(|stack| u32::from(stack.count))
            .sum()
    }

    fn insert_capacity(&self, item_id: ItemId, stack_size: u16) -> u32 {
        self.slots
            .iter()
            .map(|slot| match slot {
                Some(stack) if stack.item_id == item_id && stack.count < stack_size => {
                    u32::from(stack_size - stack.count)
                }
                Some(_) => 0,
                None => u32::from(stack_size),
            })
            .sum()
    }
}

impl From<InventoryError> for ContainerError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::UnknownItem => Self::UnknownItem,
            InventoryError::InsufficientSpace => Self::InsufficientSpace,
            InventoryError::InsufficientItems => {
                unreachable!("container transfers remove a known slot stack")
            }
        }
    }
}

impl From<InventoryError> for BurnerDrillError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::UnknownItem => Self::UnknownItem,
            InventoryError::InsufficientSpace => Self::InsufficientSpace,
            InventoryError::InsufficientItems => {
                unreachable!("burner drill transfers remove a known slot stack")
            }
        }
    }
}

impl From<InventoryError> for FurnaceError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::UnknownItem => Self::UnknownItem,
            InventoryError::InsufficientSpace => Self::InsufficientSpace,
            InventoryError::InsufficientItems => {
                unreachable!("furnace transfers remove a known slot stack")
            }
        }
    }
}

impl From<InventoryError> for AssemblerError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::UnknownItem => Self::UnknownItem,
            InventoryError::InsufficientSpace => Self::InsufficientSpace,
            InventoryError::InsufficientItems => {
                unreachable!("assembler transfers remove a known slot stack")
            }
        }
    }
}

fn stack_in_slot(inventory: &Inventory, slot_index: usize) -> Result<ItemStack, ContainerError> {
    inventory
        .slots
        .get(slot_index)
        .ok_or(ContainerError::InvalidSlot { slot_index })?
        .ok_or(ContainerError::EmptySlot { slot_index })
}

fn ensure_inventory_can_accept(
    catalog: &PrototypeCatalog,
    inventory: &Inventory,
    stack: ItemStack,
) -> Result<(), ContainerError> {
    if inventory.can_insert(catalog, stack.item_id, stack.count) {
        Ok(())
    } else {
        Err(ContainerError::InsufficientSpace)
    }
}

impl Simulation {
    pub fn new(seed: u64, prototypes: PrototypeCatalog) -> Self {
        let world = WorldSim::new(seed, prototypes);
        let entities = EntityStore::new_test_entities(seed);
        let player = find_player_start(&world, &entities.occupancy)
            .expect("test world should contain a walkable player start");
        let mut player_inventory = Inventory::player();
        let burner_mining_drill = item_id(&world.prototypes, "burner_mining_drill");
        let stone_furnace = item_id(&world.prototypes, "stone_furnace");
        player_inventory
            .insert(&world.prototypes, burner_mining_drill, 1)
            .expect("player starting inventory should accept burner mining drill");
        player_inventory
            .insert(&world.prototypes, stone_furnace, 1)
            .expect("player starting inventory should accept stone furnace");

        Self {
            tick: 0,
            world,
            entities,
            player,
            player_inventory,
            manual_mining_progress: None,
            crafting_queue: CraftingQueue::default(),
        }
    }

    pub fn new_test_world(seed: u64) -> Self {
        Self::new(
            seed,
            PrototypeCatalog::load_base().expect("base prototype catalog should load"),
        )
    }

    pub fn tick(&mut self) {
        self.tick += 1;
        self.entities.advance(Tick(self.tick), self.world.seed);
        self.advance_transport_belts();
        self.advance_burner_mining_drills();
        self.advance_furnaces();
        self.advance_assembling_machines();
        self.advance_inserters();
        self.advance_manual_crafting();
    }

    pub fn tick_count(&self) -> u64 {
        self.tick
    }

    pub fn current_tick(&self) -> Tick {
        Tick(self.tick)
    }

    pub fn seed(&self) -> u64 {
        self.world.seed
    }

    pub fn prototype_count(&self) -> usize {
        self.world.prototypes.item_count()
    }

    pub fn state_hash(&self) -> u64 {
        let mut hasher = StableHasher::default();
        self.hash(&mut hasher);
        hasher.finish()
    }

    pub fn move_player(&mut self, direction_x: f32, direction_y: f32, delta_seconds: f32) {
        if delta_seconds <= 0.0 {
            return;
        }

        let direction_length = (direction_x * direction_x + direction_y * direction_y).sqrt();
        if direction_length <= f32::EPSILON {
            return;
        }

        let distance = PLAYER_MOVEMENT_SPEED_TILES_PER_SECOND * delta_seconds;
        self.move_player_by_tiles(
            direction_x / direction_length * distance,
            direction_y / direction_length * distance,
        );
    }

    pub fn move_player_by_tiles(&mut self, delta_x_tiles: f32, delta_y_tiles: f32) {
        let delta_x = tiles_to_fixed(delta_x_tiles);
        let delta_y = tiles_to_fixed(delta_y_tiles);

        self.try_move_player_axis(delta_x, 0);
        self.try_move_player_axis(0, delta_y);
    }

    pub fn can_player_occupy_tile(&self, x: i32, y: i32) -> bool {
        player_can_occupy_tile(&self.world, &self.entities.occupancy, x, y)
    }

    pub fn update_manual_mining(&mut self, target: Option<ManualMiningTarget>) {
        let Some(target) = target else {
            self.manual_mining_progress = None;
            return;
        };

        if !self.is_valid_manual_mining_target(target) {
            self.manual_mining_progress = None;
            return;
        }

        let mut progress = match self.manual_mining_progress {
            Some(progress) if progress.target == target => progress,
            _ => ManualMiningProgress {
                target,
                progress_ticks: 0,
                required_ticks: MANUAL_MINING_TICKS_PER_ITEM,
            },
        };

        if progress.progress_ticks < progress.required_ticks {
            progress.progress_ticks += 1;
        }

        if progress.progress_ticks < progress.required_ticks {
            self.manual_mining_progress = Some(progress);
            return;
        }

        let resource_item = self
            .world
            .tile_at(target.x, target.y)
            .and_then(|tile| tile.resource.map(|resource| resource.resource_item));
        let Some(resource_item) = resource_item else {
            self.manual_mining_progress = None;
            return;
        };

        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, resource_item, 1)
        {
            self.manual_mining_progress = Some(progress);
            return;
        }

        let mined = self
            .world
            .mine_resource_at(target.x, target.y, 1)
            .expect("validated manual mining target should still contain a resource");
        debug_assert_eq!(mined.resource_item, resource_item);
        debug_assert_eq!(mined.amount, 1);
        self.player_inventory
            .insert(&self.world.prototypes, mined.resource_item, 1)
            .expect("manual mining checked inventory capacity before inserting");

        self.manual_mining_progress = if self.is_valid_manual_mining_target(target) {
            Some(ManualMiningProgress {
                target,
                progress_ticks: 0,
                required_ticks: MANUAL_MINING_TICKS_PER_ITEM,
            })
        } else {
            None
        };
    }

    pub fn start_manual_craft(&mut self, recipe_id: RecipeId) -> Result<(), CraftingError> {
        let recipe = self
            .world
            .prototypes
            .recipes
            .get(recipe_id.index())
            .filter(|recipe| recipe.id == recipe_id)
            .ok_or(CraftingError::MissingRecipe(recipe_id))?;

        if !matches!(
            recipe.category,
            CraftingCategory::Crafting | CraftingCategory::Manual
        ) {
            return Err(CraftingError::NotManualRecipe(recipe_id));
        }

        for ingredient in &recipe.ingredients {
            let required = recipe
                .ingredients
                .iter()
                .filter(|candidate| candidate.item == ingredient.item)
                .map(|candidate| u32::from(candidate.amount))
                .sum();
            if self.player_inventory.count(ingredient.item) < required {
                return Err(CraftingError::InsufficientIngredients);
            }
        }

        for ingredient in &recipe.ingredients {
            self.player_inventory
                .remove(ingredient.item, ingredient.amount)
                .expect("manual crafting checked ingredients before removing");
        }

        self.crafting_queue.entries.push_back(CraftingJob {
            recipe_id,
            remaining_ticks: recipe.crafting_time_ticks,
        });

        Ok(())
    }

    fn advance_manual_crafting(&mut self) {
        let Some(job) = self.crafting_queue.entries.front_mut() else {
            return;
        };

        if job.remaining_ticks > 0 {
            job.remaining_ticks -= 1;
        }

        if job.remaining_ticks > 0 {
            return;
        }

        let recipe_id = job.recipe_id;
        let recipe = self
            .world
            .prototypes
            .recipes
            .get(recipe_id.index())
            .filter(|recipe| recipe.id == recipe_id)
            .expect("queued manual craft should reference an existing recipe");
        let mut inventory = self.player_inventory.clone();

        for product in &recipe.products {
            if inventory
                .insert(&self.world.prototypes, product.item, product.amount)
                .is_err()
            {
                return;
            }
        }

        self.player_inventory = inventory;
        self.crafting_queue.entries.pop_front();
    }

    fn is_valid_manual_mining_target(&self, target: ManualMiningTarget) -> bool {
        self.world
            .tile_at(target.x, target.y)
            .and_then(|tile| tile.resource)
            .is_some()
            && self.is_manual_mining_target_in_reach(target)
    }

    fn is_manual_mining_target_in_reach(&self, target: ManualMiningTarget) -> bool {
        let reach = tiles_to_fixed(MANUAL_MINING_REACH_TILES);
        let dx = self.player.x - tile_center_fixed(target.x);
        let dy = self.player.y - tile_center_fixed(target.y);

        dx * dx + dy * dy <= reach * reach
    }

    fn try_move_player_axis(&mut self, delta_x: i64, delta_y: i64) {
        if delta_x == 0 && delta_y == 0 {
            return;
        }

        let candidate = PlayerState {
            x: self.player.x + delta_x,
            y: self.player.y + delta_y,
        };
        let (tile_x, tile_y) = candidate.tile_position();

        if self.can_player_occupy_tile(tile_x, tile_y) {
            self.player = candidate;
        }
    }

    pub fn can_place_entity(
        &self,
        prototype_id: EntityPrototypeId,
        x: i32,
        y: i32,
        direction: Direction,
    ) -> Result<EntityFootprint, BuildError> {
        let footprint = self.world.entity_footprint(prototype_id, x, y, direction)?;
        let prototype = self
            .world
            .prototypes
            .entities
            .get(prototype_id.index())
            .filter(|prototype| prototype.id == prototype_id)
            .ok_or(BuildError::MissingPrototype(prototype_id))?;
        self.world
            .validate_entity_footprint_for_prototype(prototype, &footprint)?;
        self.entities
            .occupancy
            .validate_available(&footprint, None)?;

        Ok(footprint)
    }

    pub fn place_entity(
        &mut self,
        prototype_id: EntityPrototypeId,
        x: i32,
        y: i32,
        direction: Direction,
    ) -> Result<u64, BuildError> {
        let footprint = self.can_place_entity(prototype_id, x, y, direction)?;
        let prototype = &self.world.prototypes.entities[prototype_id.index()];
        let inventory_slot_count = prototype.inventory_slot_count;
        let burner_mining_drill = burner_mining_drill_state_for_prototype(prototype);
        let furnace = furnace_state_for_prototype(prototype);
        let assembling_machine = assembling_machine_state_for_prototype(prototype);
        let transport_belt = transport_belt_segment_for_prototype(prototype, direction);
        let inserter = inserter_state_for_prototype(prototype);
        Ok(self.entities.reserve_entity(EntityReservation {
            prototype_id,
            x,
            y,
            direction,
            footprint,
            inventory_slot_count,
            burner_mining_drill,
            furnace,
            assembling_machine,
            transport_belt,
            inserter,
        }))
    }

    pub fn rotate_entity(
        &mut self,
        entity_id: u64,
        direction: Direction,
    ) -> Result<(), BuildError> {
        let entity = self
            .entities
            .placed_entity(entity_id)
            .cloned()
            .ok_or(BuildError::MissingEntity(entity_id))?;
        let footprint =
            self.world
                .entity_footprint(entity.prototype_id, entity.x, entity.y, direction)?;
        let prototype = self
            .world
            .prototypes
            .entities
            .get(entity.prototype_id.index())
            .filter(|prototype| prototype.id == entity.prototype_id)
            .ok_or(BuildError::MissingPrototype(entity.prototype_id))?;

        self.world
            .validate_entity_footprint_for_prototype(prototype, &footprint)?;
        self.entities
            .occupancy
            .validate_available(&footprint, Some(entity_id))?;
        self.entities
            .update_entity_footprint(entity_id, direction, footprint)
    }

    pub fn remove_entity(&mut self, entity_id: u64) -> Option<PlacedEntity> {
        self.entities.remove_placed_entity(entity_id)
    }

    pub fn entity_inventory(&self, entity_id: u64) -> Result<&Inventory, ContainerError> {
        self.entities.entity_inventory(entity_id)
    }

    pub fn entity_inventory_mut(
        &mut self,
        entity_id: u64,
    ) -> Result<&mut Inventory, ContainerError> {
        self.entities.entity_inventory_mut(entity_id)
    }

    pub fn transfer_player_slot_to_entity(
        &mut self,
        entity_id: u64,
        player_slot_index: usize,
    ) -> Result<(), ContainerError> {
        let stack = stack_in_slot(&self.player_inventory, player_slot_index)?;
        let entity_inventory = self.entities.entity_inventory(entity_id)?;
        ensure_inventory_can_accept(&self.world.prototypes, entity_inventory, stack)?;

        self.player_inventory.slots[player_slot_index] = None;
        self.entities
            .entity_inventory_mut(entity_id)?
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(ContainerError::from)
    }

    pub fn transfer_entity_slot_to_player(
        &mut self,
        entity_id: u64,
        entity_slot_index: usize,
    ) -> Result<(), ContainerError> {
        let stack = {
            let entity_inventory = self.entities.entity_inventory(entity_id)?;
            stack_in_slot(entity_inventory, entity_slot_index)?
        };
        ensure_inventory_can_accept(&self.world.prototypes, &self.player_inventory, stack)?;

        self.entities.entity_inventory_mut(entity_id)?.slots[entity_slot_index] = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(ContainerError::from)
    }

    pub fn burner_drill_state(
        &self,
        entity_id: u64,
    ) -> Result<&BurnerMiningDrillState, BurnerDrillError> {
        self.entities.burner_drill_state(entity_id)
    }

    pub fn transfer_player_slot_to_burner_drill_fuel(
        &mut self,
        entity_id: u64,
        player_slot_index: usize,
    ) -> Result<(), BurnerDrillError> {
        let stack = self
            .player_inventory
            .slots
            .get(player_slot_index)
            .ok_or(BurnerDrillError::InvalidSlot {
                slot_index: player_slot_index,
            })?
            .ok_or(BurnerDrillError::EmptySlot {
                slot_index: player_slot_index,
            })?;

        if fuel_value_joules(&self.world.prototypes, stack.item_id).is_none() {
            return Err(BurnerDrillError::InvalidFuel(stack.item_id));
        }

        let state = self.entities.burner_drill_state(entity_id)?;
        if !burner_fuel_slot_can_accept(&self.world.prototypes, state.energy.fuel_slot, stack) {
            return Err(BurnerDrillError::InsufficientSpace);
        }

        self.player_inventory.slots[player_slot_index] = None;
        let state = self.entities.burner_drill_state_mut(entity_id)?;
        insert_into_single_slot(&mut state.energy.fuel_slot, stack);

        Ok(())
    }

    pub fn transfer_burner_drill_fuel_to_player(
        &mut self,
        entity_id: u64,
    ) -> Result<(), BurnerDrillError> {
        let stack = self
            .entities
            .burner_drill_state(entity_id)?
            .energy
            .fuel_slot
            .ok_or(BurnerDrillError::EmptySlot {
                slot_index: BURNER_MINING_DRILL_FUEL_SLOT_INDEX,
            })?;
        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(BurnerDrillError::InsufficientSpace);
        }

        self.entities
            .burner_drill_state_mut(entity_id)?
            .energy
            .fuel_slot = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(BurnerDrillError::from)
    }

    pub fn transfer_burner_drill_output_to_player(
        &mut self,
        entity_id: u64,
    ) -> Result<(), BurnerDrillError> {
        let stack = self
            .entities
            .burner_drill_state(entity_id)?
            .output_slot
            .ok_or(BurnerDrillError::EmptySlot {
                slot_index: BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX,
            })?;
        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(BurnerDrillError::InsufficientSpace);
        }

        self.entities.burner_drill_state_mut(entity_id)?.output_slot = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(BurnerDrillError::from)
    }

    pub fn furnace_state(&self, entity_id: u64) -> Result<&FurnaceState, FurnaceError> {
        self.entities.furnace_state(entity_id)
    }

    pub fn belt_segment(&self, entity_id: u64) -> Result<&BeltSegment, BeltError> {
        self.entities.belt_segment(entity_id)
    }

    pub fn inserter_state(&self, entity_id: u64) -> Result<&InserterState, InserterError> {
        self.entities.inserter_state(entity_id)
    }

    pub fn insert_item_onto_belt(
        &mut self,
        entity_id: u64,
        lane_index: usize,
        item_id: ItemId,
    ) -> Result<(), BeltError> {
        self.entities
            .insert_item_onto_belt(entity_id, lane_index, item_id)
    }

    pub fn transfer_player_slot_to_furnace_input(
        &mut self,
        entity_id: u64,
        player_slot_index: usize,
    ) -> Result<(), FurnaceError> {
        let stack = self
            .player_inventory
            .slots
            .get(player_slot_index)
            .ok_or(FurnaceError::InvalidSlot {
                slot_index: player_slot_index,
            })?
            .ok_or(FurnaceError::EmptySlot {
                slot_index: player_slot_index,
            })?;

        if first_matching_smelting_recipe(&self.world.prototypes, stack.item_id).is_none() {
            return Err(FurnaceError::InvalidInput(stack.item_id));
        }

        let state = self.entities.furnace_state(entity_id)?;
        if !input_slot_can_accept(&self.world.prototypes, state.input_slot, stack) {
            return Err(FurnaceError::InsufficientSpace);
        }

        self.player_inventory.slots[player_slot_index] = None;
        let state = self.entities.furnace_state_mut(entity_id)?;
        insert_into_single_slot(&mut state.input_slot, stack);

        Ok(())
    }

    pub fn transfer_player_slot_to_furnace_fuel(
        &mut self,
        entity_id: u64,
        player_slot_index: usize,
    ) -> Result<(), FurnaceError> {
        let stack = self
            .player_inventory
            .slots
            .get(player_slot_index)
            .ok_or(FurnaceError::InvalidSlot {
                slot_index: player_slot_index,
            })?
            .ok_or(FurnaceError::EmptySlot {
                slot_index: player_slot_index,
            })?;

        if fuel_value_joules(&self.world.prototypes, stack.item_id).is_none() {
            return Err(FurnaceError::InvalidFuel(stack.item_id));
        }

        let state = self.entities.furnace_state(entity_id)?;
        if !burner_fuel_slot_can_accept(&self.world.prototypes, state.energy.fuel_slot, stack) {
            return Err(FurnaceError::InsufficientSpace);
        }

        self.player_inventory.slots[player_slot_index] = None;
        let state = self.entities.furnace_state_mut(entity_id)?;
        insert_into_single_slot(&mut state.energy.fuel_slot, stack);

        Ok(())
    }

    pub fn transfer_furnace_input_to_player(&mut self, entity_id: u64) -> Result<(), FurnaceError> {
        let stack =
            self.entities
                .furnace_state(entity_id)?
                .input_slot
                .ok_or(FurnaceError::EmptySlot {
                    slot_index: FURNACE_INPUT_SLOT_INDEX,
                })?;
        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(FurnaceError::InsufficientSpace);
        }

        self.entities.furnace_state_mut(entity_id)?.input_slot = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(FurnaceError::from)
    }

    pub fn transfer_furnace_fuel_to_player(&mut self, entity_id: u64) -> Result<(), FurnaceError> {
        let stack = self
            .entities
            .furnace_state(entity_id)?
            .energy
            .fuel_slot
            .ok_or(FurnaceError::EmptySlot {
                slot_index: FURNACE_FUEL_SLOT_INDEX,
            })?;
        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(FurnaceError::InsufficientSpace);
        }

        self.entities.furnace_state_mut(entity_id)?.energy.fuel_slot = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(FurnaceError::from)
    }

    pub fn transfer_furnace_output_to_player(
        &mut self,
        entity_id: u64,
    ) -> Result<(), FurnaceError> {
        let stack =
            self.entities
                .furnace_state(entity_id)?
                .output_slot
                .ok_or(FurnaceError::EmptySlot {
                    slot_index: FURNACE_OUTPUT_SLOT_INDEX,
                })?;
        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(FurnaceError::InsufficientSpace);
        }

        self.entities.furnace_state_mut(entity_id)?.output_slot = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(FurnaceError::from)
    }

    pub fn assembler_state(
        &self,
        entity_id: u64,
    ) -> Result<&AssemblingMachineState, AssemblerError> {
        self.entities.assembler_state(entity_id)
    }

    pub fn select_assembler_recipe(
        &mut self,
        entity_id: u64,
        recipe_id: RecipeId,
    ) -> Result<(), AssemblerError> {
        let recipe = self
            .world
            .prototypes
            .recipes
            .get(recipe_id.index())
            .filter(|recipe| recipe.id == recipe_id)
            .ok_or(AssemblerError::MissingRecipe(recipe_id))?;
        if recipe.category != CraftingCategory::Crafting {
            return Err(AssemblerError::InvalidRecipe(recipe_id));
        }

        let state = self.entities.assembler_state_mut(entity_id)?;
        if state.selected_recipe == Some(recipe_id) {
            return Ok(());
        }
        if !assembler_is_empty_for_recipe_change(state) {
            return Err(AssemblerError::RecipeChangeRequiresEmpty { entity_id });
        }

        state.selected_recipe = Some(recipe_id);
        state.crafting_progress_ticks = 0;
        state.crafting_required_ticks = assembler_required_ticks(
            recipe.crafting_time_ticks,
            state.crafting_speed_numerator,
            state.crafting_speed_denominator,
        );

        Ok(())
    }

    pub fn can_select_assembler_recipe(
        &self,
        entity_id: u64,
        recipe_id: RecipeId,
    ) -> Result<bool, AssemblerError> {
        let recipe = self
            .world
            .prototypes
            .recipes
            .get(recipe_id.index())
            .filter(|recipe| recipe.id == recipe_id)
            .ok_or(AssemblerError::MissingRecipe(recipe_id))?;
        if recipe.category != CraftingCategory::Crafting {
            return Err(AssemblerError::InvalidRecipe(recipe_id));
        }

        let state = self.entities.assembler_state(entity_id)?;
        Ok(state.selected_recipe == Some(recipe_id) || assembler_is_empty_for_recipe_change(state))
    }

    pub fn assembler_ingredient_status(
        &self,
        entity_id: u64,
    ) -> Result<Vec<AssemblerIngredientStatus>, AssemblerError> {
        let state = self.entities.assembler_state(entity_id)?;
        let Some(recipe) = selected_assembler_recipe(&self.world.prototypes, state) else {
            return if let Some(recipe_id) = state.selected_recipe {
                Err(AssemblerError::MissingRecipe(recipe_id))
            } else {
                Ok(Vec::new())
            };
        };
        if recipe.category != CraftingCategory::Crafting {
            return Err(AssemblerError::InvalidRecipe(recipe.id));
        }

        Ok(recipe
            .ingredients
            .iter()
            .map(|ingredient| {
                let required = u32::from(ingredient.amount);
                let available = state.input_inventory.count(ingredient.item);
                AssemblerIngredientStatus {
                    item: ingredient.item,
                    required,
                    available,
                    missing: required.saturating_sub(available),
                }
            })
            .collect())
    }

    pub fn transfer_player_slot_to_assembler_input(
        &mut self,
        entity_id: u64,
        player_slot_index: usize,
    ) -> Result<(), AssemblerError> {
        let stack = self
            .player_inventory
            .slots
            .get(player_slot_index)
            .ok_or(AssemblerError::InvalidSlot {
                slot_index: player_slot_index,
            })?
            .ok_or(AssemblerError::EmptySlot {
                slot_index: player_slot_index,
            })?;
        let state = self.entities.assembler_state(entity_id)?;
        if !assembler_input_can_accept(&self.world.prototypes, state, stack) {
            return Err(AssemblerError::InvalidInput(stack.item_id));
        }
        if !state
            .input_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(AssemblerError::InsufficientSpace);
        }

        self.player_inventory.slots[player_slot_index] = None;
        self.entities
            .assembler_state_mut(entity_id)?
            .input_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(AssemblerError::from)
    }

    pub fn transfer_assembler_input_slot_to_player(
        &mut self,
        entity_id: u64,
        slot_index: usize,
    ) -> Result<(), AssemblerError> {
        let stack = {
            let state = self.entities.assembler_state(entity_id)?;
            stack_in_assembler_inventory_slot(&state.input_inventory, slot_index)?
        };
        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(AssemblerError::InsufficientSpace);
        }

        self.entities
            .assembler_state_mut(entity_id)?
            .input_inventory
            .slots[slot_index] = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(AssemblerError::from)
    }

    pub fn transfer_assembler_output_slot_to_player(
        &mut self,
        entity_id: u64,
        slot_index: usize,
    ) -> Result<(), AssemblerError> {
        let stack = {
            let state = self.entities.assembler_state(entity_id)?;
            stack_in_assembler_inventory_slot(&state.output_inventory, slot_index)?
        };
        if !self
            .player_inventory
            .can_insert(&self.world.prototypes, stack.item_id, stack.count)
        {
            return Err(AssemblerError::InsufficientSpace);
        }

        self.entities
            .assembler_state_mut(entity_id)?
            .output_inventory
            .slots[slot_index] = None;
        self.player_inventory
            .insert(&self.world.prototypes, stack.item_id, stack.count)
            .map_err(AssemblerError::from)
    }

    pub fn advance_transport_belts(&mut self) {
        let tile_to_belt = transport_belt_tile_map(&self.entities);
        let lane_keys = self
            .entities
            .transport_belts
            .keys()
            .flat_map(|entity_id| {
                [
                    BeltLaneKey {
                        entity_id: *entity_id,
                        lane_index: 0,
                    },
                    BeltLaneKey {
                        entity_id: *entity_id,
                        lane_index: 1,
                    },
                ]
            })
            .collect::<Vec<_>>();
        let mut advancement = TransportBeltAdvancement::new(&mut self.entities, tile_to_belt);

        for key in lane_keys {
            advancement.process_lane(key);
        }
    }

    fn advance_burner_mining_drills(&mut self) {
        let drill_ids = self
            .entities
            .burner_mining_drills
            .keys()
            .copied()
            .collect::<Vec<_>>();

        for entity_id in drill_ids {
            let Some(placed) = self.entities.placed_entity(entity_id).cloned() else {
                continue;
            };
            let prototype = &self.world.prototypes.entities[placed.prototype_id.index()];
            let Some(mining_drill) = prototype.mining_drill.as_ref() else {
                continue;
            };
            let target =
                first_resource_in_mining_area(&self.world, &placed.footprint, mining_drill);
            let Some((target, resource_item)) = target else {
                if let Ok(state) = self.entities.burner_drill_state_mut(entity_id) {
                    state.resource_target = None;
                    state.mining_progress_ticks = 0;
                }
                continue;
            };

            let output_target = drill_output_target(&self.entities, &placed);
            let output_can_accept =
                self.entities
                    .burner_drill_state(entity_id)
                    .is_ok_and(|state| {
                        drill_output_target_can_accept(
                            &self.world.prototypes,
                            &self.entities,
                            output_target,
                            state.output_slot,
                            resource_item,
                            1,
                        )
                    });
            if !output_can_accept {
                if let Ok(state) = self.entities.burner_drill_state_mut(entity_id) {
                    state.resource_target = Some(target);
                }
                continue;
            }

            let completed = {
                let state = self
                    .entities
                    .burner_drill_state_mut(entity_id)
                    .expect("burner drill id came from burner drill state map");
                state.resource_target = Some(target);
                let joules_per_tick =
                    state.energy.energy_usage_watts / FIXED_SIM_TICKS_PER_SECOND_F64;
                if state.energy.energy_remaining_joules + f64::EPSILON < joules_per_tick
                    && !try_consume_fuel(&self.world.prototypes, &mut state.energy)
                {
                    continue;
                }

                if state.energy.energy_remaining_joules + f64::EPSILON < joules_per_tick {
                    continue;
                }

                state.energy.energy_remaining_joules -= joules_per_tick;
                state.mining_progress_ticks += 1;

                if state.mining_progress_ticks < state.mining_required_ticks {
                    false
                } else {
                    state.mining_progress_ticks = 0;
                    true
                }
            };

            if !completed {
                continue;
            }

            let mined = self
                .world
                .mine_resource_at(target.x, target.y, 1)
                .expect("selected drill target should contain a resource");
            debug_assert_eq!(mined.resource_item, resource_item);
            debug_assert_eq!(mined.amount, 1);
            insert_drill_output(
                &mut self.entities,
                entity_id,
                output_target,
                mined.resource_item,
                mined.amount as u16,
                &self.world.prototypes,
            );
        }
    }

    fn advance_furnaces(&mut self) {
        let furnace_ids = self.entities.furnaces.keys().copied().collect::<Vec<_>>();

        for entity_id in furnace_ids {
            if self.entities.placed_entity(entity_id).is_none() {
                continue;
            }

            let Some((recipe_id, required_ticks, ingredient, product)) = self
                .entities
                .furnace_state(entity_id)
                .ok()
                .and_then(|state| furnace_work_selection(&self.world.prototypes, state.input_slot))
            else {
                if let Ok(state) = self.entities.furnace_state_mut(entity_id) {
                    state.active_recipe = None;
                    state.crafting_progress_ticks = 0;
                    state.crafting_required_ticks = 0;
                }
                continue;
            };

            let output_can_accept = self.entities.furnace_state(entity_id).is_ok_and(|state| {
                output_slot_can_accept(
                    &self.world.prototypes,
                    state.output_slot,
                    product.item,
                    product.amount,
                )
            });
            if !output_can_accept {
                if let Ok(state) = self.entities.furnace_state_mut(entity_id) {
                    state.active_recipe = Some(recipe_id);
                    state.crafting_required_ticks = required_ticks;
                }
                continue;
            }

            let completed = {
                let state = self
                    .entities
                    .furnace_state_mut(entity_id)
                    .expect("furnace id came from furnace state map");
                if state.active_recipe != Some(recipe_id) {
                    state.active_recipe = Some(recipe_id);
                    state.crafting_progress_ticks = 0;
                    state.crafting_required_ticks = required_ticks;
                }

                let joules_per_tick =
                    state.energy.energy_usage_watts / FIXED_SIM_TICKS_PER_SECOND_F64;
                if state.energy.energy_remaining_joules + f64::EPSILON < joules_per_tick
                    && !try_consume_fuel(&self.world.prototypes, &mut state.energy)
                {
                    continue;
                }

                if state.energy.energy_remaining_joules + f64::EPSILON < joules_per_tick {
                    continue;
                }

                state.energy.energy_remaining_joules -= joules_per_tick;
                state.crafting_progress_ticks += 1;

                if state.crafting_progress_ticks < required_ticks {
                    false
                } else {
                    state.crafting_progress_ticks = 0;
                    true
                }
            };

            if !completed {
                continue;
            }

            let state = self
                .entities
                .furnace_state_mut(entity_id)
                .expect("furnace id came from furnace state map");
            remove_from_single_slot(&mut state.input_slot, ingredient.item, ingredient.amount)
                .expect("selected furnace input should still contain ingredient");
            insert_output_item(&mut state.output_slot, product.item, product.amount);
        }
    }

    fn advance_assembling_machines(&mut self) {
        let assembler_ids = self
            .entities
            .assembling_machines
            .keys()
            .copied()
            .collect::<Vec<_>>();

        for entity_id in assembler_ids {
            if self.entities.placed_entity(entity_id).is_none() {
                continue;
            }

            let Some((recipe, required_ticks)) = self
                .entities
                .assembler_state(entity_id)
                .ok()
                .and_then(|state| {
                    let recipe = selected_assembler_recipe(&self.world.prototypes, state)?;
                    Some((
                        recipe,
                        assembler_required_ticks(
                            recipe.crafting_time_ticks,
                            state.crafting_speed_numerator,
                            state.crafting_speed_denominator,
                        ),
                    ))
                })
            else {
                if let Ok(state) = self.entities.assembler_state_mut(entity_id) {
                    state.crafting_progress_ticks = 0;
                    state.crafting_required_ticks = 0;
                }
                continue;
            };

            let can_craft = self.entities.assembler_state(entity_id).is_ok_and(|state| {
                assembler_has_ingredients(&state.input_inventory, &recipe.ingredients)
                    && assembler_output_can_accept(
                        &self.world.prototypes,
                        &state.output_inventory,
                        &recipe.products,
                    )
            });
            if !can_craft {
                if let Ok(state) = self.entities.assembler_state_mut(entity_id) {
                    state.crafting_required_ticks = required_ticks;
                }
                continue;
            }

            let completed = {
                let state = self
                    .entities
                    .assembler_state_mut(entity_id)
                    .expect("assembler id came from assembler state map");
                state.crafting_required_ticks = required_ticks;
                state.crafting_progress_ticks += 1;

                if state.crafting_progress_ticks < required_ticks {
                    false
                } else {
                    state.crafting_progress_ticks = 0;
                    true
                }
            };

            if !completed {
                continue;
            }

            let state = self
                .entities
                .assembler_state_mut(entity_id)
                .expect("assembler id came from assembler state map");
            for ingredient in &recipe.ingredients {
                state
                    .input_inventory
                    .remove(ingredient.item, ingredient.amount)
                    .expect("assembler checked ingredients before completion");
            }
            for product in &recipe.products {
                state
                    .output_inventory
                    .insert(&self.world.prototypes, product.item, product.amount)
                    .expect("assembler checked output capacity before completion");
            }
        }
    }

    fn advance_inserters(&mut self) {
        let inserter_ids = self.entities.inserters.keys().copied().collect::<Vec<_>>();

        for entity_id in inserter_ids {
            let Some(placed) = self.entities.placed_entity(entity_id).cloned() else {
                continue;
            };
            let Ok(state) = self.entities.inserter_state(entity_id).cloned() else {
                continue;
            };
            let (pickup_tile, drop_tile) = inserter_transfer_tiles(&placed);

            let next_state = match state {
                InserterState::WaitingForItem => {
                    let Some(item_id) = peek_inserter_source_item(&self.entities, pickup_tile)
                    else {
                        continue;
                    };
                    let item = ItemStack { item_id, count: 1 };
                    if !inserter_target_can_accept(
                        &self.world.prototypes,
                        &self.entities,
                        drop_tile,
                        item,
                    ) {
                        continue;
                    }

                    InserterState::Picking {
                        ticks_left: BASIC_INSERTER_PICKUP_TICKS,
                    }
                }
                InserterState::Picking { ticks_left } => {
                    if ticks_left > 1 {
                        InserterState::Picking {
                            ticks_left: ticks_left - 1,
                        }
                    } else if let Some(item_id) =
                        peek_inserter_source_item(&self.entities, pickup_tile)
                    {
                        let item = ItemStack { item_id, count: 1 };
                        if !inserter_target_can_accept(
                            &self.world.prototypes,
                            &self.entities,
                            drop_tile,
                            item,
                        ) {
                            InserterState::WaitingForItem
                        } else {
                            try_take_inserter_source_item(&mut self.entities, pickup_tile, item_id)
                                .map_or(InserterState::WaitingForItem, |item| {
                                    InserterState::Holding { item }
                                })
                        }
                    } else {
                        InserterState::WaitingForItem
                    }
                }
                InserterState::Holding { item } => {
                    if !inserter_target_can_accept(
                        &self.world.prototypes,
                        &self.entities,
                        drop_tile,
                        item,
                    ) {
                        InserterState::Holding { item }
                    } else if try_drop_inserter_item(
                        &self.world.prototypes,
                        &mut self.entities,
                        drop_tile,
                        item,
                    ) {
                        InserterState::Dropping {
                            ticks_left: BASIC_INSERTER_DROP_TICKS,
                        }
                    } else {
                        InserterState::Holding { item }
                    }
                }
                InserterState::Dropping { ticks_left } => {
                    if ticks_left > 1 {
                        InserterState::Dropping {
                            ticks_left: ticks_left - 1,
                        }
                    } else {
                        InserterState::WaitingForItem
                    }
                }
            };

            if let Ok(state) = self.entities.inserter_state_mut(entity_id) {
                *state = next_state;
            }
        }
    }
}

impl PlayerState {
    pub fn position_tiles(self) -> (f32, f32) {
        (
            self.x as f32 / PLAYER_POSITION_SCALE as f32,
            self.y as f32 / PLAYER_POSITION_SCALE as f32,
        )
    }

    pub fn tile_position(self) -> (i32, i32) {
        (fixed_to_tile(self.x), fixed_to_tile(self.y))
    }

    pub fn x_fixed(self) -> i64 {
        self.x
    }

    pub fn y_fixed(self) -> i64 {
        self.y
    }

    fn centered_on_tile(x: i32, y: i32) -> Self {
        Self {
            x: tile_center_fixed(x),
            y: tile_center_fixed(y),
        }
    }
}

impl WorldSim {
    pub fn new(seed: u64, prototypes: PrototypeCatalog) -> Self {
        let chunks = generate_test_chunks(seed, &prototypes);
        Self {
            seed,
            prototypes,
            chunks,
        }
    }

    pub fn new_seeded(seed: u64) -> Self {
        Self::new(
            seed,
            PrototypeCatalog::load_base().expect("base prototype catalog should load"),
        )
    }

    pub fn tile_at(&self, x: i32, y: i32) -> Option<&TileCell> {
        let (coord, index) = chunk_coord_and_tile_index(x, y);

        self.chunks
            .get(&coord)
            .and_then(|chunk| chunk.tiles.get(index))
    }

    pub fn resource_hash(&self) -> u64 {
        let mut hasher = StableHasher::default();

        for chunk in self.chunks.values() {
            for (index, tile) in chunk.tiles.iter().enumerate() {
                let Some(resource) = tile.resource else {
                    continue;
                };

                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                let x = chunk.coord.x * CHUNK_SIZE + local_x;
                let y = chunk.coord.y * CHUNK_SIZE + local_y;

                x.hash(&mut hasher);
                y.hash(&mut hasher);
                resource.resource_item.hash(&mut hasher);
                resource.amount.hash(&mut hasher);
            }
        }

        hasher.finish()
    }

    pub fn mine_resource_at(&mut self, x: i32, y: i32, amount: u32) -> Option<MinedResource> {
        if amount == 0 {
            return None;
        }

        let ids = WorldPrototypeIds::from_catalog(&self.prototypes);
        let tile = self.tile_at_mut(x, y)?;
        let resource = tile.resource.as_mut()?;
        let mined_amount = amount.min(resource.amount);
        let mined = MinedResource {
            resource_item: resource.resource_item,
            amount: mined_amount,
        };

        resource.amount -= mined_amount;
        if resource.amount == 0 {
            tile.resource = None;
            tile.collision = collision_for_tile(tile.tile_id, ids);
        }

        Some(mined)
    }

    pub fn can_build_on_tile(&self, x: i32, y: i32) -> Result<(), BuildError> {
        let tile = self
            .tile_at(x, y)
            .ok_or(BuildError::OutsideGeneratedChunks { x, y })?;

        if tile.collision.buildable {
            Ok(())
        } else {
            Err(BuildError::TileBlocked { x, y })
        }
    }

    pub fn entity_footprint(
        &self,
        prototype_id: EntityPrototypeId,
        x: i32,
        y: i32,
        direction: Direction,
    ) -> Result<EntityFootprint, BuildError> {
        let prototype = self
            .prototypes
            .entities
            .get(prototype_id.index())
            .filter(|prototype| prototype.id == prototype_id)
            .ok_or(BuildError::MissingPrototype(prototype_id))?;

        Ok(EntityFootprint::from_size(
            x,
            y,
            prototype.size.x,
            prototype.size.y,
            direction,
        ))
    }

    pub fn validate_entity_footprint(&self, footprint: &EntityFootprint) -> Result<(), BuildError> {
        footprint.validate()?;

        for (x, y) in footprint.tiles() {
            self.can_build_on_tile(x, y)?;
        }

        Ok(())
    }

    fn validate_entity_footprint_for_prototype(
        &self,
        prototype: &factory_data::EntityPrototype,
        footprint: &EntityFootprint,
    ) -> Result<(), BuildError> {
        if prototype.entity_kind != EntityKind::MiningDrill || prototype.mining_drill.is_none() {
            return self.validate_entity_footprint(footprint);
        }

        footprint.validate()?;
        for (x, y) in footprint.tiles() {
            let tile = self
                .tile_at(x, y)
                .ok_or(BuildError::OutsideGeneratedChunks { x, y })?;
            if !tile.collision.walkable {
                return Err(BuildError::TileBlocked { x, y });
            }
        }

        let mining_drill = prototype
            .mining_drill
            .as_ref()
            .expect("mining drill prototype should have mining metadata");
        if first_resource_in_mining_area(self, footprint, mining_drill).is_none() {
            return Err(BuildError::TileBlocked {
                x: footprint.x,
                y: footprint.y,
            });
        }

        Ok(())
    }

    fn tile_at_mut(&mut self, x: i32, y: i32) -> Option<&mut TileCell> {
        let (coord, index) = chunk_coord_and_tile_index(x, y);

        self.chunks
            .get_mut(&coord)
            .and_then(|chunk| chunk.tiles.get_mut(index))
    }
}

fn chunk_coord_and_tile_index(x: i32, y: i32) -> (ChunkCoord, usize) {
    let coord = ChunkCoord {
        x: x.div_euclid(CHUNK_SIZE),
        y: y.div_euclid(CHUNK_SIZE),
    };
    let local_x = x.rem_euclid(CHUNK_SIZE) as usize;
    let local_y = y.rem_euclid(CHUNK_SIZE) as usize;
    let index = local_y * CHUNK_SIZE as usize + local_x;

    (coord, index)
}

fn find_player_start(world: &WorldSim, occupancy: &OccupancyGrid) -> Option<PlayerState> {
    let max_radius = (TEST_WORLD_MAX_CHUNK - TEST_WORLD_MIN_CHUNK + 1) * CHUNK_SIZE;

    for radius in 0..=max_radius {
        for y in -radius..=radius {
            for x in -radius..=radius {
                if x.abs().max(y.abs()) != radius {
                    continue;
                }

                if player_can_occupy_tile(world, occupancy, x, y) {
                    return Some(PlayerState::centered_on_tile(x, y));
                }
            }
        }
    }

    None
}

fn player_can_occupy_tile(world: &WorldSim, occupancy: &OccupancyGrid, x: i32, y: i32) -> bool {
    let Some(tile) = world.tile_at(x, y) else {
        return false;
    };

    tile.collision.walkable && occupancy.entity_at(x, y).is_none()
}

fn tiles_to_fixed(tiles: f32) -> i64 {
    (tiles * PLAYER_POSITION_SCALE as f32).round() as i64
}

fn fixed_to_tile(value: i64) -> i32 {
    value.div_euclid(PLAYER_POSITION_SCALE) as i32
}

fn tile_center_fixed(tile: i32) -> i64 {
    i64::from(tile) * PLAYER_POSITION_SCALE + PLAYER_POSITION_SCALE / 2
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BeltLaneKey {
    entity_id: u64,
    lane_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BeltLaneVisitState {
    Processing,
    Done,
}

struct TransportBeltAdvancement<'a> {
    entities: &'a mut EntityStore,
    tile_to_belt: BTreeMap<(i32, i32), u64>,
    visit_states: BTreeMap<BeltLaneKey, BeltLaneVisitState>,
}

impl<'a> TransportBeltAdvancement<'a> {
    fn new(entities: &'a mut EntityStore, tile_to_belt: BTreeMap<(i32, i32), u64>) -> Self {
        Self {
            entities,
            tile_to_belt,
            visit_states: BTreeMap::new(),
        }
    }

    fn process_lane(&mut self, key: BeltLaneKey) {
        match self.visit_states.get(&key).copied() {
            Some(BeltLaneVisitState::Done | BeltLaneVisitState::Processing) => return,
            None => {}
        }

        if !self.entities.transport_belts.contains_key(&key.entity_id) {
            return;
        }

        self.visit_states
            .insert(key, BeltLaneVisitState::Processing);

        let downstream = self.downstream_lane_key(key);
        if let Some(downstream) = downstream
            && self.visit_states.get(&downstream) != Some(&BeltLaneVisitState::Processing)
        {
            self.process_lane(downstream);
        }

        self.advance_lane_items(key, downstream);
        self.visit_states.insert(key, BeltLaneVisitState::Done);
    }

    fn downstream_lane_key(&self, key: BeltLaneKey) -> Option<BeltLaneKey> {
        let placed = self.entities.placed_entities.get(&key.entity_id)?;
        let segment = self.entities.transport_belts.get(&key.entity_id)?;
        let (dx, dy) = direction_tile_delta(segment.dir);
        let next_entity_id = self.tile_to_belt.get(&(placed.x + dx, placed.y + dy))?;

        Some(BeltLaneKey {
            entity_id: *next_entity_id,
            lane_index: key.lane_index,
        })
    }

    fn advance_lane_items(&mut self, key: BeltLaneKey, downstream: Option<BeltLaneKey>) {
        let mut items = {
            let segment = self
                .entities
                .transport_belts
                .get_mut(&key.entity_id)
                .expect("lane processing validated belt existence");
            std::mem::take(&mut segment.lanes[key.lane_index].items)
        };
        let mut advanced_descending = Vec::with_capacity(items.len());
        let mut downstream_item_position: Option<u16> = None;

        while let Some(mut item) = items.pop() {
            let mut next_position = item.position_subtile + BASIC_BELT_SPEED_SUBTILES_PER_TICK;
            if let Some(ahead_position) = downstream_item_position {
                next_position =
                    next_position.min(ahead_position.saturating_sub(BELT_ITEM_SPACING_SUBTILES));
            }

            if next_position >= BELT_SUBTILES_PER_TILE {
                let carried_position = next_position - BELT_SUBTILES_PER_TILE;
                if let Some(downstream) = downstream
                    && self.try_insert_carried_item(downstream, item.item_id, carried_position)
                {
                    continue;
                }

                item.position_subtile = BELT_SUBTILES_PER_TILE - 1;
            } else {
                item.position_subtile = next_position;
            }

            downstream_item_position = Some(item.position_subtile);
            advanced_descending.push(item);
        }

        let segment = self
            .entities
            .transport_belts
            .get_mut(&key.entity_id)
            .expect("lane processing validated belt existence");
        let lane = &mut segment.lanes[key.lane_index];
        lane.items = advanced_descending.into_iter().rev().collect();
    }

    fn try_insert_carried_item(
        &mut self,
        key: BeltLaneKey,
        item_id: ItemId,
        position_subtile: u16,
    ) -> bool {
        if self.visit_states.get(&key) == Some(&BeltLaneVisitState::Processing) {
            return false;
        }

        let Some(segment) = self.entities.transport_belts.get_mut(&key.entity_id) else {
            return false;
        };
        let lane = &mut segment.lanes[key.lane_index];
        if !belt_lane_can_accept_position(lane, position_subtile) {
            return false;
        }

        lane.items.insert(
            0,
            BeltItem {
                item_id,
                position_subtile,
            },
        );
        true
    }
}

fn transport_belt_tile_map(entities: &EntityStore) -> BTreeMap<(i32, i32), u64> {
    entities
        .transport_belts
        .keys()
        .filter_map(|entity_id| {
            entities
                .placed_entities
                .get(entity_id)
                .map(|placed| ((placed.x, placed.y), *entity_id))
        })
        .collect()
}

fn belt_lane_can_accept_position(lane: &BeltLane, position_subtile: u16) -> bool {
    lane.items
        .first()
        .is_none_or(|first| first.position_subtile >= position_subtile + BELT_ITEM_SPACING_SUBTILES)
}

fn direction_tile_delta(direction: Direction) -> (i32, i32) {
    match direction {
        Direction::North => (0, 1),
        Direction::East => (1, 0),
        Direction::South => (0, -1),
        Direction::West => (-1, 0),
    }
}

impl EntityStore {
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn placed_len(&self) -> usize {
        self.placed_entities.len()
    }

    pub fn occupancy(&self) -> &OccupancyGrid {
        &self.occupancy
    }

    pub fn placed_entity(&self, entity_id: u64) -> Option<&PlacedEntity> {
        self.placed_entities.get(&entity_id)
    }

    pub fn placed_entities(&self) -> impl Iterator<Item = &PlacedEntity> {
        self.placed_entities.values()
    }

    fn new_test_entities(seed: u64) -> Self {
        Self {
            entities: vec![SimEntity {
                id: 1,
                x: (seed % 97) as i64,
                y: (seed % 53) as i64,
            }],
            placed_entities: BTreeMap::new(),
            entity_inventories: BTreeMap::new(),
            burner_mining_drills: BTreeMap::new(),
            furnaces: BTreeMap::new(),
            assembling_machines: BTreeMap::new(),
            transport_belts: BTreeMap::new(),
            inserters: BTreeMap::new(),
            occupancy: OccupancyGrid::default(),
            next_entity_id: 2,
        }
    }

    fn entity_inventory(&self, entity_id: u64) -> Result<&Inventory, ContainerError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(ContainerError::MissingEntity(entity_id));
        }

        self.entity_inventories
            .get(&entity_id)
            .ok_or(ContainerError::NotContainer(entity_id))
    }

    fn entity_inventory_mut(&mut self, entity_id: u64) -> Result<&mut Inventory, ContainerError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(ContainerError::MissingEntity(entity_id));
        }

        self.entity_inventories
            .get_mut(&entity_id)
            .ok_or(ContainerError::NotContainer(entity_id))
    }

    fn burner_drill_state(
        &self,
        entity_id: u64,
    ) -> Result<&BurnerMiningDrillState, BurnerDrillError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(BurnerDrillError::MissingEntity(entity_id));
        }

        self.burner_mining_drills
            .get(&entity_id)
            .ok_or(BurnerDrillError::NotBurnerDrill(entity_id))
    }

    fn burner_drill_state_mut(
        &mut self,
        entity_id: u64,
    ) -> Result<&mut BurnerMiningDrillState, BurnerDrillError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(BurnerDrillError::MissingEntity(entity_id));
        }

        self.burner_mining_drills
            .get_mut(&entity_id)
            .ok_or(BurnerDrillError::NotBurnerDrill(entity_id))
    }

    fn furnace_state(&self, entity_id: u64) -> Result<&FurnaceState, FurnaceError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(FurnaceError::MissingEntity(entity_id));
        }

        self.furnaces
            .get(&entity_id)
            .ok_or(FurnaceError::NotFurnace(entity_id))
    }

    fn furnace_state_mut(&mut self, entity_id: u64) -> Result<&mut FurnaceState, FurnaceError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(FurnaceError::MissingEntity(entity_id));
        }

        self.furnaces
            .get_mut(&entity_id)
            .ok_or(FurnaceError::NotFurnace(entity_id))
    }

    fn assembler_state(&self, entity_id: u64) -> Result<&AssemblingMachineState, AssemblerError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(AssemblerError::MissingEntity(entity_id));
        }

        self.assembling_machines
            .get(&entity_id)
            .ok_or(AssemblerError::NotAssembler(entity_id))
    }

    fn assembler_state_mut(
        &mut self,
        entity_id: u64,
    ) -> Result<&mut AssemblingMachineState, AssemblerError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(AssemblerError::MissingEntity(entity_id));
        }

        self.assembling_machines
            .get_mut(&entity_id)
            .ok_or(AssemblerError::NotAssembler(entity_id))
    }

    fn belt_segment(&self, entity_id: u64) -> Result<&BeltSegment, BeltError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(BeltError::MissingEntity(entity_id));
        }

        self.transport_belts
            .get(&entity_id)
            .ok_or(BeltError::NotTransportBelt(entity_id))
    }

    fn belt_segment_mut(&mut self, entity_id: u64) -> Result<&mut BeltSegment, BeltError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(BeltError::MissingEntity(entity_id));
        }

        self.transport_belts
            .get_mut(&entity_id)
            .ok_or(BeltError::NotTransportBelt(entity_id))
    }

    fn inserter_state(&self, entity_id: u64) -> Result<&InserterState, InserterError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(InserterError::MissingEntity(entity_id));
        }

        self.inserters
            .get(&entity_id)
            .ok_or(InserterError::NotInserter(entity_id))
    }

    fn inserter_state_mut(&mut self, entity_id: u64) -> Result<&mut InserterState, InserterError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(InserterError::MissingEntity(entity_id));
        }

        self.inserters
            .get_mut(&entity_id)
            .ok_or(InserterError::NotInserter(entity_id))
    }

    fn insert_item_onto_belt(
        &mut self,
        entity_id: u64,
        lane_index: usize,
        item_id: ItemId,
    ) -> Result<(), BeltError> {
        let segment = self.belt_segment_mut(entity_id)?;
        let lane = segment
            .lanes
            .get_mut(lane_index)
            .ok_or(BeltError::InvalidLane { lane_index })?;
        if !belt_lane_can_accept_position(lane, 0) {
            return Err(BeltError::Blocked);
        }

        lane.items.insert(
            0,
            BeltItem {
                item_id,
                position_subtile: 0,
            },
        );
        Ok(())
    }

    fn reserve_entity(&mut self, reservation: EntityReservation) -> u64 {
        let id = self.next_entity_id;
        self.next_entity_id += 1;
        self.occupancy
            .reserve_footprint(id, &reservation.footprint)
            .expect("validated footprint reservation should succeed");
        self.placed_entities.insert(
            id,
            PlacedEntity {
                id,
                prototype_id: reservation.prototype_id,
                x: reservation.x,
                y: reservation.y,
                direction: reservation.direction,
                footprint: reservation.footprint,
            },
        );
        if let Some(slot_count) = reservation.inventory_slot_count {
            self.entity_inventories
                .insert(id, Inventory::with_slot_count(slot_count));
        }
        if let Some(state) = reservation.burner_mining_drill {
            self.burner_mining_drills.insert(id, state);
        }
        if let Some(state) = reservation.furnace {
            self.furnaces.insert(id, state);
        }
        if let Some(state) = reservation.assembling_machine {
            self.assembling_machines.insert(id, state);
        }
        if let Some(segment) = reservation.transport_belt {
            self.transport_belts.insert(id, segment);
        }
        if let Some(state) = reservation.inserter {
            self.inserters.insert(id, state);
        }
        id
    }

    fn update_entity_footprint(
        &mut self,
        entity_id: u64,
        direction: Direction,
        footprint: EntityFootprint,
    ) -> Result<(), BuildError> {
        let entity = self
            .placed_entities
            .get_mut(&entity_id)
            .ok_or(BuildError::MissingEntity(entity_id))?;
        self.occupancy
            .release_footprint(entity_id, &entity.footprint);
        self.occupancy
            .reserve_footprint(entity_id, &footprint)
            .expect("validated footprint reservation should succeed");
        entity.direction = direction;
        entity.footprint = footprint;
        if let Some(segment) = self.transport_belts.get_mut(&entity_id) {
            segment.dir = direction;
        }

        Ok(())
    }

    fn remove_placed_entity(&mut self, entity_id: u64) -> Option<PlacedEntity> {
        let entity = self.placed_entities.remove(&entity_id)?;
        self.entity_inventories.remove(&entity_id);
        self.burner_mining_drills.remove(&entity_id);
        self.furnaces.remove(&entity_id);
        self.assembling_machines.remove(&entity_id);
        self.transport_belts.remove(&entity_id);
        self.inserters.remove(&entity_id);
        self.occupancy
            .release_footprint(entity_id, &entity.footprint);
        Some(entity)
    }

    fn advance(&mut self, tick: Tick, seed: u64) {
        for entity in &mut self.entities {
            let step = splitmix64(seed ^ entity.id ^ tick.0);
            entity.x += ((step & 0b11) as i64) - 1;
            entity.y += (((step >> 2) & 0b11) as i64) - 1;
        }
    }
}

impl EntityFootprint {
    pub fn from_size(x: i32, y: i32, width: i32, height: i32, direction: Direction) -> Self {
        let (width, height) = match direction {
            Direction::North | Direction::South => (width, height),
            Direction::East | Direction::West => (height, width),
        };

        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn validate(&self) -> Result<(), BuildError> {
        if self.width <= 0 || self.height <= 0 {
            Err(BuildError::InvalidFootprint {
                width: self.width,
                height: self.height,
            })
        } else {
            Ok(())
        }
    }

    pub fn tiles(&self) -> Vec<(i32, i32)> {
        let mut tiles = Vec::with_capacity((self.width * self.height) as usize);

        for y in self.y..self.y + self.height {
            for x in self.x..self.x + self.width {
                tiles.push((x, y));
            }
        }

        tiles
    }
}

impl OccupancyGrid {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.occupied_tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.occupied_tiles.is_empty()
    }

    pub fn entity_at(&self, x: i32, y: i32) -> Option<u64> {
        self.occupied_tiles.get(&(x, y)).copied()
    }

    pub fn validate_available(
        &self,
        footprint: &EntityFootprint,
        ignored_entity_id: Option<u64>,
    ) -> Result<(), BuildError> {
        footprint.validate()?;

        for (x, y) in footprint.tiles() {
            if let Some(entity_id) = self.entity_at(x, y)
                && Some(entity_id) != ignored_entity_id
            {
                return Err(BuildError::EntityOccupied { x, y, entity_id });
            }
        }

        Ok(())
    }

    pub fn reserve_footprint(
        &mut self,
        entity_id: u64,
        footprint: &EntityFootprint,
    ) -> Result<(), BuildError> {
        self.validate_available(footprint, None)?;

        for tile in footprint.tiles() {
            self.occupied_tiles.insert(tile, entity_id);
        }

        Ok(())
    }

    pub fn release_footprint(&mut self, entity_id: u64, footprint: &EntityFootprint) {
        for tile in footprint.tiles() {
            if self.entity_at(tile.0, tile.1) == Some(entity_id) {
                self.occupied_tiles.remove(&tile);
            }
        }
    }
}

#[derive(Default)]
struct StableHasher {
    hash: u64,
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, bytes: &[u8]) {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        if self.hash == 0 {
            self.hash = FNV_OFFSET;
        }

        for byte in bytes {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(FNV_PRIME);
        }
    }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

fn generate_test_chunks(seed: u64, prototypes: &PrototypeCatalog) -> BTreeMap<ChunkCoord, Chunk> {
    let ids = WorldPrototypeIds::from_catalog(prototypes);
    let resource_map = generate_resource_map(
        seed,
        ids,
        TEST_WORLD_MIN_CHUNK * CHUNK_SIZE,
        TEST_WORLD_MAX_CHUNK * CHUNK_SIZE + CHUNK_SIZE - 1,
    );
    let mut chunks = BTreeMap::new();

    for chunk_y in TEST_WORLD_MIN_CHUNK..=TEST_WORLD_MAX_CHUNK {
        for chunk_x in TEST_WORLD_MIN_CHUNK..=TEST_WORLD_MAX_CHUNK {
            let coord = ChunkCoord {
                x: chunk_x,
                y: chunk_y,
            };
            chunks.insert(coord, generate_chunk(seed, coord, ids, &resource_map));
        }
    }

    chunks
}

fn generate_chunk(
    seed: u64,
    coord: ChunkCoord,
    ids: WorldPrototypeIds,
    resource_map: &BTreeMap<(i32, i32), ResourceCell>,
) -> Chunk {
    let mut tiles = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize);

    for local_y in 0..CHUNK_SIZE {
        for local_x in 0..CHUNK_SIZE {
            let x = coord.x * CHUNK_SIZE + local_x;
            let y = coord.y * CHUNK_SIZE + local_y;
            tiles.push(generate_tile(seed, x, y, ids, resource_map));
        }
    }

    Chunk { coord, tiles }
}

fn generate_tile(
    seed: u64,
    x: i32,
    y: i32,
    ids: WorldPrototypeIds,
    resource_map: &BTreeMap<(i32, i32), ResourceCell>,
) -> TileCell {
    let (tile_id, mut collision) = generate_terrain(seed, x, y, ids);
    let resource = resource_map.get(&(x, y)).copied();

    if resource.is_some() {
        collision = TileCollision {
            walkable: true,
            buildable: false,
            minable: true,
        };
    }

    TileCell {
        tile_id,
        collision,
        resource,
    }
}

fn generate_terrain(seed: u64, x: i32, y: i32, ids: WorldPrototypeIds) -> (TileId, TileCollision) {
    let terrain_hash = hash_world(seed, x, y);
    let terrain_roll = terrain_hash % 100;
    if terrain_roll < 10 {
        (
            ids.water,
            TileCollision {
                walkable: false,
                buildable: false,
                minable: false,
            },
        )
    } else if terrain_roll < 35 {
        (ids.dirt, ground_collision())
    } else {
        (ids.grass, ground_collision())
    }
}

fn ground_collision() -> TileCollision {
    TileCollision {
        walkable: true,
        buildable: true,
        minable: false,
    }
}

fn collision_for_tile(tile_id: TileId, ids: WorldPrototypeIds) -> TileCollision {
    if tile_id == ids.water {
        TileCollision {
            walkable: false,
            buildable: false,
            minable: false,
        }
    } else {
        ground_collision()
    }
}

fn generate_resource_map(
    seed: u64,
    ids: WorldPrototypeIds,
    min_tile: i32,
    max_tile: i32,
) -> BTreeMap<(i32, i32), ResourceCell> {
    let centers = generate_resource_patch_centers(seed, ids, min_tile, max_tile);
    let mut resources = BTreeMap::new();

    for y in min_tile..=max_tile {
        for x in min_tile..=max_tile {
            let (tile_id, _) = generate_terrain(seed, x, y, ids);
            if tile_id == ids.water {
                continue;
            }

            if let Some(resource) = resource_at_patch_tile(seed, x, y, &centers) {
                resources.insert((x, y), resource);
            }
        }
    }

    resources
}

fn generate_resource_patch_centers(
    seed: u64,
    ids: WorldPrototypeIds,
    min_tile: i32,
    max_tile: i32,
) -> Vec<ResourcePatchCenter> {
    let configs = resource_patch_configs(ids);
    let starting_offsets = [(-22, -14), (18, -12), (-16, 20), (20, 18)];
    let mut centers = Vec::new();

    for (index, config) in configs.iter().enumerate() {
        let (x, y) = starting_offsets[index];
        centers.push(ResourcePatchCenter {
            resource_item: config.resource_item,
            x,
            y,
            radius: config.radius,
            richness: config.richness,
        });
    }

    let min_grid = min_tile.div_euclid(RESOURCE_PATCH_GRID_SIZE) - 1;
    let max_grid = max_tile.div_euclid(RESOURCE_PATCH_GRID_SIZE) + 1;

    for grid_y in min_grid..=max_grid {
        for grid_x in min_grid..=max_grid {
            for config in configs {
                let hash = hash_resource_center(seed, grid_x, grid_y, config.resource_item);
                if hash % 100 >= u64::from(config.frequency_percent) {
                    continue;
                }

                let jitter_x = ((hash >> 8) % (RESOURCE_PATCH_GRID_JITTER * 2 + 1) as u64) as i32
                    - RESOURCE_PATCH_GRID_JITTER;
                let jitter_y = ((hash >> 16) % (RESOURCE_PATCH_GRID_JITTER * 2 + 1) as u64) as i32
                    - RESOURCE_PATCH_GRID_JITTER;

                centers.push(ResourcePatchCenter {
                    resource_item: config.resource_item,
                    x: grid_x * RESOURCE_PATCH_GRID_SIZE + RESOURCE_PATCH_GRID_SIZE / 2 + jitter_x,
                    y: grid_y * RESOURCE_PATCH_GRID_SIZE + RESOURCE_PATCH_GRID_SIZE / 2 + jitter_y,
                    radius: config.radius,
                    richness: config.richness,
                });
            }
        }
    }

    centers
}

fn resource_at_patch_tile(
    seed: u64,
    x: i32,
    y: i32,
    centers: &[ResourcePatchCenter],
) -> Option<ResourceCell> {
    let mut best: Option<ResourceCandidate> = None;

    for center in centers {
        let dx = x - center.x;
        let dy = y - center.y;
        let distance_sq = dx * dx + dy * dy;
        let radius = center.radius + resource_edge_noise(seed, x, y, center.resource_item);
        let radius_sq = radius * radius;

        if distance_sq > radius_sq {
            continue;
        }

        let score = radius_sq - distance_sq;
        if best.is_none_or(|candidate| score > candidate.score) {
            best = Some(ResourceCandidate {
                center: *center,
                distance_sq,
                radius_sq,
                score,
            });
        }
    }

    best.map(|candidate| {
        let radius_sq = candidate.radius_sq.max(1) as u32;
        let distance_sq = candidate.distance_sq.max(0) as u32;
        let falloff = (radius_sq - distance_sq).max(1);
        let base = candidate.center.richness / 3;
        let scaled = candidate.center.richness * falloff / radius_sq;
        let variation =
            (hash_world(seed ^ 0x1d17_5f2c_6b31_f011, x, y) % u64::from(base.max(1))) as u32;

        ResourceCell {
            resource_item: candidate.center.resource_item,
            amount: base + scaled + variation,
        }
    })
}

fn resource_patch_configs(ids: WorldPrototypeIds) -> [ResourcePatchConfig; 4] {
    [
        ResourcePatchConfig {
            resource_item: ids.resources[0],
            frequency_percent: 68,
            radius: 9,
            richness: 700,
        },
        ResourcePatchConfig {
            resource_item: ids.resources[1],
            frequency_percent: 62,
            radius: 8,
            richness: 650,
        },
        ResourcePatchConfig {
            resource_item: ids.resources[2],
            frequency_percent: 55,
            radius: 10,
            richness: 800,
        },
        ResourcePatchConfig {
            resource_item: ids.resources[3],
            frequency_percent: 48,
            radius: 7,
            richness: 520,
        },
    ]
}

fn resource_edge_noise(seed: u64, x: i32, y: i32, resource_item: ItemId) -> i32 {
    let hash = hash_world(
        seed ^ 0x7b5d_1f25_8c92_f6a3 ^ u64::from(resource_item.raw()),
        x,
        y,
    );
    (hash % (RESOURCE_PATCH_EDGE_NOISE * 2 + 1) as u64) as i32 - RESOURCE_PATCH_EDGE_NOISE
}

fn hash_resource_center(seed: u64, grid_x: i32, grid_y: i32, resource_item: ItemId) -> u64 {
    hash_world(
        seed ^ 0xa24b_aed4_963e_e407 ^ u64::from(resource_item.raw()).rotate_left(17),
        grid_x,
        grid_y,
    )
}

#[derive(Clone, Copy)]
struct ResourcePatchConfig {
    resource_item: ItemId,
    frequency_percent: u8,
    radius: i32,
    richness: u32,
}

#[derive(Clone, Copy)]
struct ResourcePatchCenter {
    resource_item: ItemId,
    x: i32,
    y: i32,
    radius: i32,
    richness: u32,
}

#[derive(Clone, Copy)]
struct ResourceCandidate {
    center: ResourcePatchCenter,
    distance_sq: i32,
    radius_sq: i32,
    score: i32,
}

fn hash_world(seed: u64, x: i32, y: i32) -> u64 {
    let x_bits = x as i64 as u64;
    let y_bits = y as i64 as u64;
    splitmix64(seed ^ x_bits.rotate_left(32) ^ y_bits.rotate_left(1))
}

#[derive(Clone, Copy)]
struct WorldPrototypeIds {
    grass: TileId,
    dirt: TileId,
    water: TileId,
    resources: [ItemId; 4],
}

impl WorldPrototypeIds {
    fn from_catalog(prototypes: &PrototypeCatalog) -> Self {
        Self {
            grass: tile_id(prototypes, "grass"),
            dirt: tile_id(prototypes, "dirt"),
            water: tile_id(prototypes, "water"),
            resources: [
                item_id(prototypes, "iron_ore"),
                item_id(prototypes, "copper_ore"),
                item_id(prototypes, "coal"),
                item_id(prototypes, "stone"),
            ],
        }
    }
}

fn tile_id(prototypes: &PrototypeCatalog, name: &str) -> TileId {
    prototypes
        .tiles
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required tile prototype {name:?}"))
}

fn item_id(prototypes: &PrototypeCatalog, name: &str) -> ItemId {
    prototypes
        .items
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required item prototype {name:?}"))
}

#[cfg(test)]
fn recipe_id(prototypes: &PrototypeCatalog, name: &str) -> RecipeId {
    prototypes
        .recipes
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required recipe prototype {name:?}"))
}

fn item_stack_size(prototypes: &PrototypeCatalog, item_id: ItemId) -> Option<u16> {
    prototypes
        .items
        .get(item_id.index())
        .filter(|prototype| prototype.id == item_id)
        .map(|prototype| prototype.stack_size)
}

fn fuel_value_joules(prototypes: &PrototypeCatalog, item_id: ItemId) -> Option<u64> {
    prototypes
        .items
        .get(item_id.index())
        .filter(|prototype| prototype.id == item_id)
        .and_then(|prototype| prototype.fuel_value_joules)
}

fn burner_mining_drill_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<BurnerMiningDrillState> {
    if prototype.entity_kind != EntityKind::MiningDrill {
        return None;
    }

    let burner = prototype.burner.as_ref()?;
    let mining_drill = prototype.mining_drill.as_ref()?;

    Some(BurnerMiningDrillState {
        energy: BurnerEnergy {
            fuel_slot: None,
            energy_remaining_joules: 0.0,
            energy_usage_watts: burner.energy_usage_watts as f64,
        },
        mining_progress_ticks: 0,
        mining_required_ticks: mining_drill.ticks_per_item,
        resource_target: None,
        output_slot: None,
    })
}

fn furnace_state_for_prototype(prototype: &factory_data::EntityPrototype) -> Option<FurnaceState> {
    if prototype.entity_kind != EntityKind::Furnace {
        return None;
    }

    let burner = prototype.burner.as_ref()?;

    Some(FurnaceState {
        input_slot: None,
        energy: BurnerEnergy {
            fuel_slot: None,
            energy_remaining_joules: 0.0,
            energy_usage_watts: burner.energy_usage_watts as f64,
        },
        output_slot: None,
        active_recipe: None,
        crafting_progress_ticks: 0,
        crafting_required_ticks: 0,
    })
}

fn assembling_machine_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<AssemblingMachineState> {
    if prototype.entity_kind != EntityKind::AssemblingMachine {
        return None;
    }

    let assembling_machine = prototype.assembling_machine.as_ref()?;

    Some(AssemblingMachineState {
        selected_recipe: None,
        input_inventory: Inventory::with_slot_count(assembling_machine.input_slot_count),
        output_inventory: Inventory::with_slot_count(assembling_machine.output_slot_count),
        crafting_progress_ticks: 0,
        crafting_required_ticks: 0,
        crafting_speed_numerator: assembling_machine.crafting_speed_numerator,
        crafting_speed_denominator: assembling_machine.crafting_speed_denominator,
    })
}

fn transport_belt_segment_for_prototype(
    prototype: &factory_data::EntityPrototype,
    direction: Direction,
) -> Option<BeltSegment> {
    (prototype.entity_kind == EntityKind::TransportBelt).then(|| BeltSegment::new(direction))
}

fn inserter_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<InserterState> {
    (prototype.entity_kind == EntityKind::Inserter).then_some(InserterState::WaitingForItem)
}

fn inserter_transfer_tiles(placed: &PlacedEntity) -> ((i32, i32), (i32, i32)) {
    let (dx, dy) = direction_tile_delta(placed.direction);

    (
        (placed.x - dx, placed.y - dy),
        (placed.x + dx, placed.y + dy),
    )
}

fn peek_inserter_source_item(entities: &EntityStore, pickup_tile: (i32, i32)) -> Option<ItemId> {
    let entity_id = entities.occupancy.entity_at(pickup_tile.0, pickup_tile.1)?;

    if let Some(inventory) = entities.entity_inventories.get(&entity_id) {
        return inventory
            .slots
            .iter()
            .flatten()
            .map(|stack| stack.item_id)
            .next();
    }

    if let Some(furnace) = entities.furnaces.get(&entity_id) {
        return furnace.output_slot.map(|stack| stack.item_id);
    }

    if let Some(assembler) = entities.assembling_machines.get(&entity_id) {
        return assembler
            .output_inventory
            .slots
            .iter()
            .flatten()
            .map(|stack| stack.item_id)
            .next();
    }

    entities
        .transport_belts
        .get(&entity_id)
        .and_then(belt_pickup_item)
}

fn inserter_target_can_accept(
    catalog: &PrototypeCatalog,
    entities: &EntityStore,
    drop_tile: (i32, i32),
    item: ItemStack,
) -> bool {
    let Some(entity_id) = entities.occupancy.entity_at(drop_tile.0, drop_tile.1) else {
        return false;
    };

    if let Some(inventory) = entities.entity_inventories.get(&entity_id) {
        return inventory.can_insert(catalog, item.item_id, item.count);
    }

    if let Some(furnace) = entities.furnaces.get(&entity_id) {
        return input_slot_can_accept(catalog, furnace.input_slot, item);
    }

    if let Some(assembler) = entities.assembling_machines.get(&entity_id) {
        return assembler_input_can_accept(catalog, assembler, item)
            && assembler
                .input_inventory
                .can_insert(catalog, item.item_id, item.count);
    }

    entities
        .transport_belts
        .get(&entity_id)
        .is_some_and(|segment| {
            item.count == 1 && belt_output_lane_index(segment, item.item_id).is_some()
        })
}

fn try_take_inserter_source_item(
    entities: &mut EntityStore,
    pickup_tile: (i32, i32),
    item_id: ItemId,
) -> Option<ItemStack> {
    let entity_id = entities.occupancy.entity_at(pickup_tile.0, pickup_tile.1)?;

    if let Some(inventory) = entities.entity_inventories.get_mut(&entity_id) {
        inventory.remove(item_id, 1).ok()?;
        return Some(ItemStack { item_id, count: 1 });
    }

    if let Some(furnace) = entities.furnaces.get_mut(&entity_id) {
        remove_from_single_slot(&mut furnace.output_slot, item_id, 1).ok()?;
        return Some(ItemStack { item_id, count: 1 });
    }

    if let Some(assembler) = entities.assembling_machines.get_mut(&entity_id) {
        assembler.output_inventory.remove(item_id, 1).ok()?;
        return Some(ItemStack { item_id, count: 1 });
    }

    if let Some(segment) = entities.transport_belts.get_mut(&entity_id)
        && remove_one_item_from_belt(segment, item_id)
    {
        return Some(ItemStack { item_id, count: 1 });
    }

    None
}

fn try_drop_inserter_item(
    catalog: &PrototypeCatalog,
    entities: &mut EntityStore,
    drop_tile: (i32, i32),
    item: ItemStack,
) -> bool {
    let Some(entity_id) = entities.occupancy.entity_at(drop_tile.0, drop_tile.1) else {
        return false;
    };

    if let Some(inventory) = entities.entity_inventories.get_mut(&entity_id) {
        return inventory.insert(catalog, item.item_id, item.count).is_ok();
    }

    if let Some(furnace) = entities.furnaces.get_mut(&entity_id) {
        if !input_slot_can_accept(catalog, furnace.input_slot, item) {
            return false;
        }

        insert_into_single_slot(&mut furnace.input_slot, item);
        return true;
    }

    if let Some(assembler) = entities.assembling_machines.get_mut(&entity_id) {
        if !assembler_input_can_accept(catalog, assembler, item) {
            return false;
        }

        return assembler
            .input_inventory
            .insert(catalog, item.item_id, item.count)
            .is_ok();
    }

    if let Some(segment) = entities.transport_belts.get_mut(&entity_id) {
        if item.count != 1 {
            return false;
        }

        let Some(lane_index) = belt_output_lane_index(segment, item.item_id) else {
            return false;
        };
        segment.lanes[lane_index].items.insert(
            0,
            BeltItem {
                item_id: item.item_id,
                position_subtile: 0,
            },
        );
        return true;
    }

    false
}

fn belt_pickup_item(segment: &BeltSegment) -> Option<ItemId> {
    segment.lanes[0]
        .items
        .iter()
        .max_by_key(|item| item.position_subtile)
        .or_else(|| {
            segment.lanes[1]
                .items
                .iter()
                .max_by_key(|item| item.position_subtile)
        })
        .map(|item| item.item_id)
}

fn remove_one_item_from_belt(segment: &mut BeltSegment, item_id: ItemId) -> bool {
    for lane in &mut segment.lanes {
        let Some((item_index, _)) = lane
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.item_id == item_id)
            .max_by_key(|(_, item)| item.position_subtile)
        else {
            continue;
        };

        lane.items.remove(item_index);
        return true;
    }

    false
}

fn first_resource_in_mining_area(
    world: &WorldSim,
    footprint: &EntityFootprint,
    mining_drill: &factory_data::MiningDrillPrototype,
) -> Option<(ManualMiningTarget, ItemId)> {
    let width = mining_drill.mining_area.x.min(footprint.width).max(0);
    let height = mining_drill.mining_area.y.min(footprint.height).max(0);

    for y in footprint.y..footprint.y + height {
        for x in footprint.x..footprint.x + width {
            let Some(resource) = world.tile_at(x, y).and_then(|tile| tile.resource) else {
                continue;
            };
            return Some((ManualMiningTarget { x, y }, resource.resource_item));
        }
    }

    None
}

fn first_matching_smelting_recipe(
    catalog: &PrototypeCatalog,
    input_item: ItemId,
) -> Option<&factory_data::RecipePrototype> {
    catalog.recipes.iter().find(|recipe| {
        recipe.category == CraftingCategory::Smelting
            && recipe.ingredients.len() == 1
            && recipe.products.len() == 1
            && recipe.ingredients[0].item == input_item
    })
}

fn furnace_work_selection(
    catalog: &PrototypeCatalog,
    input_slot: Option<ItemStack>,
) -> Option<(
    RecipeId,
    u32,
    factory_data::ItemAmount,
    factory_data::ItemAmount,
)> {
    let input_stack = input_slot?;
    let recipe = first_matching_smelting_recipe(catalog, input_stack.item_id)?;
    let ingredient = recipe.ingredients[0].clone();
    if input_stack.count < ingredient.amount {
        return None;
    }
    let product = recipe.products[0].clone();

    Some((recipe.id, recipe.crafting_time_ticks, ingredient, product))
}

fn input_slot_can_accept(
    catalog: &PrototypeCatalog,
    input_slot: Option<ItemStack>,
    stack: ItemStack,
) -> bool {
    if first_matching_smelting_recipe(catalog, stack.item_id).is_none() {
        return false;
    }

    output_slot_can_accept(catalog, input_slot, stack.item_id, stack.count)
}

fn assembler_required_ticks(
    recipe_ticks: u32,
    speed_numerator: u32,
    speed_denominator: u32,
) -> u32 {
    let numerator = speed_numerator.max(1);
    let denominator = speed_denominator.max(1);
    recipe_ticks
        .saturating_mul(denominator)
        .saturating_add(numerator - 1)
        / numerator
}

fn assembler_is_empty_for_recipe_change(state: &AssemblingMachineState) -> bool {
    state.crafting_progress_ticks == 0
        && state.input_inventory.slots.iter().all(Option::is_none)
        && state.output_inventory.slots.iter().all(Option::is_none)
}

fn selected_assembler_recipe<'a>(
    catalog: &'a PrototypeCatalog,
    state: &AssemblingMachineState,
) -> Option<&'a factory_data::RecipePrototype> {
    let recipe_id = state.selected_recipe?;
    catalog
        .recipes
        .get(recipe_id.index())
        .filter(|recipe| recipe.id == recipe_id)
}

fn assembler_input_can_accept(
    catalog: &PrototypeCatalog,
    state: &AssemblingMachineState,
    stack: ItemStack,
) -> bool {
    let Some(recipe_id) = state.selected_recipe else {
        return false;
    };
    let Some(recipe) = catalog
        .recipes
        .get(recipe_id.index())
        .filter(|recipe| recipe.id == recipe_id && recipe.category == CraftingCategory::Crafting)
    else {
        return false;
    };

    recipe
        .ingredients
        .iter()
        .any(|ingredient| ingredient.item == stack.item_id)
}

fn assembler_has_ingredients(
    input_inventory: &Inventory,
    ingredients: &[factory_data::ItemAmount],
) -> bool {
    let mut required = BTreeMap::<ItemId, u32>::new();
    for ingredient in ingredients {
        *required.entry(ingredient.item).or_default() += u32::from(ingredient.amount);
    }

    required
        .into_iter()
        .all(|(item_id, count)| input_inventory.count(item_id) >= count)
}

fn assembler_output_can_accept(
    catalog: &PrototypeCatalog,
    output_inventory: &Inventory,
    products: &[factory_data::ItemAmount],
) -> bool {
    let mut output = output_inventory.clone();
    products
        .iter()
        .all(|product| output.insert(catalog, product.item, product.amount).is_ok())
}

fn stack_in_assembler_inventory_slot(
    inventory: &Inventory,
    slot_index: usize,
) -> Result<ItemStack, AssemblerError> {
    inventory
        .slots
        .get(slot_index)
        .ok_or(AssemblerError::InvalidSlot { slot_index })?
        .ok_or(AssemblerError::EmptySlot { slot_index })
}

fn burner_fuel_slot_can_accept(
    catalog: &PrototypeCatalog,
    fuel_slot: Option<ItemStack>,
    stack: ItemStack,
) -> bool {
    if fuel_value_joules(catalog, stack.item_id).is_none() {
        return false;
    }

    let Some(stack_size) = item_stack_size(catalog, stack.item_id) else {
        return false;
    };

    match fuel_slot {
        None => stack.count <= stack_size,
        Some(existing) if existing.item_id == stack.item_id => {
            u32::from(existing.count) + u32::from(stack.count) <= u32::from(stack_size)
        }
        Some(_) => false,
    }
}

fn output_slot_can_accept(
    catalog: &PrototypeCatalog,
    output_slot: Option<ItemStack>,
    item_id: ItemId,
    count: u16,
) -> bool {
    let Some(stack_size) = item_stack_size(catalog, item_id) else {
        return false;
    };

    match output_slot {
        None => count <= stack_size,
        Some(existing) if existing.item_id == item_id => {
            u32::from(existing.count) + u32::from(count) <= u32::from(stack_size)
        }
        Some(_) => false,
    }
}

fn drill_output_target(entities: &EntityStore, placed: &PlacedEntity) -> DrillOutputTarget {
    let (x, y) = drill_output_tile(placed);
    match entities.occupancy.entity_at(x, y) {
        None => DrillOutputTarget::InternalSlot,
        Some(entity_id) if entity_id == placed.id => DrillOutputTarget::InternalSlot,
        Some(entity_id) if entities.transport_belts.contains_key(&entity_id) => {
            DrillOutputTarget::Belt(entity_id)
        }
        Some(entity_id) if entities.entity_inventories.contains_key(&entity_id) => {
            DrillOutputTarget::Inventory(entity_id)
        }
        Some(_) => DrillOutputTarget::Blocked,
    }
}

fn drill_output_tile(placed: &PlacedEntity) -> (i32, i32) {
    match placed.direction {
        Direction::North => (
            placed.footprint.x + placed.footprint.width / 2,
            placed.footprint.y + placed.footprint.height,
        ),
        Direction::East => (
            placed.footprint.x + placed.footprint.width,
            placed.footprint.y + placed.footprint.height / 2,
        ),
        Direction::South => (
            placed.footprint.x + placed.footprint.width / 2,
            placed.footprint.y - 1,
        ),
        Direction::West => (
            placed.footprint.x - 1,
            placed.footprint.y + placed.footprint.height / 2,
        ),
    }
}

fn drill_output_target_can_accept(
    catalog: &PrototypeCatalog,
    entities: &EntityStore,
    output_target: DrillOutputTarget,
    internal_output_slot: Option<ItemStack>,
    item_id: ItemId,
    count: u16,
) -> bool {
    match output_target {
        DrillOutputTarget::InternalSlot => {
            output_slot_can_accept(catalog, internal_output_slot, item_id, count)
        }
        DrillOutputTarget::Inventory(entity_id) => entities
            .entity_inventories
            .get(&entity_id)
            .is_some_and(|inventory| inventory.can_insert(catalog, item_id, count)),
        DrillOutputTarget::Belt(entity_id) => entities
            .transport_belts
            .get(&entity_id)
            .is_some_and(|segment| belt_output_lane_index(segment, item_id).is_some()),
        DrillOutputTarget::Blocked => false,
    }
}

fn insert_drill_output(
    entities: &mut EntityStore,
    drill_entity_id: u64,
    output_target: DrillOutputTarget,
    item_id: ItemId,
    count: u16,
    catalog: &PrototypeCatalog,
) {
    match output_target {
        DrillOutputTarget::InternalSlot => {
            let state = entities
                .burner_drill_state_mut(drill_entity_id)
                .expect("burner drill id came from burner drill state map");
            insert_output_item(&mut state.output_slot, item_id, count);
        }
        DrillOutputTarget::Inventory(entity_id) => {
            entities
                .entity_inventories
                .get_mut(&entity_id)
                .expect("validated output inventory should still exist")
                .insert(catalog, item_id, count)
                .expect("validated output inventory should accept drill product");
        }
        DrillOutputTarget::Belt(entity_id) => {
            let segment = entities
                .transport_belts
                .get_mut(&entity_id)
                .expect("validated output belt should still exist");
            let lane_index = belt_output_lane_index(segment, item_id)
                .expect("validated belt lane should accept");
            segment.lanes[lane_index].items.insert(
                0,
                BeltItem {
                    item_id,
                    position_subtile: 0,
                },
            );
        }
        DrillOutputTarget::Blocked => {
            unreachable!("blocked drill output is checked before mining")
        }
    }
}

fn belt_output_lane_index(segment: &BeltSegment, _item_id: ItemId) -> Option<usize> {
    if belt_lane_can_accept_position(&segment.lanes[0], 0) {
        Some(0)
    } else if belt_lane_can_accept_position(&segment.lanes[1], 0) {
        Some(1)
    } else {
        None
    }
}

fn insert_into_single_slot(slot: &mut Option<ItemStack>, stack: ItemStack) {
    match slot {
        Some(existing) => existing.count += stack.count,
        None => *slot = Some(stack),
    }
}

fn insert_output_item(slot: &mut Option<ItemStack>, item_id: ItemId, count: u16) {
    insert_into_single_slot(slot, ItemStack { item_id, count });
}

fn remove_from_single_slot(
    slot: &mut Option<ItemStack>,
    item_id: ItemId,
    count: u16,
) -> Result<(), InventoryError> {
    let Some(mut stack) = *slot else {
        return Err(InventoryError::InsufficientItems);
    };
    if stack.item_id != item_id || stack.count < count {
        return Err(InventoryError::InsufficientItems);
    }

    stack.count -= count;
    *slot = (stack.count > 0).then_some(stack);
    Ok(())
}

fn try_consume_fuel(catalog: &PrototypeCatalog, energy: &mut BurnerEnergy) -> bool {
    let Some(mut fuel_stack) = energy.fuel_slot else {
        return false;
    };
    let Some(fuel_value) = fuel_value_joules(catalog, fuel_stack.item_id) else {
        return false;
    };

    fuel_stack.count -= 1;
    energy.fuel_slot = (fuel_stack.count > 0).then_some(fuel_stack);
    energy.energy_remaining_joules += fuel_value as f64;

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn world_tile_lookup_is_stable_across_chunk_boundaries() {
        let world = WorldSim::new_seeded(123);

        let left_of_origin = world.tile_at(-1, 0).expect("-1 should be in chunk -1");
        let previous_chunk_tile = world.tile_at(-33, 0).expect("-33 should be in chunk -2");
        let previous_chunk = world
            .chunks
            .get(&ChunkCoord { x: -2, y: 0 })
            .expect("previous negative chunk should exist");

        assert_eq!(
            left_of_origin,
            &world
                .chunks
                .get(&ChunkCoord { x: -1, y: 0 })
                .expect("left chunk should exist")
                .tiles[31]
        );
        assert!(world.tile_at(-32, 0).is_some());
        assert_eq!(previous_chunk_tile, &previous_chunk.tiles[31]);
    }

    #[test]
    fn generated_chunks_have_expected_shape() {
        let world = WorldSim::new_seeded(123);

        assert_eq!(world.chunks.len(), 16);
        for chunk in world.chunks.values() {
            assert_eq!(chunk.tiles.len(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
        }
    }

    #[test]
    fn resource_generation_is_deterministic() {
        let a = WorldSim::new_seeded(123);
        let b = WorldSim::new_seeded(123);

        assert_eq!(a.resource_hash(), b.resource_hash());
    }

    #[test]
    fn seed_123_contains_all_resource_item_types() {
        let world = WorldSim::new_seeded(123);
        let ids = WorldPrototypeIds::from_catalog(&world.prototypes);
        let resource_items = world
            .chunks
            .values()
            .flat_map(|chunk| chunk.tiles.iter())
            .filter_map(|tile| tile.resource.map(|resource| resource.resource_item))
            .collect::<BTreeSet<_>>();

        for resource_item in ids.resources {
            assert!(
                resource_items.contains(&resource_item),
                "missing generated resource item {resource_item:?}"
            );
        }
    }

    #[test]
    fn mining_decreases_resource_amount() {
        let mut world = WorldSim::new_seeded(123);
        let (x, y, before) = first_resource_tile(&world);

        let mined = world
            .mine_resource_at(x, y, 25)
            .expect("resource tile should be minable");
        let after = world
            .tile_at(x, y)
            .expect("mined tile should still exist")
            .resource
            .expect("resource should remain after partial mining");

        assert_eq!(mined.amount, 25);
        assert_eq!(after.amount, before.amount - 25);
        assert_eq!(after.resource_item, before.resource_item);
    }

    #[test]
    fn over_mining_clears_resource_tile() {
        let mut world = WorldSim::new_seeded(123);
        let (x, y, before) = first_resource_tile(&world);

        let mined = world
            .mine_resource_at(x, y, before.amount + 1)
            .expect("resource tile should be minable");
        let tile = world.tile_at(x, y).expect("mined tile should still exist");

        assert_eq!(mined.amount, before.amount);
        assert!(tile.resource.is_none());
        assert!(tile.collision.buildable);
        assert!(!tile.collision.minable);
    }

    #[test]
    fn resource_hash_changes_after_mining() {
        let mut world = WorldSim::new_seeded(123);
        let before_hash = world.resource_hash();
        let (x, y, _) = first_resource_tile(&world);

        world
            .mine_resource_at(x, y, 1)
            .expect("resource tile should be minable");

        assert_ne!(world.resource_hash(), before_hash);
    }

    #[test]
    fn manual_mining_one_ore_decreases_resource_by_one() {
        let mut sim = Simulation::new_test_world(123);
        let (x, y, resource) = first_resource_tile(&sim.world);
        let target = ManualMiningTarget { x, y };
        sim.player = PlayerState::centered_on_tile(x, y);
        let before_count = sim.player_inventory.count(resource.resource_item);

        for _ in 0..MANUAL_MINING_TICKS_PER_ITEM {
            sim.update_manual_mining(Some(target));
        }

        let after_resource = resource_amount_at(&sim.world, x, y).expect("resource should remain");
        assert_eq!(
            sim.player_inventory.count(resource.resource_item),
            before_count + 1
        );
        assert_eq!(after_resource, resource.amount - 1);
    }

    #[test]
    fn manual_mining_can_mine_each_generated_resource_type() {
        let mut sim = Simulation::new_test_world(123);
        let resource_names = ["iron_ore", "copper_ore", "coal", "stone"];

        for resource_name in resource_names {
            let resource_item = item_id(&sim.world.prototypes, resource_name);
            let (x, y, before_amount) = first_resource_tile_for_item(&sim.world, resource_item);
            let before_count = sim.player_inventory.count(resource_item);
            sim.player = PlayerState::centered_on_tile(x, y);

            for _ in 0..MANUAL_MINING_TICKS_PER_ITEM {
                sim.update_manual_mining(Some(ManualMiningTarget { x, y }));
            }

            assert_eq!(
                sim.player_inventory.count(resource_item),
                before_count + 1,
                "{resource_name} should be inserted into inventory"
            );
            assert_eq!(
                resource_amount_at(&sim.world, x, y),
                Some(before_amount - 1),
                "{resource_name} resource amount should decrease by one"
            );
        }
    }

    #[test]
    fn manual_mining_does_not_decrement_resource_before_full_duration() {
        let mut sim = Simulation::new_test_world(123);
        let (x, y, resource) = first_resource_tile(&sim.world);
        let target = ManualMiningTarget { x, y };
        sim.player = PlayerState::centered_on_tile(x, y);
        let before_count = sim.player_inventory.count(resource.resource_item);

        for _ in 0..MANUAL_MINING_TICKS_PER_ITEM - 1 {
            sim.update_manual_mining(Some(target));
        }

        assert_eq!(
            sim.player_inventory.count(resource.resource_item),
            before_count
        );
        assert_eq!(resource_amount_at(&sim.world, x, y), Some(resource.amount));
        assert_eq!(
            sim.manual_mining_progress
                .expect("manual mining should be in progress")
                .progress_ticks,
            MANUAL_MINING_TICKS_PER_ITEM - 1
        );
    }

    #[test]
    fn manual_mining_target_change_cancels_previous_progress() {
        let mut sim = Simulation::new_test_world(123);
        let ((first_x, first_y), (second_x, second_y)) = nearby_resource_pair(&sim.world);
        let first = ManualMiningTarget {
            x: first_x,
            y: first_y,
        };
        let second = ManualMiningTarget {
            x: second_x,
            y: second_y,
        };
        sim.player = PlayerState::centered_on_tile(first_x, first_y);

        for _ in 0..10 {
            sim.update_manual_mining(Some(first));
        }
        sim.update_manual_mining(Some(second));

        assert_eq!(
            sim.manual_mining_progress,
            Some(ManualMiningProgress {
                target: second,
                progress_ticks: 1,
                required_ticks: MANUAL_MINING_TICKS_PER_ITEM,
            })
        );
    }

    #[test]
    fn manual_mining_moving_beyond_reach_cancels_progress() {
        let mut sim = Simulation::new_test_world(123);
        let (x, y, _) = first_resource_tile(&sim.world);
        let target = ManualMiningTarget { x, y };
        sim.player = PlayerState::centered_on_tile(x, y);

        for _ in 0..10 {
            sim.update_manual_mining(Some(target));
        }
        sim.player = PlayerState::centered_on_tile(x + 3, y);
        sim.update_manual_mining(Some(target));

        assert_eq!(sim.manual_mining_progress, None);
    }

    #[test]
    fn manual_mining_full_inventory_prevents_completion_without_decrementing_resource() {
        let mut sim = Simulation::new_test_world(123);
        let (x, y, resource) = first_resource_tile(&sim.world);
        let burner_mining_drill = item_id(&sim.world.prototypes, "burner_mining_drill");
        sim.player = PlayerState::centered_on_tile(x, y);
        sim.player_inventory = Inventory::with_slot_count(1);
        sim.player_inventory
            .insert(&sim.world.prototypes, burner_mining_drill, 1)
            .expect("test inventory should accept one blocking item");

        for _ in 0..MANUAL_MINING_TICKS_PER_ITEM {
            sim.update_manual_mining(Some(ManualMiningTarget { x, y }));
        }

        assert_eq!(sim.player_inventory.count(resource.resource_item), 0);
        assert_eq!(resource_amount_at(&sim.world, x, y), Some(resource.amount));
        assert_eq!(
            sim.manual_mining_progress
                .expect("full inventory should keep completed progress")
                .progress_ticks,
            MANUAL_MINING_TICKS_PER_ITEM
        );
    }

    #[test]
    fn two_by_two_entity_cannot_overlap_another_entity() {
        let mut sim = Simulation::new_test_world(123);
        let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
        let (x, y) = first_buildable_rect(&sim.world, 4, 2);

        let first = sim
            .place_entity(furnace, x, y, Direction::North)
            .expect("first furnace should be placeable");
        let error = sim
            .place_entity(furnace, x + 1, y, Direction::North)
            .expect_err("second furnace should overlap the first");

        assert!(matches!(
            error,
            BuildError::EntityOccupied {
                entity_id,
                ..
            } if entity_id == first
        ));
    }

    #[test]
    fn entity_cannot_be_placed_on_water() {
        let mut sim = Simulation::new_test_world(123);
        let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
        let (x, y) = first_water_tile(&sim.world);

        let error = sim
            .place_entity(inserter, x, y, Direction::North)
            .expect_err("water should block entity placement");

        assert!(matches!(error, BuildError::TileBlocked { x: bx, y: by } if bx == x && by == y));
    }

    #[test]
    fn entity_cannot_be_placed_outside_generated_chunks() {
        let mut sim = Simulation::new_test_world(123);
        let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
        let outside_x = (TEST_WORLD_MAX_CHUNK + 1) * CHUNK_SIZE;

        let error = sim
            .place_entity(inserter, outside_x, 0, Direction::North)
            .expect_err("unloaded chunks should block entity placement");

        assert!(matches!(
            error,
            BuildError::OutsideGeneratedChunks { x, y: 0 } if x == outside_x
        ));
    }

    #[test]
    fn rotation_updates_entity_footprint() {
        let mut catalog =
            PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let inserter = entity_id_by_name(&catalog, "inserter");
        catalog.entities[inserter.index()].size.y = 2;

        let mut sim = Simulation::new(123, catalog);
        let (x, y) = first_buildable_rect(&sim.world, 2, 2);
        let entity_id = sim
            .place_entity(inserter, x, y, Direction::North)
            .expect("rectangular entity should be placeable");

        assert_eq!(sim.entities.occupancy().entity_at(x, y), Some(entity_id));
        assert_eq!(
            sim.entities.occupancy().entity_at(x, y + 1),
            Some(entity_id)
        );
        assert_eq!(sim.entities.occupancy().entity_at(x + 1, y), None);

        sim.rotate_entity(entity_id, Direction::East)
            .expect("rotated rectangular entity should still be placeable");

        let entity = sim
            .entities
            .placed_entity(entity_id)
            .expect("placed entity should remain");
        assert_eq!(entity.footprint.width, 2);
        assert_eq!(entity.footprint.height, 1);
        assert_eq!(sim.entities.occupancy().entity_at(x, y), Some(entity_id));
        assert_eq!(
            sim.entities.occupancy().entity_at(x + 1, y),
            Some(entity_id)
        );
        assert_eq!(sim.entities.occupancy().entity_at(x, y + 1), None);
    }

    #[test]
    fn chest_placement_creates_sixteen_inventory_slots() {
        let mut sim = Simulation::new_test_world(123);
        let chest = entity_id_by_name(&sim.world.prototypes, "chest");
        let (x, y) = first_buildable_rect(&sim.world, 1, 1);

        let entity_id = sim
            .place_entity(chest, x, y, Direction::North)
            .expect("chest should be placeable");

        assert_eq!(
            sim.entity_inventory(entity_id)
                .expect("chest should have an inventory")
                .slots
                .len(),
            16
        );
    }

    #[test]
    fn catalog_loads_assembler_metadata() {
        let sim = Simulation::new_test_world(123);
        let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
        let prototype = &sim.world.prototypes.entities[assembler.index()];
        let metadata = prototype
            .assembling_machine
            .as_ref()
            .expect("assembler prototype should load metadata");

        assert_eq!(prototype.entity_kind, EntityKind::AssemblingMachine);
        assert_eq!((prototype.size.x, prototype.size.y), (3, 3));
        assert_eq!(metadata.crafting_speed_numerator, 1);
        assert_eq!(metadata.crafting_speed_denominator, 2);
        assert_eq!(
            metadata.input_slot_count,
            ASSEMBLING_MACHINE_INPUT_SLOT_COUNT
        );
        assert_eq!(
            metadata.output_slot_count,
            ASSEMBLING_MACHINE_OUTPUT_SLOT_COUNT
        );
    }

    #[test]
    fn assembler_crafts_gears_from_iron_plates() {
        let mut sim = Simulation::new_test_world(123);
        let assembler_id = place_assembling_machine(&mut sim);
        let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

        sim.select_assembler_recipe(assembler_id, recipe)
            .expect("crafting recipe should be accepted by assembler");
        sim.player_inventory = Inventory::player();
        sim.player_inventory.slots[0] = Some(ItemStack {
            item_id: iron_plate,
            count: 2,
        });
        sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
            .expect("assembler should accept gear ingredients");

        for _ in 0..60 {
            sim.tick();
        }

        let state = sim
            .assembler_state(assembler_id)
            .expect("assembler should expose state");
        assert_eq!(state.input_inventory.count(iron_plate), 0);
        assert_eq!(state.output_inventory.count(iron_gear_wheel), 1);
        assert_eq!(state.crafting_progress_ticks, 0);
        assert_eq!(state.crafting_required_ticks, 60);
    }

    #[test]
    fn assembler_blocks_without_inputs() {
        let mut sim = Simulation::new_test_world(123);
        let assembler_id = place_assembling_machine(&mut sim);
        let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

        sim.select_assembler_recipe(assembler_id, recipe)
            .expect("crafting recipe should be accepted by assembler");
        sim.player_inventory = Inventory::player();
        sim.player_inventory.slots[0] = Some(ItemStack {
            item_id: iron_plate,
            count: 1,
        });
        sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
            .expect("assembler should accept partial ingredients");

        for _ in 0..90 {
            sim.tick();
        }

        let state = sim
            .assembler_state(assembler_id)
            .expect("assembler should expose state");
        assert_eq!(state.input_inventory.count(iron_plate), 1);
        assert_eq!(state.output_inventory.count(iron_gear_wheel), 0);
        assert_eq!(state.crafting_progress_ticks, 0);
    }

    #[test]
    fn assembler_blocks_when_output_full() {
        let mut sim = Simulation::new_test_world(123);
        let assembler_id = place_assembling_machine(&mut sim);
        let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
        let stack_size = item_stack_size(&sim.world.prototypes, iron_gear_wheel)
            .expect("gear should have stack size");

        sim.select_assembler_recipe(assembler_id, recipe)
            .expect("crafting recipe should be accepted by assembler");
        sim.player_inventory = Inventory::player();
        sim.player_inventory.slots[0] = Some(ItemStack {
            item_id: iron_plate,
            count: 2,
        });
        sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
            .expect("assembler should accept gear ingredients");
        sim.entities
            .assembler_state_mut(assembler_id)
            .expect("assembler should expose mutable state")
            .output_inventory
            .slots[0] = Some(ItemStack {
            item_id: iron_gear_wheel,
            count: stack_size,
        });

        for _ in 0..60 {
            sim.tick();
        }

        let state = sim
            .assembler_state(assembler_id)
            .expect("assembler should expose state");
        assert_eq!(state.input_inventory.count(iron_plate), 2);
        assert_eq!(
            state.output_inventory.count(iron_gear_wheel),
            u32::from(stack_size)
        );
        assert_eq!(state.crafting_progress_ticks, 0);
    }

    #[test]
    fn invalid_assembler_recipe_is_rejected() {
        let mut sim = Simulation::new_test_world(123);
        let assembler_id = place_assembling_machine(&mut sim);
        let smelting_recipe = recipe_id(&sim.world.prototypes, "iron_plate");

        assert_eq!(
            sim.select_assembler_recipe(assembler_id, smelting_recipe),
            Err(AssemblerError::InvalidRecipe(smelting_recipe))
        );
        assert_eq!(
            sim.assembler_state(assembler_id)
                .expect("assembler should expose state")
                .selected_recipe,
            None
        );
    }

    #[test]
    fn selecting_different_assembler_recipe_on_empty_assembler_succeeds() {
        let mut sim = Simulation::new_test_world(123);
        let assembler_id = place_assembling_machine(&mut sim);
        let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
        let cable_recipe = recipe_id(&sim.world.prototypes, "copper_cable");

        sim.select_assembler_recipe(assembler_id, gear_recipe)
            .expect("initial recipe should be accepted");
        sim.select_assembler_recipe(assembler_id, cable_recipe)
            .expect("empty assembler should allow recipe changes");

        let state = sim
            .assembler_state(assembler_id)
            .expect("assembler should expose state");
        assert_eq!(state.selected_recipe, Some(cable_recipe));
        assert_eq!(state.crafting_progress_ticks, 0);
        assert_eq!(state.crafting_required_ticks, 60);
    }

    #[test]
    fn selecting_same_assembler_recipe_while_non_empty_preserves_progress() {
        let mut sim = Simulation::new_test_world(123);
        let assembler_id = place_assembling_machine(&mut sim);
        let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");

        sim.select_assembler_recipe(assembler_id, gear_recipe)
            .expect("initial recipe should be accepted");
        {
            let state = sim
                .entities
                .assembler_state_mut(assembler_id)
                .expect("assembler should expose mutable state");
            state.input_inventory.slots[0] = Some(ItemStack {
                item_id: iron_plate,
                count: 1,
            });
            state.crafting_progress_ticks = 17;
        }
        let before = sim
            .assembler_state(assembler_id)
            .expect("assembler should expose state")
            .clone();

        sim.select_assembler_recipe(assembler_id, gear_recipe)
            .expect("same recipe selection should be idempotent");

        assert_eq!(
            sim.assembler_state(assembler_id)
                .expect("assembler should expose state"),
            &before
        );
    }

    #[test]
    fn selecting_different_assembler_recipe_with_input_items_fails_without_mutation() {
        let mut sim = Simulation::new_test_world(123);
        let assembler_id = place_assembling_machine(&mut sim);
        let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
        let cable_recipe = recipe_id(&sim.world.prototypes, "copper_cable");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");

        sim.select_assembler_recipe(assembler_id, gear_recipe)
            .expect("initial recipe should be accepted");
        sim.entities
            .assembler_state_mut(assembler_id)
            .expect("assembler should expose mutable state")
            .input_inventory
            .slots[0] = Some(ItemStack {
            item_id: iron_plate,
            count: 1,
        });
        let before = sim
            .assembler_state(assembler_id)
            .expect("assembler should expose state")
            .clone();

        assert_eq!(
            sim.select_assembler_recipe(assembler_id, cable_recipe),
            Err(AssemblerError::RecipeChangeRequiresEmpty {
                entity_id: assembler_id
            })
        );
        assert_eq!(
            sim.assembler_state(assembler_id)
                .expect("assembler should expose state"),
            &before
        );
    }

    #[test]
    fn selecting_different_assembler_recipe_with_output_items_fails_without_mutation() {
        let mut sim = Simulation::new_test_world(123);
        let assembler_id = place_assembling_machine(&mut sim);
        let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
        let cable_recipe = recipe_id(&sim.world.prototypes, "copper_cable");
        let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

        sim.select_assembler_recipe(assembler_id, gear_recipe)
            .expect("initial recipe should be accepted");
        sim.entities
            .assembler_state_mut(assembler_id)
            .expect("assembler should expose mutable state")
            .output_inventory
            .slots[0] = Some(ItemStack {
            item_id: iron_gear_wheel,
            count: 1,
        });
        let before = sim
            .assembler_state(assembler_id)
            .expect("assembler should expose state")
            .clone();

        assert_eq!(
            sim.select_assembler_recipe(assembler_id, cable_recipe),
            Err(AssemblerError::RecipeChangeRequiresEmpty {
                entity_id: assembler_id
            })
        );
        assert_eq!(
            sim.assembler_state(assembler_id)
                .expect("assembler should expose state"),
            &before
        );
    }

    #[test]
    fn selecting_different_assembler_recipe_with_progress_fails_without_mutation() {
        let mut sim = Simulation::new_test_world(123);
        let assembler_id = place_assembling_machine(&mut sim);
        let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
        let cable_recipe = recipe_id(&sim.world.prototypes, "copper_cable");

        sim.select_assembler_recipe(assembler_id, gear_recipe)
            .expect("initial recipe should be accepted");
        sim.entities
            .assembler_state_mut(assembler_id)
            .expect("assembler should expose mutable state")
            .crafting_progress_ticks = 1;
        let before = sim
            .assembler_state(assembler_id)
            .expect("assembler should expose state")
            .clone();

        assert_eq!(
            sim.select_assembler_recipe(assembler_id, cable_recipe),
            Err(AssemblerError::RecipeChangeRequiresEmpty {
                entity_id: assembler_id
            })
        );
        assert_eq!(
            sim.assembler_state(assembler_id)
                .expect("assembler should expose state"),
            &before
        );
    }

    #[test]
    fn assembler_ingredient_status_reports_partial_ingredients() {
        let mut sim = Simulation::new_test_world(123);
        let assembler_id = place_assembling_machine(&mut sim);
        let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");

        sim.select_assembler_recipe(assembler_id, recipe)
            .expect("crafting recipe should be accepted by assembler");
        sim.entities
            .assembler_state_mut(assembler_id)
            .expect("assembler should expose mutable state")
            .input_inventory
            .slots[0] = Some(ItemStack {
            item_id: iron_plate,
            count: 1,
        });

        assert_eq!(
            sim.assembler_ingredient_status(assembler_id)
                .expect("ingredient status should be available"),
            vec![AssemblerIngredientStatus {
                item: iron_plate,
                required: 2,
                available: 1,
                missing: 1,
            }]
        );
    }

    #[test]
    fn inserter_moves_ingredients_from_chest_to_assembler() {
        let mut sim = Simulation::new_test_world(123);
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
        let (chest_id, inserter_id, assembler_id) = place_chest_inserter_assembler_line(&mut sim);
        sim.select_assembler_recipe(assembler_id, recipe)
            .expect("crafting recipe should be accepted by assembler");
        sim.entity_inventory_mut(chest_id)
            .expect("chest should have inventory")
            .slots[0] = Some(ItemStack {
            item_id: iron_plate,
            count: 1,
        });

        run_inserter_until_idle(&mut sim, inserter_id);

        assert_eq!(
            sim.entity_inventory(chest_id)
                .expect("chest should have inventory")
                .count(iron_plate),
            0
        );
        assert_eq!(
            sim.assembler_state(assembler_id)
                .expect("assembler should expose state")
                .input_inventory
                .count(iron_plate),
            1
        );
    }

    #[test]
    fn inserter_removes_assembler_output_to_chest() {
        let mut sim = Simulation::new_test_world(123);
        let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
        let (assembler_id, inserter_id, chest_id) = place_assembler_inserter_chest_line(&mut sim);
        sim.entities
            .assembler_state_mut(assembler_id)
            .expect("assembler should expose mutable state")
            .output_inventory
            .slots[0] = Some(ItemStack {
            item_id: iron_gear_wheel,
            count: 1,
        });

        run_inserter_until_idle(&mut sim, inserter_id);

        assert_eq!(
            sim.assembler_state(assembler_id)
                .expect("assembler should expose state")
                .output_inventory
                .count(iron_gear_wheel),
            0
        );
        assert_eq!(
            sim.entity_inventory(chest_id)
                .expect("chest should have inventory")
                .count(iron_gear_wheel),
            1
        );
    }

    #[test]
    fn assembler_state_hash_remains_deterministic_for_same_seed_actions() {
        let mut first = Simulation::new_test_world(123);
        let mut second = Simulation::new_test_world(123);
        run_same_assembler_actions(&mut first);
        run_same_assembler_actions(&mut second);

        assert_eq!(first.state_hash(), second.state_hash());
    }

    #[test]
    fn chest_inventory_accepts_items() {
        let mut sim = Simulation::new_test_world(123);
        let chest = entity_id_by_name(&sim.world.prototypes, "chest");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let (x, y) = first_buildable_rect(&sim.world, 1, 1);
        let entity_id = sim
            .place_entity(chest, x, y, Direction::North)
            .expect("chest should be placeable");
        let catalog = sim.world.prototypes.clone();

        sim.entity_inventory_mut(entity_id)
            .expect("chest should expose mutable inventory")
            .insert(&catalog, iron_plate, 25)
            .expect("chest should accept iron plates");

        assert_eq!(
            sim.entity_inventory(entity_id)
                .expect("chest should have inventory")
                .count(iron_plate),
            25
        );
    }

    #[test]
    fn player_can_transfer_stack_to_chest() {
        let mut sim = Simulation::new_test_world(123);
        let chest = entity_id_by_name(&sim.world.prototypes, "chest");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let (x, y) = first_buildable_rect(&sim.world, 1, 1);
        let entity_id = sim
            .place_entity(chest, x, y, Direction::North)
            .expect("chest should be placeable");
        sim.player_inventory = Inventory::player();
        sim.player_inventory.slots[5] = Some(ItemStack {
            item_id: iron_plate,
            count: 42,
        });

        sim.transfer_player_slot_to_entity(entity_id, 5)
            .expect("stack should transfer to chest");

        assert_eq!(sim.player_inventory.slots[5], None);
        assert_eq!(
            sim.entity_inventory(entity_id)
                .expect("chest should have inventory")
                .count(iron_plate),
            42
        );
    }

    #[test]
    fn transfer_to_full_chest_fails_without_changing_player_inventory() {
        let mut sim = Simulation::new_test_world(123);
        let chest = entity_id_by_name(&sim.world.prototypes, "chest");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let coal = item_id(&sim.world.prototypes, "coal");
        let (x, y) = first_buildable_rect(&sim.world, 1, 1);
        let entity_id = sim
            .place_entity(chest, x, y, Direction::North)
            .expect("chest should be placeable");
        sim.player_inventory = Inventory::player();
        sim.player_inventory.slots[3] = Some(ItemStack {
            item_id: iron_plate,
            count: 12,
        });
        {
            let inventory = sim
                .entity_inventory_mut(entity_id)
                .expect("chest should expose inventory");
            for slot in &mut inventory.slots {
                *slot = Some(ItemStack {
                    item_id: coal,
                    count: 100,
                });
            }
        }
        assert!(
            !sim.entity_inventory(entity_id)
                .expect("chest should have inventory")
                .can_insert(&sim.world.prototypes, iron_plate, 12)
        );
        let player_before = sim.player_inventory.clone();

        assert_eq!(
            sim.transfer_player_slot_to_entity(entity_id, 3),
            Err(ContainerError::InsufficientSpace)
        );
        assert_eq!(sim.player_inventory, player_before);
    }

    #[test]
    fn transfer_from_chest_to_full_player_fails_without_changing_chest_inventory() {
        let mut sim = Simulation::new_test_world(123);
        let chest = entity_id_by_name(&sim.world.prototypes, "chest");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let coal = item_id(&sim.world.prototypes, "coal");
        let (x, y) = first_buildable_rect(&sim.world, 1, 1);
        let entity_id = sim
            .place_entity(chest, x, y, Direction::North)
            .expect("chest should be placeable");
        sim.player_inventory = Inventory::with_slot_count(1);
        sim.player_inventory
            .insert(&sim.world.prototypes, coal, 100)
            .expect("player inventory should accept blocking stack");
        let inventory = sim
            .entity_inventory_mut(entity_id)
            .expect("chest should expose inventory");
        inventory.slots[0] = Some(ItemStack {
            item_id: iron_plate,
            count: 8,
        });
        let chest_before = sim
            .entity_inventory(entity_id)
            .expect("chest should have inventory")
            .clone();

        assert_eq!(
            sim.transfer_entity_slot_to_player(entity_id, 0),
            Err(ContainerError::InsufficientSpace)
        );
        assert_eq!(
            sim.entity_inventory(entity_id)
                .expect("chest should still have inventory"),
            &chest_before
        );
    }

    #[test]
    fn burner_drill_without_fuel_remains_idle() {
        let mut sim = Simulation::new_test_world(123);
        let coal = item_id(&sim.world.prototypes, "coal");
        let (entity_id, x, y, before) = place_burner_drill_on_resource(&mut sim, coal);

        for _ in 0..240 {
            sim.tick();
        }

        let state = sim
            .burner_drill_state(entity_id)
            .expect("burner drill should expose state");
        assert_eq!(state.energy.energy_remaining_joules, 0.0);
        assert_eq!(state.mining_progress_ticks, 0);
        assert_eq!(state.output_slot, None);
        assert_eq!(resource_amount_at(&sim.world, x, y), Some(before));
    }

    #[test]
    fn burner_drill_with_coal_mines_output() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let coal = item_id(&sim.world.prototypes, "coal");
        let (entity_id, x, y, before) = place_burner_drill_on_resource(&mut sim, iron_ore);
        sim.player_inventory = Inventory::player();
        sim.player_inventory.slots[0] = Some(ItemStack {
            item_id: coal,
            count: 1,
        });
        sim.transfer_player_slot_to_burner_drill_fuel(entity_id, 0)
            .expect("coal should transfer to drill fuel");

        for _ in 0..240 {
            sim.tick();
        }

        let state = sim
            .burner_drill_state(entity_id)
            .expect("burner drill should expose state");
        assert_eq!(
            state.output_slot,
            Some(ItemStack {
                item_id: iron_ore,
                count: 1,
            })
        );
        assert_eq!(state.mining_progress_ticks, 0);
        assert_eq!(state.energy.energy_remaining_joules, 3_400_000.0);
        assert_eq!(resource_amount_at(&sim.world, x, y), Some(before - 1));
    }

    #[test]
    fn one_coal_powers_burner_drill_for_exactly_1600_ticks() {
        let mut sim = Simulation::new_test_world(123);
        let coal = item_id(&sim.world.prototypes, "coal");
        let (entity_id, _, _, _) = place_burner_drill_on_resource(&mut sim, coal);
        sim.player_inventory = Inventory::player();
        sim.player_inventory.slots[0] = Some(ItemStack {
            item_id: coal,
            count: 1,
        });
        sim.transfer_player_slot_to_burner_drill_fuel(entity_id, 0)
            .expect("coal should transfer to drill fuel");

        for _ in 0..1600 {
            sim.tick();
        }

        let state = sim
            .burner_drill_state(entity_id)
            .expect("burner drill should expose state");
        assert_eq!(state.energy.fuel_slot, None);
        assert_eq!(state.energy.energy_remaining_joules, 0.0);
        assert_eq!(state.output_slot.map(|stack| stack.count), Some(6));
        assert_eq!(state.mining_progress_ticks, 160);

        sim.tick();

        let state = sim
            .burner_drill_state(entity_id)
            .expect("burner drill should expose state");
        assert_eq!(state.energy.energy_remaining_joules, 0.0);
        assert_eq!(state.mining_progress_ticks, 160);
    }

    #[test]
    fn blocked_burner_drill_output_pauses_without_consuming_fuel() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let coal = item_id(&sim.world.prototypes, "coal");
        let (entity_id, x, y, before) = place_burner_drill_on_resource(&mut sim, iron_ore);
        let state = sim
            .entities
            .burner_drill_state_mut(entity_id)
            .expect("burner drill should expose state");
        state.energy.fuel_slot = Some(ItemStack {
            item_id: coal,
            count: 1,
        });
        state.output_slot = Some(ItemStack {
            item_id: coal,
            count: 1,
        });

        for _ in 0..10 {
            sim.tick();
        }

        let state = sim
            .burner_drill_state(entity_id)
            .expect("burner drill should expose state");
        assert_eq!(
            state.energy.fuel_slot,
            Some(ItemStack {
                item_id: coal,
                count: 1,
            })
        );
        assert_eq!(state.energy.energy_remaining_joules, 0.0);
        assert_eq!(state.mining_progress_ticks, 0);
        assert_eq!(resource_amount_at(&sim.world, x, y), Some(before));
    }

    #[test]
    fn invalid_burner_drill_fuel_is_rejected() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let (entity_id, _, _, _) = place_burner_drill_on_resource(&mut sim, iron_ore);
        sim.player_inventory = Inventory::player();
        sim.player_inventory.slots[0] = Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        });

        assert_eq!(
            sim.transfer_player_slot_to_burner_drill_fuel(entity_id, 0),
            Err(BurnerDrillError::InvalidFuel(iron_ore))
        );
        assert_eq!(
            sim.burner_drill_state(entity_id)
                .expect("burner drill should expose state")
                .energy
                .fuel_slot,
            None
        );
        assert_eq!(
            sim.player_inventory.slots[0],
            Some(ItemStack {
                item_id: iron_ore,
                count: 1,
            })
        );
    }

    #[test]
    fn burner_drill_outputs_ore_after_required_ticks() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let coal = item_id(&sim.world.prototypes, "coal");
        let (entity_id, _, _, _) = place_burner_drill_on_resource(&mut sim, iron_ore);
        add_fuel_to_burner_drill(&mut sim, entity_id, coal, 1);

        for _ in 0..240 {
            sim.tick();
        }

        assert_eq!(
            sim.burner_drill_state(entity_id)
                .expect("burner drill should expose state")
                .output_slot,
            Some(ItemStack {
                item_id: iron_ore,
                count: 1,
            })
        );
    }

    #[test]
    fn burner_drill_consumes_resource_tile() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let coal = item_id(&sim.world.prototypes, "coal");
        let (entity_id, x, y, before) = place_burner_drill_on_resource(&mut sim, iron_ore);
        add_fuel_to_burner_drill(&mut sim, entity_id, coal, 1);

        for _ in 0..240 {
            sim.tick();
        }

        assert_eq!(resource_amount_at(&sim.world, x, y), Some(before - 1));
    }

    #[test]
    fn belt_moves_item_to_next_segment() {
        let mut sim = Simulation::new_test_world(123);
        let belts = place_belt_line(&mut sim, 2);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        sim.insert_item_onto_belt(belts[0], 0, iron_ore)
            .expect("empty belt entry should accept an item");

        for _ in 0..32 {
            sim.tick();
        }

        assert!(
            sim.belt_segment(belts[0]).unwrap().lanes[0]
                .items
                .is_empty()
        );
        let second_lane = &sim.belt_segment(belts[1]).unwrap().lanes[0].items;
        assert_eq!(second_lane.len(), 1);
        assert_eq!(second_lane[0].item_id, iron_ore);
    }

    #[test]
    fn belt_does_not_duplicate_items() {
        let mut sim = Simulation::new_test_world(123);
        let belts = place_belt_line(&mut sim, 20);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        feed_belt_items(&mut sim, belts[0], iron_ore, 100);

        for _ in 0..2_000 {
            sim.tick();
        }

        assert_eq!(total_belt_item_count(&sim), 100);
    }

    #[test]
    fn blocked_belt_preserves_item_order() {
        let mut sim = Simulation::new_test_world(123);
        let belts = place_belt_line(&mut sim, 1);
        let inserted = [
            item_id(&sim.world.prototypes, "iron_ore"),
            item_id(&sim.world.prototypes, "copper_ore"),
            item_id(&sim.world.prototypes, "coal"),
            item_id(&sim.world.prototypes, "stone"),
        ];

        for item_id in inserted {
            loop {
                if sim.insert_item_onto_belt(belts[0], 0, item_id).is_ok() {
                    break;
                }
                sim.tick();
            }
            for _ in 0..8 {
                sim.tick();
            }
        }
        for _ in 0..200 {
            sim.tick();
        }

        let lane = &sim.belt_segment(belts[0]).unwrap().lanes[0].items;
        let downstream_to_upstream = lane
            .iter()
            .rev()
            .map(|item| item.item_id)
            .collect::<Vec<_>>();
        assert_eq!(downstream_to_upstream, inserted);
        for pair in lane.windows(2) {
            assert!(
                pair[1].position_subtile - pair[0].position_subtile >= BELT_ITEM_SPACING_SUBTILES
            );
        }
    }

    #[test]
    fn burner_drill_outputs_ore_onto_belt() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let coal = item_id(&sim.world.prototypes, "coal");
        let (drill_id, belt_id, _, _, _) =
            place_burner_drill_outputting_to_belt(&mut sim, iron_ore);
        add_fuel_to_burner_drill(&mut sim, drill_id, coal, 1);

        for _ in 0..240 {
            sim.tick();
        }

        assert_eq!(
            sim.burner_drill_state(drill_id)
                .expect("drill should expose state")
                .output_slot,
            None
        );
        assert!(
            sim.belt_segment(belt_id)
                .expect("belt should expose state")
                .lanes
                .iter()
                .any(|lane| lane.items.iter().any(|item| item.item_id == iron_ore))
        );
    }

    #[test]
    fn belt_line_moves_100_items_across_20_tiles() {
        let mut sim = Simulation::new_test_world(123);
        let belts = place_belt_line(&mut sim, 20);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        feed_belt_items(&mut sim, belts[0], iron_ore, 100);

        for _ in 0..1_000 {
            sim.tick();
        }

        assert_eq!(total_belt_item_count(&sim), 100);
        assert!(
            sim.belt_segment(*belts.last().unwrap())
                .unwrap()
                .lanes
                .iter()
                .any(|lane| !lane.items.is_empty())
        );
    }

    #[test]
    fn burner_drill_blocks_when_output_inventory_full() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let coal = item_id(&sim.world.prototypes, "coal");
        let (drill_id, chest_id, x, y, before) =
            place_burner_drill_outputting_to_chest(&mut sim, iron_ore);
        add_fuel_to_burner_drill(&mut sim, drill_id, coal, 1);
        fill_inventory_with(&mut sim, chest_id, coal);

        for _ in 0..240 {
            sim.tick();
        }

        let state = sim
            .burner_drill_state(drill_id)
            .expect("burner drill should expose state");
        assert_eq!(state.energy.energy_remaining_joules, 0.0);
        assert_eq!(
            state.energy.fuel_slot,
            Some(ItemStack {
                item_id: coal,
                count: 1,
            })
        );
        assert_eq!(state.mining_progress_ticks, 0);
        assert_eq!(resource_amount_at(&sim.world, x, y), Some(before));
        assert_eq!(
            sim.entity_inventory(chest_id)
                .expect("chest should have inventory")
                .count(iron_ore),
            0
        );
    }

    #[test]
    fn burner_drill_outputs_into_adjacent_chest() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let coal = item_id(&sim.world.prototypes, "coal");
        let (drill_id, chest_id, _, _, _) =
            place_burner_drill_outputting_to_chest(&mut sim, iron_ore);
        add_fuel_to_burner_drill(&mut sim, drill_id, coal, 1);

        for _ in 0..240 {
            sim.tick();
        }

        assert_eq!(
            sim.entity_inventory(chest_id)
                .expect("chest should have inventory")
                .count(iron_ore),
            1
        );
        assert_eq!(
            sim.burner_drill_state(drill_id)
                .expect("burner drill should expose state")
                .output_slot,
            None
        );
    }

    #[test]
    fn burner_drill_placed_on_coal_produces_coal() {
        let mut sim = Simulation::new_test_world(123);
        let coal = item_id(&sim.world.prototypes, "coal");
        let (entity_id, _, _, _) = place_burner_drill_on_resource(&mut sim, coal);
        add_fuel_to_burner_drill(&mut sim, entity_id, coal, 1);

        for _ in 0..240 {
            sim.tick();
        }

        assert_eq!(
            sim.burner_drill_state(entity_id)
                .expect("burner drill should expose state")
                .output_slot,
            Some(ItemStack {
                item_id: coal,
                count: 1,
            })
        );
    }

    #[test]
    fn burner_drill_without_resource_in_mining_area_refuses_placement() {
        let sim = Simulation::new_test_world(123);
        let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
        let (x, y) = first_buildable_rect_without_resource(&sim.world, 2, 2);

        assert!(matches!(
            sim.can_place_entity(drill, x, y, Direction::North),
            Err(BuildError::TileBlocked { .. })
        ));
    }

    #[test]
    fn burner_drill_hash_is_deterministic_for_same_seed_and_inputs() {
        let mut a = Simulation::new_test_world(123);
        let mut b = Simulation::new_test_world(123);
        let coal = item_id(&a.world.prototypes, "coal");
        let a_entity = place_burner_drill_on_resource(&mut a, coal).0;
        let b_entity = place_burner_drill_on_resource(&mut b, coal).0;

        for (sim, entity_id) in [(&mut a, a_entity), (&mut b, b_entity)] {
            sim.player_inventory = Inventory::player();
            sim.player_inventory.slots[0] = Some(ItemStack {
                item_id: coal,
                count: 2,
            });
            sim.transfer_player_slot_to_burner_drill_fuel(entity_id, 0)
                .expect("coal should transfer to drill fuel");
        }

        for _ in 0..1000 {
            a.tick();
            b.tick();
        }

        assert_eq!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn furnace_smelts_iron_ore_to_iron_plate() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let coal = item_id(&sim.world.prototypes, "coal");
        let entity_id = place_stone_furnace(&mut sim);
        add_furnace_input_and_fuel(&mut sim, entity_id, iron_ore, coal);

        for _ in 0..210 {
            sim.tick();
        }

        let state = sim
            .furnace_state(entity_id)
            .expect("furnace should expose state");
        assert_eq!(state.input_slot, None);
        assert_eq!(
            state.output_slot,
            Some(ItemStack {
                item_id: iron_plate,
                count: 1,
            })
        );
        assert_eq!(state.crafting_progress_ticks, 0);
        assert_eq!(state.energy.energy_remaining_joules, 3_685_000.0);
    }

    #[test]
    fn furnace_does_not_smelts_without_fuel() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let entity_id = place_stone_furnace(&mut sim);
        sim.player_inventory = Inventory::player();
        sim.player_inventory.slots[0] = Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        });
        sim.transfer_player_slot_to_furnace_input(entity_id, 0)
            .expect("ore should transfer to furnace input");

        for _ in 0..210 {
            sim.tick();
        }

        let state = sim
            .furnace_state(entity_id)
            .expect("furnace should expose state");
        assert_eq!(
            state.input_slot,
            Some(ItemStack {
                item_id: iron_ore,
                count: 1,
            })
        );
        assert_eq!(state.output_slot, None);
        assert_eq!(state.energy.energy_remaining_joules, 0.0);
        assert_eq!(state.crafting_progress_ticks, 0);
    }

    #[test]
    fn furnace_blocks_when_output_full() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let copper_plate = item_id(&sim.world.prototypes, "copper_plate");
        let coal = item_id(&sim.world.prototypes, "coal");
        let entity_id = place_stone_furnace(&mut sim);
        add_furnace_input_and_fuel(&mut sim, entity_id, iron_ore, coal);
        let state = sim
            .entities
            .furnace_state_mut(entity_id)
            .expect("furnace should expose state");
        state.output_slot = Some(ItemStack {
            item_id: copper_plate,
            count: 1,
        });

        for _ in 0..210 {
            sim.tick();
        }

        let state = sim
            .furnace_state(entity_id)
            .expect("furnace should expose state");
        assert_eq!(
            state.input_slot,
            Some(ItemStack {
                item_id: iron_ore,
                count: 1,
            })
        );
        assert_eq!(
            state.energy.fuel_slot,
            Some(ItemStack {
                item_id: coal,
                count: 1,
            })
        );
        assert_eq!(state.energy.energy_remaining_joules, 0.0);
        assert_eq!(state.crafting_progress_ticks, 0);
        assert_eq!(
            state.output_slot.map(|stack| stack.item_id),
            Some(copper_plate)
        );
        assert_eq!(
            state.output_slot.map(|stack| stack.item_id == iron_plate),
            Some(false)
        );
    }

    #[test]
    fn furnace_smelts_copper_ore_to_copper_plate() {
        let mut sim = Simulation::new_test_world(123);
        let copper_ore = item_id(&sim.world.prototypes, "copper_ore");
        let copper_plate = item_id(&sim.world.prototypes, "copper_plate");
        let coal = item_id(&sim.world.prototypes, "coal");
        let entity_id = place_stone_furnace(&mut sim);
        add_furnace_input_and_fuel(&mut sim, entity_id, copper_ore, coal);

        for _ in 0..210 {
            sim.tick();
        }

        assert_eq!(
            sim.furnace_state(entity_id)
                .expect("furnace should expose state")
                .output_slot,
            Some(ItemStack {
                item_id: copper_plate,
                count: 1,
            })
        );
    }

    #[test]
    fn furnace_smelts_stone_to_stone_brick() {
        let mut sim = Simulation::new_test_world(123);
        let stone = item_id(&sim.world.prototypes, "stone");
        let stone_brick = item_id(&sim.world.prototypes, "stone_brick");
        let coal = item_id(&sim.world.prototypes, "coal");
        let recipe = recipe_id(&sim.world.prototypes, "stone_brick");
        let entity_id = place_stone_furnace(&mut sim);
        add_furnace_input_and_fuel(&mut sim, entity_id, stone, coal);

        for _ in 0..210 {
            sim.tick();
        }

        let state = sim
            .furnace_state(entity_id)
            .expect("furnace should expose state");
        assert_eq!(state.active_recipe, Some(recipe));
        assert_eq!(
            state.output_slot,
            Some(ItemStack {
                item_id: stone_brick,
                count: 1,
            })
        );
    }

    #[test]
    fn invalid_furnace_input_is_rejected() {
        let mut sim = Simulation::new_test_world(123);
        let coal = item_id(&sim.world.prototypes, "coal");
        let entity_id = place_stone_furnace(&mut sim);
        sim.player_inventory = Inventory::player();
        sim.player_inventory.slots[0] = Some(ItemStack {
            item_id: coal,
            count: 1,
        });

        assert_eq!(
            sim.transfer_player_slot_to_furnace_input(entity_id, 0),
            Err(FurnaceError::InvalidInput(coal))
        );
        assert_eq!(
            sim.furnace_state(entity_id)
                .expect("furnace should expose state")
                .input_slot,
            None
        );
        assert_eq!(
            sim.player_inventory.slots[0],
            Some(ItemStack {
                item_id: coal,
                count: 1,
            })
        );
    }

    #[test]
    fn non_container_entities_reject_inventory_access() {
        let mut sim = Simulation::new_test_world(123);
        let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
        let (x, y) = first_buildable_rect(&sim.world, 1, 1);
        let entity_id = sim
            .place_entity(inserter, x, y, Direction::North)
            .expect("inserter should be placeable");

        assert_eq!(
            sim.entity_inventory(entity_id),
            Err(ContainerError::NotContainer(entity_id))
        );
    }

    #[test]
    fn player_starts_on_walkable_generated_tile() {
        let sim = Simulation::new_test_world(123);
        let (x, y) = sim.player.tile_position();
        let tile = sim
            .world
            .tile_at(x, y)
            .expect("player start should be in a generated chunk");

        assert!(tile.collision.walkable);
        assert!(sim.can_player_occupy_tile(x, y));
    }

    #[test]
    fn player_cannot_move_into_water() {
        let mut sim = Simulation::new_test_world(123);
        let (start, delta) = first_player_approach_to_water(&sim);
        let before = PlayerState::centered_on_tile(start.0, start.1);
        sim.player = before;

        sim.move_player_by_tiles(delta.0, delta.1);

        assert_eq!(sim.player, before);
    }

    #[test]
    fn player_cannot_move_into_unloaded_tiles() {
        let mut sim = Simulation::new_test_world(123);
        let (start, delta) = first_player_approach_to_unloaded_tile(&sim);
        let before = PlayerState::centered_on_tile(start.0, start.1);
        sim.player = before;

        sim.move_player_by_tiles(delta.0, delta.1);

        assert_eq!(sim.player, before);
    }

    #[test]
    fn player_cannot_move_into_occupied_entity_tile() {
        let mut sim = Simulation::new_test_world(123);
        let (start, delta) = first_player_approach_to_occupied_tile(&mut sim);
        let before = PlayerState::centered_on_tile(start.0, start.1);
        sim.player = before;

        sim.move_player_by_tiles(delta.0, delta.1);

        assert_eq!(sim.player, before);
    }

    #[test]
    fn player_axis_separated_movement_slides_along_blocked_edges() {
        let mut sim = Simulation::new_test_world(123);
        let (start, expected) = first_player_slide_fixture(&mut sim);
        sim.player = PlayerState::centered_on_tile(start.0, start.1);

        sim.move_player_by_tiles(1.0, 1.0);

        assert_eq!(sim.player.tile_position(), expected);
    }

    #[test]
    fn inventory_merges_stacks_until_stack_size() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let iron_plate = item_id(&catalog, "iron_plate");
        let mut inventory = Inventory::with_slot_count(2);

        inventory
            .insert(&catalog, iron_plate, 99)
            .expect("first insert should fit");
        inventory
            .insert(&catalog, iron_plate, 2)
            .expect("second insert should fill existing stack first");

        assert_eq!(
            inventory.slots,
            vec![
                Some(ItemStack {
                    item_id: iron_plate,
                    count: 100,
                }),
                Some(ItemStack {
                    item_id: iron_plate,
                    count: 1,
                }),
            ]
        );
    }

    #[test]
    fn inventory_rejects_insert_when_full() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let iron_plate = item_id(&catalog, "iron_plate");
        let coal = item_id(&catalog, "coal");
        let mut inventory = Inventory::with_slot_count(1);

        inventory
            .insert(&catalog, iron_plate, 100)
            .expect("initial stack should fit");
        let before = inventory.clone();

        assert_eq!(
            inventory.insert(&catalog, coal, 1),
            Err(InventoryError::InsufficientSpace)
        );
        assert_eq!(inventory, before);
    }

    #[test]
    fn inventory_remove_is_atomic() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let iron_plate = item_id(&catalog, "iron_plate");
        let mut inventory = Inventory::with_slot_count(1);

        inventory
            .insert(&catalog, iron_plate, 3)
            .expect("initial stack should fit");
        let before = inventory.clone();

        assert_eq!(
            inventory.remove(iron_plate, 4),
            Err(InventoryError::InsufficientItems)
        );
        assert_eq!(inventory, before);
        assert_eq!(inventory.count(iron_plate), 3);
    }

    #[test]
    fn crafting_consumes_ingredients_and_outputs_product() {
        let mut sim = Simulation::new_test_world(123);
        let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
        sim.player_inventory = Inventory::player();
        sim.player_inventory
            .insert(&sim.world.prototypes, iron_plate, 2)
            .expect("test inventory should accept ingredients");

        sim.start_manual_craft(recipe)
            .expect("craft should start with enough ingredients");

        assert_eq!(sim.player_inventory.count(iron_plate), 0);
        assert_eq!(sim.player_inventory.count(iron_gear_wheel), 0);
        assert_eq!(
            sim.crafting_queue.entries.front(),
            Some(&CraftingJob {
                recipe_id: recipe,
                remaining_ticks: 30,
            })
        );

        for _ in 0..30 {
            sim.tick();
        }

        assert_eq!(sim.player_inventory.count(iron_gear_wheel), 1);
        assert!(sim.crafting_queue.entries.is_empty());
    }

    #[test]
    fn crafting_does_not_start_without_ingredients() {
        let mut sim = Simulation::new_test_world(123);
        let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        sim.player_inventory = Inventory::player();
        sim.player_inventory
            .insert(&sim.world.prototypes, iron_plate, 1)
            .expect("test inventory should accept partial ingredients");
        let before = sim.player_inventory.clone();

        assert_eq!(
            sim.start_manual_craft(recipe),
            Err(CraftingError::InsufficientIngredients)
        );
        assert_eq!(sim.player_inventory, before);
        assert!(sim.crafting_queue.entries.is_empty());
    }

    #[test]
    fn crafting_product_appears_only_after_configured_ticks() {
        let mut sim = Simulation::new_test_world(123);
        let recipe = recipe_id(&sim.world.prototypes, "transport_belt");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
        let transport_belt = item_id(&sim.world.prototypes, "transport_belt");
        sim.player_inventory = Inventory::player();
        sim.player_inventory
            .insert(&sim.world.prototypes, iron_plate, 1)
            .expect("test inventory should accept iron plate");
        sim.player_inventory
            .insert(&sim.world.prototypes, iron_gear_wheel, 1)
            .expect("test inventory should accept gear");

        sim.start_manual_craft(recipe)
            .expect("craft should start with enough ingredients");
        for _ in 0..29 {
            sim.tick();
        }

        assert_eq!(sim.player_inventory.count(transport_belt), 0);
        assert_eq!(
            sim.crafting_queue
                .entries
                .front()
                .map(|job| job.remaining_ticks),
            Some(1)
        );

        sim.tick();

        assert_eq!(sim.player_inventory.count(transport_belt), 2);
        assert!(sim.crafting_queue.entries.is_empty());
    }

    #[test]
    fn full_inventory_pauses_completed_craft_until_space_is_freed() {
        let mut sim = Simulation::new_test_world(123);
        let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
        let coal = item_id(&sim.world.prototypes, "coal");
        sim.player_inventory = Inventory::with_slot_count(1);
        sim.player_inventory
            .insert(&sim.world.prototypes, iron_plate, 2)
            .expect("single stack should fit ingredients");
        sim.start_manual_craft(recipe)
            .expect("craft should start with enough ingredients");
        sim.player_inventory
            .insert(&sim.world.prototypes, coal, 100)
            .expect("blocking stack should fill inventory");

        for _ in 0..30 {
            sim.tick();
        }

        assert_eq!(sim.player_inventory.count(iron_gear_wheel), 0);
        assert_eq!(sim.crafting_queue.entries.len(), 1);
        assert_eq!(
            sim.crafting_queue
                .entries
                .front()
                .map(|job| job.remaining_ticks),
            Some(0)
        );

        sim.tick();
        assert_eq!(sim.player_inventory.count(iron_gear_wheel), 0);
        assert_eq!(sim.crafting_queue.entries.len(), 1);

        sim.player_inventory
            .remove(coal, 100)
            .expect("test should be able to free blocking stack");
        sim.tick();

        assert_eq!(sim.player_inventory.count(iron_gear_wheel), 1);
        assert!(sim.crafting_queue.entries.is_empty());
    }

    #[test]
    fn smelting_recipes_cannot_be_manually_crafted() {
        let mut sim = Simulation::new_test_world(123);
        let recipe = recipe_id(&sim.world.prototypes, "iron_plate");
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        sim.player_inventory = Inventory::player();
        sim.player_inventory
            .insert(&sim.world.prototypes, iron_ore, 1)
            .expect("test inventory should accept ore");

        assert_eq!(
            sim.start_manual_craft(recipe),
            Err(CraftingError::NotManualRecipe(recipe))
        );
        assert_eq!(sim.player_inventory.count(iron_ore), 1);
        assert!(sim.crafting_queue.entries.is_empty());
    }

    #[test]
    fn base_catalog_contains_expected_manually_craftable_recipes() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let recipe_names = [
            "stone_furnace",
            "burner_mining_drill",
            "transport_belt",
            "inserter",
            "assembling_machine",
            "lab",
            "automation_science_pack",
        ];

        for recipe_name in recipe_names {
            let recipe = catalog
                .recipes
                .iter()
                .find(|recipe| recipe.name == recipe_name)
                .unwrap_or_else(|| panic!("missing recipe {recipe_name:?}"));
            assert!(
                matches!(
                    recipe.category,
                    CraftingCategory::Crafting | CraftingCategory::Manual
                ),
                "{recipe_name} should be manually craftable"
            );
        }
    }

    #[test]
    fn player_starts_with_drill_and_furnace_only() {
        let sim = Simulation::new_test_world(123);
        let burner_mining_drill = item_id(&sim.world.prototypes, "burner_mining_drill");
        let stone_furnace = item_id(&sim.world.prototypes, "stone_furnace");
        let occupied_slots = sim
            .player_inventory
            .slots
            .iter()
            .filter_map(|slot| *slot)
            .collect::<Vec<_>>();

        assert_eq!(
            sim.player_inventory.slots.len(),
            PLAYER_INVENTORY_SLOT_COUNT
        );
        assert_eq!(sim.player_inventory.count(burner_mining_drill), 1);
        assert_eq!(sim.player_inventory.count(stone_furnace), 1);
        assert_eq!(occupied_slots.len(), 2);
        assert_eq!(
            occupied_slots.iter().map(|stack| stack.count).sum::<u16>(),
            2
        );
    }

    #[test]
    fn inventory_insert_never_exceeds_item_stack_size() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let copper_cable = item_id(&catalog, "copper_cable");
        let mut inventory = Inventory::with_slot_count(2);

        inventory
            .insert(&catalog, copper_cable, 201)
            .expect("two cable stacks should fit");

        assert_eq!(inventory.count(copper_cable), 201);
        for stack in inventory.slots.iter().flatten() {
            assert!(stack.count <= 200);
        }
    }

    #[test]
    fn zero_count_insert_and_remove_are_no_ops() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let unknown_item = ItemId::new(u16::MAX);
        let mut inventory = Inventory::with_slot_count(1);

        inventory
            .insert(&catalog, unknown_item, 0)
            .expect("zero-count insert should be a no-op");
        inventory
            .remove(unknown_item, 0)
            .expect("zero-count remove should be a no-op");

        assert_eq!(inventory.slots, vec![None]);
    }

    #[test]
    fn inserter_moves_item_from_chest_to_furnace() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let (chest_id, inserter_id, furnace_id) = place_chest_inserter_furnace_line(&mut sim);

        sim.entity_inventory_mut(chest_id)
            .expect("chest should have inventory")
            .slots[0] = Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        });

        run_inserter_until_idle(&mut sim, inserter_id);

        assert_eq!(
            sim.entity_inventory(chest_id)
                .expect("chest should have inventory")
                .count(iron_ore),
            0
        );
        assert_eq!(
            sim.furnace_state(furnace_id)
                .expect("furnace should have state")
                .input_slot,
            Some(ItemStack {
                item_id: iron_ore,
                count: 1
            })
        );
        assert!(matches!(
            sim.inserter_state(inserter_id)
                .expect("inserter should have state"),
            InserterState::WaitingForItem | InserterState::Dropping { .. }
        ));
        assert!(!matches!(
            sim.inserter_state(inserter_id)
                .expect("inserter should have state"),
            InserterState::Holding { .. }
        ));
        assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
    }

    #[test]
    fn inserter_waits_when_target_full() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let stack_size = item_stack_size(&sim.world.prototypes, iron_ore)
            .expect("iron ore should have stack size");
        let (chest_id, inserter_id, furnace_id) = place_chest_inserter_furnace_line(&mut sim);

        sim.entity_inventory_mut(chest_id)
            .expect("chest should have inventory")
            .slots[0] = Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        });
        sim.entities
            .furnace_state_mut(furnace_id)
            .expect("furnace should have state")
            .input_slot = Some(ItemStack {
            item_id: iron_ore,
            count: stack_size,
        });

        for _ in 0..BASIC_INSERTER_PICKUP_TICKS + BASIC_INSERTER_DROP_TICKS + 10 {
            sim.tick();
        }

        assert_eq!(
            sim.inserter_state(inserter_id)
                .expect("inserter should have state"),
            &InserterState::WaitingForItem
        );
        assert_eq!(
            sim.entity_inventory(chest_id)
                .expect("chest should have inventory")
                .count(iron_ore),
            1
        );
        assert_eq!(
            sim.furnace_state(furnace_id)
                .expect("furnace should have state")
                .input_slot,
            Some(ItemStack {
                item_id: iron_ore,
                count: stack_size
            })
        );
        assert!(!matches!(
            sim.inserter_state(inserter_id)
                .expect("inserter should have state"),
            InserterState::Holding { .. }
        ));
        assert_eq!(
            total_item_count_in_sim(&sim, iron_ore),
            u32::from(stack_size) + 1
        );
    }

    #[test]
    fn inserter_preserves_item_count() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let (chest_id, _inserter_id, _furnace_id) = place_chest_inserter_furnace_line(&mut sim);

        sim.entity_inventory_mut(chest_id)
            .expect("chest should have inventory")
            .slots[0] = Some(ItemStack {
            item_id: iron_ore,
            count: 3,
        });

        let ticks = (BASIC_INSERTER_PICKUP_TICKS + BASIC_INSERTER_DROP_TICKS + 5) * 3;
        for _ in 0..ticks {
            sim.tick();
            assert_eq!(total_item_count_in_sim(&sim, iron_ore), 3);
        }
    }

    #[test]
    fn inserter_moves_item_from_belt_to_furnace() {
        let mut sim = Simulation::new_test_world(123);
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let (belt_id, inserter_id, furnace_id) = place_belt_inserter_furnace_line(&mut sim);

        sim.insert_item_onto_belt(belt_id, 0, iron_ore)
            .expect("belt should accept ore");

        run_inserter_until_idle(&mut sim, inserter_id);

        assert_eq!(total_belt_count_for_item(&sim, iron_ore), 0);
        assert_eq!(
            sim.furnace_state(furnace_id)
                .expect("furnace should have state")
                .input_slot,
            Some(ItemStack {
                item_id: iron_ore,
                count: 1
            })
        );
        assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
    }

    #[test]
    fn inserter_moves_furnace_output_to_chest() {
        let mut sim = Simulation::new_test_world(123);
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let (furnace_id, inserter_id, chest_id) = place_furnace_inserter_chest_line(&mut sim);

        sim.entities
            .furnace_state_mut(furnace_id)
            .expect("furnace should have state")
            .output_slot = Some(ItemStack {
            item_id: iron_plate,
            count: 1,
        });

        run_inserter_until_idle(&mut sim, inserter_id);

        assert_eq!(
            sim.furnace_state(furnace_id)
                .expect("furnace should have state")
                .output_slot,
            None
        );
        assert_eq!(
            sim.entity_inventory(chest_id)
                .expect("chest should have inventory")
                .count(iron_plate),
            1
        );
        assert_eq!(total_item_count_in_sim(&sim, iron_plate), 1);
    }

    #[test]
    fn inserter_moves_furnace_output_to_belt() {
        let mut sim = Simulation::new_test_world(123);
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        let (furnace_id, inserter_id, _belt_id) = place_furnace_inserter_belt_line(&mut sim);

        sim.entities
            .furnace_state_mut(furnace_id)
            .expect("furnace should have state")
            .output_slot = Some(ItemStack {
            item_id: iron_plate,
            count: 1,
        });

        run_inserter_until_idle(&mut sim, inserter_id);

        assert_eq!(
            sim.furnace_state(furnace_id)
                .expect("furnace should have state")
                .output_slot,
            None
        );
        assert_eq!(total_belt_count_for_item(&sim, iron_plate), 1);
        assert_eq!(total_item_count_in_sim(&sim, iron_plate), 1);
    }

    #[test]
    fn inserter_uses_rotated_direction_for_pickup_and_drop() {
        let mut sim = Simulation::new_test_world(123);
        let chest = entity_id_by_name(&sim.world.prototypes, "chest");
        let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
        let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
        let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
        let (x, y) = first_buildable_rect_without_resource(&sim.world, 4, 2);

        let chest_id = sim
            .place_entity(chest, x, y, Direction::North)
            .expect("chest should be placeable");
        let inserter_id = sim
            .place_entity(inserter, x + 1, y, Direction::North)
            .expect("inserter should be placeable");
        let furnace_id = sim
            .place_entity(furnace, x + 2, y, Direction::North)
            .expect("furnace should be placeable");
        sim.entity_inventory_mut(chest_id)
            .expect("chest should have inventory")
            .slots[0] = Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        });

        for _ in 0..BASIC_INSERTER_PICKUP_TICKS + 2 {
            sim.tick();
        }
        assert_eq!(
            sim.entity_inventory(chest_id)
                .expect("chest should have inventory")
                .count(iron_ore),
            1
        );
        assert_eq!(
            sim.furnace_state(furnace_id)
                .expect("furnace should have state")
                .input_slot,
            None
        );

        sim.rotate_entity(inserter_id, Direction::East)
            .expect("inserter should rotate");
        run_inserter_until_idle(&mut sim, inserter_id);

        assert_eq!(
            sim.entity_inventory(chest_id)
                .expect("chest should have inventory")
                .count(iron_ore),
            0
        );
        assert_eq!(
            sim.furnace_state(furnace_id)
                .expect("furnace should have state")
                .input_slot,
            Some(ItemStack {
                item_id: iron_ore,
                count: 1
            })
        );
        assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
    }

    fn first_resource_tile(world: &WorldSim) -> (i32, i32, ResourceCell) {
        for chunk in world.chunks.values() {
            for (index, tile) in chunk.tiles.iter().enumerate() {
                if let Some(resource) = tile.resource {
                    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                    return (
                        chunk.coord.x * CHUNK_SIZE + local_x,
                        chunk.coord.y * CHUNK_SIZE + local_y,
                        resource,
                    );
                }
            }
        }

        panic!("expected at least one resource tile");
    }

    fn first_resource_tile_for_item(world: &WorldSim, resource_item: ItemId) -> (i32, i32, u32) {
        for chunk in world.chunks.values() {
            for (index, tile) in chunk.tiles.iter().enumerate() {
                let Some(resource) = tile.resource else {
                    continue;
                };

                if resource.resource_item != resource_item {
                    continue;
                }

                let (x, y) = tile_coord(chunk, index);
                return (x, y, resource.amount);
            }
        }

        panic!("expected at least one resource tile for {resource_item:?}");
    }

    fn place_belt_line(sim: &mut Simulation, length: i32) -> Vec<u64> {
        let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
        for (x, y) in all_tile_coords(&sim.world) {
            if (0..length).all(|offset| {
                sim.can_place_entity(belt, x + offset, y, Direction::East)
                    .is_ok()
            }) {
                return (0..length)
                    .map(|offset| {
                        sim.place_entity(belt, x + offset, y, Direction::East)
                            .expect("validated belt line tile should be placeable")
                    })
                    .collect();
            }
        }

        panic!("expected placeable belt line of length {length}");
    }

    fn feed_belt_items(sim: &mut Simulation, belt_id: u64, item_id: ItemId, count: usize) {
        let mut inserted = 0;
        let mut lane_index = 0;

        while inserted < count {
            if sim
                .insert_item_onto_belt(belt_id, lane_index, item_id)
                .is_ok()
            {
                inserted += 1;
                lane_index = 1 - lane_index;
            }
            sim.tick();
        }
    }

    fn total_belt_item_count(sim: &Simulation) -> usize {
        sim.entities
            .placed_entities()
            .filter_map(|placed| sim.belt_segment(placed.id).ok())
            .map(|segment| {
                segment
                    .lanes
                    .iter()
                    .map(|lane| lane.items.len())
                    .sum::<usize>()
            })
            .sum()
    }

    fn place_burner_drill_on_resource(
        sim: &mut Simulation,
        resource_item: ItemId,
    ) -> (u64, i32, i32, u32) {
        let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
        for (x, y) in all_tile_coords(&sim.world) {
            let Some(resource) = sim.world.tile_at(x, y).and_then(|tile| tile.resource) else {
                continue;
            };
            if resource.resource_item != resource_item {
                continue;
            }
            if sim.can_place_entity(drill, x, y, Direction::North).is_err() {
                continue;
            }

            let entity_id = sim
                .place_entity(drill, x, y, Direction::North)
                .expect("validated drill target should be placeable");
            return (entity_id, x, y, resource.amount);
        }

        panic!("expected placeable resource tile for burner drill");
    }

    fn place_burner_drill_outputting_to_chest(
        sim: &mut Simulation,
        resource_item: ItemId,
    ) -> (u64, u64, i32, i32, u32) {
        let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
        let chest = entity_id_by_name(&sim.world.prototypes, "chest");
        for direction in [
            Direction::North,
            Direction::East,
            Direction::South,
            Direction::West,
        ] {
            for (x, y) in all_tile_coords(&sim.world) {
                let Some(resource) = sim.world.tile_at(x, y).and_then(|tile| tile.resource) else {
                    continue;
                };
                if resource.resource_item != resource_item {
                    continue;
                }
                if sim.can_place_entity(drill, x, y, direction).is_err() {
                    continue;
                }

                let footprint = sim
                    .world
                    .entity_footprint(drill, x, y, direction)
                    .expect("validated drill prototype should have a footprint");
                let placed = PlacedEntity {
                    id: 0,
                    prototype_id: drill,
                    x,
                    y,
                    direction,
                    footprint,
                };
                let (output_x, output_y) = drill_output_tile(&placed);
                if sim
                    .can_place_entity(chest, output_x, output_y, Direction::North)
                    .is_err()
                {
                    continue;
                }

                let drill_id = sim
                    .place_entity(drill, x, y, direction)
                    .expect("validated drill target should be placeable");
                let chest_id = sim
                    .place_entity(chest, output_x, output_y, Direction::North)
                    .expect("validated chest output target should be placeable");
                return (drill_id, chest_id, x, y, resource.amount);
            }
        }

        panic!("expected burner drill fixture with adjacent chest output");
    }

    fn place_burner_drill_outputting_to_belt(
        sim: &mut Simulation,
        resource_item: ItemId,
    ) -> (u64, u64, i32, i32, u32) {
        let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
        let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
        for direction in [
            Direction::North,
            Direction::East,
            Direction::South,
            Direction::West,
        ] {
            for (x, y) in all_tile_coords(&sim.world) {
                let Some(resource) = sim.world.tile_at(x, y).and_then(|tile| tile.resource) else {
                    continue;
                };
                if resource.resource_item != resource_item {
                    continue;
                }
                if sim.can_place_entity(drill, x, y, direction).is_err() {
                    continue;
                }

                let footprint = sim
                    .world
                    .entity_footprint(drill, x, y, direction)
                    .expect("validated drill prototype should have a footprint");
                let placed = PlacedEntity {
                    id: 0,
                    prototype_id: drill,
                    x,
                    y,
                    direction,
                    footprint,
                };
                let (output_x, output_y) = drill_output_tile(&placed);
                if sim
                    .can_place_entity(belt, output_x, output_y, direction)
                    .is_err()
                {
                    continue;
                }

                let drill_id = sim
                    .place_entity(drill, x, y, direction)
                    .expect("validated drill target should be placeable");
                let belt_id = sim
                    .place_entity(belt, output_x, output_y, direction)
                    .expect("validated belt output target should be placeable");
                return (drill_id, belt_id, x, y, resource.amount);
            }
        }

        panic!("expected burner drill fixture with adjacent belt output");
    }

    fn add_fuel_to_burner_drill(
        sim: &mut Simulation,
        entity_id: u64,
        fuel_item: ItemId,
        count: u16,
    ) {
        sim.player_inventory = Inventory::player();
        sim.player_inventory.slots[0] = Some(ItemStack {
            item_id: fuel_item,
            count,
        });
        sim.transfer_player_slot_to_burner_drill_fuel(entity_id, 0)
            .expect("fuel should transfer to burner drill");
    }

    fn place_stone_furnace(sim: &mut Simulation) -> u64 {
        let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
        let (x, y) = first_buildable_rect(&sim.world, 2, 2);
        sim.place_entity(furnace, x, y, Direction::North)
            .expect("stone furnace should be placeable")
    }

    fn place_assembling_machine(sim: &mut Simulation) -> u64 {
        let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
        let (x, y) = first_buildable_rect(&sim.world, 3, 3);
        sim.place_entity(assembler, x, y, Direction::North)
            .expect("assembling machine should be placeable")
    }

    fn add_furnace_input_and_fuel(
        sim: &mut Simulation,
        entity_id: u64,
        input_item: ItemId,
        fuel_item: ItemId,
    ) {
        sim.player_inventory = Inventory::player();
        sim.player_inventory.slots[0] = Some(ItemStack {
            item_id: input_item,
            count: 1,
        });
        sim.player_inventory.slots[1] = Some(ItemStack {
            item_id: fuel_item,
            count: 1,
        });
        sim.transfer_player_slot_to_furnace_input(entity_id, 0)
            .expect("input should transfer to furnace");
        sim.transfer_player_slot_to_furnace_fuel(entity_id, 1)
            .expect("fuel should transfer to furnace");
    }

    fn fill_inventory_with(sim: &mut Simulation, entity_id: u64, item_id: ItemId) {
        let stack_size = item_stack_size(&sim.world.prototypes, item_id)
            .expect("test item should have a stack size");
        let inventory = sim
            .entity_inventory_mut(entity_id)
            .expect("test entity should have inventory");
        for slot in &mut inventory.slots {
            *slot = Some(ItemStack {
                item_id,
                count: stack_size,
            });
        }
    }

    fn first_buildable_rect_without_resource(
        world: &WorldSim,
        width: i32,
        height: i32,
    ) -> (i32, i32) {
        for chunk in world.chunks.values() {
            for (index, _) in chunk.tiles.iter().enumerate() {
                let (x, y) = tile_coord(chunk, index);
                let footprint = EntityFootprint {
                    x,
                    y,
                    width,
                    height,
                };

                if world.validate_entity_footprint(&footprint).is_ok()
                    && footprint.tiles().iter().all(|(tile_x, tile_y)| {
                        world
                            .tile_at(*tile_x, *tile_y)
                            .and_then(|tile| tile.resource)
                            .is_none()
                    })
                {
                    return (x, y);
                }
            }
        }

        panic!("expected buildable area without resources");
    }

    fn place_chest_inserter_furnace_line(sim: &mut Simulation) -> (u64, u64, u64) {
        let chest = entity_id_by_name(&sim.world.prototypes, "chest");
        let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
        let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
        let (x, y) = first_buildable_rect_without_resource(&sim.world, 4, 2);
        let chest_id = sim
            .place_entity(chest, x, y, Direction::North)
            .expect("chest should be placeable");
        let inserter_id = sim
            .place_entity(inserter, x + 1, y, Direction::East)
            .expect("inserter should be placeable");
        let furnace_id = sim
            .place_entity(furnace, x + 2, y, Direction::North)
            .expect("furnace should be placeable");

        (chest_id, inserter_id, furnace_id)
    }

    fn place_chest_inserter_assembler_line(sim: &mut Simulation) -> (u64, u64, u64) {
        let chest = entity_id_by_name(&sim.world.prototypes, "chest");
        let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
        let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
        let (x, y) = first_buildable_rect_without_resource(&sim.world, 5, 3);
        let chest_id = sim
            .place_entity(chest, x, y + 1, Direction::North)
            .expect("chest should be placeable");
        let inserter_id = sim
            .place_entity(inserter, x + 1, y + 1, Direction::East)
            .expect("inserter should be placeable");
        let assembler_id = sim
            .place_entity(assembler, x + 2, y, Direction::North)
            .expect("assembler should be placeable");

        (chest_id, inserter_id, assembler_id)
    }

    fn place_belt_inserter_furnace_line(sim: &mut Simulation) -> (u64, u64, u64) {
        let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
        let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
        let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
        let (x, y) = first_buildable_rect_without_resource(&sim.world, 4, 2);
        let belt_id = sim
            .place_entity(belt, x, y, Direction::East)
            .expect("belt should be placeable");
        let inserter_id = sim
            .place_entity(inserter, x + 1, y, Direction::East)
            .expect("inserter should be placeable");
        let furnace_id = sim
            .place_entity(furnace, x + 2, y, Direction::North)
            .expect("furnace should be placeable");

        (belt_id, inserter_id, furnace_id)
    }

    fn place_furnace_inserter_chest_line(sim: &mut Simulation) -> (u64, u64, u64) {
        let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
        let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
        let chest = entity_id_by_name(&sim.world.prototypes, "chest");
        let (x, y) = first_buildable_rect_without_resource(&sim.world, 4, 2);
        let furnace_id = sim
            .place_entity(furnace, x, y, Direction::North)
            .expect("furnace should be placeable");
        let inserter_id = sim
            .place_entity(inserter, x + 2, y, Direction::East)
            .expect("inserter should be placeable");
        let chest_id = sim
            .place_entity(chest, x + 3, y, Direction::North)
            .expect("chest should be placeable");

        (furnace_id, inserter_id, chest_id)
    }

    fn place_assembler_inserter_chest_line(sim: &mut Simulation) -> (u64, u64, u64) {
        let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
        let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
        let chest = entity_id_by_name(&sim.world.prototypes, "chest");
        let (x, y) = first_buildable_rect_without_resource(&sim.world, 5, 3);
        let assembler_id = sim
            .place_entity(assembler, x, y, Direction::North)
            .expect("assembler should be placeable");
        let inserter_id = sim
            .place_entity(inserter, x + 3, y + 1, Direction::East)
            .expect("inserter should be placeable");
        let chest_id = sim
            .place_entity(chest, x + 4, y + 1, Direction::North)
            .expect("chest should be placeable");

        (assembler_id, inserter_id, chest_id)
    }

    fn place_furnace_inserter_belt_line(sim: &mut Simulation) -> (u64, u64, u64) {
        let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
        let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
        let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
        let (x, y) = first_buildable_rect_without_resource(&sim.world, 4, 2);
        let furnace_id = sim
            .place_entity(furnace, x, y, Direction::North)
            .expect("furnace should be placeable");
        let inserter_id = sim
            .place_entity(inserter, x + 2, y, Direction::East)
            .expect("inserter should be placeable");
        let belt_id = sim
            .place_entity(belt, x + 3, y, Direction::East)
            .expect("belt should be placeable");

        (furnace_id, inserter_id, belt_id)
    }

    fn run_inserter_until_idle(sim: &mut Simulation, inserter_id: u64) {
        for _ in 0..BASIC_INSERTER_PICKUP_TICKS + BASIC_INSERTER_DROP_TICKS + 20 {
            sim.tick();
            if matches!(
                sim.inserter_state(inserter_id)
                    .expect("inserter should have state"),
                InserterState::WaitingForItem
            ) {
                return;
            }
        }

        panic!("inserter did not return to idle");
    }

    fn total_item_count_in_sim(sim: &Simulation, item_id: ItemId) -> u32 {
        sim.player_inventory.count(item_id)
            + sim
                .entities
                .entity_inventories
                .values()
                .map(|inventory| inventory.count(item_id))
                .sum::<u32>()
            + sim
                .entities
                .furnaces
                .values()
                .map(|furnace| {
                    count_slot_item(furnace.input_slot, item_id)
                        + count_slot_item(furnace.energy.fuel_slot, item_id)
                        + count_slot_item(furnace.output_slot, item_id)
                })
                .sum::<u32>()
            + sim
                .entities
                .burner_mining_drills
                .values()
                .map(|drill| {
                    count_slot_item(drill.energy.fuel_slot, item_id)
                        + count_slot_item(drill.output_slot, item_id)
                })
                .sum::<u32>()
            + sim
                .entities
                .assembling_machines
                .values()
                .map(|assembler| {
                    assembler.input_inventory.count(item_id)
                        + assembler.output_inventory.count(item_id)
                })
                .sum::<u32>()
            + total_belt_count_for_item(sim, item_id)
            + sim
                .entities
                .inserters
                .values()
                .map(|state| match state {
                    InserterState::Holding { item } if item.item_id == item_id => {
                        u32::from(item.count)
                    }
                    _ => 0,
                })
                .sum::<u32>()
    }

    fn total_belt_count_for_item(sim: &Simulation, item_id: ItemId) -> u32 {
        sim.entities
            .transport_belts
            .values()
            .map(|segment| {
                segment
                    .lanes
                    .iter()
                    .flat_map(|lane| lane.items.iter())
                    .filter(|item| item.item_id == item_id)
                    .count() as u32
            })
            .sum()
    }

    fn count_slot_item(slot: Option<ItemStack>, item_id: ItemId) -> u32 {
        match slot {
            Some(stack) if stack.item_id == item_id => u32::from(stack.count),
            _ => 0,
        }
    }

    fn run_same_assembler_actions(sim: &mut Simulation) {
        let assembler_id = place_assembling_machine(sim);
        let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
        let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
        sim.select_assembler_recipe(assembler_id, recipe)
            .expect("crafting recipe should be accepted by assembler");
        sim.player_inventory = Inventory::player();
        sim.player_inventory.slots[0] = Some(ItemStack {
            item_id: iron_plate,
            count: 4,
        });
        sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
            .expect("assembler should accept gear ingredients");
        for _ in 0..125 {
            sim.tick();
        }
    }

    fn resource_amount_at(world: &WorldSim, x: i32, y: i32) -> Option<u32> {
        world
            .tile_at(x, y)
            .and_then(|tile| tile.resource.map(|resource| resource.amount))
    }

    fn nearby_resource_pair(world: &WorldSim) -> ((i32, i32), (i32, i32)) {
        let resources = all_tile_coords(world)
            .into_iter()
            .filter(|(x, y)| {
                world
                    .tile_at(*x, *y)
                    .and_then(|tile| tile.resource)
                    .is_some()
            })
            .collect::<Vec<_>>();

        for first in &resources {
            for second in &resources {
                if first == second {
                    continue;
                }

                let dx = first.0 - second.0;
                let dy = first.1 - second.1;
                if dx * dx + dy * dy <= 6 {
                    return (*first, *second);
                }
            }
        }

        panic!("expected two resource tiles close enough to mine from one position");
    }

    fn first_water_tile(world: &WorldSim) -> (i32, i32) {
        for chunk in world.chunks.values() {
            for (index, tile) in chunk.tiles.iter().enumerate() {
                if !tile.collision.buildable {
                    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                    return (
                        chunk.coord.x * CHUNK_SIZE + local_x,
                        chunk.coord.y * CHUNK_SIZE + local_y,
                    );
                }
            }
        }

        panic!("expected at least one water tile");
    }

    fn first_buildable_rect(world: &WorldSim, width: i32, height: i32) -> (i32, i32) {
        for chunk in world.chunks.values() {
            for (index, _) in chunk.tiles.iter().enumerate() {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                let x = chunk.coord.x * CHUNK_SIZE + local_x;
                let y = chunk.coord.y * CHUNK_SIZE + local_y;
                let footprint = EntityFootprint {
                    x,
                    y,
                    width,
                    height,
                };

                if world.validate_entity_footprint(&footprint).is_ok() {
                    return (x, y);
                }
            }
        }

        panic!("expected at least one buildable {width}x{height} area");
    }

    fn first_player_approach_to_water(sim: &Simulation) -> ((i32, i32), (f32, f32)) {
        for chunk in sim.world.chunks.values() {
            for (index, tile) in chunk.tiles.iter().enumerate() {
                if tile.collision.walkable {
                    continue;
                }

                let (x, y) = tile_coord(chunk, index);
                for (dx, dy) in CARDINAL_DIRECTIONS {
                    let start = (x - dx, y - dy);
                    if sim.can_player_occupy_tile(start.0, start.1) {
                        return (start, (dx as f32, dy as f32));
                    }
                }
            }
        }

        panic!("expected a water tile with a walkable adjacent approach");
    }

    fn first_player_approach_to_unloaded_tile(sim: &Simulation) -> ((i32, i32), (f32, f32)) {
        for chunk in sim.world.chunks.values() {
            for (index, _) in chunk.tiles.iter().enumerate() {
                let (x, y) = tile_coord(chunk, index);
                if !sim.can_player_occupy_tile(x, y) {
                    continue;
                }

                for (dx, dy) in CARDINAL_DIRECTIONS {
                    if sim.world.tile_at(x + dx, y + dy).is_none() {
                        return ((x, y), (dx as f32, dy as f32));
                    }
                }
            }
        }

        panic!("expected a walkable boundary tile next to an unloaded chunk");
    }

    fn first_player_approach_to_occupied_tile(sim: &mut Simulation) -> ((i32, i32), (f32, f32)) {
        let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");

        for (x, y) in all_tile_coords(&sim.world) {
            if sim
                .can_place_entity(inserter, x, y, Direction::North)
                .is_err()
            {
                continue;
            }

            for (dx, dy) in CARDINAL_DIRECTIONS {
                let start = (x - dx, y - dy);
                if sim.can_player_occupy_tile(start.0, start.1) {
                    sim.place_entity(inserter, x, y, Direction::North)
                        .expect("validated occupied target should be placeable");
                    return (start, (dx as f32, dy as f32));
                }
            }
        }

        panic!("expected a placeable entity tile with a walkable adjacent approach");
    }

    fn first_player_slide_fixture(sim: &mut Simulation) -> ((i32, i32), (i32, i32)) {
        let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");

        for (x, y) in all_tile_coords(&sim.world) {
            let start = (x - 1, y);
            let expected = (x - 1, y + 1);

            if sim
                .can_place_entity(inserter, x, y, Direction::North)
                .is_ok()
                && sim.can_player_occupy_tile(start.0, start.1)
                && sim.can_player_occupy_tile(expected.0, expected.1)
            {
                sim.place_entity(inserter, x, y, Direction::North)
                    .expect("validated slide blocker should be placeable");
                return (start, expected);
            }
        }

        panic!("expected a slide fixture with an occupied x-axis target and open y-axis target");
    }

    fn tile_coord(chunk: &Chunk, index: usize) -> (i32, i32) {
        let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
        let local_y = (index as i32).div_euclid(CHUNK_SIZE);
        (
            chunk.coord.x * CHUNK_SIZE + local_x,
            chunk.coord.y * CHUNK_SIZE + local_y,
        )
    }

    fn all_tile_coords(world: &WorldSim) -> Vec<(i32, i32)> {
        world
            .chunks
            .values()
            .flat_map(|chunk| {
                chunk
                    .tiles
                    .iter()
                    .enumerate()
                    .map(move |(index, _)| tile_coord(chunk, index))
            })
            .collect()
    }

    fn entity_id_by_name(catalog: &PrototypeCatalog, name: &str) -> EntityPrototypeId {
        catalog
            .entities
            .iter()
            .find(|prototype| prototype.name == name)
            .map(|prototype| prototype.id)
            .unwrap_or_else(|| panic!("missing required entity prototype {name:?}"))
    }

    const CARDINAL_DIRECTIONS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
}
