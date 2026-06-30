use super::super::super::*;
use super::*;

pub(in crate::simulation::tests) fn place_powered_fixture_origin(
    sim: &mut Simulation,
    fixture_width: i32,
    fixture_height: i32,
    pole_offset: (i32, i32),
) -> (i32, i32) {
    let (x, y, _) =
        place_powered_fixture_origin_with_boiler(sim, fixture_width, fixture_height, pole_offset);
    (x, y)
}

pub(in crate::simulation::tests) fn place_powered_fixture_origin_with_boiler(
    sim: &mut Simulation,
    fixture_width: i32,
    fixture_height: i32,
    pole_offset: (i32, i32),
) -> (i32, i32, EntityId) {
    let pump = entity_id_by_name(&sim.world.prototypes, "offshore_pump");
    let boiler = entity_id_by_name(&sim.world.prototypes, "boiler");
    let steam_engine = entity_id_by_name(&sim.world.prototypes, "steam_engine");
    let pole = entity_id_by_name(&sim.world.prototypes, "small_electric_pole");
    let coal = item_id(&sim.world.prototypes, "coal");

    for (x, y) in all_tile_coords(&sim.world) {
        let fixture_x = x + 8;
        let fixture_y = y + 1;
        let source_pole = (x + 5, y + 4);
        let target_pole = (fixture_x + pole_offset.0, fixture_y + pole_offset.1);
        let fixture = EntityFootprint {
            x: fixture_x,
            y: fixture_y,
            width: fixture_width,
            height: fixture_height,
        };

        if !fixture_is_clear_buildable(sim, &fixture)
            || !poles_within_small_pole_reach(source_pole, target_pole)
            || sim.can_place_entity(pump, x, y, Direction::North).is_err()
            || sim
                .can_place_entity(boiler, x, y + 1, Direction::North)
                .is_err()
            || sim
                .can_place_entity(steam_engine, x + 2, y + 1, Direction::North)
                .is_err()
            || sim
                .can_place_entity(pole, source_pole.0, source_pole.1, Direction::North)
                .is_err()
            || sim
                .can_place_entity(pole, target_pole.0, target_pole.1, Direction::North)
                .is_err()
        {
            continue;
        }

        sim.place_entity(pump, x, y, Direction::North)
            .expect("validated offshore pump fixture should be placeable");
        let boiler_id = sim
            .place_entity(boiler, x, y + 1, Direction::North)
            .expect("validated boiler fixture should be placeable");
        sim.place_entity(steam_engine, x + 2, y + 1, Direction::North)
            .expect("validated steam engine fixture should be placeable");
        sim.place_entity(pole, source_pole.0, source_pole.1, Direction::North)
            .expect("validated source pole fixture should be placeable");
        sim.place_entity(pole, target_pole.0, target_pole.1, Direction::North)
            .expect("validated target pole fixture should be placeable");
        sim.entities
            .boiler_state_mut(boiler_id)
            .expect("placed boiler should expose boiler state")
            .energy
            .fuel_slot = Some(ItemStack {
            item_id: coal,
            count: 50,
        });

        return (fixture_x, fixture_y, boiler_id);
    }

    panic!("expected powered fixture area");
}

pub(in crate::simulation::tests) fn place_pump_pipe_boiler_fixture(
    sim: &mut Simulation,
) -> (EntityId, EntityId, EntityId) {
    let pump = entity_id_by_name(&sim.world.prototypes, "offshore_pump");
    let pipe = entity_id_by_name(&sim.world.prototypes, "pipe");
    let boiler = entity_id_by_name(&sim.world.prototypes, "boiler");

    for (x, y) in all_tile_coords(&sim.world) {
        if sim.can_place_entity(pump, x, y, Direction::North).is_err()
            || sim
                .can_place_entity(pipe, x, y + 1, Direction::North)
                .is_err()
            || sim
                .can_place_entity(boiler, x, y + 2, Direction::North)
                .is_err()
        {
            continue;
        }

        let pump_id = sim
            .place_entity(pump, x, y, Direction::North)
            .expect("validated pump should be placeable");
        let pipe_id = sim
            .place_entity(pipe, x, y + 1, Direction::North)
            .expect("validated pipe should be placeable");
        let boiler_id = sim
            .place_entity(boiler, x, y + 2, Direction::North)
            .expect("validated boiler should be placeable");
        return (pump_id, pipe_id, boiler_id);
    }

    panic!("expected pump-pipe-boiler fixture area");
}

pub(in crate::simulation::tests) fn fixture_is_clear_buildable(
    sim: &Simulation,
    footprint: &EntityFootprint,
) -> bool {
    footprint.tiles().into_iter().all(|(x, y)| {
        sim.world
            .tile_at(x, y)
            .is_some_and(|tile| tile.collision.buildable && tile.resource.is_none())
            && sim.entities.occupancy().entity_at(x, y).is_none()
    })
}

pub(in crate::simulation::tests) fn poles_within_small_pole_reach(
    first: (i32, i32),
    second: (i32, i32),
) -> bool {
    let dx_x2 = i64::from((first.0 - second.0) * 2);
    let dy_x2 = i64::from((first.1 - second.1) * 2);
    dx_x2 * dx_x2 + dy_x2 * dy_x2 <= 15 * 15
}

pub(in crate::simulation::tests) fn place_disconnected_assembler_network(
    sim: &mut Simulation,
) -> EntityId {
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let pole = entity_id_by_name(&sim.world.prototypes, "small_electric_pole");

    for (x, y) in all_tile_coords(&sim.world) {
        let pole_pos = (x + 3, y + 1);
        if sim
            .can_place_entity(assembler, x, y, Direction::North)
            .is_err()
            || sim
                .can_place_entity(pole, pole_pos.0, pole_pos.1, Direction::North)
                .is_err()
            || !pole_is_disconnected_from_existing_poles(sim, pole_pos)
        {
            continue;
        }

        let pole_id = sim
            .place_entity(pole, pole_pos.0, pole_pos.1, Direction::North)
            .expect("validated disconnected pole should be placeable");
        debug_assert!(sim.entities.electric_poles.contains_key(&pole_id));
        return sim
            .place_entity(assembler, x, y, Direction::North)
            .expect("validated disconnected assembler should be placeable");
    }

    panic!("expected disconnected assembler network fixture");
}

pub(in crate::simulation::tests) fn first_placeable_offshore_pump(
    sim: &Simulation,
    pump: EntityPrototypeId,
) -> (i32, i32) {
    all_tile_coords(&sim.world)
        .into_iter()
        .find(|(x, y)| sim.can_place_entity(pump, *x, *y, Direction::North).is_ok())
        .expect("expected placeable offshore pump shoreline")
}

pub(in crate::simulation::tests) fn first_buildable_offshore_pump_footprint_away_from_water(
    sim: &Simulation,
    pump: EntityPrototypeId,
) -> (i32, i32) {
    let prototype = &sim.world.prototypes.entities[pump.index()];
    for (x, y) in all_tile_coords(&sim.world) {
        let footprint =
            EntityFootprint::from_size(x, y, prototype.size.x, prototype.size.y, Direction::North);
        if sim.world.validate_entity_footprint(&footprint).is_err()
            || sim
                .entities
                .occupancy()
                .validate_available(&footprint, None)
                .is_err()
        {
            continue;
        }
        let north_edge_is_water = (x..x + footprint.width).any(|tile_x| {
            sim.world
                .tile_at(tile_x, y - 1)
                .is_some_and(|tile| !tile.collision.walkable && !tile.collision.buildable)
        });
        if !north_edge_is_water {
            return (x, y);
        }
    }

    panic!("expected buildable offshore pump footprint away from water");
}

pub(in crate::simulation::tests) fn pole_is_disconnected_from_existing_poles(
    sim: &Simulation,
    pole_pos: (i32, i32),
) -> bool {
    sim.entities.electric_poles.keys().all(|entity_id| {
        let placed = sim
            .entities
            .placed_entity(*entity_id)
            .expect("electric pole state should belong to a placed entity");
        !poles_within_small_pole_reach((placed.x, placed.y), pole_pos)
    })
}

pub(in crate::simulation::tests) fn set_fluid_box(
    sim: &mut Simulation,
    entity_id: EntityId,
    box_index: usize,
    fluid_id: FluidId,
    amount_milliunits: u64,
) {
    let state = sim
        .entities
        .fluid_boxes
        .get_mut(&entity_id)
        .and_then(|boxes| boxes.get_mut(box_index))
        .expect("test entity should expose requested fluid box");
    state.fluid_id = (amount_milliunits > 0).then_some(fluid_id);
    state.amount_milliunits = amount_milliunits;
    sim.invalidate_fluid_state();
}

pub(in crate::simulation::tests) fn total_fluid_amount(sim: &Simulation, fluid_id: FluidId) -> u64 {
    sim.entities
        .fluid_boxes
        .values()
        .flat_map(|boxes| boxes.iter())
        .filter(|state| state.fluid_id == Some(fluid_id))
        .map(|state| state.amount_milliunits)
        .sum()
}
