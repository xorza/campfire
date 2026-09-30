# Control

Who moves a unit, and how. Both kinds go through the control relation of the [overview](00-overview.md#control).

## Orders

Units that take orders: move, attack, cast (ability, target), stop, hold. Players, bots and AI issue the same orders, and an order names the units it goes to. The capability runs them: it asks `navigation` for a path, `combat` for attacks and `abilities` for casts.

**AI.** A unit type names an AI script; `think(ctx, unit)` runs every `think_ms`, rounded up to whole ticks, in the Think stage.

- Think times are staggered by stable id: a unit first thinks in the first tick whose number leaves the remainder of its id when divided by its period, so the units of a type spread over the period, and then a period after each think. The units due longest think first, then by stable id. A unit whose call finds the think pool spent stays due, so under load AI thinks later, and no unit misses its turn for good.
- AI sees only game queries (units in a radius, the nearest visible enemy, recent attackers) and the sim RNG; results are sorted by stable id. Every call of a tick's Think sees the units as the stage began. Until a match has `vision`, every unit is visible.
- Orders go through `ctx`, which checks each as it is queued: an AI orders only the unit that thinks, and an attack needs a unit with an attack and a living enemy target. An AI type loads only when its script has `think(ctx, unit)`. They apply when the call returns, and not at all when it fails. `order_follow_lane` drops the target; the unit then walks back to the waypoint it had not reached.
- The reference MOBA ships creep, tower and camp AI as ordinary scripts.

## Character

Units a player drives directly, one input frame per tick: movement keys, view angles (yaw, pitch), buttons (fire, jump, crouch, use, reload, and the unit's abilities), and the tick the player's screen was showing.

The character controller moves a 3D capsule against level geometry, with gravity, jumping, stairs, slopes, crouching and ladders. Clients predict their own character every frame with the same code.

## Sent to clients

Position and the current animation go to everyone who sees the unit. The client predicts what its player controls directly, and the start of its own orders; everything else is interpolated.
