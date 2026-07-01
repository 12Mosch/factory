use super::*;

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

    pub fn placed_entity(&self, entity_id: EntityId) -> Option<&PlacedEntity> {
        self.placed_entities.get(&entity_id)
    }

    pub fn placed_entities(&self) -> impl Iterator<Item = &PlacedEntity> {
        self.placed_entities.values()
    }

    pub(super) fn new_test_entities(seed: u64) -> Self {
        Self {
            entities: vec![SimEntity {
                id: EntityId::new(1),
                x: (seed % 97) as i64,
                y: (seed % 53) as i64,
            }],
            placed_entities: BTreeMap::new(),
            entity_inventories: BTreeMap::new(),
            burner_mining_drills: BTreeMap::new(),
            furnaces: BTreeMap::new(),
            assembling_machines: BTreeMap::new(),
            labs: BTreeMap::new(),
            electric_poles: BTreeMap::new(),
            electric_consumers: BTreeMap::new(),
            steam_engines: BTreeMap::new(),
            boilers: BTreeMap::new(),
            offshore_pumps: BTreeMap::new(),
            fluid_boxes: BTreeMap::new(),
            transport_belts: BTreeMap::new(),
            splitters: BTreeMap::new(),
            inserters: BTreeMap::new(),
            occupancy: OccupancyGrid::default(),
            next_entity_id: 2,
        }
    }

    pub(super) fn entity_inventory(
        &self,
        entity_id: EntityId,
    ) -> Result<&Inventory, ContainerError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(ContainerError::MissingEntity(entity_id));
        }

        self.entity_inventories
            .get(&entity_id)
            .or_else(|| self.labs.get(&entity_id).map(|lab| &lab.inventory))
            .ok_or(ContainerError::NotContainer(entity_id))
    }

    pub(super) fn entity_inventory_mut(
        &mut self,
        entity_id: EntityId,
    ) -> Result<&mut Inventory, ContainerError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(ContainerError::MissingEntity(entity_id));
        }

        self.entity_inventories
            .get_mut(&entity_id)
            .or_else(|| self.labs.get_mut(&entity_id).map(|lab| &mut lab.inventory))
            .ok_or(ContainerError::NotContainer(entity_id))
    }

    pub(super) fn lab_state(&self, entity_id: EntityId) -> Result<&LabState, LabError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(LabError::MissingEntity(entity_id));
        }

        self.labs.get(&entity_id).ok_or(LabError::NotLab(entity_id))
    }

    pub(super) fn lab_state_mut(&mut self, entity_id: EntityId) -> Result<&mut LabState, LabError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(LabError::MissingEntity(entity_id));
        }

        self.labs
            .get_mut(&entity_id)
            .ok_or(LabError::NotLab(entity_id))
    }

    pub(super) fn burner_drill_state(
        &self,
        entity_id: EntityId,
    ) -> Result<&BurnerMiningDrillState, BurnerDrillError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(BurnerDrillError::MissingEntity(entity_id));
        }

        self.burner_mining_drills
            .get(&entity_id)
            .ok_or(BurnerDrillError::NotBurnerDrill(entity_id))
    }

    pub(super) fn burner_drill_state_mut(
        &mut self,
        entity_id: EntityId,
    ) -> Result<&mut BurnerMiningDrillState, BurnerDrillError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(BurnerDrillError::MissingEntity(entity_id));
        }

        self.burner_mining_drills
            .get_mut(&entity_id)
            .ok_or(BurnerDrillError::NotBurnerDrill(entity_id))
    }

    pub(super) fn furnace_state(&self, entity_id: EntityId) -> Result<&FurnaceState, FurnaceError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(FurnaceError::MissingEntity(entity_id));
        }

        self.furnaces
            .get(&entity_id)
            .ok_or(FurnaceError::NotFurnace(entity_id))
    }

    pub(super) fn furnace_state_mut(
        &mut self,
        entity_id: EntityId,
    ) -> Result<&mut FurnaceState, FurnaceError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(FurnaceError::MissingEntity(entity_id));
        }

        self.furnaces
            .get_mut(&entity_id)
            .ok_or(FurnaceError::NotFurnace(entity_id))
    }

    pub(super) fn boiler_state(&self, entity_id: EntityId) -> Result<&BoilerState, BoilerError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(BoilerError::MissingEntity(entity_id));
        }

        self.boilers
            .get(&entity_id)
            .ok_or(BoilerError::NotBoiler(entity_id))
    }

    pub(super) fn boiler_state_mut(
        &mut self,
        entity_id: EntityId,
    ) -> Result<&mut BoilerState, BoilerError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(BoilerError::MissingEntity(entity_id));
        }

        self.boilers
            .get_mut(&entity_id)
            .ok_or(BoilerError::NotBoiler(entity_id))
    }

    pub(super) fn fluid_box_states(&self, entity_id: EntityId) -> Option<&[FluidBoxState]> {
        self.fluid_boxes.get(&entity_id).map(Vec::as_slice)
    }

    pub(super) fn assembler_state(
        &self,
        entity_id: EntityId,
    ) -> Result<&AssemblingMachineState, AssemblerError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(AssemblerError::MissingEntity(entity_id));
        }

        self.assembling_machines
            .get(&entity_id)
            .ok_or(AssemblerError::NotAssembler(entity_id))
    }

    pub(super) fn assembler_state_mut(
        &mut self,
        entity_id: EntityId,
    ) -> Result<&mut AssemblingMachineState, AssemblerError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(AssemblerError::MissingEntity(entity_id));
        }

        self.assembling_machines
            .get_mut(&entity_id)
            .ok_or(AssemblerError::NotAssembler(entity_id))
    }

    pub(super) fn belt_segment(&self, entity_id: EntityId) -> Result<&BeltSegment, BeltError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(BeltError::MissingEntity(entity_id));
        }

        self.transport_belts
            .get(&entity_id)
            .ok_or(BeltError::NotTransportBelt(entity_id))
    }

    pub(super) fn belt_segment_mut(
        &mut self,
        entity_id: EntityId,
    ) -> Result<&mut BeltSegment, BeltError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(BeltError::MissingEntity(entity_id));
        }

        self.transport_belts
            .get_mut(&entity_id)
            .ok_or(BeltError::NotTransportBelt(entity_id))
    }

    pub(super) fn inserter_state(
        &self,
        entity_id: EntityId,
    ) -> Result<&InserterState, InserterError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(InserterError::MissingEntity(entity_id));
        }

        self.inserters
            .get(&entity_id)
            .ok_or(InserterError::NotInserter(entity_id))
    }

    pub(super) fn inserter_state_mut(
        &mut self,
        entity_id: EntityId,
    ) -> Result<&mut InserterState, InserterError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(InserterError::MissingEntity(entity_id));
        }

        self.inserters
            .get_mut(&entity_id)
            .ok_or(InserterError::NotInserter(entity_id))
    }

    pub(super) fn splitter_state(
        &self,
        entity_id: EntityId,
    ) -> Result<&SplitterState, SplitterError> {
        if !self.placed_entities.contains_key(&entity_id) {
            return Err(SplitterError::MissingEntity(entity_id));
        }

        self.splitters
            .get(&entity_id)
            .ok_or(SplitterError::NotSplitter(entity_id))
    }

    pub(super) fn insert_item_onto_belt(
        &mut self,
        entity_id: EntityId,
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

    pub(super) fn reserve_entity(&mut self, reservation: EntityReservation) -> EntityId {
        let id = EntityId::new(self.next_entity_id);
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
        if let Some(state) = reservation.lab {
            self.labs.insert(id, state);
        }
        if let Some(state) = reservation.electric_pole {
            self.electric_poles.insert(id, state);
        }
        if let Some(state) = reservation.electric_consumer {
            self.electric_consumers.insert(id, state);
        }
        if let Some(state) = reservation.steam_engine {
            self.steam_engines.insert(id, state);
        }
        if let Some(state) = reservation.boiler {
            self.boilers.insert(id, state);
        }
        if let Some(state) = reservation.offshore_pump {
            self.offshore_pumps.insert(id, state);
        }
        if let Some(fluid_boxes) = reservation.fluid_boxes {
            self.fluid_boxes.insert(id, fluid_boxes);
        }
        if let Some(segment) = reservation.transport_belt {
            self.transport_belts.insert(id, segment);
        }
        if let Some(state) = reservation.splitter {
            self.splitters.insert(id, state);
        }
        if let Some(state) = reservation.inserter {
            self.inserters.insert(id, state);
        }
        id
    }

    pub(super) fn update_entity_footprint(
        &mut self,
        entity_id: EntityId,
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
        if let Some(splitter) = self.splitters.get_mut(&entity_id) {
            splitter.dir = direction;
        }

        Ok(())
    }

    pub(super) fn remove_placed_entity(&mut self, entity_id: EntityId) -> Option<PlacedEntity> {
        let entity = self.placed_entities.remove(&entity_id)?;
        self.entity_inventories.remove(&entity_id);
        self.burner_mining_drills.remove(&entity_id);
        self.furnaces.remove(&entity_id);
        self.assembling_machines.remove(&entity_id);
        self.labs.remove(&entity_id);
        self.electric_poles.remove(&entity_id);
        self.electric_consumers.remove(&entity_id);
        self.steam_engines.remove(&entity_id);
        self.boilers.remove(&entity_id);
        self.offshore_pumps.remove(&entity_id);
        self.fluid_boxes.remove(&entity_id);
        self.transport_belts.remove(&entity_id);
        self.splitters.remove(&entity_id);
        self.inserters.remove(&entity_id);
        self.occupancy
            .release_footprint(entity_id, &entity.footprint);
        Some(entity)
    }

    pub(super) fn advance(&mut self, tick: Tick, seed: u64) {
        for entity in &mut self.entities {
            let step = splitmix64(seed ^ entity.id.raw() ^ tick.0);
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

    pub fn contains_tile(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
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

    pub fn entity_at(&self, x: i32, y: i32) -> Option<EntityId> {
        self.occupied_tiles.get(&(x, y)).copied()
    }

    pub fn entity_ids_in_tile_rect(
        &self,
        min_x: i32,
        max_x: i32,
        min_y: i32,
        max_y: i32,
    ) -> BTreeSet<EntityId> {
        if min_x > max_x || min_y > max_y {
            return BTreeSet::new();
        }

        self.occupied_tiles
            .range((min_x, i32::MIN)..=(max_x, i32::MAX))
            .filter_map(|(&(x, y), &entity_id)| {
                (x >= min_x && x <= max_x && y >= min_y && y <= max_y).then_some(entity_id)
            })
            .collect()
    }

    pub fn validate_available(
        &self,
        footprint: &EntityFootprint,
        ignored_entity_id: Option<EntityId>,
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
        entity_id: EntityId,
        footprint: &EntityFootprint,
    ) -> Result<(), BuildError> {
        self.validate_available(footprint, None)?;

        for tile in footprint.tiles() {
            self.occupied_tiles.insert(tile, entity_id);
        }

        Ok(())
    }

    pub fn release_footprint(&mut self, entity_id: EntityId, footprint: &EntityFootprint) {
        for tile in footprint.tiles() {
            if self.entity_at(tile.0, tile.1) == Some(entity_id) {
                self.occupied_tiles.remove(&tile);
            }
        }
    }
}
