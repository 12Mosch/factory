use bevy::prelude::Resource;
use factory_sim::{Simulation, SimulationTickProfile};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Resource)]
pub struct SimResource {
    inner: Arc<RwLock<Simulation>>,
}

pub type SimReadGuard<'a> = RwLockReadGuard<'a, Simulation>;
pub type SimWriteGuard<'a> = RwLockWriteGuard<'a, Simulation>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimAccessError {
    Poisoned,
    Busy,
}

impl SimResource {
    pub fn new(sim: Simulation) -> Self {
        Self {
            inner: Arc::new(RwLock::new(sim)),
        }
    }

    pub fn read(&self) -> SimReadGuard<'_> {
        self.inner.read().expect("simulation lock poisoned")
    }

    pub fn try_write(&self) -> Option<SimWriteGuard<'_>> {
        self.inner.try_write().ok()
    }

    pub fn write_for_tests(&mut self) -> SimWriteGuard<'_> {
        self.inner.write().expect("simulation lock poisoned")
    }

    pub fn replace(&self, sim: Simulation) -> Result<(), SimAccessError> {
        let mut guard = self.inner.try_write().map_err(|error| match error {
            std::sync::TryLockError::Poisoned(_) => SimAccessError::Poisoned,
            std::sync::TryLockError::WouldBlock => SimAccessError::Busy,
        })?;
        *guard = sim;
        Ok(())
    }

    pub(crate) fn clone_handle(&self) -> Arc<RwLock<Simulation>> {
        Arc::clone(&self.inner)
    }
}

#[derive(Resource, Default)]
pub(crate) struct UpsStats {
    pub(crate) elapsed: f64,
    pub(crate) fixed_ticks: u32,
    pub ups: f64,
}

#[derive(Resource, Default)]
pub struct SimProfileStats {
    pub last_tick: SimulationTickProfile,
    pub rolling_average_sim_tick_ms: f64,
    pub save_blocked_fixed_ticks: u64,
}
