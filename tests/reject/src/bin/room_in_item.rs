//! C5: a room can't be put inside an item.
use game::*;
use weft::World;

fn main() {
    let _world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let lamp = t.spawn(Item { name: "lamp" }, (In, hall));
        #[cfg(not(twin))]
        t.relate(hall, In, lamp); // ~ERROR E0277
        #[cfg(twin)]
        t.relate(lamp, In, hall);
        t.spawn(Settings, ())
    });
}
