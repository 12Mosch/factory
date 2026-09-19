use bevy::prelude::Resource;
use factory_sim::{Simulation, SimulationTickProfile};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;

#[derive(Resource)]
pub struct SimResource {
    inner: Option<Arc<RwLock<Simulation>>>,
    replacement_revision: u64,
    generation: Arc<AtomicU64>,
    active_snapshot_captures: Arc<AtomicU64>,
    snapshot_blocked_fixed_ticks: Arc<AtomicU64>,
}

pub type SimReadGuard<'a> = RwLockReadGuard<'a, Simulation>;
pub type SimWriteGuard<'a> = RwLockWriteGuard<'a, Simulation>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimAccessError {
    Poisoned,
    Busy,
}

/// A failed atomic installation that returns the uninstalled candidate, so
/// async callers can retain and retry it instead of losing a validated world.
/// The simulation is boxed to keep the `Err` variant small.
#[derive(Debug)]
pub struct InstallConflict {
    pub cause: SimAccessError,
    pub simulation: Box<Simulation>,
}

impl SimResource {
    /// Creates the explicit pre-game state, before a world has been started or loaded.
    pub fn empty() -> Self {
        Self {
            inner: None,
            replacement_revision: 0,
            generation: Arc::new(AtomicU64::new(0)),
            active_snapshot_captures: Arc::new(AtomicU64::new(0)),
            snapshot_blocked_fixed_ticks: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Creates an initialized resource containing an active simulation.
    pub fn new(sim: Simulation) -> Self {
        Self {
            inner: Some(Arc::new(RwLock::new(sim))),
            replacement_revision: 0,
            generation: Arc::new(AtomicU64::new(0)),
            active_snapshot_captures: Arc::new(AtomicU64::new(0)),
            snapshot_blocked_fixed_ticks: Arc::new(AtomicU64::new(0)),
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

    /// Installs the first world or replaces the active world in one atomic
    /// step: the single `try_write` both swaps the simulation and publishes
    /// the new generation before the guard is released. A save worker that
    /// acquires the read lock therefore always observes a consistent
    /// (world, generation) pair, never a new world tagged with the previous
    /// generation. On contention the candidate is returned with the error so
    /// async callers can retain and retry it instead of dropping a validated
    /// world.
    pub fn install(&mut self, sim: Simulation) -> Result<(), InstallConflict> {
        if let Some(inner) = &self.inner {
            let mut guard = match inner.try_write() {
                Ok(guard) => guard,
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(InstallConflict {
                        cause: SimAccessError::Poisoned,
                        simulation: Box::new(sim),
                    });
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    return Err(InstallConflict {
                        cause: SimAccessError::Busy,
                        simulation: Box::new(sim),
                    });
                }
            };
            *guard = sim;
            self.replacement_revision = self.replacement_revision.wrapping_add(1);
            self.generation
                .store(self.replacement_revision, Ordering::Release);
        } else {
            self.inner = Some(Arc::new(RwLock::new(sim)));
            self.replacement_revision = self.replacement_revision.wrapping_add(1);
            self.generation
                .store(self.replacement_revision, Ordering::Release);
        }
        Ok(())
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
        self.install(sim).map_err(|conflict| conflict.cause)
    }

    /// Returns the wrapping revision incremented after every successful world installation.
    pub(crate) fn replacement_revision(&self) -> u64 {
        self.replacement_revision
    }

    /// Clones the active simulation handle for background snapshot capture.
    pub(crate) fn clone_handle(&self) -> Arc<RwLock<Simulation>> {
        Arc::clone(
            self.inner
                .as_ref()
                .expect("simulation accessed before a world was started or loaded"),
        )
    }

    /// Pins the simulation handle and the requested world generation.
    /// Workers compare the live generation after acquiring the read lock and
    /// discard the request as stale when a different world was installed
    /// while it waited, so a snapshot never mixes tick identity across worlds.
    pub(crate) fn snapshot_source(&self) -> SnapshotSource {
        SnapshotSource {
            simulation: self.clone_handle(),
            generation: Arc::clone(&self.generation),
            active_captures: Arc::clone(&self.active_snapshot_captures),
            blocked_fixed_ticks: Arc::clone(&self.snapshot_blocked_fixed_ticks),
        }
    }

    pub(crate) fn note_snapshot_blocked_fixed_tick(&self) {
        if self.active_snapshot_captures.load(Ordering::Acquire) > 0 {
            self.snapshot_blocked_fixed_ticks
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub(crate) struct SnapshotSource {
    pub(crate) simulation: Arc<RwLock<Simulation>>,
    pub(crate) generation: Arc<AtomicU64>,
    pub(crate) active_captures: Arc<AtomicU64>,
    pub(crate) blocked_fixed_ticks: Arc<AtomicU64>,
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

/// Runtime telemetry for the fixed-step catch-up policy.
///
/// A capped frame intentionally loses wall time rather than retaining an
/// unbounded simulation backlog. These counters make that degradation visible
/// in the debug overlay and available to headless diagnostics.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct FixedStepCatchUpStats {
    /// Simulation ticks executed during the current rendered frame.
    pub fixed_ticks_this_frame: u32,
    /// Highest number of simulation ticks observed in one rendered frame.
    pub peak_fixed_ticks_per_frame: u32,
    /// Rendered frames whose wall-clock delta exceeded the catch-up ceiling.
    pub capped_frames: u64,
    /// Wall time discarded from the current rendered frame.
    pub dropped_time_this_frame: Duration,
    /// Total wall time discarded since application startup.
    pub total_dropped_time: Duration,
}
