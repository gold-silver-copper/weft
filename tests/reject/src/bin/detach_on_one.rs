//! C12: `detach` would leave a `one` edge's source with no edge at all.
#![feature(decl_macro)]
use weft::*;

#[kind(root)]
pub struct Settings;

#[kind]
pub struct Room;

#[kind(needs = [In])]
pub struct Item;

#[cfg_attr(not(twin), relation(Item -> Room, one, detach))] // ~ERROR "detach"
#[cfg_attr(twin, relation(Item -> Room, one, cascade))]
pub struct In;

module!(pub Map { kinds: [Room, Item], relations: [In] });
game!(pub Game: Settings [Map]);

fn main() {}
