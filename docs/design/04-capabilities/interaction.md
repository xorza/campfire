# Interaction

## Mechanism

A unit uses another unit near it: one mechanism for doors, the bomb, loot, capture points, shrines, NPCs, vehicles and garrisons, the `use` and `enter` action kinds ([Kinds](actions.md#kinds)), aimed at a unit whose type has a `use` or an `enter` section. Its first cut, the `use` kind, is built, as [Uses](../13-bodies-uses-shop-gravity.md#uses) gives it; `enter` is planned. The capability adds the two sections and their books, and no system: `orders` fills a use slot, and the casts of `abilities` run it, so a `use` action needs both.

## Data

- **The object:** `use = { action, users }`: an action of its package of the `use` kind, and a filter of the units that may use it, by how the object's team regards the user's and the user's tags, `all` by default. The action gives the range, in meters, the time, a held use's channel and what interrupts it, as `damage_interrupts`. An `enter` section (planned): how many it holds, of which filter, and whether they act from inside.
- **The user:** `uses = { kind }`, the mode's slot kind of one action slot that holds the use under way, of one rank, which its units spawn with empty, after their other slots of that kind; a type without it uses nothing.

## Rules

- **Use.** The order `use = target` fills each of its units' use slot with the object's action at rank 1, ready, unless it holds that action already, and runs it as that slot's order: it walks in range, checks, winds up and resolves, and its channel is a held use. The object gives the action, so each object brings its own outcome, as Dota 2's outposts give each hero a hidden capture ability. A unit with no use slot, or that `users` does not select, drops the order, and so does a unit whose player's vision group does not see the object; an order of the use slot by its index starts nothing. Load checks: a `use` action has one rank, a range in meters, an aim at a unit, an instant delivery and no `cooldown_ms`, `charges`, `toggle`, `charge` or passive modifier, and needs `abilities`; no type's slots or choice put an action in a `uses` kind, which is of one rank and no inventory's kind; a `use` section names a `use` action of its package, and only a `use` section does: no slots, no item and no loadout entry. A unit holds its use slot within its 256 slots.
- **Held uses** end by a new order, a block of `use` ([Tags](stats.md#tags)), and not one of casting alone, death, and with `damage_interrupts` by damage ([Channels](actions.md#over-time)); `on_channel_tick(ctx, unit, target)` takes the object, so a held use can add to its progress.
- **Shared progress** is the object's script state, which each user's channel adds to.
- **Doors** change level geometry, pathing and vision blockers when they open or close.
- **Enter and exit:** a unit rides or garrisons inside a vehicle, a transport or a building, acts from inside when the host allows, and exits beside it.
- **Dialogue** is a use of a unit with topics ([Quests](quests.md)); trading uses `items` shops; picking a lock is a held use whose outcome the mode's script decides.

## State and derived

- **State:** each object's progress and users; who is inside each host.

## Script API

The action hooks of the objects' actions; `unit.inside`, `unit.passengers`.

## Network

An object's state goes to everyone who sees it; a dialogue only to its player.

## Cost

Each held use costs its channel a tick.

## Genres

A shooter's bomb and doors; a battle royale's loot, doors and vehicles; an RTS's garrisons and transports; an MMO's quest givers, doors and mounts; a MOBA's shrines and neutral objectives.
