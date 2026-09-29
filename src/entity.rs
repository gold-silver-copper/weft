use alloc::vec::Vec;
use core::fmt;

/// A handle to an entity.
///
/// Handles are generational: once an entity is despawned, its handle is never
/// alive again, even if its slot is reused.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Entity {
    index: usize,
    generation: u32,
}

impl Entity {
    /// The slot this entity occupies. Unique among living entities.
    #[must_use]
    pub const fn index(self) -> usize {
        self.index
    }

    /// How many entities used this slot before this one.
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

impl fmt::Debug for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}v{}", self.index, self.generation)
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// The error for operations on an entity that doesn't exist (anymore).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoSuchEntity(pub Entity);

impl fmt::Display for NoSuchEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "entity {} does not exist", self.0)
    }
}

impl core::error::Error for NoSuchEntity {}

#[derive(Clone, Copy)]
struct Slot {
    generation: u32,
    alive: bool,
}

/// Allocates entity handles and tracks which are alive.
#[derive(Default)]
pub(crate) struct Entities {
    slots: Vec<Slot>,
    free: Vec<usize>,
    alive: usize,
}

impl Entities {
    pub(crate) fn alloc(&mut self) -> Entity {
        self.alive = self.alive.saturating_add(1);
        while let Some(index) = self.free.pop() {
            if let Some(slot) = self.slots.get_mut(index) {
                slot.alive = true;
                return Entity { index, generation: slot.generation };
            }
        }
        let index = self.slots.len();
        self.slots.push(Slot { generation: 0, alive: true });
        Entity { index, generation: 0 }
    }

    pub(crate) fn free(&mut self, entity: Entity) -> bool {
        let Some(slot) = self.slots.get_mut(entity.index) else { return false };
        if !slot.alive || slot.generation != entity.generation {
            return false;
        }
        slot.alive = false;
        self.alive = self.alive.saturating_sub(1);
        // A slot whose generation would wrap is retired rather than reused,
        // so a stale handle can never come back to life.
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            self.free.push(entity.index);
        }
        true
    }

    pub(crate) fn is_alive(&self, entity: Entity) -> bool {
        self.slots
            .get(entity.index)
            .is_some_and(|slot| slot.alive && slot.generation == entity.generation)
    }

    pub(crate) fn len(&self) -> usize {
        self.alive
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.alive)
            .map(|(index, slot)| Entity { index, generation: slot.generation })
    }
}
