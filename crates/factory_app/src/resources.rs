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

    /// Creates an initialized resource containing an active simulation.
    pub fn new(sim: Simulation) -> Self {
        Self {
            inner: Some(Arc::new(RwLock::new(sim))),
            replacement_revision: 0,
        }
    }

    /// Returns whether a world has been started or loaded.
    pub fn is_initialized(&self) -> bool {
        self.inner.is_some()
    }

    /// Locks the active simulation for reading.
    ///
    /// Panics when called before world entry or after lock poisoning.
    pub fn read(&self) -> SimReadGuard<'_> {
        self.inner
            .as_ref()
            .expect("simulation accessed before a world was started or loaded")
            .read()
            .expect("simulation lock poisoned")
    }

    /// Tries to lock the active simulation without blocking.
    ///
    /// Returns `None` when no world exists or the lock is unavailable.
    pub fn try_write(&self) -> Option<SimWriteGuard<'_>> {
        self.inner.as_ref()?.try_write().ok()
    }

    /// Locks the active simulation for test setup, blocking until it is available.
    pub fn write_for_tests(&mut self) -> SimWriteGuard<'_> {
        self.inner
            .as_ref()
            .expect("simulation accessed before a world was started or loaded")
            .write()
            .expect("simulation lock poisoned")
    }

    /// Installs the first world or replaces the active world without blocking a save reader.
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

    /// Returns the wrapping revision incremented after every successful world installation.
    pub(crate) fn replacement_revision(&self) -> u64 {
        self.replacement_revision
    }

    /// Clones the active simulation handle for background save serialization.
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
