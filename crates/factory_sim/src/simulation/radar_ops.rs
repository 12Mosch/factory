use super::*;

impl Simulation {
    pub(super) fn advance_radars(&mut self) {
        let mut radars = std::mem::take(&mut self.entities.radars);
        for (&entity_id, state) in &mut radars {
            let Some((center, metadata)) = self.radar_scan_inputs(entity_id) else {
                continue;
            };

            // One grant advances both clocks, preserving their exact ratio
            // under partial power through the shared permyriad remainder.
            if !electric_work_allowed_for(
                &self.power,
                &mut self.entities.electric_consumers,
                entity_id,
            ) {
                continue;
            }

            state.nearby_scan_progress_ticks += 1;
            state.far_scan_progress_ticks += 1;

            if state.nearby_scan_progress_ticks >= metadata.nearby_scan_interval_ticks {
                state.nearby_scan_progress_ticks = 0;
                self.complete_nearby_radar_scan(center, metadata.nearby_reveal_radius_chunks);
            }
            if state.far_scan_progress_ticks >= metadata.far_scan_interval_ticks {
                state.far_scan_progress_ticks = 0;
                if !state.far_scan_complete {
                    state.far_scan_complete = self.complete_far_radar_scan(center, metadata, state);
                }
            }
        }
        self.entities.radars = radars;
    }

    fn radar_scan_inputs(
        &self,
        entity_id: EntityId,
    ) -> Option<(ChunkCoord, factory_data::RadarPrototype)> {
        let placed = self.entities.placed_entity(entity_id)?;
        let prototype = self.world.prototypes.entity(placed.prototype_id)?;
        let metadata = prototype.radar?;
        let center_x = placed
            .footprint
            .x
            .checked_add(i64::from(placed.footprint.width / 2))?;
        let center_y = placed
            .footprint
            .y
            .checked_add(i64::from(placed.footprint.height / 2))?;
        Some((ChunkCoord::from_tile(center_x, center_y)?, metadata))
    }

    fn complete_nearby_radar_scan(&mut self, center: ChunkCoord, radius: u16) {
        let radius = i32::from(radius);
        let mut generated = Vec::new();
        for y_offset in -radius..=radius {
            let Some(y) = center.y.checked_add(y_offset) else {
                continue;
            };
            for x_offset in -radius..=radius {
                let Some(x) = center.x.checked_add(x_offset) else {
                    continue;
                };
                let coord = ChunkCoord { x, y };
                if self.world.chunks.contains_key(&coord) {
                    generated.push(coord);
                } else {
                    self.request_chunk_generation(coord, ChunkGenerationPriority::RadarReveal);
                }
            }
        }
        self.reveal_generated_chunks(&generated);
    }

    fn complete_far_radar_scan(
        &mut self,
        center: ChunkCoord,
        metadata: factory_data::RadarPrototype,
        state: &mut RadarState,
    ) -> bool {
        let candidate_count = crate::radar::far_scan_candidate_count(
            metadata.nearby_reveal_radius_chunks,
            metadata.far_scan_radius_chunks,
        );
        for visited in 0..candidate_count {
            let cursor = (state.far_scan_cursor + visited) % candidate_count;
            let Some(offset) = far_scan_offset(
                metadata.nearby_reveal_radius_chunks,
                metadata.far_scan_radius_chunks,
                cursor,
            ) else {
                continue;
            };
            let (Some(x), Some(y)) = (
                center.x.checked_add(offset.x),
                center.y.checked_add(offset.y),
            ) else {
                continue;
            };
            let coord = ChunkCoord { x, y };
            if self.chart.revealed_chunks.contains(&coord)
                || self.chunk_generation_queue.radar_reveal.contains(&coord)
            {
                continue;
            }

            state.far_scan_cursor = (cursor + 1) % candidate_count;
            if self.world.chunks.contains_key(&coord) {
                self.reveal_generated_chunks(&[coord]);
            } else {
                self.request_chunk_generation(coord, ChunkGenerationPriority::RadarReveal);
            }
            return false;
        }
        true
    }
}

/// Maps a durable sweep cursor to a clockwise ring coordinate. Each ring
/// begins at its top-left corner and the sweep proceeds from inner to outer.
fn far_scan_offset(nearby_radius: u16, far_radius: u16, mut cursor: u64) -> Option<ChunkCoord> {
    for radius in u64::from(nearby_radius) + 1..=u64::from(far_radius) {
        let ring_len = radius * 8;
        if cursor >= ring_len {
            cursor -= ring_len;
            continue;
        }

        let radius = i32::try_from(radius).expect("u16 radar radius always fits in i32");
        let side_len = radius as u64 * 2;
        let ring_offset =
            |offset| i32::try_from(offset).expect("radar ring offset always fits in i32");
        let offset = if cursor < side_len {
            ChunkCoord {
                x: -radius + ring_offset(cursor),
                y: radius,
            }
        } else if cursor < side_len * 2 {
            ChunkCoord {
                x: radius,
                y: radius - ring_offset(cursor - side_len),
            }
        } else if cursor < side_len * 3 {
            ChunkCoord {
                x: radius - ring_offset(cursor - side_len * 2),
                y: -radius,
            }
        } else {
            ChunkCoord {
                x: -radius,
                y: -radius + ring_offset(cursor - side_len * 3),
            }
        };
        return Some(offset);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn nearby_radius_three_contains_49_coordinates() {
        let coords = (-3..=3)
            .flat_map(|y| (-3..=3).map(move |x| ChunkCoord { x, y }))
            .collect::<BTreeSet<_>>();
        assert_eq!(coords.len(), 49);
    }

    #[test]
    fn far_sweep_is_unique_clockwise_and_wraps() {
        let count = crate::radar::far_scan_candidate_count(3, 14);
        assert_eq!(count, 792);
        let offsets = (0..count)
            .map(|cursor| far_scan_offset(3, 14, cursor).expect("valid cursor"))
            .collect::<Vec<_>>();
        assert_eq!(offsets[0], ChunkCoord { x: -4, y: 4 });
        assert_eq!(offsets[1], ChunkCoord { x: -3, y: 4 });
        assert_eq!(offsets[8], ChunkCoord { x: 4, y: 4 });
        assert_eq!(offsets[16], ChunkCoord { x: 4, y: -4 });
        assert_eq!(offsets[24], ChunkCoord { x: -4, y: -4 });
        assert_eq!(offsets.iter().copied().collect::<BTreeSet<_>>().len(), 792);
        assert!(
            offsets
                .iter()
                .all(|offset| (4..=14).contains(&offset.x.abs().max(offset.y.abs())))
        );
        assert!(far_scan_offset(3, 14, count).is_none());
    }

    #[test]
    fn checked_scan_coordinates_do_not_overflow_world_edges() {
        let center = ChunkCoord {
            x: i32::MAX,
            y: i32::MIN,
        };
        for cursor in 0..crate::radar::far_scan_candidate_count(3, 14) {
            let offset = far_scan_offset(3, 14, cursor).expect("valid cursor");
            let _ = center.x.checked_add(offset.x);
            let _ = center.y.checked_add(offset.y);
        }
    }
}
