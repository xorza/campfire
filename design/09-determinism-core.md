# Campfire — Determinism Core

Numbers, vectors, randomness, stable ids and the state hash: the base every result depends on.

## Decisions

- **D1. Own `Num(i64)`, 40.24, `*` and `/` rounded to nearest, ties to even.** No downward drift, one rounding rule everywhere. 40.24 over 32.32: 256× more range (±5.5 × 10¹¹), so script products overflow far later; the resolution, 6 × 10⁻⁸, stays far below anything a player sees.
- **D2. `round()` to an integer is half away from zero** (`2.5 → 3`, `−2.5 → −3`). What script authors expect.
- **D3. Coordinates stay within ±2²⁰ m.** Exact squared distances fit a `u128` with room to spare: `3 · (2⁴⁵)² < 2⁹²`.
- **D4. `proptest` for tests, `criterion` behind `bench`,** each added with the first code that uses it. Differential tests find rounding cases no table lists; the worst tick must be measured.
- **D5. One BLAKE3 hash per component type, then a hash over those.** The first divergence names its component type at once.
- **D6. No `fixed` or `fixed_analytics`; own parser, constants and trig.** `fixed` rounds `*` toward −∞ and its constants down; `fixed_analytics` reaches 48 ulp in `atan2`. Own series at high internal precision reach 1–2 ulp.

## Design

- **`Num`:** 40.24 fixed-point in an `i64`. Checked arithmetic; `*` and `/` round once, to nearest, ties to even; an overflow in engine code panics, and in a script it is a script error. Exact decimal parsing of data strings, which returns an error on bad input. No conversion from or to floats.
- **Roots and trig:** `sqrt` and vector lengths are exact to the nearest value; a float gives only the first estimate, which integer steps correct, so the result never depends on it. `sin_cos` and `atan2` come from tabled series computed at compile time, in integer steps, so they give the same bits on every machine. Their error bound is what the tests prove: within 0.501 ulp of `f64` over sweeps of angles and points. No search of the hard cases, the values near a rounding midpoint, proves them correctly rounded, so the design claims only that bound.
- **`Vec3`:** checked operations; `dot` and rotations round once; `within(other, radius)` compares exact squared distances, with no root.
- **`Rng`:** one BLAKE3-keyed sequence per (stream, entity, tick), keyed by the segment seed, so no draw depends on another's order and systems can draw in parallel. `below`, `chance` and `chance_ratio` are exact; `chance` always takes one word. Debug builds panic when a tick opens one sequence twice. A stream is an `RngStream`, a constant its owner declares, such as combat's `combat.roll`: a name of 1 to 64 lowercase letters, digits, `_` and `.`, checked when it is built, so no stream is text made while a match runs. The streams belong to several crates and to their tests, so a closed enum in the math crate would have to name each of them; each owner names its own instead.
- **Stable ids:** handed out in order and never reused; the index of ids to entities is the order every hash walks.
- **State:** each state type has a fixed name, and the registry hashes, snapshots and restores all of them in name order, then by stable id. A snapshot holds exactly the bytes the hash reads, so they cannot disagree. A restore reads untrusted bytes and returns an error for every flaw, never a panic. It restores into a match whose books the packages loaded, and a resource the snapshot records as absent is removed. Once every type is decoded, each value is checked against the books and the other types: a unit type, team, player, path, action, rank, modifier, stat place, param, track or damage kind must be one the match has, a mode's or a modifier's state must have its declared fields' types, and a unit with a route must walk as one of the mode's kinds of walker. The check is a required method of the state traits, so the compiler proves that every state type has one.

## Tests

- Every operation has hand-computed cases, including ties, limits and sign changes, and `proptest` differential tests against exact references.
- Trig and `sqrt` stay within 0.501 ulp of `f64` over sweeps; the RNG passes the official BLAKE3 vectors; the state hash is the same whatever order a world was built in, and a snapshot restores to the same hash.
- The **golden test** hashes fixed workloads, one digest per section (`num`, `trig`, `vec3`, `rng`, `state`), and CI requires the same digests on every platform. They change only when a release changes results on purpose.
