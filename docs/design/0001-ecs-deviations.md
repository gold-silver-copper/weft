# Deviations

## D1: The toolchain pin is named `nightly-2026-09-21`
- Kind: deviation
- Files: rust-toolchain.toml
- Pins: rust-toolchain.toml fb4a4c2e7f2b0acf
- Claims: -
- Why: M0 asks for "the dated nightly c's spike ran: 2026-09-20, rustc 1.100.0-nightly". That compiler is rustc `bba531001 2026-09-20`, which rustup publishes as `nightly-2026-09-21` (a nightly is dated the day after its commit). Pinning `nightly-2026-09-20` would pin the build before it. The pin also adds `rust-src`, which the pinned rust-analyzer needs to load the sysroot for C14.
- Evidence: rustup's channel manifest for the date, and the installed compiler.

```text
$ rustup toolchain install nightly-2026-09-21 --profile minimal --component clippy --component rust-analyzer --component rust-src
info: latest update on 2026-09-21 for version 1.100.0-nightly (bba531001 2026-09-20)
  nightly-2026-09-21-aarch64-apple-darwin installed - rustc 1.100.0-nightly (bba531001 2026-09-20)
$ grep date ~/.rustup/toolchains/nightly-aarch64-apple-darwin/lib/rustlib/multirust-channel-manifest.toml   # the toolchain c's spike ran
date = "2026-09-21"
```

## D2: Two reject cases rejected the right thing at the wrong line
- Kind: deviation
- Files: tests/reject/src/bin/handle_enters_other_turn.rs, tests/reject/src/bin/caller_lacks_hook_bound.rs
- Pins: tests/reject/src/bin/handle_enters_other_turn.rs 3f1e57909b2f4aa3, tests/reject/src/bin/caller_lacks_hook_bound.rs 602f3b42793094de
- Claims: C1, C7
- Why: Both cases were written in M0, before the library existed. In `handle_enters_other_turn`, rustc reports the foreign handle where it is bound inside the other turn's closure (E0521), not where it is indexed, so the misuse now sits on one line: `lamp.map(|lamp| u[lamp].name)`, twinned with the same read through the handle's own turn. In `caller_lacks_hook_bound`, `main` matched on two iterators that borrow the turn and then called `walk` in an arm, an E0502 of the case's own making; it now binds the two handles first. Neither case's invalid state or twin changed meaning.
- Evidence: the harness's report before the change.

```text
3 of 21 reject cases are wrong:
caller_lacks_hook_bound: fails at Some(("src/bin/caller_lacks_hook_bound.rs", 58, 58)), not only at its marker (line 44): cannot borrow `*t` as mutable because it is also borrowed as immutable
handle_enters_other_turn: fails at Some(("src/bin/handle_enters_other_turn.rs", 10, 10)), not only at its marker (line 12): borrowed data escapes outside of closure
```

## D3: Milestones M1–M5 land as one library commit
- Kind: deviation
- Files: -
- Pins: -
- Claims: -
- Why: The gates written in M0 can't turn a claim green before most of the API exists. The reject cases, the P5 doctests and the program runner all use P5's `game` crate, which needs sets, `ranked`, two-way and `many` relations; `tests/behaviour/claims.rs` needs sets, `cast` and ranks; the fuzzer needs hooks. So M1–M4 would each have turned no claim green, which the milestone rule forbids, and splitting the library into commits that raise nothing would have been bookkeeping, not checkpoints. The library was built in the milestones' order and lands in one commit; each milestone's own verification items were run on it and are listed in the commit.
- Evidence: every claim green on the first commit that could run them all, except C14 (see D5).

## D4: Semantics the record left open
- Kind: deviation
- Files: -
- Pins: -
- Claims: C3, C8, C17
- Why: Decisions the design needed and the record doesn't make, each checked by the fuzzer's model:
  (1) `spill` moves a source up to the nearest holder that survives, walking the same relation; with none, the source goes too, so `is_doomed` and despawn's `Held` check count it.
  (2) Hook chains get a depth limit of 64 and a budget of 16,384 hooks per top-level operation; past either, the next hooked `relate` or `spawn` is refused with a new variant, `Refused::Unsettled`, before it changes anything (M5 said to log a new variant).
  (3) A live `spawn` is refusable, as a type, exactly when one of its needed relations has a hook or is `refuse`: a `refuse` edge into a doomed entity is refused with `Doomed`; other policies spawn into a doomed holder and follow it.
  (4) `<->` relations must be `many`: a two-way `one` or `opt` would need both ends replaced at once.
  (5) C17 needs the `one` edges between nodes to form no ownership loop, because nodes share each other through `Rc`: `game!` rejects `one` relations that could let a kind own itself, directly or through other kinds, unless the loop is `ranked` with rising ranks. A game has at most 128 kinds for that check.
- Evidence: `tests/fuzz.rs` `differential` (60 worlds x 30 turns: 6817 ops applied, 403 refused, 348 of them held despawns), `hooks_settle`, and `tests/behaviour/claims.rs` `drop_frees_everything`, all passing.

## D5: Ranks are types (Peano), not const generics
- Kind: deviation
- Files: examples/p3.rs, game/src/lib.rs, examples/p6.rs, examples/p7.rs, tests/fuzz.rs, tests/behaviour/claims.rs, tests/reject/src/lib.rs, tests/reject/src/bin/chest_in_itself.rs, tests/reject/src/bin/chest_in_its_contents.rs
- Pins: examples/p3.rs 792e402492196dc1, game/src/lib.rs 0e18e55d88db0119, examples/p6.rs febe35b435faff26, examples/p7.rs c165775c0551d50b, tests/fuzz.rs 4e686c22e325bb12, tests/behaviour/claims.rs bfa1f4fe7a3932d4, tests/reject/src/lib.rs 1a0c140d0653e2e6, tests/reject/src/bin/chest_in_itself.rs e679a1f89993fe12, tests/reject/src/bin/chest_in_its_contents.rs b2fa1ae5ad67e56d
- Claims: C5, C14, C15, C16
- Why: C14 was red: the pinned rust-analyzer reported `??ty: 233`, and every one of them was a const-generic argument literal, the `1` in `Chest::<1>` and in every `Chest<1>` that `game!` expands. rust-analyzer doesn't type literal const arguments at all: a four-line program with no weft and no features reproduces it, on the pinned build and on stable 1.98.1. No spelling of the frozen programs that kept `Chest<const R: u8>` could reach `??ty: 0`. So ranks moved to types, as the prompt allows in advance: `Zero`, `Up<R>`, and `Top` for unranked kinds, with `R0`–`R15` naming the first sixteen as types and values; `ranked` compares them with a trait, `Below`, whose error still names both kinds and says `the relation is ranked, and ranks must rise`. weft no longer enables `generic_const_exprs`, only `decl_macro`, which also retires the record's toolchain risk (the feature's fall-back to the old trait solver) and makes rank errors trait errors that rust-analyzer shows natively, where before they showed only under `cargo check`. The cost is in the programs: a rank-generic kind holds its rank as a zero-sized field, so `Chest<const R: u8>` became `Chest<R>` with `pub rank: R`, `Chest::<2> { name }` became `Chest { name, rank: R2 }`, `Chest<1>` became `Chest<R1>`, and `const R: u8` in a signature became `R: Rank`. Numeric `rank 0` stays. The same rewrite applies to the gates that spell ranks, and `lint_wall_is_intact` now allows only `#![feature(decl_macro)]`.
- Evidence: rust-analyzer before and after, and the reproduction.

```text
before:  exprs: 31256, ??ty: 233 (0%), ?ty: 0 (0%)   every unknown at a const argument, e.g. examples/p3.rs 74:34 (the 2 in Chest::<2>)
after:   exprs: 31062, ??ty: 0 (0%), ?ty: 0 (0%)
         pats: 4875, ??ty: 0 (0%), ?ty: 0 (0%)
repro (no dependencies):  pub struct Chest<const R: u8> { pub id: u32 }  fn main() { let a = Chest::<1> { id: 3 }; let _ = a.id; }
  rust-analyzer 1.100.0-nightly (bba531001 2026-09-20):  exprs: 6, ??ty: 1
  rust-analyzer 1.98.1 (48a229ce 2026-09-01):            exprs: 6, ??ty: 1
```
