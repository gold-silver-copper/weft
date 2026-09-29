use alloc::collections::BTreeMap;
use core::fmt;
use core::ops::{Deref, DerefMut};

use crate::query::Candidates;
use crate::{Entity, Reads, Schema, World};

/// Change-detection counter. 64 bits wide and saturating, so it never wraps.
pub type Tick = u64;

/// Data attached to entities.
///
/// A component is generic over the [`Schema`] of the world it lives in, so
/// its hooks can say exactly what else they need from that world. Most
/// components need nothing:
///
/// ```
/// # use weft::*;
/// struct Health(u32);
/// impl<S: Schema> Component<S> for Health {}
/// ```
///
/// A component whose hooks touch `Health` can only be used in schemas that
/// list `Health` (and, to change it, list it as `mut`):
///
/// ```
/// # use weft::*;
/// # struct Health(u32);
/// # impl<S: Schema> Component<S> for Health {}
/// struct Poisoned;
/// impl<S: Writes<Health>> Component<S> for Poisoned {
///     fn requires(requires: &mut Requires<'_, S>) {
///         requires.add(Health(100));
///     }
///
///     fn on_insert(world: &mut World<S>, entity: Entity) {
///         if let Some(mut health) = world.get_mut::<Health>(entity) {
///             health.0 = health.0.saturating_sub(10);
///         }
///     }
/// }
/// ```
pub trait Component<S: Schema>: Sized + 'static {
    /// Adds components that must accompany this one. They are only inserted
    /// if the entity doesn't have them already, and their own requirements
    /// are followed too.
    fn requires(requires: &mut Requires<'_, S>) {
        let _ = requires;
    }

    /// Runs after the component is inserted or replaced, once the whole
    /// bundle is in place.
    fn on_insert(world: &mut World<S>, entity: Entity) {
        let _ = (world, entity);
    }

    /// Runs before the component is removed (including on despawn), while
    /// the value can still be read.
    fn on_remove(world: &mut World<S>, entity: Entity) {
        let _ = (world, entity);
    }
}

/// Collects the components required by a [`Component`]. See [`Component::requires`].
pub struct Requires<'a, S: Schema> {
    pub(crate) world: &'a mut World<S>,
    pub(crate) entity: Entity,
}

impl<S: Schema> Requires<'_, S> {
    /// The entity the requirements are for.
    #[must_use]
    pub const fn entity(&self) -> Entity {
        self.entity
    }

    /// Inserts `component` unless the entity already has one.
    pub fn add<C>(&mut self, component: C) -> &mut Self
    where
        S: Reads<C>,
        C: Component<S>,
    {
        self.world.insert_component(self.entity, component, false);
        self
    }
}

struct Slot<T> {
    value: T,
    added: Tick,
    changed: Tick,
}

/// All values of one component type, ordered by entity.
pub struct Column<T> {
    slots: BTreeMap<Entity, Slot<T>>,
}

impl<T> Default for Column<T> {
    fn default() -> Self {
        Self { slots: BTreeMap::new() }
    }
}

impl<T> Column<T> {
    pub(crate) fn insert(&mut self, entity: Entity, value: T, tick: Tick) {
        match self.slots.get_mut(&entity) {
            Some(slot) => {
                slot.value = value;
                slot.changed = tick;
            }
            None => {
                self.slots.insert(entity, Slot { value, added: tick, changed: tick });
            }
        }
    }

    pub(crate) fn remove(&mut self, entity: Entity) -> Option<T> {
        self.slots.remove(&entity).map(|slot| slot.value)
    }

    pub(crate) fn contains(&self, entity: Entity) -> bool {
        self.slots.contains_key(&entity)
    }

    pub(crate) fn get(&self, entity: Entity) -> Option<&T> {
        self.slots.get(&entity).map(|slot| &slot.value)
    }

    pub(crate) fn get_mut(&mut self, entity: Entity, tick: Tick) -> Option<Mut<'_, T>> {
        let Slot { value, changed, .. } = self.slots.get_mut(&entity)?;
        Some(Mut { value, changed, tick })
    }

    pub(crate) fn added(&self, entity: Entity) -> Option<Tick> {
        self.slots.get(&entity).map(|slot| slot.added)
    }

    pub(crate) fn changed(&self, entity: Entity) -> Option<Tick> {
        self.slots.get(&entity).map(|slot| slot.changed)
    }

    pub(crate) fn candidates(&self) -> Candidates<'_> {
        Candidates::new(self.slots.len(), self.slots.keys().copied())
    }
}

/// Mutable access to a component. Writing through it (not reading) marks the
/// component as changed.
pub struct Mut<'a, T> {
    value: &'a mut T,
    changed: &'a mut Tick,
    tick: Tick,
}

impl<T> Mut<'_, T> {
    /// Mutates without marking the component as changed.
    pub fn bypass_change_detection(&mut self) -> &mut T {
        self.value
    }
}

impl<T> Deref for Mut<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value
    }
}

impl<T> DerefMut for Mut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        *self.changed = self.tick;
        self.value
    }
}

impl<T: fmt::Debug> fmt::Debug for Mut<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.value, f)
    }
}
