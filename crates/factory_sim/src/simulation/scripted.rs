use super::*;

pub fn scripted_inputs_for_red_science_factory() -> Vec<SimCommand> {
    vec![SimCommand::BuildRedScienceResearchFixture]
}

impl Simulation {
    pub fn new_scripted_red_science_factory() -> Self {
        let mut sim = Self::new_seeded(123);
        for command in scripted_inputs_for_red_science_factory() {
            sim.apply_command(&command)
                .expect("scripted red science commands should apply");
        }
        sim
    }

    pub(super) fn build_red_science_research_fixture(&mut self) {
        let logistics = factory_data::technology_id_by_name(&self.world.prototypes, "logistics");
        let automation = factory_data::technology_id_by_name(&self.world.prototypes, "automation");
        if !self.research.is_unlocked("logistics")
            && self.active_research() != Some(logistics)
            && !self.research_queue().contains(&logistics)
        {
            self.enqueue_research(logistics)
                .expect("logistics research should be queueable");
        }
        if !self.research.is_unlocked("automation")
            && self.active_research() != Some(automation)
            && !self.research_queue().contains(&automation)
        {
            self.enqueue_research(automation)
                .expect("automation research should be queueable after logistics");
        }

        let lab = factory_data::entity_prototype_id_by_name(&self.world.prototypes, "lab");
        let (lab_x, lab_y, boiler_id) = self
            .build_basic_power_fixture()
            .expect("scripted red science fixture should be able to place power");
        let lab_id = crate::placement::place(
            self,
            crate::placement::EntityPlacementRequest {
                prototype_id: lab,
                x: lab_x,
                y: lab_y,
                direction: Direction::North,
            },
        )
        .expect("scripted red science fixture should be able to place a lab");
        let science_pack =
            factory_data::item_id_by_name(&self.world.prototypes, "automation_science_pack");
        let coal = factory_data::item_id_by_name(&self.world.prototypes, "coal");
        self.entities
            .boiler_state_mut(boiler_id)
            .expect("placed boiler should expose boiler state")
            .energy
            .fuel_slot = ItemSlot::from_stack(
            &self.world.prototypes,
            ItemStack::new(&self.world.prototypes, coal, 10)
                .expect("scripted coal should form a valid stack"),
        )
        .expect("scripted fuel slot should be valid");
        self.entities
            .lab_state_mut(lab_id)
            .expect("placed lab should expose lab state")
            .inventory
            .insert(&self.world.prototypes, science_pack, 35)
            .expect("scripted lab inventory should accept research packs");
    }

    fn build_basic_power_fixture(&mut self) -> Option<(WorldTileCoord, WorldTileCoord, EntityId)> {
        let pump =
            factory_data::entity_prototype_id_by_name(&self.world.prototypes, "offshore_pump");
        let boiler = factory_data::entity_prototype_id_by_name(&self.world.prototypes, "boiler");
        let steam_engine =
            factory_data::entity_prototype_id_by_name(&self.world.prototypes, "steam_engine");
        let pole = factory_data::entity_prototype_id_by_name(
            &self.world.prototypes,
            "small_electric_pole",
        );
        let lab = factory_data::entity_prototype_id_by_name(&self.world.prototypes, "lab");

        'candidate: for (x, y) in self.all_tile_coords() {
            let lab_x = x + 8;
            let lab_y = y + 1;
            let source_pole = (x + 5, y + 4);
            let lab_pole = (x + 9, y + 5);
            let pump_request = scripted_placement_request(pump, x, y);
            let boiler_request = scripted_placement_request(boiler, x, y + 1);
            let steam_engine_request = scripted_placement_request(steam_engine, x + 2, y + 1);
            let source_pole_request =
                scripted_placement_request(pole, source_pole.0, source_pole.1);
            let lab_pole_request = scripted_placement_request(pole, lab_pole.0, lab_pole.1);
            let lab_request = scripted_placement_request(lab, lab_x, lab_y);
            let fixture_requests = [
                pump_request,
                boiler_request,
                steam_engine_request,
                source_pole_request,
                lab_pole_request,
            ];

            if fixture_requests
                .into_iter()
                .chain([lab_request])
                .any(|request| crate::placement::validate(self, request).is_err())
            {
                continue;
            }

            let mut placed = Vec::with_capacity(fixture_requests.len());
            for request in fixture_requests {
                match crate::placement::place(self, request) {
                    Ok(entity_id) => placed.push(entity_id),
                    Err(_) => {
                        for entity_id in placed {
                            crate::entity_mutation::remove(self, entity_id);
                        }
                        continue 'candidate;
                    }
                }
            }
            let boiler_id = placed[1];

            return Some((lab_x, lab_y, boiler_id));
        }

        None
    }

    fn all_tile_coords(&self) -> Vec<(WorldTileCoord, WorldTileCoord)> {
        self.world
            .chunks
            .values()
            .flat_map(|chunk| {
                chunk.tiles.iter().enumerate().map(move |(index, _)| {
                    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                    chunk.coord.tile_at(local_x, local_y)
                })
            })
            .collect()
    }
}

fn scripted_placement_request(
    prototype_id: EntityPrototypeId,
    x: WorldTileCoord,
    y: WorldTileCoord,
) -> crate::placement::EntityPlacementRequest {
    crate::placement::EntityPlacementRequest {
        prototype_id,
        x,
        y,
        direction: Direction::North,
    }
}

/// What a scripted chemical-science fixture entity is stuffed with after
/// placement. `None` entities only need to exist (pipes, poles, machines).
#[derive(Clone, Copy, PartialEq, Eq)]
enum ChemicalFixtureRole {
    None,
    Boiler,
    BoilerCoalChest,
    PlasticCoalChest,
    CircuitChest,
    CableChest,
    SteelChest,
    GearChest,
    PipeChest,
    CrudeTank,
    WaterTank,
    Lab,
    Turret,
}

struct ChemicalFixtureEntity {
    prototype_id: EntityPrototypeId,
    dx: WorldTileCoord,
    dy: WorldTileCoord,
    direction: Direction,
    role: ChemicalFixtureRole,
}

/// Bounding rectangle of the fixture relative to its anchor (the offshore
/// pump tile): columns `-2..=33`, rows `0..=20`. The pump's water must sit
/// north of the anchor, outside this rectangle.
const CHEMICAL_FIXTURE_RECT: (
    WorldTileCoord,
    WorldTileCoord,
    WorldTileCoord,
    WorldTileCoord,
) = (-2, 0, 33, 20);

/// Research queue for the scripted chemical science factory, in dependency
/// order: the spine that unlocks chemical science pack production first, then
/// the remaining green-science branches, then every blue-science technology.
const CHEMICAL_FIXTURE_RESEARCH_QUEUE: &[&str] = &[
    "logistics",
    "automation",
    "electric_power",
    "logistic_science_pack",
    "logistics_2",
    "fluid_handling",
    "oil_processing",
    "plastics",
    "sulfur_processing",
    "advanced_electronics",
    "engine",
    "chemical_science_pack",
    "stone_walls",
    "turrets",
    "electric_mining",
    "electric_energy_distribution_1",
    "advanced_material_processing",
    "logistics_3",
    "advanced_oil_processing",
    "lubricant",
    "advanced_material_processing_2",
    "electric_energy_distribution_2",
];

/// Machine role assignments for the scripted program: the `occurrence`-th
/// placed entity of a prototype (in `EntityId` order, i.e. placement order)
/// gets `recipe` selected once the recipe is unlocked by research.
const CHEMICAL_FIXTURE_RECIPES: &[(&str, usize, &str)] = &[
    ("oil_refinery", 0, "basic_oil_processing"),
    ("chemical_plant", 0, "plastic_bar"),
    ("chemical_plant", 1, "sulfur"),
    ("assembling_machine", 0, "advanced_circuit"),
    ("assembling_machine", 1, "chemical_science_pack"),
    ("assembling_machine", 2, "engine_unit"),
];

fn chemical_fixture_entities(catalog: &PrototypeCatalog) -> Vec<ChemicalFixtureEntity> {
    use ChemicalFixtureRole as Role;
    use Direction::{East, North, West};

    let chest = factory_data::entity_prototype_id_by_name(catalog, "chest");
    let inserter = factory_data::entity_prototype_id_by_name(catalog, "inserter");
    let small_pole = factory_data::entity_prototype_id_by_name(catalog, "small_electric_pole");
    let medium_pole = factory_data::entity_prototype_id_by_name(catalog, "medium_electric_pole");
    let pump = factory_data::entity_prototype_id_by_name(catalog, "offshore_pump");
    let boiler = factory_data::entity_prototype_id_by_name(catalog, "boiler");
    let pipe = factory_data::entity_prototype_id_by_name(catalog, "pipe");
    let steam_engine = factory_data::entity_prototype_id_by_name(catalog, "steam_engine");
    let tank = factory_data::entity_prototype_id_by_name(catalog, "storage_tank");
    let refinery = factory_data::entity_prototype_id_by_name(catalog, "oil_refinery");
    let chemical_plant = factory_data::entity_prototype_id_by_name(catalog, "chemical_plant");
    let assembler = factory_data::entity_prototype_id_by_name(catalog, "assembling_machine");
    let lab = factory_data::entity_prototype_id_by_name(catalog, "lab");
    let turret = factory_data::entity_prototype_id_by_name(catalog, "gun_turret");

    let mut entities = Vec::new();
    let mut push = |prototype_id, dx, dy, direction, role| {
        entities.push(ChemicalFixtureEntity {
            prototype_id,
            dx,
            dy,
            direction,
            role,
        });
    };

    // Power plant: one offshore pump feeds two water-chained boilers whose
    // steam collects in a pipe spine that drives four steam engines. Each
    // boiler is refuelled by an inserter from a stuffed coal chest.
    push(chest, -2, 1, North, Role::BoilerCoalChest);
    push(chest, -2, 4, North, Role::BoilerCoalChest);
    push(inserter, -1, 1, East, Role::None);
    push(inserter, -1, 4, East, Role::None);
    push(small_pole, -1, 3, North, Role::None);
    push(pump, 0, 0, North, Role::None);
    push(boiler, 0, 1, North, Role::Boiler);
    push(boiler, 0, 4, North, Role::Boiler);
    for dy in 2..=17 {
        push(pipe, 2, dy, North, Role::None);
    }
    for dy in [1, 6, 11, 16] {
        push(steam_engine, 3, dy, North, Role::None);
    }
    for dy in [3, 8, 13, 18] {
        push(small_pole, 6, dy, North, Role::None);
    }
    push(medium_pole, 12, 0, North, Role::None);

    // Fluid supply: a stuffed crude tank mates directly with the refinery's
    // crude input; a stuffed water tank feeds the sulfur plant's second
    // input through a single pipe. Petroleum gas leaves the refinery east
    // into a pipe main that the chemical plants hang under.
    push(tank, 9, 1, North, Role::CrudeTank);
    push(refinery, 12, 1, North, Role::None);
    push(tank, 23, 2, North, Role::WaterTank);
    push(pipe, 17, 4, North, Role::None);
    for dx in 17..=22 {
        push(pipe, dx, 5, North, Role::None);
    }
    push(pipe, 24, 5, North, Role::None);
    push(chemical_plant, 18, 6, North, Role::None); // plastic
    push(chemical_plant, 22, 6, North, Role::None); // sulfur
    push(chest, 16, 7, North, Role::PlasticCoalChest);
    push(inserter, 17, 7, East, Role::None);
    push(medium_pole, 19, 4, North, Role::None);
    push(medium_pole, 26, 4, North, Role::None);

    // Assembly line: plastic feeds the advanced-circuit assembler, sulfur
    // and both intermediates feed the chemical-science assembler, and the
    // engine assembler draws from stuffed part chests. Placement order
    // fixes the program's role indices: circuits, science packs, engines.
    push(assembler, 18, 10, North, Role::None); // advanced circuits
    push(assembler, 22, 10, North, Role::None); // chemical science packs
    push(assembler, 26, 10, North, Role::None); // engine units
    push(chest, 16, 10, North, Role::CircuitChest);
    push(chest, 16, 11, North, Role::CableChest);
    push(chest, 30, 10, North, Role::SteelChest);
    push(chest, 30, 11, North, Role::GearChest);
    push(chest, 30, 12, North, Role::PipeChest);
    push(inserter, 17, 10, East, Role::None); // circuits -> advanced
    push(inserter, 17, 11, East, Role::None); // cable -> advanced
    push(inserter, 19, 9, North, Role::None); // plastic -> advanced
    push(inserter, 21, 11, East, Role::None); // advanced -> science
    push(inserter, 23, 9, North, Role::None); // sulfur -> science
    push(inserter, 25, 11, West, Role::None); // engines -> science
    push(inserter, 29, 10, West, Role::None); // steel -> engines
    push(inserter, 29, 11, West, Role::None); // gears -> engines
    push(inserter, 29, 12, West, Role::None); // pipes -> engines
    push(inserter, 23, 13, North, Role::None); // science packs -> lab
    for (dx, dy) in [(15, 9), (21, 9), (27, 9)] {
        push(medium_pole, dx, dy, North, Role::None);
    }

    // Lab block: twelve labs stuffed with red and green packs; the lab at
    // (21, 14) additionally receives chemical science packs by inserter.
    for dy in [15, 19] {
        for dx in [12, 16, 20, 24, 28] {
            push(medium_pole, dx, dy, North, Role::None);
        }
    }
    for dy in [14, 18] {
        for dx in [9, 13, 17, 21, 25, 29] {
            push(lab, dx, dy, North, Role::Lab);
        }
    }

    // Perimeter defense against raids drawn in by the factory's pollution.
    for (dx, dy) in [(17, 0), (27, 0), (32, 10), (7, 14), (32, 18)] {
        push(turret, dx, dy, North, Role::Turret);
    }

    entities
}

pub fn scripted_inputs_for_chemical_science_factory() -> Vec<SimCommand> {
    vec![SimCommand::BuildChemicalScienceFactoryFixture]
}

impl Simulation {
    /// Builds a deterministic scripted factory that researches every
    /// technology through blue science, producing chemical science packs from
    /// crude oil with machines only. World seeds are scanned in order until
    /// the fixture fits, so construction is reproducible.
    pub fn new_scripted_chemical_science_factory() -> Self {
        for seed in 0..64 {
            let mut sim = Self::new_seeded(seed);
            if sim.try_build_chemical_science_factory_fixture() {
                return sim;
            }
        }

        panic!("expected a world seed in 0..64 that fits the chemical science factory fixture");
    }

    pub(super) fn build_chemical_science_factory_fixture(&mut self) {
        assert!(
            self.try_build_chemical_science_factory_fixture(),
            "scripted chemical science fixture should find a buildable anchor"
        );
    }

    /// Selects the fixture's production recipes on their machines as research
    /// unlocks them. Idempotent, so scripted runs apply it every tick; a
    /// selection that is not possible yet is retried on a later application.
    pub(super) fn run_chemical_science_factory_program(&mut self) {
        for &(prototype_name, occurrence, recipe_name) in CHEMICAL_FIXTURE_RECIPES {
            let prototype_id =
                factory_data::entity_prototype_id_by_name(&self.world.prototypes, prototype_name);
            let recipe_id = factory_data::recipe_id_by_name(&self.world.prototypes, recipe_name);
            if !recipe_is_unlocked(&self.world.prototypes, &self.research, recipe_id) {
                continue;
            }
            let Some(entity_id) = self
                .entities
                .placed_entities
                .iter()
                .filter(|(_, placed)| placed.prototype_id == prototype_id)
                .map(|(&entity_id, _)| entity_id)
                .nth(occurrence)
            else {
                continue;
            };
            let already_selected = self
                .entities
                .assembler_state(entity_id)
                .is_ok_and(|state| state.selected_recipe == Some(recipe_id));
            if !already_selected {
                let _ = self.select_assembler_recipe(entity_id, recipe_id);
            }
        }
    }

    fn try_build_chemical_science_factory_fixture(&mut self) -> bool {
        let entities = chemical_fixture_entities(&self.world.prototypes);

        'candidate: for (x, y) in self.all_tile_coords() {
            if !self.chemical_fixture_rect_is_clear(x, y) {
                continue;
            }
            for entity in &entities {
                let request = crate::placement::EntityPlacementRequest {
                    prototype_id: entity.prototype_id,
                    x: x + entity.dx,
                    y: y + entity.dy,
                    direction: entity.direction,
                };
                if crate::placement::validate(self, request).is_err() {
                    continue 'candidate;
                }
            }

            self.place_chemical_science_fixture(&entities, x, y);
            return true;
        }

        false
    }

    /// Cheap pre-filter: the fixture rectangle must be fully buildable,
    /// resource-free, unoccupied, and clear of the player before any per
    /// entity placement validation runs.
    fn chemical_fixture_rect_is_clear(&self, x: WorldTileCoord, y: WorldTileCoord) -> bool {
        let (min_dx, min_dy, max_dx, max_dy) = CHEMICAL_FIXTURE_RECT;
        let player_tile = self.player.tile_position();
        for dy in min_dy..=max_dy {
            for dx in min_dx..=max_dx {
                let (tile_x, tile_y) = (x + dx, y + dy);
                if (tile_x, tile_y) == player_tile {
                    return false;
                }
                let tile_is_clear = self
                    .world
                    .tile_at(tile_x, tile_y)
                    .is_some_and(|tile| tile.collision.buildable && tile.resource.is_none())
                    && self.entities.occupancy.entity_at(tile_x, tile_y).is_none();
                if !tile_is_clear {
                    return false;
                }
            }
        }

        true
    }

    fn place_chemical_science_fixture(
        &mut self,
        entities: &[ChemicalFixtureEntity],
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) {
        use ChemicalFixtureRole as Role;

        let coal = factory_data::item_id_by_name(&self.world.prototypes, "coal");
        let circuit = factory_data::item_id_by_name(&self.world.prototypes, "electronic_circuit");
        let cable = factory_data::item_id_by_name(&self.world.prototypes, "copper_cable");
        let steel = factory_data::item_id_by_name(&self.world.prototypes, "steel_plate");
        let gear = factory_data::item_id_by_name(&self.world.prototypes, "iron_gear_wheel");
        let pipe_item = factory_data::item_id_by_name(&self.world.prototypes, "pipe");
        let red_pack =
            factory_data::item_id_by_name(&self.world.prototypes, "automation_science_pack");
        let green_pack =
            factory_data::item_id_by_name(&self.world.prototypes, "logistic_science_pack");
        let magazine = factory_data::item_id_by_name(&self.world.prototypes, "firearm_magazine");

        for entity in entities {
            let entity_id = crate::placement::place(
                self,
                crate::placement::EntityPlacementRequest {
                    prototype_id: entity.prototype_id,
                    x: x + entity.dx,
                    y: y + entity.dy,
                    direction: entity.direction,
                },
            )
            .expect("validated chemical science fixture entity should be placeable");

            match entity.role {
                Role::None => {}
                Role::Boiler => self.stuff_boiler_fuel(entity_id, coal, 100),
                Role::BoilerCoalChest => self.stuff_entity_inventory(entity_id, coal, 1200),
                Role::PlasticCoalChest => self.stuff_entity_inventory(entity_id, coal, 300),
                Role::CircuitChest => self.stuff_entity_inventory(entity_id, circuit, 400),
                Role::CableChest => self.stuff_entity_inventory(entity_id, cable, 600),
                Role::SteelChest => self.stuff_entity_inventory(entity_id, steel, 200),
                Role::GearChest => self.stuff_entity_inventory(entity_id, gear, 200),
                Role::PipeChest => self.stuff_entity_inventory(entity_id, pipe_item, 200),
                Role::CrudeTank => self.stuff_fluid_box(entity_id, "crude_oil", 24_000_000),
                Role::WaterTank => self.stuff_fluid_box(entity_id, "water", 24_000_000),
                Role::Lab => {
                    self.stuff_lab_inventory(entity_id, red_pack, 150);
                    self.stuff_lab_inventory(entity_id, green_pack, 140);
                }
                Role::Turret => self.stuff_turret_ammo(entity_id, magazine, 200),
            }
        }

        for technology_name in CHEMICAL_FIXTURE_RESEARCH_QUEUE {
            let technology_id =
                factory_data::technology_id_by_name(&self.world.prototypes, technology_name);
            self.enqueue_research(technology_id)
                .unwrap_or_else(|_| panic!("{technology_name} should be queueable"));
        }
    }

    fn stuff_boiler_fuel(&mut self, entity_id: EntityId, coal: ItemId, count: u16) {
        self.entities
            .boiler_state_mut(entity_id)
            .expect("placed boiler should expose boiler state")
            .energy
            .fuel_slot = ItemSlot::from_stack(
            &self.world.prototypes,
            ItemStack::new(&self.world.prototypes, coal, count)
                .expect("scripted coal should form a valid stack"),
        )
        .expect("scripted fuel slot should be valid");
    }

    fn stuff_entity_inventory(&mut self, entity_id: EntityId, item_id: ItemId, count: u16) {
        self.entities
            .entity_inventories
            .get_mut(&entity_id)
            .expect("scripted fixture chest should have an inventory")
            .insert(&self.world.prototypes, item_id, count)
            .expect("scripted fixture chest should accept items");
    }

    fn stuff_lab_inventory(&mut self, entity_id: EntityId, item_id: ItemId, count: u16) {
        self.entities
            .lab_state_mut(entity_id)
            .expect("placed lab should expose lab state")
            .inventory
            .insert(&self.world.prototypes, item_id, count)
            .expect("scripted lab inventory should accept research packs");
    }

    fn stuff_turret_ammo(&mut self, entity_id: EntityId, item_id: ItemId, count: u16) {
        self.entities
            .gun_turrets
            .get_mut(&entity_id)
            .expect("placed turret should expose turret state")
            .ammo
            .insert(&self.world.prototypes, item_id, count)
            .expect("scripted turret ammo inventory should accept magazines");
    }

    fn stuff_fluid_box(&mut self, entity_id: EntityId, fluid_name: &str, amount_milliunits: u64) {
        let fluid_id = factory_data::fluid_id_by_name(&self.world.prototypes, fluid_name);
        let state = self
            .entities
            .fluid_boxes
            .get_mut(&entity_id)
            .and_then(|boxes| boxes.first_mut())
            .expect("scripted fixture tank should expose a fluid box");
        state.fluid_id = Some(fluid_id);
        state.amount_milliunits = amount_milliunits;
        self.invalidate_fluid_state();
    }
}
