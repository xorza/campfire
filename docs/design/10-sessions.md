# Campfire — Sessions

How a session outlives its server process, its players' links and its players.

## Decisions

- **A private journal makes the log durable.** The server appends each record to a journal of framed, checksummed records and syncs it on a background thread, as SQLite's WAL and LevelDB's log do; a restart reads it to its last whole record. The published log is written at the end from what the journal rebuilds, so it keeps one canonical file and no torn record reaches a verifier.
- **The journal belongs to the server's session.** The journal, its frames not yet written and the durable heads of each slot's chain are one value only a server's session holds, which the session gives each call of its log that takes a record, so a verifier's session holds none and its log keeps no state of a server. Refused: the journal inside the log, which made every verifier's log carry a journal to skip.
- **A record is encoded once.** The log encodes each record once into its own buffer, which its file's count measures, a server input's signature covers with the place before it, and the journal copies into a frame; the journal hands the sink a server frame's records in one append, so its writer wakes once a frame, not once a record. Refused: one encode for the file's count, one for the signature's message and one for the journal, and a wake for each record (D34).
- **The replay is a cursor the session owns.** A replay's state, the next tick it seals and a schedule of its own that the log's entries drive again, is a value beside the log in the session, which the tick pulls each tick's inputs through and which goes once the replay reaches the log's next tick, so a verification, a restore or a load runs on live in the same session, the log at its end throughout. Refused: a cursor that borrows the log, which no Bevy resource can hold beside the log it borrows; and the log rewound in place, which made it both a record and a cursor whose readers had to know which (N-17).
- **A checkpoint copies only what changed.** At a boundary the main thread encodes the changes since the last checkpoint into a delta, and a background thread applies it to its own copy, then encodes, hashes and writes the snapshot, so no tick waits for a world's snapshot.
- **Every state write marks its component changed.** The copy trusts Bevy's change ticks, so no state type may change while its change tick stays, and no state type holds interior mutability; a test compares the copy with a full snapshot every tick.
- **Bots are server slots.** The host gives each slot as a player, a bot or open; the server makes a bot's inputs and signs them, and an order script drives a bot.
- **The mode decides who may take a slot.** Its `[players]` gives late joins, bot takeovers and what a leaver's slot becomes; the host gives the grace period and the restore window.
- **The host picks, and the terms record the pick.** The terms hold the place of each option's picked value, the mode's options by name ([Keys](05-protocol-spec.md#keys)), as Zero Hour's options string carries its starting cash, `SC=`, to every player and into its replay's header, and OpenRA's lobby settings hold each option; a place, not the value, as the mode's fingerprint fixes the values, and the rules refuse another count or a place past an option's values, as they refuse a visibility the mode does not admit.
- **A pick is given by its value.** The server's and the local client's `--option <name>=<value>` names one of the option's values, an option not named its default; a name the mode lacks, a value its option lacks, or picks a waiting session does not hold are refused before a session opens, as another map is.
- **A restore keeps its visibility.** A host's `--visibility` or `--spectators` that do not give the waiting session's visibility are refused before it restores, as other picks and another map are; nothing asked restores it as it was. Refused: the session's own kept with no word, which hid what the host asked.
- **Pause and game speed on the local server only,** by the virtual clock of the server and its one client; neither enters the log.
- **A burst after a stall stays below the max input delay.** One frame runs at most `max_delay − 1` ticks, so no on-time input becomes late; the rest is dropped and logged, as fixed-timestep loops do (Fiedler's *Fix Your Timestep*).
- **A key has one owner.** A key goes from its file into its signer, which is not `Clone`, is shared by `Arc` and erases the key as it drops; a secret is not `Copy`, is zeroed as it drops and compares in constant time, as `secp256k1`'s `SecretKey` does. Refused: keys and seeds as `Copy` values, whose copies no one can find to erase.
- **Keys are plain key files.** A secret key is an `nsec` in a file only its owner may open; a file others may open is refused on every OS, as OpenSSH refuses a key with loose permissions. Encryption by NIP-49 comes with `identity`.
- **A journal that fails ends the server.** A failed write or sync is never retried: Linux may drop the pages a failed fsync held, so a retry can succeed over lost data, as PostgreSQL's fsyncgate showed. The host's supervisor starts the server again, and the start restores the session; the server never restarts itself.
- **A disk that stops is a journal that fails.** A write and its sync that run past two minutes, as long as Linux's hung task detector waits, or records that pile up past a bound while the disk falls behind, fail the journal as a failed sync does; a writer that waits on the disk forever, as PostgreSQL's does, keeps receipts back and memory growing with no report.
- **The newest login wins.** A join of a main key whose slot has a live link ends the old link and seats the new one, as game servers do, so a player who comes back is never locked out by a link the server has not yet seen fail.

## Slots and controllers

- **The log knows each slot's controller,** from its own records, with no package, so a verifier checks every packet against its slot's player.
- **A slot's units stay its own whatever controls it,** so the mode's rules see one player.

## Crash restore

- **A journal is cut at its first bad frame.** A restore keeps the frames before the first torn or corrupt one, cuts the file there and logs what it dropped, as RocksDB's default `kPointInTimeRecovery`; refused: refusing the whole journal, which loses the session to one bad byte, and skipping bad frames, which replays a log with a hole in it.
- **The terms are the journal's.** The private record holds the seed chain alone, and the journal's signed header is the one record of the terms; a restore refuses a seed chain the terms do not commit to, and a journal of another session than its directory's. Refused: the terms in both files, which a restore read from one and ran from the other.
- **A restore window from the journal's last write.** A server back within it replays the session from its latest checkpoint; one back after it aborts the session and publishes its log.
- **A stop is an exit, not a crash.** The first stop signal ends the frame, closes each link with `STOPPED` and deletes the port mapping; the session stays in its journal and restores on the next start.
- **Clients back with full jitter.** A client whose link fails tries again after waits that double to 8 s, each drawn at random below its bound, as AWS's full jitter, so clients that lost one server do not all return in one instant.

## Reconnect and late join

- **A returning client resumes its chain.** A client that holds its chain's history cuts it back to the head the log holds; one with no history takes the server's head, as the signatures already logged cover every earlier input.
- **A grace period before a leave.** A player whose link fails keeps the slot for the host's grace period, measured by the server's monotonic clock.

## Receipts

- **A receipt names only what a crash cannot lose.** Only inputs whose journal records are synced count. It promises that they survive a crash, not a disk that corrupts them: a restore that cuts at a bad frame drops every input after it, receipted or not.

## Checkpoints

- **Restore equivalence.** A match restored from a checkpoint plays the same ticks to the same hashes as the match that made it; every gap a test finds is closed as state, or as derived data rebuilt after a restore.
- **A checkpoint never waits for the last.** The log keeps the checkpoints begun in the order they began, and their records come in that order, so a checkpoint due while the last one's snapshot still syncs begins at once. Refused: one checkpoint begun at a time, which made the tick at the boundary wait on the last snapshot's fsync.

## Saves and loads

- **A load keeps its random outcomes.** A load cuts the log back to the save, whose segment keeps its number and seed, so the published log's segments stay consecutive and a load meets the same random outcomes for the same inputs, as XCOM's saves do.
- **Saves run on one release** until converters come with signed releases.

## Local server

- **Singleplayer is a local server.** A singleplayer client runs the server as a thread over Lightyear's in-process transport, with its player in slot 0, as Veloren and Minecraft do: one code path for singleplayer, LAN and online play ([Precedents](../research/precedents.md)).
