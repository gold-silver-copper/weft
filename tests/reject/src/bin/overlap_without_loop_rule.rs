//! C5: a relation whose endpoints overlap must say how it loops, or `game!` doesn't compile.
#![feature(decl_macro)]
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings);

#[derive(Debug)]
pub struct Shelf;
kind!(Shelf);

#[derive(Debug)]
pub struct Crate;
kind!(Crate rank 1 needs Inside);

set!(pub Rack);
is!(Shelf, Crate: Rack);

// A crate is both a source (it goes inside) and a target (it's a `Rack`).
#[cfg(not(twin))]
relation!(pub struct Inside: Crate -> Rack, one, cascade);
#[cfg(twin)]
relation!(pub struct Inside: Crate -> Rack, one, ranked, cascade);

module!(pub Store { kinds: [Shelf, Crate], relations: [Inside] });
game!(pub Warehouse: Settings [Store]); // ~ERROR E0080 "needs a loop rule"

fn main() {}
