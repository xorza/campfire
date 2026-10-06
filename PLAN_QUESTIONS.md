# Plan questions

## The mean cases

**Item.** B7's check, that the nine stage means add up to less than `server_tick/mean_3v3`, passed by criterion's medians: 164.5 µs against 171.3 µs. But it is not reliable. Each sample of a mean case times a stretch of ticks that criterion sizes, 600 to 6,000, carried on from where the last sample stopped, so each covers a different part of a match: the ten samples of `server_tick/mean_3v3` gave 75.8 µs to 245.5 µs a tick, as the quiet start and the fights fall in them. The median moved from 155.8 µs to 171.3 µs between two runs of the same tick, and by criterion's mean estimate the stages added up to 159.9 µs against 158.5 µs ([Record](docs/design/13-benches.md#record), "The mean cases"; [runner's issue log](docs/issues/runner.md)).

**Options.**

1. **One iteration is one whole match, and the case states `Throughput::Elements(6,000)`**, so criterion reports a tick's mean over whole matches, as the worst cases already play them. Exact to its id; each of the ten mean cases then plays whole matches, about 55 s each on a bench run, and in CI's test mode 1.5 s to 2 s each, about 17 s more, past the 45 s the bench step may take.
2. **Each mean case also takes its worst from the same matches**: one case for each stage and the tick, `server_stage/3v3_<stage>`, whose iteration is a match, reporting the mean through criterion and the worst as a printed figure. The mean and the worst then come from the same matches; criterion holds only one, so the worst leaves its regression check.
3. **Keep the cases, and state their spread.** The record says a mean case agrees with another to about 10 %, and the check of the stage sum is dropped. Nothing changes in CI.

**Recommendation.** Option 1. A case must measure what its id says, and the mean of a match is what the stages' share of a tick is read from. CI's limit then needs to rise to about 60 s, or the worst cases and the mean cases share their matches in CI's test mode only.

**Blocked.** Nothing in the plan: the record states the spread until the choice is made.

## Kernels for the hot stages (B8 to B10)

**Item.** The investigation of B8 to B10 is done and written into design 13, [Hot stages](docs/design/13-benches.md#hot-stages): the path each stage spends its time on, and the kernel it proposes. You chose to review each proposal before its kernel is built.

- **B8, Inputs:** its worst is tick 0 alone, the map's static bodies entering the pathing grid; `pathing_grid/build` and `pathing_grid/one`.
- **B9, Move:** its worst is each wave's spawn, 24 creeps planning their lane routes; `route_planner/crowded` and `route_planner/spread`.
- **B10, Resolve:** its cost is the damage pass's `calc_damage` hook and the script view's reads, and `View::read` is 14.2 % of the whole run; `script_view/read`.

**Options.**

1. **Accept the proposals as written**, and the loop builds the five cases.
2. **Change them**: another scene, another case, or no kernel for a stage. Inputs' worst comes once a match, at its start, so its kernel measures what a later static body costs more than what the match pays.

**Recommendation.** Option 1, with `script_view/read` first: it is the largest path with no kernel, and it serves Think as well as Resolve.

**Blocked.** The kernels of B8, B9 and B10, which stay in the plan.
