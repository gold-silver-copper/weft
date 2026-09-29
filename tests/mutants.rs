//! Gate for C3, C4, C9 and C18: the gates catch real bugs.
//!
//! The library carries three planted bugs, each behind `--cfg weft_mutant="<name>"`. For each
//! one this test builds the library with the bug switched on and runs the gate that must
//! catch it. The gate has to *fail a test*: a build error doesn't count as a kill, and a gate
//! that still passes means the mutant survived.
//!
//! | mutant                     | the bug                                                  | killed by               |
//! |----------------------------|----------------------------------------------------------|-------------------------|
//! | `purge_keeps_edges`        | despawning keeps the edges that touch the despawned      | `fuzz::differential`    |
//! | `back_reads_forward_label` | a two-way edge read from its target shows the source's label | `fuzz::differential` |
//! | `cast_by_string`           | `cast` compares kind names instead of identity           | `claims::cast_matrix`   |

use std::path::PathBuf;
use std::process::Command;
use std::thread;

const MUTANTS: [(&str, &str, &str); 3] = [
    ("purge_keeps_edges", "fuzz", "differential"),
    ("back_reads_forward_label", "fuzz", "differential"),
    ("cast_by_string", "claims", "cast_matrix"),
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Runs `test_target::test_name` with `mutant` planted. `Ok` if a test failed.
fn kill(mutant: &str, test_target: &str, test_name: &str) -> Result<(), String> {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()))
        .current_dir(root())
        .args(["test", "--color=never", "-p", "weft", "--test", test_target, "--", "--exact", test_name])
        .env("CARGO_TARGET_DIR", root().join("target/gates").join(format!("mutant-{mutant}")))
        .env_remove("RUSTFLAGS")
        .env("CARGO_ENCODED_RUSTFLAGS", format!("--cfg\x1fweft_mutant=\"{mutant}\""))
        .output()
        .map_err(|e| format!("{mutant}: can't run cargo: {e}"))?;
    let output = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if out.status.success() {
        return Err(format!("{mutant} survived: `{test_target}::{test_name}` still passes"));
    }
    if !output.contains(&format!("test {test_name} ... FAILED")) {
        return Err(format!("{mutant}: `{test_target}::{test_name}` didn't fail as a test (a build error isn't a kill):\n{output}"));
    }
    Ok(())
}

#[test]
fn every_mutant_is_killed() {
    let runs: Vec<_> = MUTANTS
        .iter()
        .map(|&(mutant, target, test)| thread::spawn(move || kill(mutant, target, test)))
        .collect();
    let failures: Vec<String> = runs
        .into_iter()
        .map(|run| run.join().unwrap_or_else(|_| Err("a mutant run panicked".to_owned())))
        .filter_map(Result::err)
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
