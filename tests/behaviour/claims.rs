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

#[derive(Debug)]
pub struct Settings;
kind!(root Settings);

#[derive(Debug)]
pub struct Room;
kind!(Room);

set!(pub Spirit);

/// Two kinds with the same name, in different modules.
pub mod a {
    use super::Spirit;
    use weft::*;

    #[derive(Debug)]
    pub struct Ghost {
        pub n: u32,
    }
    kind!(Ghost);
    is!(Ghost: Spirit);
}

pub mod b {
    use super::Spirit;
    use weft::*;

    #[derive(Debug)]
    pub struct Ghost {
        pub n: u32,
    }
    kind!(Ghost);
    is!(Ghost: Spirit);
}

relation!(pub struct Haunts: Spirit -> Room, many, detach);

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
            let allowed = ["#![feature(generic_const_exprs, decl_macro)]", "#![allow(incomplete_features)]"];
            assert!(allowed.contains(&text.trim()), "src/lib.rs:{}: unexpected crate attribute `{text}`", line + 1);
        }
    }
}

#[test]
fn no_third_party_crates() {
    let ours = ["weft", "game", "reject", "accept"];
    for lock in ["Cargo.lock", "tests/reject/Cargo.lock", "tests/accept/Cargo.lock"] {
        let path = root().join(lock);
        if !path.exists() {
            continue;
        }
        let text = read(&path);
        for line in text.lines() {
            if let Some(name) = line.strip_prefix("name = \"").and_then(|n| n.strip_suffix('"')) {
                assert!(ours.contains(&name), "{lock} lists a third-party package: {name}");
            }
            assert!(!line.starts_with("source = "), "{lock} pulls a package from a registry or git: {line}");
        }
    }
    let manifest = read(&root().join("Cargo.toml"));
    let deps = manifest.split("[dependencies]").nth(1).unwrap_or_default();
    let deps = deps.split("\n[").next().unwrap_or_default();
    assert!(deps.trim().is_empty(), "weft's [dependencies] must stay empty, found: {deps}");
}

#[test]
fn source_is_clean() {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut dirs = vec![root().join("src")];
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

    #[derive(Debug)]
    pub struct Deeds(pub Tally);
    kind!(root Deeds needs Seat);

    #[derive(Debug)]
    pub struct Hall(pub Tally, pub u32);
    kind!(Hall);

    #[derive(Debug)]
    pub struct Box_<const R: u8>(pub Tally, pub u32);
    kind!(Box_<const R: u8> rank R needs Inside);

    #[derive(Debug)]
    pub struct Heir(pub Tally, pub u32);
    kind!(Heir needs Lives);

    set!(pub Holder);
    set!(pub Stuff);
    is!(Hall, Heir: Holder);
    is!(Box_<const R: u8>: Holder, Stuff);

    relation!(pub struct Seat: Deeds -> Hall, one, refuse);
    relation!(pub struct Inside: Stuff -> Holder, one, ranked, cascade);
    relation!(pub struct Lives: Heir -> Hall, one, refuse);
    relation!(pub struct Road(pub u32): Hall <-> Hall, many, cyclic, detach);
    relation!(pub struct Envies: Heir -> Heir, many, cyclic, detach);
    component!(pub struct Worth(pub Tally): Heir);

    module!(pub Estate {
        kinds: [Hall, Box_<0>, Box_<1>, Heir],
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
                let outer = t.spawn(Box_::<1>(Tally::new(), i as u32), (Inside, hall));
                t.spawn(Box_::<0>(Tally::new(), i as u32), (Inside, outer));
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
                let boxes: Vec<_> = t.all::<Box_<0>>().collect();
                // Shuffle the small boxes between heirs and halls: every move grows a history.
                for (i, &small) in boxes.iter().enumerate() {
                    let n = (i as u32 + round) as usize;
                    let moved = if n % 2 == 0 {
                        t.relate(small, Inside, heirs[n % heirs.len()])
                    } else {
                        t.relate(small, Inside, halls[n % halls.len()])
                    };
                    assert_eq!(moved, Ok(()));
                }
                if round % 10 == 0 {
                    // Spawn a few more, despawn a few: despawned entities count too.
                    let hall = halls[round as usize % halls.len()];
                    let outer = t.spawn(Box_::<1>(Tally::new(), round), (Inside, hall));
                    t.spawn(Box_::<0>(Tally::new(), round), (Inside, outer));
                    let oldest = t.all::<Box_<1>>().next();
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
