# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Navigation.** A Zero Hour mover braked to a step above 0 that stands, its velocity cut below the still speed, counts no stuck tick, as its progress is measured by its speed of 0, so it never plans again; Zero Hour holds a blocked mover stuck only while it faces its path and its blocker stands (`AIUpdateInterface::processCollision`, `needToRotate`) ([Crowd movement](../docs/research/crowd-movement.md)).

## Ready

- **Client.** Systems of client modules take combat's, projectiles', stats' and vision's parts as `Option`: the look states combat's weapons and production's producers (`view/look_states/mod.rs`), the shot stand-ins combat's barrels and turrets and projectiles' specs (`view/shot_stand_ins/mod.rs`), the effects combat's deaths (`view/effects/mod.rs`), the command panel stats' upgrades (`command_panel/mod.rs`), and the minimap and the ghosts the fog of vision's module (`view/minimap/mod.rs`, `view/ghost/mod.rs`), where 02-engine-core.md (A client module list) has each part a module of the capability it needs.
- **Client.** For a model that shows a clip's rounds, each shot rebuilds the hide list as strings and matches every node by name (`look_states/draw_show.rs`, `unit_looks/mod.rs`, `unit_models/mod.rs`), and the command panel's `show` rebuilds its layout each frame, collecting a `Vec<Does>` for a look that places its buttons.
- **Navigation.** A blocked mover's speed is its brake alone, where Zero Hour's `doLocomotor` caps it at a bump limit too, which falls 5 % each frame the mover stays blocked and, once it is free, rises 5 % each frame from a fifth of its speed or more (`AIUpdate.cpp:2219-2238`).
