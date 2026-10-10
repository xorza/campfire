# Math timings

The `math` crate's atomic benches on the branch `math_opt`, in ns for each operation: the bench's
time over its 4,096 inputs. Ryzen is a Ryzen 7 6800U (x86-64-v3), pinned to one core; M2 is an
Apple M2. Each figure is the best of three runs, the base and the change run in turns. The base is
`7873fd75`, before the branch.

## Current

| Bench | Ryzen base | Ryzen now | Change | M2 base | M2 now | Change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `atomic/ceil_root/wide` | 6.56 | 5.97 | -9 % | 5.47 | 3.75 | -31 % |
| `atomic/floor_root/lanes` | 0.64 | 0.63 | -2 % | 0.70 | 0.70 | +0 % |
| `atomic/floor_root/narrow` | 3.07 | 2.92 | -5 % | 1.44 | 1.45 | +0 % |
| `atomic/floor_root/wide` | 5.33 | 4.69 | -12 % | 4.27 | 2.70 | -37 % |
| `atomic/num/atan2` | 19.55 | 19.59 | +0 % | 26.24 | 26.23 | -0 % |
| `atomic/num/atan2_near_axis` | 19.34 | 19.35 | +0 % | 21.07 | 21.07 | -0 % |
| `atomic/num/div` | 3.98 | 3.10 | -22 % | 8.86 | 2.47 | -72 % |
| `atomic/num/mul` | 1.92 | 1.93 | +0 % | 1.07 | 1.09 | +2 % |
| `atomic/num/mul_div` | 5.82 | 4.32 | -26 % | 8.87 | 7.73 | -13 % |
| `atomic/num/sin_cos` | 17.25 | 14.52 | -16 % | 12.05 | 12.02 | -0 % |
| `atomic/num/sin_cos_huge` | 17.36 | 14.49 | -17 % | 12.23 | 12.05 | -2 % |
| `atomic/num/sqrt` | 4.99 | 2.22 | -55 % | 3.63 | 2.05 | -43 % |
| `atomic/product_sum/sum` | 32.58 | 19.47 | -40 % | 33.83 | 11.85 | -65 % |
| `atomic/rng/below` | 7.94 | 7.93 | -0 % | 8.70 | 8.97 | +3 % |
| `atomic/rng/chance` | 7.85 | 7.83 | -0 % | 8.95 | 8.97 | +0 % |
| `atomic/rng/chance_ratio` | 8.21 | 8.16 | -1 % | 8.93 | 8.95 | +0 % |
| `atomic/rng/fraction` | 7.58 | 7.61 | +0 % | 8.43 | 8.43 | +0 % |
| `atomic/rng/next_u64` | 7.47 | 7.55 | +1 % | 8.32 | 8.36 | +0 % |
| `atomic/rng/open` | 42.55 | 21.27 | -50 % | 49.42 | 16.71 | -66 % |
| `atomic/rng/pick` | 7.98 | 7.93 | -1 % | 8.70 | 8.74 | +0 % |
| `atomic/rounding/divide` | 6.10 | 6.02 | -1 % | 7.24 | 6.09 | -16 % |
| `atomic/rounding/shift_right` | 3.72 | 3.71 | -0 % | 2.96 | 2.96 | +0 % |
| `atomic/u256/cmp_products` | 8.28 | 7.26 | -12 % | 7.51 | 4.95 | -34 % |
| `atomic/u256/div_ceiling` | 5.41 | 5.35 | -1 % | 8.52 | 8.52 | -0 % |
| `atomic/u256/div_nearest_even` | 6.24 | 6.22 | -0 % | 10.74 | 10.88 | +1 % |
| `atomic/u256/product` | 2.11 | 2.13 | +1 % | 1.27 | 1.27 | +0 % |
| `atomic/u256/shr_rounded` | 5.46 | 5.38 | -1 % | 3.72 | 3.72 | -0 % |
| `atomic/vec3/distance` | 11.06 | 5.97 | -46 % | 6.22 | 3.33 | -46 % |
| `atomic/vec3/dot` | 3.70 | 3.41 | -8 % | 2.18 | 2.18 | +0 % |
| `atomic/vec3/length` | 13.75 | 4.57 | -67 % | 5.65 | 2.74 | -51 % |
| `atomic/vec3/normalized` | 30.06 | 15.17 | -50 % | 33.88 | 9.77 | -71 % |
| `atomic/vec3/rotated_y` | 4.73 | 4.70 | -1 % | 2.88 | 2.88 | -0 % |
| `atomic/vec3/step_toward` | 31.69 | 18.96 | -40 % | 46.80 | 24.94 | -47 % |
| `atomic/vec3/within` | 2.72 | 2.63 | -3 % | 2.32 | 2.32 | +0 % |

## Kept

| Change | Where it gains |
| --- | --- |
| `Rounding::divide`: magnitudes below 2⁶⁴ divide natively, not by the `u128` library call | `num/div` −22 %, −72 %; every `Num` division |
| Roots: a `u64` path; values below 2¹⁰² correct a float root on 64-bit residuals; larger ones take a residual step in floats, no integer division; `#[inline(always)]` | `floor_root/wide`, `ceil_root/wide` |
| `NearestRoot`: the rounded estimate checked by `n² − n < v ≤ n² + n`, for `Num::sqrt` and `Vec3::length` | `num/sqrt`, `vec3/length`, `distance` |
| `Vec3::normalized`: one `f64` scale and a `u64` correction for each component | `vec3/normalized` −50 %, −71 % |
| `sin_cos`: the quadrant and the table's sign by selects, no `match` | `num/sin_cos` −16 % on the Ryzen |
| `Rng::new`: the message in one `update` | `rng/open` −50 %, −66 % |
| `ProductSum`: one 256-bit two's complement sum, no branch on signs | `product_sum/sum` −40 %, −65 % |
| `U256::cmp_products`: 256-bit products when both values fit 128 bits | `u256/cmp_products` −12 %, −34 % |

## Tried and dropped

| Change | Ryzen | M2 | Why dropped |
| --- | ---: | ---: | --- |
| Root estimates rounded to nearest, corrected with no branch | `vec3/length` +28 % | −11 % | The correction lies on the dependency chain; a truncated estimate is almost always the floor, and the CPU predicts the rare step |
| Root floor as `round(e − ½)` by the magic number | `floor_root/wide` +15 % | +30 % | A whole estimate, as a square's, ties to the even one below: 2 % of the bench's values took a step |
| Narrow root from two exact signed parts and an FMA | −4 % | `floor_root/narrow` +43 % | `ucvtf` converts a `u64` in one instruction on ARM |
| `floor_root` left to the inliner | `floor_root/narrow` +11 % | +22 % | The call stays |
| `atan2`'s `i128` division by a float estimate, a residual step and an exact correction | `num/atan2` +23 % | −15 % | x86's compiler-builtins divides by a 64-bit divisor with `div`; a gain on ARM alone needs a platform split |
| Exact circle overlap in four lanes, 23-bit limbs | 4.5 × slower | 3.8 × slower | AVX2 and NEON have no 64 × 64 → 128 lane multiply |
