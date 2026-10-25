# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Targets by class

A creep's `find` walks only the heroes, not the hundreds of creeps around it: the script view's target index keeps a grid for each class of targets, and a query walks only the classes its filter selects. A batch in `script` alone, with the design it changes. The spike is `../campfire-perf/targets`, its numbers in [the perf journal](docs/research/perf-journal.md#session-of-2026-10-25): `battle/crowded_think` −22 % and −24 %, `server_tick/mean_3v3` −4.0 % and −1.9 %, `local` and `mac`.

### T1. A target grid for each class (script's view, its target index)

- [ ] `TargetIndex` keeps one grid of every target, and each query walks every body within its reach and tests each against its filter (`target_index.rs:18`, `:94`), though the filter reads only the relation of the body's team and its tag set → the index keeps a grid for each class, a team and its exact `TagSet`, each refreshed at the read that indexes them, so a class that stood still keeps its grid (`BodyGrid::refresh`); a query tests each class once against its filter and the relation of its team, and walks only the grids of the classes it selects. The classes stay between reads, an empty one cleared, so a read allocates nothing once the match has met its classes. Builds: a pass visits what changed.
- [ ] `visit` gives the bodies class by class, where it gave them in one grid's order (`target_index.rs:57`): `find` sorts its rows and `nearest` takes the least of (distance, stable id, row), so no result depends on the order, and the doc of `visit` says so.
- [ ] A query charges a step for each body the grids of its selected classes give, as "A list costs a step a row" (02-engine-core.md) says, so it charges less than one grid gave: `02-engine-core.md`'s decision names the class grids, and `06-script-api.md` gains "Targets are indexed by class": a grid for each team and tag set, its filter tested once a class, as a physics engine tests a query's layer mask before any distance, as Unity's and Godot's layers do; refused: a grid for each tag, which holds a unit once for each of its tags; one grid tested body by body, as Zero Hour's `PartitionFilter` chain tests after the distance; a fixed charge a query, which EIP-150 and Wasmtime's host fuel show lets a script make the engine work for free.
- [ ] `target_index.rs` gains its tests, so it becomes `target_index/{mod.rs,tests.rs}`.
- Checks: a test of random units of three teams and several tag sets, moved and retagged over several reads, whose `find` and `nearest` for a sweep of filters, reaches and teams equal one grid's walk tested body by body, and whose charge equals the count of bodies within reach among the selected classes, hand-computed for one case; a filter that selects no class charges nothing. The Zero Hour golden's `think_ops` and every pinned count of operations move once, and are written again with `CAMPFIRE_BLESS=1`. `local` and `mac`: `battle/crowded_think` and `server_stage/mean_3v3_think` gain as the spike's did; `server_tick/worst_3v3` and the Zero Hour worst ticks lose nothing beyond their floors.

## Foundations
