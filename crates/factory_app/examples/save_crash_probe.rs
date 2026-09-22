//! Subprocess crash probe for save-recovery tests.
//!
//! Simulates a game process crashing at one interrupted save boundary by
//! leaving crash artifacts on disk with no cleanup, then exiting uncleanly.
//! The parent test runs the real catalog recovery afterwards and asserts the
//! recovery invariant (previous or new complete save, never a mixture).
//!
//! Uses only stdlib file operations so the probe builds portably on Windows
//! and Linux with no game dependencies. Arguments:
//!
//! `save_crash_probe <root> <mode>`
//!
//! The root must contain `quicksave.factsim` (old primary, except the
//! missing-primary mode prepares it), `new-staging.factsim` (valid new
//! generation), and `old-staging.factsim` (copy of the old generation).
//! Modes:
//!
//! * `temp-pending` — new bytes flushed to a temp artifact, install not begun.
//! * `backup-with-primary` — rollback backup exists, primary intact.
//! * `missing-primary-with-backup` — backup exists, primary deleted
//!   (crash between backup and install).
//! * `new-primary-old-backup` — new primary installed, old backup not retired
//!   (crash after commit, before cleanup).
//! * `ambiguous` — corrupt primary with two different valid backups (no
//!   guessing: recovery must preserve both).

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: save_crash_probe <root> <mode>");
        return ExitCode::from(2);
    }
    let root = PathBuf::from(&args[1]);
    let mode = args[2].as_str();
    let primary = root.join("quicksave.factsim");
    let new_staging = root.join("new-staging.factsim");
    let old_staging = root.join("old-staging.factsim");
    let temp = root.join("quicksave.factsim.tmp-probe-1");
    let backup_one = root.join("quicksave.factsim.bak-probe-1");
    let backup_two = root.join("quicksave.factsim.bak-probe-2");

    let result = match mode {
        "temp-pending" => fs::copy(&new_staging, &temp).map(|_| ()),
        "backup-with-primary" => fs::copy(&primary, &backup_one).map(|_| ()),
        "missing-primary-with-backup" => {
            fs::copy(&primary, &backup_one).and_then(|_| fs::remove_file(&primary))
        }
        "new-primary-old-backup" => fs::copy(&new_staging, &primary)
            .and_then(|_| fs::copy(&old_staging, &backup_one).map(|_| ())),
        "ambiguous" => fs::write(&primary, b"corrupt primary")
            .and_then(|_| fs::copy(&old_staging, &backup_one).map(|_| ()))
            .and_then(|_| fs::copy(&new_staging, &backup_two).map(|_| ())),
        _ => {
            eprintln!("unknown crash mode: {mode}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) = result {
        eprintln!("crash probe failed for mode {mode}: {error}");
        return ExitCode::from(1);
    }
    // Exit uncleanly with no cleanup, exactly like a crashed game process.
    // The parent test owns recovery.
    std::process::exit(1);
}
