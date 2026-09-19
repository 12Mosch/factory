//! Bounded asynchronous persistence lifecycle.
//!
//! One explicit lifecycle contract covers save and load jobs: shared phases
//! and errors, frame schedules that only enqueue requests and collect
//! finished workers, and world installation at a controlled application
//! boundary ([`crate::save_load::enter_swapped_world`]) through the shared
//! presentation/input reset. Queue transitions stay separate per direction
//! by design — saves serialize FIFO while loads are newest-wins — so each
//! lives with its queue in `jobs` and `loads` instead of a single merged
//! machine. Snapshot capture, encoding, disk I/O, file inspection,
//! recovery, decoding, and validation run on bounded background workers.
//!
//! # Bounds
//!
//! * Save workers: at most [`MAX_SAVE_WORKERS`] running. Only the running job
//!   retains an immutable snapshot generation ([`MAX_RETAINED_SAVE_GENERATIONS`]).
//!   Queued requests retain only parameters, never snapshot memory.
//! * Queued saves: at most [`MAX_QUEUED_SAVES`]. Manual (explicit) saves are
//!   FIFO; autosaves coalesce by target and apply backpressure by dropping.
//! * Load workers: at most [`MAX_LOAD_WORKERS`] running with at most
//!   [`MAX_QUEUED_LOADS`] queued. A newer load request supersedes a queued
//!   (not yet started) load; a running load installs only when still current.
//! * Catalog validation workers: at most one (see `catalog::validation`).
//!
//! # Ordering
//!
//! * Saves to the same target serialize in FIFO request order, including
//!   repeated named overwrites of one save.
//! * Each save captures exactly one completed tick of the requested world.
//!   The admission tick is recorded lock-free for observability, but a
//!   queued request captures the latest completed tick of that world when
//!   its worker starts, so accepted FIFO saves keep working while the
//!   simulation ticks. A request queued while a different world is installed
//!   is discarded as stale instead of capturing another world's bytes, and
//!   the captured tick is reported in the outcome, so requested
//!   completed-tick identity is never mixed across worlds.
//! * Each load result carries its request id and the world generation observed
//!   when its worker started. A result installs only when its request is still
//!   the newest and no newer world installation happened after the worker
//!   started. Out-of-order or superseded completions are discarded, never
//!   installed over a newer world.
//! * Each load candidate also records the target's writer epoch and file
//!   identity before its worker opens the file. A save committed afterwards,
//!   or an external replacement of the path, no longer matches the decoded
//!   instance, so the request restarts and converges on the current bytes
//!   instead of installing a rollback.
//!
//! # Shutdown and cancellation
//!
//! Queued (not yet started) jobs are cancelled on shutdown and never reported
//! as committed. A running save is joined so an accepted first save cannot be
//! abandoned; a pre-commit cancellation is reported as cancelled, never as
//! committed. Post-commit cleanup failures never roll back a committed save.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// Save cancellation lifecycle for the atomic commit gate. The worker claims
/// the commit point with a single compare-exchange, so either cancellation
/// wins (the worker aborts before the rename) or the worker wins (a later
/// `cancel` observes `COMMITTING` and cannot report successful cancellation
/// for a save that is about to commit).
pub const SAVE_CANCEL_ACTIVE: u8 = 0;
/// Cancellation was requested before the commit point; the worker aborts.
pub const SAVE_CANCEL_REQUESTED: u8 = 1;
/// The worker claimed the commit point; cancellation is too late.
pub const SAVE_CANCEL_COMMITTING: u8 = 2;

/// At most one background save encoder at a time.
pub const MAX_SAVE_WORKERS: usize = 1;
/// At most one immutable snapshot generation retained by a running save.
pub const MAX_RETAINED_SAVE_GENERATIONS: usize = 1;
/// Bounded FIFO for accepted manual saves waiting for the save worker.
pub const MAX_QUEUED_SAVES: usize = 4;
/// At most one background load decoder at a time.
pub const MAX_LOAD_WORKERS: usize = 1;
/// Bounded queue for load requests waiting for the load worker. Only the
/// newest queued load is retained; accepting a newer request immediately
/// supersedes an older queued load and any retained older candidate.
pub const MAX_QUEUED_LOADS: usize = 1;

static NEXT_PERSISTENCE_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Monotonic id tagging one accepted persistence request. Results carry the
/// id of the request that produced them so stale or out-of-order completions
/// can be discarded instead of installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PersistenceRequestId(pub u64);

impl PersistenceRequestId {
    pub fn next() -> Self {
        Self(NEXT_PERSISTENCE_REQUEST_ID.fetch_add(1, Ordering::Relaxed))
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

impl fmt::Display for PersistenceRequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "#{}", self.0)
    }
}

/// Observable save progress phase. Frame code reads this through atomics;
/// workers advance it at phase boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveJobPhase {
    Queued,
    Capturing,
    Encoding,
    Writing,
    Committing,
}

impl SaveJobPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Capturing => "capturing snapshot",
            Self::Encoding => "encoding",
            Self::Writing => "writing",
            Self::Committing => "committing",
        }
    }

    pub(crate) fn encode(self) -> u8 {
        match self {
            Self::Queued => 0,
            Self::Capturing => 1,
            Self::Encoding => 2,
            Self::Writing => 3,
            Self::Committing => 4,
        }
    }

    pub(crate) fn decode(value: u8) -> Self {
        match value {
            1 => Self::Capturing,
            2 => Self::Encoding,
            3 => Self::Writing,
            4 => Self::Committing,
            _ => Self::Queued,
        }
    }
}

/// Observable load progress phase. Decoding includes the validation that
/// the decode paths run internally before returning a candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadJobPhase {
    Queued,
    Reading,
    Decoding,
    ReadyToInstall,
}

impl LoadJobPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Reading => "reading",
            Self::Decoding => "decoding and validating",
            Self::ReadyToInstall => "ready to install",
        }
    }

    pub(crate) fn encode(self) -> u8 {
        match self {
            Self::Queued => 0,
            Self::Reading => 1,
            Self::Decoding => 2,
            Self::ReadyToInstall => 3,
        }
    }

    pub(crate) fn decode(value: u8) -> Self {
        match value {
            1 => Self::Reading,
            2 => Self::Decoding,
            3 => Self::ReadyToInstall,
            _ => Self::Queued,
        }
    }
}

/// Actionable save failure. Pre-commit cancellation is distinct from a
/// committed save; post-commit cleanup failures never surface as errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveJobError {
    /// World exceeds the snapshot capture budget; previous save is intact.
    CaptureBudget,
    /// Simulation lock poisoned; restart is the actionable recovery.
    LockPoisoned,
    /// Snapshot capture failed before any byte was written.
    CaptureFailed(String),
    /// Encoding failed before any byte was committed.
    Encode(String),
    /// Disk I/O failed before commit; previous save is intact.
    Io(String),
    /// Cancelled before commit; never reported as committed.
    Cancelled,
    /// A newer world was installed while waiting; result discarded.
    Stale,
}

impl fmt::Display for SaveJobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CaptureBudget => {
                write!(
                    formatter,
                    "save exceeds this build's snapshot capture budget"
                )
            }
            Self::LockPoisoned => write!(formatter, "simulation lock poisoned"),
            Self::CaptureFailed(detail) => {
                write!(formatter, "snapshot capture failed: {detail}")
            }
            Self::Encode(detail) => write!(formatter, "{detail}"),
            Self::Io(detail) => write!(formatter, "save I/O failed: {detail}"),
            Self::Cancelled => write!(formatter, "save cancelled before commit"),
            Self::Stale => write!(formatter, "save superseded by a newer world"),
        }
    }
}

/// Actionable load failure. Transient I/O stays retryable and is never
/// reported as corruption.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadJobError {
    /// Save is no longer in the catalog.
    NotFound,
    /// Save is recognized but this build cannot load it; carries the reason.
    Incompatible(String),
    /// Observed bytes are malformed or truncated.
    Corrupt(String),
    /// Payload exceeds this build's limits.
    TooLarge,
    /// File is momentarily unreadable (lock, mid-sync replacement). Retry
    /// instead of treating the save as corrupt.
    TransientIo(String),
    /// Other I/O failure while reading the file.
    Io(String),
    /// Cancelled before installation; the active world is untouched.
    Cancelled,
    /// Superseded by a newer load or world installation; safely discarded.
    Stale,
}

impl fmt::Display for LoadJobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(formatter, "save is no longer in the catalog"),
            Self::Incompatible(reason) => write!(formatter, "{reason}"),
            Self::Corrupt(detail) => {
                write!(formatter, "save file is corrupt or incomplete: {detail}")
            }
            Self::TooLarge => write!(
                formatter,
                "save exceeds this build's save size or collection limits"
            ),
            Self::TransientIo(detail) => {
                write!(
                    formatter,
                    "save is temporarily unreadable ({detail}); try again"
                )
            }
            Self::Io(detail) => write!(formatter, "save I/O failed: {detail}"),
            Self::Cancelled => write!(formatter, "load cancelled before installation"),
            Self::Stale => write!(formatter, "load superseded by a newer world"),
        }
    }
}
