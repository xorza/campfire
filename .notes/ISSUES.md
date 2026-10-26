# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Combat.** A turret carries every weapon of its slot kind, where Zero Hour's turret carries only the slots its `ControlledWeaponSlots` names: a weapon of another slot of a Zero Hour unit with a turret waits for the turret's aim, and its shots leave its barrels turned and pitched with the turret.
- **Client files.** A look's state may name a condition of `ConditionTest::Never`, a condition of Zero Hour that no test of the engine's says, which the client counts in its choice of a state: data of one game the engine carries, where AGENTS.md's Direction asks the import to resolve it.
- **Net.** On macOS CI, about one run in two of `local::local_server::tests::a_client_plays_a_local_server_which_publishes_the_log_as_it_drops` passes its 10 s deadline, where a passing run takes 2.5 s; it did not fail in 140 runs on Linux, under CPU load, with slow file syncs, or with long and irregular frames. In CI run 40307695871 it waited for the replay's end, the log's next tick 40: the watcher, watching from tick 36, stood before tick 41, one past the log's end, which the wait never meets.

## Ready

