# Runner

Design: [Modules](../design/02-engine-core.md#modules), `runner`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide


## Research

- The 3v3 match test, `a_3v3_match_replays_to_the_same_hashes`, runs about 8 s, past the 1 s bound for one test: about 6 s without its copy check, which adds about 2 s.

## Ready

- **Plan: B11.** The mean cases, `server_tick/mean_3v3` and `server_stage/mean_3v3_<stage>`, measure each sample over a stretch of ticks that criterion sizes, carried on from the last sample's match, so each sample covers a different part of a match: the ten of one run gave 75.8 µs to 245.5 µs a tick, and the median moved from 155.8 µs to 171.3 µs between two runs of the same tick. Their ids say the mean of a match, and design 13 says the stage means add up to less than the tick's, which no two separate runs of these cases show.

