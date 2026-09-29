# Campfire — Determinism Core (Stage 1)

The first engine code: numbers, vectors, randomness, stable ids and the state hash.

**Exit criteria:** every operation has tests with hand-computed expected values; one golden test hashes a fixed workload, and CI requires the same digest on Linux x86_64, Windows x86_64 and macOS aarch64.

## Decisions

- **D1. Own `Num(i64)`, 40.24, `*` and `/` rounded to nearest, ties to even.** No downward drift, one rounding rule everywhere. 40.24 over 32.32: 256× more range (±5.5 × 10¹¹), so script products overflow far later; the resolution, 6 × 10⁻⁸, stays far below anything a player sees.
- **D2. `round()` to an integer is half away from zero** (`2.5 → 3`, `−2.5 → −3`). What script authors expect.
- **D3. Coordinates stay within ±2²⁰ m.** Exact squared distances fit a `u128` with room to spare: `3 · (2⁴⁵)² < 2⁹²`.
- **D4. `proptest` for tests, `criterion` behind `bench`,** each added with the first code that uses it. Differential tests find rounding cases no table lists; the worst tick must be measured.
- **D5. One BLAKE3 hash per component type, then a hash over those.** The first divergence names its component type at once.
- **D6. No `fixed` or `fixed_analytics`; own parser, constants and trig.** `fixed` rounds `*` toward −∞ and its constants down; `fixed_analytics` reaches 48 ulp in `atan2`. Own series at high internal precision reach 1–2 ulp.

## Design

### `math::Num` (`num.rs`)

- `#[repr(transparent)] struct Num(i64)`: value × 2²⁴. `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]`, serialized as its `i64`.
- **Arithmetic.** `+`, `-`, unary `-` are checked. `*` rounds the exact `i128` product to nearest, ties to even, before the shift; `/` computes `(a << 24) / b` in `i128` with the same rounding. An overflow or a division by zero panics: in engine code it is a bug. The script layer calls `checked_*` versions, which return `None`, and turns that into a `script_error`.
- **With integers:** `Num * i64` and `Num / i64`, exact scaling without a conversion.
- **Conversions:** `Num::from_int(i64) -> Option<Num>` (exact, for `|i| < 2³⁹`); `floor`, `ceil`, `round` to `i64`; `from_bits`, `to_bits`. No conversion from or to floats.
- **Parsing:** `FromStr` for data strings such as `"7.5"`: the integer part, the first 25 fractional digits as an exact fraction, a flag for any later non-zero digit, one rounding to nearest, ties to even. 25 digits suffice because every midpoint between neighbouring values, `(2j + 1) / 2²⁵`, has exactly 25; later digits only break a tie. Package data is untrusted, so parsing returns an error, never panics. `Display` prints the exact decimal, at most 24 fractional digits.
- **`sqrt`:** the root of `raw << 24`, rounded to nearest; the square root of an integer is never exactly a half. An `f64` square root gives the estimate, then integer steps correct it to exactly ⌊√·⌋, so the result does not depend on the float; three times as fast as `u128::isqrt`. It is the one float in the sim crates, under an `expect` with this reason.
- **Kernels** work at 2⁻⁶² in `i64`, so every product is one 64×64 multiply. Their tables and constants are computed at compile time by exact series (Taylor, and `atan` with two argument halvings), so nothing is written by hand.
- **`sin_cos`** returns `SinCos { sin, cos }`: the quadrant from one multiply by 2/π; the remainder reduced exactly with π/2 at 101 bits, modulo 2¹²⁸; the nearest of 52 tabled angles `k/64`; a 4-term series on the rest, |b| ≤ 1/128; the angle-addition formula; one rounding at the end.
- **`atan2`:** the nearest of 33 tabled tangents `k/32`, found with one 64-bit division; the vector rotated by that angle, which needs no division; one division for the remaining tangent, |u| ≤ 1/64; a 4-term series; quadrant fix-up; one rounding.
- **Measured:** both are correctly rounded (at most 0.5 ulp against `f64` over a million inputs in each of four regions) and take about 15–20 ns, against 200–230 ns for the plain series.
- **Constants:** `ZERO`, `ONE`, `EPSILON`, `MIN`, `MAX`, `PI`, `TAU`, `FRAC_PI_2`. π comes from Machin's formula (`16·atan(1/5) − 4·atan(1/239)`) at 2⁻¹²⁰ in a `const fn`; compilation fails if the formula's error bound could move π across a rounding midpoint.

### `math::Vec3` (`vec3/`)

- `struct Vec3 { x: Num, y: Num, z: Num }` with checked `+`, `-`, `-v`, `* Num`, `/ Num`; operators panic, `checked_*` return `None`.
- **Rounded once:** `dot` and `rotated_y(SinCos)` round the exact sum of raw products a single time, through `Num::from_raw_products`.
- **Exact length:** `length` and `distance` sum the squared raw components in `u128`, which always fits (three squares of at most 2⁶³), and take the nearest root through `Num::from_root_of_bits`, the same estimate-and-correct root as `sqrt`, with one Newton step above 2⁵² so correction never loops long.
- **`within(other, radius)`** compares exact squared distances, with no root: the fast path for radius queries.
- **`normalized`** (`None` for zero) takes one reciprocal ⌊2⁸⁶ / length⌋ and three multiplies, each quotient corrected by its exact remainder, so every component equals the correctly rounded division; `direction_to(other)` normalizes the difference.
- The sim checks D3 where it sets a position; results that exceed `Num` return `None` and cannot wrap.

### `math::Rng` (`rng/`)

- **`RngSource`** holds the `SegmentSeed` and opens sequences: `begin_tick(tick)`, then `open(stream, entity) -> Rng`. It takes `&self`, so systems can open sequences in parallel.
- **The message** is `"campfire/rng/v1" ‖ u32 length ‖ stream name ‖ u64 entity ‖ u64 tick`, little-endian and length-prefixed, so no two fields run together; the key is the segment seed; the output is BLAKE3's extended output.
- **Whole blocks:** BLAKE3 computes output 64 bytes at a time and `OutputReader::fill` redoes that for every partial read, so `Rng` fills a 64-byte buffer and serves 8 words from each compression.
- `next_u64()`: the next 8 bytes. `below(n)`: Lemire's multiply-and-reject, one step generic over the word width. `chance(p: Num)`: `p ≤ 0` is false, `p ≥ 1` is true, else `(word >> 40) < p.to_bits()`, exactly probability `p`; it always takes one word, so later draws never depend on `p`. `chance_ratio(num, den)`: `below(den) < num`, exactly `num / den`, for chances too small for 24 fractional bits, such as a 1-in-a-million drop. `pick(len)` is `below(len)`.
- **One sequence per (stream, entity, tick).** A second `Rng` with the same three values repeats the draws of the first. Debug builds record the pairs opened since `begin_tick` behind a `Mutex` and panic on a repeat; running a tick again, as rollback does, calls `begin_tick` again. Release builds keep no record.

### `sim`: stable ids and the state hash

- **`StableId(u64)`** is an immutable component; **`IdAllocator`** (a resource) hands out ids in order and never reuses one, so a dead unit's handle keeps its meaning. The allocator's next id is state and is hashed: two worlds that differ in it diverge at the next spawn.
- **`EntityIndex`** (a resource): `BTreeMap<StableId, Entity>`, the order every hash walks, with no sorting per tick. `StableId`'s `on_insert` and `on_discard` hooks keep it current on every path: `World`, `Commands`, replacement, despawn. Being immutable, a `StableId` changes only by an insert, which the hooks see.
- **`SimComponent`** and **`SimResource`**: traits for the types that are state, each with a `NAME` such as `"moba.health"`; names fix the order, since `TypeId` is not stable across builds. Registering a name twice panics. Components the network layer adds are not state and never enter the hash.
- **`StateRegistry`** registers the state types and does three things with them: hash, snapshot, restore. For each type in name order it feeds `(StableId, component)` pairs in id order, or a resource as an `Option` so a missing one differs from an empty one, through postcard into a sink, batched in a 64-byte buffer. The sink is BLAKE3 for the hash (then one hash over the `(name, type hash)` list, D5) or a byte vector for a snapshot, so a snapshot and its hash cannot disagree.
- **`sim.entities`**, a built-in type, lists every live stable id, so an entity with no registered component still exists after a restore.
- **Snapshot:** the tag `campfire/snapshot/v1`, then for each type a `u32` name length, the name, a `u64` body length and the body: exactly the bytes that type's hash consumes.
- **Restore** reads untrusted bytes, so every flaw is a `SnapshotError`, never a panic: a missing tag, truncation, types that differ from the registry, a value that does not decode (with postcard's error), stable ids that do not strictly increase (only the canonical encoding is accepted), a component for an unlisted entity, an allocator behind the ids in use, trailing bytes. The entity list is restored first, so components find their entities whatever the name order.
- `hash_by_type` puts the per-type hashes into a caller's reused `&mut Vec`, and `snapshot` writes into a caller's reused `&mut Vec`. `det-ci` compares per-type hashes; production computes only `hash`, at checkpoints.

## Tests

- **`Num`:** hand-computed cases for each operation, including ties (`0.5 × 2⁻²⁴` both ways), `MIN`, `MAX`, `−1` raw and sign changes; `proptest` differential tests against an exact rational reference in `i128`.
- **Parsing and π:** π from Machin must equal a 40-digit decimal rounded by the parser.
- **`sqrt`, `distance`:** exact cases (`sqrt(4) = 2`, the 3-4-5 triangle, `2⁻²⁴` steps), and the rounding rule against `isqrt` directly.
- **Trig:** exact points (`sin 0 = 0`, `cos 0 = 1`, `atan2(1, 1)` = `PI / 4`), exact symmetries, and sweeps over small and huge angles and over `atan2` grids, at most 0.501 ulp from `f64`. Floats appear only in these tests, under an `expect` with its reason.
- **`Rng`:** the official BLAKE3 keyed vectors, with extended output; the documented message layout across a block boundary. Lemire's method and `chance_ratio`, checked exhaustively with 16-bit words. `chance` at `p = 0`, `1`, `2⁻²⁴`, `1 − 2⁻²⁴`, and that it always takes one word.
- **State hash:** the same world built in two orders hashes equal; changing one field changes exactly that type's hash; extra non-sim components change nothing; the allocator's next id changes only its own hash; the buffered writer equals BLAKE3 over postcard's bytes, across its buffer size. The index follows spawn, replacement and despawn. A snapshot restores to the same hash and re-snapshots byte for byte, the allocator continuing as before; every truncation of a real snapshot is refused, no single-byte corruption panics, and a crafted snapshot for each flaw returns exactly its error.
- **Golden test** (`sim/tests/golden.rs`, public API only): fixed workloads for `num`, `trig`, `vec3`, `rng` and `state` (the latter including the snapshot bytes and their restore), each with its own checked-in digest, so a mismatch names the section of the first divergence. Inputs come from `SplitMix64` in the test. The digests hold in debug, release and `-C target-cpu=native`; a broken tie rule in `Num` fails exactly `num`. They change only when a release changes results on purpose.

## Plan

The open steps are in [PLAN.md](../PLAN.md).

## Not in Stage 1

Script bindings for `Num` (Stage 4), snapshots on disk and checkpoints (Stage 4), the Lightyear prototype (Stage 2), CI.
