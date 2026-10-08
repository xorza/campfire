# Net

Design: [Modules](../design/02-engine-core.md#modules), `net`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- In the reference 3v3, a client that rolls back only on a misprediction rolls back on every confirmed update while its hero walks at its base after the first wave: 2,000 rollbacks of 3 ticks each in 2,000 frames, where the lane mode's walk makes none. Its frame costs 152.0 µs against 67.4 µs for a client that never rolls back. Which predicted part differs from the server's is not known.

- A client whose server dropped time after a stall past a frame's bound runs ahead of it, and Lightyear shifts the client's timeline back whole ticks at a time; from then on the client's sim mispredicts its own hero every tick and rolls back every tick, its hero standing where the server no longer has it, in the net scenario of a 2 s server stall.

- **Plan: F5.** A restored server's bots lose the payloads that waited for a later tick at the stop: `BotDriver::new` skips all that a bot's script had before the tick the server runs on from, where [Server bots](../design/10-sessions.md#server-bots) logs the payloads past a tick's max inputs in the next tick.

- **Stage 8.** Every replicated component of a unit goes to every client that sees the unit. Design 04 sends pools other than life to the owner or the team ([Combat](../design/04-capabilities/combat.md#network)), and experience and points to the owner ([Progression](../design/04-capabilities/progression.md#network)).

## Ready

- **Plan: F13.** `SessionDir::find` walks `sessions/` in the order `fs::read_dir` gives, which differs by OS and file system, and returns at the first flaw, so a data directory with a stray entry and two unpublished sessions gives `FindError::Stray` on one OS and `FindError::Several` on another.
- **Plan: F11.** A client that leaves refuses, with a warning, a receipt that reaches it after its leave: it plays no match from its leave on, so `ReceiptRefusal::NotPlaying`, and the receipt of its last inputs is lost. [Receipts](../design/10-sessions.md#receipts) has the client keep its chain until its link closes and take such a receipt. The LAN check failed on it in CI run 37537426577 on macOS, as a restored server that ran behind sent a receipt as bot 1 left.
- **Stage 8.** No production state replicates: a client sees no train queue and no player resource, where design 04's production sends a player's queues and resources to that player ([Production](../design/04-capabilities/production.md#network)).
- **Stage 10.** No unit's inventory replicates: a client sees no item its units carry, where design 04's items send a unit's own inventory to its owner ([Items](../design/04-capabilities/items.md#network)).

