use super::super::*;
use super::ids::*;

pub(super) fn validate_fluid_box_states(sim: &Simulation) -> Result<(), SimValidationError> {
    for placed in sim.entities.placed_entities.values() {
        let prototype = sim.world.prototypes.entity(placed.prototype_id).ok_or(
            SimValidationError::InvalidEntityPrototype {
                entity_id: placed.id,
                prototype_id: placed.prototype_id,
            },
        )?;
        let states = sim.entities.fluid_boxes.get(&placed.id);
        if prototype.fluid_boxes.is_empty() {
            if states.is_some() {
                return Err(SimValidationError::InvalidEntityState {
                    entity_id: placed.id,
                });
            }
            continue;
        }

        let Some(states) = states else {
            return Err(SimValidationError::InvalidEntityState {
                entity_id: placed.id,
            });
        };
        if states.len() != prototype.fluid_boxes.len() {
            return Err(SimValidationError::InvalidEntityState {
                entity_id: placed.id,
            });
        }

        for (box_index, (state, fluid_box)) in
            states.iter().zip(prototype.fluid_boxes.iter()).enumerate()
        {
            if state.amount_milliunits > fluid_box.capacity_milliunits {
                return Err(SimValidationError::InvalidFluidBoxState {
                    entity_id: placed.id,
                    box_index,
                });
            }
            if state.amount_milliunits == 0 {
                if state.fluid_id.is_some() {
                    return Err(SimValidationError::InvalidFluidBoxState {
                        entity_id: placed.id,
                        box_index,
                    });
                }
                continue;
            }

            let Some(fluid_id) = state.fluid_id else {
                return Err(SimValidationError::InvalidFluidBoxState {
                    entity_id: placed.id,
                    box_index,
                });
            };
            if !fluid_exists(&sim.world.prototypes, fluid_id)
                || fluid_box.filter.is_some_and(|filter| filter != fluid_id)
            {
                return Err(SimValidationError::InvalidFluidBoxState {
                    entity_id: placed.id,
                    box_index,
                });
            }
        }
    }

    Ok(())
}

pub(super) fn validate_fluid_network_snapshots(sim: &Simulation) -> Result<(), SimValidationError> {
    let expected_boxes = sim
        .entities
        .fluid_boxes
        .iter()
        .flat_map(|(entity_id, boxes)| {
            let entity_id = *entity_id;
            (0..boxes.len()).map(move |box_index| (entity_id, box_index))
        })
        .collect::<BTreeSet<_>>();
    let mut networked_boxes = BTreeSet::new();

    for (expected_network_id, network) in sim.fluid_networks.iter().enumerate() {
        if network.network_id != expected_network_id as u32
            || network.box_count != network.boxes.len()
            || network.total_milliunits > network.capacity_milliunits
        {
            return Err(SimValidationError::InvalidFluidNetwork {
                network_id: network.network_id,
            });
        }

        let mut seen_boxes = BTreeSet::new();
        let mut total = 0_u64;
        let mut capacity = 0_u64;
        let mut filters = BTreeSet::new();
        let mut nonempty_fluids = BTreeSet::new();
        for box_snapshot in &network.boxes {
            let box_key = (box_snapshot.entity_id, box_snapshot.box_index);
            if !seen_boxes.insert(box_key) || !networked_boxes.insert(box_key) {
                return Err(SimValidationError::InvalidFluidNetwork {
                    network_id: network.network_id,
                });
            }
            let placed = sim.entities.placed_entity(box_snapshot.entity_id).ok_or(
                SimValidationError::InvalidFluidNetwork {
                    network_id: network.network_id,
                },
            )?;
            let prototype = sim.world.prototypes.entity(placed.prototype_id).ok_or(
                SimValidationError::InvalidFluidNetwork {
                    network_id: network.network_id,
                },
            )?;
            let fluid_box = prototype.fluid_boxes.get(box_snapshot.box_index).ok_or(
                SimValidationError::InvalidFluidNetwork {
                    network_id: network.network_id,
                },
            )?;
            let state = sim
                .entities
                .fluid_boxes
                .get(&box_snapshot.entity_id)
                .and_then(|boxes| boxes.get(box_snapshot.box_index))
                .ok_or(SimValidationError::InvalidFluidNetwork {
                    network_id: network.network_id,
                })?;

            if box_snapshot.capacity_milliunits != fluid_box.capacity_milliunits
                || box_snapshot.amount_milliunits != state.amount_milliunits
                || box_snapshot.fluid_id != state.fluid_id
                || box_snapshot.filter != fluid_box.filter
            {
                return Err(SimValidationError::InvalidFluidNetwork {
                    network_id: network.network_id,
                });
            }
            if let Some(filter) = box_snapshot.filter {
                filters.insert(filter);
            }
            if box_snapshot.amount_milliunits > 0 {
                let Some(fluid_id) = box_snapshot.fluid_id else {
                    return Err(SimValidationError::InvalidFluidNetwork {
                        network_id: network.network_id,
                    });
                };
                if network
                    .fluid_id
                    .is_some_and(|network_fluid| network_fluid != fluid_id)
                {
                    return Err(SimValidationError::InvalidFluidNetwork {
                        network_id: network.network_id,
                    });
                }
                nonempty_fluids.insert(fluid_id);
            }
            total = total.saturating_add(box_snapshot.amount_milliunits);
            capacity = capacity.saturating_add(box_snapshot.capacity_milliunits);
        }

        if total != network.total_milliunits || capacity != network.capacity_milliunits {
            return Err(SimValidationError::InvalidFluidNetwork {
                network_id: network.network_id,
            });
        }
        let filter_fluid = single_fluid(filters.iter().copied());
        let nonempty_fluid = single_fluid(nonempty_fluids.iter().copied());
        let expected_blocked = filters.len() > 1
            || nonempty_fluids.len() > 1
            || filter_fluid
                .zip(nonempty_fluid)
                .is_some_and(|(filter, fluid)| filter != fluid);
        if network.blocked != expected_blocked {
            return Err(SimValidationError::InvalidFluidNetwork {
                network_id: network.network_id,
            });
        }
        let expected_fluid_id = if nonempty_fluids.len() > 1 {
            None
        } else {
            nonempty_fluid.or(filter_fluid)
        };
        if network.fluid_id != expected_fluid_id {
            return Err(SimValidationError::InvalidFluidNetwork {
                network_id: network.network_id,
            });
        }
    }

    if networked_boxes != expected_boxes {
        return Err(SimValidationError::InvalidFluidNetwork { network_id: 0 });
    }

    Ok(())
}

fn single_fluid(mut fluids: impl Iterator<Item = FluidId>) -> Option<FluidId> {
    let first = fluids.next()?;
    fluids.next().is_none().then_some(first)
}
