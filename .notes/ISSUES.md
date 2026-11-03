# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Navigation.** A Zero Hour mover braked to a step above 0 that stands, its velocity cut below the still speed, counts no stuck tick, as its progress is measured by its speed of 0, so it never plans again; Zero Hour holds a blocked mover stuck only while it faces its path and its blocker stands (`AIUpdateInterface::processCollision`, `needToRotate`) ([Crowd movement](../docs/research/crowd-movement.md)).
- **Navigation.** A blocked mover's speed is its brake alone, where Zero Hour's `doLocomotor` caps it at a bump limit too, which falls 5 % each frame the mover stays blocked and, once it is free, rises 5 % each frame from a fifth of its speed or more (`AIUpdate.cpp:2219-2238`); the limit, read on this engine's brake or on a touch, stalls plays of the Zero Hour check ([Crowd movement](../docs/research/crowd-movement.md)).

## Ready

