# API

How an app registers its protocol and names what the stack publishes.

## Registration

- **Registration is order-independent and ends at `finish`.** The first registration creates a
  registry, the `finish` of the plugin that holds it seals it, and a later registration panics.
  The access a `SystemParam` declares is then the access it uses. Refused: a rule that the
  plugins come before the protocol, which an app could break silently; one seal in the facade,
  which leaves an app built from the layer crates open.
- **A registry's hash holds its final entries, in the order of their ids.** Each entry's type and
  what the wire depends on enter it: a message's directions and its entity mapping, a channel's
  setup and directions, a component's replication. Refused: a hash fed by each registration
  call, which tells apart two orders of the calls that make one protocol and misses what a later
  call adds.
- **The registry hash is a domain hash.** The registry hashes through `common`'s `DomainHasher`
  under a `campfire/<name>/v<n>` tag, as the protocol decides for every hash ([Encoding and cryptography](../05-protocol-spec.md#encoding-and-cryptography)).
  Refused: a hasher of the stack's own, a second rule for one kind of digest.
- **One encoding of a message and a component.** A registration takes a serde type and encodes it
  through `campfire_codec::Binary`, so a message's bytes are canonical as every other encoding is,
  and a non-canonical one is refused on receipt. Refused: custom encodings beside it, each with
  rules of its own, and bans on each of their registration calls, a rule a type can hold.
- **One builder per protocol item, and the replication mode lives in its type.** A component's
  registration replicates whole or as diffs, and prediction and interpolation take the mode from
  the type. Refused: a `_diff` twin of every method; 16 App methods and 10 builder methods for one
  interpolation rule.
- **A component's `.interpolate` is its default rule,** at the lowest priority and with no
  filter; a bundle, a filter or a priority goes through `register_interpolation_rule`. A bundle
  rule takes each member's mode from that member's registration, at `finish`, which makes a
  bundle that mixes modes impossible to get wrong.
- **A rule's fn goes through one generic constructor for each shape,** `with<M>(f: impl
  IntoLerpFn<S, M>)`, with one impl for a component's `Fn(&C, &C, f32) -> C` and one for a
  bundle's `Fn((&A, &B), (&A, &B), f32) -> (A, B)`, kept apart by a marker type, and the fn is
  stored boxed. A bundle's blend then reads each member where its history holds it, with no
  clone. A closure with untyped parameters needs their types. Practice: Bevy's
  `IntoSystem<In, Out, Marker>`. Refused: `&(A, B)` endpoints, which clone each member twice for
  each blended entity on each frame; a fn pointer of an associated type, which rustc cannot
  coerce a fn item to while the target is unknown; a second builder for bundles, which repeats
  every constructor.
- **No `unsafe` but what an outside API requires.** Each registered component, message and rule
  runs typed systems whose `Query` walks its columns, and a registry keeps a type's fns behind a
  checked `Any` downcast. Kept: `polling`'s registration, the test allocator's `GlobalAlloc`, and
  the synced timelines' `SystemParam`s, which skip a system on a condition no derive expresses.
  Refused: erased archetype walks through `UnsafeWorldCell` and transmuted fn pointers, whose
  saving of one system run for each type no bench measured, as Bevy's generic systems do.
- **One plugin group for each kind of app, and no plugin adds another.** `ClientPlugins` and
  `ServerPlugins` list every layer, the shared ones once; a layer whose side takes several
  plugins gives them as a group of its own, `ServerReplicationPlugins` among them. An IO's
  endpoint plugin, `EndpointPlugin<UdpIo>` or the emulated network's
  `EndpointPlugin<EmulatedWires>`, is listed only behind that IO's cargo feature, and Bevy's
  `TimePlugin` is the app's. A layer adds its registry on both sides and its systems only where
  they run. Refused: a plugin that adds what it needs under `is_plugin_added`, which hides the
  order of plugins, and gives one plugin two installs.
- **The stack stays on Bevy's plugins.** Its layers are plugins in groups, as `net` is. Refused:
  the layers as Campfire's `Module`s, each one entry of a match's list with a capability, books and
  the sim's stages, which a network layer has none of.

## Surface

- **One path for each item:** private file modules, one root `pub use` for each published item,
  and no `prelude`. Refused: public modules with a prelude, which give two paths and make the file
  layout public API; a prelude alone, which puts `prelude::` in every path.
- **An export stays when an app uses it,** whether a workspace crate names it or not: what it
  adds, orders, constructs, reads or configures. An item that only its own crate adds, orders or
  constructs is `pub(crate)`, such as a plugin that only the crate's own plugins add, which no app
  can disable from a plugin group. Refused: "what the workspace names", which removes the
  connection lifecycle and the diagnostics paths.
- **A feature exists only with a caller and a test.** Deleted on that rule: timeline-targeted
  message delivery, with no working target timeline and no caller; and client-to-server
  replication, which bevy_replicon 0.45 does not do, so the call was a silent no-op. A client
  sends its data as messages. Lockstep, a zero prediction limit that added 2 ticks of input delay
  with no stated reason, which no code in the workspace ran.

## Code

- **A state the code cannot reach is a type.** A channel's message is a run of parts, a whole one
  a single part, and a send key names a message and a part, as QUIC names a stream's bytes by
  offset, so no key names a shape the message lacks; the handshake is a `ClientEstablish` or a
  `ServerEstablish`, so the server's has no `connect`; an error names its cases, not a `String`.
  Refused: a key typed by channel family and shape, which still meets a message of the other shape
  at run time.

## Vocabulary

- **One word for each concept, and one concept for each word.** The stack takes 02's
  [Names](../02-engine-core.md#names): a link entity is `link` and its other end the `peer`; a
  type's id on the wire is `net_id` and a sequence number is `message_id`; a lost packet is lost;
  counters are `*Counts` and estimates `*Stats`; per-call arguments are `*Cx` and kept state is
  `*State`; a member follows its root, which an override stops, and attaches to it and detaches
  from it; a timeline's current instant is `now`. Refused: a
  `*Metadata` name, which says nothing of what a type holds.

## Entities in messages

- **Only a client maps the entities a message names,** through replicon's map, then the link's
  own pairs from the hello. A server keeps its entities, and an entity that a
  client cannot map goes as `Entity::PLACEHOLDER` each way. Practice: Replicon, which maps on the
  client only. Refused: a mark in the entity's bits, which is generation bit 31 of a real entity;
  a flag beside each entity, which a `MapEntities` impl may visit in a different order on each
  peer.
- **A server accepts from a client an entity it replicates, or the link's own.** Any other is
  `Entity::PLACEHOLDER`, not a protocol violation, as an honest client names an entity that was
  hidden or despawned while its message was in flight. Practice: Unreal gives an unmapped RPC
  reference as null with `net.DelayUnmappedRPCs` off. Refused: the link's `ClientVisibility`, as
  replicon 0.45.2 has no public test of whole-entity visibility and a mask bit can hide only
  components; the stack's own entity-scope bits, which miss an app's replicon filters.
