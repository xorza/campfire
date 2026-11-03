# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

## Ready

- **Client.** Systems of client modules that join every mode take combat's, production's, stats' and vision's books as `Option` (`view/effects/mod.rs`, `view/look_states/mod.rs`, `view/shot_stand_ins/mod.rs`, `command_panel/mod.rs`, `view/minimap/mod.rs`, `view/ghost/mod.rs`), where 02-engine-core.md (A client module list) has each part a module of the capability it needs.
- **Client.** For a model that shows a clip's rounds, each shot rebuilds the hide list as strings and matches every node by name (`look_states/draw_show.rs`, `unit_looks/mod.rs`, `unit_models/mod.rs`), and the command panel's `show` rebuilds its layout each frame, collecting a `Vec<Does>` for a look that places its buttons.
- **Navigation.** A braked mover drives at the speed its brake lets it, turning by a quarter of that speed, so one braked to a stop by a unit that stands ahead along its facing neither backs nor turns toward a waypoint off its facing, and stands so for good: two of China's supply trucks of the bases play stand so by their supply center for 96 and 126 s, where Zero Hour's `locoUpdate_moveTowardsPosition` turns a blocked mover where it stands toward its goal by its locomotor's whole turn rate, whatever its speed, and `blockedBy` frees it once the unit lies off its cone ([Crowd movement](../docs/research/crowd-movement.md)).
- **Navigation.** Soldiers that walk different ways brake each other to a stop, and collision parts them, where the slice decided that walkers that step pass those that step another way (entries-and-blocking.md, Walkers that step pass those that step another way, planned): GLA's Angry Mob stands stuck in the bases play 555 times of the 1 s samples, its members braked by one another ([Crowd movement](../docs/research/crowd-movement.md)).
