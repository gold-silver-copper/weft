//! `tests/deviations.rs`: after M0, nothing changes without a deviation that pins it.
//!
//! M0 copies P1–P8 out of the decision record, copies its claims table into
//! `docs/design/0001-ecs/claims.md`, writes every gate the table names, records the
//! fingerprint of each of those files in `docs/design/0001-ecs/baseline.txt` (one
//! `<path> <fnv64>` per line), and tags `gates-m0`. From then on the executor never stops.
//! When a program won't compile, a gate fails, or it wants to depart from the design, it makes
//! the smallest change that keeps every claim and appends an entry to
//! `docs/design/0001-ecs-deviations.md` that pins the fingerprint each changed file has
//! *after* the change. For example:
//!
//!     ## D3: P6's `play` spells out a rank bound
//!     - Kind: deviation
//!     - Files: examples/p6.rs
//!     - Pins: examples/p6.rs 9f2c41d07a1be3e8
//!     - Claims: C14
//!     - Why: one line on what forced it
//!     - Evidence: the compiler output, pasted in a fence under the entry
//!
//! A later, unlogged edit to the same file moves its fingerprint off the last pin, and this
//! test fails. A deleted gate fails too. `Kind: shortfall` marks a claim whose gate still
//! fails at the end; the final report lists every shortfall.

use std::fs;
use std::path::Path;

const RECORD: &str = "docs/design/0001-ecs.md";
const LOG: &str = "docs/design/0001-ecs-deviations.md";
const BASELINE: &str = "docs/design/0001-ecs/baseline.txt";
const CLAIMS: &str = "docs/design/0001-ecs/claims.md";
const REPORT: &str = "docs/design/0001-ecs-report.md";

/// Where each frozen program lives in the tree.
const PROGRAMS: [(&str, &str); 8] = [
    ("P1", "examples/p1.rs"),
    ("P2", "examples/p2.rs"),
    ("P3", "examples/p3.rs"),
    ("P4", "examples/p4.rs"),
    ("P5", "game/src/lib.rs"),
    ("P6", "examples/p6.rs"),
    ("P7", "examples/p7.rs"),
    ("P8", "tests/deviations.rs"),
];

/// Every entry fills in each of these. `-` stands for none.
const FIELDS: [&str; 6] = ["Kind", "Files", "Pins", "Claims", "Why", "Evidence"];

fn read(path: &str) -> String {
    let full = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()))
}

fn normalize(text: &str) -> String {
    text.lines().map(str::trim_end).collect::<Vec<_>>().join("\n").trim().to_owned()
}

/// FNV-1a, 64 bits, over the normalized text.
fn fingerprint(text: &str) -> String {
    let hash = normalize(text)
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3));
    format!("{hash:016x}")
}

/// The fenced block under each `### P<n>: …` heading of the record's `## Frozen API`.
fn frozen(record: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let (mut in_api, mut program, mut fence) = (false, None::<String>, None::<String>);
    let mut body = String::new();
    for line in record.lines() {
        if let Some(close) = &fence {
            if line.trim_end() == close {
                if let Some(p) = program.take() {
                    out.push((p, std::mem::take(&mut body)));
                }
                fence = None;
            } else {
                body.push_str(line);
                body.push('\n');
            }
        } else if line.starts_with("## ") {
            in_api = line.trim_end() == "## Frozen API";
        } else if let Some(heading) = line.strip_prefix("### ").filter(|_| in_api) {
            program = heading.split(':').next().map(str::to_owned);
        } else if in_api && program.is_some() && line.starts_with("```") {
            fence = Some(line.chars().take_while(|&c| c == '`').collect());
        }
    }
    out
}

/// `(path, fingerprint)` for every file M0 froze.
fn baseline() -> Vec<(String, String)> {
    read(BASELINE)
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(path, hash)| (path.to_owned(), hash.trim().to_owned()))
        .collect()
}

/// `(id, fields)` for every `## D<n>: <title>` entry of the log.
fn entries(log: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in log.lines() {
        if let Some(rest) = line.strip_prefix("## D") {
            out.push((format!("D{}", rest.split(':').next().unwrap_or_default()), Vec::new()));
        } else if let Some((_, fields)) = out.last_mut() {
            if let Some((key, value)) = line.strip_prefix("- ").and_then(|l| l.split_once(": ")) {
                fields.push((key.to_owned(), value.trim().to_owned()));
            }
        }
    }
    out
}

fn field<'a>(fields: &'a [(String, String)], name: &str) -> &'a str {
    fields.iter().find(|(k, _)| k == name).map_or("", |(_, v)| v.as_str())
}

/// `(path, fingerprint)` for every pin in an entry.
fn pins(fields: &[(String, String)]) -> Vec<(&str, &str)> {
    field(fields, "Pins").split(", ").filter_map(|pin| pin.split_once(' ')).collect()
}

#[test]
fn the_baseline_is_the_record() {
    let frozen = frozen(&read(RECORD));
    let baseline = baseline();
    assert_eq!(frozen.len(), PROGRAMS.len(), "the record must freeze P1–P8");
    for ((name, text), (program, path)) in frozen.iter().zip(PROGRAMS) {
        assert_eq!(name, program, "the record's programs are out of order");
        let pinned = baseline.iter().find(|(p, _)| p == path).map(|(_, h)| h.as_str());
        assert_eq!(pinned, Some(fingerprint(text).as_str()), "{path}: the M0 baseline isn't the record's {program}");
    }
}

#[test]
fn nothing_changed_without_a_pin() {
    let log = entries(&read(LOG));
    for (path, then) in baseline() {
        let now = fingerprint(&read(&path));
        if now == then {
            continue;
        }
        let pinned = log
            .iter()
            .rev()
            .flat_map(|(_, fields)| pins(fields))
            .find(|(p, _)| *p == path)
            .map(|(_, hash)| hash);
        assert_eq!(
            pinned,
            Some(now.as_str()),
            "{path} changed since M0, and the last deviation naming it doesn't pin its current fingerprint {now}"
        );
    }
}

#[test]
fn every_entry_is_complete() {
    for (n, (id, fields)) in entries(&read(LOG)).iter().enumerate() {
        assert_eq!(*id, format!("D{}", n + 1), "entries run D1, D2, … with no gaps");
        for name in FIELDS {
            assert!(!field(fields, name).is_empty(), "{id}: no `- {name}:`");
        }
        let kind = field(fields, "Kind");
        assert!(matches!(kind, "deviation" | "shortfall"), "{id}: `Kind` is {kind:?}");
        let pinned = pins(fields);
        for file in field(fields, "Files").split(", ").filter(|f| *f != "-") {
            assert!(pinned.iter().any(|(p, _)| *p == file), "{id}: {file} is listed but not pinned");
        }
    }
}

#[test]
fn every_record_claim_is_kept_or_logged() {
    let (record, claims) = (read(RECORD), read(CLAIMS));
    let log = entries(&read(LOG));
    for row in record.lines().filter(|l| l.starts_with("| C")) {
        let id = row.split('|').nth(1).map(str::trim).unwrap_or_default();
        let kept = claims.lines().any(|l| l == row);
        let logged = log.iter().any(|(_, f)| field(f, "Claims").split(", ").any(|c| c == id));
        assert!(kept || logged, "{id} changed or vanished from the claims register and no deviation names it");
    }
}

#[test]
fn every_claim_names_gates_that_exist() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let claims = read(CLAIMS);
    for row in claims.lines().filter(|l| l.starts_with("| C")) {
        let cells: Vec<&str> = row.split('|').map(str::trim).collect();
        let [_, id, claim, gates, fails, ..] = cells.as_slice() else { panic!("malformed claim row: {row}") };
        assert!(!fails.is_empty(), "{id} ({claim}) doesn't say when its gates fail");
        for gate in gates.split(", ").map(|g| g.trim_matches('`')) {
            let exists = if gate.starts_with("cargo ") || gate.starts_with("rust-analyzer ") {
                true
            } else if let Some((file, name)) = gate.split_once("::") {
                read(file).contains(&format!("fn {name}("))
            } else {
                root.join(gate).exists()
            };
            assert!(exists, "{id}: gate `{gate}` doesn't exist");
        }
    }
}

#[test]
fn every_shortfall_is_in_the_report() {
    let shortfalls: Vec<String> = entries(&read(LOG))
        .into_iter()
        .filter(|(_, f)| field(f, "Kind") == "shortfall")
        .map(|(id, _)| id)
        .collect();
    if shortfalls.is_empty() {
        return;
    }
    let report = read(REPORT);
    for id in shortfalls {
        assert!(report.contains(&format!("{id}:")), "shortfall {id} is missing from the final report");
    }
}
