//! `ranked`: an edge only goes from a lower rank to a higher one, so it can't loop.
//!
//! The comparison is a `generic_const_exprs` expression. A bare
//! `Assert<{ X::RANK < Y::RANK }>: IsTrue` fails as E0308 ("expected `false`,
//! found `true`"), so the result is turned into a type first and the failure
//! lands on a trait with a message instead.

use crate::kind::Kind;

#[doc(hidden)]
pub struct Assert<const B: bool>;

#[doc(hidden)]
pub trait Truth {
    type Out;
}
impl Truth for Assert<true> {
    type Out = Rise;
}
impl Truth for Assert<false> {
    type Out = Fall;
}

#[doc(hidden)]
pub enum Rise {}
#[doc(hidden)]
pub enum Fall {}

#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`{X}` doesn't fit in `{Y}`: the relation is ranked, and ranks must rise",
    label = "`{X}` doesn't rank below `{Y}`"
)]
pub trait Rises<X, Y> {}
impl<X, Y> Rises<X, Y> for Rise {}

#[doc(hidden)]
pub trait Compare<Y> {
    type Lt;
}
impl<X: Kind, Y: Kind> Compare<Y> for X
where
    Assert<{ X::RANK < Y::RANK }>: Truth,
{
    type Lt = <Assert<{ X::RANK < Y::RANK }> as Truth>::Out;
}

/// `Self::RANK < Y::RANK`.
#[diagnostic::on_unimplemented(message = "`{Self}` doesn't fit in `{Y}`: the relation is ranked, and ranks must rise")]
pub trait Fits<Y> {}
impl<X: Compare<Y>, Y> Fits<Y> for X where <X as Compare<Y>>::Lt: Rises<X, Y> {}
