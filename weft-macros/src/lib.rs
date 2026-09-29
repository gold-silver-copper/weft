//! The declaration macros of [weft](https://docs.rs/weft): `#[kind]`, `#[relation]`,
//! `#[component]`, `module!` and `game!`. Use them through `weft`, which re-exports them.
//!
//! They expand to paths under `::weft`, so the crate that uses them must depend on `weft`
//! under that name.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
// `deny`, not `forbid`: darling's `#[derive(FromMeta)]` generates code that trips
// `clippy::unreachable`, and only a local `allow` on that one struct can admit it. A panic in
// a proc macro is a compile error, never a runtime one.
#![deny(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::string_slice,
    clippy::get_unwrap,
    clippy::unwrap_in_result,
    clippy::panic_in_result_fn,
    clippy::exit
)]

use proc_macro::TokenStream;

mod game;
mod kind;
mod relation;

/// `items` joined by `separator`, for interpolating a list into `quote!` without its
/// repetition syntax, which rust-analyzer can't type.
fn join<T: quote::ToTokens>(items: impl IntoIterator<Item = T>, separator: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let mut out = proc_macro2::TokenStream::new();
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            out.extend(separator.clone());
        }
        item.to_tokens(&mut out);
    }
    out
}

/// `items` each followed by `separator`: `A, B, C,`.
fn each<T: quote::ToTokens>(items: impl IntoIterator<Item = T>, separator: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let mut out = proc_macro2::TokenStream::new();
    for item in items {
        item.to_tokens(&mut out);
        out.extend(separator.clone());
    }
    out
}

/// Declares an entity kind.
///
/// ```text
/// #[kind]                                   // any number per world, rank `Top`
/// pub struct Room { pub name: &'static str }
///
/// #[kind(needs = [In], rank = 0, is = [Thing])]
/// pub struct Item { pub name: &'static str }
///
/// #[kind(needs = [In], rank = R, is = [Thing, Holder])]
/// pub struct Chest<R> { pub name: &'static str, pub rank: R }
///
/// #[kind(root, needs = [Start, Plays])]     // one per world; its data is the settings
/// pub struct Settings;
/// ```
///
/// - `needs`: the kind's `one` relations, in the order `spawn` takes their edges (up to 8).
///   Every kind that can be the source of a `one` relation must need it; `game!` checks.
/// - `rank`: a number, a rank type such as `R3`, or the struct's own type parameter, whose
///   value the struct then holds (`Chest { name: "box", rank: R1 }`). `Top` if left out.
/// - `is`: sets the kind belongs to. `is!` does the same from anywhere else.
/// - `root`: the game's root kind.
///
/// The struct gets `#[derive(Debug)]` unless it already derives it.
#[proc_macro_attribute]
pub fn kind(attr: TokenStream, item: TokenStream) -> TokenStream {
    keep_item(kind::expand(attr.into(), item.clone().into()), item)
}

/// Declares a relation, with every rule it keeps.
///
/// ```text
/// #[relation(Thing -> Holder, one, ranked, spill)]
/// pub struct In;
///
/// #[relation(Room <-> Room, many, cyclic, detach)]
/// pub struct Exit(pub &'static str);
///
/// #[relation(Walker -> Room, one, refuse, on_relate = arrive: Map)]
/// pub struct At;
/// ```
///
/// - `->` or `<->`: one-way, or two-way with a value for each direction (`many` only).
/// - arity: `one`, `opt` or `many`.
/// - loop rule, required if the endpoints can overlap: `ranked` or `cyclic`.
/// - despawn policy: `cascade`, `detach` (not for `one`), `spill` (`one` only) or `refuse`.
/// - `on_relate = f: Bound`: `f` runs whenever a live turn makes an edge, and may use anything
///   `Bound` gives (`Map + Folk` for more than one). Every caller relating this relation must
///   meet `Bound`.
///
/// The options come in any order. The struct gets `#[derive(Debug)]` unless it derives it.
#[proc_macro_attribute]
pub fn relation(attr: TokenStream, item: TokenStream) -> TokenStream {
    keep_item(relation::expand(attr.into(), item.clone().into()), item)
}

/// Declares a component: changeable state for entities of one kind.
///
/// ```text
/// #[component(Item)]
/// pub struct Lit(pub bool);
/// ```
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    keep_item(kind::component(attr.into(), item.clone().into()), item)
}

/// Declares a module: a named group of kinds, relations and components that games include.
///
/// ```text
/// module!(pub Map { kinds: [Room, Player], relations: [Exit, At], components: [Lit] });
/// ```
///
/// It defines a trait, `Map`, that every game including the module meets, so `G: Map` gives
/// generic code all of the module's parts; and a macro of the same name that `game!` calls by
/// path, so a module from another crate drops into a game. Crates that declare modules enable
/// `#![feature(decl_macro)]`.
#[proc_macro]
pub fn module(input: TokenStream) -> TokenStream {
    game::module(input.into()).unwrap_or_else(syn::Error::into_compile_error).into()
}

/// Declares a game: a root kind and the modules it includes.
///
/// ```text
/// game!(pub Game: Settings [Map, containers::Containers]);
/// ```
///
/// Under `cargo check`, it also rejects declarations that could break the graph: a kind that
/// can be the source of a `one` relation but doesn't need it; a relation with no loop rule
/// whose endpoints overlap; a `cascade` or `spill` relation the root can be the source of;
/// and `one` relations through which an entity could own itself.
#[proc_macro]
pub fn game(input: TokenStream) -> TokenStream {
    game::game(input.into()).unwrap_or_else(syn::Error::into_compile_error).into()
}

/// The step `game!` and every module's macro take in turn. Not part of the API.
#[doc(hidden)]
#[proc_macro]
pub fn __game_step(input: TokenStream) -> TokenStream {
    game::step(input.into()).unwrap_or_else(syn::Error::into_compile_error).into()
}

/// On an error, the struct is still emitted, so the error isn't buried under "not found" ones.
fn keep_item(result: syn::Result<proc_macro2::TokenStream>, item: TokenStream) -> TokenStream {
    match result {
        Ok(tokens) => tokens.into(),
        Err(error) => {
            let mut tokens = error.into_compile_error();
            tokens.extend(proc_macro2::TokenStream::from(item));
            tokens.into()
        }
    }
}
