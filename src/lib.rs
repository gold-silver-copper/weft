//! A typed entity-relation graph for text games, where invalid states don't compile.
//!
//! A game is kinds (`kind!`), open sets of kinds (`set!`, `is!`), relations between them
//! (`relation!`) and components (`component!`), grouped into modules (`module!`) and put
//! together by `game!`. A `World` of that game changes only in turns, and every rule it keeps
//! is a type where Rust allows one:
//!
//! - **Handles can't go stale.** `Id<'t, K>` is branded to its turn: it can't leave it or
//!   enter another world's, so if a handle exists, its entity exists.
//! - **Reads are total.** `t[id]` and `target` on a `one` relation read fields; there is no
//!   `Option`, no fallback value, and kinds need no `Default`.
//! - **Relations keep their shape.** Endpoints are typed; a `one` edge is given at spawn and
//!   can only be moved; a two-way relation is one fact with a value for each end;
//!   `ranked` relations can't loop, because every edge must go up in rank.
//! - **Hooks declare their reach.** `on_relate = f: Bound` runs `f` on every new edge, and
//!   every caller that relates the relation must meet `Bound`. Chains that never settle are
//!   refused.
//! - **Despawns are planned.** A despawn dooms its entity; `cascade`, `detach`, `spill` and
//!   `refuse` decide what happens to the edges into it when the turn ends, and a despawn a
//!   `refuse` edge would break is refused.
//! - **`game!` checks the declarations** under `cargo check`: a `one` relation its source
//!   kind doesn't `need`, overlapping endpoints without a loop rule, `cascade` or `spill` from
//!   the root, and `one` edges that could let an entity own itself.
//!
//! Safe Rust only, no dependencies, and no panics: runtime refusals are `Refused`, and `Sure`
//! operations return their value bare. Nightly, for `decl_macro` only: crates that call
//! `module!` enable `#![feature(decl_macro)]`.
//!
//! ```
//! #![feature(decl_macro)]
//! use weft::*;
//!
//! #[derive(Debug)]
//! pub struct Settings;
//! kind!(root Settings);
//!
//! #[derive(Debug)]
//! pub struct Room {
//!     pub name: &'static str,
//! }
//! kind!(Room);
//!
//! #[derive(Debug)]
//! pub struct Item {
//!     pub name: &'static str,
//! }
//! kind!(Item needs In); // an item is always somewhere
//!
//! relation!(pub struct In: Item -> Room, one, cascade);
//! relation!(pub struct Exit(pub &'static str): Room <-> Room, many, cyclic, detach);
//!
//! module!(pub Map { kinds: [Room, Item], relations: [In, Exit] });
//! game!(pub Game: Settings [Map]);
//!
//! let mut world = World::<Game>::new(|t| {
//!     let hall = t.spawn(Room { name: "hall" }, ());
//!     let cellar = t.spawn(Room { name: "cellar" }, ());
//!     t.relate(hall, (Exit("down"), Exit("up")), cellar);
//!     t.spawn(Item { name: "lamp" }, (In, hall));
//!     t.spawn(Settings, ())
//! });
//!
//! let moved = world.turn(|t| {
//!     let lamp = t.all::<Item>().next()?;
//!     let here = t.target::<In>(lamp); // an `Id`, not an `Option`
//!     let (down, _) = t.targets::<Exit>(here).next()?;
//!     t.relate(lamp, In, down).ok()?; // `In` is `one`: relating again moves it
//!     Some(t[t.target::<In>(lamp)].name)
//! });
//! assert_eq!(moved, Some("cellar"));
//! assert_eq!(world.check(), Ok(()));
//! ```
//!
//! The design and its trade-offs: `DESIGN.md`, and the decision records in `docs/design/`.

#![feature(decl_macro)]
#![forbid(unsafe_code)]
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

mod chain;
mod component;
mod graph;
mod handle;
mod kind;
mod macros;
mod probe;
mod rank;
mod relation;
mod schema;
mod turn;
mod world;

pub use component::Component;
pub use graph::Broken;
pub use handle::{Handle, Id, Is, Member, Narrow, Set};
pub use kind::{Kind, Rooted};
pub use rank::{
    Fits, R0, R1, R2, R3, R4, R5, R6, R7, R8, R9, R10, R11, R12, R13, R14, R15, Rank, Top, Up, Zero,
};
pub use relation::{
    Allows, Arity, Cascade, Cyclic, Detach, Detachable, Fallibility, Holds, LoopRule, Many, NoLoops, OnRelate, One, Opt,
    PolicyKind, Ranked, Refuse, Refused, Relation, Single, Spill, Sure, TwoWay, Value,
};
pub use schema::{Has, Schema};
pub use turn::{Genesis, Live, Needs, Phase, Turn};
pub use world::World;

/// What the macros expand to. Not part of the API.
#[doc(hidden)]
pub mod __private {
    pub use std::boxed::Box;
    pub use std::rc::Rc;
    pub use std::vec::Vec;

    pub use crate::graph::Table;
    pub use crate::handle::{Dyn, Erased};
    pub use crate::kind::{Carries, KindList, Node, OneSlot, Plural, RootRole};
    pub use crate::probe::{Otherwise, Probe, nth, owns_itself};
    pub use crate::rank::Below;
    pub use crate::relation::{
        Arity, Link, RelLog, check_relation, __link_many as link_many, __link_one as link_one, __link_opt as link_opt,
        __one_way_sources as one_way_sources, __one_way_targets as one_way_targets, __read_one as read_one,
        __read_opt as read_opt, __two_way as two_way,
    };
    pub use crate::schema::{Owned, Part, Shared, Side};

    /// `(A, (B, (C, ())))` from `A, B, C,`.
    pub macro hlist {
        () => { () },
        ($a:ty, $($rest:ty,)*) => { ($a, $crate::__private::hlist!($($rest,)*)) },
    }
}
