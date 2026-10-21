# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Base defenses

A player builds a Patriot battery, a tunnel network and a Stinger site and sees each fire at an enemy that comes near, the site's three Stinger soldiers with missiles, each soldier back 30 s after it dies, and the tunnel's two defenders once; and trains Technicals, each of one of three chassis, and Inferno cannons, and sees them fire, and Tomahawks after a Strategy Center. The Scud launcher, which needs a science, the demo trap, which has no body, the weapons of `NOT_AIRBORNE`, and the Tomahawk's weapon, which waits a `PreAttackDelay`, stay dropped. Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The play (S)

The user. Last: it ends the slice. Its decisions are in [Production](docs/design/04-capabilities/production.md) and [Zero Hour](docs/design/09-zero-hour.md).

- **Check:** the user plays Tournament Desert in the client and sees a Patriot battery, a tunnel network and a Stinger site fire at the bot's units, a killed Stinger soldier come back, Technicals of each chassis and an Inferno cannon fire, and a Tomahawk train after a Strategy Center.

## Slice: Test matches

The user starts the client as a spectator of Tournament Desert, where two bots, China and America, each start with a base of every building its faction builds and two of each unit it trains, and send their armies at each other; and the user starts a match of their own on such a base ([proposal](docs/design/12-test-matches.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. A session of bots alone (M)

`net`, `client`, 08-sessions.md. First: the watch needs it, with the plain start of today.

- **The session.** `LobbyOpening` names whom the lobby waits for: its players, or, of a local watch, its watcher; `Lobby::new` (`network/net/src/sim_server/lobby/mod.rs:159`) opens a local watch's plan of no player seat and keeps `LobbyError::NoPlayer` for every other, and `Lobby::start_when_full` (`lobby/mod.rs:281`) starts a local watch's match as its spectator joins; `exit_due` (`network/net/src/sim_server/sim_server_world/mod.rs:135`) ends a session of no player seat as its match ends or a stop asks it to, never because every player left. Builds: what can be forgotten is a type.
- **The local watch.** With `--spectate`, the client's arguments (`apps/client/src/command_line/mod.rs:121`, as `server_bot` at `:174` reads one bot alone) take a server bot in seat 0, and `LocalServer::start` (`network/net/src/local/local_server/mod.rs:140`) leaves seat 0 to it or open and admits one spectator of `SpectatorFog::All` (`HostVisibility`, `:150`).
- **Check:** tests of a local watch's lobby that starts as its watcher joins and not before, of a dedicated plan of no player refused, and of a session of no player that ends with its match and not before; the arguments' bot in seat 0 when it watches, refused when it plays; a net test of a local session of two bots and a watcher who receives both teams' units from tick 0.

### 2. Room near a point (M)

`mode`, `navigation`, 06-script-api-reference.md. After batch 1, independent of it.

- **Every body's room.** `NavigationColumn::room_for` (`capabilities/navigation/src/navigation_column.rs:68`) tests any body, a circle as a box, against the walls, the static bodies and every body the call spawns, and `spawn_unit` (`capabilities/mode/src/mode_api.rs:649`) keeps each spawn's body in the call's list (`ModeCall::boxes`, which becomes its bodies). Builds: one home for each operation.
- **The search.** The mode API's `room_near(type, position, gap, range)` walks the grid points of `gap` meters within `range` of `position`, by distance and then by angle, each compared in integers, to the first where a unit of `type` has room `gap` meters clear of each body, or none; its points are bounded by `range` over `gap`, which the call refuses past a bound.
- **Check:** tests of the search by hand-computed points: the position itself when clear, the first point past a wall, a body and a body the call spawns, the gap kept, the order of two points at one distance, and none past `range`; a circle refused by `spawn_unit` over a building, and two walkers of one call on two points.

### 3. A building that spawns whole (S)

`production`, 04-capabilities/production.md. After batch 2, independent of it.

- **The spawn.** A producer that spawns whole, not as a site, sends its free units (`Trains::free_units`) and its kept spawns (`KeptSpawns::send`) by production's spawn callback, which only the server's spawner runs, as a site does as it completes (`capabilities/production/src/construction/mod.rs:388`). Builds: a kind is a marker, and an entity spawns whole.
- **Check:** tests of a producer spawned whole that frees its units and keeps its spawns in its first tick, of a site that frees them only as it completes, and of a restore that frees nothing again.

### 4. The base start (M)

`import`, the Zero Hour rules package, the Zero Hour check, 09. After batches 2 and 3, whose search and spawns it uses.

- **The data.** The import writes each faction's `base_buildings_<faction>`, the buildings its start builder builds but its faction building, once each, and `base_units_<faction>`, the types those buildings and its faction building train, once each, in their command sets' order, beside its start params (`apps/import/src/zero_hour/production_import/mod.rs`); the rules data declares the choice `start` of `standard` and `base` and `base_count = 2`.
- **The script.** `on_match_start` (`packages/zero-hour/scripts/mode.rhai`) spawns a seat of `base` its faction building, each base building at `room_near` of its start with a gap of 10 within 400, and `base_count` of each base unit at `room_near` of its rally with a gap of 1, leaving out one with no room.
- **The bots.** `checks/zero-hour-check/bots/` gains a script for each of Tournament Desert's starts that trains its units, moves its army into the other start every 40 s, and orders it to attack the nearest enemy it sees 20 s after each move.
- **Check:** tests of the import's base lists from fixtures, the faction building and repeats left out; `cargo drive run zero-hour`, on request, pins a play of two base starts: each base building whole, `base_count` of each base unit, no body on another or on a wall, and a match of the two bots that ends with a result within its limit.

### 5. The fold and the play (S)

08-sessions.md, 04-capabilities/production.md, 06-script-api.md, 09. Last: it ends the slice.

- **The fold.** The decisions go to 08-sessions.md, whose "Singleplayer is a local server" gains the watcher, production.md, the mode's script API and 09, the facts stay in research/test-matches.md, and the proposal is deleted.
- **Check:** the user watches Tournament Desert's two bots fight from their bases in the client, and plays a match of their own on a base.

## Foundations
