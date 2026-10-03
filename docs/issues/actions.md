# Actions

Design: [Actions](../design/04-capabilities/actions.md). Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- **Stage 7.** The `spawn` effect has no unit type to spawn from an avatar or a loadout: those packages hold delivery types alone, and `ctx.spawn_unit` takes only the mode's own types and avatars. A summon also has no timed life to end it.

## Ready

- **Plan: P2.** A slot kind's `levels` loads as any list of numbers: [Data](../design/04-capabilities/actions.md#data) needs one level for each rank, each at least 1 and at least the one before, on a kind with `ranks`, in a mode with a `level` track. The reference 3v3 gives no levels, where design 07 gives the basic and ultimate ranks theirs.
- **Stage 16.** The projectile section's `gravity` is planned: no projectile falls, so no grenade arcs and no bullet drops.

