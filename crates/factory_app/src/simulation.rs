use bevy::prelude::*;

use crate::resources::{SimResource, UpsStats};

pub(crate) fn tick_sim(mut sim: ResMut<SimResource>, mut ups: ResMut<UpsStats>) {
    sim.sim.tick();
    ups.fixed_ticks += 1;
}
