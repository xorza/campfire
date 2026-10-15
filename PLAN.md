# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

**Slice 5, units that drive.** The user starts Tournament Desert in the client, finds America's command center and dozer at their start, selects the dozer, and drives it round the cliffs to the other start, at its speed and turn rate, on the drawn ground. The design is [Units that drive](docs/design/17-units-that-drive.md); the facts are [Zero Hour research](docs/research/zero-hour.md#movement). The Zero Hour check plays the drive, and loads every imported map.

**Slice 6, the issue log's ready items.** The user right-clicks the shrine with a hero and the hero uses it, and opens a night map and a snowy map of Zero Hour and sees their night and snow models; the file index reads through the codec.

**Optimization, in a session the user starts.** The change-driven passes wait in [optimization](.notes/optimization.md), with what the first session measured.

## 1. Zero Hour's units drive

`import`: `zero_hour`; `source/packages/zero-hour`; `checks/zero-hour-check`. It needs the turn rate, the load's rules and the selection to play.

### U9. The Zero Hour check compares fingerprints and plays the parity suite

(`checks/zero-hour-check`, the parity oracle; Zero Hour D5, D7, D8.) The Zero Hour check compares the fingerprint of its import with the one the other OS gave, and plays the parity suite against the oracle's frames, as design 12 D7 says. It waits for the oracle, the fork of GeneralsX that writes each logic frame. Check: a run that names no install fails; a run on Linux and one on macOS give one fingerprint; the suite's replays meet the measure.
