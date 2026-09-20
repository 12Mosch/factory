//! Atomic commit and durability guarantees for save installation.
//!
//! This module formalizes the protocol that [`super::container`] implements:
//! temporary-file encoding, platform-specific replacement with a rollback
//! backup, backup retirement, and directory synchronization. It also owns the
//! injectable fault boundary used by deterministic tests and the durability
//! result that distinguishes logical installation from durability barriers.
//!
//! # Protocol
//!
//! 1. Encode the complete save into a sibling temporary file
//!    (`<name>.factsim.tmp-<nonce>`), flush the buffer, `sync_all` the file,
//!    then sync the parent directory. No canonical path is touched.
//! 2. Claim the commit point with a single `ACTIVE -> COMMITTING`
//!    compare-exchange on the request cancel flag. A racing cancel wins here
//!    and aborts without installing; once claimed, cancellation is too late.
//! 3. **Commit point**: atomically install the temporary file as the
//!    canonical save. When a primary already exists, first preserve a
//!    rollback backup (`<name>.factsim.bak-<nonce>`), then replace:
//!    Unix uses a hard link (or copy) for the backup followed by `rename`;
//!    Windows uses `ReplaceFileW` (or `MoveFileExW` with `WRITE_THROUGH` for
//!    new files); other platforms copy then rename. When no primary exists,
//!    install with a no-replace primitive (hard link plus no-clobber rename
//!    on Unix, `MoveFileExW` on Windows) and retry as a replacement if a
//!    primary appeared concurrently.
//! 4. **Durability barrier**: flush the installed file and directory metadata
//!    (`sync_installed_file`). This barrier may fail while the installation
//!    itself has already committed.
//! 5. **Post-commit cleanup**: retire the backup (rename to
//!    `<backup>.retired` so a cleanup crash can never leave it eligible for
//!    recovery, then delete) and sync the parent directory. Cleanup failures
//!    never roll back a committed save; the next catalog scan retries them.
//!
//! # Logical installation vs durability vs cleanup
//!
//! * Logical installation succeeds at the rename in step 3. From that moment
//!   the new bytes are the canonical save, even if later barriers fail.
//! * The durability barrier in step 4 only affects crash resilience, not
//!   visibility. Its failure is reported as
//!   [`SaveDurability::InstalledButUnsynced`] — still a committed save, never
//!   an I/O error that would invite an unsafe overwrite retry.
//! * Cleanup in step 5 is best-effort. Leftover backups or retired markers
//!   are removed by the next recovery scan.
//!
//! # Single-process / single-writer ownership
//!
//! `SAVE_ARTIFACT_LOCK` serializes directory mutations across threads inside
//! this process (save workers, catalog scans, deletions). It cannot
//! coordinate two game instances: concurrent writers sharing one save root
//! are unsupported. Each save root must have at most one writer process;
//! readers (catalog scans) run in the same process. Behavior under
//! concurrent writers is undefined beyond the atomicity of a single rename:
//! the last rename wins, and recovery preserves ambiguous candidates rather
//! than guessing.
//!
//! # Future indexed / incremental generations
//!
//! Whole-snapshot saves remain self-contained files. When incremental record
//! reuse ships, the generation manifest must commit last: write all reachable
//! record blobs first, then atomically install the complete manifest, and
//! retain old reachable records until that manifest commit is durable. Never
//! patch the active save in place and never delete a record referenced by
//! another retained generation. Recovery validates candidates before
//! promotion and preserves ambiguous candidates rather than guessing — the
//! same rule the whole-file recovery in `catalog::recovery` implements
//! today (exactly one valid backup promotes; zero or several preserve).
//!
//! # Platform guarantees
//!
//! See `docs/save-atomic-commit.md` for the supported filesystem table and
//! for the precise degraded behavior when stronger durability (file or
//! directory `fsync`, `ReplaceFileW`, no-replace rename) is unavailable.

use std::io;

/// Durability outcome of a committed save.
///
/// Logical installation (the rename) and the durability barrier (post-rename
/// file and directory sync) are distinct. A committed save with a failed
/// barrier is still the canonical save; it is only less resilient to an OS
/// crash or power loss.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SaveDurability {
    /// File and directory metadata were flushed after installation.
    Durable,
    /// The rename committed but a post-rename sync failed. Carries the
    /// barrier error for status reporting. Must not be reported as a fully
    /// durable success, and must not trigger an overwrite retry: retrying
    /// would replace a committed save.
    InstalledButUnsynced { reason: String },
}

impl SaveDurability {
    /// Human-readable barrier failure, if any.
    pub(crate) fn degraded_reason(&self) -> Option<&str> {
        match self {
            Self::Durable => None,
            Self::InstalledButUnsynced { reason } => Some(reason),
        }
    }

    /// Builds the degraded variant from any barrier error.
    pub(crate) fn unsynced(error: impl std::fmt::Display) -> Self {
        Self::InstalledButUnsynced {
            reason: error.to_string(),
        }
    }
}

impl std::fmt::Display for SaveDurability {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Durable => write!(formatter, "durable"),
            Self::InstalledButUnsynced { reason } => {
                write!(formatter, "installed but not synced: {reason}")
            }
        }
    }
}

/// One interruptible phase of the commit protocol, in execution order.
///
/// Each variant names the filesystem side effect whose failure is injected.
/// `SyncInstalled` is special: it names the post-commit durability barrier,
/// which degrades durability instead of failing the save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommitFaultPhase {
    /// Creating the save root directory.
    CreateDir,
    /// Creating the sibling temporary file with `create_new`.
    CreateTemp,
    /// Failing after the encoder ran but before the flush (partial write).
    Write,
    /// Flushing the buffered encoder.
    Flush,
    /// Syncing the temporary file before installation.
    SyncTemp,
    /// Syncing the parent directory before the commit point.
    SyncParentPre,
    /// Preserving the rollback backup of the previous primary.
    Backup,
    /// The atomic rename that makes the new save canonical.
    Rename,
    /// The post-commit durability barrier (file plus directory sync).
    SyncInstalled,
    /// Retiring the rollback backup after a replacing commit.
    Retire,
    /// Syncing the parent directory after post-commit cleanup.
    SyncParentPost,
}

/// Deterministic fault injection for the commit protocol.
///
/// Production passes [`CommitFaults::none`]; tests pass one failing phase to
/// prove the recovery invariant at every interrupted boundary. The error kind
/// selects the simulated condition: `StorageFull` for disk-full,
/// `PermissionDenied` for permission or locked files, `Other` for partial
/// I/O, and so on. At most one phase fails per instance so each test names
/// the exact boundary it exercises; combine tests for the full matrix.
#[derive(Clone, Debug)]
pub(crate) struct CommitFaults {
    fail_at: Option<CommitFaultPhase>,
    error_kind: io::ErrorKind,
    message: &'static str,
}

impl Default for CommitFaults {
    fn default() -> Self {
        Self::none()
    }
}

impl CommitFaults {
    /// No injected failures (production behavior).
    pub(crate) fn none() -> Self {
        Self {
            fail_at: None,
            error_kind: io::ErrorKind::Other,
            message: "injected commit fault",
        }
    }

    /// Fails `phase` with `kind` (disk-full, permission, locked, partial).
    #[cfg(test)]
    pub(crate) fn fail_at(phase: CommitFaultPhase, kind: io::ErrorKind) -> Self {
        Self {
            fail_at: Some(phase),
            error_kind: kind,
            message: "injected commit fault",
        }
    }

    /// Whether `phase` is the injected failure.
    pub(crate) fn fails_at(&self, phase: CommitFaultPhase) -> bool {
        self.fail_at == Some(phase)
    }

    /// Returns the injected error for `phase`, if any.
    pub(crate) fn check(&self, phase: CommitFaultPhase) -> io::Result<()> {
        if self.fails_at(phase) {
            Err(io::Error::new(self.error_kind, self.message))
        } else {
            Ok(())
        }
    }

    /// Builds the injected barrier error for the durability phase, if any.
    pub(crate) fn sync_barrier_error(&self) -> Option<io::Error> {
        if self.fails_at(CommitFaultPhase::SyncInstalled) {
            Some(io::Error::new(self.error_kind, self.message))
        } else {
            None
        }
    }
}

/// Formats the committed-save status without hiding degraded durability.
///
/// A degraded barrier is still a commit (never an error, never a retry), but
/// its message must say so explicitly instead of claiming full durability.
pub(crate) fn format_save_success(display_name: &str, durability: &SaveDurability) -> String {
    match durability {
        SaveDurability::Durable => format!("{display_name} saved."),
        SaveDurability::InstalledButUnsynced { reason } => format!(
            "{display_name} saved, but durability is degraded ({reason}); the new save is active."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durability_reports_degraded_instead_of_silent_success() {
        let durable = SaveDurability::Durable;
        assert_eq!(durable.degraded_reason(), None);
        assert_eq!(
            format_save_success("Quicksave", &durable),
            "Quicksave saved."
        );

        let degraded = SaveDurability::unsynced("injected commit fault");
        assert_eq!(degraded.degraded_reason(), Some("injected commit fault"));
        let message = format_save_success("Quicksave", &degraded);
        assert!(
            message.contains("durability is degraded"),
            "degraded durability must not read as fully durable success: {message}"
        );
        assert!(
            message.contains("Quicksave saved"),
            "degraded durability is still a commit, not an error: {message}"
        );
    }

    #[test]
    fn fault_plan_fires_once_at_the_named_phase() {
        let faults = CommitFaults::fail_at(CommitFaultPhase::Rename, io::ErrorKind::StorageFull);
        assert!(faults.check(CommitFaultPhase::Backup).is_ok());
        let error = faults
            .check(CommitFaultPhase::Rename)
            .expect_err("rename fault must fire");
        assert_eq!(error.kind(), io::ErrorKind::StorageFull);
        assert!(faults.sync_barrier_error().is_none());

        let barrier = CommitFaults::fail_at(
            CommitFaultPhase::SyncInstalled,
            io::ErrorKind::PermissionDenied,
        );
        assert!(barrier.check(CommitFaultPhase::Rename).is_ok());
        let barrier_error = barrier
            .sync_barrier_error()
            .expect("barrier fault must surface as durability");
        assert_eq!(barrier_error.kind(), io::ErrorKind::PermissionDenied);
    }
}
