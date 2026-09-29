//! A turn: a branded view of the world. Handles from it can't outlive it.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::ops::Index;
use std::rc::Rc;

use crate::component::Component;
use crate::graph;
use crate::handle::{Handle, Id, Is, Member, Narrow};
use crate::kind::{Home, Kind, Node, OneSlot, Plural, Role};
use crate::relation::{
    Allows, Detachable, Fallibility, Holds, Link, One, OnRelate, Policy, PolicyKind, Refused, RelLog, Relation, SpawnOf,
    Value,
};
use crate::schema::{Has, Schema};

/// How deep hooks may nest before the next one is refused.
const MAX_DEPTH: u32 = 64;
/// How many hooks one operation may run, however they nest, before the next is refused.
const BUDGET: u32 = 1 << 14;

/// Genesis builds the world: nothing can be refused, and no hooks run. Every later turn is
/// `Live`.
pub trait Phase: 'static {
    #[doc(hidden)]
    type Ctx<'t, G: Schema>;
}

/// The turn inside `World::new`.
pub enum Genesis {}

/// Every turn after genesis.
pub enum Live {}

impl Phase for Genesis {
    type Ctx<'t, G: Schema> = ();
}

impl Phase for Live {
    type Ctx<'t, G: Schema> = LiveCtx<'t, G>;
}

#[doc(hidden)]
pub struct LiveCtx<'t, G: Schema> {
    pub(crate) root: &'t Rc<Node<G::Root>>,
    pub(crate) settings: &'t mut G::Root,
}

/// A view of the world for the length of one turn.
///
/// Handles (`Id<'t, _>`) come from it and can't leave it. Reads take `&self`; anything that
/// changes the graph takes `&mut self`, so the graph can't change while you iterate it.
/// Despawns are deferred: a despawned entity is doomed, still readable, and goes when the turn
/// ends, along with whatever cascades from it.
pub struct Turn<'t, G: Schema, P: Phase = Live> {
    pub(crate) shared: &'t G::Shared,
    pub(crate) comps: &'t mut G::Comps,
    pub(crate) ctx: P::Ctx<'t, G>,
    pub(crate) doomed: Vec<usize>,
    /// The despawn plan for `doomed`, until the graph next changes.
    plan: Cell<Option<BTreeSet<usize>>>,
    depth: u32,
    fired: u32,
    brand: PhantomData<fn(&'t ()) -> &'t ()>,
}

type Src<V> = <<V as Value>::Relation as Relation>::Source;
type Tgt<V> = <<V as Value>::Relation as Relation>::Target;
type Loops<V> = <<V as Value>::Relation as Relation>::Loops;

fn log<G: Has<R>, R: Relation>(shared: &G::Shared) -> &RelLog<R> {
    <G as Has<R>>::part(shared)
}

impl<'t, G: Schema, P: Phase> Turn<'t, G, P> {
    pub(crate) fn new(shared: &'t G::Shared, comps: &'t mut G::Comps, ctx: P::Ctx<'t, G>) -> Self {
        Turn { shared, comps, ctx, doomed: Vec::new(), plan: Cell::new(None), depth: 0, fired: 0, brand: PhantomData }
    }

    /// The graph changed: the despawn plan must be worked out again.
    fn changed(&self) {
        self.plan.set(None);
    }

    fn with_plan<T>(&self, f: impl FnOnce(&BTreeSet<usize>) -> T) -> T {
        let plan = self.plan.take().unwrap_or_else(|| graph::plan(self.shared, &self.doomed));
        let out = f(&plan);
        self.plan.set(Some(plan));
        out
    }

    fn doomed_addr(&self, addr: usize) -> bool {
        !self.doomed.is_empty() && self.with_plan(|plan| plan.contains(&addr))
    }

    /// Every entity of kind `K`, oldest first, including ones doomed this turn.
    pub fn all<K: Kind>(&self) -> impl Iterator<Item = Id<'t, K>> + '_
    where
        G: Has<K>,
    {
        <G as Has<K>>::part(self.shared).nodes.iter().map(Id::new)
    }

    /// The target of a `one` edge (an `Id`, always there) or an `opt` edge (an `Option`).
    pub fn target<R: Relation<Arity: crate::relation::Single>>(
        &self,
        source: impl Holds<'t, R>,
    ) -> <R::Arity as crate::relation::Single>::Out<Id<'t, R::Target>>
    where
        G: Has<R>,
    {
        source.read(log::<G, R>(self.shared))
    }

    /// Every `R` edge from `source`, oldest first: the target and the edge's value. From a
    /// two-way relation's end, the other end with the value on this end's side.
    pub fn targets<R: Relation>(&self, source: impl Is<'t, R::Source>) -> impl Iterator<Item = (Id<'t, R::Target>, &'t R)> + '_
    where
        G: Has<R>,
    {
        R::targets_of(log::<G, R>(self.shared), source.to().addr()).into_iter()
    }

    /// Every `R` edge into `target`, oldest first: the source and the edge's value.
    pub fn sources<R: Relation>(&self, target: impl Is<'t, R::Target>) -> impl Iterator<Item = (Id<'t, R::Source>, &'t R)> + '_
    where
        G: Has<R>,
    {
        R::sources_of(log::<G, R>(self.shared), target.to().addr()).into_iter()
    }

    /// Following `R` from `source`: its target, that target's target, and so on.
    pub fn ancestors<R: Relation>(&self, source: impl Is<'t, R::Source>) -> impl Iterator<Item = Id<'t, R::Target>> + '_
    where
        G: Has<R>,
    {
        let log = log::<G, R>(self.shared);
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let mut at = source.to().addr();
        while seen.insert(at) {
            let Some(up) = log.live().find(|e| e.src_addr() == at) else { break };
            out.push(Id::new(R::Target::borrow(up.dst())));
            at = up.dst_addr();
        }
        out.into_iter()
    }

    /// Everything that reaches `target` through `R`, at any depth, nearest first.
    pub fn descendants<R: Relation>(&self, target: impl Is<'t, R::Target>) -> impl Iterator<Item = Id<'t, R::Source>> + '_
    where
        G: Has<R>,
    {
        let log = log::<G, R>(self.shared);
        let start = target.to().addr();
        let mut out = Vec::new();
        let mut seen = BTreeSet::from([start]);
        let mut level = vec![start];
        while !level.is_empty() {
            let mut next = Vec::new();
            for &at in &level {
                for e in log.live().filter(|e| e.dst_addr() == at) {
                    if seen.insert(e.src_addr()) {
                        next.push(e.src_addr());
                        out.push(Id::new(R::Source::borrow(e.src())));
                    }
                }
            }
            level = next;
        }
        out.into_iter()
    }

    /// Narrows a set handle to a kind: `Some` if this very entity is a `K`.
    pub fn cast<K: Kind>(&self, id: impl Narrow<'t, K>) -> Option<Id<'t, K>>
    where
        G: Has<K>,
    {
        #[cfg(not(weft_mutant = "cast_by_string"))]
        {
            let at = id.addr();
            self.all::<K>().find(|k| k.addr() == at)
        }
        #[cfg(weft_mutant = "cast_by_string")]
        {
            let name = |label: String| label.split('#').next().map(str::to_owned);
            let wanted = name(format!("{}", crate::handle::Label(id.erased())));
            self.all::<K>().find(|k| name(format!("{k}")) == wanted)
        }
    }

    /// The entity's `C`, if it has one.
    pub fn get<C: Component>(&self, id: Id<'t, C::Kind>) -> Option<&C>
    where
        G: Has<C>,
    {
        let at = id.addr();
        let rows = &<G as Has<C>>::part(&*self.comps).rows;
        rows.iter().find(|(node, _)| crate::handle::Erased::addr(node) == at).map(|(_, c)| c)
    }

    /// The entity's `C`, mutably, if it has one.
    pub fn get_mut<C: Component>(&mut self, id: Id<'t, C::Kind>) -> Option<&mut C>
    where
        G: Has<C>,
    {
        let at = id.addr();
        let rows = &mut <G as Has<C>>::part_mut(&mut *self.comps).rows;
        rows.iter_mut().find(|(node, _)| crate::handle::Erased::addr(node) == at).map(|(_, c)| c)
    }

    /// Gives the entity a `C`, returning the one it had.
    pub fn insert<C: Component>(&mut self, id: Id<'t, C::Kind>, component: C) -> Option<C>
    where
        G: Has<C>,
    {
        match self.get_mut::<C>(id) {
            Some(old) => Some(std::mem::replace(old, component)),
            None => {
                <G as Has<C>>::part_mut(&mut *self.comps).rows.push((Rc::clone(id.rc()), component));
                None
            }
        }
    }

    /// Takes the entity's `C`, if it has one.
    pub fn remove<C: Component>(&mut self, id: Id<'t, C::Kind>) -> Option<C>
    where
        G: Has<C>,
    {
        let at = id.addr();
        let rows = &mut <G as Has<C>>::part_mut(&mut *self.comps).rows;
        let i = rows.iter().position(|(node, _)| crate::handle::Erased::addr(node) == at)?;
        rows.swap_remove_checked(i).map(|(_, c)| c)
    }

    /// Removes the edge between `source` and `target` (either way round, if two-way).
    /// Whether there was one. `one` edges can't be removed, only moved with `relate`.
    pub fn unrelate<R: Detachable>(&mut self, source: impl Is<'t, R::Source>, target: impl Is<'t, R::Target>) -> bool
    where
        G: Has<R>,
    {
        let found = log::<G, R>(self.shared).unlink(source.to().addr(), target.to().addr());
        self.changed();
        found
    }

    fn link<X: Handle, V: Value, Y: Handle>(&self, s: Id<'t, X>, v: V, d: Id<'t, Y>)
    where
        G: Has<V::Relation>,
        V::Relation: Link<X, Y>,
    {
        V::Relation::link(log::<G, V::Relation>(self.shared), s, v.vals(), d);
        self.changed();
    }
}

/// Removing one element of a `Vec` by index, without the panicking `swap_remove`.
trait SwapRemove<T> {
    fn swap_remove_checked(&mut self, i: usize) -> Option<T>;
}

impl<T> SwapRemove<T> for Vec<T> {
    fn swap_remove_checked(&mut self, i: usize) -> Option<T> {
        let last = self.pop()?;
        match self.get_mut(i) {
            Some(slot) => Some(std::mem::replace(slot, last)),
            None => Some(last),
        }
    }
}

// ------------------------------------------------------------------ needs

/// The `one` edges a spawn is given: `()`, `(R, target)`, or a tuple of those, in the order
/// `#[kind(needs = [..])]` lists them.
pub trait Needs<'t, K: Kind, G: Schema>: Sized {
    /// Whether the spawn can be refused.
    #[doc(hidden)]
    type Err: Fallibility;
    #[doc(hidden)]
    type Ends: Copy;
    #[doc(hidden)]
    fn split(self) -> (K::Links, Self::Ends);
    #[doc(hidden)]
    fn record(node: Id<'t, K>, shared: &'t G::Shared);
}

/// The live half of `Needs`: refusing before anything changes, and running the hooks after.
#[doc(hidden)]
pub trait Fire<'t, K: Kind, G: Schema>: Needs<'t, K, G> {
    fn guard(ends: Self::Ends, t: &Turn<'t, G, Live>) -> Result<(), <Self::Err as Fallibility>::Error>;
    fn fire(ends: Self::Ends, node: Id<'t, K>, t: &mut Turn<'t, G, Live>) -> Result<(), <Self::Err as Fallibility>::Error>;
}

impl<'t, K: Kind<Links = ()>, G: Schema> Needs<'t, K, G> for () {
    type Err = crate::relation::Sure;
    type Ends = ();

    fn split(self) -> ((), ()) {
        ((), ())
    }

    fn record(_: Id<'t, K>, _: &'t G::Shared) {}
}

impl<'t, K: Kind<Links = ()>, G: Schema> Fire<'t, K, G> for () {
    fn guard((): (), _: &Turn<'t, G, Live>) -> Result<(), std::convert::Infallible> {
        Ok(())
    }

    fn fire((): (), _: Id<'t, K>, _: &mut Turn<'t, G, Live>) -> Result<(), std::convert::Infallible> {
        Ok(())
    }
}

impl<'t, K, R, Y, G> Needs<'t, K, G> for (R, Id<'t, Y>)
where
    G: Has<R>,
    R: Relation<Vals = R, Arity = One>,
    K: Kind<Links = (Rc<OneSlot<R>>,)> + Member<R::Source>,
    Y: Member<R::Target>,
    R::Loops: Allows<K, Y>,
{
    type Err = SpawnOf<R>;
    type Ends = (Id<'t, Y>,);

    fn split(self) -> ((Rc<OneSlot<R>>,), (Id<'t, Y>,)) {
        let (value, to) = self;
        ((Rc::new(OneSlot::new(value, Y::own(to.r))),), (to,))
    }

    fn record(node: Id<'t, K>, shared: &'t G::Shared) {
        log::<G, R>(shared).push_one(<K as Member<R::Source>>::own(node.r), Rc::clone(&node.node().links.0));
    }
}

impl<'t, K, R, Y, G> Fire<'t, K, G> for (R, Id<'t, Y>)
where
    G: Has<R>,
    R: Relation<Vals = R, Arity = One> + OnRelate<G>,
    K: Kind<Links = (Rc<OneSlot<R>>,)> + Member<R::Source>,
    Y: Member<R::Target>,
    R::Loops: Allows<K, Y>,
{
    fn guard((to,): (Id<'t, Y>,), t: &Turn<'t, G, Live>) -> Result<(), <SpawnOf<R> as Fallibility>::Error> {
        t.spawn_guard::<R>(to.addr())
    }

    fn fire((to,): (Id<'t, Y>,), node: Id<'t, K>, t: &mut Turn<'t, G, Live>) -> Result<(), <SpawnOf<R> as Fallibility>::Error> {
        t.spawn_hook::<R>(node.up(), to.up())
    }
}

/// The spawn's fallibility for needs `R1, R2, ..`: refusable if any is.
macro_rules! err {
    ($r:ident) => { SpawnOf<$r> };
    ($r:ident, $($rest:ident),+) => { <SpawnOf<$r> as Fallibility>::Or<err!($($rest),+)> };
}

/// Folds the per-need results into the spawn's.
macro_rules! combine {
    ($g:expr, $r:ident) => { $g };
    ($g:expr, $r:ident, $($gs:expr, $rs:ident),+) => {
        <SpawnOf<$r> as Fallibility>::left::<err!($($rs),+)>($g)
            .and(<SpawnOf<$r> as Fallibility>::right::<err!($($rs),+)>(combine!($($gs, $rs),+)))
    };
}

macro_rules! needs {
    ($($r:ident $y:ident $i:tt),+) => {
        impl<'t, K, G, $($r, $y),+> Needs<'t, K, G> for ($(($r, Id<'t, $y>),)+)
        where
            G: Schema $(+ Has<$r>)+,
            K: Kind<Links = ($(Rc<OneSlot<$r>>,)+)> $(+ Member<<$r as Relation>::Source>)+,
            $(
                $r: Relation<Vals = $r, Arity = One>,
                $y: Member<<$r as Relation>::Target>,
                <$r as Relation>::Loops: Allows<K, $y>,
            )+
        {
            type Err = err!($($r),+);
            type Ends = ($(Id<'t, $y>,)+);

            fn split(self) -> (K::Links, Self::Ends) {
                (($(Rc::new(OneSlot::new(self.$i.0, $y::own(self.$i.1.r))),)+), ($(self.$i.1,)+))
            }

            fn record(node: Id<'t, K>, shared: &'t G::Shared) {
                let links = &node.node().links;
                $(log::<G, $r>(shared).push_one(<K as Member<<$r as Relation>::Source>>::own(node.r), Rc::clone(&links.$i));)+
            }
        }

        impl<'t, K, G, $($r, $y),+> Fire<'t, K, G> for ($(($r, Id<'t, $y>),)+)
        where
            G: Schema $(+ Has<$r>)+,
            K: Kind<Links = ($(Rc<OneSlot<$r>>,)+)> $(+ Member<<$r as Relation>::Source>)+,
            $(
                $r: Relation<Vals = $r, Arity = One> + OnRelate<G>,
                $y: Member<<$r as Relation>::Target>,
                <$r as Relation>::Loops: Allows<K, $y>,
            )+
        {
            fn guard(ends: Self::Ends, t: &Turn<'t, G, Live>) -> Result<(), <Self::Err as Fallibility>::Error> {
                combine!($(t.spawn_guard::<$r>(ends.$i.addr()), $r),+)
            }

            fn fire(ends: Self::Ends, node: Id<'t, K>, t: &mut Turn<'t, G, Live>) -> Result<(), <Self::Err as Fallibility>::Error> {
                combine!($(t.spawn_hook::<$r>(node.up(), ends.$i.up()), $r),+)
            }
        }
    };
}

needs!(A YA 0, B YB 1);
needs!(A YA 0, B YB 1, C YC 2);
needs!(A YA 0, B YB 1, C YC 2, D YD 3);
needs!(A YA 0, B YB 1, C YC 2, D YD 3, E YE 4);
needs!(A YA 0, B YB 1, C YC 2, D YD 3, E YE 4, F YF 5);
needs!(A YA 0, B YB 1, C YC 2, D YD 3, E YE 4, F YF 5, H YH 6);
needs!(A YA 0, B YB 1, C YC 2, D YD 3, E YE 4, F YF 5, H YH 6, I YI 7);

// ------------------------------------------------------------------ genesis

impl<'t, G: Schema> Turn<'t, G, Genesis> {
    /// Spawns an entity with the `one` edges its kind needs. In genesis nothing can refuse,
    /// so this is the handle, or, for the root, the handle and the settings, which the
    /// closure given to `World::new` returns.
    pub fn spawn<K: Kind, A: Needs<'t, K, G>>(&mut self, data: K, needs: A) -> <K::Role as Role<K>>::Out<'t>
    where
        K::Role: Home<K, G>,
    {
        let (payload, extra) = K::Role::split(data);
        let (links, _) = needs.split();
        let id = <K::Role as Home<K, G>>::list(self.shared).push(payload, links);
        A::record(id, self.shared);
        K::Role::out(id, extra)
    }

    /// Makes an edge. For `one` and `opt`, moves it. Genesis runs no hooks.
    pub fn relate<X, V, Y>(&mut self, source: Id<'t, X>, value: V, target: Id<'t, Y>)
    where
        G: Has<V::Relation>,
        V: Value,
        V::Relation: Link<X, Y>,
        Loops<V>: Allows<X, Y>,
        X: Member<Src<V>>,
        Y: Member<Tgt<V>>,
    {
        self.link(source, value, target);
    }
}

// ------------------------------------------------------------------ live turns

impl<'t, G: Schema> Turn<'t, G, Live> {
    /// A new top-level operation gets the whole hook budget.
    fn enter(&mut self) {
        if self.depth == 0 {
            self.fired = 0;
        }
    }

    fn hook_room(&self) -> bool {
        self.depth < MAX_DEPTH && self.fired < BUDGET
    }

    fn run_hook<R: OnRelate<G>>(&mut self, s: Id<'t, R::Source>, d: Id<'t, R::Target>) -> Result<(), <R::Hook as Fallibility>::Error> {
        self.depth = self.depth.saturating_add(1);
        self.fired = self.fired.saturating_add(1);
        let result = R::fire(self, s, d);
        self.depth = self.depth.saturating_sub(1);
        result
    }

    fn spawn_guard<R: Relation>(&self, target: usize) -> Result<(), <SpawnOf<R> as Fallibility>::Error> {
        <SpawnOf<R> as Fallibility>::guard(|| {
            if R::Policy::POLICY == Policy::Refuse && self.doomed_addr(target) {
                Some(Refused::Doomed)
            } else if <R::Hook as Fallibility>::CAN_FAIL && !self.hook_room() {
                Some(Refused::Unsettled)
            } else {
                None
            }
        })
    }

    fn spawn_hook<R: OnRelate<G>>(&mut self, s: Id<'t, R::Source>, d: Id<'t, R::Target>) -> Result<(), <SpawnOf<R> as Fallibility>::Error> {
        let fired = self.run_hook::<R>(s, d);
        <<R::Policy as PolicyKind>::Holds as Fallibility>::right::<R::Hook>(fired)
    }

    /// Spawns an entity with the `one` edges its kind needs, then runs their hooks.
    ///
    /// Returns the handle, or `Result<Id, Refused>` if one of those relations has a hook or
    /// refuses to let its target go: a `refuse` edge can't point at a doomed entity. A
    /// refused spawn creates nothing; a hook that refuses leaves the entity in place.
    pub fn spawn<K, A>(&mut self, data: K, needs: A) -> <A::Err as Fallibility>::Wrap<Id<'t, K>>
    where
        K: Kind<Role = Plural, Payload = K>,
        A: Fire<'t, K, G>,
        G: Has<K>,
    {
        self.enter();
        let (links, ends) = needs.split();
        if let Err(refused) = A::guard(ends, self) {
            return A::Err::wrap(Err(refused));
        }
        let id = <G as Has<K>>::part(self.shared).push(data, links);
        A::record(id, self.shared);
        self.changed();
        let fired = A::fire(ends, id, self);
        A::Err::wrap(fired.map(|()| id))
    }

    /// Makes an edge, or moves it for `one` and `opt`, then runs the relation's hook.
    ///
    /// Refused, with nothing changed, if the target is doomed or the hooks running already
    /// went too deep. If the hook refuses, the edge and the hook's earlier steps stay.
    pub fn relate<X, V, Y>(&mut self, source: Id<'t, X>, value: V, target: Id<'t, Y>) -> Result<(), Refused>
    where
        G: Has<V::Relation>,
        V: Value,
        V::Relation: Link<X, Y> + OnRelate<G>,
        Loops<V>: Allows<X, Y>,
        X: Member<Src<V>>,
        Y: Member<Tgt<V>>,
    {
        self.enter();
        if self.doomed_addr(target.addr()) {
            return Err(Refused::Doomed);
        }
        let hooked = <<V::Relation as Relation>::Hook as Fallibility>::CAN_FAIL;
        if hooked && !self.hook_room() {
            return Err(Refused::Unsettled);
        }
        self.link(source, value, target);
        if !hooked {
            return Ok(());
        }
        self.run_hook::<V::Relation>(source.up(), target.up()).map_err(Into::into)
    }

    /// The root: the entity whose data is the settings.
    pub fn root(&self) -> Id<'t, G::Root> {
        Id::new(self.ctx.root)
    }

    /// The game's settings: the root's data.
    pub fn settings(&self) -> &G::Root {
        self.ctx.settings
    }

    /// The game's settings, mutably.
    pub fn settings_mut(&mut self) -> &mut G::Root {
        self.ctx.settings
    }

    /// Dooms an entity. It stays readable and goes at the end of the turn, with everything
    /// that cascades from it; what spills out of it moves up to whatever held it.
    ///
    /// Refused, with nothing changed, if anything outside that despawn still holds one of
    /// the entities it takes, through a `refuse` relation.
    pub fn despawn<K: Kind<Role = Plural>>(&mut self, id: Id<'t, K>) -> Result<(), Refused>
    where
        G: Has<K>,
    {
        let mut explicit = self.doomed.clone();
        explicit.push(id.addr());
        let plan = graph::plan(self.shared, &explicit);
        if graph::held(self.shared, &plan) {
            return Err(Refused::Held);
        }
        self.doomed = explicit;
        self.plan.set(Some(plan));
        Ok(())
    }

    /// Whether the entity goes at the end of this turn.
    pub fn is_doomed<X: Handle>(&self, id: Id<'t, X>) -> bool {
        self.doomed_addr(id.addr())
    }
}

/// Reading an entity's data: a field read that can't miss. Set handles can't be indexed:
/// `cast` one to its kind first. There is no `IndexMut`; what changes lives in components.
impl<'t, G, K, P> Index<Id<'t, K>> for Turn<'t, G, P>
where
    G: Has<K>,
    K: Kind<Payload = K>,
    P: Phase,
{
    type Output = K;

    fn index(&self, id: Id<'t, K>) -> &K {
        &id.node().data
    }
}
