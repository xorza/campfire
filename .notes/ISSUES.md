# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Net.** On macOS CI, about one run in two of `local::local_server::tests::a_client_plays_a_local_server_which_publishes_the_log_as_it_drops` passes its 10 s deadline, where a passing run takes 2.5 s; it did not fail in 140 runs on Linux, under CPU load, with slow file syncs, or with long and irregular frames. In CI run 40307695871 it waited for the replay's end, the log's next tick 40: the watcher, watching from tick 36, stood before tick 41, one past the log's end, which the wait never meets.

## Ready

- **Orders.** A unit type's `orders.ai` names only a script of its own package, so modes that share an AI keep a copy each: the test packages' `tower_ai.rhai` is three identical files, and their `creep_ai.rhai` three that drifted apart.
- **Script.** A script cannot test whether another unit's body comes within a reach of a unit's edge when either body is a box, whose `radius` reads `()`: the MOBA and proving creep AIs measure their aggro range centre to centre, where `nearest_visible`, which picks their targets, measures it from the edge.
