# Workspace audit C: code Campfire and the stack both hold

Read-only audit of `/home/xxorza/Projects/campfire` at `68ca71cbc` (2026-10-19). Paths are relative to `source/crates/` unless they start with `docs/` or `source/`. Nothing was built or run; each "not checked" names what only a build or a run would show.

**Batch 1 state.** Batch 1 has landed for threads and error reports. Every stack thread starts through `campfire_store::Worker` (`net-endpoint/src/endpoint/mod.rs:230`, `net-packets/src/internals/plain_host.rs:64`, `net-port-map/src/port_map/mod.rs:65`). Every stack error log goes through `campfire_common::ErrorReport`. The port map uses `campfire_store::SharedPort`, and the scenario tests use `campfire_store::internals::Parallel`. `net-common` holds no `Worker` and no `ErrorReport`. None of these is listed again below.

## Summary

| ID | Concept | Implementations | Winner | Home | Already planned | Size |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | Tick rate | `sim::TickRate` (u32 Hz, floor ns), `net-core::TickRate` (u16 Hz, nearest ns, rates changed by `SetTickRate`), `net::PaceSpeed::tick` | One rational type, as planned, but **two resources**: the match's rate and the timeline's rate | type in `common`; each resource in its own crate | batch "The stack on Campfire's base crates", step 2 (type only); the resource split is **new** | M |
| C2 | Tick spans and overstep | `fixed::U32F32/I32F32/U0F16` in `net-core`, `Num` in `math` | `Num` | `net-core` on `math` | batch "The stack on Campfire's base crates", step 1 | M |
| C3 | Tick counters | `common::Tick` (u64, limit 2⁶²), `net-core::NetTick` (u32, saturating), bridged by `net::MatchClock` | Keep both; `MatchClock` stays the one bridge | as is | decided (design 12, "A word keeps Campfire's meaning") | none |
| C4 | Seeded generator | `math::SplitMix64` (internals), `net-common::SplitMix64` (+ `next_f32`, biased `below`) | `math`'s, with `Rng`'s unbiased `below` and `Num` draws | `math` | batch "The stack on Campfire's base crates", step 3 (move); the draw semantics are **new** | S |
| C5 | OS randomness | `protocol::Entropy` (getrandom 0.3), `net-common::Random::Os` (ring), `net-udp::os_random::secret` (ring, bypasses `Random`) | `Random` within the stack; `Entropy` stays for keys | `net-common`, `protocol` | **new** | S |
| C6 | Channel binding | `common::ChannelBinding` (`Value32`), `net-packets::ChannelBinding` (`pub [u8; 32]`), `Option<ChannelBinding>` for plain sessions | `common`'s | `common` | batch "The stack on Campfire's base crates", step 3; the local link's binding is **new** | S |
| C7 | Server's raw TLS key | `common::TlsKey`, `net-udp::ServerKey`, `quic::spki`/`PinnedKeyVerifier` copies | `TlsKey` | `common` | batch "Campfire on the stack", step 3 ("if they are one key"): they are one key | S |
| C8 | Server secret and derivations | `quic::DerivedSecret`, `net-udp::DerivedSecret`, `ServerSecret` in `Zeroizing`, `common::Secret<N>` | `net-udp`'s derivation, bytes in `common::Secret<32>`, labels in Campfire's tag form | `net-udp` | **new** (labels change the published `TlsKey`) | S |
| C9 | Log capture in tests | two `ErrorCapture`s in `net-stack`, `campfire_log::LogCheck` and `capture` | `campfire-log`, with a scoped capture | `log` | **new** | M |
| C10 | Allocation audit | `common::alloc_audit` (process window), `net-stack/tests/alloc` (per-thread counter + own `Audit`), harness's bench counter | `common`'s | `common` (internals) | issue log (Ready), **not in PLAN**; a batch "Campfire on the stack" risk is **new** | M |
| C11 | Bench policy | `net-common::bench::{Kind, Scale, count, without_logs}`, string group names and hand `sample_size` in ~35 Campfire groups | `Kind` + `Scale` | Decide (below) | issue log (Ready); design 12 keeps it the stack's | S–M |
| C12 | Hashers and maps | `net-core::StableHasher`/`RegistryHasher` (untagged BLAKE3, 64 bits), `common::DomainHasher`; `net-core::HashMap` alias and 10 direct `bevy_platform` maps | `DomainHasher` under a tag | `common` | **new** | S |
| C13 | Clocks | `net-core::Instant` + `net-timeline::Clock`, `Time<Real>::elapsed()` in `net`'s session timers, `net::os::Clock` (Unix seconds) | the stack's `Clock` for the session's monotonic timers; `os::Clock` stays | `net-timeline`, `net` | **new**, low priority | S |
| C14 | Peer and player ids | `PeerId`/`RemoteId`/`LocalId`/`NetId`, `Seat`, `Owner` | none: different concepts | as is | none needed | none |
| C15 | Link end and close codes | `quic::{AppCode, CloseLink, LinkEnd}`, `net-payload::{AppCode, UnlinkReason}`, `net-link::Unlink`, `net::{CloseCode, LinkLossCause}` | the stack's | `net-payload`, `net-link` | batch "Campfire on the stack", step 3 (quic deleted); the mapping is spelled out below | S |
| C16 | Admission per address | `quic::{Admission, AddressLimit}`, `net-udp::{Admission, AdmissionConfig}` | `net-udp`'s | `net-udp` | batch "Campfire on the stack", step 3 | S |
| C17 | Port map | `quic/src/port_map/`, `net-port-map` | `net-port-map` | `net-port-map` | batch "Campfire on the stack", step 3 | S |
| C18 | Wire encoding | `net::WireCodec`, `net-messages` default serde through `codec`, `net-bytes::ToBytes` | `codec` through the stack's registration; `ToBytes` for headers | `codec`, `net-bytes` | batch "Campfire on the stack", step 2; `ToBytes` canonicality is **new** | S |
| C19 | Link models and test drivers | harness `DelayLine` + `LinkModel` + `CrossbeamIo`, `net-emulator`, `net-stepper` | emulator now, stepper later | `net-emulator`, `net-stepper` | batch "Campfire on the stack", steps 4–5; stepper: design 12 "Planned" only | L |
| C20 | Buffers and arenas | `net-common::{Arena, ReadyBuffer}`, `net-core::TickBuffer`, `sim::DueIndex`, `net::SentInputs`/`ChainHistory`, harness `Arena` | none: different contracts | as is | decided (design 12) | none |
| C21 | Wake signal | `net-packets::Signal` (Mutex + Condvar), the Condvar wakes in `store`'s writers | optional `store` primitive | `store` | **new**, optional | S |
| C22 | Host-client topology | the stack's `HostClient` path in about 30 files; Campfire's local server on a thread | Decide | stack | **new** | M–L |
| C23 | Names across both | `Clock`×3, `Direction`, `Arena`, `Channels`, `Lifetime`, `Replication`, `Order`, `Start`, `Started`; `*Config` (stack, 13 types) beside `*Setup` (Campfire) | Decide per word | | **new** (`docs/research/vocabulary.md` covers Campfire only) | M |

## Findings

### C1. One tick rate type, but two resources

**Evidence.**
- `sim/sim/src/tick_rate/mod.rs:10`: `TickRate(NonZeroU32)`, a `Resource`. `length()` (`:22`) rounds `10⁹/hz` down. `ticks`/`duration`/`window` round milliseconds to ticks up, and `cut` rounds them down (`:28-60`). The sim declares it as its own resource (`sim/sim/src/sim_world_builder.rs:56`). About 20 capability systems read `Res<TickRate>`: stats, combat, areas, abilities, navigation, orders, projectiles, units.
- `network/net-core/src/tick_rate.rs:20`: `TickRate(NonZeroU16)`, a `Resource` under the `ecs` feature. `period()` (`:33`) rounds to the nearest nanosecond, and `duration_of`/`ticks_in` round once in `U32F32`.
- `network/net-timeline/src/local_timeline_plugin.rs:24-31`: `SetTickRate` overwrites the `TickRate` **resource** and `Time<Fixed>`'s timestep. Readers of that resource include `net-sync`, `net-interpolation`, `net-prediction` and `net-replication/src/hello_plugin.rs`.
- `network/net/src/pace.rs:54`: `PaceSpeed::tick` divides an already floored `Duration` by 2 or 4, which rounds a second time. `local/local_pace.rs:20` feeds it to Lightyear's `SetTickDuration`.
- The client builds the sim in the app's own world (`network/net/src/sim_client/mod.rs:202`: `Release::build(world, …)` on `app.world_mut()`). The sim and the stack therefore share one `World`.

**Differences.** The integer types differ (u32 Hz against u16 Hz), the rounding of a tick's length differs (floor against nearest), and the stack has rational conversions where the sim has millisecond conversions. They also mean different things. The sim's rate is the session's terms' rate: it is fixed for the match, hashed into the session id ("u32 tick rate", `docs/design/05-protocol-spec.md:43`), and the books are built at it. The stack's rate is the timeline's current pace, which batch "Campfire on the stack" step 4 changes by speed.

**Proposal.**
- Merge the type in `campfire-common`, as batch "The stack on Campfire's base crates", step 2 plans: ticks over whole seconds, the length to the nearest nanosecond, plus the sim's four millisecond conversions.
- Keep **two resources**: the match's rate, which only the terms set, and the timeline's rate, which `SetTickRate` sets.
- Two reasons force this:
  1. `common` may not depend on `bevy_ecs`: "`common` depends on no crate but `serde`, `derive_more`, its hashers…" (02, Structural rules, enforced by the manifest test). A type in `common` therefore cannot derive `Resource`, and each side needs a newtype resource anyway.
  2. If both sides read one `TickRate` resource, a local match at 2× would make every capability's `Res<TickRate>` see twice the rate. Each millisecond-to-tick conversion in a tick would double, the local server's log would stop replaying under the verifier at the terms' rate, and client prediction would diverge from the server.
- Suggested names: the sim keeps `TickRate` as its resource around the common value (or `MatchRate`), and `net-timeline` owns `TimelineRate`.
- `PaceSpeed::tick` goes: the pace multiplies the rational rate.

**Plan.** batch "The stack on Campfire's base crates", step 2 merges the type. batch "Campfire on the stack", step 4 ("the pace sets the match's rate times the speed through `SetTickRate`") does not say which resource the sim reads. The two-resource split is new.

**Size.** M. Crates: `common`, `sim`, `runner`, `package`, the capability crates, `net-core`, `net-timeline`, `net-sync`, `net-interpolation`, `net-prediction`, `net-replication`, `net`, `client`.

**Confidence.** High on the shared-world hazard and the manifest rule. Medium on the names.

### C2. Tick spans in `fixed` against `Num`

**Evidence.** `net-core/src/tick_time/mod.rs:29` (`TickInstant` on `U32F32`), `:138` (`TickDelta` on `I32F32`), `:281` (`PositiveTickDelta`), and `net-core/src/overstep.rs:19` (`Overstep` on `U0F16`, `u8` on the wire). Campfire's `math::Num` has 24 fractional bits.

**Differences.**
- The fractional step is 2⁻³² for `fixed` and 2⁻²⁴ for `Num`.
- `fixed` saturates, as the stack's design wants ("The tick family saturates").
- `Num` follows its own rounding rules (07-determinism-core).

**Proposal.** Follow batch "The stack on Campfire's base crates", step 1. Make sure `Num`'s arithmetic for these types saturates as `fixed`'s did. The pinned values in `tick_time/tests.rs` are the check.

**Size.** M. Crates: `net-core`, `net-timeline`, `net-sync`, `net-interpolation`, `net-prediction`, `net-emulator`; `fixed` leaves the lockfile.

**Confidence.** High that this is planned. Whether `Num` has saturating operations for each one `TickInstant` and `TickDelta` use was not checked.

### C3. Match ticks and network ticks stay apart

**Evidence.** `common/src/tick.rs:9` (`Tick(u64)`, decode refused past 2⁶²), `net-core/src/net_tick.rs:22` (`NetTick(pub u32)`, saturating, `i32` differences, 4 bytes on the wire), and `net/src/match_clock.rs:9` (`MatchClock`, the one mapping, which panics past `u32`).

**Proposal.** No merge. The two types make different promises: the match's time is untrusted state, while `NetTick` is the timeline's wire counter. `MatchClock` survives batch "Campfire on the stack" unchanged, apart from its import.

**Confidence.** High.

### C4. `SplitMix64` and what a draw means

**Evidence.** `sim/math/src/rng/split_mix64.rs:5` (internals only, `next_u64` only) and `network/net-common/src/split_mix64.rs:4`.
- `next_f32` (`:22`) takes the top 24 bits as an `f32`. `below` (`:29`) is a multiply-shift with no rejection, biased below `bound/2⁶⁴`.
- `sim/math/src/rng/mod.rs:73` (`Rng::below`) uses Lemire's method with rejection and is exact. `:84` (`chance(Num)`) compares 24 bits. `:91` (`fraction`) is also 24 bits.
- Users of the stack's draws: `net-emulator/src/link_model_run/mod.rs:55-109` (production in singleplayer once batch "Campfire on the stack" lands), `net-stack/src/scenario/faults.rs:43-62`, `net-common/src/internals/mutations.rs`, `net-packets/src/endpoint_core/tests.rs:310-315`, and the harness's `in_process_match/delay_line.rs:49`.

**Differences.** Two `below` rules, one biased and one exact. `f32` probabilities against `Num` probabilities.

**Proposal.**
- Keep math's `SplitMix64` (batch "The stack on Campfire's base crates", step 3), out of `internals`.
- Give it `below`, `chance(Num)` and `fraction()` that share `Rng`'s private `lemire_step`, `chance_from_word` and `fraction_of_word`, so a draw means the same on either generator. Correctness first: an exact `below`.
- Remove `next_f32` with batch "The stack on Campfire's base crates", step 1, which already moves the emulator's probabilities to `Num`.

**Cost.** Seeded emulator runs, and the scenario seeds that pin outcomes, may move. That was not checked.

**Size.** S. Crates: `math`, `net-common`, `net-emulator`, `net-stack`, `net-packets` tests, `harness`.

**Confidence.** High.

### C5. Three sources of OS random bytes

**Evidence.** `session/protocol/src/entropy.rs:6` (`Entropy::{System, Fixed(u8)}` on `getrandom` 0.3), `network/net-common/src/random.rs:8` (`Random::{Os(ring SystemRandom), Seeded(SplitMix64)}`), and `network/net-udp/src/os_random.rs:7`. The last uses `ring` directly, beside `Random::os()`, which the same crate uses at `sockets/mod.rs:74,122`. Its callers are `server_secret.rs:18` and `sockets/mod.rs:71`. The lockfile holds `getrandom` 0.2.17 (ring's), 0.3.4 (Campfire's) and 0.4.3.

**Differences.** `Entropy` is `Copy` with a constant-byte test variant, built for keys and signatures. `Random` is a stateful stream with a seeded variant, built for repeatable runs.

**Proposal.**
- Inside the stack, delete `net-udp::os_random`. Draw `ServerSecret::generate` and the token secret from `Random::os()`, so the stack has one source. A seeded run then also seeds them, where today `serve` always draws from the OS.
- Keep `Entropy` and `Random` apart. They make two promises, and the layers forbid one shared home: `protocol` is above the base layer, and `common` may not take `getrandom`.

**Size.** S. Crates: `net-udp`.

**Confidence.** High on the duplicate. Medium on keeping two types.

### C6. `ChannelBinding`, and what an in-process link signs

**Evidence.** `common/src/channel_binding.rs:7` (`Value32` alias: hex, serde, order), `network/net-packets/src/channel_binding.rs:5` (`pub struct ChannelBinding(pub [u8; 32])`), and `net-packets/src/establish.rs:69` (`binding: Option<ChannelBinding>`, `None` for a plain session). Campfire's connect answer signs `&ChannelBinding` (`session/protocol/src/connect/mod.rs:31`). Today the local link signs the constant `ChannelBinding::new([0; 32])` and pins `TlsKey::new([0; 32])` (`network/net/src/local/local_server/mod.rs:48,112`).

**Proposal.** Merge as batch "The stack on Campfire's base crates", step 3 plans, with `net-packets` naming `campfire_common::ChannelBinding`.

**New.** Batch "Campfire on the stack" step 4 links singleplayer through the emulator. `net-udp` offers a sealed handshake over the emulator (`EmulatedServer::serve` and `emulated_client`, `net-udp/src/sockets/mod.rs:128-178`), which yields a real binding and a real raw key. A plain emulated session yields `None`. Two options:
- **(a)** Seal the local link. One path, and the answer signs a real binding. Cost: a TLS handshake at a singleplayer start.
- **(b)** Keep a plain link and sign a fixed binding. The `[0; 32]` stand-in stays.

**Size.** S. Crates: `common`, `net-packets`, `net-udp`, `net-endpoint`, and `net` (batch "Campfire on the stack").

**Confidence.** High on the facts. The choice is the user's.

### C7. `TlsKey` and `ServerKey` are one key

**Evidence.**
- `common/src/tls_key.rs:6`: `TlsKey = Value32<TlsKeyMeaning>`, "The Ed25519 public key a server presents to TLS with no certificate".
- `network/net-udp/src/server_key.rs:4`: `ServerKey([u8; 32])`, "The Ed25519 public key a server presents with no certificate (RFC 7250)".
- `quic/src/spki.rs` and `net-udp/src/spki.rs` differ only in that type (12 changed lines).
- `quic/src/pinned_key_verifier.rs` duplicates `net-udp/src/pinned_key_verifier.rs` in its verify methods.
- Campfire's "server key" is a different key: the secp256k1 key in `ServerPin.key` (`net/src/sim_client/server_pin.rs:10`, which also holds `tls_key: TlsKey`).

**Proposal.** `net-udp` names `campfire_common::TlsKey`: its `ServerTrust::pinned`, `ServerIdentity::server_key` and `Spki::of`. Delete `net-udp::ServerKey`. Reasons:
- One key gets one type.
- `TlsKey` already has the hex form that a listing and a pin read.
- "server key" already means the session's signing key in Campfire, so the stack's word would give one word two meanings.

**Plan.** batch "Campfire on the stack", step 3 says "`TlsKey`, which the stack's raw key replaces if they are one key". They are one key. The direction should be the reverse: `TlsKey` replaces `ServerKey`.

**Size.** S. Crates: `net-udp`, `net-stepper`, `client`, `server`.

**Confidence.** High.

### C8. The server secret's derivation changes the published key

**Evidence.**
- `quic/src/derived_secret.rs:19-20` labels the derivations `campfire/tls/v1` and `campfire/quic-reset/v1`.
- `net-udp/src/derived_secret.rs:20-21` labels them `campfire/udp/tls/v1` and `campfire/udp/reset/v1`.
- Both use HKDF-SHA256 with no salt.
- The server today passes `Signer::secret_bytes()` (`session/protocol/src/signer.rs:60`) into `ServerSocket::bind` (`apps/server/src/main.rs:100`).
- `net-udp::ServerSecret` holds `Zeroizing<[u8; 32]>` (`server_secret.rs:8`). `common::Secret<N>` (`common/src/secret.rs:16`) promises that "every type that holds secret bytes holds them in one", with constant-time equality.
- `HashDomain::new` (`common/src/domain_hasher.rs`) accepts only `campfire/<name>/v<n>`, with no `/` inside `<name>`. `campfire/udp/tls/v1` does not have that form.

**Consequences.**
- The same key file gives a different `TlsKey` after batch "Campfire on the stack", so each listing's pin changes once. That is acceptable under "One release on both ends", but it should be said.
- The labels break Campfire's tag form.

**Proposal.**
- Feed `ServerSecret` from the server key's secret, as batch "Campfire on the stack", step 3 plans.
- Rename the labels in Campfire's form, for example `campfire/udp-tls/v1` and `campfire/udp-reset/v1`, or keep quic's TLS label so the published key does not change.
- Hold `ServerSecret`'s bytes and the token secret (`sockets/mod.rs:150`, a plain `[u8; 32]` on the stack) in `common::Secret<32>`.

**Size.** S. Crates: `net-udp`, `server`.

**Confidence.** High on the facts. The label choice is the user's.

### C9. Log capture in tests

**Evidence.**
- `net-stack/src/scenario/error_capture.rs:12`: a thread-scoped `with_default`. It keeps `ERROR` events whose target starts with `campfire_net`, as strings.
- `net-stack/src/shared_plugin/tests.rs:155`: a second `ErrorCapture` that counts the same events.
- `log/src/json_layer.rs:61`: `LogCheck`, the process's subscriber at Warn and Error. Each event must be taken by type or the test fails, which serves 02's "Warnings fail tests".
- `log/src/json_layer.rs:135`: `capture(f)`, already thread-scoped but `pub(crate)`.

**Differences.**
- Level: Error only against Warn and Error.
- Scope: one thread against the process. Thread scope is deliberate in the stack, whose scenarios run in parallel through `Parallel::map` (`13-net-stack/testing.md:98-100`).
- Target filter: only `campfire_net` targets against every target.
- Typed take: none in the stack, `LogEvent`s in Campfire.

**Proposal.** `campfire-log`'s internals gain a scoped check: `capture` made public as a `LogCheck` around a closure, with the same take-or-fail rule. Both `ErrorCapture`s go. Each stack warning that a scenario logs then becomes a test failure, which is Campfire's rule, and that cost should be stated. A thread-scoped capture misses a stack session that runs on its `Worker` thread (`net-endpoint` `Driver::Thread`). The emulated scenarios run inline, but that was not checked for every scenario.

**Size.** M. Crates: `log`, `net-stack`.

**Confidence.** Medium. The fix is clear, but the number of warnings the stack's tests log today is unknown.

### C10. Three counting allocators, two audit drivers, and a feature unification risk

**Evidence.**
- `common/src/alloc_audit/counting_allocator.rs:23` counts every thread but the main one inside a process window, with `backtrace`. `common/src/alloc_audit/mod.rs:22` (`AllocAudit`) probes or fixes the warmup and holds each frame to a budget.
- `net-stack/tests/alloc/allocator/mod.rs:20` counts per thread with `std::backtrace`. `net-stack/tests/alloc/audit/mod.rs:36` (`Audit`) has a fixed warmup of 128 and 256 frames. Its `:106` `assert_one_thread()` relies on "`bevy_tasks` builds without `multi_threaded`".
- `harness/src/bench/allocations.rs:24` is a plain total counter used as a criterion `Measurement`.
- The designs conflict:
  - 02, "Allocation audits": "Refused: a window of the test's thread alone, which passes a worker that allocates."
  - `13-net-stack/testing.md:72-77`: "One allocation binary, with a per-thread counter of our own… Refused: … process-wide counters… the `backtrace` crate".

**Risk (new).** The workspace's `bevy` enables `multi_threaded` (`source/Cargo.toml:220`). After batch "Campfire on the stack" moves every crate to one Bevy, a workspace build unifies that feature into `net-stack`'s test binaries. `assert_one_thread` would then fail, or the per-thread counter would miss pool threads. The stack's tests that assume single-threaded schedules might also change behaviour (`shared_plugin/tests.rs` already sets `SingleThreadedExecutor` by hand). Not checked: no build was run.

**Proposal.**
- `common`'s audit wins on correctness, since it counts workers, and the nextest process per test makes a process window safe.
- `net-stack/tests/alloc` moves onto `CountingAllocator` and `AllocAudit`, with `AllocAudit::warmup(128)` and 256 frames, and drops its allocator and driver. Its trace formatter folds into `AllocTrace` if it has something `AllocTrace` lacks.
- The harness's bench measurement reads a total counter that `common`'s allocator exposes.
- `13-net-stack/testing.md`'s decision is replaced by 02's.

**Plan.** The issue log lists it (Ready, **Net-stack**). PLAN.md does not, and batch "Campfire on the stack" should take it because of the risk.

**Size.** M. Crates: `common`, `net-stack`, `harness`.

**Confidence.** High on the duplicate and the design conflict. Medium on the risk.

### C11. Bench policy

**Evidence.**
- `net-common/src/bench/kind.rs:6` (`Kind::{Atomic, Integration}`), `scale.rs:7` (`Scale::{Sample, Frame}`: 50 samples over 1 s, or 10 over 3 s), and `bench/mod.rs:17,22` (`without_logs`, `count`).
- Campfire spells `"atomic/…"` and `"integration/…"` as string literals in about 35 groups. It sets `sample_size(10|20)` by hand in `harness/src/bench/*.rs`, `session/protocol/src/server_seeds/bench.rs:18` and `match_build.rs`, and uses criterion's defaults elsewhere.

**Differences.** A typed `Kind` against strings, which Campfire's "Types over strings" rule refuses. One sampling policy against ad hoc settings.

**Decide.**
- **(a)** `Kind`, `Scale`, `count` and `without_logs` move into `common`'s `internals` with an optional `criterion`, the precedent being `backtrace` behind `internals`. Every Campfire group adopts them. Cost: the manifest rule of `common` admits `criterion` behind `internals`.
- **(b)** A small base crate `campfire-bench`, never shipped. Cost: one more crate row.
- **(c)** Keep design 12's choice: `Scale` stays the stack's, and Campfire's groups keep their own settings. The issue log's item then goes to Decide.

**Size.** S for (c), M for (a) or (b). Crates: `common` or a new one, `net-common`, and every crate with a `bench.rs`.

**Confidence.** High on the facts.

### C12. Registry hash and maps

**Evidence.**
- `net-core/src/stable_hasher.rs:7` hashes with BLAKE3, keeps 8 bytes and has **no domain tag**. `registry_hasher.rs:13` builds on it, and peers compare the result (`net-stack/src/protocol_check_plugin.rs:20`, `ProtocolCheck` of three `Option<u64>`).
- `05-protocol-spec.md:16` requires "every other hash the protocol and the sim make" to be BLAKE3 led by `campfire/<name>/v<n>`, with `common::DomainHasher` as the tool.
- `StableHasher` has no other user.
- `net-core/src/collections.rs:6` is a `HashMap` alias with `FixedHasher`, but 10 stack files import `bevy_platform::collections::HashMap` directly. Campfire's other crates use no hash maps: only `BTreeMap`, plus the stack and `rules`.

**Proposal.**
- `RegistryHasher` builds on `DomainHasher` with a tag such as `campfire/registry/v1`, keeping a `u64` if the wire wants one.
- `StableHasher` goes.
- The stack's maps all name `net-core`'s alias, or the alias goes. Pick one.
- The protocol check's hash becomes part of the 05 spec in batch "The fold".

**Size.** S. Crates: `net-core`, `net-messages`, `net-replication`, `net-transport`, `net-channels`, `common` (no change).

**Confidence.** High on the facts. The tag is a choice.

### C13. Clocks

**Evidence.**
- `net-core/src/instant.rs:15` (`Instant(u64)` in nanoseconds since the app's first update, the origin of `Time<Real>::elapsed()`) and `net-timeline/src/clock.rs:17` (`Clock`, with `Os` / `Frame` / `Auto` sources).
- Campfire's session timers read `Time<Real>::elapsed()` as a `Duration`: grace and offers in `net/src/sim_server/offering.rs:116-279` and `door.rs:266`, retry waits in `sim_client/mod.rs:372,512`, and receipts in `receipts.rs:39`.
- `net/src/os.rs:16` (`Clock::{System, Manual}`) gives Unix seconds for delegation expiry. It is a different quantity, and `13-net-stack/time.md` refuses its shape for the network clock.
- `store`/`sim` use `std::time::Instant` only for stage timing and benches.

**Proposal.**
- The session's monotonic timers take the stack's `Clock::now()` (`Instant`), following "One clock per app" (13-net-stack/time.md). A harness-driven app then has one time for both, and a real server reads the OS at the read, not at frame start.
- `net::os::Clock` stays and is renamed (see C23).

**Size.** S–M. Crates: `net`.

**Confidence.** Medium: the gain is consistency, and no bug was found.

### C14. Ids: no merge

`PeerId` (`net-core/src/peer_id.rs:13`) names a link's peer: an entity, an address, local or the server. `NetId` (`net_id.rs:13`) names a registered type on the wire, and `RemoteId`/`LocalId` (`net-connection/src/remote_id.rs:13`) hold a `PeerId`. `Seat` (`common/src/seat.rs:7`) names a session's player. Campfire maps links to seats in `net` (`Door`, `SeatLinks`, `PlayerLink`), and nothing in the stack knows a seat. There is nothing to merge, and the mapping survives in `net`. Confidence: high.

### C15. Ends of a link

**Evidence.**
- `quic/src/app_code.rs:9` and `net-payload/src/app_code.rs:11` are the same type. Both use 16 as the first app code, with 0 for "ended" and 1 for "dropped" (`STOPPED` in quic). quic also has 2 for `UNFIT`.
- `quic/src/link_end.rs:15` (`LinkEnd`, 14 variants) corresponds to `net-payload/src/unlink_reason.rs:13` (`UnlinkReason`, 13 variants).
- `quic/src/close_link.rs` (`CloseLink { entity, code }`) corresponds to `net-link/src/unlink.rs` (`Unlink { entity, reason }`).
- `net/src/close_code.rs:9` (`CloseCode`, 16–22) and `net/src/events/link_lost/mod.rs:17` (`LinkLossCause`, which carries `LinkEnd`) survive.

**Proposal.** `CloseCode::app()` returns `net-payload::AppCode`, and a close becomes `Unlink { reason: UnlinkReason::Closed(code.app()) }`. `LinkLossCause::Link` carries `UnlinkReason`, which serializes, as a `LogEvent` needs. `LinkLossCause::InProcess` goes once the local link runs on the emulator, since an emulated link reports an `UnlinkReason`. quic's `PeerStopped`, `Ended`, `Unfit`, `ClosedInHandshake` and `VersionMismatch` have no counterpart. The LAN check reads none of them; only `link_lost/tests.rs` names variants.

**Size.** S. Crates: `net`.

**Confidence.** High.

### C16. Admission

`quic/src/admission/mod.rs` and `net-udp/src/admission/mod.rs` hold the same token-bucket algorithm: `admit` at `:80` in both, and the diff is renames. `AddressLimit` (`quic/src/address_limit.rs:9`, seats and grace) maps onto `AdmissionConfig` (`net-udp/src/admission_config.rs:8`, per address and refill). `net-udp`'s server keeps live counts by key (`secure_establishment/server_side.rs:58,129-139`), which already fixes the issue log's **Quic** Ready item on `EndpointState::admit` walking every peer. Proposal: `apps/server` builds `AdmissionConfig::new(seats, grace)` (batch "Campfire on the stack", step 3). Size S. Confidence high.

### C17. Port map

`quic/src/port_map/` is the ancestor of `net-port-map`. The stack's copy draws from `Random`, takes a `MapRequest` (port and name) and cites `13-net-stack/port-map.md`. quic's last commits on these files (2026-10-19, `92c263bf4`) only renamed citations and `public` to `is_public`. `net-port-map` wins, and `apps/server/src/main.rs:132` calls it (batch "Campfire on the stack", step 3). Size S. Confidence high. A line-by-line check of `upnp_gateway/mod.rs` (382 changed lines) for a fix quic has and the stack lacks was **not done**.

### C18. Wire encoding

**Evidence.** `net/src/net_protocol/wire_codec.rs:18` (`WireCodec`, Lightyear 0.30's hooks into `campfire_codec::Binary`). The stack already encodes serde messages through `codec` (`net-messages/src/serialize_fns.rs:96-185`, `net-bytes/src/writer` and `reader`, `net-replication/src/component_codec.rs:7`).

**Proposal.** `WireCodec` goes (batch "Campfire on the stack", step 2).

**New (alignment).** `net-bytes::ToBytes` decoding is not canonical:
- `Option` reads any nonzero tag as `Some` (`net-bytes/src/to_bytes.rs:53`).
- A varint reads non-minimal lengths (`reader.rs:135-143`).

02 says "One encoding of each value" for the serde formats, and design 12 keeps `ToBytes` for the stack's headers. Once 05 holds those byte layouts (batch "The fold"), a third party can send two spellings of one header. Whether the spec admits that is a decision. The item belongs in the issue log, not in this slice.

**Size.** S.

**Confidence.** High on the facts.

### C19. Link models and test drivers

**Evidence.** `harness/src/in_process_match/delay_line.rs:22` (`DelayLine`: frame-counted delay, jitter and loss on `SplitMix64`, and `pin_round_trip`), `link_model.rs:5` (`LinkModel { delay, jitter, loss_per_mille, seed }`), `mod.rs:356` (`CrossbeamIo::new_pair`), against `net-emulator` (`LinkModel` in netem's terms, `EmulatedNetwork`, `LinkConditioner`) and `net-stepper`.

**Proposal.**
- As planned in batch "Campfire on the stack", steps 4–5: the emulator replaces `DelayLine`, `LinkModel` and `CrossbeamIo`, and the `Frame` clock replaces the round-trip pin.
- `InProcessMatch` on the stepper is in design 12 under "Planned" but has no PLAN.md entry. It should be a step after the slice, as the design says, or a Foundations entry.

**Size.** L. Crates: `harness`, `net`.

**Confidence.** High.

### C20. Buffers and arenas: no merge

`net-common::Arena` splits byte chunks into views, `ReadyBuffer` is a keyed heap that keeps FIFO order within a key, and `net-core::TickBuffer` is one entry per tick. Campfire's `sim::DueIndex` (an index of units by due tick, deduplicated by stable id), `net::SentInputs` (several inputs per tick, each with a landing state) and `ChainHistory` (heads by seq) keep other contracts. Design 12 already keeps `Arena` and `ReadyBuffer` in the stack. The harness's `Arena` (`harness/src/arena.rs:26`, a test match's books) only shares the name (C23). Confidence: high.

### C21. Wake signal (optional)

`net-packets/src/signal.rs:8` (`Signal`: `Mutex<bool>` + `Condvar`, with `Wake`) does what each of `store`'s writers builds by hand from `WorkerLock` and `Condvar` (`latest_writer/mod.rs:23`, `stream_writer/mod.rs:37-39`, `append_writer`). A `store::Signal` beside `Worker` would give one wake and one poisoning policy. Size S. Crates: `store`, `net-packets`. Confidence: low-medium; this is a nicety and fixes no bug.

### C22. The host-client topology

**Evidence.** The stack carries a `HostClient` path, where a client lives inside the server's app with no IO. It spans `net-connection/src/host_plugin/`, `network_topology*`, `net-messages` (`message_plugin`, `multi_message_sender`), `net-replication` (`server_replicon_plugin.rs` with 17 mentions, `control_plugin`, `replication_target`), `net-sync`, `net-interpolation`, `net-transport` and `net-stack/src/protocol_check_plugin.rs`: about 30 files. `13-net-stack/api.md:72-91` and `layering.md:48` decide how it works. Campfire's 08-sessions "Singleplayer is a local server" runs the server on a thread over a link, and design 12 "One in-process link" puts that link on the emulator. Nothing in Campfire will use the host client.

**Decide.**
- **(a)** Delete the host-client path under "No dead code". The stack's tests of it go too. Cost: L, and a later listen-server mode would need it again.
- **(b)** Keep it as a stack feature and record the reason in `13-net-stack/api.md`.

**Confidence.** High that Campfire does not use it. Its share of the stack's frame cost was not measured.

### C23. Names that collide once both live in one workspace

**Evidence.** Public types declared in both a stack crate and a Campfire crate:
- `Clock`: `net/src/os.rs:16` (Unix seconds), `net-timeline/src/clock.rs:17` (the network clock) and `stats/src/modifiers/modifier_clocks.rs:36`.
- `Direction`: `math`'s vector against `net-core`'s link direction.
- `Arena`: the harness against `net-common`.
- `Channels`: `abilities` (channelled casts) against `net-channels`.
- `Lifetime`: `stats` against `net-replication`.
- `Replication`: a `state-types` trait against a `net-replication` enum.
- `Order`: `capabilities` against `net-stepper`.
- `Start`: `production` against `net-connection`.
- `Started`: `actions` against `net-connection`.

The quic-only collisions (`AppCode`, `BindError`, `LinkBinding`, `PortMap`, `ServerSocket`) go with `quic`. The stack's 13 `*Config` types sit beside Campfire's `*Setup` types, which "Names" in 02 gives one meaning ("what a thing starts from").

**Proposal.** Decide per word, as the vocabulary audit did for Campfire's own code (`docs/research/vocabulary.md` predates the stack). Cheapest first: `net::os::Clock` → `UnixClock` (S), harness `Arena` → `ArenaMatch` or similar (S). Then `*Config`: either Names admits it as "a component's settings", or it becomes `*Setup` (M).

**Confidence.** High on the facts.

## What of `net` and `quic` survives batch "Campfire on the stack", and where

### `quic`: everything goes but these

| Piece (path) | Counterpart in the stack | Survives as |
| --- | --- | --- |
| `AppCode` (`app_code.rs`) | `net-payload::AppCode` | the stack's; `UNFIT` (2) has no use, as the stack's MTU is fixed at 1,200 |
| `CloseLink` (`close_link.rs`) | `net-link::Unlink { reason: Closed(code) }` | the stack's |
| `LinkEnd` (`link_end.rs`) | `net-payload::UnlinkReason` | the stack's; `net::LinkLossCause` re-points (C15) |
| `LinkBinding` (`link_binding.rs`) | `net-endpoint::LinkBinding` | the stack's, on `common::ChannelBinding` (C6) |
| `LinkTimes::CAMPFIRE` (`link_times.rs`), 1 s and 5 s | `net-packets::SessionConfig::default()`, the same values | the stack's; the citation moves to 08 |
| `LINK_MIN_MTU` (`lib.rs`) | `net-payload::DEFAULT_MTU` less `HEADROOM`/`TAILROOM` | the stack's constant; `harness/tests/net/prototype.rs:25,192-200` re-points |
| `AddressLimit` (`address_limit.rs`) | `net-udp::AdmissionConfig` | a call in `apps/server`: per address = seats, refill = grace (C16) |
| `ALPN = b"campfire"` (`server_key.rs:18`) | `UdpConfig::new(alpn)` | a constant in `net` or `apps/server`, passed to `UdpConfig` |
| `ServerKey` derivation from the server's own key (`server_key.rs`, `derived_secret.rs`) | `net-udp::{ServerSecret, ServerIdentity::raw_key}` | the caller feeds `Signer::secret_bytes()` into `ServerSecret` (C8) |
| `TlsKey` (in `common`) | `net-udp::ServerKey` | `common::TlsKey`, which the stack names (C7) |
| `PortMap` (`port_map/`) | `net-port-map::PortMap` | the stack's (C17) |
| `ServerSocket`, `QuicEndpoint`, `QuicPlugin`, `BindError` | `net-udp::{ServerSocket, ClientSocket, BindError}`, `net-endpoint::EndpointPlugin` | the stack's; `apps/client/src/connection.rs`, `apps/server/src/main.rs`, `apps/server/tests/stop.rs` re-point |
| `Admission`, `UsedTokens`, `RecvBatch`, `UdpPort`, `Spki`, `PinnedKeyVerifier`, `ResetKey`, `ArenaRing`, `Outbox`, `IoThread`, `EndpointState` | `net-udp`/`net-packets` | gone |
| `tests/alloc`, `benches/quic.rs`, `ThreadFixture` | `net-stack/tests/alloc`, `net-udp` benches | gone; `common/src/alloc_audit/alloc_trace/tests.rs` fixture strings re-point (batch "Campfire on the stack", step 3 lists it) |

### `net`: Campfire's game networking stays; only its transport seams change

| Piece | Survives | Change in batch "Campfire on the stack" |
| --- | --- | --- |
| Session host: `sim_server/` (lobby, door, offering, seat links, player link, spectators, viewer, checkpoints and checkpoint thread, journal and session dir, receipts, data filter, kind shows, match visibility, team rooms, bots, restore) | yes, in `net` | `campfire_quic::CloseLink` → `Unlink`; Lightyear imports → stack crates; timers optionally on `Clock` (C13) |
| Client: `sim_client/` (join state, sent inputs, chain history, receipt writer, client dir, `ServerPin`, `PlayerSigner`) | yes, in `net` | `ServerPin.tls_key` keeps `TlsKey`; `QuicEndpoint` → `ClientSocket` + `ServerTrust::pinned(TlsKey)` |
| Key files: `key_file/` (`KeyFile` over `store::SecretFile`, nsec through `protocol`) | yes, in `net`; the server's secret now also derives the UDP secret | none to the file format |
| Nostr and verifier links: the nsec format and event ids (`protocol`), session-log publishing (`Publisher`, `session_dir`), receipts the verifier reads | yes, in `net` and `protocol` | none |
| `CloseCode` | yes, in `net` | maps onto `net-payload::AppCode` |
| `LinkLossCause` and every `events/` `LogEvent` | yes, in `net` | `Link(UnlinkReason)`; `InProcess` goes once the local link is emulated |
| `MatchClock` (`NetTick` ↔ `Tick`) | yes, in `net` | import only (C3) |
| `Pace`, `PaceSpeed`; `LocalPace` | `Pace` yes; `PaceSpeed::tick` and `LocalPace`'s `TickDuration` writes go | the timeline's rate × speed through `SetTickRate`; the sim's rate untouched (C1) |
| `LocalServer`, `LocalLink`, `LocalRelink` | yes, in `net` | `CrossbeamIo` → the emulator; decide sealed or plain (C6) |
| `net_protocol/` (channels, messages, `NetProtocol`) | yes, in `net` | `ChannelConfig` and the stack's registration; `WireCodec` goes (C18) |
| `Os` (`Clock` in Unix seconds + `Entropy`), `SessionTimes`, `Hosting`, `faults/`, `process_exit` | yes, in `net` | `os::Clock` renamed (C23) |

## Not checked

- Effects that only a build or a run would show: whether `net-stack`'s tests see more than one task-pool thread once `multi_threaded` unifies (C10), and which seeded stack tests move when the `below` and `chance` rules change (C4).
- Whether `Num` offers saturating operations for each one the tick family uses (C2).
- How many stack `warn!` sites the scenarios hit today, which a Campfire `LogCheck` would turn into failures (C9).
- A line-by-line review of `quic`'s and `net-port-map`'s `upnp_gateway` for a fix one has and the other lacks (C17).
- How `net`'s rollback of the sim (`Unpredicted`, `StateRewritten`) relates to `net-prediction`'s rollback, beyond the imports.
