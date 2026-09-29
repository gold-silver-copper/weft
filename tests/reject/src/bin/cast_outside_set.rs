//! C9: `cast` only narrows to a kind the handle's set contains.
use game::*;

fn main() {
    let mut world = small::<Game>();
    world.turn(|t| {
        if let Some(hall) = t.all::<Room>().next() {
            for (thing, _) in t.sources::<In>(hall) {
                #[cfg(not(twin))]
                let _ = t.cast::<Room>(thing); // ~ERROR E0277
                #[cfg(twin)]
                let _ = t.cast::<Item>(thing);
            }
        }
    });
}
