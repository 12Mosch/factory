use super::*;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub(super) struct FluidSubsystem {
    pub(super) networks: Vec<FluidNetworkSnapshot>,
    #[serde(skip, default)]
    pub(super) network_ids_by_box: HashMap<FluidBoxKey, u32>,
}

impl FluidSubsystem {
    pub(super) fn from_networks(networks: Vec<FluidNetworkSnapshot>) -> Self {
        let network_ids_by_box = network_ids_by_box(&networks);
        Self {
            networks,
            network_ids_by_box,
        }
    }

    pub(super) fn replace_networks(&mut self, networks: Vec<FluidNetworkSnapshot>) {
        self.network_ids_by_box = network_ids_by_box(&networks);
        self.networks = networks;
    }

    pub(super) fn clear_networks(&mut self) {
        self.networks.clear();
        self.network_ids_by_box.clear();
    }
}

impl Hash for FluidSubsystem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.networks.hash(state);
    }
}

fn network_ids_by_box(networks: &[FluidNetworkSnapshot]) -> HashMap<FluidBoxKey, u32> {
    let box_count = networks.iter().map(|network| network.boxes.len()).sum();
    let mut network_ids_by_box = HashMap::with_capacity(box_count);
    for network in networks {
        for box_snapshot in &network.boxes {
            network_ids_by_box.insert(
                FluidBoxKey {
                    entity_id: box_snapshot.entity_id,
                    box_index: box_snapshot.box_index,
                },
                network.network_id,
            );
        }
    }
    network_ids_by_box
}
