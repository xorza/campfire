# Interaction

## Mechanism

A unit uses another unit near it. One mechanism for doors, the bomb, loot, capture points, NPCs, vehicles and garrisons: the `use` and `enter` action kinds ([Kinds](actions.md#kinds)), aimed at a unit whose type has a `use` section.

## Data

A unit type's `use` section: range, who may use it (a filter), whether the use is instant or held for a time (plant, defuse, capture, revive), what interrupts a held use (moving, damage, a block), and whether progress is shared (a capture point). An `enter` section: how many it holds, of which filter, and whether they act from inside.

## Rules

- **Use** is an action aimed at the object; a held use is its channel, which the object's interrupts end, and a block of `use` too ([Tags](stats.md#tags)). Its effects and hooks are the action's: `on_resolve` when it completes, `on_interrupt` when it is cut.
- **Shared progress** is the object's script state, which each user's channel adds to.
- **Doors** change level geometry, pathing and vision blockers when they open or close.
- **Enter and exit:** a unit enters a vehicle, a transport or a building, and rides or garrisons inside; it acts from inside when the host allows; exit puts it beside the host.
- **Dialogue** is a use of a unit with topics ([Quests](quests.md)); trading uses `items` shops; a lock to pick or a pocket to pick is a held use whose outcome the mode's script decides from the player's inputs.

## State and derived

- **State:** each object's progress and users; who is inside each host.

## Script API

The action hooks on the object's actions; `unit.inside`, `unit.passengers`.

## Network

An object's state goes to everyone who sees it; a dialogue only to its player.

## Cost

Each held use costs its channel a tick.

## Genres

A shooter's bomb and doors; a battle royale's loot, doors and vehicles; an RTS's garrisons and transports; an MMO's quest givers, doors and mounts; a MOBA's shrines and neutral objectives.
