use bevy::prelude::Resource;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub struct AppInputState {
    pub world_blocked: bool,
    pub escape_consumed: bool,
}

/// Whether the rail connectivity overlay is being drawn.
///
/// Off by default: it answers a question a player only asks while laying track,
/// and the answer is drawn on top of the track itself.
#[derive(Resource, Default)]
pub struct RailGraphOverlay {
    pub enabled: bool,
}

/// The train the debug routing key is about to send somewhere.
///
/// Sending a train to a rail is two presses' worth of information — which
/// train, and which rail — and the cursor can only name one of them at a time,
/// so the first press has to be remembered somewhere. Frame-side state rather
/// than simulation state: it is a half-finished input, not something the world
/// knows about, and a save that remembered it would be remembering a keystroke.
#[derive(Resource, Default)]
pub struct TrainRoutingSelection {
    pub train: Option<factory_sim::TrainId>,
}

/// Debug train keys pressed since the fixed step last looked, in the order they
/// were pressed.
///
/// A key press is an edge, and an edge belongs to a frame rather than to a fixed
/// step: a dropped frame runs several fixed steps while `just_pressed` stays
/// true for all of them, and a fast one runs several frames before any fixed
/// step at all. So the edges are collected in the frame schedule and the fixed
/// step consumes them — a train picked up by a stutter and put straight back
/// down is not what the player asked for.
///
/// A queue rather than a flag per key, because both the order and the repeats
/// matter: two presses of the drive key are two changes of direction, and drive
/// followed by brake is not the same instruction as brake followed by drive.
#[derive(Resource, Default)]
pub struct TrainDebugInput {
    pending: VecDeque<TrainDebugPress>,
}

/// One press of a debug train key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrainDebugPress {
    /// Pull away, or turn around if the train is already driving.
    Drive,
    Brake,
    /// Pick a train up, or put it down where the cursor is pointing.
    Route,
}

impl TrainDebugInput {
    /// Presses held for a fixed step that has not run yet.
    ///
    /// Deeper than the handful of presses a player can make between two fixed
    /// steps, and shallow enough that a simulation held up for a long time — a
    /// load, a hitch — comes back to a keystroke or two rather than to a minute
    /// of them replayed at once.
    const CAPACITY: usize = 8;

    pub fn push(&mut self, press: TrainDebugPress) {
        if self.pending.len() < Self::CAPACITY {
            self.pending.push_back(press);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Forgets every press held. What the player pressed at was the world, and
    /// a press that can no longer mean what it meant is dropped rather than
    /// acted on later against something else.
    pub fn clear(&mut self) {
        self.pending.clear();
    }

    /// Takes the presses waiting, oldest first.
    pub fn drain(&mut self) -> impl Iterator<Item = TrainDebugPress> + '_ {
        self.pending.drain(..)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Presses come back in the order they went in, repeats and all — the whole
    /// reason this is a queue.
    #[test]
    fn presses_are_kept_in_order_and_capped() {
        let mut input = TrainDebugInput::default();
        assert!(input.is_empty());

        input.push(TrainDebugPress::Drive);
        input.push(TrainDebugPress::Drive);
        input.push(TrainDebugPress::Brake);
        assert_eq!(
            input.drain().collect::<Vec<_>>(),
            vec![
                TrainDebugPress::Drive,
                TrainDebugPress::Drive,
                TrainDebugPress::Brake
            ]
        );
        assert!(input.is_empty(), "draining leaves nothing behind");

        for _ in 0..64 {
            input.push(TrainDebugPress::Route);
        }
        assert_eq!(input.drain().count(), TrainDebugInput::CAPACITY);
    }
}
