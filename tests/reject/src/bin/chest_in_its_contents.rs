//! C5: a chest can't be put inside a chest it contains.
use game::*;
use weft::{R1, R2, World};

fn main() {
    let _world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let big = t.spawn(Chest { name: "big", rank: R2 }, (In, hall));
        let small = t.spawn(Chest { name: "small", rank: R1 }, (In, big));
        #[cfg(not(twin))]
        t.relate(big, In, small); // ~ERROR E0277 "the relation is ranked, and ranks must rise"
        #[cfg(twin)]
        t.relate(small, In, hall);
        t.spawn(Settings, ())
    });
}
