//! C6: no kind needs `Default`. None of these could have one: `NonZeroU32` has no zero.
#![feature(decl_macro)]
use std::num::NonZeroU32;
use weft::*;

#[derive(Debug)]
pub struct Settings {
    pub seed: NonZeroU32,
}
kind!(root Settings needs Start);

#[derive(Debug)]
pub struct Room {
    pub number: NonZeroU32,
}
kind!(Room);

#[derive(Debug)]
pub struct Key {
    pub teeth: NonZeroU32,
}
kind!(Key needs Opens);

relation!(pub struct Start: Settings -> Room, one, refuse);
relation!(pub struct Opens: Key -> Room, one, cascade);

module!(pub Keys { kinds: [Room, Key], relations: [Start, Opens] });
game!(pub Game: Settings [Keys]);

fn main() {
    let [one, two, three] = [1, 2, 3].map(NonZeroU32::new).map(|n| n.unwrap_or(NonZeroU32::MIN));
    let mut world = World::<Game>::new(|t| {
        let vault = t.spawn(Room { number: two }, ());
        t.spawn(Key { teeth: three }, (Opens, vault));
        t.spawn(Settings { seed: one }, (Start, vault))
    });
    let read = world.turn(|t| {
        let key = t.all::<Key>().next()?;
        let room = t.target::<Opens>(key);
        let start = t.target::<Start>(t.root());
        assert!(room == start);
        Some((t[key].teeth.get(), t[room].number.get(), t.settings().seed.get()))
    });
    assert_eq!(read, Some((3, 2, 1)));
    assert_eq!(world.check(), Ok(()));
}
