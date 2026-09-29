//! C1: a handle can't leave the turn that handed it out.
use game::*;

fn main() {
    let mut world = small::<Game>();
    #[cfg(not(twin))]
    let lamp = world.turn(|t| t.all::<Item>().next()); // ~ERROR
    #[cfg(twin)]
    let lamp = world.turn(|t| t.all::<Item>().next().map(|lamp| t[lamp].name));
    let _ = lamp;
}
