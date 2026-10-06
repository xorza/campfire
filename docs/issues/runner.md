# Runner

Design: [Modules](../design/02-engine-core.md#modules), `runner`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide



## Research

- The 3v3 match test, `a_3v3_match_replays_to_the_same_hashes`, runs about 8 s, past the 1 s bound for one test: about 6 s without its copy check, which adds about 2 s.

## Ready

- **Plan: B13.** The worst cases, `server_tick/worst_3v3` and `worst_3v3_checkpointed`, measure only a match's first tick, 3.9 ms to 4.5 ms against about 1.3 ms for the next: the sim schedule's first build and the pathing grid's first labels. No checkpoint copies in tick 0, so the checkpointed case cannot show the copy's cost, which design 10's Cost line reads from it.


