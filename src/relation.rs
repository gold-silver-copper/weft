//! Relations: `relation!`, and how their edges are stored.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::convert::Infallible;
use std::fmt;
use std::rc::Rc;

use crate::chain::Chain;
use crate::graph::{Audit, Broken, Piece};
use crate::handle::{Handle, Id, Member};
use crate::kind::{Carries, OneSlot};
use crate::schema::{Part, Schema, Shared};
use crate::turn::{Live, Turn};

// ------------------------------------------------------------------ arity

/// Exactly one target per source. The edge is required at spawn (`kind!(K needs R)`),
/// relating again moves it, and it can't be unrelated.
pub enum One {}
/// At most one target per source. Relating again replaces the edge.
pub enum Opt {}
/// Any number of targets per source.
pub enum Many {}

/// `One`, `Opt` or `Many`.
pub trait Arity: 'static {
    #[doc(hidden)]
    const ONE: bool;
    /// At most one edge per source.
    #[doc(hidden)]
    const SINGLE: bool;
}

impl Arity for One {
    const ONE: bool = true;
    const SINGLE: bool = true;
}

impl Arity for Opt {
    const ONE: bool = false;
    const SINGLE: bool = true;
}

impl Arity for Many {
    const ONE: bool = false;
    const SINGLE: bool = false;
}

/// Arities with at most one target, where `target` makes sense.
pub trait Single: Arity {
    /// `T` for `One`, `Option<T>` for `Opt`.
    type Out<T>;
}

impl Single for One {
    type Out<T> = T;
}

impl Single for Opt {
    type Out<T> = Option<T>;
}

// ------------------------------------------------------------------ loop rules

/// No loop rule: the relation's source and target sets don't overlap, so it can't loop.
/// `game!` checks that they don't.
pub enum NoLoops {}
/// `ranked`: every edge goes from a lower-ranked kind to a higher-ranked one, so it can't
/// loop. Checked by the type of every `relate` and `spawn`.
pub enum Ranked {}
/// `cyclic`: loops are allowed.
pub enum Cyclic {}

/// A loop rule.
pub trait LoopRule: 'static {
    #[doc(hidden)]
    const RANKED: bool;
    /// Whether edges of the relation can never form a loop. `World::check` verifies it.
    #[doc(hidden)]
    const ACYCLIC: bool;
}

impl LoopRule for NoLoops {
    const RANKED: bool = false;
    const ACYCLIC: bool = true;
}

impl LoopRule for Ranked {
    const RANKED: bool = true;
    const ACYCLIC: bool = true;
}

impl LoopRule for Cyclic {
    const RANKED: bool = false;
    const ACYCLIC: bool = false;
}

/// The loop rule allows an edge from an `X` to a `Y`.
#[diagnostic::on_unimplemented(message = "`{X}` doesn't fit in `{Y}`: the relation is ranked, and ranks must rise")]
pub trait Allows<X, Y> {}

impl<X, Y> Allows<X, Y> for NoLoops {}

impl<X, Y> Allows<X, Y> for Cyclic {}

impl<X: crate::rank::Fits<Y>, Y> Allows<X, Y> for Ranked {}

// ------------------------------------------------------------------ despawn policies

/// What despawning a target does to the edges into it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[doc(hidden)]
pub enum Policy {
    Cascade,
    Detach,
    Spill,
    Refuse,
}

/// `cascade`, `detach`, `spill` or `refuse`.
pub trait PolicyKind: 'static {
    #[doc(hidden)]
    const POLICY: Policy;
    /// `Refused` for `refuse`, whose target may already be doomed; `Sure` otherwise.
    #[doc(hidden)]
    type Holds: Fallibility;
}

/// `cascade`: the source goes with its target.
pub enum Cascade {}
/// `detach`: the edge goes, the source stays. Not for `one` relations.
pub enum Detach {}
/// `spill`: the source moves up to whatever held its target, through the same relation;
/// with nowhere to go, it goes with its target. Only for `one` relations.
pub enum Spill {}
/// `refuse`: the target can't be despawned while the edge exists.
pub enum Refuse {}

impl PolicyKind for Cascade {
    const POLICY: Policy = Policy::Cascade;
    type Holds = Sure;
}

impl PolicyKind for Detach {
    const POLICY: Policy = Policy::Detach;
    type Holds = Sure;
}

impl PolicyKind for Spill {
    const POLICY: Policy = Policy::Spill;
    type Holds = Sure;
}

impl PolicyKind for Refuse {
    const POLICY: Policy = Policy::Refuse;
    type Holds = Refused;
}

// ------------------------------------------------------------------ relations

/// A relation, declared with `relation!`: typed endpoints, an arity, a loop rule, a despawn
/// policy, and optionally a hook.
pub trait Relation: Part<Side = Shared, Store = RelLog<Self>> + fmt::Debug + Sized + 'static {
    /// The kind or set edges come from.
    type Source: Handle;
    /// The kind or set edges go to.
    type Target: Handle;
    /// `One`, `Opt` or `Many`.
    type Arity: Arity;
    /// `NoLoops`, `Ranked` or `Cyclic`.
    type Loops: LoopRule;
    /// What despawning a target does to its edges.
    type Policy: PolicyKind;
    #[doc(hidden)]
    type Vals: 'static;
    /// `Refused` if the relation has a hook, which may refuse; `Sure` if it has none.
    #[doc(hidden)]
    type Hook: Fallibility;
    #[doc(hidden)]
    const NAME: &'static str;
    #[doc(hidden)]
    const TWO_WAY: bool;
    #[doc(hidden)]
    fn fwd(vals: &Self::Vals) -> &Self;
    #[doc(hidden)]
    fn back(vals: &Self::Vals) -> &Self;
    #[doc(hidden)]
    fn targets_of<'t>(log: &'t RelLog<Self>, at: usize) -> Vec<(Id<'t, Self::Target>, &'t Self)>;
    #[doc(hidden)]
    fn sources_of<'t>(log: &'t RelLog<Self>, at: usize) -> Vec<(Id<'t, Self::Source>, &'t Self)>;
}

/// What `relate` takes: the relation's value, or for a two-way relation, one value per
/// direction.
#[diagnostic::on_unimplemented(
    message = "`relate` doesn't take `{Self}` here",
    label = "not a relation value `relate` takes",
    note = "a two-way (`<->`) relation takes a pair, one value per direction: `(Exit(\"down\"), Exit(\"up\"))`"
)]
pub trait Value {
    /// The relation the value belongs to.
    type Relation: Relation;
    #[doc(hidden)]
    fn vals(self) -> <Self::Relation as Relation>::Vals;
}

/// The rules `relation!` checks as soon as a relation is declared: what's wrong with its
/// declaration, if anything. `relation!` turns the answer into a `cargo check` error.
#[doc(hidden)]
pub const fn check_relation<R: Relation>() -> Option<&'static str> {
    let one = <R::Arity as Arity>::ONE;
    let many = !<R::Arity as Arity>::SINGLE;
    match <R::Policy as PolicyKind>::POLICY {
        Policy::Detach if one => {
            Some("`detach` would leave a `one` edge's source with no edge: use `cascade`, `spill` or `refuse`")
        }
        Policy::Spill if !one => Some("`spill` moves the source's `one` edge up a level, so it needs a `one` relation"),
        _ if R::TWO_WAY && !many => Some("a two-way (`<->`) relation must be `many`"),
        _ => None,
    }
}

/// A two-way (`<->`) relation: one fact, with a value for each direction.
pub trait TwoWay: Relation<Vals = (Self, Self)> {}

impl<R: TwoWay> Value for (R, R) {
    type Relation = R;

    fn vals(self) -> (R, R) {
        self
    }
}

/// `opt` and `many` relations can be unrelated. A `one` edge is moved, never dropped.
#[diagnostic::on_unimplemented(message = "`{Self}` is `one`: move its edge with `relate`; it can't be dropped")]
pub trait Detachable: Relation {}

/// Making an edge from an `X` to a `Y`.
#[doc(hidden)]
pub trait Link<X: Handle, Y: Handle>: Relation {
    fn link<'t>(log: &'t RelLog<Self>, s: Id<'t, X>, vals: Self::Vals, d: Id<'t, Y>);
}

/// A handle whose `R` target can be read with `target`: a kind that carries a `one` edge, or
/// any source of an `opt` one.
#[diagnostic::on_unimplemented(message = "`target::<{R}>` needs a handle to a kind that has the edge; `cast` a set handle to its kind first")]
pub trait Holds<'t, R: Relation<Arity: Single>> {
    #[doc(hidden)]
    fn read(self, log: &'t RelLog<R>) -> <R::Arity as Single>::Out<Id<'t, R::Target>>;
}

/// The hook a relation runs when a live turn makes one of its edges, and the bound a
/// caller must meet to make one. `relation!` implements it; with `on_relate = f: Bound`,
/// only for games that meet `Bound`.
pub trait OnRelate<G: Schema>: Relation {
    #[doc(hidden)]
    fn fire<'t>(
        t: &mut Turn<'t, G, Live>,
        s: Id<'t, Self::Source>,
        d: Id<'t, Self::Target>,
    ) -> Result<(), <Self::Hook as Fallibility>::Error>;
}

// ------------------------------------------------------------------ refusals

/// Whether an operation can be refused at runtime, as a type: `Sure` if it can't, `Refused`
/// if it can. A `Sure` operation runs no runtime check at all, and returns its value bare.
pub trait Fallibility: 'static {
    #[doc(hidden)]
    const CAN_FAIL: bool;
    /// `Infallible` or `Refused`.
    #[doc(hidden)]
    type Error: Into<Refused>;
    /// `T`, or `Result<T, Refused>`.
    #[doc(hidden)]
    type Wrap<T>;
    /// Refusable if either is.
    #[doc(hidden)]
    type Or<E: Fallibility>: Fallibility;
    /// Runs `check` only if the operation can be refused.
    #[doc(hidden)]
    fn guard(check: impl FnOnce() -> Option<Refused>) -> Result<(), Self::Error>;
    #[doc(hidden)]
    fn wrap<T>(result: Result<T, Self::Error>) -> Self::Wrap<T>;
    #[doc(hidden)]
    fn left<E: Fallibility>(result: Result<(), Self::Error>) -> Result<(), <Self::Or<E> as Fallibility>::Error>;
    #[doc(hidden)]
    fn right<E: Fallibility>(result: Result<(), E::Error>) -> Result<(), <Self::Or<E> as Fallibility>::Error>;
}

/// Can't be refused.
pub enum Sure {}

impl Fallibility for Sure {
    const CAN_FAIL: bool = false;
    type Error = Infallible;
    type Wrap<T> = T;
    type Or<E: Fallibility> = E;

    fn guard(_: impl FnOnce() -> Option<Refused>) -> Result<(), Infallible> {
        Ok(())
    }

    fn wrap<T>(result: Result<T, Infallible>) -> T {
        match result {
            Ok(t) => t,
            Err(never) => match never {},
        }
    }

    fn left<E: Fallibility>(result: Result<(), Infallible>) -> Result<(), E::Error> {
        match result {
            Ok(()) => Ok(()),
            Err(never) => match never {},
        }
    }

    fn right<E: Fallibility>(result: Result<(), E::Error>) -> Result<(), E::Error> {
        result
    }
}

impl Fallibility for Refused {
    const CAN_FAIL: bool = true;
    type Error = Refused;
    type Wrap<T> = Result<T, Refused>;
    type Or<E: Fallibility> = Refused;

    fn guard(check: impl FnOnce() -> Option<Refused>) -> Result<(), Refused> {
        check().map_or(Ok(()), Err)
    }

    fn wrap<T>(result: Result<T, Refused>) -> Result<T, Refused> {
        result
    }

    fn left<E: Fallibility>(result: Result<(), Refused>) -> Result<(), Refused> {
        result
    }

    fn right<E: Fallibility>(result: Result<(), E::Error>) -> Result<(), Refused> {
        result.map_err(Into::into)
    }
}

impl From<Infallible> for Refused {
    fn from(never: Infallible) -> Refused {
        match never {}
    }
}

/// Whether spawning with an edge of `R` can be refused: if `R` refuses to let its target go
/// (the target may already be doomed), or has a hook (which may refuse).
#[doc(hidden)]
pub type SpawnOf<R> = <<<R as Relation>::Policy as PolicyKind>::Holds as Fallibility>::Or<<R as Relation>::Hook>;

/// Why a live turn refused an operation: graph state no type can see.
///
/// A refused `relate`, `spawn` or `despawn` changed nothing. When a hook refuses, the edge
/// that ran it and the hook's own earlier steps stay: each of them left a correct graph.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Refused {
    /// Something outside the despawn still points at it through a `refuse` relation.
    Held,
    /// The target is despawned at the end of this turn.
    Doomed,
    /// A chain of hooks didn't settle: it went too deep or ran too many hooks.
    Unsettled,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Refused::Held => "something still holds it through a `refuse` relation",
            Refused::Doomed => "the target is despawned at the end of this turn",
            Refused::Unsettled => "a chain of hooks didn't settle",
        })
    }
}

impl std::error::Error for Refused {}

// ------------------------------------------------------------------ edge storage

/// Every edge of one relation, oldest first. A `one` edge also lives in its source's node,
/// and its entry here shares that slot.
#[doc(hidden)]
pub struct RelLog<R: Relation> {
    pub(crate) entries: Chain<Entry<R>>,
}

impl<R: Relation> Default for RelLog<R> {
    fn default() -> Self {
        RelLog { entries: Chain::default() }
    }
}

pub(crate) struct Entry<R: Relation> {
    src: <R::Source as Handle>::Owned,
    src_addr: usize,
    edge: Edge<R>,
    alive: Cell<bool>,
}

enum Edge<R: Relation> {
    One(Rc<OneSlot<R>>),
    Free { dst: <R::Target as Handle>::Owned, dst_addr: usize, vals: R::Vals },
}

impl<R: Relation> Entry<R> {
    pub(crate) fn src(&self) -> &<R::Source as Handle>::Owned {
        &self.src
    }

    pub(crate) fn src_addr(&self) -> usize {
        self.src_addr
    }

    pub(crate) fn dst(&self) -> &<R::Target as Handle>::Owned {
        match &self.edge {
            Edge::One(slot) => &slot.current().1,
            Edge::Free { dst, .. } => dst,
        }
    }

    pub(crate) fn dst_addr(&self) -> usize {
        match &self.edge {
            Edge::One(slot) => slot.addr.get(),
            Edge::Free { dst_addr, .. } => *dst_addr,
        }
    }

    /// The value, read from the source's side.
    pub(crate) fn fwd(&self) -> &R {
        match &self.edge {
            Edge::One(slot) => &slot.current().0,
            Edge::Free { vals, .. } => R::fwd(vals),
        }
    }

    /// The value, read from the target's side. Differs from `fwd` only for two-way edges.
    pub(crate) fn back(&self) -> &R {
        match &self.edge {
            Edge::One(slot) => &slot.current().0,
            #[cfg(not(weft_mutant = "back_reads_forward_label"))]
            Edge::Free { vals, .. } => R::back(vals),
            #[cfg(weft_mutant = "back_reads_forward_label")]
            Edge::Free { vals, .. } => R::fwd(vals),
        }
    }

    /// Whether this edge joins `a` and `b`: from `a` to `b`, or either way if two-way.
    fn joins(&self, a: usize, b: usize) -> bool {
        let (s, d) = (self.src_addr, self.dst_addr());
        (s == a && d == b) || (R::TWO_WAY && s == b && d == a)
    }
}

fn addr_of<X: Handle>(owned: &X::Owned) -> usize {
    X::erase(X::borrow(owned)).addr()
}

impl<R: Relation> RelLog<R> {
    pub(crate) fn live(&self) -> impl Iterator<Item = &Entry<R>> {
        self.entries.iter().filter(|e| e.alive.get())
    }

    pub(crate) fn push_one(&self, src: <R::Source as Handle>::Owned, slot: Rc<OneSlot<R>>) {
        let src_addr = addr_of::<R::Source>(&src);
        self.entries.push(Entry { src, src_addr, edge: Edge::One(slot), alive: Cell::new(true) });
    }

    fn push_free(&self, src: <R::Source as Handle>::Owned, dst: <R::Target as Handle>::Owned, vals: R::Vals) {
        let (src_addr, dst_addr) = (addr_of::<R::Source>(&src), addr_of::<R::Target>(&dst));
        self.entries.push(Entry { src, src_addr, edge: Edge::Free { dst, dst_addr, vals }, alive: Cell::new(true) });
    }

    /// Removes every edge that joins `s` and `d`. Whether there was one.
    pub(crate) fn unlink(&self, s: usize, d: usize) -> bool {
        let mut found = false;
        for e in self.live().filter(|e| e.joins(s, d)) {
            e.alive.set(false);
            found = true;
        }
        found
    }

    /// Where a spilled source lands: starting at `from` and walking up this relation, the
    /// first holder that isn't in `set`. `None` if the walk runs out.
    fn lands(&self, from: usize, set: &BTreeSet<usize>) -> Option<&<R::Target as Handle>::Owned> {
        let mut at = from;
        for _ in 0..=self.entries.len() {
            let up = self.live().find(|e| e.src_addr == at)?;
            if !set.contains(&up.dst_addr()) {
                return Some(up.dst());
            }
            at = up.dst_addr();
        }
        None
    }
}

#[doc(hidden)]
pub fn __one_way_targets<R: Relation>(log: &RelLog<R>, at: usize) -> Vec<(Id<'_, R::Target>, &R)> {
    log.live().filter(|e| e.src_addr == at).map(|e| (Id::new(R::Target::borrow(e.dst())), e.fwd())).collect()
}

#[doc(hidden)]
pub fn __one_way_sources<R: Relation>(log: &RelLog<R>, at: usize) -> Vec<(Id<'_, R::Source>, &R)> {
    log.live().filter(|e| e.dst_addr() == at).map(|e| (Id::new(R::Source::borrow(&e.src)), e.fwd())).collect()
}

/// Both directions of a two-way relation: each end sees the other with its own value.
#[doc(hidden)]
pub fn __two_way<S: Handle, R: Relation<Source = S, Target = S>>(log: &RelLog<R>, at: usize) -> Vec<(Id<'_, S>, &R)> {
    let mut out = Vec::new();
    for e in log.live() {
        if e.src_addr == at {
            out.push((Id::new(S::borrow(e.dst())), e.fwd()));
        } else if e.dst_addr() == at {
            out.push((Id::new(S::borrow(&e.src)), e.back()));
        }
    }
    out
}

#[doc(hidden)]
pub fn __link_one<'t, R, X, Y>(_: &'t RelLog<R>, s: Id<'t, X>, value: R, d: Id<'t, Y>)
where
    R: Relation<Vals = R>,
    X: Carries<R>,
    Y: Member<R::Target>,
{
    X::slot(&s.node().links).push(Rc::new(value), Y::own(d.r));
}

#[doc(hidden)]
pub fn __link_opt<'t, R, X, Y>(log: &'t RelLog<R>, s: Id<'t, X>, vals: R::Vals, d: Id<'t, Y>)
where
    R: Relation,
    X: Member<R::Source>,
    Y: Member<R::Target>,
{
    let at = s.addr();
    for e in log.live().filter(|e| e.src_addr == at) {
        e.alive.set(false);
    }
    log.push_free(X::own(s.r), Y::own(d.r), vals);
}

#[doc(hidden)]
pub fn __link_many<'t, R, X, Y>(log: &'t RelLog<R>, s: Id<'t, X>, vals: R::Vals, d: Id<'t, Y>)
where
    R: Relation,
    X: Member<R::Source>,
    Y: Member<R::Target>,
{
    log.unlink(s.addr(), d.addr());
    log.push_free(X::own(s.r), Y::own(d.r), vals);
}

#[doc(hidden)]
pub fn __read_one<'t, R, K>(id: Id<'t, K>) -> Id<'t, R::Target>
where
    R: Relation<Arity = One>,
    K: Carries<R>,
{
    Id::new(R::Target::borrow(&K::slot(&id.node().links).current().1))
}

#[doc(hidden)]
pub fn __read_opt<'t, R, X>(id: Id<'t, X>, log: &'t RelLog<R>) -> Option<Id<'t, R::Target>>
where
    R: Relation<Arity = Opt>,
    X: Member<R::Source>,
{
    let at = id.addr();
    log.live().find(|e| e.src_addr == at).map(|e| Id::new(R::Target::borrow(e.dst())))
}

// ------------------------------------------------------------------ the log as graph storage

impl<R: Relation> Piece for RelLog<R> {
    fn grow(&self, set: &BTreeSet<usize>, out: &mut Vec<usize>) {
        let policy = R::Policy::POLICY;
        if !matches!(policy, Policy::Cascade | Policy::Spill) {
            return;
        }
        for e in self.live().filter(|e| set.contains(&e.dst_addr()) && !set.contains(&e.src_addr)) {
            if policy == Policy::Cascade || self.lands(e.dst_addr(), set).is_none() {
                out.push(e.src_addr);
            }
        }
    }

    fn held(&self, set: &BTreeSet<usize>) -> bool {
        R::Policy::POLICY == Policy::Refuse
            && self.live().any(|e| set.contains(&e.dst_addr()) && !set.contains(&e.src_addr))
    }

    fn spill(&self, set: &BTreeSet<usize>) {
        if R::Policy::POLICY != Policy::Spill {
            return;
        }
        for e in self.live().filter(|e| set.contains(&e.dst_addr()) && !set.contains(&e.src_addr)) {
            if let (Edge::One(slot), Some(to)) = (&e.edge, self.lands(e.dst_addr(), set)) {
                slot.push(Rc::clone(&slot.current().0), R::Target::dup(to));
            }
        }
    }

    fn sweep(&mut self, set: &BTreeSet<usize>) {
        #[cfg(not(weft_mutant = "purge_keeps_edges"))]
        self.entries.retain(|e| e.alive.get() && !set.contains(&e.src_addr) && !set.contains(&e.dst_addr()));
        #[cfg(weft_mutant = "purge_keeps_edges")]
        self.entries.retain(|e| e.alive.get());
    }

    fn fmt_edges(&self, at: usize, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for e in self.live().filter(|e| e.src_addr == at) {
            write!(f, "  {:?} ", e.fwd())?;
            f.write_str(if R::TWO_WAY { "<-> " } else { "-> " })?;
            R::Target::erase(R::Target::borrow(e.dst())).label(f)?;
            if R::TWO_WAY {
                write!(f, " {:?}", e.back())?;
            }
            writeln!(f)?;
        }
        Ok(())
    }

    fn audit(&self, audit: &mut Audit) {
        let mut pairs = BTreeSet::new();
        for e in self.live() {
            let (s, d) = (e.src_addr, e.dst_addr());
            if !audit.nodes.contains(&s) || !audit.nodes.contains(&d) {
                audit.broken.push(Broken::Dangling { relation: R::NAME });
            }
            if let Edge::One(slot) = &e.edge {
                audit.logged_slots.push((Rc::as_ptr(slot).addr(), s, R::NAME));
            }
            let pair = if R::TWO_WAY { (s.min(d), s.max(d)) } else if R::Arity::SINGLE { (s, 0) } else { (s, d) };
            if !pairs.insert(pair) {
                audit.broken.push(Broken::Doubled { relation: R::NAME });
            }
        }
        if R::Loops::ACYCLIC {
            let edges: Vec<(usize, usize)> = self.live().map(|e| (e.src_addr, e.dst_addr())).collect();
            if crate::graph::has_cycle(&edges) {
                audit.broken.push(Broken::Cycle { relation: R::NAME });
            }
        }
    }
}
