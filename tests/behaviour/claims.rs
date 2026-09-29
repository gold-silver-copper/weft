//! Gates for C9 (`cast` compares identity), C15 (lint wall, no dependencies, clean source)
//! and C17 (dropping a world drops everything it spawned).
#![feature(decl_macro)]

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

use weft::*;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ------------------------------------------------------------------ C9: cast

#[kind(root)]
pub struct Settings;

#[kind]
pub struct Room;

set!(pub Spirit);

/// Two kinds with the same name, in different modules.
pub mod a {
    use super::Spirit;
    use weft::*;

    #[kind]
    pub struct Ghost {
        pub n: u32,
    }
    is!(Ghost: Spirit);
}

pub mod b {
    use super::Spirit;
    use weft::*;

    #[kind]
    pub struct Ghost {
        pub n: u32,
    }
    is!(Ghost: Spirit);
}

#[relation(Spirit -> Room, many, detach)]
pub struct Haunts;

module!(pub Haunting { kinds: [Room, a::Ghost, b::Ghost], relations: [Haunts] });
game!(pub Manor: Settings [Haunting]);

#[test]
fn cast_matrix() {
    let mut world = World::<Manor>::new(|t| {
        let hall = t.spawn(Room, ());
        for n in 0..3 {
            let ghost = t.spawn(a::Ghost { n }, ());
            t.relate(ghost, Haunts, hall);
            let ghost = t.spawn(b::Ghost { n: n + 10 }, ());
            t.relate(ghost, Haunts, hall);
        }
        t.spawn(Settings, ())
    });
    world.turn(|t| {
        let hall = t.all::<Room>().next().expect("the hall");
        let spirits: Vec<Id<'_, Spirit>> = t.sources::<Haunts>(hall).map(|(s, _)| s).collect();
        assert_eq!(spirits.len(), 6);
        let (mut from_a, mut from_b) = (Vec::new(), Vec::new());
        for &spirit in &spirits {
            match (t.cast::<a::Ghost>(spirit), t.cast::<b::Ghost>(spirit)) {
                (Some(ghost), None) => {
                    assert!(ghost.up::<Spirit>() == spirit, "cast found another node than the one it was given");
                    from_a.push(t[ghost].n);
                }
                (None, Some(ghost)) => {
                    assert!(ghost.up::<Spirit>() == spirit, "cast found another node than the one it was given");
                    from_b.push(t[ghost].n);
                }
                (x, y) => panic!("a spirit must be exactly one kind of ghost, got {x:?} and {y:?}"),
            }
        }
        from_a.sort_unstable();
        from_b.sort_unstable();
        assert_eq!(from_a, [0, 1, 2]);
        assert_eq!(from_b, [10, 11, 12]);
        // Straight from a kind handle, too: an `a::Ghost` is never a `b::Ghost`.
        for ghost in t.all::<a::Ghost>() {
            assert!(t.cast::<b::Ghost>(ghost.up::<Spirit>()).is_none());
            assert!(t.cast::<a::Ghost>(ghost.up::<Spirit>()) == Some(ghost));
        }
    });
}

// ------------------------------------------------------------------ C15: the source

/// The lint block of `src/lib.rs` at `af02049`, which must survive byte for byte.
const WALL: &str = "#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![forbid(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::string_slice,
    clippy::get_unwrap,
    clippy::unwrap_in_result,
    clippy::panic_in_result_fn,
    clippy::exit,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types
)]
";

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn lint_wall_is_intact() {
    let lib = read(&root().join("src/lib.rs"));
    assert!(lib.contains(WALL), "src/lib.rs no longer carries af02049's lint block byte for byte");
    for (line, text) in lib.lines().enumerate() {
        if text.trim_start().starts_with("#![") && !WALL.contains(text) {
            let allowed = ["#![feature(decl_macro)]"];
            assert!(allowed.contains(&text.trim()), "src/lib.rs:{}: unexpected crate attribute `{text}`", line + 1);
        }
    }
}

/// The crates of this repository.
const OURS: [&str; 5] = ["weft", "weft-macros", "game", "reject", "accept"];

/// The third-party crates the owner approved (deviation D7): the proc-macro toolchain
/// `weft-macros` is built on, and what it pulls in. Nothing else, and none at runtime.
const APPROVED: [&str; 9] =
    ["syn", "quote", "proc-macro2", "unicode-ident", "darling", "darling_core", "darling_macro", "ident_case", "strsim"];

/// The `[dependencies]` table of a manifest, as its lines.
fn dependencies(manifest: &str) -> Vec<String> {
    let table = manifest.split("\n[dependencies]\n").nth(1).unwrap_or_default();
    let table = table.split("\n[").next().unwrap_or_default();
    table.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_owned).collect()
}

#[test]
fn no_third_party_crates() {
    for lock in ["Cargo.lock", "tests/reject/Cargo.lock", "tests/accept/Cargo.lock"] {
        let path = root().join(lock);
        if !path.exists() {
            continue;
        }
        for package in read(&path).split("[[package]]").skip(1) {
            let field = |key: &str| {
                package.lines().find_map(|l| l.strip_prefix(key).and_then(|v| v.strip_prefix(" = \"")).map(|v| v.trim_end_matches('"')))
            };
            let name = field("name").unwrap_or_default();
            match field("source") {
                None => assert!(OURS.contains(&name), "{lock}: `{name}` is neither ours nor from a registry"),
                Some(source) => {
                    assert!(APPROVED.contains(&name), "{lock} lists a third-party crate that wasn't approved: {name}");
                    assert!(source.starts_with("registry+"), "{lock}: `{name}` comes from {source}, not crates.io");
                }
            }
        }
    }
    let weft = dependencies(&read(&root().join("Cargo.toml")));
    assert!(
        weft.len() == 1 && weft.iter().all(|d| d.starts_with("weft-macros = { path = ")),
        "weft depends on nothing but `weft-macros`, found: {weft:?}"
    );
    for dep in dependencies(&read(&root().join("weft-macros/Cargo.toml"))) {
        let name = dep.split([' ', '=']).next().unwrap_or_default();
        assert!(APPROVED.contains(&name), "weft-macros depends on a crate that wasn't approved: {dep}");
    }
}

#[test]
fn source_is_clean() {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut dirs = vec![root().join("src"), root().join("weft-macros/src")];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())).filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    assert!(!files.is_empty());
    for file in files {
        let text = read(&file).replace("#![forbid(unsafe_code)]", "");
        for banned in ["unsafe", "module_path!", "type_name", "TypeId", "dyn Any"] {
            assert!(!text.contains(banned), "{} contains `{banned}`", file.display());
        }
    }
}

// ------------------------------------------------------------------ C17: drop

thread_local! {
    static MADE: Cell<u64> = const { Cell::new(0) };
    static DROPPED: Cell<u64> = const { Cell::new(0) };
}

/// Counts itself in and out. Every kind in `Estate` carries one.
#[derive(Debug)]
pub struct Tally;

impl Tally {
    fn new() -> Self {
        MADE.with(|m| m.set(m.get() + 1));
        Tally
    }
}

impl Drop for Tally {
    fn drop(&mut self) {
        DROPPED.with(|d| d.set(d.get() + 1));
    }
}

pub mod estate {
    use super::Tally;
    use weft::*;

    #[kind(root, needs = [Seat])]
    pub struct Deeds(pub Tally);

    #[kind]
    pub struct Hall(pub Tally, pub u32);

    #[kind(needs = [Inside], rank = R)]
    pub struct Box_<R>(pub Tally, pub u32, pub R);

    #[kind(needs = [Lives])]
    pub struct Heir(pub Tally, pub u32);

    set!(pub Holder);
    set!(pub Stuff);
    is!(Hall, Heir: Holder);
    is!(Box_<R>: Holder, Stuff);

    #[relation(Deeds -> Hall, one, refuse)]
    pub struct Seat;

    #[relation(Stuff -> Holder, one, ranked, cascade)]
    pub struct Inside;

    #[relation(Heir -> Hall, one, refuse)]
    pub struct Lives;

    #[relation(Hall <-> Hall, many, cyclic, detach)]
    pub struct Road(pub u32);

    #[relation(Heir -> Heir, many, cyclic, detach)]
    pub struct Envies;

    #[component(Heir)]
    pub struct Worth(pub Tally);

    module!(pub Estate {
        kinds: [Hall, Box_<R0>, Box_<R1>, Heir],
        relations: [Seat, Inside, Lives, Road, Envies],
        components: [Worth],
    });
    game!(pub Manor: Deeds [Estate]);
}

#[test]
fn drop_frees_everything() {
    use estate::*;
    let made_before = MADE.with(Cell::get);
    let dropped_before = DROPPED.with(Cell::get);
    {
        let mut world = World::<estate::Manor>::new(|t| {
            let halls: Vec<_> = (0..4).map(|i| t.spawn(Hall(Tally::new(), i), ())).collect();
            for (i, pair) in halls.windows(2).enumerate() {
                t.relate(pair[0], (Road(i as u32), Road(i as u32 + 100)), pair[1]);
            }
            t.relate(halls[3], (Road(7), Road(8)), halls[0]); // a cycle of roads
            for (i, &hall) in halls.iter().enumerate() {
                let heir = t.spawn(Heir(Tally::new(), i as u32), (Lives, hall));
                let outer = t.spawn(Box_(Tally::new(), i as u32, R1), (Inside, hall));
                t.spawn(Box_(Tally::new(), i as u32, R0), (Inside, outer));
                t.insert(heir, Worth(Tally::new()));
            }
            let heirs: Vec<_> = t.all::<Heir>().collect();
            for (&a, &b) in heirs.iter().zip(heirs.iter().cycle().skip(1)) {
                t.relate(a, Envies, b); // envy goes round in a circle
            }
            t.spawn(Deeds(Tally::new()), (Seat, halls[0]))
        });
        for round in 0..200_u32 {
            world.turn(|t| {
                let halls: Vec<_> = t.all::<Hall>().collect();
                let heirs: Vec<_> = t.all::<Heir>().collect();
                let boxes: Vec<_> = t.all::<Box_<R0>>().collect();
                // Shuffle the small boxes between heirs and halls: every move grows a history.
                for (i, &small) in boxes.iter().enumerate() {
                    let n = (i as u32 + round) as usize;
                    let moved = if n.is_multiple_of(2) {
                        t.relate(small, Inside, heirs[n % heirs.len()])
                    } else {
                        t.relate(small, Inside, halls[n % halls.len()])
                    };
                    assert_eq!(moved, Ok(()));
                }
                if round % 10 == 0 {
                    // Spawn a few more, despawn a few: despawned entities count too.
                    let hall = halls[round as usize % halls.len()];
                    let outer = t.spawn(Box_(Tally::new(), round, R1), (Inside, hall));
                    t.spawn(Box_(Tally::new(), round, R0), (Inside, outer));
                    let oldest = t.all::<Box_<R1>>().next();
                    if let Some(old) = oldest {
                        assert_eq!(t.despawn(old), Ok(()));
                    }
                }
            });
            assert_eq!(world.check(), Ok(()));
        }
        // A long history on one edge: dropping it must not recurse once per move.
        world.turn(|t| {
            let halls: Vec<_> = t.all::<Hall>().collect();
            let heir = t.all::<Heir>().next().expect("an heir");
            for i in 0..20_000 {
                assert_eq!(t.relate(heir, Lives, halls[i % halls.len()]), Ok(()));
            }
        });
    }
    let made = MADE.with(Cell::get) - made_before;
    let dropped = DROPPED.with(Cell::get) - dropped_before;
    assert!(made > 0);
    assert_eq!(dropped, made, "dropping the world must drop every entity and component it ever held");
}
