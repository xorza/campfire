# The session log's state: proposal

A slice proposal for plan batch 6.4 (N-17, N-14, N-13), for review before it is built. When the batch lands, each lasting decision moves into [Sessions](10-sessions.md) and the [protocol](05-protocol-spec.md), and this document goes.

**The slice.** A server keeps in memory what its next tick needs and the entries since its latest checkpoint, writes the published log from its journal, and a verifier replays a file it only reads. Nothing a player sees changes, and no published file changes by a byte.

## Decisions

- **The live log is its latest checkpoint's state and the entries since.** As Raft keeps a snapshot and the tail of its log, the server's log holds the log's own state at the boundary of its latest recorded checkpoint, which that checkpoint's carry records, and the entries after it; a checkpoint's record drops the entries before its boundary. The journal is the record of the rest. Refused: every entry of the session in memory, which grows with the session and which 10-sessions.md does not ask for (D34).
- **What reads the past reads the carry.** A load goes back to a save from that checkpoint's carry, not from the log's own past decoded again. The bot driver learns from the carry whether a slot changed hands since tick 0, which the carry gains as a field of each slot, as it now reads every slot change of the log. Refused: the journal read again at a load, a file read on the tick's thread.
- **The replay is a cursor the session owns.** A replay's state, the tick ends it replays to and a schedule of its own, is a value beside the log in the session, which the tick pulls each tick's inputs through, and which goes once the replay reaches the log's end, so a restore runs on live in the same session. Refused: a cursor that borrows the log, which no Bevy resource can hold beside the log it borrows; and the rewound mode inside the log, which makes the log both a record and a cursor (N-17).
- **A published log is written from the journal, in two passes.** At the session's end a reader walks the journal's frames once to count what the file's counts need, the segments, the ticks of each and the entries of each tick, each load's cut applied, then once more to write the file, so the memory it takes is a frame's and the counts', not the session's. Refused: a log rebuilt whole from the journal and encoded, which holds the session in memory once more; and the file written from the live log, which no longer holds it (N-14).
- **The signature policy is a parameter of the decode.** A decode checks every signature, or trusts them as the log's own past, by an argument, so the log holds no mode that its other calls must know (N-17).

## Goldens

- **No golden moves by design.** The live log takes the same entries in the same order, and the file a published session gets is the file the in-memory log writes today. A golden that moves shows a record that moved, and the batch explains each such move before it lands.

## Checks

- A restore from a journal of three checkpoints holds the entries since the last of them alone, and a test counts them.
- Each pinned session log, its journal replayed, publishes to the same bytes as the log that was in memory, a load among them.
- The verifier verifies every pinned log, through the cursor.
- A load, a restore and a published log each read no entry before the latest checkpoint from memory.
- `./drive.py check` passes, and no golden moves.
