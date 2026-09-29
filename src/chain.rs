//! An append-only list that grows through `&self`.
//!
//! A turn borrows the world's storage shared for its whole lifetime `'t` and still spawns,
//! moves and relates. Everything those operations add goes onto a `Chain`, whose links are
//! `OnceCell`s: a link, once written, never moves or changes, so a reference to it lives as
//! long as the chain is borrowed. That is what makes `t[id]` and `target` plain field reads.
//! Between turns the world has `&mut` access again and compacts chains with `retain`.

use std::cell::OnceCell;

pub(crate) struct Chain<T> {
    head: OnceCell<Box<Link<T>>>,
}

struct Link<T> {
    value: T,
    next: OnceCell<Box<Link<T>>>,
}

impl<T> Default for Chain<T> {
    fn default() -> Self {
        Chain { head: OnceCell::new() }
    }
}

impl<T> Chain<T> {
    /// Appends `value` and returns it, borrowed for as long as the chain is.
    pub(crate) fn push(&self, value: T) -> &T {
        let mut cell = &self.head;
        while let Some(link) = cell.get() {
            cell = &link.next;
        }
        // `cell` was just seen empty, and nothing runs before it is filled: the closure runs,
        // and the reference returned is to `value`.
        &cell.get_or_init(|| Box::new(Link { value, next: OnceCell::new() })).value
    }

    pub(crate) fn iter(&self) -> Iter<'_, T> {
        Iter { cell: &self.head }
    }

    pub(crate) fn last(&self) -> Option<&T> {
        self.iter().last()
    }

    pub(crate) fn len(&self) -> usize {
        self.iter().count()
    }

    /// Keeps the values `keep` accepts, in order.
    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        let mut kept = Vec::new();
        let mut next = self.head.take();
        while let Some(mut link) = next {
            next = link.next.take();
            if keep(&link.value) {
                kept.push(link.value);
            }
        }
        let mut head = OnceCell::new();
        for value in kept.into_iter().rev() {
            head = OnceCell::from(Box::new(Link { value, next: head }));
        }
        self.head = head;
    }
}

/// Unlinks one link at a time, so a long chain doesn't drop recursively.
impl<T> Drop for Chain<T> {
    fn drop(&mut self) {
        let mut next = self.head.take();
        while let Some(mut link) = next {
            next = link.next.take();
        }
    }
}

pub(crate) struct Iter<'a, T> {
    cell: &'a OnceCell<Box<Link<T>>>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        let link = self.cell.get()?;
        self.cell = &link.next;
        Some(&link.value)
    }
}
