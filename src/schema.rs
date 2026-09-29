//! Games: `game!` builds a schema, the list of everything a world stores.

use crate::graph::{Comps, Graph};
use crate::kind::{Kind, KindList, RootRole};

/// Where a part's storage lives.
#[doc(hidden)]
pub trait Side {
    type Of<G: Schema>;
}

/// Graph storage: kinds and relations. A turn borrows it shared, and it grows through `&`.
#[doc(hidden)]
pub enum Shared {}

/// Component storage: a turn borrows it mutably.
#[doc(hidden)]
pub enum Owned {}

impl Side for Shared {
    type Of<G: Schema> = G::Shared;
}

impl Side for Owned {
    type Of<G: Schema> = G::Comps;
}

/// A kind, relation or component, with the storage `game!` gives it.
#[doc(hidden)]
pub trait Part: 'static {
    type Side: Side;
    type Store: Default;
}

/// A game: its root kind and the storage `game!` generated for it.
pub trait Schema: Sized + 'static {
    /// The root kind, whose data is the game's settings.
    type Root: Kind<Role = RootRole>;
    #[doc(hidden)]
    type Shared: Default + Graph;
    #[doc(hidden)]
    type Comps: Default + Comps;
    #[doc(hidden)]
    fn root_list(shared: &Self::Shared) -> &KindList<Self::Root>;
}

/// The game stores `X`: a kind, a relation or a component listed by one of its modules.
///
/// Generic code states what it touches as `Has<..>` bounds, or as a module's trait, which
/// is shorthand for all of that module's `Has` bounds.
#[diagnostic::on_unimplemented(
    message = "`{Self}` doesn't store `{X}`",
    label = "`{X}` isn't in this game",
    note = "list `{X}` in a `module!` that the game's `game!` includes, or, in generic code, add `Has<{X}>` or a module's trait to the bounds"
)]
pub trait Has<X: Part>: Schema {
    #[doc(hidden)]
    fn part(s: &<X::Side as Side>::Of<Self>) -> &X::Store;
    #[doc(hidden)]
    fn part_mut(s: &mut <X::Side as Side>::Of<Self>) -> &mut X::Store;
}
