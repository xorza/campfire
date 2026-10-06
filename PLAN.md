# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Stage 6: Sessions

The design is [Sessions](docs/design/10-sessions.md): a journal that makes the log durable, crash restore, slots and their controllers, reconnects and late joins, server bots, receipts, checkpoints by a copy of the changed state, saves and loads, and the local server. Each step updates the design documents it changes: design 05's log format and inputs, design 02's lifecycle, design 08 and its generated reference for each name it makes run.

2. **K2. Keep the TLS identity** (server; [Crash restore](docs/design/10-sessions.md#crash-restore)): the server keeps its TLS certificate and key in its data directory, made when missing and made again at a start when the certificate would expire within two days. Tests: two server starts in turn on one data directory give one certificate hash. Waits on a question: [K2](PLAN_QUESTIONS.md#k2-keep-the-tls-identity).
18. **X1. The LAN check** (lan-check; [Tests](docs/design/10-sessions.md#tests)): the check kills the server mid-match, starts it again on its data directory, stops one bot client and starts it again with its key file, and passes when every order either took effect as the rules say or was logged as discarded at a resume, and the published log verifies to the server's final hash; and it plays a match of a client bot against server bots on a local server with no network, whose log verifies. CI runs both on every OS.
