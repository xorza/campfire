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
