# Lightyear in the tree — research

What the fork of Lightyear that Campfire takes into its tree holds, and what Campfire uses of the fork it builds on today, for [Lightyear in the tree](../design/12-lightyear-in-tree.md). Facts only. Read on 19 October 2026: the fork's branch `canonical-state-and-bounded-passes` at `16797a83`, which its `main` did not yet hold (`main` stood at `3e81ef79`, two commits past `f4b95d9d`); Campfire at `5d6cab490` with its uncommitted tree, on Lightyear `54e6be21`.

## The fork

- **Its distance from Campfire's pin.** `54e6be21` is an ancestor of the branch, 84 commits back; the diff between them changes 1,370 files, 90,193 lines added and 143,841 removed.
- **Its crates.** 22 crates and an `xtask`, about 86,000 lines of Rust: `prediction` 15,206, `replication` 13,873, `interpolation` 9,342, `channels` 9,328, `messages` 5,105, `session` 5,100, `udp` 4,598, `port_map` 3,610, `lightyear` 2,959, `sync` 2,574, `connection` 2,476, `core` 2,243, `sim` 1,861, `stepper` 1,314, `serde` 1,260, `transport` 1,111, `endpoint` 957, `common` 871, `ecs` 688, `timeline` 657, `payload` 580, `link` 461.
- **Its Bevy.** Bevy 0.20 and `bevy_replicon` 0.45.2 from crates.io, with no patch. The fork moved from Bevy 0.19 in `9e7b4394`, after its API had changed in `9092f4d2` and `51ce211b`; no commit holds Campfire's API on Bevy 0.20.
- **Its tests.** 917 tests under nextest, all passed in 1.0 s of wall time on the i9-13980HX, none slow, on Campfire's toolchain `beta-2026-10-14`.
- **Its lints under Campfire's.** With Campfire's workspace lints and `clippy.toml` in place of its own: 251 `needless_pass_by_value` and 36 `type_complexity`, the two lints the fork allows and Campfire allows at the root of each Bevy crate instead (17 crates and 12); 4 others; 2 calls of `postcard` in `serde`; 1 thread started in `common/src/worker.rs`; and the `xtask`'s file reads.
- **Its overlaps with Campfire.** `port_map` holds Campfire's `quic/src/port_map` files and adds `listening`, `shared_port` and `stop`. `udp` holds the decisions of Campfire's `quic` without QUIC: the raw Ed25519 key, one secret for the key and the reset key by HKDF, the `tls-exporter` binding, a Retry for each new address, an admission for each address or IPv6 /64 by a cell rate, `quinn-udp`, a thread for each endpoint. `common` holds a `Worker`, an `ErrorReport` and a `SplitMix64` of the same algorithm and shape as `campfire-store`'s, `campfire-common`'s and `campfire-math`'s. Its `TickRate` is a whole `NonZeroU16` of hertz, its period rounded to the nearest nanosecond; `campfire-sim`'s is a `NonZeroU32`, its length rounded down.
- **Its numbers.** `fixed` in `core` alone, in 8 files: `U32F32` for an unsigned tick span, `I32F32` for a signed one, `U0F16` for the overstep a packet carries.
- **Its documents.** `docs/design/` holds 10 documents and `docs/research/` 4; `testing.md` and `time.md` name Campfire's harness and its `DelayLine`. Its citations take the form `` (`file.md`: "Title") ``.
- **Its own lint files.** `replication`, `interpolation` and `prediction` each hold a `clippy.toml` that adds Bevy's despawns to the workspace's bans; the `xtask` writes and checks them.
- **Its benches.** One bench target, in the facade, whose ids are `<kind>/<crate>/<group>/<rest>`.

## Campfire's use of Lightyear

- **The crates that name it.** `net` (21 files of 12,778 lines), `quic` (5 of 8,056), `harness` (12 of 19,100), `client` (3), `server` (2).
- **What is gone from the fork.** Of the items Campfire names: `LinkReceiveSystems`, `LinkMtu`, `RecvPayload`, `CrossbeamIo`, `ChannelSettings`, `ReliableSettings`, `NetworkDirection`, `PredictionBuilderExt`, `Replicated`, `TickDuration`, `SetTickDuration`, `is_in_rollback`, `ConfirmHistory`, `PingManager` and `fragment_size_for_min_mtu`. Every other item it names exists, most in the crate of its layer.
- **Its Replicon patch.** One commit on 0.44.2, `44fa6612`, "Fix mutations lost when components lose visibility"; Replicon 0.45 holds the fix as `733b910`, "Don't drop mutations from the tick a component loses visibility".
- **Its base crates.** `common`, `codec`, `store` and `math` depend on no Bevy crate; `schedule` depends on `bevy_ecs`.
