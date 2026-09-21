# Save atomic commit and durability guarantees (issue #298)

At every interrupted save boundary, reopening finds either the previous
complete save or the new complete save. Files from different snapshot
generations are never combined.

This document formalizes the protocol implemented in
`crates/factory_app/src/save_load/commit.rs` (`SaveDurability`,
`CommitFaults`) and `container.rs` (`write_temporary_and_commit`), the
recovery in `catalog/recovery.rs`, and the status reporting in
`save_load.rs` and `ui/display.rs`.

## Commit protocol

1. Encode the complete save into a sibling temporary file
   `<name>.factsim.tmp-<nonce>`. Flush the buffer, `sync_all` the file,
   then sync the parent directory. The canonical path is untouched.
2. Claim the commit point with a single `ACTIVE -> COMMITTING`
   compare-exchange on the request cancel flag. A racing cancel wins and
   aborts without installing; once claimed, cancellation reports too late.
3. **Commit point (logical installation).** Atomically install the temp file:
   - Primary exists: preserve a rollback backup
     `<name>.factsim.bak-<nonce>`, then replace.
   - No primary: install with a no-replace primitive; on a race where a
     primary appeared, fall back to replacement.
4. **Durability barrier.** Sync the installed file and directory metadata
   (`sync_installed_file`).
5. **Post-commit cleanup.** Retire the backup by renaming it to
   `<backup>.retired` (so a cleanup crash can never leave it eligible for
   recovery), delete it, and sync the parent directory. Failures here never
   roll back the commit; the next catalog scan retries them.

The nonce (`pid-timestamp-counter`) makes concurrent attempts distinct; the
process-wide artifact lock serializes directory mutations across threads.

## Logical installation vs durability vs cleanup

- Logical installation succeeds at the rename in step 3. The new bytes are
  canonical from that moment, even if later barriers fail.
- The step-4 barrier affects only crash resilience. Its failure yields
  `SaveDurability::InstalledButUnsynced { reason }`: still a committed save,
  reported as `"<name> saved, but durability is degraded (...)"` with
  `Success` status — never an I/O error, and never an automatic overwrite
  retry (retrying would replace a committed save). Degraded commits skip
  further post-commit barriers during cleanup (files are still removed) so
  the verdict matches a state with no successful barrier after the rename;
  a later successful parent sync would otherwise make the rename durable
  after it was already classified as unsynced. Degraded durability is
  always reported, including implicit autosaves landing under an active
  error status.
- Step-5 cleanup is best-effort. Leftover backups or `.retired` markers are
  removed by the next recovery scan.

## Platform behavior

| Platform | Replacement | New-file install | Directory sync |
| --- | --- | --- | --- |
| Unix | Hard link (or copy + sync) for backup, `sync_parent`, then `rename` (atomic) | Hard link then no-clobber `renameat2`/`renameatx_np`, else hard link | `File::open(dir).sync_all()` |
| Windows | `ReplaceFileW` with backup (atomic, `WRITE_THROUGH` for new files via `MoveFileExW`) | `MoveFileExW` with `MOVEFILE_WRITE_THROUGH` | Directory handle with `FILE_FLAG_BACKUP_SEMANTICS`, then `sync_all` |
| Other | Copy + sync for backup, `sync_parent`, then `rename` | Hard link (no exclusive rename) | No-op |

When stronger durability is unavailable (filesystem without directory
`fsync`, `sync_all` returning `ENOSYS`/permission, locked handles on
Windows, or platforms with no portable barrier primitive), the save still
commits; status reports degraded durability and no retry is scheduled.
Pre-commit barrier failures (temp sync, pre-commit parent sync, backup)
instead fail as I/O with the previous save intact. Pre-commit directory
sync stays best-effort so saves proceed where no primitive exists; only
the post-commit verdict degrades.

## Single-process / single-writer ownership

`SAVE_ARTIFACT_LOCK` coordinates threads inside one process only. A save
root must have at most one writer process; concurrent game instances sharing
a root are unsupported. Beyond single-rename atomicity (last rename wins),
concurrent-writer behavior is undefined. Recovery preserves ambiguous
candidates rather than guessing (below), and catalog scans serialize with
the writer on the artifact lock so a scan never observes a mixture.

## Recovery

Runs on the catalog scan worker under the artifact lock, never on frame
schedules:

- Temporary (`*.tmp-*`) and `.retired` artifacts are always removed.
- Valid primary: all backups removed, primary kept.
- Missing or corrupt primary: each backup is fully validated (container +
  simulation payload, id/kind match). Corrupt backups are removed;
  temporarily unreadable or oversized ones are retained for a later scan.
  Exactly one valid candidate promotes; zero or several preserve everything
  instead of guessing. Duplicate identical backups are deduplicated.
- Intentional deletion removes the primary plus all active and retired
  artifacts under the lock and bumps the scan epoch, so an in-flight scan
  cannot resurrect the entry.

## Future indexed / incremental generations

Whole-snapshot saves stay self-contained. When record reuse ships, the
generation manifest commits last: write all reachable record blobs first,
atomically install the complete manifest, retain old reachable records
until that manifest commit is durable, never patch the active save in
place, and never delete a record referenced by another retained generation.
Recovery validates candidates before promotion and preserves ambiguous
candidates — the same rule whole-file recovery implements today.

## Fault coverage

Deterministic injection (`CommitFaults`, one failing `CommitFaultPhase` per
test) covers write, flush, temp sync, pre-commit parent sync, backup,
rename, durability barrier, retirement, and post-commit parent sync with
disk-full (`StorageFull`), permission/locked (`PermissionDenied`, matching
Windows sharing violations), and partial I/O (`Other`, leaving a flushed
partial prefix that cleanup must remove). Post-cleanup sync faults remove
files without issuing further barriers. The `save_crash_probe` binary
supplements this with subprocess crash states on Windows and Linux
(temp-pending, backup-with-primary, missing-primary-with-backup,
new-primary-old-backup, ambiguous): it reconstructs artifact states with
plain file copies rather than running the real commit in the child (which
would require shipping a crash hook in production); the fault matrix covers
the real boundaries in-process, while the probe covers what in-process
tests cannot — recovery driven by another OS process holding no shared
mutex or epoch state. Concurrent catalog scans during commits,
intentional-deletion resurrection, and cancellation preserving the only
recoverable backup are covered alongside the existing interruption-phase
and ambiguous-backup suites.
