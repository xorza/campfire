# Campfire — Sessions

How a session outlives its server process, its players' links and its players.

## Decisions

- **A private journal makes the log durable.** The server appends each record to a journal of framed, checksummed records and syncs it on a background thread, as SQLite's WAL and LevelDB's log do; a restart reads it to its last whole record. The published log is written at the end from what the journal rebuilds, so it keeps one canonical file and no torn record reaches a verifier.
- **A checkpoint copies only what changed.** At a boundary the main thread encodes the changes since the last checkpoint into a delta, and a background thread applies it to its own copy, then encodes, hashes and writes the snapshot, so no tick waits for a world's snapshot.
- **Every state write marks its component changed.** The copy trusts Bevy's change ticks, so no state type may change while its change tick stays, and no state type holds interior mutability; a test compares the copy with a full snapshot every tick.
- **Bots are server slots.** The host gives each slot as a player, a bot or open; the server makes a bot's inputs and signs them, and an order script drives a bot.
- **The mode decides who may take a slot.** Its `[players]` gives late joins, bot takeovers and what a leaver's slot becomes; the host gives the grace period and the restore window.
- **Pause and game speed on the local server only,** by the virtual clock of the server and its one client; neither enters the log.
- **A burst after a stall stays below the max input delay.** One frame runs at most `max_delay − 1` ticks, so no on-time input becomes late; the rest is dropped and logged, as fixed-timestep loops do (Fiedler's *Fix Your Timestep*).
- **Keys are plain key files.** A secret key is an `nsec` in a file only its owner may open; a file others may open is refused on every OS, as OpenSSH refuses a key with loose permissions. Encryption by NIP-49 comes with `identity`.
- **A journal that fails ends the server.** A failed write or sync is never retried: Linux may drop the pages a failed fsync held, so a retry can succeed over lost data, as PostgreSQL's fsyncgate showed. The host's supervisor starts the server again, and the start restores the session; the server never restarts itself.
- **The newest login wins.** A join of a main key whose slot has a live link ends the old link and seats the new one, as game servers do, so a player who comes back is never locked out by a link the server has not yet seen fail.

## Slots and controllers

- **The log knows each slot's controller,** from its own records, with no package, so a verifier checks every packet against its slot's player.
- **A slot's units stay its own whatever controls it,** so the mode's rules see one player.

## Crash restore

- **A restore window from the journal's last write.** A server back within it replays the session from its latest checkpoint; one back after it aborts the session and publishes its log.
- **A stop is an exit, not a crash.** The first stop signal ends the frame, closes each link with `STOPPED` and deletes the port mapping; the session stays in its journal and restores on the next start.
- **Clients back with full jitter.** A client whose link fails tries again after waits that double to 8 s, each drawn at random below its bound, as AWS's full jitter, so clients that lost one server do not all return in one instant.

## Reconnect and late join

- **A returning client resumes its chain.** A client that holds its chain's history cuts it back to the head the log holds; one with no history takes the server's head, as the signatures already logged cover every earlier input.
- **A grace period before a leave.** A player whose link fails keeps the slot for the host's grace period, measured by the server's monotonic clock.

## Receipts

- **A receipt names only what a crash cannot lose.** Only inputs whose journal records are synced count.

## Checkpoints

- **Restore equivalence.** A match restored from a checkpoint plays the same ticks to the same hashes as the match that made it; every gap a test finds is closed as state, or as derived data rebuilt after a restore.

## Saves and loads

- **A load keeps its random outcomes.** A load cuts the log back to the save, whose segment keeps its number and seed, so the published log's segments stay consecutive and a load meets the same random outcomes for the same inputs, as XCOM's saves do.
- **Saves run on one release** until converters come with signed releases.

## Local server

- **Singleplayer is a local server.** A singleplayer client runs the server as a thread over Lightyear's in-process transport, with its player in slot 0, as Veloren and Minecraft do: one code path for singleplayer, LAN and online play ([Precedents](../research/precedents.md)).
