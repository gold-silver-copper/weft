use core::convert::Infallible;

use crate::{Component, Entity, Links, NoSuchEntity, Reads, Relation, Schema, To, World};

/// What [`World::spawn`] and [`World::insert`] accept: a [`Component`], a
/// relation with its target ([`To`]), or a tuple of bundles.
///
/// A bundle's [`Error`](Bundle::Error) says, in its type, whether inserting it
/// can fail. Components can't fail; a relation fails if its target is dead.
/// So `world.spawn(Pos(0))` returns an [`Entity`], while
/// `world.spawn((Pos(0), To(ChildOf, parent)))` returns
/// `Result<Entity, NoSuchEntity>`.
///
/// The whole bundle, including [required](Component::requires) components,
/// is inserted before any hook runs, so hooks see the complete entity.
///
/// `M` tells the impls for components, relations and tuples apart; it's
/// always inferred.
pub trait Bundle<S: Schema, M>: 'static {
    /// [`Infallible`] if inserting can't fail, [`NoSuchEntity`] otherwise.
    type Error: Outcome;

    #[doc(hidden)]
    fn check(&self, world: &World<S>) -> Result<(), Self::Error>;

    #[doc(hidden)]
    fn apply(self, world: &mut World<S>, entity: Entity);
}

/// The two ways inserting a [`Bundle`] can turn out, as types.
pub trait Outcome: Sized + 'static {
    /// `T` for [`Infallible`], `Result<T, NoSuchEntity>` for [`NoSuchEntity`].
    type Wrap<T>;
    /// The outcome of two bundles together: fallible if either is.
    type Or<B: Outcome>: Outcome;

    #[doc(hidden)]
    fn success<T>(value: T) -> Self::Wrap<T>;
    #[doc(hidden)]
    fn failure<T>(self) -> Self::Wrap<T>;
    #[doc(hidden)]
    fn into_error(self) -> NoSuchEntity;
}

impl Outcome for Infallible {
    type Wrap<T> = T;
    type Or<B: Outcome> = B;

    fn success<T>(value: T) -> T {
        value
    }

    fn failure<T>(self) -> T {
        match self {}
    }

    fn into_error(self) -> NoSuchEntity {
        match self {}
    }
}

impl Outcome for NoSuchEntity {
    type Wrap<T> = Result<T, NoSuchEntity>;
    type Or<B: Outcome> = NoSuchEntity;

    fn success<T>(value: T) -> Result<T, NoSuchEntity> {
        Ok(value)
    }

    fn failure<T>(self) -> Result<T, NoSuchEntity> {
        Err(self)
    }

    fn into_error(self) -> NoSuchEntity {
        self
    }
}

/// Converts a part's error into the error of the tuple containing it.
pub trait Lift<O> {
    /// Does the conversion.
    fn lift(self) -> O;
}

impl<O> Lift<O> for Infallible {
    fn lift(self) -> O {
        match self {}
    }
}

impl Lift<NoSuchEntity> for NoSuchEntity {
    fn lift(self) -> NoSuchEntity {
        self
    }
}

/// Marks the [`Bundle`] impl for components.
pub enum IsComponent {}

/// Marks the [`Bundle`] impl for [`To`].
pub enum IsRelation {}

impl<S, C> Bundle<S, IsComponent> for C
where
    S: Reads<C>,
    C: Component<S>,
{
    type Error = Infallible;

    fn check(&self, _: &World<S>) -> Result<(), Infallible> {
        Ok(())
    }

    fn apply(self, world: &mut World<S>, entity: Entity) {
        world.insert_component(entity, self, true);
    }
}

impl<S, R> Bundle<S, IsRelation> for To<R>
where
    S: Links<R>,
    R: Relation<S>,
{
    type Error = NoSuchEntity;

    fn check(&self, world: &World<S>) -> Result<(), NoSuchEntity> {
        if world.is_alive(self.1) { Ok(()) } else { Err(NoSuchEntity(self.1)) }
    }

    fn apply(self, world: &mut World<S>, entity: Entity) {
        let To(relation, target) = self;
        world.relate_edge(entity, relation, target);
    }
}

macro_rules! or {
    () => { Infallible };
    ($head:ident $(, $tail:ident)*) => { <$head::Error as Outcome>::Or<or!($($tail),*)> };
}

macro_rules! tuple_bundle {
    ($($name:ident $marker:ident),*) => {
        tuple_bundle!(@impl [$($name $marker),*] ($($name),*));
    };
    (@impl [$($name:ident $marker:ident),*] $all:tt) => {
        impl<S: Schema, $($name: Bundle<S, $marker>, $marker),*> Bundle<S, ($($marker,)*)> for ($($name,)*)
        where
            $($name::Error: Lift<tuple_bundle!(@error $all)>,)*
        {
            type Error = or!($($name),*);

            #[allow(non_snake_case, unused_variables)]
            fn check(&self, world: &World<S>) -> Result<(), Self::Error> {
                let ($($name,)*) = self;
                $($name.check(world).map_err(Lift::lift)?;)*
                Ok(())
            }

            #[allow(non_snake_case, unused_variables)]
            fn apply(self, world: &mut World<S>, entity: Entity) {
                let ($($name,)*) = self;
                $($name.apply(world, entity);)*
            }
        }
    };
    (@error ($($all:ident),*)) => { or!($($all),*) };
}

tuple_bundle!();
tuple_bundle!(A MA);
tuple_bundle!(A MA, B MB);
tuple_bundle!(A MA, B MB, C MC);
tuple_bundle!(A MA, B MB, C MC, D MD);
tuple_bundle!(A MA, B MB, C MC, D MD, E ME);
tuple_bundle!(A MA, B MB, C MC, D MD, E ME, F MF);
tuple_bundle!(A MA, B MB, C MC, D MD, E ME, F MF, G MG);
tuple_bundle!(A MA, B MB, C MC, D MD, E ME, F MF, G MG, H MH);
