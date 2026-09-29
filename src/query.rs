use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::Cell;
use core::marker::PhantomData;

use crate::component::{Column, Mut, Tick};
use crate::entity::Entities;
use crate::relation::{Edges, One, Relation};
use crate::schema::{Columns, Lend, LendMut, Loans, Peek};
use crate::world::Deferred;
use crate::{Entity, Links, Owns, Reads, Schema, World, Writes};

/// Some entities, with a count, so queries can iterate the shortest list.
pub struct Candidates<'w> {
    len: usize,
    entities: Box<dyn Iterator<Item = Entity> + 'w>,
}

impl<'w> Candidates<'w> {
    pub(crate) fn new(len: usize, entities: impl Iterator<Item = Entity> + 'w) -> Self {
        Self { len, entities: Box::new(entities) }
    }

    fn shortest(a: Option<Self>, b: Option<Self>) -> Option<Self> {
        match (a, b) {
            (Some(a), Some(b)) => Some(if b.len < a.len { b } else { a }),
            (a, b) => a.or(b),
        }
    }
}

/// Anything that can appear in a query. Knows which entities it could match.
pub trait Term<S: Schema> {
    /// A list containing every entity this term matches, if it has one.
    #[doc(hidden)]
    fn candidates(world: &World<S>) -> Option<Candidates<'_>> {
        let _ = world;
        None
    }
}

/// What a query term yields per entity, independent of the schema.
pub trait QueryData {
    /// The fetched value.
    type Item<'a>;
}

/// What a [`Query`] (over `&World`) fetches per entity.
///
/// | term               | item                | matches when          |
/// |--------------------|---------------------|-----------------------|
/// | `&T`               | `&T`                | entity has `T`        |
/// | `Option<Q>`        | `Option<Q::Item>`   | always                |
/// | `Entity`           | `Entity`            | always                |
/// | [`Targets<R>`]     | `&[Entity]`         | always                |
/// | [`Sources<R>`]     | `&[Entity]`         | always                |
/// | tuples             | tuples              | every element matches |
#[diagnostic::on_unimplemented(
    message = "`{Self}` can't be fetched through `&World`",
    label = "not a read-only query",
    note = "`&mut T` terms need exclusive access: use `world.query_mut::<..>()`"
)]
pub trait ReadQuery<S: Schema>: Term<S> + QueryData {
    /// Fetches for one entity, or `None` if it doesn't match.
    fn read(world: &World<S>, entity: Entity) -> Option<Self::Item<'_>>;
}

/// What a [`QueryMut`] (over `&mut World`) fetches per entity: everything a
/// [`ReadQuery`] can, plus `&mut T` for components the schema lists as `mut`.
///
/// Each term borrows its column out of `L`, the type-level state of every
/// column: `&mut T` takes it exclusively, `&T` shares it. Borrowing the same
/// column mutably twice, or mutably and shared, is a type error, just like
/// it is for ordinary references.
///
/// ```compile_fail,E0277
/// # use weft::*;
/// # struct Pos(i32); impl<S: Schema> Component<S> for Pos {}
/// # schema! { struct Game { components: [mut Pos], relations: [], resources: [] } }
/// let mut world = World::<Game>::new();
/// world.query_mut::<(&mut Pos, &Pos)>(); // `Pos` is already borrowed mutably
/// ```
pub trait BorrowQuery<S: Schema, L: Loans>: Term<S> + QueryData {
    /// The borrowed columns.
    type State<'w>;
    /// `L` after this term's loans.
    type Rest: Loans;

    /// Takes this term's columns out of the list.
    fn claim<'w>(list: L::At<'w>, context: Context<'w, S>) -> (Self::State<'w>, <Self::Rest as Loans>::At<'w>);

    /// Fetches for one entity, or `None` if it doesn't match.
    fn fetch<'a>(state: &'a mut Self::State<'_>, entity: Entity) -> Option<Self::Item<'a>>;
}

/// The schema can lend out the columns the query `Q` borrows, all at once.
///
/// For a concrete schema this is checked automatically. Code that is generic
/// over the schema states what it borrows as a bound, with `'static` in place
/// of the query's lifetimes:
///
/// ```
/// # use weft::*;
/// # struct Pos(i32); impl<S: Schema> Component<S> for Pos {}
/// # struct Vel(i32); impl<S: Schema> Component<S> for Vel {}
/// fn physics<S: Borrows<(&'static mut Pos, &'static Vel)>>(world: &mut World<S>) {
///     world.query_mut::<(&mut Pos, &Vel)>().for_each(|(mut pos, vel)| pos.0 += vel.0);
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` can't lend out everything the query `{Q}` borrows",
    label = "the query's borrows don't fit this schema",
    note = "every `&mut T` needs `mut T` in the schema, and a component borrowed mutably can't be borrowed again",
    note = "in code generic over the schema, add the bound `S: Borrows<{Q}>` (with `'static` lifetimes)"
)]
pub trait Borrows<Q: QueryData>: Schema {
    #[doc(hidden)]
    type State<'w>;
    #[doc(hidden)]
    type Rest: Loans;

    #[doc(hidden)]
    fn candidates(world: &World<Self>) -> Option<Candidates<'_>>;

    #[doc(hidden)]
    fn claim<'w>(
        columns: <Self::Columns as Loans>::At<'w>,
        context: Context<'w, Self>,
    ) -> (Self::State<'w>, <Self::Rest as Loans>::At<'w>);

    #[doc(hidden)]
    fn fetch<'a>(state: &'a mut Self::State<'_>, entity: Entity) -> Option<Q::Item<'a>>;
}

impl<S: Schema, Q: BorrowQuery<S, S::Columns>> Borrows<Q> for S {
    type State<'w> = Q::State<'w>;
    type Rest = Q::Rest;

    fn candidates(world: &World<S>) -> Option<Candidates<'_>> {
        Q::candidates(world)
    }

    fn claim<'w>(
        columns: <S::Columns as Loans>::At<'w>,
        context: Context<'w, S>,
    ) -> (Q::State<'w>, <Q::Rest as Loans>::At<'w>) {
        Q::claim(columns, context)
    }

    fn fetch<'a>(state: &'a mut Q::State<'_>, entity: Entity) -> Option<Q::Item<'a>> {
        Q::fetch(state, entity)
    }
}

/// What terms can borrow besides columns.
pub struct Context<'w, S: Schema> {
    relations: &'w S::Relations,
    tick: Tick,
}

impl<S: Schema> Clone for Context<'_, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: Schema> Copy for Context<'_, S> {}

// -------------------------------------------------------------------- &T

impl<S: Reads<T>, T: 'static> Term<S> for &T {
    fn candidates(world: &World<S>) -> Option<Candidates<'_>> {
        Some(world.column::<T>().candidates())
    }
}

impl<T: 'static> QueryData for &T {
    type Item<'a> = &'a T;
}

impl<S: Reads<T>, T: 'static> ReadQuery<S> for &T {
    fn read(world: &World<S>, entity: Entity) -> Option<&T> {
        world.get::<T>(entity)
    }
}

impl<S, T, L> BorrowQuery<S, L> for &T
where
    S: Reads<T>,
    T: 'static,
    L: Lend<S::Index, Target = Column<T>>,
{
    type State<'w> = &'w Column<T>;
    type Rest = L::Rest;

    fn claim<'w>(list: L::At<'w>, _: Context<'w, S>) -> (&'w Column<T>, <L::Rest as Loans>::At<'w>) {
        L::lend(list)
    }

    fn fetch<'a>(state: &'a mut &Column<T>, entity: Entity) -> Option<&'a T> {
        state.get(entity)
    }
}

// ---------------------------------------------------------------- &mut T

impl<T: 'static> QueryData for &mut T {
    type Item<'a> = Mut<'a, T>;
}

impl<S: Reads<T>, T: 'static> Term<S> for &mut T {
    fn candidates(world: &World<S>) -> Option<Candidates<'_>> {
        Some(world.column::<T>().candidates())
    }
}

impl<S, T, L> BorrowQuery<S, L> for &mut T
where
    S: Writes<T>,
    T: 'static,
    L: LendMut<S::Index, Target = Column<T>>,
{
    type State<'w> = (&'w mut Column<T>, Tick);
    type Rest = L::Rest;

    fn claim<'w>(list: L::At<'w>, context: Context<'w, S>) -> (Self::State<'w>, <L::Rest as Loans>::At<'w>) {
        let (column, rest) = L::lend_mut(list);
        ((column, context.tick), rest)
    }

    fn fetch<'a>((column, tick): &'a mut Self::State<'_>, entity: Entity) -> Option<Mut<'a, T>> {
        column.get_mut(entity, *tick)
    }
}

// ---------------------------------------------------------------- Entity

impl QueryData for Entity {
    type Item<'a> = Entity;
}

impl<S: Schema> Term<S> for Entity {}

impl<S: Schema> ReadQuery<S> for Entity {
    fn read(_: &World<S>, entity: Entity) -> Option<Entity> {
        Some(entity)
    }
}

impl<S: Schema, L: Loans> BorrowQuery<S, L> for Entity {
    type State<'w> = ();
    type Rest = L;

    fn claim<'w>(list: L::At<'w>, _: Context<'w, S>) -> ((), L::At<'w>) {
        ((), list)
    }

    fn fetch(_: &mut (), entity: Entity) -> Option<Entity> {
        Some(entity)
    }
}

// ------------------------------------------------------------- Option<Q>

impl<Q: QueryData> QueryData for Option<Q> {
    type Item<'a> = Option<Q::Item<'a>>;
}

impl<S: Schema, Q: Term<S>> Term<S> for Option<Q> {}

impl<S: Schema, Q: ReadQuery<S>> ReadQuery<S> for Option<Q> {
    fn read(world: &World<S>, entity: Entity) -> Option<Self::Item<'_>> {
        Some(Q::read(world, entity))
    }
}

impl<S: Schema, L: Loans, Q: BorrowQuery<S, L>> BorrowQuery<S, L> for Option<Q> {
    type State<'w> = Q::State<'w>;
    type Rest = Q::Rest;

    fn claim<'w>(list: L::At<'w>, context: Context<'w, S>) -> (Q::State<'w>, <Q::Rest as Loans>::At<'w>) {
        Q::claim(list, context)
    }

    fn fetch<'a>(state: &'a mut Q::State<'_>, entity: Entity) -> Option<Self::Item<'a>> {
        Some(Q::fetch(state, entity))
    }
}

// ------------------------------------------------------- relation terms

/// Query term: the entity's outgoing `R` edges (possibly none).
pub struct Targets<R>(PhantomData<fn() -> R>);

/// Query term: the entity's incoming `R` edges (possibly none).
pub struct Sources<R>(PhantomData<fn() -> R>);

macro_rules! edge_term {
    ($name:ident, $list:ident) => {
        impl<R> QueryData for $name<R> {
            type Item<'a> = &'a [Entity];
        }

        impl<S: Links<R>, R: Relation<S>> Term<S> for $name<R> {}

        impl<S: Links<R>, R: Relation<S>> ReadQuery<S> for $name<R> {
            fn read(world: &World<S>, entity: Entity) -> Option<&[Entity]> {
                Some(world.$list::<R>(entity))
            }
        }

        impl<S: Links<R>, R: Relation<S>, L: Loans> BorrowQuery<S, L> for $name<R> {
            type State<'w> = &'w Edges<R>;
            type Rest = L;

            fn claim<'w>(list: L::At<'w>, context: Context<'w, S>) -> (&'w Edges<R>, L::At<'w>) {
                (S::edges(context.relations), list)
            }

            fn fetch<'a>(state: &'a mut &Edges<R>, entity: Entity) -> Option<&'a [Entity]> {
                Some(state.$list(entity))
            }
        }
    };
}

edge_term!(Targets, targets);
edge_term!(Sources, sources);

// ---------------------------------------------------------------- filters

/// Which entities a query keeps, as a type.
///
/// | filter              | keeps entities that                           |
/// |---------------------|-----------------------------------------------|
/// | `()`                | (all)                                         |
/// | [`With<T>`]         | have a `T`                                    |
/// | [`Without<T>`]      | don't have a `T`                              |
/// | [`Added<T>`]        | got a `T` since the system last ran           |
/// | [`Changed<T>`]      | got or wrote a `T` since the system last ran  |
/// | [`HasTarget<R>`]    | have an outgoing `R` edge                     |
/// | [`HasSource<R>`]    | have an incoming `R` edge                     |
/// | [`Not<F>`]          | fail `F`                                      |
/// | [`Or<(F, ..)>`](Or) | pass any                                      |
/// | `(F, ..)`           | pass all                                      |
pub trait Filter<S: Schema> {
    /// See [`Term::candidates`].
    #[doc(hidden)]
    fn candidates(world: &World<S>) -> Option<Candidates<'_>> {
        let _ = world;
        None
    }

    /// Whether `entity` passes.
    fn matches(world: &World<S>, entity: Entity) -> bool;
}

/// Filter: has a `T`.
pub struct With<T>(PhantomData<fn() -> T>);
/// Filter: doesn't have a `T`.
pub type Without<T> = Not<With<T>>;
/// Filter: `T` was added since the system last ran (see [`World::clear_trackers`]).
pub struct Added<T>(PhantomData<fn() -> T>);
/// Filter: `T` was added or written since the system last ran (see [`World::clear_trackers`]).
pub struct Changed<T>(PhantomData<fn() -> T>);
/// Filter: has an outgoing `R` edge. `Not<HasTarget<ChildOf>>` selects roots.
pub struct HasTarget<R>(PhantomData<fn() -> R>);
/// Filter: has an incoming `R` edge. `Not<HasSource<ChildOf>>` selects leaves.
pub struct HasSource<R>(PhantomData<fn() -> R>);
/// Filter: `F` fails.
pub struct Not<F>(PhantomData<fn() -> F>);
/// Filter: any filter in the tuple passes.
pub struct Or<F>(PhantomData<fn() -> F>);

macro_rules! column_filter {
    ($name:ident, |$column:ident, $entity:ident, $since:ident| $check:expr) => {
        impl<S: Reads<T>, T: 'static> Filter<S> for $name<T> {
            fn candidates(world: &World<S>) -> Option<Candidates<'_>> {
                Some(world.column::<T>().candidates())
            }

            fn matches(world: &World<S>, $entity: Entity) -> bool {
                let $column = world.column::<T>();
                let $since = world.last_run;
                $check
            }
        }
    };
}

column_filter!(With, |column, entity, _since| column.contains(entity));
column_filter!(Added, |column, entity, since| column.added(entity).is_some_and(|tick| tick > since));
column_filter!(Changed, |column, entity, since| column.changed(entity).is_some_and(|tick| tick > since));

impl<S: Links<R>, R: Relation<S>> Filter<S> for HasTarget<R> {
    fn matches(world: &World<S>, entity: Entity) -> bool {
        !world.targets::<R>(entity).is_empty()
    }
}

impl<S: Links<R>, R: Relation<S>> Filter<S> for HasSource<R> {
    fn matches(world: &World<S>, entity: Entity) -> bool {
        !world.sources::<R>(entity).is_empty()
    }
}

impl<S: Schema, F: Filter<S>> Filter<S> for Not<F> {
    fn matches(world: &World<S>, entity: Entity) -> bool {
        !F::matches(world, entity)
    }
}

// ----------------------------------------------------------------- tuples

macro_rules! tuples {
    ($last:ident; $($name:ident $from:ident $to:ident),*) => {
        impl<S: Schema, $($name: Term<S>),*> Term<S> for ($($name,)*) {
            fn candidates(world: &World<S>) -> Option<Candidates<'_>> {
                let best = None;
                $(let best = Candidates::shortest(best, $name::candidates(world));)*
                best
            }
        }

        impl<$($name: QueryData),*> QueryData for ($($name,)*) {
            type Item<'a> = ($($name::Item<'a>,)*);
        }

        impl<S: Schema, $($name: ReadQuery<S>),*> ReadQuery<S> for ($($name,)*) {
            fn read(world: &World<S>, entity: Entity) -> Option<Self::Item<'_>> {
                Some(($($name::read(world, entity)?,)*))
            }
        }

        #[allow(non_snake_case)]
        impl<S: Schema, L0: Loans, $($name, $to: Loans),*> BorrowQuery<S, L0> for ($($name,)*)
        where
            $($name: BorrowQuery<S, $from, Rest = $to>,)*
        {
            type State<'w> = ($($name::State<'w>,)*);
            type Rest = $last;

            fn claim<'w>(list: L0::At<'w>, context: Context<'w, S>) -> (Self::State<'w>, $last::At<'w>) {
                $(let ($name, list) = $name::claim(list, context);)*
                (($($name,)*), list)
            }

            fn fetch<'a>(state: &'a mut Self::State<'_>, entity: Entity) -> Option<Self::Item<'a>> {
                let ($($name,)*) = state;
                Some(($($name::fetch($name, entity)?,)*))
            }
        }

        impl<S: Schema, $($name: Filter<S>),*> Filter<S> for ($($name,)*) {
            fn candidates(world: &World<S>) -> Option<Candidates<'_>> {
                let best = None;
                $(let best = Candidates::shortest(best, $name::candidates(world));)*
                best
            }

            fn matches(world: &World<S>, entity: Entity) -> bool {
                true $(&& $name::matches(world, entity))*
            }
        }

        impl<S: Schema, $($name: Filter<S>),*> Filter<S> for Or<($($name,)*)> {
            fn matches(world: &World<S>, entity: Entity) -> bool {
                false $(|| $name::matches(world, entity))*
            }
        }
    };
}

impl<S: Schema> Filter<S> for () {
    fn matches(_: &World<S>, _: Entity) -> bool {
        true
    }
}

tuples!(L1; A L0 L1);
tuples!(L2; A L0 L1, B L1 L2);
tuples!(L3; A L0 L1, B L1 L2, C L2 L3);
tuples!(L4; A L0 L1, B L1 L2, C L2 L3, D L3 L4);
tuples!(L5; A L0 L1, B L1 L2, C L2 L3, D L3 L4, E L4 L5);
tuples!(L6; A L0 L1, B L1 L2, C L2 L3, D L3 L4, E L4 L5, F L5 L6);
tuples!(L7; A L0 L1, B L1 L2, C L2 L3, D L3 L4, E L4 L5, F L5 L6, G L6 L7);
tuples!(L8; A L0 L1, B L1 L2, C L2 L3, D L3 L4, E L4 L5, F L5 L6, G L6 L7, H L7 L8);

/// Entities that pass `F`, drawn from the shortest list that contains every
/// match of the query (whose own candidates are `query`).
fn select<'w, S: Schema, F: Filter<S>>(
    world: &'w World<S>,
    query: Option<Candidates<'w>>,
) -> impl Iterator<Item = Entity> + 'w {
    let entities = match Candidates::shortest(query, F::candidates(world)) {
        Some(candidates) => candidates.entities,
        None => Box::new(world.entities.iter()),
    };
    entities.filter(move |&entity| F::matches(world, entity))
}

// ------------------------------------------------------------ read queries

/// A read-only query: fetches `Q` for every entity matching `Q` and `F`.
/// Copyable; iterate it as often as you like.
///
/// ```
/// # use weft::*;
/// # struct Pos(i32); impl<S: Schema> Component<S> for Pos {}
/// # struct Frozen; impl<S: Schema> Component<S> for Frozen {}
/// # schema! { struct Game { components: [Pos, Frozen], relations: [ChildOf], resources: [] } }
/// # let world = World::<Game>::new();
/// for (entity, pos) in world.query::<(Entity, &Pos)>().filtered::<Without<Frozen>>() {
///     println!("{entity} is at {}", pos.0);
/// }
/// ```
pub struct Query<'w, S: Schema, Q, F = ()> {
    world: &'w World<S>,
    marker: PhantomData<fn() -> (Q, F)>,
}

impl<S: Schema, Q, F> Clone for Query<'_, S, Q, F> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: Schema, Q, F> Copy for Query<'_, S, Q, F> {}

impl<'w, S: Schema, Q: ReadQuery<S>, F: Filter<S>> Query<'w, S, Q, F> {
    pub(crate) const fn new(world: &'w World<S>) -> Self {
        Self { world, marker: PhantomData }
    }

    /// Adds the filter `G`.
    #[must_use]
    pub const fn filtered<G: Filter<S>>(self) -> Query<'w, S, Q, (F, G)> {
        Query { world: self.world, marker: PhantomData }
    }

    /// Fetches one entity, if it matches.
    #[must_use]
    pub fn get(&self, entity: Entity) -> Option<Q::Item<'w>> {
        if self.world.is_alive(entity) && F::matches(self.world, entity) {
            Q::read(self.world, entity)
        } else {
            None
        }
    }

    /// The only match. `None` if there are zero or several.
    #[must_use]
    pub fn single(&self) -> Option<Q::Item<'w>> {
        let mut iter = self.iter();
        let only = iter.next()?;
        iter.next().is_none().then_some(only)
    }

    /// Whether nothing matches.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }

    /// Iterates the matches.
    #[must_use]
    pub fn iter(&self) -> QueryIter<'w, S, Q> {
        let world = self.world;
        QueryIter { world, entities: Box::new(select::<S, F>(world, Q::candidates(world))), marker: PhantomData }
    }
}

impl<'w, S: Schema, Q: ReadQuery<S>, F: Filter<S>> IntoIterator for Query<'w, S, Q, F> {
    type Item = Q::Item<'w>;
    type IntoIter = QueryIter<'w, S, Q>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over a [`Query`].
pub struct QueryIter<'w, S: Schema, Q> {
    world: &'w World<S>,
    entities: Box<dyn Iterator<Item = Entity> + 'w>,
    marker: PhantomData<fn() -> Q>,
}

impl<'w, S: Schema, Q: ReadQuery<S>> Iterator for QueryIter<'w, S, Q> {
    type Item = Q::Item<'w>;

    fn next(&mut self) -> Option<Self::Item> {
        let world = self.world;
        self.entities.find_map(|entity| Q::read(world, entity))
    }
}

// ---------------------------------------------------------- borrow queries

/// A query with exclusive access: fetches `Q` for every entity matching `Q`
/// and `F`, including `&mut T` terms.
///
/// ```
/// # use weft::*;
/// # struct Pos(i32); impl<S: Schema> Component<S> for Pos {}
/// # struct Vel(i32); impl<S: Schema> Component<S> for Vel {}
/// # schema! { struct Game { components: [mut Pos, Vel], relations: [], resources: [] } }
/// # let mut world = World::<Game>::new();
/// world.query_mut::<(&mut Pos, &Vel)>().for_each(|(mut pos, vel)| pos.0 += vel.0);
/// ```
pub struct QueryMut<'w, S: Schema, Q, F = ()> {
    world: &'w mut World<S>,
    marker: PhantomData<fn() -> (Q, F)>,
}

impl<'w, S, Q, F> QueryMut<'w, S, Q, F>
where
    S: Borrows<Q>,
    Q: QueryData,
    F: Filter<S>,
{
    pub(crate) const fn new(world: &'w mut World<S>) -> Self {
        Self { world, marker: PhantomData }
    }

    /// Adds the filter `G`.
    #[must_use]
    pub fn filtered<G: Filter<S>>(self) -> QueryMut<'w, S, Q, (F, G)> {
        QueryMut { world: self.world, marker: PhantomData }
    }

    /// Calls `f` with each match.
    pub fn for_each(self, mut f: impl FnMut(Q::Item<'_>)) {
        self.for_each_with(|item, _| f(item));
    }

    /// Calls `f` with each match and a [`View`] of everything the query
    /// doesn't borrow mutably: other entities' components, relations,
    /// resources.
    ///
    /// ```
    /// # use weft::*;
    /// # struct Pos(i32); impl<S: Schema> Component<S> for Pos {}
    /// # struct Offset(i32); impl<S: Schema> Component<S> for Offset {}
    /// # schema! { struct Game { components: [mut Pos, Offset], relations: [ChildOf], resources: [] } }
    /// # let mut world = World::<Game>::new();
    /// // Each child moves by its parent's offset.
    /// world.query_mut::<(&mut Pos, Targets<ChildOf>)>().for_each_with(|(mut pos, parents), view| {
    ///     for &parent in parents {
    ///         pos.0 += view.get::<Offset>(parent).map_or(0, |o| o.0);
    ///     }
    /// });
    /// ```
    ///
    /// Reading a component the query borrows mutably doesn't compile:
    ///
    /// ```compile_fail,E0277
    /// # use weft::*;
    /// # struct Pos(i32); impl<S: Schema> Component<S> for Pos {}
    /// # schema! { struct Game { components: [mut Pos], relations: [ChildOf], resources: [] } }
    /// # let mut world = World::<Game>::new();
    /// world.query_mut::<(&mut Pos, Targets<ChildOf>)>().for_each_with(|(mut pos, parents), view| {
    ///     pos.0 += view.get::<Pos>(parents[0]).map_or(0, |p| p.0); // `Pos` is borrowed by the query
    /// });
    /// ```
    pub fn for_each_with(self, mut f: impl FnMut(Q::Item<'_>, &mut View<'w, S, <S as Borrows<Q>>::Rest>)) {
        let selected: Vec<Entity> = select::<S, F>(self.world, S::candidates(self.world)).collect();
        let World { entities, columns, relations, resources, deferred, tick, .. } = self.world;
        let relations: &'w S::Relations = relations;
        let context = Context { relations, tick: *tick };
        let (mut state, rest) = S::claim(Columns::refs(columns), context);
        let mut view = View { rest, entities, relations, resources, deferred };
        for entity in selected {
            if let Some(item) = S::fetch(&mut state, entity) {
                f(item, &mut view);
            }
        }
    }
}

/// Everything a [`QueryMut`] doesn't borrow mutably. See [`QueryMut::for_each_with`].
///
/// Which components are readable is tracked in the type `L`: reading one that
/// the query borrows mutably is a compile error.
pub struct View<'w, S: Schema, L: Loans> {
    rest: L::At<'w>,
    entities: &'w Entities,
    relations: &'w S::Relations,
    resources: &'w mut S::Resources,
    deferred: &'w Cell<Vec<Deferred<S>>>,
}

impl<S: Schema, L: Loans> View<'_, S, L> {
    /// Borrows a component of any entity, if the query doesn't borrow `T`
    /// mutably.
    #[must_use]
    pub fn get<T>(&self, entity: Entity) -> Option<&T>
    where
        S: Reads<T>,
        L: Peek<S::Index, Target = Column<T>>,
        T: 'static,
    {
        L::peek(&self.rest).get(entity)
    }

    /// Whether `entity` is alive.
    #[must_use]
    pub fn is_alive(&self, entity: Entity) -> bool {
        self.entities.is_alive(entity)
    }

    /// What `source` points at through `R`, oldest edge first.
    #[must_use]
    pub fn targets<R>(&self, source: Entity) -> &[Entity]
    where
        S: Links<R>,
        R: Relation<S>,
    {
        S::edges(self.relations).targets(source)
    }

    /// What points at `target` through `R`, oldest edge first.
    #[must_use]
    pub fn sources<R>(&self, target: Entity) -> &[Entity]
    where
        S: Links<R>,
        R: Relation<S>,
    {
        S::edges(self.relations).sources(target)
    }

    /// The single target of an exclusive relation.
    #[must_use]
    pub fn target<R>(&self, source: Entity) -> Option<Entity>
    where
        S: Links<R>,
        R: Relation<S, Arity = One>,
    {
        self.targets::<R>(source).first().copied()
    }

    /// The value on an edge.
    #[must_use]
    pub fn relation<R>(&self, source: Entity, target: Entity) -> Option<&R>
    where
        S: Links<R>,
        R: Relation<S>,
    {
        S::edges(self.relations).get(R::SYMMETRIC, source, target)
    }

    /// Borrows a resource.
    #[must_use]
    pub fn resource<T>(&self) -> &T
    where
        S: Owns<T>,
    {
        S::resource(self.resources)
    }

    /// Mutably borrows a resource.
    pub fn resource_mut<T>(&mut self) -> &mut T
    where
        S: Owns<T>,
    {
        S::resource_mut(self.resources)
    }

    /// Queues a structural change for the next [`World::flush`].
    pub fn defer(&self, work: impl FnOnce(&mut World<S>) + 'static) {
        let mut queue = self.deferred.take();
        queue.push(Box::new(work));
        self.deferred.set(queue);
    }
}
