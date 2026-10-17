# Questions of the architecture plan

Each question waits for the user's decision. The plan's items point here.

## Q10. Batch 5.3, one action pipeline: review of its proposal

- **Item:** 5.3, K-17, C-3 and E-5. The proposal is [action-pipeline.md](../docs/design/action-pipeline.md), written as Q7 decided.
- **Why it waits:** the batch flips `stats` above `actions` and moves the stat effects and the modifier ways into `stats`. AGENTS.md asks a change of this size for a design the user reviews before it is built.
- **Options:**
  1. **Accept the proposal (recommended).** One start and one commit in `actions`, each kind commits at its moment of today, four contracts in `units`, and no golden moves.
  2. **Accept it with one commit moment for every kind.** Trains and builds then pay at their end, as casts do. Cost: the goldens of every match with a train or a build move.
  3. **Change the proposal.** The user names the decisions to change.
- **Blocked:** 5.3, and 5.1, which needs 5.3's callbacks.

## Q11. Batch 5.5, R-31: the room data in `units` costs more than Q8 measured

- **Item:** 5.5, R-31, the second bullet. Q8 decided "the room data in `units`": the static bodies' index and the walls become types of `units`, which navigation fills, with the pathing grid's open-cell query.
- **Why it waits:** the open-cell query reads navigation's `PathingGrid` (`navigation/src/pathing_grid/mod.rs`, 311 lines), whose clearances hold the regions, the walls' cells and the static bodies' marks, built from `Terrain`, `Regions`, `Segment` and `WalkerKinds`; and the ring search is `RoutePlanner::nearest_open` (`route_planner/mod.rs:415`). The index, `BodyIndex` (484 lines, 387 of tests), reads `Segment` and `Walker`. Q8 named the index and the walls; the grid and its search were not measured.
- **Options:**
  1. **Move the room data, as Q8 decided.** `Walls`, `BodyIndex` with `Segment` and `Walker`, and the clearances' blocked cells with the ring search move to `units`; navigation keeps the regions and fills the rest. Cost: about 1,500 lines of navigation's core, with their tests and bench, move to `units`, which then holds navigation's pathing types.
  2. **The walls in `units`, the static bodies through the unit grid, the open cell as a one-shot system (recommended).** The walls are map data and move to `units`. The placement tests the static bodies through `UnitsNear`, which it reads already, among the bodies that do not walk. A train's spawn place asks for the open cell through a one-shot system that `units` declares and navigation registers, which the train's exclusive finish runs. Cost: the placement's test of static bodies reads the unit grid in place of navigation's index, so a golden may move where the two differ within a tick.
  3. **Keep the dependency.** `production` keeps `navigation` for the two reads. Cost: R-31 stays in part.
- **Blocked:** the second bullet of 5.5 alone.


## Q12. Batch 6.4, N-11: parallel signature checks with no measurement

- **Item:** 6.4, N-11, the part "`check` and `apply` split, with parallel checks across slots". The aux randomness part is done.
- **Why it waits:** N-11's cost is "Plausible": no bench measures the frame's signature checks. A BIP-340 check with libsecp256k1 costs some tens of µs, so a frame of 8 slots' packets takes well under 1 ms on one thread, an estimate that no bench of this code confirms. A fan-out to worker threads costs a wake for each, of a similar size, and the threads must come through `store`, which has no scoped parallel map yet. The plan's own scale says a "plausible" item needs a test or a measurement to prove it.
- **Options:**
  1. **Measure first (recommended).** Add a bench of `record_inputs` with 8 and 32 slots' packets. Split and fan out only when the checks take more than a set share of the frame's budget. Cost: one bench now, and the change later if the bench asks for it.
  2. **Split and fan out now.** `SessionLog::record` splits into `check` (`&self`, the signature) and `apply`. `store` gains a scoped parallel map, and the frame checks the packets of different slots on it and applies them in slot order. Cost: a new proxy API and thread wakes in each frame, with no measured gain.
  3. **Split only.** `check` and `apply` split, and the frame calls them in turn on one thread. Cost: the code changes shape for a gain that comes only with option 2.
- **Blocked:** the check-and-apply bullet of 6.4 alone.

## Q13. Batch 6.4, N-17, N-14 and N-13: the session log's redesign needs a proposal

- **Item:** 6.4, its first three bullets: `LogReplay`, the decode's signature policy and `JournaledLog` (N-17); the live log as state, and publication from the journal (N-14, D34); one encode for each record (N-13).
- **Why it waits:** the three change one type, `SessionLog` (1,247 lines, 2,156 of tests), its journal, the runner's session and replay, and net's restore, load and publication. Two of the plan's shapes do not fit the code as built:
  - A restore replays the log from the latest checkpoint and then runs on live, in the same `Session` resource, whose tick system pulls each tick's inputs from the session's own log. A `LogReplay` that borrows the log cannot live in that resource. The replay's state can leave the log only as a value the session owns beside it: the tick ends and a schedule of its own.
  - A live log of state only drops the entries before its latest checkpoint, but `SessionLog::changes` serves the bot driver every slot change since tick 0, and a load decodes the log's own past through `encode_through`. Both then read the journal or the checkpoint's carry.
  - A published file holds counts before what they count, so a bounded stream of the journal reads it twice: once to count, once to write, each load's cut applied.
- **Options:**
  1. **A proposal first (recommended).** I write `docs/design/session-log-state.md`, as 5.3's proposal was written, with the decisions above, the cost of each, and its checks, for review. The code waits for it. Cost: one review round.
  2. **The small parts now, the rest later.** The decode's signature policy becomes a parameter, and the journal with its durable heads moves into a value only the server's session holds. The state-only log, the publication and the one encode wait. Cost: two passes over the same code.
  3. **Keep the log's shape.** The replay mode becomes an explicit enum of the log, and the rest stays. Cost: N-14 stays against D34 and 10-sessions.md, which say the server publishes from the journal.
- **Blocked:** the first three bullets of 6.4.
