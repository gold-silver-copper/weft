//! C6: a set handle has no data to read, so there is nothing to fall back to.
use game::*;

fn main() {
    let mut world = small::<Game>();
    world.turn(|t| {
        if let Some(hall) = t.all::<Room>().next() {
            for (thing, _) in t.sources::<In>(hall) {
                #[cfg(not(twin))]
                let _ = t[thing].name; // ~ERROR E0277
                #[cfg(twin)]
                let _ = t.cast::<Item>(thing).map(|item| t[item].name);
            }
        }
    });
}
