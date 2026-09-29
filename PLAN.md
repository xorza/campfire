# Plan — next steps

Open items only, in order. Remove an item when it is done. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/09-determinism-core.md](design/09-determinism-core.md).

1. **Stable ids** in `sim`: `StableId`, `IdAllocator`, `EntityIndex`, and the sim's own spawn and despawn.
2. **State hash** in `sim`: `SimComponent` with a stable `NAME`, the postcard-to-BLAKE3 flavor, `StateHasher` with per-type hashes into a reused buffer. Tests: build order does not matter, one changed field changes exactly its type's hash, non-sim components change nothing. Bench: hash of a MOBA-sized world.
3. **Golden test**: one fixed workload over arithmetic, trig, distances, draws and a world hash, against a checked-in digest.
4. **CI**: GitHub Actions on Linux x86_64, Windows x86_64 and macOS aarch64, running the check chain and the golden test. Only this proves cross-platform determinism; this machine builds x86_64 Linux alone.
5. **Benchmark hygiene**: record the reference numbers of every bench, measured pinned on an idle machine, and state a worst-case budget per tick for the sim core. Measurements so far ran beside other heavy processes, with about 20% noise.
6. **Optimization pass**, on those numbers. Known costs: `Vec3::normalized` (about 44 ns: the length root plus one 128-bit division for the reciprocal), `distance` (about 12 ns, the root), `Num::sqrt` (about 13 ns), `Num::div` (about 8 ns, a 128-bit division).
