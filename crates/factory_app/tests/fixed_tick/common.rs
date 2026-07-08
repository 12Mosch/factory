use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use factory_app::FactoryAppPlugin;
use factory_app::placement::build::buildable_prototypes;
use factory_app::build::resources::{BuildSelection, HotbarState};
use factory_app::resources::SimResource;
use factory_data::{EntityPrototypeId, ItemId, PrototypeCatalog};
use factory_sim::{CHUNK_SIZE, Direction, EntityFootprint, Inventory, ItemStack, Simulation};
use std::time::Duration;

pub fn run_to_tick_with_frame_rate(frame_rate: f64, target_tick: u64) -> (u64, u64) {
    let mut app = test_app(Duration::from_secs_f64(1.0 / frame_rate));
    run_until_tick(&mut app, target_tick);
    sim_tick_and_hash(&app)
}

pub fn test_app(frame_duration: Duration) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FactoryAppPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(frame_duration));
    app
}

pub fn run_until_tick(app: &mut App, target_tick: u64) {
    while app.world().resource::<SimResource>().sim.tick_count() < target_tick {
        app.update();
    }
}

pub fn sim_tick_and_hash(app: &App) -> (u64, u64) {
    let sim = &app.world().resource::<SimResource>().sim;
    (sim.tick_count(), sim.state_hash())
}

pub fn pixel_at(map: &factory_app::rendering::map_texture::MapPixels, tile: (i32, i32)) -> [u8; 4] {
    let local_x = (tile.0 - map.bounds.min_x) as u32;
    let local_y = (tile.1 - map.bounds.min_y) as u32;
    let flipped_y = map.bounds.height - 1 - local_y;
    let offset = ((flipped_y * map.bounds.width + local_x) * 4) as usize;
    [
        map.data[offset],
        map.data[offset + 1],
        map.data[offset + 2],
        map.data[offset + 3],
    ]
}

pub fn first_resource_tile_for_app(sim: &Simulation) -> (i32, i32, factory_sim::ResourceCell) {
    sim.world()
        .chunks
        .values()
        .flat_map(|chunk| {
            chunk
                .tiles
                .iter()
                .enumerate()
                .filter_map(move |(index, tile)| {
                    let resource = tile.resource?;
                    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                    Some((
                        chunk.coord.x * CHUNK_SIZE + local_x,
                        chunk.coord.y * CHUNK_SIZE + local_y,
                        resource,
                    ))
                })
        })
        .next()
        .expect("generated world should contain resource tiles")
}

pub fn format_item_name_for_test(sim: &Simulation, item_id: ItemId) -> String {
    let name = &sim.catalog().items[item_id.index()].name;
    name.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn first_available_build_selection(app: &App) -> BuildSelection {
    let sim = &app.world().resource::<SimResource>().sim;
    let buildable = buildable_prototypes(sim.catalog())
        .into_iter()
        .find(|buildable| sim.player_inventory().count(buildable.item_id) > 0)
        .expect("starting inventory should include at least one buildable item");
    buildable.selection()
}

pub fn first_available_hotbar_slot(app: &App) -> (usize, BuildSelection) {
    let sim = &app.world().resource::<SimResource>().sim;
    let hotbar = app.world().resource::<HotbarState>();
    hotbar
        .slots
        .iter()
        .enumerate()
        .find_map(|(slot_index, slot)| {
            let selection = (*slot)?;
            (sim.player_inventory().count(selection.item_id) > 0).then_some((slot_index, selection))
        })
        .expect("default hotbar should include at least one item from the starting inventory")
}

pub fn hotbar_key_for_slot(slot_index: usize) -> KeyCode {
    match slot_index {
        0 => KeyCode::Digit1,
        1 => KeyCode::Digit2,
        2 => KeyCode::Digit3,
        3 => KeyCode::Digit4,
        4 => KeyCode::Digit5,
        5 => KeyCode::Digit6,
        6 => KeyCode::Digit7,
        7 => KeyCode::Digit8,
        8 => KeyCode::Digit9,
        9 => KeyCode::Digit0,
        _ => panic!("test hotbar slot should be addressable by number key"),
    }
}

pub fn place_powered_fixture_origin(
    sim: &mut Simulation,
    fixture_width: i32,
    fixture_height: i32,
    pole_offset: (i32, i32),
) -> (i32, i32) {
    let pump = entity_id_by_name(sim.catalog(), "offshore_pump");
    let boiler = entity_id_by_name(sim.catalog(), "boiler");
    let steam_engine = entity_id_by_name(sim.catalog(), "steam_engine");
    let pole = entity_id_by_name(sim.catalog(), "small_electric_pole");
    let coal = item_id_by_name(sim.catalog(), "coal");

    for (x, y) in all_tile_coords(sim) {
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

        *sim.player_inventory_mut() = Inventory::player();
        sim.player_inventory_mut().slots[0] = Some(ItemStack {
            item_id: coal,
            count: 50,
        });
        sim.transfer_player_slot_to_boiler_fuel(boiler_id, 0)
            .expect("boiler should accept coal fuel");

        return (fixture_x, fixture_y);
    }

    panic!("expected powered fixture area");
}

pub fn all_tile_coords(sim: &Simulation) -> Vec<(i32, i32)> {
    sim.world()
        .chunks
        .values()
        .flat_map(|chunk| {
            chunk.tiles.iter().enumerate().map(move |(index, _)| {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                (
                    chunk.coord.x * CHUNK_SIZE + local_x,
                    chunk.coord.y * CHUNK_SIZE + local_y,
                )
            })
        })
        .collect()
}

pub fn fixture_is_clear_buildable(sim: &Simulation, footprint: &EntityFootprint) -> bool {
    sim.world().validate_entity_footprint(footprint).is_ok()
        && sim
            .entities()
            .occupancy()
            .validate_available(footprint, None)
            .is_ok()
        && footprint.tiles().into_iter().all(|(x, y)| {
            sim.world()
                .tile_at(x, y)
                .is_some_and(|tile| tile.resource.is_none())
        })
}

pub fn poles_within_small_pole_reach(first: (i32, i32), second: (i32, i32)) -> bool {
    let dx_x2 = i64::from((first.0 - second.0) * 2);
    let dy_x2 = i64::from((first.1 - second.1) * 2);
    dx_x2 * dx_x2 + dy_x2 * dy_x2 <= 15 * 15
}

pub fn first_buildable_rect(sim: &Simulation, prototype_id: EntityPrototypeId) -> (i32, i32) {
    let prototype = &sim.catalog().entities[prototype_id.index()];

    for chunk in sim.world().chunks.values() {
        for (index, _) in chunk.tiles.iter().enumerate() {
            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            let x = chunk.coord.x * CHUNK_SIZE + local_x;
            let y = chunk.coord.y * CHUNK_SIZE + local_y;
            let footprint = EntityFootprint {
                x,
                y,
                width: prototype.size.x,
                height: prototype.size.y,
            };

            if sim.world().validate_entity_footprint(&footprint).is_ok()
                && sim
                    .entities()
                    .occupancy()
                    .validate_available(&footprint, None)
                    .is_ok()
            {
                return (x, y);
            }
        }
    }

    panic!("expected at least one buildable area");
}

pub fn first_placeable_resource_rect(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
    resource_item: ItemId,
) -> (i32, i32) {
    for chunk in sim.world().chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            let Some(resource) = tile.resource else {
                continue;
            };
            if resource.resource_item != resource_item {
                continue;
            }

            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            let x = chunk.coord.x * CHUNK_SIZE + local_x;
            let y = chunk.coord.y * CHUNK_SIZE + local_y;

            if sim
                .can_place_entity(prototype_id, x, y, Direction::North)
                .is_ok()
            {
                return (x, y);
            }
        }
    }

    panic!("expected at least one placeable resource area");
}

pub fn entity_id_by_name(catalog: &PrototypeCatalog, name: &str) -> EntityPrototypeId {
    factory_data::entity_prototype_id_by_name(catalog, name)
}

pub fn item_id_by_name(catalog: &PrototypeCatalog, name: &str) -> ItemId {
    factory_data::item_id_by_name(catalog, name)
}

pub fn recipe_id_by_name(catalog: &PrototypeCatalog, name: &str) -> factory_data::RecipeId {
    factory_data::recipe_id_by_name(catalog, name)
}

pub fn complete_research_by_name(sim: &mut Simulation, technology_name: &str) {
    let technology_id = technology_id_by_name(sim.catalog(), technology_name);
    let required_units = sim.catalog().technologies[technology_id.index()].required_units;

    sim.select_research(technology_id)
        .unwrap_or_else(|_| panic!("{technology_name} should be selectable"));
    sim.add_research_units(required_units)
        .unwrap_or_else(|_| panic!("{technology_name} should complete"));
}

pub fn technology_id_by_name(catalog: &PrototypeCatalog, name: &str) -> factory_data::TechnologyId {
    factory_data::technology_id_by_name(catalog, name)
}
