//! C5: a chest can't be put inside itself.
use game::*;
use weft::{R1, World};

fn main() {
    let _world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let chest = t.spawn(Chest { name: "chest", rank: R1 }, (In, hall));
        #[cfg(not(twin))]
        t.relate(chest, In, chest); // ~ERROR E0277 "the relation is ranked, and ranks must rise"
        #[cfg(twin)]
        t.relate(chest, In, hall);
        t.spawn(Settings, ())
    });
}
