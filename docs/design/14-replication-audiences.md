# Campfire — Replication audiences

Who receives each part of the state, and how a mode and its host set it. Design 04 gives each capability's network data a default audience: all who see a unit, its team, or its owner; design 03 gives each script state field its `sync`. A mode sets the rules between players, as League of Legends shows a hero's items to all and Zero Hour a player's money to no other player; the host sets what spectators and dead players see, as Counter-Strike's server overrides its game mode's `mp_forcecamera` and a Dota 2 lobby's host admits spectators ([Spectating](../research/spectating.md)).

What the code still lacks of design 04:

| Data | Design 04 | Code |
| --- | --- | --- |
| What a predicting client reads | only what it holds (D12) | 39 reads of a type its client does not hold, the server's or another player's, which `release::tests::prediction_reads` lists in `AWAITING` |

Each mistake of this kind is impossible to write or fails at once: the compiler requires a kind on each state type, the load checks the mode's audiences, the server checks the host's settings as it starts, every consumer reads one list of state types, and tests compare what each client holds with what it may hold.

## Decisions

### Recipients and kinds

- **D1. Four recipients.** For an entity and a client: the **owner**, the client of the slot that owns it; an **ally**, another client of its team; an **enemy**, a client of another team; a **spectator**, a client with no slot. An audience is a set of them. The owner is in every audience but the server's, so the owner's prediction always holds what it reads (D12). A unit reaches an enemy only while the enemy's team sees it, and a spectator as `[visibility] spectators` says (D15); the player, team and match entities (D7) reach every client, as they stand outside the vision rooms (D10). A player out of play is still its entities' owner and its team's ally, and may see the other teams as a spectator does (D16).
- **D2. Each state type names its kind.** A capability's state type implements `Replication`, with two items and no default: `const KIND: DataKind`, and `type Sending` (D9); a type with no kind does not compile. `capabilities` gives the kind of `sim`'s `Position`, as `sim` holds no genre. The kinds, a closed list of at most 32, as replicon's filters number (D10):

  | Kind | Holds |
  | --- | --- |
  | `unit` | A unit's public state: type, team, owner, place, body, level, death, destination, the action under way |
  | `life` | The life pool |
  | `pools` | The other pools |
  | `modifiers` | A unit's modifiers |
  | `cooldowns` | Each slot's ready tick, charges, and the tick a toggle that is on pays next |
  | `progression` | Experience and points |
  | `inventory` | A unit's inventory |
  | `prediction` | What only the owner's prediction reads: route, progress, modifiers' clocks, spawn point, respawn |
  | `resources`, `supply`, `queues`, `player_modifiers` | A player's |
  | `match` | The match's public state: the teams' relations, its end, who is out of play |
  | `mode_state` | The mode's script state fields |
  | `unit_state_owner`, `unit_state_team`, `unit_state_all` | A unit's script state fields of each `sync` but `none`, as a kind has one audience |
  | `server` | What never leaves the server, a unit's fields of `sync = "none"` among it |

- **D3. One kind for each type.** A filter hides whole components, so a component of two kinds is split along them: `Pools` into `Life`, the life pool's meter, and `Pools`, the rest by pool id with the life pool's place empty; `ActionSlots` into `ActionSlots`, of `unit`, and `SlotTimes`, of `cooldowns`, one for each slot in order; `UnitState` into `UnitState<S>` for each `sync` class, the book naming each field's class and its place in it. A rule that names a pool by its id, as a cost, a regen or a restore does, reads `Life` and `Pools` as one, the life pool's id routing to `Life`.

### What the mode sets

- **D4. Each kind has a default audience, and a mode may set it.**

  | Kind | Default audience | The mode may set |
  | --- | --- | --- |
  | `unit`, `match` | owner, allies, enemies, spectators | nothing: vision and `spectators` decide who holds a unit |
  | `life`, `modifiers`, `player_modifiers`, `mode_state` | owner, allies, enemies, spectators | enemies, spectators |
  | `pools`, `cooldowns`, `progression`, `inventory`, `resources`, `supply`, `queues` | owner | allies, enemies, spectators |
  | `unit_state_owner`, `unit_state_team`, `unit_state_all` | by `sync`: `owner`; `team`, the owner and allies; `all`, everyone | spectators |
  | `prediction` | owner | nothing |
  | `server` | none | nothing |

  The mode's `data/mode.toml` sets them in `[visibility]`: each kind to the whole list of the recipients the table lets the mode set for it, so a setting both adds and removes them, and the others keep their place in the default; `spectators` (D15); `out` (D16); and `host`, the settings it leaves to the host (D14).

  ```toml
  [visibility]
  inventory = ["allies", "enemies", "spectators"]   # every hero's items to all
  resources = ["spectators"]                        # a player's money to no other player
  spectators = "all"                                # every unit, through any fog
  out = "team"                                      # a dead player sees what its team sees
  host = ["spectators", "out"]                      # what the host may set again
  ```

  The load refuses an unknown name, a recipient the table does not let the mode set, a value a setting does not take, a setting named twice, and a name in `host` that is no setting the mode may set. A setting the mode does not name keeps its default. Design 04's "the owner, or the team as the mode sets" is the mode's choice of that kind's allies.
- **D5. A mode's hidden data is not predicted.** Where a mode hides a unit's modifiers from enemies, an enemy's client cannot derive that unit's stats, so an aura of it on its own units comes from the server as a correction. Design 04's defaults hide nothing a prediction reads. A client keeps a held modifier from a source whose modifiers it does not hold, or that it does not hold at all, as the server sent it, as it cannot tell whether that source's aura still reaches: the server's word corrects it once as it comes, and once as it goes.

### What the host sets

- **D14. Three layers: the engine's defaults, the mode's `[visibility]`, and the host's settings for what the mode leaves to it.** The mode's `host` list names the settings the host may set again, as Unreal marks the game mode properties a server's ini may set; a mode with no list leaves `spectators` and `out`, and an empty list leaves nothing. A kind in the list goes to the host within the recipients D4 lets a mode set. The host gives them as server flags, `--visibility <setting>=<value>`, read into the same enums. The server refuses to start on a setting the list lacks, an unknown name or value, a recipient D4 forbids, or a setting named twice, naming each. The settings that result hold for the whole session, and its terms carry them ([Session id](05-protocol-spec.md#keys)): every player signs them, a client knows what it will hold before it plays, and the log proves what each client could see.
- **D15. The host admits spectators; the spectators' fog is a setting.** `--spectators <count>`, the most links with no slot, 0 unless the host says, which the terms carry with D14's settings. A live spectator can tell a player what it sees, which League of Legends' delay of 180 s and Dota 2's of 120 s exist to stop; with no delay yet (What waits), the default admits none. `spectators` takes `all`, every unit through any fog, the default, or `teams`: the spectator sees one team's view or all teams' together, and changes which by a message, as a Dota 2 spectator chooses its fog. A spectator joins by a signed answer, as a player does, asking to watch: the server checks it as a player's, admits it while fewer than the terms' count watch, seats no slot, and the log holds nothing of it ([Protocol](05-protocol-spec.md#connection), step 3). With `all`, every entity the server replicates stands in one more room, the spectators', and a spectator's link stands in it, so it holds even a unit no team sees; with `teams`, its link stands in the rooms of the teams it chose, every team's until its first choice, `WatchTeams`, which the server takes only in such a session. The kinds reach it as their audiences say, and it predicts nothing.

### Players out of play

- **D16. A player out of play sees what its team sees, or what a spectator sees.** A player is out of play while it holds avatars and every one is dead, or while the mode's script holds it out, `ctx.set_out(player, bool)`, as an eliminated or a defeated player is: the script's hold is `HeldOut` on its player entity (D7), the server's alone, and the server derives `OutOfPlay` there as each tick ends, of the kind `match`, which every client receives as it changes, as supply is (D9). A player of no avatar, as in Zero Hour, is out of play only as its mode's script holds it out. `out = "team"`, the default, changes nothing, as `mp_forcecamera 1`; `out = "spectators"`, as `mp_forcecamera 0`, makes it a spectator to each entity of another team, with the spectators' fog and audiences. As it plays again, the server removes what it held of the other teams' entities, as a unit leaves vision. Its link's `Viewer` carries whether it is out, inserted again as `OutOfPlay` comes and goes, so each kind's filter and its rooms follow it (D10): out, it stands in its team's room and the spectators', every room with `all`, every team's with `teams`, as `all` holds every entity in the spectators' room once a spectator or a player out of play may see so. What it sees is live, so it can tell its allies who still play: the default is `team`. A black screen until the round ends, `mp_forcecamera 2`, is a picture the client draws.

### Where state lives

- **D6. A resource never replicates.** Lightyear replicates entities' components, so `SimResource` has no kind: state a client receives lives on an entity.
- **D7. A player's and the match's state live on entities.** The match's start spawns one entity for the match and one for each slot, as Unreal keeps a `GameState` and a `PlayerState` for each player; a team's entity comes with the first state a team holds. Each takes a named stable id past every id the allocator hands out, `StableId::MATCH` and `StableId::of_player(slot)`, so their spawn moves no unit's id, nor a draw or a think keyed by it, and a client finds them with no lookup. A player's entity holds `Player`, its slot, and `Team`; its resources, supply, modifiers and whether it is out of play (D16) are components of it, each of its kind, which each installed capability puts on it as it spawns, supply as the server first derives it. A train queue stays a component of its producer, of the kind `queues`, as StarCraft II keeps a queue on its building: the producer replicates as a unit, and the kind's filter gives its queue to the audience the mode sets. The match entity holds the teams' relations, the match's end once it comes, and the mode's script state fields, a component for each class, `none` and `all`. A mode field's `sync` is `none` or `all`, as the match has no owner or team: the load refuses `owner` and `team` on a mode field.
- **D8. State reaches a client only by replication.** No state type goes by a message, so no code must remember to send one: the `Relations` and `MatchEnd` messages go, as the match entity holds both, and design 13's row of resources is D7's player entity. Messages carry the protocol alone: the offer, the join, the match start, acknowledgments and receipts.

### One list

- **D9. Each capability names its state types once.** A module's declarations (`Declares::declare`) list every type it adds, and every consumer reads them: the install, which fills the `StateRegistry`, for the hash, the snapshot and the copy; `NetProtocol`, which registers each sent type for replication, with its prediction; and the kind filters (D10). The declaration audit checks that each type is declared once and sits on a declared kind. Each type declares how it is sent: `SentOnce`, as its entity spawns, for an immutable type, so the compiler refuses a write after; `SentOnInsert`, as its entity spawns and on each insert after, for an immutable type whose one change is a new insert, as `Owner` and `Team`; `SentOnChange`; `SentPredicted`, which its owner's client predicts, for a type that compares and clones; and `NotSent`, the one way of `server` and of no other kind, which a build checks. A part of a type that changes after spawn is a type of its own, as `Area`'s trigger tick is `AreaTrigger`. No type requires a type of another kind, as `Modifiers` once required `ModifierClocks`: a client that receives the first would hold the second, at its default, where its kind's audience holds it not. A list also declares the components a match derives again, as `UnitStats`, which no hash, snapshot or client holds, and those the server derives and sends, as a player's supply, which its kind's audience receives as it changes and no hash or snapshot holds: a client holds only the units it sees, so it cannot derive them; the proving match checks after each tick that every component a unit holds is a declared type.
- **D10. One replicon filter for each kind.** Units keep their vision rooms; a spectator joins the rooms D15 gives; the player and match entities stand in every team's room, and the spectators' with `all`, so every seated link and every spectator receives them, and no link with neither. A link's `Viewer`, a seated player by its slot and team or a spectator, sets its rooms and is what each filter reads: the owner, an ally of the entity's team or of an entity of no team, as the match's, an enemy of another team, or a spectator. Each kind a mode may set, and the owner's prediction, has a filter; `unit` and `match` need none, as vision decides who holds them. Each kind of entity declares these on its marker (`EntityKind`): its rooms, by vision or every team's, and whether its owner's client predicts it, a unit but no delivery; net shows each kind the core declares by them, and sets the prediction again as an entity's owner changes, or as its owner's player takes a seat. On each entity, the filter of each kind its components hold carries the entity's owner, a player's entity its own slot, and its team and the kind's audience in this match, and hides the kind's components from each client outside it. The list registers, for each type of a filtered kind, an observer that inserts the filter as the type is added, and again as `Owner` or `Team` changes, as a filter is immutable: no system adds one, so none can forget one. A client holds these entities unpredicted, as the server sends them, and predicts nothing until it holds the match entity, which every tick reads. Replicon's scope is a static type, and its masks from run-time lists are private, so each kind's scope is a tuple of its types; a test builds each kind's types from the lists and names each type a tuple lacks or holds wrongly.
- **D17. An owner changes by an insert alone.** `Owner` is immutable, as `Team` is, so a change of owner is a new `Owner` inserted, which each kind's filter follows by its observers; no code writes an owner in place, where a filter would keep the old one. It is sent on each insert, `SentOnInsert`, a sending kind of an immutable type that `SentOnce`, sent at the spawn alone, is not; so is `Team`, which a change of owner moves (D18). A client that the new owner's filters hide kinds from receives the new owner in the same update as the removals: replicon 0.44.2 dropped a mutation from the tick a filter hid a component from its client, and Campfire's fork of it fixes that, until upstream does.
- **D18. A unit's owner is a player of its team.** The owner's client then holds its unit wherever its team sees it, with no path around the team's vision. A change of owner to a player of another team moves the unit to that team with it, as a capture does in Zero Hour and Dominate in Dota, and `Owner`'s restore check refuses a unit whose owner is of another team.

### What a client predicts

- **D11. A client derives each derived component for every unit whose inputs it holds.** `UnitStats` derives from a unit's type, level and modifiers and its player's modifiers, which reach every client that holds the unit by default, so the client derives it for each such unit, and an aura's numbers resolve as the server's do; D5 holds for a mode that hides them. It derives a unit's stats and tags, and writes nothing else of a unit it holds as the server sent it; and a unit spawns with its type's stats at level 1, as its first refresh derives them, so a reader in the stage it spawns in finds them.
- **D12. Prediction reads only what its client holds.** A test walks the client's prediction schedule and checks that each component a system reads is derived on the client, or of a kind whose default audience holds every client that holds the entity (`unit`, `match`, `life`, `modifiers`, `player_modifiers`, `mode_state`, `unit_state_all`), or read on the client's predicted units alone, `Without<Unpredicted>`, whose owner holds every kind but `server`. It names the system and the type of each failure. Its world is built as a client's, by the match spec's `predicting`, which puts `Unpredicted`'s filter in place before any system's query is made: a query takes the default filters as it is made, so one made before would read every unit a client holds.

### The checks

- **D13. A leak test over every type and two settings.** A scripted 3v3 match in process, with a spectator of each fog and a dead hero's player among its clients, checks after each tick that each client holds each component on exactly the entities its kind's audience admits: once with the defaults, and once with every setting at its other extreme, through the host's flags for those the mode leaves to it and the mode's `[visibility]` for the rest. A client is checked after its frame and before the server's, as it then holds what the server's last frame sent; a unit its own client predicts is checked as an entity alone, as its prediction runs ahead of the server. It is generic over the lists, so a wrong filter, a tuple that lacks a type, a dropped setting and a new type each fail by the type, the entity and the client.

## What waits

- A unit's destination reaches all who see it, as a client's steering reads whether each unit in its way walks; a flag of whether it walks would hide where it walks.
- The spectator delay, which designs 01 and 03 give the host, and spectator bets need: a live link's rooms cannot hold a stream back, but a spectator that plays the session's log behind the delay, as Valve's GOTV relays a match after `tv_delay`, needs no rooms. Until then a spectator is live, and the host admits none unless it says (D15).
- A modifier only its owner's client sees, and equipped items that show to all who see a unit: no mode needs either yet.
- Sending a kind only to the clients that predict or interpolate a unit, as Unity's `SendTypeOptimization` does: bandwidth, measured first.
