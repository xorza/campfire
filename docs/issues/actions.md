# Actions

Design: [Actions](../design/04-capabilities/actions.md). Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- **Stage 7.** The `spawn` effect has no unit type to spawn from an avatar or a loadout: those packages hold delivery types alone, and `ctx.spawn_unit` takes only the mode's own types and avatars. A summon also has no timed life to end it.

## Ready

- **Plan: M4.** The load takes a projectile's `collide` and `sight_radius` and ignores them, and refuses a `vision` section on a delivery type, where the design gives `hits = "none"` and a delivery's `vision`.
- **Plan: M5.** The load refuses the `launch` effect as planned.
