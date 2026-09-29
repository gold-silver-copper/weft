//! C12: `detach` would leave a `one` edge's source with no edge at all.
#![feature(decl_macro)]
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings);

#[derive(Debug)]
pub struct Room;
kind!(Room);

#[derive(Debug)]
pub struct Item;
kind!(Item needs In);

#[cfg(not(twin))]
relation!(pub struct In: Item -> Room, one, detach); // ~ERROR E0080 "detach"
#[cfg(twin)]
relation!(pub struct In: Item -> Room, one, cascade);

module!(pub Map { kinds: [Room, Item], relations: [In] });
game!(pub Game: Settings [Map]);

fn main() {}
