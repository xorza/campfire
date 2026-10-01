# Roadmap — milestone 1

Open items only. Remove an item when it is done; remove a stage when it is empty.

## 3. Vertical slice
- Routes and collision: units plan around structures and around units that block them, as the fourth playtest showed they must; then a playtest of the 1v1 on LAN. The rest works, and verifies on Linux and macOS.

## 4. Genre proofs
- A tiny `det-ci` test mode for each target game: a CS round, an RTS skirmish, a BR zone, an MMO zone, a Diablo level, an RTS mission, an RPG town ([Genres](design/04-capabilities/genres.md#genre-proofs)), with the first cut of each capability they need: `items`, `progression`, `interaction`, `production`, `character`, `hitboxes`, level geometry, `quests`, `world`; and the test that every pair of capabilities meets in a test mode ([Testing combinations](design/04-capabilities/00-overview.md#testing-combinations)).

## 5. Engine semantics
- Scripting: fixed Rhai hashing seed, explicit Rhai limits, package load checks, all-or-nothing calls, tick budget, typed state, timers.
- Capabilities: every primitive the reference game uses. Server: crash restore, reconnect, late join, receipts.
- Client: interpolation, presentation scripts, asset limits.

## 6. Reference game
- 3v3 two-lane map, 6 heroes, ~20 items, structures, neutral objective, team-view bots.

## 7. Hardening
- `det-ci` bot matches per commit, worst-case tick budgets, log tamper tests, mDNS.
- Flood limits before signature checks; NIP-49 encrypted local key files.
- Anti-cheat: log-based detection plugins, review requests and community review verdicts.
