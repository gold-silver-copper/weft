//! C1: a handle from one world's turn can't be used in another world's turn.
use game::*;

fn main() {
    let mut here = small::<Game>();
    let mut there = small::<Game>();
    here.turn(|t| {
        let lamp = t.all::<Item>().next();
        there.turn(|u| {
            if let Some(lamp) = lamp {
                #[cfg(not(twin))]
                let _ = u[lamp].name; // ~ERROR
                #[cfg(twin)]
                let _ = (t[lamp].name, u.all::<Item>().next().map(|other| u[other].name));
            }
        });
    });
}
