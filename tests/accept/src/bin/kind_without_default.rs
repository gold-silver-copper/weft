//! C6: no kind needs `Default`. None of these could have one: `NonZeroU32` has no zero.
#![feature(decl_macro)]
use std::num::NonZeroU32;
use weft::*;

#[kind(root, needs = [Start])]
pub struct Settings {
    pub seed: NonZeroU32,
}

#[kind]
pub struct Room {
    pub number: NonZeroU32,
}

#[kind(needs = [Opens])]
pub struct Key {
    pub teeth: NonZeroU32,
}

#[relation(Settings -> Room, one, refuse)]
pub struct Start;

#[relation(Key -> Room, one, cascade)]
pub struct Opens;

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
