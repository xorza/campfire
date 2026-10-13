# Control

## Mechanism

Who starts a unit's actions and moves it, and what players choose before their units spawn: `orders` for units that take orders, `character` for units a player drives ([Control](00-overview.md#control)). Both start the same actions ([Actions](actions.md)).

## Data

A unit type's `orders = { ai = "scripts/creep.rhai", think_ms = 500 }` names its AI; `character = { height, radius, step_height, jump_speed }` makes it a unit a player can drive. The mode's `[choices.<name>]` declare what players choose ([Choices](#choices)).

## Rules

### Orders

Move, attack, an action (by its slot, with a target), stop, hold (planned), follow a path, and the orders other capabilities add: `use` ([Interaction](interaction.md)), production's `cancel_train`, `cancel_build` and `rally` ([Production](production.md#rules)), items' `buy`, `sell` and `swap` ([Items](items.md#rules)), and progression's `learn` ([Progression](progression.md#rules)), which apply as the tick's inputs do, not as a unit's action. Each is a command of the capability that adds it, which decodes and applies it with no `orders` installed ([Commands](00-overview.md#commands)), and each capability's orders apply in Inputs in the order of the module list, those of `orders` last: a buy and a cast of the bought item in one tick both apply. Players, bots and AI issue the same orders. `orders` runs them: it asks `navigation` for a route, and starts actions through the pipeline.

- **Queues** (planned): an order waits behind the current ones (shift-click).
- **Groups:** a command names its units in increasing stable id, and each that its player controls carries it out; an empty list, one out of order or one that names a unit twice is ignored. A move to many units moves them as a group sharing one route ([Groups](navigation.md#groups)); formations and flow fields come later.
- **Stop:** a unit ends what it has under way, an attack, a cast in its windup or walking in range, or a charge, with nothing spent; cuts a channel; drops its target and destination; leaves its path. A train queue stays.
- **Out of sight.** An order's unit target is one its player's group sees as the order applies, any unit in a match with no `vision`; an order at any other is ignored, so a stable id never tells a client what its group does not see. A predicting client takes each unit it holds as seen, as the server sends it only those; what each group sees stays the server's ([Replication audiences](../14-replication-audiences.md), D12).
- **Out of play.** In a session whose players out of play see what a spectator sees, `out = "spectators"`, each order of a player out of play is ignored, as it would act on what its group does not see; its client predicts it ignored ([Replication audiences](../14-replication-audiences.md), D16).
- **Kept behind a block.** An order a tag blocks waits, and runs when the block ends ([Tags](stats.md#tags)).

### AI

A unit type names an AI script; `on_think(ctx, unit)` runs every `think_ms`, rounded up to whole ticks, in Think.

- Think times are staggered by stable id: a unit first thinks in the first tick whose number leaves its id's remainder by its period, then a period after each think. The units due longest think first, then by stable id. A unit whose call finds the think pool spent stays due, so under load AI thinks later, and no unit loses its turn.
- AI sees only game queries and the sim RNG; results are sorted by stable id, and every call of a tick's Think sees the units as the stage began. Without `vision`, every unit is visible.
- Orders go through `ctx`, checked as they are queued: an AI orders only the unit that thinks, and an attack needs a weapon whose filter selects the target. They apply when the call returns, and not at all when it fails. `order_follow_path` drops the target, and the unit walks back to the waypoint it had not reached. `order_move(unit, pos)` drops the target and walks to `pos`, within the bounds; a unit on a path leaves it until `order_follow_path`. An AI type loads only when its script has `on_think`.
- **Reset.** `order_reset(unit)` drops the target and walks the unit to `unit.spawn_pos`, off any path, as a League of Legends jungle monster's hard reset: it takes no order until it arrives, and as the next Think begins, before any unit thinks, its life and pools are full again. A unit that dies on the way stops resetting, unhealed.
- The MOBA test content's creep, tower and camp AI are ordinary scripts.
- An avatar takes no AI, and the load refuses one with `orders`: a player controls it, and a bot plays it through player inputs.

### Character

Planned. A player drives a unit with one input frame per tick: movement keys, view angles (yaw, pitch), buttons (jump, crouch, and one for each action slot the mode binds), and the tick the player's screen showed. A button starts its slot's action, aimed where the character looks. The controller moves a 3D capsule against level geometry, with gravity, jumping, stairs, slopes, crouching and ladders, on its layer; clients predict their own character every frame with the same code.

### Choices

A mode that lets players choose before their units spawn, a hero, a class, a faction, a loadout, declares each choice:

```toml
[choices.hero]
offers = "avatars"   # the avatar packages the mode depends on
unique = true        # no two players choose the same
[choices.spells]
offers = "loadout"   # the entries of the mode's loadout
count = 2            # the values a player chooses; 1 by default
slot = "spell"       # the slot kind the entries fill
```

A choice of loadout entries names the slot kind they fill, and every such choice fills a kind of the same ranks. `ctx.choose(player, choice, values)` records a player's choice from a mode input: exactly `count` values, of which one may be given alone, none twice, each one the choice offers, and with `unique` none another player chose; a second choice replaces the first. `ctx.chosen(player, choice)` lists the values, empty before a choice, and `ctx.available(player, choice, value)` tells whether no one else chose a unique value. The choices are state. `ctx.team_of(player)` names a player's team. `ctx.spawn_unit(type, team, pos, player)` spawns a unit owned by `player`, who must play on `team`, so a unit a player orders names the player's vision group; it spawns when the call ends, and the call gets a new unit for `ctx.grant` and its `.state` ([New units at once](../08-script-api.md#rules)). `ctx.grant(unit, slot_kind, ids)` puts loadout entries in a slot kind.

## State and derived

- **State:** each unit's orders, its AI's next think; each player's choices; later each driven unit's last input frame and the controller's state.

## Script API

`ctx.order_move`, `ctx.order_attack`, `ctx.order_action`, `ctx.order_follow_path`, `ctx.order_reset`; `ctx.choose`, `ctx.chosen`, `ctx.available`, `ctx.team_of`, `ctx.spawn_unit`, `ctx.grant`; the hook `on_think(ctx, unit)`.

## Network

Position and animation go to everyone who sees the unit. The client predicts what its player drives and the start of its own orders; everything else is interpolated.

## Cost

Each due AI costs one `on_think` call, under the think pool; each order its route request once; each driven unit a controller step a tick.

## Genres

A MOBA and an RTS give orders; a shooter, a battle royale and an action MMO drive characters; a tab-target MMO and a first-person commander use both. Choices make every genre's pick, class, faction or loadout.
