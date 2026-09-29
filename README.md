# weft

A typed entity-relation graph for text games, where invalid states don't compile. No `unsafe` and no panics; the only dependencies are `syn`, `quote`, `proc-macro2` and `darling`, at compile time, for the declaration macros. Nightly, for `decl_macro` in crates that declare modules.

```rust
#![feature(decl_macro)]
use weft::*;

#[kind(root, needs = [Start])]
pub struct Settings;

#[kind(is = [Holder])]
pub struct Room { pub name: &'static str }

#[kind(needs = [In], rank = 0, is = [Thing])]   // an item is always in exactly one place
pub struct Item { pub name: &'static str }

#[kind(needs = [In], rank = R, is = [Thing, Holder])]   // a Chest<R2> holds Chest<R1>s, never another Chest<R2>
pub struct Chest<R> { pub name: &'static str, pub rank: R }

set!(pub Thing);
set!(pub Holder);

#[relation(Thing -> Holder, one, ranked, spill)]
pub struct In;

#[relation(Room <-> Room, many, cyclic, detach)]
pub struct Exit(pub &'static str);

#[relation(Settings -> Room, one, refuse)]
pub struct Start;

module!(pub Keep { kinds: [Room, Item, Chest<R1>, Chest<R2>], relations: [In, Exit, Start] });
game!(pub Game: Settings [Keep]);

let mut world = World::<Game>::new(|t| {
    let hall = t.spawn(Room { name: "hall" }, ());
    let cellar = t.spawn(Room { name: "cellar" }, ());
    t.relate(hall, (Exit("down"), Exit("up")), cellar);       // one fact, a label for each end
    let chest = t.spawn(Chest { name: "chest", rank: R1 }, (In, hall));
    t.spawn(Item { name: "coin" }, (In, chest));
    t.spawn(Settings, (Start, hall))
});

world.turn(|t| -> Result<(), Refused> {
    let Some(chest) = t.all::<Chest<R1>>().next() else { return Ok(()) };
    let hall = t.target::<Start>(t.root());                  // an Id, never an Option
    t.despawn(chest)?;                                       // the coin spills into the hall
    assert!(t.despawn(hall) == Err(Refused::Held));          // Start refuses to let the hall go
    Ok(())
})?;
```

## Rejected by the compiler

| mistake | how |
|---|---|
| a handle kept past its turn, or used in another world's turn | its lifetime is the turn's, and invariant |
| spawning an item without its container, or into two | `#[kind(needs = [In])]` makes the edge a `spawn` argument |
| unrelating a `one` edge | `one` relations have no `unrelate`; relating again moves the edge |
| a one-way door on a two-way relation | `<->` relations take a pair of values |
| a chest in itself, or in its own contents | `ranked`: every edge must go up in rank |
| a room in an item, or `At` pointing at a lamp | relation endpoints are typed |
| reading data through a set handle | set handles have no data; `cast` to a kind first |
| relating a hooked relation without its hook's bound | `on_relate = f: Map` makes callers carry `Map` |
| changing the graph while iterating it | reads borrow the turn, changes need it `&mut` |
| a `one` relation its source doesn't `need`, overlapping endpoints without a loop rule, `cascade` from the root, `detach` on `one`, `spill` on `many` | `game!` and `#[relation]` check under `cargo check`, pointing at the offending token where they can |

At runtime, only graph state no type can see is refused, as `Refused`: despawning something a `refuse` edge holds (`Held`), relating into something doomed this turn (`Doomed`), and hook chains that don't settle (`Unsettled`). A refusal changes nothing.

## Verified

`cargo test --workspace` runs every gate: 21 invalid states that must fail `cargo check` at a marked line (each with a twin that compiles), a differential fuzzer against a naive model (with a planted fault it must catch), three planted library bugs that the gates must kill, the seven canonical programs, and the decision record's claims register. See [DESIGN.md](DESIGN.md) and [docs/design](docs/design).
