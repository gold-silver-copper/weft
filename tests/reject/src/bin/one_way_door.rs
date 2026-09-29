//! C4: a two-way relation takes a label for each direction.
use game::*;
use weft::World;

fn main() {
    let _world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let cellar = t.spawn(Room { name: "cellar" }, ());
        #[cfg(not(twin))]
        t.relate(hall, Exit("down"), cellar); // ~ERROR E0277
        #[cfg(twin)]
        t.relate(hall, (Exit("down"), Exit("up")), cellar);
        t.spawn(Settings, ())
    });
}
