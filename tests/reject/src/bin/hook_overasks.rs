//! C7: a hook can only reach what its declared bound names.
#![feature(decl_macro)]
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings);

#[derive(Debug)]
pub struct Room;
kind!(Room);

#[derive(Debug)]
pub struct Player;
kind!(Player needs At);

#[derive(Debug)]
pub struct Item;
kind!(Item needs In);

set!(pub Holder);
is!(Room, Player: Holder);

relation!(pub struct In: Item -> Holder, one, cascade);
// `pick_up` moves items through `In`, but `Walk` only names `At`.
#[cfg(not(twin))]
relation!(pub struct At: Player -> Room, one, refuse, on_relate = pick_up: Walk); // ~ERROR E0277
#[cfg(twin)]
relation!(pub struct At: Player -> Room, one, refuse, on_relate = pick_up: Map);

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
