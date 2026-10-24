# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide


## Research


## Ready

- **Navigation.** **Plan: Y1.** In `integration/navigation/*_crowd`, idle friend #366 is asked to step aside to a place on idle friend #364, from tick 1,283: its walk steers round #364, ends short in its own cell, arrives short at once, and the walker asks it again every 4 ticks; it never moves and is never hemmed in.
- **Net.** **Plan: A1.** On CI's `windows-2025`, each of the 10 tests of `campfire-net-stack`'s `alloc` binary that traces allocations times out at 30 s, so the gate fails there; they pass on `ubuntu-26.04` and `macos-26`. Seen in runs 40120124881 (`68e772d55`) and 40128753732.
