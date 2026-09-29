//! C2: an `Item` can't be spawned without its container.
use game::*;
use weft::World;

fn main() {
    let _world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        #[cfg(not(twin))]
        t.spawn(Item { name: "lamp" }, ()); // ~ERROR E0271
        #[cfg(twin)]
        t.spawn(Item { name: "lamp" }, (In, hall));
        let _ = hall;
        t.spawn(Settings, ())
    });
}
