# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Bug reports

The slice of [Bug reports](docs/design/bug-reports.md): a player who sees a bug presses F12 and gets a folder with the session's log up to that tick, a screenshot and the camera; an agent replays the log to that tick and reads the units there, and opens the report in the client to see the same frame. Three batches, each one commit; its checks are that document's.

### R1. A report (client files, codec, client, host)

- [ ] The key map has no chord of a report (`client-files/src/keys_file.rs:224`) → the `report` chord, F12 by default, which every keys file holds (bug-reports.md, A report is a folder of plain files).
- [ ] `codec` encodes no PNG → it encodes an RGBA frame as a compressed PNG, by the `png` crate, a dependency for the user's approval (bug-reports.md, The screenshot is the frame the window shows).
- [ ] The client writes no report → on the chord, it captures the window's frame by Bevy's `Screenshot`, and writes `screenshot.png` and `report.toml`, of the camera rig's state, its place and forward, the session, the tick drawn, the player slot, the selected units and the window's size, into `<data>/reports/<session>-<tick>/` through `store` (bug-reports.md, A report is a folder of plain files; bug-reports.md, The camera is the rig's state).
- [ ] A local server publishes its log only at its session's end (`host/src/server/session_dir/mod.rs:241`) → the report's chord sends the local server a command, as the save's does (`host/src/server/checkpoints/mod.rs:143`), and the server settles its journal and publishes the log so far, its seed revealed, into the report's folder (bug-reports.md, The log is the local server's, published at the report's tick).
- Checks: the client's tests write a report of a fixed scene, its camera read back to the same rig and its PNG decoded to the frame; a test of the local server publishes its log at a tick and replays it to the hash the session held there; in the client, F12 in a Zero Hour skirmish writes a report.

### R2. A report replays to its tick (runner, harness, driver)

- [ ] No tool replays a published log to a tick → `cargo drive run report <dir> [--probe <period_ms>:<units>]` replays the report's log by the runner's replay to its tick and writes a table of its units there, and each period's before it, by the harness's probe (bug-reports.md, A report replays to its tick).
- Checks: the command on a report of a scripted match tables the units the match held at the report's tick.

### R3. The client opens a report (client, host)

- [ ] The client opens no report → `--report <dir>` replays the report's log on the local server to its tick, holds it paused there, and puts the camera at the report's rig, the player spectating (bug-reports.md, The client opens a report, planned).
- Checks: a test opens a report and finds the paused tick and the rig; in the client, an agent opens a report of a Zero Hour skirmish and sees its frame.

## Foundations
