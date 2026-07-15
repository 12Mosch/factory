use bevy::prelude::{ColorMaterial, Entity, Handle, Mesh, Resource};
use factory_sim::{ChunkCoord, EntityId};
use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Resource, Default)]
pub struct RenderSyncStats {
    pub player: Duration,
    pub world_tiles: Duration,
    pub resources: Duration,
    pub placed_entities: Duration,
    pub belt_directions: Duration,
    pub belt_items: Duration,
    pub total: Duration,
}

impl RenderSyncStats {
    pub fn record_player(&mut self, elapsed: Duration) {
        self.player = elapsed;
        self.update_total();
    }

    pub fn record_world_tiles(&mut self, elapsed: Duration) {
        self.world_tiles = elapsed;
        self.update_total();
    }

    pub fn record_resources(&mut self, elapsed: Duration) {
        self.resources = elapsed;
        self.update_total();
    }

    pub fn record_placed_entities(&mut self, elapsed: Duration) {
        self.placed_entities = elapsed;
        self.update_total();
    }

    pub fn record_belt_directions(&mut self, elapsed: Duration) {
        self.belt_directions = elapsed;
        self.update_total();
    }

    pub fn record_belt_items(&mut self, elapsed: Duration) {
        self.belt_items = elapsed;
        self.update_total();
    }

    fn update_total(&mut self) {
        self.total = self.player
            + self.world_tiles
            + self.resources
            + self.placed_entities
            + self.belt_directions
            + self.belt_items;
    }
}

#[derive(Resource)]
pub(crate) struct VisibleEntityIds {
    pub(crate) ids: HashSet<EntityId>,
    pub(crate) visible_revision: u64,
    pub(crate) entity_topology_revision: u64,
}

impl Default for VisibleEntityIds {
    fn default() -> Self {
        Self {
            ids: HashSet::new(),
            visible_revision: u64::MAX,
            entity_topology_revision: u64::MAX,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Resource)]
pub(crate) struct RenderDetail {
    pub(crate) show_resource_amount_labels: bool,
    pub(crate) show_belt_directions: bool,
    pub(crate) show_belt_items: bool,
    pub(crate) show_belt_item_labels: bool,
}

impl Default for RenderDetail {
    fn default() -> Self {
        Self {
            show_resource_amount_labels: true,
            show_belt_directions: true,
            show_belt_items: true,
            show_belt_item_labels: true,
        }
    }
}

#[derive(Resource, Default)]
pub struct WorldRenderCache {
    pub chunk_entities: BTreeMap<ChunkCoord, Entity>,
    pub chunk_meshes: BTreeMap<ChunkCoord, Handle<Mesh>>,
    pub material: Option<Handle<ColorMaterial>>,
    pub last_visible_revision: u64,
    pub last_chunk_revision: u64,
    pub last_reload_token: u64,
}

#[derive(Resource, Default)]
pub(crate) struct BeltItemRenderPool {
    pub(crate) sprites: Vec<Entity>,
    pub(crate) labels: Vec<Entity>,
}
