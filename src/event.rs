use alloc::rc::Rc;
use alloc::vec::Vec;
use core::any::{Any, TypeId};
use core::cell::Cell;
use core::ops::Deref;

use crate::{Entity, NoSuchEntity, Schema, World};

/// An event triggered for the whole world with [`World::trigger`].
pub trait Event: 'static {}

/// An event triggered on an entity with [`World::trigger_at`].
///
/// After the entity's observers run, the event can move on to another entity,
/// along any relation you like:
///
/// ```
/// # use weft::*;
/// /// Bubbles up the hierarchy until an observer stops it.
/// struct Click;
/// impl<S: Links<ChildOf>> EntityEvent<S> for Click {
///     fn propagate(world: &World<S>, entity: Entity) -> Option<Entity> {
///         world.target::<ChildOf>(entity)
///     }
/// }
/// ```
pub trait EntityEvent<S: Schema>: 'static {
    /// Where the event goes after `entity`. `None` (the default) stops it.
    /// Cycles end propagation.
    fn propagate(world: &World<S>, entity: Entity) -> Option<Entity> {
        let _ = (world, entity);
        None
    }
}

/// What an [`EntityEvent`] observer receives. Derefs to the event.
pub struct On<'a, E> {
    event: &'a E,
    entity: Entity,
    stopped: &'a Cell<bool>,
}

impl<E> On<'_, E> {
    /// The entity the event is currently at.
    #[must_use]
    pub const fn entity(&self) -> Entity {
        self.entity
    }

    /// Doesn't propagate any further. The other observers of the current
    /// entity still run.
    pub fn stop_propagation(&self) {
        self.stopped.set(true);
    }
}

impl<E> Deref for On<'_, E> {
    type Target = E;

    fn deref(&self) -> &E {
        self.event
    }
}

/// Identifies an observer, for [`World::unobserve`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObserverId(u64);

type Callback<S> = Rc<dyn Fn(&mut World<S>, &dyn Any, Option<Entity>, &Cell<bool>)>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Global,
    Entity(Entity),
    AnyEntity,
}

struct Entry<S: Schema> {
    id: ObserverId,
    event: TypeId,
    scope: Scope,
    callback: Callback<S>,
}

pub(crate) struct Observers<S: Schema> {
    entries: Vec<Entry<S>>,
    next: u64,
}

impl<S: Schema> Default for Observers<S> {
    fn default() -> Self {
        Self { entries: Vec::new(), next: 0 }
    }
}

impl<S: Schema> Observers<S> {
    fn add(&mut self, event: TypeId, scope: Scope, callback: Callback<S>) -> ObserverId {
        let id = ObserverId(self.next);
        self.next = self.next.saturating_add(1);
        self.entries.push(Entry { id, event, scope, callback });
        id
    }

    fn remove(&mut self, id: ObserverId) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        self.entries.len() != before
    }

    pub(crate) fn forget(&mut self, entity: Entity) {
        self.entries.retain(|entry| entry.scope != Scope::Entity(entity));
    }

    /// The callbacks to run, cloned so they can run while the world is
    /// mutably borrowed, and even add or remove observers themselves.
    fn matching(&self, event: TypeId, at: Option<Entity>) -> Vec<Callback<S>> {
        let entity_scopes;
        let scopes: &[Scope] = match at {
            Some(entity) => {
                entity_scopes = [Scope::Entity(entity), Scope::AnyEntity];
                &entity_scopes
            }
            None => &[Scope::Global],
        };
        let mut callbacks = Vec::new();
        for &scope in scopes {
            callbacks.extend(
                self.entries
                    .iter()
                    .filter(|entry| entry.event == event && entry.scope == scope)
                    .map(|entry| Rc::clone(&entry.callback)),
            );
        }
        callbacks
    }
}

impl<S: Schema> World<S> {
    /// Runs `observer` every time an `E` is triggered with [`World::trigger`].
    ///
    /// Observers are `Fn`, not `FnMut`: an observer can trigger events that
    /// reach itself again, and a shared `Fn` can be re-entered safely where an
    /// `FnMut` would need a runtime borrow flag.
    pub fn observe<E: Event>(&mut self, observer: impl Fn(&mut Self, &E) + 'static) -> ObserverId {
        let callback: Callback<S> =
            Rc::new(move |world: &mut Self, event: &dyn Any, _: Option<Entity>, _: &Cell<bool>| {
                if let Some(event) = event.downcast_ref::<E>() {
                    observer(world, event);
                }
            });
        self.observers.add(TypeId::of::<E>(), Scope::Global, callback)
    }

    /// Runs `observer` every time an `E` is triggered on, or propagates to,
    /// `entity`. Removed when the entity is despawned.
    ///
    /// # Errors
    /// If `entity` is dead.
    pub fn observe_entity<E: EntityEvent<S>>(
        &mut self,
        entity: Entity,
        observer: impl Fn(&mut Self, On<'_, E>) + 'static,
    ) -> Result<ObserverId, NoSuchEntity> {
        if !self.is_alive(entity) {
            return Err(NoSuchEntity(entity));
        }
        Ok(self.observers.add(TypeId::of::<E>(), Scope::Entity(entity), entity_callback(observer)))
    }

    /// Runs `observer` every time an `E` is triggered on, or propagates to,
    /// any entity. Runs after the entity's own observers.
    pub fn observe_entities<E: EntityEvent<S>>(
        &mut self,
        observer: impl Fn(&mut Self, On<'_, E>) + 'static,
    ) -> ObserverId {
        self.observers.add(TypeId::of::<E>(), Scope::AnyEntity, entity_callback(observer))
    }

    /// Removes an observer. Returns `false` if it was already gone.
    pub fn unobserve(&mut self, id: ObserverId) -> bool {
        self.observers.remove(id)
    }

    /// Runs every observer of `E`, in the order they were added.
    pub fn trigger<E: Event>(&mut self, event: E) {
        let stopped = Cell::new(false);
        for callback in self.observers.matching(TypeId::of::<E>(), None) {
            callback(self, &event, None, &stopped);
        }
    }

    /// Runs the observers of `E` at `entity` (its own, then
    /// [`observe_entities`](World::observe_entities) ones), then moves on to
    /// [`EntityEvent::propagate`]'s answer, until propagation ends or is stopped.
    ///
    /// # Errors
    /// If `entity` is dead.
    pub fn trigger_at<E: EntityEvent<S>>(&mut self, entity: Entity, event: E) -> Result<(), NoSuchEntity> {
        if !self.is_alive(entity) {
            return Err(NoSuchEntity(entity));
        }
        let stopped = Cell::new(false);
        let mut visited = Vec::from([entity]);
        let mut current = entity;
        loop {
            for callback in self.observers.matching(TypeId::of::<E>(), Some(current)) {
                callback(self, &event, Some(current), &stopped);
            }
            if stopped.get() {
                return Ok(());
            }
            match E::propagate(self, current) {
                Some(next) if self.is_alive(next) && !visited.contains(&next) => {
                    visited.push(next);
                    current = next;
                }
                _ => return Ok(()),
            }
        }
    }
}

fn entity_callback<S: Schema, E: 'static>(observer: impl Fn(&mut World<S>, On<'_, E>) + 'static) -> Callback<S> {
    Rc::new(move |world: &mut World<S>, event: &dyn Any, at: Option<Entity>, stopped: &Cell<bool>| {
        if let (Some(event), Some(entity)) = (event.downcast_ref::<E>(), at) {
            observer(world, On { event, entity, stopped });
        }
    })
}
