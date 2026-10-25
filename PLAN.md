# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Bug reports

The slice of [Bug reports](docs/design/bug-reports.md): a player who sees a bug presses F12 and gets a folder with the session's log up to that tick, a screenshot and the camera; an agent replays the log to that tick and reads the units there, and opens the report in the client to see the same frame. Each batch is one commit; its checks are that document's.

### R3. The client opens a report (client, host)

- [ ] The client opens no report → `--report <dir>` replays the report's log on the local server to its tick, holds it paused there, and puts the camera at the report's rig, the player spectating (bug-reports.md, The client opens a report, planned).
- Checks: a test opens a report and finds the paused tick and the rig; in the client, an agent opens a report of a Zero Hour skirmish and sees its frame.

## Foundations
