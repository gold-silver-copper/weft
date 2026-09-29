//! Operations over a game's whole storage: the despawn plan, the end of a turn, the audit
//! and the snapshot.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::rc::Rc;

use crate::handle::Erased;
use crate::kind::{Kind, KindList, Links, Node, Role};

/// A piece of graph storage: a kind's list or a relation's log.
#[doc(hidden)]
pub trait Piece {
    /// Every node's address.
    fn nodes(&self, _: &mut Vec<usize>) {}
    /// Sources that go along with `set`: `cascade` edges into it, and `spill` edges into it
    /// with nowhere to land.
    fn grow(&self, _: &BTreeSet<usize>, _: &mut Vec<usize>) {}
    /// A `refuse` edge from outside `set` points into it.
    fn held(&self, _: &BTreeSet<usize>) -> bool {
        false
    }
    /// Moves `spill` edges out of `set`.
    fn spill(&self, _: &BTreeSet<usize>) {}
    /// Drops everything in `set`, and every edge that touches it.
    fn sweep(&mut self, set: &BTreeSet<usize>);
    /// One line per node, each followed by the node's edges.
    fn fmt_nodes(&self, _: &mut fmt::Formatter<'_>, _: &dyn fmt::Debug, _: &dyn Fn(usize, &mut fmt::Formatter<'_>) -> fmt::Result) -> fmt::Result {
        Ok(())
    }
    /// The edges that start at `at`.
    fn fmt_edges(&self, _: usize, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
    fn audit(&self, _: &mut Audit) {}
}

/// A game's whole graph storage, as a list of pieces.
#[doc(hidden)]
pub trait Graph {
    fn each(&self, f: &mut dyn FnMut(&dyn Piece));
    fn each_mut(&mut self, f: &mut dyn FnMut(&mut dyn Piece));
}

impl Graph for () {
    fn each(&self, _: &mut dyn FnMut(&dyn Piece)) {}
    fn each_mut(&mut self, _: &mut dyn FnMut(&mut dyn Piece)) {}
}

impl<H: Piece, T: Graph> Graph for (H, T) {
    fn each(&self, f: &mut dyn FnMut(&dyn Piece)) {
        f(&self.0);
        self.1.each(f);
    }

    fn each_mut(&mut self, f: &mut dyn FnMut(&mut dyn Piece)) {
        f(&mut self.0);
        self.1.each_mut(f);
    }
}

/// Everything that goes at the end of the turn: `explicit`, what cascades from it, and what
/// would spill out of it with nowhere to land.
pub(crate) fn plan(graph: &impl Graph, explicit: &[usize]) -> BTreeSet<usize> {
    let mut set: BTreeSet<usize> = explicit.iter().copied().collect();
    loop {
        let mut more = Vec::new();
        graph.each(&mut |p| p.grow(&set, &mut more));
        let before = set.len();
        set.extend(more);
        if set.len() == before {
            return set;
        }
    }
}

/// Whether a `refuse` edge from outside `set` points into it.
pub(crate) fn held(graph: &impl Graph, set: &BTreeSet<usize>) -> bool {
    let mut held = false;
    graph.each(&mut |p| held = held || p.held(set));
    held
}

// ------------------------------------------------------------------ kind lists

impl<K: Kind> Piece for KindList<K> {
    fn nodes(&self, out: &mut Vec<usize>) {
        out.extend(self.nodes.iter().map(Erased::addr));
    }

    fn sweep(&mut self, set: &BTreeSet<usize>) {
        self.nodes.retain(|n| !set.contains(&n.addr()));
    }

    fn fmt_nodes(
        &self,
        f: &mut fmt::Formatter<'_>,
        root: &dyn fmt::Debug,
        edges: &dyn Fn(usize, &mut fmt::Formatter<'_>) -> fmt::Result,
    ) -> fmt::Result {
        for node in self.nodes.iter() {
            node.label(f)?;
            f.write_str(" ")?;
            K::Role::describe(&node.data, root, f)?;
            writeln!(f)?;
            edges(node.addr(), f)?;
        }
        Ok(())
    }

    fn audit(&self, audit: &mut Audit) {
        for node in self.nodes.iter() {
            let mut targets = Vec::new();
            node.links.targets(&mut targets);
            if targets.iter().any(|t| !audit.nodes.contains(t)) {
                audit.broken.push(Broken::Dangling { relation: "a `one` relation" });
            }
            let mut slots = Vec::new();
            node.links.slots(&mut slots);
            audit.node_slots.extend(slots.into_iter().map(|(slot, relation)| (slot, node.addr(), relation)));
        }
    }
}

// ------------------------------------------------------------------ components

/// A component's values, one per entity that has one.
#[doc(hidden)]
pub struct Table<C: crate::component::Component> {
    pub(crate) rows: Vec<(Rc<Node<C::Kind>>, C)>,
}

impl<C: crate::component::Component> Default for Table<C> {
    fn default() -> Self {
        Table { rows: Vec::new() }
    }
}

/// A game's whole component storage.
#[doc(hidden)]
pub trait Comps {
    fn sweep(&mut self, set: &BTreeSet<usize>);
}

impl Comps for () {
    fn sweep(&mut self, _: &BTreeSet<usize>) {}
}

impl<C: crate::component::Component, T: Comps> Comps for (Table<C>, T) {
    fn sweep(&mut self, set: &BTreeSet<usize>) {
        self.0.rows.retain(|(node, _)| !set.contains(&node.addr()));
        self.1.sweep(set);
    }
}

// ------------------------------------------------------------------ the audit

/// What `World::check` collects on its way through the storage.
#[doc(hidden)]
#[derive(Default)]
pub struct Audit {
    pub(crate) nodes: BTreeSet<usize>,
    /// `(slot, node, relation)` for every `one` slot a node carries.
    pub(crate) node_slots: Vec<(usize, usize, &'static str)>,
    /// `(slot, source, relation)` for every live `one` edge a relation's log lists.
    pub(crate) logged_slots: Vec<(usize, usize, &'static str)>,
    pub(crate) broken: Vec<Broken>,
}

impl Audit {
    /// Every slot a node carries is listed exactly once by its relation's log, under that
    /// node, and the log lists no other slot.
    pub(crate) fn match_slots(&mut self) {
        let mut listed: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        for &(slot, src, _) in &self.logged_slots {
            let seen = listed.entry((slot, src)).or_insert(0);
            *seen = seen.saturating_add(1);
        }
        for &(slot, node, relation) in &self.node_slots {
            if listed.remove(&(slot, node)) != Some(1) {
                self.broken.push(Broken::Unlinked { relation });
            }
        }
        for &(slot, src, relation) in &self.logged_slots {
            if listed.contains_key(&(slot, src)) {
                self.broken.push(Broken::Unlinked { relation });
            }
        }
    }
}

/// An invariant `World::check` found broken. Seeing one is a bug in weft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Broken {
    /// An edge of this relation points at, or from, an entity that isn't in the world.
    Dangling {
        /// The relation.
        relation: &'static str,
    },
    /// A `one` edge of this relation isn't carried by exactly one entity, or is carried by an
    /// entity it isn't listed under.
    Unlinked {
        /// The relation.
        relation: &'static str,
    },
    /// This relation has two edges where its arity allows one.
    Doubled {
        /// The relation.
        relation: &'static str,
    },
    /// The edges of this relation, which can't loop, form a loop.
    Cycle {
        /// The relation.
        relation: &'static str,
    },
}

impl fmt::Display for Broken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Broken::Dangling { relation } => write!(f, "an edge of `{relation}` points at an entity that isn't in the world"),
            Broken::Unlinked { relation } => write!(f, "a `one` edge of `{relation}` isn't carried by exactly the entity it's listed under"),
            Broken::Doubled { relation } => write!(f, "`{relation}` has two edges where its arity allows one"),
            Broken::Cycle { relation } => write!(f, "the edges of `{relation}` form a loop, which it doesn't allow"),
        }
    }
}

impl std::error::Error for Broken {}

/// Whether the directed graph with these edges has a cycle.
pub(crate) fn has_cycle(edges: &[(usize, usize)]) -> bool {
    let mut incoming: BTreeMap<usize, usize> = BTreeMap::new();
    let mut out: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &(s, d) in edges {
        incoming.entry(s).or_insert(0);
        let n = incoming.entry(d).or_insert(0);
        *n = n.saturating_add(1);
        out.entry(s).or_default().push(d);
    }
    let mut ready: Vec<usize> = incoming.iter().filter(|(_, n)| **n == 0).map(|(v, _)| *v).collect();
    let mut removed = 0_usize;
    while let Some(v) = ready.pop() {
        removed = removed.saturating_add(1);
        for d in out.get(&v).map_or(&[][..], Vec::as_slice) {
            if let Some(n) = incoming.get_mut(d) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    ready.push(*d);
                }
            }
        }
    }
    removed != incoming.len()
}
