//! C7: every caller of a relation carries the bound its hook declares.
#![feature(decl_macro)]
use weft::*;

#[kind(root)]
pub struct Settings;

#[kind]
pub struct Room;

#[kind(needs = [At])]
pub struct Player;

#[kind(needs = [In])]
pub struct Item;

set!(pub Holder);
is!(Room, Player: Holder);

#[relation(Item -> Holder, one, cascade)]
pub struct In;
#[relation(Player -> Room, one, refuse, on_relate = pick_up: Map)]
pub struct At;

module!(pub Map { kinds: [Room, Player, Item], relations: [In, At] });
game!(pub Game: Settings [Map]);

fn pick_up<'t, G: Map>(t: &mut Turn<'t, G>, me: Id<'t, Player>, room: Id<'t, Room>) -> Result<(), Refused> {
    let loose: Vec<_> = t.sources::<In>(room).map(|(item, _)| item).collect();
    for item in loose {
        t.relate(item, In, me)?;
    }
    Ok(())
}

/// Walks the player. Its bounds name `At`, but not everything `At`'s hook reaches.
#[cfg(not(twin))]
fn walk<'t, G>(t: &mut Turn<'t, G>, me: Id<'t, Player>, to: Id<'t, Room>) -> Result<(), Refused>
where
    G: Has<At> + Has<Player> + Has<Room>,
{
    t.relate(me, At, to) // ~ERROR E0277
}
#[cfg(twin)]
fn walk<'t, G: Map>(t: &mut Turn<'t, G>, me: Id<'t, Player>, to: Id<'t, Room>) -> Result<(), Refused> {
    t.relate(me, At, to)
}

fn main() {
    let mut world = World::<Game>::new(|t| {
        let hall = t.spawn(Room, ());
        t.spawn(Player, (At, hall));
        t.spawn(Settings, ())
    });
    let _ = world.turn(|t| {
        let (me, room) = (t.all::<Player>().next(), t.all::<Room>().next());
        match (me, room) {
            (Some(me), Some(room)) => walk(t, me, room),
            _ => Ok(()),
        }
    });
}
