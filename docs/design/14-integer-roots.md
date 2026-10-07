# Campfire — Integer roots

Proposal: one exact integer root in `math`, faster than `core`'s at every width, for `math`'s own roots and for every caller that takes the root of a wide integer.

## Today

- **The math crate is not hot.** Profiled with frame pointers ([B7](13-benches.md#decisions)) on one core of a Ryzen 7 6800U, each sample counted by its source file, inlined code included: `campfire-math`'s own code takes 1.1 % of `server_tick/mean_3v3`, 1.4 % of `route_planner/walled`, the path of the worst ticks, 0.1 % of `pathing_grid/one`, and less than 0.1 % of `collision/crowded` and `fog/sight`. The RNG, BLAKE3 included, takes 0.02 % of the mean tick. A change to its own operations saves at most about 1 % of any of them.
- **Two callers take roots outside it.** `Grid::spans_within`, which vision's fog calls for each sight, takes the floor root of each row's squared half-width with `u128::isqrt`; `Collider::part` takes the floor root of each contact's squared distance the same way. `u128::isqrt` takes 24.6 % of `fog/sight`, 4.0 % of `collision/crowded` and 2.8 % of `server_tick/mean_3v3`, all of the last in the Vision stage.
- **The callers' values fit 64 bits.** A sight of 10 m is 2²⁸·³ halves of a bit, so a row's squared half-width is below 2⁵⁷; two bodies in contact stand about a meter apart, 2²⁴ bits, so a contact's squared distance is near 2⁴⁸. The types allow more, 2⁹⁶ for a row and 2⁹¹ for a contact by [D3](09-determinism-core.md#decisions), and such values are rare.
- **`math` has a faster root, and keeps it to itself.** `Num::from_root_of_bits`, `pub(crate)`, behind `Num::sqrt` and `Vec3::length`, takes an f64 estimate, corrects it with exact integer steps on `u128`, and rounds to nearest; it takes values below 2¹²⁶ only. A scratch program, on the same core, over 4,096 values of each width, with each result checked against `u128::isqrt`:

  | Width | `u128::isqrt` | Its floor part, on `u128` | The same on `u64` |
  | --- | --- | --- | --- |
  | 32 bits | 7.1 ns | 7.6 ns | 4.1 ns |
  | 48 bits | 9.7 ns | 7.9 ns | 4.1 ns |
  | 56 bits | 9.7 ns | 7.9 ns | 4.1 ns |
  | 64 bits | 9.3 ns | 7.6 ns | 4.3 ns |
  | 90 bits | 22.5 ns | 7.6 ns | — |
  | 120 bits | 22.8 ns | 11.5 ns | — |

  At the callers' widths its `u128` steps save only a sixth; the same steps on `u64` take less than half the time.
- **Division is a different case.** The wide divisions of the collider and the route planner, `__divti3`, take 7.2 % of `collision/crowded` and 3.3 % of `route_planner/walled`. compiler-builtins already divides a `u128` by a divisor below 2⁶⁴ with the processor's 128-by-64 `div`: 2.6 ns, and 3.9 ns for an `i128` with its signs. An f64 estimate corrected the same way takes 10.6 ns. This proposal leaves division as it is.

## How others take it

- `u128::isqrt` in `core`, and `num-integer`'s `Roots` for `u128`, take Karatsuba's square root over 64-bit halves, with no float: Newton's steps on a `u128` would each pay a 128-bit division, and an f64 does not hold every `u128`. `num-integer` takes an f64 guess for `u64` only, and corrects it with Newton's steps.
- Fixed-point game libraries, Photon Quantum's `FP` (48.16) and libfixmath (16.16), take roots from tables or from iterations of their own, and approximate them.
- `math`'s root takes the f64 guess at every width and makes it exact. The float gives only the first estimate; integer steps then compare the root's square with the value, so the result does not depend on the float, as [Determinism Core](09-determinism-core.md#design) requires of roots. Karatsuba's root avoids a float that `math` trusts only as a guess, and the measurement above finds it slower than the two paths of R1 at every width.

## Decisions

- **R1. One exact floor root, in `math`, in two paths.** `FloorRoot::floor_root` for `u128`, exact for every value, `u128::MAX` included.
  - **Below 2⁶⁴**, on `u64`: the f64 guess of a value below 2⁵³ is exact, and above it one rounding leaves the root within 1, which the integer steps correct: down while the root's square, checked with `checked_mul` for a root of 2³², passes the value, then up while value − root² > 2·root, which is (root + 1)² ≤ value with no overflow.
  - **At 2⁶⁴ and above**, the value on `u128` and the root on `u64`: for a root above 2⁵², the guess can be off by up to 2¹⁰, and one Newton step, one 128-by-64 division, brings it within 1. The guess's cast saturates, so a guess of 2⁶⁴, which only a value near 2¹²⁸ gives, becomes 2⁶⁴ − 1, the largest root, and so does a Newton step's 2⁶⁴. Each square is one widening multiply of the `u64` root, which no overflow check slows, where a `u128` root's square takes three multiplies.
- **R2. `Num`'s roots on the same root.** `Num::from_root_of_bits` becomes the floor root and its rounding step, so the f64 code exists once. `Num::sqrt` and `Vec3::length` keep their bits, and most of their values take the `u64` path: the root of a `Num` below 65,536, whose bits squared stay below 2⁶⁴, and the length of a vector shorter than 256 m.
- **R3. A trait, as `num-integer`'s `Roots` is.** `u128` is not `math`'s type, so a method on it needs a trait of `math`'s own; a call reads `square.floor_root()`, as `square.isqrt()` reads now. A free function, `root::floor(square)`, would be the only free function `math` exports.
- **R4. The callers keep their floor.** Both callers take the floor, and so does the new root, so every result keeps its bits: the proving match's goldens and the golden digests stay the same, and a change of either shows a defect.
- **R5. The trig tables keep `isqrt`.** They are built in `const` code at compile time, where the f64 root cannot run, and cost nothing at run time.
- **R6. A primitive case, `root/floor`.** 4,096 values drawn from `split_mix`, their widths spread evenly from 1 to 128 bits, so that each path counts: the `u64` path, the `u128` guess alone, and the Newton step above 2¹⁰⁴. [Benches](13-benches.md#decisions) B2 gains the group `root`.

## Built

**M1.** On one core of the Ryzen 7 6800U, each case's median against the code before, in one session:

| Case | Before | After | Change |
| --- | --- | --- | --- |
| `root/floor` | 49.7 µs, `u128::isqrt` | 22.6 µs | −55 % |
| `num/sqrt` | 36.1 µs | 20.4 µs | −43 % |
| `vec3/distance` | 53.4 µs | 43.6 µs | −19 % |
| `vec3/normalized` | 132.4 µs | 125.3 µs | −5.5 % |

Three steps the first version lacked made these figures. A root of a `u128` that is below 2⁶⁴ squares with three multiplies, and a checked `*` adds an overflow test to each, as the release profile checks overflow; the `u64` root and its widening square take one multiply and no test, and the f64 guess converts to a `u64` with no library call. A version with them still made `vec3/normalized` 11 % slower, from 20 million mispredicted branches to 203 million: `unit_component`'s rounding and sign, as random as its components, compiled to branches beside the new root. They are arithmetic now, a carry and a negation by mask, so no layout of the code can make them branches; with them it is 5.5 % faster. With its points within ±100 m, where every square is below 2⁶⁴, `vec3/distance` is 27 % faster and `vec3/normalized` 7 %. A first correction step in each direction without a branch made every case slower, 20 % for `root/floor`, and is not kept.

**M2.** `Grid::spans_within` and `spans_closer` take `floor_root`. In one session on the same core: `fog/sight` from 457.7 µs to 331.4 µs, −27.7 %, and `server_tick/mean_3v3` from 159.0 µs to 152.4 µs a tick, −4.2 %, both past the estimate below, as the root at a sight's widths takes less than the scratch program's 4.2 ns. `collision/*` stays within 0.6 %.

## Cost

From the profiles, with the root at 4.2 ns in place of 9.5 ns: `fog/sight` about 14 % faster, `collision/crowded` about 2 %, and `server_tick/mean_3v3` about 1.6 %; `num/sqrt` and `vec3/distance` gain too. Each step measures its cases before and after, in one run on one core, and the Cost section of the capability it changes takes the new figure.

## Tests

- **The root.** Hand-computed cases at each edge: 0, 1, 2, 3 and 4; the bounds of each path, 2⁵³, 2⁶⁴, 2¹⁰⁴ and 2¹²⁶, and the values one below and one above each; k² − 1, k² and k² + 2k for k = 2³² − 1, 2³², 2⁵² − 1, 2⁵², 2⁵² + 1, 2⁶³ and 2⁶⁴ − 1, the last of which is `u128::MAX`, whose root is 2⁶⁴ − 1. A `proptest` differential test against `u128::isqrt` over values of every width.
- **The same bits.** The `num` and `vec3` golden digests, `Num::sqrt`'s and `Vec3::length`'s tests, the grid's span tests, the collider's tests, and the proving match's goldens, all unchanged.

## Plan

The steps are in [PLAN.md](../../PLAN.md#integer-roots).
