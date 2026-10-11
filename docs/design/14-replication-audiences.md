# Campfire — Replication audiences

Who receives each part of the state, and how a mode sets it. Design 04 gives each capability's network data a default audience: all who see a unit, its team, or its owner ([Combat](04-capabilities/combat.md#network), [Progression](04-capabilities/progression.md#network), [Production](04-capabilities/production.md#network), [Actions](04-capabilities/actions.md#network)); design 03 gives each script state field one, its `sync`, and a spectator everything, after the host's delay ([Script state](03-game-scripting.md#script-state)). The code has neither as a rule: it sends each replicated unit to the rooms of the teams that see it, so every component goes to every client that sees the unit, sends six components to the owner alone through `OwnedBy`, whose list `net` keeps by hand, has no path for state outside an entity, and has no spectator ([Foundational issues](../../.notes/FOUNDATIONS.md)). A mode can set none of it: whether an enemy sees a hero's mana, its cooldowns or its items is a rule of the game, as League of Legends shows a hero's items to all and Zero Hour a player's money to no other player.

What goes wrong today, by design 04:

| Data | Design 04 | Code |
| --- | --- | --- |
| Pools other than life | the owner, or the team as the mode sets | all who see the unit: one `Pools` holds life and the rest |
| Cooldowns and charges | the owner, or the team as the mode sets | all who see the unit, in `ActionSlots` |
| Points, experience | the owner | points to all who see the unit; experience to no client |
| Script state fields | as each field's `sync` says | `UnitState` and `ModeState` to no client |
| A player's resources, supply, queues, modifiers | the player, or the team as the mode sets | no client: `PlayerResources` and `PlayerModifiers` are world resources |
| An aura's source stats | the client holds each aura's modifier on its own units | missing: the client derives `UnitStats` only for the units it predicts |

The design makes each mistake of this kind impossible to write or loud at once: a kind the compiler requires on each state type, an audience for each kind that the mode sets and the load checks, one list of state types that every consumer reads, and tests that check what each client holds against what the mode lets it hold.

## Decisions

### Recipients and kinds

- **D1. Four recipients.** For an entity and a client: the **owner**, the client of the slot that owns it; an **ally**, another client of its team; an **enemy**, a client of another team; a **spectator**, a client with no slot, which joins to watch. An audience is a set of them. The owner is in every audience but the server's: no mode hides a unit's own data from its own player, so the owner's prediction always holds what it reads (D12). A unit reaches an enemy only while the enemy's team sees it, as vision decides; it reaches a spectator as the mode's `[visibility] spectators` says, `all`, every unit through any fog, by default, as design 03 has it, or `none`; the player, team and match entities (D7) reach every client, a spectator of `none` too, as they stand outside the vision rooms (D10).
- **D2. Each state type names its kind.** A capability's state type implements `Kinded`, a trait of `capabilities` with one item, `const KIND: DataKind`, and no default; the list of D9 takes only types that implement it, so a type of the list with no kind does not compile. `sim` keeps no kind, as it holds no genre: `capabilities` gives the kind of `sim`'s own `Position`, as its trait may. A kind is a group of data the rules treat as one: `unit`, a unit's public state, as its type, team, owner, place, body, level, death, destination and the action under way; `life`; `pools`, the mode's other pools; `modifiers`; `cooldowns`, each slot's ready tick and charges; `progression`, experience and points; `inventory`; `prediction`, what only the owner's prediction reads, as a unit's route, progress, modifiers' clocks and respawn; `resources`, `supply`, `queues` and `player_modifiers`, a player's; `match`, the match's public state, the teams' relations and its end, and `mode_state`, the mode's fields; `unit_state`, by each field's `sync`; and `server`, which never leaves the server. The list is closed: a new kind is a line of the engine, at most 32 of them, as replicon's filters number (D10).
- **D3. One kind for each type.** A component whose parts are of different kinds is split along them, as a filter hides whole components: `Pools` into `Life` and `Pools`; `ActionSlots` into `ActionSlots`, of `unit`, and `SlotTimes`, of `cooldowns`; `UnitState` into one component for each `sync` class of its fields, the book naming each field's component and place.

### What the mode sets

- **D4. Each kind has a default audience, and a mode may set it.** The engine's defaults are design 04's and design 03's:

  | Kind | Default audience | The mode may set |
  | --- | --- | --- |
  | `unit`, `match` | owner, allies, enemies, spectators | nothing: vision and `spectators` decide who holds a unit |
  | `life`, `modifiers`, `player_modifiers`, `mode_state` | owner, allies, enemies, spectators | enemies, spectators |
  | `pools`, `cooldowns`, `progression`, `inventory`, `resources`, `supply`, `queues` | owner | allies, enemies, spectators |
  | `unit_state` | each field's `sync`: `owner`; `team`, the owner and allies; `all`, everyone, spectators too | spectators, for each `sync` class |
  | `prediction` | owner | nothing: the prediction's own data |
  | `server` | none | nothing |

  The mode's `data/mode.toml` sets them in `[visibility]`: each kind to the whole list of recipients its audience holds beside the owner, so a setting both adds and removes, and the units the spectators see.

  ```toml
  [visibility]
  inventory = ["allies", "enemies", "spectators"]   # every hero's items to all, as in League of Legends
  resources = ["spectators"]                        # a player's money to no other player
  spectators = "all"                                # every unit, through any fog
  ```

  The load reads each name into its kind and each recipient into its enum, and refuses an unknown kind, a recipient the table does not let the mode set, and a kind named twice; a kind the mode does not name keeps its default. Design 04's "the owner, or the team as the mode sets" is the mode's choice of that kind's allies.
- **D5. A mode's hidden data is not predicted.** A client derives what it holds: where a mode hides a unit's modifiers from enemies, an enemy's client cannot derive that unit's stats, and an aura of that unit on its own units comes from the server, corrected as a misprediction. Design 04's defaults hide nothing that a prediction reads.

### Where state lives

- **D6. A resource never replicates.** Lightyear replicates the components of entities, so `SimResource` has no kind and a client receives no resource: state a client receives lives on an entity.
- **D7. State of a player, a team or the match lives on an entity.** The server spawns an entity for each slot, which holds the slot's `Owner` and `Team`, one for each team, and one for the match, as Unreal keeps a `PlayerState` for each player and a `GameState`. The player's row of resources, its supply, its queues and its modifiers become components of its entity, each of its kind; the mode's fields of `sync = "all"` a component of the match entity, as the match has no owner or team for the other classes; the teams' relations a component of the match entity.
- **D8. State reaches a client only by replication.** No state type goes by a message, so no code must remember to send one after each change: the `Relations` and `MatchEnd` messages go, as both are state the match entity holds, and design 13's D7, the row of resources sent after each tick that changes it, is D7's player entity. Messages carry the protocol alone: the offer, the join, the match start, the acknowledgments and receipts.

### One list

- **D9. Each capability names its state types once.** A capability's `state_types(types: &mut impl StateTypes)` lists every type it adds, and every consumer reads that list: the `StateRegistry`, for the hash, the snapshot and the copy, and `NetProtocol`, which registers each type of a kind other than `server` for replication, with its prediction, and in its kind's filter. `NetProtocol`'s list by capability and `OwnedBy`'s scope go; a capability's `install` calls its list, not a call for each type, and a test checks that a match's registry holds exactly what the lists name, so no `install` registers a type outside its list. Replication's other settings default to the safe choice: a type is predicted only when it says so, and sent on every change unless it says it never changes.
- **D10. One replicon filter for each kind.** Units keep their vision rooms, which decide which units an enemy holds, and a spectator joins the rooms the mode's `spectators` says; the player, team and match entities stand outside the rooms and replicate to every client. On each entity, the filter of each kind its components hold carries the entity's owner and team and its kind's audience in this match, and hides the kind's components from each client outside it. The list registers, for each type of a kind that has a filter, an observer that inserts the kind's filter as the type is added, and inserts it again as the entity's `Owner` or `Team` changes, as a filter is immutable: no system adds a filter, so none can forget one. Replicon's scope is a static type, at most 32, and a mask from a list made at run time is private to the crate, so each kind's scope is a tuple of its types; a test builds each kind's types from the capabilities' lists and checks the tuple holds exactly them, naming each type missing or extra, so a new type a tuple lacks fails the suite the day it is added.

### What a client predicts

- **D11. A client derives each derived component for every unit whose inputs it holds.** `UnitStats` derives from a unit's type, level and modifiers and its player's modifiers, which reach every client that holds the unit by the defaults, so the client derives it for each such unit, not only those it predicts, and an aura's numbers resolve from its source's stats as the server's do; D5 holds for a mode that hides them.
- **D12. Prediction reads only what its client holds.** A test walks the client's prediction schedule, and checks for each component a system reads that it is derived on the client, or of a kind whose default audience holds every client that holds the entity (`unit`, `match`, `life`, `modifiers`, `player_modifiers`), or that the system's query reads it on the client's predicted units alone, with `Without<Unpredicted>`, whose owner holds every kind but `server` (D1). A predicted system that reads data its client never receives fails it, by the system's and the type's names; a mode that narrows a default is D5's.

### The checks

- **D13. A leak test over every type and two settings.** A scripted 3v3 match in process, two teams of three and a spectator, checks after each tick that each client holds each component on exactly the entities its kind's audience admits for that client: once with the defaults, and once with a `[visibility]` that sets every kind the mode may to its other extreme. It is generic over the capabilities' lists, so a wrong filter, a tuple a type lacks, a setting the load drops, and a new type, are each a failure that names the type, the entity and the client. Its other checks are D10's tuple test and D12's prediction test.

## What waits

- A unit's destination reaches all who see it, as a client's steering reads whether each unit in its walker's way walks (`Walking::steer`); a flag of whether it walks, in its place, would keep where it walks from enemies.
- The spectator's delay, which design 03 gives the host: a spectator's stream held back by it.
- A modifier type that only its owner's client sees, and equipped items that show to all who see a unit: no mode needs either yet.
- Sending a kind only to the clients that predict, or interpolate, a unit, as Unity's `SendTypeOptimization` does: bandwidth, measured first.

