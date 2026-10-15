# Math performance

The atomic benches of `campfire-math` and `campfire-geometry`, before and after each change of the branch `perf/math`. Every change keeps each result bit for bit: the tests cross-check each new path against the exact one it replaces, and the scripted matches play to their goldens.

## Machines and method

- **Ryzen:** AMD Ryzen 7 6800U, Linux, x86-64-v3, boost on, the bench pinned to one core.
- **M2:** Apple M2, MacBook Air, macOS, passively cooled.
- **A run:** two worktrees, `master` at `7a29072b` and the branch, each built by `./drive.py bench --bench math --bench geometry --no-run`, so each keeps its own artifacts. The two bench binaries run in turn, three times each, 2 s of measurement a case, and each figure is the median of the three, in ns per element: each case runs 4,096 elements an iteration.
- **Noise:** two runs of one binary differ by up to 2 % on the Ryzen and up to 10 % on the M2, which heats under a long run. A case whose code did not change can move by up to 10 %, as the linker places its code at another offset ([Benches](../design/02-engine-core.md#benches)): `atomic/root/floor_narrow` runs the same instructions an element on both builds, at 9 % more cycles.

## Results

Before is `master` at `7a29072b`; after is the branch at `61c8310e`. Each figure is in ns an element.

| Case | Ryzen before | Ryzen after | Δ | M2 before | M2 after | Δ |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `atomic/body_box/overlap` | 7.73 | 6.07 | -22 % | 5.59 | 4.32 | -23 % |
| `atomic/body_box/reach` | 9.56 | 4.01 | -58 % | 16.90 | 2.83 | -83 % |
| `atomic/body_box/reach_box` | 814 | 16.80 | -98 % | 1,224 | 12.49 | -99 % |
| `atomic/direction/along` | 7.62 | 1.76 | -77 % | 5.38 | 1.36 | -75 % |
| `atomic/direction/along_flat` | 5.75 | 1.34 | -77 % | 4.98 | 1.02 | -80 % |
| `atomic/direction/apart` | 24.67 | 8.59 | -65 % | 14.00 | 6.27 | -55 % |
| `atomic/direction/apart_flat` | 5.70 | 1.70 | -70 % | 3.16 | 1.32 | -58 % |
| `atomic/direction/of` | 52.22 | 34.51 | -34 % | 87.14 | 27.03 | -69 % |
| `atomic/direction/of_flat` | 41.74 | 29.99 | -28 % | 70.18 | 22.07 | -69 % |
| `atomic/direction/scaled_away` | 8.88 | 6.41 | -28 % | 7.33 | 5.55 | -24 % |
| `atomic/direction/scaled_away_flat` | 6.00 | 4.27 | -29 % | 5.84 | 5.05 | -14 % |
| `atomic/grid/box_spans_closer` | 1,550 | 949 | -39 % | 1,456 | 878 | -40 % |
| `atomic/grid/spans_closer` | 69.59 | 70.49 | +1 % | 73.77 | 74.43 | +1 % |
| `atomic/grid/touches` | 107 | 81.70 | -23 % | 111 | 83.25 | -25 % |
| `atomic/num/atan2` | 14.00 | 13.70 | -2 % | 18.52 | 20.41 | +10 % |
| `atomic/num/atan2_near_axis` | 13.83 | 13.38 | -3 % | 11.96 | 12.26 | +3 % |
| `atomic/num/div` | 2.82 | 2.80 | -1 % | 2.61 | 2.66 | +2 % |
| `atomic/num/mul` | 1.61 | 1.61 | +0 % | 0.897 | 0.894 | -0 % |
| `atomic/num/mul_div` | 4.30 | 4.28 | -0 % | 5.86 | 5.52 | -6 % |
| `atomic/num/saturating_mul` | 2.28 | 1.89 | -17 % | 1.53 | 1.39 | -9 % |
| `atomic/num/sin_cos` | 12.14 | 11.47 | -5 % | 9.01 | 9.11 | +1 % |
| `atomic/num/sin_cos_huge` | 12.13 | 11.46 | -6 % | 9.00 | 9.40 | +4 % |
| `atomic/num/sqrt` | 2.24 | 1.96 | -13 % | 2.22 | 1.89 | -15 % |
| `atomic/rng/below` | 8.03 | 8.05 | +0 % | 8.76 | 8.99 | +3 % |
| `atomic/rng/chance` | 7.96 | 7.88 | -1 % | 8.96 | 8.98 | +0 % |
| `atomic/rng/chance_ratio` | 8.35 | 8.29 | -1 % | 8.95 | 8.95 | -0 % |
| `atomic/rng/fraction` | 7.58 | 7.59 | +0 % | 8.44 | 8.44 | +0 % |
| `atomic/rng/next_u64` | 7.62 | 7.56 | -1 % | 8.34 | 8.50 | +2 % |
| `atomic/rng/open` | 20.87 | 20.98 | +1 % | 17.71 | 17.77 | +0 % |
| `atomic/rng/pick` | 8.02 | 8.04 | +0 % | 8.72 | 8.97 | +3 % |
| `atomic/root/ceil_wide` | 5.39 | 5.51 | +2 % | 3.55 | 3.58 | +1 % |
| `atomic/root/floor_lanes` | 0.671 | 0.700 | +4 % | 0.699 | 0.699 | +0 % |
| `atomic/root/floor_narrow` | 2.43 | 2.43 | +0 % | 1.45 | 1.45 | +0 % |
| `atomic/root/floor_wide` | 4.51 | 4.34 | -4 % | 2.61 | 2.62 | +0 % |
| `atomic/root/nearest_cmp_wide` | 1.91 | 1.47 | -23 % | 2.26 | 1.11 | -51 % |
| `atomic/root/nearest_wide` | 4.04 | 3.64 | -10 % | 2.78 | 2.25 | -19 % |
| `atomic/rounding/divide` | 6.53 | 6.27 | -4 % | 5.52 | 5.20 | -6 % |
| `atomic/rounding/shift_right` | 3.72 | 3.74 | +0 % | 2.97 | 2.38 | -20 % |
| `atomic/placed/comes_closer_box` | 8.74 | 4.42 | -49 % | 11.62 | 2.90 | -75 % |
| `atomic/placed/comes_closer_circle` | 4.54 | 2.25 | -50 % | 6.66 | 1.86 | -72 % |
| `atomic/vec3/distance` | 6.05 | 5.73 | -5 % | 3.33 | 2.83 | -15 % |
| `atomic/vec3/dot` | 2.62 | 2.62 | -0 % | 2.05 | 2.05 | -0 % |
| `atomic/vec3/length` | 4.55 | 4.47 | -2 % | 2.75 | 2.26 | -18 % |
| `atomic/vec3/normalized` | 15.14 | 12.78 | -16 % | 9.77 | 6.30 | -36 % |
| `atomic/vec3/rotated_y` | 3.88 | 3.74 | -3 % | 2.43 | 2.43 | +0 % |
| `atomic/vec3/step_toward` | 17.03 | 16.69 | -2 % | 16.60 | 12.42 | -25 % |
| `atomic/vec3/within` | 2.63 | 2.64 | +0 % | 2.33 | 2.33 | +0 % |

`atomic/body_box/reach_box` and `atomic/grid/touches` are new on the branch; their before is the same case built on `master`. On the M2, `num/atan2` runs 10 % slower with strictly fewer instructions: its machine code differs from `master`'s only by two overflow checks that change 13 removed. The `rng` cases, whose code the branch did not change, move by up to 3 %. Both move with where the code lies.

### Integration

The systems' cases that take these operations, in ms an iteration, the M2's for navigation only. Math and geometry take about 3 % of the 3v3's server tick on the Ryzen, by `perf`'s samples of their own functions, so the whole tick moves little.

| Case | Ryzen before | Ryzen after | Δ | M2 before | M2 after | Δ |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `integration/battle/crowded_act` | 11.97 | 11.98 | +0 % |  |  |  |
| `integration/battle/crowded_collide` | 9.59 | 9.77 | +2 % |  |  |  |
| `integration/battle/crowded_hit` | 7.37 | 7.48 | +1 % |  |  |  |
| `integration/battle/crowded_inputs` | 5.13 | 5.05 | -2 % |  |  |  |
| `integration/battle/crowded_mode` | 0.848 | 0.842 | -1 % |  |  |  |
| `integration/battle/crowded_move` | 8.84 | 8.85 | +0 % |  |  |  |
| `integration/battle/crowded_resolve` | 45.38 | 45.10 | -1 % |  |  |  |
| `integration/battle/crowded_think` | 123 | 122 | -1 % |  |  |  |
| `integration/battle/crowded_vision` | 11.67 | 11.49 | -2 % |  |  |  |
| `integration/collision/boxes` | 1.18 | 0.855 | -28 % | 1.49 | 0.864 | -42 % |
| `integration/collision/bridge` | 1.24 | 0.892 | -28 % | 1.56 | 0.901 | -42 % |
| `integration/collision/crowded` | 0.420 | 0.411 | -2 % | 0.490 | 0.450 | -8 % |
| `integration/collision/spread` | 0.178 | 0.174 | -2 % | 0.196 | 0.193 | -1 % |
| `integration/fog/sight` | 0.169 | 0.168 | -1 % |  |  |  |
| `integration/fog/still` | 0.071 | 0.070 | -1 % |  |  |  |
| `integration/pathing_grid/enter` | 0.019 | 0.019 | +2 % | 0.015 | 0.015 | +0 % |
| `integration/route_planner/group` | 2.81 | 2.67 | -5 % | 2.85 | 2.51 | -12 % |
| `integration/route_planner/group_alone` | 25.35 | 24.63 | -3 % | 25.40 | 23.27 | -8 % |
| `integration/route_planner/walled` | 28.63 | 26.69 | -7 % | 29.90 | 26.22 | -12 % |
| `integration/server_tick/mean_3v3` | 731 | 725 | -1 % |  |  |  |
| `integration/server_tick/worst_3v3` | 0.530 | 0.514 | -3 % |  |  |  |

## Changes

Each change, what it measured against the change before it, Ryzen first, then M2, unless it names another base.

1. **`Direction::of` divides by one float reciprocal.** The three axes of a unit share the divisor, the root, which lies from 2⁶² to 2⁶³. One float reciprocal gives each quotient an estimate within 2¹², and two exact integer steps correct it, where the Ryzen called `__udivti3` in `libgcc_s` for each axis and the M2 took a float estimate with its own reciprocal for each. The square takes one widening product an axis, the length rounds on `u64`, and the wide root's estimate is inline. `of` −20 %, −62 %; `body_box/reach` +3 %, −38 %; `placed/comes_closer_box` +2 %, −28 %.
2. **`Direction`'s measures take one widening product for each axis.** An axis within ±2⁴⁷ times a unit's axis within ±2⁶² is one `i64` product to `i128`, with no overflow check; the rounding to the nearest unit adds `2⁶¹ − 1` and the floor's last bit before the shift. `along` −76 %, −75 %; `apart` −66 %, −52 %; `scaled_away` −28 %, −24 %.
3. **`Rounding::shift_right` compares the rest with one threshold.** Each mode's rounding is the rest past a threshold, half less the floor's last bit, 0 or the mask, in place of the compares of the rest and the half. `shift_right` +1 %, −20 %; `num/saturating_mul` −17 %, +9 %.
4. **`Num::saturating_mul` rounds as `from_raw_products` does**, by the added half less one and the floor's last bit, in one helper both take. `saturating_mul` −17 %, −4 %, against the +9 % that change 3 cost it on the M2.
5. **A box's edge tests share three products.** With `a` and `b` the half edges, each edge's `along` and each corner's squared distance follow from a point's `off · a`, `off · b` and `off · off` and the box's `a · a`, `a · b` and `b · b`, in place of a subtraction and a dot product for each edge. `BodyBox::nearest` and `comes_closer` make a half edge's direction only for an edge whose inside is nearest, where they made both for each call. Against `master`, the Ryzen: `body_box/reach` −31 %, `placed/comes_closer_box` −25 %, `grid/box_spans_closer` −7 %; the M2: −58 %, −31 %, −11 %.
6. **A path's approach makes its direction only between its ends.** `Approach::of` finds first where the path comes nearest, at its start, its end or between, and makes the path's `Direction` only for the end's place and between; `Approach::nearest_of`, the order alone, which `Placed::comes_closer` takes, makes it only between. `placed/comes_closer_circle` 4.51 to 3.38 ns on the Ryzen, −25 %.
7. **`Direction::of` takes the reciprocal its root's estimate made.** The wide root's estimate divides once already, and its reciprocal lies within 2⁻⁵⁰·⁶ of `1 / root`: the units' estimates take it, with no division of their own, and start before the root's integer steps end. `of` 41 to 34 ns, 33 to 28 ns; `of_flat` 33 to 30 ns, 26 to 24 ns.
8. **`sin_cos` steps with no overflow checks, and rounds its result by one threshold.** Every value of its series is at most 1 at 2⁻⁶², so no sum of two wraps; `to_num_narrow` compares its 38-bit rest on `i64` with half less the floor's last bit. `sin_cos` −5 % on the Ryzen; within the M2's noise.
9. **A grid row's box tests share the row's terms, and the box's tests wrap where their bounds allow.** A point's slab values, products with the half edges and square are linear in its `x` along a row, so a row makes the parts of its `z` once, and each cell adds one product for each; the grid steps a cell's offset in `i64`. Each box value within the world's bound stays below 2¹⁰⁰, so the slab and path tests add, subtract and take magnitudes with no overflow check, and test `|v| ≤ bound` as `v + bound`, unsigned, against twice the bound. Against `master`, the Ryzen: `grid/box_spans_closer` −23 %, `body_box/overlap` −22 %; the M2: −28 %, −23 %.
10. **The nearest root steps with no overflow checks.** `nearest_root_cmp` squares and adds a reach of at most 2⁶³ with wrapping steps, and the nearest root's correction steps move a root by one within its estimate's error, so they wrap too. `root/nearest_cmp_wide` −23 %, −50 %; `root/nearest_wide` −10 %, −17 %; `num/sqrt` −12 %, −15 %; `vec3/length` −2 %, −18 %.
11. **A unit vector's components correct with no overflow checks.** `unit_component`'s quotient is at most 2²⁴ + 1 and its rest within ±2⁶³, so its correction and its sign wrap. `vec3/normalized` −16 %, −35 % (with change 10's root).
12. **A step's moves correct with no overflow checks.** The M2's move estimate corrects and signs a move below 2⁵⁰ with wrapping steps, and each axis advances by a wrapping sum, as a move ends between the start and the target. `vec3/step_toward` −2 % on the Ryzen, −25 % on the M2.
13. **The M2's wide division corrects with no overflow checks.** Its step is below 2¹⁶ and its rest below 2¹⁵ divisors, so the `i128` correction wraps. On the M2: `rounding/divide` −5 %, `num/mul_div` −5 %; `num/atan2` within its noise.
14. **A box's and a shape's early-outs take no overflow checks.** `BodyBox::extent` sums two magnitudes within `MAX_BOUND`, `BodyBox::nearest` tests an axis by an unsigned magnitude, and `Placed::comes_closer` subtracts positions within the world's bound, all with wrapping steps: most calls end there; a box's corners and edge terms are made with wrapping steps too. Against `master`: `body_box/reach` −47 %, −74 %; `placed/comes_closer_box` −39 %, −65 %; `placed/comes_closer_circle` −37 %, −61 %.
15. **A box's distance from an edge's line is first bounded with no direction.** The measure by the half edge's `Direction` lies within 0.51 of the true distance `|C| / |h|`, and `C`, the point's cross product with the half edge from the edge's start, is its slab value and a quarter of the area, exact. So `|C| ≥ (reach + 2)·⌈|h|⌉` is past the reach and `|C| < (reach − 1)·⌊|h|⌋` short of it, from one floor root of each half edge; only a band two units wide takes the direction, which a point's or a path's test then makes. Against `master`: `body_box/reach` −58 %, −83 %; `placed/comes_closer_box` −45 %, −71 %; `grid/box_spans_closer` −25 %, −26 %.
16. **Two boxes' reach orders each corner as a point's reach does, after an early-out.** A gap along an axis past both extents and the reach puts every distance between the boxes at least a unit past it; else the least of the eight corners' orders against the reach, each by `Edges::against` with the lines' bounds, gives the order of the nearest corner's distance, where each corner took `Frame::nearest_to` and both of a box's directions. The new case `atomic/body_box/reach_box`, measured on `master` by the same case added there: 799 to 17 ns, 1,200 to 12.5 ns.
17. **A push out of a box asks the reach first, and turns the half edge's direction for the edge's normal.** `push_out` returns at once for a point outside the box at least the radius away, as `Edges::against` with the lines' bounds decides it; `Frame::nearest_to` makes a half edge's direction only for an edge whose inside is nearest; and the push through an edge takes the half edge's direction turned a quarter, negated for edges 2 and 3, where it made the normal's own: the normal is twice the half edge turned, and `Direction`'s new `turned` and negation give exactly what `of` gives for the turned and negated vector, as its tests check. The integration case `integration/collision/boxes`, on the Ryzen: 1.173 to 0.846 ms, −28 %; `integration/collision/bridge` −28 %.
18. **A path's test of a box's corners bounds each corner's distance from the path's line first.** As in change 15, from one floor root of the path's square; the path's direction is made only for a corner in the band. `placed/comes_closer_box` 4.69 to 4.45 ns on the Ryzen.
19. **A segment's walk over the grid divides through `Rounding` and steps with no overflow checks.** Its floors took `i128::div_euclid`, a library call; `Rounding::Floor.divide` gives the same floor by one native division for magnitudes within 64 bits. Its products and its quotients' sums, below 2¹⁰⁰ within the world's bound, wrap. The new case `atomic/grid/touches`, measured on `master` by the same case added there: 106 to 81 ns, −24 %; 109 to 84 ns, −23 %.
20. **A grid cell past a box's line by its slab value is out with no edge's test.** A slab value past the quarter area and a line's bound puts the cell's centre at least two units past the reach from the edges along the other half edge, so from the box: most cells next to a run lie so. The cell's products with the half edges and its square are made only after. `grid/box_spans_closer` 1,145 to 938 ns on the Ryzen, −39 % against `master`.
21. **A grid box's distance takes its centres in `i64`.** Centres and positions lie within 2⁴⁸ halves of a bit, so `Grid::box_distance`, which the route planner takes for each cell, makes its offsets in wrapping `i64` and only its squares in `i128`. `integration/route_planner/walled` −2 %, from +1 % before it.
22. **A path's order between its ends is bounded before its direction measures it.** The 3D measure lies within 1.4 of the true distance `|C| / |path|` (design 09, D9), and `|C|` lies from the largest axis of the exact cross product `C` to the sum of its axes, one and the same on a planar map: the largest axis at least `(reach + 3)·⌈|path|⌉` is past the reach, the sum below `(reach − 2)·⌊|path|⌋` short of it, and only the band between takes the path's `Direction`. `placed/comes_closer_circle` 2.81 to 2.27 ns, −50 % against `master`; `integration/route_planner/walled` −8 %, `group` −5 %.
23. **x86-64 divides 128 bits by 64 with one inline `div`.** Rust's `u128` division reaches the instruction only through `__udivti3` in `libgcc_s`; `WideDivision` now takes it inline, in `unsafe` assembly guarded so it cannot fault, for a divisor and a quotient within 64 bits. `Direction::of` on x86-64 then divides each axis by the root with it, in place of the float reciprocal, which the M2 keeps. Against `master`, on the Ryzen: `direction/of` −44 %, `direction/of_flat` −50 %, `num/mul_div` −17 %, `rounding/divide` −18 %, `num/atan2` −3 %; `num/div`, whose 64-bit path does not change, +8 % with the same instructions an element. The M2 does not change.

## Tried and dropped

- **`Direction::of`'s reciprocal from the float root of the square**, so it waits on no integer root: +9 % and +13 % on the Ryzen, as a second square root and division share the one divider with the root's own.
- **The M2's float estimate of a wide quotient on the Ryzen**, in place of `__udivti3` and its one `div`: `num/atan2` +63 %, `num/mul_div` +50 %, `rounding/divide` +19 %.
- **The floor root's correction steps with no overflow checks**: `root/floor_narrow` +17 %, `root/ceil_wide` +11 % on the Ryzen, as the loops compiled to more instructions an element.
- **The M2's float move estimate on the Ryzen**, with its overflow checks gone: `vec3/step_toward` +4 %, as the Ryzen's three `div`s stay faster.
- **`Grid::reveal_run`'s cell indices with no overflow checks**: `grid/spans_closer` +32 % on the Ryzen.
- **The grid's box test's directions made on demand in a `OnceCell`**: `grid/box_spans_closer` +26 %, as the cell's interior mutability made each test load the whole test again.
- **`Grid::index` by a stored float reciprocal and one exact step**, in place of an `i64` division: `integration/fog/still` +3 %, `integration/fog/sight` +2 % on the Ryzen.
- **`atan2`'s rotation and series with no overflow checks**: `num/atan2` +1 % and +2 % on the Ryzen.
- **`Vec3::checked_rotated_y`'s sums with no overflow checks**, for a turn's sine and cosine of at most 1: `vec3/rotated_y` −1 % on the Ryzen, within what placement moves.
- **`Approach`'s dot product and end square with no overflow checks**: `placed/comes_closer_circle` −1 %, as most calls end before them.
- **The grid's disc rows' offset with no overflow checks**: `grid/spans_closer` −1 %, within what placement moves.
- **The inline `div` for `Rounding::divide`'s magnitudes within 64 bits**, in place of the compiler's 64-bit division and its branch to a 32-bit one: `vec3/step_toward` +19 %, `num/div` +11 %, `rounding/divide` +6 % on the Ryzen, as a 128-by-64 `div` of small values takes longer than the narrower ones.
