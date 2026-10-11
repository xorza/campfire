# Math timings

The `math` crate's atomic benches on the branch `math_opt`, in ns for each operation: the bench's
time over its 4,096 inputs. Ryzen is a Ryzen 7 6800U (x86-64-v3), pinned to one core; M2 is an
Apple M2. Each figure is the best of three runs, the base and the change run in turns. The base is
`7873fd75`, before the branch; "now" is `f8648da5`. Later commits: `7ddf9782`, the narrow root
truncated through `i64`, `floor_root/narrow` −19 % on the Ryzen; `a88ac345`, a box's push with
one fine root and a float first try, `collision/boxes` −6.5 % and −7.9 %; `19b774ab`, products
rounded by an added half and a product summed in one 256-bit step, `num/mul` −17 % and −19 %,
`vec3/dot` −24 % and −7 %, `vec3/rotated_y` −21 % and −16 %, `product_sum/sum` −21 % and −7 %,
`num/atan2` −5 % and −1 %; `b932e59e`, squares summed wrapping, `vec3/within` −6 % and 0 %.

## Current

| Bench | Ryzen base | Ryzen now | Change | M2 base | M2 now | Change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `atomic/ceil_root/wide` | 6.57 | 5.96 | -9 % | 5.47 | 3.67 | -33 % |
| `atomic/floor_root/lanes` | 0.64 | 0.63 | -1 % | 0.70 | 0.70 | +0 % |
| `atomic/floor_root/narrow` | 3.06 | 2.91 | -5 % | 1.44 | 1.45 | +0 % |
| `atomic/floor_root/wide` | 5.36 | 4.69 | -13 % | 4.28 | 2.70 | -37 % |
| `atomic/num/atan2` | 19.57 | 14.45 | -26 % | 26.23 | 18.73 | -29 % |
| `atomic/num/atan2_near_axis` | 19.36 | 14.04 | -28 % | 21.07 | 12.24 | -42 % |
| `atomic/num/div` | 3.97 | 2.81 | -29 % | 8.75 | 2.62 | -70 % |
| `atomic/num/mul` | 1.93 | 1.87 | -3 % | 1.06 | 1.10 | +4 % |
| `atomic/num/mul_div` | 5.81 | 4.32 | -26 % | 8.44 | 5.76 | -32 % |
| `atomic/num/sin_cos` | 17.29 | 12.06 | -30 % | 12.12 | 8.69 | -28 % |
| `atomic/num/sin_cos_huge` | 17.36 | 12.08 | -30 % | 12.25 | 8.78 | -28 % |
| `atomic/num/sqrt` | 5.62 | 2.23 | -60 % | 3.69 | 2.04 | -45 % |
| `atomic/product_sum/sum` | 32.69 | 19.25 | -41 % | 34.57 | 11.85 | -66 % |
| `atomic/rng/below` | 7.95 | 7.90 | -1 % | 8.70 | 8.99 | +3 % |
| `atomic/rng/chance` | 7.81 | 7.82 | +0 % | 8.95 | 8.96 | +0 % |
| `atomic/rng/chance_ratio` | 8.26 | 8.16 | -1 % | 8.93 | 8.95 | +0 % |
| `atomic/rng/fraction` | 7.58 | 7.58 | +0 % | 8.43 | 8.43 | +0 % |
| `atomic/rng/next_u64` | 7.53 | 7.53 | +0 % | 8.33 | 8.35 | +0 % |
| `atomic/rng/open` | 42.51 | 21.06 | -50 % | 48.89 | 16.71 | -66 % |
| `atomic/rng/pick` | 7.97 | 7.94 | -0 % | 8.70 | 8.72 | +0 % |
| `atomic/rounding/divide` | 6.07 | 6.45 | +6 % | 7.20 | 5.36 | -26 % |
| `atomic/rounding/shift_right` | 3.71 | 3.71 | +0 % | 2.96 | 2.97 | +0 % |
| `atomic/u256/cmp_products` | 8.25 | 7.24 | -12 % | 7.51 | 4.95 | -34 % |
| `atomic/u256/div_ceiling` | 5.39 | 5.37 | -0 % | 8.50 | 6.47 | -24 % |
| `atomic/u256/div_nearest_even` | 6.28 | 6.16 | -2 % | 10.97 | 8.65 | -21 % |
| `atomic/u256/product` | 2.10 | 2.13 | +1 % | 1.27 | 1.27 | +0 % |
| `atomic/u256/shr_rounded` | 5.48 | 5.39 | -2 % | 3.68 | 3.74 | +2 % |
| `atomic/vec3/distance` | 11.00 | 6.18 | -44 % | 6.22 | 3.33 | -46 % |
| `atomic/vec3/dot` | 3.69 | 3.62 | -2 % | 2.18 | 2.18 | +0 % |
| `atomic/vec3/length` | 13.98 | 4.54 | -68 % | 5.65 | 2.75 | -51 % |
| `atomic/vec3/normalized` | 30.06 | 15.15 | -50 % | 33.87 | 9.77 | -71 % |
| `atomic/vec3/rotated_y` | 4.70 | 4.71 | +0 % | 2.86 | 2.88 | +1 % |
| `atomic/vec3/step_toward` | 31.52 | 16.95 | -46 % | 46.84 | 16.59 | -65 % |
| `atomic/vec3/within` | 2.71 | 2.78 | +3 % | 2.32 | 2.32 | +0 % |

## Capabilities, from `Flat` on `i64` axes

The `capabilities` benches before and after `8ab365d8`, in µs for each run of the bench.

| Bench | Ryzen before | Ryzen after | Change | M2 before | M2 after | Change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `atomic/body_box/overlap` | 148.6 | 31.5 | −79 % | 76.5 | 22.9 | −70 % |
| `atomic/body_box/reach` | 346.4 | 145.7 | −58 % | 185.9 | 122.9 | −34 % |
| `atomic/shape/comes_within_box` | 1,049.8 | 475.8 | −55 % | 580.1 | 397.7 | −31 % |
| `atomic/grid/box_spans_closer` | 49,977 | 24,074 | −52 % | 28,409 | 22,401 | −21 % |
| `integration/collision/boxes` | 1,259.4 | 966.5 | −23 % | 1,054.0 | 985.0 | −7 % |
| `integration/collision/bridge` | 1,320.1 | 1,004.3 | −24 % | 1,104.2 | 1,027.5 | −7 % |
| `integration/route_planner/walled` | 28,563 | 27,037 | −5 % | 27,663 | 27,726 | 0 % |

## Kept

| Change | Where it gains |
| --- | --- |
| `Rounding::divide`: magnitudes below 2⁶⁴ divide natively, not by the `u128` library call | `num/div`; every `Num` division |
| Roots: a `u64` path; values below 2¹⁰² correct a float root on 64-bit residuals; larger ones take a residual step in floats, no integer division; `#[inline(always)]` | `floor_root/wide`, `ceil_root/wide` |
| `NearestRoot`: the rounded estimate checked by `n² − n < v ≤ n² + n`, for `Num::sqrt` and `Vec3::length` | `num/sqrt`, `vec3/length`, `distance` |
| `Vec3::normalized`: one `f64` scale and a `u64` correction for each component | `vec3/normalized` |
| `sin_cos`: the quadrant and the table's sign by selects; the series by high-half products; results rounded on `i64` | `num/sin_cos` |
| `atan2`: the series by high-half products; the angle's magnitude rounded on `u64`; the rotated tangent by `WideDivision` | `num/atan2` |
| `Rng::new`: the message in one `update` | `rng/open` |
| `ProductSum`: one 256-bit two's complement sum, no branch on signs; each product's two parts joined into one addend first | `product_sum/sum` |
| `Num::from_raw_products` and `atan2`'s rounding: `(p + 2ⁿ⁻¹ − 1 + bitₙ(p)) >> n` in place of the rest's compares | `num/mul`, `vec3/dot`, `vec3/rotated_y`, `num/atan2` |
| `U256::cmp_products`: 256-bit products when both values fit 128 bits | `u256/cmp_products` |
| `WideDivision`: a `u128` division by each platform's fastest way: native on x86-64, a float estimate and an exact correction on AArch64, one step for quotients below 2⁴⁹ | `num/mul_div`, `u256/div_*`, `rounding/divide` on the M2 |
| `StepMoves`: `step_toward`'s moves natively on x86-64, from one float ratio on AArch64 | `vec3/step_toward` |
| `Flat` on `i64` axes, each product widened | `capabilities`: boxes and their collisions |

`rounding/divide` on the Ryzen is 6 % slower: the bench turns through all three rounding modes, and
the split of `divide` into a runtime and a `const` form changed the loop's layout. With one mode, as
its callers pass it, `num/div` is 29 % faster.

## Tried and dropped

| Change | Ryzen | M2 | Why dropped |
| --- | ---: | ---: | --- |
| Root estimates rounded to nearest, corrected with no branch | `vec3/length` +28 % | −11 % | The correction lies on the dependency chain; a truncated estimate is almost always the floor, and the CPU predicts the rare step |
| Root floor as `round(e − ½)` by the magic number | `floor_root/wide` +15 % | +30 % | A whole estimate, as a square's, ties to the even one below: 2 % of the bench's values took a step |
| Narrow root from two exact signed parts and an FMA | −4 % | `floor_root/narrow` +43 % | `ucvtf` converts a `u64` in one instruction on ARM |
| `floor_root` left to the inliner | `floor_root/narrow` +11 % | +22 % | The call stays |
| `atan2`'s and `step_toward`'s divisions by float estimates on x86-64 | `num/atan2` +23 %, `vec3/step_toward` +26 % | −15 %, −37 % | x86-64 divides by a 64-bit divisor with `div`; kept on AArch64 alone, behind `WideDivision` and `StepMoves` |
| `ProductSum::add` with one product for a count within `i64` | `product_sum/sum` +5 % | +129 % | The branch keeps the 8-term loop rolled, each term waiting on the last one's carries |
| Exact circle overlap in four lanes, 23-bit limbs | 4.5 × slower | 3.8 × slower | AVX2 and NEON have no 64 × 64 → 128 lane multiply |
| BLAKE3 output blocks in parallel lanes | — | — | Not built: `blake3` has a many-block XOF for AVX-512 alone, and a stream draws few words, so its first block dominates |
| `atan2`'s table index by an `f64` division, its floor exact | `num/atan2` +21 % | +23 % | A 64-bit integer division of 39 bits by 33 is faster on both |
| `Frame::nearest_to` from sums of shared products, 9 products in place of 14 | `body_box/reach` +16 % | +7 % | Each sum is a checked `i128` addition; the edges' own `i64` subtractions and widening products cost less |
| `U256::cmp_products` with no product for a factor of 1 | −10 % | +11 % | No gain in the box benches, whose compares it serves |
| `Collider::reaching` by an `f64` estimate and two exact 256-bit checks | `collision/crowded` +3 % | 0 % | It runs too rarely beside the broadphase to show |
| `sin_cos`'s quadrant from the product's high half, the reduction's product on `i64` | `num/sin_cos` +2 % | +3 % | No gain |
| `sin_cos`'s results rounded by the added half | `num/sin_cos` −5 % | +10 % | `sin_cos` waits on the rounding, whose chain is a step longer; kept in `atan2` alone |
| `Rounding::divide`'s 64-bit path rounded on `u64` | `vec3/step_toward` +16 %, `rounding/divide` −7 % | `num/div` −10 %, `rounding/divide` +9 % | No steady gain: this function's speed follows its code's layout more than its instructions |
