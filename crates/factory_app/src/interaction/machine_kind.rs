use factory_data::EntityKind;
use factory_sim::{EntityId, Simulation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenMachineKind {
    Chest,
    MiningDrill,
    Furnace,
    Boiler,
    Assembler,
    /// A rocket silo: its ingredient slots, its progress, and how much of a
    /// rocket stands in it. There is no recipe picker — the silo builds the one
    /// thing it can build.
    RocketSilo,
    Lab,
    Turret,
    Inserter,
    Beacon,
    NuclearReactor,
    Roboport,
    /// A heat network entity with nothing to configure (heat pipe, heat
    /// exchanger). Opening it shows its temperature, which is what explains a
    /// heat network that is not yet making steam.
    HeatBuffer,
    ConstantCombinator,
    ArithmeticCombinator,
    DeciderCombinator,
    /// A named stop beside the track: its name, its train limit, and the
    /// circuit controls every connectable entity gets.
    TrainStop,
    /// An entity whose only configurable surface is its circuit connector
    /// (belts, pumps, tanks, accumulators, lamps). Without this the player
    /// would have no way to reach their conditions.
    Circuit,
}

/// Which window opening `entity_id` shows, or `None` for an entity with nothing
/// to show.
///
/// The per-kind windows come first and the circuit connector is the fall-back, so
/// an entity with both — a chest with a condition on it — opens its inventory
/// rather than a bare condition editor.
///
/// The kind lookup deliberately does not gate that fall-back. `machine_kind` is
/// derived from which *owned* per-kind state map holds the entity, and auxiliary
/// state — a fluid box, a circuit connection — never names a kind. An entity
/// whose only state is auxiliary therefore has no kind at all, and gating on one
/// hid the connector window from exactly the entities it was written for: pumps
/// and tanks, which keep only a fluid box, and rail signals, which keep nothing
/// because their aspect is derived every tick. Placement is still checked, by
/// both branches independently.
pub(crate) fn open_machine_kind(sim: &Simulation, entity_id: EntityId) -> Option<OpenMachineKind> {
    let machine_window =
        factory_sim::entity_access::machine_kind(sim, entity_id).and_then(|kind| match kind {
            EntityKind::Chest => Some(OpenMachineKind::Chest),
            EntityKind::MiningDrill => Some(OpenMachineKind::MiningDrill),
            EntityKind::Furnace => Some(OpenMachineKind::Furnace),
            EntityKind::Boiler => Some(OpenMachineKind::Boiler),
            EntityKind::AssemblingMachine => Some(OpenMachineKind::Assembler),
            EntityKind::RocketSilo => Some(OpenMachineKind::RocketSilo),
            EntityKind::Lab => Some(OpenMachineKind::Lab),
            EntityKind::Beacon => Some(OpenMachineKind::Beacon),
            EntityKind::NuclearReactor => Some(OpenMachineKind::NuclearReactor),
            EntityKind::Roboport => Some(OpenMachineKind::Roboport),
            EntityKind::HeatPipe | EntityKind::HeatExchanger => Some(OpenMachineKind::HeatBuffer),
            EntityKind::GunTurret => Some(OpenMachineKind::Turret),
            EntityKind::ConstantCombinator => Some(OpenMachineKind::ConstantCombinator),
            EntityKind::ArithmeticCombinator => Some(OpenMachineKind::ArithmeticCombinator),
            EntityKind::DeciderCombinator => Some(OpenMachineKind::DeciderCombinator),
            EntityKind::TrainStop => Some(OpenMachineKind::TrainStop),
            EntityKind::Inserter => sim
                .entities()
                .placed_entity(entity_id)
                .and_then(|placed| sim.catalog().entity(placed.prototype_id))
                .and_then(|prototype| prototype.burner.as_ref())
                .map(|_| OpenMachineKind::Inserter),
            _ => None,
        });
    machine_window.or_else(|| {
        factory_sim::entity_access::circuit_connector(sim, entity_id)
            .map(|_| OpenMachineKind::Circuit)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::{CHUNK_SIZE, Direction};

    /// Every prototype the catalog gives a circuit connector to has to be
    /// openable, or the player has no way to reach the one thing it can be
    /// configured to do. [`OpenMachineKind::Circuit`] exists precisely to be that
    /// window for entities with nothing else to show.
    ///
    /// Driven off the catalog rather than a hand-written list, because the list is
    /// what went stale: a connector added to a kind that keeps no state of its own
    /// is unreachable, and nothing said so.
    #[test]
    fn every_entity_with_a_circuit_connector_can_be_opened() {
        let mut sim = Simulation::new_test_world(123);
        let connectors = sim
            .catalog()
            .entities
            .iter()
            .filter(|prototype| {
                prototype.circuit_connector.is_some() && prototype.build_item.is_some()
            })
            .map(|prototype| (prototype.id, prototype.name.clone()))
            .collect::<Vec<_>>();
        assert!(
            connectors.len() > 10,
            "the base catalog wires up a good many entities"
        );

        let mut unopenable = Vec::new();
        for (prototype_id, name) in connectors {
            let entity_id = place_somewhere(&mut sim, prototype_id, &name);
            if open_machine_kind(&sim, entity_id).is_none() {
                unopenable.push(name);
            }
        }

        assert!(
            unopenable.is_empty(),
            "wired entities the player cannot open: {unopenable:?}"
        );
    }

    /// Places one of `prototype_id` wherever it fits, laying whatever else it
    /// needs beside it first.
    ///
    /// A signal is the one connector-bearing prototype that cannot stand on its
    /// own: it has to bind to a rail joint running the way it faces, so the test
    /// lays two rails and drops it beside the joint between them.
    ///
    /// Panics rather than reporting failure, because a prototype that fits nowhere
    /// in the test world is a broken fixture and not a case to skip: skipping it
    /// would let the test pass while covering one entity fewer than it claims to.
    fn place_somewhere(
        sim: &mut Simulation,
        prototype_id: factory_data::EntityPrototypeId,
        name: &str,
    ) -> EntityId {
        let rail = crate::utils::find_entity_prototype_id(sim.catalog(), "rail_straight");
        let needs_rail = sim
            .catalog()
            .entity(prototype_id)
            .is_some_and(|prototype| prototype.entity_kind.is_rail_signal());

        for (x, y) in placeable_tiles(sim) {
            if !needs_rail {
                if let Ok(entity_id) = place(sim, prototype_id, x, y, Direction::North) {
                    return entity_id;
                }
                continue;
            }
            // Two rails end to end, and the signal beside the joint between them.
            if place(sim, rail, x, y, Direction::North).is_err() {
                continue;
            }
            if place(sim, rail, x, y + 2, Direction::North).is_err() {
                continue;
            }
            sim.tick();
            if let Ok(entity_id) = place(sim, prototype_id, x + 1, y + 2, Direction::North) {
                return entity_id;
            }
        }

        panic!("{name} should be placeable somewhere in the test world");
    }

    fn place(
        sim: &mut Simulation,
        prototype_id: factory_data::EntityPrototypeId,
        x: i64,
        y: i64,
        direction: Direction,
    ) -> Result<EntityId, factory_sim::BuildError> {
        factory_sim::placement::place(
            sim,
            factory_sim::placement::EntityPlacementRequest {
                prototype_id,
                x,
                y,
                direction,
            },
        )
    }

    fn placeable_tiles(sim: &Simulation) -> Vec<(i64, i64)> {
        sim.world()
            .chunks
            .values()
            .flat_map(|chunk| {
                (0..chunk.tiles.len()).map(move |index| {
                    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                    chunk.coord.tile_at(local_x, local_y)
                })
            })
            .collect()
    }
}
