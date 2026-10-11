# Interaction

## Mechanism

Planned. A unit uses another unit near it: one mechanism for doors, the bomb, loot, capture points, shrines, NPCs, vehicles and garrisons, the `use` and `enter` action kinds ([Kinds](actions.md#kinds)), aimed at a unit whose type has a `use` or an `enter` section. Its first cut, the `use` kind, is [Uses](../13-bodies-uses-shop-gravity.md#uses).

## Data

- **The object:** `use = { action, users }`: an action of its package of the `use` kind, and a filter of the units that may use it, `all` by default. The action gives the range, the time, a held use's channel and what interrupts it, as `damage_interrupts`. An `enter` section: how many it holds, of which filter, and whether they act from inside.
- **The user:** `uses = { kind }`, the mode's slot kind of one action slot that holds the use under way; a type without it uses nothing.

## Rules

- **Use.** The order `use = target` fills each of its units' use slot with the object's action at rank 1, ready, unless it holds that action already, and runs it as that slot's order: it walks in range, checks, winds up and resolves, and its channel is a held use. The object gives the action, so each object brings its own outcome, as Dota 2's outposts give each hero a hidden capture ability. A unit with no use slot, or that `users` does not select, drops the order. Load checks: a `use` action has one rank, an instant delivery and no `cooldown_ms`, `charges`, `toggle` or `charge`; no type's slots or choice put an action in a `uses` kind; a `use` section names a `use` action, and only a `use` section does.
- **Held uses** end by a new order, a block of `use` ([Tags](stats.md#tags)), death, and with `damage_interrupts` by damage; `on_channel_tick(ctx, unit, target)` takes the object, so a held use can add to its progress.
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
