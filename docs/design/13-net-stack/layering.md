# Layering

How the stack's crates divide their work and depend on each other. The session's own layering
is in [session](session.md); the tests' homes are in [testing](testing.md).

## Kinds

- **A crate is a problem crate, a glue crate or test support.** A problem crate holds one problem
  with no engine in it: a protocol, a codec, a model, an algorithm. A glue crate binds a problem
  crate to the ECS, or holds a problem whose nouns are entities and components. Test support
  joins problems for tests. Refused: each layer one crate with its ECS code inside, which compiles
  the engine into every protocol and codec test. Source: `quinn-proto` holds QUIC with no IO and
  no OS time and `quinn` binds it to Tokio; `renet` and `bevy_renet`; rust-analyzer, where one
  crate alone knows LSP.
- **A problem crate depends on problem crates alone.** No Bevy crate but `bevy_platform` is
  among its required dependencies, so it builds and tests alone, and a test of `rules` refuses
  one. `bevy_platform`, Bevy's platform layer with no engine in it, gives the clock that Bevy's
  `Time` reads and the fixed-hash map, so a problem crate's instants and maps are the ECS's.
- **A plain type the ECS holds takes its ECS derives behind its crate's features,** which the
  glue crates turn on: `Reflect` behind `reflect`, and `Resource` behind `ecs`, as `TickRate` does.
  So one type keeps one name in both kinds of crate. Refused: dropping the derives, which the ECS's
  resources and components need; a remote reflection or a newtype in the glue, which repeats each
  type's fields or its name.
- **A glue crate never depends on an IO crate.** An IO reaches the ECS through
  `EndpointPlugin<Io>`, which the facade registers.

## Splits

- **The shared vocabulary is plain, and the clock and the timelines are a glue crate of their
  own:** `net-core`, `net-timeline`. Refused: a plain tick crate beside an ECS `core`,
  or the vocabulary in `common`, each of which leaves a crate two jobs.
- **The iteration over links is `net-transport`'s.** `AdaptiveQueryIterMut` runs one link
  serially, as Bevy's `par_iter_mut` starts a task for one item, and each of its callers,
  transport and the layers above it, iterates links. Refused: a crate for one helper; a copy in
  each user, which can become different; `net-core`, which gives the vocabulary a second
  job.
- **A link's channels and their two packet steps are a problem crate, and `Transport` is the
  component that wraps them.** `net-channels` reads what a session opened and packs messages
  into its budget; `net-transport` holds the component, the registry and the systems.
  Refused: the channel files alone in the problem crate, which makes about 15 of its internal
  types public for the glue's packet loops.
- **The entity maps, their views and the context registry are `net-messages`',** their one
  user; the codec of an entity is a module of messages' own, as the orphan rule keeps `serde`'s
  `ToBytes` from it. Refused: an `ecs` feature in `net-bytes`, which gives the byte codec a
  second problem.
- **The link estimator and the timeline controller stay in `net-sync`:** four small files
  that a crate of their own would not pay for.
- **The host client's path stays in the glue.** `net-transport` and `net-messages`
  read `HostClient` and `Disconnected` from `net-connection`, as `api.md`'s "A host client
  keeps an endpoint for each side" decides; `net-channels` does not.

## Edges

- **A crate's dev-dependencies are itself with `internals` and crates it already depends on;** a
  glue crate adds the engine and, through `internals`, the stepper. Refused: a problem crate's test
  that runs an app, which puts the engine and the glue under it.
- **An edge that only tests need is optional, `internals` turns it on, and it points to test
  support or to a lower crate's `internals`.** Each other edge is one the crate's code uses.
