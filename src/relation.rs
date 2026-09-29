use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::vec::Vec;

use crate::{Entity, Schema, World};

/// How many targets a source may have through a relation: [`One`] or [`Many`].
pub trait Arity: sealed::Sealed + 'static {
    #[doc(hidden)]
    const EXCLUSIVE: bool;
}

/// Each source has at most one target. Relating to a new target replaces the
/// old edge, and [`World::target`] becomes available.
pub enum One {}

/// Sources can have any number of targets.
pub enum Many {}

impl Arity for One {
    const EXCLUSIVE: bool = true;
}

impl Arity for Many {
    const EXCLUSIVE: bool = false;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::One {}
    impl Sealed for super::Many {}
}

/// A typed edge `source --R--> target` carrying a value of `R`.
///
/// ```
/// # use weft::*;
/// /// Many-to-many, with a value on each edge.
/// struct Likes(u32);
/// impl<S: Schema> Relation<S> for Likes {
///     type Arity = Many;
/// }
///
/// /// Undirected and exclusive: one partner, and it goes both ways.
/// struct MarriedTo;
/// impl<S: Schema> Relation<S> for MarriedTo {
///     type Arity = One;
///     const SYMMETRIC: bool = true;
/// }
/// ```
pub trait Relation<S: Schema>: Sized + 'static {
    /// [`One`] or [`Many`] targets per source. There is no default: pick one.
    type Arity: Arity;

    /// Edges are undirected and share one value.
    const SYMMETRIC: bool = false;

    /// Despawning a target despawns every source pointing at it, recursively.
    const CASCADE: bool = false;

    /// Runs after an edge is created or its value replaced.
    fn on_relate(world: &mut World<S>, source: Entity, target: Entity) {
        let _ = (world, source, target);
    }

    /// Runs before an edge is removed (including on despawn), while its value
    /// can still be read.
    fn on_unrelate(world: &mut World<S>, source: Entity, target: Entity) {
        let _ = (world, source, target);
    }
}

/// A relation value and its target, for use in a bundle:
/// `world.spawn((Name("leaf"), To(ChildOf, branch)))`.
pub struct To<R>(pub R, pub Entity);

/// The built-in hierarchy: `child --ChildOf--> parent`. Exclusive and cascading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChildOf;

impl<S: Schema> Relation<S> for ChildOf {
    type Arity = One;
    const CASCADE: bool = true;
}

/// All edges of one relation type, with adjacency lists in both directions.
/// Lists keep insertion order and maps are ordered, so iteration is deterministic.
pub struct Edges<R> {
    values: BTreeMap<(Entity, Entity), R>,
    targets: BTreeMap<Entity, Vec<Entity>>,
    sources: BTreeMap<Entity, Vec<Entity>>,
}

impl<R> Default for Edges<R> {
    fn default() -> Self {
        Self { values: BTreeMap::new(), targets: BTreeMap::new(), sources: BTreeMap::new() }
    }
}

impl<R> Edges<R> {
    fn key(symmetric: bool, source: Entity, target: Entity) -> (Entity, Entity) {
        if symmetric && target < source { (target, source) } else { (source, target) }
    }

    pub(crate) fn targets(&self, source: Entity) -> &[Entity] {
        self.targets.get(&source).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn sources(&self, target: Entity) -> &[Entity] {
        self.sources.get(&target).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn get(&self, symmetric: bool, source: Entity, target: Entity) -> Option<&R> {
        self.values.get(&Self::key(symmetric, source, target))
    }

    pub(crate) fn get_mut(&mut self, symmetric: bool, source: Entity, target: Entity) -> Option<&mut R> {
        self.values.get_mut(&Self::key(symmetric, source, target))
    }

    pub(crate) fn insert(&mut self, symmetric: bool, source: Entity, target: Entity, value: R) {
        let key = Self::key(symmetric, source, target);
        if let Some(slot) = self.values.get_mut(&key) {
            *slot = value;
            return;
        }
        self.values.insert(key, value);
        self.link(source, target);
        if symmetric && source != target {
            self.link(target, source);
        }
    }

    pub(crate) fn remove(&mut self, symmetric: bool, source: Entity, target: Entity) -> Option<R> {
        let value = self.values.remove(&Self::key(symmetric, source, target))?;
        self.unlink(source, target);
        if symmetric && source != target {
            self.unlink(target, source);
        }
        Some(value)
    }

    fn link(&mut self, source: Entity, target: Entity) {
        self.targets.entry(source).or_default().push(target);
        self.sources.entry(target).or_default().push(source);
    }

    fn unlink(&mut self, source: Entity, target: Entity) {
        for (map, key, value) in [(&mut self.targets, source, target), (&mut self.sources, target, source)] {
            if let Some(list) = map.get_mut(&key) {
                list.retain(|&e| e != value);
                if list.is_empty() {
                    map.remove(&key);
                }
            }
        }
    }

    /// Every edge touching `entity`, in either direction, each listed once.
    pub(crate) fn edges_of(&self, symmetric: bool, entity: Entity) -> Vec<(Entity, Entity)> {
        let outgoing = self.targets(entity).iter().map(|&t| (entity, t));
        if symmetric {
            return outgoing.collect(); // incoming mirrors outgoing
        }
        outgoing.chain(self.sources(entity).iter().map(|&s| (s, entity))).collect()
    }

    pub(crate) fn discard(&mut self, symmetric: bool, entity: Entity) {
        for (source, target) in self.edges_of(symmetric, entity) {
            self.remove(symmetric, source, target);
        }
    }
}

/// Breadth-first walk along a relation. Visits each reachable entity once,
/// so cycles are fine. The starting entity is not yielded.
pub struct Traverse<'w, R> {
    edges: &'w Edges<R>,
    queue: VecDeque<Entity>,
    seen: BTreeSet<Entity>,
    upward: bool,
}

impl<'w, R> Traverse<'w, R> {
    pub(crate) fn new(edges: &'w Edges<R>, start: Entity, upward: bool) -> Self {
        let mut walk = Self { edges, queue: VecDeque::new(), seen: BTreeSet::from([start]), upward };
        walk.enqueue_neighbors(start);
        walk
    }

    fn enqueue_neighbors(&mut self, entity: Entity) {
        let next = if self.upward { self.edges.targets(entity) } else { self.edges.sources(entity) };
        for &e in next {
            if self.seen.insert(e) {
                self.queue.push_back(e);
            }
        }
    }
}

impl<R> Iterator for Traverse<'_, R> {
    type Item = Entity;

    fn next(&mut self) -> Option<Entity> {
        let entity = self.queue.pop_front()?;
        self.enqueue_neighbors(entity);
        Some(entity)
    }
}
