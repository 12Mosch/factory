use factory_data::{
    CraftingCategory, EntityPrototypeId, ItemId, PrototypeCatalog, RecipeId, TileId,
};
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
pub struct EntityStore {
    entities: Vec<SimEntity>,
    placed_entities: BTreeMap<u64, PlacedEntity>,
    entity_inventories: BTreeMap<u64, Inventory>,
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
        self.world.validate_entity_footprint(&footprint)?;
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
        let inventory_slot_count =
            self.world.prototypes.entities[prototype_id.index()].inventory_slot_count;
        Ok(self.entities.reserve_entity(
            prototype_id,
            x,
            y,
            direction,
            footprint,
            inventory_slot_count,
        ))
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

        self.world.validate_entity_footprint(&footprint)?;
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

    fn reserve_entity(
        &mut self,
        prototype_id: EntityPrototypeId,
        x: i32,
        y: i32,
        direction: Direction,
        footprint: EntityFootprint,
        inventory_slot_count: Option<usize>,
    ) -> u64 {
        let id = self.next_entity_id;
        self.next_entity_id += 1;
        self.occupancy
            .reserve_footprint(id, &footprint)
            .expect("validated footprint reservation should succeed");
        self.placed_entities.insert(
            id,
            PlacedEntity {
                id,
                prototype_id,
                x,
                y,
                direction,
                footprint,
            },
        );
        if let Some(slot_count) = inventory_slot_count {
            self.entity_inventories
                .insert(id, Inventory::with_slot_count(slot_count));
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

        Ok(())
    }

    fn remove_placed_entity(&mut self, entity_id: u64) -> Option<PlacedEntity> {
        let entity = self.placed_entities.remove(&entity_id)?;
        self.entity_inventories.remove(&entity_id);
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
