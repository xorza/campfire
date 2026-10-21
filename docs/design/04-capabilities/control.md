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
- **Order queues, `hold`, attack-move and patrol are planned,** with the jungle leash as data, each with the slice that needs it.
- **An idle unit scans for a target, as its type declares.** A unit type's `acquire = { scan_ms, targets }` makes a unit that stands idle, with no destination, nothing under way and no attack target, take as its attack target, each `scan_ms` and staggered by stable id as `on_think` is, the nearest unit its team sees, center to center, that one of `targets` selects and that the first of its weapons that selects it reaches, the lower stable id at one distance. Practice: Zero Hour's `getNextMoodTarget`, as a human player's unit runs it, and OpenRA's `Defend` stance, a human player's default ([Zero Hour automatic attack](../../research/zero-hour-auto-attack.md)). Refused: a Rhai `on_think` for each unit, which an army of hundreds of idle units would call at each scan; a range of the scan's own past the weapons' reach.
- **A scanned target holds only in reach.** An attack target keeps its source, an order or the scan; the chase walks to an ordered one and drops a scanned one that no weapon of the unit reaches any more, for a unit that walks or stands, so it scans again at its next turn. Practice: Zero Hour fails a human player's attack of `CMD_FROM_AI` out of its pursuit, and OpenRA's `Defend` attacks without moving. Refused: a chase of a scanned target, which a bot gives by its orders where its game wants one.
- **The scan is the server's.** Only a role that computes state scans; a predicting client takes the target from the server's state and corrects to it. Refused: a client's own scan, which reads the enemies at their interpolated places, behind the server's, and so mispredicts anyway.
- **A unit that scans neither builds nor gathers.** The load refuses `acquire` on a type with no weapon, and on one with a build or a gather, whose loops stand a unit idle between their steps. Refused: a test of each capability's loop in the scan, which would tie `orders` to `production`.
- **Retaliation, a turret's own scan on the move and stances, planned.** Each with the slice that needs it: an idle ally's attack on a unit that damaged one of its team, a turret that takes its own target while its unit walks, and a player's choice of a unit's stance.
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

- **Choices before the spawn.** A mode declares what players choose, a hero, a class, a faction or a loadout, each from avatars, loadout entries or values its data lists, unique or not; a choice of listed values takes one, which a session's host may pick for a seat before the start, as Zero Hour's and OpenRA's lobbies give each player a faction ([Sessions](../08-sessions.md#decisions)); the choices are state, and a unit a player owns plays on the player's team, so it names the player's vision group.

## Network

- **A client predicts what its player drives and the start of its orders;** the rest is interpolated.
