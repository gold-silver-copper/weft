//! P7's `t.target::<At>(walkers[0])`: a `one` edge is read from the kind that carries it, so
//! a set handle is `cast` first.
use game::*;

fn main() {
    let mut world = small::<Game>();
    world.turn(|t| {
        if let Some(hall) = t.all::<Room>().next() {
            for (thing, _) in t.sources::<In>(hall) {
                #[cfg(not(twin))]
                let _ = t.target::<In>(thing); // ~ERROR E0277
                #[cfg(twin)]
                let _ = t.cast::<Item>(thing).map(|item| t.target::<In>(item));
            }
        }
    });
}
