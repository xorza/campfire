# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Navigation.** A mover braked to a stop by a unit that stands ahead along its facing takes a speed of 0 for its whole drive, so it neither backs nor turns toward a waypoint off its facing, and stands so for good: two of China's supply trucks of the bases play stand so by their supply center for 96 and 126 s, where Zero Hour blocks no unit that moves backwards and holds no unit that must turn as stuck ([Crowd movement](../docs/research/crowd-movement.md)). Zero Hour brakes a wheeled mover's speed to its blocker's too, and a wheel turns only while it moves (`Locomotor::moveTowardsPositionWheels`), so what frees Zero Hour's mover there is not known; its turn speed is a quarter of its locomotor's unbraked speed, where `ZeroHourMover` takes a quarter of the braked one.

## Ready

- **Client.** Systems of client modules that join every mode take combat's, production's, stats' and vision's books as `Option` (`view/effects/mod.rs`, `view/look_states/mod.rs`, `view/shot_stand_ins/mod.rs`, `command_panel/mod.rs`, `view/minimap/mod.rs`, `view/ghost/mod.rs`), where 02-engine-core.md (A client module list) has each part a module of the capability it needs.
- **Client.** For a model that shows a clip's rounds, each shot rebuilds the hide list as strings and matches every node by name (`look_states/draw_show.rs`, `unit_looks/mod.rs`, `unit_models/mod.rs`), and the command panel's `show` rebuilds its layout each frame, collecting a `Vec<Does>` for a look that places its buttons.
- **Navigation.** Soldiers that walk different ways brake each other to a stop, and collision parts them, where the slice decided that walkers that step pass those that step another way (entries-and-blocking.md, Walkers that step pass those that step another way, planned): GLA's Angry Mob stands stuck in the bases play 555 times of the 1 s samples, its members braked by one another ([Crowd movement](../docs/research/crowd-movement.md)).
- **Rules.** `every_rule::the_workspace_keeps_every_rule` takes 1.02 s alone, past the bound of 1 s: its rules parse or lex most of the workspace's files each on its own, as a parsed `syn` file is no `Send` value the rules could share, and on 32 threads the slowest, the files' import order, the test build's `internals` and the gates of the module declarations, take about 1 s each (measured 3 November 2026).
