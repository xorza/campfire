# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Effects

The slice of [Effects](docs/design/effects.md): a player of a Zero Hour skirmish sees each shot's fire effect at its barrel, each projectile's exhaust, each hit's burst and each death's explosion, as the game's FX lists and particle systems draw them. Its facts are [Zero Hour's effects](docs/research/zero-hour-effects.md); its checks are that document's.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them ([question](PLAN_QUESTIONS.md#effects-end-of-the-slice)); then the slice's decisions fold into the documents of their systems, `02-engine-core.md` for the format and the client's particles and `09-zero-hour.md` for what the import converts and loses, the code's citations of `effects.md` with them, and `effects.md` goes.

## Console

The slice of [Console](docs/design/console.md): an agent in Claude Code starts a local match of the MOBA test mode in the client, runs it paused to a tick, tables its units and shows a screenshot of one of them, while the user watches the window. Its facts are [The console](docs/research/console.md); its checks are that document's. It stands after Effects' steps, and waits for no player's check of a slice before it. Its new dependencies, `schemars`, `strsim`, `rmcp`, `tokio` and `base64`, were approved on 2026-10-27, each added in the batch that names it.

### End of the slice

It stands last, as the user's check is of the whole slice.

- [ ] Claude Code, after `cargo drive run mcp` and the approval of the project's `.mcp.json`, connects and lists the tools ([question](PLAN_QUESTIONS.md#console-end-of-the-slice)); the user watches the agent start a local match of the MOBA test mode, run it to a tick, table its units and show a screenshot of one; in a local Zero Hour skirmish of bots alone, the agent finds the walkers stuck at a tick with the tools alone; then the slice's decisions fold into the documents of their systems, `02-engine-core.md` for the console, the MCP server and the view, and `08-sessions.md` for the run, the planned ones into `03-game-scripting.md`, `05-protocol-spec.md` and `06-script-api.md`, and `console.md` goes.

## Wrecks

The slice of [Wrecks](docs/design/wrecks.md): a player of a Zero Hour skirmish sees a destroyed tank's hulk and debris, a destroyed aircraft's fall, and a tank's tread dust and track marks. Its facts are [Zero Hour's deaths](docs/research/zero-hour-deaths.md); its checks are that document's. It stands after Console, whose screenshots let an agent see its wrecks before the player's check, and extends Effects' deaths; it waits for no player's check of a slice before it.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] An agent runs a Tournament Desert skirmish through the console and screenshots a destroyed Crusader, a destroyed Comanche and a driving tank, each fault it sees going to a step before the player's check ([question](PLAN_QUESTIONS.md#wrecks-end-of-the-slice)); then a player sees the hulks, debris, dust and track marks, and an aircraft's fall, in the client as the game draws them; then the slice's decisions fold into `02-engine-core.md` and `09-zero-hour.md`, and `wrecks.md` goes.

## Helicopters

The slice of [Helicopters](docs/design/helicopters.md): a USA player of a Zero Hour skirmish trains a Chinook that gathers supplies by air and a Comanche that flies over cliffs and fights, which only weapons that hit the air reach in flight, and which falls when shot down. Its facts are [Zero Hour's helicopters](docs/research/zero-hour-helicopters.md); its checks are that document's. It stands after Wrecks, whose aircraft falls it shows, and waits for no player's check of a slice before it.

### H1. A flier holds a height and is airborne by it

Touches navigation, the units' status tags and combat's tests; it stands first, as the import's fliers need it.

- [ ] No unit holds a height (helicopters.md, A flier holds a height; helicopters.md, A flier walks in the plane): a walker steps toward its waypoints in 3D (`walking.rs:492`), so a waypoint on the ground pulls any height down. A unit type's `flight` section gives its height, climb and airborne height; a flier walks, routes and arrives in the plane on its layer, and after the walk its height in the world moves toward the sim's ground under it plus its height at its climb, never below the ground, on the server and in a predicting client alike. Builds: a kind's part declared once, and a pass that visits only fliers. Checks: navigation's tests climb a flier from the ground to its height at its rate, ease it over a step of the ground and never below it, walk it in the plane to a ground waypoint at its height, part two fliers on the air layer, and pass one over a ground unit and a wall of the ground layer; a production test gathers a full load with a flier from a node and brings it to a drop-off, as a gather reaches its dock by the planar metric whatever the height (helicopters.md, A flying gatherer docks over its dock).
- [ ] No tag tells a unit in the air from one on the ground (helicopters.md, Airborne is a tag of height): navigation declares the engine tag `airborne`, which a flier holds while its height above the ground passes its airborne height, turned as `gathering` is (`gather_loop/mod.rs:511`). Checks: a test gives and takes the tag as a flier crosses its airborne height on the way up and down; combat's test of weapons by tags shows a weapon of `enemies:!airborne` that selects a flier only on its pad and one of `enemies:airborne` only in flight.

### H2. The import flies Zero Hour's helicopters

Touches the import, the Zero Hour rules package and the golden; it stands after H1, whose parts it writes.

- [ ] The import flies nothing (helicopters.md, The import flies a hovering locomotor; helicopters.md, A weapon's anti flags are its targets; helicopters.md, A helipad is an exit): a type of no driving locomotor gets no speed (`imported_type/mod.rs:148`), so a train of it (`production_import/mod.rs:364`) and its gather (`supply_import.rs:149`) are dropped; a weapon that is not `AntiGround` is dropped (`combat_import/mod.rs:451`); and an airfield's `ParkingPlaceBehavior` gives no exit (`ini_build.rs:89`). The rules package declares the layers `ground` and `air`; an object whose normal locomotor hovers on `AIR` becomes a flier on `air` of its locomotor's speed, turn rate, `PreferredHeight` and `AirborneTargetingHeight`, its climb the mean speed of the original's climb to its height by its `Lift` and gravity; a weapon's `AntiGround` and `AntiAirborneVehicle` give its target filter; an airfield's helipad bone is the exit of its helicopters; the Chinook gathers; and jets, transports, drones and a hover's tilt are counted as losses. Checks: the import's tests convert a hovering locomotor and a jet's, the climb of the Chinook's lift worked by hand, each pair of anti flags, a helipad exit and a flying gatherer, each loss once; the Zero Hour check's golden moves only by the helicopters' types, trains, weapons and losses, each change read.

### H3. A skirmish of helicopters

Touches the Zero Hour check; it stands after H2, as it plays the import.

- [ ] No play flies (helicopters.md, Checks): the Zero Hour check plays no unit in the air. A play of the check gives a USA builder a Chinook that brings a full load to its supply center, and an attacker's Comanche that a Stinger site shoots down, their ticks and the money pinned in the golden. Checks: the play passes, and its record replays.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] An agent runs a Tournament Desert skirmish through the console and screenshots a Chinook over a supply dock and a Comanche over a cliff, each fault it sees going to a step before the player's check; then a player sees a Chinook gather, a Comanche fly and fight, ground weapons ignore it in flight, and a Comanche shot down fall, in the client; then the slice's decisions fold into `04-capabilities/navigation.md`, `04-capabilities/stats.md`, `04-capabilities/combat.md`, `04-capabilities/production.md` and `09-zero-hour.md`, and `helicopters.md` goes.

## Changed

- W4's dust scales and track numbers are the effects file's `treads` and `tracks`, which the import writes of `W3DTankDraw`'s and `GlobalData`'s, not the client's constants, and a look's track has its width, its model's tread bones' span, besides its texture: a capability names no constant of one game, and the game's track is as wide as the tank's treads.

- W3's fall plays a start of its own, the jet's initial death taken out of the death's start, and a jet keeps its drawing's velocity as it falls, its ground death written by every jet with its height, 9 frames squared of the game's gravity: in the game a jet on the ground plays no initial death and dies at once whatever its ground death names, and its locomotor keeps its speed as it falls (`JetSlowDeathBehavior::onDie`, `beginSlowDeath`); a torn drawing's gauges go with the HUD.

- W2's wrecks meet the ground by the effects file's `wrecks`, its gravity with the game's ground stiffness, friction and rest, which the import writes, not by its gravity alone: the game's landing takes its ground's stiffness, and its friction and rest are the physics' own numbers, data a game gives.
- C3's `sim_run` takes 110 s of wall time unless named, not 120 s: two minutes is Claude Code's wait before it sends a call to the background, which the run stays under.
- C2's client answers a run once it received the state of the tick the run paused at and drew a frame after it, not once its view draws that tick: the local server's pause stops the client's clock too, so its interpolation never reaches the tick (console.md, A run answers once its client received its tick).
- C2's client of a remote server refuses every request of the match, not the run alone, as it holds no session's phase and only its team's view (console.md, A remote server's client answers no request of the match).

## Foundations
