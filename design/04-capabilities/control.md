# Control

Who moves a unit, and how. Both kinds go through the control relation of the [overview](00-overview.md#control).

## Orders

Units that take orders: move, attack, cast (ability, target), stop, hold. Players, bots and AI issue the same orders, and an order names the units it goes to. The capability runs them: it asks `navigation` for a path, `combat` for attacks and `abilities` for casts.

**AI.** A unit type names an AI script; `think(ctx, unit)` runs every `think_ms`, rounded up to whole ticks, in the Think stage.

- Think times are staggered by stable id, so units do not all think in the same tick.
- AI sees only game queries (units in a radius, the nearest visible enemy, recent attackers) and the sim RNG; results are sorted by stable id. Orders go through `ctx`.
- The reference MOBA ships creep, tower and camp AI as ordinary scripts.

## Character

Units a player drives directly, one input frame per tick: movement keys, view angles (yaw, pitch), buttons (fire, jump, crouch, use, reload, and the unit's abilities), and the tick the player's screen was showing.

The character controller moves a 3D capsule against level geometry, with gravity, jumping, stairs, slopes, crouching and ladders. Clients predict their own character every frame with the same code.

## Sent to clients

Position and the current animation go to everyone who sees the unit. The client predicts what its player controls directly, and the start of its own orders; everything else is interpolated.
