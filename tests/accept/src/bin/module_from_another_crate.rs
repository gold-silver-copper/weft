//! C13: a module from another crate drops into this crate's `game!`, with set membership
//! declared upstream (`Bag`) and downstream (`Room`).
#![feature(decl_macro)]
use accept::{Bag, Holder, In, Item, Weight};
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings);

#[derive(Debug)]
pub struct Room {
    pub name: &'static str,
}
kind!(Room);

is!(Room: Holder); // set membership declared downstream

module!(pub Town { kinds: [Room], relations: [] });
game!(pub Game: Settings [Town, accept::Containers]);

fn main() {
    let mut world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let bag = t.spawn(Bag { name: "bag" }, ());
        let lamp = t.spawn(Item { name: "lamp" }, (In, hall));
        let coin = t.spawn(Item { name: "coin" }, (In, bag));
        t.insert(lamp, Weight(2));
        t.insert(coin, Weight(1));
        t.spawn(Settings, ())
    });
    let moved = world.turn(|t| -> Result<u32, Refused> {
        let (Some(hall), Some(bag)) = (t.all::<Room>().next(), t.all::<Bag>().next()) else { return Ok(0) };
        let Some(lamp) = t.all::<Item>().find(|&i| t[i].name == "lamp") else { return Ok(0) };
        assert_eq!(accept::load(t, hall.up()), 2);
        t.relate(lamp, In, bag)?;
        assert_eq!(accept::load(t, hall.up()), 0);
        Ok(accept::load(t, bag.up()))
    });
    assert_eq!(moved, Ok(3));
    assert_eq!(world.check(), Ok(()));
    println!("{world}");
}
