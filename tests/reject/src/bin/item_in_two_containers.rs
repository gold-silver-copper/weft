//! C2: an `Item` can't be spawned into two containers.
use game::*;
use weft::World;

fn main() {
    let _world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let me = t.spawn(Player, (At, hall));
        #[cfg(not(twin))]
        t.spawn(Item { name: "lamp" }, ((In, hall), (In, me))); // ~ERROR E0271
        #[cfg(twin)]
        t.spawn(Item { name: "lamp" }, (In, me));
        t.spawn(Settings, ())
    });
}
