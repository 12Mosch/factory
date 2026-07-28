use bevy::prelude::Resource;

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
