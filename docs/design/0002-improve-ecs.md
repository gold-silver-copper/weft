# Decision: improve-ecs

## Context

The question was how to improve `designs/ecs/06-execution-prompt.md`, the execution prompt for the `ecs` decision (option d, typed entity kinds, recorded at `weft/docs/design/0001-ecs.md`), before it runs against weft at `af02049`. The brief names four problems:

1. **The `Default` fallback.** `Index` and `target` on `one` relations silently return made-up data, and no gate checks for it.
2. **Weaknesses the record accepted and the prompt freezes:**
   - same-kind cycles are refused only at runtime;
   - a hook sees only its own relation;
   - `cast` compares `module_path!()` strings.
3. **Rigidity.** "Stop and report" on any snag will stall a long run.
4. **Weak verification.** The gates don't prove what the prompt claims.

The constraints that decided the outcome:

- **Relation rules are type-enforced.** Exactly-one, two-way, acyclic and endpoint rules are enforced by the type system. There are no stale handles, no panics, no `unsafe` and no dependencies.
- **Nightly is allowed, and required if that is what closes the weaknesses.** The executor should close them, not freeze them.
- **Error timing.** `cargo check` errors are preferred. `cargo build` errors are acceptable only when nothing else works.
- **The executor never stops.** It deviates as needed and records everything, and the user reviews at the end.
- **Taste rules for the prompt:**
  - state goals, not walls of prohibitions;
  - say what must hold, not how to build it;
  - checks prove the claimed behaviour;
  - no accepted weakness is left in prose where nothing enforces it.
- **Nothing is sacred.** Option d, its API and its frozen programs may all change.

## Decision

**Adopt option c (nightly-maximal) as the design. Amend it with the parts of a, e and f that their spikes proved. Write the execution prompt in e's goal-only form, with f's gates-first milestone and ledger.**

### Why c

c is the only option whose spike closed all four accepted weaknesses at the type level. The others made them detectable. Its spike passed `cargo check --offline --examples`.

- **Same-kind cycles don't compile.** `relate(big, In, big)` and `relate(big, In, small)` fail under `cargo check` as E0277 with ``the relation is ranked, and ranks must rise``. There is no ICE and no post-monomorphisation error (c spike Q2).
  - Every other spiked option still compiles `t.relate(chest, In, chest)` and returns `Err(Refused::Cycle)`: a's P3/P5/P7, d's `try_relate`, e's G2 floor, f's red C06.
  - e's spike tried three other routes on nightly and showed that same-kind handles can't be told apart by type. Rejection came only at `cargo build`, or legal moves broke as well (auto trait → E0277/E0751, negative impl → E0366, specialization → E0080 at build only).
  - So ranks are the only compile-time route in evidence.
- **Reads are total.** `t[id]` is `&id.node().data`, and `target` on `one` has no miss arm (c spike, `src/decl.rs:386`). Kinds no longer derive `Default`.
  - a, d, e and f all keep the fallback and count it.
  - a's own reviewer notes that counting a miss after returning invented data "improves observability, not totality".
- **Hooks declare their reach.** `on_relate = pick_up: Map` works: P7's `pick_up` relates `In` from `At` (c spike). e's spike proved the same `OnRelate<G>` bound shape with mutual hooks, no E0275 or E0283, and `??ty: 0`.
- **`cast` compares identity, not strings.** `t.cast::<K>` finds the node by address; there is no tag.
- **Only weft needs `generic_const_exprs`.** User crates enable only `decl_macro`, and only because they call `module!`. The downstream `probe` crate needs no `#![feature]` at all and still gets the rank E0277s (c spike Q3).

### Amendments to c, each from spike evidence

1. **No `specialization`.** c never built its `game!` audits. Use a's stable probes instead: inherent-const fallback, one probe per (kind, relation) pair in `game!`.
   - a's spike showed `cargo check` E0080 at the `game!` line for these, including when the `is!` is upstream or downstream:
     - a missing `needs`;
     - overlapping endpoints without a loop rule;
     - `cascade` from the root.
   - weft's features become `generic_const_exprs` and `decl_macro` only.
2. **Replace the blanket `impl<G: Schema, S: Set> Has<S> for G`.** a, e and f each hit E0119 with a `game!` in another crate. It is fixed either by a's `Stored::Tag`/`Holds` dispatch or by deleting the blanket, as e and f did.
3. **Frozen programs lose the `generic_const_exprs` header,** and P5's two `needs`-shape doctests expect **E0271** rather than E0308 (c spike Q3).
4. **`World::new`'s genesis closure returns `Rooted<'t, Root>`,** which carries the settings (c spike). The programs are unchanged.
5. **Dropping a `World` drops everything.** c's spike found that `Rc` cycles leak with no `Drop`. C17 makes this a claim.
6. **Hook failures don't roll back** (e spike). A hook's `Err` leaves the triggering edge and the hook's earlier steps in place, and each of those steps is valid. C3's fuzzer covers hooks that refuse partway. C8 bounds hook chains.

### Why this prompt shape

- **Goal-only, from e.** The reviewers faulted c's own prompt for prescribing `Rc`, `OnceCell` and `specialization`. The prompt states the claims below and leaves the representation to the executor.
- **Gates first, from f.** M0 writes every gate before library code.
  - f's spike ran the full register in 17.8 s cold.
  - One `cargo check --keep-going` covers all 22 reject cases, and one more covers their twins. There were no lock deadlocks.
  - rustdoc checks `compile_fail,E…` codes only on nightly (f spike). The nightly pin therefore also makes the doctest codes real gates.
- **Pinned fingerprints, from f.** Every P8 variant (a, b, c, d, f) let one log entry excuse every later edit. f's spike closed that by pinning each file's fingerprint after the change, and a live run caught an unlogged second edit.

### The four problems, mapped

| Problem in the brief | Named change in the prompt | Claims |
|---|---|---|
| `Default` fallback | Total reads: `one` edges stored with their source; kinds without `Default`; set handles can't be indexed | C6 |
| Accepted weaknesses | `ranked` loop rule; hook bounds declared on the relation and carried by callers; `cast` by identity; `game!` declaration probes | C5, C7, C8, C9, C12 |
| Too rigid | Never-stop deviation protocol: each change pins its file's new fingerprint, and unmet claims become `shortfall` entries | C19 |
| Weak verification | M0 writes every gate first: twins, canaries, mutants, a differential fuzzer against a naive model, and this claims table | every row, C18 |

### Claims

M0 copies this table verbatim into `docs/design/0001-ecs/claims.md`.

| id | claim | gates | fails when |
|---|---|---|---|
| C1 | If a handle exists, its entity exists: no handle leaves its turn or enters another world's turn | `tests/reject/src/bin/handle_leaves_turn.rs`, `tests/reject/src/bin/handle_enters_other_turn.rs`, `tests/cases.rs::every_reject_case_fails_at_its_marker` | a case compiles, or fails somewhere other than its marked line |
| C2 | An item is always in exactly one container | `tests/reject/src/bin/item_in_no_container.rs`, `tests/reject/src/bin/item_in_two_containers.rs`, `tests/reject/src/bin/unrelate_one.rs`, `tests/fuzz.rs::differential` | a case compiles; the fuzzer finds an item with no holder or with two |
| C3 | No edge points at a despawned entity, and every op leaves a correct graph, including hooks that refuse partway | `tests/fuzz.rs::differential`, `tests/mutants.rs::every_mutant_is_killed`, `tests/programs.rs::every_program_runs` | `World::check` or the naive model disagrees after any op; the `purge_keeps_edges` mutant survives; P3's second turn fails |
| C4 | A two-way relation is one fact that reads correctly from both ends | `tests/reject/src/bin/one_way_door.rs`, `tests/fuzz.rs::differential`, `tests/mutants.rs::every_mutant_is_killed` | relating `Exit` with one label compiles; the `back_reads_forward_label` mutant survives |
| C5 | A forbidden cycle doesn't compile: a chest in itself, a chest in its own contents, a room in an item, a relation whose endpoints overlap without a loop rule | `tests/reject/src/bin/chest_in_itself.rs`, `tests/reject/src/bin/chest_in_its_contents.rs`, `tests/reject/src/bin/room_in_item.rs`, `tests/reject/src/bin/overlap_without_loop_rule.rs`, `tests/reject/src/bin/refused_has_no_cycle.rs` | any case compiles under `cargo check`, fails only at `cargo build`, or the rank cases fail without `the relation is ranked, and ranks must rise` |
| C6 | Reads are total: `t[id]` and `target` on a `one` relation read stored data, with no fallback value and no `Default` bound | `tests/accept/src/bin/kind_without_default.rs`, `tests/reject/src/bin/index_through_set.rs`, `tests/fuzz.rs::differential`, `tests/cases.rs::every_accept_case_compiles` | a kind needs `Default`; indexing a set handle compiles; any read disagrees with the naive model |
| C7 | A hook may touch every relation its declared bound names, and every caller of that relation carries the bound | `tests/reject/src/bin/caller_lacks_hook_bound.rs`, `tests/reject/src/bin/hook_overasks.rs`, `tests/accept/src/bin/mutual_hooks.rs`, `tests/programs.rs::every_program_runs` | P7's `pick_up` asserts fail; a caller without the bound compiles; two hooks relating each other's relations fail with E0275 or E0283 |
| C8 | A hook chain that never settles is refused, never a stack overflow or a hang | `tests/fuzz.rs::hooks_settle` | the test overflows or passes its turn budget |
| C9 | `cast` compares no strings | `tests/behaviour/claims.rs::cast_matrix`, `tests/reject/src/bin/cast_outside_set.rs`, `tests/mutants.rs::every_mutant_is_killed` | two kinds both named `Ghost` cast into each other; the `cast_by_string` mutant survives; casting to a non-member compiles |
| C10 | Relation endpoints are typed | `tests/reject/src/bin/wrong_endpoint.rs` | `(At, lamp)` compiles |
| C11 | Changing the graph while iterating it doesn't compile | `tests/reject/src/bin/conflicting_query.rs` | it compiles, or fails without E0502 |
| C12 | `game!` rejects bad declarations under `cargo check`: a `one` relation its source kind doesn't `need`, `cascade` or `spill` from the root, `detach` on `one`, `spill` on anything but `one` | `tests/reject/src/bin/one_not_in_needs.rs`, `tests/reject/src/bin/root_cascade.rs`, `tests/reject/src/bin/detach_on_one.rs`, `tests/reject/src/bin/spill_on_many.rs` | a case compiles, or fails only at `cargo build` |
| C13 | A module from another crate drops into a downstream `game!`, with set membership declared in either crate | `tests/accept/src/bin/module_from_another_crate.rs`, `tests/cases.rs::every_accept_case_compiles` | the downstream `game!` fails, with E0119 or anything else |
| C14 | Everything infers, in concrete and generic code | `rust-analyzer analysis-stats .` | the pinned toolchain's rust-analyzer reports `??ty` or `?ty` above 0 |
| C15 | No `unsafe`, no dependencies, no library panics, no runtime type lookup | `cargo clippy -p weft --lib -- -D warnings`, `tests/behaviour/claims.rs::lint_wall_is_intact`, `tests/behaviour/claims.rs::no_third_party_crates`, `tests/behaviour/claims.rs::source_is_clean` | clippy fails; a lint leaves the `af02049` forbid block; `Cargo.lock` lists another package; `src` contains `unsafe`, `module_path!`, `type_name`, `TypeId` or `dyn Any` |
| C16 | P1–P7 compile and behave as written | `tests/programs.rs::every_program_runs`, `cargo test -p game` | an example exits non-zero; an assert fails; a line its comments promise is missing from its stdout |
| C17 | Dropping a `World` drops every entity it ever spawned, despawned ones included | `tests/behaviour/claims.rs::drop_frees_everything` | drops don't equal spawns |
| C18 | The gates can fail | `tests/cases.rs::every_twin_compiles`, `tests/cases.rs::wrong_code_canary_fails`, `tests/fuzz.rs::planted_fault_is_caught`, `tests/mutants.rs::every_mutant_is_killed` | a twin doesn't compile; a doctest with the wrong error code passes; the fuzzer misses its planted fault; a mutant survives |
| C19 | Every change after M0 is recorded | `tests/deviations.rs::nothing_changed_without_a_pin`, `tests/deviations.rs::every_record_claim_is_kept_or_logged` | see P8 |

### What the prompt must say

Only outcomes:

- **Goal:** every claim above is green, P1–P8 behave as written, and the executor finishes without stopping.
- **Milestones:** M0 writes the frozen copies, `claims.md`, every gate, `baseline.txt` and the empty log, then tags `gates-m0`. Later milestones raise the number of green claims without turning a green claim red.
- **Deviation protocol:**
  - Make the smallest change that keeps every claim, log it as P8 describes, and continue.
  - A gate may change only through a pinned deviation. It is never deleted.
  - A claim still red at the end is a `shortfall`, listed in the final report. A shortfall is not acceptance.
  - Peano ranks in place of `generic_const_exprs` are an allowed deviation, because they still meet C5.
- **Gate scoping:**
  - The library runs clippy under the forbid wall.
  - Programs and tests run with `-A clippy::disallowed_macros`, because `clippy.toml` bans `assert!` (noted in a's and e's options).
  - C14 uses the pinned toolchain's rust-analyzer. a's and e's spikes found the Homebrew build's numbers meaningless.

### What would change my recommendation

I would switch to **a's stable design with f's harness** if either of these happens:

- At full scale, the total-read representation can't meet C1, C6 and C17 together without a miss arm or `unsafe`.
- C5 can't hold at `cargo check` in P1–P7 even with Peano ranks.

That switch would accept runtime acyclicity and a counted fallback as shortfalls, and the user would have to approve them explicitly.

## Options considered

- **a: Harden d in place.** Marked spike by the reviewers; the spike passed.
  - It keeps d's representation and adds a claims table, mutation runs and a deviation log.
  - **Not chosen:** its P3, P5 and P7 keep `t.relate(chest, In, chest)` compiling and returning `Err(Refused::Cycle)`, and kinds keep `Default` for a fallback counted by `Violation::FallbackRead`. The brief asks for both to be closed, and c's spike closed both.
  - **Kept:** its stable `game!` probes (amendment 1) and its E0119 finding (amendment 2).
- **b: Close every weakness on stable.** Arena handles `Id<'t, 'w, K>`, a separate `nest` API with `Outside`/`Clear`, and typed-table `cast`.
  - **Rejected by the reviewers:** "the proposed prompt dictates implementation mechanisms, and P8’s checks demonstrably do not enforce its claimed deviation completeness. / breaks taste rule "Minimal: few concepts" (splits hierarchy out of relations into a separate `nest` API) and constraint "Rules on relations are enforced by the type system: acyclic" (same-kind cycles still compile and are checked at runtime by `nest`)."
- **c: Nightly-maximal.** Chosen, with the amendments above. Marked spike by the reviewers; the spike passed.
- **d: Best of the spikes.** a's Peano `Relate` proofs with `try_relate`, home-module hooks, and `decl_macro` modules. Marked spike by the reviewers; the spike passed.
  - **Not chosen:**
    - Same-kind acyclicity is still a runtime `try_relate → Err(Cycle)` (d spike; P3 and P7 assert it).
    - The fallback arm stays.
    - Generic code carries one `Relate<R, S, T>` bound per kind pair.
    - A `Relate` bound hijacked unrelated `relate` calls (E0271) until a hidden `Via` trait was added.
    - rustc warns `recursion_depth_exceeding_limit` from 40 kinds.
    - rust-analyzer reported false E0277s in P7.
    - A hidden public `internal::id` forges handles.
- **e: Goal-only prompt.** Marked spike by the reviewers; the spike passed.
  - **Not chosen as the design:** its G2 floor lets the executor stop at runtime acyclicity, and its own spike showed G2's preferred level is unreachable on d's representation. Its P8 compares no baseline, so an unlogged respelling passes. G4 wasn't implemented in the spike: `cast` still compared `module_path!()` tags.
  - **Kept:** its prompt form (goals and claims, no implementation directives) and its `OnRelate<G>` hook-bound evidence.
- **f: Verification first.** Marked spike by the reviewers; the spike passed.
  - **Not chosen as the design:** its API keeps `Kind: Default` and a runtime `Refused::Cycle`, and it makes both type-level fixes `Reach` claims. Its example deviation D2 already concedes C06.
  - **Kept:** gates written in M0, the reject harness with twins, canaries, mutants, and fingerprint-pinned deviations. f's spike showed that fix closes the `logged(id)` hole.

## Consequences

**Easier**

- Reads can't return invented data, so `World::check` is an audit, not the only defence.
- A chest can't be put in itself or in its contents, and a room can't go in an item; none of these compiles. `Refused` shrinks to `Held` and `Doomed`.
- A hook can touch any relation its bound names, and every caller sees that bound.
- Two kinds with the same name can't be confused by `cast`.
- A module from another crate works in a downstream `game!` (amendment 2).
- The executor can deviate without stopping, and every deviation is pinned and reviewable.

**Harder**

- **Ranks.** Nesting depth is fixed per kind (`Chest<1>`, `Chest<2>`). Two chests of the same rank can't nest even when that would be acyclic.
  - A same-kind DAG with no static layering must use ranked kinds or be `cyclic`.
  - Code generic over a rank must write `where Chest<R>: Fits<Room>`, or it fails with E0283 (c spike).
  - Ranks are a new concept. The concept count stays at 13.
- **Set handles can't be indexed.** Code must `cast` to a kind first, and `target` needs a concrete-kind source. P5's `take` and P7's `pick_up` filter to `Item`, so a `Chest<1>` on the floor stays where it is.
- **Kind data is read-only after spawn.** There is no `IndexMut`. Mutable per-entity state lives in components; only the root's settings are `&mut`.
- **Toolchain.** weft pins a dated nightly; c's spike ran rustc 1.100.0-nightly from 2026-09-20.
  - On that nightly, `generic_const_exprs` warns that the next-generation trait solver doesn't support it and falls back to the old solver. Every toolchain bump needs the full gate run.
  - Rank errors show under `cargo check` and flycheck, not in rust-analyzer's native diagnostics.
- **Hook failures are partial.** A hook's `Err` doesn't roll back its triggering edge (C3 keeps the graph correct regardless).
- **Memory.** `one`-edge chains only grow, because nodes are shared through `Rc` and can't be compacted. Performance is a non-goal. Leaks are covered by C17.
- **Size.** The library is about 3,000–3,500 lines, plus about 2,000 lines of gates (c spike estimate).

**Deleted**

- From weft at `af02049`:
  - all of `src/`, about 2,862 lines;
  - `tests/weft.rs` and `examples/school.rs`;
  - `#![no_std]` and `extern crate alloc`.
- Kept byte for byte: `#![forbid(unsafe_code)]`, the `#![forbid(clippy::…)]` block, `clippy.toml` and the empty `[dependencies]`.
- From option d's design:
  - `Kind: Default`, the per-kind fallback and `Raw::NONE`;
  - `Refused::Cycle` and the runtime `acyclic` check;
  - `IndexMut` on kinds;
  - `Kind::TAG`, `Relation::TAG` and the `module_path!()` comparison;
  - the fixed hook bound;
  - `#[macro_export]` module callbacks;
  - the blanket `Has<S>` and its `Box::leak`;
  - "genesis `relate` returns `Result` iff `acyclic`".
- From `06-execution-prompt.md`:
  - "Do not redesign" and its nine binding implementation items;
  - the file list;
  - every "stop and report";
  - "`Index` with the per-kind `Default` fallback arm";
  - "`cast` (the `module_path!()` tag comparison)";
  - "builds on stable, with no `#![feature]`";
  - the run-only, exit-0 gates.

**Migrations that follow**

- **The ecs record is amended.** This record replaces the Frozen API and the claims of `docs/design/0001-ecs.md`, and P8 reads the amended record at that path.
- **New files:**
  - `rust-toolchain.toml` pinning the nightly;
  - the workspace crate `game` (P5);
  - the detached `tests/reject` and `tests/accept` packages;
  - `docs/design/0001-ecs/{claims.md, baseline.txt}`;
  - `docs/design/0001-ecs-deviations.md`, which starts with a `# Deviations` title.
- **User crates** that call `module!` add `#![feature(decl_macro)]`.
- **The execution prompt is rewritten** in the goal-only form above.

## Frozen API

P1–P7 are option c's programs as spiked, with two spike-driven edits: user crates enable only `decl_macro`, and P5's `needs`-shape doctests expect E0271 (both from c spike Q3). P8 is new: it combines c's record-parsing with f's fingerprint pinning and checks the claims register.

### P1: Rooms, exits and taking an item

```rust
#![feature(decl_macro)]
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings needs Start, Plays);

#[derive(Debug)]
pub struct Room {
    pub name: &'static str,
    pub desc: &'static str,
}
kind!(Room);

#[derive(Debug)]
pub struct Player;
kind!(Player needs At);

#[derive(Debug)]
pub struct Item {
    pub name: &'static str,
}
kind!(Item needs In); // an `Item` can't be spawned without its container

set!(pub Holder);
is!(Room, Player: Holder);

// A two-way exit is one fact with a label for each direction.
relation!(pub struct Exit(pub &'static str): Room <-> Room, many, cyclic, detach);
// `Item` is not a `Holder`, so `In` can't loop and needs no loop rule (`game!` checks this).
relation!(pub struct In: Item -> Holder, one, cascade);
// Where the player stands. Setting it runs `entered`, which may use anything in `Map`.
relation!(pub struct At: Player -> Room, one, refuse, on_relate = entered: Map);
relation!(pub struct Start: Settings -> Room, one, refuse);
relation!(pub struct Plays: Settings -> Player, one, refuse);

module!(pub Map { kinds: [Room, Player, Item], relations: [Exit, In, At, Start, Plays] });
game!(pub Game: Settings [Map]);

fn entered<'t, G: Map>(t: &mut Turn<'t, G>, _: Id<'t, Player>, room: Id<'t, Room>) -> Result<(), Refused> {
    println!("You enter the {}.", t[room].name);
    Ok(())
}

fn describe<'t, G: Map>(t: &Turn<'t, G>, room: Id<'t, Room>) {
    println!("{}\n{}", t[room].name, t[room].desc);
    for (to, exit) in t.targets::<Exit>(room) {
        println!("  {}: {}", exit.0, t[to].name);
    }
    for (item, _) in t.sources::<In>(room) {
        println!("  There is a {} here.", t[item].name);
    }
}

/// `take <noun>`. `In` is exactly-one, so relating the item again *is* the move.
fn take<'t, G: Map>(t: &mut Turn<'t, G>, me: Id<'t, Player>, noun: &str) -> Result<bool, Refused> {
    let here = t.target::<At>(me);
    let found = t.sources::<In>(here).map(|(item, _)| item).find(|&item| t[item].name == noun);
    match found {
        Some(item) => t.relate(item, In, me).map(|()| true),
        None => Ok(false),
    }
}

fn main() -> Result<(), Refused> {
    let mut world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "Hall", desc: "A draughty hall." }, ());
        let cellar = t.spawn(Room { name: "Cellar", desc: "It smells of apples." }, ());
        let garden = t.spawn(Room { name: "Garden", desc: "Roses, everywhere." }, ());
        t.relate(hall, (Exit("down"), Exit("up")), cellar);
        t.relate(hall, (Exit("north"), Exit("south")), garden);
        t.relate(cellar, (Exit("tunnel"), Exit("trapdoor")), garden);
        t.spawn(Item { name: "lamp" }, (In, cellar));
        t.spawn(Item { name: "rug" }, (In, cellar));
        let me = t.spawn(Player, (At, hall));
        t.spawn(Settings, ((Start, hall), (Plays, me)))
    });

    world.turn(|t| -> Result<(), Refused> {
        let me = t.target::<Plays>(t.root());
        let here = t.target::<At>(me);
        describe(t, here); // "Hall", "down: Cellar", "north: Garden"

        // "go down": `entered` prints "You enter the Cellar."
        let down = t.targets::<Exit>(here).find(|(_, exit)| exit.0 == "down").map(|(room, _)| room);
        let Some(cellar) = down else { return Ok(()) };
        t.relate(me, At, cellar)?;
        describe(t, cellar); // "up: Hall", "tunnel: Garden", the lamp and the rug

        // "take lamp"
        assert!(take(t, me, "lamp")?);
        let carried: Vec<_> = t.sources::<In>(me).map(|(item, _)| t[item].name).collect();
        let lying: Vec<_> = t.sources::<In>(cellar).map(|(item, _)| t[item].name).collect();
        assert_eq!(carried, ["lamp"]);
        assert_eq!(lying, ["rug"]);
        println!("You carry: {}", carried.join(", "));
        Ok(())
    })
}
```

### P2: Who the blacksmith likes

```rust
#![feature(decl_macro)]
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings);

#[derive(Debug)]
pub struct Npc {
    pub name: &'static str,
}
kind!(Npc);

// How well two NPCs know each other, from each side. One fact, two values.
relation!(pub struct Knows(pub u8): Npc <-> Npc, many, cyclic, detach);
// How much the source likes the target. Not mutual.
relation!(pub struct Likes(pub u8): Npc -> Npc, many, cyclic, detach);

module!(pub Village { kinds: [Npc], relations: [Knows, Likes] });
game!(pub Game: Settings [Village]);

/// Everyone `who` likes, most-liked first.
fn liked_by<'t, G: Village>(t: &Turn<'t, G>, who: Id<'t, Npc>) -> Vec<(&'static str, u8)> {
    let mut liked: Vec<_> = t.targets::<Likes>(who).map(|(npc, likes)| (t[npc].name, likes.0)).collect();
    liked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    liked
}

fn main() {
    let mut world = World::<Game>::new(|t| {
        let [smith, baker, miller, witch] =
            ["smith", "baker", "miller", "witch"].map(|name| t.spawn(Npc { name }, ()));
        t.relate(smith, (Knows(9), Knows(7)), baker);
        t.relate(smith, (Knows(3), Knows(8)), witch);
        t.relate(smith, Likes(8), baker);
        t.relate(smith, Likes(2), miller);
        t.relate(smith, Likes(5), witch);
        t.relate(baker, Likes(9), smith);
        t.spawn(Settings, ())
    });

    world.turn(|t| {
        let Some(smith) = t.all::<Npc>().find(|&npc| t[npc].name == "smith") else { return };

        let liked = liked_by(t, smith);
        assert_eq!(liked, [("baker", 8), ("witch", 5), ("miller", 2)]);
        for (name, how_much) in liked {
            println!("{name}: {how_much}");
        }

        // A join over two relations: people the smith likes *and* knows, with how well.
        let both: Vec<_> = t
            .targets::<Likes>(smith)
            .filter_map(|(npc, likes)| {
                let knows = t.targets::<Knows>(smith).find(|&(k, _)| k == npc)?;
                Some((t[npc].name, likes.0, knows.1 .0))
            })
            .collect();
        assert_eq!(both, [("baker", 8, 9), ("witch", 5, 3)]);
        for (name, likes, knows) in both {
            println!("{name} (likes {likes}, knows {knows})");
        }
    });
}
```

### P3: Keeping the graph correct under change

```rust
#![feature(decl_macro)]
use std::error::Error;
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings needs Start, Plays);

#[derive(Debug)]
pub struct Room {
    pub name: &'static str,
}
kind!(Room); // rank 255 (the default): anything ranked lower fits in it

#[derive(Debug)]
pub struct Player;
kind!(Player needs At);

#[derive(Debug)]
pub struct Item {
    pub name: &'static str,
}
kind!(Item rank 0 needs In); // holds nothing

/// A container that can itself be put away. Its rank is in its type:
/// a `Chest<2>` holds `Chest<1>`s and `Item`s, never another `Chest<2>`.
#[derive(Debug)]
pub struct Chest<const R: u8> {
    pub name: &'static str,
}
kind!(Chest<const R: u8> rank R needs In);

set!(pub Thing); // what can be put somewhere
set!(pub Holder); // what things can be put in
is!(Item: Thing);
is!(Chest<const R: u8>: Thing, Holder);
is!(Room, Player: Holder);

// `Chest` is both a `Thing` and a `Holder`, so `In` could loop. `ranked` makes every edge
// go from a lower rank to a higher one, so no loop can be written: `cargo check` refuses it.
// Without `ranked` (or `cyclic`), `game!` below doesn't compile.
relation!(pub struct In: Thing -> Holder, one, ranked, cascade);
relation!(pub struct Exit(pub &'static str): Room <-> Room, many, cyclic, detach);
relation!(pub struct At: Player -> Room, one, refuse);
relation!(pub struct Start: Settings -> Room, one, refuse);
relation!(pub struct Plays: Settings -> Player, one, refuse);

module!(pub Keep {
    kinds: [Room, Player, Item, Chest<1>, Chest<2>],
    relations: [In, Exit, At, Start, Plays],
});
game!(pub Game: Settings [Keep]);

fn room<'t>(t: &Turn<'t, Game>, name: &str) -> Result<Id<'t, Room>, String> {
    t.all::<Room>().find(|&r| t[r].name == name).ok_or(format!("no room {name}"))
}

fn item<'t>(t: &Turn<'t, Game>, name: &str) -> Result<Id<'t, Item>, String> {
    t.all::<Item>().find(|&i| t[i].name == name).ok_or(format!("no item {name}"))
}

fn chest<'t, const R: u8>(t: &Turn<'t, Game>, name: &str) -> Result<Id<'t, Chest<R>>, String>
where
    Game: Has<Chest<R>>,
{
    t.all::<Chest<R>>().find(|&c| t[c].name == name).ok_or(format!("no chest {name}"))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let cellar = t.spawn(Room { name: "cellar" }, ());
        t.relate(hall, (Exit("down"), Exit("up")), cellar);
        let big = t.spawn(Chest::<2> { name: "big chest" }, (In, hall));
        let small = t.spawn(Chest::<1> { name: "small chest" }, (In, big));
        t.spawn(Item { name: "coin" }, (In, small));
        t.spawn(Item { name: "key" }, (In, small));
        let me = t.spawn(Player, (At, hall));
        t.spawn(Settings, ((Start, hall), (Plays, me)))
    });
    world.check()?;

    world.turn(|t| -> Result<(), Box<dyn Error>> {
        let (hall, cellar) = (room(t, "hall")?, room(t, "cellar")?);
        let (big, small) = (chest::<2>(t, "big chest")?, chest::<1>(t, "small chest")?);
        let key = item(t, "key")?;

        // 1. Move the outer chest. One edge changes; everything inside comes along.
        t.relate(big, In, cellar)?;
        assert_eq!(t.target::<In>(big), cellar);
        assert_eq!(t.descendants::<In>(cellar).count(), 4);
        assert_eq!(t.ancestors::<In>(key).collect::<Vec<_>>(), [small.up::<Holder>(), big.up(), cellar.up()]);
        assert_eq!(t.sources::<In>(hall).count(), 0);

        // 2. Forbidden states don't compile, so there is nothing to refuse at runtime:
        // t.relate(big, In, big);          // E0277: `Chest<2>` doesn't fit in `Chest<2>`: `In` is ranked
        // t.relate(big, In, small);        // E0277: `Chest<2>` doesn't fit in `Chest<1>`: `In` is ranked
        // t.relate(cellar, In, big);       // E0277: `Room` is not a `Thing`
        // t.unrelate::<In>(big, cellar);   // E0277: `In` is `one`; move it, don't drop it

        // 3. The door is one fact, readable from both sides with its own label.
        let from_hall: Vec<_> = t.targets::<Exit>(hall).map(|(r, e)| (r, e.0)).collect();
        let from_cellar: Vec<_> = t.targets::<Exit>(cellar).map(|(r, e)| (r, e.0)).collect();
        assert_eq!(from_hall, [(cellar, "down")]);
        assert_eq!(from_cellar, [(hall, "up")]);

        // 4. Collapse the cellar. It's doomed now and goes at the end of this turn:
        //    `In` cascades (both chests and their contents), `Exit` detaches.
        t.despawn(cellar)?;
        assert!(t.is_doomed(key));
        t.relate(key, In, hall)?; // grab the key out of the rubble first
        assert!(!t.is_doomed(key));
        // The hall is the start room and the player stands in it: `Start`/`At` refuse.
        assert_eq!(t.despawn(hall), Err(Refused::Held));
        Ok(())
    })?;
    world.check()?;

    world.turn(|t| -> Result<(), Box<dyn Error>> {
        let hall = room(t, "hall")?;
        assert!(room(t, "cellar").is_err());
        assert_eq!(t.all::<Chest<2>>().count() + t.all::<Chest<1>>().count(), 0);
        let left: Vec<_> = t.all::<Item>().map(|i| t[i].name).collect();
        assert_eq!(left, ["key"]);
        assert_eq!(t.target::<In>(t.all::<Item>().next().ok_or("no key")?), hall);
        assert_eq!(t.targets::<Exit>(hall).count(), 0); // no exit leads into the rubble
        Ok(())
    })?;
    world.check()?;

    print!("{world}");
    Ok(())
}
```

### P4: Extending a game

```rust
#![feature(decl_macro)]
use std::error::Error;
use weft::*;

/// A reusable module. A game drops it in with one path in `game!` and one
/// `is!` line naming which of its own kinds can hold items.
pub mod containers {
    use weft::*;

    #[derive(Debug)]
    pub struct Item {
        pub name: &'static str,
    }
    kind!(Item needs In);

    set!(pub Holder);

    relation!(pub struct In: Item -> Holder, one, cascade);
    component!(pub struct Weight(pub u32): Item);

    module!(pub Containers { kinds: [Item], relations: [In], components: [Weight] });

    /// Total weight of everything inside `holder`, at any depth.
    pub fn load<'t, G: Containers>(t: &Turn<'t, G>, holder: Id<'t, Holder>) -> u32 {
        t.descendants::<In>(holder)
            .filter_map(|item| t.get::<Weight>(item))
            .fold(0, |sum, w| sum.saturating_add(w.0))
    }
}

use containers::{Holder, In, Item, Weight};

#[derive(Debug)]
pub struct Settings;
kind!(root Settings needs Plays);

#[derive(Debug)]
pub struct Room {
    pub name: &'static str,
}
kind!(Room);

#[derive(Debug)]
pub struct Player;
kind!(Player needs At);

#[derive(Debug)]
pub struct Npc {
    pub name: &'static str,
}
kind!(Npc needs At);

#[derive(Debug)]
pub struct Door {
    pub name: &'static str,
}
kind!(Door needs Opens);

set!(pub Walker);
is!(Player, Npc: Walker);
is!(Room, Player, Npc: Holder); // plug this game's kinds into the module's set

relation!(pub struct At: Walker -> Room, one, refuse);
relation!(pub struct Plays: Settings -> Player, one, refuse);
relation!(pub struct Opens: Door -> Room, one, cascade);
// The new relation, every rule in one line: an NPC guards at most one door (`opt`),
// only NPCs guard and only doors are guarded, and a destroyed door just stops being guarded.
relation!(pub struct Guards: Npc -> Door, opt, detach);
// The new component: only items can be lit. No edit to `Item` or `containers`.
component!(pub struct Lit(pub bool): Item);

module!(pub Town {
    kinds: [Room, Player, Npc, Door],
    relations: [At, Plays, Opens, Guards],
    components: [Lit],
});
// Modules are named by path. Two modules called `Containers` in different crates don't collide.
game!(pub Game: Settings [Town, containers::Containers]);

/// `give <noun> to <npc>`. The bounds are the whole story: it touches `In` and `At`
/// and the kinds they connect. `Guards` and `Lit` are unreachable from here.
pub fn give<'t, G>(t: &mut Turn<'t, G>, me: Id<'t, Player>, noun: &str, to: &str) -> Result<String, Refused>
where
    G: Has<In> + Has<At> + Has<Item> + Has<Player> + Has<Npc> + Has<Room>,
{
    let here = t.target::<At>(me);
    let item = t.sources::<In>(me).map(|(item, _)| item).find(|&item| t[item].name == noun);
    let npc = t
        .sources::<At>(here)
        .filter_map(|(walker, _)| t.cast::<Npc>(walker))
        .find(|&npc| t[npc].name == to);
    match (item, npc) {
        (None, _) => Ok(format!("You have no {noun}.")),
        (_, None) => Ok(format!("There is no {to} here.")),
        (Some(item), Some(npc)) => {
            t.relate(item, In, npc)?;
            Ok(format!("You give the {noun} to the {to}."))
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut world = World::<Game>::new(|t| {
        let square = t.spawn(Room { name: "square" }, ());
        let vault = t.spawn(Room { name: "vault" }, ());
        let door = t.spawn(Door { name: "vault door" }, (Opens, vault));
        let gate = t.spawn(Door { name: "gate" }, (Opens, square));
        let me = t.spawn(Player, (At, square));
        let smith = t.spawn(Npc { name: "smith" }, (At, square));
        let lamp = t.spawn(Item { name: "lamp" }, (In, me));
        t.insert(lamp, Lit(true));
        t.insert(lamp, Weight(2));
        t.relate(smith, Guards, door);
        t.relate(smith, Guards, gate); // `opt`: replaces, so the smith guards only the gate
        t.spawn(Settings, (Plays, me))
    });

    world.turn(|t| -> Result<(), Box<dyn Error>> {
        let me = t.target::<Plays>(t.root());
        println!("{}", give(t, me, "lamp", "smith")?); // "You give the lamp to the smith."

        let smith = t.all::<Npc>().find(|&n| t[n].name == "smith").ok_or("no smith")?;
        let lamp = t.all::<Item>().find(|&i| t[i].name == "lamp").ok_or("no lamp")?;
        assert_eq!(t.target::<In>(lamp), smith);
        assert_eq!(t.get::<Lit>(lamp).map(|l| l.0), Some(true));
        assert_eq!(containers::load(t, smith.up()), 2);
        assert_eq!(t.target::<Guards>(smith).map(|d| t[d].name), Some("gate"));
        Ok(())
    })?;
    world.check()?;
    Ok(())
}
```

### P5: Testing a game

````rust
//! `game/src/lib.rs`: a small game crate, with its tests.
#![feature(decl_macro)]
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings);

#[derive(Debug)]
pub struct Room {
    pub name: &'static str,
}
kind!(Room);

#[derive(Debug)]
pub struct Player;
kind!(Player needs At);

#[derive(Debug)]
pub struct Item {
    pub name: &'static str,
}
kind!(Item rank 0 needs In);

#[derive(Debug)]
pub struct Chest<const R: u8> {
    pub name: &'static str,
}
kind!(Chest<const R: u8> rank R needs In);

#[derive(Debug)]
pub struct Npc {
    pub name: &'static str,
}
kind!(Npc);

set!(pub Thing);
is!(Item: Thing);
set!(pub Holder);
is!(Room, Player: Holder);
is!(Chest<const R: u8>: Thing, Holder);

relation!(pub struct In: Thing -> Holder, one, ranked, cascade);
relation!(pub struct At: Player -> Room, one, refuse);
relation!(pub struct Exit(pub &'static str): Room <-> Room, many, cyclic, detach);
relation!(pub struct Likes(pub u8): Npc -> Npc, many, cyclic, detach);

module!(pub Map { kinds: [Room, Player, Item, Chest<1>, Chest<2>], relations: [In, At, Exit] });
module!(pub Folk { kinds: [Npc], relations: [Likes] });
game!(pub Game: Settings [Map, Folk]);

/// `take <noun>`: needs only `In`, `At` and the kinds they connect.
pub fn take<'t, G>(t: &mut Turn<'t, G>, me: Id<'t, Player>, noun: &str) -> Result<bool, Refused>
where
    G: Has<In> + Has<At> + Has<Item> + Has<Player> + Has<Room>,
{
    let here = t.target::<At>(me);
    let found = t
        .sources::<In>(here)
        .filter_map(|(thing, _)| t.cast::<Item>(thing))
        .find(|&item| t[item].name == noun);
    match found {
        Some(item) => t.relate(item, In, me).map(|()| true),
        None => Ok(false),
    }
}

/// A room, a player and a lamp, in any game that has a `Map`.
pub fn small<G: Map + Schema<Root = Settings>>() -> World<G> {
    World::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        t.spawn(Player, (At, hall));
        t.spawn(Item { name: "lamp" }, (In, hall));
        t.spawn(Settings, ())
    })
}

/// Invariants that must not compile. Each block is a `compile_fail` doctest, and the
/// pinned nightly makes rustdoc check the stated error code.
///
/// An item in no container:
/// ```compile_fail,E0271
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     t.spawn(Item { name: "lamp" }, ()); // `Item needs In`
///     t.spawn(Settings, ())
/// });
/// ```
/// An item in two containers:
/// ```compile_fail,E0271
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     let hall = t.spawn(Room { name: "hall" }, ());
///     let me = t.spawn(Player, (At, hall));
///     t.spawn(Item { name: "lamp" }, ((In, hall), (In, me))); // `In` is `one`
///     t.spawn(Settings, ())
/// });
/// ```
/// Taking an item out of every container:
/// ```compile_fail,E0277
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     let hall = t.spawn(Room { name: "hall" }, ());
///     let lamp = t.spawn(Item { name: "lamp" }, (In, hall));
///     t.unrelate::<In>(lamp, hall); // `In` is `one`: there's no unrelate
///     t.spawn(Settings, ())
/// });
/// ```
/// A handle outliving its turn (the stale-handle case):
/// ```compile_fail
/// use game::*;
/// let mut world = game::small::<Game>();
/// let lamp = world.turn(|t| t.all::<Item>().next()); // `Id<'t, _>` can't leave the turn
/// ```
/// A one-way door where the game declared two-way exits:
/// ```compile_fail,E0277
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     let hall = t.spawn(Room { name: "hall" }, ());
///     let cellar = t.spawn(Room { name: "cellar" }, ());
///     t.relate(hall, Exit("down"), cellar); // `Exit` is `<->`: give both labels
///     t.spawn(Settings, ())
/// });
/// ```
/// A room inside an item:
/// ```compile_fail,E0277
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     let hall = t.spawn(Room { name: "hall" }, ());
///     let chest = t.spawn(Chest::<1> { name: "chest" }, (In, hall));
///     t.relate(hall, In, chest); // `Room` is not a `Thing`
///     t.spawn(Settings, ())
/// });
/// ```
/// A chest inside itself:
/// ```compile_fail,E0277
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     let hall = t.spawn(Room { name: "hall" }, ());
///     let chest = t.spawn(Chest::<1> { name: "chest" }, (In, hall));
///     t.relate(chest, In, chest); // `In` is ranked: `Chest<1>` only fits above rank 1
///     t.spawn(Settings, ())
/// });
/// ```
/// A chest inside its own contents:
/// ```compile_fail,E0277
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     let hall = t.spawn(Room { name: "hall" }, ());
///     let big = t.spawn(Chest::<2> { name: "big" }, (In, hall));
///     let small = t.spawn(Chest::<1> { name: "small" }, (In, big));
///     t.relate(big, In, small); // rank 2 doesn't fit in rank 1
///     t.spawn(Settings, ())
/// });
/// ```
/// A wrong endpoint:
/// ```compile_fail,E0277
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     let hall = t.spawn(Room { name: "hall" }, ());
///     let lamp = t.spawn(Item { name: "lamp" }, (In, hall));
///     t.spawn(Player, (At, lamp)); // `At` only targets `Room`
///     t.spawn(Settings, ())
/// });
/// ```
/// Reading data through a set handle (what used to hit the `Default` fallback):
/// ```compile_fail,E0277
/// use game::*;
/// let mut world = game::small::<Game>();
/// world.turn(|t| {
///     if let Some(hall) = t.all::<Room>().next() {
///         for (thing, _) in t.sources::<In>(hall) {
///             let _ = t[thing].name; // `Thing` is a set: `cast` it to a kind first
///         }
///     }
/// });
/// ```
/// A conflicting query: changing the graph while iterating it:
/// ```compile_fail,E0502
/// use game::*;
/// let mut world = game::small::<Game>();
/// world.turn(|t| {
///     let hall = t.all::<Room>().next();
///     let me = t.all::<Player>().next();
///     if let (Some(hall), Some(me)) = (hall, me) {
///         for (thing, _) in t.sources::<In>(hall) {
///             if let Some(item) = t.cast::<Item>(thing) {
///                 let _ = t.relate(item, In, me);
///             }
///         }
///     }
/// });
/// ```
pub mod proofs {}

#[cfg(test)]
mod tests {
    use super::*;

    // Only what `take` touches: no `Npc`, no `Likes`.
    game!(Bare: Settings [Map]);

    #[test]
    fn take_moves_the_lamp() {
        let mut world = small::<Game>();
        world.turn(|t| {
            let me = t.all::<Player>().next().unwrap();
            let hall = t.target::<At>(me);
            assert_eq!(take(t, me, "lamp"), Ok(true));
            let lamp = t.all::<Item>().next().unwrap();
            assert_eq!(t.target::<In>(lamp), me);
            assert_eq!(t.sources::<In>(hall).count(), 0);
        });
        world.check().unwrap();
    }

    #[test]
    fn take_in_isolation() {
        let mut world = small::<Bare>();
        world.turn(|t| {
            let me = t.all::<Player>().next().unwrap();
            assert_eq!(take(t, me, "sword"), Ok(false));
            assert_eq!(take(t, me, "lamp"), Ok(true));
            assert_eq!(take(t, me, "lamp"), Ok(false)); // already carried
        });
        world.check().unwrap();
    }

    #[test]
    fn snapshot() {
        let mut world = small::<Bare>();
        world
            .turn(|t| {
                let me = t.all::<Player>().next().unwrap();
                take(t, me, "lamp")
            })
            .unwrap();
        let expected = "\
Settings#0 Settings
Room#0 Room { name: \"hall\" }
Player#0 Player
  At -> Room#0
Item#0 Item { name: \"lamp\" }
  In -> Player#0
";
        assert_eq!(world.to_string(), expected);
    }

    #[test]
    fn chests_nest_move_and_cascade() {
        let mut world = small::<Bare>();
        world.turn(|t| {
            let hall = t.all::<Room>().next().unwrap();
            let me = t.all::<Player>().next().unwrap();
            let big = t.spawn(Chest::<2> { name: "big" }, (In, hall));
            let small = t.spawn(Chest::<1> { name: "small" }, (In, big));
            let coin = t.spawn(Item { name: "coin" }, (In, small));
            t.relate(big, In, me).unwrap();
            assert_eq!(t.ancestors::<In>(coin).collect::<Vec<_>>(), [small.up::<Holder>(), big.up(), me.up()]);
            t.despawn(big).unwrap(); // `In` cascades: the small chest and the coin go too
            assert!(t.is_doomed(coin));
        });
        world.check().unwrap();
        world.turn(|t| {
            assert_eq!(t.all::<Chest<1>>().count(), 0);
            assert_eq!(t.all::<Item>().map(|i| t[i].name).collect::<Vec<_>>(), ["lamp"]);
        });
        world.check().unwrap();
    }
}
````

### P6: Configuring a game

```rust
#![feature(decl_macro)]
use weft::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    #[default]
    Normal,
    Hard,
}

/// Game-wide settings are the root entity's data: one per world, always there,
/// typed, and reachable from every turn with `t.settings()`.
#[derive(Debug, Default)]
pub struct Settings {
    pub turn_limit: u32,
    pub difficulty: Difficulty,
}
// The starting room is an edge, not a field. `Start` refuses despawn, so the
// setting can never name a room that's gone.
kind!(root Settings needs Start, Plays);

#[derive(Debug)]
pub struct Room {
    pub name: &'static str,
}
kind!(Room);

#[derive(Debug)]
pub struct Player;
kind!(Player needs At);

#[derive(Debug)]
pub struct Npc {
    pub name: &'static str,
}
kind!(Npc needs At);

#[derive(Debug)]
pub struct Item {
    pub name: &'static str,
}
kind!(Item rank 0 needs In);

#[derive(Debug)]
pub struct Chest<const R: u8> {
    pub name: &'static str,
}
kind!(Chest<const R: u8> rank R needs In);

set!(pub Walker);
is!(Player, Npc: Walker);
set!(pub Thing);
is!(Item: Thing);
set!(pub Holder);
is!(Room, Player: Holder);
// Nesting on: a chest is both a thing and a holder, and `ranked` keeps `In` loop-free at
// compile time. To turn nesting off, drop this line and `Chest<1>` from `Containers`.
is!(Chest<const R: u8>: Thing, Holder);

// Each relation's rules are chosen where it's declared.
// Two-way (`<->`): relate takes both labels, unrelate removes both.
relation!(pub struct Exit(pub &'static str): Room <-> Room, many, cyclic, detach);
// One-way (`->`): a chute you can't climb back up.
relation!(pub struct Chute: Room -> Room, many, cyclic, detach);
// Destroying a chest spills its contents into whatever held it.
relation!(pub struct In: Thing -> Holder, one, ranked, spill);
// A room with someone standing in it can't be destroyed. Arriving runs `arrive`.
relation!(pub struct At: Walker -> Room, one, refuse, on_relate = arrive: Map);
relation!(pub struct Start: Settings -> Room, one, refuse);
relation!(pub struct Plays: Settings -> Player, one, refuse);
relation!(pub struct Likes(pub u8): Npc -> Npc, many, cyclic, detach);

module!(pub Map { kinds: [Room, Player], relations: [Exit, Chute, At, Start, Plays] });
module!(pub Containers { kinds: [Item, Chest<1>], relations: [In] });
module!(pub Folk { kinds: [Npc], relations: [Likes] });

// Parts on or off: two games from the same modules.
game!(pub Full: Settings [Map, Containers, Folk]);
game!(pub Quiet: Settings [Map, Containers]); // no NPCs, e.g. for tests

fn arrive<'t, G: Map>(t: &mut Turn<'t, G>, who: Id<'t, Walker>, room: Id<'t, Room>) -> Result<(), Refused> {
    if t.cast::<Player>(who).is_some() {
        println!("You arrive in the {}.", t[room].name);
    }
    Ok(())
}

fn build<G>(settings: Settings) -> World<G>
where
    G: Map + Containers + Schema<Root = Settings>,
{
    World::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let pit = t.spawn(Room { name: "pit" }, ());
        t.relate(hall, Chute, pit);
        t.spawn(Item { name: "lamp" }, (In, hall));
        let box_ = t.spawn(Chest::<1> { name: "box" }, (In, hall));
        t.spawn(Item { name: "coin" }, (In, box_));
        let me = t.spawn(Player, (At, hall));
        t.spawn(settings, ((Start, hall), (Plays, me)))
    })
}

/// NPC behaviour. Only compiles for games that include `Folk`.
fn haunt<'t, G>(t: &mut Turn<'t, G>) -> Result<(), Refused>
where
    G: Map + Folk + Schema<Root = Settings>,
{
    if t.settings().difficulty != Difficulty::Hard {
        return Ok(());
    }
    let me = t.target::<Plays>(t.root());
    let here = t.target::<At>(me);
    let npcs: Vec<_> = t.all::<Npc>().collect();
    for npc in npcs {
        t.relate(npc, At, here)?;
    }
    Ok(())
}

fn play<G>(
    world: &mut World<G>,
    commands: &[&str],
    mut npcs: impl for<'t> FnMut(&mut Turn<'t, G>) -> Result<(), Refused>,
) -> Result<u32, Refused>
where
    G: Map + Containers + Schema<Root = Settings>,
{
    let mut turns = 0;
    for &command in commands {
        let over = world.turn(|t| -> Result<bool, Refused> {
            if turns >= t.settings().turn_limit {
                return Ok(true);
            }
            let me = t.target::<Plays>(t.root());
            let here = t.target::<At>(me);
            match command {
                "jump" => {
                    let below = t.targets::<Chute>(here).map(|(room, _)| room).next();
                    if let Some(below) = below {
                        t.relate(me, At, below)?;
                    }
                }
                "take lamp" => {
                    let lamp = t
                        .sources::<In>(here)
                        .filter_map(|(thing, _)| t.cast::<Item>(thing))
                        .find(|&i| t[i].name == "lamp");
                    if let Some(lamp) = lamp {
                        t.relate(lamp, In, me)?;
                    }
                }
                _ => println!("Nothing happens."),
            }
            npcs(t)?;
            Ok(false)
        })?;
        if over {
            break;
        }
        turns += 1;
    }
    Ok(turns)
}

fn main() -> Result<(), Refused> {
    let mut full: World<Full> = build(Settings { turn_limit: 20, difficulty: Difficulty::Hard });
    full.turn(|t| {
        let start = t.target::<Start>(t.root());
        t.spawn(Npc { name: "ghost" }, (At, start)).map(drop)
    })?;
    play(&mut full, &["take lamp", "jump"], haunt::<Full>)?;

    let mut quiet: World<Quiet> = build(Settings { turn_limit: 5, ..Settings::default() });
    play(&mut quiet, &["take lamp", "jump"], |_| Ok(()))?;
    // play(&mut quiet, &[], haunt::<Quiet>); // doesn't compile: `Quiet` has no `Npc`, no `Likes`

    // Retune mid-game: settings are ordinary data on the root.
    full.turn(|t| t.settings_mut().difficulty = Difficulty::Easy);
    print!("{full}");
    Ok(())
}
```

### P7: The accepted weaknesses

```rust
#![feature(decl_macro)]
use std::error::Error;
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings needs Start, Plays);

// No kind derives `Default`: nothing is ever read from a made-up row.
#[derive(Debug)]
pub struct Room {
    pub name: &'static str,
}
kind!(Room);

#[derive(Debug)]
pub struct Player;
kind!(Player needs At);

#[derive(Debug)]
pub struct Npc {
    pub name: &'static str,
}
kind!(Npc needs At);

#[derive(Debug)]
pub struct Item {
    pub name: &'static str,
}
kind!(Item rank 0 needs In);

#[derive(Debug)]
pub struct Chest<const R: u8> {
    pub name: &'static str,
}
kind!(Chest<const R: u8> rank R needs In);

set!(pub Walker);
is!(Player, Npc: Walker);
set!(pub Thing);
is!(Item: Thing);
set!(pub Holder);
is!(Room, Player: Holder);
is!(Chest<const R: u8>: Thing, Holder);

relation!(pub struct In: Thing -> Holder, one, ranked, spill);
// The hook's bound is a whole module, not just `At`: it may move items through `In`.
// Every caller that relates `At` carries `Map` in its own bounds, so the effect is visible.
relation!(pub struct At: Walker -> Room, one, refuse, on_relate = pick_up: Map);
relation!(pub struct Start: Settings -> Room, one, refuse);
relation!(pub struct Plays: Settings -> Player, one, refuse);

module!(pub Map {
    kinds: [Room, Player, Npc, Item, Chest<1>],
    relations: [In, At, Start, Plays],
});
game!(pub Game: Settings [Map]);

/// When the player enters a room, they pick up every loose item in it.
fn pick_up<'t, G: Map>(t: &mut Turn<'t, G>, who: Id<'t, Walker>, room: Id<'t, Room>) -> Result<(), Refused> {
    let Some(me) = t.cast::<Player>(who) else { return Ok(()) };
    let loose: Vec<_> = t.sources::<In>(room).filter_map(|(thing, _)| t.cast::<Item>(thing)).collect();
    for item in loose {
        t.relate(item, In, me)?;
        println!("You pick up the {}.", t[item].name);
    }
    Ok(())
}

fn attic<'t>(t: &Turn<'t, Game>) -> Result<Id<'t, Room>, &'static str> {
    t.all::<Room>().find(|&r| t[r].name == "attic").ok_or("no attic")
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let attic = t.spawn(Room { name: "attic" }, ());
        let crate_ = t.spawn(Chest::<1> { name: "crate" }, (In, attic));
        t.spawn(Item { name: "lamp" }, (In, attic));
        t.spawn(Item { name: "coin" }, (In, crate_));
        t.spawn(Npc { name: "ghost" }, (At, attic));
        let me = t.spawn(Player, (At, hall)); // genesis runs no hooks
        t.spawn(Settings, ((Start, hall), (Plays, me)))
    });

    world.turn(|t| -> Result<(), Box<dyn Error>> {
        let me = t.target::<Plays>(t.root());
        let attic = attic(t)?;

        // A hook that touches another relation: entering runs `pick_up`, which relates `In`.
        t.relate(me, At, attic)?; // prints "You pick up the lamp."
        let carried: Vec<_> = t.sources::<In>(me).filter_map(|(x, _)| t.cast::<Item>(x)).map(|i| t[i].name).collect();
        assert_eq!(carried, ["lamp"]); // the crate stays: `pick_up` only takes `Item`s

        // A chest inside itself doesn't compile; a legal nesting does.
        let crate_ = t.all::<Chest<1>>().next().ok_or("no crate")?;
        // t.relate(crate_, In, crate_);  // E0277: `Chest<1>` doesn't fit in `Chest<1>`: `In` is ranked

        // Reads are total. The `one` edge is stored with the crate, so `target` is an `Id`,
        // not an `Option`, and there is no fallback row behind `t[..]`.
        let holder = t.target::<In>(crate_);
        assert_eq!(holder, attic);
        // let _ = t[holder];             // E0277: `Holder` is a set; it has no data to read

        // Narrowing is identity: `cast` finds this very node among the game's `Npc`s.
        let walkers: Vec<_> = t.sources::<At>(attic).map(|(w, _)| w).collect();
        let npcs: Vec<_> = walkers.iter().filter_map(|&w| t.cast::<Npc>(w)).map(|n| t[n].name).collect();
        assert_eq!(npcs, ["ghost"]);
        assert_eq!(walkers.iter().filter(|&&w| t.cast::<Player>(w).is_some()).count(), 1);
        // let _ = t.cast::<Room>(walkers[0]);   // E0277: `Room` is not a `Walker`
        // let _ = t.target::<At>(walkers[0]);   // E0277: `target` needs a kind; `cast` the walker first

        t.despawn(crate_)?; // `In` spills: the coin moves to the attic at the end of the turn
        Ok(())
    })?;
    world.check()?;

    world.turn(|t| -> Result<(), Box<dyn Error>> {
        let attic = attic(t)?;
        let coin = t.all::<Item>().find(|&i| t[i].name == "coin").ok_or("no coin")?;
        assert_eq!(t.target::<In>(coin), attic);
        assert_eq!(t.all::<Chest<1>>().count(), 0);
        Ok(())
    })?;
    world.check()?;

    print!("{world}");
    Ok(())
}
```

### P8: The executor hits a snag

````rust
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
````

## What would reopen this

Only these pieces of new evidence justify revisiting the decision:

- **Ranks fail.** On the pinned nightly, the `ranked` rule can't make P5's `chest_in_itself` and `chest_in_its_contents` fail under `cargo check` while P1–P7 compile unannotated. That means an ICE, a failure that shows only at `cargo build`, or E0283 in P1–P7's generic code. It must also fail after the allowed fallback to Peano ranks.
- **Total reads fail.** A total-read representation can't meet C1, C6 and C17 together without `unsafe`, a panic, or a read path with a miss arm.
- **Declaration probes fail.** a's stable `game!` probes stop producing `cargo check` errors for C12 or the overlap case in C5 once weft enables `generic_const_exprs`, or under the old solver it falls back to. No other route without `specialization` works either.
- **Inference fails.** C14 can't reach `??ty: 0, ?ty: 0` with the pinned toolchain's rust-analyzer because it doesn't evaluate `generic_const_exprs`. That puts the nightly bet against the brief's "rust-analyzer feedback while typing is the main experience".
- **A modelling need ranks can't express.** A canonical program the user adds needs same-kind nesting without static layering, such as two identical bags that can each hold the other.
- **Cross-crate `game!` fails.** C13 fails for a downstream `game!` even after the blanket `Has<S>` is replaced.

Nothing else reopens it. That includes:

- executor deviations that keep every claim green;
- toolchain warnings that don't break a gate;
- library size;
- memory growth from append-only edge chains, because performance is a non-goal.