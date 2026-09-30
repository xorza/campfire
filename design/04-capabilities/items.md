# Items

Things a unit carries, equips or finds. One mechanism for MOBA items, shooter weapons and grenades, battle royale loot and MMO gear.

- **Item types** in data: stack size, slot kind, the stats and modifiers it grants while carried or equipped, the abilities it grants (an active item, a grenade), or the `hitscan` weapon it is.
- **Inventory:** slots per unit, set by the unit type; equipment slots (weapon, armor, trinket) and bag slots. Items move between slots, stack and split.
- **World items:** an item on the ground is an entity; `interaction` picks it up, and a unit drops what it carries. Loot spawns from the seeded RNG.
- **Shops:** buying and selling for a player resource, where and when the mode allows (a MOBA base, CS buy time, an MMO vendor).
- **Commands:** use, equip, move, drop, buy, sell. Checks run in the sim, like casts.

## Sent to clients

A unit's own inventory goes to its owner; equipped items that show (a weapon in hand) go to everyone who sees the unit.
