# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Bots.** The Zero Hour check's two bot matches end only under some seeds: under the seed of the hash before BLAKE3's key derivation mode, the bases' match ended at tick 17622, won by team 1, and the attacker's against the builder at 35306, won by team 0; under the seed after, neither ends within its limit of 20 minutes.

## Ready

- **Sim.** The state registry's per-type hashes (`state_registry/hash_sink.rs`) are BLAKE3 of their bytes with no domain tag, against 05's rule that every hash the sim makes has one.
