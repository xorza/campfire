# Items

## Mechanism

Things a unit carries, equips or finds. One mechanism for MOBA items, shooter weapons and grenades, battle royale loot and MMO gear. A carried item is an entry of its carrier's inventory, not a unit: its type, its stack and its uses, as League of Legends keeps an item in a slot; an item on the ground is a unit, planned with world items, which picking it up turns into an entry and dropping it back.

## Data

- **Item types:** the mode package's `[items.<id>]`, as its shop and its inventories name them; the load refuses an avatar or a loadout package that holds one. `cost`, whole amounts in its mode's player resources, `{ gold = 1100 }`; `components`, the items of its own package it is built from, as League of Legends' and Dota 2's recipes; `stack`, how many share one inventory slot, 1 by default; `uses`, a consumable's, which its action spends one of as it resolves, the item going with the last; `modifiers`, of its package, held from the carrier while it carries the item; and `action`, an action of its package, the item's active.
- **Inventories:** a carrier's unit type gives `inventory = { slots, kind }`: how many slots, at least one, and the mode's slot kind whose action slots they fill, one each, in order, after the kinds before it ([Actions](actions.md#data)), a kind of one rank, as an item's action has; a slot with no item, or an item with no action, holds none. The type's own slots and its inventory's are no more than a unit holds.
- **Shops:** the mode's `[shop]`: `items`, the item types it sells, each costing only in the resource it takes; `resource`, the player resource it takes; `at`, a marker tag: the regions of the carrier's team's markers with that tag are its shops ([Space and map](00-overview.md#space-and-map)), so each such marker has a region and a team; and `sell_share`, the share of an item's cost a sale gives back, from 0 to 1, `0`, `1` or a decimal string of at most nine digits, kept exactly, so 70% of 300 is 210. An item costs at least what its components cost, in each resource, so no recipe's price falls below nothing.

## Rules

- **Orders:** `buy`, `sell` and `swap` apply in the Inputs stage, in input order, to a unit of the player's that has an inventory, so a later one in a tick sees what an earlier one changed; one that fails a check is dropped.
- **Buying:** the order `buy = id` buys an item the shop sells, for a unit its player controls that has an inventory and stands in one of its shops, or is dead, as League of Legends lets a dead player shop. It pays the item's cost less the cost of the components it holds, which it gives up, each counted once, as both games build a recipe; the item joins a slot of its type with room in its stack, else the first empty slot once the components left. An order the player cannot pay, or with no room, is dropped.
- **Selling:** `sell = slot` gives up the item in the slot, where the unit could buy, for `sell_share` of its cost times its stack, rounded down to a whole amount.
- **Moving:** `swap = { from, to }` swaps two slots, anywhere and at any time; an active on cooldown keeps its cooldown in its new slot.
- **Actives:** an item's action sits in its slot's action slot at rank 1, and its order is the slot's, as any action's ([Actions](actions.md#the-pipeline)); a consumable's spends a use as it resolves, together with its cost and its cooldown. An item's action is in the `use` group, whatever its kind: a tag that blocks `use` stops it, and one that blocks `cast` does not, as a stun stops an item in League of Legends and Dota 2 and a silence does not.
- **Passives:** a carried item holds its modifiers from the carrier, as an action's passive is held ([Modifiers](stats.md#modifiers)); two of one item hold each modifier once, as League of Legends' unique passives.
- **Death:** a unit keeps what it carries.
### Planned

Equipment slots, world items, loot, crafting, owners and trade come with the genres that need them: the shooter's and battle royale's stage 8, and the RPG's and MMO's stage 9.

- **Crafting** is the `craft` action kind: a recipe (`[recipes.<id>]`) names the items and pools it takes, the station it needs, if any, as a filter on a unit near, and what it makes: an item, or modifiers added to an item it takes, as Skyrim's enchanting and smithing improve an item. An item's added modifiers are its state, so a save and a trade keep them.
- **Owners.** An item's holder is the unit that carries it. In a world, an item of a tradable type also has an owner, a player's main key, which stays with it when it is stored, dropped or held by a follower; only its owner may sell it or trade it. A type's `tradable` is off unless the mode turns it on, and `bound` makes an item its first owner's for good.
- **Trade** between two players in a world is an order of each: each offers items and player resources, both confirm, and the sim swaps them in one tick, all or nothing. A sale for sats is the `payments` module's ([Item sale](../05-protocol-spec.md#item-sale)): its reservation and transfer are recorded inputs, so the world's log proves the item's history.

## State and derived

- **State:** each inventory's slots, each an item type, its stack and its uses; later, each item on the ground, its added modifiers, its owner and any sale that holds it.
- **Derived:** the modifiers and actions an item grants, from its type, while it is carried; the cost of a recipe less its components, from the item types.

## Script API

None in the MOBA's cut: its packages read no item, and orders buy, sell and move them. Later, `ctx.give_item(unit, id)` and `ctx.drop(unit, slot)`; an item on the ground is a unit handle with the fields of its sections.

## Network

A unit's own inventory goes to its owner; equipped items that show (a weapon in hand) go to everyone who sees the unit. Items on the ground replicate as units. A client predicts no buy or sale: its gold and its slots come from the server.

## Cost

Each inventory change costs a refresh of its carrier. Items on the ground cost what units that do not move cost.

## Genres

A MOBA's shop items and wards; a shooter's guns, grenades and armor; a battle royale's loot; an MMO's gear and consumables; Diablo's drops with random modifiers; Skyrim's smithing, alchemy and enchanting.
