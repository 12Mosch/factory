use bevy::prelude::Resource;
use factory_sim::WorldTileCoord;
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

/// Manual driving presses collected since the fixed step last looked, in the
/// order they were made.
///
/// A key press is an edge, and an edge belongs to a frame rather than to a fixed
/// step: a dropped frame runs several fixed steps while `just_pressed` stays
/// true for all of them, and a fast one runs several frames before any fixed
/// step at all. So the presses are collected in the frame schedule and the fixed
/// step consumes them — a train pulling away twice because a frame was dropped
/// is not what the player asked for.
///
/// A queue rather than a flag per key, because both the order and the repeats
/// matter: two presses of the drive key are two changes of direction, and drive
/// followed by brake is not the same instruction as brake followed by drive.
/// Each press carries the tile it was made over for the same reason it carries
/// its order — where the player was pointing is part of what they pressed, and
/// re-reading the cursor when the fixed step gets round to it would answer with
/// wherever the mouse has since moved to.
#[derive(Resource, Default)]
pub struct TrainManualInput {
    pending: VecDeque<TrainManualPress>,
}

/// One press of a manual driving key, and where it was aimed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrainManualPress {
    pub key: TrainManualKey,
    pub tile: (WorldTileCoord, WorldTileCoord),
}

/// Which manual driving key was pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrainManualKey {
    /// Pull away, or turn around if the train is already driving.
    Drive,
    Brake,
}

impl TrainManualInput {
    /// Presses held for a fixed step that has not run yet.
    ///
    /// Deeper than the handful of presses a player can make between two fixed
    /// steps, and shallow enough that a simulation held up for a long time — a
    /// load, a hitch — comes back to a keystroke or two rather than to a minute
    /// of them replayed at once.
    const CAPACITY: usize = 8;

    pub fn push(&mut self, key: TrainManualKey, tile: (WorldTileCoord, WorldTileCoord)) {
        if self.pending.len() < Self::CAPACITY {
            self.pending.push_back(TrainManualPress { key, tile });
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
    pub fn drain(&mut self) -> impl Iterator<Item = TrainManualPress> + '_ {
        self.pending.drain(..)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Presses come back in the order they went in, repeats and all, each still
    /// aimed where it was made — the whole reason this is a queue of presses
    /// rather than a flag per key.
    #[test]
    fn presses_are_kept_in_order_with_their_tiles_and_capped() {
        let mut input = TrainManualInput::default();
        assert!(input.is_empty());

        input.push(TrainManualKey::Brake, (1, 2));
        input.push(TrainManualKey::Drive, (3, 4));
        input.push(TrainManualKey::Drive, (3, 4));
        assert_eq!(
            input.drain().collect::<Vec<_>>(),
            vec![
                TrainManualPress {
                    key: TrainManualKey::Brake,
                    tile: (1, 2)
                },
                TrainManualPress {
                    key: TrainManualKey::Drive,
                    tile: (3, 4)
                },
                TrainManualPress {
                    key: TrainManualKey::Drive,
                    tile: (3, 4)
                },
            ]
        );
        assert!(input.is_empty(), "draining leaves nothing behind");

        for _ in 0..64 {
            input.push(TrainManualKey::Drive, (0, 0));
        }
        assert_eq!(input.drain().count(), TrainManualInput::CAPACITY);
    }
}
