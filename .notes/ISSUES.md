# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Net.** On macOS CI, about one run in two of `local::local_server::tests::a_client_plays_a_local_server_which_publishes_the_log_as_it_drops` passes its 10 s deadline, where a passing run takes 2.5 s; it did not fail in 140 runs on Linux, under CPU load, with slow file syncs, or with long and irregular frames. The deadline's message names the wait it ended.

## Ready

- **Orders.** A unit type's `orders.ai` names only a script of its own package, so modes that share an AI keep a copy each: the test packages' `tower_ai.rhai` is three identical files, and their `creep_ai.rhai` three that drifted apart.
- **Script.** A script cannot test whether another unit's body comes within a reach of a unit's edge when either body is a box, whose `radius` reads `()`: the MOBA and proving creep AIs measure their aggro range centre to centre, where `nearest_visible`, which picks their targets, measures it from the edge.
