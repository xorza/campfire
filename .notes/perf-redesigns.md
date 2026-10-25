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
