# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Stage 6: Sessions

The design is [Sessions](docs/design/10-sessions.md): a journal that makes the log durable, crash restore, slots and their controllers, reconnects and late joins, server bots, receipts, checkpoints by a copy of the changed state, saves and loads, and the local server. Each step updates the design documents it changes: design 05's log format and inputs, design 02's lifecycle, design 08 and its generated reference for each name it makes run.

18. **X1. The LAN check: the server's restart** (lan-check; [Tests](docs/design/10-sessions.md#tests)): the check kills the server mid-match and starts it again on its data directory, and passes when every order either took effect as the rules say or was logged as discarded at a resume, and the published log verifies to the server's final hash. CI runs it on every OS. The bot client's stop and restart with its key file, and the match on a local server, are done. It follows K2, as the clients must reconnect to the certificate they pinned.
