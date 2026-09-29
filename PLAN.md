# Plan — next steps

Open items only, in order. Remove an item when it is done. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/09-determinism-core.md](design/09-determinism-core.md).

1. **`Vec3`** in `math`: checked `+ − −v`, `* Num`, `/ Num`, `dot`; exact `length` and `distance` from squared raw differences in `u128`; `normalized`, `direction_to`, `rotated_y(SinCos)`. Reuse `sqrt`'s estimate-and-correct root for the `u128` sum. Tests with hand-computed values (3-4-5, 2⁻²⁴ steps, the D3 bound); bench for `distance` and `normalized`.
2. **`Rng`** in `math`: keyed BLAKE3 extended output with the length-prefixed message; `next_u64`, Lemire `below`, `chance`, `chance_ratio`, `pick`; the debug check that each (stream, entity, tick) opens once. Tests: official BLAKE3 keyed vectors, Lemire and `chance_ratio` exhaustive with 16-bit words. Bench: cost of opening an `Rng` and of one draw.
3. **Stable ids** in `sim`: `StableId`, `IdAllocator`, `EntityIndex`, and the sim's own spawn and despawn.
4. **State hash** in `sim`: `SimComponent` with a stable `NAME`, the postcard-to-BLAKE3 flavor, `StateHasher` with per-type hashes into a reused buffer. Tests: build order does not matter, one changed field changes exactly its type's hash, non-sim components change nothing. Bench: hash of a MOBA-sized world.
5. **Golden test**: one fixed workload over arithmetic, trig, distances, draws and a world hash, against a checked-in digest.
6. **CI**: GitHub Actions on Linux x86_64, Windows x86_64 and macOS aarch64, running the check chain and the golden test. Only this proves cross-platform determinism; this machine builds x86_64 Linux alone.
7. **Benchmark hygiene**: record the reference numbers of every bench, measured pinned on an idle machine, and state a worst-case budget per tick for the sim core. Measurements so far ran beside other heavy processes, with about 20% noise.
