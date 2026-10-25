# Performance redesigns

Redesigns that the optimization session of 2026-10-25 spiked, each measured against `afddf65d8` on the bench cases; the numbers and the method are in [the perf journal](../docs/research/perf-journal.md#session-of-2026-10-25). Each spike lives uncommitted in a worktree of its own beside the repository, for a read of its diff. None is in `master`; the session's exact changes are, as `078b2c269`.

## A counted fog

Spike: `../campfire-perf/fog` (`git -C ../campfire-perf/fog diff`).

- **The cost.** Each Vision stage clears every group's bitmap and reveals every seer's sight again, row by row, whether the seer moved or not: `Fog::sight` and `Bitmap::set` take 6.2 % of a 3v3 run's samples, and the vision stage 55 ms of a 520 ms match. Buildings, towers, wards and idle units, which never move, pay it every tick; a Zero Hour base holds dozens.
- **The change.** Each cell keeps the count of each group's sights on it, and of each group's detectors, beside the bitmap, whose bit stays set while its count is above 0. A seer keeps its runs, group, detection and brush; a seer whose key is unchanged costs nothing, one that moved counts on the cells its rows gained and takes back those they lost, and one that changed its group, detection or brush, or left, takes back all its runs. A reveal counts for its tick and is taken back as the next stage begins.
- **Established practice.** Zero Hour's `PartitionManager` keeps a count of lookers per cell and player and has each object look and unlook as it moves; Warcraft III and StarCraft II keep reference-counted sight the same way. Refused: the bitmaps rebuilt each tick, as now, whose cost grows with every seer however still.
- **Same bits.** The fog's bitmaps stay a pure function of the seers and the reveals: the spike's test plays 200 random ticks of 40 seers over three groups, two brushes, detection and reveals, and finds every cell's bit equal to a fog built again each tick. A restored match fills it from nothing, as now.
- **Numbers.** `local`, quiet, four interleaved pairs, each pair the same way: `battle/crowded_vision` −64.1 %, `server_stage/mean_3v3_vision` −32.8 %, `server_tick/mean_3v3` −3.8 %; `fog/still` −81 %, `fog/sight`, every seer moved, −10 %. The worst 3v3 tick does not move (−0.3 %). `mac`, three pairs, each the same way: `battle/crowded_vision` −60.0 %, `mean_3v3_vision` −34.5 %, `server_tick/mean_3v3` −3.3 %.
- **Costs.** Two `u16` counts a cell and group beside each bitmap, 26 KB for each group on the 3v3's 96 × 136 m map at 1 m cells; a seer's runs kept between ticks. A seer that moves every tick pays its rows' ends twice, which the bitmaps' clear no longer costs.
- **Left for the change.** The spike leaves `SightCache<Sighting>`, the per-tick bitmap clear and `Bitmap::set` in place unused, and has run only the vision crate's tests; the change deletes them, runs the gate, and moves the cross-check into `sight_maps`' tests.

## A target index split by team and tags

Spike: `../campfire-perf/targets` (`git -C ../campfire-perf/targets diff`), on top of the filter-first test that `078b2c269` holds.

- **The cost.** The script view keeps one grid of every target, and `ctx.find`, `ctx.find_visible` and `ctx.nearest_visible` walk every body within reach, then test each against the filter: a creep's `ctx.find(…, "allies:avatar")` walks the hundreds of creeps around it to find the few heroes. In the battle, the walks and their reach tests take 9 % of the samples.
- **The change.** The index keeps a grid for each class of targets, a team and its exact tag set, built at the same read; a query tests each class once against its filter and the relation of its team, and walks only the grids of the classes it selects. A battle holds about ten classes.
- **Established practice.** Physics engines filter a spatial query by its layer mask before any distance test, as Unity's and Godot's layers do; the layers here are the classes the filters name. Zero Hour's `PartitionFilter` chain tests its filters only after the distance, as the spike's base does. Refused: a grid for each tag, which holds a unit once for each of its tags, and filters that intersect them.
- **Same results.** `find` sorts its rows and `nearest` takes the least of (distance, stable id, row), so neither depends on the walk's order; the spike changes no result.
- **A decision it needs: the charge.** Each body the grid gives counts one step toward the call's limit and its pool's budget (`TargetIndex::each_reached`). The split gives fewer bodies, so a query charges less, and a script near its budget runs further than now; still deterministic, but a change to the operation counts every replay holds. The options: charge the bodies walked, as the spike does, which makes a selective filter cheap for scripts too; or charge a fixed cost a query, as of its reach, which drops the dependence on the crowd altogether.
- **Numbers.** Against `afddf65d8`, the filter-first test included: `mac`, three interleaved pairs, each pair the same way: `battle/crowded_think` −24.3 %, `server_stage/mean_3v3_think` −4.35 %, `server_tick/mean_3v3` −1.94 %; `local`, four pairs: `battle/crowded_think` −22.1 %, `mean_3v3_think` −6.76 %, `server_tick/mean_3v3` −4.00 %; the vision and resolve stages inside their floors. The filter-first test alone, which `078b2c269` holds, gives about a quarter of the think stage's gain: `battle/crowded_think` −4 to −6 %.
- **Costs.** A grid for each class, kept between reads: about ten small grids in place of one, rebuilt at the same reads.

## Rhai 1.27.0 costs the tick 2 %

Found by the second session, after `9de47b915` upgraded Rhai to 1.27.0 and wrote the Zero Hour golden file again for its one more bot-think operation a play. A decision, not a redesign.

- **Numbers.** The tip on 1.27.0 against the same tip on 1.26.1, quiet, each pair the same way: a native call from a script costs 9 % more on `local` and 6 % more on `mac`; the battle's think stage 3.5 % and 4.2 % more; the 3v3's think stage 3.4 % and 2.9 % more; the whole 3v3 tick 1.8 % and 2.0 % more. The worst 3v3 tick does not move.
- **The options.** Pin `=1.26.1` again and write the golden file back, which keeps the tick's 2 % and the old operation counts; or keep 1.27.0 for what it brings, and take the 2 % and the new counts. Rhai 1.27.0 ships no changelog in its crate, and its source rewrote the evaluator and the AST, so what it brings needs a read of its release notes first.

## Spiked and dropped

- **Rhai's function resolutions kept from call to call.** A patched Rhai whose call takes the caller's caches, one set for each script: the think stage −1.2 %, the 3v3 tick −0.2 %, inside its floor. Most of `resolve_fn`'s time is the hash of each call's argument types and the probe of a cache that already holds the function, which every call pays; a fork of Rhai for it is not worth its upkeep.
