# Control

## Mechanism

- **Orders and characters start the same actions.** `orders` runs units that take orders, `character` units a player drives; both start actions through the one pipeline ([Actions](actions.md)).

## Orders

- **Every capability's orders are its commands.** Each capability decodes and applies its own orders in Inputs, in the module list's order, those of `orders` last, so a buy and a cast of the bought item in one tick both apply; players, bots and AI issue the same orders.
- **A given order is an event.** `orders` gives every order of every source and triggers `UnitOrdered` for it, which each capability that keeps a loop of the unit's observes to set or end its own, production its build and gather, interaction the use slot; so `orders` writes no part of theirs. Refused: `orders` writing production's and interaction's parts, which tied it to both crates; and each capability decoding its own build, gather and use commands, which changes the command protocol and the order of two orders to one unit in a tick. As Bevy's observers carry a change to the systems that own what it touches.
- **A group is one command.** An order names its units in increasing stable id, each its player controls carries it out, and a move of many shares one route ([Groups](navigation.md#groups)).
- **An unseen target is ignored.** An order's unit target must be one its player's group sees as it applies, so a stable id never tells a client what its group does not see ([Replication audiences](../10-replication-audiences.md)).
- **A player out of play gives no orders** when players out of play see what a spectator sees, as an order would act on what its group does not see.
- **A blocked order waits** and runs when the block ends.
- **A stop spends nothing.** A stop ends what a unit has under way with nothing spent; a train queue stays.
- **Order queues, `hold`, attack-move and patrol are planned,** and an idle unit's target acquisition as native code from data, with the jungle leash as data, each with the slice that needs it.
- **A directive is generic over its references.** Each directive type names its units, slots, places and item types through `OrderRefs`, ids by default: a command carries the ids, a bot's script the names, and one exhaustive map for each type gives the one for the other, so a variant added to a directive reaches the scripts with no list of its own. Refused: a named copy of the directives in `bots`, which a new variant leaves behind; the script's own keys, which named a part of the orders.
- **A bot's script names what it orders.** A slot by its action's name, found as the order is due among each commanded unit's slots, as a grant or an item moves them, as Dota 2's `GetAbilityByName`; a target by a filter, the nearest living unit it selects from the order's first unit of those its team sees, as an order aims only at those, as Dota 2's `GetNearbyCreeps`; a place by a marker's name, which its map holds once, as OpenRA's `Map.NamedActor`, or by its coordinates. Refused: slot places and stable ids, which hold only for one spawn order; a slot found at the load; a marker by its tag and place.

## AI

- **AI thinks in staggered turns.** `on_think` runs every `think_ms`, staggered by stable id; a unit whose call finds the think pool spent stays due, so under load AI thinks later and no unit loses its turn.
- **AI sees the stage's start.** Every call of a tick's Think sees the units as the stage began, and queries sort by stable id.
- **AI orders only its own unit,** checked as queued and applied when the call returns.
- **A reset as a jungle monster's.** `order_reset` walks a unit to its spawn, taking no order on the way, and refills it as it arrives, as League of Legends' jungle monsters hard-reset.
- **An avatar has no AI.** A player controls it, and a bot plays it through player inputs.

## Character

- **One input frame a tick, predicted.** Planned: a player drives a unit with one input frame a tick, keys, view angles, buttons and the tick its screen showed; a capsule controller moves it, and its client predicts it with the same code.

## Choices

- **Choices before the spawn.** A mode declares what players choose, a hero, a class, a faction or a loadout, each from avatars or loadout entries, unique or not; the choices are state, and a unit a player owns plays on the player's team, so it names the player's vision group.

## Network

- **A client predicts what its player drives and the start of its orders;** the rest is interpolated.
