# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Effects

The slice of [Effects](docs/design/effects.md): a player of a Zero Hour skirmish sees each shot's fire effect at its barrel, each projectile's exhaust, each hit's burst and each death's explosion, as the game's FX lists and particle systems draw them. Its facts are [Zero Hour's effects](docs/research/zero-hour-effects.md); its checks are that document's.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them; then the slice's decisions fold into the documents of their systems, `02-engine-core.md` for the format and the client's particles and `09-zero-hour.md` for what the import converts and loses, the code's citations of `effects.md` with them, and `effects.md` goes.

## Console

The slice of [Console](docs/design/console.md): an agent in Claude Code starts a local match of the MOBA test mode in the client, runs it paused to a tick, tables its units and shows a screenshot of one of them, while the user watches the window. Its facts are [The console](docs/research/console.md); its checks are that document's. It stands after Effects' steps, and waits for no player's check of a slice before it. Its new dependencies, `schemars`, `strsim`, `rmcp`, `tokio` and `base64`, were approved on 2026-10-27, each added in the batch that names it.

### End of the slice

It stands last, as the user's check is of the whole slice.

- [ ] Claude Code, after `cargo drive run mcp` and the approval of the project's `.mcp.json`, connects and lists the tools; the user watches the agent start a local match of the MOBA test mode, run it to a tick, table its units and show a screenshot of one; in a local Zero Hour skirmish of bots alone, the agent finds the walkers stuck at a tick with the tools alone; then the slice's decisions fold into the documents of their systems, `02-engine-core.md` for the console, the MCP server and the view, and `08-sessions.md` for the run, the planned ones into `03-game-scripting.md`, `05-protocol-spec.md` and `06-script-api.md`, and `console.md` goes.

## Wrecks

The slice of [Wrecks](docs/design/wrecks.md): a player of a Zero Hour skirmish sees a destroyed tank's hulk and debris, a destroyed aircraft's fall, and a tank's tread dust and track marks. Its facts are [Zero Hour's deaths](docs/research/zero-hour-deaths.md); its checks are that document's. It stands after Console, whose screenshots let an agent see its wrecks before the player's check, and extends Effects' deaths; it waits for no player's check of a slice before it.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] An agent runs a Tournament Desert skirmish through the console and screenshots a destroyed Crusader, a destroyed Comanche and a driving tank, each fault it sees going to a step before the player's check; then a player sees the hulks, debris, dust and track marks, and an aircraft's fall, in the client as the game draws them; then the slice's decisions fold into `02-engine-core.md` and `09-zero-hour.md`, and `wrecks.md` goes.

## Helicopters

The slice of [Helicopters](docs/design/helicopters.md): a USA player of a Zero Hour skirmish trains a Chinook that gathers supplies by air and a Comanche that flies over cliffs and fights, which only weapons that hit the air reach in flight, and which falls when shot down. Its facts are [Zero Hour's helicopters](docs/research/zero-hour-helicopters.md); its checks are that document's. It stands after Wrecks, whose aircraft falls it shows, and waits for no player's check of a slice before it.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] An agent runs a Tournament Desert skirmish through the console and screenshots a Chinook over a supply dock and a Comanche over a cliff, each fault it sees going to a step before the player's check; then a player sees a Chinook gather, a Comanche fly and fight, ground weapons ignore it in flight, and a Comanche shot down fall, in the client; then the slice's decisions fold into `04-capabilities/navigation.md`, `04-capabilities/stats.md`, `04-capabilities/combat.md`, `04-capabilities/production.md` and `09-zero-hour.md`, and `helicopters.md` goes.

## Jets

The slice of [Jets](docs/design/jets.md): a player of a Zero Hour skirmish trains a Raptor or a MiG at an airfield, which waits at its parking place, taxis to its runway, takes off down it, strikes a target in passes, comes back to land and reload, and falls when shot down. Its facts are [Zero Hour's jets](docs/research/zero-hour-jets.md); its checks are that document's. It stands after Helicopters, whose fliers it extends, and waits for no player's check of a slice before it.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] An agent runs a Tournament Desert skirmish through the console and screenshots a jet on its runway, in a pass over its target and parked again, each fault it sees going to a step before the player's check (a question: [PLAN_QUESTIONS.md](PLAN_QUESTIONS.md#jets-end-of-the-slice)); then a player sees a jet taxi, take off down its runway, strike in passes, land and reload, and fall when shot down, in the client; then the slice's decisions fold into `04-capabilities/navigation.md`, `04-capabilities/combat.md`, a new `04-capabilities/airfields.md` and `09-zero-hour.md`, and `jets.md` goes.

## Changed

- J6's Raptor flies over GLA's command center before its attack order, as an attack aims only at what its team sees; the play pins its train and that flight too. A missile of an `IgnitionDelay` now converts, igniting at once, as `09-zero-hour.md` decides, since the drop left the Raptor and the Stealth Fighter with no weapon.

- J5 reads an airfield's places and runways from its model's bones as the import reads a helipad, a train that makes a jet makes it at a place, `at_place`, so a helicopter of the same airfield leaves by its helipad; a base start and a bot's army leave the jets out, as a base start's units stand at their rally and not at an airfield's place, and a bot does not fly sorties; the airfield's `ApproachHeight` and `HealAmountPerSecond` are not converted, as J4 lands by its own sink and heals nothing. The losses go to `09-zero-hour.md` as the slice folds.

- J4 lands a jet from an approach past its runway's end as far as it flies while it sinks from its height at its climb, not from the airfield's `ApproachHeight`, as the flight holds no height between; a jet drops its orders as it goes home and takes none up after its reload, where Zero Hour resumes its last; a jet's heal at its place and the life a jet with no airfield loses are left out, as no contract of combat lets a capability above it deal a heal or a damage, and a jet on the ground of a gone airfield stays, where Zero Hour kills it: each goes to the issue log. A jet with no airfield flies on as it is.

- J3's places are production's, a producer's `places` that its `HeldPlaces` hold for the units each made there by `MadeAt`, and the airfield's own section gives each place's prep point and runway: production makes the unit and lands nothing, so the place is its exit; a train waits at its end, looked at each tick, while each place holds a unit, where Zero Hour holds a place as the train is queued, as the queue's due time reads no book. A jet taxis and rolls by forced dashes along the airfield's points, facing each leg, with the units core's `FlightGoal` holding it on the ground, which navigation leaves to its forced moves, and the engine tag `grounded` blocking its walk and its attacks; the sorties go after production's starts in Act.

- J2's winged aim reads a status tag of the units core, `on_wings`, which navigation turns with `air_target`, as combat and orders depend on no navigation of their own and a contract below them holds what both read; it shifts the engine tags after `air_target` by one. A clip's home reload is its `refill = "home"`, beside its `reload_ms`, which stays its time; an empty clip of home starts nothing, so its weapon gives way to no other. The orders' chase keeps a unit on wings flying at its target in reach.

- J1's winged flier passes a waypoint, and arrives, once it lies within its circling radius, as a wing cannot meet a point it turns round; its test pins the walk on the ground, the arrival, the first turns of the circle and a tick across it, and bounds each other tick by its step's rounding, as each step's sine and cosine round apart.

- H3's play starts America and GLA from base starts, not a builder and an attacker: America's base start holds its Chinooks and Comanches, and GLA's every building its workers build, so the play orders only the gather, a stop of the other Chinooks, which their supply center sends to gather, and the flight, and the Comanche falls to every weapon of GLA's base that hits the air, not to a Stinger site alone. The Comanche's climb is pinned too, to the height its flight data gives.

- H2 counts the jets as before, as trains of a unit that does not walk, and records the transports, the drones and a hover's tilt as losses of `09-zero-hour.md` as the slice folds, not as counts of their own: no stage reads a `TransportContain` or a draw's tilt, so a count would name what no stage converts. The skirmish play stands the Chinook each building sends to gather, so its money counts what it spends; the helicopters' play pins a gather. The helipad's bone is read from the airfield's model, which no fixture model holds, so only the production test of its exit and the Zero Hour check's golden hold it.

- H1 gives every unit its status tags from the core, not each capability that turns one: a host declares a part once, and a flying gatherer holds both production's and navigation's tags; every unit's join carries their 5 bytes. The engine tag is `air_target`, not `airborne`, a game's own word, which the test MOBA's knock-up grants. Two fliers parting and one passing over a ground unit and a wall are the air layer's own tests, which flight leaves as they were, so no flight test repeats them.

- W4's dust scales and track numbers are the effects file's `treads` and `tracks`, which the import writes of `W3DTankDraw`'s and `GlobalData`'s, not the client's constants, and a look's track has its width, its model's tread bones' span, besides its texture: a capability names no constant of one game, and the game's track is as wide as the tank's treads.

- W3's fall plays a start of its own, the jet's initial death taken out of the death's start, and a jet keeps its drawing's velocity as it falls, its ground death written by every jet with its height, 9 frames squared of the game's gravity: in the game a jet on the ground plays no initial death and dies at once whatever its ground death names, and its locomotor keeps its speed as it falls (`JetSlowDeathBehavior::onDie`, `beginSlowDeath`); a torn drawing's gauges go with the HUD.

- W2's wrecks meet the ground by the effects file's `wrecks`, its gravity with the game's ground stiffness, friction and rest, which the import writes, not by its gravity alone: the game's landing takes its ground's stiffness, and its friction and rest are the physics' own numbers, data a game gives.
- C3's `sim_run` takes 110 s of wall time unless named, not 120 s: two minutes is Claude Code's wait before it sends a call to the background, which the run stays under.
- C2's client answers a run once it received the state of the tick the run paused at and drew a frame after it, not once its view draws that tick: the local server's pause stops the client's clock too, so its interpolation never reaches the tick (console.md, A run answers once its client received its tick).
- C2's client of a remote server refuses every request of the match, not the run alone, as it holds no session's phase and only its team's view (console.md, A remote server's client answers no request of the match).

## Foundations
