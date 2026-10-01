# Items

## Mechanism

Things a unit carries, equips or finds. One mechanism for MOBA items, shooter weapons and grenades, battle royale loot and MMO gear. An item is a unit of a type with an `item` section: on the ground it stands in the world; carried, it sits in its carrier's inventory and grants what its type says.

## Data

A unit type's `item` section: stack size, the inventory slot it takes (an equipment slot or a bag), and what it grants while carried or equipped: modifiers, held as passives from the carrier ([Modifiers](stats.md#modifiers)); actions, in the item's slot ([Actions](actions.md)); and pools, such as a gun's ammunition. A carrier's type gives its `inventory`: equipment slots (weapon, armor, trinket) and bag slots. The mode's shops: where and when each sells, and the player resource it costs.

## Rules

- **Inventory:** items move between slots, stack and split. An equipped weapon's attack action is the carrier's weapon; a grenade is a cast action with a projectile delivery.
- **World items:** an item on the ground is a unit; a use action picks it up ([Interaction](interaction.md)), and a unit drops what it carries. Loot spawns at markers from random tables, rolled on the secret stream ([Random tables](00-overview.md#random-tables)).
- **Shops:** buying and selling for a player resource, where and when the mode allows (a MOBA base, CS buy time, an MMO vendor).
- **Crafting** is the `craft` action kind: a recipe (`[recipes.<id>]`) names the items and pools it takes, the station it needs, if any, as a filter on a unit near, and what it makes: an item, or modifiers added to an item it takes, as Skyrim's enchanting and smithing improve an item. An item's added modifiers are its state, so a save and a trade keep them.
- **Owners.** An item's holder is the unit that carries it. In a world, an item of a tradable type also has an owner, a player's main key, which stays with it when it is stored, dropped or held by a follower; only its owner may sell it or trade it. A type's `tradable` is off unless the mode turns it on, and `bound` makes an item its first owner's for good.
- **Trade** between two players in a world is an order of each: each offers items and player resources, both confirm, and the sim swaps them in one tick, all or nothing. A sale for sats is the `payments` module's ([Item sale](../05-protocol-spec.md#item-sale)): its reservation and transfer are recorded inputs, so the world's log proves the item's history.
- **Commands:** use, equip, move, drop, buy, sell, craft, trade, each an order. Checks run in the sim, as an action's do.

## State and derived

- **State:** each item unit, its stack, pools and added modifiers, its owner and any sale that holds it; each inventory's slots.
- **Derived:** the modifiers and actions an item grants, from its type, while it is carried or equipped.

## Script API

`ctx.give_item(unit, type)`, `ctx.drop(unit, slot)`; `unit.items`; an item is a unit handle with the fields of its sections.

## Network

A unit's own inventory goes to its owner; equipped items that show (a weapon in hand) go to everyone who sees the unit. Items on the ground replicate as units.

## Cost

Each inventory change costs a refresh of its carrier. Items on the ground cost what units that do not move cost.

## Genres

A MOBA's shop items and wards; a shooter's guns, grenades and armor; a battle royale's loot; an MMO's gear and consumables; Diablo's drops with random modifiers; Skyrim's smithing, alchemy and enchanting.
