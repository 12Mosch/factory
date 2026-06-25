use factory_data::PrototypeCatalog;

#[derive(Clone, Debug)]
pub struct Simulation {
    tick: u64,
    world: World,
    prototypes: PrototypeCatalog,
}

#[derive(Clone, Debug)]
struct World {
    seed: u64,
}

impl Simulation {
    pub fn new(seed: u64, prototypes: PrototypeCatalog) -> Self {
        Self {
            tick: 0,
            world: World { seed },
            prototypes,
        }
    }

    pub fn new_test_world(seed: u64) -> Self {
        Self::new(seed, PrototypeCatalog::test_catalog())
    }

    pub fn tick(&mut self) {
        self.tick += 1;
    }

    pub fn tick_count(&self) -> u64 {
        self.tick
    }

    pub fn seed(&self) -> u64 {
        self.world.seed
    }

    pub fn prototype_count(&self) -> usize {
        self.prototypes.item_count()
    }
}
