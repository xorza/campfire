# Replication

How a replicated entity's settings reach the members of its hierarchy.

## Hierarchy inheritance

The aspects are `Replicate`, `PredictionTarget`, `InterpolationTarget`, `Rooms` and manual
visibility.

- **A member follows its root for each aspect, unless it overrides that aspect.** One generic
  public marker, `Overridden<A>`, records the override, and its removal makes the member follow
  again. Refused: a provenance marker on the copies (`ReplicateCloned` and its two target
  counterparts) beside an override marker for rooms and visibility. The two polarities gave three
  rules for one question, and a copy marker could not tell a removal by the user from a missing
  copy, so a member could never stop following a target.
- **Absence is a value.** A member follows its root's absence too, and a member that removes an
  aspect while its root has one overrides it. This is how a member stops being predicted, or
  replicates to everyone while its root is in rooms. Refused: absence as a transient state that
  the next root change fills, which made those choices impossible.
- **A write equal to the root's value is a follow.** Observers cannot tell a user's insert from
  the hierarchy's write, so a member write that leaves the member's component unequal to the
  root's overrides, and an equal one follows. Manual visibility is an exception. Its write API
  tells user writes from the hierarchy's, so a direct write on a member overrides without a
  comparison across every sender.
- **An entity joins with what it holds.** At its first attachment, a component that differs from the
  root's, or manual visibility that hides it from a sender differently than the root, is an
  override. Everything else follows. A reattachment follows the new root and keeps the overrides.
  The hierarchy walk attaches the shallowest roots first, so no child joins an intermediate root for
  one flush and follows a value that was its own.
- **Detaching never widens what an entity replicates.** An entity that leaves its root drops the
  copies that let it replicate (`Replicate` and the two targets) and keeps the copies that limit
  where it replicates (`Rooms` and manual visibility) as its own. Refused: one rule for all
  aspects. Dropping the filters makes an entity with its own `Replicate` public when it leaves
  its parent, and keeping the grants makes an unparented child continue to replicate.

## The sender

- **A paused app still sends.** A send follows the fixed loop's step, once a frame, so a paused
  app, which runs no step, would send nothing; while `Time<Virtual>` is paused, the send
  interval runs on real time and a send goes with no step, mapped to the tick the timeline
  stands at, as several Replicon ticks may map to one. A link that joins a paused app, as a
  replay's viewer joins a replay held at a report's tick, so takes its state. Refused: a send of
  a replay's own, which a paused load or a join of a paused local match would also need.
- **A send follows each tick, planned.** The server sends after every tick it runs, a catch-up
  frame's included, so every tick's state reaches a client and a state that holds one tick, as
  a death before its unit despawns, is never merged away; a paused app keeps its send of no
  step. Practice: Unreal's `TearOff` replicates a dying actor's final state before the server
  stops sending it ([Zero Hour's deaths](../../research/zero-hour-deaths.md#other-engines)).
  Refused: a send once a frame, which a catch-up frame merges; a death message, which reverses
  effects.md's refusal of events; a tear-off that hides a dead unit from the sim but not from
  replication, a stack feature of several batches; a fixed keep of every dead unit, which a
  longer stall still merges away.

## The receiver

- **A client that both targets name is predicted alone.** `InterpolationYield`, a mirror of the
  prediction target's mode, is a second filter of `InterpolatedSend` that hides it on each link
  the prediction target names, as Replicon shows a component where every filter of its scope does,
  and the sender logs each such link.
  Refused: both markers, with which prediction and interpolation both write the component; a
  receiver that drops `Interpolated` beside `Predicted`, which leaves an entity on neither
  timeline when the prediction target goes; a field of `InterpolationTarget` kept in step, which
  hierarchy inheritance compares and would rewrite.
- **Each frame prunes every diff receiver to its history's front.** The prune drops a late pending
  diff below the front; such a diff would hold interpolation's floor, and so the front, for good.
  Refused: a prune only on a confirmed tick change or a moved front, which then never runs; a late
  diff refused at the receive with the prune moved into the holders, a change of two holders to
  spare one storage lookup for each entity.
- **The server alone ends a replicated entity's life on the receiver.** The server sends a despawn
  for each entity it despawns or hides, so the receiver's despawn of an entity detaches each
  `Remote` entity that only local entities hold under it and despawns those local entities with it,
  and every lifecycle path despawns through `DespawnReplicated`. The received `ChildOf` is kept as
  `RemoteParent`, and `ChildOf` is its view while the parent is mapped. Refused: Bevy's recursive
  despawn, which ends each member the server keeps when its root stops replicating, is hidden, is
  despawned late by interpolation, or is cleaned up at a disconnect; a sender that hides the members
  first, which leaves the receiver's own despawns recursive. Source: Unreal does not destroy the
  actors attached to an actor it destroys, and an app that wants them gone destroys them itself
  (Epic Developer Community, "How can you destroy attached actors when parent is destroyed?").
- **The value on screen is its own component.** A frame rule maps a fixed-tick component `C` to a
  visual component `V` the app names, which each frame writes from `C`'s last two ticks at the fixed
  overstep, and on an entity that is not frame-interpolated from `C`'s live value; `C` keeps the
  simulated value, so every fixed tick, a rollback's replay among them, reads the simulation, and a
  correction offsets `V` only where a frame rule of `C` writes it, as only that write takes the
  offset away again. Source: Bevy's `physics_in_fixed_timestep` example, and Unity Physics'
  graphical smoothing, which writes `LocalToWorld` and leaves `LocalTransform`. Refused: the visual
  written into `C` and the simulated value restored before the fixed loop, which leaves every other
  entry into the fixed schedule, a rollback's replay among them, to remember the restore; frame
  interpolation and correction deleted, as no shipped code uses them, which loses the feature for a
  future game.
- **Every despawn of the replication crates is `DespawnReplicated`, in a proxy crate of its own.**
  `net-replication`, `net-interpolation` and `net-prediction` each have a `clippy.toml`, the
  workspace's plus its own entries, that bans Bevy's despawns, and `net-despawns`, the one crate
  whose root exempts that ban, holds `DespawnReplicated`, which every despawn of the three calls; on
  an entity with no `Remote` entity under it, as each on a server, it despawns as Bevy does.
  Refused: an `expect` at each other despawn naming why its entity is not a replicated one, which
  judges each site by hand and keeps a despawn that is safe only while no `Remote` child exists; a
  hook or an observer, as Bevy can stop no despawn and queues the despawn of each child from its own
  hook (`bevy_ecs` 0.20, `relationship/mod.rs`); the rule in documentation alone, which a new
  despawn bypasses unseen; a workspace-wide entry, which would send every crate's despawns through
  the network stack.
- **A pre-spawn's tick is the simulated one, and the receiver makes its pre-spawns in `FixedMain`.**
  Only the simulation of a tick gives the client and the server one tick for a
  spawn, and only a spawn in `FixedMain` is made again by a rollback's replay, so a default hash
  or a receiver's pre-spawn outside it panics. Refused: the last simulated tick outside
  `FixedMain`, whose offset from the server's tick depends on the schedule. Source: Netcode for
  Entities 1.6.1 moved its predicted spawns into the prediction loop for the same reason.
- **An unmatched pre-spawn expires when the server's applied state reaches its tick.** Replicon
  applies a mutate message only after its update tick, and sends one on each send, so the latest
  completed confirmed tick means that each entity the server spawned through its tick is here.
  Refused: a timer, which despawns too early under a long send interval and RTT, and so makes a
  duplicate. Source: Netcode for Entities destroys an unclassified predicted spawn when the
  client's interpolation tick reaches its spawn tick, and its `ClientTickRate` docs note that
  snapshot congestion can make that too early, which a completed confirmed tick is not.
