# Roadmap — milestone 1

Open items only. Remove an item when it is done; remove a stage when it is empty.

## 0. Before code
- CI on Linux x86_64, Windows x86_64, macOS aarch64.

## 2. Prototype gate
- One unit in Lightyear's World, predicted client, bare-`World` verifier: equal hash every tick on all CI platforms.

## 3. Vertical slice
- 1v1, one lane, one hero with one Rhai ability, creeps, tower, fog, capsule art, LAN.
- Handshake, session log with seed commit-reveal, verifier on another OS. Is it fun?

## 4. Genre proofs
- Neutral core: modes declare damage kinds, stats and resources; avatar, loadout, spawn group and path replace hero, spells, wave and lane in the core API.
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
