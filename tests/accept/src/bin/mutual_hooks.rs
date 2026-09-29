//! C7: two hooks that relate each other's relations compile (no E0275, no E0283) and settle.
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
pub struct Npc {
    pub name: &'static str,
}
kind!(Npc needs At);

// Arriving somewhere makes an NPC follow whoever is already there (`At` relates `Follows`);
// following someone takes the follower to the leader's room (`Follows` relates `At`).
relation!(pub struct At: Npc -> Room, one, refuse, on_relate = arrive: Town);
relation!(pub struct Follows: Npc -> Npc, opt, cyclic, detach, on_relate = catch_up: Town);

module!(pub Town { kinds: [Room, Npc], relations: [At, Follows] });
game!(pub Game: Settings [Town]);

fn arrive<'t, G: Town>(t: &mut Turn<'t, G>, npc: Id<'t, Npc>, room: Id<'t, Room>) -> Result<(), Refused> {
    let leader = t.sources::<At>(room).map(|(other, _)| other).find(|&other| other != npc);
    match leader {
        Some(leader) if t.target::<Follows>(npc) != Some(leader) => t.relate(npc, Follows, leader),
        _ => Ok(()),
    }
}

fn catch_up<'t, G: Town>(t: &mut Turn<'t, G>, npc: Id<'t, Npc>, leader: Id<'t, Npc>) -> Result<(), Refused> {
    let there = t.target::<At>(leader);
    if t.target::<At>(npc) == there { Ok(()) } else { t.relate(npc, At, there) }
}

fn main() {
    let mut world = World::<Game>::new(|t| {
        let square = t.spawn(Room { name: "square" }, ());
        let inn = t.spawn(Room { name: "inn" }, ());
        t.spawn(Npc { name: "smith" }, (At, square));
        t.spawn(Npc { name: "baker" }, (At, inn));
        t.spawn(Settings, ())
    });
    let result = world.turn(|t| -> Result<(&'static str, &'static str), Refused> {
        let (Some(smith), Some(baker)) = (t.all::<Npc>().next(), t.all::<Npc>().nth(1)) else { return Err(Refused::Held) };
        t.relate(baker, Follows, smith)?; // `catch_up` walks the baker to the square
        let here = t.target::<At>(baker);
        let leader = t.target::<Follows>(baker).map_or("nobody", |l| t[l].name);
        Ok((t[here].name, leader))
    });
    assert_eq!(result, Ok(("square", "smith")));
    assert_eq!(world.check(), Ok(()));
}
