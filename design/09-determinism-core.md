# Campfire — Determinism Core (Stage 1)

The first engine code: numbers, vectors, randomness, stable ids and the state hash.

**Exit criteria:** every operation has tests with hand-computed expected values; one golden test hashes a fixed workload, and CI requires the same digest on Linux x86_64, Windows x86_64 and macOS aarch64.

## Decisions

- **D1. Own `Num(i64)`, 32.32, `*` and `/` rounded to nearest, ties to even.** No downward drift, one rounding rule everywhere.
- **D2. `round()` to an integer is half away from zero** (`2.5 → 3`, `−2.5 → −3`). What script authors expect.
- **D3. Coordinates stay within ±2²⁰ m.** Exact squared distances fit a `u128`: `3 · (2⁵³)² < 2¹²⁸`.
- **D4. `proptest` for tests, `criterion` behind `bench`,** each added with the first code that uses it. Differential tests find rounding cases no table lists; the worst tick must be measured.
- **D5. One BLAKE3 hash per component type, then a hash over those.** The first divergence names its component type at once.
- **D6. No `fixed` or `fixed_analytics`; own parser, constants and trig.** `fixed` rounds `*` toward −∞ and its constants down; `fixed_analytics` reaches 48 ulp in `atan2`. Own series at high internal precision reach 1–2 ulp.

## Design

### `math::Num` (`num.rs`)

- `#[repr(transparent)] struct Num(i64)`: value × 2³². `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]`, serialized as its `i64`.
- **Arithmetic.** `+`, `-`, unary `-` are checked. `*` rounds the exact `i128` product to nearest, ties to even, before the shift; `/` computes `(a << 32) / b` in `i128` with the same rounding. An overflow or a division by zero panics: in engine code it is a bug. The script layer calls `checked_*` versions, which return `None`, and turns that into a `script_error`.
- **With integers:** `Num * i64` and `Num / i64`, exact scaling without a conversion.
- **Conversions:** `Num::from_int(i64) -> Option<Num>` (exact, for `|i| < 2³¹`); `floor`, `ceil`, `round` to `i64`; `from_bits`, `to_bits`. No conversion from or to floats.
- **Parsing:** `FromStr` for data strings such as `"7.5"`: the integer part, the first 27 fractional digits as an exact fraction (`10²⁷ · 2³²` fits a `u128`), a sticky flag for any later non-zero digit, one rounding to nearest, ties to even. Package data is untrusted, so parsing returns an error, never panics.
- **`sqrt`:** `u128::isqrt` of `raw << 32`, rounded to nearest; the square root of an integer is never exactly a half.
- **`sin_cos`** returns `SinCos { sin, cos }`: reduce by π/2 with a 96-bit π, then Taylor series on |x| ≤ π/4 in `i128` with 62 fractional bits (8 terms bring the remainder below 2⁻⁴⁵), coefficients 1/n! computed, one rounding at the end.
- **`atan2`:** reduce to `atan(t)`, `0 ≤ t ≤ 1`, by quadrant; halve once with `atan u = 2·atan(u / (1 + √(1 + u²)))` through the exact `isqrt`; series with coefficients 1/(2k+1) at the same precision.
- **Constants:** `ZERO`, `ONE`, `PI`, `TAU`, `FRAC_PI_2`. π comes from Machin's formula (`16·atan(1/5) − 4·atan(1/239)`) in a `const fn`, in `u128` with guard bits, rounded to nearest.

### `math::Vec3` (`vec3.rs`)

- `struct Vec3 { x: Num, y: Num, z: Num }` with checked `+`, `-`, `-v`, `* Num`, `/ Num`, and `dot` for directions.
- **Exact length:** `distance(a, b)` and `length()` sum squared raw differences in `u128` and take a rounded `isqrt`, exact to the last bit.
- `normalized() -> Option<Vec3>` (`None` for zero), `direction_to(other)`, `rotated_y(SinCos)` for turns on the ground plane.
- The sim checks D3 where it sets a position; the `u128` arithmetic is checked too, so a violation panics and cannot wrap.

### `math::Rng` (`rng.rs`)

- `Rng::new(seed, stream, entity, tick)`: the key is the segment seed; the message is `"campfire/rng/v1" ‖ u32 length ‖ stream name ‖ u64 entity ‖ u64 tick`, little-endian and length-prefixed, so no two fields run together. Output is BLAKE3's extended output, read in order.
- `next_u64()`: the next 8 bytes. `below(n)`: Lemire's multiply-and-reject. `chance(p: Num)`: `p ≤ 0` is false, `p ≥ 1` is true, else `(next_u64() >> 32) < p.to_bits()`, exactly probability `p`. `pick(len)` is `below(len)`.
- **One sequence per (stream, entity, tick).** A second `Rng` with the same three values repeats the draws of the first. Debug builds record the triples opened in the current tick and assert each opens once.

### `sim`: stable ids and the state hash

- **`StableId(u64)`** is a component; **`IdAllocator`** (a resource) hands out ids in order and never reuses one, so a dead unit's handle keeps its meaning.
- **`EntityIndex`** (a resource): `BTreeMap<StableId, Entity>`, kept by the sim's own spawn and despawn; stable-id order without sorting each tick.
- **`SimComponent`**: a trait for each component in the state, with a `NAME` such as `"moba.health"`; names fix the order, since `TypeId` is not stable across builds. Components Lightyear adds are not `SimComponent`s and never enter the hash.
- **`StateHasher`**: for each component type in name order, it feeds `(StableId, component)` pairs in id order through a postcard flavor straight into BLAKE3, with no byte buffer, then hashes the `(name, type hash)` list (D5). State resources are hashed the same way. Snapshots use the same encoder with a `Vec` as the sink, so a snapshot and its hash cannot disagree.
- Per-type hashes go into a caller's reused `&mut Vec`. `det-ci` compares them; production computes only the final hash, at checkpoints.

## Tests

- **`Num`:** hand-computed cases for each operation, including ties (`0.5 × 2⁻³²` both ways), `MIN`, `MAX`, `−1` raw and sign changes; `proptest` differential tests against an exact rational reference in `i128`.
- **Parsing and π:** π from Machin must equal a 40-digit decimal rounded by the parser.
- **`sqrt`, `distance`:** exact cases (`sqrt(4) = 2`, the 3-4-5 triangle, `2⁻³²` steps), and the rounding rule against `isqrt` directly.
- **Trig:** exact points (`sin 0 = 0`, `cos 0 = 1`, `atan2(1, 1)` = `PI / 4`), symmetries, and sweeps over 2 million angles and 1 million `atan2` points, at most 2 ulp from `f64`. Floats appear only in these tests, under an `expect` with its reason.
- **`Rng`:** the official BLAKE3 keyed vectors, with extended output. Lemire's method, generic over the word width, checked exhaustively with 16-bit words. `chance` at `p = 0`, `1`, `2⁻³²`, `1 − 2⁻³²`.
- **State hash:** the same world built in two orders hashes equal; changing one field changes exactly that type's hash; extra non-sim components change nothing.
- **Golden test:** a fixed workload (arithmetic, trig, distances, draws, one world hash) against a checked-in digest, the reference that platforms must match.

## Plan

Each step ends with the check chain passing for the crates it touches.

1. **`Num`**: type, arithmetic with rounding, conversions; tests.
2. **`Num` functions**: exact parser, Machin π, `sqrt`, `sin_cos`, `atan2`; the ulp sweeps.
3. **`Vec3`**: operations, exact distance and length, normalizing, rotation; tests.
4. **`Rng`**: message layout, `next_u64`, `below`, `chance`, `pick`, the debug alias check; BLAKE3 vectors and the exhaustive 16-bit test.
5. **`sim` ids**: `StableId`, `IdAllocator`, `EntityIndex`, sim spawn and despawn.
6. **State hash**: `SimComponent`, the postcard-to-BLAKE3 flavor, `StateHasher` with per-type hashes; tests.
7. **Golden test** in `math` and `sim`.
8. **Benches**: the worst tick for the RNG and the hash at MOBA scale.

## Not in Stage 1

Script bindings for `Num` (Stage 4), snapshots on disk and checkpoints (Stage 4), the Lightyear prototype (Stage 2), CI.
