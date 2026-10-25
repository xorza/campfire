# Campfire — Replication audiences

Who receives each part of the state, and how a mode and its host set it. A mode sets the rules between players, as League of Legends shows a hero's items to all and Zero Hour a player's money to no other player; the host sets what spectators and players out of play see, as Counter-Strike's server overrides its game mode's `mp_forcecamera` and a Dota 2 lobby's host admits spectators ([Spectating](../research/spectating.md)). Each mistake of this kind is impossible to write or fails at once.

## Recipients and kinds

- **Four recipients.** For an entity and a client: the **owner**, the client of the player slot that owns it; an **ally**, another client of its team; an **enemy**, a client of another team; a **spectator**, a client with no player slot. The owner is in every audience but the server's, so its prediction always holds what it reads.
- **Each state type names its kind.** A state type implements `Replication` with a `KIND` and a `Sending` and no default, so a type with no kind does not compile; the kinds are a closed list of at most 32, as replicon's filters number:

| Kind | Holds |
| --- | --- |
| `unit` | A unit's public state: type, team, owner, place, body, level, death, destination, the action under way, status tags, a site and its progress |
| `life` | The life pool |
| `pools` | The other pools |
| `modifiers` | A unit's modifiers |
| `cooldowns` | Each slot's ready tick, charges, toggle payments |
| `progression` | Experience and points |
| `inventory` | A unit's inventory |
| `prediction` | What only the owner's prediction reads |
| `resources`, `supply`, `queues`, `player_modifiers` | A player's |
| `match` | The match's public state: relations, its end, who is out of play |
| `mode_state` | The state of the mode's own script |
| `unit_state_owner`, `unit_state_team`, `unit_state_all` | A unit's script state of each `audience` but `none` |
| `player_state_owner`, `player_state_team`, `player_state_all` | A player's script state of each `audience` but `none` |
| `server` | What never leaves the server |
| `commanded` | The units a player's orders command |

- **The commanded units are their player's prediction.** They are a kind of their own, `commanded`, sent to their player alone and set by no mode, as an enemy's selection tells another player what it plans, and a predicting client applies its own selects as it applies its orders, so its prediction commands what the server will; a select replaces or adds units, so a replay of one gives the same units. Refused: `prediction`, which a client receives only of the units it predicts, and a player's entity is none; state the client keeps apart from the match, which a rollback to the server's state leaves behind.
- **One kind for each type.** A filter hides whole components, so a component of two kinds is split along them, as `Pools` into `Life` and `Pools`, and `ActionSlots` into `ActionSlots` and `SlotTimes`.

## What the mode sets

- **Each kind has a default audience, and a mode may set it.**

| Kind | Default audience | The mode may set |
| --- | --- | --- |
| `unit`, `match` | everyone | nothing: vision decides who holds a unit |
| `life`, `modifiers`, `player_modifiers`, `mode_state` | everyone | enemies, spectators |
| `pools`, `cooldowns`, `progression`, `inventory`, `resources`, `supply`, `queues` | owner | allies, enemies, spectators |
| `unit_state_*` | by `audience` | spectators |
| `prediction`, `commanded` | owner | nothing |
| `server` | none | nothing |

- **A mode's hidden data is not predicted.** Where a mode hides a unit's modifiers from enemies, an enemy's client cannot derive its stats, so an aura of it comes from the server as a correction; the defaults hide nothing a prediction reads.

## What the host sets

- **Three layers.** The engine's defaults, the mode's `[visibility]`, and the host's settings for what the mode leaves to it, as Unreal marks the game mode properties a server's ini may set; the server refuses to start on a setting the mode does not leave it, and the session's terms carry the result, so every player signs it and the log proves what each client could hold.
- **The host admits spectators, and their fog is a setting.** A live spectator can tell a player what it sees, which League of Legends' delay of 180 s and Dota 2's of 120 s exist to stop; with no delay yet, the default admits none. A spectator sees every unit through any fog, or one team's view or all teams', as a Dota 2 spectator chooses its fog.

## Players out of play

- **A player out of play sees what its team sees, or what a spectator sees.** A player is out of play while every avatar it holds is dead, or while the mode holds it out, as an eliminated player is; when it sees what a spectator sees, the server ignores its orders, which would act on what its team does not see, and the setting enters the terms so the verifier ignores them alike.

## Where state lives

- **A resource never replicates.** The network stack replicates entities' components, so state a client receives lives on an entity.
- **The match and each player are entities.** The match's start spawns one entity for the match and one for each player slot, as Unreal keeps a `GameState` and a `PlayerState` for each player, each with a named stable id past the allocator's; a player's resources, supply and modifiers are its components, each of its kind, so each player learns its own resources and no other's by default.
- **State reaches a client only by replication.** No state type goes by a message, so no code must remember to send one; messages carry the protocol alone.

## One list

- **Each capability names its state types once.** A module's declarations list every type it adds, and the install, the network's registration and the data filters all read that list; each type declares how it is sent, once, on insert, on change, predicted, or never.
- **One replicon filter for each kind.** Units keep their vision rooms; the player and match entities stand in every team's room, so every placed link and spectator receives them.
- **An entity spawns with its replication.** On the server, each kind's marker requires `Replicate`, empty `Rooms` and, of a kind its owner predicts, a `PredictionTarget` of no link, so an entity spawns whole; its show after the sim tick it came in fills its rooms and its owner's link in place, as an observer of the marker lists it, so no link receives it before the tick it replicates in, and a link's `LinkOf` requires its `ReplicationSender`. Refused: the parts inserted at the show, a second move of each entity between archetypes; and the rooms filled as the marker comes, which showed the match's entities in the frame that opened the match, before its first tick, and left the client's match unstarted.
- **An owner changes by an insert alone.** `Owner` is immutable, so a change of owner is a new insert each filter follows by its observers, and no code writes an owner in place, where a filter would keep the old one.
- **A unit's owner is a player of its team.** The owner's client then holds its unit wherever its team sees it, with no path around the team's vision; a change of owner to another team moves the unit with it, as a capture does in Zero Hour and Dominate in Dota.

## What a client predicts

- **A client derives what it can for every unit it holds.** Stats and tags derive from what reaches every client by default, so the client derives them for each unit it holds, as the server does.
- **A client writes only what it predicts or derives.** A type sent on change and not predicted takes the server's older value at each change, so a client's own write of it shows as a jump back, as a growing site's life showed as a hit; a type the client must write is predicted, which a rollback compares.
- **Prediction reads only what its client holds.** A test walks the client's prediction schedule and checks that each component a system reads is derived on the client, of a kind every holder receives, or read on its own predicted units alone, and names each failure.

## The checks

- **A leak test over every type and two settings.** A scripted 3v3 in process, with spectators of each fog and a player out of play among its clients, checks after each tick that each client holds each component on exactly the entities its audience admits, once with the defaults and once with every setting at its other extreme.

## Planned

- **A spectator delay,** by a spectator that plays the session's log behind it, as Valve's GOTV relays a match after `tv_delay`, which needs no rooms.
- **Sending a kind only to the clients that use it,** as Unity's `SendTypeOptimization` does, once bandwidth is measured.
