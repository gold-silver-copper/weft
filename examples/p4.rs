#![feature(decl_macro)]
use std::error::Error;
use weft::*;

/// A reusable module. A game drops it in with one path in `game!` and one
/// `is!` line naming which of its own kinds can hold items.
pub mod containers {
    use weft::*;

    #[kind(needs = [In])]
    pub struct Item {
        pub name: &'static str,
    }

    set!(pub Holder);

    #[relation(Item -> Holder, one, cascade)]
    pub struct In;

    #[component(Item)]
    pub struct Weight(pub u32);

    module!(pub Containers { kinds: [Item], relations: [In], components: [Weight] });

    /// Total weight of everything inside `holder`, at any depth.
    pub fn load<'t, G: Containers>(t: &Turn<'t, G>, holder: Id<'t, Holder>) -> u32 {
        t.descendants::<In>(holder)
            .filter_map(|item| t.get::<Weight>(item))
            .fold(0, |sum, w| sum.saturating_add(w.0))
    }
}

use containers::{Holder, In, Item, Weight};

#[kind(root, needs = [Plays])]
pub struct Settings;

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

#[kind(needs = [Opens])]
pub struct Door {
    pub name: &'static str,
}

set!(pub Walker);
is!(Player, Npc: Walker);
is!(Room, Player, Npc: Holder); // plug this game's kinds into the module's set

#[relation(Walker -> Room, one, refuse)]
pub struct At;

#[relation(Settings -> Player, one, refuse)]
pub struct Plays;

#[relation(Door -> Room, one, cascade)]
pub struct Opens;
// The new relation, every rule in one line: an NPC guards at most one door (`opt`),
// only NPCs guard and only doors are guarded, and a destroyed door just stops being guarded.
#[relation(Npc -> Door, opt, detach)]
pub struct Guards;
// The new component: only items can be lit. No edit to `Item` or `containers`.
#[component(Item)]
pub struct Lit(pub bool);

module!(pub Town {
    kinds: [Room, Player, Npc, Door],
    relations: [At, Plays, Opens, Guards],
    components: [Lit],
});
// Modules are named by path. Two modules called `Containers` in different crates don't collide.
game!(pub Game: Settings [Town, containers::Containers]);

/// `give <noun> to <npc>`. The bounds are the whole story: it touches `In` and `At`
/// and the kinds they connect. `Guards` and `Lit` are unreachable from here.
pub fn give<'t, G>(t: &mut Turn<'t, G>, me: Id<'t, Player>, noun: &str, to: &str) -> Result<String, Refused>
where
    G: Has<In> + Has<At> + Has<Item> + Has<Player> + Has<Npc> + Has<Room>,
{
    let here = t.target::<At>(me);
    let item = t.sources::<In>(me).map(|(item, _)| item).find(|&item| t[item].name == noun);
    let npc = t
        .sources::<At>(here)
        .filter_map(|(walker, _)| t.cast::<Npc>(walker))
        .find(|&npc| t[npc].name == to);
    match (item, npc) {
        (None, _) => Ok(format!("You have no {noun}.")),
        (_, None) => Ok(format!("There is no {to} here.")),
        (Some(item), Some(npc)) => {
            t.relate(item, In, npc)?;
            Ok(format!("You give the {noun} to the {to}."))
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut world = World::<Game>::new(|t| {
        let square = t.spawn(Room { name: "square" }, ());
        let vault = t.spawn(Room { name: "vault" }, ());
        let door = t.spawn(Door { name: "vault door" }, (Opens, vault));
        let gate = t.spawn(Door { name: "gate" }, (Opens, square));
        let me = t.spawn(Player, (At, square));
        let smith = t.spawn(Npc { name: "smith" }, (At, square));
        let lamp = t.spawn(Item { name: "lamp" }, (In, me));
        t.insert(lamp, Lit(true));
        t.insert(lamp, Weight(2));
        t.relate(smith, Guards, door);
        t.relate(smith, Guards, gate); // `opt`: replaces, so the smith guards only the gate
        t.spawn(Settings, (Plays, me))
    });

    world.turn(|t| -> Result<(), Box<dyn Error>> {
        let me = t.target::<Plays>(t.root());
        println!("{}", give(t, me, "lamp", "smith")?); // "You give the lamp to the smith."

        let smith = t.all::<Npc>().find(|&n| t[n].name == "smith").ok_or("no smith")?;
        let lamp = t.all::<Item>().find(|&i| t[i].name == "lamp").ok_or("no lamp")?;
        assert_eq!(t.target::<In>(lamp), smith);
        assert_eq!(t.get::<Lit>(lamp).map(|l| l.0), Some(true));
        assert_eq!(containers::load(t, smith.up()), 2);
        assert_eq!(t.target::<Guards>(smith).map(|d| t[d].name), Some("gate"));
        Ok(())
    })?;
    world.check()?;
    Ok(())
}
