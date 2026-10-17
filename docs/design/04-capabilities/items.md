# Items

## Mechanism

- **A carried item is an inventory entry, not a unit.** Its type, stack and uses, as League of Legends keeps an item in a slot; an item on the ground is a unit, planned with world items.
- **An item has a name.** An item type's `name` is a message id of its package's locale, as an avatar's is, and the load checks each locale holds it.
- **Shops are units.** A unit type's `shop` makes its units shops, as Warcraft III's shops and couriers are units; a shop is its type's data, not state.
- **A shop search visits the shop units.** The search for the nearest shop that a client's panel makes each frame visits an index of the units whose type has a shop: the install takes those its world holds, and observers of a unit's type keep it, as a type is inserted on every spawn, restore and replication and never changes in place, so its readers take it as `Res`. Refused: a scan of every living unit at each search, and a take by the first search, which made every reader hold `ResMut` (02-engine-core.md, A shared index refreshes in its own system).
- **A sale returns an exact share.** A shop's sale share is a decimal of at most nine digits kept exactly, so 70 % of 300 is 210.
- **A recipe gives back its components.** A buy pays the cost less the components the unit holds, which it gives up, as League of Legends' and Dota 2's recipes do.

## Rules

- **Orders in input order.** `buy`, `sell` and `swap` apply in Inputs in input order, so a later one sees what an earlier one changed.
- **The buyer filter decides who shops dead.** A shop whose filter selects a dead unit serves it from anywhere: `allies` lets a dead player shop, as League of Legends does, and `allies:!dead` does not, as Warcraft III does.
- **An item's action is a use.** It sits in its slot at rank 1, filled afresh, and is in the `use` group whatever its kind, so a stun stops it and a silence does not, as in League of Legends and Dota 2.
- **Unique passives.** Two items with one modifier hold it once.
- **A unit keeps what it carries at death.**

## Planned

- **Crafting** is the `craft` kind: a recipe takes items and pools at a station and makes an item or adds modifiers to one, as Skyrim's enchanting does.
- **Owners and trade.** In a world, a tradable item has an owner, a player's main key; a trade between players is all or nothing in one tick, and a sale for sats is the `payments` module's ([Item sale](../05-protocol-spec.md#item-sale)).

## Network

- **A client predicts no buy or sale.** Its gold and slots come from the server.
