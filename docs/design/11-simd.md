# Campfire — SIMD

Proposal: lanes of 256 bits in `math`, the same on x86-64-v3 and on armv8-a, exact as the scalar code is, for the hot paths whose items are independent and narrow; and no inline assembly.

## Today

Measured on one core of a Ryzen 7 6800U, from the bench profile, with `perf record` and each sample counted by its function or its source line.

- **The 3v3's mean tick is scripts and systems.** In `server_tick/mean_3v3`, the largest functions are vision's `Bitmap::set`, 6.9 %, Rhai's `resolve_fn`, 4.2 %, `vision::see`, 4.2 %, `stats`' refresh, 3.7 %, `hold_passives`, 3.4 %, and `View::read`, 2.3 %; `FloorRoot::floor_root` takes 1.7 %. Rhai's interpreter, the script view's rows and Bevy's systems are branches and pointers, and no lane makes them faster.
- **The fog's rows are the largest path that lanes fit.** In `fog/sight`, a row's arithmetic in `Grid::spans` takes about 60 %: the grid's own lines 28 %, the root 12 %, and `core`'s integer division and conversions 20 %. The bitmap's words take about 25 %. Each row of a sight is independent of the others, its values fit 64 bits, and it divides only by the cell.
- **The other kernels do not fit lanes.**
  - `route_planner/walled`: `RoutePlanner::plan` 44 % and `Walkable::blocks_counting` 32 %, a cell-by-cell walk with an `i128` division for each column, and `Grid::box_distance` 15 %, exact squares above 2⁶⁴.
  - `pathing_grid/one`: `Regions::label` 93 %, a flood fill one cell at a time. A chunk's row is 64 cells, one `u64`, which suits bit operations on whole rows, not lanes.
  - `collision/crowded`: the overlap tests take about 10 %, and the Collide stage takes 3.7 % of the mean tick, so lanes there save at most about 0.3 % of it. `Collider::shift` divides `i128`s.
- **`Num` and `Vec3` need products that no lane has.** Their products are 64 by 64 bits into 128. AVX2 and NEON multiply 32 by 32 bits into 64 at most; four emulated wide products cost as much as four scalar `mul`s.
- **BLAKE3 is already vectorized.** The crate takes AVX2 at run time and NEON on every little-endian aarch64 by its build script; the RNG does not show in the mean tick's profile above 0.8 %.
- **The release build checks overflow, which stops vectorization.** `overflow-checks = true`, so each `+`, `-` and `*` gets a branch, and LLVM keeps the loop scalar: a scratch lane root with checked arithmetic took 2.4 ns a root, the same code with wrapping arithmetic 2.0 ns.
- **One op without a vector form makes a whole kernel scalar.** AVX2 has no packed conversion between `i64` and `f64`. A scratch kernel of four fog rows at once, with `as f64`, compiled to scalar code: 166 ns a sight against 200 ns for the engine's rows. The same kernel with each conversion an exact bit trick compiled to `vsqrtpd`, `vpmuludq` and `vpcmpgtq`: 91 ns a sight, 2.2 times as fast, the same spans for 4,096 sights of 8 to 12 m and 2,000 sights on cell edges.
- **An exact root in lanes is four times as fast.** A scratch root of four `u64`s below 2⁶² at once: 0.51 ns a root, against 3.0 ns for `FloorRoot`'s `u64` path; equal to `u128::isqrt` for 4,096 values of each width and for 633,000 values around squares up to 2⁶².
- **No stable portable SIMD in Rust.** `std::simd` is unstable on the pinned 1.99.0: `portable_simd`, issue 86656.
- **Intrinsics need `unsafe`, which the workspace denies.** An intrinsic is a `#[target_feature]` function: calling it is safe only from a function with the same `#[target_feature]`, not from code that `-C target-cpu=x86-64-v3` builds, and a load from a pointer is `unsafe` always.

## How others take it

- **Highway** (Google, C++): one set of ops that each target does well, and an op that a target lacks is left out, not emulated in secret. Vectors take the target's native width. A widening multiply is `MulEven`, the even lanes, as AVX2's `vpmuludq` does.
- **`std::simd`**: `Simd<T, N>` of any width; LLVM splits a vector wider than the target's into native ones, so `<4 x i64>` is two NEON registers.
- **`wide`** over **`safe_arch`**: fixed types such as `i32x8`, one AVX2 register or two NEON halves; each intrinsic wrapped in a safe function that `cfg(target_feature)` gates, `unsafe` inside.
- **`pulp`** and **`fearless_simd`**: a token proves at run time that a level is present, for one binary over many processors. Campfire fixes the level when it builds, so it needs no dispatch.
- **Rapier**: its `enhanced-determinism` feature does not combine with its float SIMD, which gives different results on different platforms. Campfire's lanes are integers, or floats only where the result is proven exact.
- **Hand-written assembly**: FFmpeg, dav1d and rav1e write their DSP kernels in assembly, with speedups of 3 to 94 times over C, and up to 13 times over C for dav1d's NEON. Those are long kernels with high register pressure, often against C with no SIMD. The Rust reference makes each `asm!` block a black box to the optimizer: no constant hoisted out of it, no value kept in a register across it, no vectorization through it, never in a `const fn`, `unsafe` always. The Rust project advises intrinsics over `asm!` where they exist.

## Inline assembly

Measured on the same core, each result checked against the Rust code:

| Kernel | Rust | `asm!` |
| --- | --- | --- |
| `u128` by `u64`, the quotient within 64 bits, `div` | 1.92 ns, 3.28 ns in a chain | 1.89 ns, 3.30 ns in a chain |
| Exact root of four `u64`s, one `asm!` block for each four | 0.51 ns a root | 1.68 ns a root |
| The same, the whole loop of 4,096 in one `asm!` block | 0.54 ns a root | 0.49 ns a root |

- **Division.** LLVM does not emit `div` for a `u128` by a `u64`, as the quotient may not fit 64 bits. `compiler-builtins` takes `div` itself when the divisor fits 64 bits, so the call costs nothing that `asm!` saves. aarch64 has no 128-by-64 division.
- **Lanes.** A block for each group of lanes is three times as slow: each call reloads its constants and goes through memory. The whole loop in assembly is 10 % faster, at the throughput of `vsqrtpd`, the limit of both. A fog kernel in assembly would have to hold its runs' output too, in two copies, x86-64 and aarch64, each with its own tests, for at most that 10 % of the rows.

So no `asm!` in the engine. A path whose Rust stays far from what its instructions allow, after lanes, is measured again and proposed alone.

## Decisions

- **D1. One width: 256 bits.** Four 64-bit lanes on both baselines: one AVX2 register on x86-64-v3, two NEON registers on armv8-a. One code path, one buffer padding, and no result that depends on the width, as every op is lane by lane.
- **D2. Arrays and lane loops, no intrinsics.** A lane type holds `[T; 4]`; each op is an `#[inline(always)]` loop over the lanes of one operation, which LLVM lowers to the vector instruction: no `unsafe`, no dependency, one body for every target. The toolchain is pinned, so the code changes only with a deliberate bump, and the cases of D8 measure each bump. Intrinsics would guarantee the instructions, at three bodies for each op and `unsafe` in each; `wide` would add a dependency and `unsafe` of its own.
- **D3. Only ops with a vector form on both.** An op is in the set when AVX2 and NEON each have one instruction for it, or a short exact sequence that LLVM vectorizes:

  | Op | AVX2 | NEON |
  | --- | --- | --- |
  | `i64` add, subtract, and, or, xor | `vpaddq` … `vpxor` | `add.2d` … `eor.16b` |
  | `i64` signed compare | `vpcmpgtq` | `cmgt.2d` |
  | `u64` compare | sign flip and `vpcmpgtq` | `cmhi.2d` |
  | `i64` min, max | compare and `vpblendvb` | compare and `bsl` |
  | 32 by 32 into 64 multiply | `vpmuludq`, `vpmuldq` | `umull`, `smull` |
  | Shift by a constant, logical | `vpsllq`, `vpsrlq` | `shl.2d`, `ushr.2d` |
  | `f64` add, subtract, divide, square root, floor | `vaddpd` … `vroundpd` | `fadd.2d` … `frintm.2d` |
  | `i64` and `f64` within 2⁵¹, exactly | bit trick: `vpaddq`, `vsubpd` | bit trick: `add.2d`, `fsub.2d` |
  | A mask's bits | `vmovmskpd` | and, `addp` |

  Left out: a 64 by 64 multiply, integer division, gathers and scatters, and the `as` casts between `i64` and `f64`, none of which AVX2 vectorizes.
- **D4. Lanes wrap, inside a domain each op states.** A checked op would not vectorize. Each op's doc names the domain where it cannot wrap, and a debug build asserts it. A kernel checks its inputs once, in release, before it takes the lanes, and takes its scalar path for inputs outside: both paths are exact, so a result never depends on which one ran, and no lane ever wraps.
- **D5. Exact, as the scalar code is.** Integer lanes give the scalar bits. A float is an estimate that integer steps correct, as `FloorRoot` takes it, or exact by a proof: an integer below 2⁵³ in magnitude, divided by a positive integer below 2⁵³, rounds to a quotient whose floor is the exact floor, as the distance from a quotient that is not whole to the next whole number, at least 1/divisor, passes half its unit in the last place. Rust never fuses a multiply and an add on its own.
- **D6. `FloorRoot` for `U64x4`.** Exact for every `u64`: the value's halves convert to `f64` exactly, their sum rounds once, the root once, and the root rounds to the nearest integer by adding 2⁵², so the estimate is the floor or one above it; the estimate is held to 2³² − 1, and one step down whose square passes the value makes it exact.
- **D7. The public types are integers.** `I64x4`, `U64x4` and `Mask64x4` are public; `F64x4` stays within `math`, behind ops such as `I64x4::div_euclid` by a constant, so no caller takes a float, as the sim's rules require.
- **D8. The first caller: the grid's rows.** `Grid::spans`, under the fog's sights and the pathing grid's bodies, takes four rows a step when the reach and a cell together are below 2³¹ halves of a bit, a radius up to about 64 m, so each square fits 62 bits; past that it takes its rows one by one, as now. The callback gets the same runs in the same order.
- **D9. Each caller is measured on x86-64, and stays only when faster there.** `root/lanes` beside `root/floor`, on the same 4,096 values; `fog/sight`, `pathing_grid/one` and `server_tick/mean_3v3` before and after, on the Ryzen 7 6800U. A lane path that is not faster there is taken out. No case is measured on NEON, so a lane path can be slower there and nothing shows it; CI's arm64 runner proves only that its results are the same.
- **D10. The lane set grows only with a caller.** A new op comes with the kernel that needs it, with its row in D3's table and its measurement.

## Cost

From the scratch kernels: `fog/sight` about 33 % faster, from its rows' 60 % at 2.2 times the speed; `server_tick/mean_3v3` about 3 % faster, the share of `vision::see` and the root. The bitmap's words, the larger part of the fog in the 3v3, stay as they are. The figures on NEON are not known, as D9 does not measure them.

## Tests

- Each op against its scalar meaning, on each lane, at the edges of its domain.
- `FloorRoot` for `U64x4` against `FloorRoot` for `u128`: the edge cases of `FloorRoot`'s own tests that fit 64 bits, k² − 1, k² and k² + 2k for k up to 2³² − 1, `u64::MAX`, and a property over every width.
- `Grid::spans` in lanes against its scalar path: a property over positions and radii, centers on cell edges and corners, reaches at the gate and one past it, and the edges of the bounds; the proving match's goldens and the golden digests unchanged.
- CI runs the tests and the cross-verify on x86-64 and on the arm64 runner of macOS, so a lane result that differs between the two fails there too.
