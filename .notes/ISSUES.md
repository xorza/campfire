# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide


## Research


## Ready

- **Actions.** `ActionsFixtures::load` builds an action's weapon, train and params by hand, and builds no range bonus, research spec or gather spec of a fixture-loaded action.
- **Airfields.** Airfields writes production's `HeldPlaces`, `Doors` and `MadeAt` and combat's `ClipShots`, and re-derives which clips refill at home, where AGENTS.md has a capability ask another through a contract below both.
- **Code style.** Over 400 Rust files outside `campfire-capabilities` put a `use` of the crate or of another crate out of the order std, external, crate.
- **Import.** `ZeroHour::write` is some 500 lines through which each type's sections pass in 19 maps; the types close after production and combat read the models' bones, so the converted models, the sounds and the terrain textures wait in memory until the end, and one 1,000-line test pins every stage.
- **Navigation.** The nearest-open-cell scans, of up to 2²² cells, run outside the tick's route budget, and the crowd costs hold three `u16` grids for each kind and team, some 231 MB at 1024² with 6 kinds and 6 teams, made mid-tick.
- **Net.** The frame-interpolation, interpolation and prediction registries copy one seal, deferral and sorted insert, about 150 lines.
- **Logging.** The console parses each event's JSON line back into fields, as `LogTap` hands out the file writer's text.
- **Script.** The match's names, tags, teams, filters and data tables, which no Rhai type holds, live in the Rhai crate under `scripts::view`, so every crate that resolves a name depends on `script`, and `MatchNames` is declared as a `Cache` whose `clear` does nothing.
- **Session.** A restore checks the signature of every journal packet of the session, 18.3 µs each (15.8 s for each hour of 8 players), though the live log forgets all before its latest checkpoint; the owner decided on 2026-10-29 that a restore trusts its own journal before its base and checks the records after it, which needs the journal to record, at each checkpoint, the log's file length and each player slot's chain and controller, as the checkpoint's signed record carries the match's state alone.
- **Session.** No tool prints a session log as text: its header, segments and checkpoints, each tick's packets and server inputs, the seed reveal and the result.
- **Session.** A publication reads the journal twice, and an abort three times, as the shape it counts (each tick's entries across loads) is rebuilt by a first read rather than kept as the journal appends.
- **Tests.** Run alone, four tests pass 1 s: the scenario's server stall (1.06 s), the verifier's 3v3 log with learn orders (1.05 s), the local server's play (1.02 s, on the OS clock), and the leak check (3.1 s), which the name `scripted_match` exempts though it is no mode's scripted match.
- **Test packages.** The 3v3's tree holds its own copies of `test-ai`'s creep and tower scripts, as `PackageTree` reads a mode's packages within its tree alone.
- **Units.** The `Units` module declares and installs the script runtime, its host, view, budgets, prints, failures and core API, beside the unit model.
