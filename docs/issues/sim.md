# Sim

Design: [Modules](../design/02-engine-core.md#modules), `sim`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- Nothing calls `World::clear_trackers`, so Bevy's removed-component messages are never updated: each component a unit loses, and each component of each entity despawned, stays in them for the whole match, and a persistent world grows them without bound.

## Ready

- A snapshot carries no data version: the [engine core](../design/02-engine-core.md) says a release raises one whenever the snapshot's format changes, but no code writes or checks it, so a snapshot of an older layout of a component, a `Route` or a `PathWalker`, fails to decode with no word of its version, or decodes into other fields.

- `StateChanges` keeps the change tick of the last copy, which Bevy's check of the world's change ticks does not clamp: when the next copy comes more than 2³² change ticks later, that tick's age wraps, it reads as recent, and the copy misses values that changed before it.
