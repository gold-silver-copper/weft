//! C12: the root is never despawned, so no relation from it may `cascade`.
#![feature(decl_macro)]
use weft::*;

#[kind(root, needs = [Start])]
pub struct Settings;

#[kind]
pub struct Room;

#[cfg_attr(not(twin), relation(Settings -> Room, one, cascade))]
#[cfg_attr(twin, relation(Settings -> Room, one, refuse))]
pub struct Start;

module!(pub Map { kinds: [Room], relations: [Start] });
game!(pub Game: Settings [Map]); // ~ERROR E0080 "root"

fn main() {}
