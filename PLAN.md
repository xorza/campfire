# Plan — next steps

Open items only, in order. Remove an item when it is done. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/09-determinism-core.md](design/09-determinism-core.md).

1. **Stable ids** in `sim`: `StableId`, `IdAllocator`, `EntityIndex`, and the sim's own spawn and despawn.
2. **State hash** in `sim`: `SimComponent` with a stable `NAME`, the postcard-to-BLAKE3 flavor, `StateHasher` with per-type hashes into a reused buffer. Tests: build order does not matter, one changed field changes exactly its type's hash, non-sim components change nothing. Bench: hash of a MOBA-sized world.
3. **Golden test**: one fixed workload over arithmetic, trig, distances, draws and a world hash, against a checked-in digest.
