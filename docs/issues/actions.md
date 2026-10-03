# Actions

Design: [Actions](../design/04-capabilities/actions.md). Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- **Stage 7.** The `spawn` effect of an avatar's or a loadout's action has no unit type to spawn: those packages hold delivery types alone, and `ctx.spawn_unit` takes only the mode's own types and avatars.

## Ready

- **Stage 5.** A cast whose target is out of its range drops its order: `start_casts` stops the unit, where [The pipeline](../design/04-capabilities/actions.md#the-pipeline) and [Over time](../design/04-capabilities/actions.md#over-time) make it walk in range first, as an attack's chase does.
- **Stage 16.** The projectile section's `gravity` is planned: no projectile falls, so no grenade arcs and no bullet drops.

