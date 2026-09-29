//! What `game!` asks about each pair of its kinds and relations, and answers under
//! `cargo check`.
//!
//! Every answer is an inherent associated const of `Probe` that only exists when its bounds
//! hold; when they don't, the path falls back to `Otherwise`'s `false`. The choice is made
//! per concrete type, so it sees every `is!` and `kind!` in the whole crate graph, which is
//! why the questions live in `game!` rather than in `relation!`. Option a's spike showed the
//! technique on stable Rust; the crates that expand `game!` need no feature for it.

use std::marker::PhantomData;

use crate::handle::Member;
use crate::kind::{Carries, Kind};
use crate::rank::Rank;
use crate::relation::{Cascade, LoopRule, NoLoops, One, Relation, Spill};

/// A question about the kind `K` and the relation `R`, or with `B`, about an edge of `R`
/// from a `K` to a `B`.
#[doc(hidden)]
pub struct Probe<K, R, B = ()>(PhantomData<Of<K, R, B>>);

/// What a `Probe` asks about, without owning any of it.
type Of<K, R, B> = fn() -> (K, R, B);

/// The answer when a question's bounds don't hold.
#[doc(hidden)]
pub trait Otherwise {
    const ONE_SOURCE: bool = false;
    const NEEDED: bool = false;
    const LOOPLESS_OVERLAP: bool = false;
    const TAKES: bool = false;
    const OWNS: bool = false;
}

impl<K, R, B> Otherwise for Probe<K, R, B> {}

/// `K` can be the source of the `one` relation `R`.
impl<K: Kind, R: Relation<Arity = One>> Probe<K, R>
where
    K: Member<R::Source>,
{
    #[allow(dead_code)]
    pub const ONE_SOURCE: bool = true;
}

/// `kind!(K needs R)`.
impl<K: Carries<R>, R: Relation> Probe<K, R> {
    #[allow(dead_code)]
    pub const NEEDED: bool = true;
}

/// `R` has no loop rule, and `K` can be both its source and its target.
impl<K: Kind, R: Relation<Loops = NoLoops>> Probe<K, R>
where
    K: Member<R::Source> + Member<R::Target>,
{
    #[allow(dead_code)]
    pub const LOOPLESS_OVERLAP: bool = true;
}

/// `K` can be the source of `R`, and despawning `R`'s target despawns or moves `K`.
impl<K: Kind, R: Relation> Probe<K, R>
where
    K: Member<R::Source>,
    R::Policy: Takes,
{
    #[allow(dead_code)]
    pub const TAKES: bool = true;
}

/// A `K` can carry a `one` edge of `R` to a `B`: the `K` owns the `B` for as long as the
/// edge, or any past target of it, is remembered.
impl<K: Carries<R>, R: Relation<Arity = One>, B: Kind> Probe<K, R, B>
where
    B: Member<R::Target>,
{
    #[allow(dead_code)]
    pub const OWNS: bool = !<R::Loops as LoopRule>::RANKED || <K::Rank as Rank>::NUM < <B::Rank as Rank>::NUM;
}

/// `cascade` and `spill`: the policies that despawn or move the source.
#[doc(hidden)]
pub trait Takes {}

impl Takes for Cascade {}

impl Takes for Spill {}

/// `edges[i][j]`: kind `i` can own kind `j` through a `one` edge. `Some(i)` if kind `i` can
/// own itself, directly or through other kinds. Games have at most 128 kinds.
#[doc(hidden)]
pub const fn owns_itself(edges: &[&[bool]]) -> Option<usize> {
    if edges.len() > 128 {
        return Some(0);
    }
    // One bit per kind still in the graph. Kahn's algorithm: peel off kinds that nothing
    // left can own; whatever never peels off is on a loop.
    let mut live = 0_u128;
    let mut rows = edges;
    while let [_, rest @ ..] = rows {
        live = live.wrapping_shl(1) | 1;
        rows = rest;
    }
    loop {
        let mut peeled = false;
        let (mut j, mut bit) = (0_usize, 1_u128);
        while j < edges.len() {
            if live & bit != 0 && !owned(edges, live, j) {
                live &= !bit;
                peeled = true;
            }
            j = j.saturating_add(1);
            bit = bit.wrapping_shl(1);
        }
        if live == 0 {
            return None;
        }
        if !peeled {
            let (mut k, mut bit) = (0_usize, 1_u128);
            while live & bit == 0 {
                k = k.saturating_add(1);
                bit = bit.wrapping_shl(1);
            }
            return Some(k);
        }
    }
}

/// Some kind still in `live` can own kind `j`.
const fn owned(edges: &[&[bool]], live: u128, j: usize) -> bool {
    let (mut rows, mut bit) = (edges, 1_u128);
    while let [row, rest @ ..] = rows {
        if live & bit != 0 && cell(row, j) {
            return true;
        }
        rows = rest;
        bit = bit.wrapping_shl(1);
    }
    false
}

const fn cell(row: &[bool], j: usize) -> bool {
    let (mut cells, mut k) = (row, 0_usize);
    while let [value, rest @ ..] = cells {
        if k == j {
            return *value;
        }
        cells = rest;
        k = k.saturating_add(1);
    }
    false
}

/// The `i`th message, for a `game!` check that found kind `i` on a loop.
#[doc(hidden)]
pub const fn nth(messages: &[&'static str], i: usize) -> &'static str {
    let (mut rest, mut k) = (messages, 0_usize);
    while let [message, tail @ ..] = rest {
        if k == i {
            return message;
        }
        rest = tail;
        k = k.saturating_add(1);
    }
    "a game can have at most 128 kinds"
}
