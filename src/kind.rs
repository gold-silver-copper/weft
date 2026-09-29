//! Kinds: `kind!`, the nodes they are stored in, and the `one` edges each node carries.

use std::cell::Cell;
use std::fmt;
use std::rc::Rc;

use crate::chain::Chain;
use crate::handle::{Handle, Id};
use crate::relation::Relation;
use crate::schema::{Part, Schema, Shared};

/// An entity kind, declared with `kind!`.
///
/// A kind's data is fixed at spawn and read with `t[id]`; it needs no `Default`. What changes
/// lives in components and edges.
pub trait Kind: Part<Side = Shared, Store = KindList<Self>> + fmt::Debug + Sized + 'static {
    /// Where the kind stands in `ranked` relations: an edge of a ranked relation goes from a
    /// lower rank to a strictly higher one. `255` unless `kind!` says `rank`.
    const RANK: u8;
    #[doc(hidden)]
    type Role: Role<Self>;
    #[doc(hidden)]
    type Payload: 'static;
    #[doc(hidden)]
    type Links: Links;
    #[doc(hidden)]
    fn write_name(f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

/// One entity: its data and its `one` edges, as fields. Reading either can't miss.
#[doc(hidden)]
pub struct Node<K: Kind> {
    pub(crate) data: K::Payload,
    pub(crate) links: K::Links,
    pub(crate) serial: usize,
}

/// Ordinary kinds: any number per world.
#[doc(hidden)]
pub enum Plural {}

/// The root kind: exactly one per world, and its data is the game's settings.
#[doc(hidden)]
pub enum RootRole {}

#[doc(hidden)]
pub trait Role<K: Kind> {
    type Extra;
    type Out<'t>;
    fn split(data: K) -> (K::Payload, Self::Extra);
    fn out<'t>(id: Id<'t, K>, extra: Self::Extra) -> Self::Out<'t>;
    fn describe(payload: &K::Payload, root: &dyn fmt::Debug, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl<K: Kind<Payload = K>> Role<K> for Plural {
    type Extra = ();
    type Out<'t> = Id<'t, K>;

    fn split(data: K) -> (K, ()) {
        (data, ())
    }

    fn out<'t>(id: Id<'t, K>, (): ()) -> Id<'t, K> {
        id
    }

    fn describe(payload: &K, _: &dyn fmt::Debug, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{payload:?}")
    }
}

impl<K: Kind<Payload = ()>> Role<K> for RootRole {
    type Extra = K;
    type Out<'t> = Rooted<'t, K>;

    fn split(data: K) -> ((), K) {
        ((), data)
    }

    fn out<'t>(id: Id<'t, K>, data: K) -> Rooted<'t, K> {
        Rooted { id, data }
    }

    fn describe(_: &(), root: &dyn fmt::Debug, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{root:?}")
    }
}

/// What spawning the root returns in genesis: its handle and its data. `World::new`'s
/// closure returns it, and the world keeps the data as its settings.
pub struct Rooted<'t, K: Kind> {
    pub(crate) id: Id<'t, K>,
    pub(crate) data: K,
}

/// Where a kind's nodes live in a game: plural kinds in their own list, the root in the
/// game's root list.
#[doc(hidden)]
pub trait Home<K: Kind, G: Schema> {
    fn list(shared: &G::Shared) -> &KindList<K>;
}

impl<K: Kind, G: crate::schema::Has<K>> Home<K, G> for Plural {
    fn list(shared: &G::Shared) -> &KindList<K> {
        <G as crate::schema::Has<K>>::part(shared)
    }
}

impl<K: Kind, G: Schema<Root = K>> Home<K, G> for RootRole {
    fn list(shared: &G::Shared) -> &KindList<K> {
        G::root_list(shared)
    }
}

/// Every node of one kind, oldest first.
#[doc(hidden)]
pub struct KindList<K: Kind> {
    pub(crate) nodes: Chain<Rc<Node<K>>>,
    next: Cell<usize>,
}

impl<K: Kind> Default for KindList<K> {
    fn default() -> Self {
        KindList { nodes: Chain::default(), next: Cell::new(0) }
    }
}

impl<K: Kind> KindList<K> {
    pub(crate) fn push(&self, data: K::Payload, links: K::Links) -> Id<'_, K> {
        let serial = self.next.get();
        self.next.set(serial.saturating_add(1));
        Id::new(self.nodes.push(Rc::new(Node { data, links, serial })))
    }
}

/// The `one` edges a kind carries, one slot per relation in `kind!(K needs ..)`.
#[doc(hidden)]
pub trait Links: 'static {
    /// The current target of every slot.
    fn targets(&self, out: &mut Vec<usize>);
    /// Every slot, by address, with its relation's name.
    fn slots(&self, out: &mut Vec<(usize, &'static str)>);
}

macro_rules! links {
    ($($r:ident $i:tt),*) => {
        impl<$($r: Relation),*> Links for ($(Rc<OneSlot<$r>>,)*) {
            #[allow(unused_variables)]
            fn targets(&self, out: &mut Vec<usize>) {
                $(out.push(self.$i.addr.get());)*
            }
            #[allow(unused_variables)]
            fn slots(&self, out: &mut Vec<(usize, &'static str)>) {
                $(out.push((Rc::as_ptr(&self.$i).addr(), $r::NAME));)*
            }
        }
    };
}

links!();
links!(A 0);
links!(A 0, B 1);
links!(A 0, B 1, C 2);
links!(A 0, B 1, C 2, D 3);

/// `K` carries a `one` edge of `R`: `kind!(K needs R)`.
#[doc(hidden)]
pub trait Carries<R: Relation>: Kind {
    fn slot(links: &Self::Links) -> &Rc<OneSlot<R>>;
}

/// A `one` edge. The first target is set at spawn; every move appends. The current target is
/// the last one written, so reading it can't miss. `addr` mirrors the current target's
/// address for the bookkeeping that doesn't need a handle.
#[doc(hidden)]
pub struct OneSlot<R: Relation> {
    first: (Rc<R>, <R::Target as Handle>::Owned),
    later: Chain<(Rc<R>, <R::Target as Handle>::Owned)>,
    pub(crate) addr: Cell<usize>,
}

impl<R: Relation> OneSlot<R> {
    pub(crate) fn new(value: R, target: <R::Target as Handle>::Owned) -> Self {
        let addr = R::Target::erase(R::Target::borrow(&target)).addr();
        OneSlot { first: (Rc::new(value), target), later: Chain::default(), addr: Cell::new(addr) }
    }

    pub(crate) fn current(&self) -> &(Rc<R>, <R::Target as Handle>::Owned) {
        self.later.last().unwrap_or(&self.first)
    }

    pub(crate) fn push(&self, value: Rc<R>, target: <R::Target as Handle>::Owned) {
        let (_, target) = self.later.push((value, target));
        self.addr.set(R::Target::erase(R::Target::borrow(target)).addr());
    }
}
