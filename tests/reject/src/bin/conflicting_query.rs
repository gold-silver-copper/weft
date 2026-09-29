//! C11: changing the graph while iterating it doesn't compile.
use game::*;

fn main() {
    let mut world = small::<Game>();
    world.turn(|t| {
        let hall = t.all::<Room>().next();
        let me = t.all::<Player>().next();
        if let (Some(hall), Some(me)) = (hall, me) {
            #[cfg(not(twin))]
            for (thing, _) in t.sources::<In>(hall) {
                if let Some(item) = t.cast::<Item>(thing) {
                    let _ = t.relate(item, In, me); // ~ERROR E0502
                }
            }
            #[cfg(twin)]
            for item in t.sources::<In>(hall).filter_map(|(thing, _)| t.cast::<Item>(thing)).collect::<Vec<_>>() {
                let _ = t.relate(item, In, me);
            }
        }
    });
}
