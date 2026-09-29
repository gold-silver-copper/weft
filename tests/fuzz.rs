//! Gates for C2, C3, C4, C6, C8 and C18: a differential fuzzer.
//!
//! `differential` builds random worlds and plays random turns of spawns, moves, relates,
//! unrelates, despawns and component writes against both the library and `Model`, a naive
//! re-implementation of the same rules written for clarity rather than speed. After every
//! operation it compares everything the public API can observe, and every operation's
//! outcome (`Ok`, or which `Refused`). After every turn `World::check` must pass.
//!
//! `planted_fault_is_caught` breaks the model on purpose (cascade stops cascading) and
//! requires the fuzzer to notice, so a comparison that checks nothing can't pass.
//!
//! `hooks_settle` runs hooks that never settle, linearly and branching, and requires them to
//! be refused rather than overflow the stack or hang.
#![feature(decl_macro)]

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use weft::*;

// ============================================================ the game under test

#[derive(Debug)]
pub struct Settings {
    pub seed: u64,
}
kind!(root Settings needs Start);

#[derive(Debug)]
pub struct Room {
    pub id: u32,
}
kind!(Room);

#[derive(Debug)]
pub struct Npc {
    pub id: u32,
}
kind!(Npc needs At);

#[derive(Debug)]
pub struct Item {
    pub id: u32,
}
kind!(Item rank 0 needs In);

#[derive(Debug)]
pub struct Chest<const R: u8> {
    pub id: u32,
}
kind!(Chest<const R: u8> rank R needs In);

#[derive(Debug)]
pub struct Coin {
    pub id: u32,
}
kind!(Coin rank 0 needs Stash);

set!(pub Thing);
set!(pub Holder);
is!(Item: Thing);
is!(Chest<const R: u8>: Thing, Holder);
is!(Room, Npc: Holder);

// Things nest by rank and spill up when their holder goes; coins go with theirs.
relation!(pub struct In: Thing -> Holder, one, ranked, spill);
relation!(pub struct Stash: Coin -> Holder, one, ranked, cascade);
relation!(pub struct Start: Settings -> Room, one, refuse);
// Arriving in a room guards it, then picks up its loose items.
relation!(pub struct At: Npc -> Room, one, refuse, on_relate = arrive: Fz);
relation!(pub struct Exit(pub u8): Room <-> Room, many, cyclic, detach);
relation!(pub struct Likes(pub u8): Npc -> Npc, many, cyclic, detach);
relation!(pub struct Guards: Npc -> Room, opt, detach);
relation!(pub struct Owns: Npc -> Item, many, refuse);
component!(pub struct Weight(pub u32): Item);

module!(pub Fz {
    kinds: [Room, Npc, Item, Chest<1>, Chest<2>, Coin],
    relations: [In, Stash, Start, At, Exit, Likes, Guards, Owns],
    components: [Weight],
});
game!(pub Fuzz: Settings [Fz]);

/// The hook refuses partway when the NPC is doomed: the `At` edge and the `Guards` edge
/// stay, and moving the first item fails.
fn arrive<'t, G: Fz>(t: &mut Turn<'t, G>, npc: Id<'t, Npc>, room: Id<'t, Room>) -> Result<(), Refused> {
    t.relate(npc, Guards, room)?;
    let mut loose: Vec<_> = t.sources::<In>(room).filter_map(|(x, _)| t.cast::<Item>(x)).collect();
    loose.sort_by_key(|&item| t[item].id);
    for item in loose {
        t.relate(item, In, npc)?;
    }
    Ok(())
}

// ============================================================ the model

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum K {
    Room,
    Npc,
    Item,
    Chest1,
    Chest2,
    Coin,
}

const ROOT: u32 = u32::MAX;

impl K {
    fn rank(self) -> u8 {
        match self {
            K::Item | K::Coin => 0,
            K::Chest1 => 1,
            K::Chest2 => 2,
            K::Room | K::Npc => 255,
        }
    }
    fn holds(self) -> bool {
        matches!(self, K::Room | K::Npc | K::Chest1 | K::Chest2)
    }
    fn is_thing(self) -> bool {
        matches!(self, K::Item | K::Chest1 | K::Chest2)
    }
}

/// The rules, written out plainly: one map or list per relation, entities by id.
#[derive(Default, Debug)]
struct Model {
    kinds: BTreeMap<u32, K>,
    inn: BTreeMap<u32, u32>,
    stash: BTreeMap<u32, u32>,
    at: BTreeMap<u32, u32>,
    start: u32,
    /// `(a, b, label seen from a, label seen from b)`, oldest first.
    exits: Vec<(u32, u32, u8, u8)>,
    likes: Vec<(u32, u32, u8)>,
    guards: BTreeMap<u32, u32>,
    owns: Vec<(u32, u32)>,
    weight: BTreeMap<u32, u32>,
    /// Explicit despawns this turn.
    doomed: Vec<u32>,
    next: u32,
    /// The planted fault: `Stash` stops cascading.
    fault: bool,
}

type Outcome = Result<(), Refused>;

impl Model {
    fn fresh(&mut self, kind: K) -> u32 {
        let id = self.next;
        self.next += 1;
        self.kinds.insert(id, kind);
        id
    }

    fn of(&self, kind: K) -> Vec<u32> {
        self.kinds.iter().filter(|(_, k)| **k == kind).map(|(id, _)| *id).collect()
    }

    /// Where a spilled thing lands: the first holder up its `In` chain that survives.
    fn spill_target(&self, thing: u32, set: &BTreeSet<u32>) -> Option<u32> {
        let mut at = *self.inn.get(&thing)?;
        while set.contains(&at) {
            at = *self.inn.get(&at)?;
        }
        Some(at)
    }

    /// Everything that goes at the end of this turn: the explicit despawns, what cascades
    /// from them, and what would spill but has nowhere to land.
    fn plan(&self, extra: Option<u32>) -> BTreeSet<u32> {
        let mut set: BTreeSet<u32> = self.doomed.iter().copied().chain(extra).collect();
        loop {
            let before = set.len();
            if !self.fault {
                for (&coin, &holder) in &self.stash {
                    if set.contains(&holder) {
                        set.insert(coin);
                    }
                }
            }
            let stranded: Vec<u32> = self
                .inn
                .iter()
                .filter(|(thing, holder)| set.contains(holder) && !set.contains(thing))
                .filter(|(thing, _)| self.spill_target(**thing, &set).is_none())
                .map(|(thing, _)| *thing)
                .collect();
            set.extend(stranded);
            if set.len() == before {
                return set;
            }
        }
    }

    /// Sources of `refuse` edges into `n`.
    fn holders(&self, n: u32) -> Vec<u32> {
        let mut out: Vec<u32> = self.at.iter().filter(|(_, r)| **r == n).map(|(npc, _)| *npc).collect();
        if self.start == n {
            out.push(ROOT);
        }
        out.extend(self.owns.iter().filter(|(_, item)| *item == n).map(|(npc, _)| *npc));
        out
    }

    fn despawn(&mut self, x: u32) -> Outcome {
        let plan = self.plan(Some(x));
        if plan.iter().any(|&n| self.holders(n).iter().any(|h| !plan.contains(h))) {
            return Err(Refused::Held);
        }
        self.doomed.push(x);
        Ok(())
    }

    fn doomed(&self, target: u32) -> Outcome {
        if self.plan(None).contains(&target) { Err(Refused::Doomed) } else { Ok(()) }
    }

    fn relate_in(&mut self, thing: u32, holder: u32) -> Outcome {
        self.doomed(holder)?;
        self.inn.insert(thing, holder);
        Ok(())
    }

    fn relate_stash(&mut self, coin: u32, holder: u32) -> Outcome {
        self.doomed(holder)?;
        self.stash.insert(coin, holder);
        Ok(())
    }

    fn walk(&mut self, npc: u32, room: u32) -> Outcome {
        self.doomed(room)?;
        self.at.insert(npc, room);
        self.arrive(npc, room)
    }

    fn arrive(&mut self, npc: u32, room: u32) -> Outcome {
        self.doomed(room)?;
        self.guards.insert(npc, room);
        let loose: Vec<u32> = self
            .inn
            .iter()
            .filter(|(item, holder)| **holder == room && self.kinds.get(item) == Some(&K::Item))
            .map(|(item, _)| *item)
            .collect();
        for item in loose {
            self.relate_in(item, npc)?;
        }
        Ok(())
    }

    fn relate_exit(&mut self, a: u32, b: u32, x: u8, y: u8) -> Outcome {
        self.doomed(b)?;
        self.exits.retain(|e| !((e.0 == a && e.1 == b) || (e.0 == b && e.1 == a)));
        self.exits.push((a, b, x, y));
        Ok(())
    }

    fn unrelate_exit(&mut self, a: u32, b: u32) -> bool {
        let before = self.exits.len();
        self.exits.retain(|e| !((e.0 == a && e.1 == b) || (e.0 == b && e.1 == a)));
        self.exits.len() != before
    }

    fn relate_likes(&mut self, a: u32, b: u32, v: u8) -> Outcome {
        self.doomed(b)?;
        self.likes.retain(|e| !(e.0 == a && e.1 == b));
        self.likes.push((a, b, v));
        Ok(())
    }

    fn unrelate_likes(&mut self, a: u32, b: u32) -> bool {
        let before = self.likes.len();
        self.likes.retain(|e| !(e.0 == a && e.1 == b));
        self.likes.len() != before
    }

    fn relate_guards(&mut self, npc: u32, room: u32) -> Outcome {
        self.doomed(room)?;
        self.guards.insert(npc, room);
        Ok(())
    }

    fn unrelate_guards(&mut self, npc: u32, room: u32) -> bool {
        if self.guards.get(&npc) == Some(&room) {
            self.guards.remove(&npc);
            true
        } else {
            false
        }
    }

    fn relate_owns(&mut self, npc: u32, item: u32) -> Outcome {
        self.doomed(item)?;
        self.owns.retain(|e| *e != (npc, item));
        self.owns.push((npc, item));
        Ok(())
    }

    fn unrelate_owns(&mut self, npc: u32, item: u32) -> bool {
        let before = self.owns.len();
        self.owns.retain(|e| *e != (npc, item));
        self.owns.len() != before
    }

    /// The end of a turn: spill what can be spilled, then drop everything in the plan and
    /// every edge that touches it.
    fn end_turn(&mut self) {
        let plan = self.plan(None);
        let moves: Vec<(u32, u32)> = self
            .inn
            .iter()
            .filter(|(thing, holder)| plan.contains(holder) && !plan.contains(thing))
            .filter_map(|(thing, _)| self.spill_target(*thing, &plan).map(|to| (*thing, to)))
            .collect();
        for (thing, to) in moves {
            self.inn.insert(thing, to);
        }
        let gone = |id: &u32| plan.contains(id);
        self.kinds.retain(|id, _| !gone(id));
        self.inn.retain(|a, b| !gone(a) && !gone(b));
        self.stash.retain(|a, b| !gone(a) && !gone(b));
        self.at.retain(|a, b| !gone(a) && !gone(b));
        self.exits.retain(|e| !gone(&e.0) && !gone(&e.1));
        self.likes.retain(|e| !gone(&e.0) && !gone(&e.1));
        self.guards.retain(|a, b| !gone(a) && !gone(b));
        self.owns.retain(|e| !gone(&e.0) && !gone(&e.1));
        self.weight.retain(|a, _| !gone(a));
        self.doomed.clear();
    }

    /// `(id, label)` pairs an exit shows from `room`.
    fn exits_from(&self, room: u32) -> Vec<(u32, u8)> {
        let mut out: Vec<(u32, u8)> = self
            .exits
            .iter()
            .filter_map(|&(a, b, x, y)| {
                if a == room {
                    Some((b, x))
                } else if b == room {
                    Some((a, y))
                } else {
                    None
                }
            })
            .collect();
        out.sort_unstable();
        out
    }
}

// ============================================================ reading the world

/// Every handle of this turn, by id.
struct Ids<'t> {
    kinds: BTreeMap<u32, K>,
    rooms: BTreeMap<u32, Id<'t, Room>>,
    npcs: BTreeMap<u32, Id<'t, Npc>>,
    items: BTreeMap<u32, Id<'t, Item>>,
    c1: BTreeMap<u32, Id<'t, Chest<1>>>,
    c2: BTreeMap<u32, Id<'t, Chest<2>>>,
    coins: BTreeMap<u32, Id<'t, Coin>>,
}

fn ids<'t>(t: &Turn<'t, Fuzz>) -> Result<Ids<'t>, String> {
    let mut kinds = BTreeMap::new();
    let mut dup = |id: u32, k: K| match kinds.insert(id, k) {
        None => Ok(()),
        Some(other) => Err(format!("id {id} is both {other:?} and {k:?}")),
    };
    let rooms: BTreeMap<_, _> = t.all::<Room>().map(|x| (t[x].id, x)).collect();
    let npcs: BTreeMap<_, _> = t.all::<Npc>().map(|x| (t[x].id, x)).collect();
    let items: BTreeMap<_, _> = t.all::<Item>().map(|x| (t[x].id, x)).collect();
    let c1: BTreeMap<_, _> = t.all::<Chest<1>>().map(|x| (t[x].id, x)).collect();
    let c2: BTreeMap<_, _> = t.all::<Chest<2>>().map(|x| (t[x].id, x)).collect();
    let coins: BTreeMap<_, _> = t.all::<Coin>().map(|x| (t[x].id, x)).collect();
    let counts = [
        (K::Room, t.all::<Room>().count(), rooms.len()),
        (K::Npc, t.all::<Npc>().count(), npcs.len()),
        (K::Item, t.all::<Item>().count(), items.len()),
        (K::Chest1, t.all::<Chest<1>>().count(), c1.len()),
        (K::Chest2, t.all::<Chest<2>>().count(), c2.len()),
        (K::Coin, t.all::<Coin>().count(), coins.len()),
    ];
    for (k, listed, distinct) in counts {
        if listed != distinct {
            return Err(format!("all::<{k:?}>() lists {listed} entities with only {distinct} distinct ids"));
        }
    }
    rooms.keys().try_for_each(|&id| dup(id, K::Room))?;
    npcs.keys().try_for_each(|&id| dup(id, K::Npc))?;
    items.keys().try_for_each(|&id| dup(id, K::Item))?;
    c1.keys().try_for_each(|&id| dup(id, K::Chest1))?;
    c2.keys().try_for_each(|&id| dup(id, K::Chest2))?;
    coins.keys().try_for_each(|&id| dup(id, K::Coin))?;
    Ok(Ids { kinds, rooms, npcs, items, c1, c2, coins })
}

fn holder_id<'t>(t: &Turn<'t, Fuzz>, h: Id<'t, Holder>) -> u32 {
    if let Some(x) = t.cast::<Room>(h) {
        t[x].id
    } else if let Some(x) = t.cast::<Npc>(h) {
        t[x].id
    } else if let Some(x) = t.cast::<Chest<1>>(h) {
        t[x].id
    } else if let Some(x) = t.cast::<Chest<2>>(h) {
        t[x].id
    } else {
        panic!("a holder that is no holder kind: {h}")
    }
}

fn thing_id<'t>(t: &Turn<'t, Fuzz>, x: Id<'t, Thing>) -> u32 {
    if let Some(i) = t.cast::<Item>(x) {
        t[i].id
    } else if let Some(c) = t.cast::<Chest<1>>(x) {
        t[c].id
    } else if let Some(c) = t.cast::<Chest<2>>(x) {
        t[c].id
    } else {
        panic!("a thing that is no thing kind: {x}")
    }
}

fn sorted<T: Ord>(mut v: Vec<T>) -> Vec<T> {
    v.sort();
    v
}

fn same<T: PartialEq + std::fmt::Debug>(what: impl std::fmt::Display, world: T, model: T) -> Result<(), String> {
    if world == model { Ok(()) } else { Err(format!("{what}: the world has {world:?}, the model {model:?}")) }
}

/// Everything observable, compared with the model.
fn compare(t: &Turn<'_, Fuzz>, m: &Model) -> Result<(), String> {
    let ids = ids(t)?;
    same("entities", &ids.kinds, &m.kinds)?;
    let plan = m.plan(None);
    for (&id, &kind) in &ids.kinds {
        let doomed = match kind {
            K::Room => t.is_doomed(ids.rooms[&id]),
            K::Npc => t.is_doomed(ids.npcs[&id]),
            K::Item => t.is_doomed(ids.items[&id]),
            K::Chest1 => t.is_doomed(ids.c1[&id]),
            K::Chest2 => t.is_doomed(ids.c2[&id]),
            K::Coin => t.is_doomed(ids.coins[&id]),
        };
        same(format_args!("is_doomed({id})"), doomed, plan.contains(&id))?;
    }
    same("Start", t[t.target::<Start>(t.root())].id, m.start)?;

    // `one` edges, from both ends.
    for (&id, &x) in &ids.items {
        same(format_args!("In of item {id}"), holder_id(t, t.target::<In>(x)), m.inn[&id])?;
        let up: Vec<u32> = t.ancestors::<In>(x).map(|h| holder_id(t, h)).collect();
        let mut chain = vec![m.inn[&id]];
        while let Some(&next) = chain.last().and_then(|h| m.inn.get(h)) {
            chain.push(next);
        }
        same(format_args!("ancestors of item {id}"), up, chain)?;
        let owners = sorted(t.sources::<Owns>(x).map(|(n, _)| t[n].id).collect());
        let model = sorted(m.owns.iter().filter(|e| e.1 == id).map(|e| e.0).collect());
        same(format_args!("owners of item {id}"), owners, model)?;
        same(format_args!("weight of item {id}"), t.get::<Weight>(x).map(|w| w.0), m.weight.get(&id).copied())?;
    }
    for (&id, &x) in &ids.c1 {
        same(format_args!("In of chest {id}"), holder_id(t, t.target::<In>(x)), m.inn[&id])?;
    }
    for (&id, &x) in &ids.c2 {
        same(format_args!("In of chest {id}"), holder_id(t, t.target::<In>(x)), m.inn[&id])?;
    }
    for (&id, &x) in &ids.coins {
        same(format_args!("Stash of coin {id}"), holder_id(t, t.target::<Stash>(x)), m.stash[&id])?;
    }
    for (&id, &x) in &ids.npcs {
        same(format_args!("At of npc {id}"), t[t.target::<At>(x)].id, m.at[&id])?;
        same(format_args!("Guards of npc {id}"), t.target::<Guards>(x).map(|r| t[r].id), m.guards.get(&id).copied())?;
        let likes = sorted(t.targets::<Likes>(x).map(|(n, v)| (t[n].id, v.0)).collect());
        let model = sorted(m.likes.iter().filter(|e| e.0 == id).map(|e| (e.1, e.2)).collect());
        same(format_args!("Likes from npc {id}"), likes, model)?;
        let liked = sorted(t.sources::<Likes>(x).map(|(n, v)| (t[n].id, v.0)).collect());
        let model = sorted(m.likes.iter().filter(|e| e.1 == id).map(|e| (e.0, e.2)).collect());
        same(format_args!("Likes into npc {id}"), liked, model)?;
        let owns = sorted(t.targets::<Owns>(x).map(|(i, _)| t[i].id).collect());
        let model = sorted(m.owns.iter().filter(|e| e.0 == id).map(|e| e.1).collect());
        same(format_args!("Owns of npc {id}"), owns, model)?;
    }

    // Everything that points into a holder.
    let holders: Vec<(u32, Id<'_, Holder>)> = ids
        .rooms
        .iter()
        .map(|(&id, &x)| (id, x.up::<Holder>()))
        .chain(ids.npcs.iter().map(|(&id, &x)| (id, x.up::<Holder>())))
        .chain(ids.c1.iter().map(|(&id, &x)| (id, x.up::<Holder>())))
        .chain(ids.c2.iter().map(|(&id, &x)| (id, x.up::<Holder>())))
        .collect();
    for (id, h) in holders {
        let inside = sorted(t.sources::<In>(h).map(|(x, _)| thing_id(t, x)).collect());
        let model = sorted(m.inn.iter().filter(|(_, v)| **v == id).map(|(k, _)| *k).collect());
        same(format_args!("In into {id}"), inside, model)?;
        let coins = sorted(t.sources::<Stash>(h).map(|(c, _)| t[c].id).collect());
        let model = sorted(m.stash.iter().filter(|(_, v)| **v == id).map(|(k, _)| *k).collect());
        same(format_args!("Stash into {id}"), coins, model)?;
        let below = sorted(t.descendants::<In>(h).map(|x| thing_id(t, x)).collect());
        let mut model = BTreeSet::new();
        let mut queue = vec![id];
        while let Some(at) = queue.pop() {
            for (&thing, _) in m.inn.iter().filter(|(_, v)| **v == at) {
                if model.insert(thing) {
                    queue.push(thing);
                }
            }
        }
        same(format_args!("descendants of {id}"), below, model.into_iter().collect())?;
    }

    for (&id, &room) in &ids.rooms {
        let exits = sorted(t.targets::<Exit>(room).map(|(r, e)| (t[r].id, e.0)).collect());
        same(format_args!("exits from room {id}"), exits, m.exits_from(id))?;
        let back = sorted(t.sources::<Exit>(room).map(|(r, e)| (t[r].id, e.0)).collect());
        same(format_args!("exits into room {id}"), back, m.exits_from(id))?;
        let standing = sorted(t.sources::<At>(room).map(|(n, _)| t[n].id).collect());
        let model = sorted(m.at.iter().filter(|(_, r)| **r == id).map(|(n, _)| *n).collect());
        same(format_args!("npcs at room {id}"), standing, model)?;
        let guards = sorted(t.sources::<Guards>(room).map(|(n, _)| t[n].id).collect());
        let model = sorted(m.guards.iter().filter(|(_, r)| **r == id).map(|(n, _)| *n).collect());
        same(format_args!("guards of room {id}"), guards, model)?;
    }
    Ok(())
}

// ============================================================ driving both

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn pick(&mut self, from: &[u32]) -> Option<u32> {
        (!from.is_empty()).then(|| from[self.below(from.len())])
    }
    fn label(&mut self) -> u8 {
        (self.next() % 250) as u8
    }
}

#[derive(Default, Debug)]
struct Stats {
    applied: u64,
    refused: u64,
    held: u64,
    despawned: u64,
}

/// Runs `op` against the world, `model` against the model, and requires the same outcome.
fn agree(stats: &mut Stats, what: impl std::fmt::Display, world: Outcome, model: Outcome) -> Result<(), String> {
    if world != model {
        return Err(format!("{what}: the world says {world:?}, the model {model:?}"));
    }
    match world {
        Ok(()) => stats.applied += 1,
        Err(_) => stats.refused += 1,
    }
    Ok(())
}

/// A handle of any holder kind, as the concrete kind, for code that must be written per kind.
macro_rules! holder {
    ($ids:expr, $id:expr, [$($k:ident => $field:ident),*], |$h:ident| $body:expr) => {
        match $ids.kinds[&$id] {
            $(K::$k => {
                let $h = $ids.$field[&$id];
                $body
            })*
            other => panic!("{other:?} can't hold this"),
        }
    };
}

fn holders_above(m: &Model, rank: u8) -> Vec<u32> {
    m.kinds.iter().filter(|(_, k)| k.holds() && k.rank() > rank).map(|(id, _)| *id).collect()
}

fn genesis<'t>(t: &mut Turn<'t, Fuzz, Genesis>, m: &mut Model, rng: &mut Rng) -> Rooted<'t, Settings> {
    let rooms: Vec<(u32, Id<'t, Room>)> = (0..2 + rng.below(3))
        .map(|_| {
            let id = m.fresh(K::Room);
            (id, t.spawn(Room { id }, ()))
        })
        .collect();
    for _ in 0..rng.below(4) {
        let (a, b) = (rooms[rng.below(rooms.len())], rooms[rng.below(rooms.len())]);
        let (x, y) = (rng.label(), rng.label());
        m.exits.retain(|e| !((e.0 == a.0 && e.1 == b.0) || (e.0 == b.0 && e.1 == a.0)));
        m.exits.push((a.0, b.0, x, y));
        t.relate(a.1, (Exit(x), Exit(y)), b.1);
    }
    for _ in 0..rng.below(3) {
        let room = rooms[rng.below(rooms.len())];
        let id = m.fresh(K::Npc);
        m.at.insert(id, room.0); // genesis runs no hooks
        t.spawn(Npc { id }, (At, room.1));
    }
    for _ in 0..rng.below(4) {
        let room = rooms[rng.below(rooms.len())];
        let id = m.fresh(K::Item);
        m.inn.insert(id, room.0);
        t.spawn(Item { id }, (In, room.1));
    }
    let start = rooms[0];
    m.start = start.0;
    t.spawn(Settings { seed: rng.0 }, (Start, start.1))
}

/// One random operation, on both, then a comparison of everything.
fn step(t: &mut Turn<'_, Fuzz>, m: &mut Model, rng: &mut Rng, stats: &mut Stats) -> Result<(), String> {
    let ids = ids(t)?;
    let rooms = m.of(K::Room);
    let npcs = m.of(K::Npc);
    let items = m.of(K::Item);
    let things: Vec<u32> = m.kinds.iter().filter(|(_, k)| k.is_thing()).map(|(id, _)| *id).collect();
    let everyone: Vec<u32> = m.kinds.keys().copied().collect();
    let id = m.next;
    match rng.below(21) {
        0 => {
            m.fresh(K::Room);
            t.spawn(Room { id }, ());
            stats.applied += 1;
        }
        1 => {
            let Some(room) = rng.pick(&rooms) else { return Ok(()) };
            let world = t.spawn(Npc { id }, (At, ids.rooms[&room])).map(drop);
            let model = m.doomed(room).and_then(|()| {
                m.fresh(K::Npc);
                m.at.insert(id, room);
                m.arrive(id, room)
            });
            agree(stats, format_args!("spawn npc {id} at {room}"), world, model)?;
        }
        2 => {
            let Some(h) = rng.pick(&holders_above(m, 0)) else { return Ok(()) };
            holder!(ids, h, [Room => rooms, Npc => npcs, Chest1 => c1, Chest2 => c2], |to| t.spawn(Item { id }, (In, to)));
            m.fresh(K::Item);
            m.inn.insert(id, h);
            stats.applied += 1;
        }
        3 => {
            let Some(h) = rng.pick(&holders_above(m, 1)) else { return Ok(()) };
            holder!(ids, h, [Room => rooms, Npc => npcs, Chest2 => c2], |to| t.spawn(Chest::<1> { id }, (In, to)));
            m.fresh(K::Chest1);
            m.inn.insert(id, h);
            stats.applied += 1;
        }
        4 => {
            let Some(h) = rng.pick(&holders_above(m, 2)) else { return Ok(()) };
            holder!(ids, h, [Room => rooms, Npc => npcs], |to| t.spawn(Chest::<2> { id }, (In, to)));
            m.fresh(K::Chest2);
            m.inn.insert(id, h);
            stats.applied += 1;
        }
        5 => {
            let Some(h) = rng.pick(&holders_above(m, 0)) else { return Ok(()) };
            holder!(ids, h, [Room => rooms, Npc => npcs, Chest1 => c1, Chest2 => c2], |to| t.spawn(Coin { id }, (Stash, to)));
            m.fresh(K::Coin);
            m.stash.insert(id, h);
            stats.applied += 1;
        }
        6 => {
            let Some(x) = rng.pick(&things) else { return Ok(()) };
            let Some(h) = rng.pick(&holders_above(m, m.kinds[&x].rank())) else { return Ok(()) };
            let world = match m.kinds[&x] {
                K::Item => {
                    let x = ids.items[&x];
                    holder!(ids, h, [Room => rooms, Npc => npcs, Chest1 => c1, Chest2 => c2], |to| t.relate(x, In, to))
                }
                K::Chest1 => {
                    let x = ids.c1[&x];
                    holder!(ids, h, [Room => rooms, Npc => npcs, Chest2 => c2], |to| t.relate(x, In, to))
                }
                K::Chest2 => {
                    let x = ids.c2[&x];
                    holder!(ids, h, [Room => rooms, Npc => npcs], |to| t.relate(x, In, to))
                }
                other => panic!("{other:?} is no thing"),
            };
            agree(stats, format_args!("move {x} into {h}"), world, m.relate_in(x, h))?;
        }
        7 => {
            let Some(c) = rng.pick(&m.of(K::Coin)) else { return Ok(()) };
            let Some(h) = rng.pick(&holders_above(m, 0)) else { return Ok(()) };
            let coin = ids.coins[&c];
            let world = holder!(ids, h, [Room => rooms, Npc => npcs, Chest1 => c1, Chest2 => c2], |to| t.relate(coin, Stash, to));
            agree(stats, format_args!("stash coin {c} in {h}"), world, m.relate_stash(c, h))?;
        }
        8 => {
            let (Some(n), Some(r)) = (rng.pick(&npcs), rng.pick(&rooms)) else { return Ok(()) };
            let world = t.relate(ids.npcs[&n], At, ids.rooms[&r]);
            agree(stats, format_args!("walk npc {n} to {r}"), world, m.walk(n, r))?;
        }
        9 => {
            let (Some(a), Some(b)) = (rng.pick(&rooms), rng.pick(&rooms)) else { return Ok(()) };
            let (x, y) = (rng.label(), rng.label());
            let world = t.relate(ids.rooms[&a], (Exit(x), Exit(y)), ids.rooms[&b]);
            agree(stats, format_args!("exit {a} -{x}/{y}- {b}"), world, m.relate_exit(a, b, x, y))?;
        }
        10 => {
            let (Some(a), Some(b)) = (rng.pick(&rooms), rng.pick(&rooms)) else { return Ok(()) };
            same(format_args!("unrelate exit {a} {b}"), t.unrelate::<Exit>(ids.rooms[&a], ids.rooms[&b]), m.unrelate_exit(a, b))?;
        }
        11 => {
            let (Some(a), Some(b)) = (rng.pick(&npcs), rng.pick(&npcs)) else { return Ok(()) };
            let v = rng.label();
            let world = t.relate(ids.npcs[&a], Likes(v), ids.npcs[&b]);
            agree(stats, format_args!("{a} likes {b} {v}"), world, m.relate_likes(a, b, v))?;
        }
        12 => {
            let (Some(a), Some(b)) = (rng.pick(&npcs), rng.pick(&npcs)) else { return Ok(()) };
            same(format_args!("unrelate likes {a} {b}"), t.unrelate::<Likes>(ids.npcs[&a], ids.npcs[&b]), m.unrelate_likes(a, b))?;
        }
        13 => {
            let (Some(n), Some(r)) = (rng.pick(&npcs), rng.pick(&rooms)) else { return Ok(()) };
            let world = t.relate(ids.npcs[&n], Guards, ids.rooms[&r]);
            agree(stats, format_args!("{n} guards {r}"), world, m.relate_guards(n, r))?;
        }
        14 => {
            let (Some(n), Some(r)) = (rng.pick(&npcs), rng.pick(&rooms)) else { return Ok(()) };
            same(format_args!("unrelate guards {n} {r}"), t.unrelate::<Guards>(ids.npcs[&n], ids.rooms[&r]), m.unrelate_guards(n, r))?;
        }
        15 => {
            let (Some(n), Some(i)) = (rng.pick(&npcs), rng.pick(&items)) else { return Ok(()) };
            let world = t.relate(ids.npcs[&n], Owns, ids.items[&i]);
            agree(stats, format_args!("{n} owns {i}"), world, m.relate_owns(n, i))?;
        }
        16 => {
            let (Some(n), Some(i)) = (rng.pick(&npcs), rng.pick(&items)) else { return Ok(()) };
            same(format_args!("unrelate owns {n} {i}"), t.unrelate::<Owns>(ids.npcs[&n], ids.items[&i]), m.unrelate_owns(n, i))?;
        }
        17 => {
            let Some(i) = rng.pick(&items) else { return Ok(()) };
            let item = ids.items[&i];
            if rng.below(2) == 0 {
                let w = rng.label().into();
                let old = t.insert(item, Weight(w)).map(|w| w.0);
                same(format_args!("insert weight on {i}"), old, m.weight.insert(i, w))?;
            } else {
                same(format_args!("remove weight from {i}"), t.remove::<Weight>(item).map(|w| w.0), m.weight.remove(&i))?;
            }
        }
        _ => {
            let Some(x) = rng.pick(&everyone) else { return Ok(()) };
            let world = match m.kinds[&x] {
                K::Room => t.despawn(ids.rooms[&x]),
                K::Npc => t.despawn(ids.npcs[&x]),
                K::Item => t.despawn(ids.items[&x]),
                K::Chest1 => t.despawn(ids.c1[&x]),
                K::Chest2 => t.despawn(ids.c2[&x]),
                K::Coin => t.despawn(ids.coins[&x]),
            };
            let model = m.despawn(x);
            match model {
                Err(Refused::Held) => stats.held += 1,
                Ok(()) => stats.despawned += 1,
                _ => {}
            }
            agree(stats, format_args!("despawn {x}"), world, model)?;
        }
    }
    Ok(())
}

/// Plays `turns` random turns on one random world, comparing after every step.
fn run(seed: u64, fault: bool, turns: usize, stats: &mut Stats) -> Result<(), String> {
    let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut m = Model { fault, ..Model::default() };
    let mut world = World::<Fuzz>::new(|t| genesis(t, &mut m, &mut rng));
    world.turn(|t| compare(t, &m)).map_err(|e| format!("seed {seed}, genesis: {e}"))?;
    for turn in 0..turns {
        let ops = 1 + rng.below(10);
        world.turn(|t| -> Result<(), String> {
            for op in 0..ops {
                step(t, &mut m, &mut rng, stats)
                    .and_then(|()| compare(t, &m))
                    .map_err(|e| format!("seed {seed}, turn {turn}, op {op}: {e}"))?;
            }
            Ok(())
        })?;
        m.end_turn();
        world.check().map_err(|e| format!("seed {seed}, after turn {turn}: check() failed: {e}"))?;
        world.turn(|t| compare(t, &m)).map_err(|e| format!("seed {seed}, after turn {turn}: {e}"))?;
    }
    Ok(())
}

const WORLDS: u64 = 60;
const TURNS: usize = 30;

#[test]
fn differential() {
    let mut stats = Stats::default();
    for seed in 1..=WORLDS {
        if let Err(e) = run(seed, false, TURNS, &mut stats) {
            panic!("{e}");
        }
    }
    println!(
        "{WORLDS} worlds x {TURNS} turns: the world matched the model after every op, and check() held every turn; \
         {} ops applied, {} refused ({} held despawns, {} despawns)",
        stats.applied, stats.refused, stats.held, stats.despawned
    );
    assert!(stats.held > 0 && stats.despawned > 0 && stats.refused > stats.held, "the fuzzer must reach refusals: {stats:?}");
}

#[test]
fn planted_fault_is_caught() {
    let mut stats = Stats::default();
    let caught = (1..=WORLDS).find_map(|seed| run(seed, true, TURNS, &mut stats).err());
    match caught {
        Some(report) => println!("caught the planted fault: {report}"),
        None => panic!("the model stopped cascading `Stash`, and {WORLDS} worlds didn't notice"),
    }
}

// ============================================================ hooks that never settle

pub mod tennis {
    use weft::*;

    #[derive(Debug)]
    pub struct Umpire;
    kind!(root Umpire);

    #[derive(Debug)]
    pub struct Court {
        pub side: u8,
    }
    kind!(Court);

    #[derive(Debug)]
    pub struct Ball;
    kind!(Ball needs On);

    // Every landing sends the ball over the net: a chain that never settles.
    relation!(pub struct On: Ball -> Court, one, cascade, on_relate = volley: Tennis);
    // Every echo answers twice: a chain that never settles and doubles at every step.
    relation!(pub struct Echo: Court -> Court, many, cyclic, detach, on_relate = echo: Tennis);

    module!(pub Tennis { kinds: [Court, Ball], relations: [On, Echo] });
    game!(pub Match: Umpire [Tennis]);

    fn volley<'t, G: Tennis>(t: &mut Turn<'t, G>, ball: Id<'t, Ball>, court: Id<'t, Court>) -> Result<(), Refused> {
        let other = t.all::<Court>().find(|&c| c != court);
        match other {
            Some(other) => t.relate(ball, On, other),
            None => Ok(()),
        }
    }

    fn echo<'t, G: Tennis>(t: &mut Turn<'t, G>, a: Id<'t, Court>, b: Id<'t, Court>) -> Result<(), Refused> {
        let first = t.relate(b, Echo, a);
        let second = t.relate(a, Echo, b);
        first.and(second)
    }
}

#[test]
fn hooks_settle() {
    use tennis::*;
    let started = Instant::now();
    let mut game = World::<Match>::new(|t| {
        let near = t.spawn(Court { side: 0 }, ());
        t.spawn(Court { side: 1 }, ());
        t.spawn(Ball, (On, near));
        t.spawn(Umpire, ())
    });
    let (linear, branching, calm) = game.turn(|t| {
        let near = t.all::<Court>().find(|&c| t[c].side == 0).expect("near court");
        let far = t.all::<Court>().find(|&c| t[c].side == 1).expect("far court");
        let ball = t.all::<Ball>().next().expect("ball");
        let linear = t.relate(ball, On, far);
        let branching = t.relate(near, Echo, far);
        // The turn goes on: a refused chain doesn't poison the operations after it.
        let calm = t.unrelate::<Echo>(near, far);
        (linear, branching, calm)
    });
    assert!(linear.is_err(), "a volley that never ends must be refused, got {linear:?}");
    assert!(branching.is_err(), "an echo that doubles forever must be refused, got {branching:?}");
    assert!(calm, "the echo edge was made before its hook ran, so it can be removed");
    assert_eq!(game.check(), Ok(()));
    let spent = started.elapsed();
    assert!(spent < Duration::from_secs(30), "refusing the chains took {spent:?}");
}
