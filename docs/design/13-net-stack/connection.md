# Connection

How a link, a connection and a server come up and go down, and who ends each.

## Lifetimes

- **The connection layer ends every server-side link.** `ServerConnectionPlugin` despawns each
  `ClientOf` that is not a host client once it is `Disconnected`, for any cause, with `try_`
  commands, and a reconnect is a new entity. Practice: bevy_replicon's backend contract, aeronet's
  `ConnectionPlugin`, renet, naia. Refused: each IO despawning its own links, which neither the
  session's endpoints nor Campfire's QUIC does; despawning on `Unlinked`, which skips
  `Disconnected` and its observers; keeping the entity and stripping replicon's state, which
  replicon forbids and which leaks every per-connection component.
- **Cleanup hangs off removal, not off one event.** The peer map entry goes with `Connected`'s
  removal, and only when it names this entity; `ControlledBy` sessions end with the controlling
  link's despawn; replicon's client components come and go with `Connected`; send queues clear
  with the session.
- **One state per marker, in parallel for each layer.** A link is linking, linked or unlinked; a
  connection is connecting, connected, disconnecting or disconnected; a server is starting,
  started, stopping or stopped. Each layer maps the one below it state for state, each down state
  keeps a typed reason, each request carries an `AppCode`, and each request works on a server-side
  `ClientOf`. The markers are the one state. Refused: a mirror enum such as `ClientState` or a
  `Link::state` field, which can disagree with the markers. A link has no going-down state, as an
  unlink is immediate.
- **Each IO closes its end on a local `Unlink`,** and each IO pair has distinct addresses.
- **An unlinked link holds nothing of its last connection.** `Unlinked` empties the link's
  session queues, for every IO: the next session would read them as its own, as the fate of a
  payload it never sent. Refused: each IO emptying its own, as Campfire's QUIC did, which leaves the
  defect in every IO that forgets.
- **"Stopped" names a server only.** A peer whose endpoint went away, as its app exited, closes
  with `AppCode::DROPPED`, which this end reads as `ClosedByPeer(AppCode::DROPPED)`.
- **Removing `ReplicationSender` detaches a link, and re-adding it replicates anew.** Every
  replicated entity is hidden from the link through one filter bit whose lifetime is
  `WhileVisible`, so its client despawns them, and clearing the bit makes Replicon spawn each
  one again. The link keeps its other Replicon state and its manual visibility. Refused: a pause
  that drops the link's messages, which loses the spawns and despawns it skips, as Replicon
  resends no update; resetting Replicon's `ClientTicks`, which Replicon keeps private; and
  forbidding the removal.
