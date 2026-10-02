# Test review

When you address an item, delete it. When a group is empty, delete its heading.

Paths are relative to `source/crates/`. Line numbers are at `c38f0da`. Each item gives the place, the problem and a better shape. The groups are sorted by severity and benefit. Items about production code that a test review found, and that `REVIEW.md` already holds (for example `PackagePath` and `./`), are not repeated here.

## Summary

- The suite has 316 tests and passes. Assertions are mostly exact, and most expected values have a derivation in a comment. That part is good.
- Setup is the main cost. In capabilities, seven module harnesses wrap `TestMatch` and each writes the same dozen helpers again. In abilities, combat, stats and projectiles, 52 % of the test-body lines are setup (group 2).
- Small helpers are copied across crates: `num()` 23 times, the 30 Hz `RATE` 11 times, `SEED_CHAIN` 7 times, `keypair` 9 times, SplitMix64 5 times (group 3).
- No test is over 1 s when the machine is idle. The slowest are near 0.5 s by design (group 7).
- No new dependency is necessary. Group 9 lists the decisions that the style guide does not settle.

## 2. Harnesses

### 2.5 Net

- [ ] **The round trip pinned to the link model** — `net/src/local_match/delay_line.rs` pins the measured round trip to zero, and `mod.rs` adds the modeled round trip to the sync margin. Better: pin the round trip to the link model's own value, and remove the `jitter_margin` workaround in `LocalMatch::client`. That step needs new derivations of the expected values. Also check that the join order (`play_by_team`) is then fixed.
## 3. Helpers copied across crates

- [ ] **`SEED_CHAIN = SeedChain::new([9; 32], MIN)`** — net's copies use `LocalMatch::SEED_CHAIN` now; reference_3v3 and headless still hold their own. Better: one with `FixedSession`.
- [ ] **`keypair(byte)`** — 9 copies in 5 crates (local_match:468, sim_client/tests.rs:12, lobby.rs:284, connect/tests.rs:15, session_log/tests.rs:27, delegation/tests.rs:5, input_chain/bench.rs:15, reference_3v3.rs:108, headless.rs:80). `NOW = 1_700_000_000` appears 6 times. A `TestKey(u8)` in protocol `internals` saves only about 25 lines. Do it with `FixedSession`, or not at all.
- [ ] **SplitMix64** — 5 copies: math `num/bench.rs:18`, net `delay_line.rs:80-87`, capabilities `regions.rs:390` and `broadphase.rs:170`, and sim `golden.rs:30`. The golden copy is frozen on purpose, so keep it. The regions and broadphase copies can draw from `campfire_math::Rng` through a `Grid::scatter(&self, &mut Rng, num, den)`. That changes the generated scenes: check the `crowded > 100` and `wide > 100` floors in broadphase again. The collision bench's history also restarts.
- [ ] **The path to `packages/`** — written 7 ways: reference_3v3.rs:23, headless.rs:29,31, local_match, lobby, reference_abilities.rs:36 (`format!`), mode_package.rs:17, package_dir/tests.rs:110. Better: one gated `PackageDir::workspace(..)`.

## 4. One style for one check

- [ ] **Tick steps have five styles** — `game.tick(&[])`, `walk.tick()`, `world.run_schedule(SimUpdate)` (vision 6×, abilities 14×, stats 18×), `run_until(n)`, and `while …SimTick…start().get() <= t + 4` loops. `TestMatch::step` / `run_until` / `now` cover all of them.
- [ ] **Script failures are read in more than five ways** — `matches!` on a vector (abilities:401), a table of fn pointers (abilities:690), `all(..) && count() == 1` (abilities:1162), `matches!` with a guard (abilities:1356), `calls()` (abilities:1241), mode's `failures()` (mode:741), orders' `think()` (orders:273). The mode tests also mix `assert!(failures().is_empty())` (817, 1592, 1770) with `assert_eq!(…, [])`. Better: `ScriptFailures::calls()` and `assert_eq!` only.
- [ ] **"Its types are state" is tested in three ways, and four types have no test**:
  - `combat:585` asserts the full registry of 19 names, including `stats.*`, so a new stats type breaks a combat test. `navigation:693` does the same with 17 names.
  - `projectiles:282` asserts only `any(..)`, and `vision/tests.rs:193-197` checks 2 names with no restore.
  - `areas.area`, `production.train_queue`, `progression.experience` and `projectiles.cast_hits` have no test.
  - The snapshot, restore and hash steps are written 5 times (combat:597, projectiles:280, stats:565, navigation:703, orders:1117).

  Better: one table test in `capability_set`. For each capability, compare `state_names()` with and without it, on top of what it needs. The difference must equal its own list exactly. Then round-trip a match with one unit that holds every component.
- [ ] **Positions and fractions are written in several ways**:
  - `at(Num, Num, Num)` (abilities, 36 lines of `Num::ZERO, Num::ZERO`), `at(i64, i64, i64)` (combat, orders), and `at(x, z)` (projectiles, vision, mode);
  - fractions as `from_bits(1 << 23)` (9×), as a floored `(9 << FRAC_BITS) / 10`, as `Num::from_str("3.25")`, and as quarter closures.

  `HALF` and `QUARTER` constants are safe. Do not change a floored fraction to `"0.35".parse()` without new derivations of the expected values, because the parser rounds to nearest and the shift floors.
- [ ] **Spawns** — `give_type_tags` is called in the abilities, orders and units spawns, but not in combat or projectiles (combat:303 adds it by hand). The stats `unit()` returns an `Entity` (stats:128), where every other harness returns a `StableId`. `TestMatch::spawn` settles both.
- [ ] **One hero runs at two rates** — the reference heroes run at 30 Hz in `reference_abilities` and at 20 Hz in the 3v3, and no test checks timing across rates. For example, Eruption's 625 ms is 19 ticks at 30 Hz and 13 ticks at 20 Hz. Low priority.
- [ ] **No golden for a whole match** — the sim golden pins 6 digests. Runner, verifier and net compare live against replay in one process and one binary, so they cannot see a difference between builds. Better: pin BLAKE3 over the 72 tick hashes of headless `scripted_creeps_and_towers…`, which plays `packages/test`. It changes when a state type is added, and that update is deliberate. This is also the first input `checks/det-ci` (an empty `main`) needs.

## 6. Hermetic and stable fixtures

- [ ] **The package flaws depend on MOBA content** — the flaws in `mode_package.rs` edit the reference packages. Their value edits go by TOML key path, so a balance change no longer breaks them, but a change to a name or to the map's shape still does. Give the flaw table a complete `packages/test` fixture of its own.
## 7. Time

The suite takes about 4 s. These times were measured with no other load:

| Test | Idle | Notes |
|---|---|---|
| `match_3v3::a_3v3_match_replays_to_the_same_hashes` | 0.35–0.61 s | 0.995 s and 1.47 s were measured under load |
| `session_log::every_truncation_and_every_flip…` | 0.20 s | a minimal log |
| `mode_package::every_flaw…` | 0.36 s | 1.24 s under load |
| net scenarios | 0.05–0.30 s each | the binary takes 0.29 s in parallel |

- [ ] **`every_flaw…` has room for about 390 flaws** — at about 2.5 ms each, it passes 1 s idle near 390 flaws. When it grows, split the table by area (manifest, mode data, map, heroes, scripts) into tests that run in parallel.

## 9. Decisions for you

These items depend on rules that the style guide does not settle. The harness work in group 2 needs answers to the first three.

1. **How a module harness extends `TestMatch`.** "`impl` blocks only in the struct's own file" stops a module from adding methods to `TestMatch`. The choices are a wrapper with a named field (`walk.sim.step()`), or a wrapper with `Deref` / `DerefMut` to `TestMatch`. `Deref` used as inheritance is an anti-pattern, but it stays in test code. Both reviews of capabilities recommend the named field.
2. **A home for a harness type.** "Test code sits at the end of the production file it reaches into" gives no home to a type that reaches into nothing, such as `TestMatch`, `HashTrail`, `FixedSession` or `Arena`. The `reference_3v3.rs` file is already a gated file of its own. Confirm that a gated file of its own (`test_match.rs`, `hash_trail.rs`, `fixed_session.rs`) is the rule. If it is not, `capability_set/mod.rs` must move its `mod tests` to `tests.rs` to make room.
3. **Sharing across crates.** `runner` needs `TestMatch`, so the gate becomes `any(test, feature = "internals")`, with an export from `campfire_capabilities::internals`. It is a feature edit in runner's dev-dependencies, not a new dependency.
7. **The silent skip of integration tests.** `runner` and `net` set `required-features = ["internals"]`. Plain `cargo test -p campfire-runner` builds 0 tests and skips all 9 integration tests with no message. The verification chain uses `--all-features`, so it is not affected. A self dev-dependency with `internals` fixes it, but that is a manifest change. You can also accept the skip.
9. **proptest in capabilities.** It is a workspace dependency, but only `math` uses it. The tables in 5.2 give most of the value without it. Adding it to `campfire-capabilities` dev-dependencies needs your approval.

## Order of work

1. The decisions in group 9, items 1–3.
2. `TestMatch` as the running harness, with its helpers beside their types (2.1, 2.2). Then the module harnesses become thin wrappers.
3. `LocalMatch` methods, `HashTrail` and `FixedSession` (2.4, 2.5). `HashTrail` then serves net, runner and verifier.
4. The exact readers and assertions (5.1), the crate-wide state sweep (group 4), and the delete in group 7.
5. The cross-checks and boundaries (5.2), the content decoupling (group 6), and layout (group 8).
