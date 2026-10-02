# Test review

When you address an item, delete it. When a group is empty, delete its heading.

Paths are relative to `source/crates/`. Line numbers are at `c38f0da`. Each item gives the place, the problem and a better shape. The groups are sorted by severity and benefit. Items about production code that a test review found, and that `REVIEW.md` already holds (for example `PackagePath` and `./`), are not repeated here.

## Summary

- The harness work is done: `TestMatch` runs a match for every capability's tests, and data constructors sit beside their types. What is left is listed below, smallest first in each group.
- No test is over 1 s when the machine is idle (group 7).

## 2. Harnesses

### 2.5 Net

- [ ] **The round trip pinned to the link model** — `net/src/local_match/delay_line.rs` pins the measured round trip to zero, and `mod.rs` adds the modeled round trip to the sync margin. Better: pin the round trip to the link model's own value, and remove the `jitter_margin` workaround in `LocalMatch::client`. That step needs new derivations of the expected values. Also check that the join order (`play_by_team`) is then fixed.
## 3. Helpers copied across crates

- [ ] **SplitMix64** — 5 copies: math `num/bench.rs:18`, net `delay_line.rs:80-87`, capabilities `regions.rs:390` and `broadphase.rs:170`, and sim `golden.rs:30`. The golden copy is frozen on purpose, so keep it. The regions and broadphase copies can draw from `campfire_math::Rng` through a `Grid::scatter(&self, &mut Rng, num, den)`. That changes the generated scenes: check the `crowded > 100` and `wide > 100` floors in broadphase again. The collision bench's history also restarts.
## 4. One style for one check

- [ ] **Script failures are read in three ways still** — `CallError::kind` and `ScriptFailures::calls` with `assert_eq!` serve most sites now. Left: the abilities table of fn pointers (its overflow case reads what a script raised), mode's `failures()`, and orders' `think()`.
- [ ] **Positions are written in three ways** — `at(Num, Num, Num)` (abilities, with many lines of `Num::ZERO, Num::ZERO`), `at(i64, i64, i64)` (combat, orders), and `at(x, z)` (projectiles, vision, mode). Halves and quarters are `Num::HALF` and `Num::QUARTER` now. Do not change a floored fraction such as `(9 << FRAC_BITS) / 10` to `"0.9".parse()` without new derivations, because the parser rounds to nearest and the shift floors.
- [ ] **The stats `unit()` returns an `Entity`** (`stats/tests.rs`), where every other harness spawns through `TestMatch::spawn` and gives a `StableId`. Its units have no position, which `spawn` takes.
- [ ] **One hero runs at two rates** — the reference heroes run at 30 Hz in `reference_abilities` and at 20 Hz in the 3v3, and no test checks timing across rates. For example, Eruption's 625 ms is 19 ticks at 30 Hz and 13 ticks at 20 Hz. Low priority.
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
