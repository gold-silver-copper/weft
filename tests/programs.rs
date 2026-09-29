//! Gate for C3 and C16: the frozen programs P1–P4, P6 and P7 run to completion and print
//! what their comments promise. (P5 is the `game` crate: `cargo test -p game` runs it.)
//!
//! A program that panics exits non-zero, so every `assert!` in it is part of this gate, and so
//! is every `world.check()?` (P3 checks after each of its turns).

use std::path::PathBuf;
use std::process::Command;

/// Each program, and the lines its comments promise, in the order they must appear.
const PROGRAMS: [(&str, &[&str]); 6] = [
    ("p1", &[
        "Hall",
        "A draughty hall.",
        "  down: Cellar",
        "  north: Garden",
        "You enter the Cellar.",
        "Cellar",
        "It smells of apples.",
        "  up: Hall",
        "  tunnel: Garden",
        "  There is a lamp here.",
        "  There is a rug here.",
        "You carry: lamp",
    ]),
    ("p2", &["baker: 8", "witch: 5", "miller: 2", "baker (likes 8, knows 9)", "witch (likes 5, knows 3)"]),
    ("p3", &["Room#0 Room { name: \"hall\" }", "  In -> Room#0"]),
    ("p4", &["You give the lamp to the smith."]),
    ("p6", &["You arrive in the pit.", "You arrive in the pit."]),
    ("p7", &["You pick up the lamp."]),
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn every_program_runs() {
    let target = root().join("target/gates/programs");
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()))
        .current_dir(root())
        .args(["build", "--color=never", "--examples"])
        .env("CARGO_TARGET_DIR", &target)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .output()
        .unwrap_or_else(|e| panic!("can't run cargo: {e}"));
    assert!(out.status.success(), "the examples must build:\n{}", String::from_utf8_lossy(&out.stderr));

    let mut failures = Vec::new();
    for (name, promised) in PROGRAMS {
        let bin = target.join("debug/examples").join(name);
        let out = Command::new(&bin).output().unwrap_or_else(|e| panic!("{}: {e}", bin.display()));
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() {
            failures.push(format!("{name} exited with {}:\n{stdout}{}", out.status, String::from_utf8_lossy(&out.stderr)));
            continue;
        }
        let mut lines = stdout.lines();
        for line in promised {
            if !lines.any(|l| l == *line) {
                failures.push(format!("{name}: {line:?} is missing, or out of order, in:\n{stdout}"));
                break;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
