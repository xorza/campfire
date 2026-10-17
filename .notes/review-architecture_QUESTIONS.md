# Questions

## Q18: How does a due index stay true when a write does not come from the pass (7.1)?

**Background.** The plan says that a process pushes into its `DueIndex` where it sets a time. Three kinds of writes do not go through those places:
- A restore builds a new world.
- A client's rollback writes each predicted part in place, through `write_component_with_change_detection`, with no hook. Lightyear predicts only mutable parts (`prepare_rollback_component<C: Component<Mutability = Mutable>>`, `lightyear_prediction/src/rollback.rs`).
- A unit that a client starts to predict again (`Unpredicted` removed) keeps the times that replication wrote.

A push only at the places that set a time misses all three, and an attack or a cast then never resolves on the client. The client runs some timed passes: `Volley::collect`, `Dying::respawn` and `Holds::hold_charges`.

**Options.**

| Option | What it does | Cost |
| --- | --- | --- |
| A. Immutable due parts with hooks | Each due tick is its own immutable part. Its `on_insert` hook pushes, so every write pushes. | Lightyear cannot predict an immutable part, so this fails for the predicted passes. It also moves archetypes, or needs one more part on each unit for each process. |
| B. Push where set, rebuild at a rewind (recommended) | Each process pushes where it sets a time. An index that sees a tick earlier than its last take was rewound or restored, so it rebuilds itself with one scan. It also takes the units that a client predicts again. In debug builds, each take scans everything and asserts that no due unit was missed. | Every place that sets a time must push. The debug audit finds a missed push in the tests. A rollback costs one scan for each index. |
| C. Change detection feeds the index | Each consuming pass reads `Changed<T>` on the part that holds its time, as `DueRun` does now, and pushes those units. | It never misses a write, but it reads the change tick of every unit in each tick (about 1 ns for each unit). That is still a scan, which K-13 asks to remove, but with no logic for each unit. |

**Blocked.** 7.1 waits for this answer. 7.4 (`SiteProgress` and the route cursor as their own parts) changes the same parts, so it waits too.

## Q19: Do the index lookups of C-14 and K-21 stay as they are (7.2)?

**Background.** The plan asks for `Single` for the match entity and a dense table of the players by slot (C-14), and for a cached `SystemState` in the damage pass (K-21). Both findings said that the cost needs a measurement first. A profile of the 3v3's mean tick (`docs/research/server-tick-profile.md`) shows no lookup of the entity index's B-tree on its own, each under 0.5 % of the samples. The damage pass's own reads of the units are 0.13 % of the samples.

**Options.**

| Option | What it does | Cost |
| --- | --- | --- |
| A. Drop both (recommended) | The lookups stay. The plan loses the two steps, and the profile is the record of why. | None now. A larger match could make them show, and a new profile would find it. |
| B. Do both | `OnMatch` reads the match entity through `Single`, a resource holds each player's entity by slot, and the damage pass keeps a `SystemState`. | About a day of work across `players`, `actions`, `combat` and every reader of `OnMatch` and `PlayerRows`, for no measured gain. `Single` skips a system while the match entity is missing. |

**Blocked.** Only these two steps of 7.2.

## Q20: Do the gather searches and the static following stay scans (7.3)?

**Background.** R-5 asks that the gather loop's node and drop-off searches use the unit grid, and that each node keep its queue of waiting workers. R-30 asks that room making and the following of a static change query only the bounds of the change. Measured on 2026-10-18 (`./drive.py bench -- integration/gather` at `12a98cb63`), a whole tick of the gather bench, 200 workers at 16 nodes in two fields, costs 20.5 µs. Room making and the static following run only as a box spawns or a static body comes or goes, and then visit each walker once.

**Options.**

| Option | What it does | Cost |
| --- | --- | --- |
| A. Keep the scans (recommended) | The plan loses R-5 and R-30, and the measurement is the record of why. | A Zero Hour economy of hundreds of nodes would make the node search grow with nodes times workers, and a new bench would show it. |
| B. Do both | The searches query the unit grid, a node keeps its queue as state, and the static following queries the change's bounds. | The unit grid must be fresh at each search, or a search misses a unit that moved since its last build. A node's queue changes the snapshot's format. The following of a static change needs an index of the routes' legs. |

**Blocked.** Only R-5 and R-30 of 7.3. R-6 of 7.3 waits for Q18.
