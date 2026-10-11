# Items

## Mechanism

Things a unit carries, equips or finds: MOBA items, shooter weapons and grenades, battle royale loot, MMO gear. A carried item is an entry of its carrier's inventory, not a unit: its type, stack and uses, as League of Legends keeps an item in a slot. An item on the ground is a unit, planned with world items, which picking it up turns into an entry.

## Data

- **Item types:** the mode package's `[items.<id>]`; the load refuses one in an avatar or a loadout package. `name`, a message id ([Human text](../03-game-scripting.md#game-package)); `cost`, whole amounts of player resources, `{ gold = 1100 }`; `components`, the items of its package it is built from, as League of Legends' and Dota 2's recipes; `stack`, how many share a slot, 1 by default; `uses`, a consumable's, one spent as its action resolves, the item going with the last; `modifiers`, held from the carrier while carried; `action`, the item's active.
- **Inventories:** a carrier's type gives `inventory = { slots, kind }`: at least one slot, and the slot kind of one rank whose action slots they fill, one each, after the kinds before it ([Actions](actions.md#data)). A slot with no item, or an item with no action, holds none.
- **Shops:** the mode's `[shop]`: `items`, the types it sells, each costing only its resource; `resource`, the player resource it takes; `at`, a marker tag whose regions of the carrier's team are its shops; and `sell_share`, the share of a cost a sale returns, from 0 to 1, a decimal of at most nine digits kept exactly, so 70 % of 300 is 210. An item costs at least its components, in each resource.

## Rules

- **Orders:** `buy`, `sell` and `swap` apply in Inputs, in input order, to a unit of the player's with an inventory, so a later one sees what an earlier one changed; one that fails a check is dropped.
- **Buying:** `buy = id` buys an item the shop sells, for a unit in one of its shops or dead, as League of Legends lets a dead player shop. It pays the cost less the components it holds, which it gives up, each counted once; the item joins a slot of its type with room in its stack, else the first empty slot once the components left. An order the player cannot pay, or with no room, is dropped.
- **Selling:** `sell = slot` gives up the slot's item where the unit could buy, for `sell_share` of its cost times its stack, rounded down.
- **Moving:** `swap = { from, to }` swaps two slots anywhere at any time; an active keeps its cooldown in its new slot.
- **Actives:** an item's action sits in its slot's action slot at rank 1, filled afresh, ready and with no charges, when an item of another type comes into the slot, and empty when none is; no unit type or choice puts actions in an inventory's kind, which the load checks. A consumable's spends a use as it resolves, with its cost and cooldown. An item's action is in the `use` group, whatever its kind: a stun stops it and a silence does not, as in League of Legends and Dota 2.
- **Passives:** a carried item holds its modifiers from the carrier at rank 1; two items with one modifier hold it once, as unique passives. No unit type's `passive` and no action's `passive_modifier` is an item's modifier, which the load checks.
- **Death:** a unit keeps what it carries.

### Planned

Equipment, world items, loot, crafting, owners and trade come with the imported games that need them.

- **Equipment** that shows, as a weapon in hand, goes to everyone who sees its unit.
- **Crafting** is the `craft` kind: a recipe (`[recipes.<id>]`) names the items and pools it takes, the station it needs as a filter on a unit near, and what it makes, an item or modifiers added to one, as Skyrim's enchanting does. Added modifiers are the item's state.
- **Owners.** In a world, an item of a tradable type has an owner, a player's main key, kept when it is stored, dropped or held by a follower; only its owner may sell or trade it. `tradable` is off unless the mode turns it on, and `bound` makes an item its first owner's for good.
- **Trade** between two players is an order of each: each offers items and resources, both confirm, and the sim swaps them in one tick, all or nothing. A sale for sats is the `payments` module's ([Item sale](../05-protocol-spec.md#item-sale)).

## State and derived

- **State:** each inventory's slots, each an item type, its stack and its uses; later each item on the ground, its added modifiers, its owner and any sale that holds it.
- **Derived:** the modifiers and actions an item grants while carried; a recipe's cost less its components.

## Script API

None in the MOBA's cut: orders buy, sell and move items. Later `ctx.give_item(unit, id)` and `ctx.drop(unit, slot)`.

## Network

A unit's inventory goes to its owner, or as the mode sets ([Replication audiences](../14-replication-audiences.md), `inventory`). A client predicts no buy or sale: its gold and slots come from the server.

## Cost

Each inventory change costs a refresh of its carrier. Items on the ground cost what units that do not move cost.

## Genres

A MOBA's shop items and wards; a shooter's guns, grenades and armor; a battle royale's loot; an MMO's gear and consumables; Diablo's drops with random modifiers; Skyrim's smithing and enchanting.
