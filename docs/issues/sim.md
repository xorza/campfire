# Sim

Design: [Modules](../design/02-engine-core.md#modules), `sim`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- Nothing calls `World::clear_trackers`, so Bevy's removed-component messages are never updated: each component a unit loses, and each component of each entity despawned, stays in them for the whole match, and a persistent world grows them without bound.

## Ready
