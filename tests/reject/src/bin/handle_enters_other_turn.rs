//! C1: a handle from one world's turn can't be used in another world's turn.
use game::*;

fn main() {
    let mut here = small::<Game>();
    let mut there = small::<Game>();
    here.turn(|t| {
        let lamp = t.all::<Item>().next();
        there.turn(|u| {
            #[cfg(not(twin))]
            let name = lamp.map(|lamp| u[lamp].name); // ~ERROR E0521
            #[cfg(twin)]
            let name = lamp.map(|lamp| t[lamp].name);
            let _ = (name, u.all::<Room>().count());
        });
    });
}
