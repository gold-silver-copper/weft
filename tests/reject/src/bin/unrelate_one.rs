//! C2: a `one` edge can't be dropped, only moved, so an item never ends up in no container.
use game::*;
use weft::World;

fn main() {
    let _world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let me = t.spawn(Player, (At, hall));
        let lamp = t.spawn(Item { name: "lamp" }, (In, hall));
        #[cfg(not(twin))]
        t.unrelate::<In>(lamp, hall); // ~ERROR E0277
        #[cfg(twin)]
        t.relate(lamp, In, me);
        let _ = me;
        t.spawn(Settings, ())
    });
}
