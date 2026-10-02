# Actions

Design: [Actions](../design/04-capabilities/actions.md). Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

## Ready

- **Plan: F3.** Cinder's `chain_fire.rhai` writes `next.state` on the result of `ctx.projectile`, which returns `()`: every bounce of Chain Fire that finds a next target fails its `on_hit` call.
