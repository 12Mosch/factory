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
/// Sending a train to a rail is two presses' worth of information — which
/// train, and which rail — and the cursor can only name one of them at a time,
/// so the first press has to be remembered somewhere. Frame-side state rather
/// than simulation state: it is a half-finished input, not something the world
/// knows about, and a save that remembered it would be remembering a keystroke.
#[derive(Resource, Default)]
pub struct TrainRoutingSelection {
    pub train: Option<factory_sim::TrainId>,
}

/// Debug train keys pressed since the fixed step last looked.
///
/// A key press is an edge, and an edge belongs to a frame rather than to a
/// fixed step: a dropped frame runs several fixed steps while `just_pressed`
/// stays true for all of them, so a press read straight from the keyboard in
/// `FixedUpdate` fires once, twice, or not at all depending on the frame rate.
/// One press is one action, so the edge is collected in the frame schedule and
/// the fixed step consumes it — a train picked up by a stutter and put straight
/// back down is not what the player asked for.
#[derive(Resource, Default)]
pub struct TrainDebugInput {
    pub drive: bool,
    pub brake: bool,
    pub route: bool,
}

impl TrainDebugInput {
    /// Takes the driving keys, leaving them consumed.
    ///
    /// Consumed whether or not the step that took them finds anything under the
    /// cursor to act on: a press that found nothing is a press that happened,
    /// and holding it back would fire it later at a cursor that has moved.
    pub fn take_driving(&mut self) -> (bool, bool) {
        (
            std::mem::take(&mut self.drive),
            std::mem::take(&mut self.brake),
        )
    }

    /// Takes the routing key, leaving it consumed.
    pub fn take_routing(&mut self) -> bool {
        std::mem::take(&mut self.route)
    }
}
