//! C5: a chest can't be put inside a chest it contains.
use game::*;
use weft::World;

fn main() {
    let _world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let big = t.spawn(Chest::<2> { name: "big" }, (In, hall));
        let small = t.spawn(Chest::<1> { name: "small" }, (In, big));
        #[cfg(not(twin))]
        t.relate(big, In, small); // ~ERROR E0277 "the relation is ranked, and ranks must rise"
        #[cfg(twin)]
        t.relate(small, In, hall);
        t.spawn(Settings, ())
    });
}
