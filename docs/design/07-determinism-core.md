# Campfire — Determinism Core

Numbers, vectors, randomness, stable ids and the state hash: the base every result depends on.

## Decisions

- **Own `Num`, 40.24.** `Num(i64)` with 24 fraction bits, `*` and `/` rounded to nearest, ties to even, so nothing drifts down and one rounding rule holds everywhere. 40.24 over 32.32: 256 times the range, so script products overflow far later, while the resolution stays far below anything a player sees.
- **`round` is half away from zero,** as script authors expect.
- **Coordinates within ±2²⁰ m.** Exact squared distances then fit a `u128` with room to spare; every call, order and restore that takes a place checks the bound.
- **One step: an `i128`, rounded once.** Every operation computes its result from its inputs in one `i128` and rounds it once, to nearest or by the `Rounding` its rule names, as Java's `BigDecimal.divide` takes a rounding mode; where an exact result would pass 128 bits, it rounds an input first, by at most two bits, which no player sees. Photon Quantum and `mas-bandwidth/fixed` keep one 128-bit step as their widest. Floats made deterministic, as Box2D v3's, are not taken: the client's other code may set a thread's flush-to-zero, and a NaN's bits would reach the state hash.
- **A crate of the sim bans floats at its root.** Each crate a match runs on warns on `clippy::float_arithmetic` at its root, with the reason beside it, and a `rules` test holds the line in each; the lint is in no group the workspace turns on, so a crate-root attribute is the one place it can hold. A hash map's order is banned by `clippy.toml`'s `disallowed-types`, not by a second lint level.
- **Exact where it fits.** A test between two points compares squares exactly; a distance to a line or a box's edge comes from the line's `Direction`, to the nearest bit.
- **A push rounds away from what it leaves,** so two bodies end touching or apart, and the next measure sees them apart: the one tolerance of collision, defined beside the push.
- **A compound value is a chain of rounded steps.** A stat's `add`, `pct` and `cut` each round once; a scaled param sums its products unsaturated and stops only the sum, so products past the range still cancel.
- **Own parser, constants and trig, not `fixed`.** `fixed` rounds `*` toward −∞ and its constants down; `fixed_analytics` reaches 48 ulp in `atan2`; own series at high internal precision stay within 0.501 ulp over the tests' sweeps, the one bound claimed.
- **Trig in degrees is exact at right angles.** `SinCos::of_degrees` reduces its angle exactly to a quarter turn, so each multiple of 90° is exact and an angle's sine is its complement's cosine.
- **Roots corrected by integers.** A float gives only a root's first estimate, which integer steps correct, so the result never depends on it.
- **Float lanes compute exact results.** A lane of `f64`s gives only a result that round-to-nearest gives exactly, as the floor of a quotient below 2⁵¹, or an estimate that integer steps correct, as a root's. Rust holds the default rounding, whose change is undefined behaviour, as the documentation of `_mm_setcsr` says, and no lane's value is subnormal, so a thread's flush-to-zero changes none. Refused: a float's own value in the state, as above.
- **One div instruction for a wide division on x86-64.** `WideDivision` divides 128 bits by 64 with one inline `div` in `asm!`, guarded so it cannot fault, the one `unsafe` that no outside API asks for, as Rust's `u128` division reaches the instruction only through `__udivti3`: `direction/of` 44 % and `rounding/divide` 18 % less on the Ryzen ([Math performance](../research/math-performance.md)). Refused: the library call, and a float estimate corrected by integers, which the M2 keeps and which costs the Ryzen up to 63 %.
- **No float converts.** `From` only where every value converts exactly, `TryFrom` where one can fail, as the Rust API guidelines' C-CONV says; an identity has no `From` of its integer, which has no meaning of its own.
- **A counter-based cryptographic RNG.** One BLAKE3-keyed sequence per stream, entity and tick, keyed by the segment seed, so no draw depends on another's order and systems can draw in parallel; each owner names its own streams.
- **Stable ids never reused.** Ids are handed out in order; an entity a match holds once takes a named id far past the allocator's, so its spawn moves no unit's id.
- **One hash per state type.** BLAKE3 per type, then one over the list, so the first divergence names its type.
- **A snapshot is what the hash reads.** A snapshot holds exactly the bytes the hash reads, so they cannot disagree; a restore reads untrusted bytes, returns an error for every flaw and never panics, and checks each value against the books and its entity's kind, which are required items of the state traits, so the compiler proves every state type has both.
- **Wider exact references stay in tests.** A differential test checks each rounded step against an exact reference; one past 128 bits lives in its test, never in the API.
- **Goldens on every platform.** Fixed workloads hash to one digest per section, and CI requires the same digests on every platform.
