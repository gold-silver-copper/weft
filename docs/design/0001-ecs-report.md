# Report: executing `improve-ecs`

The run that built weft against `docs/design/0002-improve-ecs.md`, on branch `improve-ecs` from `af02049`. It finished without stopping. Every claim is green; no claim is a shortfall.

## Claims

Checked on the final tree, cold (`cargo clean` first): `cargo test --workspace` passed every test binary and doctest in 30 s including the build; both clippy runs passed; rust-analyzer as below.

| id | claim | status | gates, as run |
|---|---|---|---|
| C1 | no handle leaves its turn or enters another world's | green | `handle_leaves_turn`, `handle_enters_other_turn` fail at their markers; twins compile |
| C2 | an item is always in exactly one container | green | `item_in_no_container` (E0271), `item_in_two_containers` (E0271), `unrelate_one` (E0277); `fuzz::differential` |
| C3 | no dangling edge; every op leaves a correct graph, hooks that refuse partway included | green | `fuzz::differential`; `purge_keeps_edges` killed; P3 runs both turns with `check()` after each |
| C4 | a two-way relation is one fact, read correctly from both ends | green | `one_way_door` (E0277); `fuzz::differential`; `back_reads_forward_label` killed |
| C5 | forbidden cycles don't compile | green | `chest_in_itself`, `chest_in_its_contents` (E0277, "the relation is ranked, and ranks must rise"), `room_in_item` (E0277), `overlap_without_loop_rule` (E0080 at `game!`), `refused_has_no_cycle` (E0599), all under `cargo check` |
| C6 | reads are total: no fallback, no `Default` | green | `kind_without_default` builds and runs; `index_through_set` (E0277); `fuzz::differential` |
| C7 | a hook reaches what its bound names; callers carry the bound | green | `caller_lacks_hook_bound`, `hook_overasks` (E0277); `mutual_hooks` builds, settles and runs; P7's `pick_up` asserts hold; also `code_needs_a_missing_module` (E0277, D6) |
| C8 | a hook chain that never settles is refused | green | `fuzz::hooks_settle`: a linear and a doubling chain both refused, no overflow, well inside the time budget |
| C9 | `cast` compares no strings | green | `claims::cast_matrix`; `cast_outside_set` (E0277); `cast_by_string` killed |
| C10 | relation endpoints are typed | green | `wrong_endpoint` (E0277) |
| C11 | changing the graph while iterating it doesn't compile | green | `conflicting_query` (E0502) |
| C12 | `game!` and `relation!` reject bad declarations under `cargo check` | green | `one_not_in_needs`, `root_cascade` (E0080 at `game!`), `detach_on_one`, `spill_on_many` (E0080 at `relation!`) |
| C13 | a module from another crate drops into a downstream `game!` | green | `module_from_another_crate`: membership declared upstream (`Bag`) and downstream (`Room`), no E0119; builds and runs |
| C14 | everything infers | green | `rust-analyzer analysis-stats .`: `exprs: 45917, ??ty: 0, ?ty: 0`; `pats: 5832, ??ty: 0, ?ty: 0` (after D5 and D7, with the proc macros expanded) |
| C15 | no `unsafe`, no dependencies beyond the approved compile-time proc-macro crates (D7), no library panics, no runtime type lookup | green | `cargo clippy -p weft --lib -- -D warnings` passes under the af02049 forbid wall; `lint_wall_is_intact`, `no_third_party_crates` (an allow-list, seen to fail), `source_is_clean` (over `src` and `weft-macros/src`) pass |
| C16 | P1–P7 compile and behave as written | green | `programs::every_program_runs` (every promised line, in order); `cargo test -p game`: 4 tests, including the exact snapshot, and 11 `compile_fail` doctests with their stated codes; every line P3, P6 and P7 comment out fails with E0277 when uncommented, and each has a reject case or doctest (D6) |
| C17 | dropping a world drops everything it ever spawned | green | `claims::drop_frees_everything`, including despawned entities and a 20,000-move edge history |
| C18 | the gates can fail | green | every twin compiles; the wrong-code canary fails; the fuzzer catches its planted fault (`seed 1, turn 17: is_doomed(14)`); all three mutants killed as test failures |
| C19 | every change after M0 is recorded | green | `deviations::nothing_changed_without_a_pin`, `every_record_claim_is_kept_or_logged`, and the other four P8 tests |

The fuzzer's run: 60 worlds x 30 turns, the world matched the model after every operation and `check()` held after every turn; 6,817 operations applied, 403 refused (348 of them `Held` despawns), 1,043 despawns.

## Deviations

- **D1**: the toolchain pin is named `nightly-2026-09-21`, which is rustc `bba531001 2026-09-20`, the compiler c's spike ran.
- **D2**: two reject cases were fixed to fail at the line rustc reports, `handle_enters_other_turn` (E0521 lands where the foreign handle is bound) and `caller_lacks_hook_bound` (a borrow error of its own in `main`).
- **D3**: M1–M5 landed as one library commit, because the M0 gates share P5's `game` crate and couldn't turn a claim green before most of the API existed.
- **D4**: semantics the record left open: `spill` with nowhere to land despawns, hook chains have a depth limit and budget (`Refused::Unsettled`), spawn fallibility follows `refuse` and hooks, `<->` is `many` only, and `one` edges may not form an ownership loop over kinds (at most 128 kinds per game).
- **D5**: ranks are Peano types, not const generics, because rust-analyzer can't type a const-generic literal; this drops `generic_const_exprs` and changes how P3, P5, P6 and P7 spell ranks (`Chest<R>` with a `rank: R` field, `Chest<R1>`).
- **D6**: two more reject cases, for the invalid states P6 and P7 comment out that no case covered (a module-bounded call for a game without the module; `target` through a set handle), plus clearer messages for `Kind`, `Member` and `Carries`.
- **D7**: at the owner's request, declarations are attributes (`#[kind(needs = [In], rank = 0)]`, `#[relation(..)]`, `#[component(..)]`) and proc macros (`module!`, `game!`) in a new crate, `weft-macros`, built on `syn`, `quote`, `proc-macro2` and `darling`, which the owner approved; C15's "no dependencies" becomes an allow-list of those; the weft library needs no nightly feature any more.

## Reopen triggers

None was hit. The nearest was **Inference fails**: before D5, C14 stood at `??ty: 233`. The cause wasn't the one the trigger names (`generic_const_exprs` not being evaluated). It was rust-analyzer not typing any const-generic literal argument, reproduced with no weft at all. The prompt's pre-approved fallback, Peano ranks, cleared it, and **Ranks fail** doesn't hold after that fallback: the rank cases fail under `cargo check` with the rank message, and P1–P7 compile unannotated.

## Toolchain

`rust-toolchain.toml` pins `nightly-2026-09-21`: rustc 1.100.0-nightly (bba531001 2026-09-20), with `clippy`, `rust-analyzer` and `rust-src`. After D5 and D7, the weft library enables no nightly feature; crates that call `module!` enable `decl_macro`.

## rust-analyzer

The pinned build (`rust-analyzer 1.100.0-nightly (bba531001 2026-09-20)`), `analysis-stats .` on the final tree:

```text
exprs: 45917, ??ty: 0 (0%), ?ty: 0 (0%), !ty: 0
pats: 5832, ??ty: 0 (0%), ?ty: 0 (0%), !ty: 0
Mir failed bodies: 0 (0%)
Failed data layouts: 0 (0%)
```

Rank errors are trait errors now, so rust-analyzer reports them natively: `rust-analyzer diagnostics` on `tests/reject` shows E0277 on the offending `relate` line of both rank cases.

## For review

- `cargo clippy --workspace --all-targets` passes with two style warnings, both in gate files. One is in `tests/deviations.rs`, which is P8 as frozen; the other is in `tests/behaviour/claims.rs`. They are left as they are rather than spent on pinned deviations.
- The ownership-loop rule (D4 point 5) forbids `one` relations such as "every NPC follows exactly one other NPC", which may loop. Such relations can still be `opt` or `many`.
- `Refused::Unsettled`'s limits (64 deep, 16,384 hooks per operation) are constants in `src/turn.rs`.
