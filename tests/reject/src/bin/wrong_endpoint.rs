//! C10: relation endpoints are typed: `At` only targets a `Room`.
use game::*;
use weft::World;

fn main() {
    let _world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let lamp = t.spawn(Item { name: "lamp" }, (In, hall));
        #[cfg(not(twin))]
        t.spawn(Player, (At, lamp)); // ~ERROR E0277
        #[cfg(twin)]
        t.spawn(Player, (At, hall));
        let _ = lamp;
        t.spawn(Settings, ())
    });
}
