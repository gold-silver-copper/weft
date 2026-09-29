//! C12: a kind that can be the source of a `one` relation must `need` it.
#![feature(decl_macro)]
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings);

#[derive(Debug)]
pub struct Room;
kind!(Room);

#[derive(Debug)]
pub struct Npc;
#[cfg(not(twin))]
kind!(Npc);
#[cfg(twin)]
kind!(Npc needs At);

relation!(pub struct At: Npc -> Room, one, refuse);

module!(pub Town { kinds: [Room, Npc], relations: [At] });
game!(pub Game: Settings [Town]); // ~ERROR E0080 "must `need` it"

fn main() {}
