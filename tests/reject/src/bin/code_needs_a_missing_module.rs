//! P6's `play(&mut quiet, &[], haunt::<Quiet>)`: code bounded by a module doesn't compile for
//! a game that leaves that module out.
use game::*;
use weft::{Turn, World, game};

game!(pub Bare: Settings [Map]); // `Map` without `Folk`: no `Npc`, no `Likes`

/// NPC gossip: needs `Folk`.
fn gossip<'t, G: Folk>(t: &Turn<'t, G>) -> usize {
    t.all::<Npc>().count()
}

fn main() {
    let mut bare: World<Bare> = small::<Bare>();
    let mut full: World<Game> = small::<Game>();
    #[cfg(not(twin))]
    let _ = bare.turn(|t| gossip(t)); // ~ERROR E0277
    #[cfg(twin)]
    let _ = full.turn(|t| gossip(t));
    let _ = (bare.turn(|t| t.all::<Room>().count()), full.turn(|t| t.all::<Npc>().count()));
}
