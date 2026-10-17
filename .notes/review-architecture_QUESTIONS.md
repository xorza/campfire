# Questions of the architecture plan

Decisions that the plan in `review-architecture.md` cannot take alone. Each names its item, the options, the recommendation and what waits for it.

## Q1. Batch 1.1, transport bounds: where the fix goes

- **Item:** 1.1, N-1 and N-2. Lightyear's fragment receiver allocates `num_fragments × fragment_size` from the peer's count before any check, and a link that did not join uses every reliable channel with no budget (fork `xorza/lightyear` at `54e6be2`, `crates/transport/transport/src/channel/fragments/receive.rs:137-147`, `channel/receive.rs:179-215`).
- **Why it waits:** the fix belongs in the fork's transport crate, a repository apart from this one. This plan may commit and push only on its own branch of `campfire`.
- **Options:**
  1. **Fix the fork (recommended).** A branch of `xorza/lightyear` gives each channel a largest message size, refuses a larger fragment count before it allocates, caps the half-built messages for each link and channel, and expires them on reliable channels too, as ENet and yojimbo check before they allocate. `net_protocol` declares each message's largest encoding, and this repository pins the fork by `rev`. Cost: a change in the fork, which the user commits and pushes, or allows.
  2. **A guard in `quic` or `net`.** It reads Lightyear's packet header from each datagram before Lightyear does, and drops one whose fragment count passes a bound. Cost: a second reader of Lightyear's wire format, which breaks when the fork changes it.
  3. **Vendor the transport crate.** Copy `lightyear_transport` into the workspace as a path patch and fix it there. Cost: a copy of an outside crate to keep in step with the fork.
- **Blocked:** batch 1.1 alone. No later batch needs it.

## Q2. Batch 3.4, a script's print: where it goes

- **Item:** 3.4, B-3. The rule "no crate of the sim's closure requires `store`, `tracing` or `log`" holds for every crate but `campfire-script`, whose debug build passes a script's `print` and `debug` to `tracing::debug!` (`capabilities/core/script/src/script_host/mod.rs`, `ScriptHost`). The test `the_sim_reads_no_file_and_logs_nothing_and_the_protocol_reads_no_file` in `rules` names that one exception.
- **Why it waits:** the choice decides what a scripter sees of a print, and no design document says.
- **Options:**
  1. **A print is a message of the match (recommended).** The script host writes each print into a `Messages<ScriptPrinted>`, which the app reads and logs, as OpenRA's Lua `Media.Debug` reaches the game's own debug output. The sim then logs nothing, and the exception goes. Cost: one message type, and a reader in the server and the client.
  2. **Keep the exception.** A debug build's print goes to `tracing` at the debug level, and the test names it. Cost: the sim logs, in a debug build, the one place it does.
  3. **No print.** Rhai's `print` and `debug` do nothing in every build. Cost: a scripter loses print debugging.
- **Blocked:** nothing. The test holds every other crate now.
