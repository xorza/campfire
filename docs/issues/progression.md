# Progression

Design: [Progression](../design/04-capabilities/progression.md). Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

- **Stage 9.** Perks are designed in part: [Data](../design/04-capabilities/progression.md#data) does not say which unit types may take a perk (every unit with points, or only those whose type lists it), or whether perks may exclude each other, as Dota 2's talent pairs do. The code has no perks, and `ctx.grant_perk` and `unit.has_perk` are planned.

## Research

## Ready

- **Plan: P3.** Units have no points: design 04's progression gives a unit with the `level` track a point for each level it has, and `unit.points`, `unit.track_level` and `unit.xp` are planned.
- **Plan: P4.** No order learns a rank: only the mode's `ctx.learn` does, and [Learning](../design/04-capabilities/progression.md#rules) gives the player the `learn` order.
