# Plan questions

## The worst cases

**Item.** The worst tick of every reference 3v3 match is its first: 3.9 ms to 4.5 ms in three matches, against about 1.3 ms for the next, the waves' ticks. Tick 0 pays the sim schedule's first build, which a real server pays in its first tick too, and the pathing grid's first labels of the map's static bodies ([Hot stages](docs/design/13-benches.md#hot-stages), Inputs). So `server_tick/worst_3v3` and `server_tick/worst_3v3_checkpointed` measure only tick 0. No checkpoint copies there, so the checkpointed case cannot show the copy's cost, though design 10's Cost line reads it from that case ([runner's issue log](docs/issues/runner.md)). Found in the review of B11.

**Options.**

1. **Split the start from the running match.** `worst_3v3` and `worst_3v3_checkpointed` take the worst of ticks 1 to 5,999, and a new `server_tick/first_3v3` times tick 0 alone. The worst cases then show the waves' ticks and the copy; the start keeps its own figure. The stage cases' worst stays as it is.
2. **Also take the schedule's build out of the tick.** Option 1, and the server builds the sim schedule as its session starts, so the first tick pays only its own work. This changes `net`'s and `runner`'s start, outside the benches.
3. **Keep the cases.** The worst tick is the budget, and the first tick is one of them; the record states that the worst cases measure the start.

**Recommendation.** Option 1. A case must measure what its id says: the worst of a match that runs, and the cost of the checkpoint copy that the checkpointed case is for. Option 2 is worth its own system step later, as no server needs the build inside a tick.

**Blocked.** Nothing in the plan: the record states what the worst cases measure until the choice is made.
