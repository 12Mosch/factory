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

/// The train the debug routing key is about to send somewhere.
///
/// Sending a train to a rail is two clicks' worth of information — which train,
/// and which rail — and the cursor can only name one of them at a time, so the
/// first press has to be remembered somewhere. Frame-side state rather than
/// simulation state: it is a half-finished input, not something the world knows
/// about, and a save that remembered it would be remembering a keystroke.
#[derive(Resource, Default)]
pub struct TrainRoutingSelection {
    pub train: Option<factory_sim::TrainId>,
}
