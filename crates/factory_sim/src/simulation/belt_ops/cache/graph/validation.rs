use super::*;

impl TransportLaneGraph {
    /// Checks the saved execution plan before any unchecked slot traversal or
    /// incremental patch can use it. Pending edits may refer to removed lanes;
    /// a clean plan must additionally match the live entity geometry.
    pub(in crate::simulation::belt_ops::cache) fn is_valid(
        &self,
        entities: &EntityStore,
        clean: bool,
    ) -> bool {
        if self.upstream_by_slot.len() != self.lanes.len() {
            return false;
        }
        let mut expected_upstreams = vec![Vec::new(); self.lanes.len()];
        let mut vacant = std::collections::BTreeSet::new();
        for (slot, lane) in self.lanes.iter().enumerate() {
            let Some(key) = lane.key else {
                vacant.insert(slot as u32);
                if lane.downstream != TransportLaneDownstream::Missing || lane.run != VACANT_SLOT {
                    return false;
                }
                continue;
            };
            let valid_key = match key {
                TransportLaneKey::Belt { lane_index, .. } => lane_index < 2,
                TransportLaneKey::Splitter {
                    input_port,
                    lane_index,
                    ..
                } => input_port < 2 && lane_index < 2,
            };
            if !valid_key
                || lane_raw_index(key).and_then(|raw| self.slot_by_raw.get(raw))
                    != Some(slot as u32)
            {
                return false;
            }
            if clean {
                let speed = match key {
                    TransportLaneKey::Belt { entity_id, .. } => entities
                        .transport_belts
                        .get(&entity_id)
                        .map(|state| state.speed_subtiles_per_tick),
                    TransportLaneKey::Splitter { entity_id, .. } => entities
                        .splitters
                        .get(&entity_id)
                        .map(|state| state.speed_subtiles_per_tick),
                };
                if speed != Some(lane.speed_subtiles_per_tick)
                    || self.resolve_downstream(entities, slot) != lane.downstream
                {
                    return false;
                }
            }
            for target in downstream_targets(lane.downstream) {
                if self
                    .lanes
                    .get(target.raw())
                    .is_none_or(|record| record.key.is_none())
                {
                    return false;
                }
                let upstreams = &mut expected_upstreams[target.raw()];
                if !upstreams.contains(&slot) {
                    upstreams.push(slot);
                }
            }
        }
        if clean
            && self.lanes.len() - vacant.len()
                != entities.transport_belts.len() * 2 + entities.splitters.len() * 4
        {
            return false;
        }
        for (raw, slot) in self.slot_by_raw.occupied_entries() {
            if self
                .lanes
                .get(slot as usize)
                .and_then(|lane| lane.key)
                .and_then(lane_raw_index)
                .and_then(|lane_raw| u64::try_from(lane_raw).ok())
                != Some(raw)
            {
                return false;
            }
        }
        if self.free_slots.len() != vacant.len()
            || self.free_slots.iter().any(|slot| !vacant.remove(slot))
        {
            return false;
        }
        for (actual, mut expected) in self.upstream_by_slot.iter().zip(expected_upstreams) {
            let mut actual = actual.iter().map(|slot| slot.raw()).collect::<Vec<_>>();
            actual.sort_unstable();
            expected.sort_unstable();
            if actual != expected {
                return false;
            }
        }
        let mut visited = vec![false; self.lanes.len()];
        for (run, record) in self.run_records.iter().enumerate() {
            let Some(end) = (record.start as usize).checked_add(record.len as usize) else {
                return false;
            };
            let Some(slots) = self.run_lane_slots.get(record.start as usize..end) else {
                return false;
            };
            for (position, &slot) in slots.iter().enumerate() {
                let Some(lane) = self.lanes.get(slot.raw()) else {
                    return false;
                };
                if lane.key.is_none()
                    || visited[slot.raw()]
                    || lane.run as usize != run
                    || lane.run_position as usize != position
                {
                    return false;
                }
                visited[slot.raw()] = true;
                if let Some(next) = slots.get(position + 1)
                    && self.chain_successor(slot.raw()) != Some(next.raw())
                {
                    return false;
                }
            }
            if let (Some(head), Some(tail)) = (slots.first(), slots.last())
                && record.cyclic != (self.chain_successor(tail.raw()) == Some(head.raw()))
            {
                return false;
            }
        }
        self.lanes
            .iter()
            .zip(visited)
            .all(|(lane, visited)| lane.key.is_some() == visited)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::{
        SaveLoadError, SimValidationError, Simulation, load_from_bytes, save_to_bytes,
    };

    #[test]
    fn malformed_transport_slots_runs_and_work_are_rejected_before_use() {
        let base = Simulation::new_test_world(293);
        for corruption in 0..4 {
            let mut sim = base.clone();
            match corruption {
                0 => sim.transport.graph.slot_by_raw.insert(0, 0),
                1 => sim.transport.graph.run_records.push(TransportRunRecord {
                    start: 1,
                    len: 1,
                    cyclic: false,
                }),
                2 => sim
                    .transport
                    .active_runs
                    .runs
                    .push(TransportRunIndex::from_index(1)),
                _ => sim.transport.dirty_regions.push(TransportDirtyRegion {
                    entity_id: EntityId::new(1),
                    footprint: crate::simulation::EntityFootprint {
                        x: i64::MAX,
                        y: 0,
                        width: i32::MAX,
                        height: i32::MAX,
                    },
                }),
            }
            assert!(matches!(
                load_from_bytes(&save_to_bytes(&sim).unwrap()),
                Err(SaveLoadError::InvalidSimulationState(
                    SimValidationError::InvalidTransportWork
                ))
            ));
        }
    }
}
