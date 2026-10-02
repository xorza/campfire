# Test review

When you address an item, delete it. When a group is empty, delete its heading.

Paths are relative to `source/crates/`. Line numbers are at `c38f0da`. Each item gives the place, the problem and a better shape. The groups are sorted by severity and benefit. Items about production code that a test review found, and that `REVIEW.md` already holds (for example `PackagePath` and `./`), are not repeated here.

## Summary

- The suite has 316 tests and passes. Assertions are mostly exact, and most expected values have a derivation in a comment. That part is good.
- The net scenario tests are not deterministic. Lightyear measures the round trip with `Instant::now()`. Under CPU load, 21 of 192 runs failed (group 1).
- Five tests have a defect that lets a wrong engine pass (group 1).
- Setup is the main cost. In capabilities, seven module harnesses wrap `TestMatch` and each writes the same dozen helpers again. In abilities, combat, stats and projectiles, 52 % of the test-body lines are setup (group 2).
- Small helpers are copied across crates: `num()` 23 times, the 30 Hz `RATE` 11 times, `SEED_CHAIN` 7 times, `keypair` 9 times, SplitMix64 5 times (group 3).
- No test is over 1 s when the machine is idle. The slowest are near 0.5 s by design (group 7).
- No new dependency is necessary. Group 9 lists the decisions that the style guide does not settle.

## 1. Tests that can pass when the engine is wrong

- [ ] **The net scenarios depend on the wall clock** — `net/src/local_match/mod.rs:100,228-231,407-423`. Lightyear measures the round trip with `Instant::now()` (`lightyear_sync-0.30.1/src/ping/plugin.rs:60,65,89`). The round trip sets the input delay, the reliable resend and the remote timeline. Under oversubscription (48 test processes on 32 threads), the delayed-link scenarios failed 21 of 192 runs:
  - rollback counts of `[11, 5]` or `[2, 0]` where the test expects `[1, 0]`;
  - an order not applied (`scenario.rs:167`);
  - a client that ends at a different position from the server (`scenario.rs:138,299,374`).

  The doc at `mod.rs:100` says "every run repeats exactly", which is false. A scratch copy tried one fix: a system in `First`, `Update` and `PostUpdate.before(SyncSystems::Sync)` sets `Link::stats.{rtt, jitter}` and `PingManager::rtt_estimator_ewma.final_stats` (both `pub`) to `Duration::ZERO`. With the fix, 0 of 400 runs failed under the same load, and no expected value changed. Better: `DelayLine::pin_round_trip` in `delay_line.rs`, with a note that the `=0.30.1` pin keeps the two fields stable. After that, pin the round trip to the link model's own value, and remove the `jitter_margin` workaround at `mod.rs:407-423`. That step needs new derivations of the expected values. Also check that the join order (`play_by_team`) is then fixed.
- [ ] **"No script failed" sees only the last tick** — `runner/tests/reference_abilities.rs:316,354,429,571,650`. `ScriptFailures` empties each tick (`capabilities/src/scripts/script_failures.rs:6-7`). These asserts run after 40–240 ticks, so a failure in an earlier tick passes. For example, at `:429` the arrows hit at ticks 8–32 and the test checks tick 40. The asserts also print nothing. Better: the harness steps one tick at a time and keeps every failure (`ScriptFailure` is `Clone`), and each test ends with `assert_eq!(arena.failures(), [])`. `match_3v3.rs:73-78` already does this.
- [ ] **The respawn test derives the wrong tick count and checks a range** — `capabilities/src/mode/tests.rs:1035,1072`. The comment says 1000 ms is 30 ticks, but the mode runs at 10 Hz (`:85`), so it is 10 ticks. The test then steps 30 times and checks that Y lives, which passes for any respawn delay from 0 to 30 ticks. Better: assert `Respawn { at: Tick::new(t + 11) }`, then step tick by tick, so that Y is dead at the last tick before the boundary and alive at it. The test at `:1522-1531` already does this. `respawn_per_level_ms` is 0, so the `per_level × level` term of `DEATHS_3V3` is not tested.
- [ ] **A refused cast is not checked to spend nothing** — `capabilities/src/abilities/tests.rs:533-548`. The loop of 8 refused casts checks only the enemy's health. A refused cast that paid mana or rage, or that started its cooldown, passes. Better: keep `(mana, rage, ready_at)` for each case and assert that it does not change. Also add the case where the cost is exactly what is left. Today the tests check only the side below the cost.
- [ ] **Asserts that assert nothing**:
  - `capabilities/src/units/tests.rs:172`: `assert!(far.get() > 0 && dead.get() > 0 && …)` only keeps bindings in use. Better: a positive control. At radius 6, `far` is found, and `dead`, `hidden` and `guarded` are not.
  - `sim/src/state_registry/tests.rs:236-245` `corruption_never_panics`: when `restore` gives `Ok`, assert that the world snapshots to the same bytes again. This also shows whether an overlong varint is accepted.
- [ ] **Latent fixture defects**:
  - `mode/tests.rs:642`: `Game::start(script, limits, files)` reads `resources` from a new `mode_files()`, not from `files`. Four more sites (`:1108,1305,1313,1842`) build the full fixture again only to read `resources`.
  - `abilities/tests.rs:263`: `Match::load` loads every action under the name `"lash_out"`. In 11 tests, `strike` has that name too, and `d.ability` reads it (`damage_handle.rs:72`). Better: `load(name, data, source)`.
  - `stats/tests.rs:461`: the aura test spawns units with a second `IdAllocator::default()`. A later `allocate()` from the world's allocator gives an id that `EntityIndex` already holds.

## 2. Harnesses

### 2.1 `TestMatch` becomes the running harness (capabilities)

Today `TestMatch` (`capability_set/mod.rs:128-173`) only builds a world. Each module wraps it again: abilities `Match`, combat `Fight`, projectiles `Volley`, navigation `Walk`, orders `Match`, units `Scene`, vision `Scene`, mode `Game`, and `stats_match`. Each of these writes its own copies of the same helpers:

| Helper | Copies | Sites |
|---|---|---|
| unpack `TestMatch`, then `add_schedule` | 10 | abilities:251, combat:95, projectiles:91, orders:212, mode:652, navigation:48, vision:60, units:74, stats:122,460 |
| `run_until` | 3 | abilities:333, combat:128, orders:303 |
| `get` / `get_ref` | 4 (3 return shapes) | abilities:339, combat:111, orders:308, navigation:106 |
| `set_blocks` | 3, byte for byte | abilities:372, combat:156, navigation:99 |
| `entity` | 5 | combat:808, projectiles:156, vision:90, units:111, mode:705 |
| `health` | 6 (`i64`, `Option<i64>`, `Num`; some rounded) | abilities:349, combat:135,839, projectiles:170, orders:318, mode:1686 |
| `probe` + `ids` | 2, byte for byte | units:116-139, vision:99-122 |
| `resource::<EntityIndex>().get(..)` inline | 51 | abilities 19, combat 14, navigation 11, orders 7 |

The schedule is a separate field only because `Mode::install` needs it (`mode/tests.rs:684`). `world.schedule_scope(SimUpdate, ..)` gives that access, and `abilities:646` and `combat:378` already use it.

- [ ] **Shape** — at the end of `capability_set/mod.rs`, in its `internals`:

  ```rust
  #[derive(Debug)]
  pub(crate) struct TestMatch {
      pub(crate) world: World, // holds the SimUpdate schedule
      pub(crate) registry: StateRegistry,
  }

  impl TestMatch {
      pub(crate) const RATE: TickRate; // 30 Hz; the mode's tests keep 10 Hz
      pub(crate) fn new(declared: &[Capability], rate: TickRate, scripts: Option<MatchScripts>) -> TestMatch;
      pub(crate) fn client(declared: &[Capability]) -> TestMatch; // no script host
      pub(crate) fn server(declared: &[Capability], scripts: MatchScripts) -> TestMatch;
      pub(crate) fn install<R>(&mut self, f: impl FnOnce(&mut World, &mut Schedule, &mut StateRegistry) -> R) -> R;
      pub(crate) fn spawn(&mut self, at: Position, parts: impl Bundle) -> StableId; // the world's allocator, type tags given
      pub(crate) fn entity(&self, id: StableId) -> Entity;
      pub(crate) fn get<C: Component>(&self, id: StableId) -> &C;
      pub(crate) fn try_get<C: Component>(&self, id: StableId) -> Option<&C>;
      pub(crate) fn get_mut<C: Component<Mutability = Mutable>>(&mut self, id: StableId) -> Mut<'_, C>;
      pub(crate) fn insert(&mut self, id: StableId, parts: impl Bundle);
      pub(crate) fn now(&self) -> Tick;
      pub(crate) fn step(&mut self);
      pub(crate) fn run_until(&mut self, tick: u64);
      pub(crate) fn block_at(&mut self, id: StableId, tick: u64, set: SimSet, blocks: &'static [Block]);
      pub(crate) fn probe(&mut self, source: &str, of: StableId) -> Result<Dynamic, CallError>;
      pub(crate) fn read(&mut self, expression: &str, of: StableId) -> Result<Dynamic, CallError>;
      pub(crate) fn state_names(&self) -> Vec<&'static str>;
      pub(crate) fn round_trip(&self, fresh: TestMatch) -> TestMatch; // asserts equal hashes
  }
  ```

  The module harnesses keep only their own verbs (`cast`, `caster`, `fire_line`, `pick`) and hold a `TestMatch`. The capabilities part saves about 450 lines. `client` and `server` make the script host visible. Today combat, projectiles and stats run only as a client, because they pass `None`. That means no `ModifierHooks`, and no combat or projectiles test runs the server configuration.

  Example: the vision `Scene` (`vision/tests.rs:30-122`) goes from 92 lines to about 35. The stats aura test (`stats/tests.rs:426-526`) goes from 100 lines to about 30, and the second allocator goes away.
- [ ] **Helpers beside their types**, each at the end of its own file in `internals`:
  - `Pools::life_left() -> Num` replaces the six `health` helpers and reads exactly (see 5.1).
  - `ScriptFailures::calls() -> Vec<FailedCall>` and `CallError::kind() -> FailureKind` (an error satellite in `scripts/error.rs`). Together they replace the 10 `assert!(matches!(…CallError::Api…))` sites with `assert_eq!`.
  - `MatchScripts::bare(limits, players)` exists (`match_scripts.rs:20-38`), but only `stats/tests.rs:448` uses it. Make it an associated fn, add `with_pools(&[&str])`, and use it at `orders:200`, `vision:52`, `units:65` and `capability_set:225-237`.
  - `ScriptLimits::ROOMY`. The roomy limits have three values today: 10k/100k (abilities, vision, units, mode), 20k/200k (orders:186), and 10k flat (stats).
  - `Unit::ids(Dynamic) -> Vec<StableId>`, `UnitTypeData::tagged(&[&str])`, and `Units::load_next_type`. These replace the copies at orders:235-262, units:86 and mode:650.
  - `Armed::bundle` and `Arms::parts` read `TickRate` from the world, not from an `hz` argument. Every caller passes `RATE.hz().get()` (abilities:287, combat:106, projectiles:151, orders:241, units:100, runner:281), so a wrong rate gives a wrong weapon period with no error. `stats::internals::load_stats` drops its `rate` argument for the same reason.
  - `combat::armed` is a `#[cfg(test)]` module, but its name is not `internals`.
- [ ] **In-crate tests do not use the exported helpers** — `load_stats`, `give_modifier`, `carried` and `pools::internals::spent` exist for `runner`, and the capabilities tests write them again:
  - `StatBook::new(..) + Stats::load` at abilities:929,992,1032,1083,1206;
  - `Match::give` at abilities:1218-1238;
  - `Modifiers::get` by hand at abilities:960,1021,1069,1491;
  - `Pools::take` at combat:908,975,989 and abilities:519,1149.

### 2.2 Data constructors (capabilities)

- [ ] **`ModifierData` (13 fields)** is written in full 8 times: abilities:932,1034,1175,1381, combat:790, stats:427, mode:401,420. Every field is an `Option`, a collection, a `bool` or the default `Reapply`, so `#[derive(Default)]` gives what an empty `{}` table reads. This saves about 110 lines. `Default` adds to the public API (group 9).
- [ ] **`ActionData` (22 fields)** — abilities:136,189 and mode:312 (`blink_data`). `targeting` has no default. Better: `ActionData::cast(targeting)` in `action_data.rs` internals. This saves about 55 lines.
- [ ] **`Instance` (16 fields) in `Application`** — combat:910, stats/tests:222,483, stats/modifiers.rs:406, mode:1626, and 10 `Application {` literals in total. Better: `Instance::bare(id, source)` and `Application::refresh(instance)`. This saves about 80 lines.
- [ ] **Smaller ones**:
  - `ProjectileData::flying(speed)`: abilities:1592 writes what `projectile(false, ZERO, None, true)` builds.
  - `TestWeapon::new(aim, range, windup)`: combat:272,417.
  - `scaling()` takes a `Ranked<Scalar>`. Today `abilities:1454-1463` takes its result apart again with `let … else { unreachable!() }`.
- [ ] **Map data** is built twice, in mode and in navigation:
  - `MarkerData` at mode:253-272 and navigation:524-532;
  - `PlacedUnitData` at mode:353 and navigation:517;
  - `MapData` at mode:340-369 and navigation:534-551.

  Better: `MapPoint::ground`, `MarkerData::tagged`, `PlacedUnitData::new` and `MapData::planar` in `mode/map_data.rs` internals.
- [ ] **Pathing grids** are built in four places: `regions.rs:380`, `route_planner.rs:455` (`walled`, which parses an ASCII map), `pathing_grid.rs:206` (`drawn`, which prints one), and 6× `load_pathing` in `navigation/tests.rs`. Better: `PathingGrid::from_ascii` and `draw` in `pathing_grid.rs` internals. `Walk::with_pathing(cell, half_side, walkers)` covers the 3 identical 16 m squares.

### 2.3 Mode harness

- [ ] **A pick prelude** — 7 scripts start with `if name == "hero" { pick(...); return; }`, and 10 tests first tick `input("hero", "hero-x")`. Better: a `PICKING` prelude, plus `Game::picking(src, limits)` and `Game::pick(slot, hero) -> Entity`. These replace the 4 `query_filtered::<Entity, With<Owner>>().single()` lookups (918, 976, 1100, 1218) and the search at 1055. This saves about 60 lines.
- [ ] **`mode/tests.rs` is 2129 lines with about 106 imports** — it also tests production (1079-1181), experience (944-1076), modifiers (1184-1267, 2083-2129), `calc_damage` (1608-1774) and roles and relations. These tests need the mode, and the layer rule (`lib.rs:1-5`) stops them from moving down into their own capability. Better: split by concern into `mode/tests/{mod.rs, start.rs, inputs.rs, progression.rs, production.rs, modifiers.rs, damage.rs, roles.rs, match_end.rs}` (group 9).
- [ ] **`calc_damage_3v3()` cuts the live package at a comment** — `mode/tests.rs:1608-1614` cuts at `find("// A source that is gone")`. The orders and mode tests otherwise keep frozen copies of reference scripts (`CREEP_AI`, `DEATHS_3V3`). Choose one policy. If the cut stays, cut at a function name.

### 2.4 Runner and verifier

- [ ] **`HashTrail`: one live-against-replay check** — the comparison is written 5 times with 3 different end checks, and two of them have no end check:
  - `match_3v3.rs:185-199`: no end check;
  - `headless.rs:227-236`: `len`;
  - `headless.rs:290-298`: `!run_tick()`;
  - net `prototype.rs:119-131`: `next_tick`;
  - net `scenario.rs:158-177`: no end check.

  Each one names only the tick. `StateRegistry::hash_by_type` exists (`sim/src/state_registry/mod.rs:114`), but `Session.state` is private (`runner/src/session.rs:22`). Better: an `internals` type in `runner`:

  ```rust
  #[derive(Debug, Default)]
  pub struct HashTrail { names: Vec<&'static str>, types: Vec<[u8; 32]>, totals: Vec<StateHash> }
  impl HashTrail {
      pub fn record(&mut self, runner: &Runner);
      pub fn totals(&self) -> &[StateHash];
      pub fn assert_same(&self, replayed: &HashTrail); // tick count, first differing tick, differing types
  }
  ```

  It needs `Session::hash_by_type` and `Runner::state_hash_by_type` in gated `internals` at the end of their files. Memory is about 3 MB for 2500 ticks.
- [ ] **`FixedSession`: one fixed-key session fixture** — `runner/src/reference_3v3.rs:23-134` and `verifier/tests/headless.rs:34-37,80-213` both define `SEED_CHAIN`, `SERVER_KEY`, `AUX`, `key`, `session_key`, `delegation`, `terms`, and an `InputChain` signing loop. `headless::run()` loads the lane mode 6 times (`:184,186`, `log()`, 3× `:200`) and signs the delegation twice. `Reference3v3::start` also signs each delegation twice (`:60-62,74`). Better:

  ```rust
  #[derive(Debug)] pub struct FixedSession { packages: ModePackages, terms: SessionTerms, players: Vec<Delegation> }
  #[derive(Debug, Clone, Copy)] pub struct InputRules { pub max_input_delay: u64, pub max_input_lead: u64, pub max_payload_len: u32, pub max_inputs_per_tick: u32 }
  #[derive(Debug)] pub struct FixedPlayer { /* chain, key, session id */ }
  impl FixedSession {
      pub fn workspace(mode: &str, players: u32, tick_hz: NonZeroU32, rules: InputRules) -> FixedSession;
      pub fn header(&self, terms: SessionTerms) -> SessionHeader;
      pub fn start(&self) -> Runner;
      pub fn player(&self, slot: u32) -> FixedPlayer;
  }
  impl FixedPlayer {
      pub fn send(&mut self, runner: &mut Runner, stamp: u64, payload: &[u8], applied: &mut Vec<Applied>) -> Result<(), InputError>;
  }
  ```

  The headless fixture goes from about 94 lines to about 25, and `Reference3v3` to about 40. Keep the same key bytes, so that no hash changes.
- [ ] **An `Arena` for `reference_abilities.rs`** — `reference_world()` (`:170-210`) copies `TestMatch::new`, because `TestMatch` is `#[cfg(test)]`. Its `ScriptLimits` (`:175-180`) are copied from the abilities tests, not from the 3v3 manifest. Each test then loads by hand what `MatchBuild` loads: 7× `compile`, 6× `Actions::load(…, 0, …, 5)`, 8× `load_type`, 4× `bind_spawn` and 5× `load_modifier`, about 52 lines. The `Order { … Action::Slot … }` literal (9 lines) appears 5 times, and `run_schedule` is called by hand 11 times. Better:
  1. Gate `TestMatch` `any(test, feature = "internals")` and export it through `campfire_capabilities::internals`.
  2. Add an `Arena` with `step`, `steps`, `cast(caster, slot, target)` and `failures()` (see group 1), and `hero(name) -> HeroLoad { actions, modifiers }`. `hero` loads in the order of `MatchBuild::run` (`match_build.rs:86-129`).
  3. Later, load through `MatchBuild` itself without `Mode::start`.

  `every_reference_ability_reads…` (`:98-110`) repeats what `Reference3v3::load()` checks, and its Veil check repeats abilities:1503-1566. Remove it or make it shorter.

### 2.5 Net

- [ ] **Methods on `LocalMatch`** — bootstrap is already 2 lines (`LocalMatch::new` + `start_match`). The repetition comes after the start:
  - the settle loop `for _ in 0..20 { local.step(); }` at scenario.rs:99,263,357,407, and other counts (10, 40, 30) at prototype.rs:177,183,217,259,271, with no derivation;
  - the rollback readout 6 times;
  - `SimTick.start().get()` inline 5 times. `next_tick` is `pub(crate)` in `scenario.rs:46` and `lane.rs:16` imports it, which couples two test files;
  - four rules for "the tower of team t" (scenario.rs:337-346, prototype.rs:234-243, lane.rs:318-335, and fog.rs:454-457 by position);
  - the client of a team, worked out 3 times;
  - `MatchSetup::solo(Check, 1, SEED_CHAIN)` 4 times, and the same 6-line `LinkModel` literal 4 times.

  Better, in `local_match/mod.rs`:

  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum End { Server, Client(usize) }
  impl MatchSetup { pub const SOLO: MatchSetup; pub const DUO: MatchSetup; }
  impl LocalMatch {
      pub const SEED_CHAIN: SeedChain;
      pub fn started(setup: MatchSetup) -> LocalMatch;
      pub fn app(&self, end: End) -> &App;
      pub fn server_tick(&self) -> u64;
      pub fn steps(&mut self, count: usize);
      pub fn step_to(&mut self, tick: u64, after_step: impl FnMut(&LocalMatch));
      pub fn settle(&mut self); // from link delay, jitter and lead
      pub fn rollbacks(&self, client: usize) -> u32;
      pub fn unit(&self, end: End, id: StableId) -> EntityRef<'_>;
      pub fn tower(&self, team: Team) -> StableId;
      pub fn client_of(&self, team: Team) -> usize;
      pub fn check_replay(&mut self) -> Replayed; // per-tick hashes, one per log tick
  }
  #[derive(Debug)] pub struct Replayed { pub ticks: u64, pub applied: Vec<AppliedAt> }
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct AppliedAt { pub stamp: u64, pub tick: u64 }
  ```

  This saves about 200 lines. The route scenario (scenario.rs:323-379 + 452-462) goes from 68 lines to about 30, and prototype.rs:107-130 goes from 24 lines to 3.
- [ ] **net's `internals` is not like the other crates** — `lib.rs:13` gates `mod local_match` on `feature = "internals"` only, and `lib.rs:34-37` re-exports `LocalMatch`, `MatchSetup` and `LinkModel` at the root. `campfire_log` uses `pub mod internals { pub use … }`. Better: the same facade.

### 2.6 Protocol and package

- [ ] **`session_log/tests.rs` repeats its submit step**:
  - `log.record(packet.inputs.iter().copied(), &packet.signature, …)` appears 6 times;
  - `SessionLog::new(header()).unwrap()` appears 9 times;
  - the `SCRIPT` expected table appears twice (`:272-278`, `:287-293`);
  - the tail check appears twice (`:297-300`, `:812-814`).

  Better: `Sent::submit(&self, log, applied)`, `new_log()`, `script_applied()` and `tail_applied()`. This saves about 50 lines. Do not merge the `SessionTerms` literals: the byte tests derive their bytes from those values (`:852`).
- [ ] **Tests of other types in `session_log/tests.rs`** — the `InputChain` layout test (`:656-726`) belongs in `input_chain/mod.rs`, which has no tests. The `SeedChain` tests (`:571-654`) belong in `seed_chain.rs`.
- [ ] **The package flaw harness is good** — `PackageDir::in_memory` plus `edited()` (`mode_package.rs:20-58`) gives a changed tree with no disk. A temp-dir harness is not necessary. Only `package_dir/tests.rs` uses the file system: add a `Drop` guard so that a panic does not leave `/tmp/campfire-package-<pid>-*`.
- [ ] **The limit tests in `mode_package.rs`** — `more_layers…` (`:144-159`) and `more_tracks…` (`:161-175`) are not in the flaw table, because `Edit` holds `&'static str`, so they `.leak()` (`:151,167`, and `Box::leak` at `:1362`). Better: a table of `LimitCase { file, with: fn(&str, usize) -> String, allowed, limit }` that asserts `allowed` loads and `allowed + 1` gives `TooMany(limit)`. That also adds the missing at-limit cases: the 3v3 already declares 11 tags, so "257 layers" is not the boundary, and no test loads 32 tracks.

## 3. Helpers copied across crates

- [ ] **`fn num(i64) -> Num`** — 23 copies, all `Num::from_int(value).unwrap()`. Better: one `const fn` in `math` (group 9).
- [ ] **The 30 Hz `RATE`** — 11 copies documented as "the MOBA's". These include `sim/tests/golden.rs:25` and `sim/src/sim_update/tests.rs:16`, but the sim knows nothing of the MOBA. Better: `TestMatch::RATE` in capabilities, and a rate with its own name in sim.
- [ ] **`SEED_CHAIN = SeedChain::new([9; 32], MIN)`** — 7 copies (scenario.rs:15, prototype.rs:20, lane.rs:18, fog.rs:14, sim_client/bench.rs:14, reference_3v3, headless). Better: `LocalMatch::SEED_CHAIN` and `FixedSession`.
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

## 5. Stronger assertions

### 5.1 Weak bounds where the exact value is known

- [ ] **Health is read rounded** — `round()` at abilities:358 (every pool), combat:137, projectiles:173 and orders:320. A health of 449.5 shows as 450. combat also has `exact_health` (839). Better: one exact reader (`Pools::life_left`) and `num(450)`, at about 60 assert sites.
- [ ] **capabilities**:
  - combat:240 `started.is_some()`: the start tick is known.
  - abilities:845-851 uses `matches!` and `is_ok()`, but `ActionError` is `PartialEq` and ids are sequential, so assert `Ok(ActionId(n))`.
  - abilities:831,898 `a && !b` cannot show which side broke.
  - abilities:492 checks only `len == 10`. Assert the `(unit, hook)` list.
  - `values/metric.rs:105-110` `is_some()`: planar at (4, 3, 2) gives `(40 << 48, 100 << 48)`.
  - `units/tests.rs:247,249` `assert!(iter.eq(..))` prints no values.
- [ ] **Navigation "planned or not"** — `navigation/tests.rs:296-429`:
  - The unplanned runs assert only `!arrived`. Pin the stop positions: the walker against the tower stops 0.9 + 0.5 short of the tower's centre, and the head-on pair stops symmetric about 0 and 1 m apart.
  - The bare 80- and 120-tick loops (318, 370, 410) should assert the arrival tick. The straight path is 32 ticks.
- [ ] **script** — `script_host/tests.rs`:
  - `:157` `other.left() < 1500`: `one()` costs exactly 3 operations, so `== 1497`.
  - `:159-173`: 20 levels pass and 40 fail. The boundary is `down(32)` ok and `down(33)` err.
  - `:109-111` `is_err()`: pin `Runtime("Too many modules imported")` and `Runtime("Function not found: sleep (i64)…")` / `timestamp ()`.
- [ ] **package** — `mode_package.rs:316` (`[fight]`) and `:964` (`foes:avatar`) accept any `Content(_)`, and `:194` any `OtherEngine(_)`. Use `read_fails` (`:118`).
- [ ] **net**, after group 1:
  - `prototype.rs:104` `rollbacks > 0`: the exact count.
  - `prototype.rs:276` `start() > tick`: `== tick + 30`, which needs one check.
  - `lane.rs:398-399` `0 < lost < full`: derive the strike count.
  - `lane.rs:371`: pin the tick.
  - `fog.rs:514` `held_until < MATCH_FRAMES - 1`: pin the frame.
- [ ] **math and client**:
  - `rng/tests.rs:178-187` asserts only `< 3` and `< u64::MAX` for `pick` and `below`. Cross-check against a parallel word stream: for the bounds `[1, 3, 1000, 1 << 63, u64::MAX]`, the expected value is the first `Some` of `lemire_step::<64>(words.next_u64(), bound)`.
  - `client` `ring.rs:31-35` brackets the end with 0.99 and 1.01. `done` is exactly 1.0 there, so assert `None`.
  - The client tolerances `1e-6` and `1e-5` (view.rs, gauge.rs, ring.rs) need a reason.

### 5.2 Missing boundaries and cross-checks

- [ ] **Regions against a flood fill** — `regions.rs:470-505` has a fixed seed, but all cases are one shape, 150×90. Add a table of shapes and densities to the same loop:
  - grids smaller than a chunk: 1×1, 1×130, 130×1;
  - exact multiples of 64 (64×64, 128×64), and 65×63;
  - all open, and all blocked;
  - a checkerboard, which is the worst case for the 4 places in `Reach` and for 2048 regions in one chunk with `u16` labels.

  The current case takes 0.18 s.
- [ ] **Route planner** — `route_planner.rs:483-597` has only hand-picked cases on 2 maps. Add a brute-force Dijkstra in the test module (cost 10 straight and 14 diagonal; a diagonal only when both side cells are open) over scattered maps. Assert:
  - the same `cost`;
  - `reached` agrees with `Regions`;
  - each waypoint segment is clear of each post;
  - `expanded` is at most the number of open cells.
- [ ] **`set_bits`** — `vision/mod.rs:141-153` has 3 hand cases, and none ends on a word boundary or starts at 64. Sweep all `0 ≤ s < e ≤ 192` (about 18,500 runs) against a bit-by-bit fill.
- [ ] **math**. These values were checked in a probe and pass today:
  - `sin_cos` is exact for every `Num` (`trig.rs:18-19`), but the sweep stops at 2⁵² raw (`num/tests.rs:433-437`). Sweep to 2⁶² in raw steps of 2¹¹, which are exact in f64. `MIN.sin_cos() == (16_412_560, 3_478_915)` raw, and `MAX.sin_cos() == (-16_412_560, 3_478_914)`.
  - `atan2`:
    - `(MIN, 0) = -FRAC_PI_2`;
    - `(0, MIN) = PI`;
    - `(MIN, MIN) = -39_530_384`;
    - `(MAX, MIN) = 39_530_384`;
    - `(MIN, MAX) = -13_176_795`;
    - `(-1, MIN) = -52_707_179`.
  - `MAX.sqrt() == n(12_439_554_047_902)`. The comment at `:333` says that it fits, but no assert checks it.
  - Integer operands (`int_operands` draws only ±1000):
    - `MIN.checked_mul_int(-1) == None`;
    - `EPSILON.checked_mul_int(i64::MAX) == Some(MAX)`;
    - `MIN.checked_div_int(i64::MIN) == Some(n(1))`;
    - `MAX.checked_div_int(i64::MIN) == Some(n(-1))`;
    - `ONE.checked_div_int(i64::MIN) == Some(ZERO)`.
  - `prop_assume!(a != i64::MIN)` at `:488` can never fail, because `a` is within ±2⁴⁵. Remove it.
  - The proptest oracle `narrow` is production code (`num/mod.rs:224`). Use `i64::try_from` instead.
- [ ] **`U256`** — no test reaches the `None` of `floor.checked_add(1)` (`u256.rs:41,69`). With `x = product(u128::MAX, 2) + product(1, 1)` (2¹²⁹ − 1), `x.round_shr(1)` and `x.round_div(2)` are both `None`. Add these:
  - the cross-check `p.round_shr(k) == p.round_div(1 << k)` for k in 1..=126;
  - a 4-limb schoolbook oracle for `product`;
  - the boundaries `round_shr(127)` and the divisor `(1 << 127) - 1`.
- [ ] **64-bit Lemire rejection** — the full sweeps use `lemire_step::<16>`. Add `[lemire_step::<64>(0, 3), lemire_step::<64>(1, 3)] == [None, Some(0)]`, because (2⁶⁴ − 3) mod 3 = 1.
- [ ] **Data refusals with no test**:
  - `Thresholds::new` with `[]`, `[0]`, `[100, 100]` and `[300, 100]` (`track_data.rs:25`);
  - `ProjectileData`: speed ≤ 0, and a negative width or range (`projectile_data.rs:49-65`);
  - `AreaData` with a negative radius (`area_data.rs:61`);
  - `TrainQueue` decode refusals (`train_queue.rs:93-98`). Every other component with a checked decode has such a test.
- [ ] **Production has no tests of its own** — the only test is `mode:1078`, which needs the full test mode. These cases have no test:
  - a dead producer's queue, which waits;
  - a zero-time train, which spawns in the same tick (the `while let` at `production/mod.rs:59`);
  - two producers, which spawn in stable-id order;
  - a producer with no owner and a resource cost.

  The `"tick {at}"` messages of the mode test are one tick off, because the hero pick already ran tick 0.
- [ ] **Areas** — `inside.allies` and a custom `affects` filter have no test. `areas/tests.rs` holds only decode and filter tests, and the one area behaviour test is in `abilities:1641`.
- [ ] **`StartError`** — 7 of 11 variants have no test: `OtherMode`, `OtherDependencies`, `UnitKit`, `Ai`, `Ability`, `Mode` and `MatchStart`. If the load check makes them impossible, they become `expect(CHECKED)`. Otherwise give each one a test.
- [ ] **content** has no tests. `PackagePath` is tested only through 4 refusals in `package_dir/tests.rs:137`. `Version::parse` accepts `01.0.0`, and no test decides if that is correct.

### 5.3 Failure messages

- [ ] **Loops whose assertion does not name the case**:
  - `mode_package.rs:1342` `.expect_err(flaw.file)`: 33 flaws edit `MODE_DATA` and 25 edit `HUSK`. Panic with `{flaw:?}`.
  - `prototype.rs:98,103,109-116,120,130` lack `{case}`.
  - `scenario.rs:167` has no message. A failure under stress printed only `right: 9`.
  - `session_log/tests.rs:523-536` (11 changes) has no index.
  - `session_log/tests.rs:974-976` has no message, and its two `WrongSeed` cases look the same.
  - abilities:1162,1356 have no message, and abilities:406 does not name the unit or the hook.
  - `state_decl/tests.rs` names a failure `"case {at}"`. Print the declaration.
- [ ] **Counts with no derivation**:
  - the settle steps (20, 10, 40, 30) in net;
  - `check_log(…, 4 | 9 | 2 | 1)`, which needs the count by hand: 2 + 2, 7 + 2, 1 + 1, 1 + 0;
  - `match_3v3.rs:105` `compiled() == 38`, which can be derived as the mode's scripts plus each dependency's scripts;
  - `PICK_END` and `FIRST_WAVE` (`:37-38`), which can be derived from `TickRate::ticks(pick_ms) − 1`;
  - the camp positions (`:154-158`), which can be derived from the map markers.
- [ ] **`match_3v3.rs:101` reads `ModeState.get()[1]` by index** — a new state field moves the index with no error. Find the index from the state names in `packages.data()`.

## 6. Hermetic and stable fixtures

- [ ] **The package flaws depend on MOBA content** — all 138 flaws in `mode_package.rs` edit MOBA text. 26 of the edits match balance digits, and 3 match a comment (`"# Its damage kinds"` at `:151,1052,1073`). A balance change breaks the table. Do these in order:
  1. Match section headers, not comments.
  2. Edit TOML by path for value flaws (`toml` is already a dependency of package).
  3. Later, a complete `packages/test` fixture that the flaw table owns.
- [ ] **`package_dir/tests.rs:116-150` asserts Husk** — its name, `slots["basic"][2]` and the start of its script. Move it to `packages/test/heroes/walker`.
- [ ] **`reference_abilities` and `match_3v3` test content** — they change when the content changes, by design. Say so in the module doc. The doc at `reference_abilities.rs:1-4` also omits Snow Owl.
- [ ] **The verifier subprocess is not hermetic**:
  - `headless.rs:375-376,395` writes `headless.log` and `headless-truncated.log` into the shared `CARGO_TARGET_TMPDIR`. Two cargo runs at the same time race on them, and nothing removes them. Add `process::id()` to the names, and remove the files at the end.
  - `headless.rs:380-386`: the spawned verifier inherits `CAMPFIRE_LOG` and `CAMPFIRE_LOG_FILTER` (`log/logging.rs:36`). Add `env_remove` for both.
- [ ] **lan-check** — `run_dir.rs:96-106` leaves its per-PID temp dir behind when an assert fails before the cleanup. A `Drop` guard fixes it.
- [ ] **proptest uses a random seed** — `num/tests.rs:185,457` and `vec3/tests.rs:178` run with `with_cases(10_000)` and `RngSeed::Random`. A failure replays with `PROPTEST_RNG_SEED`, but no `proptest-regressions/` directory exists. Choose `RngSeed::Fixed` (the same inputs each run), or keep Random and commit the regressions when they appear. Put one config constant in each file.

## 7. Time

The suite takes about 4 s. These times were measured with no other load:

| Test | Idle | Notes |
|---|---|---|
| `match_3v3::a_3v3_match_replays_to_the_same_hashes` | 0.35–0.61 s | 0.995 s and 1.47 s were measured under load |
| `session_log::every_flip_of_a_log_file_is_refused` | 0.53 s | 85 % schnorr verification |
| `mode_package::every_flaw…` | 0.36 s | 1.24 s under load |
| `verifier headless::every_corruption…` | 0.13–0.23 s | a copy of the protocol sweep |
| net scenarios | 0.05–0.30 s each | the binary takes 0.29 s in parallel |

- [ ] **Delete `verifier/tests/headless.rs:348-371` `every_corruption_of_a_log_file_is_refused`** — it repeats `protocol/src/session_log/tests.rs:890-924` with the same masks and the same comment. It calls only `SessionLog::decode`, and for truncation it checks only `is_err()`, where protocol checks `NotLog` and `Truncated`. `headless.rs:385-406` still tests the binary end to end.
- [ ] **A minimal log for the flip and truncation sweeps** — `published()` holds 11 packets, so a flip in the packet region costs about 8 signature checks. A log of 2 players, 1 tick with 1 packet of 2 inputs, 1 tail packet and the seed has every byte class with 3 signatures. The estimate is 0.53 → 0.17 s for flips and 0.15 → 0.05 s for truncation, with all 3 masks. Put the two sweeps in one loop.
- [ ] **`match_3v3` keeps its claims at its current cost** — of its time, 68 % is ticks, 31 % is state hashes (about 27 µs each) and 4 % is setup. Fewer ticks is not possible (1199, 2399 and 2499 are content times at the mode's lowest rate). Hashes only at checkpoints would miss a divergence that converges again. Better: keep the test, and if wall time becomes a problem, give it a budget in instructions (`perf stat -e instructions:u`, about 6 G) instead of seconds. The hash cost and the cost of an empty pick tick (about 31 µs) are engine performance items for `REVIEW.md`, not test items.
- [ ] **`every_flaw…` has room for about 390 flaws** — at about 2.5 ms each, it passes 1 s idle near 390 flaws. When it grows, split the table by area (manifest, mode data, map, heroes, scripts) into tests that run in parallel.

## 8. Layout and style

- [ ] **Inline tests over the split line** (more than 150 lines, or more than 40 % of the file):

  | File | Test lines | Share |
  |---|---|---|
  | `capabilities/src/scripts/script_api.rs` | 232 | 28 % |
  | `checks/lan-check/src/verdict.rs` | 200 | 57 % |
  | `capabilities/src/stats/modifiers.rs` | 180 | 32 % |
  | `capabilities/src/navigation/route_planner.rs` | 171 | 28 % |
  | `capabilities/src/navigation/regions.rs` | 167 | 31 % |
  | `capabilities/src/navigation/pathing_grid.rs` | 152 | 48 % |
  | `capabilities/src/navigation/collider.rs` | 101 | 51 % |
  | `net/src/order_script.rs` | 97 | 49 % |
  | `math/src/u256.rs` | 75 | 45 % |
  | `checks/lan-check/src/process_log.rs` | 60 | 42 % |
  | `capabilities/src/actions/weapon.rs` | 49 | 56 % |
  | `net/src/events/unit_died.rs` | 54 | 51 % |
  | `sim/src/command.rs` | 54 | 43 % |

  Four more files pass 40 % only because they are small: `log_line.rs`, `segment.rs`, `tick_rate.rs` and `hook_set.rs`. See group 9.
- [ ] **Tests in the file of another type**:
  - `navigation/tests.rs:657-690` (`Paths`) goes to `paths.rs`;
  - `navigation/tests.rs:736-747` (`MoveStep`) goes to `move_step.rs`;
  - `orders/tests.rs:374-410` (`Order` encoding) goes to `order.rs`;
  - the hook arity and `Block` serde checks in `script_api.rs` `the_reference_is_what_the_registry_writes` (`:768-790`) go to `hook.rs` and `block.rs`.
- [ ] **Coding guide**:
  - Tuple returns: `orders/tests.rs:413` `two_heroes() -> (Match, [StableId; 2])` and `route_planner.rs:455` `walled() -> (PathingGrid, BodyIndex)`.
  - `#[derive(Debug)]` is missing on `FieldNames<'a>` (`script_api.rs:~655`).
- [ ] **Comment defects**:
  - `headless.rs:25-27`: the doc of `PACKAGES` sits on `LIFE`.
  - `sim_update/tests.rs:98-100`: a doc sentence breaks in the middle.
- [ ] **Build warnings without `--all-features`** — `cargo test -p campfire-capabilities` without features gives 14 warnings: unused `stats::internals` fns and `pub` items in `combat::internals` that nothing can reach.

## 9. Decisions for you

These items depend on rules that the style guide does not settle. The harness work in group 2 needs answers to the first three.

1. **How a module harness extends `TestMatch`.** "`impl` blocks only in the struct's own file" stops a module from adding methods to `TestMatch`. The choices are a wrapper with a named field (`walk.sim.step()`), or a wrapper with `Deref` / `DerefMut` to `TestMatch`. `Deref` used as inheritance is an anti-pattern, but it stays in test code. Both reviews of capabilities recommend the named field.
2. **A home for a harness type.** "Test code sits at the end of the production file it reaches into" gives no home to a type that reaches into nothing, such as `TestMatch`, `HashTrail`, `FixedSession` or `Arena`. The `reference_3v3.rs` file is already a gated file of its own. Confirm that a gated file of its own (`test_match.rs`, `hash_trail.rs`, `fixed_session.rs`) is the rule. If it is not, `capability_set/mod.rs` must move its `mod tests` to `tests.rs` to make room.
3. **Sharing across crates.** `runner` needs `TestMatch`, so the gate becomes `any(test, feature = "internals")`, with an export from `campfire_capabilities::internals`. It is a feature edit in runner's dev-dependencies, not a new dependency.
4. **A `tests/` directory for `mode/tests.rs`.** The rule names only `foo/{mod.rs, tests.rs}`. The layer rule stops the production, progression and damage tests from moving down, so the split needs `mode/tests/*.rs`.
5. **`#[derive(Default)]` on `ModifierData`** adds to its public API. The other choice is a test constructor in `internals`.
6. **`Num` from an integer in tests.** Choose one: a production `const fn Num::int(i64) -> Num` that panics on overflow, or a gated one in `math` `internals`. The gated one needs `campfire-math` with `internals` in the dev-dependencies of 8 crates.
7. **The silent skip of integration tests.** `runner` and `net` set `required-features = ["internals"]`. Plain `cargo test -p campfire-runner` builds 0 tests and skips all 9 integration tests with no message. The verification chain uses `--all-features`, so it is not affected. A self dev-dependency with `internals` fixes it, but that is a manifest change. You can also accept the skip.
8. **A size floor for the 40 % split rule.** `hook_set.rs` (42 lines), `tick_rate.rs`, `log_line.rs`, `segment.rs` and several files in the group 8 table pass 40 % only because they are small.
9. **proptest in capabilities.** It is a workspace dependency, but only `math` uses it. The tables in 5.2 give most of the value without it. Adding it to `campfire-capabilities` dev-dependencies needs your approval.
10. **The proptest seed policy** (group 6).

## Order of work

1. Group 1: the net round trip pin, the failures across ticks in `reference_abilities`, the respawn test, refused casts, the empty asserts and the fixture defects.
2. The decisions in group 9, items 1–3.
3. `TestMatch` as the running harness, with its helpers beside their types (2.1, 2.2). Then the module harnesses become thin wrappers.
4. `LocalMatch` methods, `HashTrail` and `FixedSession` (2.4, 2.5). `HashTrail` then serves net, runner and verifier.
5. The exact readers and assertions (5.1), the crate-wide state sweep (group 4), and the delete in group 7.
6. The cross-checks and boundaries (5.2), the content decoupling (group 6), and layout (group 8).
