//! The world: the graph, the components and the root's settings.

use std::fmt;
use std::rc::Rc;

use crate::graph::{self, Audit, Broken, Comps, Graph};
use crate::handle::Erased;
use crate::kind::{Node, Rooted};
use crate::schema::Schema;
use crate::turn::{Genesis, Live, LiveCtx, Turn};

/// A game world: every entity of the game `G`, their edges and components, and the
/// settings.
///
/// Everything happens in turns, through `World::turn`; handles never outlive them. Dropping
/// the world drops every entity it ever held.
pub struct World<G: Schema> {
    shared: G::Shared,
    comps: G::Comps,
    root: Rc<Node<G::Root>>,
    settings: G::Root,
}

impl<G: Schema> World<G> {
    /// Builds the world in its genesis turn, which ends by spawning the root with the
    /// settings and returning what that spawn returns. Genesis refuses nothing and runs no
    /// hooks. If it spawned the root more than once, only the root it returns is kept.
    pub fn new(build: impl for<'t> FnOnce(&mut Turn<'t, G, Genesis>) -> Rooted<'t, G::Root>) -> Self {
        let shared = G::Shared::default();
        let mut comps = G::Comps::default();
        let (root, settings) = {
            let mut t = Turn::<G, Genesis>::new(&shared, &mut comps, ());
            let rooted = build(&mut t);
            (Rc::clone(rooted.id.rc()), rooted.data)
        };
        let mut world = World { shared, comps, root, settings };
        let kept = world.root.addr();
        let extra: Vec<usize> = G::root_list(&world.shared).nodes.iter().map(Erased::addr).filter(|&a| a != kept).collect();
        world.finish(&extra);
        world
    }

    /// Runs one turn and returns what the closure returns. Despawns take effect when it
    /// ends.
    pub fn turn<T>(&mut self, f: impl for<'t> FnOnce(&mut Turn<'t, G>) -> T) -> T {
        let (out, doomed) = {
            let ctx = LiveCtx { root: &self.root, settings: &mut self.settings };
            let mut t = Turn::<G, Live>::new(&self.shared, &mut self.comps, ctx);
            let out = f(&mut t);
            (out, t.doomed)
        };
        self.finish(&doomed);
        out
    }

    /// The end of a turn: what spills moves up, and everything doomed goes, with every edge
    /// that touches it.
    fn finish(&mut self, explicit: &[usize]) {
        if explicit.is_empty() {
            return;
        }
        let plan = graph::plan(&self.shared, explicit);
        self.shared.each(&mut |p| p.spill(&plan));
        self.shared.each_mut(&mut |p| p.sweep(&plan));
        self.comps.sweep(&plan);
    }

    /// Audits the graph: every edge joins two entities in the world, every `one` edge is
    /// carried by exactly its source, no arity is exceeded, and no relation that can't loop
    /// has a loop. The type system should make all of this impossible to break; this is the
    /// check the fuzzer runs after every turn.
    pub fn check(&self) -> Result<(), Broken> {
        let mut audit = Audit::default();
        let mut nodes = Vec::new();
        self.shared.each(&mut |p| p.nodes(&mut nodes));
        audit.nodes = nodes.into_iter().collect();
        self.shared.each(&mut |p| p.audit(&mut audit));
        audit.match_slots();
        audit.broken.into_iter().next().map_or(Ok(()), Err)
    }
}

/// The snapshot: one line per entity (`Kind#serial Data`), the root first, then each kind
/// in `game!` order, oldest first; under each entity, one line per edge it's the source of.
impl<G: Schema> fmt::Display for World<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let edges = |at: usize, f: &mut fmt::Formatter<'_>| -> fmt::Result {
            let mut result = Ok(());
            self.shared.each(&mut |p| {
                if result.is_ok() {
                    result = p.fmt_edges(at, f);
                }
            });
            result
        };
        let mut result = Ok(());
        self.shared.each(&mut |p| {
            if result.is_ok() {
                result = p.fmt_nodes(f, &self.settings, &edges);
            }
        });
        result
    }
}
