//! The upstream crate for `module_from_another_crate`: a reusable containers module.
//!
//! One kind (`Bag`) joins the module's `Holder` set here, upstream. The downstream game adds
//! its own `Room` to the same set.
#![feature(decl_macro)]
use weft::*;

#[derive(Debug)]
pub struct Item {
    pub name: &'static str,
}
kind!(Item needs In);

#[derive(Debug)]
pub struct Bag {
    pub name: &'static str,
}
kind!(Bag);

set!(pub Holder);
is!(Bag: Holder); // set membership declared upstream

relation!(pub struct In: Item -> Holder, one, cascade);
component!(pub struct Weight(pub u32): Item);

module!(pub Containers { kinds: [Item, Bag], relations: [In], components: [Weight] });

/// Total weight of everything directly inside `holder`.
pub fn load<'t, G: Containers>(t: &Turn<'t, G>, holder: Id<'t, Holder>) -> u32 {
    t.sources::<In>(holder)
        .filter_map(|(item, _)| t.get::<Weight>(item))
        .fold(0, |sum, w| sum.saturating_add(w.0))
}
