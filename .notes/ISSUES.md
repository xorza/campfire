# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Navigation.** A blocked mover's speed is its brake alone, where Zero Hour's `doLocomotor` caps it at a bump limit too, which falls 5 % each frame the mover stays blocked and, once it is free, rises 5 % each frame from a fifth of its speed or more (`AIUpdate.cpp:2219-2238`); the limit, read on this engine's brake or on a touch, stalls plays of the Zero Hour check, and still does with a Zero Hour mover stuck by its block ([Crowd movement](../docs/research/crowd-movement.md)).

## Ready

