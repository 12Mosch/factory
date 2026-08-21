use bevy::prelude::Resource;
use factory_sim::{Simulation, SimulationTickProfile};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Resource)]
pub struct SimResource {
    inner: Option<Arc<RwLock<Simulation>>>,
    replacement_revision: u64,
}

pub type SimReadGuard<'a> = RwLockReadGuard<'a, Simulation>;
pub type SimWriteGuard<'a> = RwLockWriteGuard<'a, Simulation>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimAccessError {
    Poisoned,
    Busy,
}

impl SimResource {
    /// Creates the explicit pre-game state, before a world has been started or loaded.
    pub fn empty() -> Self {
        Self {
            inner: None,
            replacement_revision: 0,
        }
    }

    pub fn new(sim: Simulation) -> Self {
        Self {
            inner: Some(Arc::new(RwLock::new(sim))),
            replacement_revision: 0,
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.inner.is_some()
    }

    pub fn read(&self) -> SimReadGuard<'_> {
        self.inner
            .as_ref()
            .expect("simulation accessed before a world was started or loaded")
            .read()
            .expect("simulation lock poisoned")
    }

    pub fn try_write(&self) -> Option<SimWriteGuard<'_>> {
        self.inner.as_ref()?.try_write().ok()
    }

    pub fn write_for_tests(&mut self) -> SimWriteGuard<'_> {
        self.inner
            .as_ref()
            .expect("simulation accessed before a world was started or loaded")
            .write()
            .expect("simulation lock poisoned")
    }

    pub fn replace(&mut self, sim: Simulation) -> Result<(), SimAccessError> {
        if let Some(inner) = &self.inner {
            let mut guard = inner.try_write().map_err(|error| match error {
                std::sync::TryLockError::Poisoned(_) => SimAccessError::Poisoned,
                std::sync::TryLockError::WouldBlock => SimAccessError::Busy,
            })?;
            *guard = sim;
        } else {
            self.inner = Some(Arc::new(RwLock::new(sim)));
        }
        self.replacement_revision = self.replacement_revision.wrapping_add(1);
        Ok(())
    }

    pub(crate) fn replacement_revision(&self) -> u64 {
        self.replacement_revision
    }

    pub(crate) fn clone_handle(&self) -> Arc<RwLock<Simulation>> {
        Arc::clone(
            self.inner
                .as_ref()
                .expect("simulation accessed before a world was started or loaded"),
        )
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
