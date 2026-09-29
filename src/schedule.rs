use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::component::Tick;
use crate::{Schema, World};

type System<S> = Box<dyn FnMut(&mut World<S>)>;

/// Systems, run in the order they were added.
///
/// A system is an `FnMut(&mut World<S>)`: the schedule owns it and runs it
/// alone, so it may keep state between runs in whatever it captures. Each
/// system gets its own change detection ([`Changed`](crate::Changed) means
/// "since this system last ran"), and deferred work ([`World::defer`]) is
/// applied after every system.
///
/// ```
/// # use weft::*;
/// # struct Pos(i32); impl<S: Schema> Component<S> for Pos {}
/// # schema! { struct Game { components: [mut Pos], relations: [], resources: [] } }
/// # let mut world = World::<Game>::new();
/// let mut frame = 0;
/// let mut schedule = Schedule::new();
/// schedule
///     .add(move |world: &mut World<Game>| {
///         frame += 1;
///         world.query_mut::<&mut Pos>().for_each(|mut pos| pos.0 += frame);
///     })
///     .add(|world: &mut World<Game>| {
///         for pos in world.query::<&Pos>().filtered::<Changed<Pos>>() {
///             println!("moved to {}", pos.0);
///         }
///     });
/// schedule.run(&mut world);
/// ```
pub struct Schedule<S: Schema> {
    systems: Vec<(System<S>, Tick)>,
}

impl<S: Schema> Default for Schedule<S> {
    fn default() -> Self {
        Self { systems: Vec::new() }
    }
}

impl<S: Schema> Schedule<S> {
    /// An empty schedule.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a system to the end.
    pub fn add(&mut self, system: impl FnMut(&mut World<S>) + 'static) -> &mut Self {
        self.systems.push((Box::new(system), 0));
        self
    }

    /// Runs every system once, in order.
    pub fn run(&mut self, world: &mut World<S>) {
        for (system, last_run) in &mut self.systems {
            *last_run = world.run_tracked(*last_run, system);
        }
    }
}
