//! Ranks, as types. A `ranked` relation only relates a lower rank to a strictly higher one,
//! so it can't loop, and a relate that would break that is a trait error: rust-analyzer shows
//! it as you type, and `cargo check` rejects it.
//!
//! A rank is `Zero`, `Up<R>` (one above `R`), or `Top` (above every other rank, and what a
//! kind gets when `#[kind]` doesn't say `rank`). `R0` to `R15` name the first sixteen, as types
//! and as values.

use std::fmt;

use crate::kind::Kind;

/// A kind's rank, as a type: `Zero`, `Up<R>` or `Top`.
pub trait Rank: Copy + fmt::Debug + 'static {
    /// The rank as a number; `Top` is `u16::MAX`.
    const NUM: u16;
}

/// The lowest rank.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Zero;

/// The rank one above `R`.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Up<R>(pub R);

/// Above every other rank: the rank of kinds `#[kind]` gives no `rank`.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Top;

impl Rank for Zero {
    const NUM: u16 = 0;
}

impl<R: Rank> Rank for Up<R> {
    const NUM: u16 = R::NUM.saturating_add(1);
}

impl Rank for Top {
    const NUM: u16 = u16::MAX;
}

impl fmt::Debug for Zero {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("0")
    }
}

impl<R: Rank> fmt::Debug for Up<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", Self::NUM)
    }
}

impl fmt::Debug for Top {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("top")
    }
}

macro_rules! ranks {
    ($first:ident $(, $name:ident = $below:ident)*) => {
        #[doc = "Rank 0."]
        pub type $first = Zero;
        #[doc = "Rank 0."]
        pub const $first: $first = Zero;
        $(
            #[doc = concat!("The rank above `", stringify!($below), "`.")]
            pub type $name = Up<$below>;
            #[doc = concat!("The rank above `", stringify!($below), "`.")]
            pub const $name: $name = Up($below);
        )*
    };
}

ranks!(
    R0,
    R1 = R0,
    R2 = R1,
    R3 = R2,
    R4 = R3,
    R5 = R4,
    R6 = R5,
    R7 = R6,
    R8 = R7,
    R9 = R8,
    R10 = R9,
    R11 = R10,
    R12 = R11,
    R13 = R12,
    R14 = R13,
    R15 = R14
);

/// `Self` is a strictly lower rank than `B`. `X` and `Y` are the kinds being related, there
/// to name them when it fails.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`{X}` doesn't fit in `{Y}`: the relation is ranked, and ranks must rise",
    label = "`{X}` doesn't rank below `{Y}`",
    note = "an edge of a `ranked` relation goes from a lower rank to a strictly higher one, so it can never loop"
)]
pub trait Below<B, X, Y> {}

impl<B: Rank, X, Y> Below<Up<B>, X, Y> for Zero {}

impl<A: Below<B, X, Y>, B: Rank, X, Y> Below<Up<B>, X, Y> for Up<A> {}

impl<X, Y> Below<Top, X, Y> for Zero {}

impl<R: Rank, X, Y> Below<Top, X, Y> for Up<R> {}

/// A `Self` fits in a `Y` through a ranked relation: `Self`'s rank is below `Y`'s.
#[diagnostic::on_unimplemented(message = "`{Self}` doesn't fit in `{Y}`: the relation is ranked, and ranks must rise")]
pub trait Fits<Y> {}

impl<X: Kind, Y: Kind> Fits<Y> for X where X::Rank: Below<Y::Rank, X, Y> {}
