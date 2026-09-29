//! C12: the root is never despawned, so no relation from it may `cascade`.
#![feature(decl_macro)]
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings needs Start);

#[derive(Debug)]
pub struct Room;
kind!(Room);

#[cfg(not(twin))]
relation!(pub struct Start: Settings -> Room, one, cascade);
#[cfg(twin)]
relation!(pub struct Start: Settings -> Room, one, refuse);

module!(pub Map { kinds: [Room], relations: [Start] });
game!(pub Game: Settings [Map]); // ~ERROR E0080 "root"

fn main() {}
