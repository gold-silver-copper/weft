//! C5: a relation whose endpoints overlap must say how it loops, or `game!` doesn't compile.
#![feature(decl_macro)]
use weft::*;

#[kind(root)]
pub struct Settings;

#[kind]
pub struct Shelf;

#[kind(needs = [Inside], rank = 1)]
pub struct Crate;

set!(pub Rack);
is!(Shelf, Crate: Rack);

// A crate is both a source (it goes inside) and a target (it's a `Rack`).
#[cfg_attr(not(twin), relation(Crate -> Rack, one, cascade))]
#[cfg_attr(twin, relation(Crate -> Rack, one, ranked, cascade))]
pub struct Inside;

module!(pub Store { kinds: [Shelf, Crate], relations: [Inside] });
game!(pub Warehouse: Settings [Store]); // ~ERROR E0080 "needs a loop rule"

fn main() {}
