# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## 1. One shape for hashes, ids and keys

`common`: `bytes32`, new `value32`, `content_hash`, `domain_hasher`, `fingerprint`, `state_hash`, `tls_key`, `channel_binding`; `protocol`: its ids, hashes and seeds; `package`; `package-disk`; `sim`: `state_registry`; `math`: `rng`; `import`: `zero_hour` versions; `checks`.

### V2. Every hash through its hasher, by its name

(Every SHA-256 and BLAKE3 site; design 05.) The package's fingerprint and the import's archive pins call `sha2`, and nine sites `blake3`, each BLAKE3 one updating its domain first by hand; five values are bare arrays: the import's archive pins, a journal frame's hash, a seed chain's link, `HashSink::finish` and `TypeHash::hash`. Each site hashes through `ContentHash` or `DomainHasher`, and each value takes its type. The names follow design 05's table: `Fingerprint` becomes `PackageFingerprint`, and each `HashDomain` is a constant. The goldens and the Zero Hour fingerprints do not move.

## 2. Stable again

`source/rust-toolchain.toml`.

### T1. The pin back to stable 1.100

(`source/rust-toolchain.toml`; issue log, Driver.) The toolchain is the beta of 1.100 of 2026-10-15, for cargo's new build-dir layout, which stable has from 1.100, released 2026-11-12. On that release the pin becomes `1.100.0`, and its comment goes.

