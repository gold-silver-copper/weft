# weft: design

How weft works and why. The decisions behind it are recorded in [`docs/design/0001-ecs.md`](docs/design/0001-ecs.md) (typed entity kinds) and [`docs/design/0002-improve-ecs.md`](docs/design/0002-improve-ecs.md), which amends it. Where the implementation departs from them, [`docs/design/0001-ecs-deviations.md`](docs/design/0001-ecs-deviations.md) says so, and [`docs/design/0001-ecs-report.md`](docs/design/0001-ecs-report.md) sums up the run that built it.

## Goals

A text game is a graph of relations: rooms, exits, what is in what, who likes whom. weft keeps that graph correct, and makes as much of its correctness as Rust allows a type error that rust-analyzer shows as you type:

- no handle outlives its entity, and no read can miss;
- relation endpoints, arities, loops and two-way labels are checked by type;
- what a hook may touch is part of the signature of everything that can trigger it;
- the rest, graph state no type can see, is refused at runtime with nothing changed.

No `unsafe`, no dependencies, no panics. `src/lib.rs` forbids every panicking construct at `forbid` level, and `clippy.toml` bans the std APIs that panic on bad input.

## The pieces

| declaration | what it makes |
|---|---|
| `kind!(Item rank 0 needs In)` | an entity kind: a struct whose data is fixed at spawn, its rank, and the `one` edges it can't exist without |
| `set!(pub Holder)`, `is!(Room, Player: Holder)` | an open union of kinds, extendable from any crate |
| `relation!(pub struct In: Thing -> Holder, one, ranked, spill)` | a relation: endpoints, arity (`one`/`opt`/`many`), shape (`->`/`<->`), loop rule (`ranked`/`cyclic`), despawn policy (`cascade`/`detach`/`spill`/`refuse`), optional `on_relate` hook |
| `component!(pub struct Weight(pub u32): Item)` | changeable per-entity state |
| `module!(pub Map { kinds: [..], relations: [..], components: [..] })` | a trait meaning "the game has all of this", and a macro `game!` calls by path |
| `game!(pub Game: Settings [Map, other::Containers])` | the schema: storage for every part, `Has<X>` impls, and the declaration checks |

## Handles and total reads

A turn is `World::turn(|t| ..)`, with `t: &mut Turn<'t, G>`. Handles are `Id<'t, K>`: `'t` is invariant and higher-ranked, so a handle can't be returned from the closure or used in another world's turn. Despawn is deferred to the end of the turn, so a handle's entity exists for as long as the handle.

The turn borrows the graph shared for all of `'t` and still grows it. Every node is an `Rc<Node<K>>` kept in an append-only `Chain` whose links are `OnceCell`s: a link, once written, never moves, so a `&'t` reference into it is sound without `unsafe`. A handle is such a reference. `t[id]` is `&id.node().data`; a `one` edge lives in its source's node as a slot whose moves are appended, so `target` reads the latest one. Neither read has a miss branch, and kinds need no `Default`. Set handles point at an erased node and can't be indexed: `cast` narrows one to a kind by the node's identity, never by a name.

Between turns the world has `&mut` access and compacts: doomed nodes and edges are swept, and chains are rebuilt. Dropping the world drops every node, iteratively, so long chains can't overflow the stack. `Rc` can't free a loop, so `game!` rejects `one` relations that could let a node own itself through other nodes' slots.

## Relations

Each relation's edges live in a `RelLog`, oldest first; a `one` edge's entry shares its slot with the source node. Two-way (`<->`) relations are `many`, and an edge stores a value for each end: `targets` from either end shows the other end with the value on its own side. Relating a `one` or `opt` edge again moves it, `many` edges are unique per pair, and `one` relations have no `unrelate`.

**Ranks** are types: `Zero`, `Up<R>`, and `Top` for kinds without a rank; `R0`–`R15` name the first sixteen. A `ranked` relation requires the source's rank below the target's, as a trait (`Below`), so a chest can't go into itself or into anything it contains, and rust-analyzer reports it natively. An earlier version used const generics; rust-analyzer can't type a const-generic literal like the `1` in `Chest::<1>` (deviation D5).

**Hooks.** `on_relate = f: Bound` implements `OnRelate<G>` only for games meeting `Bound`, and `relate` requires `OnRelate<G>`, so every caller carries the bound and a hook can't reach past it. A hook runs after its edge is made; if it refuses, the edge and its earlier steps stay, each valid. Hooks may nest 64 deep and run 16,384 times per operation; beyond that, the next hooked operation is refused with `Refused::Unsettled` before it changes anything. Genesis runs no hooks.

## Despawn

`despawn` dooms an entity. The plan for the turn is the fixpoint of: everything doomed, every `cascade` source pointing into the plan, and every `spill` source that can't land (walking up its relation, no holder survives). A despawn whose plan a `refuse` edge from outside would break is refused with `Held`, and relating into a doomed entity is refused with `Doomed`; so the plan can't fail when the turn ends. Then `spill` sources move up, and the plan and every edge touching it are swept.

Whether an operation can be refused is a type: `Fallibility` is `Sure` (the value comes back bare, and no runtime check runs) or `Refused` (a `Result`). A live `spawn` is refusable exactly when one of its needed relations has a hook or is `refuse`.

## Declaration checks

`game!` asks, for every pair of its kinds and relations, questions answered at `cargo check` by inherent associated consts that only exist when their bounds hold (option a's stable technique, which sees `is!` lines from every crate): does a kind that can be a `one` relation's source `need` it; do a loop-free relation's endpoints overlap; can the root be a `cascade` or `spill` source; can `one` edges form an ownership loop. `relation!` checks `detach` on `one`, `spill` on non-`one`, and two-way non-`many` the moment it's declared.

## Verification

Everything the decision record claims has a gate that fails when the claim is false ([`docs/design/0001-ecs/claims.md`](docs/design/0001-ecs/claims.md)), and every gate has been seen to fail:

- `tests/cases.rs`: each `tests/reject` case must fail `cargo check` at its marked line with its stated code, and its `--cfg twin` must compile; the `tests/accept` cases must build and run; a doctest with a wrong error code must fail.
- `tests/fuzz.rs`: random worlds and turns against a naive model of the same rules, compared after every operation, with `World::check` after every turn; a planted model fault must be caught; hooks that never settle must be refused.
- `tests/mutants.rs`: three bugs planted in the library behind `cfg(weft_mutant = ..)` must each make a gate fail.
- `tests/programs.rs`, `cargo test -p game`: the canonical programs run and print what they promise.
- `tests/behaviour/claims.rs`: cast identity, the lint wall, no third-party crates, no `unsafe`/`TypeId`/`type_name`/`module_path!` in `src`, and drop freeing everything.
- `tests/deviations.rs`: nothing a gate depends on changes without a logged, fingerprinted deviation.

## Limits

- Nightly, for `decl_macro` (`module!` defines a macro that `game!` calls by path). The toolchain is pinned.
- Kind data is fixed at spawn; what changes lives in components. Set handles can't be indexed.
- A kind needs at most four relations; a game has at most 128 kinds; `kind!` takes numeric ranks up to 15 (name a rank type for more).
- Same-kind nesting needs ranks: two chests of the same rank can't nest, even where that wouldn't loop.
- `one` edges keep their history until the world drops. Lookups are linear scans. Performance is a non-goal.
