use alloc::boxed::Box;
use alloc::collections::{BTreeSet, VecDeque};
use alloc::vec::Vec;
use core::cell::Cell;
use core::mem;

use crate::bundle::Outcome;
use crate::component::{Column, Tick};
use crate::entity::Entities;
use crate::event::Observers;
use crate::relation::{Arity, Edges, One};
use crate::schema::{Columns, Relations};
use crate::{
    Borrows, Bundle, Component, Entity, Links, Mut, NoSuchEntity, Owns, Query, QueryData, QueryMut, ReadQuery, Reads, Relation,
    Requires, Schema, To, Traverse, Writes,
};

pub(crate) type Deferred<S> = Box<dyn FnOnce(&mut World<S>)>;

/// A hook waiting for the bundle being inserted to be complete.
enum Hook<S: Schema> {
    Component(fn(&mut World<S>, Entity), Entity),
    Relation(fn(&mut World<S>, Entity, Entity), Entity, Entity),
}

/// Entities, their components and relations, and resources, as described by
/// the [`Schema`] `S`.
///
/// Access follows Rust's own rules. Reading takes `&self`; changing a value
/// in place takes `&mut self`, as does anything structural. The borrow
/// checker does the rest: there are no runtime borrow flags, so nothing can
/// fail at runtime because of aliasing.
pub struct World<S: Schema> {
    pub(crate) entities: Entities,
    pub(crate) columns: S::Columns,
    pub(crate) relations: S::Relations,
    pub(crate) resources: S::Resources,
    pub(crate) observers: Observers<S>,
    hooks: Vec<Hook<S>>,
    pub(crate) deferred: Cell<Vec<Deferred<S>>>,
    pub(crate) tick: Tick,
    pub(crate) last_run: Tick,
}

impl<S: Schema> Default for World<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Schema> World<S> {
    /// An empty world with every resource at its default.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entities: Entities::default(),
            columns: S::Columns::default(),
            relations: S::Relations::default(),
            resources: S::Resources::default(),
            observers: Observers::default(),
            hooks: Vec::new(),
            deferred: Cell::default(),
            tick: 1,
            last_run: 0,
        }
    }

    // ---------------------------------------------------------------- entities

    /// Creates an entity with `bundle`.
    ///
    /// Returns the [`Entity`] directly if the bundle can't fail, and
    /// `Result<Entity, NoSuchEntity>` if it contains a relation whose target
    /// might be dead. Nothing is created on failure.
    pub fn spawn<M, B: Bundle<S, M>>(&mut self, bundle: B) -> <B::Error as Outcome>::Wrap<Entity> {
        if let Err(error) = bundle.check(self) {
            return error.failure();
        }
        let entity = self.entities.alloc();
        bundle.apply(self, entity);
        self.run_hooks();
        <B::Error as Outcome>::success(entity)
    }

    /// Whether `entity` is alive.
    #[must_use]
    pub fn is_alive(&self, entity: Entity) -> bool {
        self.entities.is_alive(entity)
    }

    /// How many entities are alive.
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Despawns `entity` and everything that cascades from it through
    /// [`Relation::CASCADE`], dependents first. For each entity, edge hooks
    /// run first, then every component's [`Component::on_remove`] while the
    /// entity is still complete, and only then is anything removed. Returns
    /// `false` if the entity was already dead.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.is_alive(entity) {
            return false;
        }
        let mut doomed = Vec::from([entity]);
        let mut seen = BTreeSet::from([entity]);
        let mut queue = VecDeque::from([entity]);
        while let Some(current) = queue.pop_front() {
            let mut dependents = Vec::new();
            self.relations.cascade(current, &mut dependents);
            for dependent in dependents {
                if seen.insert(dependent) {
                    doomed.push(dependent);
                    queue.push_back(dependent);
                }
            }
        }
        for entity in doomed.into_iter().rev() {
            self.despawn_one(entity);
        }
        true
    }

    fn despawn_one(&mut self, entity: Entity) {
        if !self.is_alive(entity) {
            return; // a hook got here first
        }
        // Edges first, so their hooks can still read both endpoints. Then
        // every component hook, while all components are still there.
        S::Relations::unrelate_all(self, entity);
        S::Columns::on_remove_all(self, entity);
        // Hooks may also have attached new things during teardown.
        self.columns.discard(entity);
        self.relations.discard(entity);
        self.observers.forget(entity);
        self.entities.free(entity);
    }

    // -------------------------------------------------------------- components

    pub(crate) fn column<T>(&self) -> &Column<T>
    where
        S: Reads<T>,
    {
        S::column(&self.columns)
    }

    /// Inserts `bundle` on `entity`, replacing components it already has.
    ///
    /// # Errors
    /// If `entity` is dead, or the bundle relates to a dead target. Nothing
    /// is inserted then.
    pub fn insert<M, B: Bundle<S, M>>(&mut self, entity: Entity, bundle: B) -> Result<(), NoSuchEntity> {
        if !self.is_alive(entity) {
            return Err(NoSuchEntity(entity));
        }
        bundle.check(self).map_err(Outcome::into_error)?;
        bundle.apply(self, entity);
        self.run_hooks();
        Ok(())
    }

    pub(crate) fn insert_component<C>(&mut self, entity: Entity, component: C, replace: bool)
    where
        S: Reads<C>,
        C: Component<S>,
    {
        if !self.is_alive(entity) || (!replace && self.has::<C>(entity)) {
            return;
        }
        let tick = self.tick;
        S::column_mut(&mut self.columns).insert(entity, component, tick);
        self.hooks.push(Hook::Component(C::on_insert, entity));
        C::requires(&mut Requires { world: self, entity });
    }

    fn run_hooks(&mut self) {
        for hook in mem::take(&mut self.hooks) {
            match hook {
                Hook::Component(hook, entity) => hook(self, entity),
                Hook::Relation(hook, source, target) => hook(self, source, target),
            }
        }
    }

    /// Removes a component, running [`Component::on_remove`] first.
    pub fn remove<T>(&mut self, entity: Entity) -> Option<T>
    where
        S: Reads<T>,
        T: Component<S>,
    {
        if !self.has::<T>(entity) {
            return None;
        }
        T::on_remove(self, entity);
        S::column_mut(&mut self.columns).remove(entity)
    }

    /// Whether `entity` has a `T`.
    #[must_use]
    pub fn has<T>(&self, entity: Entity) -> bool
    where
        S: Reads<T>,
    {
        self.column::<T>().contains(entity)
    }

    /// Borrows a component.
    #[must_use]
    pub fn get<T>(&self, entity: Entity) -> Option<&T>
    where
        S: Reads<T>,
    {
        self.column::<T>().get(entity)
    }

    /// Mutably borrows a component the schema lists as `mut`. Writing through
    /// the result marks it changed.
    pub fn get_mut<T>(&mut self, entity: Entity) -> Option<Mut<'_, T>>
    where
        S: Writes<T>,
    {
        let tick = self.tick;
        S::column_mut(&mut self.columns).get_mut(entity, tick)
    }

    // --------------------------------------------------------------- relations

    fn edges<R>(&self) -> &Edges<R>
    where
        S: Links<R>,
    {
        S::edges(&self.relations)
    }

    /// Creates the edge `source --relation--> target`, or replaces its value.
    /// For [`One`]-arity relations, other edges from `source` are removed first.
    ///
    /// # Errors
    /// If either entity is dead.
    pub fn relate<R>(&mut self, source: Entity, relation: R, target: Entity) -> Result<(), NoSuchEntity>
    where
        S: Links<R>,
        R: Relation<S>,
    {
        self.insert(source, To(relation, target))
    }

    pub(crate) fn relate_edge<R>(&mut self, source: Entity, relation: R, target: Entity)
    where
        S: Links<R>,
        R: Relation<S>,
    {
        if <R::Arity as Arity>::EXCLUSIVE {
            let edges = self.edges::<R>();
            let mut stale: Vec<_> = edges.targets(source).iter().map(|&t| (source, t)).collect();
            if R::SYMMETRIC {
                stale.extend(edges.targets(target).iter().map(|&t| (target, t)));
            }
            for (a, b) in stale {
                if (a, b) != (source, target) && (a, b) != (target, source) {
                    self.unrelate::<R>(a, b);
                }
            }
        }
        // Unrelate hooks may have despawned either end.
        if self.is_alive(source) && self.is_alive(target) {
            S::edges_mut(&mut self.relations).insert(R::SYMMETRIC, source, target, relation);
            self.hooks.push(Hook::Relation(R::on_relate, source, target));
        }
    }

    /// Removes an edge, running [`Relation::on_unrelate`] first.
    pub fn unrelate<R>(&mut self, source: Entity, target: Entity) -> Option<R>
    where
        S: Links<R>,
        R: Relation<S>,
    {
        if !self.is_related::<R>(source, target) {
            return None;
        }
        R::on_unrelate(self, source, target);
        S::edges_mut(&mut self.relations).remove(R::SYMMETRIC, source, target)
    }

    /// Removes every `R` edge touching `entity`, in either direction.
    pub fn unrelate_all<R>(&mut self, entity: Entity)
    where
        S: Links<R>,
        R: Relation<S>,
    {
        for (source, target) in self.edges::<R>().edges_of(R::SYMMETRIC, entity) {
            self.unrelate::<R>(source, target);
        }
    }

    /// Whether the edge `source --R--> target` exists.
    #[must_use]
    pub fn is_related<R>(&self, source: Entity, target: Entity) -> bool
    where
        S: Links<R>,
        R: Relation<S>,
    {
        self.edges::<R>().get(R::SYMMETRIC, source, target).is_some()
    }

    /// What `source` points at, oldest edge first.
    #[must_use]
    pub fn targets<R>(&self, source: Entity) -> &[Entity]
    where
        S: Links<R>,
        R: Relation<S>,
    {
        self.edges::<R>().targets(source)
    }

    /// What points at `target`, oldest edge first.
    #[must_use]
    pub fn sources<R>(&self, target: Entity) -> &[Entity]
    where
        S: Links<R>,
        R: Relation<S>,
    {
        self.edges::<R>().sources(target)
    }

    /// The single target of a [`One`]-arity relation:
    /// `world.target::<ChildOf>(e)` is the parent.
    ///
    /// ```compile_fail,E0271
    /// # use weft::*;
    /// struct Likes;
    /// impl<S: Schema> Relation<S> for Likes { type Arity = Many; }
    /// schema! { struct Game { components: [], relations: [Likes], resources: [] } }
    /// let world = World::<Game>::new();
    /// # let e: Entity = todo!();
    /// world.target::<Likes>(e); // many targets: which one would that be?
    /// ```
    #[must_use]
    pub fn target<R>(&self, source: Entity) -> Option<Entity>
    where
        S: Links<R>,
        R: Relation<S, Arity = One>,
    {
        self.targets::<R>(source).first().copied()
    }

    /// Borrows the value on an edge.
    #[must_use]
    pub fn relation<R>(&self, source: Entity, target: Entity) -> Option<&R>
    where
        S: Links<R>,
        R: Relation<S>,
    {
        self.edges::<R>().get(R::SYMMETRIC, source, target)
    }

    /// Mutably borrows the value on an edge.
    pub fn relation_mut<R>(&mut self, source: Entity, target: Entity) -> Option<&mut R>
    where
        S: Links<R>,
        R: Relation<S>,
    {
        S::edges_mut(&mut self.relations).get_mut(R::SYMMETRIC, source, target)
    }

    /// Follows targets transitively, breadth first: parent, grandparent, ...
    #[must_use]
    pub fn ancestors<R>(&self, entity: Entity) -> Traverse<'_, R>
    where
        S: Links<R>,
        R: Relation<S>,
    {
        Traverse::new(self.edges::<R>(), entity, true)
    }

    /// Follows sources transitively, breadth first: children, grandchildren, ...
    #[must_use]
    pub fn descendants<R>(&self, entity: Entity) -> Traverse<'_, R>
    where
        S: Links<R>,
        R: Relation<S>,
    {
        Traverse::new(self.edges::<R>(), entity, false)
    }

    // ----------------------------------------------------------------- queries

    /// A read-only query. See [`ReadQuery`] for what `Q` can be.
    #[must_use]
    pub const fn query<Q: ReadQuery<S>>(&self) -> Query<'_, S, Q> {
        Query::new(self)
    }

    /// A query with exclusive access, which can include `&mut T`. See
    /// [`BorrowQuery`](crate::BorrowQuery) for what `Q` can be.
    pub fn query_mut<Q: QueryData>(&mut self) -> QueryMut<'_, S, Q>
    where
        S: Borrows<Q>,
    {
        QueryMut::new(self)
    }

    // --------------------------------------------------------------- resources

    /// Borrows a resource. Resources listed in the schema always exist.
    #[must_use]
    pub fn resource<T>(&self) -> &T
    where
        S: Owns<T>,
    {
        S::resource(&self.resources)
    }

    /// Mutably borrows a resource.
    pub fn resource_mut<T>(&mut self) -> &mut T
    where
        S: Owns<T>,
    {
        S::resource_mut(&mut self.resources)
    }

    // ---------------------------------------------------------- deferred work

    /// Queues a structural change through `&self`, e.g. while iterating a
    /// query. Runs at the next [`World::flush`]; a [`Schedule`](crate::Schedule)
    /// flushes after every system.
    pub fn defer(&self, work: impl FnOnce(&mut Self) + 'static) {
        let mut queue = self.deferred.take();
        queue.push(Box::new(work));
        self.deferred.set(queue);
    }

    /// Runs deferred work, including work it defers in turn.
    pub fn flush(&mut self) {
        loop {
            let queue = self.deferred.take();
            if queue.is_empty() {
                return;
            }
            for work in queue {
                work(self);
            }
        }
    }

    // -------------------------------------------------------- change detection

    /// Outside a [`Schedule`](crate::Schedule): forgets all changes so far,
    /// so [`Added`](crate::Added) and [`Changed`](crate::Changed) only see new ones.
    pub fn clear_trackers(&mut self) {
        self.last_run = self.tick;
        self.tick = self.tick.saturating_add(1);
    }

    /// Runs `system` with `last_run` as its change baseline, flushes, and
    /// returns the tick to remember as the system's last run.
    pub(crate) fn run_tracked(&mut self, last_run: Tick, system: impl FnOnce(&mut Self)) -> Tick {
        let outer = mem::replace(&mut self.last_run, last_run);
        system(self);
        self.flush();
        let ran_at = self.tick;
        self.tick = self.tick.saturating_add(1);
        self.last_run = outer;
        ran_at
    }
}
