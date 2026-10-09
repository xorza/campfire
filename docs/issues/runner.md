# Runner

Design: [Modules](../design/02-engine-core.md#modules), `runner`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- **Plan: F14.** A `Runner`'s world, and the harness worlds `Arena` and `RestoreTarget`, run the sim schedule tick after tick and never call `World::clear_trackers`, as an `App`'s update does each frame. Bevy keeps each component removal in the world's removal messages until that call, so in a long replay or verification they may grow for the whole session.
- **Plan: F8.** The 3v3 match test, `a_3v3_match_replays_to_the_same_hashes`, runs about 8 s, past the 1 s bound for one test: about 6 s without its copy check, which adds about 2 s.

## Ready

