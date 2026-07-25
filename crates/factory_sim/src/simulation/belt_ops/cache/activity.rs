use super::PATCH_STORAGE_HEADROOM;
use super::graph::TransportLaneGraph;
use crate::simulation::EntityStore;
use crate::simulation::belt_ops::types::{
    TransportLaneKey, TransportRunIndex, TransportRunTraversalStep,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation::belt_ops) struct TransportRunVisitSlot {
    pub(in crate::simulation::belt_ops) generation: u32,
    pub(in crate::simulation::belt_ops) state: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportRunVisitStorage {
    pub(in crate::simulation::belt_ops) generation: u32,
    pub(in crate::simulation::belt_ops) states: Vec<TransportRunVisitSlot>,
    pub(in crate::simulation::belt_ops) traversal_stack: Vec<TransportRunTraversalStep>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
struct TransportRunActiveSlot {
    active_generation: u32,
    pending_generation: u32,
    active_start_position: u32,
    pending_start_position: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportRunActiveStorage {
    active_generation: u32,
    pending_generation: u32,
    /// Current belt-phase work queue. After `finish_tick`, this becomes the
    /// next tick's queue and may receive producer/pickup wakeups via
    /// `mark_active` until the next belt phase begins.
    pub(in crate::simulation::belt_ops) runs: Vec<TransportRunIndex>,
    pending_runs: Vec<TransportRunIndex>,
    marks: Vec<TransportRunActiveSlot>,
}

#[derive(Clone, Copy)]
enum TransportRunQueue {
    Active,
    Pending,
}

impl TransportRunVisitStorage {
    pub(in crate::simulation) fn begin_tick(&mut self, required_len: usize) {
        if self.states.len() < required_len {
            self.states.reserve(
                required_len
                    .saturating_sub(self.states.len())
                    .saturating_add(PATCH_STORAGE_HEADROOM),
            );
            self.states
                .resize(required_len, TransportRunVisitSlot::default());
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.states.fill(TransportRunVisitSlot::default());
            self.generation = 1;
        }
    }
}

impl TransportRunActiveStorage {
    pub(super) fn rebuild_from_entities(
        &mut self,
        entities: &EntityStore,
        graph: &TransportLaneGraph,
    ) {
        advance_active_generation(&mut self.active_generation, &mut self.marks);
        self.runs.clear();

        let required_len = graph.run_count();
        if self.marks.len() < required_len {
            self.marks.reserve(
                required_len
                    .saturating_sub(self.marks.len())
                    .saturating_add(PATCH_STORAGE_HEADROOM),
            );
            self.marks
                .resize(required_len, TransportRunActiveSlot::default());
        }

        for (&entity_id, segment) in &entities.transport_belts {
            for (lane_index, lane) in segment.lanes.iter().enumerate() {
                if !lane.items.is_empty() {
                    let key = TransportLaneKey::Belt {
                        entity_id,
                        lane_index,
                    };
                    if let Some(index) = graph.slot_for(key)
                        && let Some((run, position)) = graph.run_and_position_for_slot(index)
                    {
                        self.mark_active(run, position);
                    }
                }
            }
        }

        for (&entity_id, state) in &entities.splitters {
            for (input_port, input_lanes) in state.input_lanes.iter().enumerate() {
                for (lane_index, lane) in input_lanes.iter().enumerate() {
                    if !lane.items.is_empty() {
                        let key = TransportLaneKey::Splitter {
                            entity_id,
                            input_port,
                            lane_index,
                        };
                        if let Some(index) = graph.slot_for(key)
                            && let Some((run, position)) = graph.run_and_position_for_slot(index)
                        {
                            self.mark_active(run, position);
                        }
                    }
                }
            }
        }
    }

    pub(in crate::simulation) fn begin_tick(&mut self, required_len: usize) {
        if self.marks.len() < required_len {
            self.marks.reserve(
                required_len
                    .saturating_sub(self.marks.len())
                    .saturating_add(PATCH_STORAGE_HEADROOM),
            );
            self.marks
                .resize(required_len, TransportRunActiveSlot::default());
        }
        advance_pending_generation(&mut self.pending_generation, &mut self.marks);
        self.pending_runs.clear();
    }

    pub(in crate::simulation) fn finish_tick(&mut self) {
        advance_active_generation(&mut self.active_generation, &mut self.marks);

        self.runs.clear();
        self.runs.reserve(self.pending_runs.len());
        let mut pending_runs = std::mem::take(&mut self.pending_runs);
        for run in pending_runs.drain(..) {
            let start_position = self.marks[run.raw()].pending_start_position as usize;
            self.mark_active(run, start_position);
        }
        self.pending_runs = pending_runs;
    }

    pub(in crate::simulation::belt_ops) fn active_start_position(
        &self,
        run: TransportRunIndex,
    ) -> usize {
        self.marks
            .get(run.raw())
            .filter(|mark| mark.active_generation == self.active_generation)
            .map_or(0, |mark| mark.active_start_position as usize)
    }

    pub(in crate::simulation::belt_ops) fn mark_pending(
        &mut self,
        run: TransportRunIndex,
        start_position: usize,
    ) {
        mark_active_run(
            &mut self.marks,
            self.pending_generation,
            run,
            start_position,
            &mut self.pending_runs,
            TransportRunQueue::Pending,
        );
    }

    pub(super) fn mark_active(&mut self, run: TransportRunIndex, start_position: usize) {
        mark_active_run(
            &mut self.marks,
            self.active_generation,
            run,
            start_position,
            &mut self.runs,
            TransportRunQueue::Active,
        );
    }
}

fn advance_active_generation(generation: &mut u32, marks: &mut [TransportRunActiveSlot]) {
    advance_generation(generation, marks, |mark| {
        mark.active_generation = 0;
    });
}

fn advance_pending_generation(generation: &mut u32, marks: &mut [TransportRunActiveSlot]) {
    advance_generation(generation, marks, |mark| {
        mark.pending_generation = 0;
    });
}

fn advance_generation(
    generation: &mut u32,
    marks: &mut [TransportRunActiveSlot],
    reset_mark: impl Fn(&mut TransportRunActiveSlot),
) {
    *generation = generation.wrapping_add(1);
    if *generation == 0 {
        for mark in marks {
            reset_mark(mark);
        }
        *generation = 1;
    }
}

fn mark_active_run(
    marks: &mut Vec<TransportRunActiveSlot>,
    generation: u32,
    index: TransportRunIndex,
    start_position: usize,
    runs: &mut Vec<TransportRunIndex>,
    queue: TransportRunQueue,
) {
    if marks.len() <= index.raw() {
        marks.resize(index.raw() + 1, TransportRunActiveSlot::default());
    }
    let Some(mark) = marks.get_mut(index.raw()) else {
        return;
    };
    let start_position =
        u32::try_from(start_position).expect("transport run position capacity exceeded");
    let (current_generation, current_start_position) = match queue {
        TransportRunQueue::Active => (mark.active_generation, mark.active_start_position),
        TransportRunQueue::Pending => (mark.pending_generation, mark.pending_start_position),
    };
    if current_generation == generation {
        if start_position < current_start_position {
            match queue {
                TransportRunQueue::Active => mark.active_start_position = start_position,
                TransportRunQueue::Pending => mark.pending_start_position = start_position,
            }
        }
        return;
    }
    match queue {
        TransportRunQueue::Active => {
            mark.active_generation = generation;
            mark.active_start_position = start_position;
        }
        TransportRunQueue::Pending => {
            mark.pending_generation = generation;
            mark.pending_start_position = start_position;
        }
    }
    runs.push(index);
}
