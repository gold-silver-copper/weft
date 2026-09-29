#![feature(decl_macro)]
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings needs Start, Plays);

#[derive(Debug)]
pub struct Room {
    pub name: &'static str,
    pub desc: &'static str,
}
kind!(Room);

#[derive(Debug)]
pub struct Player;
kind!(Player needs At);

#[derive(Debug)]
pub struct Item {
    pub name: &'static str,
}
kind!(Item needs In); // an `Item` can't be spawned without its container

set!(pub Holder);
is!(Room, Player: Holder);

// A two-way exit is one fact with a label for each direction.
relation!(pub struct Exit(pub &'static str): Room <-> Room, many, cyclic, detach);
// `Item` is not a `Holder`, so `In` can't loop and needs no loop rule (`game!` checks this).
relation!(pub struct In: Item -> Holder, one, cascade);
// Where the player stands. Setting it runs `entered`, which may use anything in `Map`.
relation!(pub struct At: Player -> Room, one, refuse, on_relate = entered: Map);
relation!(pub struct Start: Settings -> Room, one, refuse);
relation!(pub struct Plays: Settings -> Player, one, refuse);

module!(pub Map { kinds: [Room, Player, Item], relations: [Exit, In, At, Start, Plays] });
game!(pub Game: Settings [Map]);

fn entered<'t, G: Map>(t: &mut Turn<'t, G>, _: Id<'t, Player>, room: Id<'t, Room>) -> Result<(), Refused> {
    println!("You enter the {}.", t[room].name);
    Ok(())
}

fn describe<'t, G: Map>(t: &Turn<'t, G>, room: Id<'t, Room>) {
    println!("{}\n{}", t[room].name, t[room].desc);
    for (to, exit) in t.targets::<Exit>(room) {
        println!("  {}: {}", exit.0, t[to].name);
    }
    for (item, _) in t.sources::<In>(room) {
        println!("  There is a {} here.", t[item].name);
    }
}

/// `take <noun>`. `In` is exactly-one, so relating the item again *is* the move.
fn take<'t, G: Map>(t: &mut Turn<'t, G>, me: Id<'t, Player>, noun: &str) -> Result<bool, Refused> {
    let here = t.target::<At>(me);
    let found = t.sources::<In>(here).map(|(item, _)| item).find(|&item| t[item].name == noun);
    match found {
        Some(item) => t.relate(item, In, me).map(|()| true),
        None => Ok(false),
    }
}

fn main() -> Result<(), Refused> {
    let mut world = World::<Game>::new(|t| {
        let hall = t.spawn(Room { name: "Hall", desc: "A draughty hall." }, ());
        let cellar = t.spawn(Room { name: "Cellar", desc: "It smells of apples." }, ());
        let garden = t.spawn(Room { name: "Garden", desc: "Roses, everywhere." }, ());
        t.relate(hall, (Exit("down"), Exit("up")), cellar);
        t.relate(hall, (Exit("north"), Exit("south")), garden);
        t.relate(cellar, (Exit("tunnel"), Exit("trapdoor")), garden);
        t.spawn(Item { name: "lamp" }, (In, cellar));
        t.spawn(Item { name: "rug" }, (In, cellar));
        let me = t.spawn(Player, (At, hall));
        t.spawn(Settings, ((Start, hall), (Plays, me)))
    });

    world.turn(|t| -> Result<(), Refused> {
        let me = t.target::<Plays>(t.root());
        let here = t.target::<At>(me);
        describe(t, here); // "Hall", "down: Cellar", "north: Garden"

        // "go down": `entered` prints "You enter the Cellar."
        let down = t.targets::<Exit>(here).find(|(_, exit)| exit.0 == "down").map(|(room, _)| room);
        let Some(cellar) = down else { return Ok(()) };
        t.relate(me, At, cellar)?;
        describe(t, cellar); // "up: Hall", "tunnel: Garden", the lamp and the rug

        // "take lamp"
        assert!(take(t, me, "lamp")?);
        let carried: Vec<_> = t.sources::<In>(me).map(|(item, _)| t[item].name).collect();
        let lying: Vec<_> = t.sources::<In>(cellar).map(|(item, _)| t[item].name).collect();
        assert_eq!(carried, ["lamp"]);
        assert_eq!(lying, ["rug"]);
        println!("You carry: {}", carried.join(", "));
        Ok(())
    })
}
