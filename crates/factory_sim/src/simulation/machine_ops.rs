use super::*;

pub(super) fn lab_has_science_packs(
    inventory: &Inventory,
    science_packs: &[factory_data::ItemAmount],
) -> bool {
    science_packs
        .iter()
        .all(|science_pack| inventory.count(science_pack.item) >= u32::from(science_pack.amount))
}

pub(super) fn burner_mining_drill_state_for_prototype(
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

pub(super) fn furnace_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<FurnaceState> {
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

pub(super) fn assembling_machine_state_for_prototype(
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

pub(super) fn lab_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<LabState> {
    (prototype.entity_kind == EntityKind::Lab).then(|| LabState {
        inventory: Inventory::with_slot_count(
            prototype
                .inventory_slot_count
                .expect("lab prototype should define inventory slots"),
        ),
        active_technology: None,
        progress_ticks: 0,
        required_ticks: 0,
    })
}

pub(super) fn transport_belt_segment_for_prototype(
    prototype: &factory_data::EntityPrototype,
    direction: Direction,
) -> Option<BeltSegment> {
    (prototype.entity_kind == EntityKind::TransportBelt).then(|| BeltSegment::new(direction))
}

pub(super) fn inserter_state_for_prototype(
    prototype: &factory_data::EntityPrototype,
) -> Option<InserterState> {
    (prototype.entity_kind == EntityKind::Inserter).then_some(InserterState::WaitingForItem)
}

pub(super) fn inserter_transfer_tiles(placed: &PlacedEntity) -> ((i32, i32), (i32, i32)) {
    let (dx, dy) = direction_tile_delta(placed.direction);

    (
        (placed.x - dx, placed.y - dy),
        (placed.x + dx, placed.y + dy),
    )
}

pub(super) fn peek_inserter_source_item(
    entities: &EntityStore,
    pickup_tile: (i32, i32),
) -> Option<ItemId> {
    let entity_id = entities.occupancy.entity_at(pickup_tile.0, pickup_tile.1)?;

    if let Some(inventory) = entities.entity_inventories.get(&entity_id) {
        return inventory
            .slots
            .iter()
            .flatten()
            .map(|stack| stack.item_id)
            .next();
    }

    if let Some(lab) = entities.labs.get(&entity_id) {
        return lab
            .inventory
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

pub(super) fn inserter_target_can_accept(
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

    if let Some(lab) = entities.labs.get(&entity_id) {
        return lab_can_accept_item(catalog, item.item_id)
            && lab.inventory.can_insert(catalog, item.item_id, item.count);
    }

    if let Some(furnace) = entities.furnaces.get(&entity_id) {
        return burner_fuel_slot_can_accept(catalog, furnace.energy.fuel_slot, item)
            || input_slot_can_accept(catalog, furnace.input_slot, item);
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

pub(super) fn try_take_inserter_source_item(
    entities: &mut EntityStore,
    pickup_tile: (i32, i32),
    item_id: ItemId,
) -> Option<ItemStack> {
    let entity_id = entities.occupancy.entity_at(pickup_tile.0, pickup_tile.1)?;

    if let Some(inventory) = entities.entity_inventories.get_mut(&entity_id) {
        inventory.remove(item_id, 1).ok()?;
        return Some(ItemStack { item_id, count: 1 });
    }

    if let Some(lab) = entities.labs.get_mut(&entity_id) {
        lab.inventory.remove(item_id, 1).ok()?;
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

pub(super) fn try_drop_inserter_item(
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

    if let Some(lab) = entities.labs.get_mut(&entity_id) {
        if !lab_can_accept_item(catalog, item.item_id) {
            return false;
        }

        return lab
            .inventory
            .insert(catalog, item.item_id, item.count)
            .is_ok();
    }

    if let Some(furnace) = entities.furnaces.get_mut(&entity_id) {
        if burner_fuel_slot_can_accept(catalog, furnace.energy.fuel_slot, item) {
            insert_into_single_slot(&mut furnace.energy.fuel_slot, item);
            return true;
        }

        if input_slot_can_accept(catalog, furnace.input_slot, item) {
            insert_into_single_slot(&mut furnace.input_slot, item);
            return true;
        }

        return false;
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

pub(super) fn belt_pickup_item(segment: &BeltSegment) -> Option<ItemId> {
    segment
        .lanes
        .iter()
        .flat_map(|lane| lane.items.iter())
        .max_by_key(|item| item.position_subtile)
        .map(|item| item.item_id)
}

pub(super) fn remove_one_item_from_belt(segment: &mut BeltSegment, item_id: ItemId) -> bool {
    let Some((lane_index, item_index, _)) = segment
        .lanes
        .iter()
        .enumerate()
        .flat_map(|(lane_index, lane)| {
            lane.items
                .iter()
                .enumerate()
                .map(move |(item_index, item)| (lane_index, item_index, item))
        })
        .filter(|(_, _, item)| item.item_id == item_id)
        .max_by_key(|(_, _, item)| item.position_subtile)
    else {
        return false;
    };

    segment.lanes[lane_index].items.remove(item_index);
    true
}

pub(super) fn first_resource_in_mining_area(
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

pub(super) fn first_matching_smelting_recipe(
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

pub(super) fn furnace_work_selection(
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

pub(super) fn input_slot_can_accept(
    catalog: &PrototypeCatalog,
    input_slot: Option<ItemStack>,
    stack: ItemStack,
) -> bool {
    if first_matching_smelting_recipe(catalog, stack.item_id).is_none() {
        return false;
    }

    output_slot_can_accept(catalog, input_slot, stack.item_id, stack.count)
}

pub(super) fn assembler_required_ticks(
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

pub(super) fn assembler_is_empty_for_recipe_change(state: &AssemblingMachineState) -> bool {
    state.crafting_progress_ticks == 0
        && state.input_inventory.slots.iter().all(Option::is_none)
        && state.output_inventory.slots.iter().all(Option::is_none)
}

pub(super) fn selected_assembler_recipe<'a>(
    catalog: &'a PrototypeCatalog,
    state: &AssemblingMachineState,
) -> Option<&'a factory_data::RecipePrototype> {
    let recipe_id = state.selected_recipe?;
    catalog
        .recipes
        .get(recipe_id.index())
        .filter(|recipe| recipe.id == recipe_id)
}

pub(super) fn assembler_input_can_accept(
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

pub(super) fn assembler_has_ingredients(
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

pub(super) fn assembler_output_can_accept(
    catalog: &PrototypeCatalog,
    output_inventory: &Inventory,
    products: &[factory_data::ItemAmount],
) -> bool {
    let mut output = output_inventory.clone();
    products
        .iter()
        .all(|product| output.insert(catalog, product.item, product.amount).is_ok())
}

pub(super) fn stack_in_assembler_inventory_slot(
    inventory: &Inventory,
    slot_index: usize,
) -> Result<ItemStack, AssemblerError> {
    inventory
        .slots
        .get(slot_index)
        .ok_or(AssemblerError::InvalidSlot { slot_index })?
        .ok_or(AssemblerError::EmptySlot { slot_index })
}

pub(super) fn burner_fuel_slot_can_accept(
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

pub(super) fn output_slot_can_accept(
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

pub(super) fn drill_output_target(
    entities: &EntityStore,
    placed: &PlacedEntity,
) -> DrillOutputTarget {
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

pub(super) fn drill_output_tile(placed: &PlacedEntity) -> (i32, i32) {
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

pub(super) fn drill_output_target_can_accept(
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

pub(super) fn insert_drill_output(
    entities: &mut EntityStore,
    drill_entity_id: EntityId,
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

pub(super) fn belt_output_lane_index(segment: &BeltSegment, _item_id: ItemId) -> Option<usize> {
    if belt_lane_can_accept_position(&segment.lanes[0], 0) {
        Some(0)
    } else if belt_lane_can_accept_position(&segment.lanes[1], 0) {
        Some(1)
    } else {
        None
    }
}

pub(super) fn insert_into_single_slot(slot: &mut Option<ItemStack>, stack: ItemStack) {
    match slot {
        Some(existing) => existing.count += stack.count,
        None => *slot = Some(stack),
    }
}

pub(super) fn insert_output_item(slot: &mut Option<ItemStack>, item_id: ItemId, count: u16) {
    insert_into_single_slot(slot, ItemStack { item_id, count });
}

pub(super) fn remove_from_single_slot(
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

pub(super) fn try_consume_fuel(catalog: &PrototypeCatalog, energy: &mut BurnerEnergy) -> bool {
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
