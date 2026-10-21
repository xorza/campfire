# Testing

How the stack is tested over an emulated network; the clocks are in [Time](time.md).

- **Deterministic simulation from one seed.** Single-threaded schedules, `FixedHasher` maps, no
  OS time, and one seed for every generator, so a failing seed replays exactly; a canary runs one
  seed twice and compares a digest of the run. Each draw is `math`'s `SplitMix64`, whose
  `below`, `chance` and `fraction` keep the sim's `Rng`'s exact rules. Practice: FoundationDB's
  simulation and TigerBeetle's VOPR, which replay a run from its seed. Refused: a biased `below`
  and a float draw of the stack's own, a second meaning of a seeded draw.
- **The stepper runs Campfire's strict schedules.** Each stepper app makes `Update` and
  `FixedUpdate` strict through `schedule`, so an ambiguity among a layer's systems fails the
  layer's own test, as it fails a server's build (02-engine-core.md, The server's frame declares
  its order). Refused: the stepper setting the schedules' build settings past their proxy.
- **A discrete-event stepper holds the time.** Each app has its own frame interval, clock drift
  and stalls, and `run_for` runs the next due frame of whichever app; `frame_step` keeps moving
  every app together. Refused: lockstep alone, which cannot express different frame rates, drift,
  or a stalled app whose next frame Bevy clamps to a 250 ms advance (`bevy_time`'s
  `DEFAULT_MAX_DELTA`).
- **The emulated network is the in-process IO, in `net-emulator`.** The facade's public `emulator`
  feature adds its endpoint. Its wires carry packet sessions, as a socket does; on `WireClock::Os`
  with `LinkModel::perfect()` it is the one in-process IO. Campfire's tests run over it too.
  Refused: `internals`, which keeps it from every crate but the stack's; crossbeam as a second
  in-process IO with the same behaviour.
- **One `LinkModel` for emulated and real links, in netem's terms.** Latency and jitter,
  Gilbert–Elliott loss, duplication, reordering, a rate limit with a drop-tail queue, and an MTU;
  `EmulatedIo` runs one per direction with a timeline of phases, and an endpoint's `LinkConditioner`
  runs one on what its IO receives, before its packet sessions. Refused: deleting the conditioner,
  which leaves no way to degrade a real link in a dev build; and corruption, which a real link's
  packet protection drops before the stack sees it.
- **A link's queue drains in order across phase changes.** A packet never leaves before the ones
  queued ahead of it, even when a phase lifts the rate limit. The swarm found that lifting it let
  new packets overtake the queue, which no real link does.
- **The packet session times a silent peer out, on every IO,** so an outage of the emulated wire
  reaches a disconnect as one of a socket does. Refused: a timeout of the wire's own, which a
  socket lacks.
- **Presets are plausible profiles.** `DuplexModel`'s LTE, EDGE and Very Bad Network take the
  Network Link Conditioner's values as third parties report them (countingup, NSHipster), as no
  Apple page lists them; they are not measurements.
- **The packet session drops a copy of a packet,** by its window of 32 accepted packets and the
  packets the frame holds, so every IO gets the same at-most-once delivery on unreliable
  channels. Refused: leaving it to the IO, since a datagram IO without its own check would
  deliver a message twice.
- **Invariants, not traces.** A scenario runs a workload under seeded faults, heals the network,
  and checks what each layer promises, so a change of timing that keeps every promise passes.
- **Swarm runs leave features out.** Each seed turns each fault on with probability one half,
  which finds more failures than one run with every fault on (Groce et al., ISSTA 2012). The
  default run fits the suite's time budget; `CAMPFIRE_SWARM_SEEDS` runs more, and
  `CAMPFIRE_SWARM_SEED` replays one.
- **The stepper lives in `net-stepper`,** a test-only crate above connection, the packet session and
  the UDP protocol, as linking a client takes a packet session's endpoint, plain or sealed, and no
  production crate below the facade has both connection and one. It holds only what is layer-free:
  apps, time, `EmulatedNetwork`, `AppSchedule`, the client kinds, the steps, and the connection
  layer's stack and its tests. A stack value supplies each side's plugins, the per-link components
  and the condition that ends `init`. Each layer's `internals` depends on it, so each edge that only
  tests need points to it, and each other edge is one the layer's code uses. A change to connection
  runs the stepper's tests too. Refused: the stepper in `net-connection::internals`, whose edges to
  the packet session and the UDP protocol no code of connection uses; a harness crate at the top
  with each crate's tests in a `tests/stack/` binary, whose dev-dependency cycle builds for `tests/`
  only, never for a `#[cfg(test)]` module, and puts the whole stack in every layer's tests.
- **A stepper's worlds are its methods,** and a wait for a change to reach a client is
  `PROPAGATION_FRAMES`, never a bare count.
- **One replicon app for registry tests.** `net-replication::internals` gives the bare replicon app
  and the diff fixtures, generic over the component, which the layers above take. Refused: a shared
  manual clock or base app, as each hand-made app runs one system with the plugins its test needs.
- **Each layer's `internals` gives its stack on the one below,** with its test protocol;
  `net-stack`'s `internals` gives the full stack.
- **A protocol's test runs on the plain host of `net-packets::internals`,** which drives an endpoint
  with a number for each packet session where the ECS has an entity, so a test of the packet session
  or the UDP protocol needs no engine; what the ECS adds, as a link's `LinkBinding`, is tested in
  `net-endpoint`. Refused: an app in the protocol's tests, which puts the engine and the glue in its
  dev-dependencies.
- **A test lives in the lowest crate whose stack has every layer it uses,** in the `tests.rs` of the
  module it asserts, so the tests a change to a crate reaches hold every test of its code. Refused:
  one test crate as every test's home, where the chain of a touched crate misses its own tests.
- **`tests/` holds only a target that must be its own binary:** the allocation audit, whose
  `#[global_allocator]` is per binary.
- **The full stack's home is `net-stack`.** It holds the scenarios and the swarm under
  `#[cfg(test)]`, and the allocation audit.
- **One allocation binary, on the workspace's audit.** `net-stack`'s `tests/alloc` runs its
  fixtures through `campfire-measure`'s allocator and audit, whose window counts every thread of
  the process but its main one, and a failure prints each trace cut to the dependency frames that
  made the allocating call and the workspace's frames down to the fixture (02-engine-core.md,
  Allocation audits). Refused: a per-thread counter of the stack's own, which misses a worker and
  held only while every task pool ran on one thread, which the workspace's Bevy does not keep;
  `stats_alloc`, whose counters give no trace.
- **An audit holds each frame to a budget after a fixed warmup.** The budget defaults to zero and
  names each allocation it allows. The warmup outlasts the last growth of the workload's buffers,
  and a buffer bounded by time reserves its bound when it is made, so no warmup waits for it to
  fill. Refused:
  palantir's probe for quiet frames, which stops inside the warmup of buffers that fill by time;
  medians and idle-to-active deltas, which hide an allocation paid on fewer than half the frames.
- **The audit and the benches run with no subscriber.** A log line costs what the application's
  subscriber makes it cost, so the stepper adds no `LogPlugin` and logs its frames at `trace`.
- **Each crate's cases are a bench target of its own, with one sampling policy.** A crate's
  `benches/<crate>.rs` calls its `bench::run`; each case sits in the `bench.rs` of the module
  whose code it measures, under `internals`, on that crate's stack, and its id is
  `<kind>/<group>/<case>` (02-engine-core.md, One bench target for each crate), its kind and its
  sampling `campfire-measure`'s (02-engine-core.md, One sampling policy). Refused: one target for
  the whole stack, whose ids carry the crate as a fourth part. Practice: palantir's bench layout.
- **Bytes are golden tests, not benches.** A byte count on the deterministic stepper is exact, so
  replication and compression costs are asserted to the byte. Refused: a criterion `Measurement`
  of bytes, whose zero variance turns its plots off and whose number nothing compares.
- **A test runs in 1 s or less.** `cargo drive test` runs cargo-nextest for its per-test timing,
  and `.config/nextest.toml` names a test past 1 s, so a slow test shows in the run instead of
  slowing it. The swarm and the canary run their seeds in parallel through `store`'s `Parallel`,
  a seed on one thread at a time, which a log check scoped to its run allows, and each reserves
  every core while it runs (`threads-required = "num-cpus"`), as five of them at once with the
  rest of the suite oversubscribed the cores past the budget. Refused:
  `cargo test`, which times no single test; a lower seed count, which would trade coverage for the
  budget.
- **The stepper's remote clients are all plain or all sealed,** as the server's one endpoint
  takes plain packet sessions or sealed ones of the UDP protocol, and that endpoint spawns each
  client's link at the client's first packet, which takes the stack's components before it links.
  Refused: a stepper that spawns the server's links, which skips the handshake every app runs.
- **Every remote client kind runs the same tests.** A test of a remote client runs each of the
  stepper's remote `ClientKind`s where the kind can matter, as the message tests, the scenarios,
  the swarm, the canary and the allocation audits do.
- **A parser's robustness is a seeded mutation test.** Seeded bit flips, cuts, extensions and
  overwrites of each packet type reach the packet session, the handshake and the transport, and none
  panics; a sealed datagram that changed one byte never opens. Refused: a fuzzer run in CI,
  whose inputs no seed replays.
- **Every gate builds the code a feature gates off.** The `ship` lint, at default features, lints
  the channels without `compression_lz4`, which the lint of every feature turns on. Refused: a
  rustdoc lane, as 02 decides (02-engine-core.md, No docs lane).
- **No lane measures coverage,** as every lane of `cargo drive` shares one build's flags and cache,
  and coverage needs its own. Refused: a lane over `cargo-llvm-cov`, a tool to install; a lane that
  sets its own `RUSTFLAGS`, against the driver's one shape for each cargo command.
- **A test has the std `#[test]` and no subscriber,** as the audits and benches run, and a test that
  reads logs holds `campfire-log`'s check, scoped to its run, as the scenarios do, which a warning
  fails as an error does. A test name states the behaviour it checks. Refused: `test-log`, a
  dependency whose shadowed `#[test]` installs a subscriber that every test pays for.
