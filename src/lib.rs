//! A typed entity-relation graph for text games, where invalid states don't compile.
//!
//! See `docs/design/0002-improve-ecs.md`.

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
