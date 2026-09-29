# Roadmap — milestone 1

Open items only. Remove an item when it is done; remove a stage when it is empty.

## 0. Before code
- CI on Linux x86_64, Windows x86_64, macOS aarch64.

## 1. Determinism core
- Snapshot encoding in `sim`: the state hasher's encoder with a byte sink.

## 2. Prototype gate
- One unit in Lightyear's World, predicted client, bare-`World` verifier: equal hash every tick on all CI platforms.
- Measure Schnorr checks per packet and rollback cost. On failure, revisit decision 1.

## 3. Vertical slice
- 1v1, one lane, one hero with one Rhai ability, creeps, tower, fog, capsule art, LAN.
- Handshake, session log with seed commit-reveal, verifier on another OS. Is it fun?

## 4. Engine semantics
- Scripting: fixed Rhai hashing seed, explicit Rhai limits, package load checks, all-or-nothing calls, tick budget, typed state, timers.
- Kit: all primitives. Server: crash restore, reconnect, late join, receipts.
- Client: interpolation, presentation scripts, asset limits.

## 5. Reference game
- 3v3 two-lane map, 6 heroes, ~20 items, structures, neutral objective, team-view bots.

## 6. Hardening
- `det-ci` bot matches per commit, worst-case tick budgets, log tamper tests, mDNS, local singleplayer server.
