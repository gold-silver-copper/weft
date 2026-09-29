#![feature(decl_macro)]
use weft::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    #[default]
    Normal,
    Hard,
}

/// Game-wide settings are the root entity's data: one per world, always there,
/// typed, and reachable from every turn with `t.settings()`.
#[derive(Debug, Default)]
pub struct Settings {
    pub turn_limit: u32,
    pub difficulty: Difficulty,
}
// The starting room is an edge, not a field. `Start` refuses despawn, so the
// setting can never name a room that's gone.
kind!(root Settings needs Start, Plays);

#[derive(Debug)]
pub struct Room {
    pub name: &'static str,
}
kind!(Room);

#[derive(Debug)]
pub struct Player;
kind!(Player needs At);

#[derive(Debug)]
pub struct Npc {
    pub name: &'static str,
}
kind!(Npc needs At);

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

set!(pub Walker);
is!(Player, Npc: Walker);
set!(pub Thing);
is!(Item: Thing);
set!(pub Holder);
is!(Room, Player: Holder);
// Nesting on: a chest is both a thing and a holder, and `ranked` keeps `In` loop-free at
// compile time. To turn nesting off, drop this line and `Chest<R1>` from `Containers`.
is!(Chest<R>: Thing, Holder);

// Each relation's rules are chosen where it's declared.
// Two-way (`<->`): relate takes both labels, unrelate removes both.
relation!(pub struct Exit(pub &'static str): Room <-> Room, many, cyclic, detach);
// One-way (`->`): a chute you can't climb back up.
relation!(pub struct Chute: Room -> Room, many, cyclic, detach);
// Destroying a chest spills its contents into whatever held it.
relation!(pub struct In: Thing -> Holder, one, ranked, spill);
// A room with someone standing in it can't be destroyed. Arriving runs `arrive`.
relation!(pub struct At: Walker -> Room, one, refuse, on_relate = arrive: Map);
relation!(pub struct Start: Settings -> Room, one, refuse);
relation!(pub struct Plays: Settings -> Player, one, refuse);
relation!(pub struct Likes(pub u8): Npc -> Npc, many, cyclic, detach);

module!(pub Map { kinds: [Room, Player], relations: [Exit, Chute, At, Start, Plays] });
module!(pub Containers { kinds: [Item, Chest<R1>], relations: [In] });
module!(pub Folk { kinds: [Npc], relations: [Likes] });

// Parts on or off: two games from the same modules.
game!(pub Full: Settings [Map, Containers, Folk]);
game!(pub Quiet: Settings [Map, Containers]); // no NPCs, e.g. for tests

fn arrive<'t, G: Map>(t: &mut Turn<'t, G>, who: Id<'t, Walker>, room: Id<'t, Room>) -> Result<(), Refused> {
    if t.cast::<Player>(who).is_some() {
        println!("You arrive in the {}.", t[room].name);
    }
    Ok(())
}

fn build<G>(settings: Settings) -> World<G>
where
    G: Map + Containers + Schema<Root = Settings>,
{
    World::new(|t| {
        let hall = t.spawn(Room { name: "hall" }, ());
        let pit = t.spawn(Room { name: "pit" }, ());
        t.relate(hall, Chute, pit);
        t.spawn(Item { name: "lamp" }, (In, hall));
        let box_ = t.spawn(Chest { name: "box", rank: R1 }, (In, hall));
        t.spawn(Item { name: "coin" }, (In, box_));
        let me = t.spawn(Player, (At, hall));
        t.spawn(settings, ((Start, hall), (Plays, me)))
    })
}

/// NPC behaviour. Only compiles for games that include `Folk`.
fn haunt<'t, G>(t: &mut Turn<'t, G>) -> Result<(), Refused>
where
    G: Map + Folk + Schema<Root = Settings>,
{
    if t.settings().difficulty != Difficulty::Hard {
        return Ok(());
    }
    let me = t.target::<Plays>(t.root());
    let here = t.target::<At>(me);
    let npcs: Vec<_> = t.all::<Npc>().collect();
    for npc in npcs {
        t.relate(npc, At, here)?;
    }
    Ok(())
}

fn play<G>(
    world: &mut World<G>,
    commands: &[&str],
    mut npcs: impl for<'t> FnMut(&mut Turn<'t, G>) -> Result<(), Refused>,
) -> Result<u32, Refused>
where
    G: Map + Containers + Schema<Root = Settings>,
{
    let mut turns = 0;
    for &command in commands {
        let over = world.turn(|t| -> Result<bool, Refused> {
            if turns >= t.settings().turn_limit {
                return Ok(true);
            }
            let me = t.target::<Plays>(t.root());
            let here = t.target::<At>(me);
            match command {
                "jump" => {
                    let below = t.targets::<Chute>(here).map(|(room, _)| room).next();
                    if let Some(below) = below {
                        t.relate(me, At, below)?;
                    }
                }
                "take lamp" => {
                    let lamp = t
                        .sources::<In>(here)
                        .filter_map(|(thing, _)| t.cast::<Item>(thing))
                        .find(|&i| t[i].name == "lamp");
                    if let Some(lamp) = lamp {
                        t.relate(lamp, In, me)?;
                    }
                }
                _ => println!("Nothing happens."),
            }
            npcs(t)?;
            Ok(false)
        })?;
        if over {
            break;
        }
        turns += 1;
    }
    Ok(turns)
}

fn main() -> Result<(), Refused> {
    let mut full: World<Full> = build(Settings { turn_limit: 20, difficulty: Difficulty::Hard });
    full.turn(|t| {
        let start = t.target::<Start>(t.root());
        t.spawn(Npc { name: "ghost" }, (At, start)).map(drop)
    })?;
    play(&mut full, &["take lamp", "jump"], haunt::<Full>)?;

    let mut quiet: World<Quiet> = build(Settings { turn_limit: 5, ..Settings::default() });
    play(&mut quiet, &["take lamp", "jump"], |_| Ok(()))?;
    // play(&mut quiet, &[], haunt::<Quiet>); // doesn't compile: `Quiet` has no `Npc`, no `Likes`

    // Retune mid-game: settings are ordinary data on the root.
    full.turn(|t| t.settings_mut().difficulty = Difficulty::Easy);
    print!("{full}");
    Ok(())
}
