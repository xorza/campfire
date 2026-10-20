# Layering

How the stack's crates divide their work and depend on each other. The packet session's own layering
is in [packet session](packet-session.md); the tests' homes are in [testing](testing.md).

## Kinds

The kinds of crate, problem, IO, glue and test support, and their rules are the engine's
([Every crate declares its kind](../02-engine-core.md#modules)).

## Splits

- **The vocabulary is plain, and the clock and the timelines a glue crate of their own.** They
  are `net-core` and `net-timeline`. Refused: a plain tick crate beside an ECS `core`,
  or the vocabulary in `common`, each of which leaves a crate two jobs.
- **The iteration over links is `net-transport`'s.** `AdaptiveQueryIterMut` runs one link
  serially, as Bevy's `par_iter_mut` starts a task for one item, and each of its callers,
  transport and the layers above it, iterates links. Refused: a crate for one helper; a copy in
  each user, which can become different; `net-core`, which gives the vocabulary a second
  job.
- **The channels are a problem crate, and `Transport` the component that wraps them.** A link's
  channels and their two packet steps are `net-channels`, which reads what a packet session opened
  and packs messages into its budget; `net-transport` holds the component, the registry and the
  systems. Refused: the channel files alone in the problem crate, which makes about 15 of its
  internal types public for the glue's packet loops.
- **The entity maps, their views and the context registry are `net-messages`',** their one
  user; the codec of an entity is a module of messages' own, as the orphan rule keeps `serde`'s
  `ToBytes` from it. Refused: an `ecs` feature in `net-bytes`, which gives the byte codec a
  second problem.
- **The link estimator and the timeline controller stay in `net-sync`.** They are four small
  files that a crate of their own would not pay for.
- **The UDP protocol is a problem crate, and its socket an IO crate.** `net-udp` holds the
  handshake, the seal, the admission and the server and client of the protocol over any datagram
  IO, the emulated network's among them; `net-socket` holds the OS's socket, `quinn-udp`'s batches
  and the poller, and carries a server or a client of `net-udp`. So `net`, a glue crate, runs the
  sealed singleplayer link with no IO crate under it, and only the apps take the socket. Refused:
  one crate of both, which puts an IO crate under every glue crate that links in the process; an
  optional socket behind a feature of `net-udp`, which every app turns on, so the rule holds on its
  letter alone. Source: `quinn-proto` holds QUIC with no IO and `quinn` binds it to the OS.
- **The emulated wires and `net-udp`'s emulated server and client are production.** The wires are
  the packet session's IO: `net-packets` holds `EmulatedWires`, a `DatagramIo` over `net-emulator`'s
  model that needs no engine, so `net-packets` stays a problem crate; the sealed singleplayer link
  runs the emulated server and client ([Workspace alignment](../14-workspace-alignment.md), The
  singleplayer link is sealed). Refused: the emulated sockets behind `internals`, which the
  singleplayer link then cannot build.

## Edges

- **A crate's dev-dependencies are itself with `internals` and crates it already depends on;** a
  glue crate adds the engine and, through `internals`, the stepper. Refused: a problem crate's test
  that runs an app, which puts the engine and the glue under it.
- **An edge that only tests need is optional, and `internals` turns it on.** It points to test
  support or to a lower crate's `internals`; each other edge is one the crate's code uses.
