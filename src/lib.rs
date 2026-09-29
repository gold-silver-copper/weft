//! A small entity component system where the compiler does the checking.
//!
//! - A [`schema!`] lists every component, relation and resource a [`World`]
//!   can hold. Using anything else doesn't compile, resources always exist,
//!   and only components marked `mut` can be changed in place.
//! - Access follows Rust's borrowing rules, enforced at compile time.
//!   [`World::query`] takes `&World`; [`World::query_mut`] takes
//!   `&mut World` and lends columns out of a type-level list. Borrowing a
//!   column mutably twice, or mutably and shared, is a type error.
//! - [`Relation`]s are typed, valued edges. [`Many`] or [`One`] target per
//!   source is part of the type, so asking for *the* target of a
//!   many-to-many relation doesn't compile.
//! - Nothing in the crate can panic. Fallible operations return `Result` or
//!   `Option`, and infallible ones say so in their types: spawning a bundle
//!   of components returns an `Entity`, spawning one with a relation returns
//!   `Result<Entity, NoSuchEntity>`. This is enforced with `forbid`-level
//!   lints against every panicking construct.
//!
//! ```
//! use weft::*;
//!
//! struct Pos(i32);
//! impl<S: Schema> Component<S> for Pos {}
//!
//! struct Vel(i32);
//! impl<S: Schema> Component<S> for Vel {}
//!
//! struct Likes(u32);
//! impl<S: Schema> Relation<S> for Likes {
//!     type Arity = Many;
//! }
//!
//! schema! {
//!     struct Game {
//!         components: [mut Pos, Vel],
//!         relations: [Likes, ChildOf],
//!         resources: [],
//!     }
//! }
//!
//! let mut world = World::<Game>::new();
//! let alice = world.spawn((Pos(0), Vel(1)));
//! let bob = world.spawn((Pos(10), Vel(-1), To(Likes(3), alice)))?;
//! world.relate(alice, Likes(5), bob)?;
//!
//! world.query_mut::<(&mut Pos, &Vel)>().for_each(|(mut pos, vel)| pos.0 += vel.0);
//!
//! assert_eq!(world.get::<Pos>(alice).map(|p| p.0), Some(1));
//! assert_eq!(world.targets::<Likes>(bob), [alice]);
//! assert_eq!(world.relation::<Likes>(alice, bob).map(|l| l.0), Some(5));
//! # Ok::<(), NoSuchEntity>(())
//! ```

#![no_std]
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

extern crate alloc;

mod bundle;
mod component;
mod entity;
mod event;
mod query;
mod relation;
mod schedule;
mod schema;
mod world;

pub use bundle::{Bundle, Outcome};
pub use component::{Component, Mut, Requires};
pub use entity::{Entity, NoSuchEntity};
pub use event::{EntityEvent, Event, ObserverId, On};
pub use query::{
    Added, BorrowQuery, Borrows, Changed, Filter, HasSource, HasTarget, Not, Or, Query, QueryData, QueryIter, QueryMut, ReadQuery,
    Sources, Targets, View, With, Without,
};
pub use relation::{Arity, ChildOf, Many, One, Relation, To, Traverse};
pub use schedule::Schedule;
pub use schema::{Links, Owns, Reads, Schema, Writes};
pub use world::World;

/// Machinery that appears in trait signatures and macro output. Not meant to
/// be used directly.
#[doc(hidden)]
pub mod internal {
    pub use crate::bundle::{IsComponent, IsRelation, Lift};
    pub use crate::component::{Column, Tick};
    pub use crate::query::{Candidates, Context, Term};
    pub use crate::relation::Edges;
    pub use crate::schema::{At, Columns, Lend, LendMut, Loans, Peek, Relations, Shared, Slot, Succ, Taken, Zero};
}
