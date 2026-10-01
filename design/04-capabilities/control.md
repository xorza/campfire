# Control

## Mechanism

Who starts a unit's actions and moves it, and what players choose before their units spawn. Both kinds of control go through the control relation of the [overview](00-overview.md#control): `orders` for units that take orders, `character` for units a player drives. Both start the same actions ([Actions](actions.md)).

## Data

A unit type's `orders = { ai = "scripts/creep.rhai", think_ms = 500 }` names its AI; `character = { height, radius, step_height, jump_speed }` makes it a unit a player can drive. The mode's `[choices.<name>]` declare what players choose ([Choices](#choices)).

## Rules

### Orders

Units that take orders: move, attack, an action (by its id, with a target), stop, hold, follow a path, and the orders of the action kinds other capabilities add (use, enter, train, build, gather). Players, bots and AI issue the same orders, and an order names the units it goes to. The capability runs them: it asks `navigation` for a route, and starts the actions through the pipeline.

- **Queues:** an order can wait behind the current ones (shift-click).
- **Groups:** one order to many units moves them as a group, in a formation, and keeps them together; many units share one path search.
- **Kept behind a block.** An order a tag blocks waits, and runs when the block ends ([Tags](stats.md#tags)).

### AI

A unit type names an AI script; `on_think(ctx, unit)` runs every `think_ms`, rounded up to whole ticks, in the Think stage.

- Think times are staggered by stable id: a unit first thinks in the first tick whose number leaves the remainder of its id when divided by its period, so the units of a type spread over the period, and then a period after each think. The units due longest think first, then by stable id. A unit whose call finds the think pool spent stays due, so under load AI thinks later, and no unit misses its turn for good.
- AI sees only game queries (units in a radius, the nearest visible unit a filter selects, recent attackers) and the sim RNG; results are sorted by stable id. Every call of a tick's Think sees the units as the stage began. Until a match has `vision`, every unit is visible.
- Orders go through `ctx`, which checks each as it is queued: an AI orders only the unit that thinks, and an attack needs a weapon whose filter selects the target. An AI type loads only when its script has `on_think(ctx, unit)`. They apply when the call returns, and not at all when it fails. `order_follow_path` drops the target; the unit then walks back to the waypoint it had not reached.
- The reference MOBA ships creep, tower and camp AI as ordinary scripts.

### Character

Units a player drives directly, one input frame per tick: movement keys, view angles (yaw, pitch), buttons (jump, crouch, and one for each action slot the mode binds: fire, use, reload, abilities), and the tick the player's screen was showing. A button starts the action of its slot, aimed where the character looks.

The character controller moves a 3D capsule against level geometry, with gravity, jumping, stairs, slopes, crouching and ladders, on its layer. Clients predict their own character every frame with the same code.

### Choices

A player owns any number of units, or none. A mode that lets players choose before their units spawn, a hero, a class, a faction, a loadout of weapons or spells, declares each choice:

```toml
[choices.hero]
offers = "avatars"   # the avatar packages the mode depends on
unique = true        # no two players choose the same
[choices.spells]
offers = "loadout"   # the entries of the mode's loadout
count = 2
```

`ctx.choose(player, choice, values)` records a player's choice, from a mode input as the mode reads it, and refuses a value the choice does not offer or, with `unique`, one another player chose; `ctx.chosen(player, choice)` and `ctx.available(player, choice, value)` read them. A unit spawns with `ctx.spawn_unit(type, team, pos, player)`, owned by `player`, and `ctx.grant(unit, slot_kind, ids)` puts actions in a slot kind of it. A MOBA's pick, a shooter's class and loadout, an RTS's faction and an MMO's character creation are choices.

## State and derived

- **State:** each unit's orders and queue, its AI's next think; each player's choices; each driven unit's last input frame and the controller's state (velocity, grounded).

## Script API

`ctx.order_move`, `ctx.order_attack`, `ctx.order_action`, `ctx.order_follow_path`, `ctx.order_reset`; `ctx.choose`, `ctx.chosen`, `ctx.available`, `ctx.spawn_unit`, `ctx.grant`; the hook `on_think(ctx, unit)`.

## Network

Position and the current animation go to everyone who sees the unit. The client predicts what its player controls directly, and the start of its own orders; everything else is interpolated.

## Cost

Each due AI costs one `on_think` call, under the think pool. Each order costs its route request once, and each driven unit one controller step a tick.

## Genres

A MOBA and an RTS give orders; a shooter, a battle royale and an action MMO drive characters; a tab-target MMO and a first-person commander use both. Choices make every genre's pick, class, faction or loadout.
