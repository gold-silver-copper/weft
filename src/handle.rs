//! Handles: `Id<'t, X>`, branded to the turn that handed it out.

use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::kind::{Kind, Node};

/// A node seen without its kind: what a set handle points at.
#[doc(hidden)]
pub trait Erased: 'static {
    /// The node's address: its identity for as long as the world holds it.
    fn addr(&self) -> usize;
    /// Another owner of the same node.
    fn boxed(&self) -> Box<Dyn>;
    /// `Kind#serial`, as the snapshot prints it.
    fn label(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

#[doc(hidden)]
pub type Dyn = dyn Erased + 'static;

impl<K: Kind> Erased for Rc<Node<K>> {
    fn addr(&self) -> usize {
        Rc::as_ptr(self).cast::<()>().addr()
    }

    fn boxed(&self) -> Box<Dyn> {
        Box::new(Rc::clone(self))
    }

    fn label(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        K::write_name(f)?;
        write!(f, "#{}", self.serial)
    }
}

/// What an `Id<'t, X>` holds, and what an edge stores for an endpoint of type `X`: a kind or
/// a set.
pub trait Handle: 'static {
    #[doc(hidden)]
    type Owned: 'static;
    #[doc(hidden)]
    type Ref<'t>: Copy;
    #[doc(hidden)]
    fn borrow<'t>(owned: &'t Self::Owned) -> Self::Ref<'t>;
    #[doc(hidden)]
    fn erase<'t>(r: Self::Ref<'t>) -> &'t Dyn;
    #[doc(hidden)]
    fn dup(owned: &Self::Owned) -> Self::Owned;
}

impl<K: Kind> Handle for K {
    type Owned = Rc<Node<K>>;
    type Ref<'t> = &'t Rc<Node<K>>;

    fn borrow(owned: &Rc<Node<K>>) -> &Rc<Node<K>> {
        owned
    }

    fn erase<'t>(r: Self::Ref<'t>) -> &'t Dyn {
        r
    }

    fn dup(owned: &Rc<Node<K>>) -> Rc<Node<K>> {
        Rc::clone(owned)
    }
}

/// An open set of kinds, declared with `set!`. `is!` puts kinds in it.
pub trait Set: Handle {}

/// `Self` is `S`, or a kind in the set `S`. Every kind and set is a member of itself;
/// `is!` adds kinds to sets.
#[diagnostic::on_unimplemented(
    message = "`{Self}` isn't a `{S}`",
    label = "expected a `{S}`",
    note = "an edge's ends must be of the relation's source and target kinds or sets; `is!` puts a kind in a set"
)]
pub trait Member<S: Handle>: Handle {
    #[doc(hidden)]
    fn up<'t>(r: Self::Ref<'t>) -> S::Ref<'t>;
    #[doc(hidden)]
    fn own(r: Self::Ref<'_>) -> S::Owned;
}

impl<K: Kind> Member<K> for K {
    fn up<'t>(r: Self::Ref<'t>) -> <K as Handle>::Ref<'t> {
        r
    }

    fn own(r: Self::Ref<'_>) -> Rc<Node<K>> {
        Rc::clone(r)
    }
}

/// A handle to an entity of kind `X`, or to an entity whose kind is in the set `X`.
///
/// Its lifetime is the turn that handed it out, and it's invariant, so it can't leave that
/// turn or enter another world's: if a handle exists, its entity exists. It's `Copy`, and two
/// handles are equal when they point at the same entity, whatever their types.
pub struct Id<'t, X: Handle> {
    pub(crate) r: X::Ref<'t>,
    brand: PhantomData<fn(&'t ()) -> &'t ()>,
}

impl<'t, X: Handle> Id<'t, X> {
    pub(crate) fn new(r: X::Ref<'t>) -> Self {
        Id { r, brand: PhantomData }
    }

    pub(crate) fn addr(self) -> usize {
        X::erase(self.r).addr()
    }

    pub(crate) fn erased(self) -> &'t Dyn {
        X::erase(self.r)
    }

    /// The same entity, seen as a member of the set `S`.
    #[must_use]
    pub fn up<S: Handle>(self) -> Id<'t, S>
    where
        X: Member<S>,
    {
        Id::new(X::up(self.r))
    }
}

impl<'t, K: Kind> Id<'t, K> {
    pub(crate) fn node(self) -> &'t Node<K> {
        self.r
    }

    pub(crate) fn rc(self) -> &'t Rc<Node<K>> {
        self.r
    }
}

impl<X: Handle> Clone for Id<'_, X> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<X: Handle> Copy for Id<'_, X> {}

impl<'t, X: Handle, Y: Handle> PartialEq<Id<'t, Y>> for Id<'t, X> {
    fn eq(&self, other: &Id<'t, Y>) -> bool {
        self.addr() == other.addr()
    }
}

impl<X: Handle> Eq for Id<'_, X> {}

impl<X: Handle> std::hash::Hash for Id<'_, X> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.addr().hash(state);
    }
}

impl<X: Handle> fmt::Display for Id<'_, X> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.erased().label(f)
    }
}

impl<X: Handle> fmt::Debug for Id<'_, X> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.erased().label(f)
    }
}

/// A handle usable where an `Id<'t, S>` is expected: any handle whose kind or set is a
/// member of `S`.
pub trait Is<'t, S: Handle>: Copy {
    #[doc(hidden)]
    fn to(self) -> Id<'t, S>;
}

impl<'t, S: Handle, X: Member<S>> Is<'t, S> for Id<'t, X> {
    fn to(self) -> Id<'t, S> {
        Id::new(X::up(self.r))
    }
}

/// A set handle that `K` belongs to, which `cast` can narrow to a `K`.
#[diagnostic::on_unimplemented(
    message = "`{K}` isn't in the set this handle points into",
    label = "can't be narrowed to `{K}`",
    note = "`cast` narrows a set handle to one of the set's kinds"
)]
pub trait Narrow<'t, K>: Copy {
    #[doc(hidden)]
    fn addr(self) -> usize;
    #[doc(hidden)]
    fn erased(self) -> &'t Dyn;
}

impl<'t, S: Handle, K: Member<S>> Narrow<'t, K> for Id<'t, S> {
    fn addr(self) -> usize {
        Id::addr(self)
    }

    fn erased(self) -> &'t Dyn {
        Id::erased(self)
    }
}

/// `Kind#serial` for the planted `cast_by_string` bug, which compares labels as text.
#[cfg(weft_mutant = "cast_by_string")]
pub(crate) struct Label<'a>(pub(crate) &'a Dyn);

#[cfg(weft_mutant = "cast_by_string")]
impl fmt::Display for Label<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.label(f)
    }
}
