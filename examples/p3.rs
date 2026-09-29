#![feature(decl_macro)]
use std::error::Error;
use weft::*;

#[kind(root, needs = [Start, Plays])]
pub struct Settings;

#[kind] // rank `Top` (the default): anything ranked lower fits in it
pub struct Room {
    pub name: &'static str,
}

#[kind(needs = [At])]
pub struct Player;

#[kind(needs = [In], rank = 0)] // holds nothing
pub struct Item {
    pub name: &'static str,
}

/// A container that can itself be put away. Its rank is in its type:
/// a `Chest<R2>` holds `Chest<R1>`s and `Item`s, never another `Chest<R2>`.
#[kind(needs = [In], rank = R)]
pub struct Chest<R> {
    pub name: &'static str,
    pub rank: R,
}

set!(pub Thing); // what can be put somewhere
set!(pub Holder); // what things can be put in
is!(Item: Thing);
is!(Chest<R>: Thing, Holder);
is!(Room, Player: Holder);

// `Chest` is both a `Thing` and a `Holder`, so `In` could loop. `ranked` makes every edge
// go from a lower rank to a higher one, so no loop can be written: `cargo check` refuses it.
// Without `ranked` (or `cyclic`), `game!` below doesn't compile.
#[relation(Thing -> Holder, one, ranked, cascade)]
pub struct In;

#[relation(Room <-> Room, many, cyclic, detach)]
pub struct Exit(pub &'static str);

#[relation(Player -> Room, one, refuse)]
pub struct At;

#[relation(Settings -> Room, one, refuse)]
pub struct Start;

#[relation(Settings -> Player, one, refuse)]
pub struct Plays;

module!(pub Keep {
    kinds: [Room, Player, Item, Chest<R1>, Chest<R2>],
    relations: [In, Exit, At, Start, Plays],
});
game!(pub Game: Settings [Keep]);

fn room<'t>(t: &Turn<'t, Game>, name: &str) -> Result<Id<'t, Room>, String> {
    t.all::<Room>().find(|&r| t[r].name == name).ok_or(format!("no room {name}"))
}

fn item<'t>(t: &Turn<'t, Game>, name: &str) -> Result<Id<'t, Item>, String> {
    t.all::<Item>().find(|&i| t[i].name == name).ok_or(format!("no item {name}"))
}

fn chest<'t, R: Rank>(t: &Turn<'t, Game>, name: &str) -> Result<Id<'t, Chest<R>>, String>
where
    Game: Has<Chest<R>>,
{
    t.all::<Chest<R>>().find(|&c| t[c].name == name).ok_or(format!("no chest {name}"))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let cellar = t.spawn(Room { name: "cellar" }, ());
        t.relate(hall, (Exit("down"), Exit("up")), cellar);
        let big = t.spawn(Chest { name: "big chest", rank: R2 }, (In, hall));
        let small = t.spawn(Chest { name: "small chest", rank: R1 }, (In, big));
        t.spawn(Item { name: "coin" }, (In, small));
        t.spawn(Item { name: "key" }, (In, small));
        let me = t.spawn(Player, (At, hall));
        t.spawn(Settings, ((Start, hall), (Plays, me)))
    });
    world.check()?;

    world.turn(|t| -> Result<(), Box<dyn Error>> {
        let (hall, cellar) = (room(t, "hall")?, room(t, "cellar")?);
        let (big, small) = (chest::<R2>(t, "big chest")?, chest::<R1>(t, "small chest")?);
        let key = item(t, "key")?;

        // 1. Move the outer chest. One edge changes; everything inside comes along.
        t.relate(big, In, cellar)?;
        assert_eq!(t.target::<In>(big), cellar);
        assert_eq!(t.descendants::<In>(cellar).count(), 4);
        assert_eq!(t.ancestors::<In>(key).collect::<Vec<_>>(), [small.up::<Holder>(), big.up(), cellar.up()]);
        assert_eq!(t.sources::<In>(hall).count(), 0);

        // 2. Forbidden states don't compile, so there is nothing to refuse at runtime:
        // t.relate(big, In, big);          // E0277: `Chest<R2>` doesn't fit in `Chest<R2>`: `In` is ranked
        // t.relate(big, In, small);        // E0277: `Chest<R2>` doesn't fit in `Chest<R1>`: `In` is ranked
        // t.relate(cellar, In, big);       // E0277: `Room` is not a `Thing`
        // t.unrelate::<In>(big, cellar);   // E0277: `In` is `one`; move it, don't drop it

        // 3. The door is one fact, readable from both sides with its own label.
        let from_hall: Vec<_> = t.targets::<Exit>(hall).map(|(r, e)| (r, e.0)).collect();
        let from_cellar: Vec<_> = t.targets::<Exit>(cellar).map(|(r, e)| (r, e.0)).collect();
        assert_eq!(from_hall, [(cellar, "down")]);
        assert_eq!(from_cellar, [(hall, "up")]);

        // 4. Collapse the cellar. It's doomed now and goes at the end of this turn:
        //    `In` cascades (both chests and their contents), `Exit` detaches.
        t.despawn(cellar)?;
        assert!(t.is_doomed(key));
        t.relate(key, In, hall)?; // grab the key out of the rubble first
        assert!(!t.is_doomed(key));
        // The hall is the start room and the player stands in it: `Start`/`At` refuse.
        assert_eq!(t.despawn(hall), Err(Refused::Held));
        Ok(())
    })?;
    world.check()?;

    world.turn(|t| -> Result<(), Box<dyn Error>> {
        let hall = room(t, "hall")?;
        assert!(room(t, "cellar").is_err());
        assert_eq!(t.all::<Chest<R2>>().count() + t.all::<Chest<R1>>().count(), 0);
        let left: Vec<_> = t.all::<Item>().map(|i| t[i].name).collect();
        assert_eq!(left, ["key"]);
        assert_eq!(t.target::<In>(t.all::<Item>().next().ok_or("no key")?), hall);
        assert_eq!(t.targets::<Exit>(hall).count(), 0); // no exit leads into the rubble
        Ok(())
    })?;
    world.check()?;

    print!("{world}");
    Ok(())
}
