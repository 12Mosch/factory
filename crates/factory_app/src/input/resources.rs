use bevy::prelude::Resource;

#[derive(Resource, Default)]
pub struct AppInputState {
    pub world_blocked: bool,
    pub escape_consumed: bool,
}
