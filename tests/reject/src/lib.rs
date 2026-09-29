//! Invalid states that must not compile, one per `src/bin/<case>.rs`.
//!
//! `tests/cases.rs` runs `cargo check` on this package and requires every case to fail at
//! the line that ends with `// ~ERROR`, optionally followed by the error code and a quoted
//! fragment of the message: `// ~ERROR E0277 "ranks must rise"`. Every error of a case
//! must be on that line, so a case can't pass by failing somewhere else.
//!
//! Each case also has a twin: its `#[cfg(twin)]` lines (or `#[cfg_attr(twin, ..)]` attributes)
//! replace its `#[cfg(not(twin))]` ones with the nearest legal program. `cargo check` with `--cfg twin` must pass, so a case
//! can't pass because the API it uses doesn't exist.
//!
//! The doctest below is the wrong-code canary. Putting a chest inside itself is E0277, not
//! E0308, so this doctest must fail; `tests/cases.rs` checks that it does. If it passed,
//! rustdoc wouldn't be checking the codes that P5's doctests state.
//!
//! ```compile_fail,E0308
//! use game::*;
//! weft::World::<Game>::new(|t| {
//!     let hall = t.spawn(Room { name: "hall" }, ());
//!     let chest = t.spawn(Chest { name: "chest", rank: weft::R1 }, (In, hall));
//!     t.relate(chest, In, chest);
//!     t.spawn(Settings, ())
//! });
//! ```
