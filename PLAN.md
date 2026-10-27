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

### J1. A flier on wings

Touches navigation; it stands first, as every later step flies a jet.

- [ ] No flier keeps moving (jets.md, A jet flies on wings): a flier walks as a walker does, standing while it turns toward its leg (`walking.rs:477`) and standing at its goal, so a jet would hover. A unit type's `flight` gains `wings`, its least speed and its circling radius; a winged flier in the air moves along its facing each tick at its speed and turns toward its goal at its turn rate as it moves, and with no goal circles the place it holds at its circling radius, on the server and in a predicting client alike; on the ground, below its airborne height, it walks as now. Builds: a part of the winged fliers alone, declared once, and a pass that visits only them. Checks: navigation's tests fly a winged flier to a goal behind it, pinning each tick of its turn and its pass, never standing in the air; circle it round its held place with no goal at its radius; and walk it on the ground as a walker.

### J2. A winged aim and a clip that reloads at home

Touches combat; it stands after J1, whose wings it aims with.

- [ ] An attacker turns in place to aim (jets.md, A jet aims by flying): a weapon no turret carries turns its attacker toward its target in reach and starts within `aim_deg` (`attacks.rs:308`). A winged attacker in the air turns no facing of its own: it keeps the target as its walk's goal and starts within `aim_deg` of the target's bearing. Checks: combat's test flies a winged attacker of a 30° aim at a target and pins the ticks of its starts, each within its aim, through a pass and a turn back, and a ground attacker's aim as before.
- [ ] A clip reloads only by time (jets.md, A clip may reload at home): a clip is ready its reload after its last shot (`attacks.rs:626`). A clip's `reload` of `home` stays empty until the airfields' reload refills it, through a contract of combat that airfields call, its shots back in proportion to the reload's time. Checks: combat's tests empty a clip of `home` and show no start after its reload's time, then refill it by the contract over a reload of half the clip in half its time.

### J3. An airfield holds its jets

Touches a new capability, `airfields`, and production; it stands after J2, as its jets fire clips that reload at home.

- [ ] No unit has a place to land (jets.md, An airfield is a capability; jets.md, A jet holds a place): a producer makes each unit at its one exit (`trains.rs:387`), and nothing holds a unit's place. The capability `airfields` declares a unit type's `airfield`, its places of a parking point, a facing, a prep point and a hangar each, and a runway each column of a start and an end, its approach height and its heal a second, and a unit type's `sortie`, its takeoff pause, idle time and out-of-ammunition share a second; an airfield's train of a sortie waits while no place is free, holds one as it is queued, and makes the unit at its place, or its hangar, which stays the unit's own while it lives. Builds: a module of its own, its parts declared on its kinds, and a contract with production for the exit. Checks: the airfields' tests train sorties into a two-place airfield, the third waiting until the first dies, each made at its own place.
- [ ] No unit takes off (jets.md, A jet takes off and lands on its runway): a jet ordered away from its place waits for its column's runway, taxis to its prep point, waits its takeoff pause, rolls from the runway's start to its end and climbs, one jet a runway, the next in line taking it as the first leaves. Checks: the airfields' tests take off two jets of one column, pinning each one's ticks at its prep point, at the runway's start and end, and in the air, the second only after the first leaves the runway.

### J4. A jet goes home

Touches airfields, combat's reload contract and the orders' idle; it stands after J3, whose places and runways it lands on.

- [ ] No unit lands, reloads or comes home (jets.md, A jet takes off and lands on its runway; jets.md, A jet goes home): a jet whose clips that reload at home are all empty, or that stands idle in the air for its sortie's idle time, returns to its airfield, waits for its runway, flies to its approach point at the airfield's approach height, lands from the runway's end to its start, taxis to its place, reloads its clips over the longest share of their reload times, and heals at the airfield's heal a second; a jet whose airfield is gone takes the nearest airfield of its player with a free place, and with none circles where its airfield stood, losing its share of life a second. Checks: the airfields' tests empty a jet's clip and pin its ticks home, on the runway, at its place and full again; return an idle one after its idle time; heal one parked; and move one to a second airfield, or circle one losing life, as its own dies.

### J5. The import flies Zero Hour's jets

Touches the import, the Zero Hour rules package and the golden; it stands after J4, whose parts it writes.

- [ ] The import flies no jet (jets.md, The import flies a winged locomotor): a type moves by its first normal locomotor that drives or hovers (`imported_type/mod.rs:153`), so a jet does not move, its train is dropped (`production_import/mod.rs:376`), a weapon's `AutoReloadsClip` is not read, and an airfield's `ParkingPlaceBehavior` gives only its helipad (`ini_build.rs:101`). An object of a `WINGS` normal locomotor and a `JetAIUpdate` becomes a flier of wings and a sortie, its taxi locomotor's speed its speed on the ground; an airfield's places and runways are read from its model's bones with its `ApproachHeight` and `HealAmountPerSecond`; a weapon of `RETURN_TO_BASE` reloads at home; and the afterburners, countermeasures, lock-on, sneaky offset, ejection, carrier decks, the Aurora's bomb, the Stealth Fighter's stealth, the Comanche's landing and a jet's pitch and roll are counted as losses. Checks: the import's tests convert the Raptor's and the MiG's locomotors and sorties, an airfield's places and runways from a fixture's bones, and a clip that returns to base, each loss once; the Zero Hour check's golden moves only by the jets' types, trains, weapons and losses and the bot matches they join, each change read.

### J6. A sortie in the check

Touches the Zero Hour check; it stands after J5, as it plays the import.

- [ ] No play flies a jet (jets.md, Checks): a play of the check gives an American player an airfield that trains a Raptor, orders it at a target structure, and pins the ticks of its takeoff, its first hit, its landing and its full clip again, and the target's life. Checks: the play passes, and its record replays.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] An agent runs a Tournament Desert skirmish through the console and screenshots a jet on its runway, in a pass over its target and parked again, each fault it sees going to a step before the player's check; then a player sees a jet taxi, take off down its runway, strike in passes, land and reload, and fall when shot down, in the client; then the slice's decisions fold into `04-capabilities/navigation.md`, `04-capabilities/combat.md`, a new `04-capabilities/airfields.md` and `09-zero-hour.md`, and `jets.md` goes.

## Changed

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
