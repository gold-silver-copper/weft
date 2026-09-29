//! C12: `spill` moves a source's `one` edge up to the next holder, so it needs a `one` relation.
#![feature(decl_macro)]
use weft::*;

#[kind(root)]
pub struct Settings;

#[kind]
pub struct Npc;

#[cfg_attr(not(twin), relation(Npc -> Npc, many, cyclic, spill))] // ~ERROR "spill"
#[cfg_attr(twin, relation(Npc -> Npc, many, cyclic, detach))]
pub struct Likes(pub u8);

module!(pub Folk { kinds: [Npc], relations: [Likes] });
game!(pub Game: Settings [Folk]);

fn main() {}
