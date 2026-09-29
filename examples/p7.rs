#![feature(decl_macro)]
use std::error::Error;
use weft::*;

#[kind(root, needs = [Start, Plays])]
pub struct Settings;

// No kind derives `Default`: nothing is ever read from a made-up row.
#[kind]
pub struct Room {
    pub name: &'static str,
}

#[kind(needs = [At])]
pub struct Player;

#[kind(needs = [At])]
pub struct Npc {
    pub name: &'static str,
}

#[kind(needs = [In], rank = 0)]
pub struct Item {
    pub name: &'static str,
}

#[kind(needs = [In], rank = R)]
pub struct Chest<R> {
    pub name: &'static str,
    pub rank: R,
}

set!(pub Walker);
is!(Player, Npc: Walker);
set!(pub Thing);
is!(Item: Thing);
set!(pub Holder);
is!(Room, Player: Holder);
is!(Chest<R>: Thing, Holder);

#[relation(Thing -> Holder, one, ranked, spill)]
pub struct In;
// The hook's bound is a whole module, not just `At`: it may move items through `In`.
// Every caller that relates `At` carries `Map` in its own bounds, so the effect is visible.
#[relation(Walker -> Room, one, refuse, on_relate = pick_up: Map)]
pub struct At;

#[relation(Settings -> Room, one, refuse)]
pub struct Start;

#[relation(Settings -> Player, one, refuse)]
pub struct Plays;

module!(pub Map {
    kinds: [Room, Player, Npc, Item, Chest<R1>],
    relations: [In, At, Start, Plays],
});
game!(pub Game: Settings [Map]);

/// When the player enters a room, they pick up every loose item in it.
fn pick_up<'t, G: Map>(t: &mut Turn<'t, G>, who: Id<'t, Walker>, room: Id<'t, Room>) -> Result<(), Refused> {
    let Some(me) = t.cast::<Player>(who) else { return Ok(()) };
    let loose: Vec<_> = t.sources::<In>(room).filter_map(|(thing, _)| t.cast::<Item>(thing)).collect();
    for item in loose {
        t.relate(item, In, me)?;
        println!("You pick up the {}.", t[item].name);
    }
    Ok(())
}

fn attic<'t>(t: &Turn<'t, Game>) -> Result<Id<'t, Room>, &'static str> {
    t.all::<Room>().find(|&r| t[r].name == "attic").ok_or("no attic")
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let attic = t.spawn(Room { name: "attic" }, ());
        let crate_ = t.spawn(Chest { name: "crate", rank: R1 }, (In, attic));
        t.spawn(Item { name: "lamp" }, (In, attic));
        t.spawn(Item { name: "coin" }, (In, crate_));
        t.spawn(Npc { name: "ghost" }, (At, attic));
        let me = t.spawn(Player, (At, hall)); // genesis runs no hooks
        t.spawn(Settings, ((Start, hall), (Plays, me)))
    });

    world.turn(|t| -> Result<(), Box<dyn Error>> {
        let me = t.target::<Plays>(t.root());
        let attic = attic(t)?;

        // A hook that touches another relation: entering runs `pick_up`, which relates `In`.
        t.relate(me, At, attic)?; // prints "You pick up the lamp."
        let carried: Vec<_> = t.sources::<In>(me).filter_map(|(x, _)| t.cast::<Item>(x)).map(|i| t[i].name).collect();
        assert_eq!(carried, ["lamp"]); // the crate stays: `pick_up` only takes `Item`s

        // A chest inside itself doesn't compile; a legal nesting does.
        let crate_ = t.all::<Chest<R1>>().next().ok_or("no crate")?;
        // t.relate(crate_, In, crate_);  // E0277: `Chest<R1>` doesn't fit in `Chest<R1>`: `In` is ranked

        // Reads are total. The `one` edge is stored with the crate, so `target` is an `Id`,
        // not an `Option`, and there is no fallback row behind `t[..]`.
        let holder = t.target::<In>(crate_);
        assert_eq!(holder, attic);
        // let _ = t[holder];             // E0277: `Holder` is a set; it has no data to read

        // Narrowing is identity: `cast` finds this very node among the game's `Npc`s.
        let walkers: Vec<_> = t.sources::<At>(attic).map(|(w, _)| w).collect();
        let npcs: Vec<_> = walkers.iter().filter_map(|&w| t.cast::<Npc>(w)).map(|n| t[n].name).collect();
        assert_eq!(npcs, ["ghost"]);
        assert_eq!(walkers.iter().filter(|&&w| t.cast::<Player>(w).is_some()).count(), 1);
        // let _ = t.cast::<Room>(walkers[0]);   // E0277: `Room` is not a `Walker`
        // let _ = t.target::<At>(walkers[0]);   // E0277: `target` needs a kind; `cast` the walker first

        t.despawn(crate_)?; // `In` spills: the coin moves to the attic at the end of the turn
        Ok(())
    })?;
    world.check()?;

    world.turn(|t| -> Result<(), Box<dyn Error>> {
        let attic = attic(t)?;
        let coin = t.all::<Item>().find(|&i| t[i].name == "coin").ok_or("no coin")?;
        assert_eq!(t.target::<In>(coin), attic);
        assert_eq!(t.all::<Chest<R1>>().count(), 0);
        Ok(())
    })?;
    world.check()?;

    print!("{world}");
    Ok(())
}
