//! Components: `#[component]`, per-entity state that changes.

use std::fmt;

use crate::graph::Table;
use crate::kind::Kind;
use crate::schema::{Owned, Part};

/// Changeable state an entity of one kind may have, declared with `#[component]`. A turn
/// reads it with `get` and changes it with `get_mut`, `insert` and `remove`.
pub trait Component: Part<Side = Owned, Store = Table<Self>> + fmt::Debug + Sized + 'static {
    /// The kind that can have it.
    type Kind: Kind;
}
