# Campfire — Storage and workers

Where the engine writes files and starts threads: one crate, `store`, owns how a file is written durably and how a thread starts, ends and reports a failure; one type, which holds the data directory's lock, owns where each file of it lives; and one resource takes every failure that ends a server. The formats stay with their owners, and the deterministic core writes no file and starts no thread. This proposal follows [Sessions](10-sessions.md), whose journal, checkpoints and receipts brought the engine's writes and threads.

## Today

Stage 6 left three worker threads and one app thread, each with its own lifecycle:

| Thread | Crate | Shape | A failure |
| --- | --- | --- | --- |
| `journal` | `protocol` | A stream: appends framed records, groups the records of a busy tick in one sync, publishes how many are durable | Ends the server, exit code 74 ([D9](10-sessions.md#decisions)) |
| `checkpoints` | `net` | One job in flight: applies a delta, snapshots, hashes, writes, signs; gives the delta back for reuse | Ends the server, exit code 74 |
| `receipts` | `net` (client) | A slot: the newest receipt replaces one not yet written | Logged; the client plays on |
| `local server` | `net` | An app run on a thread, again after each load | Logged; the thread ends |

Each starts a named thread, closes and joins it on drop, and reports a failure its own way: `JournalWatch::take_failure`, `Checkpoints::take_failure` and `ReceiptWriter::failures`, which `ServerExit` and a client system read in turn. The files of a data directory are named by `join` calls in six places: `sessions/<id>/{private, journal, snapshots/<fingerprint>}`, `logs/<id>.campfire-log`, `tls`, `lock` and `server.nsec` on a server, `receipts/<id>.receipt` and `server/` on a client, and the LAN check names them again. Only a server's data directory is locked: nothing stops two clients that share one from writing the same receipt file. `protocol`, which design 02 makes the open protocol's formats, holds the journal's thread, the durable write and the key file's read, and `runner`, a crate of the deterministic core, passes the journal through.

Three gaps of today's code the design closes:

- **A drop that hangs a crash.** Each handle joins its thread when dropped. A panic on the main thread drops the world as it unwinds (the `release` profile unwinds), so a writer blocked in a sync the disk does not answer holds the process, and the host's supervisor never starts it again, against [D9](10-sessions.md#decisions); a writer's own panic, passed on by a drop during unwinding, aborts.
- **A stale temporary file's mode.** A durable write opens `<name>.part` with mode 0600, which applies only when the file is new: a `.part` left with a looser mode keeps it, and a key file renamed from it is one others may read.
- **A disk that stalls.** A sync that does not return leaves the journal's buffer growing and the receipts still, and nothing says why.

## Decisions

- **S1. `store` owns how, the formats' owners own what.** A crate `campfire-store` holds the durable write, the secret file, the directory lock, the workers and their failures. It depends on no engine crate, as `log` does. A file's bytes stay with the crate that defines them: the journal's frames, the private record, the receipt file, the log file and the key's NIP-19 text in `protocol`, the snapshot in `sim`, the TLS file in `server`.
- **S2. `protocol` does no IO.** The session log writes each framed record into a sink it is given, `RecordSink`, a trait of `protocol`, and counts the records it wrote; it takes the count the sink made durable from its caller. This is how QUIC stacks split their state machine from their IO (`quinn-proto` from `quinn`, the "sans-IO" design): the protocol crate builds and runs in any host, a browser's verifier among them, and a test drives it with no disk and no thread. `store` does not depend on `protocol`, nor `protocol` on `store`: `net`'s `SessionJournal`, a type of its own around `store`'s `AppendWriter`, implements `RecordSink`, as Rust allows a crate to implement a foreign trait only for a type of its own.
- **S3. One thread per job, not one IO thread.** The journal syncs on its own thread, and a snapshot's write never waits in front of it: a shared thread would hold the journal's sync behind a snapshot's, so receipts would come late and a crash would lose more records. Databases keep these apart for the same reason: PostgreSQL's WAL writer, checkpointer and background writer are separate processes, and SQLite's WAL checkpoint runs apart from its commits. Bevy's `IoTaskPool` is not used: it runs async tasks, and a blocking sync would hold one of its few threads.
- **S4. One lifecycle, three queue shapes.** Every thread starts through `store`'s `Worker`: named, told to close through its job's own wake when its handle drops, and joined. A handle dropped while its thread unwinds a panic tells its worker to close and does not join it, so a crash ends the process at once, as D9 needs, and what the worker had not synced is lost, as a crash loses it; nor does it pass on the worker's panic then, which would abort. The three queues stay what their jobs need, each a type of `store` built on `Worker`: `AppendWriter`, a stream with grouped syncs and a durable count; `Exchange`, one job in flight, its buffers given back; `LatestWriter`, a slot the newest value replaces. A generic queue would hide the differences, not remove them.
- **S5. One layout, which holds the lock.** `DataDir`, in `store`, is made only by taking the directory's lock, and gives every path in it from typed ids; holding one proves the lock is held, so no code writes into a data directory it has not locked. `net`'s server and client layouts name their files through it. A client's data directory is locked too: a second client on it is refused, as a second server is.
- **S6. One fault surface.** Each worker gives its failure, typed by the step that failed, to `Faults`, a resource of `net`, with its source: `Journal`, `Snapshot` or `Receipt`. One table gives each source's policy, and `ServerExit` and the client read only it: a journal or a snapshot that fails ends the server with exit code 74, and a receipt that is not written is logged. A failure before the app runs, in a restore's abort or a local server's start, is the error of the function that met it, as now.
- **S7. A slow sync is seen.** An `AppendWriter` sync that takes longer than a second gives a `SlowSync` with its time, which the server logs as a `JournalSyncSlow` warning, as etcd warns of a slow `fdatasync`; the server plays on, as a sync that returns late loses nothing. Ending a server whose disk stops answering is the host's watchdog's call, as MongoDB's storage watchdog is an option its host turns on.

## The store crate

- **Durable files.** `DurableFile`, moved from `protocol`: a whole file written to a temporary file beside its name, synced, renamed over it, and its directory synced on Unix; a new directory synced into its parent, and a directory removed with all it holds, its parent synced. The temporary file is always made new, a stale one removed first, so it has the mode the write gives it. Each step's failure is its own error, never retried ([D9](10-sessions.md#decisions)).
- **Secret files.** `SecretFile`: bytes only their owner may read, mode 0600 on Unix; a read checks the mode of the file it opened, and refuses one others may read, as OpenSSH does; a write is durable. The key file is `protocol`'s NIP-19 text in a `SecretFile`, and the server's TLS identity, `tls`, holds a private key too, so it is a `SecretFile` as well: whoever reads it may act as the server's TLS end.
- **Data directories.** `DataDir`: made, its owner's only on Unix, and locked by the exclusive lock on its `lock` file, held until dropped; a second holder is refused. It gives the paths in it.
- **Workers.** `Worker`, and on it `AppendWriter` (the journal's writer thread as it is now, over an `AppendFile`, a file or a test's stand-in, with the cut of a torn tail and the reopen), `Exchange` (the checkpoint thread's channel pair and spare buffer, for any job) and `LatestWriter` (the receipt writer's slot). An append, a send and a give take a lock and copy, and never wait for the disk. Each holds persistent buffers, so a steady state allocates nothing per record or per frame.
- **Failures.** Each worker's failure is an enum of the step that failed, with the `io::Error` it met, given once through its handle; `store` decides no policy. After its failure an `AppendWriter` takes no record, so its durable count stops where the disk stopped.

## Protocol without IO

- **Sink.** `SessionLog::keep_journal` and `resume_journal` take a `Box<dyn RecordSink>`; `runner` passes it through, as it passes the journal now. For each record, the log calls the sink with a function that frames the record into the buffer the sink passes, so the bytes go once, straight into the writer's buffer, as they do now.
- **Durable heads.** The log counts the records it wrote into its sink from the one it was given; `SessionLog::advance_durable` takes how many of them are durable from its caller, which reads it from the `AppendWriter`. The receipts' rules do not change ([Receipts](10-sessions.md#receipts)).
- **Reading back.** `JournalFrames` and `SessionLog::from_journal` stay in `protocol`, as they parse bytes; cutting the file to its whole frames and reopening it to append move to `store`.

## Layout

`net`'s layouts name, through `DataDir`, a server's `server.nsec`, `tls`, `sessions/`, a session's `private`, `journal` and `snapshots/<fingerprint>`, and `logs/<id>.campfire-log`; and a client's `receipts/<id>.receipt` and its local server's `server/`, a data directory of its own with its own lock. `SessionDir`, `ServerTls`, the receipt writer, the client's `--local`, the LAN check and the tests take their paths from them. The names, and so the files on disk, do not change.

## Not in scope

- Reading inputs at a start: a mode's packages, a log the verifier replays, an order script, a golden. These are reads of what the user gave, once, on the thread that needs them.
- Lightyear's transport and its threads, Bevy's task pools, and the log crate's file of JSON lines, which `tracing` writes.

## Structural rules

Two rules join design 02's table. Clippy enforces each at compile time: `source/clippy.toml` lists their functions under `disallowed-methods`, and a call of one fails the check chain, which runs Clippy with `-D warnings`. A lint resolves the path a call names, so an alias or a re-export does not hide it, as a scan of the source text would let it. The list applies to the whole workspace, as Clippy takes one configuration for each crate and merges none, so the code the rules allow says so with `#[expect(clippy::disallowed_methods, reason = "…")]`, which fails once nothing it covers calls one: `store`, which owns the rules, for the whole crate; a test, for its `#[cfg(test)]` module or its integration test's crate; and any other call, at the call, with its reason.

| Rule | Enforced by |
| --- | --- |
| Only `store` writes a file: no other crate writes, syncs, renames or removes a file, or makes a directory, but tests, the log crate's file of JSON lines, the LAN check's run directory, and a test's golden file in `runner`'s `internals`. So the deterministic core, `common`, `math`, `sim`, `script`, `capabilities`, `package`, `protocol` and `runner`, writes no file. | `disallowed-methods`: `std::fs::write`, `copy`, `rename`, `hard_link`, `remove_file`, `remove_dir`, `remove_dir_all`, `create_dir`, `create_dir_all`, `set_permissions`, `DirBuilder::new`, `File::create`, `File::create_new`, `File::options`, `File::set_len`, `File::sync_all`, `File::sync_data`, `File::set_permissions`, `OpenOptions::new` |
| Every thread starts through `store`'s `Worker`, but in tests. | `disallowed-methods`: `std::thread::spawn`, `std::thread::scope`, `std::thread::Builder::spawn`, `std::thread::Builder::spawn_scoped` |

A test beside the manifest test of `common` checks that `clippy.toml` lists each of these paths, and that no source allows the lint, turns it off in a `cfg_attr` or through a group that holds it, `clippy::style` or `clippy::all`, or expects it for a crate or a module other than as above.

## Tests

- The tests that cover each piece move with it: the durable write, the key file's refusals, the lock, the journal's frames, a torn tail and a failed sync, the receipt file.
- `protocol`'s session log tests drive a `RecordSink` in memory, with no thread and no file, and a log rebuilt from what it took equals the log.
- A worker's failure is given once; a dropped `AppendWriter` writes and syncs what it holds; a handle dropped as its thread unwinds a panic returns while its worker waits on a sync that does not return, and passes no panic on.
- A durable write over a stale temporary file of mode 0644 leaves a file of mode 0600; a second client on one data directory is refused; a sync held past a second gives `SlowSync`.
- A failed snapshot write and a failed receipt write each reach `Faults` with their source, and the server's exit follows the policy: a snapshot written into a directory whose place a file holds fails on every OS.
- The layouts give each path as Stage 6 names it, so a data directory from Stage 6 restores.
- The net scenarios, the restore and checkpoint tests, and the LAN check pass unchanged.
