//! `game/src/lib.rs`: a small game crate, with its tests.
#![feature(decl_macro)]
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings);

#[derive(Debug)]
pub struct Room {
    pub name: &'static str,
}
kind!(Room);

#[derive(Debug)]
pub struct Player;
kind!(Player needs At);

#[derive(Debug)]
pub struct Item {
    pub name: &'static str,
}
kind!(Item rank 0 needs In);

#[derive(Debug)]
pub struct Chest<R> {
    pub name: &'static str,
    pub rank: R,
}
kind!(Chest<R> rank R needs In);

#[derive(Debug)]
pub struct Npc {
    pub name: &'static str,
}
kind!(Npc);

set!(pub Thing);
is!(Item: Thing);
set!(pub Holder);
is!(Room, Player: Holder);
is!(Chest<R>: Thing, Holder);

relation!(pub struct In: Thing -> Holder, one, ranked, cascade);
relation!(pub struct At: Player -> Room, one, refuse);
relation!(pub struct Exit(pub &'static str): Room <-> Room, many, cyclic, detach);
relation!(pub struct Likes(pub u8): Npc -> Npc, many, cyclic, detach);

module!(pub Map { kinds: [Room, Player, Item, Chest<R1>, Chest<R2>], relations: [In, At, Exit] });
module!(pub Folk { kinds: [Npc], relations: [Likes] });
game!(pub Game: Settings [Map, Folk]);

/// `take <noun>`: needs only `In`, `At` and the kinds they connect.
pub fn take<'t, G>(t: &mut Turn<'t, G>, me: Id<'t, Player>, noun: &str) -> Result<bool, Refused>
where
    G: Has<In> + Has<At> + Has<Item> + Has<Player> + Has<Room>,
{
    let here = t.target::<At>(me);
    let found = t
        .sources::<In>(here)
        .filter_map(|(thing, _)| t.cast::<Item>(thing))
        .find(|&item| t[item].name == noun);
    match found {
        Some(item) => t.relate(item, In, me).map(|()| true),
        None => Ok(false),
    }
}

/// A room, a player and a lamp, in any game that has a `Map`.
pub fn small<G: Map + Schema<Root = Settings>>() -> World<G> {
    World::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        t.spawn(Player, (At, hall));
        t.spawn(Item { name: "lamp" }, (In, hall));
        t.spawn(Settings, ())
    })
}

/// Invariants that must not compile. Each block is a `compile_fail` doctest, and the
/// pinned nightly makes rustdoc check the stated error code.
///
/// An item in no container:
/// ```compile_fail,E0271
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     t.spawn(Item { name: "lamp" }, ()); // `Item needs In`
///     t.spawn(Settings, ())
/// });
/// ```
/// An item in two containers:
/// ```compile_fail,E0271
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     let hall = t.spawn(Room { name: "hall" }, ());
///     let me = t.spawn(Player, (At, hall));
///     t.spawn(Item { name: "lamp" }, ((In, hall), (In, me))); // `In` is `one`
///     t.spawn(Settings, ())
/// });
/// ```
/// Taking an item out of every container:
/// ```compile_fail,E0277
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     let hall = t.spawn(Room { name: "hall" }, ());
///     let lamp = t.spawn(Item { name: "lamp" }, (In, hall));
///     t.unrelate::<In>(lamp, hall); // `In` is `one`: there's no unrelate
///     t.spawn(Settings, ())
/// });
/// ```
/// A handle outliving its turn (the stale-handle case):
/// ```compile_fail
/// use game::*;
/// let mut world = game::small::<Game>();
/// let lamp = world.turn(|t| t.all::<Item>().next()); // `Id<'t, _>` can't leave the turn
/// ```
/// A one-way door where the game declared two-way exits:
/// ```compile_fail,E0277
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     let hall = t.spawn(Room { name: "hall" }, ());
///     let cellar = t.spawn(Room { name: "cellar" }, ());
///     t.relate(hall, Exit("down"), cellar); // `Exit` is `<->`: give both labels
///     t.spawn(Settings, ())
/// });
/// ```
/// A room inside an item:
/// ```compile_fail,E0277
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     let hall = t.spawn(Room { name: "hall" }, ());
///     let chest = t.spawn(Chest { name: "chest", rank: weft::R1 }, (In, hall));
///     t.relate(hall, In, chest); // `Room` is not a `Thing`
///     t.spawn(Settings, ())
/// });
/// ```
/// A chest inside itself:
/// ```compile_fail,E0277
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     let hall = t.spawn(Room { name: "hall" }, ());
///     let chest = t.spawn(Chest { name: "chest", rank: weft::R1 }, (In, hall));
///     t.relate(chest, In, chest); // `In` is ranked: `Chest<R1>` only fits above rank 1
///     t.spawn(Settings, ())
/// });
/// ```
/// A chest inside its own contents:
/// ```compile_fail,E0277
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     let hall = t.spawn(Room { name: "hall" }, ());
///     let big = t.spawn(Chest { name: "big", rank: weft::R2 }, (In, hall));
///     let small = t.spawn(Chest { name: "small", rank: weft::R1 }, (In, big));
///     t.relate(big, In, small); // rank 2 doesn't fit in rank 1
///     t.spawn(Settings, ())
/// });
/// ```
/// A wrong endpoint:
/// ```compile_fail,E0277
/// use game::*;
/// weft::World::<Game>::new(|t| {
///     let hall = t.spawn(Room { name: "hall" }, ());
///     let lamp = t.spawn(Item { name: "lamp" }, (In, hall));
///     t.spawn(Player, (At, lamp)); // `At` only targets `Room`
///     t.spawn(Settings, ())
/// });
/// ```
/// Reading data through a set handle (what used to hit the `Default` fallback):
/// ```compile_fail,E0277
/// use game::*;
/// let mut world = game::small::<Game>();
/// world.turn(|t| {
///     if let Some(hall) = t.all::<Room>().next() {
///         for (thing, _) in t.sources::<In>(hall) {
///             let _ = t[thing].name; // `Thing` is a set: `cast` it to a kind first
///         }
///     }
/// });
/// ```
/// A conflicting query: changing the graph while iterating it:
/// ```compile_fail,E0502
/// use game::*;
/// let mut world = game::small::<Game>();
/// world.turn(|t| {
///     let hall = t.all::<Room>().next();
///     let me = t.all::<Player>().next();
///     if let (Some(hall), Some(me)) = (hall, me) {
///         for (thing, _) in t.sources::<In>(hall) {
///             if let Some(item) = t.cast::<Item>(thing) {
///                 let _ = t.relate(item, In, me);
///             }
///         }
///     }
/// });
/// ```
pub mod proofs {}

#[cfg(test)]
mod tests {
    use super::*;

    // Only what `take` touches: no `Npc`, no `Likes`.
    game!(Bare: Settings [Map]);

    #[test]
    fn take_moves_the_lamp() {
        let mut world = small::<Game>();
        world.turn(|t| {
            let me = t.all::<Player>().next().unwrap();
            let hall = t.target::<At>(me);
            assert_eq!(take(t, me, "lamp"), Ok(true));
            let lamp = t.all::<Item>().next().unwrap();
            assert_eq!(t.target::<In>(lamp), me);
            assert_eq!(t.sources::<In>(hall).count(), 0);
        });
        world.check().unwrap();
    }

    #[test]
    fn take_in_isolation() {
        let mut world = small::<Bare>();
        world.turn(|t| {
            let me = t.all::<Player>().next().unwrap();
            assert_eq!(take(t, me, "sword"), Ok(false));
            assert_eq!(take(t, me, "lamp"), Ok(true));
            assert_eq!(take(t, me, "lamp"), Ok(false)); // already carried
        });
        world.check().unwrap();
    }

    #[test]
    fn snapshot() {
        let mut world = small::<Bare>();
        world
            .turn(|t| {
                let me = t.all::<Player>().next().unwrap();
                take(t, me, "lamp")
            })
            .unwrap();
        let expected = "\
Settings#0 Settings
Room#0 Room { name: \"hall\" }
Player#0 Player
  At -> Room#0
Item#0 Item { name: \"lamp\" }
  In -> Player#0
";
        assert_eq!(world.to_string(), expected);
    }

    #[test]
    fn chests_nest_move_and_cascade() {
        let mut world = small::<Bare>();
        world.turn(|t| {
            let hall = t.all::<Room>().next().unwrap();
            let me = t.all::<Player>().next().unwrap();
            let big = t.spawn(Chest { name: "big", rank: R2 }, (In, hall));
            let small = t.spawn(Chest { name: "small", rank: R1 }, (In, big));
            let coin = t.spawn(Item { name: "coin" }, (In, small));
            t.relate(big, In, me).unwrap();
            assert_eq!(t.ancestors::<In>(coin).collect::<Vec<_>>(), [small.up::<Holder>(), big.up(), me.up()]);
            t.despawn(big).unwrap(); // `In` cascades: the small chest and the coin go too
            assert!(t.is_doomed(coin));
        });
        world.check().unwrap();
        world.turn(|t| {
            assert_eq!(t.all::<Chest<R1>>().count(), 0);
            assert_eq!(t.all::<Item>().map(|i| t[i].name).collect::<Vec<_>>(), ["lamp"]);
        });
        world.check().unwrap();
    }
}
