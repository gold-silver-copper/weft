# weft

An ECS where the compiler does the checking. No dependencies, `no_std` + `alloc`, no `unsafe`, stable Rust, and no code path in the crate that can panic.

```rust
use weft::*;

struct Pos(i32);
impl<S: Schema> Component<S> for Pos {}

struct Vel(i32);
impl<S: Schema> Component<S> for Vel {}

struct Likes(u32);
impl<S: Schema> Relation<S> for Likes {
    type Arity = Many;
}

schema! {
    struct Game {
        components: [mut Pos, Vel],     // only `mut` components can change in place
        relations: [Likes, ChildOf],
        resources: [],
    }
}

let mut world = World::<Game>::new();
let alice = world.spawn((Pos(0), Vel(1)));                       // -> Entity
let bob = world.spawn((Pos(9), Vel(-1), To(Likes(3), alice)))?;  // -> Result: alice might be dead

world.query_mut::<(&mut Pos, &Vel)>().for_each(|(mut pos, vel)| pos.0 += vel.0);
```

## Rejected at compile time

| mistake | error |
|---|---|
| using a component, relation or resource the schema doesn't list | `` `Name` is not a component of `Game` `` |
| changing a component the schema doesn't mark `mut` | `` `Id` is not mutable in `Game` `` |
| `(&mut Pos, &Pos)` or `(&mut Pos, &mut Pos)` in one query | `this query reads a component that it also borrows mutably` |
| reading, inside `for_each_with`, a component the query holds mutably | `this component is borrowed mutably by the query, so the view cannot read it` |
| `&mut T` in a query over `&World` | `` `&mut Pos` can't be fetched through `&World` `` |
| asking for *the* target of a many-to-many relation | `` type mismatch resolving `<Likes as Relation<Game>>::Arity == One` `` |
| ignoring that spawning with a relation can fail | `mismatched types` (it's a `Result`) |
| a component whose hooks need something the schema lacks | `` `Health` is not a component of `Game` `` at the `schema!` |

These are ordinary trait errors, so `cargo check` and rust-analyzer report them as you type.

## How it works

**Queries borrow like references do.** The schema's columns form a type-level list. `&mut T` lends its column out and leaves `Taken` behind. `&T` leaves `Shared<_>` behind, which can be lent again, but only as shared. A second borrow that conflicts has no trait impl to match, so the query doesn't compile. It's the borrow checker's rule, expressed in types. Whatever the query doesn't take stays readable through the `View`:

```rust
world.query_mut::<(&mut Pos, Targets<ChildOf>)>().for_each_with(|(mut pos, parents), view| {
    for &parent in parents {
        pos.0 += view.get::<Vel>(parent).map_or(0, |v| v.0);  // fine: the query doesn't hold Vel
        // view.get::<Pos>(parent)                            // compile error: the query holds Pos mutably
    }
});
```

**Capabilities are bounds.** Hooks, events and plugins are generic over the schema and state what they need. Using one in a schema that lacks those things doesn't compile:

```rust
impl<S: Writes<Health>> Component<S> for Poisoned { /* hooks may change Health */ }
fn physics<S: Borrows<(&'static mut Pos, &'static Vel)>>(world: &mut World<S>) { /* ... */ }
```

**Whether something can fail is in its type.** `Bundle::Error` is either `Infallible` or `NoSuchEntity`, and tuples combine them at the type level. So `spawn` returns an `Entity` or a `Result<Entity, NoSuchEntity>`, and there's never an error that can't happen but still has to be handled. Resources listed in the schema always exist, so `world.resource::<T>()` returns `&T`, not an `Option`.

**Each kind of callback is the closure trait that fits how it's called.** Hooks are `fn` pointers declared on the type: stateless, one per type. Observers are `Fn`, shared through `Rc`, because an observer can re-trigger itself and a shared `Fn` can be re-entered without a runtime borrow flag. Systems are `FnMut`: the schedule owns each one and runs it alone. Deferred work is `FnOnce`.

**No runtime borrow checking.** There is no `RefCell` anywhere. Reads take `&World`, writes take `&mut World`, and the only interior mutability is a `Cell<Vec<_>>` for deferred work, emptied and refilled with `take`/`set`, which can't fail.

## Panic-freedom

`src/lib.rs` forbids (not just denies) `unsafe_code`, `clippy::panic`, `unwrap_used`, `expect_used`, `indexing_slicing`, `arithmetic_side_effects`, `as_conversions`, `unreachable`, `todo`, `unimplemented` and friends. `clippy.toml` also bans every std API that can panic on bad input: `assert!` and its variants, `Vec::remove`/`swap_remove`/`insert`/`drain`/`split_off`, `BTreeMap::range`, `Iterator::sum`/`product`/`step_by`, and `RefCell`.

What remains outside the crate's control: running out of memory, stack overflow from hooks that recurse forever, and panics in your own hooks, observers and systems.

## Trade-offs

- Every component, relation and resource must be listed in a schema. Plugins stay reusable by being generic over `S` with capability bounds.
- `query_mut` uses internal iteration (`for_each`). A mutable query can't be a lending `Iterator` without `unsafe`.
- Single-threaded, and slower than an archetype ECS: storage is `BTreeMap`s, chosen for simplicity and deterministic order.

Run the example with `cargo run --example school`.
