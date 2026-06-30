use super::super::super::*;
use super::*;

pub(in crate::simulation::tests) const BASIC_UNDERGROUND_BELT_MAX_OFFSET: i32 = 5;

pub(in crate::simulation::tests) const THROUGHPUT_TEST_TICKS: usize = 60;

pub(in crate::simulation::tests) const THROUGHPUT_UPSTREAM_LEN: i32 = 12;

pub(in crate::simulation::tests) const THROUGHPUT_DOWNSTREAM_LEN: i32 = 12;

#[derive(Clone, Copy)]
pub(in crate::simulation::tests) struct BeltTier {
    pub(in crate::simulation::tests) belt: &'static str,
    pub(in crate::simulation::tests) underground_entrance: &'static str,
    pub(in crate::simulation::tests) underground_exit: &'static str,
    pub(in crate::simulation::tests) splitter: &'static str,
    pub(in crate::simulation::tests) underground_max_distance: u8,
    pub(in crate::simulation::tests) items_per_second: u32,
}

pub(in crate::simulation::tests) const BELT_TIERS: [BeltTier; 3] = [
    BeltTier {
        belt: "transport_belt",
        underground_entrance: "underground_belt_entrance",
        underground_exit: "underground_belt_exit",
        splitter: "splitter",
        underground_max_distance: 4,
        items_per_second: 15,
    },
    BeltTier {
        belt: "fast_transport_belt",
        underground_entrance: "fast_underground_belt_entrance",
        underground_exit: "fast_underground_belt_exit",
        splitter: "fast_splitter",
        underground_max_distance: 6,
        items_per_second: 30,
    },
    BeltTier {
        belt: "express_transport_belt",
        underground_entrance: "express_underground_belt_entrance",
        underground_exit: "express_underground_belt_exit",
        splitter: "express_splitter",
        underground_max_distance: 8,
        items_per_second: 45,
    },
];

pub(in crate::simulation::tests) fn straight_belt_throughput_over_one_second(
    sim: &mut Simulation,
    belt_name: &str,
) -> u32 {
    let belts = place_named_belt_line(
        sim,
        belt_name,
        THROUGHPUT_UPSTREAM_LEN + THROUGHPUT_DOWNSTREAM_LEN,
    );
    let split = THROUGHPUT_UPSTREAM_LEN as usize;
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    seed_saturated_belts(sim, &belts[..split], iron_ore);

    for _ in 0..THROUGHPUT_TEST_TICKS {
        sim.tick();
    }

    total_item_count_on_belts(sim, &belts[split..], iron_ore)
}

pub(in crate::simulation::tests) fn underground_belt_throughput_over_one_second(
    sim: &mut Simulation,
    belt_name: &str,
    entrance_name: &str,
    exit_name: &str,
    max_distance: u8,
) -> u32 {
    let (seeded, measured) =
        place_throughput_underground_pair(sim, belt_name, entrance_name, exit_name, max_distance);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    seed_saturated_belts(sim, &seeded, iron_ore);

    for _ in 0..THROUGHPUT_TEST_TICKS {
        sim.tick();
    }

    total_item_count_on_belts(sim, &measured, iron_ore)
}

pub(in crate::simulation::tests) fn splitter_throughput_over_one_second(
    sim: &mut Simulation,
    belt_name: &str,
    splitter_name: &str,
) -> u32 {
    let (input, splitter_id, outputs) =
        place_throughput_splitter_fixture(sim, belt_name, splitter_name);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    seed_saturated_belts(sim, &input, iron_ore);
    seed_saturated_splitter_input(sim, splitter_id, 0, iron_ore);

    for _ in 0..THROUGHPUT_TEST_TICKS {
        sim.tick();
    }

    total_item_count_on_belts(sim, &outputs, iron_ore)
}

pub(in crate::simulation::tests) fn place_belt_line(
    sim: &mut Simulation,
    length: i32,
) -> Vec<EntityId> {
    place_named_belt_line(sim, "transport_belt", length)
}

pub(in crate::simulation::tests) fn place_named_belt_line(
    sim: &mut Simulation,
    belt_name: &str,
    length: i32,
) -> Vec<EntityId> {
    let belt = entity_id_by_name(&sim.world.prototypes, belt_name);
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

pub(in crate::simulation::tests) fn place_throughput_underground_pair(
    sim: &mut Simulation,
    belt_name: &str,
    entrance_name: &str,
    exit_name: &str,
    max_distance: u8,
) -> (Vec<EntityId>, Vec<EntityId>) {
    let belt = entity_id_by_name(&sim.world.prototypes, belt_name);
    let entrance = entity_id_by_name(&sim.world.prototypes, entrance_name);
    let exit = entity_id_by_name(&sim.world.prototypes, exit_name);
    let underground_offset = i32::from(max_distance) + 1;

    for (x, y) in all_tile_coords(&sim.world) {
        let entrance_x = x + THROUGHPUT_UPSTREAM_LEN;
        let exit_x = entrance_x + underground_offset;
        let input_tiles = (0..THROUGHPUT_UPSTREAM_LEN)
            .map(|offset| (x + offset, y))
            .collect::<Vec<_>>();
        let output_tiles = (1..=THROUGHPUT_DOWNSTREAM_LEN)
            .map(|offset| (exit_x + offset, y))
            .collect::<Vec<_>>();

        if input_tiles.iter().any(|(tile_x, tile_y)| {
            sim.can_place_entity(belt, *tile_x, *tile_y, Direction::East)
                .is_err()
        }) || sim
            .can_place_entity(entrance, entrance_x, y, Direction::East)
            .is_err()
            || sim
                .can_place_entity(exit, exit_x, y, Direction::East)
                .is_err()
            || output_tiles.iter().any(|(tile_x, tile_y)| {
                sim.can_place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .is_err()
            })
        {
            continue;
        }

        let mut seeded = input_tiles
            .iter()
            .map(|(tile_x, tile_y)| {
                sim.place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .expect("validated throughput input belt should be placeable")
            })
            .collect::<Vec<_>>();
        seeded.push(
            sim.place_entity(entrance, entrance_x, y, Direction::East)
                .expect("validated throughput underground entrance should be placeable"),
        );
        let mut measured = vec![
            sim.place_entity(exit, exit_x, y, Direction::East)
                .expect("validated throughput underground exit should be placeable"),
        ];
        measured.extend(output_tiles.iter().map(|(tile_x, tile_y)| {
            sim.place_entity(belt, *tile_x, *tile_y, Direction::East)
                .expect("validated throughput output belt should be placeable")
        }));

        return (seeded, measured);
    }

    panic!("expected placeable throughput underground fixture for {entrance_name}");
}

pub(in crate::simulation::tests) fn place_throughput_splitter_fixture(
    sim: &mut Simulation,
    belt_name: &str,
    splitter_name: &str,
) -> (Vec<EntityId>, EntityId, Vec<EntityId>) {
    let belt = entity_id_by_name(&sim.world.prototypes, belt_name);
    let splitter = entity_id_by_name(&sim.world.prototypes, splitter_name);

    for (x, y) in all_tile_coords(&sim.world) {
        let splitter_x = x + THROUGHPUT_UPSTREAM_LEN;
        let input_tiles = (0..THROUGHPUT_UPSTREAM_LEN)
            .map(|offset| (x + offset, y))
            .collect::<Vec<_>>();
        let output0_tiles = (1..=THROUGHPUT_DOWNSTREAM_LEN)
            .map(|offset| (splitter_x + offset, y))
            .collect::<Vec<_>>();
        let output1_tiles = (1..=THROUGHPUT_DOWNSTREAM_LEN)
            .map(|offset| (splitter_x + offset, y + 1))
            .collect::<Vec<_>>();

        if input_tiles.iter().any(|(tile_x, tile_y)| {
            sim.can_place_entity(belt, *tile_x, *tile_y, Direction::East)
                .is_err()
        }) || sim
            .can_place_entity(splitter, splitter_x, y, Direction::East)
            .is_err()
            || output0_tiles.iter().any(|(tile_x, tile_y)| {
                sim.can_place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .is_err()
            })
            || output1_tiles.iter().any(|(tile_x, tile_y)| {
                sim.can_place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .is_err()
            })
        {
            continue;
        }

        let input = input_tiles
            .iter()
            .map(|(tile_x, tile_y)| {
                sim.place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .expect("validated splitter throughput input belt should be placeable")
            })
            .collect::<Vec<_>>();
        let splitter_id = sim
            .place_entity(splitter, splitter_x, y, Direction::East)
            .expect("validated throughput splitter should be placeable");
        let mut outputs = output0_tiles
            .iter()
            .map(|(tile_x, tile_y)| {
                sim.place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .expect("validated splitter throughput output belt should be placeable")
            })
            .collect::<Vec<_>>();
        outputs.extend(output1_tiles.iter().map(|(tile_x, tile_y)| {
            sim.place_entity(belt, *tile_x, *tile_y, Direction::East)
                .expect("validated splitter throughput output belt should be placeable")
        }));

        return (input, splitter_id, outputs);
    }

    panic!("expected placeable throughput splitter fixture for {splitter_name}");
}

pub(in crate::simulation::tests) fn seed_saturated_belts(
    sim: &mut Simulation,
    belts: &[EntityId],
    item_id: ItemId,
) {
    for entity_id in belts {
        let segment = sim
            .entities
            .transport_belts
            .get_mut(entity_id)
            .expect("throughput fixture should contain belt state");
        seed_saturated_lane(&mut segment.lanes[0], item_id, &[0, 64, 128, 192]);
        seed_saturated_lane(&mut segment.lanes[1], item_id, &[32, 96, 160, 224]);
    }
}

pub(in crate::simulation::tests) fn seed_saturated_splitter_input(
    sim: &mut Simulation,
    splitter_id: EntityId,
    input_port: usize,
    item_id: ItemId,
) {
    let state = sim
        .entities
        .splitters
        .get_mut(&splitter_id)
        .expect("throughput fixture should contain splitter state");
    let input_lanes = state
        .input_lanes
        .get_mut(input_port)
        .expect("throughput splitter input port should exist");
    seed_saturated_lane(&mut input_lanes[0], item_id, &[0, 64, 128, 192]);
    seed_saturated_lane(&mut input_lanes[1], item_id, &[32, 96, 160, 224]);
}

pub(in crate::simulation::tests) fn seed_saturated_lane(
    lane: &mut BeltLane,
    item_id: ItemId,
    positions: &[u16],
) {
    lane.items.clear();
    lane.items
        .extend(positions.iter().map(|position_subtile| BeltItem {
            item_id,
            position_subtile: *position_subtile,
        }));
}

pub(in crate::simulation::tests) struct SplitterFixture {
    pub(in crate::simulation::tests) input0: EntityId,
    pub(in crate::simulation::tests) input1: EntityId,
    pub(in crate::simulation::tests) splitter: EntityId,
    pub(in crate::simulation::tests) output0: Vec<EntityId>,
    pub(in crate::simulation::tests) output1: Vec<EntityId>,
}

pub(in crate::simulation::tests) fn place_splitter_fixture(
    sim: &mut Simulation,
    output_len: i32,
    connect_second_output: bool,
) -> SplitterFixture {
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    let splitter = entity_id_by_name(&sim.world.prototypes, "splitter");

    for (x, y) in all_tile_coords(&sim.world) {
        let input0 = (x, y);
        let input1 = (x, y + 1);
        let splitter_tile = (x + 1, y);
        let output0_start = (x + 2, y);
        let output1_start = (x + 2, y + 1);

        let output0_tiles = (0..output_len)
            .map(|offset| (output0_start.0 + offset, output0_start.1))
            .collect::<Vec<_>>();
        let output1_tiles = if connect_second_output {
            (0..output_len)
                .map(|offset| (output1_start.0 + offset, output1_start.1))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        if sim
            .can_place_entity(belt, input0.0, input0.1, Direction::East)
            .is_err()
            || sim
                .can_place_entity(belt, input1.0, input1.1, Direction::East)
                .is_err()
            || sim
                .can_place_entity(splitter, splitter_tile.0, splitter_tile.1, Direction::East)
                .is_err()
            || output0_tiles.iter().any(|(tile_x, tile_y)| {
                sim.can_place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .is_err()
            })
            || output1_tiles.iter().any(|(tile_x, tile_y)| {
                sim.can_place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .is_err()
            })
        {
            continue;
        }

        let input0_id = sim
            .place_entity(belt, input0.0, input0.1, Direction::East)
            .expect("validated splitter input belt should be placeable");
        let input1_id = sim
            .place_entity(belt, input1.0, input1.1, Direction::East)
            .expect("validated splitter input belt should be placeable");
        let splitter_id = sim
            .place_entity(splitter, splitter_tile.0, splitter_tile.1, Direction::East)
            .expect("validated splitter should be placeable");
        let output0 = output0_tiles
            .iter()
            .map(|(tile_x, tile_y)| {
                sim.place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .expect("validated splitter output belt should be placeable")
            })
            .collect();
        let output1 = output1_tiles
            .iter()
            .map(|(tile_x, tile_y)| {
                sim.place_entity(belt, *tile_x, *tile_y, Direction::East)
                    .expect("validated splitter output belt should be placeable")
            })
            .collect();

        return SplitterFixture {
            input0: input0_id,
            input1: input1_id,
            splitter: splitter_id,
            output0,
            output1,
        };
    }

    panic!("expected placeable splitter fixture");
}

pub(in crate::simulation::tests) fn total_item_count_on_belts(
    sim: &Simulation,
    belts: &[EntityId],
    item_id: ItemId,
) -> u32 {
    belts
        .iter()
        .filter_map(|entity_id| sim.belt_segment(*entity_id).ok())
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

pub(in crate::simulation::tests) fn place_underground_belt_pair(
    sim: &mut Simulation,
    offset: i32,
    exit_direction: Direction,
) -> (EntityId, EntityId) {
    place_underground_belt_endpoint_pair(sim, "underground_belt_exit", offset, exit_direction)
}

pub(in crate::simulation::tests) fn place_underground_belt_endpoint_pair(
    sim: &mut Simulation,
    output_endpoint_name: &str,
    offset: i32,
    output_direction: Direction,
) -> (EntityId, EntityId) {
    let entrance = entity_id_by_name(&sim.world.prototypes, "underground_belt_entrance");
    let output = entity_id_by_name(&sim.world.prototypes, output_endpoint_name);

    for (x, y) in all_tile_coords(&sim.world) {
        let output_x = x + offset;
        if sim
            .can_place_entity(entrance, x, y, Direction::East)
            .is_ok()
            && sim
                .can_place_entity(output, output_x, y, output_direction)
                .is_ok()
        {
            let entrance_id = sim
                .place_entity(entrance, x, y, Direction::East)
                .expect("validated underground entrance tile should be placeable");
            let output_id = sim
                .place_entity(output, output_x, y, output_direction)
                .expect("validated underground endpoint tile should be placeable");
            return (entrance_id, output_id);
        }
    }

    panic!("expected placeable underground belt pair with offset {offset}");
}

pub(in crate::simulation::tests) fn feed_belt_items(
    sim: &mut Simulation,
    belt_id: EntityId,
    item_id: ItemId,
    count: usize,
) {
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

pub(in crate::simulation::tests) fn total_belt_item_count(sim: &Simulation) -> usize {
    let belt_items = sim
        .entities
        .placed_entities()
        .filter_map(|placed| sim.belt_segment(placed.id).ok())
        .map(|segment| {
            segment
                .lanes
                .iter()
                .map(|lane| lane.items.len())
                .sum::<usize>()
        })
        .sum::<usize>();
    let splitter_items = sim
        .entities
        .splitters
        .values()
        .map(|state| {
            state
                .input_lanes
                .iter()
                .flat_map(|input_lanes| input_lanes.iter())
                .map(|lane| lane.items.len())
                .sum::<usize>()
        })
        .sum::<usize>();

    belt_items + splitter_items
}
