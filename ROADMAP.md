# Roadmap — milestone 1

Open items only. Remove an item when it is done; remove a stage when it is empty.

## 3. Vertical slice
- A playtest of the 1v1 on LAN with circle collision, the one gap two playtests found; the rest works, and verifies on Linux and macOS.

## 4. Genre proofs
- A tiny `det-ci` test mode for each target game: a CS round, an RTS skirmish, a BR zone, an MMO zone ([Genres](design/04-capabilities/genres.md#genre-proofs)), with the first cut of each capability they need: `items`, `progression`, `interaction`, `production`, `character`, `hitscan`, level geometry, `persistence`.

## 5. Engine semantics
- Scripting: fixed Rhai hashing seed, explicit Rhai limits, package load checks, all-or-nothing calls, tick budget, typed state, timers.
- Capabilities: every primitive the reference game uses. Server: crash restore, reconnect, late join, receipts.
- Client: interpolation, presentation scripts, asset limits.

## 6. Reference game
- 3v3 two-lane map, 6 heroes, ~20 items, structures, neutral objective, team-view bots.

## 7. Hardening
- `det-ci` bot matches per commit, worst-case tick budgets, log tamper tests, mDNS, local singleplayer server.
- Flood limits before signature checks; NIP-49 encrypted local key files.
- Anti-cheat: log-based detection plugins, review requests and community review verdicts.
