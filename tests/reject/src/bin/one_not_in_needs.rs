//! C12: a kind that can be the source of a `one` relation must need it.
#![feature(decl_macro)]
use weft::*;

#[kind(root)]
pub struct Settings;

#[kind]
pub struct Room;

#[cfg_attr(not(twin), kind)]
#[cfg_attr(twin, kind(needs = [At]))]
pub struct Npc;

#[relation(Npc -> Room, one, refuse)]
pub struct At;

module!(pub Town { kinds: [Room, Npc], relations: [At] });
game!(pub Game: Settings [Town]); // ~ERROR E0080 "must need it"

fn main() {}
