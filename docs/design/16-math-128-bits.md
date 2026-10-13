# Campfire — Math in 128 bits

Proposal, for review. Every operation of `math`, and every computation of `geometry`, `navigation`'s collider and `stats` that passes 128 bits today, computes in one native `i128` and rounds once. The exactness this gives up is at most two bits of a `Num`, 2⁻²³ m, which no player sees; the types and the code it removes are the 256- and 384-bit integers, the exact ratio, the sum of wide products, the long division, the fine roots and the float estimates that keep each comparison exact past 128 bits. The sim stays deterministic: every result is still integer arithmetic, the same bits on every machine; the results change once, and the goldens with them.

## Why

Every wide type of today keeps one value exact past 128 bits:

| Value | Width | Today |
| --- | --- | --- |
| A squared distance to a path or a box's edge, against another | up to 2¹⁸⁴ / 2⁹² | `Ratio<U256, u128>`, compared in `U384` |
| A share of a path, a box's slab span | up to 2⁹⁴ / 2⁹⁴, 2⁸² / 2⁸² | `Ratio<i128, i128>`, compared in `I256` |
| A circle's approach to a path | up to 2¹⁸² | `U256` products |
| The collider's push of two circles | up to 2¹⁴⁴ | `U256`, Knuth's division, a rounded-up root |
| The push of a circle out of a box | up to 2²¹⁴ | `U256`, `FineRoot`, a float estimate and its correction |
| A stat, `(add) × (1 + pct) × (1 − cut)` | up to 2¹⁵⁰ | `U256::shr_rounded` |
| A scaled param, a sum of products | past 2¹²⁷ | `ProductSum` |

Engines that run a deterministic sim on fixed point keep one 128-bit step as their widest: Photon Quantum's Q48.16 `FP` and its tables of roots and trig ([Quantum](https://doc.photonengine.com/quantum/v2/manual/quantum-ecs/fixed-point), [FPLut](https://doc-api.photonengine.com/en/quantum/current/class_photon_1_1_deterministic_1_1_f_p_lut.html)), and `mas-bandwidth/fixed`, whose one seam is a 128-bit intermediate and whose trig states its error ([fixed](https://github.com/mas-bandwidth/fixed)). Integer geometry keeps exact only the predicates whose products fit its width, by bounding its coordinates, as Clipper2 does ([Clipper2](https://angusj.com/clipper2/Docs/FAQ.htm)). Floats made deterministic, as Box2D v3's are ([Box2D](https://box2d.org/posts/2024/08/determinism/)), are not taken: the client runs the sim in a process whose other code may set a thread's flush-to-zero, a NaN's bits would reach the state hash, and D1 and D6 of the [Determinism core](09-determinism-core.md) stand.

## Decisions

- **D1. One step: an `i128`, rounded once to a `Num`.** Every operation of `math` computes its result from its inputs in one `i128`, and rounds it once, to nearest, ties to even, or by the `Rounding` a rule names. No type of `math` is wider than 128 bits: `U256`, `I256`, `U384`, `Ratio`, `RatioPart`, `CrossProduct`, `ProductSum`, `U256`'s long division and `WideDivision`'s use by it go. A step that would pass 128 bits rounds an input first, by an amount its doc comment states, as D3 and D6 do.
- **D2. A test between two points stays exact.** Positions lie within ±2⁴⁴ bits, so a squared distance between two points, in three axes, is below 2⁹², and `within`, the grid's reach of a cell and the circles' overlap compare squares in `u128`, exactly and with no root, as now.
- **D3. A distance to a line, a segment or a box's edge is a `Num`.** A line's direction is a unit vector with 62 bits of fraction, taken once for a path, in two axes or three, and once for each edge of a box, in its `Frame`. A point's projection on the line, its dot product with the direction, below 2¹¹⁰, and the offset less the projection along the direction, below 2¹⁰⁹ an axis, each fit an `i128`; the distance is the nearest root of that offset's squared length, below 2⁹². It lies within two bits, 2⁻²³ m, of the true distance, a bound the step's differential test proves. Two distances compare as `Num`s, and a distance against a reach as a `Num` against a `Num`: the `Ratio`s of squared distances go.
- **D4. A place on a path is its distance from the start, a `Num`.** Where a path meets a body, a box's entry or a circle's nearest point, is a `Num` along the path, not a share: a circle's is D3's projection, and a projectile sorts what it meets by it and travels it, with no `of`. A box's slab span becomes the place of each slab's end, `(bound − value) · length ÷ along`, rounded once: the slab values lie below 2⁸¹ and a path's length below 2⁴⁶, so the product is below 2¹²⁷, and a place on the path lies within one bit of the true one.
- **D5. A push rounds away from what it leaves.** The collider's push of two circles is `|along| · reach ÷ length` rounded up, with the offset's length the floor root of its square, so each rounding moves the bodies further apart, and they end touching or apart, as now; a push out of a box takes the edge's line at its rounded distance plus D3's bound, so the next test, whose distance lies within that bound of the true one, sees the body touching or apart. That bound is the one tolerance of collision, defined once beside the distance it covers. `FineRoot`, `first_estimate` and the 256-bit bound of `outward` go.
- **D6. A compound value is a chain of rounded steps.** A stat is `add`, then times `1 + pct`, then times `1 − cut`, each a `Num` product rounded to nearest and stopped at the range of a number; a scaled param a saturating sum of products each rounded to a `Num`. Each step adds at most half a bit of a `Num`, 2⁻²⁵.
- **D7. One root.** `FloorRoot` gives the floor root of a `u128`, from a float's first estimate that integer steps correct, as now; the nearest and the ceiling roots are its methods, from the floor and its rest, and `CeilRoot` merges into it.
- **D8. Platform paths and lanes stay only where they pay.** `WideDivision`'s and `step_moves`' paths for each platform, and the lanes `I64x4`, `U64x4`, `Mask64x4` and `f64x4`, which the grid's roots use, are measured after D1 to D7 land, each against its plain form; one whose case gains less than its noise goes.
- **D9. A test's exact reference may be wider.** A differential test checks each rounded step against an exact reference, as design 09 asks of every operation; a reference that passes 128 bits, a schoolbook product of limbs, lives in the test that needs it, as the import's check of its sRGB table does.

Not in this proposal: `sin_cos` and `atan2`, which already compute in integers within 128 bits; a table of a quarter wave with linear interpolation, as Photon's are, would make them shorter at a stated bound of a few units of the last place, and is a decision of its own.

## Cost

- The goldens move once, as distances, places on paths, pushes and stats round where they were exact; the step says why and by how much, as [Testing and diagnostics](02-engine-core.md#testing-and-diagnostics) asks.
- A projection and a root for each candidate may cost more than a compare of cross products; D3 takes each direction once to bound it. The step measures `atomic/body_box/`, `atomic/shape/`, `atomic/grid/` and `integration/collision/` before and after, and `integration/server_tick/worst_3v3`.
- D2 of [Wide bodies, uses, the shop and gravity](13-bodies-uses-shop-gravity.md) and the `Ratio` of [Determinism core](09-determinism-core.md) are written again as the steps land.
