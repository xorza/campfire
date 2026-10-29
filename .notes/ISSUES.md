# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide


## Research


## Ready

- **Actions.** `ActionsFixtures::load` builds an action's weapon, train and params by hand, and builds no range bonus, research spec or gather spec of a fixture-loaded action.
- **Airfields.** Airfields writes production's `HeldPlaces`, `Doors` and `MadeAt` and combat's `ClipShots`, and re-derives which clips refill at home, where AGENTS.md has a capability ask another through a contract below both.
- **Import.** `ZeroHour::write` is some 500 lines through which each type's sections pass in 19 maps; the types close after production and combat read the models' bones, so the converted models, the sounds and the terrain textures wait in memory until the end, and one 1,000-line test pins every stage.
- **Navigation.** The nearest-open-cell scans, of up to 2²² cells, run outside the tick's route budget, and the crowd costs hold three `u16` grids for each kind and team, some 231 MB at 1024² with 6 kinds and 6 teams, made mid-tick.
- **Logging.** The console parses each event's JSON line back into fields, as `LogTap` hands out the file writer's text.
- **Script.** The match's names, tags, teams, filters and data tables, which no Rhai type holds, live in the Rhai crate under `scripts::view`, so every crate that resolves a name depends on `script`, and `MatchNames` is declared as a `Cache` whose `clear` does nothing.
- **Tests.** Run alone, three tests pass 1 s: the scenario's server stall (1.06 s), the verifier's 3v3 log with learn orders (1.05 s), and the local server's play (1.02 s, on the OS clock).
- **Units.** The `Units` module declares and installs the script runtime, its host, view, budgets, prints, failures and core API, beside the unit model.
