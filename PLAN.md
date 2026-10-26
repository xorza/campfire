# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Net faults

A fix of the issue log's Net items, found 2026-10-26. It stands first, as N2 makes each CI run after it keep the frames N3 measures.

### N3. The lead of a bot's first order is measured

Touches `docs/research/lan-late-input.md` and the issue log; it stands after N2, as it reads the frames N2 keeps.

- [ ] Every failure of `lan.2` the research holds, stamps 300, 20, 302, 20 and 20, is the first order after a start or a restart; in run 40328962123, bot 0 stamped 20 while the server's last tick was 19, a lead of 1 where the objective is 2, which the sync's deadband of 1 lets stay, and after a stall of every process a lead of 1 leaves only the phase to the server's next tick for the bot's frame and send. From the artifacts of macOS runs after N2, failed and passed, and of Linux runs for comparison: each bot's tick at each frame's start less the server's, from its start or restart to its first order; the frame each order left in; whether the lead reached its objective by then. The facts go to the research, dated; the issue moves to Decide with the options they show, as a deadband on the ahead side alone, or a lead the sync reaches before a first order may go. Checks: none of the code.

## Effects

The slice of [Effects](docs/design/effects.md): a player of a Zero Hour skirmish sees each shot's fire effect at its barrel, each projectile's exhaust, each hit's burst and each death's explosion, as the game's FX lists and particle systems draw them. Its facts are [Zero Hour's effects](docs/research/zero-hour-effects.md); its checks are that document's.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them ([question](PLAN_QUESTIONS.md#effects-end-of-the-slice)); then the slice's decisions fold into the documents of their systems, `02-engine-core.md` for the format and the client's particles and `09-zero-hour.md` for what the import converts and loses, the code's citations of `effects.md` with them, and `effects.md` goes.

## Console

The slice of [Console](docs/design/console.md): an agent in Claude Code starts a local match of the MOBA test mode in the client, runs it paused to a tick, tables its units and shows a screenshot of one of them, while the user watches the window. Its facts are [The console](docs/research/console.md); its checks are that document's. It stands after Effects, whose end the user plays first.

### C1. The console's protocol and the server's console

Touches the new `campfire-console`, the workspace's dependencies, `schemars` among them, `host` and the server; it stands first, as every later batch speaks its protocol.

- [ ] No crate holds a console. `campfire-console`, a problem crate under `crates/session/`, holds the opening line, of the role and the fingerprint of the console's types, a status and a table of units, each request and reply with `serde` and `schemars`, read and written through `codec::Json`. Direction: each crate depends only on lower layers. Checks: each request and reply round-trips, and the fingerprint moves with a field of either.
- [ ] A server takes no request from outside. `--console stdio` makes a reader on a thread of `threads` queue each line of standard input, which the server's frame drains between ticks, a reply to standard output each: the status, the session's phase, tick, rate, pace, state hash, players' resources and script failures; and the table of units by `MatchProbe`, concise or detailed, of a player slot, a team, a type, ids or a probe state, a type named as the books name it. The flag ships in every binary. Direction: a name is resolved once. Checks: the server's binary under its console answers a status over its pipes, started as `apps/server/tests/stop.rs` starts it.

### C2. A local match runs to a tick

Touches `host`'s pace, `net`'s local server and its link, and the client; it stands after C1, whose requests it forwards.

- [ ] The pace runs no count of ticks. `Pace` gains a run to a tick, for a count of ticks, or until a typed condition holds after a tick, a unit's probe state, a log event of a type, a script failure or the match's end, at four times the session's rate, which then pauses and gives the tick and the cause it stopped at; the local server tests the condition after each tick. Direction: a fact fixed at load is a fact of the schedule, as the condition's names resolve as the run starts. Checks: a run of the lane match to tick 300 pauses after it, and the state hash its status gives there is the hash the verifier's replay of the published log reaches at tick 300; a run until a unit is idle stops at the first tick its probe shows it idle; a run past its wall time returns the tick it reached.
- [ ] A client takes no request from outside. The client's `--console stdio` with `--local` forwards the `sim_` requests to its local server over their link, as its journal commands go, and answers a run once it draws the tick the run paused at; a client of a remote server refuses a run. Direction: one rendering pipeline for every game, as the console reads the frame the window shows. Checks: through the harness's client and local server, a run's reply comes after the client's drawn tick is the run's.

### C3. The MCP server

Touches the new `campfire-mcp`, the workspace's dependencies, `rmcp` and `tokio` among them, the driver's table and `.mcp.json`; it stands after C2, whose run its tools serve.

- [ ] No MCP server serves the games. `campfire-mcp` serves `game_start`, `game_stop`, `game_events`, `sim_status`, `sim_units`, `sim_run` and `sim_pace` over stdio through `rmcp` on a runtime of one thread, in one order, the unit table with `anthropic/maxResultSizeChars`, each game by a handle, an unknown name failing with the names nearest it; `game_start` builds the client and the server through the driver into its log, starts the binary with `--console stdio`, refuses another fingerprint, and says when its own binary on disk is newer than the process; `game_events` reads the typed lines of the game's log file in pages. Direction: what a module adds is declared once, as the tools' schemas derive from the console's types. Checks: the tools list in one order with their schemas; a status of a server under its console; a game of another fingerprint refused.
- [ ] Claude Code has no server to start. The driver's table gains `mcp`, which builds `campfire-mcp` in `release-dev`, and the build of the client and the server `game_start` runs, in `release-dev` or the dev profile; `.mcp.json` at the repository's root names the built `campfire-mcp`, by its path from the root or by `${CLAUDE_PROJECT_DIR}`, whichever resolves on every OS. Checks: after `cargo drive run mcp`, Claude Code connects and lists the tools.

### C4. The client's view

Touches the client; it stands after C3, as its tools join the server's.

- [ ] No tool shows the window. `view_screenshot` captures the window, a rectangle of its pixels or the box round units on screen, scaled by area to sides of at most 2,000 px unless the call names another bound, encoded through `codec::Png`, with its scale to the frame, the tick drawn and the camera's rig; `view_camera` reads or sets the rig, or turns it to a unit or a point. Direction: one rendering pipeline for every game. Checks: the client's tests compute a unit's box on screen, a region's crop and its scale to a bound against hand-computed pixels, and set the rig and read it back.

### End of the slice

It stands last, as the user's check is of the whole slice.

- [ ] The user watches the agent start a local match of the MOBA test mode, run it to a tick, table its units and show a screenshot of one; in a local Zero Hour skirmish of bots alone, the agent finds the walkers stuck at a tick with the tools alone; then the slice's decisions fold into the documents of their systems, `02-engine-core.md` for the console, the MCP server and the view, and `08-sessions.md` for the run, the planned ones into `03-game-scripting.md`, `05-protocol-spec.md` and `06-script-api.md`, and `console.md` goes.

## Foundations
