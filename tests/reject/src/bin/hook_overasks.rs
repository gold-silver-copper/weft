//! C7: a hook can only reach what its declared bound names.
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

// `pick_up` moves items through `In`, but `Walk` only names `At`.
#[cfg_attr(not(twin), relation(Player -> Room, one, refuse, on_relate = pick_up: Walk))] // ~ERROR E0277
#[cfg_attr(twin, relation(Player -> Room, one, refuse, on_relate = pick_up: Map))]
pub struct At;

module!(pub Walk { kinds: [Room, Player], relations: [At] });
module!(pub Map { kinds: [Room, Player, Item], relations: [In, At] });
game!(pub Game: Settings [Map]);

fn pick_up<'t, G: Map>(t: &mut Turn<'t, G>, me: Id<'t, Player>, room: Id<'t, Room>) -> Result<(), Refused> {
    let loose: Vec<_> = t.sources::<In>(room).map(|(item, _)| item).collect();
    for item in loose {
        t.relate(item, In, me)?;
    }
    Ok(())
}

fn main() {}
