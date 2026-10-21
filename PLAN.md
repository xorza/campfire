# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## The issue log's network items

The four open items of the issue log, each fixed at its cause. They go before the slice: a stack test then fails on the warnings it causes, the slice's among them. Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### Net 1. Each warning a stack test causes, at its cause (M)

`net-interpolation`, `net-messages`, `net-replication`, `net-prediction`, `net`, `schedule`, their tests. First: batch Net 2 fails on each warning this batch leaves.

- **One set of each frame phase.** Each typed system of frame interpolation joins its phase set alone, `FramePhaseSystems::Run`, which `FrameInterpolationPlugin` already puts in the stage's set of each schedule, so the hierarchy holds no redundant edge in `RunFixedMainLoop`, `FixedPostUpdate` or `PostUpdate`. Builds: one way to say a thing.
- **A redundant edge fails every stack app's build.** `StrictSchedule` gains the hierarchy check alone, which the stepper gives each schedule of its apps beside the full strictness of `Update` and `FixedUpdate`, so the next redundant edge fails the build of every stack test, as one in `Update` does now. Builds: what can be forgotten is a test where one can see the breach.
- **A warning a test provokes is a typed event.** The stack's warnings that a test causes on purpose become `LogEvent`s of their crates, as `net`'s are: a message no system read in its frame (`net-messages`) and a link both targets name (`net-replication`); a test takes each with `LogCheck::take`, which names what it expects. `net-messages` and `net-replication` gain `campfire-log`, a crate of the workspace, as a dependency. Builds: no data in strings.
- **A test reads what it sends.** The wire-cost scenario and the replicon receiver's removal test read their messages with `ReceivedMessages`; `ProtocolClient` reads the session's offer and reports it in its state, which the server's stop test asserts; the test of an unread message's drop takes its event.
- **A duplicate pre-spawn hash is a contract breach.** Prediction asserts as a `PreSpawned` enters a client's world that no living pre-spawned entity holds its hash, as the error rule asks of a caller's logic error, in place of `bevy_replicon`'s error log, which no typed event can take; its test becomes the assert's.
- **Check:** each test that caused a warning takes it or no longer causes it; a test of `StrictSchedule`'s hierarchy check fails a schedule of a redundant edge; no golden moves.

### Net 2. The stepper's log check (S)

`net-stepper`, `net-harness`. After Net 1, whose warnings it would fail on.

- **The stepper holds the process's log check.** `Stepper` takes `LogCheck::start()` as it builds its apps and holds it as its last field, so it drops after them and fails its test on each event at Warn or Error no test took, as `Match` and `InProcessMatch` do now; `InProcessMatch` drops its own, which the stepper's replaces. Builds: what can be forgotten is a type; one way to say a thing.
- **Check:** every stack test passes under the check; a test of a stepper whose app logs a warning fails as it drops; no golden moves.

### Net 3. A pending handshake's early datagrams (M)

`net-udp`, `net-packets`, `net-harness`, 11-net-stack/udp.md. Independent of Net 1 and Net 2.

- **A pending handshake holds its early datagrams.** A sealed datagram whose connection ID a pending handshake holds goes to that handshake, which keeps a bounded few and hands them to its packet session as it is established, the rest dropped; the stateless reset answers only an ID that no packet session and no handshake holds. RFC 9001 §5.7 lets a server store the 1-RTT packets that come before the handshake's end and use them after it, and RFC 9000 §5.2 resets only a packet no connection can claim. Refused: a reset for a pending ID, which ends the client's connection that the handshake is about to establish; a drop of every early datagram, which delays the session's first messages by a resend. Builds: a fact fixed at the handshake's start is the route's.
- **Check:** a test of the emulated UDP link whose jitter brings the client's first sealed datagram before its last handshake packet: the connection is established, no reset is sent, and the early datagram's message arrives; `InProcessSetup` names its stepper's seed, and the harness's 1v1 through delayed links runs at seed 5, which fails today.

## Foundations

- **F1. A stack test runs with no log check.** The stepper builds its apps with no check of what they log, so a warning or an error that a change causes in a stack test passes unseen, and a test that causes one on purpose cannot take it. It holds the items Net interpolation, Net messages and Net stepper. Net 1 and Net 2 fix it.

## Slice: Zero Hour automatic attack

A Zero Hour player's idle units and base defenses attack the enemies that come within their weapons' reach, with no order ([proposal](docs/design/12-zero-hour-auto-attack.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The idle scan (L)

`actions`, `orders`, the capabilities' tests and a bench case. First: the scan is generic and every later batch reads it.

- **The source of an attack target.** `ActionProgress`'s attack target holds its source, an order or the scan, and each caller of `set_attack_target` names it; `Destinations::chase` walks to an ordered target and drops a scanned one that no weapon of the unit reaches, with no walk. Builds: what can be forgotten is a type.
- **The `acquire` section.** A unit type's `acquire = { scan_ms, targets }`, `targets` a list of filters of which any selects, as a projectile's `hits` is, read and checked at the load: a period above 0, one filter at least, a type with a weapon slot and no build or gather slot, each refusal an error of its own. Builds: a name is resolved once, at the load.
- **The scan.** A part of each unit of a type that scans, its next scan's tick, staggered by stable id as `NextThink` is, in an index of its own; in Think, on a role that computes state, each unit due that stands idle takes the nearest target its team sees that a filter of `targets` selects and that a weapon of it selects and reaches, by the unit grid, the lower stable id at an equal distance, and is due a period later. Builds: a pass visits what changed.
- **Check:** a test of the first scan's tick of two units of hand-computed stable ids; of the pick among targets at hand-computed distances, a tie to the lower id, and each target the scan passes over: one no filter selects, one no weapon reaches, one the team does not see, and a neutral under `hostiles`; of a scanned target that walks out of reach, dropped with no chase, against an ordered one, chased; of a unit that walks, which does not scan, and of a client's match, which never scans; of each load refusal. The bench case `integration/acquire/idle` scans 512 idle armed units at 250 ms and runs once in the gate. A state hash's pinned values move once, for the new part of a unit's progress.

### 2. Zero Hour's scan (M)

`import`, the rules package, 09. After batch 1, whose section it writes.

- **The neutral team.** The imported mode's relations make each player's team regard `neutral` as `Neutral`; 09's decision on the gather drops its refusal of that relation.
- **The base defense's tag.** `base_defense`, of `FS_BASE_DEFENSE`, in the rules package's tags and the import's tag list.
- **The scan of each type.** A type of an imported weapon whose AI module's `AutoAcquireEnemiesWhenIdle` holds `Yes` gets `acquire`, of its `MoodAttackCheckRate` or 2 s, as the import converts a time; `ATTACK_BUILDINGS` writes `targets = ["hostiles"]`; the rules package's `[defaults.acquire]` gives `targets = ["hostiles:!structure", "hostiles:base_defense"]`.
- **Check:** fixture tests of a type of `Yes` and a rate, of `Yes` and no rate, of `No`, of `ATTACK_BUILDINGS`, of an unarmed type of `Yes`, which gets none, and of the relations the mode writes; `cargo drive run zero-hour`, on request, moves its fingerprints and its skirmish play, whose idle units now fire.

### 3. Base defenses in the check (M)

`zero-hour-check`, its golden, 09, 04-capabilities/control.md. Last: it plays what the two batches before it give, and ends the slice.

- **A defense play.** China on Tournament Desert builds a power plant and a Gattling Cannon, and a bot's America dozer drives past the cannon; the play pins the tick of the cannon's first hit and of the dozer's death, with no order to the cannon.
- **The skirmish play.** Its golden pins the first hit on one of player 1's crusaders that no order of the bot gave.
- **The fold.** The orders decisions go to control.md, in place of the planned target acquisition, the import's and the losses to 09; the proposal is deleted.
- **Check:** `cargo drive run zero-hour` passes with its golden blessed, each moved value named by its cause; the user plays a skirmish in the client and watches a Gattling Cannon and idle tanks fire.
