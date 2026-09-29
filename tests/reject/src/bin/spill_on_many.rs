//! C12: `spill` moves a source's `one` edge up to the next holder, so it needs a `one` relation.
#![feature(decl_macro)]
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings);

#[derive(Debug)]
pub struct Npc;
kind!(Npc);

#[cfg(not(twin))]
relation!(pub struct Likes(pub u8): Npc -> Npc, many, cyclic, spill); // ~ERROR E0080 "spill"
#[cfg(twin)]
relation!(pub struct Likes(pub u8): Npc -> Npc, many, cyclic, detach);

module!(pub Folk { kinds: [Npc], relations: [Likes] });
game!(pub Game: Settings [Folk]);

fn main() {}
