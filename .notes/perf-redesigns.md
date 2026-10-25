# Performance redesigns

Redesigns that the optimization session of 2026-10-25 spiked, each measured against `afddf65d8` on the bench cases; the numbers and the method are in [the perf journal](../docs/research/perf-journal.md#session-of-2026-10-25). Each spike lives uncommitted in a worktree of its own beside the repository, for a read of its diff. None is in `master`; the session's exact changes are, as `078b2c269`.

## A counted fog

Done; its decisions are in [vision.md](../docs/design/04-capabilities/vision.md), A stage that costs what changed.

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
